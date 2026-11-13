//! Rust の画面の画面（一時停止メニュー・インベントリ）（main/client_rs/screens.py）。並べ方・動きは今までの画面（ursina）と同じ。

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};

use super::imp;
use super::ui::Screen;

type R<T> = PyResult<T>;
type C4 = (f64, f64, f64, f64);
pub const GREY: C4 = (0.75, 0.75, 0.75, 1.0);

/// functools.partial(f, *args, **kw)
pub fn partial<'py>(f: &Bound<'py, PyAny>, args: Vec<PyObject>, kw: Option<&Bound<'py, PyDict>>) -> R<PyObject> {
    let py = f.py();
    let mut v: Vec<PyObject> = vec![f.clone().unbind()];
    v.extend(args);
    Ok(imp(py, "functools")?.getattr("partial")?.call(PyTuple::new_bound(py, v), kw)?.unbind())
}
/// self.name に args を先に渡した関数
pub fn me(s: &Bound<'_, PyAny>, name: &str, args: Vec<PyObject>) -> R<PyObject> {
    partial(&s.getattr(name)?, args, None)
}
fn kw1<'py>(py: Python<'py>, k: &str, v: impl IntoPy<PyObject>) -> R<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item(k, v.into_py(py))?;
    Ok(d)
}
/// self.text(...)（戻り値は文字の部品）
#[allow(clippy::too_many_arguments)]
pub fn txt<'py>(s: &Bound<'py, PyAny>, t: impl IntoPy<PyObject>, x: f64, y: f64, size: f64, color: C4, anchor: (f64, f64)) -> R<Bound<'py, PyAny>> {
    let py = s.py();
    s.call_method("text", (t.into_py(py), x, y, size, color), Some(&kw1(py, "anchor", anchor)?))?.get_item(0)
}
/// self.button(...)（戻り値は widgets に入った辞書）
#[allow(clippy::too_many_arguments)]
pub fn btn<'py>(s: &Bound<'py, PyAny>, label: &str, x: f64, y: f64, w: f64, h: f64, click: PyObject, color: C4, size: f64,
                right: Option<PyObject>, info: Option<PyObject>) -> R<Bound<'py, PyAny>> {
    let py = s.py();
    let k = PyDict::new_bound(py);
    k.set_item("color", color)?;
    k.set_item("size", size)?;
    k.set_item("on_right", right.unwrap_or_else(|| py.None()))?;
    k.set_item("info", info.unwrap_or_else(|| py.None()))?;
    s.call_method("button", (label, x, y, w, h, click), Some(&k))?;
    s.getattr("widgets")?.get_item(-1)
}
fn pyint(o: &Bound<'_, PyAny>) -> R<i64> {
    o.py().import_bound("builtins")?.getattr("int")?.call1((o,))?.extract()
}
fn save_cfg(v: &Bound<'_, PyAny>) {
    let py = v.py();
    let _ = (|| -> R<()> {
        imp(py, "main.client.menu")?.getattr("save_config")?.call1((v.getattr("cfg")?,))?;
        Ok(())
    })();
}
fn audio<'py>(v: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let py = v.py();
    let d = PyDict::new_bound(py);
    d.set_item("bgm", 60)?;
    d.set_item("sfx", 80)?;
    v.getattr("cfg")?.call_method1("setdefault", ("audio", d))
}

/// Esc：一時停止メニュー
#[pyclass(module = "alpedia_core", extends = Screen, subclass)]
pub struct Pause {
    #[pyo3(get, set)]
    v: PyObject,
    #[pyo3(get, set)]
    armed: bool,
}

#[pymethods]
impl Pause {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> (Self, Screen) {
        (Pause { v: py.None(), armed: false }, Screen::blank(py))
    }
    fn __init__(slf: &Bound<'_, Self>, ui: &Bound<'_, PyAny>, viewer: PyObject) -> R<()> {
        Screen::init_base(slf.downcast::<Screen>()?, ui)?;
        slf.borrow_mut().v = viewer;
        Ok(())
    }
    fn build(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        s.call_method0("clear")?;
        btn(s, "", 0.0, 0.0, 3.0, 2.0, py.None(), (0.0, 0.0, 0.0, 0.45), 0.022, None, None)?;     // 後ろを暗く（押しても何もしない）
        let w = v.getattr("w")?;
        let net = w.getattr("net")?;
        let has_net = !net.is_none();
        let client = has_net && net.getattr("role")?.eq("client")?;
        let g = v.getattr("game")?;
        let leave = g.getattr("_leave")?;
        let mut items: Vec<(&str, PyObject)> = vec![("続ける（Esc）", s.getattr("close")?.unbind())];
        if client {        // マルチプレイに参加中（保存はホストがする）
            items.push(("ワールドから抜けてメニューへ", partial(&leave, vec!["menu".into_py(py)], None)?));
            items.push(("ワールドから抜けて終了", partial(&leave, vec!["quit".into_py(py)], None)?));
        } else {
            items.push(("保存する（F5）", s.getattr("save")?.unbind()));
            items.push(("保存してメニューへ", partial(&leave, vec!["menu".into_py(py)], None)?));
            items.push(("保存せずにメニューへ", partial(&leave, vec!["menu".into_py(py)], Some(&kw1(py, "save", false)?))?));
            items.push(("保存して終了", partial(&leave, vec!["quit".into_py(py)], None)?));
            if !has_net {
                items.push(("LAN に公開（マルチプレイ）", s.getattr("host")?.unbind()));
            }
        }
        let top = 0.3;
        let n_right = 8usize;
        let bottom = top - items.len().max(n_right) as f64 * 0.08 - 0.05 - if has_net && !client { 0.05 } else { 0.0 };
        s.call_method1("panel", (-0.62, top + 0.1, 0.62, bottom))?;
        txt(s, "一時停止", 0.0, top + 0.07, 0.034, (1.0, 1.0, 1.0, 1.0), (0.5, 0.0))?;
        let (xl, xr) = (-0.31, 0.31);
        for (k, (label, f)) in items.iter().enumerate() {
            btn(s, label, xl, top - 0.04 - k as f64 * 0.08, 0.5, 0.06, f.clone_ref(py),
                if k == 0 { (0.2, 0.45, 0.3, 1.0) } else { (0.22, 0.25, 0.32, 1.0) }, 0.023, None, None)?;
        }
        if has_net && !client {
            let a = net.call_method0("addresses")?.get_item(pyo3::types::PySlice::new_bound(py, 0, 2, 1))?;
            let joined = PyString::new_bound(py, "  ").call_method1("join", (a,))?;
            txt(s, format!("LAN に公開中: {}", joined.str()?), xl, top - 0.04 - items.len() as f64 * 0.08 + 0.01, 0.02,
                (1.0, 1.0, 1.0, 1.0), (0.5, 0.0))?;
        }
        // 右の列：キー設定・読み込み・出力・音量・見た目・リスポーン（今までの画面の一時停止メニューと同じ）
        let au = audio(&v)?;
        let blue = (0.22, 0.3, 0.42, 1.0);
        let touch = v.getattr("cfg")?.get_item("controls")?.call_method1("get", ("touch", false))?.is_truthy()?;
        let right: Vec<(String, PyObject)> = vec![
            ("操作とキー設定".into(), me(s, "later", vec![g.getattr("open_keys")?.unbind()])?),
            ("読み込み（設計図・OBJ）".into(), me(s, "later", vec![g.getattr("open_import")?.unbind()])?),
            ("見ている構築物を OBJ で出力".into(), me(s, "later", vec![v.getattr("export_obj")?.unbind()])?),
            (format!("BGM {} %（押すと +10）", pyint(&au.call_method1("get", ("bgm", 0))?)?), me(s, "volume", vec!["bgm".into_py(py)])?),
            (format!("効果音 {} %（押すと +10）", pyint(&au.call_method1("get", ("sfx", 0))?)?), me(s, "volume", vec!["sfx".into_py(py)])?),
            (format!("仮想ジョイスティック {}", if touch { "ON" } else { "OFF" }), s.getattr("touch")?.unbind()),
        ];
        for (k, (label, f)) in right.iter().enumerate() {
            btn(s, label, xr, top - 0.04 - k as f64 * 0.08, 0.5, 0.06, f.clone_ref(py), blue, 0.022, None, None)?;
        }
        let y = top - 0.04 - right.len() as f64 * 0.08;
        let cur = w.getattr("local_player")?.getattr("defn")?;
        let nm = cur.call_method1("get", ("name", cur.call_method1("get", ("id",))?))?;
        txt(s, format!("見た目: {}", nm.str()?), xr, y, 0.022, (1.0, 1.0, 1.0, 1.0), (0.5, 0.5))?;
        if w.getattr("registry")?.getattr("defs")?.get_item("players")?.len()? > 1 {
            for (dx, d, t) in [(-0.22, -1i64, "◀"), (0.22, 1, "▶")] {
                btn(s, t, xr + dx, y, 0.045, 0.045, me(s, "skin", vec![d.into_py(py)])?, blue, 0.02, None, None)?;
            }
        }
        if !client {
            let armed = slf.borrow().armed;
            btn(s, if armed { "もう一度：資源をなくして戻る" } else { "リスポーン（戻る）" }, xr, y - 0.08, 0.5, 0.06, s.getattr("respawn")?.unbind(),
                if armed { (0.6, 0.3, 0.2, 1.0) } else { (0.4, 0.3, 0.2, 1.0) }, 0.022, None, None)?;
        }
        Ok(())
    }
    /// 一時停止メニューを閉じてから開く（キー設定・読み込みの画面など）
    fn later(slf: &Bound<'_, Self>, f: &Bound<'_, PyAny>) -> R<()> {
        slf.call_method0("close")?;
        f.call0()?;
        Ok(())
    }
    fn volume(slf: &Bound<'_, Self>, key: &str) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        let au = audio(&v)?;
        let x = pyint(&au.call_method1("get", (key, 0))?)?;
        au.set_item(key, if x >= 100 { 0 } else { 100.min((x.div_euclid(10) + 1) * 10) })?;
        let snd = v.getattr("snd")?;
        if !snd.is_none() {
            snd.call_method1("apply", (v.getattr("cfg")?,))?;
        }
        if key == "sfx" {
            v.call_method1("sfx", ("click",))?;
        }
        save_cfg(&v);
        slf.call_method0("build")?;
        Ok(())
    }
    /// 仮想ジョイスティック（タッチ操作）の ON/OFF（設定に保存）。ON の間はマウスを閉じ込めない
    fn touch(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        let c = v.getattr("cfg")?.get_item("controls")?;
        let on = !c.call_method1("get", ("touch", false))?.is_truthy()?;
        c.set_item("touch", on)?;
        save_cfg(&v);
        v.getattr("w")?.call_method1("message", (format!("タッチ操作（仮想ジョイスティック） {}", if on { "ON" } else { "OFF" }),))?;
        slf.call_method0("build")?;
        Ok(())
    }
    /// 見た目を変える（ワールドに入っている MOD の見た目から）
    fn skin(slf: &Bound<'_, Self>, step: i64) -> R<()> {
        let py = slf.py();
        let w = slf.borrow().v.bind(py).getattr("w")?;
        let ids = PyList::new_bound(py, w.getattr("registry")?.getattr("defs")?.get_item("players")?.iter()?.collect::<R<Vec<_>>>()?);
        let lp = w.getattr("local_player")?;
        let cur = lp.getattr("defn")?.call_method1("get", ("id",))?;
        let i = if ids.contains(&cur)? { ids.index(&cur)? as i64 } else { 0 };
        let n = ids.len() as i64;
        lp.setattr("defn", w.call_method1("player_def", (ids.get_item((i + step).rem_euclid(n) as usize)?,))?)?;
        w.call_method1("message", (format!("見た目: {}", lp.getattr("defn")?.call_method1("get", ("name",))?.str()?),))?;
        slf.call_method0("build")?;
        Ok(())
    }
    /// リスポーン（宇宙に飛ばされた・出られない ときに）。サバイバルは手持ちの資源をなくすので2回押す
    fn respawn(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let w = slf.borrow().v.bind(py).getattr("w")?;
        if !w.getattr("creative")?.is_truthy()? && !slf.borrow().armed {
            slf.borrow_mut().armed = true;
            slf.call_method0("build")?;
            return Ok(());
        }
        slf.call_method0("close")?;
        w.call_method0("respawn")?;
        Ok(())
    }
    fn host(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method0("start_hosting")?;
        slf.call_method0("build")?;
        Ok(())
    }
    fn save(slf: &Bound<'_, Self>) -> R<()> {
        let w = slf.borrow().v.bind(slf.py()).getattr("w")?;
        w.call_method0("save")?;
        w.call_method1("message", ("保存しました",))?;
        Ok(())
    }
    fn quit(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow().v.bind(slf.py()).getattr("game")?.call_method1("_leave", ("menu",))?;
        Ok(())
    }
    fn on_key(slf: &Bound<'_, Self>, key: &str) -> R<bool> {
        if key == "escape" {
            slf.call_method0("close")?;
            return Ok(true);
        }
        Ok(false)
    }
    fn close(slf: &Bound<'_, Self>) -> R<()> {
        Screen::close(slf.downcast::<Screen>()?)?;
        slf.borrow().v.bind(slf.py()).call_method0("screen_closed")?;
        Ok(())
    }
}

/// I：インベントリ（アイテムバーに入れる物を選ぶ）。今までの画面の InventoryPanel と同じ並べ方
#[pyclass(module = "alpedia_core", extends = Screen, subclass)]
pub struct Inventory {
    #[pyo3(get, set)]
    v: PyObject,
    #[pyo3(get, set)]
    w: PyObject,
    #[pyo3(get, set)]
    reg: PyObject,
    #[pyo3(get, set)]
    player: PyObject,
    #[pyo3(get)]
    tab: String,
    #[pyo3(get)]
    page: i64,
    #[pyo3(get, set)]
    held: PyObject,
    #[pyo3(get, set)]
    cells: PyObject,
    #[pyo3(get, set)]
    slots: PyObject,
    #[pyo3(get, set)]
    info: PyObject,
    #[pyo3(get, set)]
    held_img: PyObject,
    #[pyo3(get, set)]
    _info_key: PyObject,
}

macro_rules! g {
    ($s:expr, $f:ident) => {
        $s.borrow().$f.clone_ref($s.py()).into_bound($s.py())
    };
}

fn inv_mod(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    Ok(imp(py, "main.client.inventory")?.into_any())
}

#[pymethods]
impl Inventory {
    #[classattr]
    const COLS: i64 = 9;
    #[classattr]
    const ROWS: i64 = 4;
    #[classattr]
    const CW: f64 = 0.118;
    #[classattr]
    const CH: f64 = 0.118;

    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> (Self, Screen) {
        (Inventory { v: py.None(), w: py.None(), reg: py.None(), player: py.None(), tab: "all".into(), page: 0, held: py.None(),
            cells: PyList::empty_bound(py).into_any().unbind(), slots: PyList::empty_bound(py).into_any().unbind(), info: py.None(),
            held_img: py.None(), _info_key: py.None() }, Screen::blank(py))
    }
    fn __init__(slf: &Bound<'_, Self>, ui: &Bound<'_, PyAny>, viewer: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        Screen::init_base(slf.downcast::<Screen>()?, ui)?;
        let w = viewer.getattr("w")?;
        let mut b = slf.borrow_mut();
        b.v = viewer.clone().unbind();
        b.reg = w.getattr("registry")?.unbind();
        b.player = w.getattr("player")?.unbind();
        b.w = w.unbind();
        b.tab = "all".into();
        b.page = 0;
        b.held = py.None();            // (アイテム, 元の枠 or None)
        b.cells = PyList::empty_bound(py).into_any().unbind();
        b.slots = PyList::empty_bound(py).into_any().unbind();
        b.info = py.None();
        b.held_img = py.None();
        Ok(())
    }
    fn items<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyList>> {
        let py = slf.py();
        let w = g!(slf, w);
        let tab = slf.borrow().tab.clone();
        let out = PyList::empty_bound(py);
        for it in g!(slf, reg).call_method0("palette")?.iter()? {
            let it = it?;
            if !w.call_method1("item_allowed", (&it,))?.is_truthy()? {
                continue;
            }
            let k = it.get_item(0)?;
            if (tab == "material" && !k.eq("material")?) || (tab == "device" && !k.eq("device")?) {
                continue;
            }
            out.append(it)?;
        }
        Ok(out)
    }
    fn build(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = g!(slf, w);
        let reg = g!(slf, reg);
        let player = g!(slf, player);
        let v = g!(slf, v);
        let white = (1.0, 1.0, 1.0, 1.0);
        s.call_method0("clear")?;
        let drop = s.getattr("drop_held")?.unbind();
        let bg = btn(s, "", 0.0, 0.0, 3.0, 2.0, py.None(), (0.0, 0.0, 0.0, 0.55), 0.022, Some(drop.clone_ref(py)), None)?;
        bg.set_item("click", drop)?;
        s.call_method1("panel", (-0.66, 0.46, 0.66, -0.46))?;
        txt(s, "インベントリ", -0.63, 0.44, 0.032, white, (0.0, 0.0))?;
        let tab = slf.borrow().tab.clone();
        for (k, (tid, name)) in [("all", "すべて"), ("material", "素材"), ("device", "機器")].iter().enumerate() {
            btn(s, name, -0.2 + k as f64 * 0.13, 0.405, 0.12, 0.045, me(s, "set_tab", vec![tid.into_py(py)])?,
                if *tid == tab { (0.3, 0.45, 0.65, 1.0) } else { (0.18, 0.2, 0.25, 1.0) }, 0.019, None, None)?;
        }
        txt(s, "クリックで持ち上げ → 下のアイテムバーの枠をクリックで入れる / マウスを合わせて 1〜9 キーでも入れられます",
            -0.63, 0.37, 0.0155, GREY, (0.0, 0.0))?;
        let items = Self::items(slf)?;
        let (cols, rows, cw, ch) = (Self::COLS, Self::ROWS, Self::CW, Self::CH);
        let per = cols * rows;
        let pages = 1.max((items.len() as i64 + per - 1) / per);
        let page = slf.borrow().page.min(pages - 1);
        slf.borrow_mut().page = page;
        let lo = ((page * per).max(0) as usize).min(items.len());
        let hi = (((page + 1) * per).max(0) as usize).min(items.len());
        let x0 = -(cols - 1) as f64 / 2.0 * cw;
        let y0 = 0.26;
        let cells = PyList::empty_bound(py);
        let surv = !w.getattr("creative")?.is_truthy()?;
        let inv = player.getattr("inventory")?;
        let icons = imp(py, "main.client.icons")?;
        let item_tex = imp(py, "alpedia_core")?.getattr("ui_item_tex")?;
        for (k, it) in items.iter().skip(lo).take(hi.saturating_sub(lo)).enumerate() {
            let k = k as i64;
            let x = x0 + (k % cols) as f64 * cw;
            let y = y0 - (k / cols) as f64 * ch;
            let wd = btn(s, "", x, y, cw - 0.01, ch - 0.01, me(s, "pick", vec![it.clone().unbind()])?, (0.13, 0.15, 0.2, 1.0), 0.022, None,
                Some(it.clone().unbind()))?;
            let cost = if surv { w.call_method1("item_cost", (&it,))? } else { PyDict::new_bound(py).into_any() };
            let mut ok = true;
            for kv in cost.call_method0("items")?.iter()? {
                let (r, a): (Bound<'_, PyAny>, Bound<'_, PyAny>) = kv?.extract()?;
                if !inv.call_method1("get", (r, 0.0))?.ge(a)? {
                    ok = false;
                    break;
                }
            }
            s.call_method1("image", (item_tex.call1((&reg, &it))?, x, y + if surv { 0.02 } else { 0.012 }, if surv { 0.064 } else { 0.075 },
                if ok { 1.0 } else { 0.35 }))?;
            let name: String = icons.call_method1("item_name", (&reg, &it))?.extract()?;
            let nm = if name.chars().count() <= 7 { name } else { format!("{}…", name.chars().take(6).collect::<String>()) };
            txt(s, nm, x, y - if surv { 0.026 } else { 0.045 }, 0.0138, white, (0.5, 0.5))?;
            if surv && cost.is_truthy()? {
                let mut parts = Vec::new();
                for kv in cost.call_method0("items")?.iter()? {
                    let (r, a): (Bound<'_, PyAny>, Bound<'_, PyAny>) = kv?.extract()?;
                    parts.push(format!("{}{}", reg.call_method1("res_name", (r,))?.str()?, a.call_method1("__format__", ("g",))?.str()?));
                }
                let t = parts.join(" ");
                let t = if t.chars().count() <= 12 { t } else { format!("{}…", t.chars().take(11).collect::<String>()) };
                txt(s, t, x, y - 0.046, 0.0113, if ok { (0.75, 0.85, 0.75, 1.0) } else { (1.0, 0.45, 0.4, 1.0) }, (0.5, 0.5))?;
            }
            cells.append((wd, it))?;
        }
        slf.borrow_mut().cells = cells.into_any().unbind();
        if pages > 1 {
            btn(s, "◀", 0.42, 0.405, 0.05, 0.045, me(s, "set_page", vec![(-1i64).into_py(py)])?, (0.22, 0.32, 0.45, 1.0), 0.022, None, None)?;
            txt(s, format!("{}/{}", page + 1, pages), 0.49, 0.405, 0.019, white, (0.5, 0.5))?;
            btn(s, "▶", 0.56, 0.405, 0.05, 0.045, me(s, "set_page", vec![1i64.into_py(py)])?, (0.22, 0.32, 0.45, 1.0), 0.022, None, None)?;
        }
        txt(s, "アイテムバー（クリックで持ち上げ・入れ替え / 右クリックで外す）", -0.63, -0.278, 0.0155, GREY, (0.0, 0.0))?;
        let sw = 0.085;
        let hb = player.getattr("hotbar")?;
        let slots_n: i64 = inv_mod(py)?.getattr("SLOTS")?.extract()?;
        let sel = v.getattr("sel")?;
        let slots = PyList::empty_bound(py);
        for k in 0..slots_n {
            let x = (k as f64 - (slots_n - 1) as f64 / 2.0) * (sw + 0.01);
            let y = -0.36;
            let is_sel = sel.eq(k)?;
            let wd = btn(s, "", x, y, sw, sw, me(s, "click_slot", vec![k.into_py(py)])?,
                if is_sel { (0.55, 0.45, 0.15, 1.0) } else { (0.16, 0.18, 0.23, 1.0) }, 0.022, Some(me(s, "unset", vec![k.into_py(py)])?), None)?;
            slots.append(wd)?;
            let h = hb.get_item(k)?;
            if h.is_truthy()? {
                s.call_method1("image", (item_tex.call1((&reg, h))?, x, y, sw * 0.85))?;
            }
            txt(s, (k + 1).to_string(), x - sw / 2.0 + 0.006, y + sw / 2.0 - 0.004, 0.0138, white, (0.0, 0.0))?;
        }
        slf.borrow_mut().slots = slots.into_any().unbind();
        for (k, (label, f)) in [("初期に戻す", "reset"), ("全部外す", "clear_all"), ("閉じる（Esc）", "close")].iter().enumerate() {
            btn(s, label, -0.45 + k as f64 * 0.45, -0.43, if k < 2 { 0.2 } else { 0.24 }, 0.042, s.getattr(*f)?.unbind(),
                if k < 2 { (0.3, 0.3, 0.33, 1.0) } else { (0.2, 0.45, 0.3, 1.0) }, 0.0175, None, None)?;
        }
        let info = txt(s, "", -0.62, -0.165, 0.0175, GREY, (0.0, 0.0))?;
        let mut b = slf.borrow_mut();
        b.info = info.unbind();
        b.held_img = py.None();
        b._info_key = py.None();
        Ok(())
    }
    // ---- 操作（InventoryPanel と同じ） ----
    #[pyo3(name = "set_tab")]
    fn set_tab_m(slf: &Bound<'_, Self>, t: String) -> R<()> {
        {
            let mut b = slf.borrow_mut();
            b.tab = t;
            b.page = 0;
        }
        slf.call_method0("build")?;
        Ok(())
    }
    #[pyo3(name = "set_page")]
    fn set_page_m(slf: &Bound<'_, Self>, d: i64) -> R<()> {
        {
            let mut b = slf.borrow_mut();
            b.page = 0.max(b.page + d);
        }
        slf.call_method0("build")?;
        Ok(())
    }
    fn pick(slf: &Bound<'_, Self>, item: PyObject) -> R<()> {
        let py = slf.py();
        slf.call_method0("return_held")?;
        slf.borrow_mut().held = (item, py.None()).into_py(py);
        Self::_show_held(slf)
    }
    fn click_slot(slf: &Bound<'_, Self>, k: i64) -> R<()> {
        let py = slf.py();
        let hb = g!(slf, player).getattr("hotbar")?;
        let held = g!(slf, held);
        let v = g!(slf, v);
        if held.is_none() {
            let cur = hb.get_item(k)?;
            if cur.is_truthy()? {
                slf.borrow_mut().held = (cur, k).into_py(py);
                hb.set_item(k, py.None())?;
            } else {
                v.setattr("sel", k)?;
            }
            slf.call_method0("build")?;
            Self::_show_held(slf)?;
            return Ok(());
        }
        let item = held.get_item(0)?;
        let origin = held.get_item(1)?;
        let prev = hb.get_item(k)?;
        hb.set_item(k, item)?;
        if !origin.is_none() && !prev.is_none() {
            hb.set_item(origin, prev)?;
        }
        slf.borrow_mut().held = py.None();
        v.setattr("sel", k)?;
        slf.call_method0("build")?;
        Ok(())
    }
    fn unset(slf: &Bound<'_, Self>, k: i64) -> R<()> {
        g!(slf, player).getattr("hotbar")?.set_item(k, slf.py().None())?;
        slf.call_method0("build")?;
        Ok(())
    }
    fn set_slot(slf: &Bound<'_, Self>, k: i64, item: &Bound<'_, PyAny>) -> R<()> {
        let hb = g!(slf, player).getattr("hotbar")?;
        if hb.contains(item)? {
            let j = hb.call_method1("index", (item,))?;
            hb.set_item(j, hb.get_item(k)?)?;
        }
        hb.set_item(k, item)?;
        g!(slf, v).setattr("sel", k)?;
        slf.call_method0("build")?;
        Ok(())
    }
    fn return_held(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let held = g!(slf, held);
        if held.is_truthy()? && !held.get_item(1)?.is_none() {
            let hb = g!(slf, player).getattr("hotbar")?;
            let o = held.get_item(1)?;
            if hb.get_item(&o)?.is_none() {
                hb.set_item(o, held.get_item(0)?)?;
            }
        }
        slf.borrow_mut().held = py.None();
        Ok(())
    }
    fn drop_held(slf: &Bound<'_, Self>) -> R<()> {
        if g!(slf, held).is_truthy()? {
            slf.call_method0("return_held")?;
            slf.call_method0("build")?;
        }
        Ok(())
    }
    fn reset(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        slf.borrow_mut().held = py.None();
        let p = g!(slf, player);
        p.setattr("hotbar", py.None())?;
        inv_mod(py)?.call_method1("ensure_hotbar", (p, g!(slf, reg)))?;
        slf.call_method0("build")?;
        Ok(())
    }
    fn clear_all(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        slf.borrow_mut().held = py.None();
        let n: usize = inv_mod(py)?.getattr("SLOTS")?.extract()?;
        g!(slf, player).setattr("hotbar", PyList::new_bound(py, (0..n).map(|_| py.None())))?;
        slf.call_method0("build")?;
        Ok(())
    }
    fn hovered_item(slf: &Bound<'_, Self>) -> R<PyObject> {
        let py = slf.py();
        let h = slf.getattr("hover")?;
        if h.is_none() {
            return Ok(py.None());
        }
        let info = h.call_method1("get", ("info",))?;
        if !info.is_none() {
            return Ok(info.unbind());
        }
        let slots = g!(slf, slots);
        if slots.contains(&h)? {
            return Ok(g!(slf, player).getattr("hotbar")?.get_item(slots.call_method1("index", (&h,))?)?.unbind());
        }
        Ok(py.None())
    }
    fn on_key(slf: &Bound<'_, Self>, key: &str) -> R<bool> {
        let py = slf.py();
        if key == "escape" || key == "i" {
            slf.call_method0("close")?;
        } else if key.chars().count() == 1 && "123456789".contains(key) {
            let hov = slf.getattr("hover")?;
            let mut it = if !hov.is_none() && !hov.call_method1("get", ("info",))?.is_none() { slf.call_method0("hovered_item")? } else { py.None().into_bound(py) };
            let held = g!(slf, held);
            if it.is_none() && held.is_truthy()? {
                it = held.get_item(0)?;
                slf.borrow_mut().held = py.None();
            }
            if !it.is_none() {
                slf.call_method1("set_slot", (key.parse::<i64>().unwrap() - 1, it))?;
            }
        }
        Ok(true)
    }
    fn _show_held(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let hi = g!(slf, held_img);
        if !hi.is_none() {
            hi.call_method0("remove")?;
            slf.borrow_mut().held_img = py.None();
        }
        let held = g!(slf, held);
        if held.is_truthy()? {
            let tex = imp(py, "alpedia_core")?.getattr("ui_item_tex")?.call1((g!(slf, reg), held.get_item(0)?))?;
            let m = slf.call_method1("image", (tex, 0.0, 0.0, 0.075))?;
            let b = Screen::bin(slf.downcast::<Screen>()?, 8);
            super::gfx::view(py)?.call_method1("mesh_props", (m.getattr("mid")?, true, false, true, 0, b))?;
            slf.borrow_mut().held_img = m.unbind();
        }
        Ok(())
    }
    fn update(slf: &Bound<'_, Self>, _dt: f64) -> R<()> {
        let py = slf.py();
        let hi = g!(slf, held_img);
        if !hi.is_none() {
            let (mx, my): (f64, f64) = slf.getattr("ui")?.getattr("mouse")?.extract()?;
            hi.getattr("np")?.call_method1("setPos", (mx, my, 0))?;
        }
        let mut it = slf.call_method0("hovered_item")?;
        let held = g!(slf, held);
        if it.is_none() && held.is_truthy()? {
            it = held.get_item(0)?;
        }
        let w = g!(slf, w);
        let creative = w.getattr("creative")?.is_truthy()?;
        let player = g!(slf, player);
        let inv = player.getattr("inventory")?;
        let invk: PyObject = if !creative {
            let l = py.import_bound("builtins")?.getattr("sorted")?.call1((inv.call_method0("items")?,))?;
            PyTuple::new_bound(py, l.iter()?.collect::<R<Vec<_>>>()?).into_any().unbind()
        } else {
            py.None()
        };
        let key = PyTuple::new_bound(py, [it.clone().unbind(), invk]);
        if key.eq(g!(slf, _info_key))? {
            return Ok(());
        }
        slf.borrow_mut()._info_key = key.into_any().unbind();
        let reg = g!(slf, reg);
        let lines: Bound<'_, PyList> = if it.is_truthy()? {
            inv_mod(py)?.call_method1("item_details", (&reg, &it))?.downcast_into()?
        } else {
            PyList::new_bound(py, ["アイテムにマウスを合わせると説明が出ます"])
        };
        if it.is_truthy()? && !creative {
            let cost = w.call_method1("item_cost", (&it,))?;
            if cost.is_truthy()? {
                lines.append(format!("費用: {}", reg.call_method1("res_text", (cost,))?.str()?))?;
            }
        }
        if !creative {
            let t = reg.call_method1("res_text", (&inv,))?;
            lines.append(format!("所持資源: {}", if t.is_truthy()? { t.str()?.to_string() } else { "なし".into() }))?;
        }
        let joined = PyString::new_bound(py, "\n").call_method1("join", (lines,))?;
        g!(slf, info).call_method1("set", (joined, 0.0175, GREY))?;
        Ok(())
    }
    fn close(slf: &Bound<'_, Self>) -> R<()> {
        slf.call_method0("return_held")?;
        Screen::close(slf.downcast::<Screen>()?)?;
        g!(slf, v).call_method0("screen_closed")?;
        Ok(())
    }
}
