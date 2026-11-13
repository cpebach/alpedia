//! Panda3D が入っていない PC で、MOD の画面側が使う panda3d.core の名前の一部をまねる（Rust の画面のとき）
//! （main/client_rs/panda_compat.py）。
//!
//! Rust の画面の部品（gfx.RsNode）は、setTransparency・setAttrib・setMat などに渡された値を見て動くので、
//! ここでは値の入れ物（定数・行列・ファイル名）だけを用意する。描画そのもの（Geom など）は無い（import するとエラー）。

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::imp;
use super::ursina::{bfn, f64of, is_num, make_class};

type R<T> = PyResult<T>;

#[pyclass(module = "panda3d.core")]
pub struct TransparencyAttrib {}

#[allow(non_upper_case_globals)]
#[pymethods]
impl TransparencyAttrib {
    #[classattr]
    const M_none: i64 = 0;
    #[classattr]
    const M_alpha: i64 = 1;
    #[classattr]
    const M_premultiplied_alpha: i64 = 2;
    #[classattr]
    const M_multisample: i64 = 3;
    #[classattr]
    const M_binary: i64 = 4;
    #[classattr]
    const M_dual: i64 = 5;
    #[classattr]
    const MNone: i64 = 0;
    #[classattr]
    const MAlpha: i64 = 1;
}

#[pyclass(module = "panda3d.core", subclass, dict)]
pub struct ColorBlendAttrib {
    #[pyo3(get, set)]
    args: Py<PyTuple>,
}

#[allow(non_upper_case_globals)]
#[pymethods]
impl ColorBlendAttrib {
    #[classattr]
    const M_none: i64 = 0;
    #[classattr]
    const M_add: i64 = 1;
    #[classattr]
    const M_subtract: i64 = 2;
    #[classattr]
    const M_inv_subtract: i64 = 3;
    #[classattr]
    const M_min: i64 = 4;
    #[classattr]
    const M_max: i64 = 5;
    #[classattr]
    const O_zero: i64 = 0;
    #[classattr]
    const O_one: i64 = 1;
    #[classattr]
    const O_incoming_color: i64 = 2;
    #[classattr]
    const O_one_minus_incoming_color: i64 = 3;
    #[classattr]
    const O_fbuffer_color: i64 = 4;
    #[classattr]
    const O_one_minus_fbuffer_color: i64 = 5;
    #[classattr]
    const O_incoming_alpha: i64 = 6;
    #[classattr]
    const O_one_minus_incoming_alpha: i64 = 7;

    #[new]
    #[pyo3(signature = (*a))]
    fn new(a: &Bound<'_, PyTuple>) -> Self {
        ColorBlendAttrib { args: a.clone().unbind() }
    }
    #[classmethod]
    #[pyo3(signature = (*a))]
    fn make<'py>(cls: &Bound<'py, pyo3::types::PyType>, a: &Bound<'py, PyTuple>) -> R<Bound<'py, PyAny>> {
        cls.call1(a)
    }
}

#[pyclass(module = "panda3d.core")]
pub struct SamplerState {}

#[allow(non_upper_case_globals)]
#[pymethods]
impl SamplerState {
    #[classattr]
    const FT_nearest: i64 = 0;
    #[classattr]
    const FT_linear: i64 = 1;
    #[classattr]
    const FT_nearest_mipmap_nearest: i64 = 2;
    #[classattr]
    const FT_linear_mipmap_nearest: i64 = 3;
    #[classattr]
    const FT_nearest_mipmap_linear: i64 = 4;
    #[classattr]
    const FT_linear_mipmap_linear: i64 = 5;
    #[classattr]
    const WM_clamp: i64 = 0;
    #[classattr]
    const WM_repeat: i64 = 1;
}

/// 何をしても何もしない関数（WindowProperties の名前）
#[pyclass(module = "panda3d.core")]
pub struct NoOp {}
#[pymethods]
impl NoOp {
    #[new]
    fn new() -> Self {
        NoOp {}
    }
    #[pyo3(signature = (*_a, **_k))]
    fn __call__(&self, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) {}
}

#[pyclass(module = "panda3d.core", subclass, dict)]
pub struct WindowProperties {}
#[pymethods]
impl WindowProperties {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        WindowProperties {}
    }
    fn __getattr__(&self, name: &str) -> NoOp {
        let _ = name;
        NoOp {}
    }
}

fn floats<'py>(py: Python<'py>, a: &Bound<'py, PyTuple>) -> R<Vec<f64>> {
    let it: Vec<Bound<'py, PyAny>> = if a.len() == 1 { a.get_item(0)?.iter()?.collect::<R<_>>()? } else { a.iter().collect() };
    let _ = py;
    it.iter().map(f64of).collect()
}
#[pyfunction]
#[pyo3(signature = (s, *a))]
fn _pc_mat_init(s: &Bound<'_, PyAny>, a: &Bound<'_, PyTuple>) -> R<()> {
    let py = s.py();
    let v = floats(py, a)?;
    py.import_bound("builtins")?.getattr("list")?.call_method1("__init__", (s, PyList::new_bound(py, v)))?;
    Ok(())
}
#[pyfunction]
#[pyo3(signature = (cls, *a))]
fn _pc_vec_new<'py>(cls: &Bound<'py, PyAny>, a: &Bound<'py, PyTuple>) -> R<Bound<'py, PyAny>> {
    let py = cls.py();
    let v: Vec<f64> = if a.len() == 1 && !is_num(&a.get_item(0)?) { floats(py, a)? } else { a.iter().map(|x| f64of(&x)).collect::<R<_>>()? };
    py.import_bound("builtins")?.getattr("tuple")?.call_method1("__new__", (cls, PyTuple::new_bound(py, v)))
}
#[pyfunction]
fn _pc_fn_from<'py>(cls: &Bound<'py, PyAny>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    cls.call1((p.str()?,))
}
#[pyfunction]
fn _pc_fn_to<'py>(s: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    Ok(s.py().import_bound("builtins")?.getattr("str")?.call1((s,))?)
}

fn class_from<'py>(py: Python<'py>, name: &str, base: &Bound<'py, PyAny>, ns: &Bound<'py, PyDict>) -> R<Bound<'py, PyAny>> {
    ns.set_item("__module__", "panda3d.core")?;
    ns.set_item("__slots__", PyTuple::empty_bound(py))?;
    py.import_bound("builtins")?.getattr("type")?.call1((name, (base,), ns))
}

/// panda_compat のモジュールに入れる名前
#[pyfunction]
pub fn panda_names<'py>(py: Python<'py>) -> R<Bound<'py, PyDict>> {
    let me = imp(py, "alpedia_core")?;
    let d = PyDict::new_bound(py);
    d.set_item("TransparencyAttrib", py.get_type_bound::<TransparencyAttrib>())?;
    d.set_item("ColorBlendAttrib", py.get_type_bound::<ColorBlendAttrib>())?;
    d.set_item("SamplerState", py.get_type_bound::<SamplerState>())?;
    d.set_item("WindowProperties", py.get_type_bound::<WindowProperties>())?;
    // 4×4 行列（16 個の数。行ベクトルにかける形）。gfx.RsNode.setMat にそのまま渡せる
    let ns = PyDict::new_bound(py);
    ns.set_item("__init__", bfn(py, me.getattr("_pc_mat_init")?)?)?;
    let ns2 = ns.copy()?;
    ns2.del_item("__init__")?;
    let m = {
        ns.set_item("__module__", "panda3d.core")?;
        py.import_bound("builtins")?.getattr("type")?.call1(("LMatrix4f", (py.import_bound("builtins")?.getattr("list")?,), &ns))?
    };
    d.set_item("LMatrix4f", &m)?;
    d.set_item("LMatrix4", &m)?;
    let ns = PyDict::new_bound(py);
    ns.set_item("__new__", me.getattr("_pc_vec_new")?)?;
    let v = make_class(py, "_Vec", "tuple", "panda3d.core", &ns)?;
    let v3 = class_from(py, "Vec3", &v, &PyDict::new_bound(py))?;
    let v4 = class_from(py, "Vec4", &v, &PyDict::new_bound(py))?;
    d.set_item("_Vec", &v)?;
    d.set_item("Vec3", &v3)?;
    d.set_item("Vec4", &v4)?;
    for n in ["Point3", "LPoint3f", "LVector3f", "LVecBase3f"] {
        d.set_item(n, &v3)?;
    }
    d.set_item("Point2", &v)?;
    d.set_item("Vec2", &v)?;
    let ns = PyDict::new_bound(py);
    let cm = py.import_bound("builtins")?.getattr("classmethod")?;
    ns.set_item("fromOsSpecific", cm.call1((me.getattr("_pc_fn_from")?,))?)?;
    ns.set_item("toOsSpecific", bfn(py, me.getattr("_pc_fn_to")?)?)?;
    let f = {
        ns.set_item("__module__", "panda3d.core")?;
        py.import_bound("builtins")?.getattr("type")?.call1(("Filename", (py.import_bound("builtins")?.getattr("str")?,), &ns))?
    };
    d.set_item("Filename", f)?;
    Ok(d)
}

/// 本物の panda3d が無いときだけ、このまねを panda3d.core として入れる
#[pyfunction]
pub fn panda_install(py: Python<'_>, core: &Bound<'_, PyAny>) -> R<bool> {
    if imp(py, "panda3d.core").is_ok() {
        return Ok(false);
    }
    let types = imp(py, "types")?;
    let pkg = types.getattr("ModuleType")?.call1(("panda3d",))?;
    pkg.setattr("__path__", PyList::empty_bound(py))?;
    pkg.setattr("core", core)?;
    let sm = imp(py, "sys")?.getattr("modules")?;
    sm.set_item("panda3d", pkg)?;
    sm.set_item("panda3d.core", core)?;
    Ok(true)
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    for f in [wrap_pyfunction_bound!(_pc_mat_init, m)?, wrap_pyfunction_bound!(_pc_vec_new, m)?, wrap_pyfunction_bound!(_pc_fn_from, m)?,
              wrap_pyfunction_bound!(_pc_fn_to, m)?, wrap_pyfunction_bound!(panda_names, m)?, wrap_pyfunction_bound!(panda_install, m)?] {
        m.add_function(f)?;
    }
    m.add_class::<TransparencyAttrib>()?;
    m.add_class::<ColorBlendAttrib>()?;
    m.add_class::<SamplerState>()?;
    m.add_class::<NoOp>()?;
    m.add_class::<WindowProperties>()?;
    Ok(())
}
