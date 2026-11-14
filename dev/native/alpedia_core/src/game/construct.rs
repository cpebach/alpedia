//! 構築物（main/construct.py の Construct）。各ボクセルは自分専用の8頂点を持つ「ポリゴンのボクセル」。
//! 物理の本体（Body・Proxy・ArtTopo）は lib.rs 側。ここはそのまとめ役（形の作り直し・剛体化・編集・保存）。
//! 決まった属性は Rust の欄、計算中だけの属性（v_stiff・ja・topo など）は __dict__ に置く（MOD・描画からも同じ名前で読める）。

use numpy::ndarray::IxDyn;
use numpy::{PyArray1, PyArrayMethods, PyReadonlyArrayDyn, PyReadwriteArrayDyn, PyUntypedArrayMethods};
use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyString, PyTuple, PyType};

use super::imp;

pub type R<T> = PyResult<T>;
pub type V3 = [f64; 3];

pub const CORNER_BITS: [[f64; 3]; 8] = [
    [0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [1., 1., 0.], [0., 0., 1.], [1., 0., 1.], [0., 1., 1.], [1., 1., 1.]];
pub const DIRS: [[i64; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
pub const CONTACT_SOFT: f64 = 1.0;

pub fn face_quad(d: usize) -> [usize; 4] {
    let (axis, positive) = (d / 2, d % 2 == 0);
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut q = [0usize; 4];
    for (k, (bu, bv)) in [(0, 0), (1, 0), (1, 1), (0, 1)].iter().enumerate() {
        let mut bits = [0usize; 3];
        bits[axis] = if positive { 1 } else { 0 };
        bits[u] = *bu;
        bits[v] = *bv;
        q[k] = bits[0] + 2 * bits[1] + 4 * bits[2];
    }
    if !positive {
        q.reverse();
    }
    q
}

/// リンク a→b（b = a + DIRS[d]）で結合する頂点ペア
pub fn joint_pairs(d: usize) -> [(usize, usize); 4] {
    let q = face_quad(d);
    let ax = d / 2;
    let mut o = [(0, 0); 4];
    for k in 0..4 {
        o[k] = (q[k], q[k] ^ (1 << ax));
    }
    o
}

// ---------------------------------------------------------------- 小さな道具

pub fn np(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "numpy")
}
pub fn core(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "alpedia_core")
}

/// numpy 配列（など）を C の順の f64 の列に
pub fn fv(o: &Bound<'_, PyAny>) -> R<Vec<f64>> {
    if let Ok(r) = o.extract::<PyReadonlyArrayDyn<f64>>() {
        if let Ok(s) = r.as_slice() {
            return Ok(s.to_vec());
        }
        return Ok(r.as_array().iter().copied().collect());
    }
    let a = np(o.py())?.call_method1("asarray", (o, "float64"))?;
    let r: PyReadonlyArrayDyn<f64> = a.extract()?;
    Ok(r.as_array().iter().copied().collect())
}
pub fn iv(o: &Bound<'_, PyAny>) -> R<Vec<i64>> {
    if let Ok(r) = o.extract::<PyReadonlyArrayDyn<i64>>() {
        return Ok(r.as_array().iter().copied().collect());
    }
    let a = np(o.py())?.call_method1("asarray", (o, "int64"))?;
    let r: PyReadonlyArrayDyn<i64> = a.extract()?;
    Ok(r.as_array().iter().copied().collect())
}
pub fn bv(o: &Bound<'_, PyAny>) -> R<Vec<bool>> {
    if let Ok(r) = o.extract::<PyReadonlyArrayDyn<bool>>() {
        return Ok(r.as_array().iter().copied().collect());
    }
    let a = np(o.py())?.call_method1("asarray", (o, "bool"))?;
    let r: PyReadonlyArrayDyn<bool> = a.extract()?;
    Ok(r.as_array().iter().copied().collect())
}
pub fn v3s(o: &Bound<'_, PyAny>) -> R<Vec<V3>> {
    let f = fv(o)?;
    Ok(f.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect())
}
pub fn v3(o: &Bound<'_, PyAny>) -> R<V3> {
    let f = fv(o)?;
    if f.len() < 3 {
        return Err(PyValueError::new_err("3つの数が必要です"));
    }
    Ok([f[0], f[1], f[2]])
}
pub fn m3(o: &Bound<'_, PyAny>) -> R<[[f64; 3]; 3]> {
    let f = fv(o)?;
    if f.len() < 9 {
        return Err(PyValueError::new_err("3x3 の行列が必要です"));
    }
    Ok([[f[0], f[1], f[2]], [f[3], f[4], f[5]], [f[6], f[7], f[8]]])
}
pub fn arr<'py>(py: Python<'py>, v: Vec<f64>, shape: &[usize]) -> R<Bound<'py, PyAny>> {
    Ok(PyArray1::from_vec_bound(py, v).reshape(IxDyn(shape))?.into_any())
}
pub fn iarr<'py>(py: Python<'py>, v: Vec<i64>, shape: &[usize]) -> R<Bound<'py, PyAny>> {
    Ok(PyArray1::from_vec_bound(py, v).reshape(IxDyn(shape))?.into_any())
}
pub fn barr<'py>(py: Python<'py>, v: Vec<bool>) -> Bound<'py, PyAny> {
    PyArray1::from_vec_bound(py, v).into_any()
}
pub fn a3<'py>(py: Python<'py>, v: &[V3]) -> R<Bound<'py, PyAny>> {
    arr(py, v.iter().flatten().copied().collect(), &[v.len(), 3])
}
pub fn a1<'py>(py: Python<'py>, v: V3) -> Bound<'py, PyAny> {
    PyArray1::from_vec_bound(py, v.to_vec()).into_any()
}
pub fn mat<'py>(py: Python<'py>, m: &[[f64; 3]; 3]) -> R<Bound<'py, PyAny>> {
    arr(py, m.iter().flatten().copied().collect(), &[3, 3])
}
pub fn eye() -> [[f64; 3]; 3] {
    [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
}

/// numpy の 1次元の合計と同じ順（ペアごとの和）
pub fn pw_sum(x: &[f64]) -> f64 {
    let n = x.len();
    if n < 8 {
        let mut r = 0.0;
        for v in x {
            r += v;
        }
        return r;
    }
    if n <= 128 {
        let mut r = [0.0; 8];
        r.copy_from_slice(&x[..8]);
        let mut i = 8;
        while i < n - (n % 8) {
            for j in 0..8 {
                r[j] += x[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        for v in &x[i..] {
            res += v;
        }
        return res;
    }
    let mut n2 = n / 2;
    n2 -= n2 % 8;
    pw_sum(&x[..n2]) + pw_sum(&x[n2..])
}
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn scl(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn norm(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
/// a @ M.T（行ベクトルに行列をかける）
pub fn mul_t(a: V3, m: &[[f64; 3]; 3]) -> V3 {
    [dot(a, m[0]), dot(a, m[1]), dot(a, m[2])]
}
/// a @ M
pub fn mul_r(a: V3, m: &[[f64; 3]; 3]) -> V3 {
    [a[0] * m[0][0] + a[1] * m[1][0] + a[2] * m[2][0], a[0] * m[0][1] + a[1] * m[1][1] + a[2] * m[2][1],
     a[0] * m[0][2] + a[1] * m[1][2] + a[2] * m[2][2]]
}
pub fn matmul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut o = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    o
}
pub fn solve3(a: &[[f64; 3]; 3], b: V3) -> Option<V3> {
    let m = nalgebra::Matrix3::new(a[0][0], a[0][1], a[0][2], a[1][0], a[1][1], a[1][2], a[2][0], a[2][1], a[2][2]);
    let x = m.lu().solve(&nalgebra::Vector3::new(b[0], b[1], b[2]))?;
    Some([x[0], x[1], x[2]])
}
pub fn inv3(a: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let m = nalgebra::Matrix3::new(a[0][0], a[0][1], a[0][2], a[1][0], a[1][1], a[1][2], a[2][0], a[2][1], a[2][2]);
    let i = m.try_inverse().unwrap_or_else(|| nalgebra::Matrix3::from_element(f64::NAN));
    [[i[(0, 0)], i[(0, 1)], i[(0, 2)]], [i[(1, 0)], i[(1, 1)], i[(1, 2)]], [i[(2, 0)], i[(2, 1)], i[(2, 2)]]]
}

/// 列 [(n,3)] の重み付き平均（numpy の (pos * m[:, None]).sum(0) / m.sum() と同じ順）
pub fn wmean(p: &[V3], m: &[f64]) -> V3 {
    let mut s = [0.0; 3];
    for (q, w) in p.iter().zip(m) {
        for k in 0..3 {
            s[k] += q[k] * w;
        }
    }
    let t = pw_sum(m);
    [s[0] / t, s[1] / t, s[2] / t]
}

pub fn opt<'py>(o: &Bound<'py, PyAny>, k: &str) -> R<Option<Bound<'py, PyAny>>> {
    match o.getattr(k) {
        Ok(v) => Ok(if v.is_none() { None } else { Some(v) }),
        Err(e) if e.is_instance_of::<PyAttributeError>(o.py()) => Ok(None),
        Err(e) => Err(e),
    }
}
pub fn truthy(o: &Bound<'_, PyAny>, k: &str) -> R<bool> {
    Ok(match opt(o, k)? {
        Some(v) => v.is_truthy()?,
        None => false,
    })
}
pub fn dget<'py>(d: &Bound<'py, PyAny>, k: &str, dflt: impl IntoPy<PyObject>) -> R<Bound<'py, PyAny>> {
    d.call_method1("get", (k, dflt))
}
pub fn cell_of(o: &Bound<'_, PyAny>) -> R<[i64; 3]> {
    let t: (i64, i64, i64) = o.extract()?;
    Ok([t.0, t.1, t.2])
}
pub fn tup3<'py>(py: Python<'py>, c: [i64; 3]) -> Bound<'py, PyTuple> {
    PyTuple::new_bound(py, c)
}
pub fn link_key_c<'py>(py: Python<'py>, a: [i64; 3], b: [i64; 3]) -> Bound<'py, PyTuple> {
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    PyTuple::new_bound(py, [tup3(py, a), tup3(py, b)])
}
/// 機器の種類（(reg.device(d["def"]) or {}).get("type")）
pub fn dev_type(reg: &Bound<'_, PyAny>, d: &Bound<'_, PyAny>) -> R<Option<String>> {
    let dd = reg.call_method1("device", (d.get_item("def")?,))?;
    if !dd.is_truthy()? {
        return Ok(None);
    }
    let t = dd.call_method1("get", ("type",))?;
    Ok(if t.is_none() { None } else { Some(t.extract()?) })
}
/// numpy 配列をその場で書き換える
pub fn with_rw<F: FnOnce(&mut [f64])>(o: &Bound<'_, PyAny>, f: F) -> R<bool> {
    if let Ok(mut a) = o.extract::<PyReadwriteArrayDyn<f64>>() {
        if let Ok(s) = a.as_slice_mut() {
            f(s);
            return Ok(true);
        }
    }
    Ok(false)
}

#[pyfunction]
pub fn construct_link_key<'py>(py: Python<'py>, a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
    let a = PyTuple::new_bound(py, a.iter()?.collect::<R<Vec<_>>>()?);
    let b = PyTuple::new_bound(py, b.iter()?.collect::<R<Vec<_>>>()?);
    Ok(if a.le(&b)? { PyTuple::new_bound(py, [a, b]) } else { PyTuple::new_bound(py, [b, a]) })
}

#[pyfunction]
pub fn construct_dir_index(delta: &Bound<'_, PyAny>) -> R<i64> {
    let v: Vec<f64> = delta.iter()?.map(|x| x.and_then(|x| x.extract::<f64>())).collect::<R<_>>()?;
    if v.len() != 3 {
        return Ok(-1);
    }
    let t = [v[0] as i64, v[1] as i64, v[2] as i64];
    Ok(DIRS.iter().position(|d| *d == t).map(|x| x as i64).unwrap_or(-1))
}

/// 保存する形の機器の情報（計算中だけの項目を除く）
#[pyfunction]
#[pyo3(signature = (d, reg=None))]
pub fn construct_clean_dev<'py>(py: Python<'py>, d: &Bound<'py, PyAny>, reg: Option<&Bound<'py, PyAny>>) -> R<PyObject> {
    if !d.is_truthy()? {
        return Ok(py.None());
    }
    let st = d.get_item("state")?;
    let dd = match reg {
        Some(r) if !r.is_none() => Some(r.call_method1("device", (d.get_item("def")?,))?),
        _ => None,
    };
    let saved: Bound<'_, PyAny> = match dd {
        Some(dd) if dd.is_truthy()? => {
            let t: String = dd.get_item("type")?.extract()?;
            super::devstate::devstate_to_saved(py, &t, st.downcast::<PyDict>()?)?.into_any()
        }
        _ => {
            let o = PyDict::new_bound(py);
            for (k, v) in st.downcast::<PyDict>()?.iter() {
                if !k.extract::<String>().map(|s| s.starts_with('_')).unwrap_or(false) {
                    o.set_item(k, v)?;
                }
            }
            o.into_any()
        }
    };
    let o = PyDict::new_bound(py);
    o.set_item("def", d.get_item("def")?)?;
    o.set_item("dir", d.get_item("dir")?)?;
    o.set_item("name", d.get_item("name")?)?;
    o.set_item("state", saved)?;
    Ok(o.into_any().unbind())
}

/// Rust に渡す惑星のリスト（Rust の地形が使えない惑星があれば None）
#[pyfunction]
pub fn construct_native_planets<'py>(py: Python<'py>, planets: &Bound<'py, PyAny>) -> R<PyObject> {
    let out = PyList::empty_bound(py);
    for pl in planets.iter()? {
        let pl = pl?;
        let nat = pl.call_method0("_nat")?;
        if nat.is_none() {
            return Ok(py.None());
        }
        let f = |k: &str| -> R<f64> { pl.getattr(k)?.extract::<f64>() };
        // 大気：地表の空気の密度（気圧に比例。101.3 kPa で 1.225 kg/m³）と、密度が 1/e になる高さ（空の色と同じ：半径の 1/4、30 m 以上）
        let kpa: f64 = match pl.getattr("defn")?.call_method1("get", ("atmosphere_pressure", 101.3)) {
            Ok(v) if !v.is_none() => v.extract().unwrap_or(101.3),
            _ => 101.3,
        };
        let rad = f("radius")?;
        let air = 1.225 * (kpa / 101.3).max(0.0);
        out.append((nat, f("bound")?, f("lipschitz")?, f("gm")?, rad, pl.getattr("has_water")?.is_truthy()?, air,
                    (rad * 0.25).max(30.0)))?;
    }
    Ok(out.into_any().unbind())
}

// ---------------------------------------------------------------- Construct

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Construct {
    #[pyo3(get, set)]
    pub reg: PyObject,
    #[pyo3(get, set)]
    pub id: PyObject,
    #[pyo3(get, set)]
    pub name: PyObject,
    #[pyo3(get, set)]
    pub size: f64,
    #[pyo3(get, set)]
    pub cells: PyObject,
    #[pyo3(get, set)]
    pub mats: PyObject,
    #[pyo3(get, set)]
    pub devs: PyObject,
    #[pyo3(get, set)]
    pub links: PyObject,
    #[pyo3(get, set)]
    pub extras: PyObject,
    #[pyo3(get, set)]
    pub pos: PyObject,
    #[pyo3(get, set)]
    pub vel: PyObject,
    #[pyo3(get, set)]
    pub rest: PyObject,
    #[pyo3(get, set)]
    pub act_scale: PyObject,
    #[pyo3(get, set)]
    pub storage: PyObject,
    #[pyo3(get, set)]
    pub cavity_air: PyObject,
    #[pyo3(get, set)]
    pub far_time: f64,
    #[pyo3(get, set)]
    pub version: i64,
    #[pyo3(get, set)]
    pub powered: bool,
    #[pyo3(get, set)]
    pub status: PyObject,
    #[pyo3(get, set)]
    pub index: PyObject,
    #[pyo3(get, set)]
    pub mode: String,
    #[pyo3(get, set, name = "_calm_t")]
    pub calm_t: f64,
    #[pyo3(get, set, name = "_want_soft")]
    pub want_soft: bool,
    #[pyo3(get, set)]
    pub rb: PyObject,
    #[pyo3(get, set)]
    pub rb_epoch: i64,
    #[pyo3(get, set)]
    pub sleep_epoch: i64,
    #[pyo3(get, set, name = "_in_pool")]
    pub in_pool: bool,
    #[pyo3(get, set)]
    pub ext_colliders: PyObject,
    #[pyo3(get, set)]
    pub ext_events: PyObject,
    #[pyo3(get, set)]
    pub anchor: PyObject,
    #[pyo3(get, set)]
    pub asleep: bool,
    #[pyo3(get, set)]
    pub sleep_t: f64,
}

fn zeros<'py>(py: Python<'py>, shape: &[usize]) -> R<Bound<'py, PyAny>> {
    arr(py, vec![0.0; shape.iter().product()], shape)
}

impl Construct {
    pub fn n(slf: &Bound<'_, Self>) -> R<usize> {
        slf.borrow().cells.bind(slf.py()).len()
    }
    pub fn sz(slf: &Bound<'_, Self>) -> f64 {
        slf.borrow().size
    }
    pub fn ver(slf: &Bound<'_, Self>) -> i64 {
        slf.borrow().version
    }
    pub fn g<'py>(slf: &Bound<'py, Self>, k: &str) -> R<Bound<'py, PyAny>> {
        slf.as_any().getattr(k)
    }
    pub fn set(slf: &Bound<'_, Self>, k: &str, v: impl IntoPy<PyObject>) -> R<()> {
        slf.as_any().setattr(k, v.into_py(slf.py()))
    }
    pub fn cells_v(slf: &Bound<'_, Self>) -> R<Vec<[i64; 3]>> {
        let py = slf.py();
        let c = slf.borrow().cells.clone_ref(py);
        c.bind(py).iter()?.map(|x| cell_of(&x?)).collect()
    }
    pub fn mode_is(slf: &Bound<'_, Self>, m: &[&str]) -> bool {
        let b = slf.borrow();
        m.iter().any(|x| b.mode == *x)
    }
    fn vkey<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        Ok(PyTuple::new_bound(slf.py(), [Self::ver(slf).into_py(slf.py()), Self::n(slf)?.into_py(slf.py())]))
    }
    /// getattr(self, k) がキャッシュ (key, 値...) で key が一致するならその値を返す
    fn cached<'py>(slf: &Bound<'py, Self>, k: &str, key: &Bound<'py, PyAny>) -> R<Option<Bound<'py, PyAny>>> {
        if let Some(c) = opt(slf.as_any(), k)? {
            if c.get_item(0)?.eq(key)? {
                return Ok(Some(c));
            }
        }
        Ok(None)
    }
    pub fn pos_v(slf: &Bound<'_, Self>) -> R<Vec<V3>> {
        let py = slf.py();
        let p = slf.borrow().pos.clone_ref(py);
        v3s(p.bind(py))
    }
    pub fn vel_v(slf: &Bound<'_, Self>) -> R<Vec<V3>> {
        let py = slf.py();
        let p = slf.borrow().vel.clone_ref(py);
        v3s(p.bind(py))
    }
    pub fn mass_v(slf: &Bound<'_, Self>) -> R<Vec<f64>> {
        fv(&Self::g(slf, "mass")?)
    }
    fn is_static_b(slf: &Bound<'_, Self>) -> R<bool> {
        Ok(Self::anchored_b(slf)? && !Self::mode_is(slf, &["art"]))
    }
    pub fn anchored_b(slf: &Bound<'_, Self>) -> R<bool> {
        Ok(Self::anchor_index(slf)?.len()? > 0)
    }
    fn still(slf: &Bound<'_, Self>) -> R<bool> {
        Ok(Self::is_static_b(slf)? || slf.borrow().asleep)
    }
    fn centers(slf: &Bound<'_, Self>) -> R<Vec<V3>> {
        let p = Self::pos_v(slf)?;
        Ok(p.chunks_exact(8).map(mean8).collect())
    }
    fn link_list<'py>(slf: &Bound<'py, Self>, with_extras: bool) -> R<Vec<Bound<'py, PyAny>>> {
        let mut out: Vec<Bound<'py, PyAny>> = Self::g(slf, "links")?.iter()?.collect::<R<_>>()?;
        if with_extras {
            for k in Self::g(slf, "extras")?.iter()? {
                out.push(k?);
            }
        }
        Ok(out)
    }
    fn idx_of(index: &Bound<'_, PyAny>, cell: &Bound<'_, PyAny>) -> R<Option<usize>> {
        let v = index.call_method1("get", (cell,))?;
        Ok(if v.is_none() { None } else { Some(v.extract()?) })
    }
    fn rotor_keys<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PySet>> {
        let s = PySet::empty_bound(slf.py())?;
        for r in Self::g(slf, "rotors")?.iter()? {
            s.add(r?.get_item("key")?)?;
        }
        Ok(s)
    }
}

pub fn mean8(p: &[V3]) -> V3 {
    let mut s = [0.0; 3];
    for q in p {
        for k in 0..3 {
            s[k] += q[k];
        }
    }
    let n = p.len() as f64;
    [s[0] / n, s[1] / n, s[2] / n]
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

#[pymethods]
impl Construct {
    #[new]
    #[pyo3(signature = (registry, size=1.0, id_=None, name=None, *_args, **_kw))]
    fn new(py: Python<'_>, registry: PyObject, size: f64, id_: Option<&Bound<'_, PyAny>>, name: Option<PyObject>,
           _args: &Bound<'_, PyTuple>, _kw: Option<&Bound<'_, PyDict>>) -> R<Self> {
        let id = match id_ {
            Some(i) if i.is_truthy()? => i.clone().unbind(),
            _ => imp(py, "os")?.call_method1("urandom", (5,))?.call_method0("hex")?.unbind(),
        };
        let name = name.unwrap_or_else(|| PyString::new_bound(py, "構築物").into_any().unbind());
        let e = || PyDict::new_bound(py).into_any().unbind();
        Ok(Construct {
            reg: registry, id, name, size,
            cells: PyList::empty_bound(py).into_any().unbind(),
            mats: PyList::empty_bound(py).into_any().unbind(),
            devs: PyList::empty_bound(py).into_any().unbind(),
            links: PySet::empty_bound(py)?.into_any().unbind(),
            extras: e(),
            pos: zeros(py, &[0, 3])?.unbind(),
            vel: zeros(py, &[0, 3])?.unbind(),
            rest: zeros(py, &[0, 8, 3])?.unbind(),
            act_scale: zeros(py, &[0, 3])?.unbind(),
            storage: e(), cavity_air: e(),
            far_time: 0.0, version: 0, powered: false,
            status: e(), index: e(),
            mode: "soft".into(), calm_t: 0.0, want_soft: false,
            rb: py.None(), rb_epoch: 0, sleep_epoch: 0, in_pool: false,
            ext_colliders: PyList::empty_bound(py).into_any().unbind(),
            ext_events: PyList::empty_bound(py).into_any().unbind(),
            anchor: py.None(), asleep: false, sleep_t: 0.0,
        })
    }

    // ---- 定数（MOD・ほかのモジュールから Construct.X で読む） ----
    #[classattr]
    const LOD_CELL: i64 = 4;
    #[classattr]
    const FIXED_INV: f64 = 1e-12;
    #[classattr]
    const WATER_DENSITY: f64 = 1000.0;
    #[classattr]
    const RIGID_ACC_IN: f64 = 25.0;
    #[classattr]
    const RIGID_ACC_OUT: f64 = 35.0;
    #[classattr]
    const RIGID_DEFORM_V: f64 = 0.05;
    #[classattr]
    const RIGID_CALM_T: f64 = 0.3;
    #[classattr]
    const IMPACT_SPEED: f64 = 2.0;
    #[classattr]
    const ART_HOLD: f64 = 1.0;
    #[classattr]
    const ART_ITERS: i64 = 6;
    #[classattr]
    const ART_CONTACTS: i64 = 24;
    #[classattr]
    const MOTOR_STEP_X: f64 = 10.0;
    #[classattr]
    const MOTOR_HOLD_DEG: f64 = 20.0;
    #[classattr]
    #[allow(non_snake_case)]
    fn PASSIVE_DEVICES(py: Python<'_>) -> R<PyObject> {
        Ok(PySet::new_bound(py, &["anchor", "pipe", "tank", "valve", "battery", "seat", "generator", "antenna", "computer"])?
            .into_any().unbind())
    }
    #[classattr]
    #[allow(non_snake_case)]
    fn SLOW_DEVICES(py: Python<'_>) -> R<PyObject> {
        Ok(PySet::new_bound(py, &["light"])?.into_any().unbind())
    }

    // ------------------------------------------------------------------ 構築
    /// voxels: [(cell(tuple), material_id, device_dict|None), ...]
    /// defer=True：形の作り直し（rebuild）と回転の速度は呼んだ側がする（読み込みで2回作らないように）
    #[classmethod]
    #[pyo3(signature = (registry, voxels, origin, rotation=None, velocity=None, angular_velocity=None, size=1.0, links=None, defer=false, **kw))]
    #[allow(clippy::too_many_arguments)]
    fn from_voxels<'py>(cls: &Bound<'py, PyType>, registry: &Bound<'py, PyAny>, voxels: &Bound<'py, PyAny>, origin: &Bound<'py, PyAny>,
                        rotation: Option<&Bound<'py, PyAny>>, velocity: Option<&Bound<'py, PyAny>>,
                        angular_velocity: Option<&Bound<'py, PyAny>>, size: f64, links: Option<&Bound<'py, PyAny>>, defer: bool,
                        kw: Option<&Bound<'py, PyDict>>) -> R<Bound<'py, PyAny>> {
        let py = cls.py();
        let k = PyDict::new_bound(py);
        if let Some(kw) = kw {
            k.update(kw.as_mapping())?;
        }
        k.set_item("size", size)?;
        let c = cls.call((registry,), Some(&k))?;
        let cs = c.downcast::<Construct>()?.clone();
        let rot = match rotation {
            Some(r) if !r.is_none() => m3(r)?,
            _ => eye(),
        };
        let org = v3(origin)?;
        let cl = Self::g(&cs, "cells")?;
        let ml = Self::g(&cs, "mats")?;
        let dl = Self::g(&cs, "devs")?;
        let mut cells = vec![];
        for item in voxels.iter()? {
            let item = item?;
            let (cell, m, dev) = (item.get_item(0)?, item.get_item(1)?, item.get_item(2)?);
            let mut t = [0i64; 3];
            for (q, x) in cell.iter()?.enumerate().take(3) {
                t[q] = x?.call_method0("__int__")?.extract()?;
            }
            cells.push(t);
            cl.call_method1("append", (tup3(py, t),))?;
            ml.call_method1("append", (m,))?;
            if dev.is_truthy()? {
                dl.call_method1("append", (Self::_init_device(&cs, &dev)?,))?;
            } else {
                dl.call_method1("append", (py.None(),))?;
            }
        }
        let n = cells.len();
        let mut pos = Vec::with_capacity(8 * n);
        for t in &cells {
            for cb in CORNER_BITS.iter() {
                let l = [(t[0] as f64 + cb[0]) * size, (t[1] as f64 + cb[1]) * size, (t[2] as f64 + cb[2]) * size];
                pos.push(add(mul_t(l, &rot), org));
            }
        }
        Self::set(&cs, "pos", a3(py, &pos)?)?;
        let vel0 = match velocity {
            Some(v) if !v.is_none() => v3(v)?,
            _ => [0.0; 3],
        };
        Self::set(&cs, "vel", a3(py, &vec![vel0; 8 * n])?)?;
        let mut rest = Vec::with_capacity(24 * n);
        for _ in 0..n {
            for cb in CORNER_BITS.iter() {
                rest.extend(cb.iter().map(|b| (b - 0.5) * size));
            }
        }
        Self::set(&cs, "rest", arr(py, rest, &[n, 8, 3])?)?;
        Self::set(&cs, "act_scale", arr(py, vec![1.0; 3 * n], &[n, 3])?)?;
        match links {
            Some(l) if !l.is_none() => Self::set(&cs, "links", py.get_type_bound::<PySet>().call1((l,))?)?,
            _ => {
                let set: std::collections::HashSet<[i64; 3]> = cells.iter().copied().collect();
                let ls = Self::g(&cs, "links")?.downcast_into::<PySet>()?;
                for t in &cells {
                    for d in [0usize, 2, 4] {
                        let nb = [t[0] + DIRS[d][0], t[1] + DIRS[d][1], t[2] + DIRS[d][2]];
                        if set.contains(&nb) {
                            ls.add(link_key_c(py, *t, nb))?;
                        }
                    }
                }
            }
        }
        if defer {
            return Ok(c);
        }
        Self::rebuild(&cs)?;
        let w = match angular_velocity {
            Some(v) if !v.is_none() => v3(v)?,
            _ => [0.0; 3],
        };
        if w.iter().any(|x| *x != 0.0) {
            Self::spin(&cs, w)?;
        }
        Ok(c)
    }

    /// 機器の情報を決まった形にする（状態は devstate の表のとおりにそろえる）
    fn _init_device<'py>(slf: &Bound<'py, Self>, dev: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let dev = super::actuator::act_migrate_device(py, dev)?;   // 古い「アクチュエーター」で押し出していた物はピストンに
        let reg = Self::g(slf, "reg")?;
        let def = dev.get_item("def")?;
        let dd = reg.call_method1("device", (&def,))?;
        let out = PyDict::new_bound(py);
        out.set_item("def", &def)?;
        out.set_item("dir", dget(&dev, "dir", 2)?.call_method0("__int__")?)?;
        let nm = dget(&dev, "name", py.None())?;
        let nm = if nm.is_truthy()? { nm } else if dd.is_truthy()? { dd.get_item("name")? } else { def.clone() };
        out.set_item("name", &nm)?;
        let state = dget(&dev, "state", py.None())?;
        if dd.is_truthy()? {
            let t: String = dd.get_item("type")?.extract()?;
            let params = dget(&dd, "params", PyDict::new_bound(py))?;
            let sname: String = Self::g(slf, "name")?.str()?.extract()?;
            let w = format!("（{}/{}）", sname, nm.str()?);
            let st = super::devstate::devstate_normalize(py, &t, Some(&params), if state.is_none() { None } else { Some(&state) }, &w)?;
            out.set_item("state", st)?;
        } else {
            let st = if state.is_truthy()? { state } else { PyDict::new_bound(py).into_any() };
            out.set_item("state", py.get_type_bound::<PyDict>().call1((st,))?)?;
        }
        Ok(out)
    }

    /// トポロジ（ボクセル・結合）が変わったら呼ぶ
    fn rebuild(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let cells = Self::cells_v(slf)?;
        let n = cells.len();
        let size = Self::sz(slf);
        let cl = Self::g(slf, "cells")?;
        let index = PyDict::new_bound(py);
        for (i, c) in cl.iter()?.enumerate() {
            index.set_item(c?, i)?;
        }
        Self::set(slf, "index", &index)?;
        let reg = Self::g(slf, "reg")?;
        let mats: Vec<Bound<'_, PyAny>> = Self::g(slf, "mats")?.iter()?
            .map(|m| reg.call_method1("material", (m?,))).collect::<R<_>>()?;
        let g = |k: &str| -> R<Vec<f64>> { mats.iter().map(|m| m.get_item(k)?.extract::<f64>()).collect() };
        let clip = |v: Vec<f64>| -> Vec<f64> { v.into_iter().map(|x| if x < 0.0 { 0.0 } else if x > 1.0 { 1.0 } else { x }).collect() };
        let (v_jstiff, v_break, mut v_plast);
        if n > 0 {
            let v_stiff = clip(g("stiffness")?);
            v_jstiff = clip(g("joint_stiffness")?);
            v_break = g("break_distance")?.into_iter().map(|x| x * size).collect::<Vec<_>>();
            v_plast = clip(g("plasticity")?);
            let v_density = g("density")?;
            let s3 = size.powf(3.0);
            let vm: Vec<f64> = v_density.iter().map(|d| d * s3 / 8.0).collect();
            let rep = |v: &[f64]| -> Vec<f64> { v.iter().flat_map(|x| std::iter::repeat(*x).take(8)).collect() };
            Self::set(slf, "v_stiff", arr(py, v_stiff, &[n])?)?;
            Self::set(slf, "v_jstiff", arr(py, v_jstiff.clone(), &[n])?)?;
            Self::set(slf, "v_break", arr(py, v_break.clone(), &[n])?)?;
            Self::set(slf, "v_yield", arr(py, g("yield_strain")?, &[n])?)?;
            Self::set(slf, "v_density", arr(py, v_density, &[n])?)?;
            let rb: Vec<bool> = mats.iter().map(|m| m.get_item("rebond")?.is_truthy()).collect::<R<_>>()?;
            Self::set(slf, "v_rebond", barr(py, rb))?;
            Self::set(slf, "v_rebond_dist", arr(py, g("rebond_distance")?.into_iter().map(|x| x * size).collect(), &[n])?)?;
            Self::set(slf, "inv_mass", arr(py, rep(&vm.iter().map(|m| 1.0 / m).collect::<Vec<_>>()), &[8 * n])?)?;
            Self::set(slf, "mass", arr(py, rep(&vm), &[8 * n])?)?;
            Self::set(slf, "damp", arr(py, rep(&g("damping")?), &[8 * n])?)?;
            Self::set(slf, "fric", arr(py, rep(&g("friction")?), &[8 * n])?)?;
        } else {
            for k in ["v_stiff", "v_jstiff", "v_break", "v_yield", "v_density", "v_rebond_dist", "inv_mass", "mass", "damp", "fric"] {
                Self::set(slf, k, zeros(py, &[0])?)?;
            }
            Self::set(slf, "v_rebond", barr(py, vec![]))?;
            v_jstiff = vec![];
            v_break = vec![];
            v_plast = vec![];
        }
        // アクチュエーターの出力面の結合は「回転ジョイント」として別に扱う
        let rotor_of = PyDict::new_bound(py);
        let devs = Self::g(slf, "devs")?;
        for (i, dv) in devs.iter()?.enumerate() {
            let dv = dv?;
            if dv.is_truthy()? && dev_type(&reg, &dv)?.as_deref() == Some("actuator") {
                let d: i64 = dget(&dv, "dir", 2)?.call_method0("__int__")?.extract()?;
                let c = cells[i];
                let du = DIRS[d.rem_euclid(6) as usize];
                let out = [c[0] + du[0], c[1] + du[1], c[2] + du[2]];
                rotor_of.set_item(link_key_c(py, c, out), (i, d, dv))?;
            }
        }
        let rotors = PyList::empty_bound(py);
        for v in rotor_of.values() {
            let ia: usize = v.get_item(0)?.extract()?;
            v_plast[ia] = 0.0;      // アクチュエーター自身は塑性変形させない
        }
        // 結合（ジョイント）
        let keys: Vec<Bound<'_, PyAny>> = Self::g(slf, "links")?.iter()?.collect::<R<_>>()?;
        let mut pa = vec![-1i64; keys.len()];
        let mut pb = vec![-1i64; keys.len()];
        let mut dcode = vec![-1i64; keys.len()];
        for (q, k) in keys.iter().enumerate() {
            let a = Self::idx_of(index.as_any(), &k.get_item(0)?)?;
            let b = Self::idx_of(index.as_any(), &k.get_item(1)?)?;
            if let Some(a) = a {
                pa[q] = a as i64;
            }
            if let Some(b) = b {
                pb[q] = b as i64;
            }
            if let (Some(a), Some(b)) = (a, b) {
                let dl = [cells[b][0] - cells[a][0], cells[b][1] - cells[a][1], cells[b][2] - cells[a][2]];
                if let Some(d) = DIRS.iter().position(|x| *x == dl) {
                    dcode[q] = d as i64;
                }
            }
        }
        let good: Vec<bool> = dcode.iter().map(|d| *d >= 0).collect();
        let new_links = PySet::empty_bound(py)?;
        for (q, k) in keys.iter().enumerate() {
            if good[q] {
                new_links.add(k)?;
            }
        }
        Self::set(slf, "links", &new_links)?;
        let is_rotor: Vec<bool> = if rotor_of.is_empty() { vec![false; keys.len()] } else {
            keys.iter().map(|k| rotor_of.contains(k)).collect::<R<_>>()?
        };
        for q in 0..keys.len() {
            if !(good[q] && is_rotor[q]) {
                continue;
            }
            let (ia, ib) = (pa[q] as usize, pb[q] as usize);
            let v = rotor_of.get_item(&keys[q])?.unwrap();
            let i_act: usize = v.get_item(0)?.extract()?;
            let i_out = if ia == i_act { ib } else { ia };
            v_plast[i_out] = 0.0;
            let r = PyDict::new_bound(py);
            r.set_item("a", i_act)?;
            r.set_item("b", i_out)?;
            r.set_item("d", v.get_item(1)?)?;
            r.set_item("dev", v.get_item(2)?)?;
            r.set_item("key", &keys[q])?;
            r.set_item("br", (v_break[ia].min(v_break[ib]) * 3.0).max(0.3 * size))?;
            rotors.append(r)?;
        }
        Self::set(slf, "rotors", &rotors)?;
        if n > 0 {
            Self::set(slf, "v_plast", arr(py, v_plast, &[n])?)?;
        } else {
            Self::set(slf, "v_plast", zeros(py, &[0])?)?;
        }
        let (mut ja, mut jb, mut jr, mut jk, mut jbrk) = (vec![], vec![], vec![], vec![], vec![]);
        let jkey = PyList::empty_bound(py);
        for q in 0..keys.len() {
            if !(good[q] && !is_rotor[q]) {
                continue;
            }
            let (a, b, d) = (pa[q] as usize, pb[q] as usize, dcode[q] as usize);
            let k_ = v_jstiff[a].min(v_jstiff[b]);
            let br = v_break[a].min(v_break[b]);
            for (ca, cb) in joint_pairs(d) {
                ja.push((8 * a + ca) as i64);
                jb.push((8 * b + cb) as i64);
                jr.push(0.0);
                jk.push(k_);
                jbrk.push(br);
                jkey.append(&keys[q])?;
            }
        }
        let extras = Self::g(slf, "extras")?;
        let items: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)> = extras.downcast::<PyDict>()?.iter().collect();
        for (key, lst) in items {
            let ia = Self::idx_of(index.as_any(), &key.get_item(0)?)?;
            let ib = Self::idx_of(index.as_any(), &key.get_item(1)?)?;
            let (ia, ib) = match (ia, ib) {
                (Some(a), Some(b)) => (a, b),
                _ => {
                    extras.del_item(&key)?;
                    continue;
                }
            };
            let k = v_jstiff[ia].min(v_jstiff[ib]);
            let br = v_break[ia].min(v_break[ib]);
            let mut xv = vec![PyString::new_bound(py, "x").into_any()];
            xv.extend(key.iter()?.collect::<R<Vec<_>>>()?);
            let xk = PyTuple::new_bound(py, xv);
            for e in lst.iter()? {
                let e = e?;
                let ca: i64 = e.get_item(0)?.extract()?;
                let cb: i64 = e.get_item(1)?.extract()?;
                let rest: f64 = e.get_item(2)?.extract()?;
                ja.push(8 * ia as i64 + ca);
                jb.push(8 * ib as i64 + cb);
                jr.push(rest);
                jk.push(k);
                jbrk.push(br + rest);
                jkey.append(&xk)?;
            }
        }
        let nv = 8 * n;
        let mut cnt = vec![0i64; nv];
        for &x in ja.iter().chain(jb.iter()) {
            if (x as usize) < nv {
                cnt[x as usize] += 1;
            }
        }
        let nj = ja.len();
        Self::set(slf, "ja", iarr(py, ja, &[nj])?)?;
        Self::set(slf, "jb", iarr(py, jb, &[nj])?)?;
        Self::set(slf, "jr", arr(py, jr, &[nj])?)?;
        Self::set(slf, "jk", arr(py, jk, &[nj])?)?;
        Self::set(slf, "jbrk", arr(py, jbrk, &[nj])?)?;
        Self::set(slf, "jkey", &jkey)?;
        Self::set(slf, "jcount", arr(py, cnt.iter().map(|c| (*c).max(1) as f64).collect(), &[nv])?)?;
        // 表面（描画・OBJ出力用）：結合していない向きの面
        let mut linked = vec![[false; 6]; n];
        for q in 0..keys.len() {
            if good[q] {
                let d = dcode[q] as usize;
                linked[pa[q] as usize][d] = true;
                linked[pb[q] as usize][d ^ 1] = true;
            }
        }
        let (mut fvx, mut fdr, mut fid) = (vec![], vec![], vec![]);
        for (i, l) in linked.iter().enumerate() {
            for d in 0..6 {
                if !l[d] {
                    fvx.push(i as i64);
                    fdr.push(d as i64);
                    fid.extend(face_quad(d).iter().map(|c| (8 * i + c) as i64));
                }
            }
        }
        let nf = fvx.len();
        Self::set(slf, "face_vox", iarr(py, fvx, &[nf])?)?;
        Self::set(slf, "face_dir", iarr(py, fdr, &[nf])?)?;
        Self::set(slf, "face_idx", iarr(py, fid, &[nf, 4])?)?;
        let need = match opt(slf.as_any(), "force")? {
            None => true,
            Some(f) => f.getattr("shape")?.get_item(0)?.extract::<usize>()? != nv,
        };
        if need {
            Self::set(slf, "force", zeros(py, &[nv, 3])?)?;
        }
        // 形（球＋結合部を埋める図形）と当たり判定用の球
        if n > 0 {
            let shape = imp(py, "main.shape")?;
            Self::set(slf, "topo", shape.call_method1("topology", (slf,))?)?;
            Self::set(slf, "proxW", shape.call_method1("collision_proxies", (slf,))?)?;
            slf.call_method0("_build_contacts")?;
        } else {
            let t = PyDict::new_bound(py);
            t.set_item("points", iarr(py, vec![], &[0])?)?;
            t.set_item("segs", iarr(py, vec![], &[0, 2])?)?;
            t.set_item("tris", iarr(py, vec![], &[0, 3])?)?;
            Self::set(slf, "topo", t)?;
            Self::set(slf, "proxW", py.None())?;
        }
        slf.borrow_mut().version += 1;
        Self::set(slf, "dirty", true)?;    // 配管・電力・気密の再計算が必要
        Self::wake(slf, true)
    }

    /// soft=True：形が変わる操作（アクチュエーターの動作・ブロックの追加削除）なので剛体扱いも解く
    #[pyo3(signature = (soft=false))]
    fn wake(slf: &Bound<'_, Self>, soft: bool) -> R<()> {
        let py = slf.py();
        let mut b = slf.borrow_mut();
        b.asleep = false;
        b.sleep_t = 0.0;
        if soft {
            b.want_soft = true;
            b.calm_t = 0.0;
            if b.mode != "soft" && b.mode != "art" {     // 関節つき剛体はそのまま（形は作り直す）
                b.mode = "soft".into();
                b.rb = py.None();
            }
        }
        Ok(())
    }

    /// ほぼ静止している構築物は計算を省く（スリープ）
    #[pyo3(signature = (dt, threshold=0.05, delay=1.0))]
    fn update_sleep(slf: &Bound<'_, Self>, dt: f64, threshold: f64, delay: f64) -> R<()> {
        if slf.borrow().asleep {
            return Ok(());
        }
        let f = fv(&Self::g(slf, "force")?)?;
        if Self::n(slf)? == 0 || f.iter().any(|x| *x != 0.0) {
            slf.borrow_mut().sleep_t = 0.0;
            return Ok(());
        }
        let vel = Self::g(slf, "vel")?;
        let v = fv(&vel)?;
        let mut vmax = f64::NEG_INFINITY;
        for x in &v {
            let a = x.abs();
            if a.is_nan() || vmax.is_nan() {
                vmax = f64::NAN;
            } else if a > vmax {
                vmax = a;
            }
        }
        if vmax < threshold {
            slf.borrow_mut().sleep_t += dt;
            if slf.borrow().sleep_t > delay && !truthy(slf.as_any(), "_touch_moving")? {
                slf.borrow_mut().asleep = true;
                if !with_rw(&vel, |s| s.fill(0.0))? {
                    vel.set_item(PySliceAll::all(slf.py())?, 0.0)?;
                }
                slf.borrow_mut().sleep_epoch += 1;
            }
        } else {
            slf.borrow_mut().sleep_t = 0.0;
        }
        Ok(())
    }

    // ------------------------------------------------------------------ 物理の値
    #[getter]
    fn n_vox(slf: &Bound<'_, Self>) -> R<usize> {
        Self::n(slf)
    }
    #[getter]
    fn n_vert(slf: &Bound<'_, Self>) -> R<usize> {
        Ok(8 * Self::n(slf)?)
    }
    fn center_of_mass<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let m = Self::mass_v(slf)?;
        if m.is_empty() {
            return Ok(a1(slf.py(), [0.0; 3]));
        }
        Ok(a1(slf.py(), wmean(&Self::pos_v(slf)?, &m)))
    }
    fn velocity<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let m = Self::mass_v(slf)?;
        if m.is_empty() {
            return Ok(a1(slf.py(), [0.0; 3]));
        }
        Ok(a1(slf.py(), wmean(&Self::vel_v(slf)?, &m)))
    }
    fn total_mass(slf: &Bound<'_, Self>) -> R<f64> {
        Ok(pw_sum(&Self::mass_v(slf)?))
    }

    /// アンカーのブロックの番号の配列（形が変わったときだけ数え直す）
    fn anchor_index<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let key = Self::vkey(slf)?;
        if let Some(c) = Self::cached(slf, "_anchor_cache", key.as_any())? {
            return c.get_item(1);
        }
        let reg = Self::g(slf, "reg")?;
        let mut idx = vec![];
        for (i, d) in Self::g(slf, "devs")?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? && dev_type(&reg, &d)?.as_deref() == Some("anchor") {
                idx.push(i as i64);
            }
        }
        let n = idx.len();
        let a = iarr(py, idx, &[n])?;
        Self::set(slf, "_anchor_cache", (key, &a))?;
        Ok(a)
    }
    #[getter]
    fn anchored(slf: &Bound<'_, Self>) -> R<bool> {
        Self::anchored_b(slf)
    }

    /// アクチュエーター i の両側（本体の部分と出力の先の部分）が、どちらもアンカーで固定されている（動かせない）か
    fn actuator_locked(slf: &Bound<'_, Self>, i: usize) -> R<bool> {
        let anc = iv(&Self::anchor_index(slf)?)?;
        if anc.is_empty() || !Self::g(slf, "devs")?.get_item(i)?.is_truthy()? {
            return Ok(false);
        }
        let oc = super::actuator::act_output_cell(slf.as_any(), i)?;
        let j = match Self::idx_of(&Self::g(slf, "index")?, oc.as_any())? {
            Some(j) => j,
            None => return Ok(false),
        };
        Self::part_of(slf, i)?;                // 部分の番号を最新に
        let label = iv(&Self::g(slf, "_part_cache")?.get_item(1)?)?;
        let fixed: std::collections::HashSet<i64> = anc.iter().map(|a| label[*a as usize]).collect();
        Ok(fixed.contains(&label[i]) && fixed.contains(&label[j]))
    }

    /// 形と置き場所が変わっていなければ同じになる値（止まっている物の計算の使い回しに）
    fn _pose_key<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let n = Self::n(slf)?;
        let v = Self::ver(slf);
        if n == 0 {
            return Ok(PyTuple::new_bound(py, [v.into_py(py), 0.into_py(py)]));
        }
        let p = Self::g(slf, "pos")?;
        let f = |r: isize, c: usize| -> R<f64> { p.get_item((r, c))?.extract() };
        let mode = slf.borrow().mode.clone();
        Ok(PyTuple::new_bound(py, [v.into_py(py), n.into_py(py), mode.into_py(py), f(0, 0)?.into_py(py), f(0, 1)?.into_py(py),
            f(-1, 2)?.into_py(py), f(-1, 0)?.into_py(py)]))
    }

    /// ブロックの中心。止まっている物（アンカーで固定・眠っている）は計算を使い回す
    fn centers_cached<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        if !Self::still(slf)? {
            return Self::voxel_centers(slf);
        }
        let k = Self::_pose_key(slf)?;
        if let Some(c) = Self::cached(slf, "_ctr_cache", k.as_any())? {
            return c.get_item(1);
        }
        let v = Self::voxel_centers(slf)?;
        Self::set(slf, "_ctr_cache", (k, &v))?;
        Ok(v)
    }
    fn frames_cached<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        if !Self::still(slf)? {
            return Self::voxel_frames(slf);
        }
        let k = Self::_pose_key(slf)?;
        if let Some(c) = Self::cached(slf, "_frm_cache", k.as_any())? {
            return c.get_item(1);
        }
        let v = Self::voxel_frames(slf)?;
        Self::set(slf, "_frm_cache", (k, &v))?;
        Ok(v)
    }

    /// 外側の球 (中心, 半径)。止まっている物は使い回す
    fn bounds<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let k: Bound<'py, PyAny> = if Self::still(slf)? { Self::_pose_key(slf)?.into_any() } else { py.None().into_bound(py) };
        if !k.is_none() {
            if let Some(c) = Self::cached(slf, "_bnd_cache", &k)? {
                return c.get_item(1);
            }
        }
        let b = if Self::n(slf)? == 0 {
            PyTuple::new_bound(py, [a1(py, [0.0; 3]), 0.0f64.into_py(py).into_bound(py)])
        } else {
            let p = Self::pos_v(slf)?;
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for q in &p {
                for k in 0..3 {
                    lo[k] = lo[k].min(q[k]);
                    hi[k] = hi[k].max(q[k]);
                }
            }
            let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
            PyTuple::new_bound(py, [a1(py, c), (norm(sub(hi, lo)) * 0.5).into_py(py).into_bound(py)])
        };
        Self::set(slf, "_bnd_cache", (k, &b))?;
        Ok(b.into_any())
    }

    /// 止まっていて、毎ティック処理する機器が無い。slow_ok：ライトのように、ときどきまとめて処理すれば足りる機器だけなら True
    #[pyo3(signature = (slow_ok=false))]
    fn is_quiet(slf: &Bound<'_, Self>, slow_ok: bool) -> R<bool> {
        let py = slf.py();
        if !Self::is_static_b(slf)? {
            return Ok(false);
        }
        let key = Self::vkey(slf)?;
        let cache = match Self::cached(slf, "_quiet_cache", key.as_any())? {
            Some(c) => c,
            None => {
                let reg = Self::g(slf, "reg")?;
                let cls = slf.get_type();
                let passive_s = cls.getattr("PASSIVE_DEVICES")?;
                let slow_s = cls.getattr("SLOW_DEVICES")?;
                let (mut passive, mut slow, mut comps) = (true, true, vec![]);
                for (i, d) in Self::g(slf, "devs")?.iter()?.enumerate() {
                    let d = d?;
                    if !d.is_truthy()? {
                        continue;
                    }
                    let t = dev_type(&reg, &d)?;
                    let tp = t.clone().into_py(py);
                    let inp = passive_s.contains(&tp)?;
                    passive &= inp;
                    slow &= inp || slow_s.contains(&tp)?;
                    if t.as_deref() == Some("computer") {
                        comps.push(i);
                    }
                }
                let c = PyTuple::new_bound(py, [key.into_any(), passive.into_py(py).into_bound(py),
                    PyList::new_bound(py, comps).into_any(), slow.into_py(py).into_bound(py)]);
                Self::set(slf, "_quiet_cache", &c)?;
                c.into_any()
            }
        };
        if !cache.get_item(if slow_ok { 3 } else { 1 })?.is_truthy()? {
            return Ok(false);
        }
        let devs = Self::g(slf, "devs")?;
        for i in cache.get_item(2)?.iter()? {
            if devs.get_item(i?)?.get_item("state")?.call_method1("get", ("running",))?.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    #[getter]
    fn has_devices(slf: &Bound<'_, Self>) -> R<bool> {
        let key = Self::vkey(slf)?;
        if let Some(c) = Self::cached(slf, "_hasdev", key.as_any())? {
            return c.get_item(1)?.is_truthy();
        }
        let mut any = false;
        for d in Self::g(slf, "devs")?.iter()? {
            if d?.is_truthy()? {
                any = true;
                break;
            }
        }
        Self::set(slf, "_hasdev", (key, any))?;
        Ok(any)
    }

    /// (R, t)：格子の座標（セル × 大きさ）→ ワールド。剛体なら剛体の姿勢、止まっている物は使い回す
    fn lattice_pose<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let still = Self::still(slf)?;
        if Self::mode_is(slf, &["rigid", "rails"]) && !slf.borrow().rb.is_none(py) && !still {
            return Self::rigid_fit(slf, None);
        }
        let k: Bound<'py, PyAny> = if still { Self::_pose_key(slf)?.into_any() } else { py.None().into_bound(py) };
        if !k.is_none() {
            if let Some(c) = Self::cached(slf, "_lat_cache", &k)? {
                return c.get_item(1);
            }
        }
        let out = Self::rigid_fit(slf, None)?;
        Self::set(slf, "_lat_cache", (k, &out))?;
        Ok(out)
    }

    /// 遠くから見せる「それっぽい色の箱」：(中心 (k,3), 半分の大きさ (k,3), 色 (k,3))。格子の座標（セル単位）
    fn lod_boxes<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let key = Self::vkey(slf)?;
        if let Some(c) = Self::cached(slf, "_lod_cache", key.as_any())? {
            return c.get_item(1);
        }
        let n = Self::n(slf)?;
        let out = if n == 0 {
            PyTuple::new_bound(py, [zeros(py, &[0, 3])?, zeros(py, &[0, 3])?, zeros(py, &[0, 3])?])
        } else {
            let cells = Self::cells_v(slf)?;
            let reg = Self::g(slf, "reg")?;
            let lc: i64 = slf.get_type().getattr("LOD_CELL")?.extract()?;
            let mats: Vec<Bound<'_, PyAny>> = Self::g(slf, "mats")?.iter()?.collect::<R<_>>()?;
            let devs: Vec<Bound<'_, PyAny>> = Self::g(slf, "devs")?.iter()?.collect::<R<_>>()?;
            let grey = PyList::new_bound(py, [0.6, 0.6, 0.6]);
            let mut cols = vec![];
            let mut mid: Vec<(String, String)> = vec![];
            for (m, d) in mats.iter().zip(&devs) {
                let src = if d.is_truthy()? { reg.call_method1("device", (d.get_item("def")?,))? } else { reg.call_method1("material", (m,))? };
                let src = if src.is_truthy()? { src } else { PyDict::new_bound(py).into_any() };
                cols.push(v3(&dget(&src, "color", &grey)?)?);
                let dd = if d.is_truthy()? { d.call_method1("get", ("def",))?.repr()?.to_string() } else { "None".into() };
                mid.push((m.repr()?.to_string(), dd));
            }
            let key_of = |c: &[i64; 3]| [c[0].div_euclid(lc), c[1].div_euclid(lc), c[2].div_euclid(lc)];
            let mut uniq: Vec<[i64; 3]> = cells.iter().map(key_of).collect();
            uniq.sort();
            uniq.dedup();
            let inv: Vec<usize> = cells.iter().map(|c| uniq.binary_search(&key_of(c)).unwrap()).collect();
            let gn = uniq.len();
            let mut lo = vec![[i64::MAX; 3]; gn];
            let mut hi = vec![[i64::MIN; 3]; gn];
            for (c, &g) in cells.iter().zip(&inv) {
                for k in 0..3 {
                    lo[g][k] = lo[g][k].min(c[k]);
                    hi[g][k] = hi[g][k].max(c[k] + 1);
                }
            }
            let mut col = vec![[0.0; 3]; gn];
            for g in 0..gn {
                let sel: Vec<usize> = (0..n).filter(|&i| inv[i] == g).collect();
                let mut cnt: Vec<(&(String, String), usize)> = vec![];
                for &i in &sel {
                    match cnt.iter_mut().find(|x| x.0 == &mid[i]) {
                        Some(x) => x.1 += 1,
                        None => cnt.push((&mid[i], 1)),
                    }
                }
                let mx = cnt.iter().map(|x| x.1).max().unwrap_or(0);
                let best: Vec<&(String, String)> = cnt.iter().filter(|x| x.1 == mx).map(|x| x.0).collect();
                let pick: Vec<V3> = sel.iter().filter(|&&i| best.contains(&&mid[i])).map(|&i| cols[i]).collect();
                col[g] = mean8(&pick);
            }
            let c: Vec<V3> = (0..gn).map(|g| { let mut o = [0.0; 3]; for k in 0..3 { o[k] = (lo[g][k] + hi[g][k]) as f64 * 0.5; } o }).collect();
            let e: Vec<V3> = (0..gn).map(|g| { let mut o = [0.0; 3]; for k in 0..3 { o[k] = (hi[g][k] - lo[g][k]) as f64 * 0.5; } o }).collect();
            PyTuple::new_bound(py, [a3(py, &c)?, a3(py, &e)?, a3(py, &col)?])
        };
        Self::set(slf, "_lod_cache", (key, &out))?;
        Ok(out.into_any())
    }

    /// 構築物どうしの押し出しに使う、ブロックごとの質量の逆数。アンカーで固定されたブロックはほぼ 0
    fn collision_inv<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let im = fv(&Self::g(slf, "inv_mass")?)?;
        let mut v: Vec<f64> = im.iter().step_by(8).copied().collect();
        if Self::anchored_b(slf)? {
            let fi: f64 = slf.get_type().getattr("FIXED_INV")?.extract()?;
            let n = Self::n(slf)?;
            let mut done = false;
            if Self::mode_is(slf, &["art"]) {
                if let Some(a) = opt(slf.as_any(), "art")? {
                    if dget(&a, "n", py.None())?.eq(n)? {
                        let fixed = bv(&a.get_item("fixed")?)?;
                        if fixed.iter().any(|x| *x) {
                            let gid = iv(&a.get_item("gid")?)?;
                            for (i, g) in gid.iter().enumerate() {
                                if fixed[*g as usize] {
                                    v[i] = fi;
                                }
                            }
                            done = true;
                        }
                    }
                }
            }
            if !done {
                v.iter_mut().for_each(|x| *x = fi);
            }
        }
        let n = v.len();
        arr(py, v, &[n])
    }

    /// 物理計算をせず、その場に止まっている（アンカーで固定・アクチュエーターも止まっている）
    #[getter]
    fn is_static(slf: &Bound<'_, Self>) -> R<bool> {
        Self::is_static_b(slf)
    }

    /// ブロック i の中心（ワールド座標）
    fn voxel_center<'py>(slf: &Bound<'py, Self>, i: usize) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        // そのブロックの 8 頂点だけを読む（毎フレーム音などから呼ばれ、全部の頂点を写すと大きな構築物で重い）
        let pa = slf.borrow().pos.clone_ref(py);
        if let Ok(r) = pa.bind(py).extract::<numpy::PyReadonlyArrayDyn<f64>>() {
            if let Ok(s) = r.as_slice() {
                if s.len() >= 24 * (i + 1) {
                    let mut q = [[0.0; 3]; 8];
                    for (k, row) in q.iter_mut().enumerate() {
                        row.copy_from_slice(&s[3 * (8 * i + k)..3 * (8 * i + k) + 3]);
                    }
                    return Ok(a1(py, mean8(&q)));
                }
            }
        }
        let p = Self::pos_v(slf)?;
        Ok(a1(py, mean8(&p[8 * i..8 * i + 8])))
    }
    fn voxel_centers<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        a3(slf.py(), &Self::centers(slf)?)
    }
    /// 各ボクセルのローカル軸 (n,3,3)。行が x,y,z 軸（単位ベクトル）
    fn voxel_frames<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let p = Self::pos_v(slf)?;
        let n = p.len() / 8;
        let mut out = Vec::with_capacity(9 * n);
        for b in p.chunks_exact(8) {
            for (hi, lo) in [([1, 3, 5, 7], [0, 2, 4, 6]), ([2, 3, 6, 7], [0, 1, 4, 5]), ([4, 5, 6, 7], [0, 1, 2, 3])] {
                let mut s = [0.0; 3];
                for q in 0..4 {
                    let d = sub(b[hi[q]], b[lo[q]]);
                    for k in 0..3 {
                        s[k] += d[k];
                    }
                }
                let e = [s[0] / 4.0, s[1] / 4.0, s[2] / 4.0];
                let l = ((0.0 + e[0] * e[0]) + e[1] * e[1] + e[2] * e[2]).sqrt().max(1e-9);
                out.extend([e[0] / l, e[1] / l, e[2] / l]);
            }
        }
        arr(slf.py(), out, &[n, 3, 3])
    }
    // ------------------------------------------------------------------ 1サブステップ（Rust の Body）
    /// 単独で1サブステップ進める（テスト用）
    #[pyo3(signature = (h, iters, gravity_fn, planets, gscale=1.0))]
    fn substep(slf: &Bound<'_, Self>, h: f64, iters: i64, gravity_fn: &Bound<'_, PyAny>, planets: &Bound<'_, PyAny>,
               gscale: Option<f64>) -> R<bool> {
        Self::substep_begin(slf, h, iters, gravity_fn, planets, gscale)?;
        Self::substep_end(slf, h)
    }
    /// 1サブステップの前半。gscale：重力倍率（重力は惑星から Rust で計算する。gravity_fn は昔の名残り）
    #[pyo3(signature = (h, iters, gravity_fn, planets, gscale=1.0))]
    fn substep_begin(slf: &Bound<'_, Self>, h: f64, iters: i64, gravity_fn: &Bound<'_, PyAny>, planets: &Bound<'_, PyAny>,
                     gscale: Option<f64>) -> R<()> {
        let _ = gravity_fn;
        if Self::n(slf)? == 0 {
            return Ok(());
        }
        let gs = gscale.unwrap_or(1.0);
        if Self::mode_is(slf, &["art"]) {
            Self::_native_art_begin(slf, h, planets, gs)?;
        } else {
            Self::_native_begin(slf, h, iters, planets, gs)?;
        }
        Ok(())
    }
    fn substep_end(slf: &Bound<'_, Self>, h: f64) -> R<bool> {
        if Self::n(slf)? == 0 {
            return Ok(false);
        }
        if Self::mode_is(slf, &["art"]) {
            if truthy(slf.as_any(), "_nb_art")? {
                Self::set(slf, "_nb_art", false)?;
                Self::_native_art_end(slf, h)?;
            }
            return Ok(false);
        }
        if truthy(slf.as_any(), "_nb_used")? {
            Self::set(slf, "_nb_used", false)?;
            return Self::_native_end(slf, h);
        }
        Ok(false)
    }

    /// 気密の空洞セルと、その6方向で接するボクセルの番号（浮力を伝える先）
    fn _cavity_neighbors<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let net = match opt(slf.as_any(), "_net")? {
            Some(n) if n.getattr("cavities")?.is_truthy()? => n,
            _ => return Ok(py.None().into_bound(py)),
        };
        if let Some(c) = opt(slf.as_any(), "_cav_cache")? {
            if c.get_item(0)?.is(&net) {
                return c.get_item(1);
            }
        }
        let index = Self::g(slf, "index")?;
        let mut nb: Vec<i64> = vec![];
        for cav in net.getattr("cavities")?.iter()? {
            for cell in cav?.get_item("cells")?.iter()? {
                let c = cell_of(&cell?)?;
                let mut row = [-1i64; 6];
                for (d, du) in DIRS.iter().enumerate() {
                    // その向きで最初に当たるブロック（空洞のまん中のセルも、まわりの壁に浮力を渡す。
                    // となりだけを見ていたときは、壁に触れていないセルの浮力が無くなっていた）
                    for k in 1..=64 {
                        let v: i64 = index.call_method1("get", (tup3(py, [c[0] + du[0] * k, c[1] + du[1] * k, c[2] + du[2] * k]), -1))?.extract()?;
                        if v >= 0 {
                            row[d] = v * 128 + k;          // ブロックの番号 × 128 ＋ 距離（セルの数）
                            break;
                        }
                    }
                }
                if row.iter().any(|x| *x >= 0) {
                    nb.extend(row);
                }
            }
        }
        let data: Bound<'py, PyAny> = if nb.is_empty() { py.None().into_bound(py) } else {
            let k = nb.len() / 6;
            PyTuple::new_bound(py, [iarr(py, nb, &[k, 6])?]).into_any()
        };
        Self::set(slf, "_cav_cache", (net, &data))?;
        Ok(data)
    }

    /// 各頂点の当たり判定に使う「芯」（点・線・三角形）を選ぶ（Rust の build_contacts）
    fn _build_contacts(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let cells: Vec<i64> = Self::cells_v(slf)?.into_iter().flatten().collect();
        let n = cells.len() / 3;
        let topo = Self::g(slf, "topo")?;
        let segs = iv(&topo.get_item("segs")?)?;
        let tris = iv(&topo.get_item("tris")?)?;
        let (ns, nt) = (segs.len() / 2, tris.len() / 3);
        let r = core(py)?.call_method1("build_contacts", (iarr(py, cells, &[n, 3])?, contig(&Self::g(slf, "pos")?)?,
            Self::voxel_frames(slf)?, Self::voxel_centers(slf)?, iarr(py, segs, &[ns, 2])?, iarr(py, tris, &[nt, 3])?))?;
        Self::set(slf, "ctype", r.get_item(0)?)?;
        Self::set(slf, "cidx", r.get_item(1)?)
    }
    /// 各頂点に対応する芯の最近点（この点から半径 r が見た目の表面）
    fn contact_cores<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        core(slf.py())?.call_method1("contact_cores", (Self::g(slf, "pos")?, Self::g(slf, "ctype")?, Self::g(slf, "cidx")?))
    }

    /// 地面から高い頂点を省くための (上向き, その方向の地表半径)。重心が惑星の中心なら (None, 0)
    fn _ground_ref<'py>(slf: &Bound<'py, Self>, pl: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let p = Self::pos_v(slf)?;
        let com = sub(mean8(&p), v3(&pl.getattr("center")?)?);
        let cn = norm(com);
        if cn <= 1e-6 {
            return Ok(PyTuple::new_bound(py, [py.None(), 0.0f64.into_py(py)]));
        }
        let up = [com[0] / cn, com[1] / cn, com[2] / cn];
        if let Some(gc) = opt(slf.as_any(), "_ground_cache")? {
            if gc.get_item(0)?.is(pl) {
                let u0 = v3(&gc.get_item(1)?)?;
                let dmax = (0..3).map(|k| (u0[k] - up[k]).abs()).fold(f64::NEG_INFINITY, f64::max);
                let rad: f64 = pl.getattr("radius")?.extract()?;
                if dmax * rad < 0.5 {
                    return Ok(PyTuple::new_bound(py, [a1(py, up).unbind(), gc.get_item(2)?.unbind()]));
                }
            }
        }
        let rs: f64 = pl.call_method1("surface_radius", (arr(py, up.to_vec(), &[1, 3])?,))?.get_item(0)?.extract()?;
        let upa = a1(py, up);
        Self::set(slf, "_ground_cache", (pl, &upa, rs))?;
        Ok(PyTuple::new_bound(py, [upa.unbind(), rs.into_py(py)]))
    }

    /// 地面との当たり判定（Rust）。collect なら押し戻さずに (頂点, めり込み, 法線) を返す
    #[pyo3(signature = (pl, collect=false))]
    fn _collide_planet<'py>(slf: &Bound<'py, Self>, pl: &Bound<'py, PyAny>, collect: bool) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let nat = pl.call_method0("_nat")?;
        let k = Self::contact_cores(slf)?;
        let r = Self::sz(slf) * 0.5;
        let gr = Self::_ground_ref(slf, pl)?;
        let up = gr.get_item(0)?;
        let upl: PyObject = if up.is_none() { py.None() } else { PyList::new_bound(py, v3(&up)?).into_any().unbind() };
        Self::_contig(slf, &["pos"])?;
        let args: Vec<PyObject> = vec![Self::g(slf, "pos")?.unbind(), contig(&Self::g(slf, "_prev")?)?.unbind(),
            contig(&Self::g(slf, "vel")?)?.unbind(), contig(&k)?.unbind(), contig(&Self::g(slf, "fric")?)?.unbind(), r.into_py(py),
            pl.getattr("bound")?.extract::<f64>()?.into_py(py), pl.getattr("lipschitz")?.extract::<f64>()?.into_py(py), upl,
            gr.get_item(1)?.unbind(), Self::mode_is(slf, &["rigid"]).into_py(py),
            slf.get_type().getattr("IMPACT_SPEED")?.extract::<f64>()?.into_py(py), CONTACT_SOFT.into_py(py), (!collect).into_py(py)];
        let res = nat.call_method1("collide", PyTuple::new_bound(py, args))?;
        if res.get_item(0)?.is_truthy()? {
            slf.borrow_mut().want_soft = true;
        }
        let hi = res.get_item(1)?;
        if collect && hi.len()? > 0 {
            return Ok(PyTuple::new_bound(py, [hi, res.get_item(2)?, res.get_item(3)?]).into_any());
        }
        Ok(py.None().into_bound(py))
    }

    /// 拡張が出した向き付きの箱（街の建物など）との当たり判定。
    /// 強くぶつかった速さと、動いている物体を押した力積は ext_events に記録する（拡張が after_physics で使う）
    #[pyo3(signature = (bs, h, collect=false))]
    fn _collide_boxes<'py>(slf: &Bound<'py, Self>, bs: &Bound<'py, PyAny>, h: f64, collect: bool) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let none = || py.None().into_bound(py);
        let (owner, c, a, e, idx) = (bs.get_item(0)?, bs.get_item(1)?, bs.get_item(2)?, bs.get_item(3)?, bs.get_item(4)?);
        let kc = v3s(&Self::contact_cores(slf)?)?;
        let r = Self::sz(slf) * 0.5;
        let cv = v3s(&c)?;
        let ev = v3s(&e)?;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for (q, cc) in cv.iter().enumerate() {
            let rb = norm(ev[q]);
            for k in 0..3 {
                lo[k] = lo[k].min(cc[k] - rb);
                hi[k] = hi[k].max(cc[k] + rb);
            }
        }
        for k in 0..3 {
            lo[k] -= r;
            hi[k] += r;
        }
        let sel: Vec<usize> = (0..kc.len()).filter(|&i| (0..3).all(|k| kc[i][k] > lo[k] && kc[i][k] < hi[k])).collect();
        if sel.is_empty() {
            return Ok(none());
        }
        let ks: Vec<V3> = sel.iter().map(|&i| kc[i]).collect();
        let res = core(py)?.call_method1("box_sdf", (a3(py, &ks)?, contig(&c)?, contig(&a)?, contig(&e)?))?;
        let d = fv(&res.get_item(0)?)?;
        let nn = v3s(&res.get_item(1)?)?;
        let kk = iv(&res.get_item(2)?)?;
        let idxv = iv(&idx)?;
        let (mut hi_, mut dep, mut n, mut parts) = (vec![], vec![], vec![], vec![]);
        for q in 0..sel.len() {
            let dd = d[q] - r;
            if dd < 0.0 {
                hi_.push(sel[q]);
                dep.push(dd);
                n.push(nn[q]);
                parts.push(idxv[kk[q] as usize]);
            }
        }
        if hi_.is_empty() {
            return Ok(none());
        }
        let pos_o = Self::g(slf, "pos")?;
        let mut pos = v3s(&pos_o)?;
        let vel = Self::vel_v(slf)?;
        let moving = owner.call_method0("is_moving")?.is_truthy()?;
        let ph: Vec<V3> = hi_.iter().map(|&i| pos[i]).collect();
        let vs: Vec<V3> = if moving { v3s(&owner.call_method1("point_velocity", (a3(py, &ph)?,))?)? } else { vec![[0.0; 3]; hi_.len()] };
        let vn: Vec<f64> = (0..hi_.len()).map(|q| -dot(sub(vel[hi_[q]], vs[q]), n[q])).collect();
        let evs = Self::g(slf, "ext_events")?;
        if vn.iter().any(|v| *v >= 7.0) {        // 建物の損傷（いちばん強く当たった速さ）
            let best = PyDict::new_bound(py);
            for (pa, v) in parts.iter().zip(&vn) {
                let cur: f64 = best.get_item(pa)?.map(|x| x.extract()).transpose()?.unwrap_or(0.0);
                if *v > cur {
                    best.set_item(pa, v)?;
                }
            }
            evs.call_method1("append", ((&owner, best, py.None(), py.None()),))?;
        }
        if collect {
            let nh = hi_.len();
            return Ok(PyTuple::new_bound(py, [iarr(py, hi_.iter().map(|x| *x as i64).collect(), &[nh])?,
                arr(py, dep.iter().map(|x| -x).collect(), &[nh])?, a3(py, &n)?, a3(py, &vs)?]).into_any());
        }
        let prev = v3s(&Self::g(slf, "_prev")?)?;
        let fric = fv(&Self::g(slf, "fric")?)?;
        let mass = Self::mass_v(slf)?;
        let (mut jv, mut pts) = (vec![], vec![]);
        for q in 0..hi_.len() {
            let i = hi_[q];
            let corr = scl(n[q], -dep[q]);
            pos[i] = add(pos[i], corr);
            let disp = sub(sub(pos[i], prev[i]), scl(vs[q], h));
            let tang = sub(disp, scl(n[q], dot(disp, n[q])));
            let fr = fric[i].clamp(0.0, 1.0);
            pos[i] = sub(pos[i], scl(tang, fr));
            if moving {
                let j = scl(sub(corr, scl(tang, fr)), mass[i] / h);
                jv.push(scl(j, -1.0));
                pts.push(pos[i]);
            }
        }
        write_back(slf, "pos", &pos_o, pos.iter().flatten().copied().collect(), &[pos.len(), 3])?;
        if moving {
            evs.call_method1("append", ((&owner, py.None(), a3(py, &jv)?, a3(py, &pts)?),))?;
        }
        Ok(none())
    }

    /// この構築物の Rust の Body（形が変わったら作り直す）
    fn _native_body<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let reg = Self::g(slf, "reg")?;
        let rotors: Vec<Bound<'py, PyAny>> = Self::g(slf, "rotors")?.iter()?.collect::<R<_>>()?;
        let mut hk = vec![];
        for r in &rotors {
            match super::actuator::act_hinge_of(&r.get_item("dev")?, Some(&reg))? {
                Some((a, b)) => hk.extend([a as i64, b as i64]),
                None => hk.extend([-1, -1]),
            }
        }
        let key = PyTuple::new_bound(py, [Self::ver(slf).into_py(py), Self::n(slf)?.into_py(py),
            PyTuple::new_bound(py, &hk).into_py(py)]);
        if let Some(c) = Self::cached(slf, "_nb_cache", key.as_any())? {
            return c.get_item(1);
        }
        let nr = rotors.len();
        let (mut ra, mut rva, mut rvb, mut rbr) = (vec![], vec![], vec![], vec![]);
        for r in &rotors {
            let a: i64 = r.get_item("a")?.extract()?;
            let b: i64 = r.get_item("b")?.extract()?;
            let d: usize = r.get_item("d")?.extract()?;
            ra.push(a);
            for (ca, cb) in joint_pairs(d) {
                rva.push(8 * a + ca as i64);
                rvb.push(8 * b + cb as i64);
            }
            rbr.push(r.get_item("br")?.extract::<f64>()?);
        }
        let i = |k: &str| -> R<Bound<'py, PyAny>> { cint(&Self::g(slf, k)?) };
        let f = |k: &str| -> R<Bound<'py, PyAny>> { cflt(&Self::g(slf, k)?) };
        let args = PyTuple::new_bound(py, [Self::sz(slf).into_py(py).into_bound(py), i("ja")?, i("jb")?, f("jr")?, f("jk")?, f("jbrk")?,
            f("jcount")?, f("inv_mass")?, f("mass")?, f("damp")?, f("fric")?, f("v_stiff")?, f("v_plast")?, f("v_yield")?,
            f("v_density")?, i("ctype")?, i("cidx")?, iarr(py, ra, &[nr])?, iarr(py, rva, &[nr, 4])?, iarr(py, rvb, &[nr, 4])?,
            arr(py, rbr, &[nr])?, iarr(py, hk, &[nr, 2])?]);
        // まとまりの形合わせ（大きな構築物の固さ）：セルと、まとまりに入れてよいブロック（アクチュエーターは入れない）
        let n = Self::n(slf)?;
        let mut cells = Vec::with_capacity(3 * n);
        for c in Self::g(slf, "cells")?.iter()? {
            for x in c?.iter()?.take(3) {
                cells.push(x?.call_method0("__int__")?.extract::<i64>()?);
            }
        }
        let mut ok = vec![true; n];
        for i in Self::actuator_indices(slf)?.iter()? {
            let i: usize = i?.extract()?;
            if i < n {
                ok[i] = false;
            }
        }
        let kw = PyDict::new_bound(py);
        if cells.len() == 3 * n {
            kw.set_item("cells", iarr(py, cells, &[n, 3])?)?;
            kw.set_item("cluster_ok", ok)?;
        }
        let body = core(py)?.getattr("Body")?.call(args, Some(&kw))?;
        Self::set(slf, "_nb_cache", (key, &body))?;
        Ok(body)
    }

    /// まとまりの基準の並び（Body ごと。作り直した Body では NaN から：最初の計算で今の形から作る）
    fn _crest<'py>(slf: &Bound<'py, Self>, body: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        if let Some(c) = opt(slf.as_any(), "_crest_cache")? {
            if c.get_item(0)?.is(body) {
                return c.get_item(1);
            }
        }
        let m: usize = body.getattr("n_cluster_members")?.extract()?;
        let a = arr(py, vec![f64::NAN; 3 * m.max(1)], &[3 * m.max(1)])?;
        let a = if m == 0 { arr(py, vec![], &[0])? } else { a };
        Self::set(slf, "_crest_cache", (body, &a))?;
        Ok(a)
    }

    /// 構築物どうしの押し出しに使う Rust の Proxy（形が変わったら作り直す）
    fn native_proxy<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let vinv = Self::collision_inv(slf)?;
        let anch: PyObject = if Self::anchored_b(slf)? { PyTuple::new_bound(py, fv(&vinv)?).into_py(py) } else { py.None() };
        let key = PyTuple::new_bound(py, [Self::ver(slf).into_py(py), Self::n(slf)?.into_py(py), anch]);
        if let Some(c) = Self::cached(slf, "_nproxy", key.as_any())? {
            return c.get_item(1);
        }
        let w = Self::g(slf, "proxW")?.call_method0("tocsr")?;
        w.call_method0("sort_indices")?;
        let n = Self::n(slf)?;
        let index = Self::g(slf, "index")?;
        let mut pairs = vec![];
        for k in Self::link_list(slf, true)? {
            if let (Some(a), Some(b)) = (Self::idx_of(&index, &k.get_item(0)?)?, Self::idx_of(&index, &k.get_item(1)?)?) {
                pairs.extend([a as i64, b as i64]);
            }
        }
        let np_ = pairs.len() / 2;
        let inv = cflt(&w.call_method1("__matmul__", (&vinv,))?)?;
        let obj = core(py)?.getattr("Proxy")?.call1((n, Self::sz(slf), cint(&w.getattr("indptr")?)?, cint(&w.getattr("indices")?)?,
            cflt(&w.getattr("data")?)?, inv, iarr(py, pairs, &[np_, 2])?, cflt(&Self::g(slf, "mass")?)?))?;
        Self::set(slf, "_nproxy", (key, &obj))?;
        Ok(obj)
    }

    fn _rotor_angles(slf: &Bound<'_, Self>) -> R<Vec<f64>> {
        let mut out = vec![];
        for r in Self::g(slf, "rotors")?.iter()? {
            let a: f64 = r?.get_item("dev")?.get_item("state")?.call_method1("get", ("angle", 0.0))?.extract()?;
            out.push(a.to_radians());
        }
        Ok(out)
    }

    /// Rust に渡す配列を、連続した書き込める float64 にそろえる
    #[pyo3(signature = (*names), name = "_contig")]
    fn _contig_py(slf: &Bound<'_, Self>, names: Vec<String>) -> R<()> {
        let v: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        Self::_contig(slf, &v)
    }

    fn _native_begin(slf: &Bound<'_, Self>, h: f64, iters: i64, planets: &Bound<'_, PyAny>, gscale: f64) -> R<bool> {
        let py = slf.py();
        let body = Self::_native_body(slf)?;
        let pls = construct_native_planets(py, planets)?.into_bound(py);
        let rigid = Self::mode_is(slf, &["rigid"]);
        let full = rigid && truthy(slf.as_any(), "_full_rigid")?;
        Self::ensure_prev(slf)?;
        Self::_contig(slf, &["pos", "vel", "_prev", "force"])?;
        let cav = Self::cav_arg(slf, &pls)?;
        let soft = CONTACT_SOFT;
        let imp_s: f64 = slf.get_type().getattr("IMPACT_SPEED")?.extract()?;
        let (pos, vel, prev, force) = (Self::g(slf, "pos")?, Self::g(slf, "vel")?, Self::g(slf, "_prev")?, Self::g(slf, "force")?);
        if rigid {
            let rb = Self::g(slf, "rb")?;
            let defer = full && Self::g(slf, "ext_colliders")?.is_truthy()?;
            let args: Vec<PyObject> = vec![pos.unbind(), vel.unbind(), prev.unbind(), force.unbind(), cflt(&rb.get_item("R")?)?.unbind(),
                cflt(&rb.get_item("Ib")?)?.unbind(), pls.clone().unbind(), gscale.into_py(py), h.into_py(py), cav.unbind(), full.into_py(py),
                imp_s.into_py(py), soft.into_py(py)];
            let mut a = args;
            if defer {
                a.push(true.into_py(py));
            }
            let want = body.call_method1("rigid_begin", PyTuple::new_bound(py, a))?;
            if want.is_truthy()? {
                slf.borrow_mut().want_soft = true;
            }
            if defer {
                // 完全剛体で MOD の箱（街の建物など）に触れうるとき：箱との接触は MOD のプログラムで集め、地面と一緒に解く
                Self::_native_full_contacts(slf, &body, &pls, h)?;
                Self::set(slf, "_nb_used", true)?;
                return Ok(true);
            }
        } else {
            Self::_ensure_lastR(slf)?;
            Self::_contig(slf, &["rest", "act_scale", "_last_R", "_quat"])?;
            let pmin: usize = imp(py, "main.fastphys")?.getattr("PARALLEL_MIN_VOXELS")?.extract()?;
            let par = Self::n(slf)? >= pmin && !slf.borrow().in_pool;
            let args: Vec<PyObject> = vec![pos.unbind(), vel.unbind(), prev.unbind(), force.unbind(), Self::g(slf, "rest")?.unbind(),
                Self::g(slf, "act_scale")?.unbind(), Self::g(slf, "_last_R")?.unbind(), Self::g(slf, "_quat")?.unbind(), pls.clone().unbind(),
                gscale.into_py(py), h.into_py(py), iters.into_py(py), Self::_rotor_angles(slf)?.into_py(py), par.into_py(py), cav.unbind(),
                soft.into_py(py), Self::_crest(slf, &body)?.unbind()];
            body.call_method1("soft_begin", PyTuple::new_bound(py, args))?;
        }
        let ecs: Vec<Bound<'_, PyAny>> = Self::g(slf, "ext_colliders")?.iter()?.collect::<R<_>>()?;
        for bs in ecs {
            Self::_collide_boxes(slf, &bs, h, false)?;
        }
        Self::set(slf, "_nb_used", true)?;
        Ok(true)
    }

    /// world._native_tick に渡す、この構築物の配列（dict）。まとめて計算できないときは None
    fn tick_job<'py>(slf: &Bound<'py, Self>, groups: &Bound<'py, PyAny>, planets_ok: bool) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let none = || py.None().into_bound(py);
        if !planets_ok || Self::n(slf)? == 0 || !Self::mode_is(slf, &["soft", "rigid", "art"]) {
            return Ok(none());
        }
        let body = Self::_native_body(slf)?;
        if body.is_none() {
            return Ok(none());
        }
        let (idx, lst) = (groups.get_item(0)?, groups.get_item(1)?);
        let boxes = PyList::empty_bound(py);
        for bs in Self::g(slf, "ext_colliders")?.iter()? {
            let bs = bs?;
            if bs.get_item(0)?.call_method0("is_moving")?.is_truthy()? {
                return Ok(none());          // 動いている街：サブステップごとに進めて押した反動を拡張に渡す
            }
            let c = bs.get_item(1)?;
            let cid = c.as_ptr() as usize;
            let k = idx.call_method1("get", (cid,))?;
            let k = if k.is_none() {
                let k = lst.len()?;
                idx.set_item(cid, k)?;
                lst.call_method1("append", ((cflt(&c)?, cflt(&bs.get_item(2)?)?, cflt(&bs.get_item(3)?)?, cint(&bs.get_item(4)?)?),))?;
                k.into_py(py).into_bound(py)
            } else { k };
            boxes.append(k)?;
        }
        Self::ensure_prev(slf)?;
        Self::_contig(slf, &["pos", "vel", "_prev", "force"])?;
        let pmin: usize = imp(py, "main.fastphys")?.getattr("PARALLEL_MIN_VOXELS")?.extract()?;
        let job = PyDict::new_bound(py);
        job.set_item("body", &body)?;
        job.set_item("proxy", Self::native_proxy(slf)?)?;
        job.set_item("pos", Self::g(slf, "pos")?)?;
        job.set_item("vel", Self::g(slf, "vel")?)?;
        job.set_item("prev", Self::g(slf, "_prev")?)?;
        job.set_item("force", Self::g(slf, "force")?)?;
        job.set_item("boxes", boxes)?;
        job.set_item("cav", py.None())?;
        job.set_item("parallel", Self::n(slf)? >= pmin)?;
        if Self::mode_is(slf, &["art"]) {
            let a = Self::art_fresh(slf)?;
            if dget(&a, "nat", py.None())?.is_none() {
                return Ok(none());
            }
            Self::_art_contig(slf)?;
            let (ext_want, ext_speed) = Self::ext_targets(slf, &a)?;
            let (want, spd) = Self::rot_targets(&a)?;
            job.set_item("kind", 3)?;
            job.set_item("topo", a.get_item("nat")?)?;
            for (k, s) in [("x", "X"), ("r", "R"), ("vl", "vl"), ("vl0", "vl0"), ("applied", "applied"), ("ext_applied", "ext_applied")] {
                job.set_item(k, a.get_item(s)?)?;
            }
            job.set_item("want", want)?;
            job.set_item("rot_speed", spd)?;
            job.set_item("ext_want", ext_want)?;
            job.set_item("ext_speed", ext_speed)?;
        } else if Self::mode_is(slf, &["rigid"]) {
            let rb = Self::g(slf, "rb")?;
            if rb.is_none() {
                return Ok(none());
            }
            for k in ["R", "Ib", "local", "x"] {
                if let Some(v) = fix_contig(&rb.get_item(k)?)? {
                    rb.set_item(k, v)?;
                }
            }
            let full = truthy(slf.as_any(), "_full_rigid")?;
            job.set_item("kind", if full { 2 } else { 1 })?;
            job.set_item("rb_r", rb.get_item("R")?)?;
            job.set_item("rb_ib", rb.get_item("Ib")?)?;
            job.set_item("local", rb.get_item("local")?)?;
            job.set_item("rb_x", rb.get_item("x")?)?;
        } else {
            Self::_ensure_lastR(slf)?;
            Self::_contig(slf, &["rest", "act_scale", "_last_R", "_quat"])?;
            job.set_item("kind", 0)?;
            job.set_item("rest", Self::g(slf, "rest")?)?;
            job.set_item("act_scale", Self::g(slf, "act_scale")?)?;
            job.set_item("last_r", Self::g(slf, "_last_R")?)?;
            job.set_item("quat", Self::g(slf, "_quat")?)?;
            job.set_item("angles", Self::_rotor_angles(slf)?)?;
            job.set_item("crest", Self::_crest(slf, &body)?)?;
        }
        Ok(job.into_any())
    }

    /// Rust でまとめて計算した間にちぎれた結合を外す（_native_end と同じ）
    fn tick_broken(slf: &Bound<'_, Self>, broken: &Bound<'_, PyAny>, rot: &Bound<'_, PyAny>) -> R<()> {
        let rot: Vec<usize> = iv(rot)?.into_iter().map(|x| x as usize).collect();
        let broken: Vec<usize> = iv(broken)?.into_iter().map(|x| x as usize).collect();
        Self::drop_broken(slf, &broken, &rot)?;
        Self::rebuild(slf)
    }

    // ---- 関節つき剛体（完全剛体モードでアクチュエーターが動く間）の Rust 版 ----
    fn _native_art_topo<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let a = Self::g(slf, "art")?;
        let ex: Vec<Bound<'py, PyAny>> = a.get_item("exts")?.iter()?.collect::<R<_>>()?;
        let ne = ex.len();
        let mut rows = vec![];
        let mut nbs = vec![];
        let mut e0 = vec![];
        for e in &ex {
            rows.extend(iv(&e.get_item(1)?)?);
            nbs.extend(fv(&e.get_item(2)?)?);
            e0.push(e.get_item(3)?.extract::<f64>()?);
        }
        let i = |k: &str, w: usize| -> R<Bound<'py, PyAny>> {
            let v = iv(&a.get_item(k)?)?;
            let n = if w == 0 { v.len() } else { v.len() / w };
            iarr(py, v, &if w == 0 { vec![n] } else { vec![n, w] })
        };
        let cls = slf.get_type();
        let kw = PyDict::new_bound(py);
        if bv(&a.get_item("fixed")?)?.iter().any(|x| *x) {
            kw.set_item("fixed", cint(&a.get_item("fixed")?)?)?;
            kw.set_item("xfix", cflt(&a.get_item("Xfix")?)?)?;
        }
        let args = PyTuple::new_bound(py, [i("vg", 0)?, cflt(&a.get_item("invM")?)?, cflt(&a.get_item("IinvB")?)?, i("ra", 0)?, i("rb", 0)?,
            i("A", 4)?, i("B", 4)?, i("C", 8)?, iarr(py, rows, &[ne, 4])?, arr(py, nbs, &[ne, 3])?, arr(py, e0, &[ne])?,
            cls.getattr("ART_ITERS")?, cls.getattr("ART_CONTACTS")?, cls.getattr("MOTOR_STEP_X")?.call_method0("__float__")?,
            cls.getattr("MOTOR_HOLD_DEG")?.call_method0("__float__")?, i("HK", 2)?]);
        core(py)?.getattr("ArtTopo")?.call(args, if kw.is_empty() { None } else { Some(&kw) })
    }

    fn _art_contig(slf: &Bound<'_, Self>) -> R<()> {
        let a = Self::g(slf, "art")?;
        for k in ["X", "R", "vl", "vl0", "applied", "ext_applied"] {
            if let Some(v) = fix_contig(&a.get_item(k)?)? {
                a.set_item(k, v)?;
            }
        }
        Ok(())
    }

    /// 関節つき剛体の1サブステップの前半（Rust）。MOD の箱との接触だけ集めて渡す（MOD のプログラムを呼ぶため）
    fn _native_art_begin(slf: &Bound<'_, Self>, h: f64, planets: &Bound<'_, PyAny>, gscale: f64) -> R<bool> {
        let py = slf.py();
        let a = Self::art_fresh(slf)?;
        let nat = a.get_item("nat")?;
        let body = Self::_native_body(slf)?;
        let pls = construct_native_planets(py, planets)?.into_bound(py);
        Self::ensure_prev(slf)?;
        Self::_contig(slf, &["pos", "vel", "_prev", "force"])?;
        Self::_art_contig(slf)?;
        let cav = Self::cav_arg(slf, &pls)?;
        let (ext_want, ext_speed) = Self::ext_targets(slf, &a)?;
        let (pos, vel, prev, force) = (Self::g(slf, "pos")?, Self::g(slf, "vel")?, Self::g(slf, "_prev")?, Self::g(slf, "force")?);
        let args: Vec<PyObject> = vec![body.clone().unbind(), pos.clone().unbind(), vel.unbind(), prev.clone().unbind(), force.unbind(),
            a.get_item("X")?.unbind(), a.get_item("R")?.unbind(), a.get_item("vl")?.unbind(), a.get_item("vl0")?.unbind(),
            a.get_item("ext_applied")?.unbind(), ext_want.into_py(py), ext_speed.into_py(py), pls.clone().unbind(), gscale.into_py(py),
            h.into_py(py), cav.unbind()];
        let pr = nat.call_method1("predict", PyTuple::new_bound(py, args))?;
        let (rows, d) = (pr.get_item(0)?, pr.get_item(1)?);
        let defo = PyList::empty_bound(py);
        if rows.len()? > 0 {
            defo.append((rows, d))?;
        }
        a.set_item("defo", defo)?;
        // MOD の箱（街の建物など）との接触は、MOD のプログラムを呼ぶので集める
        let (boxes, cat) = Self::collect_boxes(slf, h)?;
        let (want, spd) = Self::rot_targets(&a)?;
        let args: Vec<PyObject> = vec![body.unbind(), pos.unbind(), prev.unbind(), a.get_item("X")?.unbind(), a.get_item("R")?.unbind(),
            a.get_item("vl")?.unbind(), a.get_item("applied")?.unbind(), want.into_py(py), spd.into_py(py), pls.unbind(), h.into_py(py),
            cat[0].clone_ref(py), cat[1].clone_ref(py), cat[2].clone_ref(py), cat[3].clone_ref(py), cat[4].clone_ref(py)];
        let sv = nat.call_method1("solve", PyTuple::new_bound(py, args))?;
        a.set_item("contacts", (sv.get_item(0)?, sv.get_item(1)?, sv.get_item(2)?, sv.get_item(3)?))?;
        // 動いている街（ステーション）を押した分の反動
        let pts = v3s(&Self::g(slf, "pos")?)?;
        let cp = iv(&sv.get_item(4)?)?;
        let pts_at: Vec<V3> = cp.iter().map(|&i| pts[i as usize]).collect();
        Self::push_back_events(slf, &boxes, &sv.get_item(5)?, &sv.get_item(3)?, &sv.get_item(2)?, &pts_at, h)?;
        Self::set(slf, "_nb_art", true)?;
        Ok(true)
    }

    fn _native_art_end(slf: &Bound<'_, Self>, h: f64) -> R<()> {
        let py = slf.py();
        let a = Self::g(slf, "art")?;
        Self::_contig(slf, &["pos", "vel", "_prev"])?;
        Self::_art_contig(slf)?;
        let defo = dget(&a, "defo", py.None())?;
        let (rows, d) = if defo.is_truthy()? {
            let e = defo.get_item(0)?;
            (e.get_item(0)?, e.get_item(1)?)
        } else {
            (iarr(py, vec![], &[0])?, zeros(py, &[0, 3])?)
        };
        let ct = dget(&a, "contacts", py.None())?;
        let ct: Vec<Bound<'_, PyAny>> = if ct.is_none() {
            vec![iarr(py, vec![], &[0])?, zeros(py, &[0, 3])?, zeros(py, &[0, 3])?, zeros(py, &[0])?]
        } else {
            ct.iter()?.collect::<R<_>>()?
        };
        let args: Vec<PyObject> = vec![Self::_native_body(slf)?.unbind(), Self::g(slf, "pos")?.unbind(), Self::g(slf, "vel")?.unbind(),
            Self::g(slf, "_prev")?.unbind(), a.get_item("X")?.unbind(), a.get_item("R")?.unbind(), a.get_item("vl")?.unbind(), rows.unbind(),
            d.unbind(), ct[0].clone().unbind(), ct[1].clone().unbind(), ct[2].clone().unbind(), ct[3].clone().unbind(), h.into_py(py)];
        a.get_item("nat")?.call_method1("end", PyTuple::new_bound(py, args))?;
        a.set_item("defo", PyList::empty_bound(py))?;
        a.set_item("contacts", py.None())
    }

    fn _native_full_contacts(slf: &Bound<'_, Self>, body: &Bound<'_, PyAny>, pls: &Bound<'_, PyAny>, h: f64) -> R<()> {
        let py = slf.py();
        let (boxes, cat) = Self::collect_boxes(slf, h)?;
        let imp_s: f64 = slf.get_type().getattr("IMPACT_SPEED")?.extract()?;
        let args: Vec<PyObject> = vec![Self::g(slf, "pos")?.unbind(), Self::g(slf, "_prev")?.unbind(), Self::g(slf, "vel")?.unbind(),
            pls.clone().unbind(), h.into_py(py), imp_s.into_py(py), CONTACT_SOFT.into_py(py), cat[0].clone_ref(py), cat[1].clone_ref(py),
            cat[2].clone_ref(py), cat[3].clone_ref(py), cat[4].clone_ref(py)];
        let r = body.call_method1("rigid_contacts", PyTuple::new_bound(py, args))?;
        if r.get_item(0)?.is_truthy()? {
            slf.borrow_mut().want_soft = true;
        }
        let pts = v3s(&r.get_item(5)?)?;
        Self::push_back_events(slf, &boxes, &r.get_item(4)?, &r.get_item(3)?, &r.get_item(2)?, &pts, h)
    }

    fn _native_end(slf: &Bound<'_, Self>, h: f64) -> R<bool> {
        let py = slf.py();
        if Self::mode_is(slf, &["rigid"]) {
            let rb = Self::g(slf, "rb")?;
            rb.set_item("local", cflt(&rb.get_item("local")?)?)?;
            Self::_contig(slf, &["pos", "vel", "_prev"])?;
            let r = core(py)?.call_method1("rigid_end", (Self::g(slf, "pos")?, Self::g(slf, "vel")?, Self::g(slf, "_prev")?,
                rb.get_item("local")?, cflt(&Self::g(slf, "mass")?)?, h))?;
            rb.set_item("R", r.get_item(0)?)?;
            rb.set_item("x", r.get_item(1)?)?;
            return Ok(false);
        }
        let body = Self::_native_body(slf)?;
        Self::_contig(slf, &["rest"])?;
        let last_r = match opt(slf.as_any(), "_last_R")? {
            Some(v) => v,
            None => zeros(py, &[0])?,
        };
        let r = body.call_method1("soft_end", (Self::g(slf, "pos")?, Self::g(slf, "vel")?, Self::g(slf, "_prev")?, Self::g(slf, "rest")?,
            Self::g(slf, "act_scale")?, last_r, h, Self::_rotor_angles(slf)?, Self::_crest(slf, &body)?))?;
        let broken: Vec<usize> = iv(&r.get_item(0)?)?.into_iter().map(|x| x as usize).collect();
        let rot: Vec<usize> = iv(&r.get_item(1)?)?.into_iter().map(|x| x as usize).collect();
        let broke = !broken.is_empty() || !rot.is_empty();
        Self::drop_broken(slf, &broken, &rot)?;
        if broke {
            Self::rebuild(slf)?;
        }
        Ok(broke)
    }

    #[allow(non_snake_case)]
    fn _ensure_lastR(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let n = Self::n(slf)?;
        let need = match opt(slf.as_any(), "_last_R")? {
            Some(v) => v.len()? != n,
            None => true,
        };
        if need {
            let e = eye();
            let v: Vec<f64> = (0..n).flat_map(|_| e.iter().flatten().copied().collect::<Vec<_>>()).collect();
            Self::set(slf, "_last_R", arr(py, v, &[n, 3, 3])?)?;
        }
        if opt(slf.as_any(), "_allvox")?.map(|v| v.len().map(|l| l != n)).transpose()?.unwrap_or(true) {
            Self::set(slf, "_allvox", iarr(py, (0..n as i64).collect(), &[n])?)?;
        }
        if opt(slf.as_any(), "_quat")?.map(|v| v.len().map(|l| l != n)).transpose()?.unwrap_or(true) {
            let v: Vec<f64> = (0..n).flat_map(|_| [1.0, 0.0, 0.0, 0.0]).collect();
            Self::set(slf, "_quat", arr(py, v, &[n, 4])?)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------ 軽量化 1：剛体化
    /// 剛体化してよい構築物か（液体・粘土など柔らかい素材、動作中のアクチュエーターは不可）
    fn _rigid_allowed(slf: &Bound<'_, Self>) -> R<bool> {
        if Self::n(slf)? < 2 {
            return Ok(false);
        }
        if fv(&Self::g(slf, "v_stiff")?)?.iter().any(|x| *x < 0.5) || bv(&Self::g(slf, "v_rebond")?)?.iter().any(|x| *x) {
            return Ok(false);
        }
        let reg = Self::g(slf, "reg")?;
        for d in Self::g(slf, "devs")?.iter()? {
            let d = d?;
            if d.is_truthy()? && dev_type(&reg, &d)?.as_deref() == Some("actuator") && super::actuator::act_busy(&d.get_item("state")?)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// 重力以外の力による全体の加速度 (m/s²)
    fn body_acceleration(slf: &Bound<'_, Self>) -> R<f64> {
        let f = v3s(&Self::g(slf, "force")?)?;
        let mut s = [0.0; 3];
        for q in &f {
            for k in 0..3 {
                s[k] += q[k];
            }
        }
        Ok(norm(s) / pw_sum(&Self::mass_v(slf)?).max(1e-9))
    }
    /// (重心, 重心速度, 角速度, 慣性テンソル, 重心からの位置)
    fn _rigid_motion<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let rm = Self::rigid_motion(slf)?;
        Ok(PyTuple::new_bound(py, [a1(py, rm.x), a1(py, rm.v), a1(py, rm.w), mat(py, &rm.i)?, a3(py, &rm.r)?]))
    }
    /// 形がほとんど変わっていない（剛体として動いている）か
    fn calm_for_rigid(slf: &Bound<'_, Self>) -> R<bool> {
        let rm = Self::rigid_motion(slf)?;
        let vel = Self::vel_v(slf)?;
        let mut dmax = 0.0f64;
        let mut ext2 = f64::NEG_INFINITY;
        for (q, r) in rm.r.iter().enumerate() {
            let d = sub(vel[q], add(rm.v, cross(rm.w, *r)));
            for k in 0..3 {
                dmax = dmax.max(d[k].abs());
            }
            ext2 = ext2.max(dot(*r, *r));
        }
        let ext = ext2.sqrt();
        let c = |k: &str| -> R<f64> { slf.get_type().getattr(k)?.extract() };
        let acc_in = c("RIGID_ACC_IN")?;
        Ok(dmax < c("RIGID_DEFORM_V")? && dot(rm.w, rm.w) * ext < acc_in && Self::body_acceleration(slf)? < acc_in)
    }
    fn enter_rigid(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let rm = Self::rigid_motion(slf)?;
        let rb = PyDict::new_bound(py);
        rb.set_item("local", a3(py, &rm.r)?)?;
        rb.set_item("R", mat(py, &eye())?)?;
        rb.set_item("x", a1(py, rm.x))?;
        rb.set_item("v", a1(py, rm.v))?;
        rb.set_item("w", a1(py, rm.w))?;
        rb.set_item("Ib", mat(py, &rm.i)?)?;
        rb.set_item("ext", rm.r.iter().map(|r| dot(*r, *r)).fold(f64::NEG_INFINITY, f64::max).sqrt())?;
        let vel: Vec<V3> = rm.r.iter().map(|r| add(rm.v, cross(rm.w, *r))).collect();
        Self::set(slf, "vel", a3(py, &vel)?)?;
        let mut b = slf.borrow_mut();
        b.rb = rb.into_any().unbind();
        b.mode = "rigid".into();
        b.rb_epoch += 1;
        b.want_soft = false;
        Ok(())
    }
    fn leave_rigid(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let mut b = slf.borrow_mut();
        b.mode = "soft".into();
        b.rb = py.None();
        b.calm_t = 0.0;
        b.want_soft = false;
        Ok(())
    }

    /// 毎ティック：ブロックごと ⇔ 剛体 ⇔ レール（遠方の軌道計算）を切り替える
    fn update_mode(slf: &Bound<'_, Self>, dt: f64, world: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        if Self::n(slf)? == 0 || slf.borrow().asleep {
            return Ok(());
        }
        let phys = world.getattr("cfg")?.call_method1("get", ("physics", PyDict::new_bound(py)))?;
        // 完全剛体モードでも soft_ok の構築物（物語の見せ場の落石・木箱など）は、曲がる・ちぎれる
        let mut full = world.getattr("settings")?.call_method1("get", ("full_rigid", false))?.is_truthy()?
            && !truthy(slf.as_any(), "soft_ok")?;
        if Self::anchored_b(slf)? {          // アンカーで固定：柔らかいブロックのモードでも剛体（＋関節）として扱う
            full = true;
        }
        Self::set(slf, "_full_rigid", full)?;
        if full {
            return Self::_update_full_rigid(slf, world, &phys, dt);
        }
        if Self::mode_is(slf, &["art"]) {       // 完全剛体モードを切ったとき
            slf.borrow_mut().mode = "soft".into();
            Self::set(slf, "art", py.None())?;
        }
        let c = |k: &str| -> R<f64> { slf.get_type().getattr(k)?.extract() };
        let acc = Self::body_acceleration(slf)?;
        let pg = |k: &str, d: bool| -> R<bool> { phys.call_method1("get", (k, d))?.is_truthy() };
        let rails_d: f64 = phys.call_method1("get", ("rails_distance", 400.0))?.extract()?;
        if Self::mode_is(slf, &["rigid", "rails"]) {
            let mut out = slf.borrow().want_soft || acc > c("RIGID_ACC_OUT")? || !Self::_rigid_allowed(slf)?;
            if !out && Self::mode_is(slf, &["rigid"]) {
                let rm = Self::rigid_motion(slf)?;
                let ext: f64 = Self::g(slf, "rb")?.get_item("ext")?.extract()?;
                out = dot(rm.w, rm.w) * ext > c("RIGID_ACC_OUT")?;
            }
            if out {
                if Self::mode_is(slf, &["rails"]) {
                    Self::leave_rails(slf)?;
                }
                return Self::leave_rigid(slf);
            }
        }
        if Self::mode_is(slf, &["soft"]) {
            if !pg("rigid_enabled", true)? || slf.borrow().want_soft {
                let mut b = slf.borrow_mut();
                b.want_soft = false;
                b.calm_t = 0.0;
                return Ok(());
            }
            if Self::_rigid_allowed(slf)? && Self::calm_for_rigid(slf)? {
                slf.borrow_mut().calm_t += dt;
                if slf.borrow().calm_t >= c("RIGID_CALM_T")? {
                    Self::enter_rigid(slf)?;
                }
            } else {
                slf.borrow_mut().calm_t = 0.0;
            }
        }
        // 軽量化 2：プレイヤーから遠い構築物は「レール」に乗せる
        if Self::mode_is(slf, &["rigid"]) && pg("rails_enabled", true)? {
            if Self::_rails_ok(slf, world, rails_d)? {
                Self::enter_rails(slf)?;
            }
        } else if Self::mode_is(slf, &["rails"]) && !Self::_rails_ok(slf, world, rails_d * 0.9)? {
            Self::leave_rails(slf)?;
        }
        Ok(())
    }

    /// 機器のあるブロックの番号（kind が種類なら、その種類だけ）。形が変わったときだけ数え直す
    #[pyo3(signature = (kind=None))]
    fn dev_ids<'py>(slf: &Bound<'py, Self>, kind: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let key = Self::vkey(slf)?;
        let by = match Self::cached(slf, "_devid_cache", key.as_any())? {
            Some(c) => c.get_item(1)?,
            None => {
                let by = PyDict::new_bound(py);
                let all = PyList::empty_bound(py);
                by.set_item(py.None(), &all)?;
                let reg = Self::g(slf, "reg")?;
                for (i, d) in Self::g(slf, "devs")?.iter()?.enumerate() {
                    let d = d?;
                    if d.is_truthy()? {
                        all.append(i)?;
                        let t = dev_type(&reg, &d)?.into_py(py);
                        by.call_method1("setdefault", (t, PyList::empty_bound(py)))?.call_method1("append", (i,))?;
                    }
                }
                Self::set(slf, "_devid_cache", (key, &by))?;
                by.into_any()
            }
        };
        by.call_method1("get", (kind.map(|k| k.clone().unbind()).unwrap_or_else(|| py.None()), PyTuple::empty_bound(py)))
    }
    /// アクチュエーター（ヒンジなども）の番号（形が変わったときだけ数え直す）
    fn actuator_indices<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let key = Self::vkey(slf)?;
        if let Some(c) = Self::cached(slf, "_act_cache", key.as_any())? {
            return c.get_item(1);
        }
        let reg = Self::g(slf, "reg")?;
        let out = PyList::empty_bound(py);
        for (i, d) in Self::g(slf, "devs")?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? && dev_type(&reg, &d)?.as_deref() == Some("actuator") {
                out.append(i)?;
            }
        }
        Self::set(slf, "_act_cache", (key, &out))?;
        Ok(out.into_any())
    }
    fn _actuator_busy(slf: &Bound<'_, Self>) -> R<bool> {
        let devs = Self::g(slf, "devs")?;
        for i in Self::actuator_indices(slf)?.iter()? {
            if super::actuator::act_busy(&devs.get_item(i?)?.get_item("state")?)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 完全剛体モード：構築物は丸ごと1つの剛体。アクチュエーターが動いている間は部分ごとの剛体を関節でつないで動かす（mode "art"）
    #[pyo3(signature = (world, phys, dt=0.0))]
    fn _update_full_rigid(slf: &Bound<'_, Self>, world: &Bound<'_, PyAny>, phys: &Bound<'_, PyAny>, dt: f64) -> R<()> {
        slf.borrow_mut().want_soft = false;          // 衝突などによる「ブロックごとに戻す」要求は無視する
        if Self::_actuator_busy(slf)? {
            Self::set(slf, "_art_hold", slf.get_type().getattr("ART_HOLD")?)?;
            if Self::mode_is(slf, &["rails"]) {
                Self::leave_rails(slf)?;
            }
            if !Self::mode_is(slf, &["art"]) {
                Self::enter_art(slf)?;
            }
            return Ok(());
        }
        if Self::mode_is(slf, &["art"]) {
            let hold: f64 = match opt(slf.as_any(), "_art_hold")? { Some(v) => v.extract()?, None => 0.0 };
            if hold > 0.0 {
                Self::set(slf, "_art_hold", hold - dt)?;
                return Ok(());
            }
        }
        if Self::mode_is(slf, &["soft", "art"]) {
            Self::enter_rigid(slf)?;
        }
        if Self::anchored_b(slf)? {
            if Self::mode_is(slf, &["rails"]) {
                Self::leave_rails(slf)?;
            }
            return Ok(());
        }
        let rails_d: f64 = phys.call_method1("get", ("rails_distance", 400.0))?.extract()?;
        if Self::mode_is(slf, &["rigid"]) && phys.call_method1("get", ("rails_enabled", true))?.is_truthy()? {
            if Self::_rails_ok(slf, world, rails_d)? {
                Self::enter_rails(slf)?;
            }
        } else if Self::mode_is(slf, &["rails"]) && !Self::_rails_ok(slf, world, rails_d * 0.9)? {
            Self::leave_rails(slf)?;
        }
        Ok(())
    }

    fn _art_groups<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let g = Self::art_groups(slf)?;
        let n = g.len();
        iarr(slf.py(), g.into_iter().map(|x| x as i64).collect(), &[n])
    }

    fn _art_build(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let gid = Self::art_groups(slf)?;
        let gn = gid.iter().max().map(|x| x + 1).unwrap_or(0);
        let vg: Vec<usize> = gid.iter().flat_map(|g| std::iter::repeat(*g).take(8)).collect();
        let m = Self::mass_v(slf)?;
        let pos = Self::pos_v(slf)?;
        let mut mm = vec![0.0; gn];
        let mut x = vec![[0.0; 3]; gn];
        for (q, &g) in vg.iter().enumerate() {
            mm[g] += m[q];
        }
        for k in 0..3 {
            for (q, &g) in vg.iter().enumerate() {
                x[g][k] += m[q] * pos[q][k];
            }
        }
        for g in 0..gn {
            for k in 0..3 {
                x[g][k] /= mm[g];
            }
        }
        let vl: Vec<V3> = pos.iter().zip(&vg).map(|(p, &g)| sub(*p, x[g])).collect();
        let mut ib = vec![[[0.0; 3]; 3]; gn];
        for i in 0..3 {
            for j in 0..3 {
                for (q, &g) in vg.iter().enumerate() {
                    let rr = dot(vl[q], vl[q]);
                    ib[g][i][j] += m[q] * ((if i == j { rr } else { 0.0 }) - vl[q][i] * vl[q][j]);
                }
            }
        }
        let iinv: Vec<[[f64; 3]; 3]> = ib.iter().map(|b| {
            let tr = b[0][0] + b[1][1] + b[2][2];
            let e = 1e-6 * tr.max(1e-6);
            let mut a = *b;
            for k in 0..3 {
                a[k][k] += e;
            }
            inv3(&a)
        }).collect();
        let reg = Self::g(slf, "reg")?;
        let exts = PyList::empty_bound(py);
        for (i, d) in Self::g(slf, "devs")?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? && super::actuator::act_is_actuator(slf.as_any(), i)? {
                let dir: i64 = dget(&d, "dir", 2)?.call_method0("__int__")?.extract()?;
                let jp = joint_pairs(dir.rem_euclid(6) as usize);
                let rows: Vec<i64> = jp.iter().map(|p| (8 * i + p.0) as i64).collect();
                let face: Vec<V3> = rows.iter().map(|&r| pos[r as usize]).collect();
                let mut nb = sub(mean8(&face), mean8(&pos[8 * i..8 * i + 8]));
                let l = norm(nb).max(1e-9);
                nb = [nb[0] / l, nb[1] / l, nb[2] / l];
                let e0: f64 = d.get_item("state")?.call_method1("get", ("ext", 0.0))?.extract()?;
                exts.append((i, iarr(py, rows, &[4])?, a1(py, nb), e0))?;
            }
        }
        // アンカーのある部分は固定（動かない）。固定どうしをつなぐアクチュエーターは動かせない
        let mut fixed = vec![false; gn];
        for a in iv(&Self::anchor_index(slf)?)? {
            fixed[gid[a as usize]] = true;
        }
        let (mut ra, mut rbv, mut av, mut bvv, mut cv, mut hk) = (vec![], vec![], vec![], vec![], vec![], vec![]);
        let devs = PyList::empty_bound(py);
        let mut applied = vec![];
        for r in Self::g(slf, "rotors")?.iter()? {
            let r = r?;
            let ri: usize = r.get_item("a")?.extract()?;
            let rj: usize = r.get_item("b")?.extract()?;
            let (a, b) = (gid[ri], gid[rj]);
            if a == b || (fixed[a] && fixed[b]) {
                continue;                // 輪になっていて回せない（固定されたまま）
            }
            let d: usize = r.get_item("d")?.extract()?;
            ra.push(a as i64);
            rbv.push(b as i64);
            for (ca, cb) in joint_pairs(d) {
                av.push((8 * ri + ca) as i64);
                bvv.push((8 * rj + cb) as i64);
            }
            cv.extend((8 * ri..8 * ri + 8).map(|x| x as i64));
            let dv = r.get_item("dev")?;
            applied.push(dv.get_item("state")?.call_method1("get", ("angle", 0.0))?.extract::<f64>()?);
            match super::actuator::act_hinge_of(&dv, Some(&reg))? {
                Some((p, q)) => hk.extend([p as i64, q as i64]),
                None => hk.extend([-1, -1]),
            }
            devs.append(dv)?;
        }
        let nr = ra.len();
        let nv = vg.len();
        let art = PyDict::new_bound(py);
        art.set_item("version", Self::ver(slf))?;
        art.set_item("n", Self::n(slf)?)?;
        art.set_item("gid", iarr(py, gid.iter().map(|x| *x as i64).collect(), &[gid.len()])?)?;
        art.set_item("vg", iarr(py, vg.iter().map(|x| *x as i64).collect(), &[nv])?)?;
        art.set_item("G", gn)?;
        art.set_item("vl0", a3(py, &vl)?)?;
        art.set_item("vl", a3(py, &vl)?)?;
        let invm: Vec<f64> = mm.iter().map(|m| 1.0 / m).collect();
        art.set_item("invM", arr(py, invm.clone(), &[gn])?)?;
        let flat = |v: &[[[f64; 3]; 3]]| -> Vec<f64> { v.iter().flat_map(|m| m.iter().flatten().copied().collect::<Vec<_>>()).collect() };
        art.set_item("IinvB", arr(py, flat(&iinv), &[gn, 3, 3])?)?;
        art.set_item("Ib", arr(py, flat(&ib), &[gn, 3, 3])?)?;
        art.set_item("X", a3(py, &x)?)?;
        art.set_item("R", arr(py, flat(&vec![eye(); gn]), &[gn, 3, 3])?)?;
        let ext_applied: Vec<f64> = exts.iter().map(|e| e.get_item(3)?.extract::<f64>()).collect::<R<_>>()?;
        let ne = ext_applied.len();
        art.set_item("exts", &exts)?;
        art.set_item("ra", iarr(py, ra, &[nr])?)?;
        art.set_item("rb", iarr(py, rbv, &[nr])?)?;
        art.set_item("A", iarr(py, av, &[nr, 4])?)?;
        art.set_item("B", iarr(py, bvv, &[nr, 4])?)?;
        art.set_item("C", iarr(py, cv, &[nr, 8])?)?;
        art.set_item("devs", &devs)?;
        art.set_item("applied", arr(py, applied, &[nr])?)?;
        art.set_item("ext_applied", arr(py, ext_applied, &[ne])?)?;
        art.set_item("gv", iarr(py, (0..nv as i64).collect(), &[nv])?)?;
        art.set_item("HK", iarr(py, hk, &[nr, 2])?)?;
        art.set_item("fixed", barr(py, fixed.clone()))?;
        if fixed.iter().any(|x| *x) {
            // 固定の部分は質量・慣性を無限大として解く（関節は動く側だけを動かす）。今の姿勢をそのまま保つ
            art.set_item("invM_s", arr(py, invm.iter().zip(&fixed).map(|(v, f)| if *f { 0.0 } else { *v }).collect(), &[gn])?)?;
            let z: Vec<[[f64; 3]; 3]> = iinv.iter().zip(&fixed).map(|(v, f)| if *f { [[0.0; 3]; 3] } else { *v }).collect();
            art.set_item("IinvB_s", arr(py, flat(&z), &[gn, 3, 3])?)?;
            art.set_item("Xfix", a3(py, &x)?)?;
        }
        Self::set(slf, "art", &art)?;
        art.set_item("nat", Self::_native_art_topo(slf)?)
    }

    fn enter_art(slf: &Bound<'_, Self>) -> R<()> {
        Self::_art_build(slf)?;
        let py = slf.py();
        let mut b = slf.borrow_mut();
        b.mode = "art".into();
        b.rb = py.None();
        b.want_soft = false;
        Ok(())
    }

    // ------------------------------------------------------------------ 軽量化 2：レール
    fn _rails_ok(slf: &Bound<'_, Self>, world: &Bound<'_, PyAny>, dist: f64) -> R<bool> {
        let py = slf.py();
        let rb = Self::g(slf, "rb")?;
        let x = if rb.is_none() { Self::center_of_mass(slf)? } else { rb.get_item("x")? };
        if world.call_method1("player_distance", (&x,))?.extract::<f64>()? < dist {
            return Ok(false);
        }
        if fv(&Self::g(slf, "force")?)?.iter().any(|v| *v != 0.0) {
            return Ok(false);
        }
        // 地面の近くでは当たり判定が必要なのでレールに乗せない
        let pl = world.call_method1("nearest_planet", (&x,))?;
        if !pl.is_none() {
            let xv = v3(&x)?;
            let alt: f64 = pl.call_method1("sdf", (arr(py, xv.to_vec(), &[1, 3])?,))?.get_item(0)?.extract()?;
            let ext: f64 = if rb.is_none() { 10.0 } else { rb.get_item("ext")?.extract()? };
            if alt < ext + 15.0 {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn enter_rails(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        slf.borrow_mut().mode = "rails".into();
        let rm = Self::rigid_motion(slf)?;
        let rb = Self::g(slf, "rb")?;
        rb.set_item("x", a1(py, rm.x))?;
        rb.set_item("v", a1(py, rm.v))?;
        rb.set_item("w", a1(py, rm.w))?;
        Self::_rails_sync(slf)
    }
    fn leave_rails(slf: &Bound<'_, Self>) -> R<()> {
        Self::_rails_sync(slf)?;
        slf.borrow_mut().mode = "rigid".into();
        Ok(())
    }
    /// 重心は重力だけで（速度ベルレ法）、回転は一定の角速度で進める
    #[pyo3(signature = (dt, gravity_fn, sub=4))]
    fn rails_step(slf: &Bound<'_, Self>, dt: f64, gravity_fn: &Bound<'_, PyAny>, sub: i64) -> R<()> {
        let py = slf.py();
        let rb = Self::g(slf, "rb")?;
        let h = dt / sub as f64;
        let mut x = v3(&rb.get_item("x")?)?;
        let mut v = v3(&rb.get_item("v")?)?;
        let grav = |x: V3| -> R<V3> { v3(&gravity_fn.call1((arr(py, x.to_vec(), &[1, 3])?,))?.get_item(0)?) };
        let mut a = grav(x)?;
        for _ in 0..sub {
            v = add(v, scl(a, h * 0.5));
            x = add(x, scl(v, h));
            a = grav(x)?;
            v = add(v, scl(a, h * 0.5));
        }
        rb.set_item("x", a1(py, x))?;
        rb.set_item("v", a1(py, v))?;
        let w = v3(&rb.get_item("w")?)?;
        let wn = norm(w);
        if wn > 1e-9 {
            let k = [w[0] / wn, w[1] / wn, w[2] / wn];
            let th = wn * dt;
            let kx = [[0.0, -k[2], k[1]], [k[2], 0.0, -k[0]], [-k[1], k[0], 0.0]];
            let kk = matmul(&kx, &kx);
            let mut rw = eye();
            for i in 0..3 {
                for j in 0..3 {
                    rw[i][j] = rw[i][j] + th.sin() * kx[i][j] + (1.0 - th.cos()) * kk[i][j];
                }
            }
            let r0 = m3(&rb.get_item("R")?)?;
            rb.set_item("R", mat(py, &matmul(&rw, &r0))?)?;
        }
        Self::_rails_sync(slf)
    }
    fn _rails_sync(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let rb = Self::g(slf, "rb")?;
        let r = m3(&rb.get_item("R")?)?;
        let x = v3(&rb.get_item("x")?)?;
        let v = v3(&rb.get_item("v")?)?;
        let w = v3(&rb.get_item("w")?)?;
        let loc = v3s(&rb.get_item("local")?)?;
        let rr: Vec<V3> = loc.iter().map(|l| mul_t(*l, &r)).collect();
        Self::set(slf, "pos", a3(py, &rr.iter().map(|q| add(x, *q)).collect::<Vec<_>>())?)?;
        Self::set(slf, "vel", a3(py, &rr.iter().map(|q| add(v, cross(w, *q))).collect::<Vec<_>>())?)
    }

    // ------------------------------------------------------------------ 外から位置を決める（拡張が、動く物体に固定した構築物を一緒に動かす）
    /// 座標系 (R, x) に今の形のまま固定する（形が変わったときだけ取り直す）。物理計算は止まる（mode="rails"）
    fn hold_in_frame(slf: &Bound<'_, Self>, r: &Bound<'_, PyAny>, x: &Bound<'_, PyAny>, v: &Bound<'_, PyAny>, w: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let f = opt(slf.as_any(), "_frame_hold")?;
        let fresh = match &f {
            None => true,
            Some(f) => !f.get_item("ver")?.eq(Self::ver(slf))? || slf.borrow().rb.is_none(py) || !Self::mode_is(slf, &["rails"]),
        };
        if fresh {
            let rm = m3(r)?;
            let xv = v3(x)?;
            let local: Vec<V3> = Self::pos_v(slf)?.iter().map(|p| mul_r(sub(*p, xv), &rm)).collect();
            let fh = PyDict::new_bound(py);
            fh.set_item("ver", Self::ver(slf))?;
            Self::set(slf, "_frame_hold", fh)?;
            let rb = PyDict::new_bound(py);
            rb.set_item("ext", local.iter().map(|l| norm(*l)).fold(f64::NEG_INFINITY, f64::max))?;
            rb.set_item("local", a3(py, &local)?)?;
            rb.set_item("R", mat(py, &rm)?)?;
            rb.set_item("x", a1(py, xv))?;
            rb.set_item("v", a1(py, v3(v)?))?;
            rb.set_item("w", a1(py, v3(w)?))?;
            rb.set_item("Ib", mat(py, &eye())?)?;
            let mut b = slf.borrow_mut();
            b.mode = "rails".into();
            b.rb = rb.into_any().unbind();
            b.rb_epoch += 1;
        }
        Ok(())
    }
    /// 固定した座標系が (R, x, 速度 v, 角速度 w) に動いた：一緒に動かす
    fn move_with_frame(slf: &Bound<'_, Self>, r: &Bound<'_, PyAny>, x: &Bound<'_, PyAny>, v: &Bound<'_, PyAny>, w: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        Self::hold_in_frame(slf, r, x, v, w)?;
        let rb = Self::g(slf, "rb")?;
        rb.set_item("R", mat(py, &m3(r)?)?)?;
        rb.set_item("x", a1(py, v3(x)?))?;
        rb.set_item("v", a1(py, v3(v)?))?;
        rb.set_item("w", a1(py, v3(w)?))?;
        Self::_rails_sync(slf)?;
        let mut b = slf.borrow_mut();
        b.asleep = false;
        b.sleep_epoch += 1;
        Ok(())
    }
    /// 固定をやめて、ふつうの物理計算に戻す
    fn release_frame(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        Self::set(slf, "_frame_hold", py.None())?;
        let mut b = slf.borrow_mut();
        if b.mode == "rails" {
            b.mode = "soft".into();
            b.rb = py.None();
        }
        Ok(())
    }
    /// 描画用：剛体・レール・静止中なら (R, x, 版)。ブロックごとに動いている間は None
    fn render_transform<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let rb = Self::g(slf, "rb")?;
        if Self::mode_is(slf, &["rigid", "rails"]) && !rb.is_none() {
            let tag: PyObject = ("rb", slf.borrow().rb_epoch, Self::ver(slf)).into_py(py);
            return Ok(PyTuple::new_bound(py, [rb.get_item("R")?, rb.get_item("x")?, tag.into_bound(py)]).into_any());
        }
        if slf.borrow().asleep {
            let ep = slf.borrow().sleep_epoch;
            let ok = match opt(slf.as_any(), "_sleep_anchor")? {
                Some(a) => a.get_item(0)?.eq(ep)?,
                None => false,
            };
            if !ok {
                let p0 = Self::pos_v(slf)?.first().copied().unwrap_or([0.0; 3]);
                Self::set(slf, "_sleep_anchor", (ep, a1(py, p0)))?;
            }
            let anc = Self::g(slf, "_sleep_anchor")?.get_item(1)?;
            let tag: PyObject = ("sleep", ep, Self::ver(slf)).into_py(py);
            return Ok(PyTuple::new_bound(py, [mat(py, &eye())?, anc, tag.into_bound(py)]).into_any());
        }
        Ok(py.None().into_bound(py))
    }

    // ------------------------------------------------------------------ 編集
    /// positions: (8,3) 新しいボクセルの頂点位置。隣接ボクセルと近ければ結合する
    fn add_voxel<'py>(slf: &Bound<'py, Self>, cell: &Bound<'py, PyAny>, mat_: &Bound<'py, PyAny>, dev: &Bound<'py, PyAny>,
                      positions: &Bound<'py, PyAny>) -> R<PyObject> {
        let py = slf.py();
        let mut c = [0i64; 3];
        for (q, x) in cell.iter()?.enumerate().take(3) {
            c[q] = x?.call_method0("__int__")?.extract()?;
        }
        let index = Self::g(slf, "index")?;
        if index.contains(tup3(py, c))? {
            return Ok(false.into_py(py));
        }
        Self::g(slf, "cells")?.call_method1("append", (tup3(py, c),))?;
        Self::g(slf, "mats")?.call_method1("append", (mat_,))?;
        let d: PyObject = if dev.is_truthy()? { Self::_init_device(slf, dev)?.into_any().unbind() } else { py.None() };
        Self::g(slf, "devs")?.call_method1("append", (d,))?;
        let pnew = v3s(positions)?;
        let mut pos = Self::pos_v(slf)?;
        let mut vel = Self::vel_v(slf)?;
        pos.extend(pnew.iter().copied());
        let n = Self::n(slf)?;
        let mut v = [0.0; 3];
        if n > 1 {
            let ctr = mean8(&pnew);
            let mut best = (f64::INFINITY, 0usize);
            for j in 0..n - 1 {
                let dd = norm(sub(mean8(&pos[8 * j..8 * j + 8]), ctr));
                if dd < best.0 {
                    best = (dd, j);
                }
            }
            v = mean8(&vel[8 * best.1..8 * best.1 + 8]);
        }
        vel.extend(std::iter::repeat(v).take(8));
        Self::set(slf, "pos", a3(py, &pos)?)?;
        Self::set(slf, "vel", a3(py, &vel)?)?;
        let size = Self::sz(slf);
        let mut rest = fv(&Self::g(slf, "rest")?)?;
        for cb in CORNER_BITS.iter() {
            rest.extend(cb.iter().map(|b| (b - 0.5) * size));
        }
        Self::set(slf, "rest", arr(py, rest, &[n, 8, 3])?)?;
        let mut asc = fv(&Self::g(slf, "act_scale")?)?;
        asc.extend([1.0, 1.0, 1.0]);
        Self::set(slf, "act_scale", arr(py, asc, &[n, 3])?)?;
        let i = n - 1;
        let links = Self::g(slf, "links")?;
        for d in 0..6 {
            let nb = [c[0] + DIRS[d][0], c[1] + DIRS[d][1], c[2] + DIRS[d][2]];
            let j = match Self::idx_of(&index, tup3(py, nb).as_any())? {
                Some(j) => j,
                None => continue,
            };
            let mut dist = f64::NEG_INFINITY;
            for (ca, cb) in joint_pairs(d) {
                dist = dist.max(norm(sub(pnew[ca], pos[8 * j + cb])));
            }
            if dist < size * 0.35 {
                links.call_method1("add", (link_key_c(py, c, nb),))?;
            }
        }
        Self::set(slf, "force", zeros(py, &[8 * n, 3])?)?;
        Self::rebuild(slf)?;
        Ok(i.into_py(py))
    }

    fn remove_voxels<'py>(slf: &Bound<'py, Self>, idxs: &Bound<'py, PyAny>) -> R<Bound<'py, PyList>> {
        let py = slf.py();
        let n = Self::n(slf)?;
        let mut keep = vec![true; n];
        let cl = Self::g(slf, "cells")?;
        let removed = PyList::empty_bound(py);
        let idv: Vec<i64> = idxs.iter()?.map(|x| x?.extract::<i64>()).collect::<R<_>>()?;
        for &i in &idv {
            let i = if i < 0 { i + n as i64 } else { i } as usize;
            keep[i] = false;
        }
        for &i in &idv {
            removed.append(cl.get_item(i)?)?;
        }
        Self::apply_keep(slf, &keep)?;
        let rs = PySet::new_bound(py, &removed.iter().collect::<Vec<_>>())?;
        let nl = PySet::empty_bound(py)?;
        for k in Self::g(slf, "links")?.iter()? {
            let k = k?;
            if !rs.contains(k.get_item(0)?)? && !rs.contains(k.get_item(1)?)? {
                nl.add(k)?;
            }
        }
        Self::set(slf, "links", nl)?;
        Self::rebuild(slf)?;
        Ok(removed)
    }
    fn _apply_keep(slf: &Bound<'_, Self>, keep: &Bound<'_, PyAny>) -> R<()> {
        let k = bv(keep)?;
        Self::apply_keep(slf, &k)
    }

    /// 結合で繋がっているボクセル群に分ける
    fn components(slf: &Bound<'_, Self>) -> R<Vec<Vec<usize>>> {
        let n = Self::n(slf)?;
        let mut parent: Vec<usize> = (0..n).collect();
        let index = Self::g(slf, "index")?;
        for k in Self::link_list(slf, true)? {
            let (a, b) = (Self::idx_of(&index, &k.get_item(0)?)?, Self::idx_of(&index, &k.get_item(1)?)?);
            if let (Some(a), Some(b)) = (a, b) {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb {
                    parent[ra] = rb;
                }
            }
        }
        let mut order: Vec<usize> = vec![];
        let mut groups: std::collections::HashMap<usize, Vec<usize>> = Default::default();
        for i in 0..n {
            let r = find(&mut parent, i);
            groups.entry(r).or_insert_with(|| { order.push(r); vec![] }).push(i);
        }
        Ok(order.into_iter().map(|r| groups.remove(&r).unwrap()).collect())
    }

    /// ちぎれた部分を別の構築物に分離する。新しい構築物のリストを返す
    fn split<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyList>> {
        let py = slf.py();
        let mut comps = Self::components(slf)?;
        let out = PyList::empty_bound(py);
        if comps.len() <= 1 {
            return Ok(out);
        }
        comps.sort_by(|a, b| b.len().cmp(&a.len()));
        let cells: Vec<Bound<'py, PyAny>> = Self::g(slf, "cells")?.iter()?.collect::<R<_>>()?;
        let mats: Vec<Bound<'py, PyAny>> = Self::g(slf, "mats")?.iter()?.collect::<R<_>>()?;
        let devs: Vec<Bound<'py, PyAny>> = Self::g(slf, "devs")?.iter()?.collect::<R<_>>()?;
        let pos = Self::pos_v(slf)?;
        let vel = Self::vel_v(slf)?;
        let rest = fv(&Self::g(slf, "rest")?)?;
        let asc = fv(&Self::g(slf, "act_scale")?)?;
        let soft_ok = truthy(slf.as_any(), "soft_ok")?;
        let name = format!("{}の破片", Self::g(slf, "name")?.str()?);
        let links: Vec<Bound<'py, PyAny>> = Self::g(slf, "links")?.iter()?.collect::<R<_>>()?;
        let extras = Self::g(slf, "extras")?;
        for comp in &comps[1..] {
            let kw = PyDict::new_bound(py);
            kw.set_item("size", Self::sz(slf))?;
            kw.set_item("name", &name)?;
            let sub_ = py.get_type_bound::<Construct>().call((Self::g(slf, "reg")?,), Some(&kw))?;
            let sb = sub_.downcast::<Construct>()?;
            if soft_ok {
                sub_.setattr("soft_ok", true)?;        // 破片も柔らかいまま（完全剛体モードの中の見せ場）
            }
            let sc = PyList::new_bound(py, comp.iter().map(|&i| cells[i].clone()));
            Self::set(sb, "cells", &sc)?;
            Self::set(sb, "mats", PyList::new_bound(py, comp.iter().map(|&i| mats[i].clone())))?;
            Self::set(sb, "devs", PyList::new_bound(py, comp.iter().map(|&i| devs[i].clone())))?;
            let pv: Vec<V3> = comp.iter().flat_map(|&i| pos[8 * i..8 * i + 8].to_vec()).collect();
            let vv: Vec<V3> = comp.iter().flat_map(|&i| vel[8 * i..8 * i + 8].to_vec()).collect();
            Self::set(sb, "pos", a3(py, &pv)?)?;
            Self::set(sb, "vel", a3(py, &vv)?)?;
            let k = comp.len();
            Self::set(sb, "rest", arr(py, comp.iter().flat_map(|&i| rest[24 * i..24 * i + 24].to_vec()).collect(), &[k, 8, 3])?)?;
            Self::set(sb, "act_scale", arr(py, comp.iter().flat_map(|&i| asc[3 * i..3 * i + 3].to_vec()).collect(), &[k, 3])?)?;
            let cs = PySet::new_bound(py, &sc.iter().collect::<Vec<_>>())?;
            let sl = PySet::empty_bound(py)?;
            for l in &links {
                if cs.contains(l.get_item(0)?)? && cs.contains(l.get_item(1)?)? {
                    sl.add(l)?;
                }
            }
            Self::set(sb, "links", sl)?;
            let se = PyDict::new_bound(py);
            for (kk, v) in extras.downcast::<PyDict>()?.iter() {
                if cs.contains(kk.get_item(0)?)? && cs.contains(kk.get_item(1)?)? {
                    se.set_item(kk, v)?;
                }
            }
            Self::set(sb, "extras", se)?;
            Self::set(sb, "force", zeros(py, &[8 * k, 3])?)?;
            Self::rebuild(sb)?;
            out.append(sub_)?;
        }
        let mut keep = vec![false; Self::n(slf)?];
        for &i in &comps[0] {
            keep[i] = true;
        }
        Self::apply_keep(slf, &keep)?;
        let cs = PySet::new_bound(py, &Self::g(slf, "cells")?.iter()?.collect::<R<Vec<_>>>()?)?;
        let nl = PySet::empty_bound(py)?;
        for l in &links {
            if cs.contains(l.get_item(0)?)? && cs.contains(l.get_item(1)?)? {
                nl.add(l)?;
            }
        }
        Self::set(slf, "links", nl)?;
        Self::rebuild(slf)?;
        Ok(out)
    }

    /// 別の構築物を取り込む。cell_map: other のセル → このセルの対応。
    /// xform: {ボクセル番号: (頂点の並べ替え perm, 旧ローカル→新ローカルの回転 M)}（90°単位で向きを揃える時）
    #[pyo3(signature = (other, cell_map, xform=None))]
    fn absorb(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>, cell_map: &Bound<'_, PyAny>, xform: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        let index = Self::g(slf, "index")?;
        for c in other.getattr("cells")?.iter()? {
            if index.contains(cell_map.get_item(c?)?)? {
                return Err(PyValueError::new_err("セルが重なっています"));
            }
        }
        if let Some(x) = xform {
            if x.is_truthy()? {
                other.call_method1("_reorient", (x,))?;
            }
        }
        let mapped: Vec<Bound<'_, PyAny>> = other.getattr("cells")?.iter()?.map(|c| cell_map.get_item(c?)).collect::<R<_>>()?;
        Self::g(slf, "cells")?.call_method1("extend", (PyList::new_bound(py, mapped),))?;
        Self::g(slf, "mats")?.call_method1("extend", (other.getattr("mats")?,))?;
        Self::g(slf, "devs")?.call_method1("extend", (other.getattr("devs")?,))?;
        let n = Self::n(slf)?;
        let cat = |k: &str, w: &[usize]| -> R<()> {
            let mut a = fv(&Self::g(slf, k)?)?;
            a.extend(fv(&other.getattr(k)?)?);
            let mut sh = vec![n * if k == "pos" || k == "vel" { 8 } else { 1 }];
            sh.extend_from_slice(w);
            Self::set(slf, k, arr(py, a, &sh)?)
        };
        cat("pos", &[3])?;
        cat("vel", &[3])?;
        cat("rest", &[8, 3])?;
        cat("act_scale", &[3])?;
        let links = Self::g(slf, "links")?;
        for k in other.getattr("links")?.iter()? {
            let k = k?;
            let (a, b) = (cell_of(&cell_map.get_item(k.get_item(0)?)?)?, cell_of(&cell_map.get_item(k.get_item(1)?)?)?);
            links.call_method1("add", (link_key_c(py, a, b),))?;
        }
        let extras = Self::g(slf, "extras")?;
        for (k, v) in other.getattr("extras")?.downcast::<PyDict>()?.iter() {
            let (a, b) = (cell_of(&cell_map.get_item(k.get_item(0)?)?)?, cell_of(&cell_map.get_item(k.get_item(1)?)?)?);
            extras.set_item(link_key_c(py, a, b), PyList::new_bound(py, v.iter()?.collect::<R<Vec<_>>>()?))?;
        }
        let st = Self::g(slf, "storage")?;
        for (k, v) in other.getattr("storage")?.downcast::<PyDict>()?.iter() {
            let cur = st.call_method1("get", (&k, 0.0))?;
            st.set_item(&k, cur.add(v)?)?;
        }
        let ca = Self::g(slf, "cavity_air")?;
        for (k, v) in other.getattr("cavity_air")?.downcast::<PyDict>()?.iter() {
            ca.set_item(cell_map.call_method1("get", (&k, &k))?, v)?;
        }
        Self::set(slf, "force", zeros(py, &[8 * n, 3])?)?;
        Self::rebuild(slf)
    }

    /// ブロック keep の今の位置から格子の置き方を求め、残りのブロックをその格子の位置に置き直す
    fn snap_to_lattice(slf: &Bound<'_, Self>, keep: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let mut keep: Vec<i64> = iv(keep)?;
        let n = Self::n(slf)?;
        if keep.is_empty() || keep.len() == n {
            return Ok(());
        }
        let ks: std::collections::BTreeSet<i64> = keep.iter().copied().collect();
        let anc: Vec<i64> = iv(&Self::anchor_index(slf)?)?.into_iter().filter(|a| ks.contains(a)).collect::<std::collections::BTreeSet<_>>()
            .into_iter().collect();
        let fit: Vec<i64> = if anc.is_empty() { keep.clone() } else {
            // 固定されている部分（アクチュエーターの先で回っている所は除く）で格子を求める
            let mut s = std::collections::BTreeSet::new();
            for a in &anc {
                s.extend(iv(&Self::part_of(slf, *a as usize)?)?);
            }
            s.into_iter().collect()
        };
        let (r, t) = Self::fit(slf, Some(&fit.iter().map(|x| *x as usize).collect::<Vec<_>>()))?;
        let cells = Self::cells_v(slf)?;
        keep.retain(|_| true);
        let span = keep.iter().map(|&k| cells[k as usize].iter().map(|x| x.abs()).max().unwrap()).max().unwrap() as f64 + 50.0;
        let others: Vec<usize> = (0..n).filter(|i| !ks.contains(&(*i as i64)))
            .filter(|&i| (cells[i].iter().map(|x| x.abs()).max().unwrap() as f64) < span).collect();
        if others.is_empty() {
            return Ok(());
        }
        let size = Self::sz(slf);
        let pos_o = Self::g(slf, "pos")?;
        let mut pos = v3s(&pos_o)?;
        let vel_o = Self::g(slf, "vel")?;
        let mut vel = v3s(&vel_o)?;
        for &i in &others {
            for (c, cb) in CORNER_BITS.iter().enumerate() {
                let l = [(cells[i][0] as f64 + cb[0]) * size, (cells[i][1] as f64 + cb[1]) * size, (cells[i][2] as f64 + cb[2]) * size];
                pos[8 * i + c] = add(mul_t(l, &r), t);
                vel[8 * i + c] = [0.0; 3];
            }
        }
        write_back(slf, "pos", &pos_o, pos.iter().flatten().copied().collect(), &[8 * n, 3])?;
        write_back(slf, "vel", &vel_o, vel.iter().flatten().copied().collect(), &[8 * n, 3])?;
        if let Some(pr) = opt(slf.as_any(), "_prev")? {
            if pr.len()? == 8 * n {
                let mut p = v3s(&pr)?;
                for &i in &others {
                    for c in 0..8 {
                        p[8 * i + c] = pos[8 * i + c];
                    }
                }
                write_back(slf, "_prev", &pr, p.iter().flatten().copied().collect(), &[8 * n, 3])?;
            }
        }
        let _ = py;
        Self::wake(slf, true)
    }

    /// ボクセルの頂点番号と向きを、指定の回転で並べ替える（格子に揃えるため）
    fn _reorient(slf: &Bound<'_, Self>, xform: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let pos_o = Self::g(slf, "pos")?;
        let vel_o = Self::g(slf, "vel")?;
        let rest_o = Self::g(slf, "rest")?;
        let asc_o = Self::g(slf, "act_scale")?;
        let mut pos = v3s(&pos_o)?;
        let mut vel = v3s(&vel_o)?;
        let mut rest = v3s(&rest_o)?;
        let mut asc = v3s(&asc_o)?;
        let cells = Self::g(slf, "cells")?;
        let devs = Self::g(slf, "devs")?;
        let pmap = PyDict::new_bound(py);
        for (k, v) in xform.downcast::<PyDict>()?.iter() {
            let k: usize = k.extract()?;
            let perm: Vec<usize> = iv(&v.get_item(0)?)?.into_iter().map(|x| x as usize).collect();
            let m = m3(&v.get_item(1)?)?;
            let (p0, v0, r0) = (pos[8 * k..8 * k + 8].to_vec(), vel[8 * k..8 * k + 8].to_vec(), rest[8 * k..8 * k + 8].to_vec());
            for old in 0..8 {
                pos[8 * k + perm[old]] = p0[old];
                vel[8 * k + perm[old]] = v0[old];
                rest[8 * k + perm[old]] = mul_t(r0[old], &m);
            }
            let am = [[m[0][0].abs(), m[0][1].abs(), m[0][2].abs()], [m[1][0].abs(), m[1][1].abs(), m[1][2].abs()],
                      [m[2][0].abs(), m[2][1].abs(), m[2][2].abs()]];
            asc[k] = mul_t(asc[k], &am);
            pmap.set_item(cells.get_item(k)?, v.get_item(0)?)?;
            let d = devs.get_item(k)?;
            if d.is_truthy()? {
                let di: usize = d.get_item("dir")?.extract()?;
                let dv = DIRS[di];
                let r = mul_t([dv[0] as f64, dv[1] as f64, dv[2] as f64], &m);
                let t = [r[0].round_ties_even() as i64, r[1].round_ties_even() as i64, r[2].round_ties_even() as i64];
                d.set_item("dir", DIRS.iter().position(|x| *x == t).map(|x| x as i64).unwrap_or(-1))?;
            }
        }
        let n = pos.len();
        write_back(slf, "pos", &pos_o, pos.iter().flatten().copied().collect(), &[n, 3])?;
        write_back(slf, "vel", &vel_o, vel.iter().flatten().copied().collect(), &[n, 3])?;
        write_back(slf, "rest", &rest_o, rest.iter().flatten().copied().collect(), &[n / 8, 8, 3])?;
        write_back(slf, "act_scale", &asc_o, asc.iter().flatten().copied().collect(), &[asc.len(), 3])?;
        let extras = Self::g(slf, "extras")?;
        let items: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)> = extras.downcast::<PyDict>()?.iter().collect();
        for (key, lst) in items {
            let pa = pmap.get_item(key.get_item(0)?)?;
            let pb = pmap.get_item(key.get_item(1)?)?;
            let nl = PyList::empty_bound(py);
            for e in lst.iter()? {
                let e = e?;
                let (a, b, r) = (e.get_item(0)?, e.get_item(1)?, e.get_item(2)?);
                let a2: PyObject = match &pa { Some(p) => p.get_item(&a)?.call_method0("__int__")?.unbind(), None => a.unbind() };
                let b2: PyObject = match &pb { Some(p) => p.get_item(&b)?.call_method0("__int__")?.unbind(), None => b.unbind() };
                nl.append((a2, b2, r))?;
            }
            extras.set_item(key, nl)?;
        }
        Ok(())
    }

    /// 再結合する素材同士が近づいていたら結合を追加
    fn try_rebond(slf: &Bound<'_, Self>) -> R<bool> {
        let py = slf.py();
        let rbd = bv(&Self::g(slf, "v_rebond")?)?;
        if !rbd.iter().any(|x| *x) {
            return Ok(false);
        }
        let idx: Vec<usize> = (0..rbd.len()).filter(|&i| rbd[i]).collect();
        let ctr = Self::centers(slf)?;
        let sel: Vec<V3> = idx.iter().map(|&i| ctr[i]).collect();
        let r = core(py)?.call_method1("pairs_within", (a3(py, &sel)?, Self::sz(slf) * 1.15))?;
        let (pa, pb) = (iv(&r.get_item(0)?)?, iv(&r.get_item(1)?)?);
        let cells = Self::cells_v(slf)?;
        let links = Self::g(slf, "links")?;
        let extras = Self::g(slf, "extras")?;
        let index = Self::g(slf, "index")?;
        let pos = Self::pos_v(slf)?;
        let rdist = fv(&Self::g(slf, "v_rebond_dist")?)?;
        let mut added = false;
        for (a, b) in pa.iter().zip(&pb) {
            let (ia, ib) = (idx[*a as usize], idx[*b as usize]);
            let key = link_key_c(py, cells[ia], cells[ib]);
            if links.contains(&key)? || extras.contains(&key)? {
                continue;
            }
            let ia: usize = index.get_item(key.get_item(0)?)?.extract()?;
            let ib: usize = index.get_item(key.get_item(1)?)?.extract()?;
            let mut dd = vec![];
            for ca in 0..8 {
                for cb in 0..8 {
                    dd.push((norm(sub(pos[8 * ia + ca], pos[8 * ib + cb])), ca * 8 + cb));
                }
            }
            let lim = rdist[ia].max(rdist[ib]);
            let mut order = dd.clone();
            order.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
            let lst = PyList::empty_bound(py);
            for (dv, o) in order.iter().take(4) {
                if *dv < lim {
                    lst.append((o / 8, o % 8, dv * 0.5))?;
                }
            }
            if !lst.is_empty() {
                extras.set_item(&key, lst)?;
                added = true;
            }
        }
        if added {
            Self::rebuild(slf)?;
        }
        Ok(added)
    }

    // ------------------------------------------------------------------ 保存
    /// ブロック i と同じ「部分」（アクチュエーターの出力面の結合をたどらずにつながっているブロック）の番号の配列
    fn part_of<'py>(slf: &Bound<'py, Self>, i: usize) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let key = Self::vkey(slf)?;
        let label = match Self::cached(slf, "_part_cache", key.as_any())? {
            Some(c) => iv(&c.get_item(1)?)?,
            None => {
                let n = Self::n(slf)?;
                let mut parent: Vec<usize> = (0..n).collect();
                let joints = Self::rotor_keys(slf)?;
                let index = Self::g(slf, "index")?;
                for k in Self::link_list(slf, true)? {
                    if joints.contains(&k)? {
                        continue;
                    }
                    if let (Some(a), Some(b)) = (Self::idx_of(&index, &k.get_item(0)?)?, Self::idx_of(&index, &k.get_item(1)?)?) {
                        let rb = find(&mut parent, b);
                        let ra = find(&mut parent, a);
                        parent[ra] = rb;
                    }
                }
                let label: Vec<i64> = (0..n).map(|k| find(&mut parent, k) as i64).collect();
                Self::set(slf, "_part_cache", (key, iarr(py, label.clone(), &[n])?))?;
                label
            }
        };
        let li = label[i];
        let out: Vec<i64> = (0..label.len()).filter(|&k| label[k] == li).map(|k| k as i64).collect();
        let m = out.len();
        iarr(py, out, &[m])
    }

    /// 現在の頂点位置にもっとも合う剛体変換 (R, t)：pos ≒ R @ local + t。idx：その番号のブロックだけで合わせる
    #[pyo3(signature = (idx=None))]
    fn rigid_fit<'py>(slf: &Bound<'py, Self>, idx: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let sel = match idx {
            Some(i) if !i.is_none() => Some(iv(i)?.into_iter().map(|x| x as usize).collect::<Vec<_>>()),
            _ => None,
        };
        let (r, t) = Self::fit(slf, sel.as_deref())?;
        Ok(PyTuple::new_bound(py, [mat(py, &r)?, a1(py, t)]).into_any())
    }

    fn angular_velocity<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let m = Self::mass_v(slf)?;
        let pos = Self::pos_v(slf)?;
        let vel = Self::vel_v(slf)?;
        let com = wmean(&pos, &m);
        let vc = wmean(&vel, &m);
        let mut lm = [0.0; 3];
        let mut ii = [[0.0; 3]; 3];
        for q in 0..pos.len() {
            let r = sub(pos[q], com);
            let c = cross(r, sub(vel[q], vc));
            let rr = dot(r, r);
            for k in 0..3 {
                lm[k] += c[k] * m[q];
                for j in 0..3 {
                    ii[k][j] += m[q] * ((if k == j { rr } else { 0.0 }) - r[k] * r[j]);
                }
            }
        }
        for k in 0..3 {
            ii[k][k] += 1e-6;
        }
        Ok(a1(py, solve3(&ii, lm).unwrap_or([0.0; 3])))
    }

    fn to_dict<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let o = PyDict::new_bound(py);
        o.set_item("id", Self::g(slf, "id")?)?;
        o.set_item("name", Self::g(slf, "name")?)?;
        o.set_item("voxel_size", Self::sz(slf))?;
        let reg = Self::g(slf, "reg")?;
        let vox = PyList::empty_bound(py);
        let cells = Self::cells_v(slf)?;
        for ((c, m), d) in cells.iter().zip(Self::g(slf, "mats")?.iter()?).zip(Self::g(slf, "devs")?.iter()?) {
            vox.append(PyList::new_bound(py, [c[0].into_py(py), c[1].into_py(py), c[2].into_py(py), m?.unbind(),
                construct_clean_dev(py, &d?, Some(&reg))?]))?;
        }
        o.set_item("voxels", vox)?;
        let ls = PyList::new_bound(py, Self::g(slf, "links")?.iter()?.collect::<R<Vec<_>>>()?);
        ls.sort()?;
        let ll = PyList::empty_bound(py);
        for k in ls.iter() {
            ll.append([PyList::new_bound(py, k.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?),
                       PyList::new_bound(py, k.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?)])?;
        }
        o.set_item("links", ll)?;
        let ex = PyList::empty_bound(py);
        for (k, v) in Self::g(slf, "extras")?.downcast::<PyDict>()?.iter() {
            ex.append(PyList::new_bound(py, [PyList::new_bound(py, k.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?).into_any(),
                PyList::new_bound(py, k.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any(), v]))?;
        }
        o.set_item("extras", ex)?;
        o.set_item("storage", Self::g(slf, "storage")?)?;
        let ca = PyList::empty_bound(py);
        for (k, v) in Self::g(slf, "cavity_air")?.downcast::<PyDict>()?.iter() {
            ca.append(PyList::new_bound(py, [PyList::new_bound(py, k.iter()?.collect::<R<Vec<_>>>()?).into_any(), v]))?;
        }
        o.set_item("cavity_air", ca)?;
        // 現在の形（回転したアーム・伸びたアクチュエーター・変形）を保存
        let (r, t) = Self::fit(slf, None)?;
        let rnd = |x: f64| (x * 1e4).round_ties_even() / 1e4;
        let verts = PyList::empty_bound(py);
        for p in Self::pos_v(slf)? {
            let l = mul_r(sub(p, t), &r);
            verts.append([rnd(l[0]), rnd(l[1]), rnd(l[2])])?;
        }
        o.set_item("vertices", verts)?;
        let rest = v3s(&Self::g(slf, "rest")?)?;
        let rl = PyList::empty_bound(py);
        for blk in rest.chunks_exact(8) {
            rl.append(PyList::new_bound(py, blk.iter().map(|q| [rnd(q[0]), rnd(q[1]), rnd(q[2])].into_py(py))))?;
        }
        o.set_item("rest", rl)?;
        let anc = Self::g(slf, "anchor")?;
        if anc.is_truthy()? {
            o.set_item("anchor", anc.get_item("to")?)?;
        }
        if truthy(slf.as_any(), "soft_ok")? {
            o.set_item("soft_ok", true)?;
        }
        Ok(o)
    }

    fn spawn_dict<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let (r, t) = Self::fit(slf, None)?;
        let o = PyDict::new_bound(py);
        o.set_item("position", t.to_vec())?;
        o.set_item("rotation", r.iter().map(|x| x.to_vec()).collect::<Vec<_>>())?;
        o.set_item("velocity", Self::velocity(slf)?.call_method0("tolist")?)?;
        o.set_item("angular_velocity", Self::angular_velocity(slf)?.call_method0("tolist")?)?;
        Ok(o)
    }

    #[classmethod]
    fn from_dict<'py>(cls: &Bound<'py, PyType>, registry: &Bound<'py, PyAny>, data: &Bound<'py, PyAny>, spawn: &Bound<'py, PyAny>)
        -> R<Bound<'py, PyAny>> {
        let py = cls.py();
        let vox = PyList::empty_bound(py);
        for v in data.get_item("voxels")?.iter()? {
            let v = v?;
            vox.append(((v.get_item(0)?, v.get_item(1)?, v.get_item(2)?), v.get_item(3)?, v.get_item(4)?))?;
        }
        let links: PyObject = if data.contains("links")? {
            let s = PySet::empty_bound(py)?;
            for l in data.get_item("links")?.iter()? {
                let l = l?;
                s.add(construct_link_key(py, &l.get_item(0)?, &l.get_item(1)?)?)?;
            }
            s.into_any().unbind()
        } else { py.None() };
        let zero = PyList::new_bound(py, [0, 0, 0]);
        let kw = PyDict::new_bound(py);
        kw.set_item("size", dget(data, "voxel_size", 1.0)?)?;
        kw.set_item("links", links)?;
        kw.set_item("id_", dget(data, "id", py.None())?)?;
        kw.set_item("name", dget(data, "name", "構築物")?)?;
        kw.set_item("defer", true)?;
        let c = cls.call_method("from_voxels", (registry, vox, dget(spawn, "position", &zero)?, dget(spawn, "rotation", py.None())?,
            dget(spawn, "velocity", &zero)?, dget(spawn, "angular_velocity", &zero)?), Some(&kw))?;
        let cs = c.downcast::<Construct>()?;
        let extras = Self::g(cs, "extras")?;
        for e in dget(data, "extras", PyList::empty_bound(py))?.iter()? {
            let e = e?;
            let lst = PyList::empty_bound(py);
            for x in e.get_item(2)?.iter()? {
                lst.append(PyTuple::new_bound(py, x?.iter()?.collect::<R<Vec<_>>>()?))?;
            }
            extras.set_item(construct_link_key(py, &e.get_item(0)?, &e.get_item(1)?)?, lst)?;
        }
        Self::set(cs, "storage", py.get_type_bound::<PyDict>().call1((dget(data, "storage", PyDict::new_bound(py))?,))?)?;
        let ca = PyDict::new_bound(py);
        for kv in dget(data, "cavity_air", PyList::empty_bound(py))?.iter()? {
            let kv = kv?;
            ca.set_item(PyTuple::new_bound(py, kv.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?), kv.get_item(1)?)?;
        }
        Self::set(cs, "cavity_air", ca)?;
        let n = Self::n(cs)?;
        let rot = dget(spawn, "rotation", py.None())?;
        let rm = if rot.is_none() { eye() } else { m3(&rot)? };
        let p0 = v3(&dget(spawn, "position", &zero)?)?;
        let verts = dget(data, "vertices", py.None())?;
        if !verts.is_none() && verts.len()? == 8 * n {
            let vv = v3s(&verts)?;
            let pos: Vec<V3> = vv.iter().map(|v| add(mul_t(*v, &rm), p0)).collect();
            Self::set(cs, "pos", a3(py, &pos)?)?;
        }
        let rest = dget(data, "rest", py.None())?;
        if !rest.is_none() && rest.len()? == n {
            Self::set(cs, "rest", arr(py, fv(&rest)?, &[n, 8, 3])?)?;
        }
        Self::rebuild(cs)?;
        let w = v3(&dget(spawn, "angular_velocity", &zero)?)?;
        if w.iter().any(|x| *x != 0.0) {
            Self::spin(cs, w)?;
        }
        let anc = dget(data, "anchor", py.None())?;
        if anc.is_truthy()? {
            let d = PyDict::new_bound(py);
            d.set_item("to", anc)?;          // 動く物体（軌道上の街など）に固定。中身は拡張が決める
            Self::set(cs, "anchor", d)?;
        }
        if dget(data, "soft_ok", py.None())?.is_truthy()? {
            c.setattr("soft_ok", true)?;
        }
        Ok(c)
    }
}

pub struct RigidMotion {
    pub x: V3,
    pub v: V3,
    pub w: V3,
    pub i: [[f64; 3]; 3],
    pub r: Vec<V3>,
}

/// np.ascontiguousarray（すでにそうならそのまま）
pub fn contig<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    np(o.py())?.call_method1("ascontiguousarray", (o,))
}
pub fn cflt<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    np(o.py())?.call_method1("ascontiguousarray", (o, "float64"))
}
pub fn cint<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    np(o.py())?.call_method1("ascontiguousarray", (o, "int64"))
}
/// 連続した書き込める float64 でなければ作り直した配列（そうなら None）
pub fn fix_contig<'py>(o: &Bound<'py, PyAny>) -> R<Option<Bound<'py, PyAny>>> {
    if let Ok(a) = o.downcast::<numpy::PyArrayDyn<f64>>() {
        if a.is_c_contiguous() && a.getattr("flags")?.getattr("writeable")?.is_truthy()? {
            return Ok(None);
        }
    }
    Ok(Some(np(o.py())?.call_method1("array", (o, "float64"))?.call_method1("copy", ("C",))?))
}
/// 計算した値を元の配列に書き戻す（書き込めない配列なら作り直して入れる）
fn write_back(slf: &Bound<'_, Construct>, name: &str, o: &Bound<'_, PyAny>, data: Vec<f64>, shape: &[usize]) -> R<()> {
    let mut done = false;
    if let Ok(mut a) = o.extract::<PyReadwriteArrayDyn<f64>>() {
        if a.len() == data.len() {
            if let Ok(s) = a.as_slice_mut() {
                s.copy_from_slice(&data);
                done = true;
            } else {
                for (d, v) in a.as_array_mut().iter_mut().zip(&data) {
                    *d = *v;
                }
                done = true;
            }
        }
    }
    if !done {
        slf.as_any().setattr(name, arr(slf.py(), data, shape)?)?;
    }
    Ok(())
}

impl Construct {
    pub fn rigid_motion(slf: &Bound<'_, Self>) -> R<RigidMotion> {
        let m = Self::mass_v(slf)?;
        let pos = Self::pos_v(slf)?;
        let vel = Self::vel_v(slf)?;
        let x = wmean(&pos, &m);
        let v = wmean(&vel, &m);
        let r: Vec<V3> = pos.iter().map(|p| sub(*p, x)).collect();
        let mut l = [0.0; 3];
        let mut ii = [[0.0; 3]; 3];
        let mut mrr = Vec::with_capacity(m.len());
        for q in 0..r.len() {
            let c = cross(r[q], sub(vel[q], v));
            for k in 0..3 {
                l[k] += c[k] * m[q];
            }
            mrr.push(m[q] * dot(r[q], r[q]));
            for j in 0..3 {
                for k in 0..3 {
                    ii[j][k] -= m[q] * r[q][j] * r[q][k];
                }
            }
        }
        let s = pw_sum(&mrr);
        for k in 0..3 {
            ii[k][k] += s;
        }
        let tr = ii[0][0] + ii[1][1] + ii[2][2];
        let mut a = ii;
        for k in 0..3 {
            a[k][k] += 1e-9 * tr.max(1e-9);
        }
        let w = solve3(&a, l).unwrap_or([f64::NAN; 3]);
        Ok(RigidMotion { x, v, w, i: ii, r })
    }

    /// vel += w × (pos - 重心)
    fn spin(slf: &Bound<'_, Self>, w: V3) -> R<()> {
        let m = Self::mass_v(slf)?;
        let pos = Self::pos_v(slf)?;
        let com = wmean(&pos, &m);
        let vel_o = Self::g(slf, "vel")?;
        let mut vel = v3s(&vel_o)?;
        for (v, p) in vel.iter_mut().zip(&pos) {
            *v = add(*v, cross(w, sub(*p, com)));
        }
        let n = vel.len();
        write_back(slf, "vel", &vel_o, vel.iter().flatten().copied().collect(), &[n, 3])
    }

    pub fn fit(slf: &Bound<'_, Self>, idx: Option<&[usize]>) -> R<([[f64; 3]; 3], V3)> {
        let cells = Self::cells_v(slf)?;
        let pos = Self::pos_v(slf)?;
        let size = Self::sz(slf);
        let sel: Vec<usize> = match idx { Some(i) => i.to_vec(), None => (0..cells.len()).collect() };
        let mut local = Vec::with_capacity(8 * sel.len());
        let mut pp = Vec::with_capacity(8 * sel.len());
        for &i in &sel {
            for (c, cb) in CORNER_BITS.iter().enumerate() {
                local.push([(cells[i][0] as f64 + cb[0]) * size, (cells[i][1] as f64 + cb[1]) * size, (cells[i][2] as f64 + cb[2]) * size]);
                pp.push(pos[8 * i + c]);
            }
        }
        let lc = mean8(&local);
        let pc = mean8(&pp);
        let mut h = nalgebra::Matrix3::<f64>::zeros();
        for (l, p) in local.iter().zip(&pp) {
            let (a, b) = (sub(*l, lc), sub(*p, pc));
            for i in 0..3 {
                for j in 0..3 {
                    h[(i, j)] += a[i] * b[j];
                }
            }
        }
        let svd = nalgebra::linalg::SVD::new(h, true, true);
        let (u, mut vt) = (svd.u.unwrap(), svd.v_t.unwrap());
        let mut r = vt.transpose() * u.transpose();
        if r.determinant() < 0.0 {
            let sv = svd.singular_values;
            let kmin = (0..3).min_by(|&a, &b| sv[a].partial_cmp(&sv[b]).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
            for j in 0..3 {
                vt[(kmin, j)] *= -1.0;
            }
            r = vt.transpose() * u.transpose();
        }
        let rm = [[r[(0, 0)], r[(0, 1)], r[(0, 2)]], [r[(1, 0)], r[(1, 1)], r[(1, 2)]], [r[(2, 0)], r[(2, 1)], r[(2, 2)]]];
        let t = sub(pc, mul_t(lc, &rm));
        Ok((rm, t))
    }

    pub fn _contig(slf: &Bound<'_, Self>, names: &[&str]) -> R<()> {
        for k in names {
            let a = Self::g(slf, k)?;
            if let Some(v) = fix_contig(&a)? {
                Self::set(slf, k, v)?;
            }
        }
        Ok(())
    }
    fn ensure_prev(slf: &Bound<'_, Self>) -> R<()> {
        let pos = Self::g(slf, "pos")?;
        let ok = match opt(slf.as_any(), "_prev")? {
            Some(p) => p.getattr("shape")?.eq(pos.getattr("shape")?)?,
            None => false,
        };
        if !ok {
            Self::set(slf, "_prev", pos.call_method0("copy")?)?;
        }
        Ok(())
    }
    pub fn cav_arg<'py>(slf: &Bound<'py, Self>, pls: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let mut water = false;
        for p in pls.iter()? {
            if p?.get_item(5)?.is_truthy()? {
                water = true;
                break;
            }
        }
        if !water {
            return Ok(py.None().into_bound(py));
        }
        let c = Self::_cavity_neighbors(slf)?;
        if c.is_none() {
            return Ok(c);
        }
        cint(&c.get_item(0)?)
    }
    fn art_fresh<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let fresh = match opt(slf.as_any(), "art")? {
            Some(a) => a.get_item("version")?.eq(Self::ver(slf))? && a.get_item("n")?.eq(Self::n(slf)?)?,
            None => false,
        };
        if !fresh {
            Self::_art_build(slf)?;
        }
        Self::g(slf, "art")
    }
    fn ext_targets(slf: &Bound<'_, Self>, a: &Bound<'_, PyAny>) -> R<(Vec<f64>, Vec<f64>)> {
        let py = slf.py();
        let devs = Self::g(slf, "devs")?;
        let (mut w, mut s) = (vec![], vec![]);
        for e in a.get_item("exts")?.iter()? {
            let e = e?;
            let d = devs.get_item(e.get_item(0)?)?;
            let st = if d.is_truthy()? { d.get_item("state")? } else { PyDict::new_bound(py).into_any() };
            w.push(st.call_method1("get", ("ext", e.get_item(3)?))?.extract::<f64>()?);
            s.push(st.call_method1("get", ("ext_speed", 0.5))?.extract::<f64>()?);
        }
        Ok((w, s))
    }
    fn rot_targets(a: &Bound<'_, PyAny>) -> R<(Vec<f64>, Vec<f64>)> {
        let (mut w, mut s) = (vec![], vec![]);
        for dv in a.get_item("devs")?.iter()? {
            let st = dv?.get_item("state")?;
            w.push(st.call_method1("get", ("angle", 0.0))?.extract::<f64>()?);
            s.push(st.call_method1("get", ("rot_speed", 90.0))?.extract::<f64>()?);
        }
        Ok((w, s))
    }
    /// MOD の箱との接触を集める：(箱のリスト, [頂点, 深さ, 法線, 箱の速さ, 箱の番号])
    fn collect_boxes<'py>(slf: &Bound<'py, Self>, h: f64) -> R<(Vec<Bound<'py, PyAny>>, Vec<PyObject>)> {
        let py = slf.py();
        let boxes: Vec<Bound<'py, PyAny>> = Self::g(slf, "ext_colliders")?.iter()?.collect::<R<_>>()?;
        let (mut p, mut d, mut n, mut vs, mut ci) = (vec![], vec![], vec![], vec![], vec![]);
        for (bi, bs) in boxes.iter().enumerate() {
            let res = Self::_collide_boxes(slf, bs, h, true)?;
            if res.is_truthy()? {
                let pp = iv(&res.get_item(0)?)?;
                ci.extend(std::iter::repeat(bi as i64).take(pp.len()));
                p.extend(pp);
                d.extend(fv(&res.get_item(1)?)?);
                n.extend(fv(&res.get_item(2)?)?);
                vs.extend(fv(&res.get_item(3)?)?);
            }
        }
        let k = p.len();
        Ok((boxes, vec![iarr(py, p, &[k])?.unbind(), arr(py, d, &[k])?.unbind(), arr(py, n, &[k, 3])?.unbind(),
            arr(py, vs, &[k, 3])?.unbind(), iarr(py, ci, &[k])?.unbind()]))
    }
    /// 動いている街（ステーション）を押した分の反動を ext_events に
    fn push_back_events(slf: &Bound<'_, Self>, boxes: &[Bound<'_, PyAny>], ci: &Bound<'_, PyAny>, lam: &Bound<'_, PyAny>,
                        cn: &Bound<'_, PyAny>, pts: &[V3], h: f64) -> R<()> {
        let py = slf.py();
        let ci = iv(ci)?;
        if ci.is_empty() || !ci.iter().any(|x| *x >= 0) {
            return Ok(());
        }
        let lam = fv(lam)?;
        let cn = v3s(cn)?;
        let mut ids: Vec<i64> = ci.iter().copied().filter(|x| *x >= 0).collect();
        ids.sort();
        ids.dedup();
        let evs = Self::g(slf, "ext_events")?;
        for bi in ids {
            let sel: Vec<usize> = (0..ci.len()).filter(|&q| ci[q] == bi && lam[q] > 0.0).collect();
            if sel.is_empty() {
                continue;
            }
            let j: Vec<V3> = sel.iter().map(|&q| scl(cn[q], -(lam[q] / h))).collect();
            let p: Vec<V3> = sel.iter().map(|&q| pts[q]).collect();
            evs.call_method1("append", ((boxes[bi as usize].get_item(0)?, py.None(), a3(py, &j)?, a3(py, &p)?),))?;
        }
        Ok(())
    }
    fn drop_broken(slf: &Bound<'_, Self>, broken: &[usize], rot: &[usize]) -> R<()> {
        let links = Self::g(slf, "links")?;
        let rotors = Self::g(slf, "rotors")?;
        for &q in rot {
            links.call_method1("discard", (rotors.get_item(q)?.get_item("key")?,))?;
        }
        let jkey = Self::g(slf, "jkey")?;
        let extras = Self::g(slf, "extras")?;
        let py = slf.py();
        for &j in broken {
            let key = jkey.get_item(j)?;
            if key.get_item(0)?.eq("x")? {
                extras.call_method1("pop", (key.downcast::<PyTuple>()?.get_slice(1, 3), py.None()))?;
            } else {
                links.call_method1("discard", (key,))?;
            }
        }
        Ok(())
    }
    fn art_groups(slf: &Bound<'_, Self>) -> R<Vec<usize>> {
        let n = Self::n(slf)?;
        let mut parent: Vec<usize> = (0..n).collect();
        let rk = Self::rotor_keys(slf)?;
        let index = Self::g(slf, "index")?;
        for k in Self::link_list(slf, true)? {
            if rk.contains(&k)? {
                continue;
            }
            if let (Some(a), Some(b)) = (Self::idx_of(&index, &k.get_item(0)?)?, Self::idx_of(&index, &k.get_item(1)?)?) {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb {
                    parent[ra] = rb;
                }
            }
        }
        let roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
        let mut u = roots.clone();
        u.sort();
        u.dedup();
        Ok(roots.iter().map(|r| u.binary_search(r).unwrap()).collect())
    }
    fn apply_keep(slf: &Bound<'_, Self>, keep: &[bool]) -> R<()> {
        let py = slf.py();
        for k in ["cells", "mats", "devs"] {
            let l = Self::g(slf, k)?;
            let nl = PyList::empty_bound(py);
            for (x, kp) in l.iter()?.zip(keep) {
                if *kp {
                    nl.append(x?)?;
                }
            }
            Self::set(slf, k, nl)?;
        }
        let m = keep.iter().filter(|x| **x).count();
        let pos = Self::pos_v(slf)?;
        let vel = Self::vel_v(slf)?;
        let rest = fv(&Self::g(slf, "rest")?)?;
        let asc = fv(&Self::g(slf, "act_scale")?)?;
        let (mut p, mut v, mut r, mut a) = (vec![], vec![], vec![], vec![]);
        for (i, kp) in keep.iter().enumerate() {
            if *kp {
                p.extend_from_slice(&pos[8 * i..8 * i + 8]);
                v.extend_from_slice(&vel[8 * i..8 * i + 8]);
                r.extend_from_slice(&rest[24 * i..24 * i + 24]);
                a.extend_from_slice(&asc[3 * i..3 * i + 3]);
            }
        }
        Self::set(slf, "pos", a3(py, &p)?)?;
        Self::set(slf, "vel", a3(py, &v)?)?;
        Self::set(slf, "rest", arr(py, r, &[m, 8, 3])?)?;
        Self::set(slf, "act_scale", arr(py, a, &[m, 3])?)?;
        Self::set(slf, "force", zeros(py, &[8 * m, 3])?)
    }
}

/// vel[:] = 0 の代わり（書き込めない配列のとき）
struct PySliceAll;
impl PySliceAll {
    fn all(py: Python<'_>) -> R<Bound<'_, PyAny>> {
        Ok(pyo3::types::PySlice::full_bound(py).into_any())
    }
}
