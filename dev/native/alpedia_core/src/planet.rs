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
}

pub struct Shape {
    pub radius: f64,
    pub amp: f64,
    pub freq: f64,
    pub seed: i64,
    pub earth: Option<Earth>,
    n: usize,
    tiles: RwLock<(HashMap<(usize, i64, i64), Arc<Vec<[f64; 2]>>>, VecDeque<(usize, i64, i64)>)>,
}

pub const TILE: usize = 32;
const RES: f64 = 0.25;
const MAX_TILES: usize = 4000;

impl Shape {
    pub fn new(radius: f64, amp: f64, freq: f64, seed: i64, earth: Option<Earth>) -> Self {
        let n = ((std::f64::consts::PI / 2.0 * radius / RES / TILE as f64).ceil() as usize) * TILE;
        Shape { radius, amp, freq, seed, earth, n, tiles: RwLock::new((HashMap::new(), VecDeque::new())) }
    }

    fn ang(&self, d: [f64; 3], r: [f64; 3]) -> f64 {
        dot(d, r).clamp(-1.0, 1.0).acos() * self.radius
    }

    /// 池を除いた地表の高さ（地球型）
    pub fn height0(&self, e: &Earth, d: [f64; 3]) -> f64 {
        let sd = self.seed;
        let mut cont = (fbm(affine(d, 1.3, 11.0), sd, 4) - 0.545) / 0.1;
        let a_sp = self.ang(d, [0.0, 1.0, 0.0]);
        let e_sp = (-(a_sp / 300.0).powi(2)).exp();
        cont = cont * (1.0 - e_sp) + (cont.max(1.2) + 0.3) * e_sp;
        let base = if cont > 0.0 { cont * 6.0 } else { cont * 12.0 };
        let hills = (fbm(affine(d, 9.0, 5.0), sd + 1, 3) - 0.5) * 2.5 * e.hill_h;
        let ridge = ridged(affine(d, 3.2, 3.0), sd + 2, 4);
        let rough = ridged(affine(d, 11.0, 7.0), sd + 4, 3) * 0.6 + ridged(affine(d, 27.0, 1.0), sd + 5, 2) * 0.4;
        let mut rng = smoothstep(0.52, 0.66, fbm(affine(d, 1.6, 40.0), sd + 3, 3)) * smoothstep(0.2, 1.2, cont);
        let mdir = [0.0, (170.0 / self.radius).cos(), -(170.0 / self.radius).sin()];
        let am = self.ang(d, mdir);
        rng = rng.max((-(am / 70.0).powi(2)).exp());
        let mtn = rng * (ridge * 0.55 + 0.45) * (0.65 + 0.35 * rough);
        let h = base + hills + mtn * e.mountain_h * 1.1;
        let w = 1.0 - smoothstep(e.flat_r, e.flat_r * 1.9, a_sp);
        h * (1.0 - w) + (e.spawn_h + hills * 0.15) * w
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
    fn tile_values(&self, dirs: &[[f64; 3]]) -> Vec<[f64; 2]> {
        match &self.earth {
            None => dirs
                .iter()
                .map(|&d| {
                    let n = fbm(affine(d, self.freq, 100.0), self.seed, 3);
                    [self.radius + (n - 0.5) * 2.0 * self.amp, 0.0]
                })
                .collect(),
            Some(e) => {
                let mut h: Vec<f64> = dirs.iter().map(|&d| self.height0(e, d)).collect();
                let water = self.apply_ponds(e, dirs, &mut h);
                h.iter()
                    .zip(water.iter())
                    .map(|(&hh, &w0)| {
                        let w = w0.max(e.sea);
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
