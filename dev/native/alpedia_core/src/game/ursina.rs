//! 画面の上の部品（ursina と同じ書き方の Entity・Text・Button・InputField・mouse・camera.ui・color など）
//! （main/client_rs/ursina_shim.py）。
//!
//! プログラムの画面・メニュー・MOD の画面側は ursina の書き方で作られている（ursina 自体はもう使わない）。
//! main.client_rs.ursina_shim を sys.modules["ursina"] に入れておくと（install）、そのコードが Rust の画面に描かれる。
//! 座標は ursina と同じ（画面の縦の高さが 1、真ん中が原点、x 右・y 上。z が小さいほど手前）。

use std::sync::atomic::{AtomicI64, Ordering};

use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{IntoPyDict, PyDict, PyFloat, PyList, PyLong, PySet, PyTuple};

use super::gfx::{view, RsDynamicMesh, RsNode, RsTex, RsText};
use super::imp;

type R<T> = PyResult<T>;

pub const BIN0: i64 = 100000; // 画面の上の部品は、ゲームの HUD より後（手前）に描く
pub const TEXT_SIZE: f64 = 0.025; // ursina の Text.size（scale 1 の文字の大きさ）
pub const FONT_K: f64 = 1.1; // 文字の高さ → フォントの大きさ（ursina の見た目に合わせる）

// ---------------------------------------------------------------------------
// Python の tuple・list を元にしたクラス（Color・Vec2・Vec3 など）を Rust の関数で作る
// ---------------------------------------------------------------------------

/// クラスに入れる関数（self を先に渡す。tuple を元にしたクラスのメソッド用）
#[pyclass(module = "alpedia_core")]
pub struct BoundFn {
    f: PyObject,
}

#[pymethods]
impl BoundFn {
    #[pyo3(signature = (*a, **k))]
    fn __call__(&self, py: Python<'_>, a: &Bound<'_, PyTuple>, k: Option<&Bound<'_, PyDict>>) -> R<PyObject> {
        Ok(self.f.bind(py).call(a, k)?.unbind())
    }
    fn __get__(slf: &Bound<'_, Self>, obj: Option<&Bound<'_, PyAny>>, _cls: Option<&Bound<'_, PyAny>>) -> R<PyObject> {
        let py = slf.py();
        match obj.filter(|o| !o.is_none()) {
            None => Ok(slf.clone().into_any().unbind()),
            Some(o) => Ok(imp(py, "types")?.getattr("MethodType")?.call1((slf.borrow().f.bind(py), o))?.unbind()),
        }
    }
}

pub fn po<T: IntoPy<PyObject>>(py: Python<'_>, x: T) -> PyObject {
    x.into_py(py)
}
pub fn tb<T: IntoPy<PyObject>>(py: Python<'_>, x: T) -> Bound<'_, PyAny> {
    x.into_py(py).into_bound(py)
}
pub fn bfn(py: Python<'_>, f: Bound<'_, PyAny>) -> R<PyObject> {
    Ok(Py::new(py, BoundFn { f: f.unbind() })?.into_any())
}
fn prop_item(py: Python<'_>, i: usize) -> R<PyObject> {
    let ig = imp(py, "operator")?.getattr("itemgetter")?.call1((i,))?;
    Ok(py.import_bound("builtins")?.getattr("property")?.call1((ig,))?.unbind())
}
pub fn make_class<'py>(py: Python<'py>, name: &str, base: &str, module: &str, ns: &Bound<'py, PyDict>) -> R<Bound<'py, PyAny>> {
    let b = py.import_bound("builtins")?;
    ns.set_item("__module__", module)?;
    ns.set_item("__slots__", PyTuple::empty_bound(py))?;
    b.getattr("type")?.call1((name, (b.getattr(base)?,), ns))
}
pub fn f64of(o: &Bound<'_, PyAny>) -> R<f64> {
    if let Ok(v) = o.extract::<f64>() {
        return Ok(v);
    }
    o.py().import_bound("builtins")?.getattr("float")?.call1((o,))?.extract()
}
pub fn is_num(o: &Bound<'_, PyAny>) -> bool {
    o.is_instance_of::<PyLong>() || o.is_instance_of::<PyFloat>()
}
fn is_seq(o: &Bound<'_, PyAny>) -> bool {
    o.is_instance_of::<PyTuple>() || o.is_instance_of::<PyList>()
}
fn tuple_new<'py>(cls: &Bound<'py, PyAny>, items: Vec<f64>) -> R<Bound<'py, PyAny>> {
    let py = cls.py();
    py.import_bound("builtins")?.getattr("tuple")?.call_method1("__new__", (cls, PyTuple::new_bound(py, items)))
}

static COLOR: GILOnceCell<PyObject> = GILOnceCell::new();
static VEC2: GILOnceCell<PyObject> = GILOnceCell::new();
static VEC3: GILOnceCell<PyObject> = GILOnceCell::new();

#[pyfunction]
#[pyo3(signature = (cls, r, g, b, a=None))]
fn _color_new<'py>(cls: &Bound<'py, PyAny>, r: &Bound<'py, PyAny>, g: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>, a: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
    let a = match a { Some(a) => f64of(a)?, None => 1.0 };
    tuple_new(cls, vec![f64of(r)?, f64of(g)?, f64of(b)?, a])
}
#[pyfunction]
#[pyo3(signature = (cls, x=None, y=None))]
fn _vec2_new<'py>(cls: &Bound<'py, PyAny>, x: Option<&Bound<'py, PyAny>>, y: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
    let x = match x { Some(v) => f64of(v)?, None => 0.0 };
    let y = match y { Some(v) => f64of(v)?, None => 0.0 };
    tuple_new(cls, vec![x, y])
}
#[pyfunction]
#[pyo3(signature = (cls, x=None, y=None, z=None))]
fn _vec3_new<'py>(cls: &Bound<'py, PyAny>, x: Option<&Bound<'py, PyAny>>, y: Option<&Bound<'py, PyAny>>, z: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
    let py = cls.py();
    if let Some(s) = x.filter(|v| is_seq(v)) {
        let mut v: Vec<Bound<'py, PyAny>> = s.iter()?.collect::<R<_>>()?;
        while v.len() < 3 {
            v.push(0i64.into_py(py).into_bound(py));
        }
        return tuple_new(cls, vec![f64of(&v[0])?, f64of(&v[1])?, f64of(&v[2])?]);
    }
    let g = |o: Option<&Bound<'py, PyAny>>| -> R<f64> { match o { Some(v) => f64of(v), None => Ok(0.0) } };
    tuple_new(cls, vec![g(x)?, g(y)?, g(z)?])
}
fn it2(o: &Bound<'_, PyAny>) -> R<(f64, f64)> {
    Ok((f64of(&o.get_item(0)?)?, f64of(&o.get_item(1)?)?))
}
#[pyfunction]
fn _vec2_add<'py>(s: &Bound<'py, PyAny>, o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let (a, b) = it2(s)?;
    let (c, d) = it2(o)?;
    vec2(s.py(), a + c, b + d)
}
#[pyfunction]
fn _vec2_sub<'py>(s: &Bound<'py, PyAny>, o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let (a, b) = it2(s)?;
    let (c, d) = it2(o)?;
    vec2(s.py(), a - c, b - d)
}
#[pyfunction]
fn _vec2_mul<'py>(s: &Bound<'py, PyAny>, k: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let (a, b) = it2(s)?;
    if is_seq(k) {
        let (c, d) = it2(k)?;
        return vec2(s.py(), a * c, b * d);
    }
    let k = f64of(k)?;
    vec2(s.py(), a * k, b * k)
}
fn it3(o: &Bound<'_, PyAny>) -> R<[f64; 3]> {
    Ok([f64of(&o.get_item(0)?)?, f64of(&o.get_item(1)?)?, f64of(&o.get_item(2)?)?])
}
#[pyfunction]
fn _vec3_add<'py>(s: &Bound<'py, PyAny>, o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let py = s.py();
    let o = vec3_of(py, o)?;
    let (a, b) = (it3(s)?, it3(&o)?);
    vec3(py, a[0] + b[0], a[1] + b[1], a[2] + b[2])
}
#[pyfunction]
fn _vec3_sub<'py>(s: &Bound<'py, PyAny>, o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let py = s.py();
    let o = vec3_of(py, o)?;
    let (a, b) = (it3(s)?, it3(&o)?);
    vec3(py, a[0] - b[0], a[1] - b[1], a[2] - b[2])
}
#[pyfunction]
fn _vec3_mul<'py>(s: &Bound<'py, PyAny>, k: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let a = it3(s)?;
    let k = f64of(k)?;
    vec3(s.py(), a[0] * k, a[1] * k, a[2] * k)
}

fn tuple_classes(py: Python<'_>) -> R<()> {
    if COLOR.get(py).is_some() {
        return Ok(());
    }
    let m = "ursina";
    let me = imp(py, "alpedia_core")?;
    let f = |n: &str| me.getattr(n);
    // Color
    let ns = PyDict::new_bound(py);
    ns.set_item("__new__", f("_color_new")?)?;
    for (i, n) in ["r", "g", "b", "a"].iter().enumerate() {
        ns.set_item(*n, prop_item(py, i)?)?;
    }
    let c = make_class(py, "Color", "tuple", m, &ns)?;
    let _ = COLOR.set(py, c.unbind());
    // Vec2
    let ns = PyDict::new_bound(py);
    ns.set_item("__new__", f("_vec2_new")?)?;
    ns.set_item("x", prop_item(py, 0)?)?;
    ns.set_item("y", prop_item(py, 1)?)?;
    ns.set_item("__add__", bfn(py, f("_vec2_add")?)?)?;
    ns.set_item("__sub__", bfn(py, f("_vec2_sub")?)?)?;
    ns.set_item("__mul__", bfn(py, f("_vec2_mul")?)?)?;
    let c = make_class(py, "Vec2", "tuple", m, &ns)?;
    let _ = VEC2.set(py, c.unbind());
    // Vec3
    let ns = PyDict::new_bound(py);
    ns.set_item("__new__", f("_vec3_new")?)?;
    for (i, n) in ["x", "y", "z"].iter().enumerate() {
        ns.set_item(*n, prop_item(py, i)?)?;
    }
    ns.set_item("__add__", bfn(py, f("_vec3_add")?)?)?;
    ns.set_item("__sub__", bfn(py, f("_vec3_sub")?)?)?;
    ns.set_item("__mul__", bfn(py, f("_vec3_mul")?)?)?;
    let c = make_class(py, "Vec3", "tuple", m, &ns)?;
    let _ = VEC3.set(py, c.unbind());
    Ok(())
}
fn cls<'py>(py: Python<'py>, c: &GILOnceCell<PyObject>) -> R<Bound<'py, PyAny>> {
    tuple_classes(py)?;
    Ok(c.get(py).unwrap().bind(py).clone())
}
pub fn color(py: Python<'_>, r: f64, g: f64, b: f64, a: f64) -> R<Bound<'_, PyAny>> {
    cls(py, &COLOR)?.call1((r, g, b, a))
}
/// Color(*(list(c) + [1.0])[:4])
pub fn color_of<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let mut v: Vec<Bound<'py, PyAny>> = c.iter()?.collect::<R<_>>()?;
    v.push(1.0f64.into_py(py).into_bound(py));
    v.truncate(4);
    cls(py, &COLOR)?.call1(PyTuple::new_bound(py, v))
}
pub fn vec2(py: Python<'_>, x: f64, y: f64) -> R<Bound<'_, PyAny>> {
    cls(py, &VEC2)?.call1((x, y))
}
pub fn vec3(py: Python<'_>, x: f64, y: f64, z: f64) -> R<Bound<'_, PyAny>> {
    cls(py, &VEC3)?.call1((x, y, z))
}
pub fn vec3_of<'py>(py: Python<'py>, o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    cls(py, &VEC3)?.call1((o,))
}

// ---------------------------------------------------------------------------
// 色
// ---------------------------------------------------------------------------
const NAMED: &[(&str, [f64; 4])] = &[
    ("white", [1.0, 1.0, 1.0, 1.0]), ("black", [0.0, 0.0, 0.0, 1.0]), ("clear", [0.0, 0.0, 0.0, 0.0]), ("gray", [0.5, 0.5, 0.5, 1.0]),
    ("grey", [0.5, 0.5, 0.5, 1.0]), ("light_gray", [0.75, 0.75, 0.75, 1.0]), ("lighter_gray", [0.85, 0.85, 0.85, 1.0]),
    ("dark_gray", [0.25, 0.25, 0.25, 1.0]), ("smoke", [1.0, 1.0, 1.0, 0.5]), ("red", [1.0, 0.1, 0.1, 1.0]), ("green", [0.1, 0.8, 0.2, 1.0]),
    ("lime", [0.5, 1.0, 0.2, 1.0]), ("blue", [0.1, 0.3, 1.0, 1.0]), ("azure", [0.2, 0.55, 1.0, 1.0]), ("cyan", [0.0, 1.0, 1.0, 1.0]),
    ("yellow", [1.0, 1.0, 0.1, 1.0]), ("orange", [1.0, 0.55, 0.1, 1.0]), ("magenta", [1.0, 0.1, 1.0, 1.0]), ("pink", [1.0, 0.5, 0.75, 1.0]),
    ("violet", [0.55, 0.2, 1.0, 1.0]), ("brown", [0.55, 0.35, 0.15, 1.0]), ("olive", [0.5, 0.5, 0.1, 1.0]), ("gold", [1.0, 0.8, 0.2, 1.0]),
];

/// ursina の color（color.white・color.rgb(...) など）
#[pyclass(module = "alpedia_core", name = "_ColorMod")]
pub struct ColorMod {}

#[pymethods]
impl ColorMod {
    #[new]
    fn new() -> Self {
        ColorMod {}
    }
    fn rgb<'py>(&self, py: Python<'py>, r: f64, g: f64, b: f64) -> R<Bound<'py, PyAny>> {
        if r.max(g).max(b) > 1.0 {
            return color(py, r / 255.0, g / 255.0, b / 255.0, 1.0);
        }
        color(py, r, g, b, 1.0)
    }
    #[pyo3(signature = (r, g, b, a=1.0))]
    fn rgba<'py>(&self, py: Python<'py>, r: f64, g: f64, b: f64, a: f64) -> R<Bound<'py, PyAny>> {
        if r.max(g).max(b) > 1.0 {
            return color(py, r / 255.0, g / 255.0, b / 255.0, if a > 1.0 { a / 255.0 } else { a });
        }
        color(py, r, g, b, a)
    }
    #[pyo3(signature = (h, s, v, a=1.0))]
    fn hsv<'py>(&self, py: Python<'py>, h: f64, s: f64, v: f64, a: f64) -> R<Bound<'py, PyAny>> {
        let h = h.rem_euclid(360.0) / 360.0;
        // colorsys.hsv_to_rgb と同じ
        let (r, g, b) = if s == 0.0 {
            (v, v, v)
        } else {
            let i = (h * 6.0).trunc();
            let f = h * 6.0 - i;
            let p = v * (1.0 - s);
            let q = v * (1.0 - s * f);
            let t = v * (1.0 - s * (1.0 - f));
            match (i as i64).rem_euclid(6) {
                0 => (v, t, p),
                1 => (q, v, p),
                2 => (p, v, t),
                3 => (p, q, v),
                4 => (t, p, v),
                _ => (v, p, q),
            }
        };
        color(py, r, g, b, a)
    }
    fn __getattr__<'py>(&self, py: Python<'py>, name: &str) -> R<Bound<'py, PyAny>> {
        let _ = name;
        color(py, 0.6, 0.6, 0.6, 1.0)
    }
}

// ---------------------------------------------------------------------------
// 全体の状態（部品の一覧・マウス・窓・カメラなど）
// ---------------------------------------------------------------------------
static SEQ: AtomicI64 = AtomicI64::new(0);
struct St {
    entities: Py<PyList>, // 生きている Entity（入力・update を配る順）
    dirty: Py<PySet>,
    focus: Py<PyList>,
    invokes: Py<PyList>,
    mouse: Py<Mouse>,
    window: Py<Window>,
    scene: PyObject,
    camera: Py<Camera>,
    held_keys: PyObject,
    time: Py<Time>,
    application: Py<App>,
    color: Py<ColorMod>,
}
static ST: GILOnceCell<St> = GILOnceCell::new();

fn st(py: Python<'_>) -> R<&St> {
    if let Some(s) = ST.get(py) {
        return Ok(s);
    }
    tuple_classes(py)?;
    let ct = py.get_type_bound::<ColorMod>();
    for (n, c) in NAMED {
        ct.setattr(*n, color(py, c[0], c[1], c[2], c[3])?)?;
    }
    let scene = scene_new(py, "scene")?;
    let ui = scene_new(py, "ui")?;
    let s = St {
        entities: PyList::empty_bound(py).unbind(),
        dirty: PySet::empty_bound(py)?.unbind(),
        focus: PyList::new_bound(py, [py.None()]).unbind(),
        invokes: PyList::empty_bound(py).unbind(),
        mouse: Py::new(py, Mouse::make(py)?)?,
        window: Py::new(py, Window::make(py)?)?,
        scene: scene.unbind(),
        camera: Py::new(py, Camera { ui: ui.unbind(), fov: 80.into_py(py), fov_pair: (80.0, 50.0).into_py(py), position: vec3(py, 0.0, 0.0, 0.0)?.unbind(),
            clip_plane_near: 0.05.into_py(py), clip_plane_far: 10000.into_py(py), orthographic: false.into_py(py) })?,
        held_keys: imp(py, "collections")?.getattr("defaultdict")?.call1((py.import_bound("builtins")?.getattr("int")?,))?.unbind(),
        time: Py::new(py, Time { dt: 1.0 / 60.0 })?,
        application: Py::new(py, App { paused: false.into_py(py), fonts_folder: py.None(), base: Py::new(py, Base {})?.into_any(), asset_folder: py.None() })?,
        color: Py::new(py, ColorMod {})?,
    };
    let _ = ST.set(py, s);
    Ok(ST.get(py).unwrap())
}
fn ui_root(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    Ok(st(py)?.camera.borrow(py).ui.clone_ref(py).into_bound(py))
}
fn mouse(py: Python<'_>) -> R<Bound<'_, Mouse>> {
    Ok(st(py)?.mouse.bind(py).clone())
}
fn focus(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    st(py)?.focus.bind(py).get_item(0)
}
fn set_focus(py: Python<'_>, f: &Bound<'_, PyAny>) -> R<()> {
    st(py)?.focus.bind(py).set_item(0, f)
}
fn log_exc(py: Python<'_>, msg: &str, e: PyErr) -> R<()> {
    let lg = imp(py, "main.log")?.call_method1("get", ("ursina_shim",))?;
    lg.call_method("exception", (msg,), Some(&[("exc_info", e.value_bound(py).clone())].into_py_dict_bound(py)))?;
    Ok(())
}

// 中身は分けて書いている（C-02）
mod mesh;
mod entity;
mod text;
mod button;
mod screen;
mod run;
pub(crate) use self::mesh::*;
pub(crate) use self::entity::*;
pub(crate) use self::text::*;
pub(crate) use self::button::*;
pub(crate) use self::screen::*;
pub(crate) use self::run::*;
