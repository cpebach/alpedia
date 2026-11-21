//! 舗装（地表の色を塗り替える。MOD の街・道など）。
//!
//! - 面（Grid）：場所の中心の向き d と地表に沿った軸 e1・e2 の上の、cell m 角のマス目。マスの値 0 は塗らない、
//!   1 以上は色の表（pal）の番号。となりのマスと線形に混ぜるので、縁はなめらか。
//! - 帯（Strip）：地表に沿った折れ線（向きの並び）の、幅 half m（縁 edge m でぼかす）。
//!
//! 塗った所には草木を生やさない（vegetation.rs）。座標は惑星ローカルの単位ベクトル。

#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
fn ss(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub struct Grid {
    pub d: [f64; 3],
    pub e1: [f64; 3],
    pub e2: [f64; 3],
    pub x0: f64,
    pub z0: f64,
    pub cell: f64,
    pub nx: usize,
    pub nz: usize,
    pub codes: Vec<u8>,
    pub pal: Vec<[f64; 3]>,
    /// この cos より d から離れた向きは調べない
    pub cmin: f64,
}

pub struct Strip {
    pub pts: Vec<[f64; 3]>,
    pub half: f64,
    pub edge: f64,
    pub rgb: [f64; 3],
    /// 区切り（i0, i1, 中心の向き, 中心から点までの角度の最大）：折れ線を 16 点ずつに分けた囲み
    chunks: Vec<(usize, usize, [f64; 3], f64)>,
}

pub enum Pave {
    Grid(Grid),
    Strip(Strip),
}

impl Grid {
    pub fn new(d: [f64; 3], e1: [f64; 3], e2: [f64; 3], x0: f64, z0: f64, cell: f64, nx: usize, nz: usize,
               codes: Vec<u8>, pal: Vec<[f64; 3]>, radius: f64) -> Self {
        let far = [x0.abs(), (x0 + nx as f64 * cell).abs()].iter().cloned().fold(0.0, f64::max)
            .hypot([z0.abs(), (z0 + nz as f64 * cell).abs()].iter().cloned().fold(0.0, f64::max));
        let cmin = ((far + 2.0 * cell) / radius).min(3.0).cos();
        Grid { d, e1, e2, x0, z0, cell, nx, nz, codes, pal, cmin }
    }

    /// (色の重みつきの和, 重み)
    fn at(&self, q: [f64; 3], radius: f64) -> Option<([f64; 3], f64)> {
        let c = dot(q, self.d);
        if c < self.cmin || c <= 0.0 {
            return None;
        }
        // 地表に沿った座標（plan の along の逆）
        let x = radius * dot(q, self.e1) / c;
        let z = radius * dot(q, self.e2) / c;
        let fx = (x - self.x0) / self.cell - 0.5;
        let fz = (z - self.z0) / self.cell - 0.5;
        if fx < -1.0 || fz < -1.0 || fx > self.nx as f64 || fz > self.nz as f64 {
            return None;
        }
        let (ix, iz) = (fx.floor() as i64, fz.floor() as i64);
        let (tx, tz) = (fx - ix as f64, fz - iz as f64);
        let mut rgb = [0.0; 3];
        let mut w = 0.0;
        for (dx, dz, k) in [(0, 0, (1.0 - tx) * (1.0 - tz)), (1, 0, tx * (1.0 - tz)), (0, 1, (1.0 - tx) * tz), (1, 1, tx * tz)] {
            let (a, b) = (ix + dx, iz + dz);
            if a < 0 || b < 0 || a >= self.nx as i64 || b >= self.nz as i64 || k <= 0.0 {
                continue;
            }
            let code = self.codes[a as usize * self.nz + b as usize] as usize;
            if code == 0 || code > self.pal.len() {
                continue;
            }
            let p = self.pal[code - 1];
            for j in 0..3 {
                rgb[j] += p[j] * k;
            }
            w += k;
        }
        if w <= 0.0 { None } else { Some((rgb, w)) }
    }
}

impl Strip {
    pub fn new(pts: Vec<[f64; 3]>, half: f64, edge: f64, rgb: [f64; 3], _radius: f64) -> Self {
        let mut chunks = vec![];
        let n = pts.len();
        let mut i0 = 0;
        while i0 + 1 < n {
            let i1 = (i0 + 16).min(n - 1);
            let mut c = [0.0; 3];
            for p in &pts[i0..=i1] {
                for j in 0..3 {
                    c[j] += p[j];
                }
            }
            let l = dot(c, c).sqrt().max(1e-12);
            let c = [c[0] / l, c[1] / l, c[2] / l];
            let ang = pts[i0..=i1].iter().map(|p| dot(*p, c).clamp(-1.0, 1.0).acos()).fold(0.0, f64::max);
            chunks.push((i0, i1, c, ang));
            i0 = i1;
        }
        Strip { pts, half, edge, rgb, chunks }
    }

    /// blur：見る点の間隔（m。遠景の粗いメッシュ）。帯が間隔より細いときは薄く広げる（ぎざぎざにしない）
    fn at(&self, q: [f64; 3], radius: f64, blur: f64) -> Option<([f64; 3], f64)> {
        // 粗いメッシュは三角形で色を補間するので、間隔の 2.5 倍に広げる（斜めの道のぎざぎざを消す）
        let e = self.edge.max(2.5 * blur).max(1e-6);
        let k = if blur > 2.0 * self.half { 2.0 * self.half / blur } else { 1.0 };
        let reach = self.half + self.edge / 2.0 + e / 2.0;
        let mut best = f64::INFINITY;
        let pad = (reach + 1.0) / radius;
        for (i0, i1, c, ang) in &self.chunks {
            if dot(q, *c) < (ang + pad).min(3.0).cos() {
                continue;
            }
            for i in *i0..*i1 {
                let (a, b) = (self.pts[i], self.pts[i + 1]);
                let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let aq = [q[0] - a[0], q[1] - a[1], q[2] - a[2]];
                let t = (dot(aq, ab) / dot(ab, ab).max(1e-30)).clamp(0.0, 1.0);
                let e = [aq[0] - ab[0] * t, aq[1] - ab[1] * t, aq[2] - ab[2] * t];
                best = best.min(dot(e, e));
            }
        }
        if !best.is_finite() {
            return None;
        }
        let dist = best.sqrt() * radius;
        let mid = self.half + self.edge / 2.0;
        let w = k * (1.0 - ss(mid - e / 2.0, mid + e / 2.0, dist));
        if w <= 0.0 { None } else { Some(([self.rgb[0] * w, self.rgb[1] * w, self.rgb[2] * w], w)) }
    }
}

impl Pave {
    pub fn at(&self, q: [f64; 3], radius: f64, blur: f64) -> Option<([f64; 3], f64)> {
        match self {
            Pave::Grid(g) => g.at(q, radius),
            Pave::Strip(s) => s.at(q, radius, blur),
        }
    }
}

/// いくつもの舗装を重ねる：(色, 重み 0〜1)。重みが 0 なら塗らない。あとに足した物ほど上
pub fn paint(items: &[(String, Pave)], q: [f64; 3], radius: f64, blur: f64) -> ([f64; 3], f64) {
    let mut col = [0.0; 3];
    let mut w = 0.0;
    for (_, p) in items {
        if let Some((rgb, k)) = p.at(q, radius, blur) {
            let k = k.clamp(0.0, 1.0);
            // 上に重ねる（rgb は k を掛けた色）
            for j in 0..3 {
                col[j] = col[j] * (1.0 - k) + rgb[j];
            }
            w = w * (1.0 - k) + k;
        }
    }
    if w > 0.0 {
        (col.map(|c| c / w), w)
    } else {
        (col, 0.0)
    }
}
