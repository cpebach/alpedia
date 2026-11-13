//! コンピューター機器から実行するゲーム内 Python スクリプトの実行の仕組み。main/scripting.py の中身。
//!
//! スクリプトは saves/<ワールド>/scripts/*.py に置き、`tick(api)` 関数を定義する。
//! コンピューターが起動している間、一定間隔で tick(api) が呼ばれる。
//! import できるのは math / random / statistics のみ（簡易サンドボックス）。
//! スクリプトそのものは Python のまま（Python で書いて Python で動く）。ここはそれに渡す api と、呼び出す仕組み。

use numpy::PyReadonlyArray2;
use pyo3::exceptions::{PyAttributeError, PyImportError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};

use crate::game::control::Port;

type R<T> = PyResult<T>;

const SAFE_BUILTINS: [&str; 34] = [
    "abs", "all", "any", "bool", "dict", "enumerate", "filter", "float", "int", "len", "list", "map", "max", "min", "pow", "range",
    "reversed", "round", "set", "sorted", "str", "sum", "tuple", "zip", "isinstance", "Exception", "ValueError", "KeyError",
    "IndexError", "ZeroDivisionError", "True", "False", "None", "getattr",
];
const SAFE_BUILTINS2: [&str; 4] = ["hasattr", "TypeError", "AttributeError", "RuntimeError"];
const ALLOWED: [&str; 3] = ["math", "random", "statistics"];
const FACE_NAMES: [&str; 6] = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"];

fn float_of(v: &Bound<'_, PyAny>) -> R<f64> {
    crate::game::imp(v.py(), "builtins")?.getattr("float")?.call1((v,))?.extract()
}

/// scripts/ 内のファイルパス（フォルダ外を指す名前は拒否）
#[pyfunction]
pub fn script_path<'py>(world_dir: &Bound<'py, PyAny>, name: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let s = if name.is_none() { String::new() } else { name.str()?.to_string() };
    let ok = !s.is_empty()
        && s.ends_with(".py")
        && s.len() > 3
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && !s.starts_with('.');
    if !ok {
        return Err(PyValueError::new_err(format!("ファイル名が不正です: '{s}'（半角英数字・- _ . で、.py で終わる名前）")));
    }
    world_dir.call_method1("__truediv__", ("scripts",))?.call_method1("__truediv__", (s,))
}

/// スクリプトの import（math / random / statistics だけ）
#[pyfunction]
#[pyo3(signature = (name, *_a, **_k))]
pub fn safe_import<'py>(py: Python<'py>, name: &str, _a: &Bound<'py, PyTuple>, _k: Option<&Bound<'py, PyDict>>) -> R<Bound<'py, PyAny>> {
    if ALLOWED.contains(&name) {
        return Ok(crate::game::imp(py, name)?.into_any());
    }
    Err(PyImportError::new_err(format!("ゲーム内スクリプトでは '{name}' は使えません（math, random, statistics のみ）")))
}

fn color(v: &Bound<'_, PyAny>, default: &[f64], n: usize) -> R<Vec<f64>> {
    if v.is_none() {
        return Ok(default.to_vec());
    }
    let mut x: Vec<f64> = v.iter()?.map(|e| float_of(&e?)).collect::<R<Vec<_>>>()?;
    x.truncate(4);
    if x.iter().any(|&a| a > 1.0) {
        x = x.iter().map(|a| a / 255.0).collect();
    }
    while x.len() < n {
        x.push(1.0);
    }
    Ok(x.into_iter().map(|a| a.max(0.0).min(1.0)).collect())
}

/// 表面に出す文字の辞書（text が空なら None＝消す）。direction は機器の向きの番号（face 省略時に使う）
#[pyfunction]
#[pyo3(signature = (direction, text, face=None, width=None, color=None, background=None))]
pub fn display_payload<'py>(py: Python<'py>, direction: PyObject, text: &Bound<'py, PyAny>, face: Option<&Bound<'py, PyAny>>,
                            width: Option<&Bound<'py, PyAny>>, color: Option<&Bound<'py, PyAny>>, background: Option<&Bound<'py, PyAny>>)
    -> R<Option<Bound<'py, PyDict>>> {
    if text.is_none() || text.eq("")? {
        return Ok(None);
    }
    let f: PyObject = match face.filter(|f| !f.is_none()) {
        None => direction,
        Some(f) => {
            if let Ok(s) = f.downcast::<PyString>() {
                let s = s.to_string();
                if s.to_lowercase() == "all" {
                    "all".into_py(py)
                } else {
                    let u = s.to_uppercase();
                    match FACE_NAMES.iter().position(|n| *n == u) {
                        Some(k) => k.into_py(py),
                        None => return Err(PyValueError::new_err("face は ['+X', '-X', '+Y', '-Y', '+Z', '-Z'] か 'all' です")),
                    }
                }
            } else {
                let k: i64 = crate::game::imp(py, "builtins")?.getattr("int")?.call1((f,))?.extract()?;
                k.rem_euclid(6).into_py(py)
            }
        }
    };
    let w: PyObject = match width.filter(|w| !w.is_none()) {
        None => py.None(),
        Some(w) => {
            let m = crate::game::imp(py, "builtins")?;
            let x = m.getattr("max")?.call1((w, 0.05))?;
            let x = m.getattr("min")?.call1((x, 50.0))?;
            float_of(&x)?.into_py(py)
        }
    };
    let txt: String = text.str()?.to_string().chars().take(2000).collect();
    let d = PyDict::new_bound(py);
    d.set_item("text", txt)?;
    d.set_item("face", f)?;
    d.set_item("width", w)?;
    let none = py.None().into_bound(py);
    d.set_item("color", self::color(color.unwrap_or(&none), &[1.0, 1.0, 1.0], 3)?)?;
    d.set_item("background", self::color(background.unwrap_or(&none), &[0.04, 0.05, 0.08, 0.92], 4)?)?;
    Ok(Some(d))
}

fn ext<'py>(py: Python<'py>) -> R<Bound<'py, PyModule>> {
    crate::game::imp(py, "main.extensions")
}

/// スクリプトから見た機器。中身は Port を通して読み書きする
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct DeviceHandle {
    #[pyo3(get, name = "_p")]
    p: Py<Port>,
    #[pyo3(get, name = "_i")]
    i: i64,
    #[pyo3(get, name = "_kind")]
    kind_: PyObject,
    #[pyo3(get, name = "_type")]
    type_: PyObject,
    #[pyo3(get, name = "_name")]
    name_: PyObject,
    #[pyo3(get, name = "_dir")]
    dir_: PyObject,
}

impl DeviceHandle {
    fn port<'py>(&self, py: Python<'py>) -> Bound<'py, Port> {
        self.p.bind(py).clone()
    }
    fn get(&self, py: Python<'_>, key: &str, default: PyObject) -> R<PyObject> {
        self.port(py).call_method1("device_get", (self.i, key, default)).map(|x| x.unbind())
    }
    fn set(&self, py: Python<'_>, key: &str, v: &Bound<'_, PyAny>) -> R<()> {
        self.port(py).call_method1("device_set", (self.i, key, v))?;
        Ok(())
    }
}

#[pymethods]
impl DeviceHandle {
    #[new]
    fn new(port: Py<Port>, info: &Bound<'_, PyAny>) -> R<Self> {
        Ok(DeviceHandle {
            p: port,
            i: info.get_item(0)?.extract()?,
            kind_: info.get_item(1)?.unbind(),
            type_: info.get_item(2)?.unbind(),
            name_: info.get_item(3)?.unbind(),
            dir_: info.get_item(4)?.unbind(),
        })
    }

    #[getter]
    fn name(&self, py: Python<'_>) -> PyObject {
        self.name_.clone_ref(py)
    }
    #[getter]
    fn r#type(&self, py: Python<'_>) -> PyObject {
        self.type_.clone_ref(py)
    }
    #[getter]
    fn kind(&self, py: Python<'_>) -> PyObject {
        self.kind_.clone_ref(py)
    }
    #[getter]
    fn direction(&self, py: Python<'_>) -> PyObject {
        self.dir_.clone_ref(py)
    }
    #[getter]
    fn state<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("device_state", (self.i,))
    }
    // MOD の機器（extensions）の口
    #[getter(port)]
    fn port_(&self, py: Python<'_>) -> Py<Port> {
        self.p.clone_ref(py)
    }
    #[getter]
    fn index(&self) -> i64 {
        self.i
    }
    /// MOD の機器の state["data"]（MOD が自由に使う辞書）
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("custom_data", (self.i,))
    }

    fn __getattr__(slf: &Bound<'_, Self>, name: &str) -> R<PyObject> {
        let py = slf.py();
        if name.starts_with('_') {
            return Err(PyAttributeError::new_err(name.to_string()));
        }
        let me = slf.borrow();
        let beh = me.type_.bind(py);
        let e = ext(py)?;
        let m = e.getattr("DEVICE_METHODS")?.call_method1("get", (beh, PyDict::new_bound(py)))?.call_method1("get", (name,))?;
        if !m.is_none() {
            let functools = crate::game::imp(py, "functools")?;
            return Ok(functools.getattr("partial")?.call1((m, slf))?.unbind());
        }
        let pr = e.getattr("DEVICE_PROPS")?.call_method1("get", (beh, PyDict::new_bound(py)))?.call_method1("get", (name,))?;
        if !pr.is_none() {
            return Ok(pr.call1((slf,))?.unbind());
        }
        Err(PyAttributeError::new_err(format!("{}（{}）に '{name}' はありません", me.name_.bind(py).str()?, beh.str()?)))
    }

    /// このブロックの表面に文字を表示（コンピューターのみ）
    #[pyo3(signature = (text, face=None, width=None, color=None, background=None))]
    fn display(&self, py: Python<'_>, text: &Bound<'_, PyAny>, face: Option<&Bound<'_, PyAny>>, width: Option<&Bound<'_, PyAny>>,
               color: Option<&Bound<'_, PyAny>>, background: Option<&Bound<'_, PyAny>>) -> R<()> {
        if !self.type_.bind(py).eq("computer")? {
            return Err(PyValueError::new_err("display() はコンピューターでのみ使えます"));
        }
        let dir = self.port(py).call_method1("device_dir", (self.i,))?.unbind();
        let pl = display_payload(py, dir, text, face, width, color, background)?;
        self.port(py).call_method1("set_display", (self.i, pl))?;
        Ok(())
    }

    fn clear_display(&self, py: Python<'_>) -> R<()> {
        self.port(py).call_method1("set_display", (self.i, py.None()))?;
        Ok(())
    }

    #[getter]
    fn cell<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("device_cell", (self.i,))
    }

    /// 構築物の重心から見たローカル座標での位置 [x,y,z] (m)
    #[getter]
    fn offset<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("device_offset", (self.i,))
    }

    // 推進機 -----------------------------------------------------------
    /// 点火する（電気を使う。燃えていれば何もしない）
    fn ignite(&self, py: Python<'_>) -> R<()> {
        self.set(py, "ignite", &true.into_py(py).into_bound(py))
    }
    /// 燃えているか
    #[getter]
    fn lit(&self, py: Python<'_>) -> R<bool> {
        self.get(py, "lit", false.into_py(py))?.bind(py).is_truthy()
    }
    /// いまの出力 0〜1（バルブから来る燃料の量で決まる）
    #[getter]
    fn get_throttle(&self, py: Python<'_>) -> R<PyObject> {
        let v = self.get(py, "firing", 0.0.into_py(py))?;
        Ok(if v.bind(py).is_truthy()? { v } else { 0.0.into_py(py) })
    }
    /// 前の版との互換：0 より大きい値で点火する
    #[setter]
    fn set_throttle(&self, py: Python<'_>, v: &Bound<'_, PyAny>) -> R<()> {
        self.set(py, "throttle", v)
    }

    // バルブ -----------------------------------------------------------
    /// 開き具合 0〜1（0 で閉）
    #[getter]
    fn get_open(&self, py: Python<'_>) -> R<PyObject> {
        self.get(py, "open", 1.0.into_py(py))
    }
    #[setter]
    fn set_open(&self, py: Python<'_>, v: &Bound<'_, PyAny>) -> R<()> {
        self.set(py, "open", v)
    }
    #[getter]
    fn get_flow(&self, py: Python<'_>) -> R<PyObject> {
        self.get(py, "open", 1.0.into_py(py))
    }
    #[setter]
    fn set_flow(&self, py: Python<'_>, v: &Bound<'_, PyAny>) -> R<()> {
        self.set(py, "open", v)
    }

    // アクチュエーター -------------------------------------------------
    /// 出力側を angle 度だけ回す（相対）。speed: 度/秒
    #[pyo3(signature = (angle, speed=None))]
    fn rotate(&self, py: Python<'_>, angle: PyObject, speed: Option<PyObject>) -> R<()> {
        self.port(py).call_method1("act_rotate", (self.i, angle, speed))?;
        Ok(())
    }
    /// 角度 angle 度まで回す（絶対）
    #[pyo3(signature = (angle, speed=None))]
    fn rotate_to(&self, py: Python<'_>, angle: PyObject, speed: Option<PyObject>) -> R<()> {
        self.port(py).call_method1("act_rotate", (self.i, angle, speed, true))?;
        Ok(())
    }
    /// amount ブロック分押し出す（相対）。speed: ブロック/秒
    #[pyo3(signature = (amount=None, speed=None))]
    fn extend(&self, py: Python<'_>, amount: Option<PyObject>, speed: Option<PyObject>) -> R<()> {
        self.port(py).call_method1("act_extend", (self.i, amount.unwrap_or_else(|| 1.0.into_py(py)), speed))?;
        Ok(())
    }
    /// amount ブロック分引き込む
    #[pyo3(signature = (amount=None, speed=None))]
    fn retract(&self, py: Python<'_>, amount: Option<PyObject>, speed: Option<PyObject>) -> R<()> {
        self.port(py).call_method1("act_retract", (self.i, amount.unwrap_or_else(|| 1.0.into_py(py)), speed))?;
        Ok(())
    }
    /// 伸び量を position ブロック（0〜1）にする（絶対）
    #[pyo3(signature = (position, speed=None))]
    fn extend_to(&self, py: Python<'_>, position: PyObject, speed: Option<PyObject>) -> R<()> {
        self.port(py).call_method1("act_extend", (self.i, position, speed, true))?;
        Ok(())
    }
    /// このアクチュエーターに action ができるか
    fn can(&self, py: Python<'_>, action: &str) -> R<bool> {
        self.port(py).call_method1("act_can", (self.i, action))?.extract()
    }
    /// アクチュエーターの種類："rotor" / "piston" / "hinge"
    #[getter]
    fn motion<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("act_motion", (self.i,))
    }
    /// 出力面の結合を切り離す。切り離したら True
    fn detach<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("act_detach", (self.i,))
    }
    /// 出力面の先のブロックと結合する。結果のメッセージを返す
    fn attach<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("act_attach", (self.i,))
    }
    /// P メニューで設定した動作を実行する
    fn execute<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("act_execute", (self.i,))
    }
    #[getter]
    fn angle(&self, py: Python<'_>) -> R<PyObject> {
        self.get(py, "angle", 0.0.into_py(py))
    }
    #[getter]
    fn get_extension(&self, py: Python<'_>) -> R<PyObject> {
        self.get(py, "ext", 0.0.into_py(py))
    }
    #[setter]
    fn set_extension(&self, py: Python<'_>, v: PyObject) -> R<()> {
        self.extend_to(py, v, None)
    }
    #[getter]
    fn attached<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("act_attached", (self.i,))
    }
    /// 出力側ブロックの向き（度）
    #[getter]
    fn output_angle<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("act_output_angle", (self.i,))
    }
    #[getter]
    fn busy<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("act_busy", (self.i,))
    }

    // 汎用 ON/OFF
    #[getter]
    fn get_enabled(&self, py: Python<'_>) -> R<PyObject> {
        self.get(py, "enabled", true.into_py(py))
    }
    #[setter]
    fn set_enabled(&self, py: Python<'_>, v: &Bound<'_, PyAny>) -> R<()> {
        self.set(py, "enabled", v)
    }

    // 読み取り
    #[getter]
    fn contents<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyDict>> {
        let v = self.get(py, "contents", PyDict::new_bound(py).into_any().unbind())?;
        let d = PyDict::new_bound(py);
        let b = v.bind(py);
        if b.is_truthy()? {
            d.update(b.downcast::<pyo3::types::PyMapping>()?)?;
        }
        Ok(d)
    }
    #[getter]
    fn charge(&self, py: Python<'_>) -> R<PyObject> {
        self.get(py, "charge", 0.0.into_py(py))
    }

    fn __repr__(&self, py: Python<'_>) -> R<String> {
        Ok(format!("<{} '{}'>", self.type_.bind(py).str()?, self.name_.bind(py).str()?))
    }
}

fn vlist(v: &Bound<'_, PyAny>) -> R<Vec<f64>> {
    v.iter()?.map(|x| float_of(&x?)).collect()
}

fn rt_mul(r: &Bound<'_, PyAny>, v: &Bound<'_, PyAny>) -> R<Vec<f64>> {
    let m: PyReadonlyArray2<f64> = r.extract()?;
    let a = m.as_array();
    let x = vlist(v)?;
    Ok((0..3).map(|j| (0..3).map(|k| a[[k, j]] * x[k]).sum()).collect())
}

/// スクリプトの tick(api) に渡す api。構築物の中身は Port を通してだけ読み書きする
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct ScriptAPI {
    #[pyo3(get, name = "_p")]
    p: Py<Port>,
    #[pyo3(get, name = "_ci")]
    ci: i64,
    #[pyo3(get, set)]
    memory: PyObject,
    #[pyo3(get, set)]
    logs: Py<PyList>,
}

impl ScriptAPI {
    fn port<'py>(&self, py: Python<'py>) -> Bound<'py, Port> {
        self.p.bind(py).clone()
    }
    fn pose<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method0("pose")
    }
}

#[pymethods]
impl ScriptAPI {
    #[new]
    #[pyo3(signature = (world, c, comp_idx, port=None))]
    fn new(py: Python<'_>, world: PyObject, c: PyObject, comp_idx: i64, port: Option<Py<Port>>) -> R<Self> {
        let fresh = port.is_none();
        let p = match port {
            Some(p) => p,
            None => Py::new(py, Port::new_pub(py, world, c))?,
        };
        if fresh {
            p.bind(py).setattr("frame", comp_idx)?; // 姿勢・速さの向きは、コンピューターの付いている部分で測る
        }
        let memory = p.bind(py).call_method1("comp_state", (comp_idx,))?.call_method1("setdefault", ("memory", PyDict::new_bound(py)))?.unbind();
        Ok(ScriptAPI { p, ci: comp_idx, memory, logs: PyList::empty_bound(py).unbind() })
    }

    // 状態
    #[getter]
    fn time<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).getattr("time")
    }
    #[getter]
    fn position(&self, py: Python<'_>) -> R<Vec<f64>> {
        vlist(&self.pose(py)?.get_item(0)?)
    }
    #[getter]
    fn velocity(&self, py: Python<'_>) -> R<Vec<f64>> {
        vlist(&self.pose(py)?.get_item(2)?)
    }
    #[getter]
    fn speed(&self, py: Python<'_>) -> R<f64> {
        Ok(vlist(&self.pose(py)?.get_item(2)?)?.iter().map(|x| x * x).sum::<f64>().sqrt())
    }
    #[getter]
    fn mass<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method0("mass")
    }
    #[getter]
    fn up(&self, py: Python<'_>) -> R<Vec<f64>> {
        vlist(&self.port(py).call_method0("up")?)
    }
    /// 重心の場所の重力加速度 [x,y,z] m/s²（近くのすべての天体の分を足したもの）
    #[getter]
    fn gravity(&self, py: Python<'_>) -> R<Vec<f64>> {
        vlist(&self.port(py).call_method0("gravity")?)
    }
    /// 惑星の上方向を構築物のローカル座標で表したもの（直立なら [0,1,0]）
    #[getter]
    fn up_local(&self, py: Python<'_>) -> R<Vec<f64>> {
        let r = self.pose(py)?.get_item(1)?;
        rt_mul(&r, &self.port(py).call_method0("up")?)
    }
    #[getter]
    fn velocity_local(&self, py: Python<'_>) -> R<Vec<f64>> {
        let p = self.pose(py)?;
        rt_mul(&p.get_item(1)?, &p.get_item(2)?)
    }
    #[getter]
    fn angular_velocity_local(&self, py: Python<'_>) -> R<Vec<f64>> {
        let p = self.pose(py)?;
        rt_mul(&p.get_item(1)?, &p.get_item(3)?)
    }
    #[getter]
    fn altitude<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method0("altitude")
    }
    #[getter]
    fn vertical_speed(&self, py: Python<'_>) -> R<f64> {
        let v = vlist(&self.pose(py)?.get_item(2)?)?;
        let u = vlist(&self.port(py).call_method0("up")?)?;
        Ok(v.iter().zip(u.iter()).map(|(a, b)| a * b).sum())
    }
    #[getter]
    fn power<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method0("power")
    }
    /// 操縦席に座っている間に押されているキーの集合
    #[getter]
    fn input<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method0("input_keys")
    }
    /// この構築物の操縦席に誰かが座っているか
    #[getter]
    fn seated<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method0("seated")
    }
    #[getter]
    fn storage<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method0("storage")
    }

    /// 構築物の回転行列（行が ローカルX,Y,Z 軸のワールド方向）
    fn orientation<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.pose(py)?.get_item(1)?.getattr("T")?.call_method0("tolist")
    }

    #[pyo3(signature = (r#type=None, name=None))]
    fn devices<'py>(&self, py: Python<'py>, r#type: Option<&Bound<'py, PyAny>>, name: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyList>> {
        let out = PyList::empty_bound(py);
        let t = r#type.filter(|t| t.is_truthy().unwrap_or(false));
        let n = name.filter(|n| n.is_truthy().unwrap_or(false));
        for info in self.port(py).call_method0("devices")?.iter()? {
            let info = info?;
            if let Some(t) = t {
                if !info.get_item(2)?.eq(t)? && !info.get_item(1)?.eq(t)? {
                    continue;
                }
            }
            if let Some(n) = n {
                if !info.get_item(3)?.eq(n)? {
                    continue;
                }
            }
            out.append(Py::new(py, DeviceHandle::new(self.p.clone_ref(py), &info)?)?)?;
        }
        Ok(out)
    }

    fn device(&self, py: Python<'_>, name: &Bound<'_, PyAny>) -> R<PyObject> {
        let r = self.devices(py, None, Some(name))?;
        Ok(if r.len() > 0 { r.get_item(0)?.unbind() } else { py.None() })
    }

    // --- 表示（看板） ---------------------------------------------------
    /// このコンピューターの表面に文字を表示する
    #[pyo3(signature = (text, face=None, width=None, color=None, background=None))]
    fn display(&self, py: Python<'_>, text: &Bound<'_, PyAny>, face: Option<&Bound<'_, PyAny>>, width: Option<&Bound<'_, PyAny>>,
               color: Option<&Bound<'_, PyAny>>, background: Option<&Bound<'_, PyAny>>) -> R<()> {
        let dir = self.port(py).call_method1("device_dir", (self.ci,))?.unbind();
        let pl = display_payload(py, dir, text, face, width, color, background)?;
        self.port(py).call_method1("set_display", (self.ci, pl))?;
        Ok(())
    }

    fn clear_display(&self, py: Python<'_>) -> R<()> {
        self.port(py).call_method1("set_display", (self.ci, py.None()))?;
        Ok(())
    }

    /// 矢印 vec（ワールド座標の [x,y,z]）を、構築物の重心から伸びる3Dの矢印で見せる（約0.3秒で消える）
    #[pyo3(signature = (vec, color=None, scale=None))]
    fn show_arrow(&self, py: Python<'_>, vec: &Bound<'_, PyAny>, color: Option<&Bound<'_, PyAny>>, scale: Option<&Bound<'_, PyAny>>) -> R<()> {
        let err = || PyValueError::new_err("show_arrow(vec, color, scale): vec は [x,y,z]、color は (r,g,b)");
        let v: Vec<f64> = vlist(vec).map_err(|_| err())?;
        if v.len() != 3 {
            return Err(err());
        }
        let col: Vec<f64> = match color {
            Some(c) => {
                let t = crate::game::imp(py, "builtins")?.getattr("tuple")?.call1((c,)).map_err(|_| err())?;
                let l = vlist(&t).map_err(|_| err())?;
                if l.len() < 3 {
                    return Err(err());
                }
                l[..3].to_vec()
            }
            None => vec![1.0, 0.85, 0.15],
        };
        let k = match scale {
            Some(s) => float_of(s).map_err(|_| err())?,
            None => 1.0,
        };
        let st = self.port(py).call_method1("comp_state", (self.ci,))?;
        let arrows = st.call_method1("setdefault", ("_arrows", PyDict::new_bound(py)))?;
        let t = self.port(py).getattr("time")?;
        let item = PyList::new_bound(py, [(v[0] * k).into_py(py), (v[1] * k).into_py(py), (v[2] * k).into_py(py), t.unbind()]);
        arrows.set_item(format!("{:.3},{:.3},{:.3}", col[0], col[1], col[2]), item)?;
        if arrows.len()? > 8 {
            let first = arrows.iter()?.next().unwrap()?;
            arrows.call_method1("pop", (first,))?;
        }
        Ok(())
    }

    /// このコンピューターを停止する（表示は残る）
    fn stop(&self, py: Python<'_>) -> R<()> {
        self.port(py).call_method1("device_set", (self.ci, "running", false))?;
        Ok(())
    }

    // --- 惑星・軌道 -----------------------------------------------------
    /// 最寄りの惑星: {"name", "center", "radius", "mu", "surface_gravity"}
    fn planet<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method0("planet")
    }
    /// 現在の軌道
    fn orbit<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method0("orbit")
    }
    fn cavities<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method0("cavities")
    }

    // --- 無線通信（アンテナが必要） --------------------------------------
    /// この構築物に使えるアンテナ（電力が来ていて ON）があるか
    #[getter]
    fn antenna(&self, py: Python<'_>) -> R<bool> {
        Ok(!self.port(py).call_method0("antenna")?.is_none())
    }
    /// アンテナの届く距離 m（なければ 0）
    #[getter]
    fn antenna_range(&self, py: Python<'_>) -> R<PyObject> {
        let a = self.port(py).call_method0("antenna")?;
        Ok(if a.is_truthy()? { a.get_item(0)?.unbind() } else { 0.0.into_py(py) })
    }
    /// チャンネル channel に data を送る
    fn send<'py>(&self, py: Python<'py>, channel: &Bound<'py, PyAny>, data: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("radio_send", (channel, data))
    }
    /// 前回呼んだ後にチャンネル channel に届いた情報のリスト（古い順）
    fn receive<'py>(&self, py: Python<'py>, channel: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("radio_receive", (self.ci, channel))
    }
    /// チャンネル channel に届いている一番新しい情報（max_age 秒より古いものや、無いときは default）
    #[pyo3(signature = (channel, default=None, max_age=1.0))]
    fn latest<'py>(&self, py: Python<'py>, channel: &Bound<'py, PyAny>, default: Option<PyObject>, max_age: f64) -> R<Bound<'py, PyAny>> {
        self.port(py).call_method1("radio_latest", (channel, default, max_age))
    }

    /// MOD が足した api の口（extensions.register_api）
    fn __getattr__(slf: &Bound<'_, Self>, name: &str) -> R<PyObject> {
        let py = slf.py();
        if name.starts_with('_') {
            return Err(PyAttributeError::new_err(name.to_string()));
        }
        let f = ext(py)?.getattr("API_FUNCS")?.call_method1("get", (name,))?;
        if f.is_none() {
            return Err(PyAttributeError::new_err(format!("api に '{name}' はありません")));
        }
        Ok(crate::game::imp(py, "functools")?.getattr("partial")?.call1((f, slf))?.unbind())
    }

    #[pyo3(signature = (*msg))]
    fn log(&self, py: Python<'_>, msg: &Bound<'_, PyTuple>) -> R<()> {
        let parts: Vec<String> = msg.iter().map(|m| m.str().map(|s| s.to_string())).collect::<R<_>>()?;
        let text = parts.join(" ");
        self.logs.bind(py).append(&text)?;
        let st = self.port(py).call_method1("comp_state", (self.ci,))?;
        let buf = st.call_method1("setdefault", ("_log", PyList::empty_bound(py)))?;
        let t = float_of(&self.port(py).getattr("time")?)?;
        buf.call_method1("append", (format!("[{:.1}] {text}", t),))?;
        let n = buf.len()?;
        if n > 50 {
            buf.del_item(pyo3::types::PySlice::new_bound(py, 0, (n - 50) as isize, 1))?;
        }
        let name = self.port(py).call_method1("devices_name", (self.ci,))?.str()?.to_string();
        crate::game::imp(py, "logging")?.getattr("getLogger")?.call1(("alpedia.script",))?.call_method1("info", ("[%s] %s", &name, &text))?;
        self.port(py).call_method1("message", (format!("[{name}] {text}"),))?;
        Ok(())
    }
}

/// コンピューターのスクリプトを動かす（tick(api) を一定間隔で呼ぶ）
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct ScriptRunner {
    #[pyo3(get)]
    world: PyObject,
    /// スクリプト名 → (更新時刻, 大きさ, コード)
    #[pyo3(get)]
    cache: Py<PyDict>,
}

fn safe_builtins<'py>(py: Python<'py>) -> R<Bound<'py, PyDict>> {
    let b = crate::game::imp(py, "builtins")?;
    let d = PyDict::new_bound(py);
    for k in SAFE_BUILTINS.iter().chain(SAFE_BUILTINS2.iter()) {
        if let Ok(v) = b.getattr(*k) {
            d.set_item(*k, v)?;
        }
    }
    d.set_item("__import__", wrap_pyfunction_bound!(safe_import, py)?)?;
    d.set_item("print", py.None())?;
    Ok(d)
}

#[pymethods]
impl ScriptRunner {
    #[new]
    fn new(py: Python<'_>, world: PyObject) -> Self {
        ScriptRunner { world, cache: PyDict::new_bound(py).unbind() }
    }

    fn invalidate(&self, py: Python<'_>, name: &Bound<'_, PyAny>) -> R<()> {
        self.cache.bind(py).call_method1("pop", (name, py.None()))?;
        Ok(())
    }

    fn _load<'py>(&self, py: Python<'py>, name: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let path = script_path(&self.world.bind(py).getattr("dir")?, name)?;
        if !path.call_method0("exists")?.is_truthy()? {
            return Err(pyo3::exceptions::PyFileNotFoundError::new_err(format!("スクリプト {} が scripts/ にありません", name.str()?)));
        }
        let stt = path.call_method0("stat")?;
        let mt = PyTuple::new_bound(py, [stt.getattr("st_mtime_ns")?, stt.getattr("st_size")?]).into_any();
        let cache = self.cache.bind(py);
        if let Some(ent) = cache.get_item(name)? {
            if ent.get_item(0)?.eq(&mt)? {
                return ent.get_item(1);
            }
        }
        let kw = PyDict::new_bound(py);
        kw.set_item("encoding", "utf-8")?;
        let src = path.call_method("read_text", (), Some(&kw))?;
        let code = crate::game::imp(py, "builtins")?.getattr("compile")?.call1((src, path.str()?, "exec"))?;
        cache.set_item(name, (&mt, &code))?;
        crate::game::imp(py, "logging")?.getattr("getLogger")?.call1(("alpedia.script",))?.call_method1("info", ("スクリプト読み込み: %s", name))?;
        Ok(code)
    }

    fn tick(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, dt: f64) -> R<()> {
        let py = slf.py();
        let world = slf.borrow().world.clone_ref(py).into_bound(py);
        let key = PyTuple::new_bound(py, [c.getattr("version")?.unbind(), c.getattr("cells")?.len()?.into_py(py)]).into_any();
        let cd = c.getattr("__dict__")?;
        let cache = cd.call_method1("get", ("_comp_cache",))?;
        let ids: Vec<usize> = if !cache.is_none() && cache.get_item(0)?.eq(&key)? {
            cache.get_item(1)?.extract()?
        } else {
            let reg = c.getattr("reg")?;
            let mut v = Vec::new();
            for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
                let d = d?;
                if !d.is_truthy()? {
                    continue;
                }
                let dd = reg.call_method1("device", (d.get_item("def")?,))?;
                if dd.is_truthy()? && dd.call_method1("get", ("type",))?.eq("computer")? {
                    v.push(i);
                }
            }
            c.setattr("_comp_cache", (&key, v.clone()))?;
            v
        };
        let reg = c.getattr("reg")?;
        let status = c.getattr("status")?;
        for i0 in ids {
            let devs = c.getattr("devs")?;
            if i0 >= devs.len()? || !devs.get_item(i0)?.is_truthy()? {
                continue;
            }
            let d = devs.get_item(i0)?;
            let dd = reg.call_method1("device", (d.get_item("def")?,))?;
            if !dd.is_truthy()? || !dd.get_item("type")?.eq("computer")? {
                continue;
            }
            let st = d.get_item("state")?;
            if !st.call_method1("get", ("running",))?.is_truthy()? {
                continue;
            }
            let g = |k: &str, dflt: f64| -> R<f64> { float_of(&status.call_method1("get", (k, dflt))?) };
            if g("ratio", 1.0)? <= 0.0 || (g("stored", 0.0)? <= 0.0 && g("production", 0.0)? <= 0.0) {
                st.set_item("running", false)?;
                st.set_item("error", "電力不足")?;
                continue;
            }
            let acc = float_of(&st.call_method1("get", ("_acc", 0.0))?)? + dt;
            st.set_item("_acc", acc)?;
            let interval = float_of(&dd.get_item("params")?.call_method1("get", ("interval", 0.05))?)?;
            if acc < interval {
                continue;
            }
            st.set_item("_acc", 0.0)?;
            let mut path: Option<Bound<'_, PyAny>> = None;
            let res: R<()> = (|| {
                let sname = st.call_method1("get", ("script", ""))?;
                path = Some(script_path(&world.getattr("dir")?, &sname)?);
                let code = slf.borrow()._load(py, &sname)?;
                let mut env = st.call_method1("get", ("_env",))?;
                let stale = env.is_none() || !env.call_method1("get", ("__code__",))?.is(&code);
                if stale {
                    let e = PyDict::new_bound(py);
                    e.set_item("__builtins__", safe_builtins(py)?)?;
                    e.set_item("math", crate::game::imp(py, "math")?)?;
                    e.set_item("random", crate::game::imp(py, "random")?)?;
                    e.set_item("__code__", &code)?;
                    crate::game::imp(py, "builtins")?.getattr("exec")?.call1((&code, &e))?;
                    st.set_item("_env", &e)?;
                    env = e.into_any();
                }
                let devs = c.getattr("devs")?;
                let mut i = i0;
                if i >= devs.len()? || !devs.get_item(i)?.is(&d) {
                    // 分離・結合で番号が変わった
                    let mut found = None;
                    for (k, x) in devs.iter()?.enumerate() {
                        if x?.is(&d) {
                            found = Some(k);
                            break;
                        }
                    }
                    match found {
                        Some(k) => i = k,
                        None => return Ok(()),
                    }
                }
                let api = Py::new(py, ScriptAPI::new(py, world.clone().unbind(), c.clone().unbind(), i as i64, None)?)?;
                env.get_item("__builtins__")?.set_item("print", api.bind(py).getattr("log")?)?;
                let f = env.call_method1("get", ("tick",))?;
                if f.is_none() {
                    return Err(PyRuntimeError::new_err("tick(api) 関数がありません"));
                }
                f.call1((api,))?;
                Ok(())
            })();
            if let Err(e) = res {
                let ev = e.value_bound(py);
                st.set_item("running", false)?;
                let tn = ev.get_type().name()?.to_string();
                let mut msg = format!("{tn}: {}", ev.str()?);
                let mut line: Option<i64> = None;
                if e.is_instance_of::<pyo3::exceptions::PySyntaxError>(py) {
                    line = ev.getattr("lineno").ok().and_then(|l| l.extract().ok());
                }
                if line.is_none() {
                    if let Some(p) = &path {
                        let ps = p.str()?.to_string();
                        let tb = e.traceback_bound(py);
                        if let Some(tb) = tb {
                            for fr in crate::game::imp(py, "traceback")?.getattr("extract_tb")?.call1((tb,))?.iter()? {
                                let fr = fr?;
                                if fr.getattr("filename")?.str()?.to_string() == ps {
                                    line = fr.getattr("lineno")?.extract().ok();
                                }
                            }
                        }
                    }
                }
                match line {
                    Some(l) => {
                        st.set_item("error_line", l)?;
                        msg.push_str(&format!("（{l} 行目）"));
                    }
                    None => {
                        st.call_method1("pop", ("error_line", py.None()))?;
                    }
                }
                st.set_item("error", &msg)?;
                let buf = st.call_method1("setdefault", ("_log", PyList::empty_bound(py)))?;
                let t = float_of(&world.getattr("time")?)?;
                buf.call_method1("append", (format!("[{:.1}] エラー: {msg}", t),))?;
                st.call_method1("pop", ("_env", py.None()))?;
                crate::game::imp(py, "logging")?.getattr("getLogger")?.call1(("alpedia.script",))?
                    .call_method1("error", ("スクリプトエラー [%s/%s]: %s", c.getattr("name")?, d.get_item("name")?, &msg))?;
                world.call_method1("message", (format!("スクリプトエラー: {msg}"),))?;
            }
        }
        Ok(())
    }
}
