//! 惑星の地形（planet.py / terrain_earth.py と同じ式）。
//!
//! 地表の高さはキューブマップ状のタイルにキャッシュし、双線形補間で引く。
//! 池・洞窟の配置（乱数で作る部分）は Python が作って渡す（作るのは読み込み時の1回だけなので）。

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

const M1: u32 = 374761393;
const M2: u32 = 668265263;
const M3: u32 = 2246822519;
const M4: u32 = 1274126177;

/// 値ノイズ（0..1）。planet.value_noise と同じ。
#[inline]
pub fn value_noise(p: [f64; 3], seed: i64) -> f64 {
    let fl = [p[0].floor(), p[1].floor(), p[2].floor()];
    let f = [p[0] - fl[0], p[1] - fl[1], p[2] - fl[2]];
    let u = [f[0] * f[0] * (3.0 - 2.0 * f[0]), f[1] * f[1] * (3.0 - 2.0 * f[1]), f[2] * f[2] * (3.0 - 2.0 * f[2])];
    let i = [(fl[0] as i64) as u32, (fl[1] as i64) as u32, (fl[2] as i64) as u32];
    let s = ((seed as i128 * 1442695041) & 0xFFFF_FFFF) as u32;
    let hx = [i[0].wrapping_mul(M1), i[0].wrapping_add(1).wrapping_mul(M1)];
    let hy = [i[1].wrapping_mul(M2), i[1].wrapping_add(1).wrapping_mul(M2)];
    let hz = [i[2].wrapping_mul(M3).wrapping_add(s), i[2].wrapping_add(1).wrapping_mul(M3).wrapping_add(s)];
    let wx = [1.0 - u[0], u[0]];
    let wy = [1.0 - u[1], u[1]];
    let wz = [1.0 - u[2], u[2]];
    let mut out = 0.0;
    for dx in 0..2 {
        for dy in 0..2 {
            let hxy = hx[dx].wrapping_add(hy[dy]);
            let wxy = wx[dx] * wy[dy];
            for dz in 0..2 {
                let mut h = hxy.wrapping_add(hz[dz]);
                h = (h ^ (h >> 13)).wrapping_mul(M4);
                h ^= h >> 16;
                out += wxy * wz[dz] * ((h & 0xFFFF) as f64 * (1.0 / 65535.0));
            }
        }
    }
    out
}

#[inline]
pub fn fbm(p: [f64; 3], seed: i64, octaves: i64) -> f64 {
    let (mut total, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        total += amp * value_noise([p[0] * freq, p[1] * freq, p[2] * freq], seed + o * 31);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    total / norm
}

#[inline]
fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// features.rs から使う
pub fn fbm_pub(p: [f64; 3], seed: i64, octaves: i64) -> f64 {
    fbm(p, seed, octaves)
}
pub fn ridged_pub(p: [f64; 3], seed: i64, octaves: i64) -> f64 {
    ridged(p, seed, octaves)
}

#[inline]
fn ridged(p: [f64; 3], seed: i64, octaves: i64) -> f64 {
    let n = (fbm(p, seed, octaves) - 0.5) / 0.12;
    (1.0 - n.abs() * 0.6).clamp(0.0, 1.0).powi(2)
}

#[inline]
fn affine(d: [f64; 3], k: f64, c: f64) -> [f64; 3] {
    [d[0] * k + c, d[1] * k + c, d[2] * k + c]
}

#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// 地球型の地形の値（池・洞窟の配置を含む）。
pub struct Earth {
    pub sea: f64,
    pub mountain_h: f64,
    pub hill_h: f64,
    pub flat_r: f64,
    pub spawn_h: f64,
    pub ponds_dir: Vec<[f64; 3]>,
    pub ponds_rad: Vec<f64>,
    pub ponds_depth: Vec<f64>,
    pub ponds_level: Vec<f64>,
    pub cave_a: Vec<[f64; 3]>,
    pub cave_b: Vec<[f64; 3]>,
    pub cave_ra: Vec<f64>,
    pub cave_rb: Vec<f64>,
    pub cell: f64,
    pub cave_index: HashMap<i64, (usize, usize)>,
    pub cave_cap: Vec<usize>,
    /// 出現地点のそばの山脈（シナリオ用）：出現地点からの距離と幅（m）。幅 0 なら無し
    pub range_dist: f64,
    pub range_w: f64,
    /// その山脈の高さ（m。0 なら mountain_h）
    pub range_h: f64,
    /// 地形の名所（Earth MOD など。features.rs）
    pub features: Vec<crate::features::Feature>,
    /// 川（0 で無し、1 がふつう。川の幅の倍率）
    pub rivers: f64,
}

/// 気候の軸（北極の向き）。出現地点（+Y）が温帯になるように傾けてある
/// 川の幅（ノイズの値の幅。だいたい 40 m）
const RIVER_W: f64 = 0.0065;

const POLE: [f64; 3] = [0.6265, 0.7770, 0.0610];

/// 段々（台地）：x を steps 段に分け、段の間を急にする
#[inline]
fn terrace(x: f64, steps: f64) -> f64 {
    let y = x * steps;
    let f = y.floor();
    let r = y - f;
    (f + smoothstep(0.35, 0.65, r)) / steps
}

pub struct Shape {
    pub radius: f64,
    pub amp: f64,
    pub freq: f64,
    pub seed: i64,
    pub earth: Option<Earth>,
    n: usize,
    /// 滝（名所から。最初に使うときに作る）
    falls: std::sync::OnceLock<Vec<[f64; 7]>>,
    tiles: RwLock<(HashMap<(usize, i64, i64), Arc<Vec<[f64; 2]>>>, VecDeque<(usize, i64, i64)>)>,
    /// 舗装（MOD の街・道。pave.rs）
    pub paving: RwLock<Vec<(String, crate::pave::Pave)>>,
}

pub const TILE: usize = 32;
const RES: f64 = 0.25;
const MAX_TILES: usize = 4000;

impl Shape {
    pub fn new(radius: f64, amp: f64, freq: f64, seed: i64, earth: Option<Earth>) -> Self {
        let n = ((std::f64::consts::PI / 2.0 * radius / RES / TILE as f64).ceil() as usize) * TILE;
        Shape { radius, amp, freq, seed, earth, n, falls: std::sync::OnceLock::new(), tiles: RwLock::new((HashMap::new(), VecDeque::new())),
                paving: RwLock::new(Vec::new()) }
    }

    fn ang(&self, d: [f64; 3], r: [f64; 3]) -> f64 {
        dot(d, r).clamp(-1.0, 1.0).acos() * self.radius
    }

    /// 方向 d を、波長 w（m）のノイズの座標に（地形の大きさを惑星の大きさによらず m で決める。A-02）
    #[inline]
    fn pm(&self, d: [f64; 3], w: f64, off: f64) -> [f64; 3] {
        affine(d, self.radius / w, off)
    }

    /// 気候：(気温, 湿り気)。気温は赤道で 1・極で約 -0.15、高い所ほど低い（mountain_h の 0.9 倍の高さで 1 下がる）。湿り気 0〜1
    pub fn climate(&self, e: &Earth, d: [f64; 3], h: f64) -> (f64, f64) {
        let sd = self.seed;
        let lat = dot(d, POLE).abs().clamp(0.0, 1.0).asin() / std::f64::consts::FRAC_PI_2;
        let tn = fbm(self.pm(d, 9000.0, 61.0), sd + 11, 3) - 0.5;
        let lapse = (e.mountain_h * 0.9).max(50.0);
        let temp = 1.0 - 1.15 * lat.powf(1.3) + tn * 0.45 - (h - e.sea).max(0.0) / lapse;
        let mn = fbm(self.pm(d, 12000.0, 17.0), sd + 12, 4);
        let mut moist = ((mn - 0.5) * 2.4 + 0.5).clamp(0.0, 1.0);
        // 出現地点のまわり（6 km）は草原・森になるように湿らせる
        let e_sp = (-(self.ang(d, [0.0, 1.0, 0.0]) / 6000.0).powi(2)).exp();
        moist = moist * (1.0 - e_sp) + moist.max(0.55) * e_sp;
        let mut temp = temp;
        for f in &e.features {
            if let Some((ft, fm, w)) = f.climate(d, self.radius) {
                // 名所の気候（高さで下がる分は残す）
                let lapse_part = -(h - e.sea).max(0.0) / lapse;
                temp = temp * (1.0 - w) + (ft + lapse_part) * w;
                moist = moist * (1.0 - w) + fm * w;
            }
        }
        (temp, moist)
    }

    /// 滝：[上 xyz, 下 xyz, 幅]（惑星ローカル）
    pub fn waterfalls(&self) -> &Vec<[f64; 7]> {
        self.falls.get_or_init(|| match &self.earth {
            Some(e) => crate::features::waterfalls(&e.features, self.radius, &|d| self.height0(e, d)),
            None => vec![],
        })
    }

    /// 名所の水面（m。無ければ -1e9）
    pub fn feature_water(&self, e: &Earth, d: [f64; 3]) -> f64 {
        e.features.iter().map(|f| f.water(d, self.radius)).fold(-1e9, f64::max)
    }

    /// 舗装の (色, 重み)。重み 0 は舗装していない所
    pub fn pave(&self, d: [f64; 3]) -> ([f64; 3], f64) {
        self.pave_blur(d, 0.0)
    }
    /// blur：点の間隔（m）。細い道を粗いメッシュでぎざぎざにしない
    pub fn pave_blur(&self, d: [f64; 3], blur: f64) -> ([f64; 3], f64) {
        let p = self.paving.read().unwrap();
        if p.is_empty() {
            return ([0.0; 3], 0.0);
        }
        crate::pave::paint(&p, d, self.radius, blur)
    }

    /// 名所の地表の色・草木（いちばん強いもの）
    pub fn feature_paint(&self, d: [f64; 3], h: f64, slope: f64) -> crate::features::Paint {
        let mut best = crate::features::Paint { tree: -1, ..Default::default() };
        if let Some(e) = &self.earth {
            for f in &e.features {
                let p = f.paint(d, self.radius, h, slope);
                if p.w > best.w || (p.w == best.w && p.bare > best.bare) {
                    let tree = if p.tree >= 0 { p.tree } else { best.tree };
                    best = crate::features::Paint { tree, bare: p.bare.max(best.bare), ..p };
                } else {
                    best.bare = best.bare.max(p.bare);
                    if best.tree < 0 {
                        best.tree = p.tree;
                    }
                }
            }
        }
        best
    }

    /// 乾いた暑い土地の度合い（砂漠 1・サバンナ 0.5 くらい）
    #[inline]
    fn aridity(t: f64, m: f64) -> f64 {
        smoothstep(0.45, 0.75, t) * smoothstep(0.5, 0.2, m)
    }

    /// 池を除いた地表の高さ（地球型）。A-02：大陸・丘・山脈・台地（メサ）・砂丘。大きさは m で決める
    pub fn height0(&self, e: &Earth, d: [f64; 3]) -> f64 {
        self.height_full(e, d).0
    }

    /// 川：曲がりくねった線（ゆがめたノイズの等高線）に沿って、地形を削って水を入れる。(削る深さ, 川の水面) 。
    /// 水面は削る前の地面の 1 m 下なので、川は土地の低い方へ下っていく。海の近く・高い山・名所の中には作らない
    fn river(&self, e: &Earth, d: [f64; 3], h: f64) -> (f64, f64) {
        if e.rivers <= 0.0 {
            return (0.0, -1e9);
        }
        let sd = self.seed;
        let mask = smoothstep(e.sea + 0.5, e.sea + 4.0, h) * smoothstep(800.0, 450.0, h);
        if mask <= 0.0 {
            return (0.0, -1e9);
        }
        let fw = e.features.iter().map(|f| f.weight(d, self.radius)).fold(0.0, f64::max);
        let mask = mask * (1.0 - fw);
        if mask <= 0.0 {
            return (0.0, -1e9);
        }
        let q = self.pm(d, 9000.0, 31.0);
        let wp = [fbm(self.pm(d, 2600.0, 57.0), sd + 41, 3) - 0.5, fbm(self.pm(d, 2600.0, 77.0), sd + 42, 3) - 0.5,
                  fbm(self.pm(d, 2600.0, 97.0), sd + 43, 3) - 0.5];
        let v = fbm([q[0] + wp[0] * 1.6, q[1] + wp[1] * 1.6, q[2] + wp[2] * 1.6], sd + 40, 2);
        let a = (v - 0.5).abs();
        let width = RIVER_W * e.rivers;
        if a > width * 3.0 {
            return (0.0, -1e9);
        }
        let ch = 1.0 - smoothstep(0.0, width, a);
        let bank = 1.0 - smoothstep(width, width * 3.0, a);
        let carve = mask * (ch * 4.5 + bank * 1.8);
        let water = if ch > 0.15 && mask > 0.5 { h - 1.0 } else { -1e9 };
        (carve, water)
    }

    /// 地表の高さと、川の水面（無ければ -1e9）
    pub fn height_full(&self, e: &Earth, d: [f64; 3]) -> (f64, f64) {
        let sd = self.seed;
        // 大陸（波長 30 km）。出現地点（+Y）のまわり 3 km は陸
        let mut cont = (fbm(self.pm(d, 30000.0, 11.0), sd, 5) - 0.5) / 0.09;
        let a_sp = self.ang(d, [0.0, 1.0, 0.0]);
        let e_sp = (-(a_sp / 8000.0).powi(2)).exp();
        cont = cont * (1.0 - e_sp) + (cont.max(1.2) + 0.3) * e_sp;
        let base = if cont > 0.0 { (cont * 18.0).min(45.0) } else { (cont * 90.0).max(-400.0) };
        let land = smoothstep(-0.05, 0.4, cont);
        let (tt, mm) = self.climate(e, d, base);
        let arid = Self::aridity(tt, mm);
        // 丘（波長 700 m と 180 m）。湿った森・草原はなだらか、乾いた所は起伏が大きい
        let hills = ((fbm(self.pm(d, 700.0, 5.0), sd + 1, 4) - 0.5) * 2.5 + (fbm(self.pm(d, 180.0, 13.0), sd + 8, 2) - 0.5) * 0.6)
            * e.hill_h * (0.6 + 0.8 * arid);
        // 山脈（尾根の波長 7 km、山の帯 18 km）
        let ridge = ridged(self.pm(d, 7000.0, 3.0), sd + 2, 5);
        let rough = ridged(self.pm(d, 1500.0, 7.0), sd + 4, 3) * 0.6 + ridged(self.pm(d, 400.0, 1.0), sd + 5, 2) * 0.4;
        let mask = smoothstep(0.5, 0.66, fbm(self.pm(d, 18000.0, 40.0), sd + 3, 3)) * smoothstep(0.2, 1.2, cont);
        let shape = (ridge * 0.55 + 0.45) * (0.65 + 0.35 * rough);
        let mut mtn_h = mask * shape * e.mountain_h * 1.1;
        if e.range_w > 0.0 {
            let mdir = [0.0, (e.range_dist / self.radius).cos(), -(e.range_dist / self.radius).sin()];
            let rh = if e.range_h > 0.0 { e.range_h } else { e.mountain_h };
            mtn_h = mtn_h.max((-(self.ang(d, mdir) / e.range_w).powi(2)).exp() * shape * rh * 1.1);
        }
        // 台地（メサ）：乾いた土地で段々の台（高さ 90 m まで）
        let mesa_n = smoothstep(0.52, 0.72, fbm(self.pm(d, 2500.0, 23.0), sd + 6, 3));
        let mesa = terrace(mesa_n, 3.0) * 90.0 * arid * land;
        // 砂丘（砂漠だけ。波長 140 m、高さ 8 m）
        let dune = ridged(self.pm(d, 140.0, 9.0), sd + 7, 2) * 8.0 * smoothstep(0.5, 0.9, arid) * land;
        let mut h = base + hills + mtn_h + mesa + dune;
        for f in &e.features {
            h = f.height(d, self.radius, h);
        }
        let w = 1.0 - smoothstep(e.flat_r, e.flat_r * 1.9, a_sp);
        let (carve, rw) = self.river(e, d, h);
        let h = h - carve * (1.0 - w);
        let rw = if w > 0.01 { -1e9 } else { rw };
        (h * (1.0 - w) + (e.spawn_h + hills * 0.15) * w, rw)
    }

    /// dirs の高さ h を池で削り、水面の高さ（無い所は -inf）を返す（まとめて：近い池だけ調べる）
    fn apply_ponds(&self, e: &Earth, dirs: &[[f64; 3]], h: &mut [f64]) -> Vec<f64> {
        let mut water = vec![f64::NEG_INFINITY; dirs.len()];
        if e.ponds_dir.is_empty() || dirs.is_empty() {
            return water;
        }
        let mut mean = [0.0; 3];
        for d in dirs {
            for a in 0..3 {
                mean[a] += d[a];
            }
        }
        for a in 0..3 {
            mean[a] /= dirs.len() as f64;
        }
        let ml = dot(mean, mean).sqrt().max(1e-9);
        for a in 0..3 {
            mean[a] /= ml;
        }
        let spread = dirs.iter().map(|&d| self.ang(d, mean)).fold(f64::NEG_INFINITY, f64::max);
        for k in 0..e.ponds_dir.len() {
            if self.ang(e.ponds_dir[k], mean) >= spread + e.ponds_rad[k] * 2.0 {
                continue;
            }
            let lv = e.ponds_level[k];
            for (i, &d) in dirs.iter().enumerate() {
                let t = self.ang(d, e.ponds_dir[k]) / e.ponds_rad[k];
                if t >= 2.0 {
                    continue;
                }
                let bowl = if t < 1.0 {
                    lv - e.ponds_depth[k] * (1.0 - t * t)
                } else {
                    lv + (t - 1.0) * e.ponds_rad[k] * 0.8
                };
                h[i] = h[i].min(bowl);
                if t < 1.15 {
                    water[i] = water[i].max(lv);
                }
            }
        }
        water
    }

    /// タイルに保存する値：地表半径・水面半径
    pub fn tile_values(&self, dirs: &[[f64; 3]]) -> Vec<[f64; 2]> {
        match &self.earth {
            None => dirs
                .iter()
                .map(|&d| {
                    let n = fbm(affine(d, self.freq, 100.0), self.seed, 3);
                    [self.radius + (n - 0.5) * 2.0 * self.amp, 0.0]
                })
                .collect(),
            Some(e) => {
                let hf: Vec<(f64, f64)> = dirs.iter().map(|&d| self.height_full(e, d)).collect();
                let mut h: Vec<f64> = hf.iter().map(|x| x.0).collect();
                let water = self.apply_ponds(e, dirs, &mut h);
                let water: Vec<f64> = water.iter().zip(hf.iter()).map(|(w, x)| w.max(x.1)).collect();
                h.iter()
                    .zip(water.iter())
                    .zip(dirs.iter())
                    .map(|((&hh, &w0), &d)| {
                        let w = w0.max(e.sea).max(self.feature_water(e, d));
                        let mut wr = if w > hh { self.radius + w } else { 0.0 };
                        if wr == 0.0 {
                            wr = self.radius + w.min(hh - 0.05);
                        }
                        [self.radius + hh, wr]
                    })
                    .collect()
            }
        }
    }

    /// 地表半径（ノイズを直接計算）
    pub fn surface_radius_exact(&self, dirs: &[[f64; 3]]) -> Vec<f64> {
        match &self.earth {
            None => dirs
                .iter()
                .map(|&d| self.radius + (fbm(affine(d, self.freq, 100.0), self.seed, 3) - 0.5) * 2.0 * self.amp)
                .collect(),
            Some(e) => {
                let mut h: Vec<f64> = dirs.iter().map(|&d| self.height0(e, d)).collect();
                self.apply_ponds(e, dirs, &mut h);
                h.iter().map(|&x| self.radius + x).collect()
            }
        }
    }

    fn make_tile(&self, face: usize, tx: i64, ty: i64) -> Vec<[f64; 2]> {
        let (n, t) = (self.n as f64, TILE as i64);
        let major = face / 2;
        let neg = face % 2 == 1;
        let mut dirs = Vec::with_capacity((TILE + 1) * (TILE + 1));
        for i in 0..=t {
            for j in 0..=t {
                let gx = (tx * t + i) as f64 / n * 2.0 - 1.0;
                let gy = (ty * t + j) as f64 / n * 2.0 - 1.0;
                let mut d = [0.0; 3];
                d[major] = if neg { -1.0 } else { 1.0 };
                d[(major + 1) % 3] = gx;
                d[(major + 2) % 3] = gy;
                let l = dot(d, d).sqrt();
                dirs.push([d[0] / l, d[1] / l, d[2] / l]);
            }
        }
        self.tile_values(&dirs)
    }

    fn tile(&self, key: (usize, i64, i64)) -> Arc<Vec<[f64; 2]>> {
        if let Some(t) = self.tiles.read().unwrap().0.get(&key) {
            return t.clone();
        }
        let mut g = self.tiles.write().unwrap();
        if let Some(t) = g.0.get(&key) {
            return t.clone();
        }
        let t = Arc::new(self.make_tile(key.0, key.1, key.2));
        g.0.insert(key, t.clone());
        g.1.push_back(key);
        if g.0.len() > MAX_TILES {
            if let Some(old) = g.1.pop_front() {
                g.0.remove(&old);
            }
        }
        t
    }

    /// タイルにキャッシュした値（地表半径・水面半径）を双線形補間
    pub fn sample(&self, d: [f64; 3]) -> [f64; 2] {
        let n = self.n as f64;
        let ab = [d[0].abs(), d[1].abs(), d[2].abs()];
        let mut major = 0;
        if ab[1] > ab[major] {
            major = 1;
        }
        if ab[2] > ab[major] {
            major = 2;
        }
        let face = major * 2 + usize::from(d[major] < 0.0);
        let (a1, a2) = ((major + 1) % 3, (major + 2) % 3);
        let m = ab[major];
        let u = d[a1] / m;
        let v = d[a2] / m;
        let gx = ((u + 1.0) * 0.5 * n).clamp(0.0, n - 1e-6);
        let gy = ((v + 1.0) * 0.5 * n).clamp(0.0, n - 1e-6);
        let (ix, iy) = (gx.floor() as i64, gy.floor() as i64);
        let (fx, fy) = (gx - ix as f64, gy - iy as f64);
        let t = TILE as i64;
        let (tx, ty) = (ix.div_euclid(t), iy.div_euclid(t));
        let (lx, ly) = ((ix - tx * t) as usize, (iy - ty * t) as usize);
        let tile = self.tile((face, tx, ty));
        let w = TILE + 1;
        let at = |a: usize, b: usize| tile[a * w + b];
        let (v00, v10, v01, v11) = (at(lx, ly), at(lx + 1, ly), at(lx, ly + 1), at(lx + 1, ly + 1));
        let mut out = [0.0; 2];
        for c in 0..2 {
            out[c] = v00[c] * (1.0 - fx) * (1.0 - fy) + v10[c] * fx * (1.0 - fy) + v01[c] * (1.0 - fx) * fy
                + v11[c] * fx * fy;
        }
        out
    }

    /// 洞窟（くり抜き）までの距離と、その向き（惑星ローカル座標）。洞窟が無ければ (1e9, 0)。
    pub fn cave_dist(&self, q: [f64; 3]) -> (f64, [f64; 3]) {
        let mut best = (1e9, [0.0; 3]);
        let e = match &self.earth {
            Some(e) => e,
            None => return best,
        };
        let g = [(q[0] / e.cell).floor() as i64, (q[1] / e.cell).floor() as i64, (q[2] / e.cell).floor() as i64];
        let key = ((g[0] + 4096) * 8192 + (g[1] + 4096)) * 8192 + (g[2] + 4096);
        if let Some(&(st, en)) = e.cave_index.get(&key) {
            for &k in &e.cave_cap[st..en] {
                let (a, b) = (e.cave_a[k], e.cave_b[k]);
                let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let t = (dot([q[0] - a[0], q[1] - a[1], q[2] - a[2]], ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
                let c = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
                let v = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
                let l = dot(v, v).sqrt();
                let d = l - (e.cave_ra[k] + (e.cave_rb[k] - e.cave_ra[k]) * t);
                if d < best.0 {
                    let li = l.max(1e-9);
                    best = (d, [v[0] / li, v[1] / li, v[2] / li]);
                }
            }
        }
        best
    }

    /// 符号付き距離（負＝地中）。q は惑星ローカル座標。
    pub fn sdf(&self, q: [f64; 3], craters: &[[f64; 4]]) -> f64 {
        let r = dot(q, q).sqrt();
        let ri = r.max(1e-9);
        let d0 = [q[0] / ri, q[1] / ri, q[2] / ri];
        let mut d = r - self.sample(d0)[0];
        if !craters.is_empty() {
            let mut m = f64::INFINITY;
            for c in craters {
                let v = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
                m = m.min(dot(v, v).sqrt() - c[3]);
            }
            d = d.max(-m);
        }
        if self.earth.is_some() {
            d = d.max(-self.cave_dist(q).0);
        }
        d
    }

    /// 距離と勾配（法線）。planet.sdf_grad と同じ。
    pub fn sdf_grad(&self, q: [f64; 3], craters: &[[f64; 4]], eps: f64) -> (f64, [f64; 3]) {
        let r = dot(q, q).sqrt();
        let ri = r.max(1e-9);
        let dirs = [q[0] / ri, q[1] / ri, q[2] / ri];
        let rf = if dirs[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let mut t1 = cross(dirs, rf);
        let l1 = dot(t1, t1).sqrt();
        for a in 0..3 {
            t1[a] /= l1;
        }
        let t2 = cross(dirs, t1);
        let ang = eps / self.radius.max(1.0);
        let mut d1 = [dirs[0] + t1[0] * ang, dirs[1] + t1[1] * ang, dirs[2] + t1[2] * ang];
        let mut d2 = [dirs[0] + t2[0] * ang, dirs[1] + t2[1] * ang, dirs[2] + t2[2] * ang];
        let (la, lb) = (dot(d1, d1).sqrt(), dot(d2, d2).sqrt());
        for a in 0..3 {
            d1[a] /= la;
            d2[a] /= lb;
        }
        let (r0, r1, r2) = (self.sample(dirs)[0], self.sample(d1)[0], self.sample(d2)[0]);
        let mut d = r - r0;
        let mut g = [0.0; 3];
        for a in 0..3 {
            g[a] = dirs[a] - t1[a] * ((r1 - r0) / eps) - t2[a] * ((r2 - r0) / eps);
        }
        if !craters.is_empty() {
            let mut bk = 0;
            let mut bd = f64::INFINITY;
            for (k, c) in craters.iter().enumerate() {
                let v = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
                let dist = dot(v, v).sqrt() - c[3];
                if dist < bd {
                    bd = dist;
                    bk = k;
                }
            }
            let cd = -bd;
            if cd > d {
                let c = craters[bk];
                let v = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
                let vl = dot(v, v).sqrt().max(1e-9);
                d = cd;
                g = [-v[0] / vl, -v[1] / vl, -v[2] / vl];
            }
        }
        if self.earth.is_some() {
            let (c, cg) = self.cave_dist(q);
            if -c > d {
                d = -c;
                g = [-cg[0], -cg[1], -cg[2]];
            }
        }
        let gl = dot(g, g).sqrt().max(1e-9);
        (d, [g[0] / gl, g[1] / gl, g[2] / gl])
    }
}

#[inline]
pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
