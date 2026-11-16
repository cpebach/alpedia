//! ブロックエディター：右の部品の一覧（パレット）

use super::*;

#[pymethods]
impl BlockEditor {
    // ------------------------------------------------------------------
    // 右パネル
    // ------------------------------------------------------------------
    #[pyo3(signature = (parent, text, pos, r#fn, w=0.19, h=0.04, col=None, ts=0.7))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn _btn<'py>(slf: &Bound<'py, Self>, parent: &B<'py>, text: &B<'py>, pos: (f64, f64), r#fn: &B<'py>, w: f64, h: f64, col: Option<&B<'py>>, ts: f64) -> R<B<'py>> {
        btn(slf.as_any(), parent, &ss(text)?, pos, r#fn, w, h, col.filter(|c| c.is_truthy().unwrap_or(false)).cloned(), ts)
    }
    pub(crate) fn _build_right_panel(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let r = Self::ga_root(slf)?;
        let x = 0.665;
        let m = |n: &str| s.getattr(n);
        let c3 = |c: (f64, f64, f64)| rgb(s, c).map(Some);
        btn(s, &r, "このコンピューターの名前を変更", (x, 0.475), &m("open_rename")?, 0.4, 0.035, None, 0.7)?;
        mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, "ファイル名")), ("position", po(py, (x - 0.2, 0.43))), ("origin", po(py, (-0.5, 0.5))),
            ("scale", po(py, 0.7))])?;
        btn(s, &r, "別のファイルを開く ▶", (x + 0.1, 0.422), &m("open_next_file")?, 0.2, 0.032, None, 0.6)?;
        let nf = mk(s, "InputField", vec![("parent", r.clone().unbind()), ("default_value", po(py, "")), ("position", po(py, (x, 0.38))),
            ("scale", po(py, (0.4, 0.04))), ("character_limit", po(py, 30))])?;
        nf.setattr("limit_content_to", NAME_CHARS)?;
        nf.setattr("active", false)?;
        Self::sa_name_field(slf, nf)?;
        btn(s, &r, "保存", (x - 0.1, 0.32), &m("save")?, 0.19, 0.04, c3((0.2, 0.45, 0.3))?, 0.7)?;
        btn(s, &r, "保存して実行", (x + 0.1, 0.32), &m("save_and_run")?, 0.19, 0.04, c3((0.2, 0.5, 0.3))?, 0.7)?;
        btn(s, &r, "停止", (x - 0.1, 0.275), &m("stop")?, 0.19, 0.04, c3((0.5, 0.25, 0.2))?, 0.7)?;
        btn(s, &r, "Python をコピー", (x + 0.1, 0.275), &m("copy_python")?, 0.19, 0.04, None, 0.7)?;
        btn(s, &r, "Python で開く", (x - 0.1, 0.23), &m("open_python")?, 0.19, 0.04, None, 0.7)?;
        btn(s, &r, "閉じる", (x + 0.1, 0.23), &m("close")?, 0.19, 0.04, c3((0.3, 0.3, 0.33))?, 0.7)?;
        mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, "選んだブロック")), ("position", po(py, (x - 0.2, 0.18))), ("origin", po(py, (-0.5, 0.5))),
            ("scale", po(py, 0.7))])?;
        let mv = m("move")?;
        btn(s, &r, "▲ 上へ", (x - 0.1, 0.135), &partial(&mv, vec![po(py, -1)])?, 0.19, 0.04, None, 0.7)?;
        btn(s, &r, "▼ 下へ", (x + 0.1, 0.135), &partial(&mv, vec![po(py, 1)])?, 0.19, 0.04, None, 0.7)?;
        btn(s, &r, "複製", (x - 0.1, 0.09), &m("duplicate")?, 0.19, 0.04, None, 0.7)?;
        btn(s, &r, "削除", (x + 0.1, 0.09), &m("delete")?, 0.19, 0.04, c3((0.5, 0.2, 0.2))?, 0.7)?;
        s.setattr("status", mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, "")), ("position", po(py, (x - 0.2, 0.045))),
            ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.62)), ("color", cn(s, "orange")?.unbind())])?)?;
        Self::sa_info(slf, mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, "")), ("position", po(py, (x - 0.2, -0.04))),
            ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.62))])?)?;
        Self::sa_log_text(slf, mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, "")), ("position", po(py, (x - 0.2, -0.17))),
            ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.58)), ("color", cn(s, "light_gray")?.unbind())])?)?;
        mk(s, "Text", vec![("parent", r.clone().unbind()), ("position", po(py, (-0.87, -0.455))), ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.6)),
            ("color", cn(s, "light_gray")?.unbind()),
            ("text", po(py, "左のブロックを押すと、選んだ行の下（「＋ここに追加」を選べばその中）に入ります。\
                白い枠をクリックすると値やセンサー・計算を選べます。 Ctrl+S 保存 / Ctrl+R 保存して実行 / Esc 閉じる"))])?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // パレット
    // ------------------------------------------------------------------
    pub(crate) fn build_palette(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let pr = Self::ga_palette_root(slf)?;
        let kids: Vec<B<'_>> = pr.getattr("children")?.iter()?.collect::<R<_>>()?;
        for e in kids {
            crate::game::client_misc::ui_destroy(py, &e, &PyTuple::empty_bound(py), None)?;
        }
        let b = bm(py)?;
        let world = Self::ga_world(slf)?;
        let allowed = world.call_method0("block_categories")?; // シナリオでは章ごとに出す種類を絞る
        let mut cats: Vec<B<'_>> = vec![];
        for c in b.getattr("CATEGORIES")?.iter()? {
            let c = c?;
            if allowed.is_none() || allowed.contains(c.get_item(0)?)? {
                cats.push(c);
            }
        }
        let cat0 = Self::ga_cat(slf)?;
        if !cats.is_empty() && !cats.iter().any(|c| c.get_item(0).and_then(|x| x.eq(&cat0)).unwrap_or(false)) {
            Self::sa_cat(slf, cats[0].get_item(0)?)?;
        }
        let cat = Self::ga_cat(slf)?;
        let wp = Self::_hint_palette(slf)?;
        let (want_cat, want_kind, want_node) = (wp.get_item(0)?, wp.get_item(1)?, wp.get_item(2)?);
        let set_cat = s.getattr("_set_cat")?;
        for (k, c) in cats.iter().enumerate() {
            let (cid, name, crgb) = (c.get_item(0)?, c.get_item(1)?, c.get_item(2)?);
            let x = -0.79 + (k % 2) as f64 * 0.17;
            let y = 0.42 - (k / 2) as f64 * 0.042;
            let cc = if cid.eq(&cat)? { rgbo(s, &crgb)? } else { rgb(s, dim3(tup3(&crgb)?, 0.45))? };
            if want_cat.eq(&cid)? && !cid.eq(&cat)? {
                Self::_glow(slf, &pr, x, y, 0.165, 0.036)?;
            }
            btn(s, &pr, &ss(&name)?, (x, y), &partial(&set_cat, vec![cid.unbind()])?, 0.165, 0.036, Some(cc), 0.65)?;
        }
        let mut y = 0.42 - ((cats.len() + 1) / 2) as f64 * 0.042 - 0.024;
        let crgb = b.getattr("CAT_COLOR")?.get_item(&cat)?;
        let insert = s.getattr("insert")?;
        for it in b.getattr("STATEMENTS")?.call_method0("items")?.iter()? {
            let it = it?;
            let (kind, spec) = (it.get_item(0)?, it.get_item(1)?);
            if !spec.get_item("cat")?.eq(&cat)? || getd(&spec, "hidden")?.is_truthy()? || !world.call_method1("block_allowed", (&kind,))?.is_truthy()? {
                continue;
            }
            let mut label = String::new();
            for p in b.call_method1("label_parts", (spec.get_item("label")?,))?.iter()? {
                let p = p?;
                label += &if p.get_item(0)?.eq("slot")? { "□".to_string() } else { ss(&p.get_item(1)?)? };
            }
            if getd(&spec, "hat")?.is_truthy()? {
                label = format!("▶ {}", label);
            }
            if kind.eq(&want_kind)? {
                Self::_glow(slf, &pr, -0.705, y, 0.34, 0.036)?;
            }
            let bt = btn(s, &pr, &label, (-0.705, y), &partial(&insert, vec![kind.unbind()])?, 0.34, 0.036, Some(rgbo(s, &crgb)?), 0.6)?;
            let te = bt.getattr("text_entity")?;
            te.setattr("origin", (-0.5, 0))?;
            te.setattr("x", -0.47)?;
            y -= 0.041;
        }
        // 式ブロック（値・条件）：選んだ入力欄に入る
        let mut items: Vec<(String, B<'_>)> = vec![];
        for e in b.getattr("EXPR_PALETTE")?.iter()? {
            let e = e?;
            if e.get_item(0)?.eq(&cat)? {
                items.push((ss(&e.get_item(1)?)?, e.get_item(2)?));
            }
        }
        if cat.eq("var")? {
            for n in Self::var_names(slf, false)? {
                items.push((n.clone(), dict(py, vec![("type", po(py, "var")), ("name", po(py, n))])?));
            }
        }
        if cat.eq("vec")? {
            for n in Self::var_names(slf, true)? {
                items.push((n.clone(), dict(py, vec![("type", po(py, "vvar")), ("name", po(py, n))])?));
            }
        }
        let mut best: Option<B<'_>> = None;
        if !want_node.is_none() {
            let mut same = vec![];
            for (_, n) in &items {
                if getd(n, "type")?.eq(getd(&want_node, "type")?)? && getd(n, "name")?.eq(getd(&want_node, "name")?)? {
                    same.push(n.clone());
                }
            }
            let mut exact = vec![];
            for n in &same {
                let mut ok = true;
                for f in b.getattr("_MENU_FIELDS")?.iter()? {
                    let f = f?;
                    if want_node.contains(&f)? && !n.call_method1("get", (&f,))?.eq(want_node.call_method1("get", (&f,))?)? {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    exact.push(n.clone());
                }
            }
            best = exact.first().cloned().or_else(|| same.first().cloned());
        }
        if !items.is_empty() {
            mk(s, "Text", vec![("parent", pr.clone().unbind()), ("text", po(py, "値・条件のブロック（選んだ枠に入る）")), ("position", po(py, (-0.87, y + 0.018))),
                ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.55)), ("color", cn(s, "light_gray")?.unbind())])?;
            y -= 0.022;
            let mut x = -0.875;
            for (label, n) in &items {
                let w = if label.chars().count() <= 8 { 0.165 } else { 0.34 };
                if x + w > -0.53 {
                    x = -0.875;
                    y -= 0.04;
                }
                if y < -0.42 {
                    break;
                }
                if best.as_ref().map(|bb| bb.is(n)).unwrap_or(false) {
                    Self::_glow(slf, &pr, x + w / 2.0, y, w, 0.034)?;
                }
                Self::_reporter_btn(slf, &pr, label, n, x + w / 2.0, y, w)?;
                x += w + 0.01;
            }
            y -= 0.03;
        }
        if y > -0.36 {
            mk(s, "Text", vec![("parent", pr.clone().unbind()), ("position", po(py, (-0.87, y - 0.01))), ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.55)),
                ("color", cn(s, "light_gray")?.unbind()), ("text", po(py, "入力欄を押して選んでから、値・条件のブロックを押す。\n入力欄をもう一度押すと数値などを入力"))])?;
        }
        Ok(())
    }
    /// パレットの式ブロック（丸い形。条件は角ばった形）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn _reporter_btn<'py>(slf: &Bound<'py, Self>, parent: &B<'py>, label: &str, node: &B<'py>, x: f64, y: f64, w: f64) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let b = bm(py)?;
        let kind = b.call_method1("value_kind", (node,))?;
        let erg = be_expr_rgb(py, node)?;
        let ssel = Self::ga_slot_sel(slf)?;
        let ok = ssel.is_none() || {
            let base = ss(&ssel.get_item(2)?)?.split(':').next().unwrap_or("").to_string();
            b.call_method1("fits", (node, base))?.is_truthy()?
        };
        let c = if ok { rgbo(s, &erg)? } else { rgb(s, dim3(tup3(&erg)?, 0.4))? };
        let text = if !kind.eq("bool")? { label.replace('□', "○") } else { label.to_string() };
        let radius = if kind.eq("vec")? { Radius::Arrow } else if kind.eq("bool")? { Radius::R(0.18) } else { Radius::R(0.5) };
        let bt = mk(s, "Button", vec![("parent", parent.clone().unbind()), ("text", po(py, safe(&text))), ("position", po(py, (x, y))), ("scale", po(py, (w, 0.034))),
            ("color", c.unbind()), ("text_size", po(py, 0.58)), ("model", shape(py, w / 0.034, radius)?.unbind())])?;
        bt.setattr("on_click", partial(&s.getattr("put_reporter")?, vec![node.clone().unbind()])?)?;
        Ok(bt)
    }
    pub(crate) fn _set_cat(slf: &Bound<'_, Self>, cid: &B<'_>) -> R<()> {
        Self::sa_cat(slf, cid)?;
        Self::build_palette(slf)
    }

    // ------------------------------------------------------------------
    // プログラムの行
    // ------------------------------------------------------------------
    pub(crate) fn flatten<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyList>> {
        let py = slf.py();
        let rows = PyList::empty_bound(py);
        let stm = bm(py)?.getattr("STATEMENTS")?;
        fn body<'py>(py: Python<'py>, rows: &Bound<'py, PyList>, stm: &B<'py>, lst: &B<'py>, depth: i64, owner: &B<'py>) -> R<()> {
            let n_ = lst.len()?;
            for i in 0..n_ {
                let n = lst.get_item(i)?;
                rows.append(row(py, "block", depth, Some(lst), Some(i as i64), Some(&n), None)?)?;
                let spec = stm.call_method1("get", (getd(&n, "type")?, PyDict::new_bound(py)))?;
                if getd(&spec, "body")?.is_truthy()? {
                    body(py, rows, stm, &n.get_item("body")?, depth + 1, &n)?;
                    if getd(&spec, "orelse")?.is_truthy()? {
                        rows.append(row(py, "label", depth, None, None, Some(&n), Some("でなければ"))?)?;
                        body(py, rows, stm, &n.get_item("else")?, depth + 1, &n)?;
                    }
                }
            }
            rows.append(row(py, "end", depth, Some(lst), Some(lst.len()? as i64), Some(owner), None)?)?;
            Ok(())
        }
        let scripts = Self::ga_prog(slf)?.get_item("scripts")?;
        for i in 0..scripts.len()? {
            let sc = scripts.get_item(i)?;
            rows.append(row(py, "block", 0, Some(&scripts), Some(i as i64), Some(&sc), None)?)?;
            body(py, &rows, &stm, &sc.get_item("body")?, 1, &sc)?;
            rows.append(row(py, "gap", 0, None, None, None, None)?)?;
        }
        rows.append(row(py, "end", 0, Some(&scripts), Some(scripts.len()? as i64), Some(&none(py)), Some("＋ 新しいスクリプト（イベントのブロックを押す）"))?)?;
        Ok(rows)
    }
}
