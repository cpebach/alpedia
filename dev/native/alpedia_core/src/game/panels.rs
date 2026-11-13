//! 画面の上のパネル（ursina のまねで作る）：操作の説明とキー設定（main/client/keys_panel.py）。
//!
//! キーのボタンを押してから、割り当てたいキーを押すと変わる（Esc でやめる）。ほかの操作と同じキーなら入れ替える。
//! 設定は config/settings.json の controls に保存する。

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use super::imp;
use super::ursina::Entity;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

pub fn kw<'py>(py: Python<'py>, pairs: Vec<(&str, PyObject)>) -> R<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    for (k, v) in pairs {
        d.set_item(k, v)?;
    }
    Ok(d)
}
pub fn po<T: IntoPy<PyObject>>(py: Python<'_>, x: T) -> PyObject {
    x.into_py(py)
}
pub fn ursina(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "ursina")?.into_any())
}
pub fn partial<'py>(f: &B<'py>, args: Vec<PyObject>) -> R<B<'py>> {
    let py = f.py();
    let mut v: Vec<PyObject> = vec![f.clone().unbind()];
    v.extend(args);
    imp(py, "functools")?.getattr("partial")?.call1(PyTuple::new_bound(py, v))
}
fn log_exc(py: Python<'_>, logger: &str, e: &PyErr, msg: &str) -> R<()> {
    let k = PyDict::new_bound(py);
    k.set_item("exc_info", e.value_bound(py))?;
    imp(py, "main.log")?.call_method1("get", (logger,))?.call_method("exception", (msg,), Some(&k))?;
    Ok(())
}

/// キーを受け取る部品（panel._key(key) に渡す）
#[pyclass(module = "alpedia_core", extends = Entity, subclass)]
pub struct KeyCatcher {}

#[pymethods]
impl KeyCatcher {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<PyClassInitializer<Self>> {
        Ok(PyClassInitializer::from(Entity::blank(py, "Keys")?).add_subclass(KeyCatcher {}))
    }
    fn input(slf: &Bound<'_, Self>, key: &B<'_>) -> R<()> {
        let p = slf.as_any().getattr("panel")?;
        let m = slf.as_any().getattr("method").unwrap_or_else(|_| po(slf.py(), "_key").into_bound(slf.py()));
        p.call_method1(m.downcast::<pyo3::types::PyString>()?.clone(), (key,))?;
        Ok(())
    }
}

/// panel の method にキーを渡す部品を作る（parent の子）
pub fn key_catcher<'py>(py: Python<'py>, parent: &B<'py>, panel: &B<'py>, method: &str) -> R<B<'py>> {
    let k = kw(py, vec![("parent", parent.clone().unbind())])?;
    let e = py.get_type_bound::<KeyCatcher>().call((), Some(&k))?;
    e.setattr("panel", panel)?;
    e.setattr("method", method)?;
    Ok(e)
}

fn key_name<'py>(py: Python<'py>, k: &B<'py>) -> R<B<'py>> {
    imp(py, "main.client.touch_ui")?.call_method1("key_name", (k,))
}
fn kp(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.keys_panel")?.into_any())
}
fn action_label(py: Python<'_>, aid: &B<'_>) -> R<String> {
    let d = py.import_bound("builtins")?.getattr("dict")?.call1((kp(py)?.getattr("ACTIONS")?,))?;
    Ok(d.get_item(aid)?.str()?.to_string())
}

#[pyclass(module = "main.client.keys_panel", subclass, dict)]
pub struct KeysPanel {}

#[pymethods]
impl KeysPanel {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        KeysPanel {}
    }
    #[pyo3(signature = (u, cfg, parent, on_close, extra_help=None, on_change=None))]
    fn __init__(slf: &Bound<'_, Self>, u: &B<'_>, cfg: &B<'_>, parent: &B<'_>, on_close: &B<'_>, extra_help: Option<&B<'_>>, on_change: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let ur = ursina(py)?;
        let (entity, text, button, color) = (ur.getattr("Entity")?, ur.getattr("Text")?, ur.getattr("Button")?, ur.getattr("color")?);
        s.setattr("U", u)?;
        s.setattr("cfg", cfg)?;
        s.setattr("on_close", on_close)?;
        s.setattr("on_change", on_change.map(|x| x.clone().unbind()).unwrap_or_else(|| py.None()))?;
        s.setattr("color", &color)?;
        s.setattr("Button", &button)?;
        s.setattr("Text", &text)?;
        let root = entity.call((), Some(&kw(py, vec![("parent", parent.clone().unbind())])?))?;
        s.setattr("root", &root)?;
        let rgba = |r: f64, g: f64, b: f64, a: f64| -> R<PyObject> { Ok(color.call_method1("rgba", (r, g, b, a))?.unbind()) };
        let rgb = |r: f64, g: f64, b: f64| -> R<PyObject> { Ok(color.call_method1("rgb", (r, g, b))?.unbind()) };
        let r_ = || root.clone().unbind();
        button.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, (3, 2))), ("color", rgba(0.0, 0.0, 0.0, 0.6)?),
            ("z", po(py, 0.2)), ("highlight_color", rgba(0.0, 0.0, 0.0, 0.6)?), ("pressed_color", rgba(0.0, 0.0, 0.0, 0.6)?)])?))?;
        entity.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, (1.5, 0.92))), ("color", rgb(0.07, 0.08, 0.11)?), ("z", po(py, 0.1))])?))?;
        text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "操作とキー設定")), ("position", po(py, (0, 0.42))), ("origin", po(py, (0, 0))), ("scale", po(py, 1.4))])?))?;
        text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "キーのボタンを押してから、割り当てたいキーを押します（Esc でやめる）")),
            ("position", po(py, (0, 0.37))), ("origin", po(py, (0, 0))), ("scale", po(py, 0.65)), ("color", color.getattr("light_gray")?.unbind())])?))?;
        s.setattr("waiting", py.None())?;
        s.setattr("status", text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (0, -0.335))), ("origin", po(py, (0, 0))),
            ("scale", po(py, 0.7)), ("color", color.getattr("orange")?.unbind())])?))?)?;
        let btns = PyDict::new_bound(py);
        s.setattr("btns", &btns)?;
        let c = cfg.get_item("controls")?;
        for (k, a) in kp(py)?.getattr("ACTIONS")?.iter()?.enumerate() {
            let a = a?;
            let (aid, label) = (a.get_item(0)?, a.get_item(1)?);
            let (col, row) = (k / 11, k % 11);
            let x = -0.68 + col as f64 * 0.5;
            let y = 0.31 - row as f64 * 0.052;
            text.call((), Some(&kw(py, vec![("parent", r_()), ("text", label.unbind()), ("position", po(py, (x, y))), ("origin", po(py, (-0.5, 0))), ("scale", po(py, 0.7))])?))?;
            let b = button.call((), Some(&kw(py, vec![("parent", r_()), ("text", key_name(py, &c.call_method1("get", (&aid, ""))?)?.unbind()),
                ("position", po(py, (x + 0.36, y))), ("scale", po(py, (0.13, 0.042))), ("color", rgb(0.22, 0.32, 0.45)?), ("text_size", po(py, 0.7))])?))?;
            b.setattr("on_click", partial(&s.getattr("_listen")?, vec![aid.clone().unbind()])?)?;
            btns.set_item(&aid, b)?;
        }
        let mut lines: String = kp(py)?.getattr("OTHER")?.extract()?;
        if let Some(eh) = extra_help.filter(|e| e.is_truthy().unwrap_or(false)) {
            let hs: Vec<String> = eh.iter()?.map(|h| Ok(format!("・{}", h?.str()?))).collect::<R<_>>()?;
            lines = format!("{}\n{}", lines, hs.join("\n"));
        }
        text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, format!("[そのほかの操作]\n{}", lines))), ("position", po(py, (0.3, 0.33))),
            ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.62)), ("color", rgb(0.85, 0.88, 0.92)?), ("line_height", po(py, 1.1))])?))?;
        let b = button.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "初期値に戻す")), ("position", po(py, (-0.15, -0.4))), ("scale", po(py, (0.22, 0.05))),
            ("color", rgb(0.3, 0.3, 0.33)?), ("text_size", po(py, 0.75))])?))?;
        b.setattr("on_click", s.getattr("_reset")?)?;
        let b = button.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "閉じる")), ("position", po(py, (0.15, -0.4))), ("scale", po(py, (0.22, 0.05))),
            ("color", rgb(0.2, 0.5, 0.3)?), ("text_size", po(py, 0.75))])?))?;
        b.setattr("on_click", s.getattr("close")?)?;
        s.setattr("keys", key_catcher(py, &root, s, "_key")?)?;
        Ok(())
    }
    fn _listen(slf: &Bound<'_, Self>, aid: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("waiting", aid)?;
        let color = s.getattr("color")?;
        for kv in s.getattr("btns")?.call_method0("items")?.iter()? {
            let kv = kv?;
            let c = if kv.get_item(0)?.eq(aid)? { color.call_method1("rgb", (0.75, 0.5, 0.15))? } else { color.call_method1("rgb", (0.22, 0.32, 0.45))? };
            kv.get_item(1)?.setattr("color", c)?;
        }
        s.getattr("status")?.setattr("text", format!("「{}」に割り当てるキーを押してください", action_label(py, aid)?))?;
        Ok(())
    }
    fn _key(slf: &Bound<'_, Self>, key: &str) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let waiting = s.getattr("waiting")?;
        if waiting.is_none() {
            if key == "escape" {
                s.call_method0("close")?;
            }
            return Ok(());
        }
        if key.ends_with(" up") || key.contains("mouse") || key.starts_with("scroll") || key.ends_with(" hold") {
            return Ok(());
        }
        let aid = waiting;
        s.setattr("waiting", py.None())?;
        let color = s.getattr("color")?;
        for b in s.getattr("btns")?.call_method0("values")?.iter()? {
            b?.setattr("color", color.call_method1("rgb", (0.22, 0.32, 0.45))?)?;
        }
        if key == "escape" {
            s.getattr("status")?.setattr("text", "やめました")?;
            return Ok(());
        }
        let c = s.getattr("cfg")?.get_item("controls")?;
        let old = c.call_method1("get", (&aid,))?;
        let mut other = py.None().into_bound(py);
        for a in kp(py)?.getattr("ACTIONS")?.iter()? {
            let a = a?.get_item(0)?;
            if !a.eq(&aid)? && c.call_method1("get", (&a,))?.eq(key)? {
                other = a;
                break;
            }
        }
        c.set_item(&aid, key)?;
        let mut msg = format!("「{}」を {} にしました", action_label(py, &aid)?, key_name(py, &po(py, key).into_bound(py))?.str()?);
        if !other.is_none() {
            c.set_item(&other, &old)?;
            msg += &format!("（「{}」と入れ替え：{}）", action_label(py, &other)?, key_name(py, &old)?.str()?);
        }
        s.call_method1("_save", (msg,))?;
        Ok(())
    }
    fn _reset(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let defaults = imp(py, "main.config")?.getattr("DEFAULTS")?.get_item("controls")?;
        let c = s.getattr("cfg")?.get_item("controls")?;
        for a in kp(py)?.getattr("ACTIONS")?.iter()? {
            let aid = a?.get_item(0)?;
            c.set_item(&aid, defaults.call_method1("get", (&aid, ""))?)?;
        }
        s.setattr("waiting", py.None())?;
        s.call_method1("_save", ("初期値に戻しました",))?;
        Ok(())
    }
    fn _save(slf: &Bound<'_, Self>, msg: &str) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let cfg = s.getattr("cfg")?;
        let c = cfg.get_item("controls")?;
        for kv in s.getattr("btns")?.call_method0("items")?.iter()? {
            let kv = kv?;
            kv.get_item(1)?.setattr("text", key_name(py, &c.call_method1("get", (kv.get_item(0)?, ""))?)?)?;
        }
        match imp(py, "main.client.menu").and_then(|m| m.call_method1("save_config", (&cfg,))) {
            Ok(_) => s.getattr("status")?.setattr("text", msg)?,
            Err(e) if e.is_instance_of::<PyException>(py) => {
                log_exc(py, "keys", &e, "設定を保存できません")?;
                s.getattr("status")?.setattr("text", format!("保存できません: {}", e.value_bound(py).str()?))?;
            }
            Err(e) => return Err(e),
        }
        let oc = s.getattr("on_change")?;
        if oc.is_truthy()? {
            oc.call0()?;
        }
        Ok(())
    }
    fn close(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        super::client_misc::ui_destroy(py, &s.getattr("root")?, &PyTuple::empty_bound(py), None)?;
        let oc = s.getattr("on_close")?;
        if oc.is_truthy()? {
            oc.call0()?;
        }
        Ok(())
    }
}


// ---------------------------------------------------------------- device_ui
/// キーと毎フレームを受け取る部品（on_input(key)・on_update() を呼ぶ）
#[pyclass(module = "alpedia_core", extends = Entity, subclass)]
pub struct Hook {}

#[pymethods]
impl Hook {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<PyClassInitializer<Self>> {
        Ok(PyClassInitializer::from(Entity::blank(py, "Keys")?).add_subclass(Hook {}))
    }
    fn input(slf: &Bound<'_, Self>, key: &B<'_>) -> R<()> {
        let f = slf.as_any().getattr("on_input").unwrap_or_else(|_| slf.py().None().into_bound(slf.py()));
        if !f.is_none() {
            f.call1((key,))?;
        }
        Ok(())
    }
    fn update(slf: &Bound<'_, Self>) -> R<()> {
        let f = slf.as_any().getattr("on_update").unwrap_or_else(|_| slf.py().None().into_bound(slf.py()));
        if !f.is_none() {
            f.call0()?;
        }
        Ok(())
    }
}

/// parent の子に、キー（on_input）と毎フレーム（on_update）を受け取る部品を作る
pub fn hook<'py>(py: Python<'py>, parent: &B<'py>, on_input: Option<B<'py>>, on_update: Option<B<'py>>) -> R<B<'py>> {
    let e = py.get_type_bound::<Hook>().call((), Some(&kw(py, vec![("parent", parent.clone().unbind())])?))?;
    e.setattr("on_input", on_input.map(|x| x.unbind()).unwrap_or_else(|| py.None()))?;
    e.setattr("on_update", on_update.map(|x| x.unbind()).unwrap_or_else(|| py.None()))?;
    Ok(e)
}

fn dui(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.device_ui")?.into_any())
}

#[pyfunction]
pub fn dui_same_named<'py>(c: &B<'py>, name: &B<'py>) -> R<Vec<usize>> {
    let mut out = vec![];
    for (k, d) in c.getattr("devs")?.iter()?.enumerate() {
        let d = d?;
        if d.is_truthy()? && d.get_item("name")?.eq(name)? {
            out.push(k);
        }
    }
    Ok(out)
}

/// 同じ名前の機器に 1, 2, 3… と番号を付ける（位置の順）。付けた数を返す
#[pyfunction]
pub fn dui_number_same_named(py: Python<'_>, c: &B<'_>, name: &B<'_>) -> R<i64> {
    let mut idx = dui_same_named(c, name)?;
    if idx.len() < 2 {
        return Ok(0);
    }
    let cells = c.getattr("cells")?;
    let mut keyed = vec![];
    for &k in &idx {
        let cl: Vec<i64> = cells.get_item(k)?.extract()?;
        keyed.push(((cl[1], cl[2], cl[0]), k));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    idx = keyed.into_iter().map(|x| x.1).collect();
    let devs = c.getattr("devs")?;
    let mut used = std::collections::HashSet::new();
    for d in devs.iter()? {
        let d = d?;
        if d.is_truthy()? {
            used.insert(d.get_item("name")?.str()?.to_string());
        }
    }
    let nm = name.str()?.to_string();
    let (mut n, mut k) = (0, 1);
    for i in idx {
        while used.contains(&format!("{}{}", nm, k)) {
            k += 1;
        }
        let new = format!("{}{}", nm, k);
        devs.get_item(i)?.set_item("name", &new)?;
        used.insert(new);
        k += 1;
        n += 1;
    }
    let _ = py;
    Ok(n)
}

fn replace_dev(x: &B<'_>, old: &B<'_>, new: &B<'_>, hit: &mut bool) -> R<()> {
    if let Ok(d) = x.downcast::<PyDict>() {
        if d.call_method1("get", ("dev",))?.eq(old)? {
            d.set_item("dev", new)?;
            *hit = true;
        }
        let vals: Vec<B<'_>> = d.values().iter().collect();
        for v in vals {
            replace_dev(&v, old, new, hit)?;
        }
    } else if let Ok(l) = x.downcast::<pyo3::types::PyList>() {
        let vals: Vec<B<'_>> = l.iter().collect();
        for v in vals {
            replace_dev(&v, old, new, hit)?;
        }
    }
    Ok(())
}

/// この構築物のコンピューターのブロックプログラムに出てくる機器名を置き換えて保存し直す
#[pyfunction]
pub fn dui_update_programs(py: Python<'_>, world: &B<'_>, c: &B<'_>, old: &B<'_>, new: &B<'_>) -> R<i64> {
    let b = imp(py, "main.blocks")?;
    let mut count = 0;
    let scripts = world.getattr("dir")?.call_method1("__truediv__", ("scripts",))?;
    for d in c.getattr("devs")?.iter()? {
        let d = d?;
        if !d.is_truthy()? {
            continue;
        }
        let st = d.get_item("state")?;
        let bn = st.call_method1("get", ("blocks",))?;
        if !bn.is_truthy()? {
            continue;
        }
        let path = scripts.call_method1("__truediv__", (&bn,))?;
        let prog = match b.call_method1("load", (&path,)) {
            Ok(p) => p,
            Err(e) if e.is_instance_of::<PyException>(py) => continue,
            Err(e) => return Err(e),
        };
        let mut hit = false;
        replace_dev(&prog, old, new, &mut hit)?;
        if hit {
            b.call_method1("save", (&path, &prog))?;
            let bs: String = bn.extract()?;
            let base: String = bs.chars().take(bs.chars().count().saturating_sub(".blocks.json".chars().count())).collect();
            let _ = base.len();
            let pyf = scripts.call_method1("__truediv__", (format!("{}.py", base),))?;
            let k = PyDict::new_bound(py);
            k.set_item("encoding", "utf-8")?;
            pyf.call_method("write_text", (b.call_method1("to_python", (&prog, &bn))?,), Some(&k))?;
            world.getattr("runner")?.call_method1("invalidate", (pyf.getattr("name")?,))?;
            st.call_method1("pop", ("_env", py.None()))?;
            count += 1;
        }
    }
    Ok(count)
}

/// 日本語が表示できる等幅フォント（入力欄用）
#[pyfunction]
pub fn dui_find_code_font<'py>(py: Python<'py>, cfg: &B<'py>) -> R<B<'py>> {
    let custom = cfg.call_method1("get", ("graphics", PyDict::new_bound(py)))?.call_method1("get", ("code_font",))?;
    let pathc = imp(py, "pathlib")?.getattr("Path")?;
    let mut cands: Vec<B<'_>> = vec![];
    if custom.is_truthy()? {
        cands.push(custom);
    }
    for f in dui(py)?.getattr("CODE_FONT_CANDIDATES")?.iter()? {
        cands.push(f?);
    }
    for f in cands {
        if f.is_truthy()? {
            let p = pathc.call1((&f,))?;
            if p.call_method0("exists")?.is_truthy()? {
                return Ok(p);
            }
        }
    }
    Ok(py.None().into_bound(py))
}

/// 機器の名前などを設定するメニュー（ursina のまね）。owner がいれば、その上に重ねて開く（エディタなどから）
#[pyclass(module = "main.client.device_ui", name = "UDevicePanel", subclass, dict)]
pub struct UDevicePanel {}

#[pymethods]
impl UDevicePanel {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        UDevicePanel {}
    }
    #[pyo3(signature = (game, construct, index, owner=None))]
    fn __init__(slf: &Bound<'_, Self>, game: &B<'_>, construct: &B<'_>, index: &B<'_>, owner: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let ur = ursina(py)?;
        let (entity, text, button, inputf, camera, color, mouse) = (ur.getattr("Entity")?, ur.getattr("Text")?, ur.getattr("Button")?,
            ur.getattr("InputField")?, ur.getattr("camera")?, ur.getattr("color")?, ur.getattr("mouse")?);
        let owner = owner.cloned().unwrap_or_else(|| py.None().into_bound(py));
        s.setattr("game", game)?;
        s.setattr("world", game.getattr("world")?)?;
        s.setattr("c", construct)?;
        s.setattr("cell", construct.getattr("cells")?.get_item(index)?)?;
        s.setattr("owner", &owner)?;
        let u = PyDict::new_bound(py);
        u.set_item("Entity", &entity)?;
        u.set_item("Text", &text)?;
        u.set_item("Button", &button)?;
        u.set_item("color", &color)?;
        s.setattr("U", u)?;
        mouse.setattr("locked", false)?;
        if !owner.is_none() {
            owner.setattr("modal", s)?;
            for f in ["field", "name_field"] {
                if owner.hasattr(f)? {
                    owner.getattr(f)?.setattr("active", false)?;
                }
            }
        }
        let d = construct.getattr("devs")?.get_item(index)?;
        let dd = construct.getattr("reg")?.call_method1("device", (d.get_item("def")?,))?;
        let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
        let ty = dd.call_method1("get", ("type",))?;
        s.setattr("default_name", dd.call_method1("get", ("name", d.get_item("def")?))?)?;
        let type_name = dui(py)?.getattr("TYPE_NAMES")?.call_method1("get", (&ty, dd.call_method1("get", ("type", "機器"))?))?;
        s.setattr("type_name", &type_name)?;
        let root = entity.call((), Some(&kw(py, vec![("parent", camera.getattr("ui")?.unbind()), ("z", po(py, -0.5))])?))?;
        s.setattr("root", &root)?;
        let rgba = |r: f64, g: f64, b: f64, a: f64| -> R<PyObject> { Ok(color.call_method1("rgba", (r, g, b, a))?.unbind()) };
        let rgb = |r: f64, g: f64, b: f64| -> R<PyObject> { Ok(color.call_method1("rgb", (r, g, b))?.unbind()) };
        let r_ = || root.clone().unbind();
        let blocker = button.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, (3, 2))), ("color", rgba(0.0, 0.0, 0.0, 0.55)?),
            ("highlight_color", rgba(0.0, 0.0, 0.0, 0.55)?), ("pressed_color", rgba(0.0, 0.0, 0.0, 0.55)?), ("z", po(py, 0.05))])?))?;
        blocker.setattr("on_click", py.get_type_bound::<super::panda::NoOp>().call0()?)?;
        entity.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, (1.1, 0.8))), ("color", rgb(0.07, 0.08, 0.11)?), ("z", po(py, 0.02))])?))?;
        text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, format!("{} の設定", type_name.str()?))), ("position", po(py, (0, 0.35))), ("origin", po(py, (0, 0))), ("scale", po(py, 1.4))])?))?;
        text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "名前")), ("position", po(py, (-0.5, 0.25))), ("origin", po(py, (-0.5, 0))), ("scale", po(py, 0.9))])?))?;
        // 日本語が表示できるフォントで入力欄を作る（等幅の日本語フォントがあればそれを使う）
        let app = ur.getattr("application")?;
        let (old_mono, old_folder) = (text.getattr("default_monospace_font")?, app.getattr("fonts_folder")?);
        let code_font = dui_find_code_font(py, &game.getattr("cfg")?)?;
        if code_font.is_truthy()? {
            app.setattr("fonts_folder", code_font.getattr("parent")?)?;
            text.setattr("default_monospace_font", code_font.getattr("name")?)?;
        } else {
            text.setattr("default_monospace_font", text.getattr("default_font")?)?;
        }
        let field = inputf.call((), Some(&kw(py, vec![("parent", r_()), ("default_value", d.get_item("name")?.unbind()), ("position", po(py, (0.05, 0.25))),
            ("scale", po(py, (0.62, 0.05))), ("character_limit", dui(py)?.getattr("NAME_LIMIT")?.unbind()), ("active", po(py, true))])?));
        text.setattr("default_monospace_font", old_mono)?;
        app.setattr("fonts_folder", old_folder)?;
        let field = field?;
        s.setattr("field", &field)?;
        field.setattr("submit_on", vec!["enter"])?;
        field.setattr("on_submit", s.getattr("ok")?)?;
        let btn = |t: &str, pos: (f64, f64), f: B<'_>, w: f64, col: Option<PyObject>, ts: f64| -> R<B<'_>> {
            let b = button.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, t)), ("position", po(py, pos)), ("scale", po(py, (w, 0.042))),
                ("color", match col { Some(c) => c, None => rgb(0.22, 0.32, 0.45)? }), ("text_size", po(py, ts))])?))?;
            b.setattr("on_click", f)?;
            Ok(b)
        };
        btn("既定の名前", (-0.27, 0.16), partial(&s.getattr("set_text")?, vec![s.getattr("default_name")?.unbind()])?, 0.17, None, 0.7)?;
        btn("1文字消す", (-0.09, 0.16), s.getattr("backspace")?, 0.17, None, 0.7)?;
        btn("全部消す", (0.09, 0.16), partial(&s.getattr("set_text")?, vec![po(py, "")])?, 0.17, None, 0.7)?;
        btn("同名に番号を振る", (0.29, 0.16), s.getattr("number_all")?, 0.2, Some(rgb(0.6, 0.45, 0.15)?), 0.7)?;
        s.setattr("info", text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (-0.5, 0.08))), ("origin", po(py, (-0.5, 0.5))),
            ("scale", po(py, 0.72)), ("color", color.getattr("light_gray")?.unbind())])?))?)?;
        text.call((), Some(&kw(py, vec![("parent", r_()), ("position", po(py, (-0.5, -0.10))), ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.62)),
            ("color", color.getattr("light_gray")?.unbind()),
            ("text", po(py, "同じ名前の機器は、コンピューターからまとめて動きます。\n名前を分けると、ブロックの機器選択や api.device(\"名前\") で個別に制御できます。\n（名前はキーボードで入力します）"))])?))?;
        s.setattr("valve_text", py.None())?;
        if ty.eq("valve")? {
            // バルブの開き具合（燃料・物資を通す量。推進機につなげば推力が変わる）
            s.setattr("valve_text", text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (-0.5, -0.245))), ("origin", po(py, (-0.5, 0))),
                ("scale", po(py, 0.8))])?))?)?;
            let fmt = py.import_bound("builtins")?.getattr("format")?;
            for (k, v) in [0.0f64, 0.1, 0.25, 0.5, 0.75, 1.0].iter().enumerate() {
                let label: String = fmt.call1((*v, ".0%"))?.extract()?;
                btn(&label, (-0.18 + k as f64 * 0.1, -0.245), partial(&s.getattr("set_open")?, vec![po(py, *v)])?, 0.085, None, 0.7)?;
            }
        } else if ty.eq("thruster")? {
            text.call((), Some(&kw(py, vec![("parent", r_()), ("position", po(py, (-0.5, -0.245))), ("origin", po(py, (-0.5, 0))), ("scale", po(py, 0.62)),
                ("color", rgb(1.0, 0.85, 0.5)?),
                ("text", po(py, "E で点火（電気を少し使う）。燃えている間は発電もする。\n止める・出力を変えるには、タンクとのあいだにバルブを入れて、バルブで。"))])?))?;
        }
        btn("OK", (0.2, -0.33), s.getattr("ok")?, 0.16, Some(rgb(0.2, 0.5, 0.3)?), 0.8)?;
        btn("キャンセル", (0.38, -0.33), s.getattr("close")?, 0.16, Some(rgb(0.3, 0.3, 0.33)?), 0.8)?;
        s.setattr("keys", hook(py, &root, Some(s.getattr("_on_key")?), Some(s.getattr("refresh")?))?)?;
        s.call_method0("refresh")?;
        Ok(())
    }
    fn _on_key(slf: &Bound<'_, Self>, key: &B<'_>) -> R<()> {
        if key.eq("escape")? {
            slf.as_any().call_method0("close")?;
        }
        Ok(())
    }
    #[getter]
    fn index<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let s = slf.as_any();
        s.getattr("c")?.getattr("index")?.call_method1("get", (s.getattr("cell")?,))
    }
    fn text(slf: &Bound<'_, Self>) -> R<String> {
        Ok(slf.as_any().getattr("field")?.getattr("text_field")?.getattr("text")?.call_method0("strip")?.extract()?)
    }
    fn set_text(slf: &Bound<'_, Self>, s_: &str) -> R<()> {
        let py = slf.py();
        let lim: usize = dui(py)?.getattr("NAME_LIMIT")?.extract()?;
        let field = slf.as_any().getattr("field")?;
        let tf = field.getattr("text_field")?;
        let t: String = s_.chars().take(lim).collect();
        tf.setattr("text", &t)?;
        tf.getattr("cursor")?.setattr("x", t.chars().count())?;
        tf.getattr("cursor")?.setattr("y", 0)?;
        tf.call_method0("render")?;
        field.setattr("active", true)?;
        Ok(())
    }
    fn backspace(slf: &Bound<'_, Self>) -> R<()> {
        let mut t: String = slf.as_any().getattr("field")?.getattr("text_field")?.getattr("text")?.extract()?;
        t.pop();
        Self::set_text(slf, &t)
    }
    fn number_all(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = Self::index(slf)?;
        if i.is_none() {
            return Ok(());
        }
        let c = s.getattr("c")?;
        let d = c.getattr("devs")?.get_item(&i)?;
        let t = Self::text(slf)?;
        let name: B<'_> = if t.is_empty() { d.get_item("name")? } else { po(py, t).into_bound(py) };
        d.set_item("name", &name)?;
        let n = dui_number_same_named(py, &c, &name)?;
        let nn: String = d.get_item("name")?.extract()?;
        Self::set_text(slf, &nn)?;
        let msg = if n != 0 { format!("「{}」{} 個に番号を付けました", name.str()?, n) } else { format!("「{}」という名前の機器は1つだけです", name.str()?) };
        s.getattr("world")?.call_method1("message", (msg,))?;
        Ok(())
    }
    fn set_open(slf: &Bound<'_, Self>, v: f64) -> R<()> {
        let s = slf.as_any();
        let i = Self::index(slf)?;
        if i.is_none() {
            return Ok(());
        }
        let st = s.getattr("c")?.getattr("devs")?.get_item(&i)?.get_item("state")?;
        st.set_item("open", v)?;
        if v > 0.0 {
            st.set_item("set", v)?;
        }
        Ok(())
    }
    fn refresh(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = Self::index(slf)?;
        let info = s.getattr("info")?;
        if i.is_none() {
            info.setattr("text", "機器が見つかりません")?;
            return Ok(());
        }
        let c = s.getattr("c")?;
        let d = c.getattr("devs")?.get_item(&i)?;
        let vt = s.getattr("valve_text")?;
        if !vt.is_none() {
            let o: f64 = d.get_item("state")?.call_method1("get", ("open", 1.0))?.extract()?;
            let pct: String = py.import_bound("builtins")?.getattr("format")?.call1((o, ".0%"))?.extract()?;
            vt.setattr("text", format!("開き具合 {}{}", pct, if o <= 0.0 { "（閉）" } else { "" }))?;
        }
        let name = Self::text(slf)?;
        let ii: usize = i.extract()?;
        let dup = dui_same_named(&c, &po(py, name.as_str()).into_bound(py))?.into_iter().filter(|k| *k != ii).count();
        let mut lines = vec![format!("構築物: {}　種類: {}（{}）", c.getattr("name")?.str()?, s.getattr("type_name")?.str()?, d.get_item("def")?.str()?)];
        if name.is_empty() {
            lines.push("名前が空です".into());
        } else if dup > 0 {
            lines.push(format!("⚠ 同じ名前の機器がほかに {} 個あります（まとめて動きます）", dup));
        } else {
            lines.push("この構築物の中で一つだけの名前です（個別に制御できます）".into());
        }
        lines.push(format!("コンピューターからは  api.device(\"{}\")", name));
        info.setattr("text", lines.join("\n").replace('<', "＜").replace('>', "＞"))?;
        Ok(())
    }
    fn ok(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = Self::index(slf)?;
        let name = Self::text(slf)?;
        if !i.is_none() && !name.is_empty() {
            let c = s.getattr("c")?;
            let d = c.getattr("devs")?.get_item(&i)?;
            let old = d.get_item("name")?;
            d.set_item("name", &name)?;
            if !old.eq(&name)? {
                let mut n = 0;
                if dui_same_named(&c, &old)?.is_empty() {
                    n = dui_update_programs(py, &s.getattr("world")?, &c, &old, &po(py, name.as_str()).into_bound(py))?;
                }
                s.getattr("world")?.call_method1("message", (format!("名前を変更: {} → {}{}", old.str()?, name,
                    if n != 0 { format!("（ブロックプログラム {} 個も更新）", n) } else { String::new() }),))?;
                imp(py, "main.log")?.call_method1("get", ("device_ui",))?.call_method1("info", ("機器の名前を変更: %s → %s", &old, &name))?;
            }
        }
        s.call_method0("close")?;
        Ok(())
    }
    fn close(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        super::client_misc::ui_destroy(py, &s.getattr("root")?, &PyTuple::empty_bound(py), None)?;
        let owner = s.getattr("owner")?;
        if !owner.is_none() {
            owner.setattr("modal", py.None())?;
            if owner.hasattr("after_modal")? {
                owner.call_method0("after_modal")?;
            }
        } else {
            s.getattr("game")?.call_method0("editor_closed")?;
        }
        Ok(())
    }
}

/// 値の一覧から、v にいちばん近い物（v が数でなければ default）
#[pyfunction]
pub fn aui_nearest<'py>(py: Python<'py>, values: &B<'py>, v: &B<'py>, default: &B<'py>) -> R<B<'py>> {
    let f = match py.import_bound("builtins")?.getattr("float")?.call1((v,)) {
        Ok(x) => x.extract::<f64>()?,
        Err(e) if e.is_instance_of::<pyo3::exceptions::PyTypeError>(py) || e.is_instance_of::<pyo3::exceptions::PyValueError>(py) => return Ok(default.clone()),
        Err(e) => return Err(e),
    };
    let mut best: Option<(f64, B<'py>)> = None;
    for x in values.iter()? {
        let x = x?;
        let d = (super::ursina::f64of(&x)? - f).abs();
        if best.as_ref().map(|b| d < b.0).unwrap_or(true) {
            best = Some((d, x));
        }
    }
    match best {
        Some(b) => Ok(b.1),
        None => Err(pyo3::exceptions::PyValueError::new_err("min() iterable argument is empty")),
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<KeyCatcher>()?;
    m.add_class::<KeysPanel>()?;
    m.add_class::<Hook>()?;
    m.add_class::<UDevicePanel>()?;
    for f in [wrap_pyfunction_bound!(dui_same_named, m)?, wrap_pyfunction_bound!(dui_number_same_named, m)?, wrap_pyfunction_bound!(dui_update_programs, m)?,
              wrap_pyfunction_bound!(dui_find_code_font, m)?, wrap_pyfunction_bound!(aui_nearest, m)?] {
        m.add_function(f)?;
    }
    Ok(())
}
