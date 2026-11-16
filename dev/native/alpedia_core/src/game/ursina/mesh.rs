//! ursina のまね：形（Quad・Mesh）とテクスチャ

use super::*;

// ---------------------------------------------------------------------------
// 形（Quad・Mesh）とテクスチャ
// ---------------------------------------------------------------------------
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Mesh {
    #[pyo3(get, set)]
    pub(crate) vertices: PyObject,
    #[pyo3(get, set)]
    pub(crate) triangles: PyObject,
    #[pyo3(get, set)]
    pub(crate) colors: PyObject,
    #[pyo3(get, set)]
    pub(crate) uvs: PyObject,
    #[pyo3(get, set)]
    pub(crate) mode: PyObject,
}

impl Mesh {
    pub(crate) fn blank(py: Python<'_>) -> Self {
        Mesh { vertices: PyList::empty_bound(py).into_any().unbind(), triangles: py.None(), colors: py.None(), uvs: py.None(), mode: "triangle".into_py(py) }
    }
    pub(crate) fn fill(slf: &Bound<'_, Mesh>, vertices: &Bound<'_, PyAny>, triangles: PyObject, colors: PyObject, uvs: PyObject, mode: PyObject) -> R<()> {
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
    pub(crate) fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        Mesh::blank(py)
    }
    #[pyo3(signature = (vertices=None, triangles=None, colors=None, uvs=None, mode=None, **_kw))]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, vertices: Option<&Bound<'_, PyAny>>, triangles: Option<PyObject>, colors: Option<PyObject>, uvs: Option<PyObject>,
                mode: Option<PyObject>, _kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let py = slf.py();
        let e = PyTuple::empty_bound(py).into_any();
        Self::fill(slf, vertices.unwrap_or(&e), triangles.unwrap_or_else(|| py.None()), colors.unwrap_or_else(|| py.None()),
            uvs.unwrap_or_else(|| py.None()), mode.unwrap_or_else(|| "triangle".into_py(py)))
    }
    /// 頂点 (n,3)・UV (n,2)・三角形 (m,3)
    pub(crate) fn arrays<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
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


/// 角の丸い四角（幅 1・高さ 1、真ん中が原点。aspect＝幅÷高さで丸みを合わせる）
#[pyclass(module = "alpedia_core", extends = Mesh, subclass)]
pub struct Quad {}

pub(crate) fn quad_parts(radius: f64, aspect: f64, segments: i64) -> (Vec<(f64, f64, f64)>, Vec<(i64, i64, i64)>, Vec<(f64, f64)>) {
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
    pub(crate) fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> (Self, Mesh) {
        (Quad {}, Mesh::blank(py))
    }
    #[pyo3(signature = (radius=None, aspect=None, segments=None, **_kw))]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, radius: Option<&Bound<'_, PyAny>>, aspect: Option<&Bound<'_, PyAny>>, segments: Option<&Bound<'_, PyAny>>,
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

pub(crate) fn mesh_new<'py>(py: Python<'py>, v: Vec<(f64, f64, f64)>, t: Vec<(i64, i64, i64)>, uv: Vec<(f64, f64)>) -> R<Bound<'py, PyAny>> {
    let m = Bound::new(py, Mesh::blank(py))?;
    Mesh::fill(&m, &PyList::new_bound(py, v.into_iter().map(|x| po(py, x))), PyList::new_bound(py, t.into_iter().map(|x| po(py, x))).into_any().unbind(),
        py.None(), PyList::new_bound(py, uv.into_iter().map(|x| po(py, x))).into_any().unbind(), "triangle".into_py(py))?;
    Ok(m.into_any())
}

pub(crate) static MODELS: GILOnceCell<Py<PyDict>> = GILOnceCell::new();

pub(crate) fn models(py: Python<'_>) -> R<&Bound<'_, PyDict>> {
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
    pub(crate) rs: PyObject,
    #[pyo3(get, set)]
    pub(crate) filtering: PyObject,
}

#[pymethods]
impl Texture {
    #[new]
    #[pyo3(signature = (src=None, *_a, **_k))]
    pub(crate) fn new(py: Python<'_>, src: Option<&Bound<'_, PyAny>>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<Self> {
        let mut rs = py.None();
        if let Some(s) = src.filter(|s| !s.is_none()) {
            if s.is_instance_of::<RsTex>() {
                rs = s.clone().unbind();
            } else if s.hasattr("convert")? {
                // PIL の画像
                let im = s.call_method1("convert", ("RGBA",))?;
                rs = Py::new(py, crate::game::gfx::gfx_tex_rgba(py, &im.getattr("width")?, &im.getattr("height")?, &im.call_method0("tobytes")?,
                    Some("pil".into_py(py)), false)?)?.into_any();
            } else if s.is_instance_of::<pyo3::types::PyString>() {
                rs = crate::game::gfx::gfx_tex(py, s, false)?;
            }
        }
        Ok(Texture { rs, filtering: py.None() })
    }
    #[getter]
    pub(crate) fn width(&self) -> i64 {
        1
    }
    #[getter]
    pub(crate) fn height(&self) -> i64 {
        1
    }
}

#[pyfunction]
#[pyo3(signature = (path, *_a, **_k))]
pub(crate) fn ursina_load_texture<'py>(py: Python<'py>, path: &Bound<'py, PyAny>, _a: &Bound<'py, PyTuple>, _k: Option<&Bound<'py, PyDict>>) -> R<PyObject> {
    let t = Py::new(py, Texture::new(py, Some(path.str()?.as_any()), &PyTuple::empty_bound(py), None)?)?;
    if t.borrow(py).rs.is_none(py) {
        return Ok(py.None());
    }
    Ok(t.into_any())
}
#[pyfunction]
#[pyo3(signature = (*_a, **_k))]
pub(crate) fn ursina_load_model(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> PyObject {
    py.None()
}

pub(crate) fn tex_of<'py>(py: Python<'py>, t: &Bound<'py, PyAny>) -> R<PyObject> {
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
            let r = crate::game::gfx::gfx_tex(py, p.str()?.as_any(), false)?;
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
