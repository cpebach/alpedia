//! マルチプレイ（LAN）のホスト側（main/netserver.py の Host）と参加者側（main/netclient.py の Joiner・Client）。
//! ホストの PC がワールドを持ち、物理・プログラム・MOD の決まりを全部動かす。参加者は自分の体だけ動かして知らせ、
//! 置く・壊す・掘る・E などの操作はホストへ送る。

use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyBytes, PyDict, PyList, PyTuple};

use super::imp;

type R<T> = PyResult<T>;

const STATE_HZ: f64 = 20.0;
const META_HZ: f64 = 8.0;
const ADDON_HZ: f64 = 10.0;
const FILE_HZ: f64 = 1.0;
const CHUNK: usize = 512 * 1024;
const ME_HZ: f64 = 20.0;
const EDIT_HZ: f64 = 2.0;

fn slog<'py>(py: Python<'py>, name: &str) -> R<Bound<'py, PyAny>> {
    imp(py, "main.log")?.call_method1("get", (name,))
}
fn mono(py: Python<'_>) -> R<f64> {
    imp(py, "time")?.call_method0("monotonic")?.extract()
}
fn net(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "main.net")
}
fn ns(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "main.netsync")
}
fn paths(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "main.paths")
}
fn div<'py>(p: &Bound<'py, PyAny>, s: impl IntoPy<PyObject>) -> R<Bound<'py, PyAny>> {
    p.call_method1("__truediv__", (s,))
}
fn dict<'py>(py: Python<'py>, kv: Vec<(&str, PyObject)>) -> R<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    for (k, v) in kv {
        d.set_item(k, v)?;
    }
    Ok(d)
}
fn exc_log(py: Python<'_>, lg: &Bound<'_, PyAny>, e: &PyErr, msg: &str, args: Vec<PyObject>) -> R<()> {
    let mut v: Vec<PyObject> = vec![msg.into_py(py)];
    v.extend(args);
    let kw = PyDict::new_bound(py);
    kw.set_item("exc_info", e.value_bound(py))?;
    lg.call_method("exception", PyTuple::new_bound(py, v), Some(&kw))?;
    Ok(())
}
/// with world.as_player(p): f()
fn as_player<T>(py: Python<'_>, w: &Bound<'_, PyAny>, p: &Bound<'_, PyAny>, f: impl FnOnce() -> R<T>) -> R<T> {
    let ap = w.call_method1("as_player", (p,))?;
    ap.call_method0("__enter__")?;
    let r = f();
    ap.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
    r
}
fn sha1_file(py: Python<'_>, f: &Bound<'_, PyAny>) -> R<String> {
    let h = imp(py, "hashlib")?.call_method0("sha1")?;
    let fh = imp(py, "builtins")?.call_method1("open", (f, "rb"))?;
    let r = (|| -> R<()> {
        loop {
            let chunk = fh.call_method1("read", (1usize << 20,))?;
            if chunk.len()? == 0 {
                break;
            }
            h.call_method1("update", (chunk,))?;
        }
        Ok(())
    })();
    fh.call_method0("close")?;
    r?;
    h.call_method0("hexdigest")?.extract()
}

// ---------------------------------------------------------------- ホスト

static SHA: pyo3::sync::GILOnceCell<Py<PyDict>> = pyo3::sync::GILOnceCell::new();

#[pyfunction]
pub fn ns_sha1_cached(py: Python<'_>, f: &Bound<'_, PyAny>) -> R<String> {
    let st = f.call_method0("stat")?;
    let key = PyTuple::new_bound(py, [f.str()?.into_any(), st.getattr("st_mtime_ns")?, st.getattr("st_size")?]);
    let cache = SHA.get_or_init(py, || PyDict::new_bound(py).unbind()).bind(py);
    if let Some(v) = cache.get_item(&key)? {
        return v.extract();
    }
    let h = sha1_file(py, f)?;
    cache.set_item(key, &h)?;
    Ok(h)
}

/// MOD のファイルの一覧 {MOD名: [[相対パス, 大きさ, sha1], ...]}（参加者が足りない物だけ受け取るため）
#[pyfunction]
pub fn ns_mod_manifest<'py>(py: Python<'py>, mod_ids: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let out = PyDict::new_bound(py);
    for mid in mod_ids.iter()? {
        let mid = mid?;
        let root = div(&paths(py)?.getattr("MODS")?, &mid)?;
        let files = PyList::empty_bound(py);
        if root.call_method0("is_dir")?.is_truthy()? {
            let all = imp(py, "builtins")?.call_method1("sorted", (root.call_method1("rglob", ("*",))?,))?;
            for f in all.iter()? {
                let f = f?;
                let parts: Vec<String> = f.getattr("parts")?.extract()?;
                let suf: String = f.getattr("suffix")?.extract()?;
                if !f.call_method0("is_file")?.is_truthy()? || parts.iter().any(|p| p == "__pycache__") || suf == ".pyc" || suf == ".pyo" {
                    continue;
                }
                let rel = f.call_method1("relative_to", (&root,))?.call_method0("as_posix")?;
                files.append(PyList::new_bound(py, [rel, f.call_method0("stat")?.getattr("st_size")?, ns_sha1_cached(py, &f)?.into_py(py).into_bound(py)]))?;
            }
        }
        out.set_item(mid, files)?;
    }
    Ok(out)
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Peer {
    #[pyo3(get, set)]
    conn: PyObject,
    #[pyo3(get, set)]
    addr: PyObject,
    #[pyo3(get, set)]
    name: PyObject,
    #[pyo3(get, set)]
    pid: PyObject,
    #[pyo3(get, set)]
    player: PyObject,
    #[pyo3(get, set)]
    ready: bool,
    #[pyo3(get, set)]
    you_key: PyObject,
}

#[pymethods]
impl Peer {
    #[new]
    fn new(py: Python<'_>, conn: PyObject, addr: PyObject) -> Self {
        Peer { conn, addr, name: py.None(), pid: py.None(), player: py.None(), ready: false, you_key: py.None() }
    }
}

#[pyfunction]
pub fn ns_default_name(py: Python<'_>) -> R<String> {
    match imp(py, "getpass")?.call_method0("getuser") {
        Ok(u) => Ok(u.extract::<String>()?.chars().take(24).collect()),
        Err(_) => Ok(imp(py, "socket")?.call_method0("gethostname")?.extract::<String>()?.chars().take(24).collect()),
    }
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Host {
    #[pyo3(get, set)]
    w: PyObject,
    #[pyo3(get, set)]
    cfg: PyObject,
    #[pyo3(get, set)]
    name: PyObject,
    #[pyo3(get, set)]
    inbox: PyObject,
    #[pyo3(get, set)]
    listener: PyObject,
    #[pyo3(get, set)]
    port: PyObject,
    #[pyo3(get, set)]
    announcer: PyObject,
    #[pyo3(get, set)]
    peers: PyObject,
    #[pyo3(get, set)]
    next_pid: i64,
    #[pyo3(get, set)]
    cons: PyObject,
    #[pyo3(get, set)]
    meta: PyObject,
    #[pyo3(get, set)]
    craters: PyObject,
    #[pyo3(get, set)]
    addon_sent: PyObject,
    #[pyo3(get, set)]
    files: PyObject,
    #[pyo3(get, set)]
    known: PyObject,
    #[pyo3(get, set, name = "_manifest")]
    manifest: PyObject,
    #[pyo3(get, set, name = "_t")]
    t: PyObject,
    #[pyo3(get, set, name = "_last_state")]
    last_state: f64,
}

impl Host {
    fn g<'py>(slf: &Bound<'py, Self>, k: &str) -> R<Bound<'py, PyAny>> {
        slf.as_any().getattr(k)
    }
}

#[pymethods]
impl Host {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const role: &'static str = "host";

    #[new]
    #[pyo3(signature = (world, cfg, port=None))]
    fn new(py: Python<'_>, world: &Bound<'_, PyAny>, cfg: &Bound<'_, PyAny>, port: Option<i64>) -> R<Self> {
        let mp = cfg.call_method1("get", ("multiplayer", PyDict::new_bound(py)))?;
        let nm = mp.call_method1("get", ("name",))?;
        let name: PyObject = if nm.is_truthy()? { nm.unbind() } else { ns_default_name(py)?.into_py(py) };
        let net = net(py)?;
        let inbox = net.call_method0("Inbox")?;
        let port = match port { Some(p) => p, None => mp.call_method1("get", ("port", net.getattr("PORT")?))?.call_method0("__int__")?.extract()? };
        let listener = net.call_method1("Listener", (port, &inbox))?;
        let lport = listener.getattr("port")?;
        let craters = PyDict::new_bound(py);
        for (k, p) in world.getattr("planets")?.iter()?.enumerate() {
            craters.set_item(k, p?.getattr("craters")?.len()?)?;
        }
        let files = ns(py)?.getattr("FileSync")?.call1((div(&world.getattr("dir")?, "scripts")?,))?;
        files.call_method0("mark_all")?;
        files.call_method0("changes")?;
        let known = (|| -> R<Bound<'_, PyAny>> {
            let t = div(&world.getattr("dir")?, "players.json")?.call_method("read_text", (), Some(&[("encoding", "utf-8")].into_py_dict_bound(py)))?;
            imp(py, "json")?.call_method1("loads", (t,))
        })().unwrap_or_else(|_| PyDict::new_bound(py).into_any());
        let t = PyDict::new_bound(py);
        for k in ["state", "meta", "addon", "file"] {
            t.set_item(k, 0.0)?;
        }
        let me = Host {
            w: world.clone().unbind(), cfg: cfg.clone().unbind(), name, inbox: inbox.clone().unbind(), listener: listener.clone().unbind(),
            port: lport.clone().unbind(), announcer: py.None(), peers: PyDict::new_bound(py).into_any().unbind(), next_pid: 1,
            cons: ns(py)?.getattr("ConsTracker")?.call0()?.unbind(), meta: ns(py)?.getattr("MetaTracker")?.call0()?.unbind(),
            craters: craters.into_any().unbind(), addon_sent: PyDict::new_bound(py).into_any().unbind(), files: files.unbind(),
            known: known.unbind(), manifest: py.None(), t: t.into_any().unbind(), last_state: world.getattr("time")?.extract()?,
        };
        Ok(me)
    }

    /// 作ったあとの準備（知らせ・ワールドへの登録）。Python の Host(...) から呼ばれる
    fn _start(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let net = net(py)?;
        let ann = net.call_method1("Announcer", (slf.getattr("_info")?,))?;
        slf.borrow_mut().announcer = ann.unbind();
        let w = Self::g(slf, "w")?;
        w.setattr("net", slf)?;
        w.getattr("local_player")?.setattr("name", Self::g(slf, "name")?)?;
        let port = Self::g(slf, "port")?;
        let addrs: Vec<String> = net.call_method0("local_addresses")?.extract()?;
        slog(py, "netserver")?.call_method1("info", ("LAN に公開: %s ポート %d（%s）。参加できないときは、この PC のファイアウォールで TCP %d・UDP %d を許可してください（LANで公開できるようにする.bat）",
            w.getattr("name")?, &port, addrs.join(", "), &port, net.getattr("DISCOVERY_PORT")?))?;
        Ok(())
    }

    // --- 情報 ---
    fn _info<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let w = Self::g(slf, "w")?;
        dict(py, vec![("world", w.getattr("name")?.unbind()), ("host", Self::g(slf, "name")?.unbind()), ("port", Self::g(slf, "port")?.unbind()),
            ("players", (1 + w.getattr("remote")?.len()?).into_py(py)),
            ("mods", PyList::new_bound(py, w.getattr("mods")?.iter()?.collect::<R<Vec<_>>>()?).into_any().unbind())])
    }
    fn addresses(slf: &Bound<'_, Self>) -> R<Vec<String>> {
        let py = slf.py();
        let port = Self::g(slf, "port")?.str()?.to_string();
        let ips: Vec<String> = net(py)?.call_method0("local_addresses")?.extract()?;
        Ok(ips.into_iter().map(|ip| format!("{ip}:{port}")).collect())
    }
    fn can_save(&self) -> bool {
        true
    }
    fn save_extra<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let known = Self::g(slf, "known")?;
        for p in Self::g(slf, "w")?.getattr("remote")?.call_method0("values")?.iter()? {
            let p = p?;
            known.set_item(p.getattr("name")?, p.call_method0("to_dict")?)?;
        }
        let out = PyDict::new_bound(py);
        if known.is_truthy()? {
            out.set_item("players.json", known)?;
        }
        Ok(out)
    }
    fn others<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyList>> {
        Ok(PyList::new_bound(slf.py(), Self::g(slf, "w")?.getattr("remote")?.call_method0("values")?.iter()?.collect::<R<Vec<_>>>()?))
    }
    fn send(&self, msg: &Bound<'_, PyAny>) {
        let _ = msg;
    }

    // --- 毎フレーム ---
    fn tick(slf: &Bound<'_, Self>, dt: f64) -> R<()> {
        let py = slf.py();
        let _ = dt;
        Self::_process(slf)?;
        let now = mono(py)?;
        let ready = Self::ready_peers(slf, None, true)?;
        let w = Self::g(slf, "w")?;
        if ready.is_empty() {
            slf.borrow_mut().last_state = w.getattr("time")?.extract()?;
            return Ok(());
        }
        let t = Self::g(slf, "_t")?;
        let due = |k: &str, hz: f64| -> R<bool> {
            if now - t.get_item(k)?.extract::<f64>()? >= 1.0 / hz {
                t.set_item(k, now)?;
                return Ok(true);
            }
            Ok(false)
        };
        let rl = PyList::new_bound(py, &ready);
        if due("state", STATE_HZ)? {
            Self::_send_state(slf, &rl, now)?;
        }
        if due("meta", META_HZ)? {
            Self::_send_meta(slf, &rl)?;
        }
        if due("addon", ADDON_HZ)? {
            Self::_send_addons(slf, &rl)?;
        }
        if due("file", FILE_HZ)? {
            let ch = Self::g(slf, "files")?.call_method0("changes")?;
            if ch.is_truthy()? {
                Self::_broadcast(slf, &rl, dict(py, vec![("t", "files".into_py(py)), ("files", ch.unbind())])?.as_any())?;
            }
        }
        for p in &ready {
            Self::_send_you(slf, p)?;
        }
        Ok(())
    }
    fn _broadcast(slf: &Bound<'_, Self>, peers: &Bound<'_, PyAny>, msg: &Bound<'_, PyAny>) -> R<()> {
        let _ = slf;
        for p in peers.iter()? {
            p?.getattr("conn")?.call_method1("send", (msg,))?;
        }
        Ok(())
    }
    #[pyo3(signature = (ready, now=0.0))]
    fn _send_state(slf: &Bound<'_, Self>, ready: &Bound<'_, PyAny>, now: f64) -> R<()> {
        let py = slf.py();
        let _ = now;
        let w = Self::g(slf, "w")?;
        let nsm = ns(py)?;
        let r = Self::g(slf, "cons")?.call_method1("diff", (&w,))?;
        let players = PyList::empty_bound(py);
        players.append(nsm.call_method1("player_public", (w.getattr("local_player")?, 0, Self::g(slf, "name")?))?)?;
        for p in Self::g(slf, "peers")?.call_method0("values")?.iter()? {
            let p = p?;
            if p.getattr("ready")?.is_truthy()? {
                players.append(nsm.call_method1("player_public", (p.getattr("player")?, p.getattr("pid")?, p.getattr("name")?))?)?;
            }
        }
        let craters = PyList::empty_bound(py);
        let cr = Self::g(slf, "craters")?;
        for (k, pl) in w.getattr("planets")?.iter()?.enumerate() {
            let pl = pl?;
            let pc = pl.getattr("craters")?;
            let n = pc.len()?;
            let n0: usize = cr.call_method1("get", (k, 0))?.extract()?;
            if n != n0 {
                let start = n0.min(n);
                let lst = PyList::empty_bound(py);
                for c in pc.downcast::<PyList>()?.get_slice(start, n).iter() {
                    let v: Vec<f64> = c.iter()?.map(|x| x?.call_method0("__float__")?.extract::<f64>()).collect::<R<_>>()?;
                    lst.append(v)?;
                }
                craters.append(dict(py, vec![("p", k.into_py(py)), ("from", start.into_py(py)), ("list", lst.into_any().unbind())])?)?;
                cr.set_item(k, n)?;
            }
        }
        let time: f64 = w.getattr("time")?.extract()?;
        let last = slf.borrow().last_state;
        let msg = dict(py, vec![("t", "s".into_py(py)), ("time", time.into_py(py)), ("dt", (time - last).into_py(py)),
            ("c", r.get_item(0)?.unbind()), ("gone", r.get_item(1)?.unbind()), ("pl", players.into_any().unbind())])?;
        if !craters.is_empty() {
            msg.set_item("craters", craters)?;
        }
        slf.borrow_mut().last_state = time;
        Self::_broadcast(slf, ready, &msg)
    }
    fn _send_meta(slf: &Bound<'_, Self>, ready: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let m = Self::g(slf, "meta")?.call_method1("diff", (Self::g(slf, "w")?,))?;
        if m.is_truthy()? {
            Self::_broadcast(slf, ready, dict(py, vec![("t", "m".into_py(py)), ("c", m.unbind())])?.as_any())?;
        }
        Ok(())
    }
    fn _send_addons(slf: &Bound<'_, Self>, ready: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let w = Self::g(slf, "w")?;
        let out = PyDict::new_bound(py);
        let sent = Self::g(slf, "addon_sent")?;
        let nsm = ns(py)?;
        for a in w.getattr("addons")?.iter()? {
            let a = a?;
            let st = match a.call_method1("net_state", (&w,)) {
                Ok(s) => s,
                Err(e) if e.is_instance_of::<pyo3::exceptions::PyException>(py) => {
                    exc_log(py, &slog(py, "netserver")?, &e, "MOD '%s' の状態を作れません", vec![a.getattr("name")?.unbind()])?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            if st.is_none() {
                continue;
            }
            let st = nsm.call_method1("jsonable", (st,))?;
            let key = nsm.call_method1("dumps", (&st,))?;
            let nm = a.getattr("name")?;
            if !sent.call_method1("get", (&nm,))?.eq(&key)? {
                sent.set_item(&nm, key)?;
                out.set_item(nm, st)?;
            }
        }
        if !out.is_empty() {
            Self::_broadcast(slf, ready, dict(py, vec![("t", "a".into_py(py)), ("s", out.into_any().unbind())])?.as_any())?;
        }
        Ok(())
    }
    fn _send_you(slf: &Bound<'_, Self>, peer: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let nsm = ns(py)?;
        let p = peer.getattr("player")?;
        let s = p.getattr("seated")?;
        let seated: PyObject = if s.is_truthy()? {
            PyList::new_bound(py, [s.get_item(0)?, PyList::new_bound(py, s.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any()]).into_any().unbind()
        } else { py.None() };
        let you = nsm.call_method1("jsonable", (dict(py, vec![("inventory", p.getattr("inventory")?.unbind()), ("seated", seated),
            ("starter_given", p.getattr("starter_given")?.unbind()), ("credits_given", p.getattr("credits_given")?.unbind()),
            ("out_of_fuel", p.getattr("out_of_fuel")?.unbind())])?,))?;
        let key = nsm.call_method1("dumps", (&you,))?;
        let pm = p.getattr("msgs")?;
        let msgs = PyList::new_bound(py, pm.iter()?.collect::<R<Vec<_>>>()?);
        pm.call_method0("clear")?;
        if !key.eq(peer.getattr("you_key")?)? || !msgs.is_empty() {
            peer.setattr("you_key", key)?;
            let m = PyDict::new_bound(py);
            m.set_item("t", "you")?;
            m.update(you.downcast::<PyDict>()?.as_mapping())?;
            let texts = PyList::empty_bound(py);
            for x in msgs.iter() {
                texts.append(x.get_item(1)?)?;
            }
            m.set_item("msgs", texts)?;
            peer.getattr("conn")?.call_method1("send", (m,))?;
        }
        Ok(())
    }

    // --- 受け取り ---
    fn _process(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let inbox = Self::g(slf, "inbox")?;
        let empty = imp(py, "queue")?.getattr("Empty")?;
        for _ in 0..2000 {
            let item = match inbox.call_method0("get_nowait") {
                Ok(i) => i,
                Err(e) if e.is_instance_bound(py, &empty) => break,
                Err(e) => return Err(e),
            };
            let (conn, msg) = (item.get_item(0)?, item.get_item(1)?);
            if let Err(e) = Self::_handle(slf, &conn, &msg) {
                if !e.is_instance_of::<pyo3::exceptions::PyException>(py) {
                    return Err(e);
                }
                exc_log(py, &slog(py, "netserver")?, &e, "参加者からのメッセージ %s の処理でエラー", vec![msg.call_method1("get", ("t",))?.unbind()])?;
            }
        }
        Ok(())
    }
    fn _handle(slf: &Bound<'_, Self>, conn: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let t = m.call_method1("get", ("t",))?;
        let ts: String = if t.is_none() { String::new() } else { t.extract()? };
        let peers = Self::g(slf, "peers")?;
        let lg = slog(py, "netserver")?;
        if ts == "_open" {
            let p = Bound::new(py, Peer::new(py, conn.clone().unbind(), m.call_method1("get", ("addr",))?.unbind()))?;
            peers.set_item(conn, p)?;
            lg.call_method1("info", ("つながりを受け付けました: %s", m.call_method1("get", ("addr",))?))?;
            return Ok(());
        }
        let peer = peers.call_method1("get", (conn,))?;
        if peer.is_none() {
            return Ok(());
        }
        match ts.as_str() {
            "_closed" | "bye" => {
                if peer.getattr("player")?.is_none() {
                    let nm = peer.getattr("name")?;
                    let why = m.call_method1("get", ("why",))?;
                    lg.call_method1("info", ("参加の途中で切れました: %s（%s）%s", peer.getattr("addr")?,
                        if nm.is_truthy()? { nm } else { pyo3::types::PyString::new_bound(py, "名前の前").into_any() },
                        if why.is_truthy()? { why } else { pyo3::types::PyString::new_bound(py, "").into_any() }))?;
                }
                Self::_leave(slf, &peer)
            }
            "hello" => Self::_hello(slf, &peer, m),
            "get_files" => Self::_send_files(slf, &peer, &m.call_method1("get", ("files", PyList::empty_bound(py)))?),
            "ready" => Self::_ready(slf, &peer, m),
            _ => {
                if peer.getattr("ready")?.is_truthy()? {
                    Self::_game_msg(slf, &peer, m)?;
                }
                Ok(())
            }
        }
    }
    fn _hello(slf: &Bound<'_, Self>, peer: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        slog(py, "netserver")?.call_method1("info", ("参加の申し込み: %s（%s）", m.call_method1("get", ("name",))?, peer.getattr("addr")?))?;
        let conn = peer.getattr("conn")?;
        if !m.call_method1("get", ("proto",))?.eq(net(py)?.getattr("PROTO")?)? {
            conn.call_method1("send", (dict(py, vec![("t", "error".into_py(py)), ("text", "ゲームの版が違うので参加できません（ホストと同じ版にしてください）".into_py(py))])?,))?;
            return Ok(());
        }
        let n0 = m.call_method1("get", ("name",))?;
        let n0: String = if n0.is_truthy()? { n0.str()?.to_string() } else { "player".into() };
        let mut name: String = n0.trim().chars().take(24).collect();
        if name.is_empty() {
            name = "player".into();
        }
        let mut used = std::collections::HashSet::new();
        used.insert(Self::g(slf, "name")?.str()?.to_string());
        for p in Self::g(slf, "peers")?.call_method0("values")?.iter()? {
            let nm = p?.getattr("name")?;
            if nm.is_truthy()? {
                used.insert(nm.str()?.to_string());
            }
        }
        let base = name.clone();
        let mut k = 2;
        while used.contains(&name) {
            name = format!("{base}{k}");
            k += 1;
        }
        peer.setattr("name", &name)?;
        let pid = slf.borrow().next_pid;
        peer.setattr("pid", pid)?;
        slf.borrow_mut().next_pid += 1;
        let w = Self::g(slf, "w")?;
        if Self::g(slf, "_manifest")?.is_none() {
            let mf = ns_mod_manifest(py, &w.getattr("registry")?.getattr("loaded_mods")?)?;
            slf.borrow_mut().manifest = mf.into_any().unbind();
        }
        let lst = |o: Bound<'_, PyAny>| -> R<PyObject> { Ok(PyList::new_bound(py, o.iter()?.collect::<R<Vec<_>>>()?).into_any().unbind()) };
        conn.call_method1("send", (dict(py, vec![("t", "welcome".into_py(py)), ("pid", pid.into_py(py)), ("name", name.into_py(py)),
            ("world", w.getattr("name")?.unbind()), ("mods", lst(w.getattr("mods")?)?), ("loaded", lst(w.getattr("registry")?.getattr("loaded_mods")?)?),
            ("manifest", Self::g(slf, "_manifest")?.unbind()), ("host", Self::g(slf, "name")?.unbind())])?,))?;
        Ok(())
    }
    fn _send_files(slf: &Bound<'_, Self>, peer: &Bound<'_, PyAny>, files: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let conn = peer.getattr("conn")?;
        let mods = paths(py)?.getattr("MODS")?;
        for it in files.iter()? {
            let it = it?;
            let (mid, rel) = (it.get_item(0)?, it.get_item(1)?);
            let root = div(&mods, &mid)?.call_method0("resolve")?;
            let f = div(&root, &rel)?.call_method0("resolve")?;
            let inside = f.getattr("parents")?.contains(&root)?;
            if !inside || !f.call_method0("is_file")?.is_truthy()? {
                conn.call_method1("send", (dict(py, vec![("t", "file".into_py(py)), ("mod", mid.unbind()), ("rel", rel.unbind()),
                    ("data", PyBytes::new_bound(py, b"").into_any().unbind()), ("last", true.into_py(py)), ("missing", true.into_py(py))])?,))?;
                continue;
            }
            let size: usize = f.call_method0("stat")?.getattr("st_size")?.extract()?;
            let fh = imp(py, "builtins")?.call_method1("open", (&f, "rb"))?;
            let r = (|| -> R<()> {
                loop {
                    let data = fh.call_method1("read", (CHUNK,))?;
                    let last = fh.call_method0("tell")?.extract::<usize>()? >= size;
                    conn.call_method1("send", (dict(py, vec![("t", "file".into_py(py)), ("mod", mid.clone().unbind()), ("rel", rel.clone().unbind()),
                        ("data", data.unbind()), ("last", last.into_py(py))])?,))?;
                    if last {
                        break;
                    }
                }
                Ok(())
            })();
            fh.call_method0("close")?;
            r?;
        }
        conn.call_method1("send", (dict(py, vec![("t", "files_done".into_py(py))])?,))?;
        Ok(())
    }
    fn _ready(slf: &Bound<'_, Self>, peer: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let w = Self::g(slf, "w")?;
        let pname = peer.getattr("name")?;
        let old = Self::g(slf, "known")?.call_method1("get", (&pname,))?;
        let lp = w.getattr("local_player")?;
        let p = if old.is_truthy()? {
            let d = old.call_method1("get", ("def",))?;
            let p = w.call_method1("make_player", (if d.is_truthy()? { d } else { m.call_method1("get", ("def",))? },))?;
            p.call_method1("load_dict", (&old,))?;
            p
        } else {
            let fwd = match super::construct::opt(&lp, "look_fwd")? { Some(f) => super::construct::v3(&f)?, None => [0.0, 0.0, 1.0] };
            let up = super::construct::v3(&lp.getattr("up")?)?;
            let side = super::construct::cross(up, fwd);
            let pos = super::construct::v3(&lp.getattr("pos")?)?;
            let at = [pos[0] + side[0] * 1.5 + up[0] * 0.2, pos[1] + side[1] * 1.5 + up[1] * 0.2, pos[2] + side[2] * 1.5 + up[2] * 0.2];
            let p = w.call_method1("make_player", (m.call_method1("get", ("def",))?, super::construct::a1(py, at)))?;
            p.setattr("up", lp.getattr("up")?.call_method0("copy")?)?;
            p
        };
        p.setattr("name", &pname)?;
        p.setattr("pid", peer.getattr("pid")?)?;
        p.setattr("seated", py.None())?;
        peer.setattr("player", &p)?;
        w.getattr("remote")?.set_item(peer.getattr("pid")?, &p)?;
        as_player(py, &w, &p, || {
            for a in w.getattr("addons")?.iter()? {
                let a = a?;
                if let Err(e) = a.call_method1("on_player_join", (&w, &p)) {
                    if !e.is_instance_of::<pyo3::exceptions::PyException>(py) {
                        return Err(e);
                    }
                    exc_log(py, &slog(py, "netserver")?, &e, "MOD '%s' の参加の処理でエラー", vec![a.getattr("name")?.unbind()])?;
                }
            }
            Ok(())
        })?;
        let snap = w.call_method0("snapshot")?;
        snap.set_item("player.json", p.call_method0("to_dict")?)?;
        let scripts = Self::g(slf, "files")?.call_method0("all")?;
        let cf = PyDict::new_bound(py);
        for (k, pl) in w.getattr("planets")?.iter()?.enumerate() {
            cf.set_item(k, pl?.getattr("craters")?.len()?)?;
        }
        peer.getattr("conn")?.call_method1("send", (dict(py, vec![("t", "world".into_py(py)), ("files", ns(py)?.call_method1("jsonable", (snap,))?.unbind()),
            ("scripts", scripts.unbind()), ("craters_from", cf.into_any().unbind())])?,))?;
        peer.setattr("ready", true)?;
        w.call_method1("message", (format!("{} が参加しました", pname.str()?),))?;
        slog(py, "netserver")?.call_method1("info", ("参加: %s（%s）", &pname, peer.getattr("addr")?))?;
        let others = PyList::new_bound(py, Self::ready_peers(slf, Some(peer), false)?);
        Self::_broadcast(slf, &others, dict(py, vec![("t", "note".into_py(py)), ("text", format!("{} が参加しました", pname.str()?).into_py(py))])?.as_any())
    }
    fn _leave(slf: &Bound<'_, Self>, peer: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        Self::g(slf, "peers")?.call_method1("pop", (peer.getattr("conn")?, py.None()))?;
        peer.getattr("conn")?.call_method0("close")?;
        let p = peer.getattr("player")?;
        if !p.is_none() {
            let nm = peer.getattr("name")?;
            Self::g(slf, "known")?.set_item(&nm, p.call_method0("to_dict")?)?;
            let w = Self::g(slf, "w")?;
            w.getattr("remote")?.call_method1("pop", (peer.getattr("pid")?, py.None()))?;
            w.call_method1("message", (format!("{} が退出しました", nm.str()?),))?;
            slog(py, "netserver")?.call_method1("info", ("退出: %s", &nm))?;
            let rest = PyList::new_bound(py, Self::ready_peers(slf, None, false)?);
            Self::_broadcast(slf, &rest, dict(py, vec![("t", "note".into_py(py)), ("text", format!("{} が退出しました", nm.str()?).into_py(py))])?.as_any())?;
        }
        Ok(())
    }

    // --- 参加者の操作 ---
    fn _game_msg(slf: &Bound<'_, Self>, peer: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let t: String = m.get_item("t")?.extract()?;
        let w = Self::g(slf, "w")?;
        let p = peer.getattr("player")?;
        let np = imp(py, "numpy")?;
        let f64a = |o: Bound<'_, PyAny>| np.call_method1("asarray", (o, "float64"));
        let get = |k: &str, d: PyObject| m.call_method1("get", (k, d));
        match t.as_str() {
            "me" => {
                p.setattr("pos", f64a(m.get_item("pos")?)?)?;
                p.setattr("vel", f64a(m.get_item("vel")?)?)?;
                p.setattr("up", f64a(m.get_item("up")?)?)?;
                p.setattr("ground_vel", f64a(get("gv", vec![0, 0, 0].into_py(py))?)?)?;
                p.setattr("on_ground", get("og", py.None())?.is_truthy()?)?;
                p.setattr("jetpack", get("jp", py.None())?.is_truthy()?)?;
                p.setattr("flying", get("fl", py.None())?.is_truthy()?)?;
                p.setattr("thrust", f64a(get("thrust", vec![0, 0, 0].into_py(py))?)?)?;
                p.setattr("in_water", get("wt", 0.0.into_py(py))?.call_method0("__float__")?)?;
                let keys = get("keys", py.None())?;
                p.setattr("input_keys", pyo3::types::PySet::new_bound(py, &if keys.is_truthy()? { keys.iter()?.collect::<R<Vec<_>>>()? } else { vec![] })?)?;
                p.setattr("look_fwd", f64a(get("fwd", vec![0, 0, 1].into_py(py))?)?)?;
                p.setattr("pitch", get("pitch", 0.0.into_py(py))?.call_method0("__float__")?)?;
                let st = get("stand", py.None())?;
                p.setattr("stand", if st.is_truthy()? {
                    PyTuple::new_bound(py, [st.get_item(0)?, PyTuple::new_bound(py, st.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any()]).into_any()
                } else { py.None().into_bound(py) })?;
                let em = get("emote", py.None())?;
                if !em.eq(p.getattr("emote")?)? {
                    p.setattr("emote", &em)?;
                    p.setattr("emote_t0", mono(py)?)?;
                }
                let fc = get("face", py.None())?;
                p.setattr("face_dir", if fc.is_none() { fc } else { f64a(fc)? })?;
                let d = get("def", py.None())?;
                if d.is_truthy()? && !d.eq(p.getattr("defn")?.call_method1("get", ("id",))?)? {
                    p.setattr("defn", w.call_method1("player_def", (d,))?)?;
                }
                if m.contains("hotbar")? {
                    let hb = m.get_item("hotbar")?;
                    p.setattr("hotbar", if hb.is_truthy()? {
                        let l = PyList::empty_bound(py);
                        for x in hb.iter()? {
                            let x = x?;
                            l.append(if x.is_truthy()? { PyTuple::new_bound(py, x.iter()?.collect::<R<Vec<_>>>()?).into_any() } else { py.None().into_bound(py) })?;
                        }
                        l.into_any()
                    } else { py.None().into_bound(py) })?;
                }
                let s = p.getattr("seated")?;
                if s.is_truthy()? && !w.getattr("constructs")?.call_method1("get", (s.get_item(0)?,))?.is_truthy()? {
                    p.setattr("seated", py.None())?;
                }
            }
            "act" => as_player(py, &w, &p, || Self::_act(slf, &p, m))?,
            "dev" => {
                ns(py)?.call_method1("apply_dev_patch", (&w, m.get_item("id")?, get("patches", PyList::empty_bound(py).into_any().unbind())?))?;
            }
            "files" => {
                let fs = get("files", py.None())?;
                let fs = if fs.is_truthy()? { fs } else { PyDict::new_bound(py).into_any() };
                for (name, data) in fs.downcast::<PyDict>()?.iter() {
                    if Self::g(slf, "files")?.call_method1("put", (&name, data))?.is_truthy()? {
                        w.getattr("runner")?.call_method1("invalidate", (&name,))?;
                    }
                }
                // ほかの参加者へも（送った人には送り返さない）
                let others = PyList::new_bound(py, Self::ready_peers(slf, Some(peer), false)?);
                Self::_broadcast(slf, &others, dict(py, vec![("t", "files".into_py(py)), ("files", fs.unbind())])?.as_any())?;
            }
            "call" => {
                let pm = p.getattr("msgs")?;
                let n0 = pm.len()?;
                let ok = as_player(py, &w, &p, || {
                    match w.call_method1("run_addon_call", (m.get_item("addon")?, m.get_item("path")?, get("args", PyList::empty_bound(py).into_any().unbind())?)) {
                        Ok(_) => Ok(true),
                        Err(e) if e.is_instance_of::<pyo3::exceptions::PyException>(py) => {
                            exc_log(py, &slog(py, "netserver")?, &e, "MOD の操作 %s.%s でエラー",
                                vec![get("addon", py.None())?.unbind(), get("path", py.None())?.unbind()])?;
                            w.call_method1("message", (format!("⚠ エラー: {}", e.value_bound(py).str()?),))?;
                            Ok(false)
                        }
                        Err(e) => Err(e),
                    }
                })?;
                if ok {
                    pm.del_item(pyo3::types::PySlice::new_bound(py, n0 as isize, isize::MAX, 1))?;  // 結果のメッセージは参加者の PC が先に出している
                }
            }
            "chat" => {
                w.call_method1("message", (format!("{}: {}", peer.getattr("name")?.str()?, get("text", "".into_py(py))?.str()?),))?;
            }
            _ => {}
        }
        Ok(())
    }
    fn _act(slf: &Bound<'_, Self>, p: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let w = Self::g(slf, "w")?;
        let a: String = m.get_item("a")?.extract()?;
        let np = imp(py, "numpy")?;
        let eye = np.call_method1("asarray", (m.call_method1("get", ("eye", p.call_method0("eye")?))?, "float64"))?;
        let look = np.call_method1("asarray", (m.call_method1("get", ("look", vec![0, 0, 1]))?, "float64"))?;
        let reach: f64 = p.getattr("defn")?.call_method1("get", ("reach", 8.0))?.extract()?;
        let hit = || w.call_method1("raycast", (&eye, &look, reach));
        match a.as_str() {
            "remove" => {
                let h = hit()?;
                if h.is_truthy()? && h.get_item("kind")?.eq("voxel")? {
                    w.call_method1("remove", (h,))?;
                }
            }
            "dig" => {
                let h = hit()?;
                if h.is_truthy()? && (h.get_item("kind")?.eq("terrain")? || h.get_item("kind")?.eq("water")?) {
                    w.call_method1("dig", (h,))?;
                }
            }
            "place" => {
                let h = if m.call_method1("get", ("air",))?.is_truthy()? { py.None().into_bound(py) } else { hit()? };
                let item = PyTuple::new_bound(py, m.get_item("item")?.iter()?.collect::<R<Vec<_>>>()?);
                w.call_method1("place", (h, item, m.call_method1("get", ("dir", 2))?.call_method0("__int__")?, &look))?;
            }
            "interact" => {
                let tg = m.call_method1("get", ("target",))?;
                if tg.is_truthy()? {
                    let c = w.getattr("constructs")?.call_method1("get", (tg.get_item("id")?,))?;
                    if c.is_truthy()? {
                        let i = c.getattr("index")?.call_method1("get", (PyTuple::new_bound(py, tg.get_item("cell")?.iter()?.collect::<R<Vec<_>>>()?),))?;
                        if !i.is_none() {
                            let iu: usize = i.extract()?;
                            let pos = super::construct::v3s(&c.getattr("pos")?)?;
                            let h = dict(py, vec![("kind", "voxel".into_py(py)), ("construct", c.clone().unbind()), ("index", iu.into_py(py)),
                                ("t", 0.0.into_py(py)), ("face", 2.into_py(py)), ("point", super::construct::a1(py, super::construct::mean8(&pos[8 * iu..8 * iu + 8])).unbind())])?;
                            w.call_method1("interact", (h,))?;
                        }
                    }
                    return Ok(());
                }
                let h = if m.call_method1("get", ("none",))?.is_truthy()? { py.None().into_bound(py) } else { hit()? };
                w.call_method1("interact", (h,))?;
            }
            "key" => {
                w.call_method1("key_pressed", (m.get_item("key")?,))?;
            }
            "import" => {
                let data = ns(py)?.call_method1("unpack_import", (m.get_item("data")?,))?;
                w.call_method("place_import", (data,), Some(&[("look_dir", &look)].into_py_dict_bound(py)))?;
            }
            "actuator" => {
                let c = w.getattr("constructs")?.call_method1("get", (m.get_item("id")?,))?;
                if c.is_truthy()? {
                    let i = c.getattr("index")?.call_method1("get", (PyTuple::new_bound(py, m.get_item("cell")?.iter()?.collect::<R<Vec<_>>>()?),))?;
                    if !i.is_none() {
                        let msg = super::actuator::act_execute_preset(&w, &c, i.extract()?)?;
                        w.call_method1("message", (msg,))?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    // --- 終わり ---
    fn close(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let peers: Vec<Bound<'_, PyAny>> = Self::g(slf, "peers")?.call_method0("values")?.iter()?.collect::<R<_>>()?;
        for p in &peers {
            p.getattr("conn")?.call_method1("send", (dict(py, vec![("t", "bye".into_py(py)), ("text", "ホストがワールドを閉じました".into_py(py))])?,))?;
        }
        imp(py, "time")?.call_method1("sleep", (0.05,))?;
        for p in &peers {
            p.getattr("conn")?.call_method0("close")?;
        }
        Self::g(slf, "peers")?.call_method0("clear")?;
        let w = Self::g(slf, "w")?;
        w.getattr("remote")?.call_method0("clear")?;
        Self::g(slf, "listener")?.call_method0("close")?;
        let ann = Self::g(slf, "announcer")?;
        if !ann.is_none() {
            ann.call_method0("close")?;
        }
        w.setattr("net", py.None())?;
        slog(py, "netserver")?.call_method1("info", ("LAN への公開を終了",))?;
        Ok(())
    }
}

impl Host {
    fn ready_peers<'py>(slf: &Bound<'py, Self>, except: Option<&Bound<'py, PyAny>>, alive: bool) -> R<Vec<Bound<'py, PyAny>>> {
        let mut out = vec![];
        for p in Self::g(slf, "peers")?.call_method0("values")?.iter()? {
            let p = p?;
            if let Some(x) = except {
                if p.is(x) {
                    continue;
                }
            }
            if p.getattr("ready")?.is_truthy()? && (!alive || p.getattr("conn")?.getattr("alive")?.is_truthy()?) {
                out.push(p);
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------- 参加者：参加の準備（別スレッド）

fn nc(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "main.netclient")
}
fn join_err(py: Python<'_>, text: String) -> R<PyErr> {
    Ok(PyErr::from_value_bound(nc(py)?.getattr("JoinError")?.call1((text,))?))
}

#[pyfunction]
pub fn nc_net_dir(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    div(&paths(py)?.getattr("SAVES")?, ".net")
}
#[pyfunction]
pub fn nc_mods_cache(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    div(&div(&paths(py)?.getattr("ROOT")?, "cache")?, "net_mods")
}
#[pyfunction]
pub fn nc_sha1(py: Python<'_>, f: &Bound<'_, PyAny>) -> R<String> {
    sha1_file(py, f)
}

/// 参加の準備（別スレッド）。status に今していること、done / error / result
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Joiner {
    #[pyo3(get, set)]
    host: PyObject,
    #[pyo3(get, set)]
    port: i64,
    #[pyo3(get, set)]
    name: PyObject,
    #[pyo3(get, set)]
    skin: PyObject,
    #[pyo3(get, set)]
    status: String,
    #[pyo3(get, set)]
    done: bool,
    #[pyo3(get, set)]
    error: PyObject,
    #[pyo3(get, set)]
    result: PyObject,
    #[pyo3(get, set)]
    conn: PyObject,
    #[pyo3(get, set)]
    cancelled: bool,
    #[pyo3(get, set)]
    orig_mods: PyObject,
}

impl Joiner {
    fn g<'py>(slf: &Bound<'py, Self>, k: &str) -> R<Bound<'py, PyAny>> {
        slf.as_any().getattr(k)
    }
    fn set_status(slf: &Bound<'_, Self>, s: String) {
        slf.borrow_mut().status = s;
    }
}

#[pymethods]
impl Joiner {
    #[new]
    #[pyo3(signature = (host, port, name, skin=None))]
    fn new(py: Python<'_>, host: &Bound<'_, PyAny>, port: &Bound<'_, PyAny>, name: PyObject, skin: Option<PyObject>) -> R<Self> {
        Ok(Joiner {
            status: format!("{}:{} につないでいます…", host.str()?, port.str()?),
            host: host.clone().unbind(), port: port.call_method0("__int__")?.extract()?, name, skin: skin.unwrap_or_else(|| py.None()),
            done: false, error: py.None(), result: py.None(), conn: py.None(), cancelled: false, orig_mods: paths(py)?.getattr("MODS")?.unbind(),
        })
    }
    /// 別スレッドで参加の準備を始める（Python の Joiner(...) から呼ばれる）
    fn _begin(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let kw = PyDict::new_bound(py);
        kw.set_item("target", slf.getattr("_run")?)?;
        kw.set_item("daemon", true)?;
        kw.set_item("name", "net-join")?;
        imp(py, "threading")?.getattr("Thread")?.call((), Some(&kw))?.call_method0("start")?;
        Ok(())
    }
    fn cancel(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow_mut().cancelled = true;
        let c = Self::g(slf, "conn")?;
        if !c.is_none() {
            c.call_method0("close")?;
        }
        Ok(())
    }
    #[pyo3(signature = (*types, timeout=60.0))]
    fn _wait<'py>(slf: &Bound<'py, Self>, types: Vec<String>, timeout: f64) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let time = imp(py, "time")?;
        let end = time.call_method0("time")?.extract::<f64>()? + timeout;
        let empty = imp(py, "queue")?.getattr("Empty")?;
        loop {
            if slf.borrow().cancelled {
                return Err(join_err(py, "やめました".into())?);
            }
            let inbox = Self::g(slf, "conn")?.getattr("inbox")?;
            let item = match inbox.call_method("get", (), Some(&[("timeout", 0.2)].into_py_dict_bound(py))) {
                Ok(i) => i,
                Err(e) if e.is_instance_bound(py, &empty) => {
                    if time.call_method0("time")?.extract::<f64>()? > end {
                        return Err(join_err(py, "ホストから返事がありません".into())?);
                    }
                    continue;
                }
                Err(e) => return Err(e),
            };
            let m = item.get_item(1)?;
            let t = m.call_method1("get", ("t",))?;
            let ts: String = if t.is_none() { String::new() } else { t.str()?.to_string() };
            if ts == "error" {
                return Err(join_err(py, m.call_method1("get", ("text", "参加できません"))?.str()?.to_string())?);
            }
            if ts == "_closed" || ts == "bye" {
                let tx = m.call_method1("get", ("text",))?;
                let why = if tx.is_truthy()? { tx } else { m.call_method1("get", ("why",))? };
                let why = if why.is_truthy()? { format!("（{}）", why.str()?) } else { String::new() };
                return Err(join_err(py, format!("ホストとのつながりが切れました{why}。ホストがワールドを閉じた・ホストの PC のセキュリティソフトが切った などが考えられます"))?);
            }
            if types.iter().any(|x| *x == ts) {
                return Ok(m);
            }
        }
    }
    fn _run(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let lg = slog(py, "netclient")?;
        let (host, port, name) = (Self::g(slf, "host")?, slf.borrow().port, Self::g(slf, "name")?);
        lg.call_method1("info", ("参加を試みます: %s:%s（名前 %s）", &host, port, &name))?;
        match slf.call_method0("_join") {
            Ok(_) => {
                slf.borrow_mut().done = true;
                lg.call_method1("info", ("参加の準備ができました: %s:%s", &host, port))?;
            }
            Err(e) if e.is_instance_of::<pyo3::exceptions::PyException>(py) => {
                let je = nc(py)?.getattr("JoinError")?;
                if !e.is_instance_bound(py, &je) {
                    exc_log(py, &lg, &e, "参加に失敗", vec![])?;
                }
                let v = e.value_bound(py);
                let s = v.str()?.to_string();
                let msg = if s.is_empty() { v.get_type().name()?.to_string() } else { s };
                slf.borrow_mut().error = msg.clone().into_py(py);
                let st = slf.borrow().status.clone();
                lg.call_method1("warning", ("参加できません（%s:%s、%s）: %s", &host, port, st, msg))?;
                let c = Self::g(slf, "conn")?;
                if !c.is_none() {
                    c.call_method0("close")?;
                }
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }
    fn _join(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let net = net(py)?;
        let (host, port) = (Self::g(slf, "host")?, slf.borrow().port);
        let hs = host.str()?.to_string();
        let conn = match net.call_method1("connect", (&host, port)) {
            Ok(c) => c,
            Err(e) if e.is_instance_of::<pyo3::exceptions::PyConnectionRefusedError>(py) => {
                return Err(join_err(py, format!("{hs}:{port} につなげません（相手が受け付けていません：{}）。ホストが「LAN に公開」しているか、IP・ポートが合っているかを確かめてください",
                    e.value_bound(py).str()?))?);
            }
            Err(e) if e.is_instance_of::<pyo3::exceptions::PyTimeoutError>(py) || e.is_instance_of::<pyo3::exceptions::PyOSError>(py) => {
                return Err(join_err(py, format!("{hs}:{port} につなげません（返事がありません：{}）。ホストの PC のファイアウォールで止められていることが多いです（ホストの PC で「LANで公開できるようにする.bat」を実行）",
                    e.value_bound(py).str()?))?);
            }
            Err(e) => return Err(e),
        };
        slf.borrow_mut().conn = conn.clone().unbind();
        conn.call_method1("send", (dict(py, vec![("t", "hello".into_py(py)), ("name", Self::g(slf, "name")?.unbind()), ("proto", net.getattr("PROTO")?.unbind())])?,))?;
        Self::set_status(slf, "ホストの返事を待っています…".into());
        let w = Self::_wait(slf, vec!["welcome".into()], 60.0)?;
        slf.as_any().setattr("welcome", &w)?;
        // MOD：足りない物だけ受け取る
        let need = PyList::empty_bound(py);
        let mut total: f64 = 0.0;
        let cache = nc(py)?.call_method0("mods_cache")?;
        let orig = Self::g(slf, "orig_mods")?;
        let builtins = imp(py, "builtins")?;
        for (mid, files) in w.get_item("manifest")?.downcast::<PyDict>()?.iter() {
            let dst = div(&cache, &mid)?;
            let mut want = std::collections::HashSet::new();
            for f in files.iter()? {
                want.insert(f?.get_item(0)?.str()?.to_string());
            }
            if dst.call_method0("is_dir")?.is_truthy()? {
                let all = builtins.getattr("sorted")?.call((dst.call_method1("rglob", ("*",))?,), Some(&[("reverse", true)].into_py_dict_bound(py)))?;
                for f in all.iter()? {
                    let f = f?;
                    let rel = f.call_method1("relative_to", (&dst,))?.call_method0("as_posix")?.str()?.to_string();
                    if f.call_method0("is_file")?.is_truthy()? && !want.contains(&rel) {
                        f.call_method0("unlink")?;
                    }
                }
            }
            for f in files.iter()? {
                let f = f?;
                let (rel, size, sha) = (f.get_item(0)?, f.get_item(1)?.extract::<i64>()?, f.get_item(2)?.str()?.to_string());
                let t = div(&dst, &rel)?;
                let ok = |p: &Bound<'_, PyAny>| -> R<bool> {
                    Ok(p.call_method0("is_file")?.is_truthy()? && p.call_method0("stat")?.getattr("st_size")?.extract::<i64>()? == size && sha1_file(py, p)? == sha)
                };
                if ok(&t)? {
                    continue;
                }
                let mine = div(&div(&orig, &mid)?, &rel)?;
                if ok(&mine)? {
                    t.getattr("parent")?.call_method("mkdir", (), Some(&[("parents", true), ("exist_ok", true)].into_py_dict_bound(py)))?;
                    imp(py, "shutil")?.call_method1("copyfile", (&mine, &t))?;
                    continue;
                }
                need.append(PyList::new_bound(py, [mid.clone(), rel]))?;
                total += size as f64;
            }
        }
        if !need.is_empty() {
            let mut got: f64 = 0.0;
            Self::set_status(slf, format!("MOD を受け取っています… 0 / {:.1} MB", total / 1e6));
            conn.call_method1("send", (dict(py, vec![("t", "get_files".into_py(py)), ("files", need.into_any().unbind())])?,))?;
            let parts = PyDict::new_bound(py);
            loop {
                let m = Self::_wait(slf, vec!["file".into(), "files_done".into()], 120.0)?;
                if m.get_item("t")?.eq("files_done")? {
                    break;
                }
                let (md, rel) = (m.get_item("mod")?, m.get_item("rel")?);
                let key = PyTuple::new_bound(py, [&md, &rel]);
                let tmp = div(&div(&cache, &md)?, format!("{}.part", rel.str()?))?;
                tmp.getattr("parent")?.call_method("mkdir", (), Some(&[("parents", true), ("exist_ok", true)].into_py_dict_bound(py)))?;
                if !parts.contains(&key)? {
                    parts.set_item(&key, builtins.call_method1("open", (&tmp, "wb"))?)?;
                }
                let data = m.get_item("data")?;
                parts.get_item(&key)?.unwrap().call_method1("write", (&data,))?;
                got += data.len()? as f64;
                Self::set_status(slf, format!("MOD を受け取っています… {:.1} / {:.1} MB", got / 1e6, total / 1e6));
                if m.call_method1("get", ("last",))?.is_truthy()? {
                    parts.call_method1("pop", (&key,))?.call_method0("close")?;
                    let fin = div(&div(&cache, &md)?, &rel)?;
                    if m.call_method1("get", ("missing",))?.is_truthy()? {
                        tmp.call_method0("unlink")?;
                        return Err(join_err(py, format!("ホストの MOD のファイル {}/{} を受け取れません", md.str()?, rel.str()?))?);
                    }
                    tmp.call_method1("replace", (fin,))?;
                }
            }
        }
        Self::set_status(slf, "ワールドを受け取っています…".into());
        conn.call_method1("send", (dict(py, vec![("t", "ready".into_py(py)), ("def", Self::g(slf, "skin")?.unbind())])?,))?;
        let m = Self::_wait(slf, vec!["world".into()], 120.0)?;
        let nd = nc(py)?.call_method0("net_dir")?;
        let wd = div(&nd, "world")?;
        if wd.call_method0("exists")?.is_truthy()? {
            imp(py, "shutil")?.call_method("rmtree", (&wd,), Some(&[("ignore_errors", true)].into_py_dict_bound(py)))?;
        }
        div(&wd, "scripts")?.call_method("mkdir", (), Some(&[("parents", true), ("exist_ok", true)].into_py_dict_bound(py)))?;
        let enc = [("encoding", "utf-8")].into_py_dict_bound(py);
        let jkw = [("ensure_ascii", false)].into_py_dict_bound(py);
        for (fname, data) in m.get_item("files")?.downcast::<PyDict>()?.iter() {
            let text = imp(py, "json")?.call_method("dumps", (data,), Some(&jkw))?;
            div(&wd, fname)?.call_method("write_text", (text,), Some(&enc))?;
        }
        let sc = m.call_method1("get", ("scripts",))?;
        if sc.is_truthy()? {
            for (name, data) in sc.downcast::<PyDict>()?.iter() {
                let n = name.str()?.to_string();
                if !n.contains('/') && !n.contains('\\') && !n.contains("..") {
                    div(&div(&wd, "scripts")?, &name)?.call_method1("write_bytes", (data,))?;
                }
            }
        }
        div(&nd, "exports")?.call_method("mkdir", (), Some(&[("parents", true), ("exist_ok", true)].into_py_dict_bound(py)))?;
        Self::set_status(slf, "ワールドを読み込んでいます…".into());
        let res = dict(py, vec![("conn", conn.unbind()), ("pid", w.get_item("pid")?.unbind()), ("name", w.get_item("name")?.unbind()),
            ("world", w.get_item("world")?.unbind()), ("dir", wd.unbind()), ("host", w.call_method1("get", ("host", ""))?.unbind()),
            ("mods", w.get_item("mods")?.unbind())])?;
        slf.borrow_mut().result = res.into_any().unbind();
        Ok(())
    }
}

// ---------------------------------------------------------------- 参加者：遊んでいる間

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Client {
    #[pyo3(get, set)]
    conn: PyObject,
    #[pyo3(get, set)]
    pid: PyObject,
    #[pyo3(get, set)]
    name: PyObject,
    #[pyo3(get, set)]
    host_name: PyObject,
    #[pyo3(get, set)]
    w: PyObject,
    #[pyo3(get, set)]
    cfg: PyObject,
    #[pyo3(get, set)]
    others: PyObject,
    #[pyo3(get, set)]
    files: PyObject,
    #[pyo3(get, set)]
    closed: PyObject,
    #[pyo3(get, set)]
    look: PyObject,
    #[pyo3(get, set)]
    edit: PyObject,
    #[pyo3(get, set)]
    frozen_until: PyObject,
    #[pyo3(get, set, name = "_t")]
    t: PyObject,
    #[pyo3(get, set, name = "_hotbar_key")]
    hotbar_key: PyObject,
    #[pyo3(get, set, name = "_last_s")]
    last_s: f64,
}

impl Client {
    fn g<'py>(slf: &Bound<'py, Self>, k: &str) -> R<Bound<'py, PyAny>> {
        slf.as_any().getattr(k)
    }
}

/// 参加者のワールドの操作をホストへ回すための関数を作る
fn redirect<'py>(py: Python<'py>, me: &Bound<'py, Client>, kind: &'static str) -> R<Bound<'py, pyo3::types::PyCFunction>> {
    let me: Py<Client> = me.clone().unbind();
    pyo3::types::PyCFunction::new_closure_bound(py, None, None, move |a: &Bound<'_, PyTuple>, k: Option<&Bound<'_, PyDict>>| -> R<PyObject> {
        let py = a.py();
        let me = me.bind(py);
        let arg = |i: usize, name: &str| -> R<Option<Bound<'_, PyAny>>> {
            if a.len() > i {
                return Ok(Some(a.get_item(i)?));
            }
            Ok(match k { Some(k) => k.get_item(name)?, None => None })
        };
        let kw = PyDict::new_bound(py);
        match kind {
            "remove" | "dig" => {
                me.call_method1("act", (kind,))?;
                Ok(true.into_py(py))
            }
            "place" => {
                let hit = arg(0, "hit")?.unwrap_or_else(|| py.None().into_bound(py));
                kw.set_item("item", PyList::new_bound(py, arg(1, "item")?.unwrap().iter()?.collect::<R<Vec<_>>>()?))?;
                kw.set_item("dir", arg(2, "dir_index")?.unwrap().call_method0("__int__")?)?;
                kw.set_item("air", hit.is_none())?;
                kw.set_item("look", super::construct::v3(&arg(3, "look_dir")?.unwrap())?.to_vec())?;
                me.call_method("act", ("place",), Some(&kw))?;
                Ok(true.into_py(py))
            }
            "interact" => {
                let hit = arg(0, "hit")?.unwrap_or_else(|| py.None().into_bound(py));
                if hit.is_truthy()? && hit.call_method1("get", ("kind",))?.eq("voxel")? {
                    let c = hit.get_item("construct")?;
                    let cell = c.getattr("cells")?.get_item(hit.get_item("index")?)?;
                    kw.set_item("target", dict(py, vec![("id", c.getattr("id")?.unbind()),
                        ("cell", PyList::new_bound(py, cell.iter()?.collect::<R<Vec<_>>>()?).into_any().unbind())])?)?;
                } else {
                    kw.set_item("none", hit.is_none())?;
                }
                me.call_method("act", ("interact",), Some(&kw))?;
                Ok(py.None())
            }
            "key_pressed" => {
                kw.set_item("key", arg(0, "key")?.unwrap())?;
                me.call_method("act", ("key",), Some(&kw))?;
                Ok(py.None())
            }
            _ => {
                // place_import(data, position=None, look_dir=(0, 0, 1))
                let data = arg(0, "data")?.unwrap();
                let look = match arg(2, "look_dir")? { Some(l) => super::construct::v3(&l)?.to_vec(), None => vec![0.0, 0.0, 1.0] };
                kw.set_item("data", ns(py)?.call_method1("pack_import", (&data,))?)?;
                kw.set_item("look", look)?;
                me.call_method("act", ("import",), Some(&kw))?;
                let nm = data.call_method1("get", ("name", ""))?;
                me.getattr("w")?.call_method1("message", (format!("読み込み: {}（ホストに送りました）", nm.str()?),))?;
                Ok(py.None())
            }
        }
    })
}

#[pymethods]
impl Client {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const role: &'static str = "client";

    #[new]
    fn new(py: Python<'_>, info: &Bound<'_, PyAny>, world: &Bound<'_, PyAny>, cfg: PyObject) -> R<Self> {
        let files = ns(py)?.getattr("FileSync")?.call1((div(&world.getattr("dir")?, "scripts")?,))?;
        files.call_method0("mark_all")?;
        files.call_method0("changes")?;
        let t = PyDict::new_bound(py);
        t.set_item("me", 0.0)?;
        t.set_item("file", 0.0)?;
        Ok(Client {
            conn: info.get_item("conn")?.unbind(), pid: info.get_item("pid")?.unbind(), name: info.get_item("name")?.unbind(),
            host_name: info.call_method1("get", ("host", ""))?.unbind(), w: world.clone().unbind(), cfg,
            others: PyDict::new_bound(py).into_any().unbind(), files: files.unbind(), closed: py.None(),
            look: super::construct::a1(py, [0.0, 0.0, 1.0]).unbind(), edit: py.None(), frozen_until: PyDict::new_bound(py).into_any().unbind(),
            t: t.into_any().unbind(), hotbar_key: py.None(), last_s: mono(py)?,
        })
    }
    /// 作ったあとの準備（ワールドへの登録・操作をホストへ回す）。Python の Client(...) から呼ばれる
    fn _begin(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let w = Self::g(slf, "w")?;
        w.setattr("net", slf)?;
        w.setattr("exports_dir", div(&nc(py)?.call_method0("net_dir")?, "exports")?)?;
        w.getattr("local_player")?.setattr("name", Self::g(slf, "name")?)?;
        Self::_patch_world(slf)?;
        slog(py, "netclient")?.call_method1("info", ("参加中: %s（ホスト %s）", w.getattr("name")?, Self::g(slf, "host_name")?))?;
        Ok(())
    }
    fn _patch_world(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let w = Self::g(slf, "w")?;
        for k in ["remove", "dig", "place", "interact", "key_pressed", "place_import"] {
            w.setattr(k, redirect(py, slf, k)?)?;
        }
        Ok(())
    }
    #[pyo3(signature = (a, **kw))]
    fn act(slf: &Bound<'_, Self>, a: &str, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let py = slf.py();
        let p = Self::g(slf, "w")?.getattr("player")?;
        let look = match kw.map(|k| k.call_method1("pop", ("look", py.None()))).transpose()? {
            Some(l) if !l.is_none() => l,
            _ => Self::g(slf, "look")?,
        };
        let msg = dict(py, vec![("t", "act".into_py(py)), ("a", a.into_py(py)), ("eye", p.call_method0("eye")?.call_method0("tolist")?.unbind()),
            ("look", super::construct::v3(&look)?.to_vec().into_py(py))])?;
        if let Some(k) = kw {
            msg.update(k.as_mapping())?;
        }
        Self::send(slf, msg.as_any())
    }
    /// アクチュエーターの画面の「実行」：設定を先に送ってから、ホストで実行する
    fn actuator_run(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, i: usize) -> R<()> {
        let py = slf.py();
        Self::_flush_edit(slf, true)?;
        let cell = c.getattr("cells")?.get_item(i)?;
        Self::send(slf, dict(py, vec![("t", "act".into_py(py)), ("a", "actuator".into_py(py)), ("id", c.getattr("id")?.unbind()),
            ("cell", PyList::new_bound(py, cell.iter()?.collect::<R<Vec<_>>>()?).into_any().unbind())])?.as_any())
    }
    fn send(slf: &Bound<'_, Self>, msg: &Bound<'_, PyAny>) -> R<()> {
        Self::g(slf, "conn")?.call_method1("send", (msg,))?;
        Ok(())
    }
    fn can_save(&self) -> bool {
        false
    }
    fn save_extra<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        PyDict::new_bound(py)
    }
    fn addresses(&self) -> Vec<String> {
        vec![]
    }

    // --- 毎フレーム ---
    fn frame(slf: &Bound<'_, Self>, game: &Bound<'_, PyAny>, dt: f64) -> R<()> {
        let py = slf.py();
        let w = Self::g(slf, "w")?;
        Self::_process(slf, game)?;
        if Self::g(slf, "closed")?.is_truthy()? {
            return Ok(());
        }
        ns(py)?.call_method1("advance", (&w, dt))?;
        let tm: f64 = w.getattr("time")?.extract()?;
        w.setattr("time", tm + dt)?;
        for a in w.getattr("addons")?.iter()? {
            let a = a?;
            if let Err(e) = a.call_method1("net_client_tick", (&w, dt)) {
                if !e.is_instance_of::<pyo3::exceptions::PyException>(py) {
                    return Err(e);
                }
                exc_log(py, &slog(py, "netclient")?, &e, "MOD '%s' の net_client_tick でエラー", vec![a.getattr("name")?.unbind()])?;
            }
        }
        let now = mono(py)?;
        Self::_edit_sync(slf, game, now)?;
        let t = Self::g(slf, "_t")?;
        if now - t.get_item("me")?.extract::<f64>()? >= 1.0 / ME_HZ {
            t.set_item("me", now)?;
            Self::_send_me(slf)?;
        }
        if now - t.get_item("file")?.extract::<f64>()? >= 1.0 / FILE_HZ {
            t.set_item("file", now)?;
            let ch = Self::g(slf, "files")?.call_method0("changes")?;
            if ch.is_truthy()? {
                Self::send(slf, dict(py, vec![("t", "files".into_py(py)), ("files", ch.unbind())])?.as_any())?;
            }
        }
        Ok(())
    }
    /// 自分の体を動かす（物理の刻みごと）
    fn step(slf: &Bound<'_, Self>, dt: f64, r#move: &Bound<'_, PyAny>, jump: &Bound<'_, PyAny>, thrust: &Bound<'_, PyAny>) -> R<()> {
        let w = Self::g(slf, "w")?;
        w.getattr("player")?.call_method1("update", (&w, dt, r#move, jump, thrust))?;
        for c in w.getattr("constructs")?.call_method0("values")?.iter()? {         // 体重は手元では使わない（ホストで伝える）
            let f = c?.getattr("force")?;
            if super::construct::fv(&f)?.iter().any(|x| *x != 0.0) && !super::construct::with_rw(&f, |s| s.fill(0.0))? {
                f.set_item(pyo3::types::PySlice::full_bound(slf.py()), 0)?;
            }
        }
        Ok(())
    }
    fn _send_me(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let w = Self::g(slf, "w")?;
        let p = w.getattr("player")?;
        let np = imp(py, "numpy")?;
        let tl = |o: Bound<'_, PyAny>| -> R<PyObject> { Ok(np.call_method1("asarray", (o, "float64"))?.call_method0("tolist")?.unbind()) };
        let st = p.getattr("stand")?;
        let fd = p.getattr("face_dir")?;
        let keys = imp(py, "builtins")?.call_method1("sorted", (w.getattr("input_keys")?,))?;
        let msg = dict(py, vec![("t", "me".into_py(py)), ("pos", p.getattr("pos")?.call_method0("tolist")?.unbind()),
            ("vel", p.getattr("vel")?.call_method0("tolist")?.unbind()), ("up", p.getattr("up")?.call_method0("tolist")?.unbind()),
            ("gv", tl(p.getattr("ground_vel")?)?), ("og", p.getattr("on_ground")?.is_truthy()?.into_py(py)), ("jp", p.getattr("jetpack")?.is_truthy()?.into_py(py)),
            ("fl", p.getattr("flying")?.is_truthy()?.into_py(py)), ("thrust", tl(p.getattr("thrust")?)?),
            ("wt", p.getattr("in_water")?.call_method0("__float__")?.unbind()), ("keys", keys.unbind()), ("fwd", tl(p.getattr("look_fwd")?)?),
            ("pitch", p.getattr("pitch")?.call_method0("__float__")?.unbind()), ("def", p.getattr("defn")?.call_method1("get", ("id",))?.unbind()),
            ("stand", if st.is_truthy()? {
                PyList::new_bound(py, [st.get_item(0)?, PyList::new_bound(py, st.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any()]).into_any().unbind()
            } else { py.None() }),
            ("emote", p.getattr("emote")?.unbind()), ("face", if fd.is_none() { py.None() } else { tl(fd)? })])?;
        let hbo = p.getattr("hotbar")?;
        let hb: PyObject = if hbo.is_truthy()? {
            let l = PyList::empty_bound(py);
            for x in hbo.iter()? {
                let x = x?;
                l.append(if x.is_truthy()? { PyList::new_bound(py, x.iter()?.collect::<R<Vec<_>>>()?).into_any() } else { py.None().into_bound(py) })?;
            }
            ns(py)?.call_method1("jsonable", (l,))?.unbind()
        } else { py.None() };
        let key = imp(py, "json")?.call_method1("dumps", (hb.bind(py),))?;
        if !key.eq(Self::g(slf, "_hotbar_key")?)? {
            slf.borrow_mut().hotbar_key = key.unbind();
            msg.set_item("hotbar", hb)?;
        }
        Self::send(slf, msg.as_any())
    }
    /// 機器・プログラムの画面で変えた分をホストへ送る（開いている間は2回/秒、閉じたときにもう1回）
    fn _edit_sync(slf: &Bound<'_, Self>, game: &Bound<'_, PyAny>, now: f64) -> R<()> {
        let py = slf.py();
        let ed = super::construct::opt(game, "editor")?;
        let c = match &ed { Some(e) => super::construct::opt(e, "c")?, None => None };
        let fu = Self::g(slf, "frozen_until")?;
        match c {
            Some(c) if super::construct::opt(&c, "devs")?.is_some() => {
                let edit = Self::g(slf, "edit")?;
                if edit.is_none() || !edit.get_item(0)?.is(&c) {
                    Self::_flush_edit(slf, true)?;
                    let e = PyList::new_bound(py, [c.clone().unbind(), ns(py)?.call_method1("dev_base", (&c,))?.unbind(), (now + 1.0 / EDIT_HZ).into_py(py)]);
                    slf.borrow_mut().edit = e.into_any().unbind();
                } else if now >= edit.get_item(2)?.extract::<f64>()? {
                    Self::_flush_edit(slf, false)?;
                    edit.set_item(2, now + 1.0 / EDIT_HZ)?;
                }
                fu.set_item(c.getattr("id")?, now + 1.5)?;
            }
            _ => {
                if !Self::g(slf, "edit")?.is_none() {
                    Self::_flush_edit(slf, true)?;
                    slf.borrow_mut().edit = py.None();
                }
            }
        }
        Ok(())
    }
    #[pyo3(signature = (force=false))]
    fn _flush_edit(slf: &Bound<'_, Self>, force: bool) -> R<()> {
        let py = slf.py();
        let _ = force;
        let edit = Self::g(slf, "edit")?;
        if edit.is_none() {
            return Ok(());
        }
        let (c, base) = (edit.get_item(0)?, edit.get_item(1)?);
        let patches = ns(py)?.call_method1("dev_patch", (&c, &base))?;
        if patches.is_truthy()? {
            Self::send(slf, dict(py, vec![("t", "dev".into_py(py)), ("id", c.getattr("id")?.unbind()), ("patches", patches.unbind())])?.as_any())?;
            edit.set_item(1, ns(py)?.call_method1("dev_base", (&c,))?)?;
        }
        Self::g(slf, "frozen_until")?.set_item(c.getattr("id")?, mono(py)? + 1.5)?;
        Ok(())
    }

    // --- 受け取り ---
    fn _process(slf: &Bound<'_, Self>, game: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let _ = game;
        let w = Self::g(slf, "w")?;
        let inbox = Self::g(slf, "conn")?.getattr("inbox")?;
        let empty = imp(py, "queue")?.getattr("Empty")?;
        let nsm = ns(py)?;
        for _ in 0..5000 {
            let item = match inbox.call_method0("get_nowait") {
                Ok(i) => i,
                Err(e) if e.is_instance_bound(py, &empty) => break,
                Err(e) => return Err(e),
            };
            let m = item.get_item(1)?;
            let t = m.call_method1("get", ("t",))?;
            let ts: String = if t.is_none() { String::new() } else { t.str()?.to_string() };
            let r = (|| -> R<bool> {
                match ts.as_str() {
                    "s" => Self::_state(slf, &m)?,
                    "m" => {
                        let now = mono(py)?;
                        let fu = Self::g(slf, "frozen_until")?;
                        for e in m.get_item("c")?.iter()? {
                            let e = e?;
                            let c = w.getattr("constructs")?.call_method1("get", (e.get_item("id")?,))?;
                            if !c.is_none() {
                                let f: f64 = fu.call_method1("get", (c.getattr("id")?, 0))?.extract()?;
                                nsm.call_method("apply_meta", (&c, &e), Some(&[("devices", f < now)].into_py_dict_bound(py)))?;
                            }
                        }
                    }
                    "a" => {
                        for (name, st) in m.get_item("s")?.downcast::<PyDict>()?.iter() {
                            let a = w.call_method1("addon", (name,))?;
                            if !a.is_none() {
                                a.call_method1("net_apply", (&w, st))?;
                            }
                        }
                    }
                    "you" => Self::_you(slf, &m)?,
                    "files" => {
                        for (name, data) in m.get_item("files")?.downcast::<PyDict>()?.iter() {
                            Self::g(slf, "files")?.call_method1("put", (name, data))?;
                        }
                    }
                    "note" => {
                        w.call_method1("message", (m.call_method1("get", ("text", ""))?,))?;
                    }
                    "bye" | "_closed" => {
                        let tx = m.call_method1("get", ("text",))?;
                        let why = if tx.is_truthy()? { tx } else { m.call_method1("get", ("why",))? };
                        let why: PyObject = if why.is_truthy()? { why.unbind() } else { "ホストとのつながりが切れました".into_py(py) };
                        slf.borrow_mut().closed = why;
                        return Ok(true);
                    }
                    _ => {}
                }
                Ok(false)
            })();
            match r {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(e) if e.is_instance_of::<pyo3::exceptions::PyException>(py) => {
                    exc_log(py, &slog(py, "netclient")?, &e, "ホストからのメッセージ %s の処理でエラー", vec![t.unbind()])?;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn _state(slf: &Bound<'_, Self>, m: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let w = Self::g(slf, "w")?;
        let nsm = ns(py)?;
        w.setattr("time", m.get_item("time")?.call_method0("__float__")?)?;
        let dt: f64 = m.call_method1("get", ("dt", 0.05))?.extract()?;
        for e in m.get_item("c")?.iter()? {
            nsm.call_method1("apply_construct", (&w, e?, dt))?;
        }
        let cons = w.getattr("constructs")?;
        for cid in m.call_method1("get", ("gone", PyList::empty_bound(py)))?.iter()? {
            cons.call_method1("pop", (cid?, py.None()))?;
        }
        let pls = w.getattr("planets")?;
        for cr in m.call_method1("get", ("craters", PyList::empty_bound(py)))?.iter()? {
            let cr = cr?;
            let k: usize = cr.get_item("p")?.extract()?;
            if k < pls.len()? {
                let pl = pls.get_item(k)?;
                let from: usize = cr.get_item("from")?.extract()?;
                let old = pl.getattr("craters")?;
                let nl = PyList::empty_bound(py);
                for (q, x) in old.iter()?.enumerate() {
                    if q < from {
                        nl.append(x?)?;
                    }
                }
                for x in cr.get_item("list")?.iter()? {
                    nl.append(PyList::new_bound(py, x?.iter()?.collect::<R<Vec<_>>>()?))?;
                }
                pl.setattr("craters", nl)?;
                let v: i64 = pl.getattr("version")?.extract()?;
                pl.setattr("version", v + 1)?;
            }
        }
        let others = Self::g(slf, "others")?;
        let me = Self::g(slf, "pid")?;
        let seen = pyo3::types::PySet::empty_bound(py)?;
        for d in m.call_method1("get", ("pl", PyList::empty_bound(py)))?.iter()? {
            let d = d?;
            let pid = d.get_item("pid")?;
            if pid.eq(&me)? {
                continue;
            }
            seen.add(&pid)?;
            let mut p = others.call_method1("get", (&pid,))?;
            if p.is_none() {
                p = w.call_method1("make_player", (d.get_item("def")?, d.get_item("pos")?))?;
                others.set_item(&pid, &p)?;
            }
            nsm.call_method1("apply_player_public", (&w, &p, &d))?;
        }
        let gone: Vec<Bound<'_, PyAny>> = others.iter()?.filter_map(|k| k.ok()).filter(|k| !seen.contains(k).unwrap_or(true)).collect();
        for k in gone {
            others.del_item(k)?;
        }
        Ok(())
    }
    fn _you(slf: &Bound<'_, Self>, m: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let w = Self::g(slf, "w")?;
        let p = w.getattr("player")?;
        let inv = m.call_method1("get", ("inventory",))?;
        p.setattr("inventory", py.get_type_bound::<PyDict>().call1((if inv.is_truthy()? { inv } else { PyDict::new_bound(py).into_any() },))?)?;
        let s = m.call_method1("get", ("seated",))?;
        p.setattr("seated", if s.is_truthy()? {
            PyTuple::new_bound(py, [s.get_item(0)?, PyTuple::new_bound(py, s.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any()]).into_any()
        } else { py.None().into_bound(py) })?;
        for k in ["starter_given", "credits_given", "out_of_fuel"] {
            p.setattr(k, m.call_method1("get", (k,))?.is_truthy()?)?;
        }
        for text in m.call_method1("get", ("msgs", PyList::empty_bound(py)))?.iter()? {
            w.call_method1("message", (text?,))?;
        }
        Ok(())
    }
    fn close(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let r = (|| -> R<()> {
            Self::send(slf, dict(py, vec![("t", "bye".into_py(py))])?.as_any())?;
            imp(py, "time")?.call_method1("sleep", (0.05,))?;
            Ok(())
        })();
        if let Err(e) = r {
            if !e.is_instance_of::<pyo3::exceptions::PyException>(py) {
                return Err(e);
            }
        }
        Self::g(slf, "conn")?.call_method0("close")?;
        Self::g(slf, "w")?.setattr("net", py.None())?;
        slog(py, "netclient")?.call_method1("info", ("参加を終了",))?;
        Ok(())
    }
}
