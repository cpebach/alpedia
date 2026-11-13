//! 問い合わせの幾何：形への最短距離（プレイヤーとの当たり）・視線とボクセルの交差・角の暗さ（AO）・OBJ の読み込みとボクセル化。
//! どれも main/ の numpy 版と同じ計算（同じ順で比べ、同じものを選ぶ）。

use std::collections::HashMap;

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
#[inline]
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
#[inline]
fn axpy(a: [f64; 3], s: f64, b: [f64; 3]) -> [f64; 3] {
    [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2]]
}

/// 点と線分の距離と最近点（shape.dist_segments）
pub fn dist_segment(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> (f64, [f64; 3]) {
    let ab = sub(b, a);
    let t = (dot(sub(p, a), ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
    let q = axpy(a, t, ab);
    (norm(sub(p, q)), q)
}

/// 点と三角形の距離と最近点（shape.dist_triangles）
pub fn dist_triangle(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> (f64, [f64; 3]) {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let n0 = cross(ab, ac);
    let nn = norm(n0).max(1e-12);
    let n = [n0[0] / nn, n0[1] / nn, n0[2] / nn];
    let ap = sub(p, a);
    let dn = dot(ap, n);
    let proj = axpy(p, -dn, n);
    let v2 = sub(proj, a);
    let d00 = dot(ab, ab);
    let d01 = dot(ab, ac);
    let d11 = dot(ac, ac);
    let d20 = dot(v2, ab);
    let d21 = dot(v2, ac);
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
    let (d1, q1) = dist_segment(p, a, b);
    let (d2, q2) = dist_segment(p, b, c);
    let (d3, q3) = dist_segment(p, c, a);
    let de = d1.min(d2).min(d3);
    let qe = if d1 <= d2 && d1 <= d3 { q1 } else if d2 <= d3 { q2 } else { q3 };
    (de, qe)
}

/// 点 p から構築物の形（芯 = 点・線・三角形）への最短：(芯からの距離, 最近点, ボクセル番号)。近くに何も無ければ None
pub fn closest_on_shape(cen: &[[f64; 3]], segs: &[[i64; 2]], tris: &[[i64; 3]], p: [f64; 3], max_dist: f64, size: f64)
    -> Option<(f64, [f64; 3], i64)> {
    let lim = max_dist + size * 1.8;
    let near: Vec<bool> = cen.iter().map(|&c| norm(sub(c, p)) < lim).collect();
    let mut best: Option<(f64, [f64; 3], i64)> = None;
    for (i, &c) in cen.iter().enumerate() {
        if near[i] {
            let d = norm(sub(c, p));
            if best.map_or(true, |b| d < b.0) {
                best = Some((d, c, i as i64));
            }
        }
    }
    let mut best = best?;
    let mut sb: Option<(f64, [f64; 3], i64)> = None;
    for s in segs {
        let (a, b) = (s[0] as usize, s[1] as usize);
        if near[a] || near[b] {
            let (d, q) = dist_segment(p, cen[a], cen[b]);
            if sb.map_or(true, |x| d < x.0) {
                sb = Some((d, q, s[0]));
            }
        }
    }
    if let Some(x) = sb {
        if x.0 < best.0 {
            best = x;
        }
    }
    let mut tb: Option<(f64, [f64; 3], i64)> = None;
    for t in tris {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if near[a] || near[b] || near[c] {
            let (d, q) = dist_triangle(p, cen[a], cen[b], cen[c]);
            if tb.map_or(true, |x| d < x.0) {
                tb = Some((d, q, t[0]));
            }
        }
    }
    if let Some(x) = tb {
        if x.0 < best.0 {
            best = x;
        }
    }
    Some(best)
}

/// 視線（o から向き d、max_dist まで）といちばん手前で交わるボクセル：(t, 番号, 面)。
/// frames：ボクセルごとのローカル軸 (行が x, y, z)。world.raycast の構築物の部分と同じ
pub fn voxel_raycast(cen: &[[f64; 3]], frames: &[[[f64; 3]; 3]], o: [f64; 3], d: [f64; 3], size: f64, max_dist: f64)
    -> Option<(f64, i64, i64)> {
    let half = size * 0.5;
    let mut best: Option<(f64, i64, i64)> = None;
    for (i, &c) in cen.iter().enumerate() {
        let rel = sub(c, o);
        let tp = dot(rel, d);
        let perp = norm(axpy(rel, -tp, d));
        if !(perp < size && tp > -size && tp < max_dist + size) {
            continue;
        }
        let f = frames[i];
        let oc = sub(o, c);
        let mut tmin = [0.0; 3];
        let mut tmax = [0.0; 3];
        let mut ld = [0.0; 3];
        for k in 0..3 {
            let lo = dot(f[k], oc);
            ld[k] = dot(f[k], d);
            let inv = 1.0 / if ld[k].abs() < 1e-12 { 1e-12 } else { ld[k] };
            let t1 = (-half - lo) * inv;
            let t2 = (half - lo) * inv;
            tmin[k] = t1.min(t2);
            tmax[k] = t1.max(t2);
        }
        let tn = tmin[0].max(tmin[1]).max(tmin[2]);
        let tf = tmax[0].min(tmax[1]).min(tmax[2]);
        if !(tn <= tf && tf > 0.0 && tn < max_dist) {
            continue;
        }
        let t = tn.max(0.0);
        if best.map_or(true, |b| t < b.0) {
            let mut ax = 0;
            for k in 1..3 {
                if tmin[k] > tmin[ax] {
                    ax = k;
                }
            }
            let s = ld[ax];
            let sign = if s > 0.0 { -1.0 } else if s < 0.0 { 1.0 } else { 0.0 };
            let face = (ax * 2) as i64 + if sign > 0.0 { 0 } else { 1 };
            best = Some((t, i as i64, face));
        }
    }
    best
}

/// 角の暗さ（環境光の遮り）：頂点の外側の半球に、ほかのブロックがどれだけあるか（render.block_ao）。
/// gid・owner があれば、頂点の持ち主と同じ部分のブロックだけを数える
pub fn block_ao(cells: &[[i64; 3]], p: &[[f64; 3]], n: &[[f64; 3]], gid: Option<(&[i64], &[i64])>, strength: f64) -> Vec<f64> {
    let mut dirs = Vec::with_capacity(26);
    for x in -1..=1i32 {
        for y in -1..=1i32 {
            for z in -1..=1i32 {
                if (x, y, z) != (0, 0, 0) {
                    let v = [x as f64, y as f64, z as f64];
                    let l = norm(v);
                    dirs.push([v[0] / l, v[1] / l, v[2] / l]);
                }
            }
        }
    }
    let mut idx: HashMap<[i64; 3], usize> = HashMap::with_capacity(cells.len());
    for (i, c) in cells.iter().enumerate() {
        idx.entry(*c).or_insert(i);
    }
    p.iter()
        .zip(n.iter())
        .enumerate()
        .map(|(v, (&pp, &nn))| {
            let mut num = 0.0;
            let mut den = 0.0;
            for dd in &dirs {
                let w = dot(*dd, nn);
                if !(w > 0.25) {
                    continue;
                }
                den += w;
                let q = [pp[0] + nn[0] * 0.3 + dd[0] * 0.75, pp[1] + nn[1] * 0.3 + dd[1] * 0.75, pp[2] + nn[2] * 0.3 + dd[2] * 0.75];
                let cell = [q[0].floor() as i64, q[1].floor() as i64, q[2].floor() as i64];
                if let Some(&k) = idx.get(&cell) {
                    let same = match gid {
                        Some((g, owner)) => g[k] == g[owner[v] as usize],
                        None => true,
                    };
                    if same {
                        num += w;
                    }
                }
            }
            let frac = num / den.max(1e-9);
            1.0 - strength * (frac * 1.6).clamp(0.0, 1.0)
        })
        .collect()
}

/// OBJ の文字列 → (頂点, 三角形, 三角形ごとの素材名の番号, 素材名)（objio.parse_obj）
pub fn parse_obj(text: &str) -> Result<(Vec<[f64; 3]>, Vec<[i64; 3]>, Vec<i64>, Vec<String>), String> {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris = Vec::new();
    let mut tmat = Vec::new();
    let mut names: Vec<String> = vec!["default".to_string()];
    let mut name_id: HashMap<String, i64> = HashMap::new();
    name_id.insert("default".to_string(), 0);
    let mut cur = 0i64;
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        match parts[0] {
            "v" if parts.len() >= 4 => {
                let mut v = [0.0; 3];
                for k in 0..3 {
                    v[k] = parse_float(parts[1 + k]).ok_or_else(|| format!("数値として読めません: {}", parts[1 + k]))?;
                }
                verts.push(v);
            }
            "usemtl" => {
                let nm = if parts.len() > 1 { parts[1] } else { "default" };
                cur = match name_id.get(nm) {
                    Some(&k) => k,
                    None => {
                        let k = names.len() as i64;
                        names.push(nm.to_string());
                        name_id.insert(nm.to_string(), k);
                        k
                    }
                };
            }
            "f" => {
                let mut idx = Vec::with_capacity(parts.len() - 1);
                for p in &parts[1..] {
                    let s = p.split('/').next().unwrap_or("");
                    let k: i64 = s.trim().parse().map_err(|_| format!("面の番号として読めません: {p}"))?;
                    idx.push(if k > 0 { k - 1 } else { verts.len() as i64 + k });
                }
                for j in 1..idx.len().saturating_sub(1) {
                    tris.push([idx[0], idx[j], idx[j + 1]]);
                    tmat.push(cur);
                }
            }
            _ => {}
        }
    }
    Ok((verts, tris, tmat, names))
}

fn parse_float(s: &str) -> Option<f64> {
    let t = s.trim();
    if let Ok(v) = t.parse::<f64>() {
        return Some(v);
    }
    match t.to_ascii_lowercase().as_str() {
        "nan" | "+nan" | "-nan" => Some(f64::NAN),
        "inf" | "+inf" | "infinity" | "+infinity" => Some(f64::INFINITY),
        "-inf" | "-infinity" => Some(f64::NEG_INFINITY),
        _ => None,
    }
}

/// 閉じたメッシュの内部をボクセル化（x 方向のレイの交差回数）：[(i, j, k, 三角形の番号)]（objio.voxelize）
pub fn voxelize(verts: &[[f64; 3]], tris: &[[i64; 3]], size: f64, n: [i64; 3]) -> Vec<([i64; 3], usize)> {
    let mut lo = [f64::INFINITY; 3];
    for v in verts {
        for a in 0..3 {
            lo[a] = lo[a].min(v[a]);
        }
    }
    let t: Vec<[[f64; 3]; 3]> = tris.iter().map(|t| [verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]]).collect();
    let eps = 1e-4;
    let mut out = Vec::new();
    let mut hits: Vec<(f64, usize)> = Vec::new();
    for j in 0..n[1] {
        let y = lo[1] + (j as f64 + 0.5) * size + eps;
        for k in 0..n[2] {
            let z = lo[2] + (k as f64 + 0.5) * size + eps * 1.7;
            hits.clear();
            for (ti, tr) in t.iter().enumerate() {
                let (a, b, c) = (tr[0], tr[1], tr[2]);
                let v0 = [b[1] - a[1], b[2] - a[2]];
                let v1 = [c[1] - a[1], c[2] - a[2]];
                let v2 = [y - a[1], z - a[2]];
                let den = v0[0] * v1[1] - v1[0] * v0[1];
                if !(den.abs() > 1e-12) {
                    continue;
                }
                let u = (v2[0] * v1[1] - v1[0] * v2[1]) / den;
                let w = (v0[0] * v2[1] - v2[0] * v0[1]) / den;
                if u >= 0.0 && w >= 0.0 && u + w <= 1.0 {
                    hits.push((a[0] + u * (b[0] - a[0]) + w * (c[0] - a[0]), ti));
                }
            }
            if hits.is_empty() {
                continue;
            }
            hits.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut e = 0;
            while e + 1 < hits.len() {
                let (x0, ti) = hits[e];
                let x1 = hits[e + 1].0;
                let i0 = ((x0 - lo[0]) / size - 0.5).ceil().max(0.0) as i64;
                let i1 = (((x1 - lo[0]) / size - 0.5).floor() as i64).min(n[0] - 1);
                for i in i0..=i1 {
                    out.push(([i, j, k], ti));
                }
                e += 2;
            }
        }
    }
    out
}

/// レイと向き付きの箱の交差：(t, 箱の番号, 法線) か None（geom.ray_boxes）。a：箱ごとの軸 (行が軸)
pub fn ray_boxes(o: [f64; 3], d: [f64; 3], c: &[[f64; 3]], a: &[[[f64; 3]; 3]], e: &[[f64; 3]], max_t: f64)
    -> Option<(f64, i64, [f64; 3])> {
    let mut best: Option<(f64, usize, [f64; 3], [f64; 3])> = None;   // (max(tn,0), 番号, tmin, ld)
    for b in 0..c.len() {
        let oc = sub(o, c[b]);
        let mut tmin = [0.0; 3];
        let mut tmax = [0.0; 3];
        let mut ld = [0.0; 3];
        for k in 0..3 {
            let lo = dot(a[b][k], oc);
            ld[k] = dot(a[b][k], d);
            let inv = 1.0 / if ld[k].abs() < 1e-12 { 1e-12 } else { ld[k] };
            let t1 = (-e[b][k] - lo) * inv;
            let t2 = (e[b][k] - lo) * inv;
            tmin[k] = t1.min(t2);
            tmax[k] = t1.max(t2);
        }
        let tn = tmin[0].max(tmin[1]).max(tmin[2]);
        let tf = tmax[0].min(tmax[1]).min(tmax[2]);
        if !(tn <= tf && tf > 0.0 && tn < max_t) {
            continue;
        }
        let t = tn.max(0.0);
        if best.map_or(true, |x| t < x.0) {
            best = Some((t, b, tmin, ld));
        }
    }
    let (t, b, tmin, ld) = best?;
    let mut ax = 0;
    for k in 1..3 {
        if tmin[k] > tmin[ax] {
            ax = k;
        }
    }
    let s = if ld[ax] > 0.0 { -1.0 } else if ld[ax] < 0.0 { 1.0 } else { 0.0 };
    let r = a[b][ax];
    Some((t, b as i64, [r[0] * s, r[1] * s, r[2] * s]))
}
