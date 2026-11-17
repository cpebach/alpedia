//! 描画：草・木（A-02）。立体のモデルではなく、絵（sauce/textures/plants.png）を貼った板を十字に 2 枚組み合わせる。
//!
//! 置き場所は惑星ローカルの決まった格子（キューブの面ごとに、角度で等間隔）の中の乱数（格子の番号から作るので、いつも同じ所）。
//! 気候（気温・湿り気）で種類と数を決める。水の中・浜・掘った穴の中には生えない。
//! 草はプレイヤーのまわり GRASS_R m、木は TREE_R m。プレイヤーが動いたら作り直す（草は 6 m、木は 25 m ごと）。

use super::*;

/// 絵の並び（4 列 × 2 段）と大きさ（幅, 高さ m）
const SPRITES: [(f64, f64); 8] = [
    (0.9, 0.55),   // 0 草
    (0.8, 0.5),    // 1 花のある草
    (6.5, 9.0),    // 2 広葉樹
    (4.2, 10.0),   // 3 針葉樹
    (5.5, 9.5),    // 4 ヤシ
    (1.6, 3.2),    // 5 サボテン
    (1.8, 1.4),    // 6 枯れた低木
    (4.2, 9.5),    // 7 雪の針葉樹
];
const GRASS_R: f64 = 70.0;
const GRASS_CELL: f64 = 4.0;
const TREE_R: f64 = 450.0;
const TREE_CELL: f64 = 16.0;

/// なめらかな段（a → b で 0 → 1）
fn ss(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(a: i64, b: i64, c: i64, k: u32) -> u32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77) ^ (c as u32).wrapping_mul(0xC2B2_AE3D)
        ^ k.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}
fn rnd(a: i64, b: i64, c: i64, k: u32) -> f64 {
    hash(a, b, c, k) as f64 / u32::MAX as f64
}

/// 面 f の角度 (α, β) の向き
fn face_dir(f: usize, al: f64, be: f64) -> [f64; 3] {
    let (major, neg) = (f / 2, f % 2 == 1);
    let mut d = [0.0; 3];
    d[major] = if neg { -1.0 } else { 1.0 };
    d[(major + 1) % 3] = al.tan();
    d[(major + 2) % 3] = be.tan();
    let n = dot(d, d).sqrt();
    [d[0] / n, d[1] / n, d[2] / n]
}

/// 候補：(向き, 絵の番号, 大きさの倍率, 板の向き)
type Cand = ([f64; 3], usize, f64, f64);

#[derive(Default)]
pub struct Veg {
    grass_at: Option<[f64; 3]>,
    tree_at: Option<[f64; 3]>,
    version: i64,
}

impl Veg {
    /// 層（草か木）の候補の向き。cam は惑星ローカルのカメラ、r は惑星の半径
    fn cands(cam: [f64; 3], r: f64, range: f64, cell: f64, layer: u32) -> Vec<([f64; 3], f64, f64, f64)> {
        let cn = dot(cam, cam).sqrt().max(1e-9);
        let cd = [cam[0] / cn, cam[1] / cn, cam[2] / cn];
        let step = cell / r;
        let span = (range / cell).ceil() as i64 + 1;
        let lim = std::f64::consts::FRAC_PI_4;
        let mut out = vec![];
        for f in 0..6usize {
            let (major, neg) = (f / 2, f % 2 == 1);
            let s = if neg { -1.0 } else { 1.0 };
            let x0 = cd[major] * s;
            if x0 < 0.05 {
                continue;
            }
            let pa = (cd[(major + 1) % 3] / x0).atan();
            let pb = (cd[(major + 2) % 3] / x0).atan();
            let (ia, ib) = ((pa / step).floor() as i64, (pb / step).floor() as i64);
            for i in ia - span..=ia + span {
                for j in ib - span..=ib + span {
                    let (h1, h2, h3, h4) = (rnd(i, j, f as i64, layer * 4), rnd(i, j, f as i64, layer * 4 + 1),
                                            rnd(i, j, f as i64, layer * 4 + 2), rnd(i, j, f as i64, layer * 4 + 3));
                    let (al, be) = ((i as f64 + h1) * step, (j as f64 + h2) * step);
                    if al.abs() > lim || be.abs() > lim {
                        continue;
                    }
                    let d = face_dir(f, al, be);
                    let q = [d[0] - cd[0], d[1] - cd[1], d[2] - cd[2]];
                    if dot(q, q).sqrt() * r > range {
                        continue;
                    }
                    out.push((d, h3, h4, rnd(i, j, f as i64, layer * 4 + 99)));
                }
            }
        }
        out
    }

    /// 気候 (t, m) と高さから、生える物（絵の番号と、生える確率）
    fn pick(t: f64, m: f64, grass: bool) -> Option<(usize, f64)> {
        let arid = ss(0.45, 0.75, t) * ss(0.5, 0.2, m);
        if t < -0.15 {
            return None;
        }
        if grass {
            if arid > 0.6 || t < 0.05 {
                return None;
            }
            let p = (0.25 + 0.75 * m) * (1.0 - arid) * ss(0.05, 0.25, t);
            return Some((if m > 0.45 && t > 0.3 { 1 } else { 0 }, p));
        }
        if t < 0.05 {
            return Some((7, 0.25 * m));                                    // 雪の針葉樹（まばら）
        }
        if arid > 0.6 {
            return Some((if m < 0.12 { 5 } else { 6 }, 0.12));             // 砂漠：サボテン・枯れた低木
        }
        if t < 0.35 {
            return Some((3, 0.15 + 0.6 * m));                               // 寒い：針葉樹
        }
        if t > 0.7 && m > 0.6 {
            return Some((4, 0.5 * m));                                      // 暑くて湿った：ヤシ
        }
        if arid > 0.3 {
            return Some((if m > 0.3 { 2 } else { 6 }, 0.06));               // サバンナ：まばら
        }
        Some((if t < 0.5 && m > 0.5 { 3 } else { 2 }, (m - 0.2).max(0.0) * 0.7))  // 温かい：森（湿り気で増える）
    }

    /// 1 つの層のメッシュ（頂点 (N,12)・三角形）
    fn layer(planet: &B<'_>, cam: [f64; 3], grass: bool) -> R<(Vec<f32>, Vec<i64>)> {
        let py = planet.py();
        let r: f64 = f64of(&planet.getattr("radius")?)?;
        let (range, cell, lay) = if grass { (GRASS_R, GRASS_CELL, 1) } else { (TREE_R, TREE_CELL, 2) };
        let cs = Self::cands(cam, r, range, cell, lay);
        if cs.is_empty() {
            return Ok((vec![], vec![]));
        }
        let dirs: Vec<f64> = cs.iter().flat_map(|c| c.0).collect();
        let d_o = f64_2(py, dirs.clone(), 3)?;
        let sr = a1(&planet.call_method1("surface_radius_exact", (&d_o,))?)?;
        let wr = match planet.call_method1("water_radius", (&d_o,)) { Ok(w) => a1(&w)?, Err(_) => vec![0.0; cs.len()] };
        let sea: f64 = if planet.getattr("has_water")?.is_truthy()? { planet.getattr("sea").and_then(|x| x.extract()).unwrap_or(0.0) } else { -1e9 };
        let h: Vec<f64> = sr.iter().map(|x| x - r).collect();
        let nat = planet.call_method0("_nat")?;
        let clim = match nat.call_method1("climate", (&d_o, f64_1(py, h.clone()))) {
            Ok(c) => a2(&c, 2)?.d,
            Err(_) => vec![0.5; 2 * cs.len()],
        };
        // 掘った穴の中には生やさない（地表のすぐ上の点が地中なら掘ってある）
        let pts: Vec<f64> = (0..cs.len()).flat_map(|i| { let d = cs[i].0; let rr = sr[i] + 0.3; [d[0] * rr, d[1] * rr, d[2] * rr] }).collect();
        let ctr = v3(&planet.getattr("center")?)?;
        let wp: Vec<f64> = pts.iter().enumerate().map(|(k, x)| x + ctr[k % 3]).collect();
        let sdf = a1(&planet.call_method1("sdf", (f64_2(py, wp, 3)?,))?)?;
        let (s_, amb) = (sun(py)?, ambient(py)?);
        let mut v: Vec<f32> = vec![];
        let mut t: Vec<i64> = vec![];
        for (i, c) in cs.iter().enumerate() {
            let (d, h3, h4, rot) = *c;
            if wr[i] > sr[i] - 0.05 || h[i] < sea + 1.2 || sdf[i] < -0.1 {
                continue;
            }
            let (tt, mm) = (clim[2 * i], clim[2 * i + 1]);
            let (sp, p) = match Self::pick(tt, mm, grass) { Some(x) => x, None => continue };
            if h3 > p {
                continue;
            }
            let (w0, h0) = SPRITES[sp];
            let k = 0.7 + 0.6 * h4;
            let (w, hh) = (w0 * k, h0 * k);
            // 板の向き：上 = d、横 = 接線（乱数で回す）
            let (t1, t2) = {
                let a = if d[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
                let x = cross(a, d);
                let xn = dot(x, x).sqrt();
                let x = [x[0] / xn, x[1] / xn, x[2] / xn];
                (x, cross(d, x))
            };
            let ang = rot * std::f64::consts::PI;
            let (ca, sa) = (ang.cos(), ang.sin());
            let e1 = [t1[0] * ca + t2[0] * sa, t1[1] * ca + t2[1] * sa, t1[2] * ca + t2[2] * sa];
            let e2 = [t2[0] * ca - t1[0] * sa, t2[1] * ca - t1[1] * sa, t2[2] * ca - t1[2] * sa];
            let base = [d[0] * (sr[i] - 0.15), d[1] * (sr[i] - 0.15), d[2] * (sr[i] - 0.15)];
            // 色：気候で少し変える（乾いた所は黄色っぽく）、上からの光で明るさ
            let dry = ss(0.6, 0.15, mm);
            let tint = [1.0 + 0.15 * dry, 1.0, 1.0 - 0.35 * dry];
            let sh = shade_rs(s_, amb, d, [tint[0] * 0.95, tint[1] * 0.95, tint[2] * 0.95]);
            // 絵の段：画像の上の段が v の大きい方（v = 1 が画像の上）
            let (u0, vb) = ((sp % 4) as f64 * 0.25, 1.0 - ((sp / 4) as f64 + 1.0) * 0.5);
            for e in [e1, e2] {
                let b = (v.len() / 12) as i64;
                let corners = [(-0.5, 0.0, u0, vb), (0.5, 0.0, u0 + 0.25, vb), (0.5, 1.0, u0 + 0.25, vb + 0.5), (-0.5, 1.0, u0, vb + 0.5)];
                for (sx, sy, u, vv) in corners {
                    let p = [base[0] + e[0] * sx * w + d[0] * sy * hh, base[1] + e[1] * sx * w + d[1] * sy * hh, base[2] + e[2] * sx * w + d[2] * sy * hh];
                    v.extend_from_slice(&[p[0] as f32, p[1] as f32, p[2] as f32, d[0] as f32, d[1] as f32, d[2] as f32,
                                          sh[0] as f32, sh[1] as f32, sh[2] as f32, 1.0, u as f32, vv as f32]);
                }
                t.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
            }
        }
        Ok((v, t))
    }

    /// 毎フレーム：作り直す層があれば (草, 木) の新しいメッシュ（変わらない層は None）
    #[allow(clippy::type_complexity)]
    pub fn update(&mut self, planet: &B<'_>, cam: [f64; 3]) -> R<(Option<(Vec<f32>, Vec<i64>)>, Option<(Vec<f32>, Vec<i64>)>)> {
        let r: f64 = f64of(&planet.getattr("radius")?)?;
        let alt = dot(cam, cam).sqrt() - r;
        let ver: i64 = planet.getattr("version")?.extract()?;
        let changed = ver != self.version;
        self.version = ver;
        let far = |a: &Option<[f64; 3]>, lim: f64| match a {
            None => true,
            Some(p) => { let q = [cam[0] - p[0], cam[1] - p[1], cam[2] - p[2]]; dot(q, q).sqrt() > lim }
        };
        let mut g = None;
        let mut tr = None;
        if alt > TREE_R + 200.0 {
            // 高い所：何も描かない
            if self.tree_at.is_some() || self.grass_at.is_some() {
                self.tree_at = None;
                self.grass_at = None;
                return Ok((Some((vec![], vec![])), Some((vec![], vec![]))));
            }
            return Ok((None, None));
        }
        if changed || far(&self.grass_at, 6.0) {
            self.grass_at = Some(cam);
            g = Some(if alt < GRASS_R { Self::layer(planet, cam, true)? } else { (vec![], vec![]) });
        }
        if changed || far(&self.tree_at, 25.0) {
            self.tree_at = Some(cam);
            tr = Some(Self::layer(planet, cam, false)?);
        }
        Ok((g, tr))
    }
}
