//! コンピューターのブロック（ノード）プログラミング画面（main/client/block_editor.py）。スクラッチのようにブロックを並べて作る。
//!
//! - 左のパレットでブロックを選ぶと、選択中の行の下（帽子ブロックや「ここに追加」を選んでいればその中）に入る
//! - 白い入力欄をクリックすると、数値・センサー・計算・比較・変数などを選べる
//! - 保存すると scripts/<名前>.blocks.json と、変換した <名前>.py ができ、コンピューターは .py を実行する
//! - 「Python をコピー」で変換したコードをクリップボードへ
//!
//! プログラム（ブロックの木）は Python の辞書・リストのまま扱う（選んでいる行・ヒントは、そのリストそのもので覚える）。

use pyo3::exceptions::{PyException, PySyntaxError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};

use super::clo::{clo0, clo1, clokw};
use super::imp;
use super::panels::{hook, kw, partial, po};
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

pub const ROW_H: f64 = 0.037;
pub const VISIBLE: i64 = 22;
pub const HINT_AFTER: f64 = 60.0; // 見本のあるプログラムを、この秒数（開いている時間の合計）作っても完成しなければヒントを出す
pub const HINT_RGB: (f64, f64, f64) = (0.35, 1.0, 0.55);
pub const AREA_X0: f64 = -0.50;
pub const AREA_X1: f64 = 0.44;
pub const AREA_TOP: f64 = 0.42;
pub const NAME_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";

fn bm(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.blocks")?.into_any())
}
fn lg(py: Python<'_>) -> R<B<'_>> {
    imp(py, "main.log")?.call_method1("get", ("blocks",))
}
fn now(py: Python<'_>) -> R<f64> {
    imp(py, "time")?.call_method0("time")?.extract()
}
fn deepcopy<'py>(x: &B<'py>) -> R<B<'py>> {
    imp(x.py(), "copy")?.call_method1("deepcopy", (x,))
}
fn is_exc(py: Python<'_>, e: &PyErr) -> bool {
    e.is_instance_of::<PyException>(py)
}
fn ss(o: &B<'_>) -> R<String> {
    Ok(o.str()?.to_string())
}
fn dict<'py>(py: Python<'py>, pairs: Vec<(&str, PyObject)>) -> R<B<'py>> {
    Ok(kw(py, pairs)?.into_any())
}
fn none(py: Python<'_>) -> B<'_> {
    py.None().into_bound(py)
}
fn getd<'py>(d: &B<'py>, k: &str) -> R<B<'py>> {
    d.call_method1("get", (k,))
}
fn tup3(o: &B<'_>) -> R<(f64, f64, f64)> {
    Ok((f64of(&o.get_item(0)?)?, f64of(&o.get_item(1)?)?, f64of(&o.get_item(2)?)?))
}

/// Ursina の文字装飾タグ（< >）として解釈されないように全角にする。
#[pyfunction]
pub fn be_safe(s: &B<'_>) -> R<String> {
    Ok(ss(s)?.replace('<', "＜").replace('>', "＞"))
}
fn safe(s: &str) -> String {
    s.replace('<', "＜").replace('>', "＞")
}
#[pyfunction]
pub fn be_to_float(py: Python<'_>, s: &B<'_>) -> R<f64> {
    match py.import_bound("builtins")?.getattr("float")?.call1((s,)) {
        Ok(v) => v.extract(),
        Err(e) if e.is_instance_of::<PyValueError>(py) => Ok(0.0),
        Err(e) => Err(e),
    }
}
#[pyfunction]
pub fn be_wrap(s: &B<'_>, n: usize) -> R<String> {
    Ok(wrap(&ss(s)?, n))
}
fn wrap(s: &str, n: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= n {
        return s.to_string();
    }
    c.chunks(n).map(|x| x.iter().collect::<String>()).collect::<Vec<_>>().join("\n")
}
#[pyfunction]
#[pyo3(signature = (c, k=0.6))]
pub fn be_dim(c: &B<'_>, k: f64) -> R<(f64, f64, f64)> {
    let (r, g, b) = tup3(c)?;
    Ok((r * k, g * k, b * k))
}
#[pyfunction]
pub fn be_shade(rgb: &B<'_>) -> R<(f64, f64, f64)> {
    let (r, g, b) = tup3(rgb)?;
    let lum = 0.3 * r + 0.59 * g + 0.11 * b;
    let f = |v: f64| if lum < 0.55 { (v * 1.22 + 0.06).min(1.0) } else { v * 0.8 };
    Ok((f(r), f(g), f(b)))
}
/// 角の丸い四角（大きさに合わせて作る。メッシュは1つの物にしか付けられないので、毎回作る）。
#[pyfunction]
pub fn be_round_quad<'py>(py: Python<'py>, aspect: f64, radius: &B<'py>) -> R<B<'py>> {
    let a: f64 = py.import_bound("builtins")?.getattr("round")?.call1((aspect, 2))?.extract()?;
    let k = kw(py, vec![("radius", radius.clone().unbind()), ("aspect", po(py, a.max(0.2))), ("segments", po(py, 3))])?;
    imp(py, "ursina")?.getattr("Quad")?.call((), Some(&k))
}
/// 矢印の形（左にくぼみ、右がとがる）。幅 aspect・高さ 1 の形を、幅 1 に縮めて作る。
#[pyfunction]
pub fn be_arrow_mesh(py: Python<'_>, aspect: f64) -> R<B<'_>> {
    let a = aspect.max(0.8);
    let t = 0.42;
    let pts = [(-a / 2.0, 0.5), (a / 2.0 - t, 0.5), (a / 2.0, 0.0), (a / 2.0 - t, -0.5), (-a / 2.0, -0.5), (-a / 2.0 + t * 0.75, 0.0)];
    let cx = (-a / 2.0 + t * 0.75 + a / 2.0 - t) / 2.0;
    let vs = PyList::empty_bound(py);
    for k in 0..pts.len() {
        let (p, q) = (pts[k], pts[(k + 1) % pts.len()]);
        for tri in [[(cx, 0.0), p, q], [(cx, 0.0), q, p]] {
            // 両面
            for (x, y) in tri {
                vs.append((x / a, y, 0))?;
            }
        }
    }
    imp(py, "ursina")?.getattr("Mesh")?.call((), Some(&kw(py, vec![("vertices", vs.into_any().unbind()), ("mode", po(py, "triangle"))])?))
}
#[pyfunction]
pub fn be_shape<'py>(py: Python<'py>, aspect: f64, radius: &B<'py>) -> R<B<'py>> {
    if radius.eq("arrow")? {
        be_arrow_mesh(py, aspect)
    } else {
        be_round_quad(py, aspect, radius)
    }
}
fn shape<'py>(py: Python<'py>, aspect: f64, radius: Radius) -> R<B<'py>> {
    match radius {
        Radius::Arrow => be_arrow_mesh(py, aspect),
        Radius::R(r) => be_round_quad(py, aspect, &po(py, r).into_bound(py)),
    }
}
#[derive(Clone, Copy)]
enum Radius {
    Arrow,
    R(f64),
}
#[pyfunction]
pub fn be_expr_rgb<'py>(py: Python<'py>, node: &B<'py>) -> R<B<'py>> {
    let t = if node.is_instance_of::<PyDict>() { getd(node, "type")? } else { none(py) };
    let b = bm(py)?;
    let cc = b.getattr("CAT_COLOR")?;
    let ts = if t.is_none() { String::new() } else { ss(&t)? };
    if (!t.is_none() && b.getattr("VEC_TYPES")?.contains(&t)?) || ["vlen", "vcomp", "vdot", "vangle"].contains(&ts.as_str()) {
        return cc.get_item("vec");
    }
    match ts.as_str() {
        "var" => cc.get_item("var"),
        "sensor" | "bsensor" | "devval" | "devbool" | "key" => cc.get_item("sensor"),
        "radio_val" | "radio_has" | "radio_ok" => cc.get_item("radio"),
        _ => cc.get_item("op"),
    }
}
/// 入れ物の式ブロックの並び：("text", 文字) / ("child", キー, 種類) / ("menu", キー, 選べる物, 表示)
#[pyfunction]
pub fn be_expr_parts<'py>(py: Python<'py>, node: &B<'py>) -> R<Bound<'py, PyList>> {
    let b = bm(py)?;
    let t = getd(node, "type")?;
    let ts = if t.is_instance_of::<PyString>() { ss(&t)? } else { String::new() };
    let out = PyList::empty_bound(py);
    let text = |s: &str| -> R<()> { out.append(("text", s)) };
    let child = |k: &str, kind: &str| -> R<()> { out.append(("child", k, kind)) };
    let lst = |n: &str| -> R<B<'py>> { Ok(PyList::new_bound(py, b.getattr(n)?.iter()?.collect::<R<Vec<_>>>()?).into_any()) };
    let menu = |k: &str, opts: B<'py>, shown: B<'py>| -> R<()> { out.append(("menu", k, opts, shown)) };
    let g = |k: &str, d: &str| -> R<B<'py>> { node.call_method1("get", (k, d)) };
    match ts.as_str() {
        "op" => {
            child("a", "num")?;
            menu("op", lst("OPS")?, g("op", "+")?)?;
            child("b", "num")?;
        }
        "cmp" => {
            child("a", "num")?;
            menu("op", lst("CMPS")?, g("op", "=")?)?;
            child("b", "num")?;
        }
        "logic" => {
            child("a", "bool")?;
            let shown = if getd(node, "op")?.eq("and")? { "かつ" } else { "または" };
            menu("op", PyList::new_bound(py, ["and", "or"]).into_any(), po(py, shown).into_bound(py))?;
            child("b", "bool")?;
        }
        "not" => {
            child("a", "bool")?;
            text("ではない")?;
        }
        "func" => {
            menu("fn", lst("FUNCS")?, g("fn", "絶対値")?)?;
            child("a", "num")?;
        }
        "random" => {
            child("a", "num")?;
            text("から")?;
            child("b", "num")?;
            text("の乱数")?;
        }
        "join" => {
            child("a", "text")?;
            text("と")?;
            child("b", "text")?;
        }
        "devval" => {
            child("dev", "dev:*")?;
            text("の")?;
            menu("prop", lst("DEVPROPS")?, g("prop", "角度")?)?;
        }
        "devbool" => {
            child("dev", "dev:*")?;
            text("が")?;
            menu("prop", lst("DEVBOOLS")?, g("prop", "動作中")?)?;
        }
        "key" => {
            text("キー")?;
            child("key", "key")?;
            text("が押されている")?;
        }
        "radio_val" => {
            text("チャンネル")?;
            child("ch", "text")?;
            text("の受信値")?;
        }
        "radio_has" => {
            text("チャンネル")?;
            child("ch", "text")?;
            text("に届いている")?;
        }
        "vec" => {
            menu("frame", lst("VFRAMES")?, g("frame", "ワールド")?)?;
            text("矢印")?;
            child("x", "num")?;
            child("y", "num")?;
            child("z", "num")?;
        }
        "vto" => {
            text("地点")?;
            child("x", "num")?;
            child("y", "num")?;
            child("z", "num")?;
            text("への矢印")?;
        }
        "vop" => {
            child("a", "vec")?;
            menu("op", lst("VOPS")?, g("op", "+")?)?;
            child("b", "vec")?;
        }
        "vscale" => {
            child("a", "vec")?;
            menu("op", lst("VSCALES")?, g("op", "×")?)?;
            child("b", "num")?;
        }
        "vnorm" => {
            child("a", "vec")?;
            text("の向き")?;
        }
        "vclamp" => {
            child("a", "vec")?;
            text("の長さを")?;
            child("b", "num")?;
            text("までに")?;
        }
        "vperp" => {
            child("a", "vec")?;
            text("の")?;
            child("b", "vec")?;
            text("に垂直な部分")?;
        }
        "vtilt" => {
            child("a", "vec")?;
            text("を")?;
            child("b", "vec")?;
            text("から")?;
            child("n", "num")?;
            text("度以内に")?;
        }
        "vlen" => {
            child("a", "vec")?;
            text("の長さ")?;
        }
        "vcomp" => {
            child("a", "vec")?;
            text("の")?;
            menu("axis", lst("VCOMPS")?, g("axis", "前")?)?;
            text("成分")?;
        }
        "vdot" => {
            child("a", "vec")?;
            text("と")?;
            child("b", "vec")?;
            text("の内積")?;
        }
        "vangle" => {
            child("a", "vec")?;
            text("と")?;
            child("b", "vec")?;
            text("の角度")?;
        }
        _ => {
            out.append(("text", b.call_method1("summary", (node,))?))?;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- Row
#[pyclass(module = "main.client.block_editor", subclass, dict)]
pub struct Row {
    #[pyo3(get, set)]
    kind: String,
    #[pyo3(get, set)]
    depth: i64,
    #[pyo3(get, set)]
    parent: PyObject,
    #[pyo3(get, set)]
    index: PyObject,
    #[pyo3(get, set)]
    node: PyObject,
    #[pyo3(get, set)]
    text: PyObject,
}
#[pymethods]
impl Row {
    #[new]
    #[pyo3(signature = (kind, depth, parent=None, index=None, node=None, text=None))]
    fn new(py: Python<'_>, kind: String, depth: i64, parent: Option<PyObject>, index: Option<PyObject>, node: Option<PyObject>, text: Option<PyObject>) -> Self {
        Row { kind, depth, parent: parent.unwrap_or_else(|| py.None()), index: index.unwrap_or_else(|| py.None()), node: node.unwrap_or_else(|| py.None()),
            text: text.unwrap_or_else(|| po(py, "")) }
    }
}
fn row<'py>(py: Python<'py>, kind: &str, depth: i64, parent: Option<&B<'py>>, index: Option<i64>, node: Option<&B<'py>>, text: Option<&str>) -> R<Bound<'py, Row>> {
    Bound::new(py, Row { kind: kind.into(), depth, parent: parent.map(|p| p.clone().unbind()).unwrap_or_else(|| py.None()),
        index: index.map(|i| po(py, i)).unwrap_or_else(|| py.None()), node: node.map(|n| n.clone().unbind()).unwrap_or_else(|| py.None()),
        text: po(py, text.unwrap_or("")) })
}

// ---------------------------------------------------------------- BlockEditor
#[pyclass(module = "main.client.block_editor", subclass, dict)]
pub struct BlockEditor {}

fn col<'py>(s: &B<'py>) -> R<B<'py>> {
    s.getattr("U")?.get_item("color")
}
fn rgb<'py>(s: &B<'py>, c: (f64, f64, f64)) -> R<B<'py>> {
    col(s)?.call_method1("rgb", c)
}
fn rgbo<'py>(s: &B<'py>, c: &B<'py>) -> R<B<'py>> {
    rgb(s, tup3(c)?)
}
fn cn<'py>(s: &B<'py>, n: &str) -> R<B<'py>> {
    col(s)?.getattr(n)
}
fn mk<'py>(s: &B<'py>, cls: &str, pairs: Vec<(&str, PyObject)>) -> R<B<'py>> {
    let py = s.py();
    s.getattr("U")?.get_item(cls)?.call((), Some(&kw(py, pairs)?))
}
fn dim3(c: (f64, f64, f64), k: f64) -> (f64, f64, f64) {
    (c.0 * k, c.1 * k, c.2 * k)
}
/// self._btn(parent, text, pos, fn, w, h, col, ts)
#[allow(clippy::too_many_arguments)]
fn btn<'py>(s: &B<'py>, parent: &B<'py>, text: &str, pos: (f64, f64), f: &B<'py>, w: f64, h: f64, c: Option<B<'py>>, ts: f64) -> R<B<'py>> {
    let py = s.py();
    let c = match c {
        Some(c) => c,
        None => rgb(s, (0.22, 0.32, 0.45))?,
    };
    let b = mk(s, "Button", vec![("parent", parent.clone().unbind()), ("text", po(py, safe(text))), ("position", po(py, pos)), ("scale", po(py, (w, h))),
        ("color", c.unbind()), ("text_size", po(py, ts))])?;
    b.setattr("on_click", f)?;
    Ok(b)
}
fn same_sel(r: &Bound<'_, Row>, sel: &B<'_>) -> R<bool> {
    if sel.is_none() {
        return Ok(false);
    }
    let py = r.py();
    let rr = r.borrow();
    Ok(rr.parent.bind(py).is(&sel.get_item(0)?) && rr.index.bind(py).eq(sel.get_item(1)?)? && sel.get_item(2)?.eq(&rr.kind)?)
}

#[pymethods]
impl BlockEditor {
    #[classattr]
    fn _outer_rgb(py: Python<'_>) -> PyObject {
        py.None() // 入れ子を描いている間の、外側の式ブロックの色
    }
    #[classattr]
    fn _hint_time(py: Python<'_>) -> PyObject {
        PyDict::new_bound(py).into_any().unbind() // (ワールドのフォルダ, ファイル) → 見本のあるプログラムを開いていた秒数の合計
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        BlockEditor {}
    }
    fn __init__(slf: &Bound<'_, Self>, game: &B<'_>, construct: &B<'_>, index: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let ur = imp(py, "ursina")?;
        let (entity, text, color) = (ur.getattr("Entity")?, ur.getattr("Text")?, ur.getattr("color")?);
        s.setattr("game", game)?;
        s.setattr("world", game.getattr("world")?)?;
        s.setattr("c", construct)?;
        s.setattr("cell", construct.getattr("cells")?.get_item(index)?)?;
        let tk = text.clone().unbind();
        let safe_text = clokw(py, move |_a, k| {
            let py = _a.py();
            let k = match k {
                Some(k) => k.copy()?,
                None => PyDict::new_bound(py),
            };
            let t = k.get_item("text")?.map(|x| x.unbind()).unwrap_or_else(|| po(py, ""));
            k.set_item("text", be_safe(t.bind(py))?)?;
            Ok(tk.bind(py).call((), Some(&k))?.unbind())
        })?;
        let u = PyDict::new_bound(py);
        u.set_item("Entity", &entity)?;
        u.set_item("Text", safe_text)?;
        u.set_item("Button", ur.getattr("Button")?)?;
        u.set_item("InputField", ur.getattr("InputField")?)?;
        u.set_item("color", &color)?;
        s.setattr("U", u)?;
        ur.getattr("mouse")?.setattr("locked", false)?;
        s.setattr("cat", "event")?;
        s.setattr("scroll", 0)?;
        s.setattr("sel", py.None())?; // (parent list, index, kind)
        s.setattr("slot_sel", py.None())?; // 選んだ入力欄 (持ち主のノード, キー, 種類)
        s.setattr("dirty", false)?;
        s.setattr("discard_armed", false)?;
        s.setattr("dialogs", PyList::empty_bound(py))?;
        s.setattr("hint", py.None())?; // ヒント（blocks.find_hint の結果）
        s.setattr("_hint_sig", py.None())?;
        s.setattr("_hint_sol", py.None())?;
        s.setattr("_hint_poll", 0.0)?;
        s.setattr("_last_t", now(py)?)?;
        let root = entity.call((), Some(&kw(py, vec![("parent", ur.getattr("camera")?.getattr("ui")?.unbind())])?))?;
        s.setattr("root", &root)?;
        let r_ = || root.clone().unbind();
        entity.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, (3, 2))),
            ("color", color.call_method1("rgba", (0.03, 0.035, 0.05, 0.95))?.unbind()), ("z", po(py, 0.5))])?))?;
        entity.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("position", po(py, ((AREA_X0 + AREA_X1) / 2.0, 0.02, 0.4))),
            ("scale", po(py, (AREA_X1 - AREA_X0 + 0.02, 0.86))), ("color", color.call_method1("rgb", (0.08, 0.09, 0.12))?.unbind())])?))?;
        s.setattr("title", text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (-0.87, 0.485))),
            ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 1.0))])?))?)?;
        s.setattr("palette_root", entity.call((), Some(&kw(py, vec![("parent", r_())])?))?)?;
        s.setattr("rows_root", entity.call((), Some(&kw(py, vec![("parent", r_())])?))?)?;
        s.setattr("dialog_root", entity.call((), Some(&kw(py, vec![("parent", r_()), ("z", po(py, -0.2))])?))?)?;
        Self::_build_right_panel(slf)?;
        s.setattr("hint_text", text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (AREA_X0, -0.398))),
            ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.62)), ("color", color.call_method1("rgb", HINT_RGB)?.unbind())])?))?)?;
        Self::_load(slf)?;
        Self::_poll_solution(slf)?;
        s.setattr("keys", hook(py, &root, Some(s.getattr("input")?), Some(s.getattr("update")?))?)?;
        Self::build_palette(slf)?;
        Self::render(slf)
    }

    // ------------------------------------------------------------------
    // 読み込み・保存
    // ------------------------------------------------------------------
    #[getter]
    fn index<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let s = slf.as_any();
        s.getattr("c")?.getattr("index")?.call_method1("get", (s.getattr("cell")?,))
    }
    fn device<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let i = Self::index(slf)?;
        if i.is_none() {
            return Ok(none(py));
        }
        slf.as_any().getattr("c")?.getattr("devs")?.get_item(i)
    }
    fn scripts_dir<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let d = slf.as_any().getattr("world")?.getattr("dir")?.call_method1("__truediv__", ("scripts",))?;
        d.call_method("mkdir", (), Some(&kw(py, vec![("exist_ok", po(py, true))])?))?;
        Ok(d)
    }
    fn _load(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let b = bm(py)?;
        let d = Self::device(slf)?;
        let dd = if d.is_truthy()? { d.clone() } else { PyDict::new_bound(py).into_any() };
        let name = dd.call_method1("get", ("state", PyDict::new_bound(py)))?.call_method1("get", ("blocks", ""))?;
        s.setattr("prog", py.None())?;
        if name.is_truthy()? {
            let nm = ss(&name)?;
            match b.call_method1("load", (Self::scripts_dir(slf)?.call_method1("__truediv__", (&name,))?,)) {
                Ok(p) => {
                    s.setattr("prog", p)?;
                    let base: String = nm.chars().take(nm.chars().count().saturating_sub(".blocks.json".chars().count())).collect();
                    s.setattr("base", base)?;
                }
                Err(e) if is_exc(py, &e) => Self::_status(slf, &format!("{} を読めません: {}", nm, ss(e.value_bound(py))?))?,
                Err(e) => return Err(e),
            }
        }
        if s.getattr("prog")?.is_none() {
            let prog = b.call_method0("new_program")?;
            let sc = prog.get_item("scripts")?;
            sc.call_method1("append", (b.call_method1("new_stmt", ("on_start",))?,))?;
            sc.call_method1("append", (b.call_method1("new_stmt", ("on_tick",))?,))?;
            s.setattr("prog", prog)?;
            let mut k = 1;
            while Self::scripts_dir(slf)?.call_method1("__truediv__", (format!("blocks{}.blocks.json", k),))?.call_method0("exists")?.is_truthy()? {
                k += 1;
            }
            s.setattr("base", format!("blocks{}", k))?;
            s.setattr("dirty", true)?;
            if d.is_truthy()? {
                let sc = d.get_item("state")?.call_method1("get", ("script",))?;
                if sc.is_truthy()? {
                    Self::_status(slf, &format!("新しいブロックプログラムです（今の {} は変換されません）", ss(&d.get_item("state")?.get_item("script")?)?))?;
                }
            }
        }
        let tf = s.getattr("name_field")?.getattr("text_field")?;
        tf.setattr("text", s.getattr("base")?)?;
        tf.call_method0("render")?;
        Ok(())
    }
    /// scripts/ にあるブロックプログラムを順に開く（保存するとこのコンピューターに設定される）。
    fn open_next_file(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let mut files: Vec<String> = vec![];
        for p in Self::scripts_dir(slf)?.call_method1("glob", ("*.blocks.json",))?.iter()? {
            files.push(p?.getattr("name")?.extract()?);
        }
        files.sort();
        if files.is_empty() {
            return Self::_status(slf, "scripts/ にブロックプログラムがありません");
        }
        let cur = format!("{}.blocks.json", ss(&s.getattr("base")?)?);
        let nxt = match files.iter().position(|f| *f == cur) {
            Some(i) => files[(i + 1) % files.len()].clone(),
            None => files[0].clone(),
        };
        match bm(py)?.call_method1("load", (Self::scripts_dir(slf)?.call_method1("__truediv__", (&nxt,))?,)) {
            Ok(p) => s.setattr("prog", p)?,
            Err(e) if is_exc(py, &e) => return Self::_status(slf, &format!("{} を読めません: {}", nxt, ss(e.value_bound(py))?)),
            Err(e) => return Err(e),
        }
        let base: String = nxt.chars().take(nxt.chars().count() - ".blocks.json".chars().count()).collect();
        s.setattr("base", &base)?;
        let tf = s.getattr("name_field")?.getattr("text_field")?;
        tf.setattr("text", &base)?;
        tf.call_method0("render")?;
        s.setattr("sel", py.None())?;
        s.setattr("scroll", 0)?;
        let d = Self::device(slf)?;
        let dirty = !(d.is_truthy()? && d.get_item("state")?.call_method1("get", ("blocks",))?.eq(&nxt)?);
        s.setattr("dirty", dirty)?;
        Self::_status(slf, &format!("{} を開きました{}", nxt, if dirty { "（保存するとこのコンピューターに設定されます）" } else { "" }))?;
        Self::render(slf)
    }
    fn file_base(slf: &Bound<'_, Self>) -> R<String> {
        let t = slf.as_any().getattr("name_field")?.getattr("text_field")?.getattr("text")?.call_method0("strip")?;
        let n: String = ss(&t)?.chars().filter(|c| NAME_CHARS.contains(*c)).collect();
        Ok(if n.is_empty() { "blocks".into() } else { n })
    }
    fn code<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        bm(py)?.call_method1("to_python", (slf.as_any().getattr("prog")?, format!("{}.blocks.json", Self::file_base(slf)?)))
    }
    fn save(slf: &Bound<'_, Self>) -> R<bool> {
        let py = slf.py();
        let s = slf.as_any();
        let d = Self::device(slf)?;
        if d.is_none() {
            Self::_status(slf, "コンピューターが見つかりません")?;
            return Ok(false);
        }
        let base = Self::file_base(slf)?;
        let world = s.getattr("world")?;
        let pyp = match imp(py, "main.scripting")?.call_method1("script_path", (world.getattr("dir")?, format!("{}.py", base))) {
            Ok(p) => p,
            Err(e) if e.is_instance_of::<PyValueError>(py) => {
                Self::_status(slf, &ss(e.value_bound(py))?)?;
                return Ok(false);
            }
            Err(e) => return Err(e),
        };
        let code = Self::code(slf)?;
        if let Err(e) = py.import_bound("builtins")?.getattr("compile")?.call1((&code, pyp.getattr("name")?, "exec")) {
            if e.is_instance_of::<PySyntaxError>(py) {
                // 変換の不具合（通常は起きない）
                Self::_status(slf, &format!("変換エラー: {}", ss(e.value_bound(py))?))?;
                return Ok(false);
            }
            return Err(e);
        }
        bm(py)?.call_method1("save", (Self::scripts_dir(slf)?.call_method1("__truediv__", (format!("{}.blocks.json", base),))?, s.getattr("prog")?))?;
        pyp.call_method("write_text", (&code,), Some(&kw(py, vec![("encoding", po(py, "utf-8"))])?))?;
        world.getattr("runner")?.call_method1("invalidate", (pyp.getattr("name")?,))?;
        let st = d.get_item("state")?;
        st.set_item("script", pyp.getattr("name")?)?;
        st.set_item("blocks", format!("{}.blocks.json", base))?;
        for k in ["_env", "error", "error_line"] {
            st.call_method1("pop", (k, py.None()))?;
        }
        s.setattr("base", &base)?;
        s.setattr("dirty", false)?;
        Self::_status(slf, &format!("{}.blocks.json と {} を保存しました", base, ss(&pyp.getattr("name")?)?))?;
        lg(py)?.call_method1("info", ("ブロックプログラム保存: %s", &base))?;
        Ok(true)
    }
    fn save_and_run(slf: &Bound<'_, Self>) -> R<()> {
        if Self::save(slf)? {
            Self::device(slf)?.get_item("state")?.set_item("running", true)?;
            Self::_status(slf, "保存して実行しました")?;
        }
        Ok(())
    }
    fn stop(slf: &Bound<'_, Self>) -> R<()> {
        let d = Self::device(slf)?;
        if d.is_truthy()? {
            d.get_item("state")?.set_item("running", false)?;
            Self::_status(slf, "停止しました")?;
        }
        Ok(())
    }
    fn copy_python(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let code = Self::code(slf)?;
        let r = (|| -> R<()> {
            imp(py, "pyperclip")?.call_method1("copy", (&code,))?;
            let n = code.call_method0("splitlines")?.len()?;
            Self::_status(slf, &format!("Python コード（{} 行）をクリップボードにコピーしました", n))
        })();
        match r {
            Ok(()) => Ok(()),
            Err(e) if is_exc(py, &e) => {
                lg(py)?.call_method1("warning", ("クリップボードにコピーできません: %s", e.value_bound(py)))?;
                Self::_status(slf, "クリップボードにコピーできませんでした（保存すると scripts/ に .py ができます）")
            }
            Err(e) => Err(e),
        }
    }
    fn open_python(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if s.getattr("dirty")?.is_truthy()? && !Self::save(slf)? {
            return Ok(());
        }
        let (game, c, i) = (s.getattr("game")?, s.getattr("c")?, Self::index(slf)?);
        Self::close(slf, true)?;
        game.getattr("U")?.get_item("mouse")?.setattr("locked", false)?;
        game.getattr("ui_root")?.setattr("enabled", false)?;
        let ed = imp(py, "main.client.script_panel")?.getattr("ScriptPanel")?.call1((&game, c, i))?;
        game.setattr("editor", &ed)?;
        ed.call_method1("set_status", ("ブロックから作ったコードです（「エディタで開く」で見られます。「Python モードにする」で Python として使えます）",))?;
        Ok(())
    }
    #[pyo3(signature = (force=false))]
    fn close(slf: &Bound<'_, Self>, force: bool) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if s.getattr("dirty")?.is_truthy()? && !force && !s.getattr("discard_armed")?.is_truthy()? {
            s.setattr("discard_armed", true)?;
            return Self::_status(slf, "未保存の変更があります。保存するか、もう一度 Esc で破棄して閉じます");
        }
        super::client_misc::ui_destroy(py, &s.getattr("root")?, &PyTuple::empty_bound(py), None)?;
        s.getattr("game")?.call_method0("editor_closed")?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // 右パネル
    // ------------------------------------------------------------------
    #[pyo3(signature = (parent, text, pos, r#fn, w=0.19, h=0.04, col=None, ts=0.7))]
    #[allow(clippy::too_many_arguments)]
    fn _btn<'py>(slf: &Bound<'py, Self>, parent: &B<'py>, text: &B<'py>, pos: (f64, f64), r#fn: &B<'py>, w: f64, h: f64, col: Option<&B<'py>>, ts: f64) -> R<B<'py>> {
        btn(slf.as_any(), parent, &ss(text)?, pos, r#fn, w, h, col.filter(|c| c.is_truthy().unwrap_or(false)).cloned(), ts)
    }
    fn _build_right_panel(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let r = s.getattr("root")?;
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
        s.setattr("name_field", nf)?;
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
        s.setattr("info", mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, "")), ("position", po(py, (x - 0.2, -0.04))),
            ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.62))])?)?;
        s.setattr("log_text", mk(s, "Text", vec![("parent", r.clone().unbind()), ("text", po(py, "")), ("position", po(py, (x - 0.2, -0.17))),
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
    fn build_palette(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let pr = s.getattr("palette_root")?;
        let kids: Vec<B<'_>> = pr.getattr("children")?.iter()?.collect::<R<_>>()?;
        for e in kids {
            super::client_misc::ui_destroy(py, &e, &PyTuple::empty_bound(py), None)?;
        }
        let b = bm(py)?;
        let world = s.getattr("world")?;
        let allowed = world.call_method0("block_categories")?; // シナリオでは章ごとに出す種類を絞る
        let mut cats: Vec<B<'_>> = vec![];
        for c in b.getattr("CATEGORIES")?.iter()? {
            let c = c?;
            if allowed.is_none() || allowed.contains(c.get_item(0)?)? {
                cats.push(c);
            }
        }
        let cat0 = s.getattr("cat")?;
        if !cats.is_empty() && !cats.iter().any(|c| c.get_item(0).and_then(|x| x.eq(&cat0)).unwrap_or(false)) {
            s.setattr("cat", cats[0].get_item(0)?)?;
        }
        let cat = s.getattr("cat")?;
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
    fn _reporter_btn<'py>(slf: &Bound<'py, Self>, parent: &B<'py>, label: &str, node: &B<'py>, x: f64, y: f64, w: f64) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let b = bm(py)?;
        let kind = b.call_method1("value_kind", (node,))?;
        let erg = be_expr_rgb(py, node)?;
        let ssel = s.getattr("slot_sel")?;
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
    fn _set_cat(slf: &Bound<'_, Self>, cid: &B<'_>) -> R<()> {
        slf.as_any().setattr("cat", cid)?;
        Self::build_palette(slf)
    }

    // ------------------------------------------------------------------
    // プログラムの行
    // ------------------------------------------------------------------
    fn flatten<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyList>> {
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
        let scripts = slf.as_any().getattr("prog")?.get_item("scripts")?;
        for i in 0..scripts.len()? {
            let sc = scripts.get_item(i)?;
            rows.append(row(py, "block", 0, Some(&scripts), Some(i as i64), Some(&sc), None)?)?;
            body(py, &rows, &stm, &sc.get_item("body")?, 1, &sc)?;
            rows.append(row(py, "gap", 0, None, None, None, None)?)?;
        }
        rows.append(row(py, "end", 0, Some(&scripts), Some(scripts.len()? as i64), Some(&none(py)), Some("＋ 新しいスクリプト（イベントのブロックを押す）"))?)?;
        Ok(rows)
    }
    fn render(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let rr = s.getattr("rows_root")?;
        let kids: Vec<B<'_>> = rr.getattr("children")?.iter()?.collect::<R<_>>()?;
        for e in kids {
            super::client_misc::ui_destroy(py, &e, &PyTuple::empty_bound(py), None)?;
        }
        let b = bm(py)?;
        let rows = Self::flatten(slf)?;
        s.setattr("rows", &rows)?;
        let n = rows.len() as i64;
        let sc0: i64 = s.getattr("scroll")?.extract()?;
        let scroll = 0.max(sc0.min(0.max(n - VISIBLE)));
        s.setattr("scroll", scroll)?;
        let mut y = AREA_TOP;
        let hint = s.getattr("hint")?;
        let h_ = if hint.is_truthy()? && hint.get_item("kind")?.eq("insert")? { Some(hint) } else { None };
        let sel = s.getattr("sel")?;
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
    fn _slot(slf: &Bound<'_, Self>, node: &B<'_>, key: &B<'_>, kind: &str, x: f64, y: f64) -> R<f64> {
        Self::_draw_slot(slf, node, key, kind, x, y, 0)
    }
    fn _is_sel(slf: &Bound<'_, Self>, owner: &B<'_>, key: &B<'_>) -> R<bool> {
        let s2 = slf.as_any().getattr("slot_sel")?;
        Ok(!s2.is_none() && s2.get_item(0)?.is(owner) && s2.get_item(1)?.eq(key)?)
    }
    fn _metrics(&self, depth: i64) -> (f64, f64, f64) {
        metrics(depth)
    }
    #[pyo3(signature = (text, x, y, z, scale, dark=None))]
    fn _label(slf: &Bound<'_, Self>, text: &B<'_>, x: f64, y: f64, z: f64, scale: f64, dark: Option<bool>) -> R<f64> {
        label(slf.as_any(), &ss(text)?, x, y, z, scale, dark.unwrap_or(false))
    }
    #[allow(clippy::too_many_arguments)]
    fn _draw_slot(slf: &Bound<'_, Self>, owner: &B<'_>, key: &B<'_>, kind: &str, x: f64, y: f64, depth: i64) -> R<f64> {
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
    fn _draw_expr(slf: &Bound<'_, Self>, node: &B<'_>, owner: &B<'_>, key: &B<'_>, kind: &str, x: f64, y: f64, depth: i64) -> R<f64> {
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
    /// 入力欄を押した：1回目は選ぶ（パレットの式ブロックがここに入る）、選んである欄をもう一度押すと値の入力。
    fn slot_click(slf: &Bound<'_, Self>, owner: &B<'_>, key: &B<'_>, kind: &B<'_>) -> R<()> {
        let py = slf.py();
        if Self::_is_sel(slf, owner, key)? {
            return Self::open_dialog(slf, owner, key, kind);
        }
        slf.as_any().setattr("slot_sel", PyTuple::new_bound(py, [owner, key, kind]))?;
        Self::_status(slf, "左の「値・条件のブロック」を押すとここに入ります（もう一度押すと数値などを入力）")?;
        Self::build_palette(slf)?;
        Self::render(slf)
    }
    /// 演算の種類・機器の値の種類などの ▾：かつ／または は押すたびに切り替え、ほかは選ぶ画面。
    fn menu_click(slf: &Bound<'_, Self>, node: &B<'_>, key: &B<'_>, opts: &B<'_>) -> R<()> {
        let py = slf.py();
        if key.eq("op")? && getd(node, "type")?.eq("logic")? {
            node.set_item("op", if getd(node, "op")?.eq("and")? { "or" } else { "and" })?;
            slf.as_any().setattr("dirty", true)?;
            return Self::render(slf);
        }
        let mut o = vec![];
        for x in opts.iter()? {
            o.push(ss(&x?)?);
        }
        Self::open_dialog(slf, node, key, &po(py, format!("choice:{}", o.join("|"))).into_bound(py))
    }
    /// パレットの式ブロックを、選んだ入力欄に入れる（演算などは、今の値を左側に入れたまま包む）。
    fn put_reporter(slf: &Bound<'_, Self>, node: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let b = bm(py)?;
        let mut ssel = s.getattr("slot_sel")?;
        let h = s.getattr("hint")?;
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
        s.setattr("slot_sel", nxt)?;
        s.setattr("dirty", true)?;
        s.setattr("discard_armed", false)?;
        Self::build_palette(slf)?;
        Self::render(slf)
    }
    fn select(slf: &Bound<'_, Self>, row: &Bound<'_, Row>) -> R<()> {
        let py = slf.py();
        let r = row.borrow();
        slf.as_any().setattr("sel", PyTuple::new_bound(py, [r.parent.clone_ref(py), r.index.clone_ref(py), po(py, r.kind.as_str())]))?;
        drop(r);
        Self::render(slf)
    }
    fn _update_title(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let d = Self::device(slf)?;
        let title = s.getattr("title")?;
        if d.is_none() {
            title.setattr("text", "コンピューターが失われました")?;
            return Ok(());
        }
        let st = d.get_item("state")?;
        let run = if getd(&st, "running")?.is_truthy()? { "実行中" } else { "停止中" };
        let mark = if s.getattr("dirty")?.is_truthy()? { " ●未保存" } else { "" };
        title.setattr("text", safe(&format!("ブロックプログラム — {}（{}） [{}]  {}{}", ss(&d.get_item("name")?)?, ss(&s.getattr("c")?.getattr("name")?)?,
            Self::file_base(slf)?, run, mark)))?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // 編集
    // ------------------------------------------------------------------
    fn changed(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        s.setattr("dirty", true)?;
        s.setattr("discard_armed", false)?;
        Self::_refresh_hint(slf, true)?;
        Self::render(slf)
    }
    fn insert(slf: &Bound<'_, Self>, kind: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
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
        let scripts = s.getattr("prog")?.get_item("scripts")?;
        let h = s.getattr("hint")?;
        if getd(&spec, "hat")?.is_truthy()? {
            scripts.call_method1("append", (&node,))?;
            s.setattr("sel", PyTuple::new_bound(py, [scripts.clone(), po(py, scripts.len()? as i64 - 1).into_bound(py), po(py, "block").into_bound(py)]))?;
        } else if !h.is_none() && h.get_item("kind")?.eq("insert")? && h.get_item("type")?.eq(kind)? {
            h.get_item("list")?.call_method1("insert", (h.get_item("index")?, &node))?; // ヒントの所に入れる
            s.setattr("sel", PyTuple::new_bound(py, [h.get_item("list")?, h.get_item("index")?, po(py, "block").into_bound(py)]))?;
        } else {
            let mut target: Option<(B<'_>, i64)> = None;
            let sel = s.getattr("sel")?;
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
            s.setattr("sel", PyTuple::new_bound(py, [target, po(py, idx).into_bound(py), po(py, "block").into_bound(py)]))?;
        }
        Self::_ensure_visible(slf)?;
        Self::changed(slf)
    }
    fn _ensure_visible(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let rows = Self::flatten(slf)?;
        let sel = s.getattr("sel")?;
        for (k, r) in rows.iter().enumerate() {
            let r = r.downcast_into::<Row>()?;
            if sel.is_truthy()? && same_sel(&r, &sel)? {
                let k = k as i64;
                let scroll: i64 = s.getattr("scroll")?.extract()?;
                if k < scroll {
                    s.setattr("scroll", k)?;
                } else if k >= scroll + VISIBLE {
                    s.setattr("scroll", k - VISIBLE + 2)?;
                }
                return Ok(());
            }
        }
        Ok(())
    }
    fn _selected_block<'py>(slf: &Bound<'py, Self>) -> R<Option<(B<'py>, i64)>> {
        let sel = slf.as_any().getattr("sel")?;
        if !sel.is_truthy()? || !sel.get_item(2)?.eq("block")? {
            Self::_status(slf, "ブロックを選んでください")?;
            return Ok(None);
        }
        let (lst, i) = (sel.get_item(0)?, sel.get_item(1)?.extract::<i64>()?);
        Ok(if (i as usize) < lst.len()? { Some((lst, i)) } else { None })
    }
    fn r#move(slf: &Bound<'_, Self>, d: i64) -> R<()> {
        let py = slf.py();
        let Some((lst, i)) = Self::_selected_block(slf)? else { return Ok(()) };
        let j = i + d;
        if 0 <= j && (j as usize) < lst.len()? {
            let (a, b) = (lst.get_item(j)?, lst.get_item(i)?);
            lst.set_item(i, a)?;
            lst.set_item(j, b)?;
            slf.as_any().setattr("sel", PyTuple::new_bound(py, [lst, po(py, j).into_bound(py), po(py, "block").into_bound(py)]))?;
            Self::changed(slf)?;
        }
        Ok(())
    }
    fn duplicate(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let Some((lst, i)) = Self::_selected_block(slf)? else { return Ok(()) };
        lst.call_method1("insert", (i + 1, deepcopy(&lst.get_item(i)?)?))?;
        slf.as_any().setattr("sel", PyTuple::new_bound(py, [lst, po(py, i + 1).into_bound(py), po(py, "block").into_bound(py)]))?;
        Self::changed(slf)
    }
    fn delete(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let Some((lst, i)) = Self::_selected_block(slf)? else { return Ok(()) };
        lst.del_item(i)?;
        let n = lst.len()? as i64;
        if n > 0 {
            slf.as_any().setattr("sel", PyTuple::new_bound(py, [lst, po(py, i.min(n - 1)).into_bound(py), po(py, "block").into_bound(py)]))?;
        } else {
            slf.as_any().setattr("sel", py.None())?;
        }
        Self::changed(slf)
    }

    // ------------------------------------------------------------------
    // 値の選択ダイアログ
    // ------------------------------------------------------------------
    #[pyo3(signature = (kind=None))]
    fn device_names(slf: &Bound<'_, Self>, kind: Option<String>) -> R<Vec<String>> {
        let py = slf.py();
        let c = slf.as_any().getattr("c")?;
        let reg = c.getattr("reg")?;
        let behavior_of = imp(py, "main.extensions")?.getattr("behavior_of")?;
        let mut names: Vec<String> = vec![];
        let k = kind.clone().unwrap_or_default();
        let (kind_, action) = match k.split_once('.') {
            Some((a, b)) => (a.to_string(), b.to_string()), // "actuator.rotate"：その動作ができる物だけ
            None => (k.clone(), String::new()),
        };
        for d in c.getattr("devs")?.iter()? {
            let d = d?;
            if !d.is_truthy()? {
                continue;
            }
            let dd = reg.call_method1("device", (d.get_item("def")?,))?;
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
    fn var_names(slf: &Bound<'_, Self>, vec: bool) -> R<Vec<String>> {
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
        walk(&slf.as_any().getattr("prog")?, vec, &mut found)?;
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
    fn open_dialog(slf: &Bound<'_, Self>, node: &B<'_>, key: &B<'_>, kind: &B<'_>) -> R<()> {
        let py = slf.py();
        slf.as_any().getattr("dialogs")?.call_method1("append", (PyTuple::new_bound(py, [node, key, kind]),))?;
        Self::render_dialog(slf)
    }
    #[pyo3(signature = (all_=false))]
    fn close_dialog(slf: &Bound<'_, Self>, all_: bool) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if all_ {
            s.setattr("dialogs", PyList::empty_bound(py))?;
        } else if s.getattr("dialogs")?.is_truthy()? {
            s.getattr("dialogs")?.call_method0("pop")?;
        }
        Self::render_dialog(slf)?;
        Self::render(slf)
    }
    #[pyo3(signature = (node, key, value, stay=false))]
    fn set_value(slf: &Bound<'_, Self>, node: &B<'_>, key: &B<'_>, value: &B<'_>, stay: bool) -> R<()> {
        let s = slf.as_any();
        node.set_item(key, value)?;
        s.setattr("dirty", true)?;
        s.setattr("discard_armed", false)?;
        if !stay {
            Self::close_dialog(slf, false)
        } else {
            Self::render_dialog(slf)?;
            Self::render(slf)
        }
    }
    fn render_dialog(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let r = s.getattr("dialog_root")?;
        let kids: Vec<B<'_>> = r.getattr("children")?.iter()?.collect::<R<_>>()?;
        for e in kids {
            super::client_misc::ui_destroy(py, &e, &PyTuple::empty_bound(py), None)?;
        }
        let dialogs = s.getattr("dialogs")?;
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
    fn _op_cycle<'py>(slf: &Bound<'py, Self>, cur: &B<'py>) -> R<Option<(PyObject, B<'py>)>> {
        let py = slf.py();
        Ok(Self::op_cycle(slf, cur)?.map(|(n, f)| (po(py, n), f)))
    }
    /// items: [(表示, 関数, 色)] を並べる。次の y を返す。
    #[pyo3(signature = (r, items, y, cols=4, w=0.27))]
    fn _grid(slf: &Bound<'_, Self>, r: &B<'_>, items: &B<'_>, y: f64, cols: usize, w: f64) -> R<f64> {
        let mut v = vec![];
        for it in items.iter()? {
            let it = it?;
            v.push((ss(&it.get_item(0)?)?, it.get_item(1)?, it.get_item(2)?));
        }
        grid(slf.as_any(), r, &v, y, cols, w)
    }
    #[allow(clippy::too_many_arguments)]
    fn _options(slf: &Bound<'_, Self>, r: &B<'_>, node: &B<'_>, key: &B<'_>, kind: &str, cur: &B<'_>, y: f64) -> R<f64> {
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
    fn _input_row(slf: &Bound<'_, Self>, r: &B<'_>, y: f64, label: &str, value: &B<'_>, chars: &str, on_ok: &B<'_>) -> R<f64> {
        input_row(slf.as_any(), r, y, label, &ss(value)?, chars, on_ok)
    }

    // ------------------------------------------------------------------
    fn _status(slf: &Bound<'_, Self>, msg: &str) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if s.hasattr("status")? {
            s.getattr("status")?.setattr("text", safe(&wrap(msg, 24)))?;
            s.setattr("_status_t", now(py)?)?;
        }
        Ok(())
    }
    fn open_rename(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = Self::index(slf)?;
        if i.is_none() {
            return Ok(());
        }
        let c = s.getattr("c")?;
        let before = PyDict::new_bound(py);
        for (k, d) in c.getattr("devs")?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? {
                before.set_item(k, d.get_item("name")?)?;
            }
        }
        s.setattr("_names_before", before)?;
        imp(py, "main.client.device_ui")?.getattr("DevicePanel")?.call((s.getattr("game")?, c, i), Some(&kw(py, vec![("owner", s.clone().unbind())])?))?;
        Ok(())
    }
    /// 名前を変えたら、プログラム中の古い名前も新しい名前に置き換える（ほかに同名がなければ）。
    fn after_modal(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let before = s.getattr("_names_before").unwrap_or_else(|_| PyDict::new_bound(py).into_any());
        let now_ = PyDict::new_bound(py);
        for (k, d) in s.getattr("c")?.getattr("devs")?.iter()?.enumerate() {
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
            walk(&s.getattr("prog")?, &changes, &mut hit, s)?;
            if hit {
                Self::_status(slf, "名前の変更をプログラムにも反映しました（保存してください）")?;
            }
        }
        Self::render(slf)
    }
    fn input(slf: &Bound<'_, Self>, key: &B<'_>) -> R<()> {
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
    fn _hint_key<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let s = slf.as_any();
        Ok(PyTuple::new_bound(py, [ss(&s.getattr("world")?.getattr("dir")?)?, format!("{}.blocks.json", ss(&s.getattr("base")?)?)]))
    }
    /// MOD に見本を聞く（シナリオの課題など。目標が済んだら MOD は None を返す）。
    fn _poll_solution(slf: &Bound<'_, Self>) -> R<()> {
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
    fn _refresh_hint(slf: &Bound<'_, Self>, force: bool) -> R<()> {
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
    fn _hinted(slf: &Bound<'_, Self>, owner: &B<'_>, key: &B<'_>) -> R<bool> {
        let h = slf.as_any().getattr("hint")?;
        if h.is_none() {
            return Ok(false);
        }
        let k = h.get_item("kind")?;
        Ok((k.eq("expr")? || k.eq("value")?) && h.get_item("owner")?.is(owner) && h.get_item("key")?.eq(key)?)
    }
    /// ヒントで光らせるパレット：(カテゴリ, 文のブロックの種類, 式ブロックのノード)
    fn _hint_palette<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
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
    fn _glow(slf: &Bound<'_, Self>, parent: &B<'_>, x: f64, y: f64, w: f64, h: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        mk(s, "Entity", vec![("parent", parent.clone().unbind()), ("model", shape(py, (w + 0.014) / (h + 0.014), Radius::R(0.3))?.unbind()),
            ("position", po(py, (x, y, 0.01))), ("scale", po(py, (w + 0.014, h + 0.014))), ("color", rgb(s, HINT_RGB)?.unbind())])?;
        Ok(())
    }
    fn update(slf: &Bound<'_, Self>) -> R<()> {
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

impl BlockEditor {
    /// self._box(...)：角の丸い（または矢印の）押せる箱
    #[allow(clippy::too_many_arguments)]
    fn boxx<'py>(slf: &Bound<'py, Self>, x: f64, y: f64, w: f64, h: f64, c: (f64, f64, f64), z: f64, radius: Radius, on_click: &B<'py>, outline: bool,
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
    fn op_cycle<'py>(slf: &Bound<'py, Self>, cur: &B<'py>) -> R<Option<(String, B<'py>)>> {
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

fn metrics(depth: i64) -> (f64, f64, f64) {
    let d = depth as f64;
    let a = 0.84 - 0.13 * d;
    let h = ROW_H * if 0.46 > a { 0.46 } else { a };
    let b = 0.66 - 0.035 * d;
    let ts = if 0.52 > b { 0.52 } else { b };
    let z = -0.012 - d * 0.006;
    (h, ts, z)
}
/// 文字を置いて、幅を返す
fn label(s: &B<'_>, text: &str, x: f64, y: f64, z: f64, scale: f64, dark: bool) -> R<f64> {
    let py = s.py();
    let c = if dark { rgb(s, (0.08, 0.08, 0.1))? } else { cn(s, "white")? };
    let t = mk(s, "Text", vec![("parent", s.getattr("rows_root")?.unbind()), ("text", po(py, text)), ("position", po(py, (x, y, z))), ("origin", po(py, (-0.5, 0))),
        ("scale", po(py, scale)), ("color", c.unbind())])?;
    Ok(f64of(&t.getattr("width")?)? * scale)
}
/// items を並べる。次の y を返す。
fn grid(s: &B<'_>, r: &B<'_>, items: &[(String, B<'_>, B<'_>)], y: f64, cols: usize, w: f64) -> R<f64> {
    for (k, (label, f, c)) in items.iter().enumerate() {
        let x = -0.45 + (k % cols) as f64 * (w + 0.02);
        let yy = y - (k / cols) as f64 * 0.045;
        btn(s, r, label, (x, yy), f, w, 0.038, Some(c.clone()), 0.62)?;
    }
    let rows = items.len().div_ceil(cols);
    Ok(y - rows as f64 * 0.045 - 0.01)
}
#[allow(clippy::too_many_arguments)]
fn input_row(s: &B<'_>, r: &B<'_>, y: f64, label: &str, value: &str, chars: &str, on_ok: &B<'_>) -> R<f64> {
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
