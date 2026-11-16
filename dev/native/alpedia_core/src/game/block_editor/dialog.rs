//! ブロックエディター：値を入れる窓・名前を変える窓

use crate::game::construct::{con, Construct};
use super::*;

#[pymethods]
impl BlockEditor {
    pub(crate) fn open_dialog(slf: &Bound<'_, Self>, node: &B<'_>, key: &B<'_>, kind: &B<'_>) -> R<()> {
        let py = slf.py();
        Self::ga_dialogs(slf)?.call_method1("append", (PyTuple::new_bound(py, [node, key, kind]),))?;
        Self::render_dialog(slf)
    }
    #[pyo3(signature = (all_=false))]
    pub(crate) fn close_dialog(slf: &Bound<'_, Self>, all_: bool) -> R<()> {
        let py = slf.py();
        if all_ {
            Self::sa_dialogs(slf, PyList::empty_bound(py))?;
        } else if Self::ga_dialogs(slf)?.is_truthy()? {
            Self::ga_dialogs(slf)?.call_method0("pop")?;
        }
        Self::render_dialog(slf)?;
        Self::render(slf)
    }
    #[pyo3(signature = (node, key, value, stay=false))]
    pub(crate) fn set_value(slf: &Bound<'_, Self>, node: &B<'_>, key: &B<'_>, value: &B<'_>, stay: bool) -> R<()> {
        node.set_item(key, value)?;
        Self::sa_dirty(slf, true)?;
        Self::sa_discard_armed(slf, false)?;
        if !stay {
            Self::close_dialog(slf, false)
        } else {
            Self::render_dialog(slf)?;
            Self::render(slf)
        }
    }
    pub(crate) fn render_dialog(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let r = Self::ga_dialog_root(slf)?;
        let kids: Vec<B<'_>> = r.getattr("children")?.iter()?.collect::<R<_>>()?;
        for e in kids {
            crate::game::client_misc::ui_destroy(py, &e, &PyTuple::empty_bound(py), None)?;
        }
        let dialogs = Self::ga_dialogs(slf)?;
        if !dialogs.is_truthy()? {
            return Ok(());
        }
        let b = bm(py)?;
        let col_ = col(s)?;
        let last = dialogs.get_item(-1)?;
        let (node, key, kind) = (last.get_item(0)?, last.get_item(1)?, last.get_item(2)?);
        let kinds = ss(&kind)?;
        let cur = node.call_method1("get", (&key,))?;
        let dark = || col_.call_method1("rgba", (0, 0, 0, 0.55));
        let blocker = mk(s, "Button", vec![("parent", r.clone().unbind()), ("model", po(py, "quad")), ("scale", po(py, (3, 2))), ("color", dark()?.unbind()),
            ("z", po(py, 0.05)), ("highlight_color", dark()?.unbind()), ("pressed_color", dark()?.unbind())])?;
        blocker.setattr("on_click", clo0(py, |_| Ok(()))?)?;
        mk(s, "Entity", vec![("parent", r.clone().unbind()), ("model", po(py, "quad")), ("position", po(py, (0, 0.0, 0.02))), ("scale", po(py, (1.25, 0.9))),
            ("color", rgb(s, (0.1, 0.11, 0.15))?.unbind())])?;
        let base = kinds.split(':').next().unwrap_or("").to_string();
        let kind_name = match base.as_str() {
            "num" => "数値",
            "bool" => "条件",
            "text" => "文字",
            "key" => "キー",
            "var" => "変数",
            "vec" => "矢印",
            "vvar" => "矢印の変数",
            _ if kinds.starts_with("dev") => "機器",
            _ => "選択",
        };
        mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, format!("入力欄（{}）を設定", kind_name))), ("position", po(py, (-0.6, 0.42))),
            ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 1.0))])?;
        mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, format!("今の値: {}", ss(&b.call_method1("summary", (&cur,))?)?))),
            ("position", po(py, (-0.6, 0.37))), ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.8)), ("color", cn(s, "light_gray")?.unbind())])?;
        let mut y = 0.30;
        // 組み合わせの式なら、中身を編集できるようにする
        let isd = cur.is_instance_of::<PyDict>();
        let children: Vec<B<'_>> = if isd { b.call_method1("expr_children", (&cur,))?.iter()?.collect::<R<_>>()? } else { vec![] };
        let combo = isd && {
            let t = getd(&cur, "type")?;
            ["op", "cmp", "logic", "func"].iter().any(|k| t.eq(*k).unwrap_or(false))
        };
        if !children.is_empty() || combo {
            let mut x = -0.6;
            mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, "中身:")), ("position", po(py, (x, y))), ("origin", po(py, (-0.5, 0))),
                ("scale", po(py, 0.75))])?;
            x += 0.07;
            if let Some((name, f)) = Self::op_cycle(slf, &cur)? {
                btn(s, &r, &format!("種類: {}  ⟳", name), (x + 0.1, y), &f, 0.2, 0.04, None, 0.7)?;
                x += 0.215;
            }
            let od = s.getattr("open_dialog")?;
            for ch in &children {
                let (ck, ckind) = (ch.get_item(0)?, ch.get_item(1)?);
                let label = format!("{}: {}", ss(&ck)?, ss(&b.call_method1("summary", (cur.call_method1("get", (&ck,))?,))?)?);
                let w = 0.5f64.min(0.14f64.max(label.chars().count() as f64 * 0.013));
                let bt = btn(s, &r, &label, (x + w / 2.0, y), &partial(&od, vec![cur.clone().unbind(), ck.unbind(), ckind.unbind()])?, w, 0.04,
                    Some(rgb(s, (0.85, 0.85, 0.85))?), 0.7)?;
                bt.getattr("text_entity")?.setattr("color", cn(s, "black")?)?;
                x += w + 0.01;
            }
            y -= 0.06;
        }
        mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, "置き換える:")), ("position", po(py, (-0.6, y))), ("origin", po(py, (-0.5, 0))),
            ("scale", po(py, 0.75)), ("color", cn(s, "light_gray")?.unbind())])?;
        y -= 0.045;
        Self::_options(slf, &r, &node, &key, &kinds, &cur, y)?;
        if dialogs.len()? > 1 {
            btn(s, &r, "◀ 戻る", (0.36, -0.40), &s.getattr("close_dialog")?, 0.15, 0.04, None, 0.7)?;
        }
        let close_all = imp(py, "functools")?.getattr("partial")?.call((s.getattr("close_dialog")?,), Some(&kw(py, vec![("all_", po(py, true))])?))?;
        btn(s, &r, "閉じる", (0.53, -0.40), &close_all, 0.15, 0.04, Some(rgb(s, (0.3, 0.3, 0.33))?), 0.7)?;
        Ok(())
    }
    pub(crate) fn _op_cycle<'py>(slf: &Bound<'py, Self>, cur: &B<'py>) -> R<Option<(PyObject, B<'py>)>> {
        let py = slf.py();
        Ok(Self::op_cycle(slf, cur)?.map(|(n, f)| (po(py, n), f)))
    }
    /// items: [(表示, 関数, 色)] を並べる。次の y を返す。
    #[pyo3(signature = (r, items, y, cols=4, w=0.27))]
    pub(crate) fn _grid(slf: &Bound<'_, Self>, r: &B<'_>, items: &B<'_>, y: f64, cols: usize, w: f64) -> R<f64> {
        let mut v = vec![];
        for it in items.iter()? {
            let it = it?;
            v.push((ss(&it.get_item(0)?)?, it.get_item(1)?, it.get_item(2)?));
        }
        grid(slf.as_any(), r, &v, y, cols, w)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn _options(slf: &Bound<'_, Self>, r: &B<'_>, node: &B<'_>, key: &B<'_>, kind: &str, cur: &B<'_>, y: f64) -> R<f64> {
        let py = slf.py();
        let s = slf.as_any();
        let b = bm(py)?;
        let cc = b.getattr("CAT_COLOR")?;
        let cat_c = |n: &str, k: f64| -> R<B<'_>> { rgb(s, dim3(tup3(&cc.get_item(n)?)?, k)) };
        let sens_c = cat_c("sensor", 0.9)?;
        let op_c = cat_c("op", 0.9)?;
        let var_c = cat_c("var", 0.9)?;
        let grey = || rgb(s, (0.3, 0.3, 0.4));
        let base = kind.split(':').next().unwrap_or("").to_string();
        let after = kind.split_once(':').map(|x| x.1.to_string()).unwrap_or_default();
        // sv(node, key, 値) を押したときに呼ぶ関数（値は押したときに作る）
        let (sk, nk, kk) = (s.clone().unbind(), node.clone().unbind(), key.clone().unbind());
        let sv = move |make: Box<dyn Fn(Python<'_>) -> R<PyObject>>, stay: bool| -> R<B<'_>> {
            let (sk, nk, kk) = (sk.clone_ref(py), nk.clone_ref(py), kk.clone_ref(py));
            clo0(py, move |py| {
                let v = make(py)?;
                let k = kw(py, vec![("stay", po(py, stay))])?;
                sk.bind(py).call_method("set_value", (nk.bind(py), kk.bind(py), v), Some(&k))?;
                Ok(())
            })
        };
        let val = |v: PyObject| -> Box<dyn Fn(Python<'_>) -> R<PyObject>> { Box::new(move |py| Ok(v.clone_ref(py))) };
        // 押したときに d を作り直す（{"type": t, name_key: name}）
        let fresh = |pairs: Vec<(&'static str, PyObject)>| -> Box<dyn Fn(Python<'_>) -> R<PyObject>> {
            Box::new(move |py| {
                let d = PyDict::new_bound(py);
                for (k, v) in &pairs {
                    d.set_item(*k, v.clone_ref(py))?;
                }
                Ok(d.into_any().unbind())
            })
        };
        let dcopy = |x: B<'_>| -> Box<dyn Fn(Python<'_>) -> R<PyObject>> {
            let x = x.unbind();
            Box::new(move |py| Ok(deepcopy(x.bind(py))?.unbind()))
        };
        let vec_c = cat_c("vec", 0.9)?;
        if base == "key" {
            let mut items = vec![];
            for k in b.getattr("KEYS")?.iter()? {
                let k = k?;
                items.push((ss(&k)?, sv(val(k.unbind()), false)?, grey()?));
            }
            return grid(s, r, &items, y, 5, 0.2);
        }
        if base == "choice" {
            let mut items = vec![];
            for o in after.split('|') {
                items.push((o.to_string(), sv(val(po(py, o)), false)?, grey()?));
            }
            return grid(s, r, &items, y, 4, 0.27);
        }
        if base == "dev" {
            let names = Self::device_names(slf, Some(after.clone()))?;
            let mut items = vec![("すべて".to_string(), sv(val(po(py, "*")), false)?, grey()?)];
            for n in names {
                items.push((n.clone(), sv(val(po(py, n)), false)?, rgb(s, (0.25, 0.4, 0.6))?));
            }
            let y = grid(s, r, &items, y, 4, 0.27)?;
            mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, "同じ名前の機器はまとめて動きます")), ("position", po(py, (-0.6, y))),
                ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.65)), ("color", cn(s, "light_gray")?.unbind())])?;
            return Ok(y - 0.04);
        }
        if base == "var" || base == "vvar" {
            let mut items = vec![];
            for n in Self::var_names(slf, base == "vvar")? {
                items.push((n.clone(), sv(val(po(py, n)), false)?, if base == "vvar" { vec_c.clone() } else { var_c.clone() }));
            }
            let y = grid(s, r, &items, y, 4, 0.27)?;
            let (sk, nk, kk) = (s.clone().unbind(), node.clone().unbind(), key.clone().unbind());
            let on_ok = clo1(py, move |t| {
                let py = t.py();
                if t.is_truthy()? {
                    sk.bind(py).call_method1("set_value", (nk.bind(py), kk.bind(py), t))?;
                }
                Ok(())
            })?;
            return input_row(s, r, y, "新しい変数名（半角英数字）", "", NAME_CHARS, &on_ok);
        }
        if base == "vec" {
            let mut items = vec![];
            for it in b.getattr("VSENSORS")?.call_method0("items")?.iter()? {
                let it = it?;
                let (k, name) = (it.get_item(0)?, it.get_item(1)?.get_item(0)?);
                items.push((ss(&name)?, sv(fresh(vec![("type", po(py, "vsensor")), ("name", k.unbind())]), false)?, vec_c.clone()));
            }
            let mut y = grid(s, r, &items, y, 4, 0.24)?;
            let mut ops = vec![];
            for e in b.getattr("EXPR_PALETTE")?.iter()? {
                let e = e?;
                let (label, n) = (e.get_item(1)?, e.get_item(2)?);
                if b.call_method1("value_kind", (&n,))?.eq("vec")? && !n.get_item("type")?.eq("vsensor")? {
                    let nn = deepcopy(&n)?;
                    if b.call_method1("value_kind", (cur,))?.eq("vec")? && nn.contains("a")? {
                        nn.set_item("a", deepcopy(cur)?)?;
                    }
                    ops.push((ss(&label)?, sv(dcopy(nn), true)?, vec_c.clone()));
                }
            }
            y = grid(s, r, &ops, y, 3, 0.33)?;
            let mut vitems = vec![];
            for n in Self::var_names(slf, true)? {
                vitems.push((n.clone(), sv(fresh(vec![("type", po(py, "vvar")), ("name", po(py, n))]), false)?, vec_c.clone()));
            }
            return grid(s, r, &vitems, y, 6, 0.16);
        }
        // 値（数値・条件・文字）
        if base == "num" || base == "text" {
            let mut y = y;
            let isd = cur.is_instance_of::<PyDict>();
            let (sk, nk, kk) = (s.clone().unbind(), node.clone().unbind(), key.clone().unbind());
            if base == "num" {
                let v0 = if isd && getd(cur, "type")?.eq("num")? { ss(&b.call_method1("_fmt_num", (getd(cur, "v")?,))?)? } else { String::new() };
                let on_ok = clo1(py, move |t| {
                    let py = t.py();
                    let d = dict(py, vec![("type", po(py, "num")), ("v", po(py, be_to_float(py, t)?))])?;
                    sk.bind(py).call_method1("set_value", (nk.bind(py), kk.bind(py), d))?;
                    Ok(())
                })?;
                y = input_row(s, r, y, "数値を入力", &v0, "0123456789.-", &on_ok)?;
            } else {
                let v0 = if isd && getd(cur, "type")?.eq("text")? { ss(&cur.call_method1("get", ("v", ""))?)? } else { String::new() };
                let on_ok = clo1(py, move |t| {
                    let py = t.py();
                    let d = dict(py, vec![("type", po(py, "text")), ("v", t.clone().unbind())])?;
                    sk.bind(py).call_method1("set_value", (nk.bind(py), kk.bind(py), d))?;
                    Ok(())
                })?;
                let text_chars = format!("{} .,:;!?+-*/=()[]%#@&'\"<>", NAME_CHARS);
                y = input_row(s, r, y, "文字を入力（半角）", &v0, &text_chars, &on_ok)?;
                let ck = cur.clone().unbind();
                let mkjoin: Box<dyn Fn(Python<'_>) -> R<PyObject>> = Box::new(move |py| {
                    let c = ck.bind(py);
                    let a = if c.is_instance_of::<PyDict>() { c.clone() } else { dict(py, vec![("type", po(py, "text")), ("v", po(py, ""))])? };
                    let bb = dict(py, vec![("type", po(py, "sensor")), ("name", po(py, "altitude"))])?;
                    Ok(dict(py, vec![("type", po(py, "join")), ("a", a.unbind()), ("b", bb.unbind())])?.unbind())
                });
                y = grid(s, r, &[("□ と □ をつなぐ".to_string(), sv(mkjoin, true)?, op_c.clone())], y, 4, 0.27)?;
            }
            let mut items = vec![];
            for it in b.getattr("SENSORS")?.call_method0("items")?.iter()? {
                let it = it?;
                let (k, name) = (it.get_item(0)?, it.get_item(1)?.get_item(0)?);
                items.push((ss(&name)?, sv(fresh(vec![("type", po(py, "sensor")), ("name", k.unbind())]), false)?, sens_c.clone()));
            }
            items.push(("機器の値…".to_string(), sv(fresh(vec![("type", po(py, "devval")), ("dev", po(py, "*")), ("prop", po(py, "角度"))]), true)?, sens_c.clone()));
            let radio: Box<dyn Fn(Python<'_>) -> R<PyObject>> = Box::new(|py| {
                let ch = dict(py, vec![("type", po(py, "text")), ("v", po(py, "ch1"))])?;
                Ok(dict(py, vec![("type", po(py, "radio_val")), ("ch", ch.unbind())])?.unbind())
            });
            items.push(("通信の受信値…".to_string(), sv(radio, true)?, cat_c("radio", 0.9)?));
            for e in b.getattr("EXPR_PALETTE")?.iter()? {
                let e = e?;
                let (label, n) = (e.get_item(1)?, e.get_item(2)?);
                let t = n.get_item("type")?;
                if t.eq("vlen")? || t.eq("vcomp")? {
                    items.push((ss(&label)?.replace('→', "矢印"), sv(dcopy(n), true)?, vec_c.clone()));
                }
            }
            y = grid(s, r, &items, y, 5, 0.2)?;
            let mut ops = vec![];
            for e in b.getattr("EXPR_PALETTE")?.iter()? {
                let e = e?;
                let (label, n) = (e.get_item(1)?, e.get_item(2)?);
                let t = n.get_item("type")?;
                if t.eq("op")? || t.eq("func")? || t.eq("random")? {
                    let nn = deepcopy(&n)?;
                    if isd && ["num", "sensor", "var", "op", "func"].iter().any(|k| getd(cur, "type").and_then(|x| x.eq(*k)).unwrap_or(false)) {
                        nn.set_item("a", deepcopy(cur)?)?;
                    }
                    ops.push((ss(&label)?, sv(dcopy(nn), true)?, op_c.clone()));
                }
            }
            y = grid(s, r, &ops, y, 4, 0.24)?;
            let mut vitems = vec![];
            for n in Self::var_names(slf, false)? {
                vitems.push((n.clone(), sv(fresh(vec![("type", po(py, "var")), ("name", po(py, n))]), false)?, var_c.clone()));
            }
            return grid(s, r, &vitems, y, 6, 0.16);
        }
        if base == "bool" {
            let mut items = vec![];
            let isd = cur.is_instance_of::<PyDict>();
            for e in b.getattr("EXPR_PALETTE")?.iter()? {
                let e = e?;
                let (label, n) = (e.get_item(1)?, e.get_item(2)?);
                let t = ss(&n.get_item("type")?)?;
                if ["cmp", "logic", "not", "key", "devbool", "radio_has", "radio_ok"].contains(&t.as_str()) {
                    let nn = deepcopy(&n)?;
                    if t == "cmp" {
                        nn.set_item("a", dict(py, vec![("type", po(py, "sensor")), ("name", po(py, "altitude"))])?)?;
                        nn.set_item("b", dict(py, vec![("type", po(py, "num")), ("v", po(py, 10))])?)?;
                    }
                    if (t == "logic" || t == "not") && isd {
                        nn.set_item("a", deepcopy(cur)?)?;
                    }
                    items.push((ss(&label)?, sv(dcopy(nn), true)?, op_c.clone()));
                }
            }
            items.push(("はい（常に成り立つ）".to_string(), sv(fresh(vec![("type", po(py, "bool")), ("v", po(py, true))]), false)?, op_c.clone()));
            items.push(("いいえ".to_string(), sv(fresh(vec![("type", po(py, "bool")), ("v", po(py, false))]), false)?, op_c.clone()));
            return grid(s, r, &items, y, 3, 0.33);
        }
        Ok(y)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn _input_row(slf: &Bound<'_, Self>, r: &B<'_>, y: f64, label: &str, value: &B<'_>, chars: &str, on_ok: &B<'_>) -> R<f64> {
        input_row(slf.as_any(), r, y, label, &ss(value)?, chars, on_ok)
    }

    // ------------------------------------------------------------------
    pub(crate) fn _status(slf: &Bound<'_, Self>, msg: &str) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if s.hasattr("status")? {
            s.getattr("status")?.setattr("text", safe(&wrap(msg, 24)))?;
            Self::sa_status_t(slf, now(py)?)?;
        }
        Ok(())
    }
    pub(crate) fn open_rename(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = Self::index(slf)?;
        if i.is_none() {
            return Ok(());
        }
        let c = Self::ga_c(slf)?;
        let before = PyDict::new_bound(py);
        for (k, d) in Construct::g_devs(con(&c)?)?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? {
                before.set_item(k, d.get_item("name")?)?;
            }
        }
        Self::sa_names_before(slf, before)?;
        imp(py, "main.client.device_ui")?.getattr("DevicePanel")?.call((Self::ga_game(slf)?, c, i), Some(&kw(py, vec![("owner", s.clone().unbind())])?))?;
        Ok(())
    }
    /// 名前を変えたら、プログラム中の古い名前も新しい名前に置き換える（ほかに同名がなければ）。
    pub(crate) fn after_modal(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let before = Self::ga_names_before(slf).unwrap_or_else(|_| PyDict::new_bound(py).into_any());
        let now_ = PyDict::new_bound(py);
        for (k, d) in Self::ga_c(slf)?.getattr("devs")?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? {
                now_.set_item(k, d.get_item("name")?)?;
            }
        }
        let names_now: Vec<B<'_>> = now_.values().iter().collect();
        let changes = PyDict::new_bound(py);
        for it in before.call_method0("items")?.iter()? {
            let it = it?;
            let (k, old) = (it.get_item(0)?, it.get_item(1)?);
            if let Some(nw) = now_.get_item(&k)? {
                if !nw.eq(&old)? && !names_now.iter().any(|n| n.eq(&old).unwrap_or(false)) {
                    changes.set_item(&old, nw)?;
                }
            }
        }
        if !changes.is_empty() {
            let mut hit = false;
            fn walk(x: &B<'_>, changes: &Bound<'_, PyDict>, hit: &mut bool, s: &B<'_>) -> R<()> {
                if let Ok(d) = x.downcast::<PyDict>() {
                    let dv = getd(x, "dev")?;
                    if dv.is_instance_of::<PyString>() {
                        if let Some(nw) = changes.get_item(&dv)? {
                            d.set_item("dev", nw)?;
                            s.setattr("dirty", true)?;
                            *hit = true;
                        }
                    }
                    for v in d.values().iter() {
                        walk(&v, changes, hit, s)?;
                    }
                } else if let Ok(l) = x.downcast::<PyList>() {
                    for v in l.iter() {
                        walk(&v, changes, hit, s)?;
                    }
                }
                Ok(())
            }
            walk(&Self::ga_prog(slf)?, &changes, &mut hit, s)?;
            if hit {
                Self::_status(slf, "名前の変更をプログラムにも反映しました（保存してください）")?;
            }
        }
        Self::render(slf)
    }
}
