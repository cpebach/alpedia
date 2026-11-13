//! 読み込みメニュー（F8）（main/client/import_ui.py）：設計図（最初から入っている乗り物・艦・ロボット）・OBJ・構築物データを選んで、
//! 大きさや必要な資源を確かめてから置く。
//!
//! 左：読み込めるファイルの一覧（このワールドの imports・exports、ほかのワールドの exports）
//! 右：選んだものの情報。外部の OBJ は大きさ（いちばん長い辺）・細かさ（ボクセルの大きさ）・素材を指定できる。
//!     サバイバルでは必要な資源と手持ちを並べて表示する。
//! OK で目の前に置き、閉じるボタン（または Esc）で何もせず閉じる。

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::imp;
use super::panels::{hook, kw, partial, po};
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

pub const ROWS: i64 = 13;
pub const VOXEL_SIZES: [f64; 3] = [0.25, 0.5, 1.0];
pub const WARN_VOXELS: usize = 20000;

fn lg(py: Python<'_>) -> R<B<'_>> {
    imp(py, "main.log")?.call_method1("get", ("import_ui",))
}
fn objio(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.objio")?.into_any())
}
/// Python の format(v, spec)
fn pf(py: Python<'_>, v: f64, spec: &str) -> R<String> {
    py.import_bound("builtins")?.getattr("format")?.call1((v, spec))?.extract()
}
#[pyfunction]
pub fn iu_fmt_len(py: Python<'_>, x: f64) -> R<String> {
    pf(py, x, if x < 10.0 { ".2f" } else { ".1f" })
}
fn esc(s: &str) -> String {
    s.replace('<', "＜").replace('>', "＞")
}

#[pyclass(module = "main.client.import_ui", subclass, dict)]
pub struct ImportPanel {}

#[pymethods]
impl ImportPanel {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        ImportPanel {}
    }
    fn __init__(slf: &Bound<'_, Self>, game: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let ur = imp(py, "ursina")?;
        let (entity, text, button, camera, color, mouse) = (ur.getattr("Entity")?, ur.getattr("Text")?, ur.getattr("Button")?, ur.getattr("camera")?,
            ur.getattr("color")?, ur.getattr("mouse")?);
        let world = game.getattr("world")?;
        s.setattr("game", game)?;
        s.setattr("world", &world)?;
        s.setattr("reg", world.getattr("registry")?)?;
        let u = PyDict::new_bound(py);
        u.set_item("Entity", &entity)?;
        u.set_item("Text", &text)?;
        u.set_item("Button", &button)?;
        u.set_item("color", &color)?;
        s.setattr("U", u)?;
        mouse.setattr("locked", false)?;
        s.setattr("page", 0)?;
        for k in ["sel", "info", "data", "longest", "_data_key"] {
            s.setattr(k, py.None())?;
        }
        s.setattr("error", "")?;
        s.setattr("status", "")?;
        let vs = world.getattr("settings")?.call_method1("get", ("voxel_size", 0.5))?;
        s.setattr("voxel_size", f64of(&py.import_bound("builtins")?.getattr("float")?.call1((vs,))?)?)?;
        s.setattr("material", "steel")?;

        let root = entity.call((), Some(&kw(py, vec![("parent", camera.getattr("ui")?.unbind()), ("z", po(py, -0.3))])?))?;
        s.setattr("root", &root)?;
        let r_ = || root.clone().unbind();
        let rgba = |r: f64, g: f64, b: f64, a: f64| -> R<PyObject> { Ok(color.call_method1("rgba", (r, g, b, a))?.unbind()) };
        let rgb = |r: f64, g: f64, b: f64| -> R<PyObject> { Ok(color.call_method1("rgb", (r, g, b))?.unbind()) };
        let bg = button.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, (3, 2))), ("color", rgba(0.0, 0.0, 0.0, 0.55)?),
            ("highlight_color", rgba(0.0, 0.0, 0.0, 0.55)?), ("pressed_color", rgba(0.0, 0.0, 0.0, 0.55)?), ("z", po(py, 0.1))])?))?;
        bg.setattr("on_click", imp(py, "alpedia_core")?.getattr("iu_noop")?)?;
        entity.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, (1.56, 0.92))), ("position", po(py, (0, 0, 0.05))),
            ("color", rgb(0.07, 0.08, 0.11)?)])?))?;
        text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "読み込み（設計図・OBJ・構築物）")), ("position", po(py, (-0.75, 0.425))),
            ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 1.3))])?))?;
        s.setattr("list_root", entity.call((), Some(&kw(py, vec![("parent", r_())])?))?)?;
        s.setattr("ctrl_root", entity.call((), Some(&kw(py, vec![("parent", r_())])?))?)?;
        s.setattr("detail", text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (-0.17, 0.36))), ("origin", po(py, (-0.5, 0.5))),
            ("scale", po(py, 0.72)), ("color", color.getattr("light_gray")?.unbind())])?))?)?;
        s.setattr("cost_text", text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (-0.17, -0.2))), ("origin", po(py, (-0.5, 0.5))),
            ("scale", po(py, 0.72))])?))?)?;
        s.setattr("status_text", text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (-0.75, -0.395))),
            ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.7)), ("color", color.getattr("yellow")?.unbind())])?))?)?;
        let m = |n: &str| s.getattr(n);
        s.setattr("ok_btn", Self::_btn(slf, "OK（目の前に置く）", (0.42, -0.4), &m("ok")?, 0.26, 0.042, Some(&rgb(0.2, 0.5, 0.3)?.into_bound(py)), None, 0.7)?)?;
        Self::_btn(slf, "閉じる", (0.66, -0.4), &m("close")?, 0.16, 0.042, Some(&rgb(0.3, 0.3, 0.33)?.into_bound(py)), None, 0.7)?;
        Self::_btn(slf, "imports を開く", (-0.62, -0.335), &m("open_folder")?, 0.2, 0.042, Some(&rgb(0.25, 0.28, 0.35)?.into_bound(py)), None, 0.7)?;
        Self::_btn(slf, "一覧を更新", (-0.4, -0.335), &m("rescan")?, 0.17, 0.042, Some(&rgb(0.25, 0.28, 0.35)?.into_bound(py)), None, 0.7)?;
        s.setattr("keys", hook(py, &root, Some(m("_on_key")?), None)?)?;
        Self::rescan(slf)
    }
    fn _on_key(slf: &Bound<'_, Self>, key: &B<'_>) -> R<()> {
        let page: i64 = slf.as_any().getattr("page")?.extract()?;
        if key.eq("escape")? {
            Self::close(slf)?;
        } else if key.eq("scroll up")? {
            Self::set_page(slf, page - 1)?;
        } else if key.eq("scroll down")? {
            Self::set_page(slf, page + 1)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    #[pyo3(signature = (text, pos, r#fn, w=0.1, h=0.042, col=None, parent=None, ts=0.7))]
    fn _btn<'py>(slf: &Bound<'py, Self>, text: &str, pos: (f64, f64), r#fn: &B<'py>, w: f64, h: f64, col: Option<&B<'py>>, parent: Option<&B<'py>>, ts: f64) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let u = s.getattr("U")?;
        let col = match col {
            Some(c) if !c.is_none() => c.clone(),
            _ => u.get_item("color")?.call_method1("rgb", (0.22, 0.32, 0.45))?,
        };
        let parent = match parent {
            Some(p) if p.is_truthy()? => p.clone(),
            _ => s.getattr("root")?,
        };
        let b = u.get_item("Button")?.call((), Some(&kw(py, vec![("parent", parent.unbind()), ("text", po(py, text)), ("position", po(py, pos)), ("scale", po(py, (w, h))),
            ("color", col.unbind()), ("text_size", po(py, ts))])?))?;
        b.setattr("on_click", r#fn)?;
        Ok(b)
    }
    #[pyo3(signature = (text, pos, parent, scale=0.7, col=None, origin=(-0.5, 0.0)))]
    fn _text<'py>(slf: &Bound<'py, Self>, text: &str, pos: (f64, f64), parent: &B<'py>, scale: f64, col: Option<&B<'py>>, origin: (f64, f64)) -> R<B<'py>> {
        let py = slf.py();
        let u = slf.as_any().getattr("U")?;
        let col = match col {
            Some(c) if !c.is_none() => c.clone(),
            _ => u.get_item("color")?.getattr("light_gray")?,
        };
        u.get_item("Text")?.call((), Some(&kw(py, vec![("parent", parent.clone().unbind()), ("text", po(py, text)), ("position", po(py, pos)), ("origin", po(py, origin)),
            ("scale", po(py, scale)), ("color", col.unbind())])?))
    }
    fn _clear(slf: &Bound<'_, Self>, ent: &B<'_>) -> R<()> {
        let py = slf.py();
        let kids: Vec<B<'_>> = ent.getattr("children")?.iter()?.collect::<R<_>>()?;
        for ch in kids {
            super::client_misc::ui_destroy(py, &ch, &PyTuple::empty_bound(py), None)?;
        }
        Ok(())
    }

    // --- 一覧 -----------------------------------------------------------
    /// 一覧に出すか：設計図は、必要な MOD がこのワールドに入っている物だけ（シナリオの途中は出さない）。
    fn _usable(slf: &Bound<'_, Self>, src: &B<'_>) -> R<bool> {
        let py = slf.py();
        let group: String = src.get_item(0)?.extract()?;
        if !group.starts_with("設計図") {
            return Ok(true);
        }
        let world = slf.as_any().getattr("world")?;
        if !world.call_method0("blueprints_allowed")?.is_truthy()? {
            return Ok(false);
        }
        let meta = match objio(py)?.call_method1("_resolve", (src.get_item(1)?,)).and_then(|r| r.get_item(0)) {
            Ok(m) => m,
            Err(e) if e.is_instance_of::<PyException>(py) => return Ok(true),
            Err(e) => return Err(e),
        };
        let mods = world.getattr("mods")?;
        for r in meta.call_method1("get", ("requires", PyList::empty_bound(py)))?.iter()? {
            let r = r?;
            if r.is_instance_of::<pyo3::types::PyString>() && !mods.contains(&r)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn rescan(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let world = s.getattr("world")?;
        let all = objio(py)?.call_method1("list_sources", (world.getattr("dir")?, imp(py, "main.paths")?.getattr("SAVES")?, world.getattr("mods")?))?;
        let srcs = PyList::empty_bound(py);
        for x in all.iter()? {
            let x = x?;
            if Self::_usable(slf, &x)? {
                srcs.append(x)?;
            }
        }
        let old = s.getattr("sources").ok();
        let sel = s.getattr("sel")?;
        let mut cur: Option<B<'_>> = None;
        if let (false, Some(o)) = (sel.is_none(), old.as_ref()) {
            let k: i64 = sel.extract()?;
            if (k as usize) < o.len()? {
                cur = Some(o.get_item(k)?.get_item(1)?);
            }
        }
        s.setattr("sources", &srcs)?;
        s.setattr("sel", py.None())?;
        if let Some(c) = cur {
            for (k, x) in srcs.iter().enumerate() {
                if x.get_item(1)?.eq(&c)? {
                    s.setattr("sel", k)?;
                }
            }
        }
        if s.getattr("sel")?.is_none() && !srcs.is_empty() {
            Self::select(slf, 0)
        } else {
            Self::build_list(slf)?;
            Self::refresh(slf)
        }
    }
    fn set_page(slf: &Bound<'_, Self>, page: i64) -> R<()> {
        let s = slf.as_any();
        let n = s.getattr("sources")?.len()? as i64;
        let n_pages = 1.max((n + ROWS - 1) / ROWS);
        let page = 0.max((n_pages - 1).min(page));
        let cur: i64 = s.getattr("page")?.extract()?;
        if page != cur {
            s.setattr("page", page)?;
            Self::build_list(slf)?;
        }
        Ok(())
    }
    fn build_list(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let color = s.getattr("U")?.get_item("color")?;
        let lr = s.getattr("list_root")?;
        Self::_clear(slf, &lr)?;
        let (x0, y0) = (-0.5, 0.35);
        let srcs = s.getattr("sources")?;
        let n = srcs.len()? as i64;
        if n == 0 {
            Self::_text(slf, "読み込めるファイルがありません。\n\n\
                saves/<ワールド>/imports に .obj（と .alp.json）を\n置いて「一覧を更新」を押してください。\n\
                F9 で出力した構築物は exports に入り、\nここに自動で出てきます。", (-0.74, 0.3), &lr, 0.68, None, (-0.5, 0.5))?;
            return Ok(());
        }
        let page: i64 = s.getattr("page")?.extract()?;
        let sel = s.getattr("sel")?;
        let sel: Option<i64> = if sel.is_none() { None } else { Some(sel.extract()?) };
        let start = page * ROWS;
        let mut last_group: Option<String> = None;
        let select = s.getattr("select")?;
        for (row, k) in (start..(start + ROWS).min(n)).enumerate() {
            let src = srcs.get_item(k)?;
            let group: String = src.get_item(0)?.extract()?;
            let p = src.get_item(1)?;
            let y = y0 - row as f64 * 0.05;
            let name: String = p.getattr("name")?.extract()?;
            let nc: Vec<char> = name.chars().collect();
            let mut label = if nc.len() <= 30 { name.clone() } else { nc[..28].iter().collect::<String>() + "…" };
            if group.starts_with("設計図") {
                label = Self::_bp_name(slf, &p)?;
            }
            if last_group.as_deref() != Some(group.as_str()) {
                label = format!("[{}] {}", group, label);
                last_group = Some(group);
            }
            let col = if sel == Some(k) { color.call_method1("rgb", (0.3, 0.45, 0.65))? } else { color.call_method1("rgb", (0.16, 0.18, 0.23))? };
            let b = Self::_btn(slf, &label, (x0 - 0.0, y), &partial(&select, vec![po(py, k)])?, 0.5, 0.044, Some(&col), Some(&lr), 0.62)?;
            let te = b.getattr("text_entity")?;
            te.setattr("origin", (-0.5, 0))?;
            te.setattr("x", -0.47)?;
        }
        let n_pages = (n + ROWS - 1) / ROWS;
        if n_pages > 1 {
            let yb = y0 - ROWS as f64 * 0.05;
            let sp = s.getattr("set_page")?;
            Self::_btn(slf, "▲", (-0.66, yb), &partial(&sp, vec![po(py, page - 1)])?, 0.06, 0.042, None, Some(&lr), 0.7)?;
            Self::_text(slf, &format!("{}/{}", page + 1, n_pages), (-0.6, yb), &lr, 0.7, None, (-0.5, 0.0))?;
            Self::_btn(slf, "▼", (-0.34, yb), &partial(&sp, vec![po(py, page + 1)])?, 0.06, 0.042, None, Some(&lr), 0.7)?;
        }
        Ok(())
    }
    /// 設計図の表示名（.alp.json の name）。
    fn _bp_name(slf: &Bound<'_, Self>, p: &B<'_>) -> R<String> {
        let py = slf.py();
        let s = slf.as_any();
        let cache = match s.getattr("_bp_names") {
            Ok(c) => c,
            Err(_) => {
                let d = PyDict::new_bound(py).into_any();
                s.setattr("_bp_names", &d)?;
                d
            }
        };
        if !cache.contains(p)? {
            let r = (|| -> R<B<'_>> {
                let k = PyDict::new_bound(py);
                k.set_item("encoding", "utf-8")?;
                let j = imp(py, "json")?.call_method1("loads", (p.call_method("read_text", (), Some(&k))?,))?;
                let n = j.call_method1("get", ("name",))?;
                Ok(if n.is_truthy()? { n } else { p.getattr("name")? })
            })();
            let v = match r {
                Ok(v) => v,
                Err(e) if e.is_instance_of::<PyException>(py) => p.getattr("name")?,
                Err(e) => return Err(e),
            };
            cache.set_item(p, v)?;
        }
        Ok(cache.get_item(p)?.str()?.to_string())
    }
    fn select(slf: &Bound<'_, Self>, k: i64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("sel", k)?;
        s.setattr("page", k.div_euclid(ROWS))?;
        for a in ["info", "data", "_data_key"] {
            s.setattr(a, py.None())?;
        }
        s.setattr("error", "")?;
        s.setattr("status", "")?;
        let p = s.getattr("sources")?.get_item(k)?.get_item(1)?;
        let reg = s.getattr("reg")?;
        match objio(py)?.call_method1("inspect", (&reg, &p)) {
            Ok(i) => s.setattr("info", i)?,
            Err(e) if e.is_instance_of::<PyException>(py) => {
                let kk = PyDict::new_bound(py);
                kk.set_item("exc_info", e.value_bound(py))?;
                lg(py)?.call_method("exception", ("読み込みの下調べに失敗: %s", &p), Some(&kk))?;
                s.setattr("error", format!("このファイルは読み込めません: {}", e.value_bound(py).str()?))?;
            }
            Err(e) => return Err(e),
        }
        let inf = s.getattr("info")?;
        if inf.is_truthy()? && inf.get_item("kind")?.eq("obj")? {
            let fl = py.import_bound("builtins")?.getattr("float")?;
            let mx = f64of(&fl.call1((inf.get_item("extent")?.call_method0("max")?,))?)?;
            let orig = if mx != 0.0 { mx } else { 1.0 };
            let ivs = inf.get_item("voxel_size")?;
            let vs = if ivs.is_truthy()? { ivs } else { s.getattr("world")?.getattr("settings")?.call_method1("get", ("voxel_size", 0.5))? };
            s.setattr("voxel_size", f64of(&fl.call1((vs,))?)?)?;
            if !inf.get_item("meta")?.call_method1("get", ("scale",))?.is_none() {
                s.setattr("longest", orig * f64of(&inf.get_item("scale")?)?)?;
            } else if (0.5..=40.0).contains(&orig) {
                s.setattr("longest", orig)?;
            } else {
                s.setattr("longest", 8.0)?; // mm 単位で作られた OBJ などはとりあえず 8 m に
            }
            let dm = inf.get_item("default_material")?;
            let key = if dm.is_truthy()? { dm.clone() } else { po(py, "").into_bound(py) };
            let ok = reg.call_method1("get", ("materials", key))?.is_truthy()?;
            s.setattr("material", if ok { dm } else { po(py, "steel").into_bound(py) })?;
        }
        Self::build_list(slf)?;
        Self::build_controls(slf)?;
        Self::refresh(slf)
    }

    // --- 大きさ・素材の指定（外部 OBJ のみ） -------------------------------
    fn build_controls(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let color = s.getattr("U")?.get_item("color")?;
        let r = s.getattr("ctrl_root")?;
        Self::_clear(slf, &r)?;
        let inf = s.getattr("info")?;
        if !inf.is_truthy()? || !inf.get_item("kind")?.eq("obj")? {
            return Ok(());
        }
        let white = color.getattr("white")?;
        let mut y = -0.03;
        Self::_text(slf, "大きさ（いちばん長い辺）", (-0.17, y + 0.045), &r, 0.7, Some(&white), (-0.5, 0.0))?;
        s.setattr("len_text", Self::_text(slf, "", (-0.17, y), &r, 0.8, Some(&white), (-0.5, 0.0))?)?;
        let rs = s.getattr("resize")?;
        let items: [(&str, B<'_>); 5] = [("×½", partial(&rs, vec![po(py, 0.5)])?), ("−10%", partial(&rs, vec![po(py, 1.0 / 1.1)])?),
            ("+10%", partial(&rs, vec![po(py, 1.1)])?), ("×2", partial(&rs, vec![po(py, 2.0)])?), ("元の大きさ", s.getattr("reset_size")?)];
        for (k, (lab, f)) in items.iter().enumerate() {
            let x = 0.0 + k as f64 * 0.085 + if k == 4 { 0.02 } else { 0.0 };
            Self::_btn(slf, lab, (x, y), f, if k < 4 { 0.08 } else { 0.1 }, 0.042, None, Some(&r), 0.7)?;
        }
        y -= 0.065;
        Self::_text(slf, "細かさ（ボクセル）", (-0.17, y), &r, 0.7, Some(&white), (-0.5, 0.0))?;
        let cur: f64 = s.getattr("voxel_size")?.extract()?;
        let sv = s.getattr("set_voxel")?;
        for (k, v) in VOXEL_SIZES.iter().enumerate() {
            let on = (v - cur).abs() < 1e-9;
            let col = if on { color.call_method1("rgb", (0.3, 0.45, 0.65))? } else { color.call_method1("rgb", (0.2, 0.22, 0.28))? };
            Self::_btn(slf, &format!("{} m", pf(py, *v, "g")?), (0.085 + k as f64 * 0.085, y), &partial(&sv, vec![po(py, *v)])?, 0.08, 0.042, Some(&col), Some(&r), 0.7)?;
        }
        y -= 0.06;
        Self::_text(slf, "素材", (-0.17, y), &r, 0.7, Some(&white), (-0.5, 0.0))?;
        let cm = s.getattr("cycle_material")?;
        Self::_btn(slf, "◀", (0.0, y), &partial(&cm, vec![po(py, -1)])?, 0.05, 0.042, None, Some(&r), 0.7)?;
        s.setattr("mat_text", Self::_text(slf, "", (0.035, y), &r, 0.7, Some(&white), (-0.5, 0.0))?)?;
        Self::_btn(slf, "▶", (0.27, y), &partial(&cm, vec![po(py, 1)])?, 0.05, 0.042, None, Some(&r), 0.7)?;
        Ok(())
    }
    fn resize(slf: &Bound<'_, Self>, f: f64) -> R<()> {
        let s = slf.as_any();
        let l: f64 = f64of(&s.getattr("longest")?)?;
        s.setattr("longest", (l * f).clamp(0.2, 500.0))?;
        Self::_changed(slf)
    }
    fn reset_size(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let mx = py.import_bound("builtins")?.getattr("float")?.call1((s.getattr("info")?.get_item("extent")?.call_method0("max")?,))?;
        s.setattr("longest", mx)?;
        Self::_changed(slf)
    }
    fn set_voxel(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        slf.as_any().setattr("voxel_size", v)?;
        Self::build_controls(slf)?;
        Self::_changed(slf)
    }
    fn cycle_material(slf: &Bound<'_, Self>, d: i64) -> R<()> {
        let s = slf.as_any();
        let ids: Vec<B<'_>> = s.getattr("reg")?.getattr("defs")?.get_item("materials")?.iter()?.collect::<R<_>>()?;
        let cur = s.getattr("material")?;
        let mut k = 0i64;
        for (i, m) in ids.iter().enumerate() {
            if m.eq(&cur)? {
                k = i as i64;
                break;
            }
        }
        s.setattr("material", &ids[(k + d).rem_euclid(ids.len() as i64) as usize])?;
        Self::_changed(slf)
    }
    fn _changed(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("data", py.None())?;
        s.setattr("_data_key", py.None())?;
        s.setattr("status", "")?;
        Self::refresh(slf)
    }

    // --- 中身の計算 -------------------------------------------------------
    /// 選んだもの・指定した大きさでボクセルを作る（結果は覚えておく）。
    fn _prepare<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let inf = s.getattr("info")?;
        if !inf.is_truthy()? || s.getattr("error")?.is_truthy()? {
            return Ok(py.None().into_bound(py));
        }
        let p = s.getattr("sources")?.get_item(s.getattr("sel")?)?.get_item(1)?;
        let ps = p.str()?;
        let opts = PyDict::new_bound(py);
        let key: B<'_> = if inf.get_item("kind")?.eq("voxels")? {
            PyTuple::new_bound(py, [po(py, "v"), ps.into_any().unbind()]).into_any()
        } else {
            let fl = py.import_bound("builtins")?.getattr("float")?;
            let mx = f64of(&fl.call1((inf.get_item("extent")?.call_method0("max")?,))?)?;
            let scale = f64of(&s.getattr("longest")?)? / mx.max(1e-9);
            let vs = s.getattr("voxel_size")?;
            let mat = s.getattr("material")?;
            opts.set_item("voxel_size", &vs)?;
            opts.set_item("scale", scale)?;
            opts.set_item("default_material", &mat)?;
            let rd = py.import_bound("builtins")?.getattr("round")?.call1((scale, 6))?;
            PyTuple::new_bound(py, [po(py, "o"), ps.into_any().unbind(), rd.unbind(), vs.unbind(), mat.unbind()]).into_any()
        };
        if !key.eq(s.getattr("_data_key")?)? {
            s.setattr("_data_key", &key)?;
            s.setattr("data", py.None())?;
            match objio(py)?.getattr("import_construct")?.call((s.getattr("reg")?, &p), Some(&opts)) {
                Ok(d) => {
                    s.setattr("data", d)?;
                    s.setattr("_data_err", "")?;
                }
                Err(e) if e.is_instance_of::<PyException>(py) => {
                    lg(py)?.call_method1("warning", ("ボクセル化できません: %s", e.value_bound(py)))?;
                    s.setattr("_data_err", e.value_bound(py).str()?)?;
                }
                Err(e) => return Err(e),
            }
        }
        s.getattr("data")
    }
    fn refresh(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let color = s.getattr("U")?.get_item("color")?;
        let mut lines: Vec<String> = vec![];
        let mut cost_lines: Vec<String> = vec![];
        s.setattr("can_place", false)?;
        let detail = s.getattr("detail")?;
        let srcs = s.getattr("sources")?;
        let reg = s.getattr("reg")?;
        let world = s.getattr("world")?;
        let fmt_len = |x: f64| iu_fmt_len(py, x);
        let matname = |m: &B<'_>| -> R<String> { Ok(reg.call_method1("material", (m,))?.get_item("name")?.str()?.to_string()) };
        if s.getattr("sel")?.is_none() || srcs.len()? == 0 {
            detail.setattr("text", "左の一覧から置くものを選んでください")?;
        } else if s.getattr("error")?.is_truthy()? {
            detail.setattr("text", s.getattr("error")?)?;
        } else {
            let src = srcs.get_item(s.getattr("sel")?)?;
            let (group, p) = (src.get_item(0)?, src.get_item(1)?);
            let inf = s.getattr("info")?;
            let data = Self::_prepare(slf)?;
            lines.push(format!("名前: {}", inf.get_item("name")?.str()?));
            lines.push(format!("場所: {}/{}", group.str()?, p.getattr("name")?.str()?));
            let desc = inf.call_method1("get", ("description",))?;
            if desc.is_truthy()? {
                lines.push(desc.str()?.to_string());
            }
            let mods = world.getattr("mods")?;
            let reqs = inf.call_method1("get", ("requires", PyList::empty_bound(py)))?;
            let mut need = 0;
            for r in reqs.iter()? {
                if !mods.contains(r?)? {
                    need += 1;
                }
            }
            if reqs.is_truthy()? {
                let mm = imp(py, "main.modmeta")?;
                let mut names = vec![];
                for r in reqs.iter()? {
                    names.push(mm.call_method1("name_of", (r?,))?.str()?.to_string());
                }
                lines.push(format!("必要な MOD: {}{}", names.join("、"),
                    if need == 0 { "" } else { "（このワールドに入っていません。ワールド設定の「MOD を選ぶ」で入れてください）" }));
            }
            if inf.get_item("kind")?.eq("voxels")? {
                lines.push("種類: ゲーム内で作った構築物（ボクセル・機器をそのまま再現）".into());
                let n_sc = if data.is_truthy()? {
                    let sc = data.call_method1("get", ("scripts",))?;
                    if sc.is_truthy()? { sc.len()? } else { 0 }
                } else {
                    0
                };
                if n_sc > 0 {
                    lines.push(format!("プログラム {} 個も一緒に入ります（scripts フォルダへ）", n_sc));
                }
            } else {
                let ex = inf.get_item("extent")?;
                let e = |i: usize| -> R<f64> { f64of(&ex.get_item(i)?) };
                lines.push(format!("種類: 外部の OBJ（三角形 {} 個をボクセルに変換）", inf.get_item("n_tris")?.str()?));
                lines.push(format!("元の大きさ: {} × {} × {}（OBJ の単位）", fmt_len(e(0)?)?, fmt_len(e(1)?)?, fmt_len(e(2)?)?));
                let mapping = inf.get_item("mapping")?;
                if mapping.is_truthy()? {
                    let mut parts = vec![];
                    for (i, it) in mapping.call_method0("items")?.iter()?.enumerate() {
                        if i >= 4 {
                            break;
                        }
                        let it = it?;
                        parts.push(format!("{}→{}", it.get_item(0)?.str()?, matname(&it.get_item(1)?)?));
                    }
                    lines.push(format!("素材の対応: {}", parts.join("、")));
                }
                s.getattr("len_text")?.setattr("text", format!("{} m", fmt_len(f64of(&s.getattr("longest")?)?)?))?;
                s.getattr("mat_text")?.setattr("text", matname(&s.getattr("material")?)? + if mapping.is_truthy()? { "（対応表に無い面）" } else { "" })?;
            }
            if data.is_none() {
                lines.push(format!("※ ボクセルにできません: {}", s.getattr("_data_err")?.str()?));
            } else {
                let vox = data.get_item("voxels")?;
                let nv = vox.len()?;
                if nv == 0 {
                    lines.push("※ ボクセルが 0 個です（小さすぎるか、閉じていないメッシュです）".into());
                } else {
                    let mut lo = [f64::INFINITY; 3];
                    let mut hi = [f64::NEG_INFINITY; 3];
                    let mut n_dev = 0;
                    let mats = PyDict::new_bound(py);
                    for v in vox.iter()? {
                        let v = v?;
                        let c = v.get_item(0)?;
                        for i in 0..3 {
                            let x = f64of(&c.get_item(i)?)?;
                            lo[i] = lo[i].min(x);
                            hi[i] = hi[i].max(x);
                        }
                        let d = v.get_item(2)?;
                        if d.is_truthy()? {
                            n_dev += 1;
                        } else {
                            let m = v.get_item(1)?;
                            let n: i64 = match mats.get_item(&m)? { Some(x) => x.extract()?, None => 0 };
                            mats.set_item(&m, n + 1)?;
                        }
                    }
                    let size = f64of(&data.get_item("size")?)?;
                    let ext: Vec<f64> = (0..3).map(|i| (hi[i] - lo[i] + 1.0) * size).collect();
                    lines.push(format!("置いたときの大きさ: {} × {} × {} m（ボクセル {} m）", fmt_len(ext[0])?, fmt_len(ext[1])?, fmt_len(ext[2])?, pf(py, size, "g")?));
                    lines.push(format!("ボクセル {} 個{}", nv, if n_dev > 0 { format!("・機器 {} 個", n_dev) } else { String::new() }));
                    if !mats.is_empty() {
                        let mut top: Vec<(B<'_>, i64)> = vec![];
                        for (k, v) in mats.iter() {
                            top.push((k, v.extract()?));
                        }
                        top.sort_by_key(|kv| -kv.1);
                        let mut parts = vec![];
                        for (m, n) in top.iter().take(5) {
                            parts.push(format!("{} {}", matname(m)?, n));
                        }
                        lines.push(format!("素材: {}", parts.join("、")));
                    }
                    if nv > WARN_VOXELS {
                        lines.push(format!("※ ボクセルが多いので動きが重くなります（{} 個までがおすすめ）", WARN_VOXELS));
                    }
                    let mut can = need == 0;
                    if !world.getattr("creative")?.is_truthy()? {
                        let cost = world.call_method1("voxels_cost", (&vox,))?;
                        let inv = world.getattr("player")?.getattr("inventory")?;
                        cost_lines.push("必要な資源（サバイバル）".into());
                        if !cost.is_truthy()? {
                            cost_lines.push("　なし".into());
                        }
                        for it in cost.call_method0("items")?.iter()? {
                            let it = it?;
                            let (k, v) = (it.get_item(0)?, f64of(&it.get_item(1)?)?);
                            let have = f64of(&inv.call_method1("get", (&k, 0.0))?)?;
                            let ok = have >= v;
                            cost_lines.push(format!("・{} {}（手持ち {}）{}", reg.call_method1("res_name", (&k,))?.str()?, pf(py, v, ".0f")?, pf(py, have, ".0f")?,
                                if ok { "　足りています".to_string() } else { format!("　{} 足りません", pf(py, v - have, ".0f")?) }));
                            if !ok {
                                can = false;
                            }
                        }
                    } else {
                        cost_lines.push("クリエイティブ：資源は要りません".into());
                    }
                    s.setattr("can_place", can)?;
                }
            }
            detail.setattr("text", esc(&lines.join("\n")))?;
        }
        let ct = s.getattr("cost_text")?;
        ct.setattr("text", cost_lines.join("\n"))?;
        ct.setattr("color", color.getattr("white")?)?;
        let can = s.getattr("can_place")?.is_truthy()?;
        s.getattr("ok_btn")?.setattr("color", if can { color.call_method1("rgb", (0.2, 0.5, 0.3))? } else { color.call_method1("rgb", (0.25, 0.3, 0.27))? })?;
        s.getattr("status_text")?.setattr("text", s.getattr("status")?)?;
        Ok(())
    }

    // --- ボタン -----------------------------------------------------------
    fn ok(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        if !s.getattr("can_place")?.is_truthy()? || s.getattr("data")?.is_none() {
            s.setattr("status", "置けません（上の ※ や「足りません」を確認してください）")?;
            return Self::refresh(slf);
        }
        let g = s.getattr("game")?;
        let look = g.call_method0("look_vectors")?.get_item(3)?;
        let world = s.getattr("world")?;
        let k = PyDict::new_bound(slf.py());
        k.set_item("look_dir", look)?;
        let c = world.call_method("place_import", (s.getattr("data")?,), Some(&k))?;
        if c.is_none() {
            let msgs = world.getattr("msgs")?;
            if msgs.is_truthy()? {
                s.setattr("status", msgs.get_item(-1)?.get_item(1)?)?;
            } else {
                s.setattr("status", "置けませんでした")?;
            }
            return Self::refresh(slf);
        }
        Self::close(slf)
    }
    fn open_folder(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let d = s.getattr("world")?.getattr("dir")?.call_method1("__truediv__", ("imports",))?;
        let k = PyDict::new_bound(py);
        k.set_item("exist_ok", true)?;
        d.call_method("mkdir", (), Some(&k))?;
        let os = imp(py, "os")?;
        let r = (|| -> R<()> {
            if os.hasattr("startfile")? {
                os.call_method1("startfile", (d.str()?,))?; // Windows のエクスプローラーで開く
                s.setattr("status", "imports フォルダを開きました。ファイルを置いたら「一覧を更新」を押してください")?;
            } else {
                s.setattr("status", format!("フォルダ: {}", d.str()?))?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => {}
            Err(e) if e.is_instance_of::<PyException>(py) => s.setattr("status", format!("フォルダを開けません: {}", e.value_bound(py).str()?))?,
            Err(e) => return Err(e),
        }
        Self::refresh(slf)
    }
    fn close(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        super::client_misc::ui_destroy(py, &s.getattr("root")?, &PyTuple::empty_bound(py), None)?;
        s.getattr("game")?.call_method0("editor_closed")?;
        Ok(())
    }
}

#[pyfunction]
fn iu_noop() {}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<ImportPanel>()?;
    m.add_function(wrap_pyfunction_bound!(iu_fmt_len, m)?)?;
    m.add_function(wrap_pyfunction_bound!(iu_noop, m)?)?;
    Ok(())
}
