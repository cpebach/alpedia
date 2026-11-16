//! 画面：MOD から見たゲーム（GameAdapter）とその道具

use super::*;

/// カメラモードに渡す「押しているキー」（ursina の held_keys の形）
#[pyclass(module = "alpedia_core", name = "_Held", dict)]
pub struct Held {
    #[pyo3(get, set)]
    keys: Py<PySet>,
}
impl Held {
    pub(crate) fn make(v: &B<'_>) -> R<Self> {
        let py = v.py();
        let keys = PySet::empty_bound(py)?;
        for k in ga(v, "held")?.iter()? {
            let k: String = k?.extract()?;
            keys.add(keymap(&k))?;
        }
        Ok(Held { keys: keys.unbind() })
    }
}
#[pymethods]
impl Held {
    #[new]
    pub(crate) fn new(v: &B<'_>) -> R<Self> {
        Held::make(v)
    }
    pub(crate) fn __getitem__(&self, py: Python<'_>, k: &B<'_>) -> R<i64> {
        Ok(if self.keys.bind(py).contains(k)? { 1 } else { 0 })
    }
}

/// カメラモードのドラッグ：マウスの左ボタンを押している間、指1本として位置を返す
#[pyclass(module = "alpedia_core", name = "_Pointers", dict)]
pub struct Pointers {}
#[pymethods]
impl Pointers {
    #[new]
    pub(crate) fn new() -> Self {
        Pointers {}
    }
    pub(crate) fn current<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyDict>> {
        let m = shim(py)?.getattr("mouse")?;
        let d = PyDict::new_bound(py);
        if ga(&m, "left")?.is_truthy()? {
            d.set_item(0, (ga(&m, "x")?, ga(&m, "y")?))?;
        }
        Ok(d)
    }
}

/// 画面のボタンを作れなかったときの代わり（何も出さない）
#[pyclass(module = "alpedia_core", name = "_TouchStub", dict)]
pub struct TouchStub {
    #[pyo3(get, set)]
    ptr: PyObject,
    #[pyo3(get, set)]
    stick: PyObject,
}
#[pymethods]
impl TouchStub {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> R<Self> {
        Ok(TouchStub { ptr: Py::new(py, Pointers {})?.into_any(), stick: imp(py, "numpy")?.call_method1("zeros", (2,))?.unbind() })
    }
    pub(crate) fn _release_all(&self) {}
    pub(crate) fn update(&self, _dt: &B<'_>) {}
    pub(crate) fn take_look<'py>(&self, py: Python<'py>) -> R<B<'py>> {
        imp(py, "numpy")?.call_method1("zeros", (2,))
    }
    pub(crate) fn over_ui(&self, _x: &B<'_>, _y: &B<'_>) -> bool {
        false
    }
    pub(crate) fn mouse_over_ui(&self) -> bool {
        false
    }
    pub(crate) fn destroy(&self) {}
}

// ---------------------------------------------------------------- GameAdapter
/// 今までの画面の部品（ScriptPanel・BlockEditor・DevicePanel など）が使う game のまね
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct GameAdapter {
    #[pyo3(get, set)]
    v: PyObject,
}

#[pymethods]
impl GameAdapter {
    #[new]
    pub(crate) fn new(py: Python<'_>, viewer: &B<'_>) -> R<Self> {
        let _ = py;
        Ok(GameAdapter { v: viewer.clone().unbind() })
    }
    pub(crate) fn __init__(slf: &Bound<'_, Self>, viewer: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let u = shim(py)?;
        slf.borrow_mut().v = viewer.clone().unbind();
        s.setattr("world", ga(viewer, "w")?)?;
        s.setattr("cfg", ga(viewer, "cfg")?)?;
        let d = PyDict::new_bound(py);
        for (k, n) in [("Entity", "Entity"), ("Text", "Text"), ("Button", "Button"), ("camera", "camera"), ("window", "window"), ("color", "color"),
                       ("held_keys", "held_keys"), ("mouse", "mouse"), ("utime", "time"), ("application", "application")] {
            d.set_item(k, u.getattr(n)?)?;
        }
        s.setattr("U", d)?;
        let k = PyDict::new_bound(py);
        k.set_item("parent", u.getattr("camera")?.getattr("ui")?)?;
        s.setattr("ui_root", u.getattr("Entity")?.call((), Some(&k))?)?;
        s.setattr("scene_root", ga(viewer, "root")?)?; // MOD の 3D の物（DynamicMesh・SceneView）を付ける所
        s.setattr("hold", false)?; // MOD が世界の時間を止める（ムービーなど）
        s.setattr("cam_mode", py.None())?;
        s.setattr("emotes", PyDict::new_bound(py))?;
        s.setattr("leaving", false)?;
        s.setattr("load_errors", PyList::empty_bound(py))?;
        s.setattr("touch_hud", Py::new(py, TouchStub::new(py)?)?)?;
        s.setattr("vkeys", PyDict::new_bound(py))?;
        s.setattr("mouse_captured", false)?;
        Ok(())
    }
    #[getter]
    pub(crate) fn addons<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        Ok(gd(&v, "addons", PyList::empty_bound(py).into_any()))
    }
    #[getter]
    pub(crate) fn third_person<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        ga(slf.borrow().v.bind(py), "third_person")
    }
    #[setter]
    pub(crate) fn set_third_person(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let py = slf.py();
        slf.borrow().v.bind(py).setattr("third_person", on.is_truthy()?)
    }
    pub(crate) fn _camera_hook(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        let f = acall(py, "on_camera", vec![on.clone().unbind()])?;
        v.call_method1("_each_addon", (f,))?;
        Ok(())
    }
    /// C：カメラモード（今までの画面の CameraMode を ursina のまねで）
    pub(crate) fn enter_camera(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        if !ga(s, "cam_mode")?.is_none() || !ga(s, "editor")?.is_none() || ga(&ga(&v, "ui")?, "stack")?.is_truthy()?
            || ga(&ga(&ga(s, "world")?, "player")?, "seated")?.is_truthy()? {
            return Ok(());
        }
        ga(&v, "held")?.call_method0("clear")?;
        s.setattr("cam_mode", imp(py, "main.client.camera_mode")?.getattr("CameraMode")?.call1((s,))?)?;
        s.call_method1("_camera_hook", (true,))?;
        Ok(())
    }
    /// F8：読み込み（設計図・OBJ）
    pub(crate) fn open_import(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let cls = imp(py, "main.client.import_ui")?.getattr("ImportPanel")?;
        let f = imp(py, "functools")?.getattr("partial")?.call1((cls, slf))?;
        slf.call_method1("_open_panel", (f,))?;
        Ok(())
    }
    /// 操作とキー設定
    pub(crate) fn open_keys(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let extra = PyList::empty_bound(py);
        for a in Self::addons(slf)?.iter()? {
            let ht = ga(&a?, "help_text")?;
            if ht.is_truthy()? {
                extra.append(ht)?;
            }
        }
        let cls = imp(py, "main.client.keys_panel")?.getattr("KeysPanel")?;
        let f = imp(py, "functools")?.getattr("partial")?.call1((cls, ga(s, "U")?, ga(s, "cfg")?, ga(&ga(s, "ui_root")?, "parent")?, ga(s, "editor_closed")?, extra))?;
        s.call_method1("_open_panel", (f,))?;
        Ok(())
    }
    #[getter]
    pub(crate) fn paused(slf: &Bound<'_, Self>) -> R<bool> {
        let py = slf.py();
        ga(&ga(slf.borrow().v.bind(py), "ui")?, "stack")?.is_truthy()
    }
    pub(crate) fn look_vectors<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        slf.borrow().v.bind(py).call_method0("look_vectors")
    }
    pub(crate) fn exit_camera(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let cm = ga(s, "cam_mode")?;
        if cm.is_none() {
            return Ok(());
        }
        cm.call_method0("destroy")?;
        s.setattr("cam_mode", s.py().None())?;
        s.call_method1("_camera_hook", (false,))?;
        ga(s, "ui_root")?.setattr("enabled", true)?;
        s.call_method1("set_mouse_capture", (true,))?;
        Ok(())
    }
    /// MOD の画面（今までの ursina の部品）を開く
    pub(crate) fn _open_panel(slf: &Bound<'_, Self>, make: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.call_method1("set_mouse_capture", (false,))?;
        ga(slf.borrow().v.bind(py), "held")?.call_method0("clear")?;
        ga(s, "ui_root")?.setattr("enabled", false)?;
        match make.call0() {
            Ok(e) => s.setattr("editor", e)?,
            Err(e) if is_exc(py, &e) => {
                log_exc(py, &e, "画面を開けませんでした", vec![])?;
                let ed = gd(s, "editor", none(py));
                let root = gd(&ed, "root", none(py));
                s.setattr("editor", py.None())?;
                if !root.is_none() {
                    shim(py)?.call_method1("destroy", (root,))?;
                }
                s.call_method0("editor_closed")?;
                ga(s, "world")?.call_method1("message", (format!("⚠ 画面を開けませんでした: {}", e.value_bound(py).str()?),))?;
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }
    /// ワールドを出る（action："menu"・"play:名前"）
    #[pyo3(signature = (action, save=true, status=""))]
    pub(crate) fn _leave(slf: &Bound<'_, Self>, action: &B<'_>, save: bool, status: &str) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if ga(s, "leaving")?.is_truthy()? {
            return Ok(());
        }
        s.setattr("leaving", true)?;
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        v.setattr("leave_action", (action, save, status))?;
        v.setattr("quit", true)?;
        Ok(())
    }
    #[getter]
    pub(crate) fn sel<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        ga(slf.borrow().v.bind(slf.py()), "sel")
    }
    #[setter]
    pub(crate) fn set_sel(slf: &Bound<'_, Self>, k: &B<'_>) -> R<()> {
        slf.borrow().v.bind(slf.py()).setattr("sel", k)
    }
    #[getter]
    pub(crate) fn editor<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        ga(slf.borrow().v.bind(slf.py()), "editor")
    }
    #[setter]
    pub(crate) fn set_editor(slf: &Bound<'_, Self>, e: &B<'_>) -> R<()> {
        slf.borrow().v.bind(slf.py()).setattr("editor", e)
    }
    pub(crate) fn editor_closed(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        v.setattr("editor", py.None())?;
        ga(slf.as_any(), "ui_root")?.setattr("enabled", true)?;
        v.call_method0("screen_closed")?;
        Ok(())
    }
    pub(crate) fn set_mouse_capture(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method1("set_grab", (on,))?;
        Ok(())
    }
    // ---- 画面のボタン（touch_ui.TouchHUD）が使う口 ----
    #[getter]
    pub(crate) fn hit<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        ga(slf.borrow().v.bind(slf.py()), "hit")
    }
    pub(crate) fn _usable<'py>(slf: &Bound<'py, Self>, hit: &B<'py>) -> R<B<'py>> {
        slf.borrow().v.bind(slf.py()).call_method1("_usable", (hit,))
    }
    /// 画面のボタンがキーを押した（ursina のキーの名前）
    pub(crate) fn input(slf: &Bound<'_, Self>, key: &B<'_>) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        v.call_method1("_key", (key, ga(slf.as_any(), "cfg")?.get_item("controls")?))?;
        Ok(())
    }
    pub(crate) fn do_remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method1("_click", (0,))?;
        Ok(())
    }
    pub(crate) fn do_place(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method1("_click", (1,))?;
        Ok(())
    }
    #[pyo3(signature = (hit=None))]
    pub(crate) fn open_editor(slf: &Bound<'_, Self>, hit: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        if let Some(h) = hit.filter(|h| !h.is_none()) {
            v.setattr("hit", h)?;
        }
        v.call_method0("open_editor")?;
        Ok(())
    }
    #[pyo3(signature = (radius=2.6))]
    pub(crate) fn nearby_targets<'py>(slf: &Bound<'py, Self>, radius: f64) -> R<B<'py>> {
        slf.borrow().v.bind(slf.py()).call_method1("nearby_targets", (radius,))
    }
    #[pyo3(signature = (t, settings=false))]
    pub(crate) fn do_target(slf: &Bound<'_, Self>, t: &B<'_>, settings: bool) -> R<()> {
        let py = slf.py();
        if settings || get(t, "kind", py.None())?.eq("p")? {
            slf.call_method1("open_editor", (t.get_item("hit")?,))?;
        } else {
            slf.borrow().v.bind(py).call_method1("do_interact", (t.get_item("hit")?,))?;
        }
        Ok(())
    }
    pub(crate) fn touch_action(slf: &Bound<'_, Self>, kind: &B<'_>, x: &B<'_>, y: &B<'_>) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method1("touch_action", (kind, x, y))?;
        Ok(())
    }
    #[pyo3(signature = (name, volume=1.0))]
    pub(crate) fn sfx(slf: &Bound<'_, Self>, name: &B<'_>, volume: f64) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method1("sfx", (name, volume))?;
        Ok(())
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<AddonCall>()?;
    m.add_class::<Viewer>()?;
    m.add_class::<Held>()?;
    m.add_class::<Pointers>()?;
    m.add_class::<TouchStub>()?;
    m.add_class::<GameAdapter>()?;
    m.add_function(wrap_pyfunction_bound!(viewer_run, m)?)?;
    Ok(())
}
