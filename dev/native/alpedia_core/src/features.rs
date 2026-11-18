//! 地形の名所（Earth MOD など。惑星の定義の "features"）：決まった場所の地形を、その種類の形に置き換える。
//!
//! どれも惑星の表面の局所の座標 (x, z)（m。中心から、向き e1・e2）で形を決め、縁（半径の 0.8〜1）でまわりの地形となめらかにつなぐ。
//! 高さのほかに、水面（湖・入り江）・気候（気温・湿り気）・地表の色・草木（生やさない・生やす種類）も決める。
//! 地形の高さ・水は planet.rs の height0・tile_values から使うので、当たり判定・近くと遠くの描画・草木がそろう。
//!
//! 種類（現実の名所をもとにした形）：
//!   tepui          テーブル状の山（カナイマ国立公園）         pillars       砂岩の石柱の林（武陵源）
//!   travertine     白い石灰棚と水たまり（ヒエラポリス）       basalt        六角柱の玄武岩の海岸（ジャイアンツコーズウェイ）
//!   dune_lagoons   白い砂丘の間の湖（レンソイス）             cascade_lakes 段々に続く湖（プリトヴィツェ）
//!   star_dunes     赤い大砂丘（ナミブ砂海）                   karst_sea     海から立つ石灰岩の塔（ハロン湾）
//!   high_mountains 雪の高い山脈（ヒマラヤ）                   canyon        地層の縞の大峡谷（グランドキャニオン）

use crate::planet::{fbm_pub as fbm, ridged_pub as ridged};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Tepui,
    Pillars,
    Travertine,
    Basalt,
    DuneLagoons,
    CascadeLakes,
    StarDunes,
    KarstSea,
    HighMountains,
    Canyon,
}

impl Kind {
    pub fn from_name(s: &str) -> Option<Kind> {
        Some(match s {
            "tepui" => Kind::Tepui,
            "pillars" => Kind::Pillars,
            "travertine" => Kind::Travertine,
            "basalt" => Kind::Basalt,
            "dune_lagoons" => Kind::DuneLagoons,
            "cascade_lakes" => Kind::CascadeLakes,
            "star_dunes" => Kind::StarDunes,
            "karst_sea" => Kind::KarstSea,
            "high_mountains" => Kind::HighMountains,
            "canyon" => Kind::Canyon,
            _ => return None,
        })
    }
    pub fn index(self) -> i64 {
        self as i64
    }
    pub fn from_index(i: i64) -> Option<Kind> {
        use Kind::*;
        [Tepui, Pillars, Travertine, Basalt, DuneLagoons, CascadeLakes, StarDunes, KarstSea, HighMountains, Canyon]
            .get(i as usize)
            .copied()
    }
}

#[derive(Clone, Debug)]
pub struct Feature {
    pub kind: Kind,
    /// 中心の向き（惑星ローカルの単位ベクトル）と、表面の向き e1（x）・e2（z）
    pub dir: [f64; 3],
    pub e1: [f64; 3],
    pub e2: [f64; 3],
    /// 半径（m。地表に沿って）と、高さの目安（m）
    pub radius: f64,
    pub height: f64,
    pub seed: i64,
}

/// 地表の色・草木（feature_paint の戻り値）
#[derive(Clone, Copy, Debug, Default)]
pub struct Paint {
    pub rgb: [f64; 3],
    /// 色をどれだけ使うか（0〜1）
    pub w: f64,
    /// 草木を生やさない度合い（0〜1）
    pub bare: f64,
    /// 生やす木の種類（vegetation.rs の絵の番号。-1 は気候のまま）
    pub tree: i64,
}

#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
fn ss(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
#[inline]
fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
#[inline]
fn hash2(i: i64, j: i64, s: i64) -> f64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ (s as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h >> 11) as f64 / (1u64 << 53) as f64
}
/// 2 次元のノイズ（局所の座標 (x, z) を波長 w m で）
#[inline]
fn n2(x: f64, z: f64, w: f64, seed: i64, oct: i64) -> f64 {
    fbm([x / w, z / w, seed as f64 * 0.37], seed, oct)
}
#[inline]
fn r2(x: f64, z: f64, w: f64, seed: i64, oct: i64) -> f64 {
    ridged([x / w, z / w, seed as f64 * 0.41], seed, oct)
}
/// 段々（地層・棚）：x を step ごとの段に。段の間の崖は edge（0〜1）の幅
#[inline]
fn steps(x: f64, step: f64, edge: f64) -> f64 {
    let y = x / step;
    let f = y.floor();
    (f + ss(1.0 - edge, 1.0, y - f)) * step
}

/// 円柱・塔を並べる：セル cell m ごとに、確率 p で、半径 r0〜r1・高さ h0〜h1 の柱。(柱の高さの形, 一番近い柱の中心までの距離 ÷ 半径)
fn columns(x: f64, z: f64, cell: f64, p: f64, r: (f64, f64), h: (f64, f64), seed: i64, round: f64) -> (f64, f64) {
    let (ci, cj) = ((x / cell).floor() as i64, (z / cell).floor() as i64);
    let mut best = 0.0f64;
    let mut near = 9.0f64;
    for di in -1..=1 {
        for dj in -1..=1 {
            let (i, j) = (ci + di, cj + dj);
            if hash2(i, j, seed) > p {
                continue;
            }
            let cx = (i as f64 + 0.2 + 0.6 * hash2(i, j, seed + 1)) * cell;
            let cz = (j as f64 + 0.2 + 0.6 * hash2(i, j, seed + 2)) * cell;
            let rr = mix(r.0, r.1, hash2(i, j, seed + 3));
            let hh = mix(h.0, h.1, hash2(i, j, seed + 4).powf(0.7));
            // 少しでこぼこな輪郭
            let ang = (z - cz).atan2(x - cx);
            let wob = 1.0 + 0.15 * (ang * 3.0 + hash2(i, j, seed + 5) * 6.28).sin() + 0.08 * (ang * 7.0).sin();
            let d = ((x - cx).powi(2) + (z - cz).powi(2)).sqrt() / (rr * wob);
            near = near.min(d);
            if d < 1.0 {
                // 側面は急、上は丸み（round：上の丸みの割合）
                let top = hh * (1.0 - round * d * d);
                let side = ss(1.0, 0.86, d);
                best = best.max(top * side);
            }
        }
    }
    (best, near)
}

impl Feature {
    /// 点 d（単位ベクトル）の局所の座標 (x, z, 中心からの距離 ÷ 半径)。範囲の外なら None
    #[inline]
    pub fn local(&self, d: [f64; 3], r_planet: f64) -> Option<(f64, f64, f64)> {
        let c = dot(d, self.dir);
        if c < (self.radius * 1.02 / r_planet).min(3.0).cos() {
            return None;
        }
        let x = dot(d, self.e1) * r_planet;
        let z = dot(d, self.e2) * r_planet;
        let u = c.clamp(-1.0, 1.0).acos() * r_planet / self.radius;
        if u >= 1.0 {
            return None;
        }
        Some((x, z, u))
    }

    /// 点 d がこの名所の地形になっている度合い（0〜1）
    pub fn weight(&self, d: [f64; 3], r_planet: f64) -> f64 {
        match self.local(d, r_planet) { Some((_, _, u)) => Self::blend(u), None => 0.0 }
    }

    /// まわりの地形となめらかにつなぐ重み（中心 1、半径の 0.8〜1 で 0 へ）
    #[inline]
    fn blend(u: f64) -> f64 {
        1.0 - ss(0.8, 1.0, u)
    }

    /// 地形の高さ（m。基準の半径から）。h：まわりの地形の高さ
    pub fn height(&self, d: [f64; 3], r_planet: f64, h: f64) -> f64 {
        let (x, z, u) = match self.local(d, r_planet) { Some(v) => v, None => return h };
        let w = Self::blend(u);
        let s = self.seed;
        let hh = self.height;
        let fh = match self.kind {
            Kind::Tepui => {
                // テーブル状の山：大きな塊（波長 1.4 km）のしきいを超えた所が切り立った台地。台地の上は平らで少しでこぼこ
                let n = n2(x, z, 1400.0, s, 4) + (n2(x, z, 160.0, s + 9, 2) - 0.5) * 0.03;
                let mask = ss(0.555, 0.565, n);
                let base = 25.0 + (n2(x, z, 500.0, s + 3, 3) - 0.5) * 30.0;
                base + mask * (hh + (n2(x, z, 220.0, s + 5, 3) - 0.5) * 30.0)
            }
            Kind::Pillars => {
                // 石柱の林：30 m ごとに半径 5〜11 m・高さ 0.35〜1 倍の柱（ところどころ密集）。下は森の谷
                let dense = ss(0.35, 0.6, n2(x, z, 700.0, s + 2, 3));
                let (col, _) = columns(x, z, 30.0, 0.25 + 0.45 * dense, (5.0, 11.0), (hh * 0.35, hh), s, 0.08);
                let base = 20.0 + (n2(x, z, 300.0, s + 4, 3) - 0.5) * 25.0;
                base + col
            }
            Kind::Travertine => {
                // 石灰棚：x の向きに上る斜面を 1.6 m ごとの棚に。棚の縁は少し盛り上がって水をためる
                let slope = (x / self.radius + 1.0) * 0.5 * hh + (n2(x, z, 90.0, s, 2) - 0.5) * 6.0;
                let step = 1.6;
                let k = (slope / step).floor();
                let f = slope / step - k;
                10.0 + k * step + ss(0.84, 0.93, f) * 1.5
            }
            Kind::Basalt => {
                // 玄武岩の柱：x の向きに海から陸へ上る海岸（-6 m → +7 m）。0.6 m の六角柱ごとに高さが違い、0.35 m 刻み
                let base = -6.0 + (x / self.radius + 1.0) * 6.5 + (n2(x, z, 60.0, s, 2) - 0.5) * 3.0;
                let size = 0.6;
                let q = (2.0 / 3.0 * x) / size;
                let r = (-1.0 / 3.0 * x + 3f64.sqrt() / 3.0 * z) / size;
                // 六角形の格子の丸め（cube 座標）
                let (cx, cz) = (q, r);
                let cy = -cx - cz;
                let (mut rx, mut ry, mut rz) = (cx.round(), cy.round(), cz.round());
                let (dx, dy, dz) = ((rx - cx).abs(), (ry - cy).abs(), (rz - cz).abs());
                if dx > dy && dx > dz {
                    rx = -ry - rz;
                } else if dy > dz {
                    ry = -rx - rz;
                } else {
                    rz = -rx - ry;
                }
                let _ = ry;
                let jit = hash2(rx as i64, rz as i64, s) * 1.4 - 0.4;
                ((base + jit) / 0.35).round() * 0.35
            }
            Kind::DuneLagoons => {
                // 白い砂丘（三日月の尾根、波長 180 m・高さ 18 m）と、その間の低い所の湖（水面 9 m）
                let ridge = r2(x * 0.8, z * 1.6, 180.0, s, 3);
                6.0 + ridge * hh + (n2(x, z, 600.0, s + 1, 3) - 0.5) * 6.0
            }
            Kind::CascadeLakes => {
                // 段々の湖：x の向きに下る谷（幅 100 m）。140 m ごとに 9 m 下がる湖、まわりは森の丘
                let t = (x / self.radius + 1.0) * 0.5;
                let seg = (t * 2.0 * self.radius / 140.0).floor();
                let level = hh - seg * 9.0;
                let zc = 25.0 * (x / 300.0 + s as f64).sin();
                let side = ((z - zc).abs() - 45.0).max(0.0);
                let bowl = 4.0 * (1.0 - ((z - zc).abs() / 45.0).min(1.0).powi(2));
                level - bowl + 40.0 * ss(0.0, 70.0, side) + side * 0.03 + (n2(x, z, 120.0, s, 3) - 0.5) * 8.0 * ss(0.0, 30.0, side)
            }
            Kind::StarDunes => {
                // 大砂丘：高さ 180 m の尾根（波長 1.4 km）に小さな尾根と砂紋
                40.0 + r2(x, z, 1400.0, s, 4) * hh + r2(x, z, 380.0, s + 1, 3) * hh * 0.22 + r2(x * 2.0, z, 35.0, s + 2, 1) * 1.2
            }
            Kind::KarstSea => {
                // 海（-18 m）から立つ石灰岩の塔：90 m ごとに半径 18〜45 m・高さ 0.3〜1 倍、上は丸い
                let (col, _) = columns(x, z, 90.0, 0.55, (18.0, 45.0), (hh * 0.3, hh), s, 0.35);
                let sea = -18.0 + (n2(x, z, 400.0, s + 1, 2) - 0.5) * 6.0;
                if col > 0.0 { (sea + col).max(sea) } else { sea }
            }
            Kind::HighMountains => {
                // 高い山脈：中心ほど高い尾根（波長 9 km と 2.5 km）。高い所は寒くなって雪（気温は高さで下がる）
                let m = 1.0 - u * u;
                h.max(60.0) + (r2(x, z, 9000.0, s, 5) * 0.75 + r2(x, z, 2500.0, s + 1, 4) * 0.25) * hh * m
            }
            Kind::Canyon => {
                // 大峡谷：高さ 450 m の台地を、曲がりくねった川が深さ hh・上の幅 900 m で削る。壁は地層の段々
                let zc = 600.0 * (x / 2400.0 + s as f64).sin() + 250.0 * (x / 900.0 + 1.0).sin();
                let dist = (z - zc).abs();
                let half = 450.0 + (n2(x, z, 1200.0, s + 2, 2) - 0.5) * 200.0;
                let depth_f = 1.0 - (dist / half).min(1.0);
                let cut = steps(depth_f.powf(0.8) * hh, 38.0, 0.35);
                450.0 + (n2(x, z, 900.0, s, 3) - 0.5) * 30.0 - cut
            }
        };
        h * (1.0 - w) + fh * w
    }

    /// 水面の高さ（m。無ければ -1e9）
    pub fn water(&self, d: [f64; 3], r_planet: f64) -> f64 {
        let (x, z, u) = match self.local(d, r_planet) { Some(v) => v, None => return -1e9 };
        if u > 0.9 {
            return -1e9;
        }
        let s = self.seed;
        match self.kind {
            Kind::Travertine => {
                // 棚ごとの水たまり（ところどころ乾いている）
                let slope = (x / self.radius + 1.0) * 0.5 * self.height + (n2(x, z, 90.0, s, 2) - 0.5) * 6.0;
                let k = (slope / 1.6).floor();
                if n2(x, z, 25.0, s + 7, 2) > 0.42 { 10.0 + k * 1.6 + 1.25 } else { -1e9 }
            }
            Kind::DuneLagoons => 9.0,
            Kind::CascadeLakes => {
                let t = (x / self.radius + 1.0) * 0.5;
                let seg = (t * 2.0 * self.radius / 140.0).floor();
                let zc = 25.0 * (x / 300.0 + s as f64).sin();
                if (z - zc).abs() < 48.0 { self.height - seg * 9.0 - 0.2 } else { -1e9 }
            }
            Kind::Canyon => {
                let zc = 600.0 * (x / 2400.0 + s as f64).sin() + 250.0 * (x / 900.0 + 1.0).sin();
                if (z - zc).abs() < 60.0 { 450.0 - self.height + 4.0 } else { -1e9 }
            }
            _ => -1e9,
        }
    }

    /// 気候の上書き：(気温, 湿り気, 重み)
    pub fn climate(&self, d: [f64; 3], r_planet: f64) -> Option<(f64, f64, f64)> {
        let (_, _, u) = self.local(d, r_planet)?;
        let w = Self::blend(u);
        let (t, m) = match self.kind {
            Kind::Tepui => (0.85, 0.92),
            Kind::Pillars => (0.55, 0.9),
            Kind::Travertine => (0.62, 0.3),
            Kind::Basalt => (0.4, 0.6),
            Kind::DuneLagoons => (0.7, 0.15),
            Kind::CascadeLakes => (0.45, 0.88),
            Kind::StarDunes => (0.9, 0.02),
            Kind::KarstSea => (0.78, 0.88),
            Kind::HighMountains => return None,           // 高さで決まる（寒くなって雪）
            Kind::Canyon => (0.75, 0.12),
        };
        Some((t, m, w))
    }

    /// 地表の色・草木。h：高さ、slope：傾き（0 平ら 〜 1 垂直）
    pub fn paint(&self, d: [f64; 3], r_planet: f64, h: f64, slope: f64) -> Paint {
        let (x, z, u) = match self.local(d, r_planet) { Some(v) => v, None => return Paint { tree: -1, ..Default::default() } };
        let w = Self::blend(u);
        let steep = ss(0.35, 0.6, slope);
        let s = self.seed;
        let mut p = Paint { tree: -1, ..Default::default() };
        match self.kind {
            Kind::Tepui => {
                // 崖は黒っぽい赤茶の砂岩、上は草と低木
                p.rgb = [0.42, 0.32, 0.27];
                p.w = steep * w;
                p.tree = 9;
            }
            Kind::Pillars => {
                p.rgb = [0.63, 0.58, 0.5];
                p.w = steep * w;
                p.tree = if h > 60.0 { 12 } else { -1 };
            }
            Kind::Travertine => {
                p.rgb = [0.95, 0.95, 0.92];
                p.w = w;
                p.bare = w;
            }
            Kind::Basalt => {
                p.rgb = [0.2, 0.2, 0.22];
                p.w = w * ss(-3.0, 1.0, h) * ss(9.0, 4.0, h + (n2(x, z, 50.0, s, 2) - 0.5) * 4.0);
                p.bare = p.w;
            }
            Kind::DuneLagoons => {
                p.rgb = [0.97, 0.95, 0.88];
                p.w = w;
                p.bare = w;
            }
            Kind::CascadeLakes => {
                p.rgb = [0.85, 0.83, 0.74];                   // 湖の縁の石灰華
                p.w = w * ss(2.0, 0.0, (h - self.water(d, r_planet)).abs()) * 0.7;
            }
            Kind::StarDunes => {
                let k = n2(x, z, 300.0, s, 2);
                p.rgb = [0.78 + 0.08 * k, 0.42 + 0.08 * k, 0.22];
                p.w = w;
                p.bare = w;
            }
            Kind::KarstSea => {
                p.rgb = [0.58, 0.58, 0.54];
                p.w = steep * w;
                p.tree = 9;
            }
            Kind::HighMountains => {
                p.rgb = [0.42, 0.4, 0.4];
                p.w = steep * w * 0.6;
            }
            Kind::Canyon => {
                // 地層の縞（25 m ごとに色が変わる）。台地の上は乾いた草地
                let band = ((h + (n2(x, z, 400.0, s, 2) - 0.5) * 8.0) / 25.0).floor() as i64;
                let cols = [[0.72, 0.38, 0.24], [0.82, 0.55, 0.33], [0.86, 0.73, 0.52], [0.66, 0.36, 0.26], [0.78, 0.48, 0.3]];
                let c = cols[band.rem_euclid(5) as usize];
                let top = ss(430.0, 445.0, h);
                p.rgb = [mix(c[0], 0.62, top), mix(c[1], 0.56, top), mix(c[2], 0.4, top)];
                p.w = w;
                p.bare = w * (1.0 - top * 0.6);
            }
        }
        p
    }
}

/// 局所の座標 (x, z) の向き（名所の中心のまわり）
fn dir_at(f: &Feature, x: f64, z: f64, r_planet: f64) -> [f64; 3] {
    let v = [f.dir[0] + f.e1[0] * x / r_planet + f.e2[0] * z / r_planet,
             f.dir[1] + f.e1[1] * x / r_planet + f.e2[1] * z / r_planet,
             f.dir[2] + f.e1[2] * x / r_planet + f.e2[2] * z / r_planet];
    let n = dot(v, v).sqrt();
    [v[0] / n, v[1] / n, v[2] / n]
}

/// 滝：[上の点 xyz, 下の点 xyz, 幅]（惑星ローカル、m）。h_of(d)：地表の高さ（基準の半径から）
pub fn waterfalls(fs: &[Feature], r_planet: f64, h_of: &dyn Fn([f64; 3]) -> f64) -> Vec<[f64; 7]> {
    let mut out = vec![];
    let pos = |d: [f64; 3], h: f64| [d[0] * (r_planet + h), d[1] * (r_planet + h), d[2] * (r_planet + h)];
    for f in fs {
        match f.kind {
            Kind::CascadeLakes => {
                // 湖と湖の段（140 m ごとに 9 m）に、谷の真ん中の幅 28 m の滝
                let mut k = 1;
                loop {
                    let xb = 140.0 * k as f64 - f.radius;
                    if xb > f.radius * 0.75 {
                        break;
                    }
                    if xb > -f.radius * 0.75 {
                        let zc = 25.0 * (xb / 300.0 + f.seed as f64).sin();
                        let top = f.height - (k - 1) as f64 * 9.0 - 0.2;
                        let bot = f.height - k as f64 * 9.0 - 0.2;
                        let dt = dir_at(f, xb - 3.0, zc, r_planet);
                        let db = dir_at(f, xb + 7.0, zc, r_planet);
                        let (a, b) = (pos(dt, top), pos(db, bot));
                        out.push([a[0], a[1], a[2], b[0], b[1], b[2], 28.0]);
                    }
                    k += 1;
                }
            }
            Kind::Tepui => {
                // 台地の縁の、いちばん高い崖を 3 つまで（800 m 以上離して）。下は崖の麓まで
                let step = 40.0;
                let lim = f.radius * 0.7;
                let n = (2.0 * lim / step) as i64;
                let mut cand: Vec<(f64, f64, f64, f64, f64)> = vec![];       // (落差, x, z, 向き x, 向き z)
                for i in 0..n {
                    for j in 0..n {
                        let (x, z) = (-lim + i as f64 * step, -lim + j as f64 * step);
                        let h0 = h_of(dir_at(f, x, z, r_planet));
                        if h0 < f.height * 0.7 {
                            continue;
                        }
                        for (ux, uz) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                            let h1 = h_of(dir_at(f, x + ux * 60.0, z + uz * 60.0, r_planet));
                            if h0 - h1 > f.height * 0.5 {
                                cand.push((h0 - h1, x, z, ux, uz));
                            }
                        }
                    }
                }
                cand.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                let mut picked: Vec<(f64, f64)> = vec![];
                for (_, x, z, ux, uz) in cand {
                    if picked.iter().any(|p| (p.0 - x).hypot(p.1 - z) < 800.0) {
                        continue;
                    }
                    // 縁：台地の上から崖の方へ、高さが下がり始める所
                    let mut ex = x;
                    let mut ez = z;
                    let htop = h_of(dir_at(f, x, z, r_planet));
                    for s in 0..15 {
                        let (nx, nz) = (x + ux * 4.0 * s as f64, z + uz * 4.0 * s as f64);
                        if h_of(dir_at(f, nx, nz, r_planet)) < htop - 3.0 {
                            break;
                        }
                        ex = nx;
                        ez = nz;
                    }
                    // 麓：下がらなくなるまで
                    let (mut bx, mut bz) = (ex, ez);
                    let mut hb = h_of(dir_at(f, ex, ez, r_planet));
                    for _ in 0..80 {
                        let (nx, nz) = (bx + ux * 8.0, bz + uz * 8.0);
                        let hn = h_of(dir_at(f, nx, nz, r_planet));
                        if hn > hb - 1.0 {
                            break;
                        }
                        bx = nx;
                        bz = nz;
                        hb = hn;
                    }
                    let a = pos(dir_at(f, ex, ez, r_planet), htop - 1.0);
                    let b = pos(dir_at(f, bx, bz, r_planet), hb);
                    out.push([a[0], a[1], a[2], b[0], b[1], b[2], 22.0]);
                    picked.push((x, z));
                    if picked.len() >= 3 {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}
