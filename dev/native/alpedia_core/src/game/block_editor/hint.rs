//! ブロックエディター：キー入力・ヒント・毎フレーム

use super::*;

#[pymethods]
impl BlockEditor {
    pub(crate) fn input(slf: &Bound<'_, Self>, key: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if s.getattr("modal").map(|m| m.is_truthy()).unwrap_or(Ok(false))? {
            return Ok(());
        }
        let combo = imp(py, "ursina")?.getattr("input_handler")?.call_method1("get_combined_key", (key,))?;
        let dialogs = s.getattr("dialogs")?.is_truthy()?;
        if key.eq("escape")? {
            return if dialogs { Self::close_dialog(slf, true) } else { Self::close(slf, false) };
        }
        if dialogs {
            return Ok(());
        }
        if combo.eq("control+s")? {
            Self::save(slf)?;
        } else if combo.eq("control+r")? {
            Self::save_and_run(slf)?;
        } else if key.eq("scroll up")? {
            let sc: i64 = s.getattr("scroll")?.extract()?;
            s.setattr("scroll", 0.max(sc - 2))?;
            Self::render(slf)?;
        } else if key.eq("scroll down")? {
            let sc: i64 = s.getattr("scroll")?.extract()?;
            s.setattr("scroll", sc + 2)?;
            Self::render(slf)?;
        } else if key.eq("delete")? {
            Self::delete(slf)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // ヒント（MOD の見本と比べる）
    // ------------------------------------------------------------------
    pub(crate) fn _hint_key<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let s = slf.as_any();
        Ok(PyTuple::new_bound(py, [ss(&s.getattr("world")?.getattr("dir")?)?, format!("{}.blocks.json", ss(&s.getattr("base")?)?)]))
    }
    /// MOD に見本を聞く（シナリオの課題など。目標が済んだら MOD は None を返す）。
    pub(crate) fn _poll_solution(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let world = s.getattr("world")?;
        let f = format!("{}.blocks.json", ss(&s.getattr("base")?)?);
        let mut sol = none(py);
        for a in world.getattr("addons")?.iter()? {
            let a = a?;
            sol = match a.call_method1("block_hint", (&world, &f)) {
                Ok(x) => x,
                Err(e) if is_exc(py, &e) => {
                    lg(py)?.call_method("exception", ("ヒントの見本を作れません",), Some(&kw(py, vec![("exc_info", e.value_bound(py).clone().into_any().unbind())])?))?;
                    none(py)
                }
                Err(e) => return Err(e),
            };
            if sol.is_truthy()? {
                break;
            }
        }
        s.setattr("_hint_sol", sol)?;
        Ok(())
    }
    #[pyo3(signature = (force=false))]
    pub(crate) fn _refresh_hint(slf: &Bound<'_, Self>, force: bool) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let b = bm(py)?;
        let mut h = none(py);
        let sol = s.getattr("_hint_sol")?;
        if sol.is_truthy()? && f64of(&s.getattr("_hint_time")?.call_method1("get", (Self::_hint_key(slf)?, 0.0))?)? >= HINT_AFTER {
            match b.call_method1("find_hint", (s.getattr("prog")?, &sol)) {
                Ok(x) => h = x,
                Err(e) if is_exc(py, &e) => {
                    lg(py)?.call_method("exception", ("ヒントを探せません",), Some(&kw(py, vec![("exc_info", e.value_bound(py).clone().into_any().unbind())])?))?;
                }
                Err(e) => return Err(e),
            }
        }
        let sig: B<'_> = if h.is_none() {
            none(py)
        } else {
            let id = py.import_bound("builtins")?.getattr("id")?;
            PyTuple::new_bound(py, [h.get_item("kind")?, id.call1((getd(&h, "list")?,))?, getd(&h, "index")?, id.call1((getd(&h, "owner")?,))?, getd(&h, "key")?,
                getd(&h, "type")?, getd(&h, "value")?.str()?.into_any()]).into_any()
        };
        s.setattr("hint", &h)?;
        s.getattr("hint_text")?.setattr("text", be_safe(&b.call_method1("hint_text", (&h,))?)?)?;
        if !sig.eq(s.getattr("_hint_sig")?)? {
            s.setattr("_hint_sig", sig)?;
            Self::build_palette(slf)?;
            if !force {
                Self::render(slf)?;
            }
        }
        Ok(())
    }
    pub(crate) fn _hinted(slf: &Bound<'_, Self>, owner: &B<'_>, key: &B<'_>) -> R<bool> {
        let h = slf.as_any().getattr("hint")?;
        if h.is_none() {
            return Ok(false);
        }
        let k = h.get_item("kind")?;
        Ok((k.eq("expr")? || k.eq("value")?) && h.get_item("owner")?.is(owner) && h.get_item("key")?.eq(key)?)
    }
    /// ヒントで光らせるパレット：(カテゴリ, 文のブロックの種類, 式ブロックのノード)
    pub(crate) fn _hint_palette<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let h = slf.as_any().getattr("hint")?;
        let n3 = || PyTuple::new_bound(py, [py.None(), py.None(), py.None()]);
        if h.is_none() {
            return Ok(n3());
        }
        let b = bm(py)?;
        if h.get_item("kind")?.eq("insert")? {
            let cat = b.getattr("STATEMENTS")?.call_method1("get", (h.get_item("type")?, PyDict::new_bound(py)))?.call_method1("get", ("cat",))?;
            return Ok(PyTuple::new_bound(py, [cat, h.get_item("type")?, none(py)]));
        }
        if h.get_item("kind")?.eq("expr")? {
            let n = h.get_item("node")?;
            let mut cat = none(py);
            for e in b.getattr("EXPR_PALETTE")?.iter()? {
                let e = e?;
                let x = e.get_item(2)?;
                if getd(&x, "type")?.eq(getd(&n, "type")?)? && getd(&x, "name")?.eq(getd(&n, "name")?)? {
                    cat = e.get_item(0)?;
                    break;
                }
            }
            if cat.is_none() {
                let t = getd(&n, "type")?;
                if t.eq("vvar")? {
                    cat = po(py, "vec").into_bound(py);
                } else if t.eq("var")? {
                    cat = po(py, "var").into_bound(py);
                }
            }
            return Ok(PyTuple::new_bound(py, [cat, none(py), n]));
        }
        Ok(n3())
    }
    pub(crate) fn _glow(slf: &Bound<'_, Self>, parent: &B<'_>, x: f64, y: f64, w: f64, h: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        mk(s, "Entity", vec![("parent", parent.clone().unbind()), ("model", shape(py, (w + 0.014) / (h + 0.014), Radius::R(0.3))?.unbind()),
            ("position", po(py, (x, y, 0.01))), ("scale", po(py, (w + 0.014, h + 0.014))), ("color", rgb(s, HINT_RGB)?.unbind())])?;
        Ok(())
    }
    pub(crate) fn update(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let t = now(py)?;
        let last = f64of(&s.getattr("_last_t")?)?;
        let dt = (t - last).min(0.5);
        s.setattr("_last_t", t)?;
        if s.getattr("_hint_sol")?.is_truthy()? {
            let k = Self::_hint_key(slf)?;
            let ht = s.getattr("_hint_time")?;
            let v = f64of(&ht.call_method1("get", (&k, 0.0))?)?;
            ht.set_item(&k, v + dt)?;
        }
        let hp = f64of(&s.getattr("_hint_poll")?)? - dt;
        s.setattr("_hint_poll", hp)?;
        if hp <= 0.0 {
            s.setattr("_hint_poll", 1.0)?;
            Self::_poll_solution(slf)?;
            Self::_refresh_hint(slf, false)?;
        }
        let d = Self::device(slf)?;
        if d.is_none() {
            return Ok(());
        }
        let st = d.get_item("state")?;
        let cs = s.getattr("c")?.getattr("status")?;
        let cs = if cs.is_truthy()? { cs } else { PyDict::new_bound(py).into_any() };
        let stored = f64of(&cs.call_method1("get", ("stored", 0))?)?;
        let mut info = vec![format!("状態: {}", if getd(&st, "running")?.is_truthy()? { "実行中" } else { "停止中" }),
            format!("電力: {} MJ", py.import_bound("builtins")?.getattr("format")?.call1((stored / 1e6, ".2f"))?.extract::<String>()?)];
        let err = getd(&st, "error")?;
        if err.is_truthy()? {
            info.push(format!("エラー: {}", safe(&wrap(&ss(&st.get_item("error")?)?, 30))));
        }
        s.getattr("info")?.setattr("text", info.join("\n"))?;
        let logs = st.call_method1("get", ("_log", PyList::empty_bound(py)))?;
        let n = logs.len()?;
        let mut lines = vec![];
        for (k, l) in logs.iter()?.enumerate() {
            if k + 10 >= n {
                lines.push(wrap(&ss(&l?)?, 38));
            }
        }
        s.getattr("log_text")?.setattr("text", safe(&format!("出力:\n{}", lines.join("\n"))))?;
        let stt = s.getattr("status")?;
        let st_t = s.getattr("_status_t").map(|x| f64of(&x).unwrap_or(0.0)).unwrap_or(0.0);
        if stt.getattr("text")?.is_truthy()? && now(py)? - st_t > 8.0 {
            stt.setattr("text", "")?;
        }
        let tt = (now(py)? * 2.0).trunc() as i64;
        let cur = s.getattr("_tt").map(|x| x.extract::<i64>().unwrap_or(0)).unwrap_or(0);
        if tt != cur {
            s.setattr("_tt", tt)?;
            Self::_update_title(slf)?;
        }
        Ok(())
    }
}
