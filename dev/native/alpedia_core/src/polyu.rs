//! 角を落とした多面体の和（ローポリのなめらかなメッシュ。A-21）。
//! 芯（点・線・三角形）を、面・辺・角を落とした多面体 B で太らせた凸多面体（芯 ⊕ B）を作り、
//! それらの和の表面を、平らな多角形のまま直接求める（格子で測って面を取り出す方法と違い、平らな面は大きな多角形 1 つ、
//! 辺と角はくっきり）。多角形はブロックの境（整数の平面）で切り、ブロックごとに 1 つの持ち主にする。

use rayon::prelude::*;
use std::collections::HashMap;

pub type V3 = [f64; 3];

const EPS: f64 = 1e-7;

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

/// 太らせる多面体 B の頂点（軸の面の距離 a。軸の面は辺の長さ a の正方形・辺は長方形・角は三角形：
/// 頂点は (±a, ±a/2, ±a/2) の並べ替え 24 個）
fn b_verts(a: f64) -> Vec<V3> {
    let mut v = vec![];
    for ax in 0..3 {
        for s0 in [-1.0, 1.0] {
            for s1 in [-1.0, 1.0] {
                for s2 in [-1.0, 1.0] {
                    let mut p = [0.0; 3];
                    p[ax] = s0 * a;
                    p[(ax + 1) % 3] = s1 * a * 0.5;
                    p[(ax + 2) % 3] = s2 * a * 0.5;
                    v.push(p);
                }
            }
        }
    }
    v
}

/// 面の向きの候補：B の面（軸・辺・角の 26 方向）と、格子の向き（軸・面の対角・立体の対角の 13 方向）どうしの外積。
/// 芯 ⊕ B の面の向きは、この中に必ずある（芯の辺と B の辺は格子の向き、芯の面は格子の向きの外積）
fn normals() -> Vec<V3> {
    let mut lat: Vec<V3> = vec![];
    for x in -1i32..=1 {
        for y in -1i32..=1 {
            for z in -1i32..=1 {
                if (x, y, z) == (0, 0, 0) {
                    continue;
                }
                lat.push([x as f64, y as f64, z as f64]);
            }
        }
    }
    let mut out: Vec<V3> = vec![];
    let mut seen: HashMap<[i64; 3], ()> = HashMap::new();
    let mut push = |v: V3, out: &mut Vec<V3>| {
        let l = norm(v);
        if l < 1e-9 {
            return;
        }
        let n = mul(v, 1.0 / l);
        let key = [(n[0] * 1e6).round() as i64, (n[1] * 1e6).round() as i64, (n[2] * 1e6).round() as i64];
        if seen.insert(key, ()).is_none() {
            out.push(n);
        }
    };
    for d in &lat {
        push(*d, &mut out);
    }
    for a in &lat {
        for b in &lat {
            push(cross(*a, *b), &mut out);
        }
    }
    out
}

/// 凸多角形を平面 m・x = o で分ける：(m・x ≤ o の側, m・x ≥ o の側)
fn split(poly: &[V3], m: V3, o: f64) -> (Vec<V3>, Vec<V3>) {
    let d: Vec<f64> = poly.iter().map(|p| dot(m, *p) - o).collect();
    if d.iter().all(|x| *x <= EPS) {
        return (poly.to_vec(), vec![]);
    }
    if d.iter().all(|x| *x >= -EPS) {
        return (vec![], poly.to_vec());
    }
    let n = poly.len();
    let (mut a, mut b) = (Vec::with_capacity(n + 2), Vec::with_capacity(n + 2));
    for i in 0..n {
        let j = (i + 1) % n;
        let (p, q, dp, dq) = (poly[i], poly[j], d[i], d[j]);
        if dp <= EPS {
            a.push(p);
        }
        if dp >= -EPS {
            b.push(p);
        }
        if (dp > EPS && dq < -EPS) || (dp < -EPS && dq > EPS) {
            let t = dp / (dp - dq);
            let x = add(p, mul(sub(q, p), t));
            a.push(x);
            b.push(x);
        }
    }
    (a, b)
}

fn area2(poly: &[V3]) -> f64 {
    let mut s = [0.0; 3];
    for i in 1..poly.len().saturating_sub(1) {
        let c = cross(sub(poly[i], poly[0]), sub(poly[i + 1], poly[0]));
        s = add(s, c);
    }
    norm(s)
}

/// 凸多面体（平面の集まり）と、その面の多角形（芯の 1 つ目の頂点からの位置）
#[derive(Clone)]
struct Shape {
    planes: Vec<(usize, f64)>,          // (向きの番号, 芯の 1 つ目の頂点を原点にした距離)
    faces: Vec<(usize, Vec<V3>)>,       // (向きの番号, 多角形。外から見て左回り)
}

fn make_shape(rel: &[V3], ns: &[V3], bv: &[V3]) -> Shape {
    let sup = |m: V3, pts: &[V3]| pts.iter().map(|p| dot(m, *p)).fold(f64::NEG_INFINITY, f64::max);
    let planes: Vec<(usize, f64)> = ns.iter().enumerate().map(|(i, m)| (i, sup(*m, rel) + sup(*m, bv))).collect();
    let mut faces = vec![];
    let mut used = vec![];
    for &(i, o) in &planes {
        let n = ns[i];
        let t = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let u = mul(cross(n, t), 1.0 / norm(cross(n, t)));
        let v = cross(n, u);
        let c = mul(n, o);
        let l = 1.0e5;
        let mut poly: Vec<V3> = [(-l, -l), (l, -l), (l, l), (-l, l)].iter().map(|(a, b)| add(c, add(mul(u, *a), mul(v, *b)))).collect();
        for &(j, oj) in &planes {
            if j == i {
                continue;
            }
            poly = split(&poly, ns[j], oj).0;
            if poly.len() < 3 {
                break;
            }
        }
        if poly.len() >= 3 && area2(&poly) > 1e-9 {
            faces.push((i, poly));
            used.push((i, o));
        }
    }
    Shape { planes: used, faces }
}

/// 凸多角形から凸多面体 q を引いた残り（凸多角形の集まり）。
/// 同じ平面の上で重なる面（となりの多面体と同じ面）は、番号の小さい方だけ残す（same：q の方が後ろの番号なら残す）。
/// 向かい合って重なる面（中で接する所）はどちらも消す
fn subtract(piece: Vec<V3>, fn_: usize, fo: f64, q: &[(usize, V3, f64)], keep_same: bool, ns: &[V3], out: &mut Vec<Vec<V3>>) {
    // 全部が q のどれかの平面の外なら、そのまま
    for &(_, m, o) in q {
        if piece.iter().all(|p| dot(m, *p) - o > -EPS) && piece.iter().any(|p| dot(m, *p) - o > EPS) {
            out.push(piece);
            return;
        }
    }
    let fnv = ns[fn_];
    let mut rem = piece;
    for &(j, m, o) in q {
        if j == fn_ && (o - fo).abs() < 1e-6 {
            // 同じ平面・同じ向き：残すなら全部残す
            if keep_same {
                out.push(rem);
                return;
            }
            continue;
        }
        let _ = fnv;
        let (a, b) = split(&rem, m, o);
        if b.len() >= 3 && area2(&b) > 1e-12 {
            // 平面の上に乗っているだけ（全部が平面の上）は外に数えない
            if b.iter().any(|p| dot(m, *p) - o > EPS) {
                out.push(b);
            }
        }
        if a.len() < 3 || area2(&a) <= 1e-12 {
            return;
        }
        rem = a;
    }
    // 残りは q の中
}

pub struct Out {
    pub verts: Vec<V3>,
    pub faces: Vec<[i64; 3]>,
    pub face_prim: Vec<usize>,
    pub vert_prim: Vec<usize>,
}

/// prims：(芯の頂点 1〜3 個, 部分の番号)。同じ部分の多面体どうしだけ和にする（ローターで別に動く部分は別々の形）。
/// a：B の軸の面までの距離
pub fn union_mesh(prims: &[(Vec<V3>, usize)], a: f64) -> Out {
    let prof = std::env::var_os("ALPEDIA_POLY_PROF").is_some();
    let t0 = std::time::Instant::now();
    let ns = normals();
    let bv = b_verts(a);
    // 芯の形ごとに多面体を作って使い回す（格子の上の芯は形の種類が少ない）
    let key_of = |v: &[V3]| -> Vec<i64> {
        v.iter().flat_map(|p| (0..3).map(move |k| ((p[k] - v[0][k]) * 64.0).round() as i64)).collect()
    };
    let mut cache: HashMap<Vec<i64>, usize> = HashMap::new();
    let mut shapes: Vec<Shape> = vec![];
    let mut sidx = Vec::with_capacity(prims.len());
    let mut todo: Vec<(Vec<i64>, Vec<V3>)> = vec![];
    for (v, _) in prims {
        let k = key_of(v);
        let id = match cache.get(&k) {
            Some(&x) => x,
            None => {
                let x = todo.len();
                cache.insert(k.clone(), x);
                todo.push((k, v.iter().map(|p| sub(*p, v[0])).collect()));
                x
            }
        };
        sidx.push(id);
    }
    shapes.extend(todo.par_iter().map(|(_, rel)| make_shape(rel, &ns, &bv)).collect::<Vec<_>>());
    // 多面体ごとの平面（世界の位置）と箱
    let polys: Vec<(Vec<(usize, V3, f64)>, V3, V3)> = prims.iter().zip(&sidx).map(|((v, _), &s)| {
        let sh = &shapes[s];
        let pl: Vec<(usize, V3, f64)> = sh.planes.iter().map(|&(i, o)| (i, ns[i], o + dot(ns[i], v[0]))).collect();
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for (_, f) in &sh.faces {
            for p in f {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k] + v[0][k]);
                    hi[k] = hi[k].max(p[k] + v[0][k]);
                }
            }
        }
        (pl, lo, hi)
    }).collect();
    if prof { eprintln!("poly: prims {} shapes {} setup {:?}", prims.len(), shapes.len(), t0.elapsed()); }
    // 箱の格子（1 マス 2）で近くの多面体を探す
    let cell = |x: f64| (x / 2.0).floor() as i64;
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (i, (_, lo, hi)) in polys.iter().enumerate() {
        for x in cell(lo[0])..=cell(hi[0]) {
            for y in cell(lo[1])..=cell(hi[1]) {
                for z in cell(lo[2])..=cell(hi[2]) {
                    grid.entry([x, y, z]).or_default().push(i);
                }
            }
        }
    }
    // 多面体ごとに、面から近くの（同じ部分の）多面体を引く
    let pieces: Vec<Vec<(usize, Vec<V3>)>> = (0..prims.len()).into_par_iter().map(|i| {
        let (_, lo, hi) = &polys[i];
        let mut nb: Vec<usize> = vec![];
        for x in cell(lo[0])..=cell(hi[0]) {
            for y in cell(lo[1])..=cell(hi[1]) {
                for z in cell(lo[2])..=cell(hi[2]) {
                    if let Some(l) = grid.get(&[x, y, z]) {
                        nb.extend(l.iter().copied());
                    }
                }
            }
        }
        nb.sort_unstable();
        nb.dedup();
        nb.retain(|&j| {
            j != i && prims[j].1 == prims[i].1 && {
                let (_, l2, h2) = &polys[j];
                (0..3).all(|k| l2[k] <= hi[k] + EPS && h2[k] >= lo[k] - EPS)
            }
        });
        // 芯の近い順（中に入れてしまう多面体を先に）
        let c0 = prims[i].0[0];
        nb.sort_by(|&a, &b| {
            let da = dot(sub(prims[a].0[0], c0), sub(prims[a].0[0], c0));
            let db = dot(sub(prims[b].0[0], c0), sub(prims[b].0[0], c0));
            da.partial_cmp(&db).unwrap().then(a.cmp(&b))
        });
        let v0 = prims[i].0[0];
        let mut res = vec![];
        for (fi, f) in &shapes[sidx[i]].faces {
            let fo = polys[i].0.iter().find(|x| x.0 == *fi).map(|x| x.2).unwrap();
            let mut cur: Vec<Vec<V3>> = vec![f.iter().map(|p| add(*p, v0)).collect()];
            for &j in &nb {
                if cur.is_empty() {
                    break;
                }
                let (q, l2, h2) = &polys[j];
                let mut next = vec![];
                for pc in cur {
                    // 箱が重ならなければそのまま
                    let mut plo = [f64::INFINITY; 3];
                    let mut phi = [f64::NEG_INFINITY; 3];
                    for p in &pc {
                        for k in 0..3 {
                            plo[k] = plo[k].min(p[k]);
                            phi[k] = phi[k].max(p[k]);
                        }
                    }
                    if (0..3).any(|k| l2[k] > phi[k] + EPS || h2[k] < plo[k] - EPS) {
                        next.push(pc);
                        continue;
                    }
                    subtract(pc, *fi, fo, q, j > i, &ns, &mut next);
                }
                cur = next;
            }
            for pc in cur {
                res.push((*fi, pc));
            }
        }
        res
    }).collect();
    if prof { eprintln!("poly: subtract {:?} pieces {}", t0.elapsed(), pieces.iter().map(|x| x.len()).sum::<usize>()); }
    // ブロックの境（整数の平面）で切る
    let mut flat: Vec<(usize, usize, Vec<V3>)> = vec![];   // (多面体, 向き, 多角形)
    for (i, ps) in pieces.into_iter().enumerate() {
        for (fi, pc) in ps {
            let mut cur = vec![pc];
            for ax in 0..3 {
                let mut next = vec![];
                for p in cur {
                    let lo = p.iter().map(|q| q[ax]).fold(f64::INFINITY, f64::min);
                    let hi = p.iter().map(|q| q[ax]).fold(f64::NEG_INFINITY, f64::max);
                    let mut rest = p;
                    let mut m = [0.0; 3];
                    m[ax] = 1.0;
                    let mut k = (lo + EPS).ceil();
                    while k < hi - EPS {
                        let (a, b) = split(&rest, m, k);
                        if a.len() >= 3 && area2(&a) > 1e-12 {
                            next.push(a);
                        }
                        rest = b;
                        if rest.len() < 3 {
                            break;
                        }
                        k += 1.0;
                    }
                    if rest.len() >= 3 && area2(&rest) > 1e-12 {
                        next.push(rest);
                    }
                }
                cur = next;
            }
            for p in cur {
                flat.push((i, fi, p));
            }
        }
    }
    // 頂点（位置と向きが同じなら 1 つ）。T 字のつなぎ目（となりの辺の途中の頂点）は辺に足す（すき間が光らないように）
    let q = |p: V3| [(p[0] * 1e6).round() as i64, (p[1] * 1e6).round() as i64, (p[2] * 1e6).round() as i64];
    let mut pos: HashMap<[i64; 3], V3> = HashMap::new();
    for (_, _, p) in &flat {
        for v in p {
            pos.entry(q(*v)).or_insert(*v);
        }
    }
    let hc = |x: f64| (x * 4.0).floor() as i64;
    let mut hgrid: HashMap<[i64; 3], Vec<V3>> = HashMap::new();
    for v in pos.values() {
        hgrid.entry([hc(v[0]), hc(v[1]), hc(v[2])]).or_default().push(*v);
    }
    let with_t: Vec<Vec<V3>> = flat.par_iter().map(|(_, _, p)| {
        let mut out = vec![];
        let n = p.len();
        for e in 0..n {
            let (a, b) = (p[e], p[(e + 1) % n]);
            out.push(a);
            let d = sub(b, a);
            let l2 = dot(d, d);
            if l2 < 1e-14 {
                continue;
            }
            let mut mid: Vec<(f64, V3)> = vec![];
            for x in hc(a[0].min(b[0]))..=hc(a[0].max(b[0])) {
                for y in hc(a[1].min(b[1]))..=hc(a[1].max(b[1])) {
                    for z in hc(a[2].min(b[2]))..=hc(a[2].max(b[2])) {
                        if let Some(l) = hgrid.get(&[x, y, z]) {
                            for v in l {
                                let t = dot(sub(*v, a), d) / l2;
                                if t <= 1e-6 || t >= 1.0 - 1e-6 {
                                    continue;
                                }
                                let off = sub(sub(*v, a), mul(d, t));
                                if dot(off, off) < 1e-12 {
                                    mid.push((t, *v));
                                }
                            }
                        }
                    }
                }
            }
            mid.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            mid.dedup_by(|x, y| (x.0 - y.0).abs() < 1e-9);
            out.extend(mid.into_iter().map(|x| x.1));
        }
        out
    }).collect();
    if prof { eprintln!("poly: grid+T {:?} polys {}", t0.elapsed(), flat.len()); }
    // 同じブロック・同じ平面の断片は 1 つの多角形にまとめる（中の辺は向きが逆の 2 本なので消える）。
    // 輪の頂点のうち、どの多角形でもまっすぐ通り抜けるだけの点は消す（T 字のつなぎ目は作らない）
    let mut groups: HashMap<(usize, i64, [i64; 3]), Vec<usize>> = HashMap::new();
    for (k, (_, fi, p)) in flat.iter().enumerate() {
        let n = ns[*fi];
        let mut c = [0.0; 3];
        for v in p {
            c = add(c, *v);
        }
        c = mul(c, 1.0 / p.len() as f64);
        let off = (dot(n, p[0]) * 1e5).round() as i64;
        groups.entry((*fi, off, [c[0].floor() as i64, c[1].floor() as i64, c[2].floor() as i64])).or_default().push(k);
    }
    let mut glist: Vec<((usize, i64, [i64; 3]), Vec<usize>)> = groups.into_iter().collect();
    glist.sort_by(|a, b| a.1[0].cmp(&b.1[0]));
    let orient = |a: V3, b: V3, c: V3, n: V3| dot(cross(sub(b, a), sub(c, a)), n);
    // 輪を作る（できなければ None：断片のまま）
    let loops: Vec<Option<Vec<Vec<V3>>>> = glist.par_iter().map(|((fi, _, _), ks)| {
        if ks.len() == 1 {
            return Some(vec![with_t[ks[0]].clone()]);
        }
        let n = ns[*fi];
        let mut edges: HashMap<([i64; 3], [i64; 3]), V3> = HashMap::new();
        for &k in ks {
            let p = &with_t[k];
            for e in 0..p.len() {
                let (a, b) = (p[e], p[(e + 1) % p.len()]);
                let (qa, qb) = (q(a), q(b));
                if qa == qb {
                    continue;
                }
                if edges.remove(&(qb, qa)).is_none() {
                    edges.insert((qa, qb), a);
                }
            }
        }
        let mut next: HashMap<[i64; 3], ([i64; 3], V3)> = HashMap::new();
        for ((qa, qb), a) in &edges {
            if next.insert(*qa, (*qb, *a)).is_some() {
                return None;
            }
        }
        let mut out = vec![];
        let mut left: std::collections::HashSet<[i64; 3]> = next.keys().copied().collect();
        let mut starts: Vec<[i64; 3]> = next.keys().copied().collect();
        starts.sort_unstable();
        for st in starts {
            if !left.contains(&st) {
                continue;
            }
            let mut lp = vec![];
            let mut cur = st;
            loop {
                if !left.remove(&cur) {
                    return None;
                }
                let (nx, pv) = next[&cur];
                lp.push(pv);
                cur = nx;
                if cur == st {
                    break;
                }
                if !next.contains_key(&cur) {
                    return None;
                }
            }
            // 穴（向きが逆の輪）は扱わない
            let mut s = 0.0;
            for i in 1..lp.len().saturating_sub(1) {
                s += orient(lp[0], lp[i], lp[i + 1], n);
            }
            if s <= 1e-12 {
                return None;
            }
            out.push(lp);
        }
        Some(out)
    }).collect();
    // 角の点：どれかの多角形で曲がる点（断片のままの多角形の点はみな角）
    let mut corner: std::collections::HashSet<[i64; 3]> = std::collections::HashSet::new();
    for (g, (_, ks)) in glist.iter().enumerate() {
        match &loops[g] {
            None => {
                for &k in ks {
                    for v in &with_t[k] {
                        corner.insert(q(*v));
                    }
                }
            }
            Some(ls) => {
                for lp in ls {
                    let m = lp.len();
                    for i in 0..m {
                        let (a, b, c) = (lp[(i + m - 1) % m], lp[i], lp[(i + 1) % m]);
                        let (d1, d2) = (sub(b, a), sub(c, b));
                        let (l1, l2) = (norm(d1), norm(d2));
                        if l1 < 1e-12 || l2 < 1e-12 || norm(cross(d1, d2)) > 1e-7 * l1 * l2 || dot(d1, d2) <= 0.0 {
                            corner.insert(q(b));
                        }
                    }
                }
            }
        }
    }
    // 三角形に分ける（耳を切る）
    let ear_clip = |p: &[V3], n: V3| -> Option<Vec<[usize; 3]>> {
        let mut ring: Vec<usize> = (0..p.len()).collect();
        let mut tris = vec![];
        while ring.len() >= 3 {
            let m = ring.len();
            let mut cut = None;
            for k in 0..m {
                let (a, b, c) = (ring[(k + m - 1) % m], ring[k], ring[(k + 1) % m]);
                if orient(p[a], p[b], p[c], n) <= 1e-12 {
                    continue;
                }
                // ほかの点が三角形の中（辺の上も）にあれば耳でない
                let (qa, qb, qc) = (q(p[a]), q(p[b]), q(p[c]));
                let inside = ring.iter().any(|&o| {
                    let x = p[o];
                    let qx = q(x);
                    if qx == qa || qx == qb || qx == qc {
                        return false;
                    }
                    orient(p[a], p[b], x, n) >= -1e-12 && orient(p[b], p[c], x, n) >= -1e-12 && orient(p[c], p[a], x, n) >= -1e-12
                });
                if !inside {
                    cut = Some(k);
                    break;
                }
            }
            match cut {
                Some(k) => {
                    let (a, b, c) = (ring[(k + m - 1) % m], ring[k], ring[(k + 1) % m]);
                    tris.push([a, b, c]);
                    ring.remove(k);
                }
                None => {
                    // 残りが一直線（面積 0）なら終わり、そうでなければ失敗
                    let mut s = 0.0;
                    for i in 1..ring.len() - 1 {
                        s += orient(p[ring[0]], p[ring[i]], p[ring[i + 1]], n).abs();
                    }
                    if s <= 1e-10 {
                        break;
                    }
                    return None;
                }
            }
        }
        Some(tris)
    };
    let done: Vec<Vec<(Vec<V3>, usize, Vec<[usize; 3]>)>> = glist.par_iter().enumerate().map(|(g, ((fi, _, _), ks))| {
        let n = ns[*fi];
        let prim = flat[ks[0]].0;
        let mut out = vec![];
        if let Some(ls) = &loops[g] {
            for lp in ls {
                let red: Vec<V3> = lp.iter().copied().filter(|v| corner.contains(&q(*v))).collect();
                match if red.len() >= 3 { ear_clip(&red, n) } else { None } {
                    Some(t) => out.push((red, prim, t)),
                    None => {
                        out.clear();
                        break;
                    }
                }
            }
            if out.len() == ls.len() {
                return out;
            }
            out.clear();
        }
        for &k in ks {
            let p = with_t[k].clone();
            if let Some(t) = ear_clip(&p, n) {
                out.push((p, flat[k].0, t));
            }
        }
        out
    }).collect();
    let mut verts: Vec<V3> = vec![];
    let mut vert_prim: Vec<usize> = vec![];
    let mut vid: HashMap<([i64; 3], usize), i64> = HashMap::new();
    let mut faces: Vec<[i64; 3]> = vec![];
    let mut face_prim: Vec<usize> = vec![];
    for (g, (key, _)) in glist.iter().enumerate() {
        let fi = key.0;
        for (p, pr, ts) in &done[g] {
            let ids: Vec<i64> = p.iter().map(|v| {
                *vid.entry((q(*v), fi)).or_insert_with(|| {
                    verts.push(*v);
                    vert_prim.push(*pr);
                    (verts.len() - 1) as i64
                })
            }).collect();
            for t in ts {
                faces.push([ids[t[0]], ids[t[1]], ids[t[2]]]);
                face_prim.push(*pr);
            }
        }
    }
    if prof { eprintln!("poly: done {:?} tris {}", t0.elapsed(), faces.len()); }
    Out { verts, faces, face_prim, vert_prim }
}
