//! 画面の上の部品（ursina と同じ書き方の Entity・Text・Button・InputField・mouse・camera.ui・color など）
//! （main/client_rs/ursina_shim.py）。
//!
//! プログラムの画面・メニュー・MOD の画面側は ursina の書き方で作られている（ursina 自体はもう使わない）。
//! main.client_rs.ursina_shim を sys.modules["ursina"] に入れておくと（install）、そのコードが Rust の画面に描かれる。
//! 座標は ursina と同じ（画面の縦の高さが 1、真ん中が原点、x 右・y 上。z が小さいほど手前）。

use std::sync::atomic::{AtomicI64, Ordering};

use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyDict, PyFloat, PyList, PyLong, PySet, PyTuple};

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
// 形（Quad・Mesh）とテクスチャ
// ---------------------------------------------------------------------------
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Mesh {
    #[pyo3(get, set)]
    vertices: PyObject,
    #[pyo3(get, set)]
    triangles: PyObject,
    #[pyo3(get, set)]
    colors: PyObject,
    #[pyo3(get, set)]
    uvs: PyObject,
    #[pyo3(get, set)]
    mode: PyObject,
}

impl Mesh {
    fn blank(py: Python<'_>) -> Self {
        Mesh { vertices: PyList::empty_bound(py).into_any().unbind(), triangles: py.None(), colors: py.None(), uvs: py.None(), mode: "triangle".into_py(py) }
    }
    fn fill(slf: &Bound<'_, Mesh>, vertices: &Bound<'_, PyAny>, triangles: PyObject, colors: PyObject, uvs: PyObject, mode: PyObject) -> R<()> {
        let py = slf.py();
        let tup = py.import_bound("builtins")?.getattr("tuple")?;
        let vs = PyList::empty_bound(py);
        for v in vertices.iter()? {
            vs.append(tup.call1((v?,))?)?;
        }
        let mut b = slf.borrow_mut();
        b.vertices = vs.into_any().unbind();
        b.triangles = triangles;
        b.colors = colors;
        b.uvs = uvs;
        b.mode = mode;
        Ok(())
    }
}

#[pymethods]
impl Mesh {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        Mesh::blank(py)
    }
    #[pyo3(signature = (vertices=None, triangles=None, colors=None, uvs=None, mode=None, **_kw))]
    fn __init__(slf: &Bound<'_, Self>, vertices: Option<&Bound<'_, PyAny>>, triangles: Option<PyObject>, colors: Option<PyObject>, uvs: Option<PyObject>,
                mode: Option<PyObject>, _kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let py = slf.py();
        let e = PyTuple::empty_bound(py).into_any();
        Self::fill(slf, vertices.unwrap_or(&e), triangles.unwrap_or_else(|| py.None()), colors.unwrap_or_else(|| py.None()),
            uvs.unwrap_or_else(|| py.None()), mode.unwrap_or_else(|| "triangle".into_py(py)))
    }
    /// 頂点 (n,3)・UV (n,2)・三角形 (m,3)
    fn arrays<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let np = imp(py, "numpy")?;
        let b = slf.borrow();
        let fl = py.import_bound("builtins")?.getattr("float")?;
        let v = np.call_method1("asarray", (b.vertices.bind(py), &fl))?.call_method1("reshape", (-1, 3))?;
        let tris = b.triangles.bind(py);
        let t = if tris.is_truthy()? {
            let l = PyList::new_bound(py, tris.iter()?.collect::<R<Vec<_>>>()?);
            np.call_method1("asarray", (l, np.getattr("int64")?))?.call_method1("reshape", (-1, 3))?
        } else {
            let n = v.len()?;
            np.call_method("arange", ((n - n % 3) as i64,), Some(&[("dtype", np.getattr("int64")?)].into_py_dict_bound(py)))?.call_method1("reshape", (-1, 3))?
        };
        let uvs = b.uvs.bind(py);
        let uv = if uvs.is_truthy()? {
            np.call_method1("asarray", (uvs, &fl))?.call_method1("reshape", (-1, 2))?
        } else {
            let sl = PyTuple::new_bound(py, [py.import_bound("builtins")?.getattr("slice")?.call1((py.None(),))?, pyo3::types::PySlice::new_bound(py, 0, 2, 1).into_any()]);
            v.get_item(sl)?.call_method1("__add__", (0.5,))?
        };
        Ok(PyTuple::new_bound(py, [v, uv, t]))
    }
}

use pyo3::types::IntoPyDict;

/// 角の丸い四角（幅 1・高さ 1、真ん中が原点。aspect＝幅÷高さで丸みを合わせる）
#[pyclass(module = "alpedia_core", extends = Mesh, subclass)]
pub struct Quad {}

fn quad_parts(radius: f64, aspect: f64, segments: i64) -> (Vec<(f64, f64, f64)>, Vec<(i64, i64, i64)>, Vec<(f64, f64)>) {
    let r = radius.min(0.5).max(0.0);
    let a = aspect.max(1e-3);
    let mut pts: Vec<(f64, f64)> = vec![];
    if r <= 1e-4 {
        pts = vec![(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
    } else {
        let (rx, ry) = (r / a, r); // 縦の高さ 1 に対する丸み → 幅に直す
        let n = 2.max(segments * 2);
        for (cx, cy, a0) in [(0.5 - rx, 0.5 - ry, 0.0), (-0.5 + rx, 0.5 - ry, 90.0), (-0.5 + rx, -0.5 + ry, 180.0), (0.5 - rx, -0.5 + ry, 270.0)] {
            for k in 0..=n {
                let t = (a0 + 90.0 * k as f64 / n as f64).to_radians();
                pts.push((cx + rx * t.cos(), cy + ry * t.sin()));
            }
        }
    }
    let mut verts = vec![(0.0, 0.0, 0.0)];
    verts.extend(pts.iter().map(|&(x, y)| (x, y, 0.0)));
    let np_ = pts.len() as i64;
    let tris = (0..np_).map(|k| (0, 1 + k, 1 + (k + 1) % np_)).collect();
    let uvs = verts.iter().map(|&(x, y, _)| (x + 0.5, y + 0.5)).collect();
    (verts, tris, uvs)
}

#[pymethods]
impl Quad {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> (Self, Mesh) {
        (Quad {}, Mesh::blank(py))
    }
    #[pyo3(signature = (radius=None, aspect=None, segments=None, **_kw))]
    fn __init__(slf: &Bound<'_, Self>, radius: Option<&Bound<'_, PyAny>>, aspect: Option<&Bound<'_, PyAny>>, segments: Option<&Bound<'_, PyAny>>,
                _kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let py = slf.py();
        let seg: i64 = match segments { Some(sg) => py.import_bound("builtins")?.getattr("int")?.call1((sg,))?.extract()?, None => 3 };
        let r = match radius { Some(x) => f64of(x)?, None => 0.1 };
        let a = match aspect { Some(x) => f64of(x)?, None => 1.0 };
        let (v, t, uv) = quad_parts(r, a, seg);
        Mesh::fill(slf.downcast::<Mesh>()?, &PyList::new_bound(py, v.into_iter().map(|x| po(py, x))), PyList::new_bound(py, t.into_iter().map(|x| po(py, x))).into_any().unbind(),
            py.None(), PyList::new_bound(py, uv.into_iter().map(|x| po(py, x))).into_any().unbind(), "triangle".into_py(py))
    }
}

fn mesh_new<'py>(py: Python<'py>, v: Vec<(f64, f64, f64)>, t: Vec<(i64, i64, i64)>, uv: Vec<(f64, f64)>) -> R<Bound<'py, PyAny>> {
    let m = Bound::new(py, Mesh::blank(py))?;
    Mesh::fill(&m, &PyList::new_bound(py, v.into_iter().map(|x| po(py, x))), PyList::new_bound(py, t.into_iter().map(|x| po(py, x))).into_any().unbind(),
        py.None(), PyList::new_bound(py, uv.into_iter().map(|x| po(py, x))).into_any().unbind(), "triangle".into_py(py))?;
    Ok(m.into_any())
}

static MODELS: GILOnceCell<Py<PyDict>> = GILOnceCell::new();

fn models(py: Python<'_>) -> R<&Bound<'_, PyDict>> {
    if MODELS.get(py).is_none() {
        let d = PyDict::new_bound(py);
        let q = mesh_new(py, vec![(-0.5, -0.5, 0.0), (0.5, -0.5, 0.0), (0.5, 0.5, 0.0), (-0.5, 0.5, 0.0)], vec![(0, 1, 2), (0, 2, 3)],
            vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)])?;
        let n = 24i64;
        let pts: Vec<(f64, f64)> = (0..n).map(|k| {
            let t = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
            (0.5 * t.cos(), 0.5 * t.sin())
        }).collect();
        let mut verts = vec![(0.0, 0.0, 0.0)];
        verts.extend(pts.iter().map(|&(x, y)| (x, y, 0.0)));
        let uvs = verts.iter().map(|&(x, y, _)| (x + 0.5, y + 0.5)).collect();
        let c = mesh_new(py, verts, (0..n).map(|k| (0, 1 + k, 1 + (k + 1) % n)).collect(), uvs)?;
        d.set_item("quad", &q)?;
        d.set_item("circle", c)?;
        d.set_item("cube", &q)?;
        let _ = MODELS.set(py, d.unbind());
    }
    Ok(MODELS.get(py).unwrap().bind(py))
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Texture {
    #[pyo3(get, set)]
    rs: PyObject,
    #[pyo3(get, set)]
    filtering: PyObject,
}

#[pymethods]
impl Texture {
    #[new]
    #[pyo3(signature = (src=None, *_a, **_k))]
    fn new(py: Python<'_>, src: Option<&Bound<'_, PyAny>>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<Self> {
        let mut rs = py.None();
        if let Some(s) = src.filter(|s| !s.is_none()) {
            if s.is_instance_of::<RsTex>() {
                rs = s.clone().unbind();
            } else if s.hasattr("convert")? {
                // PIL の画像
                let im = s.call_method1("convert", ("RGBA",))?;
                rs = Py::new(py, super::gfx::gfx_tex_rgba(py, &im.getattr("width")?, &im.getattr("height")?, &im.call_method0("tobytes")?,
                    Some("pil".into_py(py)), false)?)?.into_any();
            } else if s.is_instance_of::<pyo3::types::PyString>() {
                rs = super::gfx::gfx_tex(py, s, false)?;
            }
        }
        Ok(Texture { rs, filtering: py.None() })
    }
    #[getter]
    fn width(&self) -> i64 {
        1
    }
    #[getter]
    fn height(&self) -> i64 {
        1
    }
}

#[pyfunction]
#[pyo3(signature = (path, *_a, **_k))]
fn ursina_load_texture<'py>(py: Python<'py>, path: &Bound<'py, PyAny>, _a: &Bound<'py, PyTuple>, _k: Option<&Bound<'py, PyDict>>) -> R<PyObject> {
    let t = Py::new(py, Texture::new(py, Some(path.str()?.as_any()), &PyTuple::empty_bound(py), None)?)?;
    if t.borrow(py).rs.is_none(py) {
        return Ok(py.None());
    }
    Ok(t.into_any())
}
#[pyfunction]
#[pyo3(signature = (*_a, **_k))]
fn ursina_load_model(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> PyObject {
    py.None()
}

fn tex_of<'py>(py: Python<'py>, t: &Bound<'py, PyAny>) -> R<PyObject> {
    if t.is_none() {
        return Ok(py.None());
    }
    if let Ok(x) = t.downcast::<Texture>() {
        return Ok(x.borrow().rs.clone_ref(py));
    }
    if t.is_instance_of::<RsTex>() {
        return Ok(t.clone().unbind());
    }
    if t.is_instance_of::<pyo3::types::PyString>() {
        let sauce = imp(py, "main.paths")?.getattr("SAUCE")?.call_method1("__truediv__", ("textures",))?;
        let s: String = t.extract()?;
        for p in [t.clone(), sauce.call_method1("__truediv__", (&s,))?, sauce.call_method1("__truediv__", (format!("{}.png", s),))?] {
            let r = super::gfx::gfx_tex(py, p.str()?.as_any(), false)?;
            if !r.is_none(py) {
                return Ok(r);
            }
        }
        return Ok(py.None());
    }
    if t.hasattr("convert")? {
        return Ok(Texture::new(py, Some(t), &PyTuple::empty_bound(py), None)?.rs);
    }
    Ok(py.None())
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

// ---------------------------------------------------------------------------
// 部品
// ---------------------------------------------------------------------------

/// ursina の Entity（画面の上の 2D の部品）
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Entity {
    #[pyo3(get, set)]
    _parent: PyObject,
    #[pyo3(get, set)]
    children: Py<PyList>,
    #[pyo3(get, set)]
    _seq: i64,
    #[pyo3(get, set)]
    _node: PyObject,
    #[pyo3(get, set)]
    _mesh: PyObject,
    #[pyo3(get, set)]
    _model: PyObject,
    #[pyo3(get, set)]
    _texture: PyObject,
    #[pyo3(get, set)]
    _color: PyObject,
    pos: [f64; 3],
    scl: [f64; 3],
    #[pyo3(get, set)]
    _origin: (f64, f64),
    #[pyo3(get, set)]
    _rot: f64,
    #[pyo3(get, set)]
    _enabled: bool,
    #[pyo3(get, set)]
    _alpha: f64,
    #[pyo3(get, set)]
    name: PyObject,
    #[pyo3(get, set)]
    ignore_input: PyObject,
    #[pyo3(get, set)]
    collider: PyObject,
    #[pyo3(get, set)]
    _alive: bool,
    #[pyo3(get, set)]
    _model_dirty: bool,
}

type B<'py> = Bound<'py, PyAny>;

fn sup<'a, 'py, T>(s: &'a Bound<'py, T>) -> R<&'a Bound<'py, Entity>> {
    Ok(s.as_any().downcast::<Entity>()?)
}

fn ent<'a, 'py>(o: &'a Bound<'py, PyAny>) -> R<&'a Bound<'py, Entity>> {
    Ok(o.downcast::<Entity>()?)
}

impl Entity {
    pub fn blank(py: Python<'_>, name: &str) -> R<Self> {
        Ok(Entity {
            _parent: py.None(), children: PyList::empty_bound(py).unbind(), _seq: 0, _node: py.None(), _mesh: py.None(), _model: py.None(),
            _texture: py.None(), _color: color(py, 1.0, 1.0, 1.0, 1.0)?.unbind(), pos: [0.0; 3], scl: [1.0; 3], _origin: (0.0, 0.0), _rot: 0.0,
            _enabled: true, _alpha: 1.0, name: name.into_py(py), ignore_input: false.into_py(py), collider: py.None(), _alive: true, _model_dirty: false,
        })
    }
    /// Entity.__init__（Python の子クラスの super().__init__ もここ）
    #[allow(clippy::too_many_arguments)]
    pub fn init(s: &B<'_>, parent: Option<&B<'_>>, model: Option<&B<'_>>, texture: Option<&B<'_>>, color_: Option<&B<'_>>, position: Option<&B<'_>>,
                scale: Option<&B<'_>>, origin: Option<&B<'_>>, rotation: Option<&B<'_>>, enabled: Option<&B<'_>>, z: Option<&B<'_>>, x: Option<&B<'_>>,
                y: Option<&B<'_>>, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let py = s.py();
        let nn = |o: Option<&B<'_>>| o.filter(|v| !v.is_none()).is_some();
        let kw = match kw { Some(k) => k.copy()?, None => PyDict::new_bound(py) };
        let tname = s.get_type().getattr("__name__")?;
        let name = match kw.get_item("name")? { Some(n) => { kw.del_item("name")?; n } None => tname };
        let collider = match kw.get_item("collider")? { Some(n) => { kw.del_item("collider")?; n.unbind() } None => py.None() };
        {
            let e = ent(s)?;
            let mut b = e.borrow_mut();
            b._parent = py.None();
            b.children = PyList::empty_bound(py).unbind();
            b._seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
            b._node = py.None();
            b._mesh = py.None();
            b._model = py.None();
            b._texture = py.None();
            b._color = color(py, 1.0, 1.0, 1.0, 1.0)?.unbind();
            b.pos = match position { Some(p) => it3(&vec3_of(py, p)?)?, None => [0.0; 3] };
            b.scl = [1.0; 3];
            b._origin = (0.0, 0.0);
            b._rot = 0.0;
            b._enabled = true;
            b._alpha = 1.0;
            b.name = name.unbind();
            b.ignore_input = false.into_py(py);
            b.collider = collider;
            b._alive = true;
        }
        let parent = match parent.filter(|p| !p.is_none()) { Some(p) => p.clone(), None => st(py)?.scene.bind(py).clone() };
        s.setattr("parent", parent)?;
        if nn(scale) {
            s.setattr("scale", scale.unwrap())?;
        }
        match origin {
            Some(o) if !o.is_none() => s.setattr("origin", o)?,
            Some(_) => {}
            None => s.setattr("origin", (0, 0))?,
        }
        if nn(rotation) {
            s.setattr("rotation", rotation.unwrap())?;
        }
        if nn(x) {
            s.setattr("x", x.unwrap())?;
        }
        if nn(y) {
            s.setattr("y", y.unwrap())?;
        }
        if nn(z) {
            s.setattr("z", z.unwrap())?;
        }
        if nn(color_) {
            s.setattr("color", color_.unwrap())?;
        }
        if nn(texture) {
            s.setattr("texture", texture.unwrap())?;
        }
        s.setattr("model", model.map(|m| m.clone()).unwrap_or_else(|| py.None().into_bound(py)))?;
        s.setattr("enabled", enabled.map(|m| m.clone()).unwrap_or_else(|| true.into_py(py).into_bound(py)))?;
        for (k, v) in kw.iter() {
            let _ = s.setattr(k.downcast::<pyo3::types::PyString>()?.clone(), v);
        }
        st(py)?.entities.bind(py).append(s)?;
        Ok(())
    }
    fn touch(s: &Bound<'_, Entity>) -> R<()> {
        if s.borrow()._alive {
            st(s.py())?.dirty.bind(s.py()).add(s)?;
        }
        Ok(())
    }
    fn parent_of<'py>(s: &Bound<'py, Entity>) -> Bound<'py, PyAny> {
        s.borrow()._parent.clone_ref(s.py()).into_bound(s.py())
    }
    fn node_of<'py>(s: &Bound<'py, Entity>) -> R<Bound<'py, PyAny>> {
        let py = s.py();
        let n = s.borrow()._node.clone_ref(py).into_bound(py);
        if !n.is_none() {
            return Ok(n);
        }
        let p = Self::parent_of(s);
        let pn = if p.is_none() { None } else { Some(p.call_method0("node")?) };
        let node = RsNode::create(py, pn.as_ref())?.into_any();
        s.borrow_mut()._node = node.clone().unbind();
        s.as_any().call_method0("_apply")?;
        Ok(node)
    }
    fn is_ui_(s: &Bound<'_, Entity>) -> R<bool> {
        let py = s.py();
        let ui = ui_root(py)?;
        let mut p = s.clone().into_any();
        while !p.is_none() {
            if p.is(&ui) {
                return Ok(true);
            }
            p = Self::parent_of(ent(&p)?);
        }
        Ok(false)
    }
    /// 自分から親へ（camera.ui の手前まで）
    fn chain<'py>(s: &Bound<'py, Entity>) -> R<Vec<Bound<'py, Entity>>> {
        let ui = ui_root(s.py())?;
        let mut out = vec![];
        let mut p = s.clone().into_any();
        while !p.is_none() && !p.is(&ui) {
            let e = ent(&p)?.clone();
            p = Self::parent_of(&e);
            out.push(e);
        }
        Ok(out)
    }
    fn world_z(s: &Bound<'_, Entity>) -> R<f64> {
        let mut z = 0.0;
        for e in Self::chain(s)? {
            z += e.borrow().pos[2];
        }
        Ok(z)
    }
    fn order(s: &Bound<'_, Entity>) -> R<i64> {
        let v = (-Self::world_z(s)? * 2000.0).min(2000.0).max(-2000.0);
        Ok((v.trunc() as i64) * 100000 + s.borrow()._seq.rem_euclid(100000))
    }
    fn world_xy(s: &Bound<'_, Entity>) -> R<(f64, f64)> {
        let (mut px, mut py_) = (0.0, 0.0);
        for e in Self::chain(s)? {
            let b = e.borrow();
            let r = (-b._rot).to_radians();
            let (c, sn) = (r.cos(), r.sin());
            let (qx, qy) = (px * b.scl[0], py_ * b.scl[1]);
            px = qx * c - qy * sn + b.pos[0];
            py_ = qx * sn + qy * c + b.pos[1];
        }
        Ok((px, py_))
    }
    fn world_scale_(s: &Bound<'_, Entity>) -> R<(f64, f64)> {
        let (mut sx, mut sy) = (1.0, 1.0);
        for e in Self::chain(s)? {
            let b = e.borrow();
            sx *= b.scl[0];
            sy *= b.scl[1];
        }
        Ok((sx, sy))
    }
    pub fn set_color_base(s: &Bound<'_, Entity>, c: &B<'_>) -> R<()> {
        let py = s.py();
        let c = py.import_bound("builtins")?.getattr("tuple")?.call1((c,))?;
        s.borrow_mut()._color = color_of(py, &c)?.unbind();
        if !s.borrow()._mesh.is_none(py) {
            s.as_any().call_method0("_paint")?;
        }
        Ok(())
    }
    fn set_origin_base(s: &Bound<'_, Entity>, v: &B<'_>) -> R<()> {
        let o0 = f64of(&v.get_item(0)?)?;
        let o1 = if v.len()? > 1 { f64of(&v.get_item(1)?)? } else { 0.0 };
        {
            let mut b = s.borrow_mut();
            b._origin = (o0, o1);
        }
        Ok(())
    }
    fn mouse_clear_if(py: Python<'_>, s: &B<'_>) -> R<()> {
        let m = mouse(py)?;
        if m.borrow().hovered_entity.bind(py).is(s) {
            m.borrow_mut().hovered_entity = py.None();
        }
        Ok(())
    }
}

#[pymethods]
impl Entity {
    #[classattr]
    fn default_shader(py: Python<'_>) -> PyObject {
        py.None()
    }
    #[classattr]
    #[allow(non_upper_case_globals)]
    const visible_self: bool = true;

    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<Self> {
        Entity::blank(py, "Entity")
    }
    #[pyo3(signature = (parent=None, model=None, texture=None, color=None, position=None, scale=None, origin=None, rotation=None, enabled=None,
                        z=None, x=None, y=None, eternal=None, **kw))]
    #[allow(clippy::too_many_arguments)]
    fn __init__(slf: &Bound<'_, Self>, parent: Option<&B<'_>>, model: Option<&B<'_>>, texture: Option<&B<'_>>, color: Option<&B<'_>>,
                position: Option<&B<'_>>, scale: Option<&B<'_>>, origin: Option<&B<'_>>, rotation: Option<&B<'_>>, enabled: Option<&B<'_>>,
                z: Option<&B<'_>>, x: Option<&B<'_>>, y: Option<&B<'_>>, eternal: Option<&B<'_>>, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let _ = eternal;
        let py = slf.py();
        let o00 = tb(py, (0, 0));
        Entity::init(slf.as_any(), parent, model, texture, color, position, scale, Some(origin.unwrap_or(&o00)), rotation, enabled, z, x, y, kw)
    }
    // ---- 親子 ----
    #[getter]
    fn parent(&self, py: Python<'_>) -> PyObject {
        self._parent.clone_ref(py)
    }
    #[setter(parent)]
    fn set_parent_p(slf: &Bound<'_, Self>, p: &B<'_>) -> R<()> {
        let py = slf.py();
        let old = Self::parent_of(slf);
        if !old.is_none() {
            let ch = old.getattr("children")?;
            if ch.contains(slf)? {
                ch.call_method1("remove", (slf,))?;
            }
        }
        slf.borrow_mut()._parent = p.clone().unbind();
        if !p.is_none() {
            p.getattr("children")?.call_method1("append", (slf,))?;
        }
        let n = slf.borrow()._node.clone_ref(py).into_bound(py);
        if !n.is_none() && !p.is_none() {
            n.setattr("parent", p.call_method0("node")?)?;
        }
        Self::touch(slf)
    }
    fn node<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        Self::node_of(slf)
    }
    #[getter]
    fn is_ui(slf: &Bound<'_, Self>) -> R<bool> {
        Self::is_ui_(slf)
    }
    // ---- 位置・大きさ ----
    fn _touch(slf: &Bound<'_, Self>) -> R<()> {
        Self::touch(slf)
    }
    /// ノードの行列（位置・回転・大きさ）と、形の z の順番
    fn _apply(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let n = slf.borrow()._node.clone_ref(py).into_bound(py);
        if n.is_none() || !slf.borrow()._alive {
            return Ok(());
        }
        let (m, en) = {
            let b = slf.borrow();
            let [sx, sy, sz] = b.scl;
            let r = (-b._rot).to_radians();
            let (c, s) = (r.cos(), r.sin());
            let [x, y, _] = b.pos;
            (vec![sx * c, sx * s, 0.0, 0.0, -sy * s, sy * c, 0.0, 0.0, 0.0, 0.0, sz, 0.0, x, y, 0.0, 1.0], b._enabled)
        };
        n.call_method1("setMat", (m,))?;
        let on = en && slf.getattr("visible_self")?.is_truthy()?;
        n.call_method0(if on { "show" } else { "hide" })?;
        Ok(())
    }
    fn _world_z(slf: &Bound<'_, Self>) -> R<f64> {
        Self::world_z(slf)
    }
    /// 画面の上の描く順（z が小さいほど後＝手前、同じなら作った順）
    fn _order(slf: &Bound<'_, Self>) -> R<i64> {
        Self::order(slf)
    }
    #[getter]
    fn position<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let p = slf.borrow().pos;
        vec3(slf.py(), p[0], p[1], p[2])
    }
    #[setter]
    fn set_position(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let v = vec3_of(slf.py(), v)?;
        Self::_set_pos(slf, &v)
    }
    #[getter]
    fn world_position<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let (x, y) = Self::world_xy(slf)?;
        vec3(slf.py(), x, y, Self::world_z(slf)?)
    }
    fn _set_pos(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let n = v.len()?;
        {
            let mut b = slf.borrow_mut();
            if n == 3 {
                b.pos = it3(v)?;
            } else {
                b.pos[0] = f64of(&v.get_item(0)?)?;
                b.pos[1] = f64of(&v.get_item(1)?)?;
            }
        }
        Self::touch(slf)
    }
    fn _seti(slf: &Bound<'_, Self>, i: usize, v: &B<'_>) -> R<()> {
        slf.borrow_mut().pos[i] = f64of(v)?;
        Self::touch(slf)
    }
    #[getter]
    fn _pos(&self) -> Vec<f64> {
        self.pos.to_vec()
    }
    #[setter(_pos)]
    fn set_pos_raw(&mut self, v: Vec<f64>) {
        for (k, x) in v.iter().take(3).enumerate() {
            self.pos[k] = *x;
        }
    }
    #[getter]
    fn _scale(&self) -> Vec<f64> {
        self.scl.to_vec()
    }
    #[setter(_scale)]
    fn set_scale_raw(&mut self, v: Vec<f64>) {
        for (k, x) in v.iter().take(3).enumerate() {
            self.scl[k] = *x;
        }
    }
    #[getter]
    fn x(&self) -> f64 {
        self.pos[0]
    }
    #[setter]
    fn set_x(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_seti(slf, 0, v)
    }
    #[getter]
    fn y(&self) -> f64 {
        self.pos[1]
    }
    #[setter]
    fn set_y(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_seti(slf, 1, v)
    }
    #[getter]
    fn z(&self) -> f64 {
        self.pos[2]
    }
    #[setter]
    fn set_z(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_seti(slf, 2, v)
    }
    #[getter]
    fn scale<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let s = slf.borrow().scl;
        vec3(slf.py(), s[0], s[1], s[2])
    }
    #[setter]
    fn set_scale(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let py = slf.py();
        if is_num(v) {
            let f = f64of(v)?;
            slf.borrow_mut().scl = [f; 3];
        } else {
            let l: Vec<B<'_>> = py.import_bound("builtins")?.getattr("list")?.call1((v,))?.iter()?.collect::<R<_>>()?;
            let z = if l.len() > 2 { f64of(&l[2])? } else { 1.0 };
            slf.borrow_mut().scl = [f64of(&l[0])?, f64of(&l[1])?, z];
        }
        Self::touch(slf)
    }
    fn _sets(slf: &Bound<'_, Self>, i: usize, v: &B<'_>) -> R<()> {
        slf.borrow_mut().scl[i] = f64of(v)?;
        Self::touch(slf)
    }
    #[getter]
    fn scale_x(&self) -> f64 {
        self.scl[0]
    }
    #[setter]
    fn set_scale_x(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_sets(slf, 0, v)
    }
    #[getter]
    fn scale_y(&self) -> f64 {
        self.scl[1]
    }
    #[setter]
    fn set_scale_y(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_sets(slf, 1, v)
    }
    #[getter]
    fn scale_z(&self) -> f64 {
        self.scl[2]
    }
    #[setter]
    fn set_scale_z(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_sets(slf, 2, v)
    }
    #[getter]
    fn world_scale<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let (sx, sy) = Self::world_scale_(slf)?;
        vec3(slf.py(), sx, sy, 1.0)
    }
    #[setter]
    fn set_world_scale(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let py = slf.py();
        let (a, b) = if is_num(v) { let f = f64of(v)?; (f, f) } else { (f64of(&v.get_item(0)?)?, f64of(&v.get_item(1)?)?) };
        let p = Self::parent_of(slf);
        let (px, py_) = if p.is_none() { (1.0, 1.0) } else { it2(&p.getattr("world_scale")?)? };
        slf.as_any().setattr("scale", tb(py, (a / px.max(1e-9), b / py_.max(1e-9), 1)))
    }
    #[getter]
    fn world_scale_x(slf: &Bound<'_, Self>) -> R<f64> {
        Ok(Self::world_scale_(slf)?.0)
    }
    #[getter]
    fn world_scale_y(slf: &Bound<'_, Self>) -> R<f64> {
        Ok(Self::world_scale_(slf)?.1)
    }
    #[getter]
    fn rotation<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        vec3(slf.py(), 0.0, 0.0, slf.borrow()._rot)
    }
    #[setter]
    fn set_rotation(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let r = if is_seq(v) { f64of(&v.get_item(2)?)? } else { f64of(v)? };
        slf.borrow_mut()._rot = r;
        Self::touch(slf)
    }
    #[getter]
    fn rotation_z(&self) -> f64 {
        self._rot
    }
    #[setter]
    fn set_rotation_z(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        slf.as_any().setattr("rotation", v)
    }
    #[getter]
    fn origin<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let o = slf.borrow()._origin;
        vec2(slf.py(), o.0, o.1)
    }
    #[setter]
    fn set_origin(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::set_origin_base(slf, v)?;
        slf.borrow_mut()._model_dirty = true;
        Self::touch(slf)
    }
    #[getter]
    fn origin_x(&self) -> f64 {
        self._origin.0
    }
    #[setter]
    fn set_origin_x(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let o = slf.borrow()._origin;
        slf.as_any().setattr("origin", (v, o.1))
    }
    #[getter]
    fn origin_y(&self) -> f64 {
        self._origin.1
    }
    #[setter]
    fn set_origin_y(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let o = slf.borrow()._origin;
        slf.as_any().setattr("origin", (o.0, v))
    }
    /// 画面の上の (x, y)（親の位置・大きさ・回転を含める。回転は z 軸だけ）
    fn _world_xy(slf: &Bound<'_, Self>) -> R<(f64, f64)> {
        Self::world_xy(slf)
    }
    /// 形（quad など）の画面の上の外枠 (x0, y0, x1, y1)。回転は無視
    fn _world_box(slf: &Bound<'_, Self>) -> R<(f64, f64, f64, f64)> {
        let (wx, wy) = Self::world_scale_(slf)?;
        let (cx, cy) = Self::world_xy(slf)?;
        let (ox, oy) = slf.borrow()._origin;
        let x0 = cx + (-0.5 - ox) * wx;
        let x1 = cx + (0.5 - ox) * wx;
        let y0 = cy + (-0.5 - oy) * wy;
        let y1 = cy + (0.5 - oy) * wy;
        Ok((x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)))
    }
    // ---- 見た目 ----
    #[getter]
    fn model(&self, py: Python<'_>) -> PyObject {
        self._model.clone_ref(py)
    }
    #[setter]
    fn set_model(slf: &Bound<'_, Self>, m: &B<'_>) -> R<()> {
        let py = slf.py();
        let m = if let Ok(s) = m.extract::<String>() {
            let ms = models(py)?;
            match ms.get_item(&s)? { Some(x) => x.unbind(), None => if s.is_empty() { py.None() } else { ms.get_item("quad")?.unwrap().unbind() } }
        } else {
            m.clone().unbind()
        };
        {
            let mut b = slf.borrow_mut();
            b._model = m;
            b._model_dirty = true;
        }
        slf.as_any().call_method0("_build_mesh")?;
        Ok(())
    }
    #[getter]
    fn texture(&self, py: Python<'_>) -> PyObject {
        self._texture.clone_ref(py)
    }
    #[setter]
    fn set_texture(slf: &Bound<'_, Self>, t: &B<'_>) -> R<()> {
        let py = slf.py();
        slf.borrow_mut()._texture = t.clone().unbind();
        let m = slf.borrow()._mesh.clone_ref(py).into_bound(py);
        if !m.is_none() {
            let r = tex_of(py, t)?;
            let id: PyObject = if r.is_none(py) { (-1i64).into_py(py) } else { r.bind(py).getattr("id")?.unbind() };
            view(py)?.call_method1("mesh_tex", (m.getattr("mid")?, 0, id))?;
        }
        Ok(())
    }
    #[getter]
    fn color(&self, py: Python<'_>) -> PyObject {
        self._color.clone_ref(py)
    }
    #[setter]
    fn set_color(slf: &Bound<'_, Self>, c: &B<'_>) -> R<()> {
        Self::set_color_base(slf, c)
    }
    #[getter]
    fn alpha(&self, py: Python<'_>) -> R<PyObject> {
        Ok(self._color.bind(py).get_item(3)?.unbind())
    }
    #[setter]
    fn set_alpha(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let py = slf.py();
        let c = slf.borrow()._color.clone_ref(py).into_bound(py);
        slf.as_any().setattr("color", (c.get_item(0)?, c.get_item(1)?, c.get_item(2)?, v))
    }
    fn _build_mesh(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        if !slf.borrow()._alive {
            return Ok(());
        }
        if slf.borrow()._model.is_none(py) || !Self::is_ui_(slf)? {
            // 3D の部品はまだ描かない（画面の上だけ）
            let m = slf.borrow()._mesh.clone_ref(py).into_bound(py);
            if !m.is_none() {
                m.call_method0("remove")?;
                slf.borrow_mut()._mesh = py.None();
            }
            return Ok(());
        }
        if slf.borrow()._mesh.is_none(py) {
            let node = Self::node_of(slf)?;
            let m = RsDynamicMesh::create(py, Some(&node))?;
            view(py)?.call_method1("mesh_overlay", (m.getattr("mid")?, Self::is_ui_(slf)?))?;
            slf.borrow_mut()._mesh = m.into_any().unbind();
        }
        slf.as_any().call_method("_paint", (), Some(&[("new", true)].into_py_dict_bound(py)))?;
        let t = slf.borrow()._texture.clone_ref(py);
        slf.as_any().setattr("texture", t)?;
        Self::touch(slf)
    }
    #[pyo3(signature = (new=false))]
    fn _paint(slf: &Bound<'_, Self>, new: bool) -> R<()> {
        let py = slf.py();
        let model = slf.borrow()._model.clone_ref(py).into_bound(py);
        let arr = model.call_method0("arrays")?;
        let np = imp(py, "numpy")?;
        let f64t = np.getattr("float64")?;
        let va = np.call_method1("ascontiguousarray", (arr.get_item(0)?, &f64t))?;
        let uva = np.call_method1("ascontiguousarray", (arr.get_item(1)?, &f64t))?;
        let t = arr.get_item(2)?;
        let vr: numpy::PyReadonlyArray2<f64> = va.extract()?;
        let uvr: numpy::PyReadonlyArray2<f64> = uva.extract()?;
        let v = vr.as_array();
        let uv = uvr.as_array();
        let n = v.nrows();
        let (ox, oy) = slf.borrow()._origin;
        let base: Vec<f64> = slf.borrow()._color.bind(py).iter()?.map(|x| f64of(&x?)).collect::<R<_>>()?;
        let cols = model.getattr("colors").unwrap_or_else(|_| py.None().into_bound(py));
        let mut out = vec![0f32; n * 12];
        let use_cols = !cols.is_none() && cols.len()? == n;
        let mut cv: Vec<[f64; 4]> = vec![];
        if use_cols {
            for c in cols.iter()? {
                let c = c?;
                let l: Vec<f64> = c.iter()?.map(|x| f64of(&x?)).collect::<R<_>>()?;
                cv.push(if l.len() >= 4 { [l[0], l[1], l[2], l[3]] } else { [l[0], l[1], l[2], 1.0] });
            }
        }
        for i in 0..n {
            let r = &mut out[i * 12..i * 12 + 12];
            r[0] = (v[[i, 0]] - ox) as f32;
            r[1] = (v[[i, 1]] - oy) as f32;
            r[5] = -1.0;
            for k in 0..4 {
                r[6 + k] = if use_cols { (cv[i][k] * base[k]) as f32 } else { base[k] as f32 };
            }
            r[10] = uv[[i, 0]] as f32;
            r[11] = uv[[i, 1]] as f32;
        }
        let a = numpy::PyArray1::from_vec_bound(py, out);
        let a = numpy::PyArrayMethods::reshape(&a, [n, 12])?;
        let mesh = slf.borrow()._mesh.clone_ref(py).into_bound(py);
        let nr: i64 = mesh.getattr("n_rows")?.extract()?;
        let tt: PyObject = if new || nr != n as i64 { t.unbind() } else { py.None() };
        mesh.call_method1("set", (a, tt))?;
        mesh.setattr("n_rows", n)?;
        Ok(())
    }
    fn _sort(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let m = slf.borrow()._mesh.clone_ref(py).into_bound(py);
        if !m.is_none() && slf.borrow()._alive {
            let b = BIN0 + Self::order(slf)?.div_euclid(100000) * 1000 + slf.borrow()._seq.rem_euclid(1000);
            view(py)?.call_method1("mesh_props", (m.getattr("mid")?, true, false, true, 0, b))?;
            if slf.borrow()._model_dirty {
                slf.borrow_mut()._model_dirty = false;
                slf.as_any().call_method0("_paint")?;
            }
        }
        Ok(())
    }
    // ---- 表示・入力 ----
    #[getter]
    fn enabled(&self) -> bool {
        self._enabled
    }
    #[setter]
    fn set_enabled(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let py = slf.py();
        let on = on.is_truthy()?;
        slf.borrow_mut()._enabled = on;
        Self::touch(slf)?;
        if !on {
            let m = mouse(py)?;
            if !m.borrow().hovered_entity.is_none(py) {
                m.borrow_mut().hovered_entity = py.None();
            }
        }
        Ok(())
    }
    #[getter]
    fn visible(&self) -> bool {
        self._enabled
    }
    #[setter]
    fn set_visible(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        Self::set_enabled(slf, on)
    }
    fn active_chain(slf: &Bound<'_, Self>) -> R<bool> {
        let mut p = slf.clone().into_any();
        while !p.is_none() {
            let e = ent(&p)?.clone();
            {
                let b = e.borrow();
                if !b._enabled || !b._alive {
                    return Ok(false);
                }
            }
            p = Self::parent_of(&e);
        }
        Ok(true)
    }
    fn show(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().setattr("enabled", true)
    }
    fn hide(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().setattr("enabled", false)
    }
    #[getter]
    fn hovered(slf: &Bound<'_, Self>) -> R<bool> {
        let py = slf.py();
        Ok(mouse(py)?.borrow().hovered_entity.bind(py).is(slf))
    }
    fn disable(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().setattr("enabled", false)
    }
    fn enable(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().setattr("enabled", true)
    }
    // Panda3D の NodePath の物はまねるだけ
    #[pyo3(name = "setTwoSided", signature = (*_a))]
    fn set_two_sided(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setDepthOffset", signature = (*_a))]
    fn set_depth_offset(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setBin", signature = (*_a))]
    fn set_bin(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setLightOff", signature = (*_a))]
    fn set_light_off(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setShaderOff", signature = (*_a))]
    fn set_shader_off(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setDepthTest", signature = (*_a))]
    fn set_depth_test(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setDepthWrite", signature = (*_a))]
    fn set_depth_write(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setTransparency", signature = (*_a))]
    fn set_transparency(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "set_two_sided", signature = (*_a))]
    fn set_two_sided2(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setColorScale", signature = (r, g, b, a=1.0))]
    fn set_color_scale(slf: &Bound<'_, Self>, r: f64, g: f64, b: f64, a: f64) -> R<()> {
        let py = slf.py();
        let n = Self::node_of(slf)?;
        view(py)?.call_method1("node_tint", (n.getattr("id")?, r, g, b, a))?;
        Ok(())
    }
    #[pyo3(signature = (v, *_a, **_k))]
    fn animate_scale(slf: &Bound<'_, Self>, v: &B<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<()> {
        slf.as_any().setattr("scale", v)
    }
    #[pyo3(signature = (v, *_a, **_k))]
    fn animate_color(slf: &Bound<'_, Self>, v: &B<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<()> {
        slf.as_any().setattr("color", v)
    }
    #[pyo3(signature = (*_a, **_k))]
    fn fade_out(&self, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) {}
    fn _destroy(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        if !slf.borrow()._alive {
            return Ok(());
        }
        slf.borrow_mut()._alive = false;
        let ch: Vec<B<'_>> = slf.borrow().children.bind(py).iter().collect();
        for c in ch {
            c.call_method0("_destroy")?;
        }
        let n = slf.borrow()._node.clone_ref(py).into_bound(py);
        if !n.is_none() {
            n.call_method0("removeNode")?;
            slf.borrow_mut()._node = py.None();
        }
        slf.borrow_mut()._mesh = py.None();
        let p = Self::parent_of(slf);
        if !p.is_none() {
            let ch = p.getattr("children")?;
            if ch.contains(slf)? {
                ch.call_method1("remove", (slf,))?;
            }
        }
        let s = st(py)?;
        let es = s.entities.bind(py);
        if es.contains(slf)? {
            es.call_method1("remove", (slf,))?;
        }
        s.dirty.bind(py).discard(slf)?;
        Self::mouse_clear_if(py, slf.as_any())?;
        if focus(py)?.is(slf) {
            set_focus(py, &py.None().into_bound(py))?;
        }
        Ok(())
    }
}

#[pyfunction]
#[pyo3(signature = (e, delay=None))]
pub fn ursina_destroy(py: Python<'_>, e: &B<'_>, delay: Option<&B<'_>>) -> R<()> {
    if e.is_none() {
        return Ok(());
    }
    if let Some(d) = delay.filter(|d| d.is_truthy().unwrap_or(false)) {
        let k = PyDict::new_bound(py);
        let _ = &k;
        return ursina_invoke(py, &imp(py, "alpedia_core")?.getattr("ursina_destroy")?, &PyTuple::new_bound(py, [e]), Some(d), None);
    }
    if e.is_instance_of::<Entity>() {
        e.call_method0("_destroy")?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 文字
// ---------------------------------------------------------------------------
#[pyclass(module = "alpedia_core", extends = Entity, subclass)]
pub struct Text {
    #[pyo3(get, set)]
    _text: String,
    #[pyo3(get, set)]
    _txt: PyObject,
    #[pyo3(get, set)]
    _wordwrap: i64,
    #[pyo3(get, set)]
    _wh: (f64, f64),
}


impl Text {
    fn wrap(&self, s: &str) -> String {
        if self._wordwrap <= 0 {
            return s.to_string();
        }
        let n = self._wordwrap as usize;
        let mut out: Vec<String> = vec![];
        for line in s.split('\n') {
            let mut ch: Vec<char> = line.chars().collect();
            while ch.len() > n {
                out.push(ch[..n].iter().collect());
                ch = ch[n..].to_vec();
            }
            out.push(ch.iter().collect());
        }
        out.join("\n")
    }
    fn render(slf: &Bound<'_, Text>) -> R<()> {
        let py = slf.py();
        let e = sup(slf)?;
        if !e.borrow()._alive || !Entity::is_ui_(e)? {
            return Ok(());
        }
        if slf.borrow()._txt.is_none(py) {
            let node = Entity::node_of(e)?;
            let k = PyDict::new_bound(py);
            k.set_item("overlay", true)?;
            let t = py.get_type_bound::<RsText>().call((node,), Some(&k))?;
            slf.borrow_mut()._txt = t.unbind();
        }
        let s = { let b = slf.borrow(); b.wrap(&b._text) };
        let ox = e.borrow()._origin.0;
        let al = if ox <= -0.25 { 0 } else if ox >= 0.25 { 2 } else { 1 }; // ursina と同じく、行ごとに origin の x でそろえる
        let txt = slf.borrow()._txt.clone_ref(py).into_bound(py);
        let wh: (f64, f64) = if !s.is_empty() {
            let c = py.import_bound("builtins")?.getattr("tuple")?.call1((e.borrow()._color.bind(py),))?;
            txt.call_method1("set", (&s, TEXT_SIZE * FONT_K, c, 0.0, al))?.extract()?
        } else {
            txt.call_method1("set", ("", TEXT_SIZE * FONT_K))?;
            (0.0, 0.0)
        };
        slf.borrow_mut()._wh = wh;
        Self::place(slf)?;
        Entity::touch(e)
    }
    fn place(slf: &Bound<'_, Text>) -> R<()> {
        let py = slf.py();
        let txt = slf.borrow()._txt.clone_ref(py).into_bound(py);
        if txt.is_none() {
            return Ok(());
        }
        let (w, h) = slf.borrow()._wh;
        let (ox, oy) = sup(slf)?.borrow()._origin;
        txt.getattr("node")?.call_method1("setPos", (-(ox + 0.5) * w, -(oy - 0.5) * h, 0.0))?;
        Ok(())
    }
}

#[pymethods]
impl Text {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const size: f64 = TEXT_SIZE;
    #[classattr]
    fn default_font(py: Python<'_>) -> PyObject {
        py.None()
    }
    #[classattr]
    fn default_monospace_font(py: Python<'_>) -> PyObject {
        py.None()
    }
    #[classattr]
    #[allow(non_upper_case_globals)]
    const default_resolution: i64 = 1080;

    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<(Self, Entity)> {
        Ok((Text { _text: String::new(), _txt: py.None(), _wordwrap: 0, _wh: (0.0, 0.0) }, Entity::blank(py, "Text")?))
    }
    #[pyo3(signature = (text=None, parent=None, origin=None, wordwrap=None, **kw))]
    fn __init__(slf: &Bound<'_, Self>, text: Option<&B<'_>>, parent: Option<&B<'_>>, origin: Option<&B<'_>>, wordwrap: Option<&B<'_>>,
                kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let py = slf.py();
        let ww: i64 = match wordwrap.filter(|w| w.is_truthy().unwrap_or(false)) {
            Some(w) => py.import_bound("builtins")?.getattr("int")?.call1((w,))?.extract()?,
            None => 0,
        };
        {
            let mut b = slf.borrow_mut();
            b._text = String::new();
            b._txt = py.None();
            b._wordwrap = ww;
            b._wh = (0.0, 0.0);
        }
        let kw = match kw { Some(k) => k.copy()?, None => PyDict::new_bound(py) };
        for k in ["start_tag", "end_tag", "font", "resolution", "background", "line_height"] {
            if kw.contains(k)? {
                kw.del_item(k)?;
            }
        }
        let col = match kw.get_item("color")? { Some(c) => { kw.del_item("color")?; c } None => color(py, 1.0, 1.0, 1.0, 1.0)? };
        let parent = match parent.filter(|p| !p.is_none()) { Some(p) => p.clone(), None => ui_root(py)? };
        let origin = match origin { Some(o) => o.clone(), None => tb(py, (-0.5, 0.5)) };
        kw.set_item("parent", parent)?;
        kw.set_item("origin", origin)?;
        // Entity.__init__(**kw)
        let g = |k: &str| -> R<Option<B<'_>>> {
            let v = kw.get_item(k)?;
            if v.is_some() {
                kw.del_item(k)?;
            }
            Ok(v)
        };
        let (pa, mo, te, co, po, sc, or, ro, en, z, x, y) = (g("parent")?, g("model")?, g("texture")?, g("color")?, g("position")?, g("scale")?,
            g("origin")?, g("rotation")?, g("enabled")?, g("z")?, g("x")?, g("y")?);
        let _ = g("eternal")?;
        Entity::init(slf.as_any(), pa.as_ref(), mo.as_ref(), te.as_ref(), co.as_ref(), po.as_ref(), sc.as_ref(), or.as_ref(), ro.as_ref(), en.as_ref(),
            z.as_ref(), x.as_ref(), y.as_ref(), Some(&kw))?;
        sup(slf)?.borrow_mut()._color = color_of(py, &col)?.unbind();
        let t: PyObject = match text { Some(t) => t.clone().unbind(), None => "".into_py(py) };
        slf.as_any().setattr("text", t)
    }
    fn _wrap(&self, s: &str) -> String {
        self.wrap(s)
    }
    #[getter]
    fn text(&self) -> String {
        self._text.clone()
    }
    #[setter]
    fn set_text(slf: &Bound<'_, Self>, s: &B<'_>) -> R<()> {
        let s = if s.is_none() { String::new() } else { s.str()?.to_string() };
        slf.borrow_mut()._text = s;
        slf.as_any().call_method0("_render")?;
        Ok(())
    }
    #[getter]
    fn wordwrap(&self) -> i64 {
        self._wordwrap
    }
    #[setter]
    fn set_wordwrap(slf: &Bound<'_, Self>, n: &B<'_>) -> R<()> {
        let py = slf.py();
        let v: i64 = if n.is_truthy()? { py.import_bound("builtins")?.getattr("int")?.call1((n,))?.extract()? } else { 0 };
        slf.borrow_mut()._wordwrap = v;
        slf.as_any().call_method0("_render")?;
        Ok(())
    }
    fn _render(slf: &Bound<'_, Self>) -> R<()> {
        Self::render(slf)
    }
    fn _place(slf: &Bound<'_, Self>) -> R<()> {
        Self::place(slf)
    }
    #[getter]
    fn color(slf: &Bound<'_, Self>) -> R<PyObject> {
        Ok(sup(slf)?.borrow()._color.clone_ref(slf.py()))
    }
    #[setter]
    fn set_color(slf: &Bound<'_, Self>, c: &B<'_>) -> R<()> {
        let py = slf.py();
        let c = py.import_bound("builtins")?.getattr("tuple")?.call1((c,))?;
        sup(slf)?.borrow_mut()._color = color_of(py, &c)?.unbind();
        let t = slf.borrow()._txt.clone_ref(py).into_bound(py);
        if !t.is_none() {
            t.setattr("key", py.None())?;
            slf.as_any().call_method0("_render")?;
        }
        Ok(())
    }
    #[getter]
    fn origin<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let o = sup(slf)?.borrow()._origin;
        vec2(slf.py(), o.0, o.1)
    }
    #[setter]
    fn set_origin(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let py = slf.py();
        Entity::set_origin_base(sup(slf)?, v)?;
        if !slf.borrow()._txt.is_none(py) {
            slf.as_any().call_method0("_render")?;
        }
        Entity::touch(sup(slf)?)
    }
    #[getter]
    fn width(&self) -> f64 {
        self._wh.0
    }
    #[getter]
    fn height(&self) -> f64 {
        self._wh.1
    }
    fn _sort(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let t = slf.borrow()._txt.clone_ref(py).into_bound(py);
        let e = sup(slf)?;
        if !t.is_none() && e.borrow()._alive {
            let b = BIN0 + Entity::order(e)?.div_euclid(100000) * 1000 + e.borrow()._seq.rem_euclid(1000) + 1;
            view(py)?.call_method1("mesh_props", (t.getattr("text")?.getattr("mid")?, true, false, true, 0, b))?;
        }
        Ok(())
    }
    fn _world_box(slf: &Bound<'_, Self>) -> R<(f64, f64, f64, f64)> {
        let e = sup(slf)?;
        let (wx, wy) = Entity::world_scale_(e)?;
        let (cx, cy) = Entity::world_xy(e)?;
        let (w0, h0) = slf.borrow()._wh;
        let (w, h) = (w0 * wx, h0 * wy);
        let (ox, oy) = e.borrow()._origin;
        let x0 = cx - (ox + 0.5) * w;
        let y1 = cy - (oy - 0.5) * h;
        Ok((x0, y1 - h, x0 + w, y1))
    }
}

// ---------------------------------------------------------------------------
// ボタン・入力欄
// ---------------------------------------------------------------------------
#[pyclass(module = "alpedia_core", extends = Entity, subclass)]
pub struct Button {
    #[pyo3(get, set)]
    _base: PyObject,
    #[pyo3(get, set)]
    highlight_color: PyObject,
    #[pyo3(get, set)]
    pressed_color: PyObject,
    #[pyo3(get, set)]
    text_entity: PyObject,
    #[pyo3(get, set)]
    _text_size: f64,
    #[pyo3(get, set)]
    on_click: PyObject,
    #[pyo3(get, set)]
    _text_color: PyObject,
}

impl Button {
    fn blank(py: Python<'_>) -> Self {
        Button { _base: py.None(), highlight_color: py.None(), pressed_color: py.None(), text_entity: py.None(), _text_size: 1.0, on_click: py.None(), _text_color: py.None() }
    }
    fn highlight(py: Python<'_>, base: &B<'_>) -> R<PyObject> {
        let v: Vec<f64> = base.iter()?.map(|x| f64of(&x?)).collect::<R<_>>()?;
        let f = |x: f64| 1f64.min(x * 1.25 + 0.05);
        Ok(cls(py, &COLOR)?.call1((f(v[0]), f(v[1]), f(v[2]), base.get_item(3)?))?.unbind())
    }
    fn fit_text(slf: &Bound<'_, Button>) -> R<()> {
        let py = slf.py();
        let te = slf.borrow().text_entity.clone_ref(py).into_bound(py);
        if !te.is_none() {
            let (wx, wy) = Entity::world_scale_(sup(slf)?)?;
            let ts = slf.borrow()._text_size;
            te.setattr("scale", (ts / wx.max(1e-6), ts / wy.max(1e-6), 1))?;
        }
        Ok(())
    }
    /// Button.__init__ の中身（InputField からも）
    fn init(s: &B<'_>, text: Option<&B<'_>>, parent: Option<&B<'_>>, model: Option<&B<'_>>, radius: &B<'_>, color_: Option<&B<'_>>,
            highlight_color: Option<&B<'_>>, pressed_color: Option<&B<'_>>, text_size: &B<'_>, text_color: Option<&B<'_>>, scale: Option<&B<'_>>,
            kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let py = s.py();
        let kw = match kw { Some(k) => k.copy()?, None => PyDict::new_bound(py) };
        for k in ["highlight_scale", "pressed_scale", "highlight_text_color", "icon", "disabled", "text_origin"] {
            if kw.contains(k)? {
                kw.del_item(k)?;
            }
        }
        let on_click = match kw.get_item("on_click")? { Some(c) => { kw.del_item("on_click")?; c.unbind() } None => py.None() };
        let quad = tb(py, "quad");
        let mut model = model.cloned().unwrap_or(quad);
        let qt = py.get_type_bound::<Quad>();
        if model.eq("quad")? && radius.is_truthy()? {
            model = qt.call((), Some(&[("radius", radius.clone()), ("aspect", 1.0f64.into_py(py).into_bound(py))].into_py_dict_bound(py)))?;
        }
        let b = s.downcast::<Button>()?;
        b.borrow_mut()._base = py.None();
        let parent = match parent.filter(|p| !p.is_none()) { Some(p) => p.clone(), None => ui_root(py)? };
        let scale = match scale.filter(|p| !p.is_none()) { Some(p) => p.clone(), None => tb(py, (0.25, 0.075)) };
        let g = |k: &str| -> R<Option<B<'_>>> {
            let v = kw.get_item(k)?;
            if v.is_some() {
                kw.del_item(k)?;
            }
            Ok(v)
        };
        let (te, po, or, ro, en, z, x, y) = (g("texture")?, g("position")?, g("origin")?, g("rotation")?, g("enabled")?, g("z")?, g("x")?, g("y")?);
        let _ = g("eternal")?;
        let o00 = tb(py, (0, 0));
        Entity::init(s, Some(&parent), Some(&model), te.as_ref(), None, po.as_ref(), Some(&scale), Some(or.as_ref().unwrap_or(&o00)), ro.as_ref(),
            en.as_ref(), z.as_ref(), x.as_ref(), y.as_ref(), Some(&kw))?;
        let e = sup(b)?;
        let cur = e.borrow()._model.clone_ref(py).into_bound(py);
        if cur.is_instance(&qt)? {
            let s_ = e.borrow().scl;
            s.setattr("model", qt.call((), Some(&[("radius", radius.clone()), ("aspect", tb(py, s_[0] / s_[1].max(1e-6)))].into_py_dict_bound(py)))?)?;
        }
        let base = match color_.filter(|c| !c.is_none()) { Some(c) => c.clone(), None => color(py, 0.18, 0.18, 0.2, 1.0)? };
        let tup = py.import_bound("builtins")?.getattr("tuple")?;
        let base = color_of(py, &tup.call1((base,))?)?;
        b.borrow_mut()._base = base.clone().unbind();
        let hc = match highlight_color.filter(|c| !c.is_none()) { Some(c) => c.clone().unbind(), None => Self::highlight(py, &base)? };
        s.setattr("highlight_color", &hc)?;
        let pc = match pressed_color.filter(|c| !c.is_none()) { Some(c) => c.clone().unbind(), None => s.getattr("highlight_color")?.unbind() };
        s.setattr("pressed_color", pc)?;
        Entity::set_color_base(e, &base)?;
        {
            let mut bb = b.borrow_mut();
            bb.text_entity = py.None();
            bb._text_size = f64of(text_size)?;
            bb.on_click = on_click;
            bb._text_color = text_color.map(|c| c.clone().unbind()).unwrap_or_else(|| py.None());
        }
        if let Some(t) = text.filter(|t| t.is_truthy().unwrap_or(false)) {
            s.setattr("text", t)?;
        }
        Ok(())
    }
}

#[pymethods]
impl Button {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<PyClassInitializer<Self>> {
        Ok(PyClassInitializer::from(Entity::blank(py, "Button")?).add_subclass(Button::blank(py)))
    }
    #[pyo3(signature = (text=None, parent=None, model=None, radius=None, color=None, highlight_color=None, pressed_color=None, text_size=None,
                        text_color=None, text_origin=None, scale=None, **kw))]
    #[allow(clippy::too_many_arguments)]
    fn __init__(slf: &Bound<'_, Self>, text: Option<&B<'_>>, parent: Option<&B<'_>>, model: Option<&B<'_>>, radius: Option<&B<'_>>, color: Option<&B<'_>>,
                highlight_color: Option<&B<'_>>, pressed_color: Option<&B<'_>>, text_size: Option<&B<'_>>, text_color: Option<&B<'_>>,
                text_origin: Option<&B<'_>>, scale: Option<&B<'_>>, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let _ = text_origin;
        let py = slf.py();
        let r01 = 0.1f64.into_py(py).into_bound(py);
        let one = 1i64.into_py(py).into_bound(py);
        Button::init(slf.as_any(), text, parent, model, radius.unwrap_or(&r01), color, highlight_color, pressed_color, text_size.unwrap_or(&one),
            text_color, scale, kw)
    }
    #[getter]
    fn color(slf: &Bound<'_, Self>) -> R<PyObject> {
        let py = slf.py();
        let b = slf.borrow()._base.clone_ref(py);
        Ok(if b.is_none(py) { sup(slf)?.borrow()._color.clone_ref(py) } else { b })
    }
    #[setter]
    fn set_color(slf: &Bound<'_, Self>, c: &B<'_>) -> R<()> {
        let py = slf.py();
        if slf.borrow()._base.is_none(py) {
            return Entity::set_color_base(sup(slf)?, c);
        }
        let tup = py.import_bound("builtins")?.getattr("tuple")?;
        let base = color_of(py, &tup.call1((c,))?)?;
        slf.borrow_mut()._base = base.clone().unbind();
        let hc = Self::highlight(py, &base)?;
        slf.as_any().setattr("highlight_color", hc)?;
        Entity::set_color_base(sup(slf)?, &base)
    }
    #[getter]
    fn text(slf: &Bound<'_, Self>) -> R<PyObject> {
        let py = slf.py();
        let te = slf.borrow().text_entity.clone_ref(py).into_bound(py);
        Ok(if te.is_none() { "".into_py(py) } else { te.getattr("text")?.unbind() })
    }
    #[setter]
    fn set_text(slf: &Bound<'_, Self>, s: &B<'_>) -> R<()> {
        let py = slf.py();
        if slf.borrow().text_entity.is_none(py) {
            let tc = slf.borrow()._text_color.clone_ref(py);
            let k = PyDict::new_bound(py);
            k.set_item("parent", slf)?;
            k.set_item("text", "")?;
            k.set_item("origin", (0, 0))?;
            k.set_item("z", -0.001)?;
            k.set_item("color", if tc.is_none(py) { color(py, 1.0, 1.0, 1.0, 1.0)?.unbind() } else { tc })?;
            let t = py.get_type_bound::<Text>().call((), Some(&k))?;
            slf.borrow_mut().text_entity = t.unbind();
            slf.as_any().call_method0("_fit_text")?;
        }
        slf.borrow().text_entity.bind(py).setattr("text", s)
    }
    #[getter]
    fn text_size(&self) -> f64 {
        self._text_size
    }
    #[setter]
    fn set_text_size(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        slf.borrow_mut()._text_size = f64of(v)?;
        slf.as_any().call_method0("_fit_text")?;
        Ok(())
    }
    fn _fit_text(slf: &Bound<'_, Self>) -> R<()> {
        Self::fit_text(slf)
    }
    #[getter]
    fn text_color(slf: &Bound<'_, Self>) -> R<PyObject> {
        let py = slf.py();
        let te = slf.borrow().text_entity.clone_ref(py).into_bound(py);
        Ok(if te.is_none() { color(py, 1.0, 1.0, 1.0, 1.0)?.unbind() } else { te.getattr("color")?.unbind() })
    }
    #[setter]
    fn set_text_color(slf: &Bound<'_, Self>, c: &B<'_>) -> R<()> {
        let py = slf.py();
        slf.borrow_mut()._text_color = c.clone().unbind();
        let te = slf.borrow().text_entity.clone_ref(py).into_bound(py);
        if !te.is_none() {
            te.setattr("color", c)?;
        }
        Ok(())
    }
    fn _hover(slf: &Bound<'_, Self>, on: bool) -> R<()> {
        let py = slf.py();
        let c = if on { slf.getattr("highlight_color")? } else { slf.borrow()._base.clone_ref(py).into_bound(py) };
        Entity::set_color_base(sup(slf)?, &c)
    }
}

/// InputField.text_field のまね（text・render・cursor）
#[pyclass(module = "alpedia_core", dict)]
pub struct TextField {
    #[pyo3(get, set)]
    owner: PyObject,
    #[pyo3(get, set)]
    text: PyObject,
    #[pyo3(get, set)]
    cursor: PyObject,
}

#[pyclass(module = "alpedia_core", dict)]
pub struct Cursor {
    #[pyo3(get, set)]
    x: PyObject,
    #[pyo3(get, set)]
    y: PyObject,
}

#[pymethods]
impl TextField {
    fn render(&self, py: Python<'_>) -> R<()> {
        self.owner.bind(py).call_method0("_show")?;
        Ok(())
    }
}

#[pyclass(module = "alpedia_core", extends = Button, subclass)]
pub struct InputField {
    #[pyo3(get, set)]
    text_field: PyObject,
    #[pyo3(get, set)]
    character_limit: PyObject,
    #[pyo3(get, set)]
    limit_content_to: PyObject,
    #[pyo3(get, set)]
    submit_on: PyObject,
    #[pyo3(get, set)]
    on_submit: PyObject,
    #[pyo3(get, set)]
    on_value_changed: PyObject,
    #[pyo3(get, set)]
    _shown: PyObject,
    #[pyo3(get, set)]
    _active: bool,
}

#[pymethods]
impl InputField {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<PyClassInitializer<Self>> {
        Ok(PyClassInitializer::from(Entity::blank(py, "InputField")?).add_subclass(Button::blank(py)).add_subclass(InputField {
            text_field: py.None(), character_limit: 24.into_py(py), limit_content_to: py.None(), submit_on: PyList::empty_bound(py).into_any().unbind(),
            on_submit: py.None(), on_value_changed: py.None(), _shown: py.None(), _active: false,
        }))
    }
    #[pyo3(signature = (default_value=None, label=None, max_lines=1, character_limit=None, text=None, active=None, **kw))]
    #[allow(clippy::too_many_arguments)]
    fn __init__(slf: &Bound<'_, Self>, default_value: Option<&B<'_>>, label: Option<&B<'_>>, max_lines: i64, character_limit: Option<PyObject>,
                text: Option<&B<'_>>, active: Option<&B<'_>>, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let _ = label;
        let py = slf.py();
        let kw = match kw { Some(k) => k.copy()?, None => PyDict::new_bound(py) };
        if !kw.contains("scale")? {
            kw.set_item("scale", (0.5, TEXT_SIZE * 2.0 * max_lines as f64))?;
        }
        if !kw.contains("color")? {
            kw.set_item("color", color(py, 0.1, 0.1, 0.12, 1.0)?)?;
        }
        // Button.__init__(**kw)
        let g = |k: &str| -> R<Option<B<'_>>> {
            let v = kw.get_item(k)?;
            if v.is_some() {
                kw.del_item(k)?;
            }
            Ok(v)
        };
        let (te, pa, mo, ra, co, hc, pc, ts, tc, _to, sc) = (g("text")?, g("parent")?, g("model")?, g("radius")?, g("color")?, g("highlight_color")?,
            g("pressed_color")?, g("text_size")?, g("text_color")?, g("text_origin")?, g("scale")?);
        let r01 = 0.1f64.into_py(py).into_bound(py);
        let one = 1i64.into_py(py).into_bound(py);
        let _ = te;
        Button::init(slf.as_any(), None, pa.as_ref(), mo.as_ref(), ra.as_ref().unwrap_or(&r01), co.as_ref(), hc.as_ref(), pc.as_ref(),
            ts.as_ref().unwrap_or(&one), tc.as_ref(), sc.as_ref(), Some(&kw))?;
        let cur = Py::new(py, Cursor { x: 0.into_py(py), y: 0.into_py(py) })?;
        fn pick<'p>(o: Option<&B<'p>>) -> Option<B<'p>> { o.filter(|v| v.is_truthy().unwrap_or(false)).cloned() }
        let init_text = pick(text).or_else(|| pick(default_value));
        let t = match init_text { Some(t) => t.str()?.into_any().unbind(), None => "".into_py(py) };
        let tf = Py::new(py, TextField { owner: slf.clone().into_any().unbind(), text: t, cursor: cur.into_any() })?;
        {
            let mut b = slf.borrow_mut();
            b.text_field = tf.into_any();
            b.character_limit = character_limit.unwrap_or_else(|| 24.into_py(py));
            b.limit_content_to = py.None();
            b.submit_on = PyList::empty_bound(py).into_any().unbind();
            b.on_submit = py.None();
            b.on_value_changed = py.None();
        }
        let k = PyDict::new_bound(py);
        k.set_item("parent", slf)?;
        k.set_item("text", "")?;
        k.set_item("origin", (-0.5, 0))?;
        k.set_item("x", -0.48)?;
        k.set_item("z", -0.001)?;
        let shown = py.get_type_bound::<Text>().call((), Some(&k))?;
        slf.borrow_mut()._shown = shown.clone().unbind();
        let (wx, wy) = Entity::world_scale_(sup(slf)?)?;
        shown.setattr("scale", (1.0 / wx.max(1e-6), 1.0 / wy.max(1e-6), 1))?;
        slf.as_any().setattr("on_click", slf.getattr("_focus")?)?;
        slf.borrow_mut()._active = false;
        slf.as_any().setattr("active", active.map(|a| a.is_truthy()).transpose()?.unwrap_or(false))
    }
    #[getter]
    fn active(&self) -> bool {
        self._active
    }
    #[setter]
    fn set_active(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let py = slf.py();
        let on = on.is_truthy()?;
        slf.borrow_mut()._active = on;
        if on {
            set_focus(py, slf.as_any())?;
        } else if focus(py)?.is(slf) {
            set_focus(py, &py.None().into_bound(py))?;
        }
        slf.as_any().call_method0("_show")?;
        Ok(())
    }
    fn _focus(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().setattr("active", true)
    }
    #[getter]
    fn value(&self, py: Python<'_>) -> R<PyObject> {
        Ok(self.text_field.bind(py).getattr("text")?.unbind())
    }
    fn _show(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let sh = slf.borrow()._shown.clone_ref(py).into_bound(py);
        if !sh.is_none() {
            let t = slf.borrow().text_field.bind(py).getattr("text")?.str()?.to_string();
            sh.setattr("text", format!("{}{}", t, if slf.borrow()._active { "｜" } else { "" }))?;
        }
        Ok(())
    }
    #[pyo3(name = "type")]
    fn type_(slf: &Bound<'_, Self>, t: &str) -> R<()> {
        let py = slf.py();
        let lim = slf.borrow().limit_content_to.clone_ref(py).into_bound(py);
        let t: String = if lim.is_truthy()? {
            let mut o = String::new();
            for ch in t.chars() {
                if lim.contains(ch.to_string())? {
                    o.push(ch);
                }
            }
            o
        } else {
            t.to_string()
        };
        let tf = slf.borrow().text_field.clone_ref(py).into_bound(py);
        let mut s = format!("{}{}", tf.getattr("text")?.str()?, t);
        let cl = slf.borrow().character_limit.clone_ref(py).into_bound(py);
        if cl.is_truthy()? {
            let n: i64 = cl.extract()?;
            let ch: Vec<char> = s.chars().collect();
            let n = if n < 0 { (ch.len() as i64 + n).max(0) as usize } else { (n as usize).min(ch.len()) };
            s = ch[..n].iter().collect();
        }
        tf.setattr("text", s)?;
        slf.as_any().call_method0("_show")?;
        let ov = slf.borrow().on_value_changed.clone_ref(py).into_bound(py);
        if ov.is_truthy()? {
            ov.call0()?;
        }
        Ok(())
    }
    fn key(slf: &Bound<'_, Self>, k: &str) -> R<bool> {
        let py = slf.py();
        if k == "backspace" {
            let tf = slf.borrow().text_field.clone_ref(py).into_bound(py);
            let mut s = tf.getattr("text")?.str()?.to_string();
            s.pop();
            tf.setattr("text", s)?;
            slf.as_any().call_method0("_show")?;
            return Ok(true);
        }
        let os = slf.borrow().on_submit.clone_ref(py).into_bound(py);
        if (k == "enter" || k == "return") && os.is_truthy()? {
            os.call0()?;
            return Ok(true);
        }
        Ok(k == "space" || k.chars().count() == 1)
    }
}

// ---------------------------------------------------------------------------
// 画面・マウス・時間
// ---------------------------------------------------------------------------
#[pyclass(module = "alpedia_core", name = "_Mouse", dict)]
pub struct Mouse {
    #[pyo3(get, set)]
    x: f64,
    #[pyo3(get, set)]
    y: f64,
    #[pyo3(get, set)]
    locked: PyObject,
    #[pyo3(get, set)]
    visible: PyObject,
    #[pyo3(get, set)]
    hovered_entity: PyObject,
    #[pyo3(get, set)]
    left: bool,
    #[pyo3(get, set)]
    right: PyObject,
    #[pyo3(get, set)]
    velocity: PyObject,
}

impl Mouse {
    fn make(py: Python<'_>) -> R<Self> {
        Ok(Mouse { x: 0.0, y: 0.0, locked: false.into_py(py), visible: true.into_py(py), hovered_entity: py.None(), left: false, right: false.into_py(py),
            velocity: vec3(py, 0.0, 0.0, 0.0)?.unbind() })
    }
}

#[pymethods]
impl Mouse {
    #[getter]
    fn position<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec3(py, self.x, self.y, 0.0)
    }
}

#[pyclass(module = "alpedia_core", name = "_Window", dict)]
pub struct Window {
    #[pyo3(get, set)]
    _size: PyObject,
    #[pyo3(get, set)]
    color: PyObject,
    #[pyo3(get, set)]
    _fullscreen: bool,
    #[pyo3(get, set)]
    borderless: PyObject,
    #[pyo3(get, set)]
    title: PyObject,
    #[pyo3(get, set)]
    vsync: PyObject,
}

impl Window {
    fn make(py: Python<'_>) -> R<Self> {
        Ok(Window { _size: (1280, 720).into_py(py), color: color(py, 0.45, 0.65, 0.9, 1.0)?.unbind(), _fullscreen: false, borderless: false.into_py(py),
            title: "Alpedia".into_py(py), vsync: true.into_py(py) })
    }
    fn wh(&self, py: Python<'_>) -> R<(f64, f64)> {
        it2(self._size.bind(py))
    }
    fn ar(&self, py: Python<'_>) -> R<f64> {
        let (w, h) = self.wh(py)?;
        Ok(w / h.max(1.0))
    }
}

fn gview(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    imp(py, "main.client.gfx")?.getattr("VIEW")
}

#[pymethods]
impl Window {
    #[getter]
    fn size<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        cls(py, &VEC2)?.call1(self._size.bind(py).downcast::<PyTuple>().map(|t| t.clone()).or_else(|_| {
            PyTuple::new_bound(py, self._size.bind(py).iter()?.collect::<R<Vec<_>>>()?).into_any().downcast_into::<PyTuple>().map_err(PyErr::from)
        })?)
    }
    /// 窓の大きさを変える（設定の解像度）
    #[setter]
    fn set_size(&mut self, py: Python<'_>, v: &B<'_>) -> R<()> {
        let int = py.import_bound("builtins")?.getattr("int")?;
        let w: i64 = int.call1((v.get_item(0)?,))?.extract()?;
        let h: i64 = int.call1((v.get_item(1)?,))?.extract()?;
        let gv = gview(py)?;
        let cur = PyTuple::new_bound(py, self._size.bind(py).iter()?.collect::<R<Vec<_>>>()?);
        if !gv.is_none() && !tb(py, (w, h)).eq(&cur)? {
            gv.call_method1("set_size", (w, h))?;
        }
        self._size = (w, h).into_py(py);
        Ok(())
    }
    #[getter]
    fn fullscreen(&self, py: Python<'_>) -> R<PyObject> {
        let gv = gview(py)?;
        Ok(if gv.is_none() { self._fullscreen.into_py(py) } else { gv.call_method0("fullscreen")?.unbind() })
    }
    #[setter]
    fn set_fullscreen(&mut self, py: Python<'_>, on: &B<'_>) -> R<()> {
        let on = on.is_truthy()?;
        self._fullscreen = on;
        let gv = gview(py)?;
        if !gv.is_none() {
            gv.call_method1("set_fullscreen", (on,))?;
        }
        Ok(())
    }
    #[getter]
    fn aspect_ratio(&self, py: Python<'_>) -> R<f64> {
        self.ar(py)
    }
    #[getter]
    fn top<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, 0.0, 0.5)
    }
    #[getter]
    fn bottom<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, 0.0, -0.5)
    }
    #[getter]
    fn left<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, -self.ar(py)? / 2.0, 0.0)
    }
    #[getter]
    fn right<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, self.ar(py)? / 2.0, 0.0)
    }
    #[getter]
    fn top_left<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, -self.ar(py)? / 2.0, 0.5)
    }
    #[getter]
    fn top_right<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, self.ar(py)? / 2.0, 0.5)
    }
    #[getter]
    fn bottom_left<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, -self.ar(py)? / 2.0, -0.5)
    }
    #[getter]
    fn bottom_right<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, self.ar(py)? / 2.0, -0.5)
    }
}

#[pyclass(module = "alpedia_core", name = "_Time", dict)]
pub struct Time {
    #[pyo3(get, set)]
    dt: f64,
}

#[pyclass(module = "alpedia_core", name = "_App", dict)]
pub struct App {
    #[pyo3(get, set)]
    paused: PyObject,
    #[pyo3(get, set)]
    fonts_folder: PyObject,
    #[pyo3(get, set)]
    base: PyObject,
    #[pyo3(get, set)]
    asset_folder: PyObject,
}

#[pymethods]
impl App {
    #[staticmethod]
    fn quit() {}
}

#[pyclass(module = "alpedia_core", name = "_Lens")]
pub struct Lens {}
#[pymethods]
impl Lens {
    #[pyo3(name = "getFov")]
    fn get_fov(&self, py: Python<'_>) -> R<PyObject> {
        Ok(st(py)?.camera.borrow(py).fov_pair.clone_ref(py))
    }
}
#[pyclass(module = "alpedia_core", name = "_CamNode")]
pub struct CamNode {}
#[pymethods]
impl CamNode {
    #[pyo3(name = "getLens")]
    fn get_lens(&self) -> Lens {
        Lens {}
    }
}
#[pyclass(module = "alpedia_core", name = "_Cam")]
pub struct Cam {}
#[pymethods]
impl Cam {
    fn node(&self) -> CamNode {
        CamNode {}
    }
}
/// application.base のまね（MOD が画面の視野角を読む：base.cam.node().getLens().getFov()）
#[pyclass(module = "alpedia_core", name = "_Base")]
pub struct Base {}
#[pymethods]
impl Base {
    #[classattr]
    fn cam() -> Cam {
        Cam {}
    }
}

#[pyclass(module = "alpedia_core", extends = Entity, subclass, name = "_Scene")]
pub struct Scene {}

fn scene_new<'py>(py: Python<'py>, name: &str) -> R<Bound<'py, PyAny>> {
    let e = Entity::blank(py, name)?;
    Ok(Bound::new(py, PyClassInitializer::from(e).add_subclass(Scene {}))?.into_any())
}

#[pyclass(module = "alpedia_core", name = "_Camera", dict)]
pub struct Camera {
    #[pyo3(get, set)]
    ui: PyObject,
    #[pyo3(get, set)]
    fov: PyObject,
    #[pyo3(get, set)]
    fov_pair: PyObject, // (横, 縦) の視野角（Rust の画面が毎フレーム入れる）
    #[pyo3(get, set)]
    position: PyObject,
    #[pyo3(get, set)]
    clip_plane_near: PyObject,
    #[pyo3(get, set)]
    clip_plane_far: PyObject,
    #[pyo3(get, set)]
    orthographic: PyObject,
}

#[pyfunction]
#[pyo3(signature = (f, *args, delay=None, **kw))]
pub fn ursina_invoke(py: Python<'_>, f: &B<'_>, args: &Bound<'_, PyTuple>, delay: Option<&B<'_>>, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
    let d = match delay { Some(d) => f64of(d)?, None => 0.0 };
    let now: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
    let k: PyObject = match kw { Some(k) => k.clone().into_any().unbind(), None => PyDict::new_bound(py).into_any().unbind() };
    st(py)?.invokes.bind(py).append(PyTuple::new_bound(py, [(now + d).into_py(py), f.clone().unbind(), args.clone().into_any().unbind(), k]))
}

#[pyclass(module = "alpedia_core", dict)]
pub struct Sequence {}
#[pymethods]
impl Sequence {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        Sequence {}
    }
    fn start(&self) {}
    fn kill(&self) {}
}

#[pyclass(module = "alpedia_core", dict)]
pub struct Audio {}
#[pymethods]
impl Audio {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        Audio {}
    }
    fn play(&self) {}
    #[pyo3(signature = (*_a, **_k))]
    fn stop(&self, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) {}
}

/// ursina の input_handler（Ctrl などと組み合わせたキーの名前）
#[pyclass(module = "alpedia_core", name = "_InputHandler")]
pub struct InputHandler {}
#[pymethods]
impl InputHandler {
    #[staticmethod]
    fn get_combined_key(py: Python<'_>, key: &str) -> R<String> {
        let hk = st(py)?.held_keys.bind(py);
        let mut mods: Vec<&str> = vec![];
        for (m, ks) in [("control", ["left control", "right control"]), ("shift", ["left shift", "right shift"]), ("alt", ["left alt", "right alt"])] {
            let mut any = false;
            for k in ks {
                if hk.call_method1("get", (k,))?.is_truthy()? {
                    any = true;
                }
            }
            if any {
                mods.push(m);
            }
        }
        let base = if key.starts_with("left ") || key.starts_with("right ") { key.split(' ').last().unwrap_or(key) } else { key };
        mods.retain(|m| *m != base);
        if mods.is_empty() {
            return Ok(key.to_string());
        }
        let mut v: Vec<String> = mods.iter().map(|s| s.to_string()).collect();
        v.push(key.to_string());
        Ok(v.join("+"))
    }
}

#[pyclass(module = "alpedia_core", dict)]
pub struct Ursina {}
#[pymethods]
impl Ursina {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        Ursina {}
    }
}

// ---------------------------------------------------------------------------
// Rust の画面からまわす
// ---------------------------------------------------------------------------
fn overrides(e: &B<'_>, name: &str) -> R<bool> {
    let t = e.get_type();
    let et = e.py().get_type_bound::<Entity>();
    let a = t.getattr(name).ok();
    let b = et.getattr(name).ok();
    let differ = match (&a, &b) {
        (None, None) => false,
        (Some(x), Some(y)) => !x.is(y),
        _ => true,
    };
    Ok(differ && e.hasattr(name)?)
}

/// 毎フレーム：invoke・update() を呼び、位置・描く順を Rust の画面へ
#[pyfunction]
pub fn ursina_frame(py: Python<'_>, view_: &B<'_>, dt: f64) -> R<()> {
    let s = st(py)?;
    s.time.borrow_mut(py).dt = dt;
    let wh = view_.call_method0("size")?;
    s.window.borrow_mut(py)._size = PyTuple::new_bound(py, wh.iter()?.collect::<R<Vec<_>>>()?).into_any().unbind();
    let now: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
    let inv = s.invokes.bind(py);
    let due: Vec<B<'_>> = inv.iter().filter(|x| x.get_item(0).and_then(|t| t.extract::<f64>()).map(|t| t <= now).unwrap_or(false)).collect();
    for x in due {
        inv.call_method1("remove", (&x,))?;
        let k = x.get_item(3)?;
        if let Err(e) = x.get_item(1)?.call(x.get_item(2)?.downcast::<PyTuple>()?, Some(k.downcast::<PyDict>()?)) {
            log_exc(py, "invoke に失敗しました", e)?;
        }
    }
    let es: Vec<B<'_>> = s.entities.bind(py).iter().collect();
    for e in &es {
        let ee = ent(e)?;
        if ee.borrow()._alive && overrides(e, "update")? && Entity::active_chain(ee)? {
            e.call_method0("update")?;
        }
    }
    let dirty = s.dirty.bind(py);
    let ds: Vec<B<'_>> = dirty.iter().collect();
    for e in &ds {
        if ent(e)?.borrow()._alive {
            e.call_method0("_apply")?;
        }
    }
    if !dirty.is_empty() {
        let es: Vec<B<'_>> = s.entities.bind(py).iter().collect();
        for e in &es {
            let ee = ent(e)?;
            if ee.borrow()._alive && Entity::is_ui_(ee)? {
                e.call_method0("_sort")?;
            }
        }
    }
    dirty.clear();
    Ok(())
}

/// マウスの下のいちばん手前のボタン
fn hit<'py>(py: Python<'py>, x: f64, y: f64) -> R<Option<Bound<'py, PyAny>>> {
    let mut best: Option<((f64, i64), B<'py>)> = None;
    let es: Vec<B<'py>> = st(py)?.entities.bind(py).iter().collect();
    for e in es {
        if !e.is_instance_of::<Button>() {
            continue;
        }
        let ee = ent(&e)?.clone();
        if !ee.borrow()._alive || e.getattr("ignore_input")?.is_truthy()? || !Entity::active_chain(&ee)? || !Entity::is_ui_(&ee)? {
            continue;
        }
        let (x0, y0, x1, y1): (f64, f64, f64, f64) = e.call_method0("_world_box")?.extract()?;
        if x0 <= x && x <= x1 && y0 <= y && y <= y1 {
            let k = (Entity::world_z(&ee)?, -ee.borrow()._seq);
            let better = match &best { None => true, Some((bk, _)) => k.0 < bk.0 || (k.0 == bk.0 && k.1 < bk.1) };
            if better {
                best = Some((k, e));
            }
        }
    }
    Ok(best.map(|b| b.1))
}

#[pyfunction]
fn ursina_hit<'py>(py: Python<'py>, x: f64, y: f64) -> R<PyObject> {
    Ok(hit(py, x, y)?.map(|h| h.unbind()).unwrap_or_else(|| py.None()))
}

/// 入力を ursina の形（"left mouse down" など）にして配る。使った（画面が開いている）なら True
#[pyfunction]
pub fn ursina_event(py: Python<'_>, view_: &B<'_>, kind: &str, a: &B<'_>, b: &B<'_>, n: i64) -> R<bool> {
    let m = mouse(py)?;
    if kind == "cursor" {
        let (w, h): (f64, f64) = it2(&view_.call_method0("size")?)?;
        let (ax, by) = (f64of(a)?, f64of(b)?);
        {
            let mut mb = m.borrow_mut();
            mb.x = (ax - w / 2.0) / h;
            mb.y = (h / 2.0 - by) / h;
        }
        let (mx, my) = { let mb = m.borrow(); (mb.x, mb.y) };
        let hb = hit(py, mx, my)?;
        let old = m.borrow().hovered_entity.clone_ref(py).into_bound(py);
        let same = match &hb { Some(h) => h.is(&old), None => old.is_none() };
        if !same {
            if old.is_instance_of::<Button>() && ent(&old)?.borrow()._alive {
                old.call_method1("_hover", (false,))?;
            }
            if let Some(h) = &hb {
                if h.is_instance_of::<Button>() {
                    h.call_method1("_hover", (true,))?;
                }
            }
            m.borrow_mut().hovered_entity = hb.map(|h| h.unbind()).unwrap_or_else(|| py.None());
        }
        return Ok(true);
    }
    if kind == "button" {
        let down = a.is_truthy()?;
        let name = format!("{} mouse {}", ["left", "right", "middle", "x"][n.clamp(0, 3) as usize], if down { "down" } else { "up" });
        if n == 0 {
            m.borrow_mut().left = down;
        }
        if down && n == 0 {
            let (mx, my) = { let mb = m.borrow(); (mb.x, mb.y) };
            if let Some(h) = hit(py, mx, my)? {
                let f = h.getattr("on_click")?;
                if !f.is_none() && f.is_callable() {
                    f.call0()?;
                }
            }
        }
        send(py, &name)?;
        return Ok(true);
    }
    if kind == "wheel" {
        send(py, if f64of(a)? > 0.0 { "scroll up" } else { "scroll down" })?;
        return Ok(true);
    }
    if let Some(k0) = kind.strip_prefix("key:") {
        let k = match k0 { "control" => "left control", "shift" => "left shift", "alt" => "left alt", x => x };
        let down = a.is_truthy()?;
        st(py)?.held_keys.bind(py).set_item(k, if down { 1 } else { 0 })?;
        let f = focus(py)?;
        if down && !f.is_none() && ent(&f)?.borrow()._alive && f.call_method1("key", (k,))?.is_truthy()? {
            return Ok(true);
        }
        send(py, &if down { k.to_string() } else { format!("{} up", k) })?;
        return Ok(true);
    }
    if let Some(t) = kind.strip_prefix("text:") {
        let f = focus(py)?;
        if !f.is_none() && ent(&f)?.borrow()._alive {
            f.call_method1("type", (t,))?;
        }
        return Ok(true);
    }
    Ok(kind == "mouse")
}

fn send(py: Python<'_>, key: &str) -> R<()> {
    let es: Vec<B<'_>> = st(py)?.entities.bind(py).iter().collect();
    for e in es {
        let ee = ent(&e)?.clone();
        if ee.borrow()._alive && overrides(&e, "input")? && Entity::active_chain(&ee)? {
            if let Err(err) = e.call_method1("input", (key,)) {
                log_exc(py, "input に失敗しました", err)?;
            }
        }
    }
    Ok(())
}

#[pyfunction]
fn ursina_send(py: Python<'_>, key: &str) -> R<()> {
    send(py, key)
}

/// 画面の上に ursina の部品が出ているか
#[pyfunction]
pub fn ursina_any_ui(py: Python<'_>) -> R<bool> {
    let ui = ui_root(py)?;
    for c in ui.getattr("children")?.iter()? {
        let c = c?;
        let e = ent(&c)?;
        if e.borrow()._alive && Entity::active_chain(e)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// ursina_shim のモジュールに入れる名前（部品・全体の状態）
#[pyfunction]
pub fn ursina_names<'py>(py: Python<'py>) -> R<Bound<'py, PyDict>> {
    let s = st(py)?;
    let d = PyDict::new_bound(py);
    d.set_item("Color", cls(py, &COLOR)?)?;
    d.set_item("Vec2", cls(py, &VEC2)?)?;
    d.set_item("Vec3", cls(py, &VEC3)?)?;
    d.set_item("color", s.color.bind(py))?;
    d.set_item("_ColorMod", py.get_type_bound::<ColorMod>())?;
    d.set_item("Mesh", py.get_type_bound::<Mesh>())?;
    d.set_item("Quad", py.get_type_bound::<Quad>())?;
    let ms = models(py)?;
    d.set_item("_QUAD", ms.get_item("quad")?)?;
    d.set_item("_MODELS", ms)?;
    d.set_item("Texture", py.get_type_bound::<Texture>())?;
    d.set_item("Entity", py.get_type_bound::<Entity>())?;
    d.set_item("Text", py.get_type_bound::<Text>())?;
    d.set_item("Button", py.get_type_bound::<Button>())?;
    d.set_item("InputField", py.get_type_bound::<InputField>())?;
    d.set_item("_TextField", py.get_type_bound::<TextField>())?;
    d.set_item("_entities", s.entities.bind(py))?;
    d.set_item("_dirty", s.dirty.bind(py))?;
    d.set_item("_focus", s.focus.bind(py))?;
    d.set_item("_invokes", s.invokes.bind(py))?;
    d.set_item("_Mouse", py.get_type_bound::<Mouse>())?;
    d.set_item("_mouse", s.mouse.bind(py))?;
    d.set_item("mouse", s.mouse.bind(py))?;
    d.set_item("_Window", py.get_type_bound::<Window>())?;
    d.set_item("window", s.window.bind(py))?;
    d.set_item("held_keys", s.held_keys.bind(py))?;
    d.set_item("time_", s.time.bind(py))?;
    d.set_item("time", s.time.bind(py))?;
    d.set_item("application", s.application.bind(py))?;
    d.set_item("_Scene", py.get_type_bound::<Scene>())?;
    d.set_item("scene", s.scene.bind(py))?;
    d.set_item("camera", s.camera.bind(py))?;
    d.set_item("Sequence", py.get_type_bound::<Sequence>())?;
    d.set_item("Audio", py.get_type_bound::<Audio>())?;
    d.set_item("input_handler", Py::new(py, InputHandler {})?)?;
    d.set_item("Ursina", py.get_type_bound::<Ursina>())?;
    d.set_item("BIN0", BIN0)?;
    d.set_item("TEXT_SIZE", TEXT_SIZE)?;
    d.set_item("FONT_K", FONT_K)?;
    let me = imp(py, "alpedia_core")?;
    for (k, f) in [("load_texture", "ursina_load_texture"), ("load_model", "ursina_load_model"), ("_tex_of", "ursina_tex_of"),
                   ("destroy", "ursina_destroy"), ("invoke", "ursina_invoke"), ("frame", "ursina_frame"), ("_hit", "ursina_hit"),
                   ("event", "ursina_event"), ("_send", "ursina_send"), ("any_ui", "ursina_any_ui")] {
        d.set_item(k, me.getattr(f)?)?;
    }
    Ok(d)
}

#[pyfunction]
fn ursina_tex_of(py: Python<'_>, t: &B<'_>) -> R<PyObject> {
    tex_of(py, t)
}

/// sys.modules["ursina"] をこのまねにする（Rust の画面のときだけ）
#[pyfunction]
pub fn ursina_install<'py>(py: Python<'py>, m: &B<'py>) -> R<B<'py>> {
    imp(py, "main.client_rs.panda_compat")?.call_method0("install")?;
    let sm = imp(py, "sys")?.getattr("modules")?;
    sm.set_item("ursina", m)?;
    let ns = PyDict::new_bound(py);
    let c = st(py)?.color.bind(py);
    for k in ["rgb", "rgba", "hsv"] {
        ns.set_item(k, c.getattr(k)?)?;
    }
    for (n, _) in NAMED {
        ns.set_item(*n, c.getattr(*n)?)?;
    }
    sm.set_item("ursina.color", imp(py, "types")?.getattr("SimpleNamespace")?.call((), Some(&ns))?)?;
    Ok(m.clone())
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    for f in [wrap_pyfunction_bound!(_color_new, m)?, wrap_pyfunction_bound!(_vec2_new, m)?, wrap_pyfunction_bound!(_vec3_new, m)?,
              wrap_pyfunction_bound!(_vec2_add, m)?, wrap_pyfunction_bound!(_vec2_sub, m)?, wrap_pyfunction_bound!(_vec2_mul, m)?,
              wrap_pyfunction_bound!(_vec3_add, m)?, wrap_pyfunction_bound!(_vec3_sub, m)?, wrap_pyfunction_bound!(_vec3_mul, m)?,
              wrap_pyfunction_bound!(ursina_load_texture, m)?, wrap_pyfunction_bound!(ursina_load_model, m)?, wrap_pyfunction_bound!(ursina_tex_of, m)?,
              wrap_pyfunction_bound!(ursina_destroy, m)?, wrap_pyfunction_bound!(ursina_invoke, m)?, wrap_pyfunction_bound!(ursina_frame, m)?,
              wrap_pyfunction_bound!(ursina_hit, m)?, wrap_pyfunction_bound!(ursina_event, m)?, wrap_pyfunction_bound!(ursina_send, m)?,
              wrap_pyfunction_bound!(ursina_any_ui, m)?, wrap_pyfunction_bound!(ursina_names, m)?, wrap_pyfunction_bound!(ursina_install, m)?] {
        m.add_function(f)?;
    }
    m.add_class::<BoundFn>()?;
    m.add_class::<ColorMod>()?;
    m.add_class::<Mesh>()?;
    m.add_class::<Quad>()?;
    m.add_class::<Texture>()?;
    m.add_class::<Entity>()?;
    m.add_class::<Text>()?;
    m.add_class::<Button>()?;
    m.add_class::<TextField>()?;
    m.add_class::<Cursor>()?;
    m.add_class::<InputField>()?;
    m.add_class::<Mouse>()?;
    m.add_class::<Window>()?;
    m.add_class::<Time>()?;
    m.add_class::<App>()?;
    m.add_class::<Lens>()?;
    m.add_class::<CamNode>()?;
    m.add_class::<Cam>()?;
    m.add_class::<Base>()?;
    m.add_class::<Scene>()?;
    m.add_class::<Camera>()?;
    m.add_class::<Sequence>()?;
    m.add_class::<Audio>()?;
    m.add_class::<InputHandler>()?;
    m.add_class::<Ursina>()?;
    Ok(())
}
