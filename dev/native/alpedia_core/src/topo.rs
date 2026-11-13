//! 構築物の形の材料（前は Python の main/shape.py の topology と construct.py の _build_contacts）。
//! - topology：形をなぞる芯（点・線・三角形）。2×2×2 の窓ごとに、結合のつながりから斜面の三角形を作る。
//! - contacts：頂点ごとに、当たり判定に使う芯（いちばん近い・外向きの物）を選ぶ。

use std::collections::{HashMap, HashSet};

const DIRS: [[i64; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

/// itertools.product((0,1), repeat=3) の順（OFFS8）
fn offs8(k: usize) -> [i64; 3] {
    [((k >> 2) & 1) as i64, ((k >> 1) & 1) as i64, (k & 1) as i64]
}

fn off_index(o: [i64; 3]) -> Option<usize> {
    if o.iter().all(|&x| x == 0 || x == 1) {
        Some(((o[0] << 2) | (o[1] << 1) | o[2]) as usize)
    } else {
        None
    }
}

/// cells：ボクセルのセル、links：結合（番号の組）、extras：再結合の組、kind：種類（同じ種類どうしだけ斜面）、
/// hull：窓の中の結合成分（OFFS8 の番号のビット）→ 三角形（OFFS8 の番号）の表。戻り値 (points, segs, tris)
pub fn topology(cells: &[[i64; 3]], links: &[[usize; 2]], extras: &[[usize; 2]], kind: &[i64],
                hull: &[Vec<[usize; 3]>]) -> (Vec<i64>, Vec<[i64; 2]>, Vec<[i64; 3]>) {
    let n = cells.len();
    let index: HashMap<[i64; 3], usize> = cells.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    let mut linkset: HashSet<(usize, usize)> = HashSet::new();
    let mut count = vec![0usize; n];
    let mut seg: HashSet<(usize, usize)> = HashSet::new();
    for l in links {
        let (a, b) = (l[0].min(l[1]), l[0].max(l[1]));
        linkset.insert((a, b));
        seg.insert((a, b));
        count[l[0]] += 1;
        count[l[1]] += 1;
    }
    for l in extras {
        seg.insert((l[0].min(l[1]), l[0].max(l[1])));
    }
    let interior: Vec<bool> = count.iter().map(|&c| c >= 6).collect();
    let linked = |a: usize, b: usize| linkset.contains(&(a.min(b), a.max(b)));
    // 2×2×2 の窓
    let mut anchors: Vec<[i64; 3]> = Vec::with_capacity(n * 8);
    {
        let mut seen: HashSet<[i64; 3]> = HashSet::new();
        for c in cells {
            for k in 0..8 {
                let o = offs8(k);
                let a = [c[0] - o[0], c[1] - o[1], c[2] - o[2]];
                if seen.insert(a) {
                    anchors.push(a);
                }
            }
        }
    }
    anchors.sort();
    let mut members: HashMap<[i64; 3], [Option<usize>; 8]> = HashMap::new();
    let mut full: HashSet<[i64; 3]> = HashSet::new();
    for a in &anchors {
        let mut m = [None; 8];
        let mut cnt = 0;
        for k in 0..8 {
            let o = offs8(k);
            if let Some(&j) = index.get(&[a[0] + o[0], a[1] + o[1], a[2] + o[2]]) {
                m[k] = Some(j);
                cnt += 1;
            }
        }
        if cnt < 2 {
            continue;
        }
        if cnt == 8 {
            let mut ok = true;
            'outer: for k in 0..8 {
                let o = offs8(k);
                for d in DIRS {
                    if let Some(k2) = off_index([o[0] + d[0], o[1] + d[1], o[2] + d[2]]) {
                        if !linked(m[k].unwrap(), m[k2].unwrap()) {
                            ok = false;
                            break 'outer;
                        }
                    }
                }
            }
            if ok {
                full.insert(*a);
            }
        }
        members.insert(*a, m);
    }
    let mut tris: HashSet<[i64; 3]> = HashSet::new();
    let sorted3 = |a: usize, b: usize, c: usize| {
        let mut t = [a as i64, b as i64, c as i64];
        t.sort();
        t
    };
    for a in &anchors {
        let Some(m) = members.get(a) else { continue };
        if full.contains(a) {
            // 内部の面は省略し、隣の窓が埋まっていない面だけ作る
            for f in 0..6 {
                let (ax, v) = (f / 2, (f % 2) as i64);
                let mut nb = *a;
                nb[ax] += if v == 1 { 1 } else { -1 };
                if full.contains(&nb) {
                    continue;
                }
                let q: Vec<usize> = (0..8).filter(|&k| offs8(k)[ax] == v).map(|k| m[k].unwrap()).collect();
                tris.insert(sorted3(q[0], q[1], q[3]));
                tris.insert(sorted3(q[0], q[3], q[2]));
            }
            continue;
        }
        // 窓の中の結合成分（同じ種類・結合している物どうし）
        let mut parent: [usize; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
        fn find(p: &mut [usize; 8], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        for k in 0..8 {
            let Some(jk) = m[k] else { continue };
            let o = offs8(k);
            for d in DIRS {
                let Some(k2) = off_index([o[0] + d[0], o[1] + d[1], o[2] + d[2]]) else { continue };
                let Some(j2) = m[k2] else { continue };
                if kind[jk] == kind[j2] && linked(jk, j2) {
                    let (r1, r2) = (find(&mut parent, k), find(&mut parent, k2));
                    parent[r1] = r2;
                }
            }
        }
        let mut comps: HashMap<usize, usize> = HashMap::new();     // 根 → ビット
        for k in 0..8 {
            if m[k].is_some() {
                let r = find(&mut parent, k);
                *comps.entry(r).or_insert(0) |= 1 << k;
            }
        }
        for (_, bits) in comps {
            if bits.count_ones() < 3 {
                continue;
            }
            for t in &hull[bits] {
                let tri = sorted3(m[t[0]].unwrap(), m[t[1]].unwrap(), m[t[2]].unwrap());
                if !(interior[tri[0] as usize] && interior[tri[1] as usize] && interior[tri[2] as usize]) {
                    tris.insert(tri);
                }
            }
        }
    }
    let points: Vec<i64> = (0..n).filter(|&i| !interior[i]).map(|i| i as i64).collect();
    let mut segs: Vec<[i64; 2]> = seg.into_iter().filter(|&(a, b)| !(interior[a] && interior[b])).map(|(a, b)| [a as i64, b as i64]).collect();
    segs.sort();
    let mut tris: Vec<[i64; 3]> = tris.into_iter().collect();
    tris.sort();
    (points, segs, tris)
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// 点と線分（numpy 版 dist_segments と同じ式）
pub fn dist_seg(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> (f64, [f64; 3]) {
    let ab = sub(b, a);
    let t = (dot(sub(p, a), ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
    let q = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
    (norm(sub(p, q)), q)
}

/// 点と三角形（numpy 版 dist_triangles と同じ式）
pub fn dist_tri(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> (f64, [f64; 3]) {
    let (ab, ac) = (sub(b, a), sub(c, a));
    let mut nrm = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
    let nn = norm(nrm).max(1e-12);
    for x in nrm.iter_mut() {
        *x /= nn;
    }
    let dn = dot(sub(p, a), nrm);
    let proj = [p[0] - nrm[0] * dn, p[1] - nrm[1] * dn, p[2] - nrm[2] * dn];
    let (v0, v1, v2) = (ab, ac, sub(proj, a));
    let (d00, d01, d11, d20, d21) = (dot(v0, v0), dot(v0, v1), dot(v1, v1), dot(v2, v0), dot(v2, v1));
    let mut den = d00 * d11 - d01 * d01;
    if den.abs() < 1e-12 {
        den = 1e-12;
    }
    let v = (d11 * d20 - d01 * d21) / den;
    let w = (d00 * d21 - d01 * d20) / den;
    let u = 1.0 - v - w;
    if u >= 0.0 && v >= 0.0 && w >= 0.0 {
        return (dn.abs(), proj);
    }
    let (d1, q1) = dist_seg(p, a, b);
    let (d2, q2) = dist_seg(p, b, c);
    let (d3, q3) = dist_seg(p, c, a);
    let de = d1.min(d2).min(d3);
    let qe = if d1 <= d2 && d1 <= d3 { q1 } else if d2 <= d3 { q2 } else { q3 };
    (de, qe)
}

/// 頂点ごとの当たり判定の芯：(種類 (8n)：-1 内部・0 点・1 線・2 三角形、芯のボクセル3つ (8n,3))。
/// pos (8n,3)、frames (n,3,3) は各ボクセルの軸（行）、cen (n,3) はボクセルの中心。
pub fn contacts(cells: &[[i64; 3]], pos: &[f64], frames: &[f64], cen: &[[f64; 3]], segs: &[[i64; 2]], tris: &[[i64; 3]])
    -> (Vec<i64>, Vec<[i64; 3]>) {
    let n = cells.len();
    let occ: HashSet<[i64; 3]> = cells.iter().copied().collect();
    let bits = |k: usize| [(k & 1) as i64, ((k >> 1) & 1) as i64, ((k >> 2) & 1) as i64];
    // 頂点のまわり8セルの空き具合 → 外向き
    let mut exposed = vec![[false; 8]; n];
    let mut outward = vec![[[0f64; 3]; 8]; n];
    for i in 0..n {
        let c = cells[i];
        for k in 0..8 {
            let b = bits(k);
            let corner = [c[0] + b[0], c[1] + b[1], c[2] + b[2]];
            let mut ow = [0f64; 3];
            for oi in 0..8 {
                let o = bits(oi);
                let cc = [corner[0] - 1 + o[0], corner[1] - 1 + o[1], corner[2] - 1 + o[2]];
                if !occ.contains(&cc) {
                    exposed[i][k] = true;
                    for t in 0..3 {
                        ow[t] += o[t] as f64 - 0.5;
                    }
                }
            }
            // ボクセルの向きへ：out_i = Σ_j ow_j F[j][i]
            let f = &frames[i * 9..i * 9 + 9];
            for t in 0..3 {
                outward[i][k][t] = ow[0] * f[t] + ow[1] * f[3 + t] + ow[2] * f[6 + t];
            }
        }
    }
    // 候補：ボクセルごとに（点 → 線の1つ目・2つ目 → 三角形の1・2・3つ目）の順
    let mut cand: Vec<Vec<(i64, [i64; 3])>> = (0..n).map(|i| vec![(0, [i as i64; 3])]).collect();
    for col in 0..2 {
        for s in segs {
            cand[s[col] as usize].push((1, [s[0], s[1], s[1]]));
        }
    }
    for col in 0..3 {
        for t in tris {
            cand[t[col] as usize].push((2, *t));
        }
    }
    let mut ctype = vec![-1i64; 8 * n];
    let mut cidx = vec![[0i64; 3]; 8 * n];
    for i in 0..n {
        for k in 0..8 {
            if !exposed[i][k] {
                continue;
            }
            let v = [pos[(8 * i + k) * 3], pos[(8 * i + k) * 3 + 1], pos[(8 * i + k) * 3 + 2]];
            let mut best = (f64::INFINITY, 0usize);
            for (ci, (ty, idx)) in cand[i].iter().enumerate() {
                let a = cen[idx[0] as usize];
                let (d, q) = match ty {
                    0 => (norm(sub(v, a)), a),
                    1 => dist_seg(v, a, cen[idx[1] as usize]),
                    _ => dist_tri(v, a, cen[idx[1] as usize], cen[idx[2] as usize]),
                };
                let dv = sub(v, q);
                let score = dot(dv, outward[i][k]) / norm(dv).max(1e-9);
                let key = (d * 1e4).round_ties_even() / 1e4 - 1e-6 * score;
                if key < best.0 {
                    best = (key, ci);
                }
            }
            let (ty, idx) = cand[i][best.1];
            ctype[8 * i + k] = ty;
            cidx[8 * i + k] = idx;
        }
    }
    (ctype, cidx)
}

/// 角ばった見た目の「長さ」：面の向き（軸・辺・角の 7 種）への射影の最大 ÷ 0.97（shape.facet_norm と同じ）
fn facet_norm(v: [f64; 3]) -> f64 {
    let a = [v[0].abs(), v[1].abs(), v[2].abs()];
    let s2 = 1.0 / 2f64.sqrt();
    let s3 = 1.0 / 3f64.sqrt();
    let c = [a[0], a[1], a[2], (a[0] + a[1]) * s2, (a[0] + a[2]) * s2, (a[1] + a[2]) * s2, (a[0] + a[1] + a[2]) * s3];
    c.iter().cloned().fold(f64::NEG_INFINITY, f64::max) / 0.97
}

/// 形の距離場（芯からの距離 − r、格子は lo + 番号 / k）。prims：(種類 0 点・1 線・2 三角形, 頂点3つ, 角ばらせるか)。
/// 芯ごとに、まわりの m×m×m の格子だけを調べる（shape.build_mesh の apply と同じ）。
pub fn shape_sdf(dims: [usize; 3], lo: [f64; 3], k: f64, r: f64, m: usize, prims: &[(u8, [[f64; 3]; 3], bool)]) -> Vec<f32> {
    use rayon::prelude::*;
    let n = dims[0] * dims[1] * dims[2];
    let mut sdf = vec![1.0f32; n];
    // 芯ごとの調べ始めの格子（x で並べて、x の帯ごとに別のスレッドで）
    let mut start: Vec<([i64; 3], usize)> = prims.iter().enumerate().map(|(pi, (kind, v, _))| {
        let nv = match kind { 0 => 1, 1 => 2, _ => 3 };
        let mut pmin = [f64::INFINITY; 3];
        for q in 0..nv {
            for t in 0..3 {
                pmin[t] = pmin[t].min(v[q][t]);
            }
        }
        ([((pmin[0] - r - 0.1 - lo[0]) * k).floor() as i64, ((pmin[1] - r - 0.1 - lo[1]) * k).floor() as i64,
          ((pmin[2] - r - 0.1 - lo[2]) * k).floor() as i64], pi)
    }).collect();
    start.sort_by_key(|x| x.0[0]);
    let slab = 4usize;
    let plane = dims[1] * dims[2];
    sdf.par_chunks_mut(slab * plane).enumerate().for_each(|(si, chunk)| {
        let x0 = (si * slab) as i64;
        let x1 = x0 + (chunk.len() / plane) as i64;           // [x0, x1)
        let mx = dims[0] as i64 - 1;
        // この帯に届く芯：調べる x の範囲 [i0, i0+m) を端で丸めたものが帯と重なる物
        let first = start.partition_point(|x| x.0[0] + (m as i64) <= x0);   // 帯より手前で終わる芯は飛ばす
        for &(i0, pi) in &start[first..] {
            if i0[0].clamp(0, mx) >= x1 {
                break;                                             // ここから先の芯は帯より後
            }
            let (kind, v, facet) = &prims[pi];
            for ox in 0..m as i64 {
                let ix = (i0[0] + ox).clamp(0, mx);
                if ix < x0 || ix >= x1 {
                    continue;
                }
                for oy in 0..m as i64 {
                    let iy = (i0[1] + oy).clamp(0, dims[1] as i64 - 1) as usize;
                    for oz in 0..m as i64 {
                        let iz = (i0[2] + oz).clamp(0, dims[2] as i64 - 1) as usize;
                        let g = [lo[0] + ix as f64 / k, lo[1] + iy as f64 / k, lo[2] + iz as f64 / k];
                        let (d, q) = match kind {
                            0 => (norm(sub(g, v[0])), v[0]),
                            1 => dist_seg(g, v[0], v[1]),
                            _ => dist_tri(g, v[0], v[1], v[2]),
                        };
                        let d = if *facet { facet_norm(sub(g, q)) } else { d };
                        let lin = ((ix - x0) as usize * dims[1] + iy) * dims[2] + iz;
                        let val = (d - r) as f32;
                        if val < chunk[lin] {
                            chunk[lin] = val;
                        }
                    }
                }
            }
        }
    });
    sdf
}
