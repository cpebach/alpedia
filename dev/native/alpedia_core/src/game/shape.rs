//! 構築物の「形」（main/shape.py）：ボクセルを球として扱い、結合したボクセルの間を埋める。
//! 形 = 芯（ボクセル中心の点・結合の線・2×2×2 で結合しているボクセル群の凸包の三角形）を半径 r でなぞった和集合。
//! 描画（マーチングキューブでメッシュ化してボクセルの8頂点で変形）と当たり判定の両方で使う。

use std::collections::HashMap;

use numpy::{PyArray1, PyReadonlyArrayDyn, PyUntypedArrayMethods};
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyDict, PyTuple};

use super::construct::{arr, bv, cell_of, fv, iarr, iv, opt, v3, v3s, R, V3};
use super::imp;
use super::shape_tables::{HULL_START, HULL_TRIS};


/// 等値面：(頂点 (n,3)、三角形 (m,3)、None、None)。頂点は配列の番号 × spacing、値が level 以下の所を中とする
#[pyfunction]
#[pyo3(signature = (vol, level=0.0, spacing=(1.0, 1.0, 1.0)))]
pub fn shape_marching_cubes<'py>(py: Python<'py>, vol: &Bound<'py, PyAny>, level: f32, spacing: (f64, f64, f64)) -> R<Bound<'py, PyTuple>> {
    let a = super::construct::np(py)?.call_method1("ascontiguousarray", (vol, "float32"))?;
    let r: PyReadonlyArrayDyn<f32> = a.extract()?;
    let sh = r.shape().to_vec();
    if sh.len() != 3 {
        return Err(pyo3::exceptions::PyValueError::new_err("marching_cubes: 3次元の配列にしてください"));
    }
    let data = r.as_slice()?.to_vec();
    let (v, f) = py.allow_threads(|| crate::geomx::marching_cubes(&data, [sh[0], sh[1], sh[2]], level, [spacing.0, spacing.1, spacing.2]));
    let (nv, nf) = (v.len(), f.len());
    Ok(PyTuple::new_bound(py, [arr(py, v.into_iter().flatten().collect(), &[nv, 3])?, iarr(py, f.into_iter().flatten().collect(), &[nf, 3])?,
        py.None().into_bound(py), py.None().into_bound(py)]))
}

/// 疎な行列（行ごとに、列の番号と値）。当たり判定の点の重みに使う
#[pyclass(module = "alpedia_core")]
pub struct Csr {
    #[pyo3(get)]
    indices: PyObject,
    #[pyo3(get)]
    data: PyObject,
    #[pyo3(get)]
    indptr: PyObject,
    #[pyo3(get)]
    rows: PyObject,
    #[pyo3(get)]
    shape: (usize, usize),
    r: Vec<usize>,
    c: Vec<usize>,
    v: Vec<f64>,
}

impl Csr {
    fn build(py: Python<'_>, rows: Vec<i64>, cols: Vec<i64>, vals: Vec<f64>, shape: (usize, usize)) -> R<Self> {
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by_key(|&i| (rows[i], cols[i]));          // 安定な並べ替え（np.lexsort と同じ）
        let r: Vec<usize> = order.iter().map(|&i| rows[i] as usize).collect();
        let c: Vec<usize> = order.iter().map(|&i| cols[i] as usize).collect();
        let v: Vec<f64> = order.iter().map(|&i| vals[i]).collect();
        let mut ip = vec![0i64; shape.0 + 1];
        for &x in &r {
            ip[x + 1] += 1;
        }
        for k in 1..ip.len() {
            ip[k] += ip[k - 1];
        }
        let n = r.len();
        let ni = ip.len();
        Ok(Csr {
            indices: iarr(py, c.iter().map(|x| *x as i64).collect(), &[n])?.unbind(),
            data: arr(py, v.clone(), &[n])?.unbind(),
            indptr: iarr(py, ip, &[ni])?.unbind(),
            rows: iarr(py, r.iter().map(|x| *x as i64).collect(), &[n])?.unbind(),
            shape, r, c, v,
        })
    }
}

#[pymethods]
impl Csr {
    #[new]
    fn new(py: Python<'_>, rows: &Bound<'_, PyAny>, cols: &Bound<'_, PyAny>, vals: &Bound<'_, PyAny>, shape: (usize, usize)) -> R<Self> {
        Self::build(py, iv(rows)?, iv(cols)?, fv(vals)?, shape)
    }
    fn tocsr(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn sort_indices(&self) {}
    fn __matmul__<'py>(&self, py: Python<'py>, x: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let a = super::construct::np(py)?.call_method1("asarray", (x, "float64"))?;
        let sh: Vec<usize> = a.getattr("shape")?.extract()?;
        let inner: usize = sh[1..].iter().product();
        let xv = fv(&a)?;
        let mut out = vec![0.0; self.shape.0 * inner];
        for q in 0..self.r.len() {
            let (r, c, w) = (self.r[q], self.c[q], self.v[q]);
            for k in 0..inner {
                out[r * inner + k] += w * xv[c * inner + k];
            }
        }
        let mut osh = vec![self.shape.0];
        osh.extend_from_slice(&sh[1..]);
        arr(py, out, &osh)
    }
}

fn hull() -> Vec<Vec<[usize; 3]>> {
    (0..256).map(|b| HULL_TRIS[HULL_START[b] as usize..HULL_START[b + 1] as usize].iter()
        .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize]).collect()).collect()
}

/// 2×2×2 の窓の中の結合成分 → 三角形 の表：(始まり (257,), 三角形 (m,3))
#[pyfunction]
pub fn shape_hull_table(py: Python<'_>) -> R<(Bound<'_, PyAny>, Bound<'_, PyAny>)> {
    let n = HULL_TRIS.len();
    Ok((iarr(py, HULL_START.iter().map(|x| *x as i64).collect(), &[257])?,
        iarr(py, HULL_TRIS.iter().flat_map(|t| t.iter().map(|x| *x as i64)).collect(), &[n, 3])?))
}

/// ブロックの種類の番号（素材ごと。機器は機器の種類ごと）。階段状の隙間を埋めるのは同じ種類どうしだけ
pub fn block_kinds_v(c: &Bound<'_, PyAny>) -> R<Vec<i64>> {
    let py = c.py();
    let names = PyDict::new_bound(py);
    let mut out = vec![];
    for (m, d) in c.getattr("mats")?.iter()?.zip(c.getattr("devs")?.iter()?) {
        let (m, d) = (m?, d?);
        let key = if d.is_truthy()? { PyTuple::new_bound(py, ["dev".into_py(py), d.get_item("def")?.unbind()]) }
                  else { PyTuple::new_bound(py, ["mat".into_py(py), m.unbind()]) };
        let n = names.len();
        out.push(names.call_method1("setdefault", (key, n))?.extract::<i64>()?);
    }
    Ok(out)
}
#[pyfunction]
pub fn shape_block_kinds<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let v = block_kinds_v(c)?;
    let n = v.len();
    iarr(py, v, &[n])
}

/// 形を作る図形の一覧: dict(points, segs, tris)（ボクセル番号の配列）
#[pyfunction]
pub fn shape_topology<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let idx = c.getattr("index")?;
    let pairs = |keys: Bound<'py, PyAny>| -> R<Vec<[usize; 2]>> {
        let mut out = vec![];
        for k in keys.iter()? {
            let k = k?;
            let a = idx.call_method1("get", (k.get_item(0)?,))?;
            let b = idx.call_method1("get", (k.get_item(1)?,))?;
            if !a.is_none() && !b.is_none() {
                out.push([a.extract()?, b.extract()?]);
            }
        }
        Ok(out)
    };
    let cells: Vec<[i64; 3]> = c.getattr("cells")?.iter()?.map(|x| cell_of(&x?)).collect::<R<_>>()?;
    let l = pairs(c.getattr("links")?)?;
    let e = match opt(c, "extras")? { Some(x) => pairs(x)?, None => vec![] };
    let k = block_kinds_v(c)?;
    let h = hull();
    let (p, s, t) = py.allow_threads(|| crate::topo::topology(&cells, &l, &e, &k, &h));
    let (np_, ns, nt) = (p.len(), s.len(), t.len());
    let d = PyDict::new_bound(py);
    d.set_item("points", iarr(py, p, &[np_])?)?;
    d.set_item("segs", iarr(py, s.into_iter().flatten().collect(), &[ns, 2])?)?;
    d.set_item("tris", iarr(py, t.into_iter().flatten().collect(), &[nt, 3])?)?;
    Ok(d)
}

/// ボクセルごとの「角ばった見た目にするか」（素材の facet）
pub fn facet_v(c: &Bound<'_, PyAny>) -> R<Vec<bool>> {
    let py = c.py();
    let reg = c.getattr("reg")?;
    let mut cache: HashMap<String, bool> = HashMap::new();
    let mut out = vec![];
    for m in c.getattr("mats")?.iter()? {
        let m = m?;
        let key = m.repr()?.to_string();
        let v = match cache.get(&key) {
            Some(v) => *v,
            None => {
                let d = reg.call_method1("material", (&m,))?;
                let d = if d.is_truthy()? { d } else { PyDict::new_bound(py).into_any() };
                let v = d.call_method1("get", ("facet", false))?.is_truthy()?;
                cache.insert(key, v);
                v
            }
        };
        out.push(v);
    }
    Ok(out)
}
/// ボクセルごとの「透けるか」（素材の opacity が 1 未満。ガラスなど。機器は透けない）
pub fn clear_v(c: &Bound<'_, PyAny>) -> R<Vec<bool>> {
    let reg = c.getattr("reg")?;
    let mut cache: HashMap<String, bool> = HashMap::new();
    let mut out = vec![];
    for (m, d) in c.getattr("mats")?.iter()?.zip(c.getattr("devs")?.iter()?) {
        let (m, d) = (m?, d?);
        if d.is_truthy()? {
            out.push(false);
            continue;
        }
        let key = m.repr()?.to_string();
        let v = match cache.get(&key) {
            Some(v) => *v,
            None => {
                let md = reg.call_method1("material", (&m,))?;
                let v = if md.is_truthy()? {
                    let o = md.call_method1("get", ("opacity", 1.0))?;
                    !o.is_none() && o.extract::<f64>().unwrap_or(1.0) < 0.999
                } else {
                    false
                };
                cache.insert(key, v);
                v
            }
        };
        out.push(v);
    }
    Ok(out)
}

#[pyfunction]
pub fn shape_facet_voxels<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    Ok(PyArray1::from_vec_bound(py, facet_v(c)?).into_any())
}

/// 点（セル単位）が属するボクセル：そのセルのボクセル、無ければいちばん近いボクセル（allowed があればその中から）
fn owner_of(cells: &[[i64; 3]], centers: &[V3], p: &[V3], allowed: Option<&[bool]>) -> Vec<usize> {
    let map: HashMap<[i64; 3], usize> = cells.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    let mut owner: Vec<i64> = p.iter().map(|q| {
        let cell = [q[0].floor() as i64, q[1].floor() as i64, q[2].floor() as i64];
        match map.get(&cell) {
            Some(&i) if allowed.map(|a| a[i]).unwrap_or(true) => i as i64,
            _ => -1,
        }
    }).collect();
    let miss: Vec<usize> = (0..p.len()).filter(|&i| owner[i] < 0).collect();
    if !miss.is_empty() {
        let cand: Vec<usize> = match allowed { None => (0..centers.len()).collect(), Some(a) => (0..a.len()).filter(|&i| a[i]).collect() };
        let cc: Vec<V3> = cand.iter().map(|&i| centers[i]).collect();
        let pp: Vec<V3> = miss.iter().map(|&i| p[i]).collect();
        let nn = crate::geomx::nearest(&cc, &pp);
        for (q, &i) in miss.iter().enumerate() {
            owner[i] = if nn[q] >= 0 { cand[nn[q] as usize] as i64 } else { -1 };
        }
    }
    owner.into_iter().map(|x| x.max(0) as usize).collect()
}

#[pyfunction]
#[pyo3(signature = (c, p, centers, allowed=None))]
pub fn shape_owner<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, p: &Bound<'py, PyAny>, centers: &Bound<'py, PyAny>,
                        allowed: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
    let cells: Vec<[i64; 3]> = c.getattr("cells")?.iter()?.map(|x| cell_of(&x?)).collect::<R<_>>()?;
    let al = match allowed { Some(a) if !a.is_none() => Some(bv(a)?), _ => None };
    let o = owner_of(&cells, &v3s(centers)?, &v3s(p)?, al.as_deref());
    let n = o.len();
    iarr(py, o.into_iter().map(|x| x as i64).collect(), &[n])
}

/// 形の表面メッシュを作る（格子座標＝セル単位）。ブロックが無ければ None
#[pyfunction]
#[pyo3(signature = (c, k=None))]
pub fn shape_build_mesh<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, k: Option<i64>) -> R<Bound<'py, PyAny>> {
    let none = py.None().into_bound(py);
    let n: usize = c.getattr("n_vox")?.extract()?;
    if n == 0 {
        return Ok(none);
    }
    let topo = c.getattr("topo")?;
    let points: Vec<usize> = iv(&topo.get_item("points")?)?.into_iter().map(|x| x as usize).collect();
    let segs: Vec<[usize; 2]> = iv(&topo.get_item("segs")?)?.chunks_exact(2).map(|x| [x[0] as usize, x[1] as usize]).collect();
    let tris: Vec<[usize; 3]> = iv(&topo.get_item("tris")?)?.chunks_exact(3).map(|x| [x[0] as usize, x[1] as usize, x[2] as usize]).collect();
    let nprim = points.len() + segs.len() + tris.len();
    let mut k: i64 = k.unwrap_or(if nprim < 60 { 8 } else if nprim < 400 { 6 } else if nprim < 1500 { 4 } else if nprim < 6000 { 3 } else { 2 });
    let r = 0.5;
    let cells: Vec<[i64; 3]> = c.getattr("cells")?.iter()?.map(|x| cell_of(&x?)).collect::<R<_>>()?;
    let centers: Vec<V3> = cells.iter().map(|q| [q[0] as f64 + 0.5, q[1] as f64 + 0.5, q[2] as f64 + 0.5]).collect();
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for q in &cells {
        for a in 0..3 {
            lo[a] = lo[a].min(q[a] as f64);
            hi[a] = hi[a].max(q[a] as f64);
        }
    }
    for a in 0..3 {
        lo[a] -= 1.0;
        hi[a] += 2.0;
    }
    let dims_of = |k: i64| -> [usize; 3] { [0, 1, 2].map(|a| ((hi[a] - lo[a]) * k as f64).ceil() as usize + 1) };
    let mut dims = dims_of(k);
    if (dims[0] as f64) * (dims[1] as f64) * (dims[2] as f64) > 30_000_000.0 {
        k = 2;
        dims = dims_of(k);
    }
    let m = (2 * k + 3) as usize;
    let hard = facet_v(c)?;
    let kf = k as f64;
    let shape_of = |pts: &[usize], sg: &[[usize; 2]], tr: &[[usize; 3]], extra: &[[V3; 2]]| -> Vec<f32> {
        let mut prims: Vec<(u8, [[f64; 3]; 3], bool)> = Vec::with_capacity(pts.len() + sg.len() + tr.len() + extra.len());
        for &p in pts {
            prims.push((0, [centers[p], [0.0; 3], [0.0; 3]], hard[p]));
        }
        for s in sg {
            prims.push((1, [centers[s[0]], centers[s[1]], [0.0; 3]], hard[s[0]] && hard[s[1]]));
        }
        for e in extra {
            prims.push((1, [e[0], e[1], [0.0; 3]], false));
        }
        for t in tr {
            prims.push((2, [centers[t[0]], centers[t[1]], centers[t[2]]], hard[t[0]] && hard[t[1]] && hard[t[2]]));
        }
        py.allow_threads(|| crate::topo::shape_sdf(dims, lo, kf, r, m, &prims))
    };
    let has_rotors = match opt(c, "rotors")? { Some(x) => x.is_truthy()?, None => false };
    let rotor_gid: Option<Vec<usize>> = if has_rotors && c.hasattr("_art_groups")? {
        Some(iv(&c.call_method0("_art_groups")?)?.into_iter().map(|x| x as usize).collect())
    } else { None };
    // 透ける素材（ガラス）と透けない素材は別の部分（別の形）にする。1つの形だと、ガラスと接した所に
    // 透けないブロックの面ができず、ガラス越しに隣のブロックの中（裏側）が見えてしまうため。
    // 別の部分にすると、境に両側の切り口の面ができる（アクチュエーターで別に動く部分と同じ作り方）
    let clear = clear_v(c)?;
    let split_clear = clear.iter().any(|x| *x) && clear.iter().any(|x| !*x);
    let gid: Option<Vec<usize>> = match (&rotor_gid, split_clear) {
        (Some(g), true) => Some(g.iter().zip(&clear).map(|(g, t)| g * 2 + *t as usize).collect()),
        (None, true) => Some(clear.iter().map(|t| *t as usize).collect()),
        (g, false) => g.clone(),
    };
    let mut parts: Vec<(Vec<f32>, Option<Vec<bool>>)> = vec![];
    match &gid {
        Some(g) if g.iter().copied().max().unwrap_or(0) > 0 => {
            // 回転ジョイント（アクチュエーター）で別に動く部分・透けるかどうかごとに、別の形・別のメッシュにする
            let mut rotor_pairs: Vec<(usize, usize)> = vec![];
            let rotors = match opt(c, "rotors")? { Some(x) if !x.is_none() => x.iter()?.collect::<R<Vec<_>>>()?, _ => vec![] };
            for r_ in rotors {
                let (a, b): (usize, usize) = (r_.get_item("a")?.extract()?, r_.get_item("b")?.extract()?);
                let key = (a.min(b), a.max(b));
                if !rotor_pairs.contains(&key) {
                    rotor_pairs.push(key);
                }
            }
            // 部分をまたぐ結合（ガラスと鋼鉄のつなぎ目など）も、両側から真ん中まで伸ばして、つながった形を保つ
            for s in &segs {
                if g[s[0]] != g[s[1]] {
                    let key = (s[0].min(s[1]), s[0].max(s[1]));
                    if !rotor_pairs.contains(&key) {
                        rotor_pairs.push(key);
                    }
                }
            }
            let gn = g.iter().copied().max().unwrap() + 1;
            let mut vols = vec![];
            for gg in 0..gn {
                let pts: Vec<usize> = points.iter().copied().filter(|&p| g[p] == gg).collect();
                let sg: Vec<[usize; 2]> = segs.iter().copied().filter(|s| g[s[0]] == gg && g[s[1]] == gg).collect();
                let tr: Vec<[usize; 3]> = tris.iter().copied().filter(|t| g[t[0]] == gg && g[t[1]] == gg && g[t[2]] == gg).collect();
                let mut half = vec![];
                for &(a, b) in &rotor_pairs {
                    for (x, y) in [(a, b), (b, a)] {
                        if g[x] == gg && g[y] != gg {
                            let (cx, cy) = (centers[x], centers[y]);
                            half.push([cx, [(cx[0] + cy[0]) * 0.5, (cx[1] + cy[1]) * 0.5, (cx[2] + cy[2]) * 0.5]]);
                        }
                    }
                }
                vols.push(shape_of(&pts, &sg, &tr, &half));
            }
            let nn = vols[0].len();
            let mut union = vec![f32::INFINITY; nn];
            let mut lab = vec![0usize; nn];
            for i in 0..nn {
                let mut best = vols[0][i];
                let mut bi = 0;
                for (gg, v) in vols.iter().enumerate().skip(1) {
                    if v[i] < best || best.is_nan() && !v[i].is_nan() {
                        best = v[i];
                        bi = gg;
                    }
                }
                union[i] = vols.iter().map(|v| v[i]).fold(f32::INFINITY, |a, b| if a.is_nan() || b.is_nan() { f32::NAN } else { a.min(b) });
                lab[i] = bi;
            }
            if union.iter().all(|x| *x >= 0.0) {
                return Ok(none);
            }
            for gg in 0..gn {
                // ほかの部分の中は外側として扱う（境の切り口が両側で同じ位置になるように符号だけ反転）
                let v: Vec<f32> = (0..nn).map(|i| if lab[i] == gg { union[i] } else { union[i].abs().max(1e-4) }).collect();
                parts.push((v, Some(g.iter().map(|x| *x == gg).collect())));
            }
        }
        _ => {
            let vol = shape_of(&points, &segs, &tris, &[]);
            if vol.iter().fold(f32::INFINITY, |a, b| a.min(*b)) >= 0.0 {
                return Ok(none);
            }
            parts.push((vol, None));
        }
    }
    let mut pall: Vec<V3> = vec![];
    let mut fall: Vec<[i64; 3]> = vec![];
    let mut oall: Vec<usize> = vec![];
    for (vol, allowed) in &parts {
        if vol.iter().fold(f32::INFINITY, |a, b| a.min(*b)) >= 0.0 {
            continue;
        }
        let (verts, mut faces) = py.allow_threads(|| crate::geomx::marching_cubes(vol, dims, 0.0, [1.0, 1.0, 1.0]));
        if faces.is_empty() {
            continue;
        }
        let p: Vec<V3> = verts.iter().map(|v| [lo[0] + v[0] / kf, lo[1] + v[1] / kf, lo[2] + v[2] / kf]).collect();
        // 面の向きをそろえる（外向き）：閉じた面の符号付き体積が正になる向き
        let mut s = [0.0; 3];
        for q in &p {
            for a in 0..3 {
                s[a] += q[a];
            }
        }
        let mean = [s[0] / p.len() as f64, s[1] / p.len() as f64, s[2] / p.len() as f64];
        let mut vol6 = 0.0;
        for f in &faces {
            let (a, b, c3) = (p[f[0] as usize], p[f[1] as usize], p[f[2] as usize]);
            let cr = super::construct::cross(super::construct::sub(b, a), super::construct::sub(c3, a));
            vol6 += super::construct::dot(super::construct::sub(a, mean), cr);
        }
        if vol6 < 0.0 {
            for f in faces.iter_mut() {
                f.swap(0, 2);
            }
        }
        let off = pall.len() as i64;
        oall.extend(owner_of(&cells, &centers, &p, allowed.as_deref()));
        fall.extend(faces.iter().map(|f| [f[0] + off, f[1] + off, f[2] + off]));
        pall.extend(p);
    }
    if pall.is_empty() {
        return Ok(none);
    }
    // 各頂点をボクセルに割り当て、そのボクセルの8頂点で補間（スキニング）
    let np_ = pall.len();
    let mut w = vec![1.0f64; np_ * 8];
    for (q, p) in pall.iter().enumerate() {
        let o = cells[oall[q]];
        let u = [p[0] - o[0] as f64, p[1] - o[1] as f64, p[2] - o[2] as f64];
        for k8 in 0..8 {
            let bits = super::construct::CORNER_BITS[k8];
            for ax in 0..3 {
                w[q * 8 + k8] *= if bits[ax] > 0.0 { u[ax] } else { 1.0 - u[ax] };
            }
        }
    }
    let d = PyDict::new_bound(py);
    let nf = fall.len();
    d.set_item("rest", arr(py, pall.iter().flatten().copied().collect(), &[np_, 3])?)?;
    d.set_item("faces", iarr(py, fall.into_iter().flatten().collect(), &[nf, 3])?)?;
    d.set_item("owner", iarr(py, oall.iter().map(|x| *x as i64).collect(), &[np_])?)?;
    d.set_item("w", arr(py, w, &[np_, 8])?)?;
    d.set_item("vidx", iarr(py, oall.iter().flat_map(|o| (0..8).map(move |k| (8 * o + k) as i64)).collect(), &[np_, 8])?)?;
    d.set_item("version", c.getattr("version")?)?;
    Ok(d.into_any())
}

// ---------------------------------------------------------------- 裏での読み込み（大きい構築物のメッシュは別のスレッドで作る）

static JOBS: GILOnceCell<Py<PyDict>> = GILOnceCell::new();
static POOL: GILOnceCell<PyObject> = GILOnceCell::new();

fn mesh_key<'py>(c: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
    Ok(PyTuple::new_bound(c.py(), [c.getattr("version")?, c.getattr("n_vox")?]))
}

/// 作ってあるメッシュ（形が変わっていなければ）。無ければ None
#[pyfunction]
pub fn shape_cached_mesh<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    if let Some(m) = opt(c, "_mesh_cache")? {
        if m.get_item(0)?.eq(mesh_key(c)?)? {
            return m.get_item(1);
        }
    }
    Ok(py.None().into_bound(py))
}
#[pyfunction]
pub fn shape_mesh_ready(c: &Bound<'_, PyAny>) -> R<bool> {
    match opt(c, "_mesh_cache")? {
        Some(m) => m.get_item(0)?.eq(mesh_key(c)?),
        None => Ok(false),
    }
}
/// メッシュを裏で作り始める（もう作ってある・作っている途中なら何もしない）。できていれば True
#[pyfunction]
pub fn shape_request_mesh(py: Python<'_>, c: &Bound<'_, PyAny>) -> R<bool> {
    let key = mesh_key(c)?;
    if shape_mesh_ready(c)? {
        return Ok(true);
    }
    let jobs = JOBS.get_or_init(py, || PyDict::new_bound(py).unbind()).bind(py);
    let id = c.getattr("id")?;
    if let Some(job) = jobs.get_item(&id)? {
        if job.get_item(0)?.eq(&key)? {
            let fut = job.get_item(1)?;
            if fut.call_method0("done")?.is_truthy()? {
                jobs.del_item(&id)?;
                return match fut.call_method0("result") {
                    Ok(m) => {
                        c.setattr("_mesh_cache", (&key, m))?;
                        Ok(true)
                    }
                    Err(e) if e.is_instance_of::<pyo3::exceptions::PyException>(py) => Ok(false),   // 作っている間に形が変わったなど
                    Err(e) => Err(e),
                };
            }
            return Ok(false);
        }
    }
    let pool = match POOL.get(py) {
        Some(p) => p.clone_ref(py),
        None => {
            let kw = PyDict::new_bound(py);
            kw.set_item("max_workers", 2)?;
            kw.set_item("thread_name_prefix", "mesh")?;
            let p = imp(py, "concurrent.futures")?.getattr("ThreadPoolExecutor")?.call((), Some(&kw))?.unbind();
            let _ = POOL.set(py, p.clone_ref(py));
            p
        }
    };
    let f = imp(py, "main.shape")?.getattr("build_mesh")?;
    let fut = pool.bind(py).call_method1("submit", (f, c))?;
    jobs.set_item(&id, (key, fut))?;
    Ok(false)
}
/// 描画用のメッシュ：作ってあればそれを、無ければ今作る
#[pyfunction]
pub fn shape_mesh_for<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    if shape_mesh_ready(c)? {
        return shape_cached_mesh(py, c);
    }
    let m = shape_build_mesh(py, c, None)?;
    c.setattr("_mesh_cache", (mesh_key(c)?, &m))?;
    Ok(m)
}

/// 点 p から形（表面）への最短：(距離, 最近点, 近いボクセル番号) または None。距離は「芯からの距離 − r」
#[pyfunction]
pub fn shape_closest_on_shape<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, p: &Bound<'py, PyAny>, max_dist: f64) -> R<Bound<'py, PyAny>> {
    if c.getattr("n_vox")?.extract::<usize>()? == 0 {
        return Ok(py.None().into_bound(py));
    }
    let cen = if c.hasattr("centers_cached")? { c.call_method0("centers_cached")? } else { c.call_method0("voxel_centers")? };
    let topo = c.getattr("topo")?;
    let np = super::construct::np(py)?;
    let segs = np.call_method1("asarray", (topo.get_item("segs")?, "int64"))?.call_method1("reshape", (-1, 2))?;
    let tris = np.call_method1("asarray", (topo.get_item("tris")?, "int64"))?.call_method1("reshape", (-1, 3))?;
    let res = imp(py, "alpedia_core")?.call_method1("closest_on_shape", (np.call_method1("ascontiguousarray", (cen, "float64"))?, segs, tris,
        v3(p)?.to_vec(), max_dist, c.getattr("size")?.extract::<f64>()?))?;
    if res.is_none() {
        return Ok(res);
    }
    Ok(PyTuple::new_bound(py, [res.get_item(0)?, np.call_method1("asarray", (res.get_item(1)?,))?, res.get_item(2)?.call_method0("__int__")?]).into_any())
}

/// 地面などとの当たり判定に使う球（半径 r）: 各ボクセル中心・結合の中点・三角形の重心。(P,) 個の重み行列（P×ボクセル数）
#[pyfunction]
pub fn shape_collision_proxies(py: Python<'_>, c: &Bound<'_, PyAny>) -> R<Csr> {
    let n: usize = c.getattr("n_vox")?.extract()?;
    let topo = c.getattr("topo")?;
    let s = iv(&topo.get_item("segs")?)?;
    let t = iv(&topo.get_item("tris")?)?;
    let (ns, nt) = (s.len() / 2, t.len() / 3);
    let mut rows = vec![];
    let mut cols = vec![];
    let mut vals = vec![];
    for i in 0..n {
        rows.push(i as i64);
        cols.push(i as i64);
        vals.push(1.0);
    }
    for q in 0..ns {
        for k in 0..2 {
            rows.push((n + q) as i64);
            cols.push(s[2 * q + k]);
            vals.push(0.5);
        }
    }
    for q in 0..nt {
        for k in 0..3 {
            rows.push((n + ns + q) as i64);
            cols.push(t[3 * q + k]);
            vals.push(1.0 / 3.0);
        }
    }
    Csr::build(py, rows, cols, vals, (n + ns + nt, n))
}
