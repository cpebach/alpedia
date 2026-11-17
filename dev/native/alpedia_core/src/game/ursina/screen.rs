//! ursina のまね：画面・マウス・時間・カメラなど

use crate::game::PyDictOps;
use super::*;

// ---------------------------------------------------------------------------
// 画面・マウス・時間
// ---------------------------------------------------------------------------
#[pyclass(module = "alpedia_core", name = "_Mouse", dict)]
pub struct Mouse {
    #[pyo3(get, set)]
    pub(crate) x: f64,
    #[pyo3(get, set)]
    pub(crate) y: f64,
    #[pyo3(get, set)]
    pub(crate) locked: PyObject,
    #[pyo3(get, set)]
    pub(crate) visible: PyObject,
    #[pyo3(get, set)]
    pub(crate) hovered_entity: PyObject,
    #[pyo3(get, set)]
    pub(crate) left: bool,
    #[pyo3(get, set)]
    pub(crate) right: PyObject,
    #[pyo3(get, set)]
    pub(crate) velocity: PyObject,
}

impl Mouse {
    pub(crate) fn make(py: Python<'_>) -> R<Self> {
        Ok(Mouse { x: 0.0, y: 0.0, locked: false.into_py(py), visible: true.into_py(py), hovered_entity: py.None(), left: false, right: false.into_py(py),
            velocity: vec3(py, 0.0, 0.0, 0.0)?.unbind() })
    }
}

#[pymethods]
impl Mouse {
    #[getter]
    pub(crate) fn position<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec3(py, self.x, self.y, 0.0)
    }
}

#[pyclass(module = "alpedia_core", name = "_Window", dict)]
pub struct Window {
    #[pyo3(get, set)]
    pub(crate) _size: PyObject,
    #[pyo3(get, set)]
    pub(crate) color: PyObject,
    #[pyo3(get, set)]
    pub(crate) _fullscreen: bool,
    #[pyo3(get, set)]
    pub(crate) borderless: PyObject,
    #[pyo3(get, set)]
    pub(crate) title: PyObject,
    #[pyo3(get, set)]
    pub(crate) vsync: PyObject,
}

impl Window {
    pub(crate) fn make(py: Python<'_>) -> R<Self> {
        Ok(Window { _size: (1280, 720).into_py(py), color: color(py, 0.45, 0.65, 0.9, 1.0)?.unbind(), _fullscreen: false, borderless: false.into_py(py),
            title: "Alpedia".into_py(py), vsync: true.into_py(py) })
    }
    pub(crate) fn wh(&self, py: Python<'_>) -> R<(f64, f64)> {
        it2(self._size.bind(py))
    }
    pub(crate) fn ar(&self, py: Python<'_>) -> R<f64> {
        let (w, h) = self.wh(py)?;
        Ok(w / h.max(1.0))
    }
}

pub(crate) fn gview(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    imp(py, "main.client.gfx")?.getattr("VIEW")
}

#[pymethods]
impl Window {
    #[getter]
    pub(crate) fn size<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        cls(py, &VEC2)?.call1(self._size.bind(py).downcast::<PyTuple>().map(|t| t.clone()).or_else(|_| {
            PyTuple::new_bound(py, self._size.bind(py).iter()?.collect::<R<Vec<_>>>()?).into_any().downcast_into::<PyTuple>().map_err(PyErr::from)
        })?)
    }
    /// 窓の大きさを変える（設定の解像度）
    #[setter]
    pub(crate) fn set_size(&mut self, py: Python<'_>, v: &B<'_>) -> R<()> {
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
    pub(crate) fn fullscreen(&self, py: Python<'_>) -> R<PyObject> {
        let gv = gview(py)?;
        Ok(if gv.is_none() { self._fullscreen.into_py(py) } else { gv.call_method0("fullscreen")?.unbind() })
    }
    #[setter]
    pub(crate) fn set_fullscreen(&mut self, py: Python<'_>, on: &B<'_>) -> R<()> {
        let on = on.is_truthy()?;
        self._fullscreen = on;
        let gv = gview(py)?;
        if !gv.is_none() {
            gv.call_method1("set_fullscreen", (on,))?;
        }
        Ok(())
    }
    #[getter]
    pub(crate) fn aspect_ratio(&self, py: Python<'_>) -> R<f64> {
        self.ar(py)
    }
    #[getter]
    pub(crate) fn top<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, 0.0, 0.5)
    }
    #[getter]
    pub(crate) fn bottom<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, 0.0, -0.5)
    }
    #[getter]
    pub(crate) fn left<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, -self.ar(py)? / 2.0, 0.0)
    }
    #[getter]
    pub(crate) fn right<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, self.ar(py)? / 2.0, 0.0)
    }
    #[getter]
    pub(crate) fn top_left<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, -self.ar(py)? / 2.0, 0.5)
    }
    #[getter]
    pub(crate) fn top_right<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, self.ar(py)? / 2.0, 0.5)
    }
    #[getter]
    pub(crate) fn bottom_left<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, -self.ar(py)? / 2.0, -0.5)
    }
    #[getter]
    pub(crate) fn bottom_right<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        vec2(py, self.ar(py)? / 2.0, -0.5)
    }
}

#[pyclass(module = "alpedia_core", name = "_Time", dict)]
pub struct Time {
    #[pyo3(get, set)]
    pub(crate) dt: f64,
}

#[pyclass(module = "alpedia_core", name = "_App", dict)]
pub struct App {
    #[pyo3(get, set)]
    pub(crate) paused: PyObject,
    #[pyo3(get, set)]
    pub(crate) fonts_folder: PyObject,
    #[pyo3(get, set)]
    pub(crate) base: PyObject,
    #[pyo3(get, set)]
    pub(crate) asset_folder: PyObject,
}

#[pymethods]
impl App {
    #[staticmethod]
    pub(crate) fn quit() {}
}

#[pyclass(module = "alpedia_core", name = "_Lens")]
pub struct Lens {}
#[pymethods]
impl Lens {
    #[pyo3(name = "getFov")]
    pub(crate) fn get_fov(&self, py: Python<'_>) -> R<PyObject> {
        Ok(st(py)?.camera.borrow(py).fov_pair.clone_ref(py))
    }
}
#[pyclass(module = "alpedia_core", name = "_CamNode")]
pub struct CamNode {}
#[pymethods]
impl CamNode {
    #[pyo3(name = "getLens")]
    pub(crate) fn get_lens(&self) -> Lens {
        Lens {}
    }
}
#[pyclass(module = "alpedia_core", name = "_Cam")]
pub struct Cam {}
#[pymethods]
impl Cam {
    pub(crate) fn node(&self) -> CamNode {
        CamNode {}
    }
}
/// application.base のまね（MOD が画面の視野角を読む：base.cam.node().getLens().getFov()）
#[pyclass(module = "alpedia_core", name = "_Base")]
pub struct Base {}
#[pymethods]
impl Base {
    #[classattr]
    pub(crate) fn cam() -> Cam {
        Cam {}
    }
}

#[pyclass(module = "alpedia_core", extends = Entity, subclass, name = "_Scene")]
pub struct Scene {}

pub(crate) fn scene_new<'py>(py: Python<'py>, name: &str) -> R<Bound<'py, PyAny>> {
    let e = Entity::blank(py, name)?;
    Ok(Bound::new(py, PyClassInitializer::from(e).add_subclass(Scene {}))?.into_any())
}

#[pyclass(module = "alpedia_core", name = "_Camera", dict)]
pub struct Camera {
    #[pyo3(get, set)]
    pub(crate) ui: PyObject,
    #[pyo3(get, set)]
    pub(crate) fov: PyObject,
    #[pyo3(get, set)]
    pub(crate) fov_pair: PyObject, // (横, 縦) の視野角（Rust の画面が毎フレーム入れる）
    #[pyo3(get, set)]
    pub(crate) position: PyObject,
    #[pyo3(get, set)]
    pub(crate) clip_plane_near: PyObject,
    #[pyo3(get, set)]
    pub(crate) clip_plane_far: PyObject,
    #[pyo3(get, set)]
    pub(crate) orthographic: PyObject,
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
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        Sequence {}
    }
    pub(crate) fn start(&self) {}
    pub(crate) fn kill(&self) {}
}

#[pyclass(module = "alpedia_core", dict)]
pub struct Audio {}
#[pymethods]
impl Audio {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        Audio {}
    }
    pub(crate) fn play(&self) {}
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn stop(&self, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) {}
}

/// ursina の input_handler（Ctrl などと組み合わせたキーの名前）
#[pyclass(module = "alpedia_core", name = "_InputHandler")]
pub struct InputHandler {}
#[pymethods]
impl InputHandler {
    #[staticmethod]
    pub(crate) fn get_combined_key(py: Python<'_>, key: &str) -> R<String> {
        let hk = st(py)?.held_keys.bind(py);
        let mut mods: Vec<&str> = vec![];
        for (m, ks) in [("control", ["left control", "right control"]), ("shift", ["left shift", "right shift"]), ("alt", ["left alt", "right alt"])] {
            let mut any = false;
            for k in ks {
                if hk.dget1(k)?.is_truthy()? {
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
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        Ursina {}
    }
}
