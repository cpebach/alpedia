//! 描画：惑星の遠景の細かさを距離で変える（A-01。大きな惑星用）
//!
//! キューブスフィアの各面を四分木に分け、カメラに近い区画ほど細かくする（いちばん細かい区画は約 64 m 四方）。
//! 区画はそれぞれ (G+1)² の格子のメッシュ。隣の区画と細かさが違う所のすき間は、縁を下に垂らした「スカート」で隠す。
//! 足りない区画は 1 フレームに少しずつ作り（できるまでは親の区画を描く）、使う区画の組が変わったときだけ 1 つのメッシュにまとめ直す。
//! 詳細な地表（TerrainPatch）の範囲の三角形は描かない（洞窟や穴を遠景が覆わないように）。

use std::collections::{HashMap, HashSet};

use super::*;

/// 区画の格子の数
const G: usize = 16;
/// 区画の大きさ × この距離より近ければ分ける
const SPLIT_K: f64 = 2.4;
/// いちばん細かい区画の大きさの目安（m）
const LEAF_M: f64 = 64.0;
/// 1 フレームに作る区画の数（まとめて 1 回で地形を計算する）
const BATCH: usize = 3;
/// 区画を作っている途中は、何フレームごとにメッシュをまとめ直すか
const REASSEMBLE_EVERY: u32 = 12;

type Key = (u8, u8, u32, u32);

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// 1 区画のメッシュ（惑星の中心からの位置）
pub struct Chunk {
    verts: Vec<f32>,
    tris: Vec<u32>,
}

#[derive(Default)]
pub struct Lod {
    radius: f64,
    max_level: u8,
    chunks: HashMap<Key, Chunk>,
    shown: Vec<Key>,
    frame_key: Vec<i64>,
    since: u32,
}

/// 面 f の (a, b)（-1〜1）の向き（tan で角度を均等に）
fn face_dir(f: u8, a: f64, b: f64) -> [f64; 3] {
    let (major, neg) = ((f / 2) as usize, f % 2 == 1);
    let ta = (a * std::f64::consts::FRAC_PI_4).tan();
    let tb = (b * std::f64::consts::FRAC_PI_4).tan();
    let mut d = [0.0; 3];
    d[major] = if neg { -1.0 } else { 1.0 };
    d[(major + 1) % 3] = ta;
    d[(major + 2) % 3] = tb;
    let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    [d[0] / n, d[1] / n, d[2] / n]
}

fn node_range(k: Key) -> (f64, f64, f64) {
    let s = 2.0 / (1u32 << k.1) as f64;
    (-1.0 + k.2 as f64 * s, -1.0 + k.3 as f64 * s, s)
}

impl Lod {
    pub fn new(radius: f64) -> Self {
        let face = std::f64::consts::FRAC_PI_2 * radius;
        let mut ml = 0u8;
        while ml < 14 && face / (1u32 << ml) as f64 > LEAF_M {
            ml += 1;
        }
        Lod { radius, max_level: ml, ..Default::default() }
    }

    /// 区画のメッシュを作る（高さと色は惑星の far_samples。法線は 1 マス外まで取って隣の区画とつながるように）
    /// 区画の格子の向き（1 マス外まで）
    fn dirs_of(k: Key) -> Vec<f64> {
        let (a0, b0, s) = node_range(k);
        let h = s / G as f64;
        let m = G + 3;
        let mut dirs = Vec::with_capacity(m * m * 3);
        for i in 0..m {
            for j in 0..m {
                let d = face_dir(k.0, a0 + (i as f64 - 1.0) * h, b0 + (j as f64 - 1.0) * h);
                dirs.extend_from_slice(&d);
            }
        }
        dirs
    }

    /// いくつかの区画をまとめて作る（高さ・色は 1 回の far_samples で。点が多いと地形の計算が並列になる）
    fn build_many(&mut self, planet: &B<'_>, keys: &[Key]) -> R<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let py = planet.py();
        let per = (G + 3) * (G + 3);
        let mut all = Vec::with_capacity(keys.len() * per * 3);
        for &k in keys {
            all.extend(Self::dirs_of(k));
        }
        let fs = planet.call_method1("far_samples", (f64_2(py, all.clone(), 3)?,))?;
        let r = a1(&fs.get_item(0)?)?;
        let col = a2(&fs.get_item(1)?, 3)?;
        for (n, &k) in keys.iter().enumerate() {
            let dirs = &all[n * per * 3..(n + 1) * per * 3];
            let rr = &r[n * per..(n + 1) * per];
            let cc: Vec<[f64; 3]> = (0..per).map(|q| col.r3(n * per + q)).collect();
            let ch = self.build(py, k, dirs, rr, &cc)?;
            self.chunks.insert(k, ch);
        }
        Ok(())
    }

    fn build(&self, py: Python<'_>, k: Key, dirs: &[f64], r: &[f64], col: &[[f64; 3]]) -> R<Chunk> {
        let (_, _, s) = node_range(k);
        let m = G + 3;
        let p = |i: usize, j: usize| -> [f64; 3] {
            let q = i * m + j;
            let rr = r[q] - 0.6;
            [dirs[q * 3] * rr, dirs[q * 3 + 1] * rr, dirs[q * 3 + 2] * rr]
        };
        let (s_, amb) = (sun(py)?, ambient(py)?);
        let n1 = G + 1;
        let cell = s * std::f64::consts::FRAC_PI_4 * self.radius / G as f64;
        let skirt = (cell * 2.0).max(3.0);
        let mut verts: Vec<f32> = Vec::with_capacity((n1 * n1 + 4 * n1) * 12);
        let push = |verts: &mut Vec<f32>, pos: [f64; 3], nrm: [f64; 3], c: [f64; 3]| {
            let sh = shade_rs(s_, amb, nrm, c);
            verts.extend_from_slice(&[pos[0] as f32, pos[1] as f32, pos[2] as f32, nrm[0] as f32, nrm[1] as f32, nrm[2] as f32,
                                      sh[0] as f32, sh[1] as f32, sh[2] as f32, 1.0, 0.0, 0.0]);
        };
        let mut nrms = vec![[0.0; 3]; n1 * n1];
        for i in 0..n1 {
            for j in 0..n1 {
                let (gi, gj) = (i + 1, j + 1);
                let du = sub(p(gi + 1, gj), p(gi - 1, gj));
                let dv = sub(p(gi, gj + 1), p(gi, gj - 1));
                let mut nv = cross(du, dv);
                let q = gi * m + gj;
                let d = [dirs[q * 3], dirs[q * 3 + 1], dirs[q * 3 + 2]];
                let l = (nv[0] * nv[0] + nv[1] * nv[1] + nv[2] * nv[2]).sqrt().max(1e-12);
                nv = [nv[0] / l, nv[1] / l, nv[2] / l];
                if dot(nv, d) < 0.0 {
                    nv = [-nv[0], -nv[1], -nv[2]];
                }
                if dot(nv, d) < 0.2 {
                    nv = d;
                }
                nrms[i * n1 + j] = nv;
                push(&mut verts, p(gi, gj), nv, col[q]);
            }
        }
        let mut tris: Vec<u32> = Vec::with_capacity(G * G * 6 + 4 * G * 6);
        let id = |i: usize, j: usize| (i * n1 + j) as u32;
        for i in 0..G {
            for j in 0..G {
                tris.extend_from_slice(&[id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
        // スカート：縁の頂点を下へ垂らした頂点と、縁との間の帯
        let edges: [Vec<(usize, usize)>; 4] = [
            (0..n1).map(|j| (0, j)).collect(),
            (0..n1).map(|j| (G, j)).collect(),
            (0..n1).map(|i| (i, 0)).collect(),
            (0..n1).map(|i| (i, G)).collect(),
        ];
        for e in edges.iter() {
            let base = (verts.len() / 12) as u32;
            for &(i, j) in e {
                let q = (i + 1) * m + (j + 1);
                let d = [dirs[q * 3], dirs[q * 3 + 1], dirs[q * 3 + 2]];
                let pp = p(i + 1, j + 1);
                push(&mut verts, [pp[0] - d[0] * skirt, pp[1] - d[1] * skirt, pp[2] - d[2] * skirt], nrms[i * n1 + j], col[q]);
            }
            for t in 0..e.len() - 1 {
                let (a, b) = (id(e[t].0, e[t].1), id(e[t + 1].0, e[t + 1].1));
                let (sa, sb) = (base + t as u32, base + t as u32 + 1);
                // 両面から見えるように 2 向き
                tris.extend_from_slice(&[a, sa, sb, a, sb, b, a, sb, sa, a, b, sb]);
            }
        }
        Ok(Chunk { verts, tris })
    }

    /// 今のカメラ（惑星ローカルの位置 cam）で描く区画を選ぶ。足りない区画は集めて、まとめて作る（次のフレームから使う）。
    /// 戻り値：(描く区画, まだ足りない区画があるか)
    fn select(&mut self, planet: &B<'_>, cam: [f64; 3]) -> R<(Vec<Key>, bool)> {
        let mut out = vec![];
        let mut stack: Vec<Key> = (0..6u8).map(|f| (f, 0u8, 0u32, 0u32)).collect();
        let roots: Vec<Key> = stack.iter().copied().filter(|k| !self.chunks.contains_key(k)).collect();
        self.build_many(planet, &roots)?;
        let mut missing: Vec<(f64, Key)> = vec![];
        while let Some(k) = stack.pop() {
            let (a0, b0, s) = node_range(k);
            let cd = face_dir(k.0, a0 + s * 0.5, b0 + s * 0.5);
            let cp = [cd[0] * self.radius, cd[1] * self.radius, cd[2] * self.radius];
            let size = s * std::f64::consts::FRAC_PI_4 * self.radius * 1.2;
            let d = norm(sub(cam, cp));
            if k.1 < self.max_level && d < size * SPLIT_K {
                let kids: Vec<Key> = (0..4u32).map(|c| (k.0, k.1 + 1, k.2 * 2 + c / 2, k.3 * 2 + c % 2)).collect();
                let lack: Vec<Key> = kids.iter().copied().filter(|c| !self.chunks.contains_key(c)).collect();
                if lack.is_empty() {
                    stack.extend(kids);
                    continue;
                }
                for c in lack {
                    missing.push((d, c));
                }
            }
            out.push(k);
        }
        // 近い区画から、1 フレームに BATCH 個まで
        missing.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let pending = !missing.is_empty();
        let want: Vec<Key> = missing.iter().take(BATCH).map(|x| x.1).collect();
        self.build_many(planet, &want)?;
        out.sort_unstable();
        Ok((out, pending))
    }

    /// 遠くなった区画を忘れる（作り直せるので、数が多いときだけ）
    fn trim(&mut self, keep: &[Key]) {
        if self.chunks.len() < 1500 {
            return;
        }
        let ks: HashSet<Key> = keep.iter().copied().collect();
        self.chunks.retain(|k, _| k.1 <= 2 || ks.contains(k));
    }

    /// 選んだ区画を 1 つのメッシュにまとめる。frame（上・接線1・接線2・半径・半幅）の中の三角形は省く
    fn assemble(&self, frame: Option<([f64; 3], [f64; 3], [f64; 3], f64, f64)>) -> (Vec<f32>, Vec<i64>) {
        let mut v = vec![];
        let mut t = vec![];
        for k in &self.shown {
            let c = match self.chunks.get(k) { Some(c) => c, None => continue };
            let base = (v.len() / 12) as i64;
            v.extend_from_slice(&c.verts);
            let inside = |i: u32| -> bool {
                match frame {
                    None => false,
                    Some((up, t1, t2, r0, half)) => {
                        let q = &c.verts[i as usize * 12..i as usize * 12 + 3];
                        let q = [q[0] as f64, q[1] as f64, q[2] as f64];
                        let cc = dot(q, up);
                        cc > 0.0 && (dot(q, t1) * r0 / cc).abs() < half - 2.0 && (dot(q, t2) * r0 / cc).abs() < half - 2.0
                    }
                }
            };
            for tr in c.tris.chunks_exact(3) {
                if frame.is_some() && inside(tr[0]) && inside(tr[1]) && inside(tr[2]) {
                    continue;
                }
                t.extend_from_slice(&[base + tr[0] as i64, base + tr[1] as i64, base + tr[2] as i64]);
            }
        }
        (v, t)
    }

    /// 毎フレーム：区画を選び直し、組か詳細な地表の範囲が変わったらメッシュを作り直す。作り直したら Some
    pub fn update(&mut self, planet: &B<'_>, cam: [f64; 3], frame: Option<([f64; 3], [f64; 3], [f64; 3], f64, f64)>)
        -> R<Option<(Vec<f32>, Vec<i64>)>> {
        let (sel, pending) = self.select(planet, cam)?;
        self.since += 1;
        let fk: Vec<i64> = match frame {
            None => vec![],
            Some((u, a, _, r0, h)) => [u[0], u[1], u[2], a[0], a[1], a[2], r0, h].iter().map(|x| (x * 1e4).round() as i64).collect(),
        };
        if sel == self.shown && fk == self.frame_key {
            return Ok(None);
        }
        // まだ区画を作っている途中は、まとめ直し（メッシュを送り直す）を間引く
        if pending && fk == self.frame_key && self.since < REASSEMBLE_EVERY && !self.shown.is_empty() {
            return Ok(None);
        }
        self.since = 0;
        self.trim(&sel);
        self.shown = sel;
        self.frame_key = fk;
        Ok(Some(self.assemble(frame)))
    }
}
