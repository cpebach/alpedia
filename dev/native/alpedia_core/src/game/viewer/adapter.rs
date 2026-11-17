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
#[allow(non_snake_case)]
pub struct GameAdapter {
    #[pyo3(get, set)]
    v: PyObject,
    /// U（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_U: Option<PyObject>,
    /// cam_mode（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_cam_mode: Option<PyObject>,
    /// cfg（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_cfg: Option<PyObject>,
    /// emotes（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_emotes: Option<PyObject>,
    /// hold（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_hold: Option<PyObject>,
    /// leaving（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_leaving: Option<PyObject>,
    /// load_errors（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_load_errors: Option<PyObject>,
    /// mouse_captured（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_mouse_captured: Option<PyObject>,
    /// scene_root（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_scene_root: Option<PyObject>,
    /// touch_hud（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_touch_hud: Option<PyObject>,
    /// ui_root（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_ui_root: Option<PyObject>,
    /// vkeys（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_vkeys: Option<PyObject>,
    /// world（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_world: Option<PyObject>,
}

#[pymethods]
impl GameAdapter {
    #[new]
    pub(crate) fn new(py: Python<'_>, viewer: &B<'_>) -> R<Self> {
        let _ = py;
        Ok(GameAdapter { a_U: None, a_cam_mode: None, a_cfg: None, a_emotes: None, a_hold: None, a_leaving: None, a_load_errors: None, a_mouse_captured: None, a_scene_root: None, a_touch_hud: None, a_ui_root: None, a_vkeys: None, a_world: None, v: viewer.clone().unbind() })
    }
    pub(crate) fn __init__(slf: &Bound<'_, Self>, viewer: &B<'_>) -> R<()> {
        let py = slf.py();
        let u = shim(py)?;
        slf.borrow_mut().v = viewer.clone().unbind();
        Self::sa_world(slf, ga(viewer, "w")?)?;
        Self::sa_cfg(slf, ga(viewer, "cfg")?)?;
        let d = PyDict::new_bound(py);
        for (k, n) in [("Entity", "Entity"), ("Text", "Text"), ("Button", "Button"), ("camera", "camera"), ("window", "window"), ("color", "color"),
                       ("held_keys", "held_keys"), ("mouse", "mouse"), ("utime", "time"), ("application", "application")] {
            d.set_item(k, u.getattr(n)?)?;
        }
        Self::sa_U(slf, d)?;
        let k = PyDict::new_bound(py);
        k.set_item("parent", u.getattr("camera")?.getattr("ui")?)?;
        Self::sa_ui_root(slf, u.getattr("Entity")?.call((), Some(&k))?)?;
        Self::sa_scene_root(slf, ga(viewer, "root")?)?; // MOD の 3D の物（DynamicMesh・SceneView）を付ける所
        Self::sa_hold(slf, false)?; // MOD が世界の時間を止める（ムービーなど）
        Self::sa_cam_mode(slf, py.None())?;
        Self::sa_emotes(slf, PyDict::new_bound(py))?;
        Self::sa_leaving(slf, false)?;
        Self::sa_load_errors(slf, PyList::empty_bound(py))?;
        Self::sa_touch_hud(slf, Py::new(py, TouchStub::new(py)?)?)?;
        Self::sa_vkeys(slf, PyDict::new_bound(py))?;
        Self::sa_mouse_captured(slf, false)?;
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
        ga(&Self::vb(slf), "third_person")
    }
    #[setter]
    pub(crate) fn set_third_person(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        Self::vb(slf).setattr("third_person", on.is_truthy()?)
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
        if !Self::ga_cam_mode(slf)?.is_none() || !ga(s, "editor")?.is_none() || ga(&ga(&v, "ui")?, "stack")?.is_truthy()?
            || ga(&ga(&Self::ga_world(slf)?, "player")?, "seated")?.is_truthy()? {
            return Ok(());
        }
        ga(&v, "held")?.call_method0("clear")?;
        Self::sa_cam_mode(slf, imp(py, "main.client.camera_mode")?.getattr("CameraMode")?.call1((s,))?)?;
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
        let f = imp(py, "functools")?.getattr("partial")?.call1((cls, Self::ga_U(slf)?, Self::ga_cfg(slf)?, ga(&Self::ga_ui_root(slf)?, "parent")?, ga(s, "editor_closed")?, extra))?;
        s.call_method1("_open_panel", (f,))?;
        Ok(())
    }
    #[getter]
    pub(crate) fn paused(slf: &Bound<'_, Self>) -> R<bool> {
        ga(&ga(&Self::vb(slf), "ui")?, "stack")?.is_truthy()
    }
    pub(crate) fn look_vectors<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        Self::vb(slf).call_method0("look_vectors")
    }
    pub(crate) fn exit_camera(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let cm = Self::ga_cam_mode(slf)?;
        if cm.is_none() {
            return Ok(());
        }
        cm.call_method0("destroy")?;
        Self::sa_cam_mode(slf, s.py().None())?;
        s.call_method1("_camera_hook", (false,))?;
        Self::ga_ui_root(slf)?.setattr("enabled", true)?;
        s.call_method1("set_mouse_capture", (true,))?;
        Ok(())
    }
    /// MOD の画面（今までの ursina の部品）を開く
    pub(crate) fn _open_panel(slf: &Bound<'_, Self>, make: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.call_method1("set_mouse_capture", (false,))?;
        ga(&Self::vb(slf), "held")?.call_method0("clear")?;
        Self::ga_ui_root(slf)?.setattr("enabled", false)?;
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
                Self::editor_closed(slf).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?;
                Self::ga_world(slf)?.call_method1("message", (format!("⚠ 画面を開けませんでした: {}", e.value_bound(py).str()?),))?;
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }
    /// ワールドを出る（action："menu"・"play:名前"）
    #[pyo3(signature = (action, save=true, status=""))]
    pub(crate) fn _leave(slf: &Bound<'_, Self>, action: &B<'_>, save: bool, status: &str) -> R<()> {
        let py = slf.py();
        if Self::ga_leaving(slf)?.is_truthy()? {
            return Ok(());
        }
        Self::sa_leaving(slf, true)?;
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        v.setattr("leave_action", (action, save, status))?;
        v.setattr("quit", true)?;
        Ok(())
    }
    #[getter]
    pub(crate) fn sel<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        ga(&Self::vb(slf), "sel")
    }
    #[setter]
    pub(crate) fn set_sel(slf: &Bound<'_, Self>, k: &B<'_>) -> R<()> {
        Self::vb(slf).setattr("sel", k)
    }
    #[getter]
    pub(crate) fn editor<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        ga(&Self::vb(slf), "editor")
    }
    #[setter]
    pub(crate) fn set_editor(slf: &Bound<'_, Self>, e: &B<'_>) -> R<()> {
        Self::vb(slf).setattr("editor", e)
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
        Self::vb(slf).call_method1("set_grab", (on,))?;
        Ok(())
    }
    // ---- 画面のボタン（touch_ui.TouchHUD）が使う口 ----
    #[getter]
    pub(crate) fn hit<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        ga(&Self::vb(slf), "hit")
    }
    pub(crate) fn _usable<'py>(slf: &Bound<'py, Self>, hit: &B<'py>) -> R<B<'py>> {
        Self::vb(slf).call_method1("_usable", (hit,))
    }
    /// 画面のボタンがキーを押した（ursina のキーの名前）
    pub(crate) fn input(slf: &Bound<'_, Self>, key: &B<'_>) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        v.call_method1("_key", (key, ga(slf.as_any(), "cfg")?.get_item("controls")?))?;
        Ok(())
    }
    pub(crate) fn do_remove(slf: &Bound<'_, Self>) -> R<()> {
        Self::vb(slf).call_method1("_click", (0,))?;
        Ok(())
    }
    pub(crate) fn do_place(slf: &Bound<'_, Self>) -> R<()> {
        Self::vb(slf).call_method1("_click", (1,))?;
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
        Self::vb(slf).call_method1("nearby_targets", (radius,))
    }
    #[pyo3(signature = (t, settings=false))]
    pub(crate) fn do_target(slf: &Bound<'_, Self>, t: &B<'_>, settings: bool) -> R<()> {
        let py = slf.py();
        if settings || get(t, "kind", py.None())?.eq("p")? {
            slf.call_method1("open_editor", (t.get_item("hit")?,))?;
        } else {
            Self::vb(slf).call_method1("do_interact", (t.get_item("hit")?,))?;
        }
        Ok(())
    }
    pub(crate) fn touch_action(slf: &Bound<'_, Self>, kind: &B<'_>, x: &B<'_>, y: &B<'_>) -> R<()> {
        Self::vb(slf).call_method1("touch_action", (kind, x, y))?;
        Ok(())
    }
    #[pyo3(signature = (name, volume=1.0))]
    pub(crate) fn sfx(slf: &Bound<'_, Self>, name: &B<'_>, volume: f64) -> R<()> {
        Self::vb(slf).call_method1("sfx", (name, volume))?;
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

// ---------------------------------------------------------------- GameAdapter の欄の読み書き（C-01。前は __dict__）
// 入れていない欄を読むと、前と同じく AttributeError（getattr(x, 名前, 既定値) も前と同じに動く）
macro_rules! gameadapter_attr_fields {
    ($($g:ident $s:ident $pg:ident $ps:ident $f:ident $name:literal),* $(,)?) => {
        #[allow(dead_code, non_snake_case)]
        impl GameAdapter { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
                match &slf.try_borrow()?.$f {
                    Some(v) => Ok(v.bind(slf.py()).clone()),
                    None => Err(pyo3::exceptions::PyAttributeError::new_err($name)),
                }
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> PyResult<()> {
                let v = v.into_py(slf.py());
                slf.try_borrow_mut()?.$f = Some(v);
                Ok(())
            }
        )* }
        #[pymethods]
        #[allow(non_snake_case)]
        impl GameAdapter { $(
            #[getter($name)]
            fn $pg(slf: &Bound<'_, Self>) -> PyResult<PyObject> {
                Ok(Self::$g(slf)?.unbind())
            }
            #[setter($name)]
            fn $ps(&mut self, v: PyObject) {
                self.$f = Some(v);
            }
        )* }
    };
}
gameadapter_attr_fields!(
    ga_U sa_U pga_U psa_U a_U "U", ga_cam_mode sa_cam_mode pga_cam_mode psa_cam_mode a_cam_mode "cam_mode", ga_cfg sa_cfg pga_cfg psa_cfg a_cfg "cfg", ga_emotes sa_emotes pga_emotes psa_emotes a_emotes "emotes", ga_hold sa_hold pga_hold psa_hold a_hold "hold", ga_leaving sa_leaving pga_leaving psa_leaving a_leaving "leaving", ga_load_errors sa_load_errors pga_load_errors psa_load_errors a_load_errors "load_errors", ga_mouse_captured sa_mouse_captured pga_mouse_captured psa_mouse_captured a_mouse_captured "mouse_captured", ga_scene_root sa_scene_root pga_scene_root psa_scene_root a_scene_root "scene_root", ga_touch_hud sa_touch_hud pga_touch_hud psa_touch_hud a_touch_hud "touch_hud", ga_ui_root sa_ui_root pga_ui_root psa_ui_root a_ui_root "ui_root", ga_vkeys sa_vkeys pga_vkeys psa_vkeys a_vkeys "vkeys", ga_world sa_world pga_world psa_world a_world "world"
);

impl GameAdapter {
    /// 画面（Viewer）。借りたままにしない（Python を呼ぶ間に、MOD がこの物の欄を書き換えても困らないように）
    pub(crate) fn vb<'py>(slf: &Bound<'py, Self>) -> Bound<'py, PyAny> {
        slf.borrow().v.bind(slf.py()).clone()
    }
}
