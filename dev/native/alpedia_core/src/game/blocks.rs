//! ブロック（ノード）プログラミング：要約・ヒント・Python コードへの変換。main/blocks.py の計算の部分。
//!
//! ブロックの表（STATEMENTS・SENSORS など）は MOD が足すので main/blocks.py の dict のまま。ここはそれを読んで使う。
//! ノードの形:
//!   文 :  {"type": "if", "cond": 式, "body": [文...], "else": [文...]}
//!   式 :  {"type": "num", "v": 1.0} / {"type": "op", "op": "+", "a": 式, "b": 式} など

use pyo3::exceptions::{PyOverflowError, PyStopIteration, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyInt, PyList, PyString};

type R<T> = PyResult<T>;

fn tbl<'py>(py: Python<'py>, name: &str) -> R<Bound<'py, PyAny>> {
    crate::game::imp(py, "main.blocks")?.getattr(name)
}

/// node.get(key)（dict でなければ None）
fn get<'py>(node: &Bound<'py, PyAny>, key: &str) -> R<Option<Bound<'py, PyAny>>> {
    match node.downcast::<PyDict>() {
        Ok(d) => d.get_item(key),
        Err(_) => Ok(None),
    }
}

/// str(node.get(key))（無ければ "None"）
fn gs(node: &Bound<'_, PyAny>, key: &str) -> R<String> {
    Ok(match get(node, key)? {
        Some(v) => v.str()?.to_string(),
        None => "None".into(),
    })
}

/// node.get(key, default) を文字列で
fn gsd(node: &Bound<'_, PyAny>, key: &str, default: &str) -> R<String> {
    Ok(match get(node, key)? {
        Some(v) => v.str()?.to_string(),
        None => default.into(),
    })
}

fn get_or_none<'py>(py: Python<'py>, node: &Bound<'py, PyAny>, key: &str) -> R<Bound<'py, PyAny>> {
    Ok(get(node, key)?.unwrap_or_else(|| py.None().into_bound(py)))
}

fn type_of(node: &Bound<'_, PyAny>) -> R<Option<String>> {
    match get(node, "type")? {
        Some(t) => Ok(t.downcast::<PyString>().ok().map(|s| s.to_string())),
        None => Ok(None),
    }
}

/// repr(str(s))
fn q(s: &Bound<'_, PyAny>) -> R<String> {
    Ok(s.str()?.repr()?.to_string())
}

fn qs(py: Python<'_>, s: &str) -> R<String> {
    Ok(PyString::new_bound(py, s).repr()?.to_string())
}

/// 表 name の key の項目の idx 番目（無ければ default）
fn table_pick(py: Python<'_>, name: &str, key: Option<Bound<'_, PyAny>>, idx: usize, default: &str) -> R<String> {
    let t = tbl(py, name)?;
    let k = key.unwrap_or_else(|| py.None().into_bound(py));
    match t.call_method1("get", (k,))? {
        v if v.is_none() => Ok(default.into()),
        v => Ok(v.get_item(idx)?.str()?.to_string()),
    }
}

fn map_get(py: Python<'_>, name: &str, key: Option<Bound<'_, PyAny>>, default: &str) -> R<String> {
    let t = tbl(py, name)?;
    let k = key.unwrap_or_else(|| py.None().into_bound(py));
    let v = t.call_method1("get", (k, default))?;
    Ok(v.str()?.to_string())
}

const NUM_TYPES: &[&str] = &["num", "op", "func", "random", "sensor", "var", "devval", "radio_val", "vlen", "vcomp", "vdot", "vangle"];
const BOOL_TYPES: &[&str] = &["bool", "cmp", "logic", "not", "key", "devbool", "radio_has", "radio_ok", "bsensor"];
const TEXT_TYPES: &[&str] = &["text", "join"];
const VEC_TYPES: &[&str] = &["vsensor", "vvar", "vec", "vto", "vop", "vscale", "vnorm", "vclamp", "vperp", "vtilt"];

pub fn value_kind_of(node: &Bound<'_, PyAny>) -> R<Option<&'static str>> {
    let Some(t) = type_of(node)? else { return Ok(None) };
    let t = t.as_str();
    Ok(if NUM_TYPES.contains(&t) {
        Some("num")
    } else if BOOL_TYPES.contains(&t) {
        Some("bool")
    } else if TEXT_TYPES.contains(&t) {
        Some("text")
    } else if VEC_TYPES.contains(&t) {
        Some("vec")
    } else {
        None
    })
}

#[pyfunction]
pub fn blocks_value_kind(node: &Bound<'_, PyAny>) -> R<Option<&'static str>> {
    value_kind_of(node)
}

/// 式ブロック node を、種類 slot（num/bool/text/vec）の入力欄に入れられるか
#[pyfunction]
pub fn blocks_fits(node: &Bound<'_, PyAny>, slot: &Bound<'_, PyAny>) -> R<bool> {
    let Some(k) = value_kind_of(node)? else { return Ok(false) };
    let slot = slot.downcast::<PyString>().map(|s| s.to_string()).unwrap_or_default();
    if slot == "text" {
        return Ok(true);
    }
    if k == "vec" || slot == "vec" {
        return Ok(k == slot);
    }
    if slot == "num" {
        return Ok(k == "num" || k == "bool");
    }
    Ok(k == slot)
}

/// 'もし {cond} なら' → [('text','もし '), ('slot','cond'), ('text',' なら')]
pub fn label_parts(label: &str) -> R<Vec<(&'static str, String)>> {
    let mut out = Vec::new();
    let mut i = 0;
    let b = label;
    while i < b.len() {
        match b[i..].find('{') {
            None => {
                out.push(("text", b[i..].to_string()));
                break;
            }
            Some(dj) => {
                let j = i + dj;
                if j > i {
                    out.push(("text", b[i..j].to_string()));
                }
                let k = j + b[j..].find('}').ok_or_else(|| PyValueError::new_err("substring not found"))?;
                out.push(("slot", b[j + 1..k].to_string()));
                i = k + 1;
            }
        }
    }
    Ok(out)
}

#[pyfunction]
pub fn blocks_label_parts(label: &str) -> R<Vec<(&'static str, String)>> {
    label_parts(label)
}

fn py_float(v: &Bound<'_, PyAny>) -> R<Option<f64>> {
    let py = v.py();
    match crate::game::imp(py, "builtins")?.getattr("float")?.call1((v,)) {
        Ok(f) => Ok(Some(f.extract()?)),
        Err(e) if e.is_instance_of::<PyTypeError>(py) || e.is_instance_of::<PyValueError>(py) => Ok(None),
        Err(e) => Err(e),
    }
}

/// 数の表示（整数なら小数点なし）
pub fn fmt_num(v: &Bound<'_, PyAny>) -> R<String> {
    let py = v.py();
    let Some(f) = py_float(v)? else { return Ok(v.str()?.to_string()) };
    if f.is_nan() {
        return Err(PyValueError::new_err("cannot convert float NaN to integer"));
    }
    if f.is_infinite() {
        return Err(PyOverflowError::new_err("cannot convert float infinity to integer"));
    }
    if f == f.trunc() && f.abs() < 1e12 {
        return Ok((f.trunc() as i64).to_string());
    }
    Ok(PyFloat::new_bound(py, f).call_method1("__format__", ("g",))?.to_string())
}

#[pyfunction]
pub fn blocks_fmt_num(v: &Bound<'_, PyAny>) -> R<String> {
    fmt_num(v)
}

/// 表示用の要約
pub fn summary(py: Python<'_>, node: &Bound<'_, PyAny>) -> R<String> {
    if node.is_none() {
        return Ok("?".into());
    }
    if !node.is_instance_of::<PyDict>() {
        if node.eq("*")? {
            return Ok("すべて".into());
        }
        return Ok(node.str()?.to_string());
    }
    let s = |k: &str| -> R<String> { summary(py, &get_or_none(py, node, k)?) };
    let t = type_of(node)?.unwrap_or_default();
    Ok(match t.as_str() {
        "num" => match get(node, "v")? {
            Some(v) => fmt_num(&v)?,
            None => "0".into(),
        },
        "text" => format!("「{}」", gsd(node, "v", "")?.replace('\n', "⏎")),
        "bool" => if get(node, "v")?.map_or(Ok(false), |v| v.is_truthy())? { "はい".into() } else { "いいえ".into() },
        "var" => gsd(node, "name", "x")?,
        "sensor" => table_pick(py, "SENSORS", get(node, "name")?, 0, "?")?,
        "op" => format!("({} {} {})", s("a")?, gs(node, "op")?, s("b")?),
        "cmp" => format!("〈{} {} {}〉", s("a")?, gs(node, "op")?, s("b")?),
        "logic" => {
            let word = if gs(node, "op")? == "and" { "かつ" } else { "または" };
            format!("〈{} {word} {}〉", s("a")?, s("b")?)
        }
        "not" => format!("〈{} ではない〉", s("a")?),
        "func" => format!("{}({})", gs(node, "fn")?, s("a")?),
        "random" => format!("乱数({}〜{})", s("a")?, s("b")?),
        "join" => format!("({}＋{})", s("a")?, s("b")?),
        "key" => format!("〈キー {} が押されている〉", gs(node, "key")?),
        "devval" => format!("({} の {})", s("dev")?, gs(node, "prop")?),
        "devbool" => format!("〈{} が {}〉", s("dev")?, gs(node, "prop")?),
        "radio_val" => format!("(チャンネル {} の受信値)", s("ch")?),
        "radio_has" => format!("〈チャンネル {} に届いている〉", s("ch")?),
        "radio_ok" => "〈アンテナが使える〉".into(),
        "bsensor" => format!("〈{}〉", table_pick(py, "BOOL_SENSORS", get(node, "name")?, 0, "?")?),
        "vsensor" => table_pick(py, "VSENSORS", get(node, "name")?, 0, "?")?,
        "vvar" => gsd(node, "name", "向き")?,
        "vec" => {
            let pre = if gs(node, "frame")? == "船から見た" { "船から見た" } else { "" };
            format!("{pre}矢印({}, {}, {})", s("x")?, s("y")?, s("z")?)
        }
        "vto" => format!("地点({}, {}, {})への矢印", s("x")?, s("y")?, s("z")?),
        "vop" | "vscale" => format!("[{} {} {}]", s("a")?, gs(node, "op")?, s("b")?),
        "vnorm" => format!("[{} の向き]", s("a")?),
        "vclamp" => format!("[{} の長さを {} までに]", s("a")?, s("b")?),
        "vperp" => format!("[{} の {} に垂直な部分]", s("a")?, s("b")?),
        "vtilt" => format!("[{} を {} から {} 度以内に]", s("a")?, s("b")?, s("n")?),
        "vlen" => format!("({} の長さ)", s("a")?),
        "vcomp" => format!("({} の {} 成分)", s("a")?, gs(node, "axis")?),
        "vdot" => format!("({} と {} の内積)", s("a")?, s("b")?),
        "vangle" => format!("({} と {} の角度)", s("a")?, s("b")?),
        _ => "?".into(),
    })
}

#[pyfunction]
pub fn blocks_summary(py: Python<'_>, node: &Bound<'_, PyAny>) -> R<String> {
    summary(py, node)
}

/// 式ノードの中の入力欄: [(キー, 種類)]
pub fn expr_children(node: &Bound<'_, PyAny>) -> R<Vec<(&'static str, &'static str)>> {
    let t = type_of(node)?.unwrap_or_default();
    Ok(match t.as_str() {
        "op" | "random" | "cmp" => vec![("a", "num"), ("b", "num")],
        "logic" => vec![("a", "bool"), ("b", "bool")],
        "not" => vec![("a", "bool")],
        "func" => vec![("a", "num")],
        "join" => vec![("a", "text"), ("b", "text")],
        "devval" | "devbool" => vec![("dev", "dev:*")],
        "key" => vec![("key", "key")],
        "radio_val" | "radio_has" => vec![("ch", "text")],
        "vec" | "vto" => vec![("x", "num"), ("y", "num"), ("z", "num")],
        "vop" | "vperp" | "vdot" | "vangle" => vec![("a", "vec"), ("b", "vec")],
        "vscale" | "vclamp" => vec![("a", "vec"), ("b", "num")],
        "vnorm" | "vlen" | "vcomp" => vec![("a", "vec")],
        "vtilt" => vec![("a", "vec"), ("b", "vec"), ("n", "num")],
        _ => vec![],
    })
}

#[pyfunction]
pub fn blocks_expr_children(node: &Bound<'_, PyAny>) -> R<Vec<(&'static str, &'static str)>> {
    expr_children(node)
}

fn stmt_spec<'py>(py: Python<'py>, t: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let s = tbl(py, "STATEMENTS")?.call_method1("get", (t, PyDict::new_bound(py)))?;
    Ok(s)
}

fn spec_flag(spec: &Bound<'_, PyAny>, k: &str) -> R<bool> {
    spec.call_method1("get", (k,))?.is_truthy()
}

/// 入力欄の種類（無ければ num）
pub fn slot_kind(py: Python<'_>, stmt_type: &Bound<'_, PyAny>, slot: &Bound<'_, PyAny>) -> R<String> {
    let spec = tbl(py, "STATEMENTS")?.get_item(stmt_type)?;
    let slots = spec.call_method1("get", ("slots", PyDict::new_bound(py)))?;
    Ok(slots.call_method1("get", (slot, "num"))?.str()?.to_string())
}

#[pyfunction]
pub fn blocks_slot_kind(py: Python<'_>, stmt_type: &Bound<'_, PyAny>, slot: &Bound<'_, PyAny>) -> R<String> {
    slot_kind(py, stmt_type, slot)
}

fn deep_copy<'py>(v: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    crate::game::imp(v.py(), "copy")?.getattr("deepcopy")?.call1((v,))
}

#[pyfunction]
pub fn blocks_new_stmt<'py>(py: Python<'py>, kind: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let spec = tbl(py, "STATEMENTS")?.get_item(kind)?;
    let node = PyDict::new_bound(py);
    node.set_item("type", kind)?;
    let defs = tbl(py, "STMT_DEFAULTS")?.call_method1("get", (kind, PyDict::new_bound(py)))?;
    node.update(deep_copy(&defs)?.downcast::<pyo3::types::PyMapping>()?)?;
    if spec_flag(&spec, "hat")? || spec_flag(&spec, "body")? {
        node.set_item("body", PyList::empty_bound(py))?;
    }
    if spec_flag(&spec, "orelse")? {
        node.set_item("else", PyList::empty_bound(py))?;
    }
    Ok(node)
}

// ---------------------------------------------------------------------------
// ヒント：見本（MOD が渡す完成形）と比べて、次にすること（置くブロック・入れる式・入力する値）を1つ探す
// ---------------------------------------------------------------------------
const MENU_FIELDS: [&str; 5] = ["op", "fn", "frame", "axis", "prop"];

fn slot_keys(py: Python<'_>, stmt_type: &Bound<'_, PyAny>) -> R<Vec<String>> {
    let spec = stmt_spec(py, stmt_type)?;
    let label = spec.call_method1("get", ("label", ""))?.str()?.to_string();
    Ok(label_parts(&label)?.into_iter().filter(|(k, _)| *k == "slot").map(|(_, s)| s).collect())
}

fn num_eq(a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>) -> R<bool> {
    match (py_float(a)?, py_float(b)?) {
        (Some(x), Some(y)) => Ok((x - y).abs() <= 1e-6 + 1e-4 * x.abs().max(y.abs())),
        _ => a.eq(b),
    }
}

fn hint_dict<'py>(py: Python<'py>, items: &[(&str, PyObject)]) -> R<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    for (k, v) in items {
        d.set_item(*k, v)?;
    }
    Ok(d)
}

fn hint_expr<'py>(py: Python<'py>, owner: &Bound<'py, PyAny>, key: &str, kind: &str, cv: &Bound<'py, PyAny>, sv: &Bound<'py, PyAny>)
    -> R<Option<Bound<'py, PyDict>>> {
    if !sv.is_instance_of::<PyDict>() {
        if !cv.eq(sv)? {
            return Ok(Some(hint_dict(py, &[("kind", "value".into_py(py)), ("owner", owner.clone().unbind()), ("key", key.into_py(py)),
                                           ("slot", kind.into_py(py)), ("value", sv.clone().unbind())])?));
        }
        return Ok(None);
    }
    let st = get_or_none(py, sv, "type")?;
    let ct = if cv.is_instance_of::<PyDict>() { get_or_none(py, cv, "type")? } else { py.None().into_bound(py) };
    let sts = st.downcast::<PyString>().map(|s| s.to_string()).unwrap_or_default();
    if sts == "num" || sts == "text" || sts == "bool" {
        let same = ct.eq(&st)? && if sts == "num" {
            num_eq(&get_or_none(py, cv, "v")?, &get_or_none(py, sv, "v")?)?
        } else {
            get_or_none(py, cv, "v")?.eq(get_or_none(py, sv, "v")?)?
        };
        if !same {
            return Ok(Some(hint_dict(py, &[("kind", "value".into_py(py)), ("owner", owner.clone().unbind()), ("key", key.into_py(py)),
                                           ("slot", kind.into_py(py)), ("value", get_or_none(py, sv, "v")?.unbind()),
                                           ("type", st.clone().unbind())])?));
        }
        return Ok(None);
    }
    if !ct.eq(&st)? || !get_or_none(py, cv, "name")?.eq(get_or_none(py, sv, "name")?)? {
        return Ok(Some(hint_dict(py, &[("kind", "expr".into_py(py)), ("owner", owner.clone().unbind()), ("key", key.into_py(py)),
                                       ("slot", kind.into_py(py)), ("node", sv.clone().unbind())])?));
    }
    for f in MENU_FIELDS {
        if let Some(sf) = get(sv, f)? {
            if !get_or_none(py, cv, f)?.eq(&sf)? {
                return Ok(Some(hint_dict(py, &[("kind", "value".into_py(py)), ("owner", cv.clone().unbind()), ("key", f.into_py(py)),
                                               ("slot", "menu".into_py(py)), ("value", sf.unbind()), ("menu", true.into_py(py))])?));
            }
        }
    }
    for (ck, ckind) in expr_children(sv)? {
        if let Some(h) = hint_expr(py, cv, ck, ckind, &get_or_none(py, cv, ck)?, &get_or_none(py, sv, ck)?)? {
            return Ok(Some(h));
        }
    }
    Ok(None)
}

fn setdefault_list<'py>(py: Python<'py>, d: &Bound<'py, PyAny>, key: &str) -> R<Bound<'py, PyAny>> {
    d.call_method1("setdefault", (key, PyList::empty_bound(py)))
}

fn hint_stmt<'py>(py: Python<'py>, cn: &Bound<'py, PyAny>, sn: &Bound<'py, PyAny>) -> R<Option<Bound<'py, PyDict>>> {
    let st = sn.get_item("type")?;
    for key in slot_keys(py, &st)? {
        let k = PyString::new_bound(py, &key);
        let kind = slot_kind(py, &st, k.as_any())?;
        let cur = cn.call_method1("get", (&key,))?;
        let sol = sn.call_method1("get", (&key,))?;
        if let Some(h) = hint_expr(py, cn, &key, &kind, &cur, &sol)? {
            return Ok(Some(h));
        }
    }
    let spec = stmt_spec(py, &st)?;
    if spec_flag(&spec, "body")? || spec_flag(&spec, "hat")? {
        let cl = setdefault_list(py, cn, "body")?;
        let sl = sn.call_method1("get", ("body", PyList::empty_bound(py)))?;
        if let Some(h) = hint_list(py, &cl, &sl, cn)? {
            return Ok(Some(h));
        }
        if spec_flag(&spec, "orelse")? {
            let cl = setdefault_list(py, cn, "else")?;
            let sl = sn.call_method1("get", ("else", PyList::empty_bound(py)))?;
            return hint_list(py, &cl, &sl, cn);
        }
    }
    Ok(None)
}

fn insert_hint<'py>(py: Python<'py>, list: &Bound<'py, PyAny>, index: usize, owner: PyObject, sn: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    hint_dict(py, &[("kind", "insert".into_py(py)), ("list", list.clone().unbind()), ("index", index.into_py(py)), ("owner", owner),
                    ("type", sn.get_item("type")?.unbind()), ("node", sn.clone().unbind())])
}

fn hint_list<'py>(py: Python<'py>, cl: &Bound<'py, PyAny>, sl: &Bound<'py, PyAny>, owner: &Bound<'py, PyAny>) -> R<Option<Bound<'py, PyDict>>> {
    let n = cl.len()?;
    for (k, sn) in sl.iter()?.enumerate() {
        let sn = sn?;
        if k >= n || !cl.get_item(k)?.call_method1("get", ("type",))?.eq(sn.call_method1("get", ("type",))?)? {
            return Ok(Some(insert_hint(py, cl, k, owner.clone().unbind(), &sn)?));
        }
        if let Some(h) = hint_stmt(py, &cl.get_item(k)?, &sn)? {
            return Ok(Some(h));
        }
    }
    Ok(None)
}

fn dumps_sorted(py: Python<'_>, v: &Bound<'_, PyAny>) -> R<String> {
    let kw = PyDict::new_bound(py);
    kw.set_item("sort_keys", true)?;
    Ok(crate::game::imp(py, "json")?.getattr("dumps")?.call((v,), Some(&kw))?.to_string())
}

/// 今のプログラム cur（エディタが編集中の物そのもの）を、見本 sol と比べて、最初の違いを返す（同じなら None）
#[pyfunction]
pub fn blocks_find_hint<'py>(py: Python<'py>, cur: &Bound<'py, PyAny>, sol: &Bound<'py, PyAny>) -> R<Option<Bound<'py, PyDict>>> {
    let scripts = setdefault_list(py, cur, "scripts")?;
    let mut used: Vec<usize> = Vec::new();
    for s in sol.call_method1("get", ("scripts", PyList::empty_bound(py)))?.iter()? {
        let s = s?;
        let stype = s.call_method1("get", ("type",))?;
        let n = scripts.len()?;
        let mut cands = Vec::new();
        for k in 0..n {
            if !used.contains(&k) && scripts.get_item(k)?.call_method1("get", ("type",))?.eq(&stype)? {
                cands.push(k);
            }
        }
        let keys = slot_keys(py, &s.get_item("type")?)?;
        let mut same = Vec::new();
        for &k in &cands {
            let c = scripts.get_item(k)?;
            let mut all = true;
            for key in &keys {
                if dumps_sorted(py, &c.call_method1("get", (key,))?)? != dumps_sorted(py, &s.call_method1("get", (key,))?)? {
                    all = false;
                    break;
                }
            }
            if all {
                same.push(k);
            }
        }
        let m = same.first().or(cands.first()).copied();
        let Some(m) = m else {
            return Ok(Some(insert_hint(py, &scripts, n, py.None(), &s)?));
        };
        used.push(m);
        if let Some(h) = hint_stmt(py, &scripts.get_item(m)?, &s)? {
            return Ok(Some(h));
        }
    }
    Ok(None)
}

/// ヒントの説明（画面の下に出す文）
#[pyfunction]
pub fn blocks_hint_text(py: Python<'_>, h: &Bound<'_, PyAny>) -> R<String> {
    if h.is_none() {
        return Ok(String::new());
    }
    let kind = h.get_item("kind")?.str()?.to_string();
    if kind == "insert" {
        let t = h.get_item("type")?;
        let spec = stmt_spec(py, &t)?;
        let lab = spec.call_method1("get", ("label", &t))?.str()?.to_string();
        let label: String = label_parts(&lab)?.into_iter().map(|(k, s)| if k == "slot" { "□".to_string() } else { s }).collect();
        return Ok(format!("ヒント：光っているブロック「{label}」を、光っている所に置こう"));
    }
    if kind == "expr" {
        return Ok(format!("ヒント：光っている欄に「{}」を入れよう（欄を押してから、光っている値のブロックを押す）",
                          summary(py, &h.get_item("node")?)?));
    }
    let v = h.get_item("value")?;
    let ty = h.call_method1("get", ("type",))?;
    if ty.eq("num")? {
        return Ok(format!("ヒント：光っている欄の数を {} にしよう（欄を2回押すと入力できる）", fmt_num(&v)?));
    }
    if h.call_method1("get", ("menu",))?.is_truthy()? {
        let shown = if v.eq("and")? { "かつ".to_string() } else if v.eq("or")? { "または".to_string() } else { v.str()?.to_string() };
        return Ok(format!("ヒント：光っている ▼ を押して「{shown}」を選ぼう"));
    }
    if ty.eq("bool")? {
        return Ok(format!("ヒント：光っている欄を「{}」にしよう", if v.is_truthy()? { "はい" } else { "いいえ" }));
    }
    Ok(format!("ヒント：光っている欄を「{}」にしよう", v.str()?))
}

// ---------------------------------------------------------------------------
// Python への変換
// ---------------------------------------------------------------------------
pub const HEADER: &str = include_str!("blocks_header.py.txt");

fn is_int_or_float(v: &Bound<'_, PyAny>) -> bool {
    v.is_instance_of::<PyInt>() || v.is_instance_of::<PyFloat>()
}

pub fn gen_expr(py: Python<'_>, node: &Bound<'_, PyAny>, kind: &str) -> R<String> {
    if !node.is_instance_of::<PyDict>() {
        return q(node);
    }
    let g = |k: &str, kind: &str| -> R<String> { gen_expr(py, &get_or_none(py, node, k)?, kind) };
    let gn = |k: &str| -> R<String> { g(k, "num") };
    let t = type_of(node)?.unwrap_or_default();
    Ok(match t.as_str() {
        "num" => {
            let v = get_or_none(py, node, "v")?;
            if is_int_or_float(&v) { fmt_num(&v)? } else { format!("_num({})", q(&v)?) }
        }
        "text" => qs(py, &gsd(node, "v", "")?)?,
        "bool" => if get(node, "v")?.map_or(Ok(false), |v| v.is_truthy())? { "True".into() } else { "False".into() },
        "var" => format!("v.get({}, 0)", qs(py, &gsd(node, "name", "x")?)?),
        "sensor" => table_pick(py, "SENSORS", get(node, "name")?, 1, "0")?,
        "op" => {
            let (a, b) = (gn("a")?, gn("b")?);
            let op = get(node, "op")?.unwrap_or_else(|| PyString::new_bound(py, "+").into_any());
            if op.eq("÷")? {
                format!("_div({a}, {b})")
            } else if op.eq("余り")? {
                format!("_mod({a}, {b})")
            } else {
                format!("({a} {} {b})", map_get(py, "OPS", Some(op), "+")?)
            }
        }
        "cmp" => format!("({} {} {})", gn("a")?, map_get(py, "CMPS", get(node, "op")?, "==")?, gn("b")?),
        "logic" => {
            let op = if gs(node, "op")? == "and" { "and" } else { "or" };
            format!("({} {op} {})", g("a", "bool")?, g("b", "bool")?)
        }
        "not" => format!("(not {})", g("a", "bool")?),
        "func" => format!("{}({})", map_get(py, "FUNCS", get(node, "fn")?, "abs")?, gn("a")?),
        "random" => format!("random.uniform({}, {})", gn("a")?, gn("b")?),
        "join" => format!("(_txt({}) + _txt({}))", g("a", "text")?, g("b", "text")?),
        "key" => format!("({} in keys)", qs(py, &gsd(node, "key", "space")?)?),
        "devval" => format!("_val(api, {}, {})", qs(py, &gsd(node, "dev", "*")?)?,
                            qs(py, &map_get(py, "DEVPROPS", get(node, "prop")?, "angle")?)?),
        "devbool" => format!("bool(_val(api, {}, {}))", qs(py, &gsd(node, "dev", "*")?)?,
                             qs(py, &map_get(py, "DEVBOOLS", get(node, "prop")?, "busy")?)?),
        "radio_val" => format!("_radio(api, {})", g("ch", "text")?),
        "radio_has" => format!("(api.latest(str({})) is not None)", g("ch", "text")?),
        "radio_ok" => "api.antenna".into(),
        "bsensor" => format!("bool({})", table_pick(py, "BOOL_SENSORS", get(node, "name")?, 1, "False")?),
        "vsensor" => table_pick(py, "VSENSORS", get(node, "name")?, 1, "[0.0, 0.0, 0.0]")?,
        "vvar" => format!("_vget(v, {})", qs(py, &gsd(node, "name", "向き")?)?),
        "vec" => {
            let xyz = format!("{}, {}, {}", gn("x")?, gn("y")?, gn("z")?);
            if gs(node, "frame")? == "船から見た" { format!("_vship(api, {xyz})") } else { format!("[{xyz}]") }
        }
        "vto" => format!("_vsub([{}, {}, {}], _V(api.position))", gn("x")?, gn("y")?, gn("z")?),
        "vop" => format!("{}({}, {})", map_get(py, "VOPS", get(node, "op")?, "_vadd")?, g("a", "vec")?, g("b", "vec")?),
        "vscale" => format!("{}({}, {})", map_get(py, "VSCALES", get(node, "op")?, "_vmul")?, g("a", "vec")?, gn("b")?),
        "vnorm" => format!("_vunit({})", g("a", "vec")?),
        "vclamp" => format!("_vclamp({}, {})", g("a", "vec")?, gn("b")?),
        "vperp" => format!("_vperp({}, {})", g("a", "vec")?, g("b", "vec")?),
        "vtilt" => format!("_vtilt({}, {}, {})", g("a", "vec")?, g("b", "vec")?, gn("n")?),
        "vlen" => format!("_vlen({})", g("a", "vec")?),
        "vcomp" => format!("_vcomp(api, {}, {})", g("a", "vec")?, qs(py, &map_get(py, "VCOMPS", get(node, "axis")?, "f")?)?),
        "vdot" => format!("_vdot({}, {})", g("a", "vec")?, g("b", "vec")?),
        "vangle" => format!("_vangle({}, {})", g("a", "vec")?, g("b", "vec")?),
        _ => if kind == "vec" { "[0.0, 0.0, 0.0]".into() } else { "0".into() },
    })
}

#[pyfunction]
#[pyo3(signature = (node, kind="num"))]
pub fn blocks_gen_expr(py: Python<'_>, node: &Bound<'_, PyAny>, kind: &str) -> R<String> {
    gen_expr(py, node, kind)
}

fn dev_loop(py: Python<'_>, node: &Bound<'_, PyAny>, kind: Option<&str>, line: String) -> R<Vec<String>> {
    let k = match kind {
        Some(k) => qs(py, k)?,
        None => "'None'".into(),
    };
    Ok(vec![format!("for d in _devs(api, {}, {k}):", qs(py, &gsd(node, "dev", "*")?)?), format!("    {line}")])
}

/// アクチュエーターの動作。「すべて」なら、その動作ができる種類だけ
fn act_loop(py: Python<'_>, node: &Bound<'_, PyAny>, action: &str, line: String) -> R<Vec<String>> {
    let dev = get(node, "dev")?;
    if dev.map_or(Ok(false), |d| d.ne("*"))? {
        return dev_loop(py, node, Some("actuator"), line);
    }
    Ok(vec!["for d in _devs(api, '*', 'actuator'):".into(), format!("    if d.can({}):", qs(py, action)?), format!("        {line}")])
}

fn indent(lines: Vec<String>) -> Vec<String> {
    lines.into_iter().map(|l| format!("    {l}")).collect()
}

/// 文 → Python の行のリスト（インデントなし）
pub fn gen_stmt(py: Python<'_>, node: &Bound<'_, PyAny>) -> R<Vec<String>> {
    let tobj = get_or_none(py, node, "type")?;
    let t = tobj.downcast::<PyString>().map(|s| s.to_string()).unwrap_or_default();
    let spec = tbl(py, "STATEMENTS")?.call_method1("get", (&tobj,))?;
    let spec = if spec.is_truthy()? { spec } else { PyDict::new_bound(py).into_any() };
    let g = |k: &str, kind: &str| -> R<String> { gen_expr(py, &get_or_none(py, node, k)?, kind) };
    let gn = |k: &str| -> R<String> { g(k, "num") };
    let code = spec.call_method1("get", ("code",))?;
    if code.is_truthy()? {
        // MOD が足したブロック（register_ext）
        let vals = PyDict::new_bound(py);
        let slots = spec.call_method1("get", ("slots", PyDict::new_bound(py)))?;
        for it in slots.call_method0("items")?.iter()? {
            let (sname, kind): (Bound<'_, PyAny>, String) = it?.extract()?;
            let v = node.call_method1("get", (&sname,))?;
            if kind.starts_with("dev:") || kind.starts_with("choice:") || ["var", "vvar", "key"].contains(&kind.as_str()) {
                let x = if v.is_none() { PyString::new_bound(py, "*").into_any() } else { v };
                vals.set_item(&sname, q(&x)?)?;
            } else {
                let k = if ["num", "bool", "text", "vec"].contains(&kind.as_str()) { kind.as_str() } else { "num" };
                vals.set_item(&sname, gen_expr(py, &v, k)?)?;
            }
        }
        let mut out = Vec::new();
        for ln in code.iter()? {
            out.push(ln?.call_method("format", (), Some(&vals))?.str()?.to_string());
        }
        return Ok(out);
    }
    let st = |k: &str, want: &str| -> R<bool> { Ok(get(node, k)?.map_or(Ok(false), |v| v.eq(want))?) };
    let pyb = |b: bool| if b { "True" } else { "False" };
    Ok(match t.as_str() {
        "wait" | "wait_until" => vec!["pass  # 待つブロックは、イベントのすぐ下（または「くり返す」の中）で使います".into()],
        "if" | "if_else" => {
            let mut out = vec![format!("if {}:", g("cond", "bool")?)];
            out.extend(indent(gen_body(py, &node.call_method1("get", ("body", PyList::empty_bound(py)))?)?));
            if t == "if_else" {
                out.push("else:".into());
                out.extend(indent(gen_body(py, &node.call_method1("get", ("else", PyList::empty_bound(py)))?)?));
            }
            out
        }
        "repeat" => {
            let mut out = vec![format!("for _i in range(max(0, int({}))):", gn("n")?)];
            out.extend(indent(gen_body(py, &node.call_method1("get", ("body", PyList::empty_bound(py)))?)?));
            out
        }
        "reset_timer" => vec!["m['_timer'] = api.time".into()],
        "stop" => vec!["api.stop()".into(), "return".into()],
        "throttle" => dev_loop(py, node, Some("thruster"), format!("d.throttle = {}", gn("value")?))?,
        "valve" => dev_loop(py, node, Some("valve"), format!("d.open = {}", pyb(st("state", "開く")?)))?,
        "ignite" => dev_loop(py, node, Some("thruster"), "d.ignite()".into())?,
        "valve_flow" => dev_loop(py, node, Some("valve"), format!("d.open = max(0.0, min(1.0, {}))", gn("value")?))?,
        "enable" => {
            let on = pyb(st("state", "ON")?);
            if !st("dev", "*")? {
                dev_loop(py, node, None, format!("d.enabled = {on}"))?
            } else {
                vec!["for d in api.devices():".into(), format!("    d.enabled = {on}")]
            }
        }
        "act_rotate" => act_loop(py, node, "rotate", format!("d.rotate({}, speed={})", gn("angle")?, gn("speed")?))?,
        "act_rotate_to" => act_loop(py, node, "rotate", format!("d.rotate_to({}, speed={})", gn("angle")?, gn("speed")?))?,
        "act_extend" => act_loop(py, node, "extend", format!("d.extend({}, speed={})", gn("amount")?, gn("speed")?))?,
        "act_retract" => act_loop(py, node, "retract", format!("d.retract({}, speed={})", gn("amount")?, gn("speed")?))?,
        "act_detach" => dev_loop(py, node, Some("actuator"), "d.detach()".into())?,
        "act_attach" => dev_loop(py, node, Some("actuator"), "d.attach()".into())?,
        "act_execute" => dev_loop(py, node, Some("actuator"), "d.execute()".into())?,
        "display" => vec![format!("api.display(str({}), width={})", g("text", "text")?, gn("width")?)],
        "clear_display" => vec!["api.clear_display()".into()],
        "print" => vec![format!("print({})", g("text", "text")?)],
        "set_var" => vec![format!("v[{}] = {}", qs(py, &gsd(node, "var", "x")?)?, gn("value")?)],
        "change_var" => {
            let name = qs(py, &gsd(node, "var", "x")?)?;
            vec![format!("v[{name}] = _num(v.get({name}, 0)) + {}", gn("value")?)]
        }
        "radio_send" => vec![format!("api.send(str({}), {})", g("ch", "text")?, gn("value")?)],
        "set_vvar" => vec![format!("v[{}] = {}", qs(py, &gsd(node, "var", "向き")?)?, g("value", "vec")?)],
        "show_vec" => {
            let colors = tbl(py, "ARROW_COLORS")?;
            let rgb = colors.call_method1("get", (get_or_none(py, node, "color")?, colors.get_item("黄")?))?;
            vec![format!("api.show_arrow({}, color={}, scale={})", g("value", "vec")?, rgb.repr()?, gn("scale")?)]
        }
        _ => vec![format!("pass  # 未対応のブロック {}", tobj.str()?)],
    })
}

pub fn gen_body(py: Python<'_>, body: &Bound<'_, PyAny>) -> R<Vec<String>> {
    let mut out = Vec::new();
    for n in body.iter()? {
        out.extend(gen_stmt(py, &n?)?);
    }
    if out.is_empty() {
        out.push("pass".into());
    }
    Ok(out)
}

#[pyfunction]
pub fn blocks_gen_stmt(py: Python<'_>, node: &Bound<'_, PyAny>) -> R<Vec<String>> {
    gen_stmt(py, node)
}

#[pyfunction]
pub fn blocks_gen_body(py: Python<'_>, body: &Bound<'_, PyAny>) -> R<Vec<String>> {
    gen_body(py, body)
}

// ---------------------------------------------------------------------------
// 待つブロック（順番に動かす）：イベントの下に「待つ」があると、上から順に1つずつ進める
// ---------------------------------------------------------------------------
pub fn has_wait(body: &Bound<'_, PyAny>) -> R<bool> {
    if body.is_none() || !body.is_truthy()? {
        return Ok(false);
    }
    for n in body.iter()? {
        let n = n?;
        let t = n.call_method1("get", ("type",))?;
        if t.eq("wait")? || t.eq("wait_until")? {
            return Ok(true);
        }
        if t.eq("repeat")? && has_wait(&n.call_method1("get", ("body",))?)? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[pyfunction]
pub fn blocks_has_wait(body: &Bound<'_, PyAny>) -> R<bool> {
    has_wait(body)
}

enum Op {
    Do(Vec<String>),
    Wait(String),
    Until(String),
    Loop(String, usize),
    End(usize),
}

fn flatten(py: Python<'_>, body: &Bound<'_, PyAny>, ops: &mut Vec<Op>) -> R<()> {
    let mut buf: Vec<Bound<'_, PyAny>> = Vec::new();
    let flush = |buf: &mut Vec<Bound<'_, PyAny>>, ops: &mut Vec<Op>| -> R<()> {
        if !buf.is_empty() {
            let l = PyList::new_bound(py, buf.iter());
            ops.push(Op::Do(gen_body(py, l.as_any())?));
            buf.clear();
        }
        Ok(())
    };
    if !body.is_none() && body.is_truthy()? {
        for n in body.iter()? {
            let n = n?;
            let t = n.call_method1("get", ("type",))?;
            if t.eq("wait")? {
                flush(&mut buf, ops)?;
                ops.push(Op::Wait(gen_expr(py, &n.call_method1("get", ("n",))?, "num")?));
            } else if t.eq("wait_until")? {
                flush(&mut buf, ops)?;
                ops.push(Op::Until(gen_expr(py, &n.call_method1("get", ("cond",))?, "bool")?));
            } else if t.eq("repeat")? && has_wait(&n.call_method1("get", ("body",))?)? {
                flush(&mut buf, ops)?;
                let k = ops.len();
                ops.push(Op::Loop(gen_expr(py, &n.call_method1("get", ("n",))?, "num")?, k));
                flatten(py, &n.call_method1("get", ("body",))?, ops)?;
                ops.push(Op::End(k));
            } else {
                buf.push(n);
            }
        }
    }
    flush(&mut buf, ops)?;
    Ok(())
}

/// 順番に動かすプログラム（インデントなし）。restart：最後まで行ったら最初から
pub fn gen_sequence(py: Python<'_>, body: &Bound<'_, PyAny>, k: usize, restart: bool) -> R<(Vec<String>, usize)> {
    let mut ops = Vec::new();
    flatten(py, body, &mut ops)?;
    let n = ops.len();
    let key = qs(py, &format!("_sq{k}"))?;
    let mut out = vec![format!("sq = m.setdefault({key}, {{'i': 0, 't': api.time, 'c': {{}}}})"), "for _g in range(200):".into(),
                       "    _s = sq['i']".into()];
    for (j, op) in ops.iter().enumerate() {
        out.push(format!("    if _s == {j}:"));
        match op {
            Op::Do(lines) => {
                out.extend(lines.iter().map(|l| format!("        {l}")));
                out.push(format!("        sq['i'] = {}; sq['t'] = api.time; continue", j + 1));
            }
            Op::Wait(e) => {
                out.push(format!("        if api.time - sq['t'] < {e}: break"));
                out.push(format!("        sq['i'] = {}; sq['t'] = api.time; continue", j + 1));
            }
            Op::Until(e) => {
                out.push(format!("        if not ({e}): break"));
                out.push(format!("        sq['i'] = {}; sq['t'] = api.time; continue", j + 1));
            }
            Op::Loop(e, id) => {
                out.push(format!("        sq['c']['{id}'] = int(max(0, {e}))"));
                let end = ops.iter().position(|x| matches!(x, Op::End(i) if i == id))
                    .ok_or_else(|| PyStopIteration::new_err(()))?;
                out.push(format!("        sq['i'] = {} if sq['c']['{id}'] > 0 else {}; sq['t'] = api.time; continue", j + 1, end + 1));
            }
            Op::End(id) => {
                let start = ops.iter().position(|x| matches!(x, Op::Loop(_, i) if i == id))
                    .ok_or_else(|| PyStopIteration::new_err(()))?;
                out.push(format!("        sq['c']['{id}'] = sq['c'].get('{id}', 1) - 1"));
                out.push(format!("        sq['i'] = {} if sq['c']['{id}'] > 0 else {}; sq['t'] = api.time; continue", start + 1, j + 1));
            }
        }
    }
    if restart {
        out.push("    sq['i'] = 0; sq['t'] = api.time".into());
    }
    out.push("    break".into());
    Ok((out, n))
}

#[pyfunction]
pub fn blocks_gen_sequence(py: Python<'_>, body: &Bound<'_, PyAny>, k: usize, restart: bool) -> R<(Vec<String>, usize)> {
    gen_sequence(py, body, k, restart)
}

fn label_format(py: Python<'_>, spec: &Bound<'_, PyAny>, t: &Bound<'_, PyAny>, s: &Bound<'_, PyAny>) -> R<String> {
    let label = spec.call_method1("get", ("label", t))?;
    let kw = PyDict::new_bound(py);
    let slots = spec.call_method1("get", ("slots", PyDict::new_bound(py)))?;
    for k2 in slots.iter()? {
        let k2 = k2?;
        kw.set_item(&k2, summary(py, &s.call_method1("get", (&k2,))?)?)?;
    }
    Ok(label.call_method("format", (), Some(&kw))?.str()?.to_string())
}

/// ブロックのプログラム → コンピューターが動かす Python のプログラム
#[pyfunction]
#[pyo3(signature = (prog, src_name="(ブロック)"))]
pub fn blocks_to_python(py: Python<'_>, prog: &Bound<'_, PyAny>, src_name: &str) -> R<String> {
    let header = PyString::new_bound(py, HEADER);
    let kw = PyDict::new_bound(py);
    kw.set_item("src", src_name)?;
    let head = header.call_method("format", (), Some(&kw))?.to_string();
    let mut lines: Vec<String> = vec![head];
    for l in ["def tick(api):", "    m = api.memory", "    if api.time - m.get('_last', -99.0) > 0.5:   # 起動し直したら最初から",
              "        m.clear()", "    m['_last'] = api.time", "    v = m.setdefault('vars', {})", "    first = '_t0' not in m",
              "    if first:", "        m['_t0'] = api.time", "        m['_timer'] = api.time", "    keys = api.input",
              "    prev = set(m.get('_keys', []))", "    m['_keys'] = sorted(keys)"] {
        lines.push(l.into());
    }
    let scripts: Vec<Bound<'_, PyAny>> = prog.call_method1("get", ("scripts", PyList::empty_bound(py)))?.iter()?.collect::<R<_>>()?;
    // 起動したとき（on_start）を先に（並べ替えは安定）
    let mut order: Vec<(usize, Bound<'_, PyAny>)> = Vec::new();
    for s in &scripts {
        let o = if s.call_method1("get", ("type",))?.eq("on_start")? { 0 } else { 1 };
        order.push((o, s.clone()));
    }
    order.sort_by_key(|x| x.0);
    for (k, (_, s)) in order.iter().enumerate() {
        let tobj = s.call_method1("get", ("type",))?;
        let t = tobj.downcast::<PyString>().map(|x| x.to_string()).unwrap_or_default();
        let spec = tbl(py, "STATEMENTS")?.call_method1("get", (&tobj, PyDict::new_bound(py)))?;
        lines.push(format!("    # --- {} ---", label_format(py, &spec, &tobj, s)?));
        let body_obj = s.call_method1("get", ("body", PyList::empty_bound(py)))?;
        let body = gen_body(py, &body_obj)?;
        let sbody = s.call_method1("get", ("body",))?;
        if has_wait(&sbody)? {
            let (seq, n) = gen_sequence(py, &sbody, k, t == "on_tick")?;
            let sk = qs(py, &format!("_sq{k}"))?;
            if t == "on_start" || t == "on_tick" {
                lines.extend(indent(seq));
            } else if t == "on_key" {
                lines.push(format!("    if {} in keys:", qs(py, &gsd(s, "key", "space")?)?));
                lines.extend(indent(indent(seq)));
            } else {
                let trig;
                let mut flag = String::new();
                if t == "on_key_down" {
                    let key = qs(py, &gsd(s, "key", "x")?)?;
                    trig = format!("{key} in keys and {key} not in prev");
                } else {
                    flag = qs(py, &format!("_w{k}"))?;
                    lines.push(format!("    _c = {}", gen_expr(py, &s.call_method1("get", ("cond",))?, "bool")?));
                    trig = format!("_c and not m.get({flag})");
                }
                lines.push(format!("    if {sk} not in m:"));
                lines.push(format!("        m[{sk}] = {{'i': {n}, 't': api.time, 'c': {{}}}}"));
                lines.push(format!("    if {trig} and m[{sk}]['i'] >= {n}:"));
                lines.push(format!("        m[{sk}]['i'] = 0"));
                lines.push(format!("        m[{sk}]['t'] = api.time"));
                if t == "on_when" {
                    lines.push(format!("    m[{flag}] = bool(_c)"));
                }
                lines.extend(indent(seq));
            }
            continue;
        }
        match t.as_str() {
            "on_start" => {
                lines.push("    if first:".into());
                lines.extend(indent(indent(body)));
            }
            "on_tick" => lines.extend(indent(body)),
            "on_key" => {
                lines.push(format!("    if {} in keys:", qs(py, &gsd(s, "key", "space")?)?));
                lines.extend(indent(indent(body)));
            }
            "on_key_down" => {
                let key = qs(py, &gsd(s, "key", "x")?)?;
                lines.push(format!("    if {key} in keys and {key} not in prev:"));
                lines.extend(indent(indent(body)));
            }
            "on_when" => {
                let flag = qs(py, &format!("_w{k}"))?;
                lines.push(format!("    _c = {}", gen_expr(py, &s.call_method1("get", ("cond",))?, "bool")?));
                lines.push(format!("    if _c and not m.get({flag}):"));
                lines.extend(indent(indent(body)));
                lines.push(format!("    m[{flag}] = bool(_c)"));
            }
            _ => {}
        }
    }
    if scripts.is_empty() {
        lines.push("    pass".into());
    }
    Ok(lines.join("\n") + "\n")
}
