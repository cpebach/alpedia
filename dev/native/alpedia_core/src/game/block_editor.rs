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

}

// 中身は分けて書いている（C-02）
mod palette;
mod draw;
mod edit;
mod dialog;
mod hint;
mod inner;
pub(crate) use self::inner::*;
