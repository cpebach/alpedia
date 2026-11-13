//! Rust の画面のクライアント全体の流れ（main/client_rs/client.py）：
//! タイトルのメニュー（menu.MainMenu を ursina のまねで）→ ワールド → メニュー。
//!
//!   python run.py --view rust            メニューから
//!   python run.py --view rust --world 名前   メニューを飛ばして

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use super::imp;

type R<T> = PyResult<T>;

fn log(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    imp(py, "main.log")?.call_method1("get", ("client_rs",))
}
fn log_exc(py: Python<'_>, e: &PyErr, msg: &str, args: impl IntoPy<Py<pyo3::types::PyTuple>>) -> R<()> {
    let a: Py<pyo3::types::PyTuple> = args.into_py(py);
    let mut v: Vec<PyObject> = vec![msg.into_py(py)];
    v.extend(a.bind(py).iter().map(|x| x.unbind()));
    let k = PyDict::new_bound(py);
    k.set_item("exc_info", e.value_bound(py))?;
    log(py)?.call_method("exception", pyo3::types::PyTuple::new_bound(py, v), Some(&k))?;
    Ok(())
}
fn is_mod_err(py: Python<'_>, e: &PyErr) -> R<bool> {
    Ok(e.value_bound(py).is_instance(&imp(py, "main.modcode")?.getattr("ModLoadError")?)?)
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct RsClient {
    #[pyo3(get, set, name = "U")]
    u: PyObject,
    #[pyo3(get, set)]
    cfg: PyObject,
    #[pyo3(get, set)]
    view: PyObject,
    #[pyo3(get, set)]
    menu: PyObject,
    #[pyo3(get, set)]
    viewer: PyObject,
    #[pyo3(get, set)]
    joining: PyObject,
    #[pyo3(get, set)]
    running: bool,
}

macro_rules! g {
    ($s:expr, $f:ident) => {
        $s.borrow().$f.clone_ref($s.py()).into_bound($s.py())
    };
}

#[pymethods]
impl RsClient {
    #[new]
    fn new(py: Python<'_>, cfg: &Bound<'_, PyAny>) -> R<Self> {
        let u = imp(py, "main.client_rs.ursina_shim")?.call_method0("install")?;
        let empty = PyDict::new_bound(py);
        let win = cfg.call_method1("get", ("window", &empty))?;
        let gfx = imp(py, "main.client.gfx")?;
        let view = gfx.call_method1("start", (win.call_method1("get", ("width", 1280))?, win.call_method1("get", ("height", 720))?, "Alpedia",
            win.call_method1("get", ("vsync", true))?))?;
        gfx.call_method1("font", (cfg.call_method1("get", ("graphics", &empty))?.call_method1("get", ("font",))?,))?;
        let ico = imp(py, "main.paths")?.getattr("SAUCE")?.call_method1("__truediv__", ("textures",))?.call_method1("__truediv__", ("alpedia.ico",))?;
        view.call_method1("set_icon", (ico.str()?,))?;
        imp(py, "main.client.sound")?.call_method1("get", (cfg,))?;
        if win.call_method1("get", ("fullscreen", false))?.is_truthy()? {
            view.call_method1("set_fullscreen", (true,))?;
        }
        Ok(RsClient { u: u.unbind(), cfg: cfg.clone().unbind(), view: view.unbind(), menu: py.None(), viewer: py.None(), joining: py.None(), running: true })
    }
    // ---- メニュー ----
    #[pyo3(signature = (status=""))]
    fn open_menu(slf: &Bound<'_, Self>, status: &str) -> R<()> {
        let py = slf.py();
        let m = imp(py, "main.client.menu")?;
        m.call_method0("purge_trash")?;
        g!(slf, view).call_method1("grab", (false,))?;
        let menu = m.getattr("MainMenu")?.call1((g!(slf, cfg), slf.getattr("play")?, slf.getattr("quit")?, slf.getattr("join")?))?;
        slf.borrow_mut().menu = menu.clone().unbind();
        if !status.is_empty() {
            menu.call_method1("error", (status,))?;
        }
        Ok(())
    }
    fn quit(&mut self) {
        self.running = false;
    }
    fn play(slf: &Bound<'_, Self>, name: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let res = (|| -> R<Bound<'_, PyAny>> {
            let world_cls = imp(py, "main.app")?.call_method1("prepare_world", (name,))?;
            world_cls.call1((name, g!(slf, cfg)))?.call_method0("load")
        })();
        let world = match res {
            Ok(w) => w,
            Err(e) => {
                if is_mod_err(py, &e)? {
                    let errs = e.value_bound(py).getattr("errors")?;
                    let joined = pyo3::types::PyString::new_bound(py, "\n").call_method1("join", (&errs,))?;
                    log(py)?.call_method1("error", ("ワールド「%s」を始められません（MOD のエラー）:\n%s", name, joined))?;
                    if slf.borrow().menu.is_none(py) {
                        slf.call_method0("open_menu")?;
                    }
                    g!(slf, menu).call_method1("show_load_errors", (name, errs))?;
                    return Ok(());
                }
                if !e.is_instance_of::<pyo3::exceptions::PyException>(py) {
                    return Err(e);
                }
                log_exc(py, &e, "ワールドの読み込みに失敗: %s", (name,))?;
                if slf.borrow().menu.is_none(py) {
                    slf.call_method0("open_menu")?;
                }
                g!(slf, menu).call_method1("error", (format!("「{}」を読み込めません: {}", name.str()?, e.value_bound(py).str()?),))?;
                return Ok(());
            }
        };
        let menu = g!(slf, menu);
        if !menu.is_none() {
            menu.call_method0("destroy")?;
            slf.borrow_mut().menu = py.None();
        }
        let v = imp(py, "main.client_rs.viewer")?.getattr("Viewer")?.call1((world, g!(slf, cfg)))?;
        slf.borrow_mut().viewer = v.unbind();
        Ok(())
    }
    // ---- マルチプレイに参加 ----
    /// MOD とワールドを受け取ってから（別スレッド）、そのワールドで遊ぶ（今までの画面の menu.run_menu と同じ流れ）
    fn join(slf: &Bound<'_, Self>, host: &Bound<'_, PyAny>, port: &Bound<'_, PyAny>, name: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let menu = g!(slf, menu);
        let j = imp(py, "main.netclient")?.getattr("Joiner")?.call1((host, port, name))?;
        let cancel = Py::new(py, JoinCancel { j: j.clone().unbind(), menu: menu.clone().unbind() })?;
        menu.call_method1("show_joining", (&j, cancel))?;
        slf.borrow_mut().joining = j.unbind();
        Ok(())
    }
    fn _check_join(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let j = g!(slf, joining);
        if j.is_none() {
            return Ok(());
        }
        let err = j.getattr("error")?;
        if err.is_truthy()? {
            slf.borrow_mut().joining = py.None();
            let menu = g!(slf, menu);
            if !menu.is_none() && !j.getattr("cancelled")?.is_truthy()? {
                menu.setattr("status", format!("参加できません：{}", err.str()?))?;
                menu.call_method0("show_multiplayer")?;
            }
        } else if j.getattr("done")?.is_truthy()? {
            slf.borrow_mut().joining = py.None();
            slf.call_method1("start_joined", (j.getattr("result")?,))?;
        }
        Ok(())
    }
    fn start_joined(slf: &Bound<'_, Self>, info: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let paths = imp(py, "main.paths")?;
        let nc = imp(py, "main.netclient")?;
        let orig = paths.getattr("MODS")?;
        paths.setattr("MODS", nc.call_method0("mods_cache")?)?; // ホストから受け取った MOD で読み込む
        let r = (|| -> R<()> {
            let k = PyDict::new_bound(py);
            k.set_item("directory", info.get_item("dir")?)?;
            let res = imp(py, "main.world")?.getattr("World")?.call((info.get_item("world")?, g!(slf, cfg)), Some(&k)).and_then(|w| w.call_method0("load"));
            let world = match res {
                Ok(w) => w,
                Err(e) => {
                    if is_mod_err(py, &e)? {
                        info.get_item("conn")?.call_method0("close")?;
                        g!(slf, menu).call_method1("show_load_errors", (info.get_item("world")?, e.value_bound(py).getattr("errors")?))?;
                        return Ok(());
                    }
                    if !e.is_instance_of::<pyo3::exceptions::PyException>(py) {
                        return Err(e);
                    }
                    log_exc(py, &e, "受け取ったワールドを読み込めません", ())?;
                    info.get_item("conn")?.call_method0("close")?;
                    g!(slf, menu).call_method1("error", (format!("受け取ったワールドを読み込めません: {}", e.value_bound(py).str()?),))?;
                    return Ok(());
                }
            };
            nc.getattr("Client")?.call1((info, &world, g!(slf, cfg)))?;
            let menu = g!(slf, menu);
            if !menu.is_none() {
                menu.call_method0("destroy")?;
                slf.borrow_mut().menu = py.None();
            }
            let v = imp(py, "main.client_rs.viewer")?.getattr("Viewer")?.call1((world, g!(slf, cfg)))?;
            slf.borrow_mut().viewer = v.unbind();
            Ok(())
        })();
        paths.setattr("MODS", orig)?;
        r
    }
    /// ワールドを出る。MOD が決めた行き先（leave_action）：("menu"・"play:名前", 保存するか, 知らせ)
    fn leave(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let v = g!(slf, viewer);
        slf.borrow_mut().viewer = py.None();
        let la = v.getattr("leave_action").ok().filter(|x| x.is_truthy().unwrap_or(false));
        let (action, save, status): (String, bool, String) = match la {
            Some(t) => (t.get_item(0)?.extract()?, t.get_item(1)?.is_truthy()?, t.get_item(2)?.str()?.to_string()),
            None => ("menu".into(), true, String::new()),
        };
        let w = v.getattr("w")?;
        let net = w.getattr("net").unwrap_or_else(|_| py.None().into_bound(py));
        let client = !net.is_none() && net.getattr("role")?.eq("client")?;
        if save && !client {
            if let Err(e) = w.call_method0("save") {
                if !e.is_instance_of::<pyo3::exceptions::PyException>(py) {
                    return Err(e);
                }
                log_exc(py, &e, "保存に失敗しました", ())?;
            }
        }
        if !net.is_none() {
            if let Err(e) = net.call_method0("close") {
                if !e.is_instance_of::<pyo3::exceptions::PyException>(py) {
                    return Err(e);
                }
                log_exc(py, &e, "マルチプレイの終了でエラー", ())?;
            }
        }
        v.call_method0("destroy")?;
        let close_app = v.getattr("close_app").map(|x| x.is_truthy().unwrap_or(false)).unwrap_or(false);
        if close_app || action == "quit" {
            slf.borrow_mut().running = false;
        } else if let Some(n) = action.strip_prefix("play:") {
            slf.call_method1("play", (n,))?;
            if slf.borrow().viewer.is_none(py) && slf.borrow().menu.is_none(py) {
                slf.call_method1("open_menu", (status,))?;
            }
        } else {
            slf.call_method1("open_menu", (status,))?;
        }
        Ok(())
    }
    // ---- まわす ----
    fn frame(slf: &Bound<'_, Self>, dt: f64) -> R<()> {
        let py = slf.py();
        let v = g!(slf, viewer);
        if !v.is_none() {
            v.call_method1("frame", (dt,))?;
            if g!(slf, viewer).getattr("quit")?.is_truthy()? {
                slf.call_method0("leave")?;
            }
            return Ok(());
        }
        let view = g!(slf, view);
        let u = g!(slf, u);
        let evs = view.call_method0("pump")?;
        for ev in evs.iter()? {
            let ev = ev?;
            let (kind, a, b, n) = (ev.get_item(0)?, ev.get_item(1)?, ev.get_item(2)?, ev.get_item(3)?);
            if kind.eq("close")? {
                slf.borrow_mut().running = false;
            }
            u.call_method1("event", (&view, kind, a, b, n))?;
        }
        u.call_method1("frame", (&view, dt))?;
        slf.call_method0("_check_join")?;
        imp(py, "main.client.sound")?.call_method0("get")?.call_method1("update", (dt,))?;
        let c = u.getattr("window")?.getattr("color")?;
        view.call_method1("set_clear", (c.get_item(0)?, c.get_item(1)?, c.get_item(2)?))?;
        view.call_method0("render")?;
        Ok(())
    }
    #[pyo3(signature = (world_name=None))]
    fn run(slf: &Bound<'_, Self>, world_name: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        if let Some(n) = world_name.filter(|n| n.is_truthy().unwrap_or(false)) {
            slf.call_method1("play", (n,))?;
        }
        if slf.borrow().viewer.is_none(py) {
            slf.call_method0("open_menu")?;
        }
        let t = imp(py, "time")?;
        let mut last: f64 = t.call_method0("perf_counter")?.extract()?;
        while slf.borrow().running {
            let now: f64 = t.call_method0("perf_counter")?.extract()?;
            let dt = (now - last).min(0.1);
            last = now;
            slf.call_method1("frame", (dt,))?;
        }
        Ok(())
    }
}

/// 参加の取り消し（参加を止めて、マルチプレイの画面に戻る）
#[pyclass(module = "alpedia_core")]
pub struct JoinCancel {
    j: PyObject,
    menu: PyObject,
}
#[pymethods]
impl JoinCancel {
    fn __call__(&self, py: Python<'_>) -> R<()> {
        self.j.bind(py).call_method0("cancel")?;
        self.menu.bind(py).call_method0("show_multiplayer")?;
        Ok(())
    }
}

#[pyfunction]
#[pyo3(signature = (cfg, world_name=None))]
pub fn rs_run_client(py: Python<'_>, cfg: &Bound<'_, PyAny>, world_name: Option<&Bound<'_, PyAny>>) -> R<()> {
    let c = py.get_type_bound::<RsClient>().call1((cfg,))?;
    c.call_method1("run", (world_name.map(|w| w.clone().unbind()).unwrap_or_else(|| py.None()),))?;
    let _ = PyList::empty_bound(py);
    Ok(())
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<RsClient>()?;
    m.add_class::<JoinCancel>()?;
    m.add_function(wrap_pyfunction_bound!(rs_run_client, m)?)?;
    Ok(())
}
