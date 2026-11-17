//! 構築物（main/construct.py の Construct）。各ボクセルは自分専用の8頂点を持つ「ポリゴンのボクセル」。
//! 物理の本体（Body・Proxy・ArtTopo）は lib.rs 側。ここはそのまとめ役（形の作り直し・剛体化・編集・保存）。
//! 値は Rust の型のある欄に持つ（C-01）。配列は numpy の配列（型つき）のまま持つので、MOD・描画・テストからも今までと同じ名前で読み書きできる。
//! 計算の途中の覚え（キャッシュ）は Python からは見えない欄。MOD が足した属性だけ __dict__ に入る。

use crate::game::planet::{pla, Planet};
use crate::game::PyDictOps;
use numpy::ndarray::IxDyn;
use numpy::{PyArray1, PyArrayDyn, PyArrayMethods, PyReadonlyArrayDyn, PyReadwriteArrayDyn, PyUntypedArrayMethods};
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
/// これ以上のボクセル数なら、ボクセルごとに複数コアで計算する（main.fastphys.PARALLEL_MIN_VOXELS と同じ値）
pub const PARALLEL_MIN_VOXELS: usize = 256;
/// ぶつかったとみなす速さ（m/s。Construct.IMPACT_SPEED）
pub const IMPACT_SPEED: f64 = 2.0;

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
    let a = crate::game::construct::as_f64(&o)?;
    let r: PyReadonlyArrayDyn<f64> = a.extract()?;
    Ok(r.as_array().iter().copied().collect())
}
pub fn iv(o: &Bound<'_, PyAny>) -> R<Vec<i64>> {
    if let Ok(r) = o.extract::<PyReadonlyArrayDyn<i64>>() {
        return Ok(r.as_array().iter().copied().collect());
    }
    let a = crate::game::construct::as_i64(&o)?;
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
pub fn dget<'py>(d: &Bound<'py, PyAny>, k: &str, dflt: impl IntoPy<PyObject>) -> R<Bound<'py, PyAny>> {
    d.dget2(k, dflt)
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
    Ok(super::registry::device_def(reg, &d.get_item("def")?)?.and_then(|x| if x.kind.is_empty() { None } else { Some(x.kind.clone()) }))
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
        let nat = Planet::_nat(pla(&pl)?).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?;
        if nat.is_none() {
            return Ok(py.None());
        }
        let f = |k: &str| -> R<f64> { pl.getattr(k)?.extract::<f64>() };
        // 大気：地表の空気の密度（気圧に比例。101.3 kPa で 1.225 kg/m³）と、密度が 1/e になる高さ（空の色と同じ：半径の 1/4、30 m 以上）
        let kpa: f64 = match pl.getattr("defn")?.dget2("atmosphere_pressure", 101.3) {
            Ok(v) if !v.is_none() => v.extract().unwrap_or(101.3),
            _ => 101.3,
        };
        let rad = f("radius")?;
        let air = 1.225 * (kpa / 101.3).max(0.0);
        // 公転（L-08）：影響圏の半径と、惑星の加速度（影響圏の中の物はこの惑星と一緒に動くので、重力からこれを引く）
        let (soi, acc) = { let b = pla(&pl)?.borrow(); (b.soi, b.oacc) };
        out.append((nat, f("bound")?, f("lipschitz")?, f("gm")?, rad, pl.getattr("has_water")?.is_truthy()?, air,
                    (rad * 0.25).max(30.0), soi, acc.to_vec()))?;
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
    pub pos: FA,
    #[pyo3(get, set)]
    pub vel: FA,
    #[pyo3(get, set)]
    pub rest: FA,
    #[pyo3(get, set)]
    pub act_scale: FA,
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
    /// 剛体・レールの間の状態（ブロックごとに動いている間は None）
    #[pyo3(get, set)]
    pub rb: Option<Py<RigidState>>,
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
    /// どの公転する惑星と一緒に動いているか（planets の番号。-1 は無し、-2 はまだ決めていない。L-08）
    #[pyo3(get, set)]
    pub frame: i64,
    /// 重さ（A-01）：最後に質量を計算したときのタンクの中身の重さ・全体の重さ（kg）・質量の版（Rust の物体を作り直す印）
    #[pyo3(get)]
    pub mass_ref: f64,
    #[pyo3(get)]
    pub mass_total: f64,
    pub mass_ver: i64,

    // ---- 物理の配列（rebuild で作る。ボクセルごと n・頂点ごと 8n・結合ごと） ----
    #[pyo3(get, set)]
    pub force: FA,
    #[pyo3(get, set, name = "_prev")]
    pub prev: FA,
    #[pyo3(get, set)]
    pub mass: FA,
    #[pyo3(get, set)]
    pub inv_mass: FA,
    #[pyo3(get, set)]
    pub damp: FA,
    #[pyo3(get, set)]
    pub fric: FA,
    #[pyo3(get, set)]
    pub v_stiff: FA,
    #[pyo3(get, set)]
    pub v_jstiff: FA,
    #[pyo3(get, set)]
    pub v_break: FA,
    #[pyo3(get, set)]
    pub v_yield: FA,
    #[pyo3(get, set)]
    pub v_density: FA,
    #[pyo3(get, set)]
    pub v_rebond_dist: FA,
    #[pyo3(get, set)]
    pub v_plast: FA,
    #[pyo3(get, set)]
    pub v_rebond: BA,
    #[pyo3(get, set)]
    pub ja: IA,
    #[pyo3(get, set)]
    pub jb: IA,
    #[pyo3(get, set)]
    pub jr: FA,
    #[pyo3(get, set)]
    pub jk: FA,
    #[pyo3(get, set)]
    pub jbrk: FA,
    #[pyo3(get, set)]
    pub jcount: FA,
    #[pyo3(get, set)]
    pub jkey: Py<PyList>,
    #[pyo3(get, set)]
    pub face_vox: IA,
    #[pyo3(get, set)]
    pub face_dir: IA,
    #[pyo3(get, set)]
    pub face_idx: IA,
    #[pyo3(get, set, name = "_last_R")]
    pub last_r: FA,
    #[pyo3(get, set, name = "_quat")]
    pub quat: FA,
    #[pyo3(get, set)]
    pub ctype: IA,
    #[pyo3(get, set)]
    pub cidx: IA,
    /// 回転ジョイント（アクチュエーターの出力面）：[{a, b, d, dev, key, br}]
    #[pyo3(get, set)]
    pub rotors: Py<PyList>,
    /// 形（main.shape.topology の dict：points・segs・tris など）
    #[pyo3(get, set)]
    pub topo: PyObject,
    #[pyo3(get, set, name = "proxW")]
    pub prox_w: PyObject,
    /// 関節つき剛体の状態（mode "art" の間だけ）
    #[pyo3(get, set)]
    pub art: Option<Py<ArtState>>,

    // ---- しるし ----
    /// 配管・電力・気密の再計算が要る
    #[pyo3(get, set)]
    pub dirty: bool,
    #[pyo3(get, set)]
    pub soft_ok: bool,
    #[pyo3(get, set, name = "_full_rigid")]
    pub full_rigid: bool,
    #[pyo3(get, set, name = "_nb_used")]
    pub nb_used: bool,
    #[pyo3(get, set, name = "_settle")]
    pub settle: f64,
    #[pyo3(get, set, name = "_touch_moving")]
    pub touch_moving: bool,
    /// ゆっくり動かす機器（明かりなど）の、たまった時間
    #[pyo3(get, set, name = "_slow_dt")]
    pub slow_dt: f64,
    /// 機器が出したお知らせ（ワールドが表示して空にする）
    #[pyo3(get, set)]
    pub notes: Py<PyList>,

    // ---- 計算の途中の覚え（Python からは見えない） ----
    pub cache: Caches,
}

/// Python の値として持っている構築物を、型つきで見る（違う物ならエラー）
#[inline]
pub fn con<'a, 'py>(c: &'a Bound<'py, PyAny>) -> R<&'a Bound<'py, Construct>> {
    Ok(c.downcast::<Construct>()?)
}

/// numpy の配列（型つき）。Python からは今までどおり numpy の配列に見える
pub type FA = Py<PyArrayDyn<f64>>;
pub type IA = Py<PyArrayDyn<i64>>;
pub type BA = Py<PyArrayDyn<bool>>;

/// 構築物の計算の覚え。どれも (鍵, 値...) の形の tuple か、作った物そのもの
#[derive(Default)]
pub struct Caches {
    pub part: Option<PyObject>,
    pub art_hold: Option<PyObject>,
    pub frame_hold: Option<PyObject>,
    pub sleep_anchor: Option<PyObject>,
    pub nb_art: Option<PyObject>,
    pub devid: Option<PyObject>,
    pub act: Option<PyObject>,
    pub anchor: Option<PyObject>,
    pub ctr: Option<PyObject>,
    pub frm: Option<PyObject>,
    pub bnd: Option<PyObject>,
    pub quiet: Option<PyObject>,
    pub hasdev: Option<PyObject>,
    pub lat: Option<PyObject>,
    pub lod: Option<PyObject>,
    pub cav: Option<PyObject>,
    pub ground: Option<PyObject>,
    pub nb: Option<PyObject>,
    pub crest: Option<PyObject>,
    pub nproxy: Option<PyObject>,
    pub allvox: Option<PyObject>,
}

/// 0 の配列（型つき）
pub fn fz(py: Python<'_>, shape: &[usize]) -> FA {
    PyArrayDyn::<f64>::zeros_bound(py, IxDyn(shape), false).unbind()
}
pub fn iz(py: Python<'_>, shape: &[usize]) -> IA {
    PyArrayDyn::<i64>::zeros_bound(py, IxDyn(shape), false).unbind()
}
pub fn bz(py: Python<'_>, shape: &[usize]) -> BA {
    PyArrayDyn::<bool>::zeros_bound(py, IxDyn(shape), false).unbind()
}
/// Vec から配列（型つき）
pub fn fa(py: Python<'_>, v: Vec<f64>, shape: &[usize]) -> R<FA> {
    Ok(PyArray1::from_vec_bound(py, v).reshape(IxDyn(shape))?.unbind())
}
pub fn ia(py: Python<'_>, v: Vec<i64>, shape: &[usize]) -> R<IA> {
    Ok(PyArray1::from_vec_bound(py, v).reshape(IxDyn(shape))?.unbind())
}
/// Python の値（numpy の配列・list など）を float64 の配列に。numpy の float64 の配列ならそのまま（同じ物）
pub fn to_fa(o: &Bound<'_, PyAny>) -> R<FA> {
    if let Ok(a) = o.downcast::<PyArrayDyn<f64>>() {
        return Ok(a.clone().unbind());
    }
    Ok(crate::game::construct::as_f64(&o)?.downcast_into::<PyArrayDyn<f64>>()?.unbind())
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
        fv(&Self::g_mass(slf)?)
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
        let mut out: Vec<Bound<'py, PyAny>> = Self::g_links(slf)?.iter()?.collect::<R<_>>()?;
        if with_extras {
            for k in Self::g_extras(slf)?.iter()? {
                out.push(k?);
            }
        }
        Ok(out)
    }
    fn idx_of(index: &Bound<'_, PyAny>, cell: &Bound<'_, PyAny>) -> R<Option<usize>> {
        let v = index.dget1(cell)?;
        Ok(if v.is_none() { None } else { Some(v.extract()?) })
    }
    fn rotor_keys<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PySet>> {
        let s = PySet::empty_bound(slf.py())?;
        for r in Self::g_rotors(slf)?.iter()? {
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
            pos: fz(py, &[0, 3]),
            vel: fz(py, &[0, 3]),
            rest: fz(py, &[0, 8, 3]),
            act_scale: fz(py, &[0, 3]),
            storage: e(), cavity_air: e(),
            far_time: 0.0, version: 0, powered: false,
            status: e(), index: e(),
            mode: "soft".into(), calm_t: 0.0, want_soft: false,
            rb: None, rb_epoch: 0, sleep_epoch: 0, in_pool: false,
            ext_colliders: PyList::empty_bound(py).into_any().unbind(),
            ext_events: PyList::empty_bound(py).into_any().unbind(),
            anchor: py.None(), asleep: false, sleep_t: 0.0, frame: -2, mass_ref: 0.0, mass_total: 0.0, mass_ver: 0,
            force: fz(py, &[0, 3]), prev: fz(py, &[0, 3]), mass: fz(py, &[0]), inv_mass: fz(py, &[0]), damp: fz(py, &[0]),
            fric: fz(py, &[0]), v_stiff: fz(py, &[0]), v_jstiff: fz(py, &[0]), v_break: fz(py, &[0]), v_yield: fz(py, &[0]),
            v_density: fz(py, &[0]), v_rebond_dist: fz(py, &[0]), v_plast: fz(py, &[0]), v_rebond: bz(py, &[0]),
            ja: iz(py, &[0]), jb: iz(py, &[0]), jr: fz(py, &[0]), jk: fz(py, &[0]), jbrk: fz(py, &[0]), jcount: fz(py, &[0]),
            jkey: PyList::empty_bound(py).unbind(), face_vox: iz(py, &[0]), face_dir: iz(py, &[0]), face_idx: iz(py, &[0, 4]),
            last_r: fz(py, &[0, 3, 3]), quat: fz(py, &[0, 4]), ctype: iz(py, &[0]), cidx: iz(py, &[0]),
            rotors: PyList::empty_bound(py).unbind(), topo: py.None(), prox_w: py.None(), art: None,
            dirty: false, soft_ok: false, full_rigid: false, nb_used: false, settle: 0.0, touch_moving: false,
            slow_dt: 0.0, notes: PyList::empty_bound(py).unbind(),
            cache: Caches::default(),
        })
    }

    /// 物理コアの Body の覚え（テストが「Rust の Body で計算した」ことを確かめる。読むだけ。無ければ AttributeError）
    #[getter(_nb_cache)]
    fn nb_cache_py(slf: &Bound<'_, Self>) -> R<PyObject> {
        Self::cache_get(slf, |c| &c.nb)?.map(|o| o.unbind()).ok_or_else(|| PyAttributeError::new_err("_nb_cache"))
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
    const IMPACT_SPEED: f64 = IMPACT_SPEED;
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
        let cl = Self::g_cells(&cs)?;
        let ml = Self::g_mats(&cs)?;
        let dl = Self::g_devs(&cs)?;
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
        Self::s_pos(&cs, a3(py, &pos)?)?;
        let vel0 = match velocity {
            Some(v) if !v.is_none() => v3(v)?,
            _ => [0.0; 3],
        };
        Self::s_vel(&cs, a3(py, &vec![vel0; 8 * n])?)?;
        let mut rest = Vec::with_capacity(24 * n);
        for _ in 0..n {
            for cb in CORNER_BITS.iter() {
                rest.extend(cb.iter().map(|b| (b - 0.5) * size));
            }
        }
        Self::s_rest(&cs, arr(py, rest, &[n, 8, 3])?)?;
        Self::s_act_scale(&cs, arr(py, vec![1.0; 3 * n], &[n, 3])?)?;
        match links {
            Some(l) if !l.is_none() => Self::s_links(&cs, py.get_type_bound::<PySet>().call1((l,))?)?,
            _ => {
                let set: std::collections::HashSet<[i64; 3]> = cells.iter().copied().collect();
                let ls = Self::g_links(&cs)?.downcast_into::<PySet>()?;
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
    pub(crate) fn _init_device<'py>(slf: &Bound<'py, Self>, dev: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let dev = super::actuator::act_migrate_device(py, dev)?;   // 古い「アクチュエーター」で押し出していた物はピストンに
        let reg = Self::g_reg(slf)?;
        let def = dev.get_item("def")?;
        let dd = crate::game::registry::dev_dict(&reg, &def)?;
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
            let sname: String = Self::g_name(slf)?.str()?.extract()?;
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
    pub(crate) fn rebuild(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let cells = Self::cells_v(slf)?;
        let n = cells.len();
        let size = Self::sz(slf);
        let cl = Self::g_cells(slf)?;
        let index = PyDict::new_bound(py);
        for (i, c) in cl.iter()?.enumerate() {
            index.set_item(c?, i)?;
        }
        Self::s_index(slf, &index)?;
        let reg = Self::g_reg(slf)?;
        let mats: Vec<std::sync::Arc<super::registry::MaterialDef>> = Self::g_mats(slf)?.iter()?
            .map(|m| super::registry::material_def(&reg, &m?)).collect::<R<_>>()?;
        let g = |f: fn(&super::registry::MaterialDef) -> f64| -> Vec<f64> { mats.iter().map(|m| f(m)).collect() };
        let clip = |v: Vec<f64>| -> Vec<f64> { v.into_iter().map(|x| x.clamp(0.0, 1.0)).collect() };
        let (v_jstiff, v_break, mut v_plast);
        if n > 0 {
            let v_stiff = clip(g(|m| m.stiffness));
            v_jstiff = clip(g(|m| m.joint_stiffness));
            v_break = g(|m| m.break_distance).into_iter().map(|x| x * size).collect::<Vec<_>>();
            v_plast = clip(g(|m| m.plasticity));
            let v_density = g(|m| m.density);
            // ブロックの重さ：素材の密度 × 体積（機器に "mass" があればそれ）＋タンクの中身（A-01）
            let (vox_m, cm) = Self::voxel_masses(slf, &v_density)?;
            let vm: Vec<f64> = vox_m.iter().map(|m| m / 8.0).collect();
            {
                let mut b = slf.try_borrow_mut()?;
                b.mass_ref = cm;
                b.mass_total = vox_m.iter().sum();
                b.mass_ver += 1;
            }
            let rep = |v: &[f64]| -> Vec<f64> { v.iter().flat_map(|x| std::iter::repeat(*x).take(8)).collect() };
            let rb: Vec<bool> = mats.iter().map(|m| m.rebond).collect();
            let mut b = slf.try_borrow_mut()?;
            b.v_stiff = fa(py, v_stiff, &[n])?;
            b.v_jstiff = fa(py, v_jstiff.clone(), &[n])?;
            b.v_break = fa(py, v_break.clone(), &[n])?;
            b.v_yield = fa(py, g(|m| m.yield_strain), &[n])?;
            b.v_density = fa(py, v_density, &[n])?;
            b.v_rebond = PyArray1::from_vec_bound(py, rb).reshape(IxDyn(&[n]))?.unbind();
            b.v_rebond_dist = fa(py, g(|m| m.rebond_distance).into_iter().map(|x| x * size).collect(), &[n])?;
            b.inv_mass = fa(py, rep(&vm.iter().map(|m| 1.0 / m).collect::<Vec<_>>()), &[8 * n])?;
            b.mass = fa(py, rep(&vm), &[8 * n])?;
            b.damp = fa(py, rep(&g(|m| m.damping)), &[8 * n])?;
            b.fric = fa(py, rep(&g(|m| m.friction)), &[8 * n])?;
        } else {
            let mut bm = slf.try_borrow_mut()?;
            let b = &mut *bm;
            for a in [&mut b.v_stiff, &mut b.v_jstiff, &mut b.v_break, &mut b.v_yield, &mut b.v_density, &mut b.v_rebond_dist,
                      &mut b.inv_mass, &mut b.mass, &mut b.damp, &mut b.fric] {
                *a = fz(py, &[0]);
            }
            b.v_rebond = bz(py, &[0]);
            v_jstiff = vec![];
            v_break = vec![];
            v_plast = vec![];
        }
        // アクチュエーターの出力面の結合は「回転ジョイント」として別に扱う
        let rotor_of = PyDict::new_bound(py);
        let devs = Self::g_devs(slf)?;
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
        let keys: Vec<Bound<'_, PyAny>> = Self::g_links(slf)?.iter()?.collect::<R<_>>()?;
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
        Self::s_links(slf, &new_links)?;
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
        Self::s_rotors(slf, &rotors)?;
        if n > 0 {
            Self::s_v_plast(slf, arr(py, v_plast, &[n])?)?;
        } else {
            Self::s_v_plast(slf, zeros(py, &[0])?)?;
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
        let extras = Self::g_extras(slf)?;
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
        Self::s_ja(slf, iarr(py, ja, &[nj])?)?;
        Self::s_jb(slf, iarr(py, jb, &[nj])?)?;
        Self::s_jr(slf, arr(py, jr, &[nj])?)?;
        Self::s_jk(slf, arr(py, jk, &[nj])?)?;
        Self::s_jbrk(slf, arr(py, jbrk, &[nj])?)?;
        Self::s_jkey(slf, &jkey)?;
        Self::s_jcount(slf, arr(py, cnt.iter().map(|c| (*c).max(1) as f64).collect(), &[nv])?)?;
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
        Self::s_face_vox(slf, iarr(py, fvx, &[nf])?)?;
        Self::s_face_dir(slf, iarr(py, fdr, &[nf])?)?;
        Self::s_face_idx(slf, iarr(py, fid, &[nf, 4])?)?;
        let need = match Self::o_force(slf)? {
            None => true,
            Some(f) => f.getattr("shape")?.get_item(0)?.extract::<usize>()? != nv,
        };
        if need {
            Self::s_force(slf, zeros(py, &[nv, 3])?)?;
        }
        // 形（球＋結合部を埋める図形）と当たり判定用の球
        if n > 0 {
            let shape = imp(py, "main.shape")?;
            Self::s_topo(slf, shape.call_method1("topology", (slf,))?)?;
            Self::s_prox_w(slf, shape.call_method1("collision_proxies", (slf,))?)?;
            Self::_build_contacts(slf).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?;
        } else {
            let t = PyDict::new_bound(py);
            t.set_item("points", iarr(py, vec![], &[0])?)?;
            t.set_item("segs", iarr(py, vec![], &[0, 2])?)?;
            t.set_item("tris", iarr(py, vec![], &[0, 3])?)?;
            Self::s_topo(slf, t)?;
            Self::s_prox_w(slf, py.None())?;
        }
        slf.borrow_mut().version += 1;
        Self::s_dirty(slf, true)?;    // 配管・電力・気密の再計算が必要
        Self::wake(slf, true)
    }

    /// soft=True：形が変わる操作（アクチュエーターの動作・ブロックの追加削除）なので剛体扱いも解く
    #[pyo3(signature = (soft=false))]
    pub(crate) fn wake(slf: &Bound<'_, Self>, soft: bool) -> R<()> {
        let mut b = slf.borrow_mut();
        b.asleep = false;
        b.sleep_t = 0.0;
        if soft {
            b.want_soft = true;
            b.calm_t = 0.0;
            if b.mode != "soft" && b.mode != "art" {     // 関節つき剛体はそのまま（形は作り直す）
                b.mode = "soft".into();
                b.rb = None;
            }
        }
        Ok(())
    }

    /// ほぼ静止している構築物は計算を省く（スリープ）
    #[pyo3(signature = (dt, threshold=0.05, delay=1.0))]
    pub(crate) fn update_sleep(slf: &Bound<'_, Self>, dt: f64, threshold: f64, delay: f64) -> R<()> {
        if slf.borrow().asleep {
            return Ok(());
        }
        let f = fv(&Self::g_force(slf)?)?;
        if Self::n(slf)? == 0 || f.iter().any(|x| *x != 0.0) {
            slf.borrow_mut().sleep_t = 0.0;
            return Ok(());
        }
        let vel = Self::g_vel(slf)?;
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
            if slf.borrow().sleep_t > delay && !Self::t_touch_moving(slf)? {
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

}

// 中身は分けて書いている（C-02）
mod query;
mod tick;
mod modes;
mod edit;
mod save;
mod inner;
mod rigid_state;
mod art_state;
pub(crate) use self::inner::*;
pub(crate) use self::rigid_state::*;
pub(crate) use self::art_state::*;

// ---------------------------------------------------------------- 欄の読み書き（C-01）
// g_欄：値（Python の値として）、s_欄：入れる（型を確かめる）、o_欄：None でなければ値、t_欄：真偽
macro_rules! py_fields {
    ($($g:ident $s:ident $o:ident $t:ident $f:ident),* $(,)?) => {
        #[allow(dead_code)]
        impl Construct { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
                Ok(slf.try_borrow()?.$f.bind(slf.py()).clone().into_any())
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> R<()> {
                let py = slf.py();
                let o = v.into_py(py);
                let t = o.extract(py)?;
                slf.try_borrow_mut()?.$f = t;
                Ok(())
            }
            #[inline]
            pub fn $o<'py>(slf: &Bound<'py, Self>) -> R<Option<Bound<'py, PyAny>>> {
                let v = Self::$g(slf)?;
                Ok(if v.is_none() { None } else { Some(v) })
            }
            #[inline]
            pub fn $t(slf: &Bound<'_, Self>) -> R<bool> {
                Self::$g(slf)?.is_truthy()
            }
        )* }
    };
}
macro_rules! val_fields {
    ($($g:ident $s:ident $o:ident $t:ident $f:ident),* $(,)?) => {
        #[allow(dead_code)]
        impl Construct { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
                Ok(slf.try_borrow()?.$f.clone().into_py(slf.py()).into_bound(slf.py()))
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> R<()> {
                let py = slf.py();
                let t = v.into_py(py).extract(py)?;
                slf.try_borrow_mut()?.$f = t;
                Ok(())
            }
            #[inline]
            pub fn $o<'py>(slf: &Bound<'py, Self>) -> R<Option<Bound<'py, PyAny>>> {
                Ok(Some(Self::$g(slf)?))
            }
            #[inline]
            pub fn $t(slf: &Bound<'_, Self>) -> R<bool> {
                Self::$g(slf)?.is_truthy()
            }
        )* }
    };
}
py_fields!(
    g_notes s_notes o_notes t_notes notes,
    g_act_scale s_act_scale o_act_scale t_act_scale act_scale,
    g_anchor s_anchor o_anchor t_anchor anchor,
    g_cavity_air s_cavity_air o_cavity_air t_cavity_air cavity_air,
    g_cells s_cells o_cells t_cells cells,
    g_cidx s_cidx o_cidx t_cidx cidx,
    g_ctype s_ctype o_ctype t_ctype ctype,
    g_damp s_damp o_damp t_damp damp,
    g_devs s_devs o_devs t_devs devs,
    g_ext_colliders s_ext_colliders o_ext_colliders t_ext_colliders ext_colliders,
    g_ext_events s_ext_events o_ext_events t_ext_events ext_events,
    g_extras s_extras o_extras t_extras extras,
    g_face_dir s_face_dir o_face_dir t_face_dir face_dir,
    g_face_idx s_face_idx o_face_idx t_face_idx face_idx,
    g_face_vox s_face_vox o_face_vox t_face_vox face_vox,
    g_force s_force o_force t_force force,
    g_fric s_fric o_fric t_fric fric,
    g_id s_id o_id t_id id,
    g_index s_index o_index t_index index,
    g_inv_mass s_inv_mass o_inv_mass t_inv_mass inv_mass,
    g_ja s_ja o_ja t_ja ja,
    g_jb s_jb o_jb t_jb jb,
    g_jbrk s_jbrk o_jbrk t_jbrk jbrk,
    g_jcount s_jcount o_jcount t_jcount jcount,
    g_jk s_jk o_jk t_jk jk,
    g_jkey s_jkey o_jkey t_jkey jkey,
    g_jr s_jr o_jr t_jr jr,
    g_last_r s_last_r o_last_r t_last_r last_r,
    g_links s_links o_links t_links links,
    g_mass s_mass o_mass t_mass mass,
    g_mats s_mats o_mats t_mats mats,
    g_name s_name o_name t_name name,
    g_pos s_pos o_pos t_pos pos,
    g_prev s_prev o_prev t_prev prev,
    g_prox_w s_prox_w o_prox_w t_prox_w prox_w,
    g_quat s_quat o_quat t_quat quat,
    g_reg s_reg o_reg t_reg reg,
    g_rest s_rest o_rest t_rest rest,
    g_rotors s_rotors o_rotors t_rotors rotors,
    g_status s_status o_status t_status status,
    g_storage s_storage o_storage t_storage storage,
    g_topo s_topo o_topo t_topo topo,
    g_v_break s_v_break o_v_break t_v_break v_break,
    g_v_density s_v_density o_v_density t_v_density v_density,
    g_v_jstiff s_v_jstiff o_v_jstiff t_v_jstiff v_jstiff,
    g_v_plast s_v_plast o_v_plast t_v_plast v_plast,
    g_v_rebond s_v_rebond o_v_rebond t_v_rebond v_rebond,
    g_v_rebond_dist s_v_rebond_dist o_v_rebond_dist t_v_rebond_dist v_rebond_dist,
    g_v_stiff s_v_stiff o_v_stiff t_v_stiff v_stiff,
    g_v_yield s_v_yield o_v_yield t_v_yield v_yield,
    g_vel s_vel o_vel t_vel vel
);
val_fields!(
    g_slow_dt s_slow_dt o_slow_dt t_slow_dt slow_dt,
    g_asleep s_asleep o_asleep t_asleep asleep,
    g_calm_t s_calm_t o_calm_t t_calm_t calm_t,
    g_dirty s_dirty o_dirty t_dirty dirty,
    g_far_time s_far_time o_far_time t_far_time far_time,
    g_full_rigid s_full_rigid o_full_rigid t_full_rigid full_rigid,
    g_in_pool s_in_pool o_in_pool t_in_pool in_pool,
    g_mode s_mode o_mode t_mode mode,
    g_nb_used s_nb_used o_nb_used t_nb_used nb_used,
    g_powered s_powered o_powered t_powered powered,
    g_rb_epoch s_rb_epoch o_rb_epoch t_rb_epoch rb_epoch,
    g_settle s_settle o_settle t_settle settle,
    g_size s_size o_size t_size size,
    g_sleep_epoch s_sleep_epoch o_sleep_epoch t_sleep_epoch sleep_epoch,
    g_sleep_t s_sleep_t o_sleep_t t_sleep_t sleep_t,
    g_soft_ok s_soft_ok o_soft_ok t_soft_ok soft_ok,
    g_touch_moving s_touch_moving o_touch_moving t_touch_moving touch_moving,
    g_version s_version o_version t_version version,
    g_want_soft s_want_soft o_want_soft t_want_soft want_soft
);

impl Construct {
    /// 覚えの欄を読む（None・無いなら None）
    pub fn cache_get<'py>(slf: &Bound<'py, Self>, f: impl Fn(&Caches) -> &Option<PyObject>) -> R<Option<Bound<'py, PyAny>>> {
        let b = slf.try_borrow()?;
        Ok(match f(&b.cache) {
            Some(o) if !o.is_none(slf.py()) => Some(o.bind(slf.py()).clone()),
            _ => None,
        })
    }
    pub fn cache_put(slf: &Bound<'_, Self>, f: impl Fn(&mut Caches) -> &mut Option<PyObject>, v: impl IntoPy<PyObject>) -> R<()> {
        let o = v.into_py(slf.py());
        *f(&mut slf.try_borrow_mut()?.cache) = Some(o);
        Ok(())
    }
    /// 覚え (鍵, 値...) の鍵が key と同じならその覚え
    pub fn cache_hit<'py>(slf: &Bound<'py, Self>, f: impl Fn(&Caches) -> &Option<PyObject>, key: &Bound<'py, PyAny>)
                          -> R<Option<Bound<'py, PyAny>>> {
        if let Some(c) = Self::cache_get(slf, f)? {
            if c.get_item(0)?.eq(key)? {
                return Ok(Some(c));
            }
        }
        Ok(None)
    }
}

// ---------------------------------------------------------------- 重さ（A-01：燃料の重さ・機器の重さ）
impl Construct {
    /// 資源の中身の重さ（kg）。1 単位の重さは資源の "unit_mass"（無ければ 1 kg）
    pub(crate) fn contents_mass(reg: &Bound<'_, PyAny>, cont: &Bound<'_, PyAny>) -> R<f64> {
        let mut m = 0.0;
        let res = reg.getattr("defs")?.get_item("resources")?;
        for it in cont.call_method0("items")?.iter()? {
            let (k, v): (Bound<'_, PyAny>, f64) = it?.extract()?;
            let um = match res.dget1(&k) {
                Ok(d) if !d.is_none() => match d.dget1("unit_mass") { Ok(u) if !u.is_none() => u.extract::<f64>().unwrap_or(1.0), _ => 1.0 },
                _ => 1.0,
            };
            m += v.max(0.0) * um;
        }
        Ok(m)
    }
    /// ブロックごとの重さ（kg）と、タンクの中身の重さの合計
    pub(crate) fn voxel_masses(slf: &Bound<'_, Self>, density: &[f64]) -> R<(Vec<f64>, f64)> {
        let py = slf.py();
        let s3 = Self::sz(slf).powi(3);
        let reg = Self::g_reg(slf)?;
        let devs = Self::g_devs(slf)?;
        let nd = devs.len()?;
        let mut out = Vec::with_capacity(density.len());
        let mut cm = 0.0;
        for (i, d) in density.iter().enumerate() {
            let mut m = d * s3;
            let dv = if i < nd { devs.get_item(i)? } else { py.None().into_bound(py) };
            if dv.is_truthy()? {
                let dd = reg.call_method1("device", (dv.get_item("def")?,))?;
                if !dd.is_none() {
                    if let Ok(x) = dd.dget1("mass") {
                        if !x.is_none() {
                            m = x.extract::<f64>()?;
                        }
                    }
                    if let Ok(st) = dv.dget1("state") {
                        if !st.is_none() {
                            if let Ok(cont) = st.dget1("contents") {
                                if !cont.is_none() {
                                    let c = Self::contents_mass(&reg, &cont)?;
                                    m += c;
                                    cm += c;
                                }
                            }
                        }
                    }
                }
            }
            out.push(m.max(1e-3));
        }
        Ok((out, cm))
    }
    /// 頂点の質量だけを計算し直す（燃料を使った・入れたとき。形は変わらない）。剛体なら重心・慣性も取り直す
    pub(crate) fn refresh_mass(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let n = Self::n(slf)?;
        // 関節つきの剛体（art）は部分ごとの質量を別に持っているので、形を作り直すまで変えない
        if n == 0 || slf.borrow().art.is_some() || slf.borrow().mode == "art" {
            return Ok(());
        }
        let dens = fv(Self::g_v_density(slf)?.as_any())?;
        if dens.len() != n {
            return Ok(());
        }
        let (vox_m, cm) = Self::voxel_masses(slf, &dens)?;
        let vm: Vec<f64> = vox_m.iter().flat_map(|m| std::iter::repeat(m / 8.0).take(8)).collect();
        let inv: Vec<f64> = vm.iter().map(|m| 1.0 / m).collect();
        {
            let mut b = slf.try_borrow_mut()?;
            b.mass = fa(py, vm, &[8 * n])?;
            b.inv_mass = fa(py, inv, &[8 * n])?;
            b.mass_ref = cm;
            b.mass_total = vox_m.iter().sum();
            b.mass_ver += 1;
        }
        if slf.borrow().mode == "rigid" && slf.borrow().rb.is_some() {
            Self::enter_rigid(slf)?;
        }
        Ok(())
    }
}
