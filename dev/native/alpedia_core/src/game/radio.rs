//! 無線通信：アンテナのある構築物どうしで、コンピューターがチャンネルを決めて情報をやり取りする。main/radio.py の中身。
//!
//! - 送る側・受ける側のどちらにも、電力が来ていて ON のアンテナが必要
//! - 届く距離は、送る側と受ける側のアンテナの届く距離（params.range）の短いほう。アンテナが複数あるときはいちばん遠くまで届くもの
//! - 送った情報は少しの間（KEEP 秒）だけ空中に残り、その間に受信した各コンピューターが1回ずつ受け取れる
//! - 送れる情報は数値・文字・True/False/None と、それらのリスト・辞書（JSON にできるもの）。大きさの上限あり
//! - 同じ構築物から送った情報は、その構築物では受け取らない（自分の声は聞こえない）

use std::collections::{HashMap, VecDeque};

use numpy::{PyArray1, PyReadonlyArrayDyn};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

use crate::net::{val_to_py, write_val, Val};

pub const KEEP: f64 = 2.0; // 送った情報が空中に残る時間（秒）
pub const MAX_PER_CHANNEL: usize = 64; // チャンネルごとに残しておく数
pub const MAX_BYTES: usize = 4096; // 1回に送れる大きさ（JSON にしたときの文字数）
pub const MAX_CHANNEL: usize = 40;

#[allow(unexpected_cfgs)]
mod err {
    pyo3::create_exception!(alpedia_core, RadioError, pyo3::exceptions::PyRuntimeError);
}
pub use err::RadioError;

/// 構築物の、使えるアンテナの (届く距離, 位置) か None
pub fn antenna(c: &Bound<'_, PyAny>) -> PyResult<Option<(f64, [f64; 3])>> {
    let devs = c.getattr("devs")?;
    let reg = c.getattr("reg")?;
    let mut best: Option<(f64, usize)> = None;
    for (i, d) in devs.iter()?.enumerate() {
        let d = d?;
        if !d.is_truthy()? {
            continue;
        }
        let dd = reg.call_method1("device", (d.get_item("def")?,))?;
        if !dd.is_truthy()? {
            continue;
        }
        let t = dd.call_method1("get", ("type",))?;
        if !t.eq("antenna")? {
            continue;
        }
        let st = d.get_item("state")?;
        if !st.call_method1("get", ("enabled", true))?.is_truthy()? || !st.call_method1("get", ("active", false))?.is_truthy()? {
            continue;
        }
        let params = dd.call_method1("get", ("params", PyDict::new_bound(c.py())))?;
        let rng: f64 = c.py().import_bound("builtins")?.getattr("float")?.call1((params.call_method1("get", ("range", 3000.0))?,))?.extract()?;
        if best.map_or(true, |b| rng > b.0) {
            best = Some((rng, i));
        }
    }
    let Some((rng, i)) = best else { return Ok(None) };
    let pos = c.getattr("pos")?;
    let a: PyReadonlyArrayDyn<f64> = pos.call_method1("astype", ("float64",))?.extract()?;
    let s = a.as_slice()?;
    if s.len() < 3 * (8 * i + 8) {
        return Ok(None);
    }
    let mut p = [0.0; 3];
    for k in 0..8 {
        for ax in 0..3 {
            p[ax] += s[3 * (8 * i + k) + ax];
        }
    }
    Ok(Some((rng, [p[0] / 8.0, p[1] / 8.0, p[2] / 8.0])))
}

fn channel_name(ch: &Bound<'_, PyAny>) -> PyResult<String> {
    let s = ch.str()?.to_string();
    let n = s.chars().count();
    if n == 0 || n > MAX_CHANNEL {
        return Err(RadioError::new_err(format!("チャンネル名は 1〜{MAX_CHANNEL} 文字です")));
    }
    Ok(s)
}

fn py_float_repr(f: f64) -> String {
    // Python の repr(float) と同じ長さになるように（文字数の上限を同じにするため）
    let s = format!("{f:?}");
    if let Some(p) = s.find('e') {
        let (m, e) = s.split_at(p);
        let e = &e[1..];
        let (sign, digits) = if let Some(d) = e.strip_prefix('-') { ("-", d) } else { ("+", e) };
        let m = m.strip_suffix(".0").unwrap_or(m);
        return format!("{m}e{sign}{digits:0>2}");
    }
    s
}

/// 送れる値だけを、JSON の値にする（Python の json.dumps と同じ決まり。NaN・無限大は不可）
fn strict(x: &Bound<'_, PyAny>, depth: usize) -> PyResult<Val> {
    if depth > 200 {
        return Err(pyo3::exceptions::PyValueError::new_err("Circular reference detected"));
    }
    if x.is_none() {
        return Ok(Val::Null);
    }
    if x.is_instance_of::<PyBool>() {
        return Ok(Val::Bool(x.is_truthy()?));
    }
    if x.is_instance_of::<PyInt>() {
        return Ok(match x.extract::<i64>() {
            Ok(v) => Val::Int(v),
            Err(_) => Val::Big(x.call_method0("__index__")?.str()?.to_string()),
        });
    }
    if x.is_instance_of::<PyFloat>() {
        let f: f64 = x.extract()?;
        if !f.is_finite() {
            let r = if f.is_nan() { "nan" } else if f > 0.0 { "inf" } else { "-inf" };
            return Err(pyo3::exceptions::PyValueError::new_err(format!("Out of range float values are not JSON compliant: {r}")));
        }
        return Ok(Val::Float(f));
    }
    if let Ok(s) = x.downcast::<PyString>() {
        return Ok(Val::Str(s.to_cow()?.into_owned()));
    }
    if let Ok(d) = x.downcast::<PyDict>() {
        let mut out = Vec::with_capacity(d.len());
        for (k, v) in d.iter() {
            let key = if k.is_instance_of::<PyString>() {
                k.extract::<String>()?
            } else if k.is_none() {
                "null".into()
            } else if k.is_instance_of::<PyBool>() {
                if k.is_truthy()? { "true".into() } else { "false".into() }
            } else if k.is_instance_of::<PyInt>() {
                k.call_method0("__index__")?.str()?.to_string()
            } else if k.is_instance_of::<PyFloat>() {
                let f: f64 = k.extract()?;
                if !f.is_finite() {
                    return Err(pyo3::exceptions::PyValueError::new_err("Out of range float values are not JSON compliant"));
                }
                py_float_repr(f)
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "keys must be str, int, float, bool or None, not {}", k.get_type().name()?)));
            };
            out.push((key, strict(&v, depth + 1)?));
        }
        return Ok(Val::Dict(out));
    }
    if x.is_instance_of::<PyList>() || x.is_instance_of::<PyTuple>() {
        let mut out = Vec::new();
        for v in x.iter()? {
            out.push(strict(&v?, depth + 1)?);
        }
        return Ok(Val::List(out));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(format!("Object of type {} is not JSON serializable", x.get_type().name()?)))
}

/// json.dumps(ensure_ascii=False) の文字数（区切りは ", " と ": "）
fn dumped_len(v: &Val) -> usize {
    match v {
        Val::Null => 4,
        Val::Bool(b) => if *b { 4 } else { 5 },
        Val::Int(i) => i.to_string().len(),
        Val::Big(s) => s.len(),
        Val::Float(f) => py_float_repr(*f).len(),
        Val::Str(s) => {
            let mut o = String::new();
            write_val(&mut o, &Val::Str(s.clone()));
            o.chars().count()
        }
        Val::List(l) => 2 + l.iter().map(dumped_len).sum::<usize>() + 2 * l.len().saturating_sub(1),
        Val::Dict(d) => 2 + d.iter().map(|(k, x)| dumped_len(&Val::Str(k.clone())) + 2 + dumped_len(x)).sum::<usize>()
            + 2 * d.len().saturating_sub(1),
    }
}

fn clean(data: &Bound<'_, PyAny>) -> PyResult<Val> {
    let v = strict(data, 0).map_err(|e| {
        RadioError::new_err(format!("送れるのは数値・文字・True/False/None と、それらのリスト・辞書です（{}）",
                                    e.value_bound(data.py())))
    })?;
    let n = dumped_len(&v);
    if n > MAX_BYTES {
        return Err(RadioError::new_err(format!("一度に送れるのは {MAX_BYTES} 文字までです（{n} 文字）")));
    }
    Ok(v)
}

struct Message {
    seq: i64,
    time: f64,
    cid: PyObject,
    cname: PyObject,
    pos: [f64; 3],
    range: f64,
    data: Val,
}

#[pyclass(module = "alpedia_core")]
pub struct RadioBus {
    #[pyo3(get)]
    world: PyObject,
    channels: HashMap<String, VecDeque<Message>>,
    #[pyo3(get)]
    seq: i64,
}

impl RadioBus {
    fn now(&self, py: Python<'_>) -> PyResult<f64> {
        self.world.bind(py).getattr("time")?.extract()
    }

    fn expire(q: &mut VecDeque<Message>, t: f64) {
        // 古いもの（ワールドを読み直して時刻が戻った場合も）
        while let Some(m) = q.front() {
            if t - m.time > KEEP || m.time > t {
                q.pop_front();
            } else {
                break;
            }
        }
    }

    fn visible(&mut self, py: Python<'_>, c: &Bound<'_, PyAny>, ch: &Bound<'_, PyAny>) -> PyResult<Vec<(usize, f64)>> {
        let Some((rng, pos)) = antenna(c)? else {
            return Err(RadioError::new_err("アンテナがない（または電力が来ていない・OFF）ので受信できません"));
        };
        let name = channel_name(ch)?;
        let t = self.now(py)?;
        let cid = c.getattr("id")?;
        let Some(q) = self.channels.get_mut(&name) else { return Ok(Vec::new()) };
        if q.is_empty() {
            return Ok(Vec::new());
        }
        Self::expire(q, t);
        let mut out = Vec::new();
        for (k, m) in q.iter().enumerate() {
            if m.cid.bind(py).eq(&cid)? {
                continue;
            }
            let d = ((m.pos[0] - pos[0]).powi(2) + (m.pos[1] - pos[1]).powi(2) + (m.pos[2] - pos[2]).powi(2)).sqrt();
            if d <= rng.min(m.range) {
                out.push((k, d));
            }
        }
        Ok(out)
    }
}

#[pymethods]
impl RadioBus {
    #[new]
    fn new(world: PyObject) -> Self {
        RadioBus { world, channels: HashMap::new(), seq: 0 }
    }

    fn send(&mut self, py: Python<'_>, c: &Bound<'_, PyAny>, channel: &Bound<'_, PyAny>, data: &Bound<'_, PyAny>) -> PyResult<i64> {
        let Some((rng, pos)) = antenna(c)? else {
            return Err(RadioError::new_err("アンテナがない（または電力が来ていない・OFF）ので送信できません"));
        };
        let ch = channel_name(channel)?;
        let t = self.now(py)?;
        self.seq += 1;
        let q = self.channels.entry(ch).or_default();
        Self::expire(q, t);
        let data = clean(data)?;
        q.push_back(Message { seq: self.seq, time: t, cid: c.getattr("id")?.unbind(), cname: c.getattr("name")?.unbind(),
                              pos, range: rng, data });
        while q.len() > MAX_PER_CHANNEL {
            q.pop_front();
        }
        Ok(self.seq)
    }

    /// 前回の受信より後に届いた情報のリスト（古い順）
    fn receive<'py>(&mut self, py: Python<'py>, c: &Bound<'py, PyAny>, comp_state: &Bound<'py, PyDict>, channel: &Bound<'py, PyAny>)
        -> PyResult<Bound<'py, PyList>> {
        let ch = channel_name(channel)?;
        let seen = comp_state.call_method1("setdefault", ("_radio_seen", PyDict::new_bound(py)))?;
        let last: i64 = seen.call_method1("get", (&ch, 0))?.extract()?;
        let vis = self.visible(py, c, channel)?;
        let t = self.now(py)?;
        let out = PyList::empty_bound(py);
        let Some(q) = self.channels.get(&ch) else { return Ok(out) };
        for (k, dist) in vis {
            let m = &q[k];
            if m.seq > last {
                let d = PyDict::new_bound(py);
                d.set_item("data", val_to_py(py, &m.data, &[])?)?;
                d.set_item("from", m.cname.bind(py))?;
                d.set_item("distance", dist)?;
                d.set_item("time", m.time)?;
                d.set_item("age", t - m.time)?;
                out.append(d)?;
                let prev: i64 = seen.call_method1("get", (&ch, 0))?.extract()?;
                seen.set_item(&ch, prev.max(m.seq))?;
            }
        }
        Ok(out)
    }

    /// 届いている中でいちばん新しい情報（なければ default）。受け取っても消えない
    #[pyo3(signature = (c, channel, default=None, max_age=1.0))]
    fn latest(&mut self, py: Python<'_>, c: &Bound<'_, PyAny>, channel: &Bound<'_, PyAny>, default: Option<PyObject>, max_age: f64)
        -> PyResult<PyObject> {
        let vis = self.visible(py, c, channel)?;
        let t = self.now(py)?;
        let ch = channel_name(channel)?;
        let mut best: Option<&Message> = None;
        if let Some(q) = self.channels.get(&ch) {
            for (k, _) in vis {
                let m = &q[k];
                if t - m.time <= max_age && best.map_or(true, |b| m.seq > b.seq) {
                    best = Some(m);
                }
            }
        }
        match best {
            Some(m) => val_to_py(py, &m.data, &[]),
            None => Ok(default.unwrap_or_else(|| py.None())),
        }
    }
}

/// 構築物の、使えるアンテナの (届く距離, 位置) か None
#[pyfunction]
pub fn radio_antenna_info(py: Python<'_>, c: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    Ok(match antenna(c)? {
        None => py.None(),
        Some((r, p)) => (r, PyArray1::from_vec_bound(py, p.to_vec())).into_py(py),
    })
}
