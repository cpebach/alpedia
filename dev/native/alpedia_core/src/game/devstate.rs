//! 機器の状態の決まった形（スキーマ）。main/devstate.py の中身。
//!
//! 機器（ブロック）ごとの状態 d["state"] に入れてよい項目は、機器の種類ごとにここで決める。
//! - 保存する項目（saved）と、計算中だけ使う項目（saved でない：保存しない）を分ける
//! - 読み込み・設置のときに normalize で、足りない項目を既定値で埋め、型をそろえ、知らない項目を捨てる
//! - "_" で始まる項目は、実行中だけの内部用（スクリプトの実行環境など）。保存も検査もしない

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Float,
    Int,
    Bool,
    Str,
    Dict,
    List,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Float => "float",
            Kind::Int => "int",
            Kind::Bool => "bool",
            Kind::Str => "str",
            Kind::Dict => "dict",
            Kind::List => "list",
        }
    }
}

#[derive(Clone, Copy)]
pub enum Def {
    None,
    F(f64),
    B(bool),
    I(i64),
    EmptyDict,
    /// params[key]、無ければ値
    PF(&'static str, f64),
    PS(&'static str, &'static str),
    /// params["contents"] の複製（無ければ {}）
    PDictCopy(&'static str),
    /// st[key]、無ければ 0.0
    StF(&'static str),
    /// アクチュエーターの既定の動き（params["motion"] が piston ならピストン）
    Preset,
}

#[derive(Clone, Copy)]
pub struct F {
    pub name: &'static str,
    pub kind: Kind,
    pub def: Def,
    pub saved: bool,
    pub optional: bool,
}

const fn f(name: &'static str, kind: Kind, def: Def) -> F {
    F { name, kind, def, saved: true, optional: false }
}
const fn opt(name: &'static str, kind: Kind) -> F {
    F { name, kind, def: Def::None, saved: true, optional: true }
}
const fn tmp(name: &'static str, kind: Kind) -> F {
    F { name, kind, def: Def::None, saved: false, optional: true }
}

use Kind::*;

/// どの機器にもある項目
pub const COMMON: &[F] = &[opt("error", Str), opt("error_line", Int), opt("display", Dict), tmp("active", Bool)];

pub fn schema(dtype: &str) -> Option<Vec<F>> {
    Some(match dtype {
        // 推進機：lit＝燃えている。ignite＝点火の頼み。firing / throttle＝いまの出力 0〜1
        "thruster" => vec![f("lit", Bool, Def::B(false)), tmp("ignite", Bool), tmp("firing", Float), tmp("throttle", Float)],
        "tank" => vec![f("contents", Dict, Def::PDictCopy("contents"))],
        "pipe" => vec![],
        // バルブ：open＝開き具合 0〜1。set＝E で開け直すときの開き具合
        "valve" => vec![f("open", Float, Def::PF("open", 1.0)), opt("set", Float)],
        "battery" => vec![f("charge", Float, Def::PF("charge", 0.0))],
        "generator" => vec![f("enabled", Bool, Def::B(true))],
        "crank" => vec![opt("pending", Float)],
        "computer" => vec![f("script", Str, Def::PS("script", "")), f("running", Bool, Def::B(false)),
                        f("memory", Dict, Def::EmptyDict), opt("blocks", Str)],
        "actuator" => vec![tmp("attached", Bool), f("ext", Float, Def::F(0.0)), f("ext_target", Float, Def::StF("ext")),
                        f("ext_speed", Float, Def::PF("speed", 0.5)), f("angle", Float, Def::F(0.0)),
                        f("angle_target", Float, Def::StF("angle")), f("rot_speed", Float, Def::PF("rot_speed", 90.0)),
                        f("preset", Dict, Def::Preset), f("hinge", Int, Def::I(-1))],
        "processor" => vec![f("progress", Float, Def::F(0.0)), f("enabled", Bool, Def::B(true)), opt("next", Int),
                         f("store", Dict, Def::EmptyDict), tmp("working", Bool)],
        "seat" => vec![],
        "anchor" => vec![],
        "light" => vec![f("enabled", Bool, Def::B(true)), opt("color", List), opt("blink", Float), opt("group", Str)],
        "air_vent" => vec![f("mode", Str, Def::PS("mode", "fill")), f("enabled", Bool, Def::B(true)), tmp("cavity", Bool)],
        "antenna" => vec![f("enabled", Bool, Def::B(true))],
        "custom" => vec![f("enabled", Bool, Def::B(true)), f("data", Dict, Def::EmptyDict)],
        _ => return None,
    })
}

/// その種類の項目（COMMON の後に種類ごとの項目。同じ名前は種類ごとのほうが勝つ）
pub fn fields(dtype: &str) -> Vec<F> {
    let mut out: Vec<F> = COMMON.to_vec();
    for x in schema(dtype).unwrap_or_default() {
        if let Some(k) = out.iter().position(|y| y.name == x.name) {
            out[k] = x;
        } else {
            out.push(x);
        }
    }
    out
}

fn preset(py: Python<'_>, params: &Bound<'_, PyDict>) -> PyResult<PyObject> {
    let d = PyDict::new_bound(py);
    let piston = params.get_item("motion")?.map(|m| m.extract::<String>().map(|s| s == "piston").unwrap_or(false)).unwrap_or(false);
    if piston {
        d.set_item("action", "extend")?;
        d.set_item("amount", 1.0)?;
        d.set_item("speed", 0.5)?;
    } else {
        d.set_item("action", "rotate")?;
        d.set_item("amount", 90.0)?;
        d.set_item("speed", 90.0)?;
    }
    Ok(d.into_any().unbind())
}

fn make_default(py: Python<'_>, f: &F, params: &Bound<'_, PyDict>, st: &Bound<'_, PyDict>) -> PyResult<PyObject> {
    Ok(match f.def {
        Def::None => py.None(),
        Def::F(x) => x.into_py(py),
        Def::B(x) => x.into_py(py),
        Def::I(x) => x.into_py(py),
        Def::EmptyDict => PyDict::new_bound(py).into_any().unbind(),
        Def::PF(k, x) => match params.get_item(k)? {
            Some(v) => v.unbind(),
            None => x.into_py(py),
        },
        Def::PS(k, x) => match params.get_item(k)? {
            Some(v) => v.unbind(),
            None => x.into_py(py),
        },
        Def::PDictCopy(k) => {
            let d = PyDict::new_bound(py);
            if let Some(v) = params.get_item(k)? {
                d.update(v.downcast::<pyo3::types::PyMapping>()?)?;
            }
            d.into_any().unbind()
        }
        Def::StF(k) => match st.get_item(k)? {
            Some(v) => v.unbind(),
            None => 0.0f64.into_py(py),
        },
        Def::Preset => preset(py, params)?,
    })
}

/// 値をその型にそろえる（Python の float() / int() / bool() / str() / list() と同じ）。dict でない物を dict にはしない
pub fn coerce<'py>(py: Python<'py>, kind: Kind, v: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if v.is_none() {
        return Ok(v.clone());
    }
    let b = py.import_bound("builtins")?;
    Ok(match kind {
        Kind::Float => b.getattr("float")?.call1((v,))?,
        Kind::Int => b.getattr("int")?.call1((v,))?,
        Kind::Bool => v.is_truthy()?.into_py(py).into_bound(py),
        Kind::Str => v.str()?.into_any(),
        Kind::Dict => {
            if !v.is_instance_of::<PyDict>() {
                return Err(pyo3::exceptions::PyTypeError::new_err("dict ではありません"));
            }
            v.clone()
        }
        Kind::List => PyList::new_bound(py, v.iter()?.collect::<PyResult<Vec<_>>>()?).into_any(),
    })
}

fn is_coerce_err(py: Python<'_>, e: &PyErr) -> bool {
    e.is_instance_of::<pyo3::exceptions::PyTypeError>(py) || e.is_instance_of::<pyo3::exceptions::PyValueError>(py)
}

fn warn(py: Python<'_>, msg: String) -> PyResult<()> {
    py.import_bound("logging")?.getattr("getLogger")?.call1(("alpedia.devstate",))?.call_method1("warning", (msg,))?;
    Ok(())
}

fn as_dict<'py>(py: Python<'py>, x: Option<&Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyDict>> {
    match x {
        Some(v) if !v.is_none() => {
            let d = PyDict::new_bound(py);
            d.update(v.downcast::<pyo3::types::PyMapping>()?)?;
            Ok(d)
        }
        _ => Ok(PyDict::new_bound(py)),
    }
}

fn repr(v: &Bound<'_, PyAny>) -> String {
    v.repr().map(|s| s.to_string()).unwrap_or_else(|_| "?".into())
}

/// 状態をこの種類の決まった形にした新しい辞書を返す（元の辞書は変えない）
#[pyfunction]
#[pyo3(signature = (dtype, params, state, r#where=""))]
pub fn devstate_normalize<'py>(py: Python<'py>, dtype: &str, params: Option<&Bound<'py, PyAny>>, state: Option<&Bound<'py, PyAny>>,
                               r#where: &str) -> PyResult<Bound<'py, PyDict>> {
    let st = as_dict(py, state)?;
    let params = as_dict(py, params)?;
    let fs = fields(dtype);
    let out = PyDict::new_bound(py);
    for (k, v) in st.iter() {
        let ks: String = k.extract()?;
        if ks.starts_with('_') {
            out.set_item(&k, v)?;
            continue;
        }
        match fs.iter().find(|f| f.name == ks) {
            None => warn(py, format!("機器 {dtype}{where} の状態の知らない項目 '{ks}' を捨てました"))?,
            Some(f) => match coerce(py, f.kind, &v) {
                Ok(x) => out.set_item(&k, x)?,
                Err(e) if is_coerce_err(py, &e) => warn(py, format!(
                    "機器 {dtype}{where} の状態 '{ks}' の値 {} が {} でないので既定値にしました", repr(&v), f.kind.name()))?,
                Err(e) => return Err(e),
            },
        }
    }
    for f in &fs {
        if !f.optional && !out.contains(f.name)? {
            let d = make_default(py, f, &params, &out)?;
            out.set_item(f.name, coerce(py, f.kind, d.bind(py))?)?;
        }
    }
    Ok(out)
}

/// 足りない項目を既定値で埋める（その辞書をそのまま書き換える。動いている機器用）
#[pyfunction]
#[pyo3(signature = (dtype, params, st))]
pub fn devstate_fill<'py>(py: Python<'py>, dtype: &str, params: Option<&Bound<'py, PyAny>>, st: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyDict>> {
    let params = as_dict(py, params)?;
    for f in fields(dtype) {
        if !f.optional && !st.contains(f.name)? {
            let d = make_default(py, &f, &params, st)?;
            st.set_item(f.name, coerce(py, f.kind, d.bind(py))?)?;
        }
    }
    Ok(st.clone())
}

/// 保存する分だけ（計算中だけの項目と "_" で始まる項目を除く）
#[pyfunction]
pub fn devstate_to_saved<'py>(py: Python<'py>, dtype: &str, state: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyDict>> {
    let fs = fields(dtype);
    let out = PyDict::new_bound(py);
    for (k, v) in state.iter() {
        let ks: String = k.extract()?;
        if ks.starts_with('_') {
            continue;
        }
        if fs.iter().find(|f| f.name == ks).map_or(true, |f| f.saved) {
            out.set_item(k, v)?;
        }
    }
    Ok(out)
}

/// 決まった形になっているか（テスト用）：問題のリスト
#[pyfunction]
pub fn devstate_check(py: Python<'_>, dtype: &str, state: &Bound<'_, PyDict>) -> PyResult<Vec<String>> {
    let fs = fields(dtype);
    let mut bad = Vec::new();
    for (k, v) in state.iter() {
        let ks: String = k.extract()?;
        if ks.starts_with('_') {
            continue;
        }
        let Some(f) = fs.iter().find(|f| f.name == ks) else {
            bad.push(format!("知らない項目 {ks}"));
            continue;
        };
        if v.is_none() {
            continue;
        }
        match coerce(py, f.kind, &v) {
            Ok(x) => {
                if !x.eq(&v)? {
                    bad.push(format!("{ks} の型が {} でない: {}", f.kind.name(), repr(&v)));
                }
            }
            Err(e) if is_coerce_err(py, &e) => bad.push(format!("{ks} の型が {} でない: {}", f.kind.name(), repr(&v))),
            Err(e) => return Err(e),
        }
    }
    for f in &fs {
        if !f.optional && !state.contains(f.name)? {
            bad.push(format!("{} がない", f.name));
        }
    }
    Ok(bad)
}

/// その種類の項目：{名前: (型, 保存するか, 無くてよいか)}
#[pyfunction]
pub fn devstate_fields<'py>(py: Python<'py>, dtype: &str) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new_bound(py);
    for f in fields(dtype) {
        out.set_item(f.name, (f.kind.name(), f.saved, f.optional))?;
    }
    Ok(out)
}
