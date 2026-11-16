//! ブロックエディター：Rust の中だけで使う道具（箱を描く など）

use super::*;

impl BlockEditor {
    /// self._box(...)：角の丸い（または矢印の）押せる箱
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn boxx<'py>(slf: &Bound<'py, Self>, x: f64, y: f64, w: f64, h: f64, c: (f64, f64, f64), z: f64, radius: Radius, on_click: &B<'py>, outline: bool,
                 glow: bool) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let rr = s.getattr("rows_root")?;
        if glow {
            mk(s, "Entity", vec![("parent", rr.clone().unbind()), ("model", shape(py, (w + 0.016) / (h + 0.016), radius)?.unbind()),
                ("position", po(py, (x + w / 2.0, y, z + 0.002))), ("scale", po(py, (w + 0.016, h + 0.016))), ("color", rgb(s, HINT_RGB)?.unbind())])?;
        }
        if outline {
            mk(s, "Entity", vec![("parent", rr.clone().unbind()), ("model", shape(py, (w + 0.008) / (h + 0.008), radius)?.unbind()),
                ("position", po(py, (x + w / 2.0, y, z + 0.0015))), ("scale", po(py, (w + 0.008, h + 0.008))), ("color", rgb(s, (1.0, 0.85, 0.1))?.unbind())])?;
        }
        let hl = ((c.0 * 1.15).min(1.0), (c.1 * 1.15).min(1.0), (c.2 * 1.15).min(1.0));
        let b = mk(s, "Button", vec![("parent", rr.clone().unbind()), ("model", shape(py, w / h, radius)?.unbind()), ("position", po(py, (x + w / 2.0, y, z))),
            ("scale", po(py, (w, h))), ("color", rgb(s, c)?.unbind()), ("highlight_color", rgb(s, hl)?.unbind()), ("pressed_color", rgb(s, c)?.unbind())])?;
        b.setattr("on_click", on_click)?;
        Ok(b)
    }
    /// 種類を順に変えるボタン（演算・比較・関数など）：(表示, 押したときの関数)
    pub(crate) fn op_cycle<'py>(slf: &Bound<'py, Self>, cur: &B<'py>) -> R<Option<(String, B<'py>)>> {
        let py = slf.py();
        let b = bm(py)?;
        let t = getd(cur, "type")?;
        let ts = if t.is_instance_of::<PyString>() { ss(&t)? } else { String::new() };
        let lst = |n: &str| -> R<Vec<B<'py>>> { b.getattr(n)?.iter()?.collect::<R<Vec<_>>>() };
        let (ops, key): (Vec<B<'py>>, &str) = match ts.as_str() {
            "op" => (lst("OPS")?, "op"),
            "cmp" => (lst("CMPS")?, "op"),
            "logic" => (vec![po(py, "and").into_bound(py), po(py, "or").into_bound(py)], "op"),
            "func" => (lst("FUNCS")?, "fn"),
            "devval" => (lst("DEVPROPS")?, "prop"),
            "devbool" => (lst("DEVBOOLS")?, "prop"),
            "vop" => (lst("VOPS")?, "op"),
            "vscale" => (lst("VSCALES")?, "op"),
            "vcomp" => (lst("VCOMPS")?, "axis"),
            "vec" => (lst("VFRAMES")?, "frame"),
            _ => return Ok(None),
        };
        let cv = getd(cur, key)?;
        let shown = if cv.eq("and")? { "かつ".to_string() } else if cv.eq("or")? { "または".to_string() } else { py.import_bound("builtins")?.getattr("format")?.call1((&cv,))?.extract()? };
        let opsk = PyList::new_bound(py, ops).into_any().unbind();
        let (sk, ck, key) = (slf.clone().into_any().unbind(), cur.clone().unbind(), key.to_string());
        let nxt = clo0(py, move |py| {
            let (ops, cur, s) = (opsk.bind(py), ck.bind(py), sk.bind(py));
            let v = getd(cur, &key)?;
            let n = ops.len()? as i64;
            let i = if ops.contains(&v)? { ops.call_method1("index", (&v,))?.extract::<i64>()? } else { -1 };
            cur.set_item(&key, ops.get_item((i + 1).rem_euclid(n))?)?;
            s.setattr("dirty", true)?;
            s.call_method0("render_dialog")?;
            s.call_method0("render")?;
            Ok(())
        })?;
        Ok(Some((shown, nxt)))
    }
}

pub(crate) fn metrics(depth: i64) -> (f64, f64, f64) {
    let d = depth as f64;
    let a = 0.84 - 0.13 * d;
    let h = ROW_H * if 0.46 > a { 0.46 } else { a };
    let b = 0.66 - 0.035 * d;
    let ts = if 0.52 > b { 0.52 } else { b };
    let z = -0.012 - d * 0.006;
    (h, ts, z)
}
/// 文字を置いて、幅を返す
pub(crate) fn label(s: &B<'_>, text: &str, x: f64, y: f64, z: f64, scale: f64, dark: bool) -> R<f64> {
    let py = s.py();
    let c = if dark { rgb(s, (0.08, 0.08, 0.1))? } else { cn(s, "white")? };
    let t = mk(s, "Text", vec![("parent", s.getattr("rows_root")?.unbind()), ("text", po(py, text)), ("position", po(py, (x, y, z))), ("origin", po(py, (-0.5, 0))),
        ("scale", po(py, scale)), ("color", c.unbind())])?;
    Ok(f64of(&t.getattr("width")?)? * scale)
}
/// items を並べる。次の y を返す。
pub(crate) fn grid(s: &B<'_>, r: &B<'_>, items: &[(String, B<'_>, B<'_>)], y: f64, cols: usize, w: f64) -> R<f64> {
    for (k, (label, f, c)) in items.iter().enumerate() {
        let x = -0.45 + (k % cols) as f64 * (w + 0.02);
        let yy = y - (k / cols) as f64 * 0.045;
        btn(s, r, label, (x, yy), f, w, 0.038, Some(c.clone()), 0.62)?;
    }
    let rows = items.len().div_ceil(cols);
    Ok(y - rows as f64 * 0.045 - 0.01)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn input_row(s: &B<'_>, r: &B<'_>, y: f64, label: &str, value: &str, chars: &str, on_ok: &B<'_>) -> R<f64> {
    let py = s.py();
    mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, label)), ("position", po(py, (-0.6, y))), ("origin", po(py, (-0.5, 0))), ("scale", po(py, 0.7))])?;
    let f = mk(s, "InputField", vec![("parent", r.clone().unbind()), ("default_value", po(py, value)), ("position", po(py, (0.02, y))), ("scale", po(py, (0.34, 0.04))),
        ("character_limit", po(py, 40)), ("active", po(py, true))])?;
    f.setattr("limit_content_to", chars)?;
    f.setattr("submit_on", PyList::new_bound(py, ["enter"]))?;
    let (fk, ok) = (f.clone().unbind(), on_ok.clone().unbind());
    let go = clo0(py, move |py| {
        let t = fk.bind(py).getattr("text_field")?.getattr("text")?.call_method0("strip")?;
        ok.bind(py).call1((t,))?;
        Ok(())
    })?;
    f.setattr("on_submit", &go)?;
    btn(s, r, "決定", (0.26, y), &go, 0.1, 0.04, Some(rgb(s, (0.2, 0.45, 0.3))?), 0.7)?;
    Ok(y - 0.055)
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<Row>()?;
    m.add_class::<BlockEditor>()?;
    for f in [wrap_pyfunction_bound!(be_safe, m)?, wrap_pyfunction_bound!(be_to_float, m)?, wrap_pyfunction_bound!(be_wrap, m)?, wrap_pyfunction_bound!(be_dim, m)?,
              wrap_pyfunction_bound!(be_shade, m)?, wrap_pyfunction_bound!(be_round_quad, m)?, wrap_pyfunction_bound!(be_arrow_mesh, m)?,
              wrap_pyfunction_bound!(be_shape, m)?, wrap_pyfunction_bound!(be_expr_rgb, m)?, wrap_pyfunction_bound!(be_expr_parts, m)?] {
        m.add_function(f)?;
    }
    Ok(())
}
