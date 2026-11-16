//! ブロックエディター：プログラムを描く

use super::*;

#[pymethods]
impl BlockEditor {
    pub(crate) fn render(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let rr = Self::ga_rows_root(slf)?;
        let kids: Vec<B<'_>> = rr.getattr("children")?.iter()?.collect::<R<_>>()?;
        for e in kids {
            crate::game::client_misc::ui_destroy(py, &e, &PyTuple::empty_bound(py), None)?;
        }
        let b = bm(py)?;
        let rows = Self::flatten(slf)?;
        Self::sa_rows(slf, &rows)?;
        let n = rows.len() as i64;
        let sc0: i64 = Self::ga_scroll(slf)?.extract()?;
        let scroll = 0.max(sc0.min(0.max(n - VISIBLE)));
        Self::sa_scroll(slf, scroll)?;
        let mut y = AREA_TOP;
        let hint = Self::ga_hint(slf)?;
        let h_ = if hint.is_truthy()? && hint.get_item("kind")?.eq("insert")? { Some(hint) } else { None };
        let sel = Self::ga_sel(slf)?;
        let select = s.getattr("select")?;
        let cat_color = b.getattr("CAT_COLOR")?;
        let stm = b.getattr("STATEMENTS")?;
        let p = |x: PyObject| x;
        for r in rows.iter().skip(scroll as usize).take(VISIBLE as usize) {
            let r = r.downcast_into::<Row>()?;
            let (kind, depth) = (r.borrow().kind.clone(), r.borrow().depth);
            let x0 = AREA_X0 + depth as f64 * 0.035;
            let w = AREA_X1 - x0;
            if let Some(h) = &h_ {
                let rb = r.borrow();
                if (kind == "block" || kind == "end") && rb.parent.bind(py).is(&h.get_item("list")?) && rb.index.bind(py).eq(h.get_item("index")?)? {
                    // ここに入れる：行の上に光る線と印
                    mk(s, "Entity", vec![("parent", rr.clone().unbind()), ("model", po(py, "quad")), ("position", po(py, (x0 + w / 2.0, y + ROW_H * 0.5, -0.03))),
                        ("scale", po(py, (w, 0.005))), ("color", rgb(s, HINT_RGB)?.unbind())])?;
                    mk(s, "Text", vec![("parent", rr.clone().unbind()), ("text", po(py, "◀ ここに入れる")), ("position", po(py, (AREA_X1 - 0.005, y + ROW_H * 0.5, -0.04))),
                        ("origin", po(py, (0.5, 0))), ("scale", po(py, 0.55)), ("color", rgb(s, HINT_RGB)?.unbind())])?;
                }
            }
            let selected = same_sel(&r, &sel)?;
            if kind == "gap" {
                y -= ROW_H * 0.6;
                continue;
            }
            if kind == "end" {
                let t0 = r.borrow().text.clone_ref(py).into_bound(py);
                let txt = if t0.is_truthy()? { ss(&t0)? } else { "＋ ここに追加".to_string() };
                let c = if selected { rgb(s, (0.35, 0.4, 0.5))? } else { rgb(s, (0.12, 0.13, 0.17))? };
                let bt = mk(s, "Button", vec![("parent", rr.clone().unbind()), ("position", po(py, (x0 + w / 2.0, y))), ("scale", po(py, (w, ROW_H * 0.8))),
                    ("color", c.unbind()), ("text", po(py, safe(&txt))), ("text_size", po(py, 0.55))])?;
                let te = bt.getattr("text_entity")?;
                te.setattr("origin", (-0.5, 0))?;
                te.setattr("x", -0.48)?;
                te.setattr("color", cn(s, "light_gray")?)?;
                bt.setattr("on_click", partial(&select, vec![r.clone().into_any().unbind()])?)?;
                y -= ROW_H;
                continue;
            }
            let node = r.borrow().node.clone_ref(py).into_bound(py);
            if kind == "label" {
                let crgb = cat_color.get_item(stm.get_item(node.get_item("type")?)?.get_item("cat")?)?;
                mk(s, "Entity", vec![("parent", rr.clone().unbind()), ("model", po(py, "quad")), ("position", po(py, (x0 + w / 2.0, y))),
                    ("scale", po(py, (w, ROW_H * 0.85))), ("color", rgb(s, dim3(tup3(&crgb)?, 0.8))?.unbind())])?;
                mk(s, "Text", vec![("parent", rr.clone().unbind()), ("text", p(r.borrow().text.clone_ref(py))), ("position", po(py, (x0 + 0.01, y))),
                    ("origin", po(py, (-0.5, 0))), ("scale", po(py, 0.75)), ("z", po(py, -0.01))])?;
                y -= ROW_H;
                continue;
            }
            let t = getd(&node, "type")?;
            let dflt = dict(py, vec![("label", node.call_method1("get", ("type", "?"))?.unbind()), ("cat", po(py, "control"))])?;
            let spec = stm.call_method1("get", (&t, dflt))?;
            let crgb = tup3(&cat_color.get_item(spec.get_item("cat")?)?)?;
            let c = if selected {
                rgb(s, ((crgb.0 * 1.25).min(1.0), (crgb.1 * 1.25).min(1.0), (crgb.2 * 1.25).min(1.0)))?
            } else {
                rgb(s, dim3(crgb, 0.8))?
            };
            let bg = mk(s, "Button", vec![("parent", rr.clone().unbind()), ("position", po(py, (x0 + w / 2.0, y))), ("scale", po(py, (w, ROW_H * 0.88))),
                ("color", c.unbind())])?;
            bg.setattr("on_click", partial(&select, vec![r.clone().into_any().unbind()])?)?;
            if selected {
                mk(s, "Entity", vec![("parent", rr.clone().unbind()), ("model", po(py, "quad")), ("position", po(py, (x0 - 0.006, y))),
                    ("scale", po(py, (0.006, ROW_H * 0.88))), ("color", cn(s, "white")?.unbind())])?;
            }
            let mut x = x0 + 0.01;
            let mut parts: Vec<B<'_>> = b.call_method1("label_parts", (spec.get_item("label")?,))?.iter()?.collect::<R<_>>()?;
            if getd(&spec, "hat")?.is_truthy()? {
                parts.insert(0, PyTuple::new_bound(py, ["text", "▶ "]).into_any());
            }
            for part in parts {
                let (pk, ps) = (part.get_item(0)?, part.get_item(1)?);
                if pk.eq("text")? {
                    let tt = mk(s, "Text", vec![("parent", rr.clone().unbind()), ("text", ps.unbind()), ("position", po(py, (x, y))), ("origin", po(py, (-0.5, 0))),
                        ("scale", po(py, 0.75)), ("z", po(py, -0.01))])?;
                    x += f64of(&tt.getattr("width")?)? * 0.75;
                } else {
                    let sk = b.call_method1("slot_kind", (node.get_item("type")?, &ps))?;
                    x += Self::_slot(slf, &node, &ps, &ss(&sk)?, x, y)? + 0.006;
                }
            }
            y -= ROW_H;
        }
        // スクロール位置
        if n > VISIBLE {
            mk(s, "Text", vec![("parent", rr.clone().unbind()),
                ("text", po(py, format!("{}〜{} / {} 行（ホイールでスクロール）", scroll + 1, n.min(scroll + VISIBLE), n))), ("position", po(py, (AREA_X1, -0.43))),
                ("origin", po(py, (0.5, 0))), ("scale", po(py, 0.55)), ("color", cn(s, "light_gray")?.unbind())])?;
        }
        Self::_update_title(slf)
    }
    /// 文ブロックの入力欄を描く（中の式ブロックも入れ子で）。幅を返す。
    pub(crate) fn _slot(slf: &Bound<'_, Self>, node: &B<'_>, key: &B<'_>, kind: &str, x: f64, y: f64) -> R<f64> {
        Self::_draw_slot(slf, node, key, kind, x, y, 0)
    }
    pub(crate) fn _is_sel(slf: &Bound<'_, Self>, owner: &B<'_>, key: &B<'_>) -> R<bool> {
        let s2 = Self::ga_slot_sel(slf)?;
        Ok(!s2.is_none() && s2.get_item(0)?.is(owner) && s2.get_item(1)?.eq(key)?)
    }
    pub(crate) fn _metrics(&self, depth: i64) -> (f64, f64, f64) {
        metrics(depth)
    }
    #[pyo3(signature = (text, x, y, z, scale, dark=None))]
    pub(crate) fn _label(slf: &Bound<'_, Self>, text: &B<'_>, x: f64, y: f64, z: f64, scale: f64, dark: Option<bool>) -> R<f64> {
        label(slf.as_any(), &ss(text)?, x, y, z, scale, dark.unwrap_or(false))
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn _draw_slot(slf: &Bound<'_, Self>, owner: &B<'_>, key: &B<'_>, kind: &str, x: f64, y: f64, depth: i64) -> R<f64> {
        let py = slf.py();
        let s = slf.as_any();
        let base = kind.split(':').next().unwrap_or("");
        let val = owner.call_method1("get", (key,))?;
        let (h, ts, z) = metrics(depth);
        let sel = Self::_is_sel(slf, owner, key)?;
        if ["var", "vvar", "dev", "key", "choice"].contains(&base) {
            // 選ぶだけの欄（変数・機器・キー・選択）：▼ の付いた丸い札
            let cc = bm(py)?.getattr("CAT_COLOR")?;
            let crgb = match base {
                "var" => tup3(&cc.get_item("var")?)?,
                "vvar" => tup3(&cc.get_item("vec")?)?,
                "dev" => (0.25, 0.50, 0.85), // 機器を選ぶ札は青
                _ => (0.82, 0.85, 0.9),
            };
            let dark = base == "key" || base == "choice";
            let txt = format!("{} ▼", ss(&bm(py)?.call_method1("summary", (&val,))?)?);
            let tw = label(s, &txt, x + 0.008, y, z - 0.003, ts, dark)?;
            let w = tw + 0.016;
            let od = partial(&s.getattr("open_dialog")?, vec![owner.clone().unbind(), key.clone().unbind(), po(py, kind)])?;
            Self::boxx(slf, x, y, w, h, crgb, z, Radius::R(0.5), &od, sel, Self::_hinted(slf, owner, key)?)?;
            return Ok(w);
        }
        Self::_draw_expr(slf, &val, owner, key, kind, x, y, depth)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn _draw_expr(slf: &Bound<'_, Self>, node: &B<'_>, owner: &B<'_>, key: &B<'_>, kind: &str, x: f64, y: f64, depth: i64) -> R<f64> {
        let py = slf.py();
        let s = slf.as_any();
        let b = bm(py)?;
        let (h, ts, z) = metrics(depth);
        let sel = Self::_is_sel(slf, owner, key)?;
        let click = partial(&s.getattr("slot_click")?, vec![owner.clone().unbind(), key.clone().unbind(), po(py, kind)])?;
        let t = if node.is_instance_of::<PyDict>() { getd(node, "type")? } else { none(py) };
        let ts_ = if t.is_instance_of::<PyString>() { Some(ss(&t)?) } else { None };
        let vk = b.call_method1("value_kind", (node,))?;
        let base = kind.split(':').next().unwrap_or("");
        let mut radius = if vk.eq("bool")? || base == "bool" { Radius::R(0.18) } else { Radius::R(0.5) };
        let arrow = vk.eq("vec")? || base == "vec";
        if arrow {
            radius = Radius::Arrow;
        }
        let (pad0, pad1) = if arrow { (h * 0.32, h * 0.42) } else { (0.0, 0.0) }; // 矢印の形：左のくぼみと右の先の分
        let plain = t.is_none() || matches!(ts_.as_deref(), Some("num") | Some("text") | Some("bool"));
        if plain || depth >= 5 {
            // 値（数値・文字・はい/いいえ）：白い入力欄。深すぎる入れ子はまとめて表示
            let txt = if ts_.as_deref() != Some("text") { ss(&b.call_method1("summary", (node,))?)? } else { ss(&node.call_method1("get", ("v", ""))?)? };
            let c: Vec<char> = txt.chars().collect();
            let txt = if c.len() > 30 { c[..29].iter().collect::<String>() + "…" } else { txt };
            let tw = label(s, if txt.is_empty() { " " } else { &txt }, x + 0.008, y, z - 0.003, ts, true)?;
            let w = (tw + 0.016).max(0.03);
            Self::boxx(slf, x, y, w, h, (0.96, 0.96, 0.96), z, radius, &click, sel, Self::_hinted(slf, owner, key)?)?;
            return Ok(w);
        }
        let mut crgb = tup3(&be_expr_rgb(py, node)?)?;
        let outer = s.getattr("_outer_rgb")?;
        if !outer.is_none() && tup3(&outer)? == crgb {
            crgb = be_shade(&po(py, crgb).into_bound(py))?; // 同じ色の入れ子は少し明るく（境目が分かるように）
        }
        if matches!(ts_.as_deref(), Some("var") | Some("sensor") | Some("bsensor") | Some("radio_ok") | Some("vsensor") | Some("vvar")) {
            let sm = ss(&b.call_method1("summary", (node,))?)?;
            let sm = sm.trim_matches(|c| c == '〈' || c == '〉');
            let tw = label(s, sm, x + 0.008 + pad0, y, z - 0.003, ts, false)?;
            let w = tw + 0.016 + pad0 + pad1;
            Self::boxx(slf, x, y, w, h, crgb, z, radius, &click, sel, Self::_hinted(slf, owner, key)?)?;
            return Ok(w);
        }
        // 入れ物（演算・比較・機器の値など）：中の入力欄を順に並べる
        let mut cx = x + 0.007 + pad0;
        let menu_click = s.getattr("menu_click")?;
        for part in be_expr_parts(py, node)?.iter() {
            let pk = ss(&part.get_item(0)?)?;
            if pk == "text" {
                cx += label(s, &ss(&part.get_item(1)?)?, cx, y, z - 0.003, ts, false)? + 0.005;
            } else if pk == "menu" {
                let (mkey, opts, shown) = (part.get_item(1)?, part.get_item(2)?, part.get_item(3)?);
                let (hh, ts2, z2) = metrics(depth + 1);
                let tw = label(s, &format!("{} ▼", ss(&shown)?), cx + 0.006, y, z2 - 0.003, ts2 * 0.92, false)?;
                let mw = tw + 0.012;
                let f = partial(&menu_click, vec![node.clone().unbind(), mkey.clone().unbind(), opts.unbind()])?;
                Self::boxx(slf, cx, y, mw, hh, dim3(crgb, 0.75), z2, Radius::R(0.5), &f, false, Self::_hinted(slf, node, &mkey)?)?;
                cx += mw + 0.005;
            } else {
                let (ck, ckind) = (part.get_item(1)?, part.get_item(2)?);
                let keep = s.getattr("_outer_rgb")?;
                s.setattr("_outer_rgb", crgb)?;
                let r = Self::_draw_slot(slf, node, &ck, &ss(&ckind)?, cx, y, depth + 1);
                s.setattr("_outer_rgb", keep)?;
                cx += r? + 0.005;
            }
        }
        let w = cx - x + 0.002 + pad1;
        Self::boxx(slf, x, y, w, h, crgb, z, radius, &click, sel, Self::_hinted(slf, owner, key)?)?;
        Ok(w)
    }
}
