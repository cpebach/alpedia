//! ブロックエディター：ブロックを選ぶ・入れる・消す・名前の一覧

use crate::game::construct::{con, Construct};
use super::*;

#[pymethods]
impl BlockEditor {
    /// 入力欄を押した：1回目は選ぶ（パレットの式ブロックがここに入る）、選んである欄をもう一度押すと値の入力。
    pub(crate) fn slot_click(slf: &Bound<'_, Self>, owner: &B<'_>, key: &B<'_>, kind: &B<'_>) -> R<()> {
        let py = slf.py();
        if Self::_is_sel(slf, owner, key)? {
            return Self::open_dialog(slf, owner, key, kind);
        }
        Self::sa_slot_sel(slf, PyTuple::new_bound(py, [owner, key, kind]))?;
        Self::_status(slf, "左の「値・条件のブロック」を押すとここに入ります（もう一度押すと数値などを入力）")?;
        Self::build_palette(slf)?;
        Self::render(slf)
    }
    /// 演算の種類・機器の値の種類などの ▾：かつ／または は押すたびに切り替え、ほかは選ぶ画面。
    pub(crate) fn menu_click(slf: &Bound<'_, Self>, node: &B<'_>, key: &B<'_>, opts: &B<'_>) -> R<()> {
        let py = slf.py();
        if key.eq("op")? && getd(node, "type")?.eq("logic")? {
            node.set_item("op", if getd(node, "op")?.eq("and")? { "or" } else { "and" })?;
            Self::sa_dirty(slf, true)?;
            return Self::render(slf);
        }
        let mut o = vec![];
        for x in opts.iter()? {
            o.push(ss(&x?)?);
        }
        Self::open_dialog(slf, node, key, &po(py, format!("choice:{}", o.join("|"))).into_bound(py))
    }
    /// パレットの式ブロックを、選んだ入力欄に入れる（演算などは、今の値を左側に入れたまま包む）。
    pub(crate) fn put_reporter(slf: &Bound<'_, Self>, node: &B<'_>) -> R<()> {
        let py = slf.py();
        let b = bm(py)?;
        let mut ssel = Self::ga_slot_sel(slf)?;
        let h = Self::ga_hint(slf)?;
        if !h.is_none() && h.get_item("kind")?.eq("expr")? && getd(node, "type")?.eq(getd(&h.get_item("node")?, "type")?)?
            && getd(node, "name")?.eq(getd(&h.get_item("node")?, "name")?)? {
            ssel = PyTuple::new_bound(py, [h.get_item("owner")?, h.get_item("key")?, h.get_item("slot")?]).into_any(); // ヒントの欄に入れる
        }
        if ssel.is_none() {
            return Self::_status(slf, "先に、入れたい入力欄（白い枠など）を押して選んでください");
        }
        let (owner, key, kind) = (ssel.get_item(0)?, ssel.get_item(1)?, ssel.get_item(2)?);
        let base = ss(&kind)?.split(':').next().unwrap_or("").to_string();
        let vbase = ["num", "bool", "text", "vec"];
        if !vbase.contains(&base.as_str()) || !b.call_method1("fits", (node, &base))?.is_truthy()? {
            let need = match base.as_str() {
                "num" => "数値",
                "bool" => "条件",
                "text" => "文字",
                "vec" => "矢印",
                _ => "選ぶだけ",
            };
            return Self::_status(slf, &format!("この欄（{}）には入れられません", need));
        }
        let nn = deepcopy(node)?;
        let cur = owner.call_method1("get", (&key,))?;
        let children = b.call_method1("expr_children", (&nn,))?;
        let mut nxt: Option<B<'_>> = None;
        let has = children.is_truthy()?;
        let c0k = if has { Some(ss(&children.get_item(0)?.get_item(1)?)?) } else { None };
        let c0_val = c0k.as_deref().map(|k| vbase.contains(&k)).unwrap_or(false);
        let vkc = b.call_method1("value_kind", (&cur,))?;
        if has && c0_val && !vkc.is_none() && b.call_method1("fits", (&cur, c0k.as_deref().unwrap()))?.is_truthy()?
            && !(c0k.as_deref() == Some("text") && vkc.eq("vec")?) {
            nn.set_item(children.get_item(0)?.get_item(0)?, deepcopy(&cur)?)?; // 今の値を左側に
            if children.len()? > 1 {
                let c1 = children.get_item(1)?;
                nxt = Some(PyTuple::new_bound(py, [nn.clone(), c1.get_item(0)?, c1.get_item(1)?]).into_any());
            }
        } else if has && c0_val {
            let c0 = children.get_item(0)?;
            nxt = Some(PyTuple::new_bound(py, [nn.clone(), c0.get_item(0)?, c0.get_item(1)?]).into_any());
        }
        owner.set_item(&key, &nn)?;
        let nxt = match nxt {
            Some(x) if x.is_truthy()? => x,
            _ => PyTuple::new_bound(py, [owner, key, kind]).into_any(),
        };
        Self::sa_slot_sel(slf, nxt)?;
        Self::sa_dirty(slf, true)?;
        Self::sa_discard_armed(slf, false)?;
        Self::build_palette(slf)?;
        Self::render(slf)
    }
    pub(crate) fn select(slf: &Bound<'_, Self>, row: &Bound<'_, Row>) -> R<()> {
        let py = slf.py();
        let r = row.borrow();
        Self::sa_sel(slf, PyTuple::new_bound(py, [r.parent.clone_ref(py), r.index.clone_ref(py), po(py, r.kind.as_str())]))?;
        drop(r);
        Self::render(slf)
    }
    pub(crate) fn _update_title(slf: &Bound<'_, Self>) -> R<()> {
        let d = Self::device(slf)?;
        let title = Self::ga_title(slf)?;
        if d.is_none() {
            title.setattr("text", "コンピューターが失われました")?;
            return Ok(());
        }
        let st = d.get_item("state")?;
        let run = if getd(&st, "running")?.is_truthy()? { "実行中" } else { "停止中" };
        let mark = if Self::ga_dirty(slf)?.is_truthy()? { " ●未保存" } else { "" };
        title.setattr("text", safe(&format!("ブロックプログラム — {}（{}） [{}]  {}{}", ss(&d.get_item("name")?)?, ss(&Self::ga_c(slf)?.getattr("name")?)?,
            Self::file_base(slf)?, run, mark)))?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // 編集
    // ------------------------------------------------------------------
    pub(crate) fn changed(slf: &Bound<'_, Self>) -> R<()> {
        Self::sa_dirty(slf, true)?;
        Self::sa_discard_armed(slf, false)?;
        Self::_refresh_hint(slf, true)?;
        Self::render(slf)
    }
    pub(crate) fn insert(slf: &Bound<'_, Self>, kind: &B<'_>) -> R<()> {
        let py = slf.py();
        let b = bm(py)?;
        let spec = b.getattr("STATEMENTS")?.get_item(kind)?;
        let node = b.call_method1("new_stmt", (kind,))?;
        for it in spec.call_method1("get", ("slots", PyDict::new_bound(py)))?.call_method0("items")?.iter()? {
            let it = it?;
            let (sl, k) = (it.get_item(0)?, ss(&it.get_item(1)?)?);
            if let Some(dk) = k.strip_prefix("dev:") {
                let names = Self::device_names(slf, Some(dk.to_string()))?;
                if names.len() == 1 {
                    node.set_item(&sl, &names[0])?;
                }
            }
        }
        let scripts = Self::ga_prog(slf)?.get_item("scripts")?;
        let h = Self::ga_hint(slf)?;
        if getd(&spec, "hat")?.is_truthy()? {
            scripts.call_method1("append", (&node,))?;
            Self::sa_sel(slf, PyTuple::new_bound(py, [scripts.clone(), po(py, scripts.len()? as i64 - 1).into_bound(py), po(py, "block").into_bound(py)]))?;
        } else if !h.is_none() && h.get_item("kind")?.eq("insert")? && h.get_item("type")?.eq(kind)? {
            h.get_item("list")?.call_method1("insert", (h.get_item("index")?, &node))?; // ヒントの所に入れる
            Self::sa_sel(slf, PyTuple::new_bound(py, [h.get_item("list")?, h.get_item("index")?, po(py, "block").into_bound(py)]))?;
        } else {
            let mut target: Option<(B<'_>, i64)> = None;
            let sel = Self::ga_sel(slf)?;
            if sel.is_truthy()? {
                let (lst, i, k) = (sel.get_item(0)?, sel.get_item(1)?.extract::<i64>()?, ss(&sel.get_item(2)?)?);
                if k == "end" && !lst.is(&scripts) {
                    target = Some((lst, i));
                } else if k == "block" && lst.is(&scripts) && (i as usize) < scripts.len()? {
                    target = Some((scripts.get_item(i)?.get_item("body")?, 0));
                } else if k == "block" && (i as usize) < lst.len()? {
                    target = Some((lst, i + 1));
                }
            }
            let (target, idx) = match target {
                Some(t) => t,
                None => {
                    if scripts.len()? == 0 {
                        scripts.call_method1("append", (b.call_method1("new_stmt", ("on_tick",))?,))?;
                    }
                    let body = scripts.get_item(-1)?.get_item("body")?;
                    let n = body.len()? as i64;
                    (body, n)
                }
            };
            target.call_method1("insert", (idx, &node))?;
            Self::sa_sel(slf, PyTuple::new_bound(py, [target, po(py, idx).into_bound(py), po(py, "block").into_bound(py)]))?;
        }
        Self::_ensure_visible(slf)?;
        Self::changed(slf)
    }
    pub(crate) fn _ensure_visible(slf: &Bound<'_, Self>) -> R<()> {
        let rows = Self::flatten(slf)?;
        let sel = Self::ga_sel(slf)?;
        for (k, r) in rows.iter().enumerate() {
            let r = r.downcast_into::<Row>()?;
            if sel.is_truthy()? && same_sel(&r, &sel)? {
                let k = k as i64;
                let scroll: i64 = Self::ga_scroll(slf)?.extract()?;
                if k < scroll {
                    Self::sa_scroll(slf, k)?;
                } else if k >= scroll + VISIBLE {
                    Self::sa_scroll(slf, k - VISIBLE + 2)?;
                }
                return Ok(());
            }
        }
        Ok(())
    }
    pub(crate) fn _selected_block<'py>(slf: &Bound<'py, Self>) -> R<Option<(B<'py>, i64)>> {
        let sel = Self::ga_sel(slf)?;
        if !sel.is_truthy()? || !sel.get_item(2)?.eq("block")? {
            Self::_status(slf, "ブロックを選んでください")?;
            return Ok(None);
        }
        let (lst, i) = (sel.get_item(0)?, sel.get_item(1)?.extract::<i64>()?);
        Ok(if (i as usize) < lst.len()? { Some((lst, i)) } else { None })
    }
    pub(crate) fn r#move(slf: &Bound<'_, Self>, d: i64) -> R<()> {
        let py = slf.py();
        let Some((lst, i)) = Self::_selected_block(slf)? else { return Ok(()) };
        let j = i + d;
        if 0 <= j && (j as usize) < lst.len()? {
            let (a, b) = (lst.get_item(j)?, lst.get_item(i)?);
            lst.set_item(i, a)?;
            lst.set_item(j, b)?;
            Self::sa_sel(slf, PyTuple::new_bound(py, [lst, po(py, j).into_bound(py), po(py, "block").into_bound(py)]))?;
            Self::changed(slf)?;
        }
        Ok(())
    }
    pub(crate) fn duplicate(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let Some((lst, i)) = Self::_selected_block(slf)? else { return Ok(()) };
        lst.call_method1("insert", (i + 1, deepcopy(&lst.get_item(i)?)?))?;
        Self::sa_sel(slf, PyTuple::new_bound(py, [lst, po(py, i + 1).into_bound(py), po(py, "block").into_bound(py)]))?;
        Self::changed(slf)
    }
    pub(crate) fn delete(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let Some((lst, i)) = Self::_selected_block(slf)? else { return Ok(()) };
        lst.del_item(i)?;
        let n = lst.len()? as i64;
        if n > 0 {
            Self::sa_sel(slf, PyTuple::new_bound(py, [lst, po(py, i.min(n - 1)).into_bound(py), po(py, "block").into_bound(py)]))?;
        } else {
            Self::sa_sel(slf, py.None())?;
        }
        Self::changed(slf)
    }

    // ------------------------------------------------------------------
    // 値の選択ダイアログ
    // ------------------------------------------------------------------
    #[pyo3(signature = (kind=None))]
    pub(crate) fn device_names(slf: &Bound<'_, Self>, kind: Option<String>) -> R<Vec<String>> {
        let py = slf.py();
        let c = Self::ga_c(slf)?;
        let reg = c.getattr("reg")?;
        let behavior_of = imp(py, "main.extensions")?.getattr("behavior_of")?;
        let mut names: Vec<String> = vec![];
        let k = kind.clone().unwrap_or_default();
        let (kind_, action) = match k.split_once('.') {
            Some((a, b)) => (a.to_string(), b.to_string()), // "actuator.rotate"：その動作ができる物だけ
            None => (k.clone(), String::new()),
        };
        for d in Construct::g_devs(con(&c)?)?.iter()? {
            let d = d?;
            if !d.is_truthy()? {
                continue;
            }
            let dd = crate::game::registry::dev_dict(&reg, &d.get_item("def")?)?;
            let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
            if !action.is_empty() && (!getd(&dd, "type")?.eq(&kind_)?
                || !imp(py, "main.actuator")?.call_method1("actions_for", (&d, &reg))?.contains(&action)?) {
                continue;
            }
            if kind.is_none() || k == "*" || getd(&dd, "type")?.eq(&kind_)? || behavior_of.call1((&dd,))?.eq(&kind_)? {
                let n = ss(&d.get_item("name")?)?;
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        Ok(names)
    }
    #[pyo3(signature = (vec=false))]
    pub(crate) fn var_names(slf: &Bound<'_, Self>, vec: bool) -> R<Vec<String>> {
        let py = slf.py();
        let mut found: Vec<B<'_>> = vec![];
        fn walk<'py>(x: &B<'py>, vec: bool, found: &mut Vec<B<'py>>) -> R<()> {
            if let Ok(d) = x.downcast::<PyDict>() {
                let t = getd(x, "type")?;
                let set_t: &[&str] = if vec { &["set_vvar"] } else { &["set_var", "change_var"] };
                if set_t.iter().any(|k| t.eq(*k).unwrap_or(false)) {
                    found.push(getd(x, "var")?);
                }
                if t.eq(if vec { "vvar" } else { "var" })? {
                    found.push(getd(x, "name")?);
                }
                for v in d.values().iter() {
                    walk(&v, vec, found)?;
                }
            } else if let Ok(l) = x.downcast::<PyList>() {
                for v in l.iter() {
                    walk(&v, vec, found)?;
                }
            }
            Ok(())
        }
        walk(&Self::ga_prog(slf)?, vec, &mut found)?;
        for n in bm(py)?.getattr(if vec { "PRESET_VVARS" } else { "PRESET_VARS" })?.iter()? {
            found.push(n?);
        }
        let mut out: Vec<String> = vec![];
        for n in found {
            if n.is_truthy()? {
                let n = ss(&n)?;
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        Ok(out)
    }
}
