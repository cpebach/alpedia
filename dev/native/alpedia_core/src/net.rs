//! LAN マルチプレイの通信（main/net.py の中身）：メッセージ ⇔ バイト列、TCP のつながり、受け付け、
//! LAN の中のワールドの知らせ（UDP）。送る・受けるは Rust のスレッドで行い、Python は inbox から取り出すだけ。
//!
//! 形式：[長さ 4 バイト][フラグ 1 バイト][JSON の長さ 4][頭の長さ 4][JSON][頭（生のデータの大きさの JSON）][生のデータ]
//! （大きいものは zlib で圧縮。フラグの 1 ビット目）。numpy の配列・bytes は JSON の外に生のまま載せる。

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use pyo3::exceptions::{PyConnectionRefusedError, PyOSError, PyTimeoutError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyBool, PyByteArray, PyBytes, PyDict, PyFloat, PyInt, PyList, PyMemoryView, PyString, PyTuple};

pub const MAGIC: &str = "alpedia-lan";
pub const PROTO: i64 = 1;

// ---------------------------------------------------------------------------
// JSON（NaN・Infinity も読み書きする。Python の json と同じ）
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub enum Val {
    Null,
    Bool(bool),
    Int(i64),
    Big(String),
    Float(f64),
    Str(String),
    List(Vec<Val>),
    Dict(Vec<(String, Val)>),
}

fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_float(out: &mut String, f: f64) {
    if f.is_nan() {
        out.push_str("NaN");
    } else if f.is_infinite() {
        out.push_str(if f > 0.0 { "Infinity" } else { "-Infinity" });
    } else {
        let s = format!("{f:?}");
        out.push_str(&s);
    }
}

pub fn write_val(out: &mut String, v: &Val) {
    match v {
        Val::Null => out.push_str("null"),
        Val::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Val::Int(i) => out.push_str(&i.to_string()),
        Val::Big(s) => out.push_str(s),
        Val::Float(f) => write_float(out, *f),
        Val::Str(s) => write_str(out, s),
        Val::List(l) => {
            out.push('[');
            for (k, x) in l.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                write_val(out, x);
            }
            out.push(']');
        }
        Val::Dict(d) => {
            out.push('{');
            for (k, (key, x)) in d.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                write_str(out, key);
                out.push(':');
                write_val(out, x);
            }
            out.push('}');
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("JSON を読めません（{what}、{} 文字目）", self.i))
    }
    fn lit(&mut self, w: &str) -> bool {
        if self.s[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            true
        } else {
            false
        }
    }
    fn val(&mut self, depth: usize) -> Result<Val, String> {
        if depth > 512 {
            return self.err("入れ子が深すぎます");
        }
        self.ws();
        if self.i >= self.s.len() {
            return self.err("途中で終わっています");
        }
        let c0 = self.s[self.i];
        match c0 {
            b'n' if self.lit("null") => Ok(Val::Null),
            b't' if self.lit("true") => Ok(Val::Bool(true)),
            b'f' if self.lit("false") => Ok(Val::Bool(false)),
            b'N' if self.lit("NaN") => Ok(Val::Float(f64::NAN)),
            b'I' if self.lit("Infinity") => Ok(Val::Float(f64::INFINITY)),
            b'"' => Ok(Val::Str(self.string()?)),
            b'[' => {
                self.i += 1;
                let mut l = Vec::new();
                self.ws();
                if self.i < self.s.len() && self.s[self.i] == b']' {
                    self.i += 1;
                    return Ok(Val::List(l));
                }
                loop {
                    l.push(self.val(depth + 1)?);
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Val::List(l));
                        }
                        _ => return self.err("配列"),
                    }
                }
            }
            b'{' => {
                self.i += 1;
                let mut d = Vec::new();
                self.ws();
                if self.i < self.s.len() && self.s[self.i] == b'}' {
                    self.i += 1;
                    return Ok(Val::Dict(d));
                }
                loop {
                    self.ws();
                    if self.s.get(self.i) != Some(&b'"') {
                        return self.err("辞書の名前");
                    }
                    let k = self.string()?;
                    self.ws();
                    if self.s.get(self.i) != Some(&b':') {
                        return self.err("辞書の :");
                    }
                    self.i += 1;
                    let v = self.val(depth + 1)?;
                    d.push((k, v));
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Val::Dict(d));
                        }
                        _ => return self.err("辞書"),
                    }
                }
            }
            _ => self.number(),
        }
    }
    fn number(&mut self) -> Result<Val, String> {
        if self.lit("-Infinity") {
            return Ok(Val::Float(f64::NEG_INFINITY));
        }
        let st = self.i;
        let mut float = false;
        while self.i < self.s.len() {
            match self.s[self.i] {
                b'0'..=b'9' | b'-' | b'+' => {}
                b'.' | b'e' | b'E' => float = true,
                _ => break,
            }
            self.i += 1;
        }
        let t = std::str::from_utf8(&self.s[st..self.i]).map_err(|_| "数".to_string())?;
        if t.is_empty() {
            return self.err("値");
        }
        if float {
            t.parse::<f64>().map(Val::Float).or_else(|_| self.err("数"))
        } else {
            match t.parse::<i64>() {
                Ok(v) => Ok(Val::Int(v)),
                Err(_) if t.trim_start_matches('-').bytes().all(|b| b.is_ascii_digit()) => Ok(Val::Big(t.to_string())),
                Err(_) => self.err("数"),
            }
        }
    }
    fn hex4(&mut self) -> Result<u32, String> {
        if self.i + 4 > self.s.len() {
            return self.err("\\u");
        }
        let h = std::str::from_utf8(&self.s[self.i..self.i + 4]).map_err(|_| "\\u".to_string())?;
        let v = u32::from_str_radix(h, 16).map_err(|_| "\\u".to_string())?;
        self.i += 4;
        Ok(v)
    }
    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let st = self.i;
            while self.i < self.s.len() && self.s[self.i] != b'"' && self.s[self.i] != b'\\' {
                self.i += 1;
            }
            out.push_str(std::str::from_utf8(&self.s[st..self.i]).map_err(|_| "文字列が UTF-8 ではありません".to_string())?);
            if self.i >= self.s.len() {
                return self.err("文字列が閉じていません");
            }
            if self.s[self.i] == b'"' {
                self.i += 1;
                return Ok(out);
            }
            self.i += 1;
            let c = *self.s.get(self.i).ok_or("文字列が閉じていません")?;
            self.i += 1;
            match c {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{08}'),
                b'f' => out.push('\u{0c}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    let mut cp = self.hex4()?;
                    if (0xD800..0xDC00).contains(&cp) && self.s[self.i..].starts_with(b"\\u") {
                        let save = self.i;
                        self.i += 2;
                        let lo = self.hex4()?;
                        if (0xDC00..0xE000).contains(&lo) {
                            cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                        } else {
                            self.i = save;
                        }
                    }
                    out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                }
                _ => return self.err("\\ の後"),
            }
        }
    }
}

pub fn parse_json(s: &[u8]) -> Result<Val, String> {
    let mut p = Parser { s, i: 0 };
    let v = p.val(0)?;
    p.ws();
    if p.i != s.len() {
        return p.err("後ろに余計なもの");
    }
    Ok(v)
}

// ---------------------------------------------------------------------------
// Python ⇔ メッセージ
// ---------------------------------------------------------------------------
struct NpTypes {
    ndarray: PyObject,
    integer: PyObject,
    floating: PyObject,
    bool_: PyObject,
    frombuffer: PyObject,
    ascontig: PyObject,
}

static NP: GILOnceCell<NpTypes> = GILOnceCell::new();

fn np(py: Python<'_>) -> PyResult<&NpTypes> {
    NP.get_or_try_init(py, || {
        let m = py.import_bound("numpy")?;
        Ok(NpTypes {
            ndarray: m.getattr("ndarray")?.unbind(),
            integer: m.getattr("integer")?.unbind(),
            floating: m.getattr("floating")?.unbind(),
            bool_: m.getattr("bool_")?.unbind(),
            frombuffer: m.getattr("frombuffer")?.unbind(),
            ascontig: m.getattr("ascontiguousarray")?.unbind(),
        })
    })
}

fn to_val(x: &Bound<'_, PyAny>, blobs: &mut Vec<Vec<u8>>, depth: usize) -> PyResult<Val> {
    let py = x.py();
    if depth > 512 {
        return Err(PyValueError::new_err("入れ子が深すぎます"));
    }
    if x.is_none() {
        return Ok(Val::Null);
    }
    if x.is_instance_of::<PyBool>() {
        return Ok(Val::Bool(x.extract::<bool>()?));
    }
    if x.is_instance_of::<PyInt>() {
        return Ok(match x.extract::<i64>() {
            Ok(v) => Val::Int(v),
            Err(_) => Val::Big(x.str()?.to_string()),
        });
    }
    if x.is_instance_of::<PyFloat>() {
        return Ok(Val::Float(x.extract::<f64>()?));
    }
    if let Ok(s) = x.downcast::<PyString>() {
        return Ok(Val::Str(s.to_cow()?.into_owned()));
    }
    if let Ok(d) = x.downcast::<PyDict>() {
        let mut out = Vec::with_capacity(d.len());
        for (k, v) in d.iter() {
            let key = if let Ok(s) = k.downcast::<PyString>() {
                s.to_cow()?.into_owned()
            } else {
                k.str()?.to_string()
            };
            out.push((key, to_val(&v, blobs, depth + 1)?));
        }
        return Ok(Val::Dict(out));
    }
    if let Ok(l) = x.downcast::<PyList>() {
        let mut out = Vec::with_capacity(l.len());
        for v in l.iter() {
            out.push(to_val(&v, blobs, depth + 1)?);
        }
        return Ok(Val::List(out));
    }
    if let Ok(t) = x.downcast::<PyTuple>() {
        let mut out = Vec::with_capacity(t.len());
        for v in t.iter() {
            out.push(to_val(&v, blobs, depth + 1)?);
        }
        return Ok(Val::List(out));
    }
    let n = np(py)?;
    if x.is_instance(n.ndarray.bind(py))? {
        let a = n.ascontig.bind(py).call1((x,))?;
        let b: Vec<u8> = a.call_method0("tobytes")?.extract()?;
        let dt: String = a.getattr("dtype")?.getattr("str")?.extract()?;
        let shape: Vec<i64> = a.getattr("shape")?.extract()?;
        blobs.push(b);
        return Ok(Val::Dict(vec![(
            "__nd__".into(),
            Val::List(vec![Val::Int(blobs.len() as i64 - 1), Val::Str(dt), Val::List(shape.into_iter().map(Val::Int).collect())]),
        )]));
    }
    if let Ok(b) = x.downcast::<PyBytes>() {
        blobs.push(b.as_bytes().to_vec());
        return Ok(Val::Dict(vec![("__b__".into(), Val::Int(blobs.len() as i64 - 1))]));
    }
    if x.is_instance_of::<PyByteArray>() || x.is_instance_of::<PyMemoryView>() {
        let b: Vec<u8> = py.get_type_bound::<PyBytes>().call1((x,))?.extract()?;
        blobs.push(b);
        return Ok(Val::Dict(vec![("__b__".into(), Val::Int(blobs.len() as i64 - 1))]));
    }
    if x.is_instance(n.bool_.bind(py))? {
        return Ok(Val::Bool(x.is_truthy()?));
    }
    if x.is_instance(n.integer.bind(py))? {
        return to_val(&x.call_method0("__int__")?, blobs, depth);
    }
    if x.is_instance(n.floating.bind(py))? {
        return Ok(Val::Float(x.call_method0("__float__")?.extract()?));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "Object of type {} is not JSON serializable",
        x.get_type().name()?
    )))
}

/// Python のメッセージ → 送るバイト列（先頭の長さ・フラグ込み）
pub fn encode_py(msg: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let mut blobs = Vec::new();
    let v = to_val(msg, &mut blobs, 0)?;
    let mut js = String::new();
    write_val(&mut js, &v);
    let head = format!("[{}]", blobs.iter().map(|b| b.len().to_string()).collect::<Vec<_>>().join(", "));
    let total: usize = blobs.iter().map(|b| b.len()).sum();
    let mut body = Vec::with_capacity(8 + js.len() + head.len() + total);
    body.extend_from_slice(&(js.len() as u32).to_le_bytes());
    body.extend_from_slice(&(head.len() as u32).to_le_bytes());
    body.extend_from_slice(js.as_bytes());
    body.extend_from_slice(head.as_bytes());
    for b in &blobs {
        body.extend_from_slice(b);
    }
    let mut flag = 0u8;
    if body.len() > 2048 {
        let mut e = ZlibEncoder::new(Vec::with_capacity(body.len() / 2), Compression::new(1));
        e.write_all(&body).map_err(|e| PyOSError::new_err(e.to_string()))?;
        let z = e.finish().map_err(|e| PyOSError::new_err(e.to_string()))?;
        if (z.len() as f64) < body.len() as f64 * 0.9 {
            body = z;
            flag = 1;
        }
    }
    let mut out = Vec::with_capacity(body.len() + 5);
    out.extend_from_slice(&((body.len() + 1) as u32).to_le_bytes());
    out.push(flag);
    out.extend_from_slice(&body);
    Ok(out)
}

/// 受けたメッセージ（Python に渡す前の形）
pub struct Msg {
    val: Val,
    blobs: Vec<Vec<u8>>,
}

/// 受けた本体（フラグの後ろ）→ メッセージ（GIL なしでできるところまで）
pub fn decode_raw(flag: u8, body: &[u8]) -> Result<Msg, String> {
    let owned;
    let body: &[u8] = if flag & 1 != 0 {
        let mut d = ZlibDecoder::new(body);
        let mut v = Vec::new();
        d.read_to_end(&mut v).map_err(|e| format!("展開できません: {e}"))?;
        owned = v;
        &owned
    } else {
        body
    };
    if body.len() < 8 {
        return Err("短すぎるメッセージ".into());
    }
    let jl = u32::from_le_bytes(body[0..4].try_into().unwrap()) as usize;
    let hl = u32::from_le_bytes(body[4..8].try_into().unwrap()) as usize;
    if 8 + jl + hl > body.len() {
        return Err("メッセージの長さが合いません".into());
    }
    let val = parse_json(&body[8..8 + jl])?;
    let sizes = match parse_json(&body[8 + jl..8 + jl + hl])? {
        Val::List(l) => l
            .into_iter()
            .map(|x| match x {
                Val::Int(i) if i >= 0 => Ok(i as usize),
                _ => Err("生のデータの大きさが読めません".to_string()),
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("生のデータの大きさが読めません".into()),
    };
    let mut off = 8 + jl + hl;
    let mut blobs = Vec::with_capacity(sizes.len());
    for s in sizes {
        if off + s > body.len() {
            return Err("生のデータが足りません".into());
        }
        blobs.push(body[off..off + s].to_vec());
        off += s;
    }
    Ok(Msg { val, blobs })
}

fn val_to_py(py: Python<'_>, v: &Val, blobs: &[Vec<u8>]) -> PyResult<PyObject> {
    Ok(match v {
        Val::Null => py.None(),
        Val::Bool(b) => b.into_py(py),
        Val::Int(i) => i.into_py(py),
        Val::Big(s) => py.get_type_bound::<PyInt>().call1((s.as_str(),))?.unbind(),
        Val::Float(f) => f.into_py(py),
        Val::Str(s) => s.into_py(py),
        Val::List(l) => {
            let out = PyList::empty_bound(py);
            for x in l {
                out.append(val_to_py(py, x, blobs)?)?;
            }
            out.into_any().unbind()
        }
        Val::Dict(d) => {
            if d.len() == 1 {
                let (k, x) = &d[0];
                if k == "__nd__" {
                    if let Val::List(a) = x {
                        if let (Some(Val::Int(i)), Some(Val::Str(dt)), Some(Val::List(sh))) = (a.first(), a.get(1), a.get(2)) {
                            let b = blobs.get(*i as usize).ok_or_else(|| PyValueError::new_err("配列のデータがありません"))?;
                            let shape: Vec<i64> = sh.iter().map(|s| if let Val::Int(n) = s { *n } else { 0 }).collect();
                            let n = np(py)?;
                            let arr = n.frombuffer.bind(py).call1((PyBytes::new_bound(py, b), dt.as_str()))?;
                            return Ok(arr.call_method1("reshape", (shape,))?.call_method0("copy")?.unbind());
                        }
                    }
                }
                if k == "__b__" {
                    if let Val::Int(i) = x {
                        let b = blobs.get(*i as usize).ok_or_else(|| PyValueError::new_err("データがありません"))?;
                        return Ok(PyBytes::new_bound(py, b).into_any().unbind());
                    }
                }
            }
            let out = PyDict::new_bound(py);
            for (k, x) in d {
                out.set_item(k, val_to_py(py, x, blobs)?)?;
            }
            out.into_any().unbind()
        }
    })
}

/// 送るバイト列（先頭 5 バイト込み）
#[pyfunction]
pub fn net_encode<'py>(py: Python<'py>, msg: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    let b = encode_py(msg)?;
    Ok(PyBytes::new_bound(py, &b))
}

/// フラグと本体 → メッセージ
#[pyfunction]
pub fn net_decode(py: Python<'_>, flag: u8, body: &[u8]) -> PyResult<PyObject> {
    let m = py.allow_threads(|| decode_raw(flag, body)).map_err(PyValueError::new_err)?;
    val_to_py(py, &m.val, &m.blobs)
}

/// JSON の文字列 → Python（LAN の知らせなどに）
fn json_to_py(py: Python<'_>, s: &str) -> PyResult<PyObject> {
    let v = parse_json(s.as_bytes()).map_err(PyValueError::new_err)?;
    val_to_py(py, &v, &[])
}

// ---------------------------------------------------------------------------
// inbox：いくつものつながりの受けたものを1列に
// ---------------------------------------------------------------------------
enum Ev {
    Msg(Msg),
    Open(String),
    Closed(Option<String>),
}

struct InboxInner {
    q: Mutex<VecDeque<(Arc<ConnInner>, Ev)>>,
    cv: Condvar,
}

impl InboxInner {
    fn push(&self, c: Arc<ConnInner>, e: Ev) {
        self.q.lock().unwrap().push_back((c, e));
        self.cv.notify_all();
    }
}

static EMPTY: GILOnceCell<PyObject> = GILOnceCell::new();

fn empty_err(py: Python<'_>) -> PyErr {
    match EMPTY.get_or_try_init(py, || -> PyResult<PyObject> { Ok(py.import_bound("queue")?.getattr("Empty")?.unbind()) }) {
        Ok(t) => PyErr::from_type_bound(t.bind(py).downcast::<pyo3::types::PyType>().unwrap().clone(), ()),
        Err(e) => e,
    }
}

/// 受けたもの (つながり, メッセージ) の列。queue.Queue と同じ get / get_nowait（空なら queue.Empty）
#[pyclass(module = "alpedia_core")]
pub struct Inbox {
    inner: Arc<InboxInner>,
}

fn ev_to_py(py: Python<'_>, c: Arc<ConnInner>, e: Ev) -> PyResult<PyObject> {
    let conn = Py::new(py, Conn { inner: c })?;
    let msg: PyObject = match e {
        Ev::Msg(m) => val_to_py(py, &m.val, &m.blobs)?,
        Ev::Open(addr) => {
            let d = PyDict::new_bound(py);
            d.set_item("t", "_open")?;
            d.set_item("addr", addr)?;
            d.into_any().unbind()
        }
        Ev::Closed(why) => {
            let d = PyDict::new_bound(py);
            d.set_item("t", "_closed")?;
            d.set_item("why", why)?;
            d.into_any().unbind()
        }
    };
    Ok(PyTuple::new_bound(py, [conn.into_any(), msg]).into_any().unbind())
}

#[pymethods]
impl Inbox {
    #[new]
    fn new() -> Self {
        Inbox { inner: Arc::new(InboxInner { q: Mutex::new(VecDeque::new()), cv: Condvar::new() }) }
    }

    fn get_nowait(&self, py: Python<'_>) -> PyResult<PyObject> {
        let item = self.inner.q.lock().unwrap().pop_front();
        match item {
            Some((c, e)) => ev_to_py(py, c, e),
            None => Err(empty_err(py)),
        }
    }

    #[pyo3(signature = (block=true, timeout=None))]
    fn get(&self, py: Python<'_>, block: bool, timeout: Option<f64>) -> PyResult<PyObject> {
        let inner = self.inner.clone();
        let item = py.allow_threads(move || {
            let end = timeout.map(|t| Instant::now() + Duration::from_secs_f64(t.max(0.0)));
            let mut q = inner.q.lock().unwrap();
            loop {
                if let Some(x) = q.pop_front() {
                    return Some(x);
                }
                if !block {
                    return None;
                }
                match end {
                    None => q = inner.cv.wait_timeout(q, Duration::from_millis(200)).unwrap().0,
                    Some(e) => {
                        let now = Instant::now();
                        if now >= e {
                            return None;
                        }
                        q = inner.cv.wait_timeout(q, e - now).unwrap().0;
                    }
                }
            }
        });
        match item {
            Some((c, e)) => ev_to_py(py, c, e),
            None => Err(empty_err(py)),
        }
    }

    fn qsize(&self) -> usize {
        self.inner.q.lock().unwrap().len()
    }

    fn empty(&self) -> bool {
        self.inner.q.lock().unwrap().is_empty()
    }
}

// ---------------------------------------------------------------------------
// 1本のつながり
// ---------------------------------------------------------------------------
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct ConnInner {
    id: u64,
    tag: String,
    stream: TcpStream,
    alive: AtomicBool,
    error: Mutex<Option<String>>,
    sent: AtomicU64,
    recv: AtomicU64,
    pending: AtomicUsize,
    tx: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    rx: Mutex<Option<mpsc::Receiver<Vec<u8>>>>,
    inbox: Arc<InboxInner>,
    started: AtomicBool,
}

impl ConnInner {
    fn new(stream: TcpStream, tag: String, inbox: Arc<InboxInner>) -> Arc<Self> {
        let _ = stream.set_nodelay(true);
        let _ = socket2::SockRef::from(&stream).set_keepalive(true);
        let (tx, rx) = mpsc::channel();
        Arc::new(ConnInner {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            tag,
            stream,
            alive: AtomicBool::new(true),
            error: Mutex::new(None),
            sent: AtomicU64::new(0),
            recv: AtomicU64::new(0),
            pending: AtomicUsize::new(0),
            tx: Mutex::new(Some(tx)),
            rx: Mutex::new(Some(rx)),
            inbox,
            started: AtomicBool::new(false),
        })
    }

    fn set_error(&self, why: Option<String>) {
        let mut e = self.error.lock().unwrap();
        if e.is_none() {
            *e = why;
        }
    }

    fn close(&self, why: Option<String>) {
        if !self.alive.swap(false, Ordering::SeqCst) {
            return;
        }
        self.set_error(why);
        self.tx.lock().unwrap().take();
        let _ = self.stream.shutdown(Shutdown::Both);
    }

    fn start(self: &Arc<Self>) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        std::thread::Builder::new()
            .name("net-recv".into())
            .spawn(move || me.reader())
            .ok();
        let me = self.clone();
        let rx = self.rx.lock().unwrap().take();
        if let Some(rx) = rx {
            std::thread::Builder::new()
                .name("net-send".into())
                .spawn(move || me.writer(rx))
                .ok();
        }
    }

    fn reader(self: Arc<Self>) {
        let mut s = match self.stream.try_clone() {
            Ok(s) => s,
            Err(e) => {
                self.set_error(Some(e.to_string()));
                self.close(None);
                return;
            }
        };
        let res: Result<(), String> = (|| {
            while self.alive.load(Ordering::SeqCst) {
                let mut h = [0u8; 5];
                read_exact(&mut s, &mut h)?;
                let ln = u32::from_le_bytes(h[0..4].try_into().unwrap()) as usize;
                if ln == 0 {
                    return Err("長さ 0 のメッセージ".into());
                }
                let mut body = vec![0u8; ln - 1];
                read_exact(&mut s, &mut body)?;
                self.recv.fetch_add(ln as u64 + 4, Ordering::Relaxed);
                let m = decode_raw(h[4], &body)?;
                self.inbox.push(self.clone(), Ev::Msg(m));
            }
            Ok(())
        })();
        let was = self.alive.load(Ordering::SeqCst);
        if let Err(e) = res {
            if was {
                self.set_error(Some(e));
            }
        }
        self.close(None);
        if was {
            let why = self.error.lock().unwrap().clone();
            self.inbox.push(self.clone(), Ev::Closed(why));
        }
    }

    fn writer(self: Arc<Self>, rx: mpsc::Receiver<Vec<u8>>) {
        let mut s = match self.stream.try_clone() {
            Ok(s) => s,
            Err(_) => return,
        };
        while let Ok(data) = rx.recv() {
            let r = s.write_all(&data);
            self.pending.fetch_sub(1, Ordering::SeqCst);
            match r {
                Ok(()) => {
                    self.sent.fetch_add(data.len() as u64, Ordering::Relaxed);
                }
                Err(e) => {
                    if self.alive.load(Ordering::SeqCst) {
                        self.set_error(Some(e.to_string()));
                    }
                    self.close(None);
                    break;
                }
            }
        }
    }
}

fn read_exact(s: &mut TcpStream, buf: &mut [u8]) -> Result<(), String> {
    let mut got = 0;
    while got < buf.len() {
        match s.read(&mut buf[got..]) {
            Ok(0) => return Err("相手が切断しました".into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

/// TCP のつながり1本。send は誰からでも呼べる（送るスレッドが順に送る）。受けたものは inbox へ
#[pyclass(module = "alpedia_core", frozen)]
pub struct Conn {
    inner: Arc<ConnInner>,
}

#[pymethods]
impl Conn {
    /// メッセージを送る（Python の値は呼んだスレッドでバイト列にする。できないものが入っていたら、つながりを閉じる）
    fn send(&self, msg: &Bound<'_, PyAny>) -> PyResult<()> {
        let c = &self.inner;
        if !c.alive.load(Ordering::SeqCst) {
            return Ok(());
        }
        match encode_py(msg) {
            Ok(data) => {
                if let Some(tx) = c.tx.lock().unwrap().as_ref() {
                    c.pending.fetch_add(1, Ordering::SeqCst);
                    if tx.send(data).is_err() {
                        c.pending.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                Ok(())
            }
            Err(e) => {
                c.close(Some(format!("送れないメッセージ: {e}")));
                Ok(())
            }
        }
    }

    fn pending(&self) -> usize {
        self.inner.pending.load(Ordering::SeqCst)
    }

    #[pyo3(signature = (why=None))]
    fn close(&self, why: Option<String>) {
        self.inner.close(why);
    }

    fn start(&self) {
        self.inner.start();
    }

    #[getter]
    fn alive(&self) -> bool {
        self.inner.alive.load(Ordering::SeqCst)
    }

    #[getter]
    fn error(&self) -> Option<String> {
        self.inner.error.lock().unwrap().clone()
    }

    #[getter]
    fn tag(&self) -> String {
        self.inner.tag.clone()
    }

    #[getter]
    fn sent_bytes(&self) -> u64 {
        self.inner.sent.load(Ordering::Relaxed)
    }

    #[getter]
    fn recv_bytes(&self) -> u64 {
        self.inner.recv.load(Ordering::Relaxed)
    }

    #[getter]
    fn inbox(&self) -> Inbox {
        Inbox { inner: self.inner.inbox.clone() }
    }

    fn __hash__(&self) -> u64 {
        self.inner.id
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.downcast::<Conn>().map(|o| o.get().inner.id == self.inner.id).unwrap_or(false)
    }

    fn __repr__(&self) -> String {
        format!("<Conn {} {}>", self.inner.id, self.inner.tag)
    }
}

fn io_err(e: std::io::Error, what: &str) -> PyErr {
    let msg = format!("{what}: {e}");
    match e.kind() {
        std::io::ErrorKind::ConnectionRefused => PyConnectionRefusedError::new_err(msg),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => PyTimeoutError::new_err(msg),
        _ => PyOSError::new_err(msg),
    }
}

/// host:port につなぐ。inbox を渡さなければ、そのつながりだけの inbox を作る
#[pyfunction]
#[pyo3(signature = (host, port, timeout=5.0, inbox=None))]
pub fn net_connect(py: Python<'_>, host: &str, port: u16, timeout: f64, inbox: Option<PyRef<'_, Inbox>>) -> PyResult<Conn> {
    let ib = inbox.map(|i| i.inner.clone()).unwrap_or_else(|| Arc::new(InboxInner { q: Mutex::new(VecDeque::new()), cv: Condvar::new() }));
    let h = host.to_string();
    let stream = py.allow_threads(move || -> std::io::Result<TcpStream> {
        let mut last = None;
        for a in (h.as_str(), port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&a, Duration::from_secs_f64(timeout.max(0.01))) {
                Ok(s) => return Ok(s),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "アドレスが見つかりません")))
    }).map_err(|e| io_err(e, &format!("{host}:{port}")))?;
    let c = ConnInner::new(stream, format!("{host}:{port}"), ib);
    c.start();
    Ok(Conn { inner: c })
}

/// ホスト：つながりを受け付ける。新しいつながりは inbox に (conn, {"t": "_open", "addr": ...}) で入る
#[pyclass(module = "alpedia_core")]
pub struct Listener {
    alive: Arc<AtomicBool>,
    #[pyo3(get)]
    port: u16,
}

#[pymethods]
impl Listener {
    #[new]
    fn new(port: u16, inbox: PyRef<'_, Inbox>) -> PyResult<Self> {
        let sock = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None).map_err(|e| io_err(e, "受け付け"))?;
        #[cfg(not(windows))]
        let _ = sock.set_reuse_address(true);
        let addr: SocketAddr = ([0, 0, 0, 0], port).into();
        sock.bind(&addr.into()).map_err(|e| io_err(e, &format!("ポート {port}")))?;
        sock.listen(8).map_err(|e| io_err(e, "受け付け"))?;
        let l: TcpListener = sock.into();
        let port = l.local_addr().map(|a| a.port()).unwrap_or(port);
        l.set_nonblocking(true).map_err(|e| io_err(e, "受け付け"))?;
        let alive = Arc::new(AtomicBool::new(true));
        let a2 = alive.clone();
        let ib = inbox.inner.clone();
        std::thread::Builder::new()
            .name("net-accept".into())
            .spawn(move || {
                while a2.load(Ordering::SeqCst) {
                    match l.accept() {
                        Ok((s, addr)) => {
                            let _ = s.set_nonblocking(false);
                            let c = ConnInner::new(s, format!("{}:{}", addr.ip(), addr.port()), ib.clone());
                            // 受信は「つながった」の後から（先に届いた挨拶を取りこぼさないように）
                            ib.push(c.clone(), Ev::Open(addr.ip().to_string()));
                            c.start();
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(30)),
                        Err(_) => std::thread::sleep(Duration::from_millis(100)),
                    }
                }
            })
            .map_err(|e| PyOSError::new_err(e.to_string()))?;
        Ok(Listener { alive, port })
    }

    #[getter]
    fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn close(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// LAN の中のワールドを見つける
// ---------------------------------------------------------------------------
fn hostname() -> Option<String> {
    #[cfg(windows)]
    {
        std::env::var("COMPUTERNAME").ok()
    }
    #[cfg(not(windows))]
    {
        let mut buf = [0u8; 256];
        let r = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if r != 0 {
            return None;
        }
        let n = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Some(String::from_utf8_lossy(&buf[..n]).into_owned())
    }
}

/// この PC の LAN 側の IP アドレス（表示用）
#[pyfunction]
pub fn net_local_addresses() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(s) = UdpSocket::bind("0.0.0.0:0") {
        if s.connect("10.255.255.255:1").is_ok() {
            if let Ok(a) = s.local_addr() {
                out.push(a.ip().to_string());
            }
        }
    }
    if let Some(h) = hostname() {
        if let Ok(addrs) = (h.as_str(), 0).to_socket_addrs() {
            for a in addrs {
                if let SocketAddr::V4(v4) = a {
                    let ip = v4.ip().to_string();
                    if !out.contains(&ip) && !ip.starts_with("127.") {
                        out.push(ip);
                    }
                }
            }
        }
    }
    if out.is_empty() {
        out.push("127.0.0.1".into());
    }
    out
}

/// ホスト：1秒ごとに LAN へ「ここでワールドを公開中」と知らせる。info_fn() の dict に magic・proto を足して送る
#[pyclass(module = "alpedia_core")]
pub struct Announcer {
    alive: Arc<AtomicBool>,
}

#[pymethods]
impl Announcer {
    #[new]
    fn new(info_fn: PyObject, port: u16) -> PyResult<Self> {
        let alive = Arc::new(AtomicBool::new(true));
        let a2 = alive.clone();
        std::thread::Builder::new()
            .name("net-announce".into())
            .spawn(move || {
                let s = match UdpSocket::bind("0.0.0.0:0") {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let _ = s.set_broadcast(true);
                while a2.load(Ordering::SeqCst) {
                    let data: Option<Vec<u8>> = Python::with_gil(|py| -> Option<Vec<u8>> {
                        if !a2.load(Ordering::SeqCst) {
                            return None;
                        }
                        let r = (|| -> PyResult<Vec<u8>> {
                            let info = info_fn.bind(py).call0()?;
                            let d = PyDict::new_bound(py);
                            d.set_item("magic", MAGIC)?;
                            d.set_item("proto", PROTO)?;
                            d.update(info.downcast::<pyo3::types::PyMapping>()?)?;
                            let mut blobs = Vec::new();
                            let v = to_val(d.as_any(), &mut blobs, 0)?;
                            let mut js = String::new();
                            write_val(&mut js, &v);
                            Ok(js.into_bytes())
                        })();
                        match r {
                            Ok(b) => Some(b),
                            Err(e) => {
                                e.print(py);
                                None
                            }
                        }
                    });
                    if let Some(b) = data {
                        for t in ["255.255.255.255", "127.0.0.1"] {
                            let _ = s.send_to(&b, (t, port));
                        }
                    }
                    for _ in 0..10 {
                        if !a2.load(Ordering::SeqCst) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            })
            .map_err(|e| PyOSError::new_err(e.to_string()))?;
        Ok(Announcer { alive })
    }

    fn close(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }
}

struct Found {
    t: f64,
    info: String,
}

fn now_s() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// 参加する側：LAN の知らせを聞いて、公開中のワールドの一覧を作る
#[pyclass(module = "alpedia_core")]
pub struct Browser {
    found: Arc<Mutex<HashMap<(String, i64), Found>>>,
    alive: Arc<AtomicBool>,
    #[pyo3(get)]
    error: Option<String>,
}

#[pymethods]
impl Browser {
    #[new]
    fn new(port: u16) -> Self {
        let found: Arc<Mutex<HashMap<(String, i64), Found>>> = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let sock = (|| -> std::io::Result<UdpSocket> {
            let s = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None)?;
            s.set_reuse_address(true)?;
            #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
            let _ = s.set_reuse_port(true);
            let addr: SocketAddr = ([0, 0, 0, 0], port).into();
            s.bind(&addr.into())?;
            let u: UdpSocket = s.into();
            u.set_read_timeout(Some(Duration::from_millis(500)))?;
            Ok(u)
        })();
        let sock = match sock {
            Ok(s) => s,
            Err(e) => {
                alive.store(false, Ordering::SeqCst);
                return Browser { found, alive, error: Some(e.to_string()) };
            }
        };
        let (f2, a2) = (found.clone(), alive.clone());
        let _ = std::thread::Builder::new().name("net-browse".into()).spawn(move || {
            let mut buf = vec![0u8; 65536];
            while a2.load(Ordering::SeqCst) {
                let (n, addr) = match sock.recv_from(&mut buf) {
                    Ok(x) => x,
                    Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
                    Err(_) => break,
                };
                let txt = match std::str::from_utf8(&buf[..n]) {
                    Ok(t) => t.to_string(),
                    Err(_) => continue,
                };
                let v = match parse_json(txt.as_bytes()) {
                    Ok(Val::Dict(d)) => d,
                    _ => continue,
                };
                let get = |k: &str| v.iter().find(|(a, _)| a == k).map(|x| &x.1);
                if !matches!(get("magic"), Some(Val::Str(m)) if m == MAGIC) {
                    continue;
                }
                let p = match get("port") {
                    Some(Val::Int(p)) => *p,
                    Some(Val::Float(p)) => *p as i64,
                    _ => 47620,
                };
                let ip = addr.ip().to_string();
                let mut f = f2.lock().unwrap();
                if ip.starts_with("127.") && f.keys().any(|k| k.0 != ip && k.1 == p) {
                    continue;
                }
                f.insert((ip, p), Found { t: now_s(), info: txt });
            }
        });
        Browser { found, alive, error: None }
    }

    #[getter]
    fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// [(ip, port, 情報の dict)]（max_age 秒以内に知らせが届いたもの、ip・port の順）
    #[pyo3(signature = (max_age=3.5))]
    fn worlds(&self, py: Python<'_>, max_age: f64) -> PyResult<Vec<(String, i64, PyObject)>> {
        let now = now_s();
        let f = self.found.lock().unwrap();
        let mut keys: Vec<&(String, i64)> = f.keys().collect();
        keys.sort();
        let mut out = Vec::new();
        for k in keys {
            let x = &f[k];
            if now - x.t < max_age {
                out.push((k.0.clone(), k.1, json_to_py(py, &x.info)?));
            }
        }
        Ok(out)
    }

    fn close(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }
}
