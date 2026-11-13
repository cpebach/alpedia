//! 形づくりの道具（前は scipy・scikit-image を使っていた物）：等値面（Marching Cubes）・空洞の塗り分け・
//! いちばん近い点・小さな点の集まりの凸包。

use std::collections::HashMap;

use crate::mc_tables::{EDGE_TABLE, TRI_TABLE};

/// 角の番号 → 格子の (dx, dy, dz)
const CORNER: [[usize; 3]; 8] = [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0], [0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]];
/// 辺の番号 → 両端の角
const EDGE: [[usize; 2]; 12] = [[0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4], [0, 4], [1, 5], [2, 6], [3, 7]];

/// 3次元の格子 vol（dims = (nx, ny, nz)、C の並び）の level の等値面。頂点は格子の番号 × spacing（軸の順は配列と同じ）。
/// 値が level 以下の所を中とする。同じ辺の頂点は1つにまとめる。戻り値 (頂点, 三角形)。
pub fn marching_cubes(vol: &[f32], dims: [usize; 3], level: f32, spacing: [f64; 3]) -> (Vec<[f64; 3]>, Vec<[i64; 3]>) {
    let [nx, ny, nz] = dims;
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut faces: Vec<[i64; 3]> = Vec::new();
    if nx < 2 || ny < 2 || nz < 2 {
        return (verts, faces);
    }
    let idx = |x: usize, y: usize, z: usize| (x * ny + y) * nz + z;
    let mut seen: HashMap<(usize, usize), i64> = HashMap::new();
    for x in 0..nx - 1 {
        for y in 0..ny - 1 {
            for z in 0..nz - 1 {
                let mut v = [0f32; 8];
                let mut g = [0usize; 8];
                let mut cube = 0usize;
                for c in 0..8 {
                    let p = idx(x + CORNER[c][0], y + CORNER[c][1], z + CORNER[c][2]);
                    g[c] = p;
                    v[c] = vol[p];
                    if v[c] <= level {
                        cube |= 1 << c;
                    }
                }
                let edges = EDGE_TABLE[cube];
                if edges == 0 {
                    continue;
                }
                let mut ev = [-1i64; 12];
                for e in 0..12 {
                    if edges & (1 << e) == 0 {
                        continue;
                    }
                    let (a, b) = (EDGE[e][0], EDGE[e][1]);
                    let key = if g[a] < g[b] { (g[a], g[b]) } else { (g[b], g[a]) };
                    let id = *seen.entry(key).or_insert_with(|| {
                        let (va, vb) = (v[a] as f64, v[b] as f64);
                        let d = vb - va;
                        let t = if d.abs() < 1e-12 { 0.5 } else { ((level as f64 - va) / d).clamp(0.0, 1.0) };
                        let mut p = [0f64; 3];
                        for k in 0..3 {
                            let ca = [x, y, z][k] + CORNER[a][k];
                            let cb = [x, y, z][k] + CORNER[b][k];
                            p[k] = (ca as f64 + (cb as f64 - ca as f64) * t) * spacing[k];
                        }
                        verts.push(p);
                        verts.len() as i64 - 1
                    });
                    ev[e] = id;
                }
                let row = &TRI_TABLE[cube];
                let mut m = 0;
                while m + 2 < 16 && row[m] >= 0 {
                    let (a, b, c) = (ev[row[m] as usize], ev[row[m + 1] as usize], ev[row[m + 2] as usize]);
                    if a != b && b != c && a != c {
                        faces.push([a, c, b]);         // 外向き（中＝値が小さい側から見て反時計回りの逆。scikit-image と同じ向き）
                    }
                    m += 3;
                }
            }
        }
    }
    (verts, faces)
}

/// 空いている所（solid でない所）を6近傍でつながりごとに番号付け（1 から。見つけた順）。戻り値 (番号の格子, 個数)。
pub fn label_empty(solid: &[bool], dims: [usize; 3]) -> (Vec<i32>, i32) {
    let [nx, ny, nz] = dims;
    let n = nx * ny * nz;
    let mut lab = vec![0i32; n];
    let mut count = 0i32;
    let mut stack: Vec<usize> = Vec::new();
    for s in 0..n {
        if solid[s] || lab[s] != 0 {
            continue;
        }
        count += 1;
        lab[s] = count;
        stack.push(s);
        while let Some(p) = stack.pop() {
            let z = p % nz;
            let y = (p / nz) % ny;
            let x = p / (ny * nz);
            let mut push = |q: usize| {
                if !solid[q] && lab[q] == 0 {
                    lab[q] = count;
                    stack.push(q);
                }
            };
            if x > 0 { push(p - ny * nz); }
            if x + 1 < nx { push(p + ny * nz); }
            if y > 0 { push(p - nz); }
            if y + 1 < ny { push(p + nz); }
            if z > 0 { push(p - 1); }
            if z + 1 < nz { push(p + 1); }
        }
    }
    (lab, count)
}

/// 点ごとに、いちばん近い候補の番号（格子に分けて探す）。
pub fn nearest(cands: &[[f64; 3]], pts: &[[f64; 3]]) -> Vec<i64> {
    if cands.is_empty() {
        return vec![-1; pts.len()];
    }
    let d2 = |a: &[f64; 3], b: &[f64; 3]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
    let brute = |p: &[f64; 3]| {
        let mut best = (f64::INFINITY, -1i64);
        for (i, c) in cands.iter().enumerate() {
            let d = d2(p, c);
            if d < best.0 || best.1 < 0 {
                best = (d, i as i64);
            }
        }
        best.1
    };
    if cands.len() <= 64 {
        // 候補が少ないときは全部調べる（候補が1点・同じ位置のときに格子が細かくなりすぎて止まらなくなるのを避ける）
        return pts.iter().map(brute).collect();
    }
    let mut lo = cands[0];
    let mut hi = cands[0];
    for c in cands {
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    let ext = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f64, f64::max);
    let cell = if ext < 1e-6 { 1.0 } else { (ext / (cands.len() as f64).cbrt().max(1.0)).max(1e-6) };
    let key = |p: &[f64; 3]| {
        [((p[0] - lo[0]) / cell).floor() as i64, ((p[1] - lo[1]) / cell).floor() as i64, ((p[2] - lo[2]) / cell).floor() as i64]
    };
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (i, c) in cands.iter().enumerate() {
        grid.entry(key(c)).or_default().push(i);
    }
    pts.iter()
        .map(|p| {
            let k0 = key(p);
            let mut best = (f64::INFINITY, -1i64);
            let mut r = 0i64;
            loop {
                // 半径 r の殻の格子を調べる。見つかった最短より遠い殻だけになったら終わり
                for dx in -r..=r {
                    for dy in -r..=r {
                        for dz in -r..=r {
                            if dx.abs().max(dy.abs()).max(dz.abs()) != r {
                                continue;
                            }
                            if let Some(list) = grid.get(&[k0[0] + dx, k0[1] + dy, k0[2] + dz]) {
                                for &i in list {
                                    let d = d2(p, &cands[i]);
                                    if d < best.0 || (d == best.0 && (i as i64) < best.1) {
                                        best = (d, i as i64);
                                    }
                                }
                            }
                        }
                    }
                }
                if best.1 >= 0 && ((r as f64) * cell).powi(2) >= best.0 {
                    break;
                }
                r += 1;
                if r > 256 {
                    return brute(p);         // 遠すぎる点は全部調べる
                }
            }
            best.1
        })
        .collect()
}

/// 小さな点の集まり（数十個まで）の凸包の三角形（面の上にいくつも点があるときは、その多角形を扇形に分ける）。
pub fn convex_hull(p: &[[f64; 3]]) -> Vec<[i64; 3]> {
    let n = p.len();
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mut ext = 0f64;
    for a in p {
        for b in p {
            ext = ext.max(dot(sub(*a, *b), sub(*a, *b)).sqrt());
        }
    }
    let eps = 1e-9 * ext.max(1.0);
    let mut planes: Vec<([f64; 3], f64)> = Vec::new();
    let mut out = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let nrm = cross(sub(p[j], p[i]), sub(p[k], p[i]));
                let l = dot(nrm, nrm).sqrt();
                if l < eps * eps.max(1e-12).sqrt() || l < 1e-12 {
                    continue;
                }
                let nrm = [nrm[0] / l, nrm[1] / l, nrm[2] / l];
                let d0 = dot(nrm, p[i]);
                let (mut pos, mut neg) = (false, false);
                for q in p {
                    let s = dot(nrm, *q) - d0;
                    if s > eps { pos = true; }
                    if s < -eps { neg = true; }
                }
                if pos && neg {
                    continue;
                }
                // 外向きにそろえる
                let (nrm, d0) = if pos { ([-nrm[0], -nrm[1], -nrm[2]], -d0) } else { (nrm, d0) };
                if planes.iter().any(|(m, e)| dot(*m, nrm) > 1.0 - 1e-9 && (e - d0).abs() < eps) {
                    continue;                          // この面はもう作った
                }
                planes.push((nrm, d0));
                // 面の上の点の、面の中での凸包（2次元・一直線上の点は外す）を扇形に
                let on: Vec<usize> = (0..n).filter(|&q| (dot(nrm, p[q]) - d0).abs() <= eps).collect();
                let u = { let a = sub(p[on[1]], p[on[0]]); let l = dot(a, a).sqrt().max(1e-12); [a[0] / l, a[1] / l, a[2] / l] };
                let v = cross(nrm, u);
                let mut q2: Vec<(f64, f64, usize)> = on.iter().map(|&q| (dot(p[q], u), dot(p[q], v), q)).collect();
                q2.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()));
                let turn = |o: &(f64, f64, usize), a: &(f64, f64, usize), b: &(f64, f64, usize)| {
                    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
                };
                let mut hull: Vec<(f64, f64, usize)> = Vec::new();
                for pass in 0..2 {
                    let start = hull.len();
                    let it: Box<dyn Iterator<Item = &(f64, f64, usize)>> = if pass == 0 { Box::new(q2.iter()) } else { Box::new(q2.iter().rev()) };
                    for pt in it {
                        while hull.len() >= start + 2 && turn(&hull[hull.len() - 2], &hull[hull.len() - 1], pt) <= eps * ext.max(1.0) {
                            hull.pop();
                        }
                        hull.push(*pt);
                    }
                    hull.pop();
                }
                let keep: Vec<usize> = hull.iter().map(|x| x.2).collect();
                for t in 1..keep.len().saturating_sub(1) {
                    out.push([keep[0] as i64, keep[t] as i64, keep[t + 1] as i64]);
                }
            }
        }
    }
    out
}
