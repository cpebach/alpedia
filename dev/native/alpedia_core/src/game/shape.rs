//! 構築物の「形」（main/shape.py）：ボクセルを球として扱い、結合したボクセルの間を埋める。
//! 形 = 芯（ボクセル中心の点・結合の線・2×2×2 で結合しているボクセル群の凸包の三角形）を半径 r でなぞった和集合。
//! 描画（マーチングキューブでメッシュ化してボクセルの8頂点で変形）と当たり判定の両方で使う。

use crate::game::PyDictOps;
use crate::game::construct::{con, Construct};
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
        let a = crate::game::construct::as_f64(&x)?;
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
    for (m, d) in Construct::g_mats(con(&c)?)?.iter()?.zip(Construct::g_devs(con(&c)?)?.iter()?) {
        let (m, d) = (m?, d?);
        let key = if d.is_truthy()? { PyTuple::new_bound(py, ["dev".into_py(py), d.get_item("def")?.unbind()]) }
                  else { PyTuple::new_bound(py, ["mat".into_py(py), m.unbind()]) };
        let n = names.len();
        out.push(names.dsetdefault(key, n)?.extract::<i64>()?);
    }
    Ok(out)
}
#[pyfunction]
pub fn shape_block_kinds<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let v = block_kinds_v(c)?;
    let n = v.len();
    iarr(py, v, &[n])
}

/// ボクセルごとの「開いている扉」（形・当たり判定・気密から外す。A-14）
pub fn open_doors_v(c: &Bound<'_, PyAny>) -> R<Vec<bool>> {
    let reg = Construct::g_reg(con(&c)?)?;
    let mut out = vec![];
    for d in Construct::g_devs(con(&c)?)?.iter()? {
        let d = d?;
        let mut open = false;
        if d.is_truthy()? {
            let dd = crate::game::registry::dev_dict(&reg, &d.get_item("def")?)?;
            if dd.is_truthy()? && dd.dget1("type")?.eq("door")? {
                open = d.get_item("state")?.dget2("open", 0.0)?.extract::<f64>().unwrap_or(0.0) > 0.5;
            }
        }
        out.push(open);
    }
    Ok(out)
}

/// 形を作る図形の一覧: dict(points, segs, tris)（ボクセル番号の配列）
#[pyfunction]
pub fn shape_topology<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let idx = Construct::g_index(con(&c)?)?;
    let pairs = |keys: Bound<'py, PyAny>| -> R<Vec<[usize; 2]>> {
        let mut out = vec![];
        for k in keys.iter()? {
            let k = k?;
            let a = idx.dget1(k.get_item(0)?)?;
            let b = idx.dget1(k.get_item(1)?)?;
            if !a.is_none() && !b.is_none() {
                out.push([a.extract()?, b.extract()?]);
            }
        }
        Ok(out)
    };
    let cells: Vec<[i64; 3]> = Construct::g_cells(con(&c)?)?.iter()?.map(|x| cell_of(&x?)).collect::<R<_>>()?;
    let l = pairs(Construct::g_links(con(&c)?)?)?;
    let e = match opt(c, "extras")? { Some(x) => pairs(x)?, None => vec![] };
    let k = block_kinds_v(c)?;
    let h = hull();
    let (mut p, mut s, mut t) = py.allow_threads(|| crate::topo::topology(&cells, &l, &e, &k, &h));
    // 開いている扉は形に入れない（見えない・通れる。A-14）
    let open = open_doors_v(c)?;
    if open.iter().any(|x| *x) {
        let o = |i: i64| open[i as usize];
        p.retain(|&i| !o(i));
        s.retain(|x| !o(x[0]) && !o(x[1]));
        t.retain(|x| !o(x[0]) && !o(x[1]) && !o(x[2]));
    }
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
    let reg = Construct::g_reg(con(&c)?)?;
    let mut cache: HashMap<String, bool> = HashMap::new();
    let mut out = vec![];
    for m in Construct::g_mats(con(&c)?)?.iter()? {
        let m = m?;
        let key = m.repr()?.to_string();
        let v = match cache.get(&key) {
            Some(v) => *v,
            None => {
                let d = crate::game::registry::mat_dict(&reg, &m)?;
                let d = if d.is_truthy()? { d } else { PyDict::new_bound(py).into_any() };
                let v = d.dget2("facet", false)?.is_truthy()?;
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
    let reg = Construct::g_reg(con(&c)?)?;
    let mut cache: HashMap<String, bool> = HashMap::new();
    let mut out = vec![];
    for (m, d) in Construct::g_mats(con(&c)?)?.iter()?.zip(Construct::g_devs(con(&c)?)?.iter()?) {
        let (m, d) = (m?, d?);
        if d.is_truthy()? {
            out.push(false);
            continue;
        }
        let key = m.repr()?.to_string();
        let v = match cache.get(&key) {
            Some(v) => *v,
            None => {
                let md = crate::game::registry::mat_dict(&reg, &m)?;
                let v = if md.is_truthy()? {
                    let o = md.dget2("opacity", 1.0)?;
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
    let cells: Vec<[i64; 3]> = Construct::g_cells(con(&c)?)?.iter()?.map(|x| cell_of(&x?)).collect::<R<_>>()?;
    let al = match allowed { Some(a) if !a.is_none() => Some(bv(a)?), _ => None };
    let o = owner_of(&cells, &v3s(centers)?, &v3s(p)?, al.as_deref());
    let n = o.len();
    iarr(py, o.into_iter().map(|x| x as i64).collect(), &[n])
}

/// 平らなメッシュ（A-15・A-16）：ブロックの外に出ている面だけを、同じ見た目・向き・内外・動く部分（ローター）ごとに大きな四角にまとめる。
/// 遠くの構築物用（近くはなめらかなメッシュ。描画が距離で切り替える）。頂点はそれぞれ 1 つのボクセルの角に付ける。
/// ガラスと透けないブロックの間は、透けないブロックの面だけ作る（ガラス越しに見える）。ガラスどうし・ガラスの裏には作らない
fn flat_mesh<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let cells: Vec<[i64; 3]> = Construct::g_cells(con(&c)?)?.iter()?.map(|x| cell_of(&x?)).collect::<R<_>>()?;
    let n = cells.len();
    if n == 0 {
        return Ok(py.None().into_bound(py));
    }
    let kind = block_kinds_v(c)?;
    let clear = clear_v(c)?;
    let open = open_doors_v(c)?;
    // ローターで別に動く部分：部分ごとにまとめ、部分の境には面を作る
    let has_rotors = match opt(c, "rotors")? { Some(x) => x.is_truthy()?, None => false };
    let part: Vec<usize> = if has_rotors && c.hasattr("_art_groups")? {
        iv(&Construct::_art_groups(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?)?.into_iter().map(|x| x as usize).collect()
    } else {
        vec![0; n]
    };
    let (pall, fall, oall, inner) = py.allow_threads(|| {
        let index: HashMap<[i64; 3], usize> = cells.iter().enumerate().filter(|(i, _)| !open[*i]).map(|(i, q)| (*q, i)).collect();
        let closed: Vec<[i64; 3]> = cells.iter().zip(&open).filter(|(_, o)| !**o).map(|(q, _)| *q).collect();
        let cav: std::collections::HashSet<[i64; 3]> = crate::game::airtight::find_cavities(&closed).into_iter().flatten().collect();
        let mut lo = [i64::MAX; 3];
        let mut hi = [i64::MIN; 3];
        for q in &cells {
            for a in 0..3 {
                lo[a] = lo[a].min(q[a]);
                hi[a] = hi[a].max(q[a]);
            }
        }
        let mut pall: Vec<V3> = vec![];
        let mut fall: Vec<[i64; 3]> = vec![];
        let mut oall: Vec<usize> = vec![];
        let mut inner: Vec<bool> = vec![];
        for ax in 0..3usize {
            let (u, v) = ((ax + 1) % 3, (ax + 2) % 3);
            let (nu, nv) = ((hi[u] - lo[u] + 1) as usize, (hi[v] - lo[v] + 1) as usize);
            for sgn in [1i64, -1] {
                for sl in lo[ax]..=hi[ax] {
                    // この面の層のマス：(見た目の番号 + 1, 内側か, ボクセル)。0 は面なし
                    let mut mask: Vec<(i64, bool, usize)> = vec![(0, false, 0); nu * nv];
                    let mut any = false;
                    for iu in 0..nu {
                        for iv in 0..nv {
                            let mut q = [0i64; 3];
                            q[ax] = sl;
                            q[u] = lo[u] + iu as i64;
                            q[v] = lo[v] + iv as i64;
                            let Some(&i) = index.get(&q) else { continue };
                            let mut nq = q;
                            nq[ax] += sgn;
                            let show = match index.get(&nq) {
                                None => true,
                                Some(&j) => part[i] != part[j] || (!clear[i] && clear[j]),
                            };
                            if show {
                                let inn = cav.contains(&nq);
                                // 見た目と動く部分の組を 1 つの番号に
                                mask[iu * nv + iv] = (((kind[i] + 1) << 16) | part[i] as i64, inn, i);
                                any = true;
                            }
                        }
                    }
                    if !any {
                        continue;
                    }
                    // まとめる（横に伸ばしてから縦に伸ばす）
                    let same = |a: &(i64, bool, usize), b: &(i64, bool, usize)| a.0 == b.0 && a.1 == b.1;
                    for iu in 0..nu {
                        let mut iv = 0;
                        while iv < nv {
                            let m0 = mask[iu * nv + iv];
                            if m0.0 == 0 {
                                iv += 1;
                                continue;
                            }
                            let mut w = 1;
                            while iv + w < nv && same(&mask[iu * nv + iv + w], &m0) {
                                w += 1;
                            }
                            let mut h = 1;
                            'grow: while iu + h < nu {
                                for k in 0..w {
                                    if !same(&mask[(iu + h) * nv + iv + k], &m0) {
                                        break 'grow;
                                    }
                                }
                                h += 1;
                            }
                            for a in 0..h {
                                for k in 0..w {
                                    mask[(iu + a) * nv + iv + k].0 = 0;
                                }
                            }
                            // 四角の 4 つの角（面の位置・u・v）と、その角に触れるボクセル
                            let plane = (sl + if sgn > 0 { 1 } else { 0 }) as f64;
                            let corners = [(iu, iv), (iu + h, iv), (iu + h, iv + w), (iu, iv + w)];
                            let base = pall.len() as i64;
                            for &(cu, cv) in &corners {
                                let mut p = [0.0; 3];
                                p[ax] = plane;
                                p[u] = (lo[u] + cu as i64) as f64;
                                p[v] = (lo[v] + cv as i64) as f64;
                                // 角に触れるボクセル（四角の中の、その角のマス）
                                let ou = if cu > iu { cu - 1 } else { cu };
                                let ov = if cv > iv { cv - 1 } else { cv };
                                let mut q = [0i64; 3];
                                q[ax] = sl;
                                q[u] = lo[u] + ou as i64;
                                q[v] = lo[v] + ov as i64;
                                oall.push(*index.get(&q).unwrap_or(&m0.2));
                                pall.push(p);
                            }
                            // 外向き（+ax なら u→v の順で反時計回り）
                            let (t1, t2) = if sgn > 0 { ([0, 1, 2], [0, 2, 3]) } else { ([0, 2, 1], [0, 3, 2]) };
                            fall.push([base + t1[0], base + t1[1], base + t1[2]]);
                            fall.push([base + t2[0], base + t2[1], base + t2[2]]);
                            inner.push(m0.1);
                            inner.push(m0.1);
                            iv += w;
                        }
                    }
                }
            }
        }
        (pall, fall, oall, inner)
    });
    if pall.is_empty() {
        return Ok(py.None().into_bound(py));
    }
    let np_ = pall.len();
    let mut w = vec![1.0f64; np_ * 8];
    for (q, p) in pall.iter().enumerate() {
        let o = cells[oall[q]];
        let uu = [p[0] - o[0] as f64, p[1] - o[1] as f64, p[2] - o[2] as f64];
        for k8 in 0..8 {
            let bits = super::construct::CORNER_BITS[k8];
            for a in 0..3 {
                w[q * 8 + k8] *= if bits[a] > 0.0 { uu[a] } else { 1.0 - uu[a] };
            }
        }
    }
    let d = PyDict::new_bound(py);
    let nf = fall.len();
    d.set_item("inner", PyArray1::from_vec_bound(py, inner))?;
    d.set_item("rest", arr(py, pall.iter().flatten().copied().collect(), &[np_, 3])?)?;
    d.set_item("faces", iarr(py, fall.into_iter().flatten().collect(), &[nf, 3])?)?;
    d.set_item("owner", iarr(py, oall.iter().map(|x| *x as i64).collect(), &[np_])?)?;
    d.set_item("w", arr(py, w, &[np_, 8])?)?;
    d.set_item("vidx", iarr(py, oall.iter().flat_map(|o| (0..8).map(move |k| (8 * o + k) as i64)).collect(), &[np_, 8])?)?;
    d.set_item("version", Construct::g_version(con(&c)?)?)?;
    d.set_item("flat", true)?;
    Ok(d.into_any())
}

/// 形の表面メッシュを作る（格子座標＝セル単位）。ブロックが無ければ None
#[pyfunction]
#[pyo3(signature = (c, k=None, smooth=true))]
pub fn shape_build_mesh<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, k: Option<i64>, smooth: bool) -> R<Bound<'py, PyAny>> {
    let none = py.None().into_bound(py);
    // 遠くの構築物：面をまとめた平らなメッシュ（A-15・A-16）
    if !smooth {
        return flat_mesh(py, c);
    }
    let n: usize = Construct::n(con(&c)?)?;
    if n == 0 {
        return Ok(none);
    }
    let topo = Construct::g_topo(con(&c)?)?;
    let points: Vec<usize> = iv(&topo.get_item("points")?)?.into_iter().map(|x| x as usize).collect();
    let segs: Vec<[usize; 2]> = iv(&topo.get_item("segs")?)?.chunks_exact(2).map(|x| [x[0] as usize, x[1] as usize]).collect();
    let tris: Vec<[usize; 3]> = iv(&topo.get_item("tris")?)?.chunks_exact(3).map(|x| [x[0] as usize, x[1] as usize, x[2] as usize]).collect();
    // 芯（点・線・三角形）を、面・辺・角を落とした多面体で太らせた形の和を、平らな多角形のまま作る（ローポリ。A-21）。
    // k（以前の格子の細かさ）は使わない
    let _ = k;
    let cells: Vec<[i64; 3]> = Construct::g_cells(con(&c)?)?.iter()?.map(|x| cell_of(&x?)).collect::<R<_>>()?;
    let centers: Vec<V3> = cells.iter().map(|q| [q[0] as f64 + 0.5, q[1] as f64 + 0.5, q[2] as f64 + 0.5]).collect();
    let has_rotors = match opt(c, "rotors")? { Some(x) => x.is_truthy()?, None => false };
    let rotor_gid: Option<Vec<usize>> = if has_rotors && c.hasattr("_art_groups")? {
        Some(iv(&Construct::_art_groups(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?)?.into_iter().map(|x| x as usize).collect())
    } else { None };
    // 透ける素材（ガラス）と透けない素材は 1 つの形にする（窓と壁が同じ面でつながる。A-17）。
    // ガラスと接した所の透けないブロックの面は、形の中になってできないので、あとで平らな四角で足す（ガラス越しに見える）。
    // 別の形に分けるのは、ローター（アクチュエーター）で別に動く部分だけ
    let clear = clear_v(c)?;
    let part = |i: usize| rotor_gid.as_ref().map(|g| g[i]).unwrap_or(0);
    // (芯の頂点, 部分, 持ち主のブロック)。
    // 2×2×2 が全部埋まっている所は、中身の詰まった立方体 1 つの芯（表面の三角形だけだと中に小さな空洞ができ、見えない面が増える）。
    // 立方体や三角形に含まれる線・点は省く（形は同じ）
    let open = open_doors_v(c)?;
    let index: HashMap<[i64; 3], usize> = cells.iter().enumerate().filter(|(i, _)| !open[*i]).map(|(i, q)| (*q, i)).collect();
    let mut cubes: std::collections::HashSet<[i64; 3]> = std::collections::HashSet::new();
    let mut prims: Vec<(Vec<V3>, usize, usize)> = vec![];
    for (i, q) in cells.iter().enumerate() {
        if open[i] {
            continue;
        }
        let mut ids = [0usize; 8];
        let mut full = true;
        for k in 0..8 {
            let o = [q[0] + (k & 1) as i64, q[1] + ((k >> 1) & 1) as i64, q[2] + ((k >> 2) & 1) as i64];
            match index.get(&o) {
                Some(&j) if part(j) == part(i) => ids[k] = j,
                _ => {
                    full = false;
                    break;
                }
            }
        }
        if full {
            cubes.insert(*q);
        }
    }
    // 埋まった立方体は、となり合うものを大きな直方体にまとめる（和の形は同じ。芯が少ないほど速い）
    {
        let mut keys: Vec<[i64; 3]> = cubes.iter().copied().collect();
        keys.sort_unstable_by_key(|k| (k[2], k[1], k[0]));
        let pc = |k: &[i64; 3]| part(index[k]);
        let mut seen: std::collections::HashSet<[i64; 3]> = std::collections::HashSet::new();
        let free = |k: [i64; 3], p: usize, seen: &std::collections::HashSet<[i64; 3]>| cubes.contains(&k) && !seen.contains(&k) && pc(&k) == p;
        for k0 in keys {
            if seen.contains(&k0) {
                continue;
            }
            let p = pc(&k0);
            let mut w = 1;
            while free([k0[0] + w, k0[1], k0[2]], p, &seen) {
                w += 1;
            }
            let mut h = 1;
            while (0..w).all(|x| free([k0[0] + x, k0[1] + h, k0[2]], p, &seen)) {
                h += 1;
            }
            let mut d = 1;
            while (0..w).all(|x| (0..h).all(|y| free([k0[0] + x, k0[1] + y, k0[2] + d], p, &seen))) {
                d += 1;
            }
            for x in 0..w {
                for y in 0..h {
                    for z in 0..d {
                        seen.insert([k0[0] + x, k0[1] + y, k0[2] + z]);
                    }
                }
            }
            let mut kv = vec![];
            for z in [0, d] {
                for y in [0, h] {
                    for x in [0, w] {
                        kv.push([(k0[0] + x) as f64 + 0.5, (k0[1] + y) as f64 + 0.5, (k0[2] + z) as f64 + 0.5]);
                    }
                }
            }
            prims.push((kv, p, index[&k0]));
        }
    }
    // ブロックの集まりが、どれかの埋まった立方体に入っているか
    let in_cube = |ids: &[usize]| -> bool {
        let mut lo = [i64::MAX; 3];
        let mut hi = [i64::MIN; 3];
        for &j in ids {
            for a in 0..3 {
                lo[a] = lo[a].min(cells[j][a]);
                hi[a] = hi[a].max(cells[j][a]);
            }
        }
        let rng = |a: usize| (hi[a] - 1).max(lo[a] - 1)..=lo[a];
        for x in rng(0) {
            for y in rng(1) {
                for z in rng(2) {
                    if hi[0] <= x + 1 && hi[1] <= y + 1 && hi[2] <= z + 1 && cubes.contains(&[x, y, z]) {
                        return true;
                    }
                }
            }
        }
        false
    };
    let mut used = vec![false; cells.len()];
    let mut tri_edges: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    // 軸に平らな 1×1 の正方形（4 つのブロック）を埋める三角形は、正方形にまとめ、さらに同じ平面でとなり合う正方形を長方形にまとめる
    // ((部分, 軸, 軸の座標), (u, v) → 正方形の 4 つのブロックのどれを三角形が使ったか)
    let mut squares: HashMap<(usize, usize, i64), HashMap<(i64, i64), (u8, Vec<[usize; 3]>)>> = HashMap::new();
    for t in &tris {
        if part(t[0]) == part(t[1]) && part(t[0]) == part(t[2]) {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                tri_edges.insert((a.min(b), a.max(b)));
            }
            for &j in t {
                used[j] = true;
            }
            if in_cube(t) {
                continue;
            }
            let q3 = [cells[t[0]], cells[t[1]], cells[t[2]]];
            let flat_ax = (0..3).find(|&a| q3[0][a] == q3[1][a] && q3[0][a] == q3[2][a]);
            let mut done = false;
            if let Some(ax) = flat_ax {
                let (u, v) = ((ax + 1) % 3, (ax + 2) % 3);
                let u0 = q3.iter().map(|q| q[u]).min().unwrap();
                let v0 = q3.iter().map(|q| q[v]).min().unwrap();
                if q3.iter().all(|q| q[u] - u0 <= 1 && q[v] - v0 <= 1) {
                    let e = squares.entry((part(t[0]), ax, q3[0][ax])).or_default().entry((u0, v0)).or_insert((0, vec![]));
                    for q in &q3 {
                        e.0 |= 1 << ((q[u] - u0) + 2 * (q[v] - v0));
                    }
                    e.1.push(*t);
                    done = true;
                }
            }
            if !done {
                prims.push((vec![centers[t[0]], centers[t[1]], centers[t[2]]], part(t[0]), t[0]));
            }
        }
    }
    // 4 つとも使われた正方形は長方形にまとめる。3 つだけの正方形は三角形のまま
    let mut sq_keys: Vec<(usize, usize, i64)> = squares.keys().copied().collect();
    sq_keys.sort_unstable();
    for key in sq_keys {
        let (p, ax, coord) = key;
        let (u, v) = ((ax + 1) % 3, (ax + 2) % 3);
        let m = &squares[&key];
        let cell_at = |a: i64, b: i64| -> [i64; 3] {
            let mut q = [0i64; 3];
            q[ax] = coord;
            q[u] = a;
            q[v] = b;
            q
        };
        let full: std::collections::HashSet<(i64, i64)> = m.iter().filter(|(_, b)| b.0 == 15).map(|(k, _)| *k).collect();
        let mut ks: Vec<(i64, i64)> = full.iter().copied().collect();
        ks.sort_unstable_by_key(|k| (k.1, k.0));
        let mut seen: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
        for k0 in ks {
            if seen.contains(&k0) {
                continue;
            }
            let mut w = 1;
            while full.contains(&(k0.0 + w, k0.1)) && !seen.contains(&(k0.0 + w, k0.1)) {
                w += 1;
            }
            let mut h = 1;
            while (0..w).all(|x| full.contains(&(k0.0 + x, k0.1 + h)) && !seen.contains(&(k0.0 + x, k0.1 + h))) {
                h += 1;
            }
            for x in 0..w {
                for y in 0..h {
                    seen.insert((k0.0 + x, k0.1 + y));
                }
            }
            let kv: Vec<V3> = [(0, 0), (w, 0), (w, h), (0, h)].iter().map(|(a, b)| {
                let q = cell_at(k0.0 + a, k0.1 + b);
                [q[0] as f64 + 0.5, q[1] as f64 + 0.5, q[2] as f64 + 0.5]
            }).collect();
            prims.push((kv, p, index[&cell_at(k0.0, k0.1)]));
        }
        // 3 つだけの正方形：その三角形をそのまま
        let mut part_sq: Vec<&(u8, Vec<[usize; 3]>)> = m.values().filter(|x| x.0 != 15).collect();
        part_sq.sort_by_key(|x| x.1[0]);
        for (_, ts) in part_sq {
            for t in ts {
                prims.push((vec![centers[t[0]], centers[t[1]], centers[t[2]]], p, t[0]));
            }
        }
    }
    for s in &segs {
        if part(s[0]) == part(s[1]) {
            used[s[0]] = true;
            used[s[1]] = true;
            if !tri_edges.contains(&(s[0].min(s[1]), s[0].max(s[1]))) && !in_cube(s) {
                prims.push((vec![centers[s[0]], centers[s[1]]], part(s[0]), s[0]));
            }
        }
    }
    for &p in &points {
        if !used[p] && !in_cube(&[p]) {
            prims.push((vec![centers[p]], part(p), p));
        }
    }
    if let Some(g) = &rotor_gid {
        // 回転ジョイント（アクチュエーター）や部分をまたぐ結合は、両側から真ん中まで伸ばして、つながった形を保つ
        let mut rotor_pairs: Vec<(usize, usize)> = vec![];
        let rotors = match opt(c, "rotors")? { Some(x) if !x.is_none() => x.iter()?.collect::<R<Vec<_>>>()?, _ => vec![] };
        for r_ in rotors {
            let (a, b): (usize, usize) = (r_.get_item("a")?.extract()?, r_.get_item("b")?.extract()?);
            let key = (a.min(b), a.max(b));
            if !rotor_pairs.contains(&key) {
                rotor_pairs.push(key);
            }
        }
        for s in &segs {
            if g[s[0]] != g[s[1]] {
                let key = (s[0].min(s[1]), s[0].max(s[1]));
                if !rotor_pairs.contains(&key) {
                    rotor_pairs.push(key);
                }
            }
        }
        for &(a, b) in &rotor_pairs {
            for (x, y) in [(a, b), (b, a)] {
                if g[x] != g[y] {
                    let (cx, cy) = (centers[x], centers[y]);
                    prims.push((vec![cx, [(cx[0] + cy[0]) * 0.5, (cx[1] + cy[1]) * 0.5, (cx[2] + cy[2]) * 0.5]], g[x], x));
                }
            }
        }
    }
    let geo: Vec<(Vec<V3>, usize)> = prims.iter().map(|(v, g, _)| (v.clone(), *g)).collect();
    // 軸の面はブロックの境より少し内側（0.485。となりの物やガラスの面と重ならないように）
    let out = py.allow_threads(|| crate::polyu::union_mesh(&geo, 0.485));
    let mut pall: Vec<V3> = out.verts;
    let mut fall: Vec<[i64; 3]> = out.faces;
    let mut oall: Vec<usize> = out.vert_prim.iter().map(|&i| prims[i].2).collect();
    if pall.is_empty() || fall.is_empty() {
        return Ok(none);
    }
    // 三角形の持ち主は、重心のあるブロック（等値面の三角形は格子の 1 マスの中にあり、ブロックの境は格子の線なので、
    // 素材の境がまっすぐになる）。頂点はブロックごとに分ける（となりのブロックの色がにじまないように。A-18）
    {
        let idx: HashMap<[i64; 3], usize> = cells.iter().enumerate().map(|(i, q)| (*q, i)).collect();
        let mut np: Vec<V3> = Vec::with_capacity(pall.len());
        let mut no: Vec<usize> = Vec::with_capacity(pall.len());
        let mut seen: HashMap<(i64, usize), i64> = HashMap::with_capacity(pall.len());
        for f in fall.iter_mut() {
            let (a, b, c3) = (pall[f[0] as usize], pall[f[1] as usize], pall[f[2] as usize]);
            let g = [((a[0] + b[0] + c3[0]) / 3.0).floor() as i64, ((a[1] + b[1] + c3[1]) / 3.0).floor() as i64,
                     ((a[2] + b[2] + c3[2]) / 3.0).floor() as i64];
            let same_part = |o: usize| rotor_gid.as_ref().map(|r| r[o] == r[oall[f[0] as usize]]).unwrap_or(true);
            let own = match idx.get(&g) {
                // ローターで別に動く部分の三角形は、その部分のブロックだけ（違う部分なら頂点の持ち主のまま）
                Some(&o) if same_part(o) => o,
                Some(_) => oall[f[0] as usize],
                None => {
                    // 斜面を埋めた三角形（空いたマスの中）：重心にいちばん近いまわりのブロック（境がギザギザにならない。A-19）
                    let cc = [(a[0] + b[0] + c3[0]) / 3.0, (a[1] + b[1] + c3[1]) / 3.0, (a[2] + b[2] + c3[2]) / 3.0];
                    let mut best = (f64::INFINITY, oall[f[0] as usize]);
                    for dx in -1..=1 {
                        for dy in -1..=1 {
                            for dz in -1..=1 {
                                if let Some(&o) = idx.get(&[g[0] + dx, g[1] + dy, g[2] + dz]) {
                                    let q = cells[o];
                                    let d2 = (q[0] as f64 + 0.5 - cc[0]).powi(2) + (q[1] as f64 + 0.5 - cc[1]).powi(2)
                                        + (q[2] as f64 + 0.5 - cc[2]).powi(2);
                                    if d2 < best.0 && same_part(o) {
                                        best = (d2, o);
                                    }
                                }
                            }
                        }
                    }
                    best.1
                }
            };
            for k in 0..3 {
                let v = f[k];
                let key = (v, own);
                let nv = match seen.get(&key) {
                    Some(&x) => x,
                    None => {
                        let x = np.len() as i64;
                        np.push(pall[v as usize]);
                        no.push(own);
                        seen.insert(key, x);
                        x
                    }
                };
                f[k] = nv;
            }
        }
        pall = np;
        oall = no;
    }
    // ガラスと接した透けないブロックの面（形の中なのでできていない）：ブロックの境の平らな四角（ガラス越しに見える。A-17）
    if clear.iter().any(|x| *x) {
        let open = open_doors_v(c)?;
        let index: HashMap<[i64; 3], usize> = cells.iter().enumerate().filter(|(i, _)| !open[*i]).map(|(i, q)| (*q, i)).collect();
        let part = |i: usize| rotor_gid.as_ref().map(|g| g[i]).unwrap_or(0);
        for (i, q) in cells.iter().enumerate() {
            if clear[i] || open[i] {
                continue;
            }
            for ax in 0..3usize {
                for sgn in [1i64, -1] {
                    let mut nq = *q;
                    nq[ax] += sgn;
                    let Some(&j) = index.get(&nq) else { continue };
                    if !clear[j] || part(i) != part(j) {
                        continue;
                    }
                    let (u, v) = ((ax + 1) % 3, (ax + 2) % 3);
                    let plane = (q[ax] + if sgn > 0 { 1 } else { 0 }) as f64;
                    let base = pall.len() as i64;
                    // 少し内側に縮める（なめらかな表面はブロックの境より少し内側にあるので、はみ出さないように）
                    for (cu, cv) in [(0.03, 0.03), (0.97, 0.03), (0.97, 0.97), (0.03, 0.97)] {
                        let mut p = [0.0; 3];
                        p[ax] = plane;
                        p[u] = q[u] as f64 + cu;
                        p[v] = q[v] as f64 + cv;
                        pall.push(p);
                        oall.push(i);
                    }
                    let (t1, t2) = if sgn > 0 { ([0, 1, 2], [0, 2, 3]) } else { ([0, 2, 1], [0, 3, 2]) };
                    fall.push([base + t1[0], base + t1[1], base + t1[2]]);
                    fall.push([base + t2[0], base + t2[1], base + t2[2]]);
                }
            }
        }
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
    // 閉じた空洞（気密の空洞と同じ。開いた扉は空き）の方を向いた面：遠くからは描かない（A-14）
    let open = open_doors_v(c)?;
    let closed: Vec<[i64; 3]> = cells.iter().zip(&open).filter(|(_, o)| !**o).map(|(q, _)| *q).collect();
    let inner: Vec<bool> = py.allow_threads(|| {
        let cav: std::collections::HashSet<[i64; 3]> = crate::game::airtight::find_cavities(&closed).into_iter().flatten().collect();
        if cav.is_empty() {
            return vec![false; fall.len()];
        }
        fall.iter().map(|f| {
            let (a, b, c3) = (pall[f[0] as usize], pall[f[1] as usize], pall[f[2] as usize]);
            let n = super::construct::cross(super::construct::sub(b, a), super::construct::sub(c3, a));
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if l < 1e-12 {
                return false;
            }
            let q = [(a[0] + b[0] + c3[0]) / 3.0 + n[0] / l * 0.35, (a[1] + b[1] + c3[1]) / 3.0 + n[1] / l * 0.35,
                     (a[2] + b[2] + c3[2]) / 3.0 + n[2] / l * 0.35];
            cav.contains(&[q[0].floor() as i64, q[1].floor() as i64, q[2].floor() as i64])
        }).collect()
    });
    let d = PyDict::new_bound(py);
    let nf = fall.len();
    d.set_item("inner", PyArray1::from_vec_bound(py, inner))?;
    d.set_item("rest", arr(py, pall.iter().flatten().copied().collect(), &[np_, 3])?)?;
    d.set_item("faces", iarr(py, fall.into_iter().flatten().collect(), &[nf, 3])?)?;
    d.set_item("owner", iarr(py, oall.iter().map(|x| *x as i64).collect(), &[np_])?)?;
    d.set_item("w", arr(py, w, &[np_, 8])?)?;
    d.set_item("vidx", iarr(py, oall.iter().flat_map(|o| (0..8).map(move |k| (8 * o + k) as i64)).collect(), &[np_, 8])?)?;
    d.set_item("version", Construct::g_version(con(&c)?)?)?;
    Ok(d.into_any())
}

// ---------------------------------------------------------------- 裏での読み込み（大きい構築物のメッシュは別のスレッドで作る）

static JOBS: GILOnceCell<Py<PyDict>> = GILOnceCell::new();
static POOL: GILOnceCell<PyObject> = GILOnceCell::new();

fn mesh_key<'py>(c: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
    let dv = c.getattr("_door_ver").unwrap_or_else(|_| IntoPy::<PyObject>::into_py(0i64, c.py()).into_bound(c.py()));
    Ok(PyTuple::new_bound(c.py(), [Construct::g_version(con(&c)?)?, Construct::n(con(&c)?)?.into_py(c.py()).into_bound(c.py()), dv]))
}

/// なめらかなメッシュと平らなメッシュの、使い回しの置き場の名前
fn cache_attr(smooth: bool) -> &'static str {
    if smooth { "_mesh_cache" } else { "_mesh_cache_flat" }
}

/// 描き直しの目安：(version, 扉の開け閉めの回数)
pub fn draw_ver<'py>(c: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
    let dv = c.getattr("_door_ver").unwrap_or_else(|_| IntoPy::<PyObject>::into_py(0i64, c.py()).into_bound(c.py()));
    Ok(PyTuple::new_bound(c.py(), [c.getattr("version")?, dv]))
}

/// 作ってあるメッシュ（形が変わっていなければ）。無ければ None
#[pyfunction]
#[pyo3(signature = (c, smooth=true))]
pub fn shape_cached_mesh<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, smooth: bool) -> R<Bound<'py, PyAny>> {
    if let Some(m) = opt(c, cache_attr(smooth))? {
        if m.get_item(0)?.eq(mesh_key(c)?)? {
            return m.get_item(1);
        }
    }
    Ok(py.None().into_bound(py))
}
#[pyfunction]
#[pyo3(signature = (c, smooth=true))]
pub fn shape_mesh_ready(c: &Bound<'_, PyAny>, smooth: bool) -> R<bool> {
    match opt(c, cache_attr(smooth))? {
        Some(m) => m.get_item(0)?.eq(mesh_key(c)?),
        None => Ok(false),
    }
}
/// メッシュを裏で作り始める（もう作ってある・作っている途中なら何もしない）。できていれば True
#[pyfunction]
#[pyo3(signature = (c, smooth=true))]
pub fn shape_request_mesh(py: Python<'_>, c: &Bound<'_, PyAny>, smooth: bool) -> R<bool> {
    let key = mesh_key(c)?;
    if shape_mesh_ready(c, smooth)? {
        return Ok(true);
    }
    let jobs = JOBS.get_or_init(py, || PyDict::new_bound(py).unbind()).bind(py);
    let id = PyTuple::new_bound(py, [Construct::g_id(con(&c)?)?.into_any(), smooth.into_py(py).into_bound(py)]);
    if let Some(job) = jobs.get_item(&id)? {
        if job.get_item(0)?.eq(&key)? {
            let fut = job.get_item(1)?;
            if fut.call_method0("done")?.is_truthy()? {
                jobs.del_item(&id)?;
                return match fut.call_method0("result") {
                    Ok(m) => {
                        c.setattr(cache_attr(smooth), (&key, m))?;
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
    let fut = pool.bind(py).call_method1("submit", (f, c, py.None(), smooth))?;
    jobs.set_item(&id, (key, fut))?;
    Ok(false)
}
/// 描画用のメッシュ：作ってあればそれを、無ければ今作る
#[pyfunction]
#[pyo3(signature = (c, smooth=true))]
pub fn shape_mesh_for<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, smooth: bool) -> R<Bound<'py, PyAny>> {
    if shape_mesh_ready(c, smooth)? {
        return shape_cached_mesh(py, c, smooth);
    }
    let m = shape_build_mesh(py, c, None, smooth)?;
    c.setattr(cache_attr(smooth), (mesh_key(c)?, &m))?;
    Ok(m)
}

/// 点 p から形（表面）への最短：(距離, 最近点, 近いボクセル番号) または None。距離は「芯からの距離 − r」
#[pyfunction]
pub fn shape_closest_on_shape<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, p: &Bound<'py, PyAny>, max_dist: f64) -> R<Bound<'py, PyAny>> {
    if Construct::n(con(&c)?)? == 0 {
        return Ok(py.None().into_bound(py));
    }
    let cen = if c.hasattr("centers_cached")? { Construct::centers_cached(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))? } else { Construct::voxel_centers(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))? };
    let topo = Construct::g_topo(con(&c)?)?;
    let np = super::construct::np(py)?;
    let segs = crate::game::construct::as_i64(&topo.get_item("segs")?)?.call_method1("reshape", (-1, 2))?;
    let tris = crate::game::construct::as_i64(&topo.get_item("tris")?)?.call_method1("reshape", (-1, 3))?;
    let res = imp(py, "alpedia_core")?.call_method1("closest_on_shape", (crate::game::construct::cflt(&cen)?, segs, tris,
        v3(p)?.to_vec(), max_dist, Construct::g_size(con(&c)?)?.extract::<f64>()?))?;
    if res.is_none() {
        return Ok(res);
    }
    Ok(PyTuple::new_bound(py, [res.get_item(0)?, np.call_method1("asarray", (res.get_item(1)?,))?, res.get_item(2)?.call_method0("__int__")?]).into_any())
}

/// 地面などとの当たり判定に使う球（半径 r）: 各ボクセル中心・結合の中点・三角形の重心。(P,) 個の重み行列（P×ボクセル数）
#[pyfunction]
pub fn shape_collision_proxies(py: Python<'_>, c: &Bound<'_, PyAny>) -> R<Csr> {
    let n: usize = Construct::n(con(&c)?)?;
    let topo = Construct::g_topo(con(&c)?)?;
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
