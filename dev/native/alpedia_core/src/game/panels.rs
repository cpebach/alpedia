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

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<KeyCatcher>()?;
    m.add_class::<KeysPanel>()?;
    Ok(())
}
