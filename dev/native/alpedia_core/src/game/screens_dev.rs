//! Rust の画面：機器の設定（P）（main/client_rs/screens_dev.py）。今までの画面の device_ui.DevicePanel・actuator_ui.ActuatorPanel と同じ中身。

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};

use super::imp;
use super::screens::{btn, me, partial, txt, GREY};
use super::ui::Screen;

type R<T> = PyResult<T>;
type C4 = (f64, f64, f64, f64);
const BLUE: C4 = (0.22, 0.32, 0.45, 1.0);
const WHITE: C4 = (1.0, 1.0, 1.0, 1.0);

/// format(v, spec)（Python と同じ書式）
fn pf(v: &Bound<'_, PyAny>, spec: &str) -> R<String> {
    Ok(v.py().import_bound("builtins")?.getattr("format")?.call1((v, spec))?.extract()?)
}
fn pff(py: Python<'_>, v: f64, spec: &str) -> R<String> {
    pf(&v.into_py(py).into_bound(py), spec)
}
fn a<'py>(s: &Bound<'py, PyAny>, k: &str) -> R<Bound<'py, PyAny>> {
    s.getattr(k)
}
fn dui(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    Ok(imp(py, "main.client.device_ui")?.into_any())
}
fn aui(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    Ok(imp(py, "main.client.actuator_ui")?.into_any())
}
fn act(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    Ok(imp(py, "main.actuator")?.into_any())
}
/// self.button(t, x, y, w, 0.042, fn, col, size)
fn b<'py>(s: &Bound<'py, PyAny>, t: &str, x: f64, y: f64, f: PyObject, w: f64, col: C4, size: f64) -> R<()> {
    s.call_method1("button", (t, x, y, w, 0.042, f, col, size))?;
    Ok(())
}
fn dev_of<'py>(s: &Bound<'py, PyAny>, i: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    a(s, "c")?.getattr("devs")?.get_item(i)
}

/// 機器の名前などの設定。on_close：閉じたあとに呼ぶ（アクチュエーターの設定から開いたとき）
#[pyclass(module = "alpedia_core", extends = Screen, subclass)]
pub struct DevicePanel {}

#[pymethods]
impl DevicePanel {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> (Self, Screen) {
        (DevicePanel {}, Screen::blank(py))
    }
    #[pyo3(signature = (ui, viewer, c, index, on_close=None))]
    fn __init__(slf: &Bound<'_, Self>, ui: &Bound<'_, PyAny>, viewer: &Bound<'_, PyAny>, c: &Bound<'_, PyAny>, index: &Bound<'_, PyAny>,
                on_close: Option<PyObject>) -> R<()> {
        let py = slf.py();
        Screen::init_base(slf.downcast::<Screen>()?, ui)?;
        let s = slf.as_any();
        s.setattr("v", viewer)?;
        s.setattr("w", viewer.getattr("w")?)?;
        s.setattr("c", c)?;
        s.setattr("cell", c.getattr("cells")?.get_item(index)?)?;
        s.setattr("on_close", on_close.unwrap_or_else(|| py.None()))?;
        let d = c.getattr("devs")?.get_item(index)?;
        let mut dd = c.getattr("reg")?.call_method1("device", (d.get_item("def")?,))?;
        if !dd.is_truthy()? {
            dd = PyDict::new_bound(py).into_any();
        }
        let ty = dd.call_method1("get", ("type",))?;
        s.setattr("dtype", &ty)?;
        s.setattr("default_name", dd.call_method1("get", ("name", d.get_item("def")?))?)?;
        s.setattr("type_name", dui(py)?.getattr("TYPE_NAMES")?.call_method1("get", (&ty, dd.call_method1("get", ("type", "機器"))?))?)?;
        s.setattr("start_name", d.get_item("name")?)?;
        Ok(())
    }
    #[getter]
    fn index<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let s = slf.as_any();
        a(s, "c")?.getattr("index")?.call_method1("get", (a(s, "cell")?,))
    }
    fn _noop(&self) {}
    fn _name_default(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        a(s, "name")?.call_method1("set", (a(s, "default_name")?,))?;
        Ok(())
    }
    fn _name_back(slf: &Bound<'_, Self>) -> R<()> {
        let f = a(slf.as_any(), "name")?;
        let mut v: String = f.getattr("value")?.extract()?;
        v.pop();
        f.call_method1("set", (v,))?;
        Ok(())
    }
    fn build(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.call_method0("clear")?;
        btn(s, "", 0.0, 0.0, 3.0, 2.0, a(s, "_noop")?.unbind(), (0.0, 0.0, 0.0, 0.55), 0.022, None, None)?;
        s.call_method1("panel", (-0.55, 0.4, 0.55, -0.4))?;
        txt(s, format!("{} の設定", a(s, "type_name")?.str()?), 0.0, 0.37, 0.035, WHITE, (0.5, 0.0))?;
        txt(s, "名前", -0.5, 0.265, 0.0225, WHITE, (0.0, 0.0))?;
        let du = dui(py)?;
        let name = s.call_method1("field", (-0.26, 0.25, 0.62, 0.05, a(s, "start_name")?, du.getattr("NAME_LIMIT")?, a(s, "ok")?))?;
        s.setattr("name", &name)?;
        b(s, "既定の名前", -0.27, 0.16, a(s, "_name_default")?.unbind(), 0.17, BLUE, 0.0175)?;
        b(s, "1文字消す", -0.09, 0.16, a(s, "_name_back")?.unbind(), 0.17, BLUE, 0.0175)?;
        b(s, "全部消す", 0.09, 0.16, partial(&name.getattr("set")?, vec!["".into_py(py)], None)?, 0.17, BLUE, 0.0175)?;
        b(s, "同名に番号を振る", 0.29, 0.16, a(s, "number_all")?.unbind(), 0.2, (0.6, 0.45, 0.15, 1.0), 0.0175)?;
        s.setattr("info", txt(s, "", -0.5, 0.09, 0.018, GREY, (0.0, 0.0))?)?;
        txt(s, "同じ名前の機器は、コンピューターからまとめて動きます。\n\
                名前を分けると、ブロックの機器選択や api.device(\"名前\") で個別に制御できます。\n\
                （名前はキーボードで入力します。日本語の入力もできます）", -0.5, -0.09, 0.0155, GREY, (0.0, 0.0))?;
        s.setattr("valve_text", py.None())?;
        let dt = a(s, "dtype")?;
        if dt.eq("valve")? {
            s.setattr("valve_text", txt(s, "", -0.5, -0.23, 0.02, WHITE, (0.0, 0.0))?)?;
            for (k, v) in [0.0, 0.1, 0.25, 0.5, 0.75, 1.0].iter().enumerate() {
                b(s, &pff(py, *v, ".0%")?, -0.18 + k as f64 * 0.1, -0.245, me(s, "set_open", vec![v.into_py(py)])?, 0.085, BLUE, 0.0175)?;
            }
        } else if dt.eq("thruster")? {
            txt(s, "E で点火（電気を少し使う）。燃えている間は発電もする。\n\
                    止める・出力を変えるには、タンクとのあいだにバルブを入れて、バルブで。", -0.5, -0.22, 0.0155, (1.0, 0.85, 0.5, 1.0), (0.0, 0.0))?;
        }
        b(s, "OK", 0.2, -0.33, a(s, "ok")?.unbind(), 0.16, (0.2, 0.5, 0.3, 1.0), 0.02)?;
        b(s, "キャンセル", 0.38, -0.33, a(s, "close")?.unbind(), 0.16, (0.3, 0.3, 0.33, 1.0), 0.02)?;
        s.setattr("_key", py.None())?;
        Ok(())
    }
    fn number_all(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = a(s, "index")?;
        if i.is_none() {
            return Ok(());
        }
        let d = dev_of(s, &i)?;
        let nv: String = a(s, "name")?.getattr("value")?.call_method0("strip")?.extract()?;
        let name: Bound<'_, PyAny> = if nv.is_empty() { d.get_item("name")? } else { PyString::new_bound(py, &nv).into_any() };
        d.set_item("name", &name)?;
        let n = dui(py)?.call_method1("number_same_named", (a(s, "c")?, &name))?;
        a(s, "name")?.call_method1("set", (dev_of(s, &i)?.get_item("name")?,))?;
        let msg = if n.is_truthy()? { format!("「{}」{} 個に番号を付けました", name.str()?, n.str()?) } else { format!("「{}」という名前の機器は1つだけです", name.str()?) };
        a(s, "w")?.call_method1("message", (msg,))?;
        Ok(())
    }
    fn set_open(slf: &Bound<'_, Self>, v: f64) -> R<()> {
        let s = slf.as_any();
        let i = a(s, "index")?;
        if i.is_none() {
            return Ok(());
        }
        let st = dev_of(s, &i)?.get_item("state")?;
        st.set_item("open", v)?;
        if v > 0.0 {
            st.set_item("set", v)?;
        }
        Ok(())
    }
    fn update(slf: &Bound<'_, Self>, _dt: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = a(s, "index")?;
        let info = a(s, "info")?;
        if i.is_none() {
            info.call_method1("set", ("機器が見つかりません", 0.018, GREY))?;
            return Ok(());
        }
        let d = dev_of(s, &i)?;
        let vt = a(s, "valve_text")?;
        if !vt.is_none() {
            let o: f64 = d.get_item("state")?.call_method1("get", ("open", 1.0))?.extract()?;
            vt.call_method1("set", (format!("開き具合 {}{}", pff(py, o, ".0%")?, if o <= 0.0 { "（閉）" } else { "" }), 0.02))?;
        }
        let name: String = a(s, "name")?.getattr("value")?.call_method0("strip")?.extract()?;
        let c = a(s, "c")?;
        let mut dup = 0;
        for k in dui(py)?.call_method1("same_named", (&c, &name))?.iter()? {
            if !k?.eq(&i)? {
                dup += 1;
            }
        }
        let mut lines = vec![format!("構築物: {}　種類: {}（{}）", c.getattr("name")?.str()?, a(s, "type_name")?.str()?, d.get_item("def")?.str()?)];
        if name.is_empty() {
            lines.push("名前が空です".into());
        } else if dup > 0 {
            lines.push(format!("⚠ 同じ名前の機器がほかに {} 個あります（まとめて動きます）", dup));
        } else {
            lines.push("この構築物の中で一つだけの名前です（個別に制御できます）".into());
        }
        lines.push(format!("コンピューターからは  api.device(\"{}\")", name));
        info.call_method1("set", (lines.join("\n"), 0.018, GREY))?;
        Ok(())
    }
    fn ok(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = a(s, "index")?;
        let name: String = a(s, "name")?.getattr("value")?.call_method0("strip")?.extract()?;
        if !i.is_none() && !name.is_empty() {
            let d = dev_of(s, &i)?;
            let old = d.get_item("name")?;
            d.set_item("name", &name)?;
            if !old.eq(&name)? {
                let c = a(s, "c")?;
                let du = dui(py)?;
                let mut n = 0i64;
                if !du.call_method1("same_named", (&c, &old))?.is_truthy()? {
                    n = du.call_method1("update_programs", (a(s, "w")?, &c, &old, &name))?.extract()?;
                }
                a(s, "w")?.call_method1("message", (format!("名前を変更: {} → {}{}", old.str()?, name,
                    if n != 0 { format!("（ブロックプログラム {} 個も更新）", n) } else { String::new() }),))?;
            }
        }
        s.call_method0("close")?;
        Ok(())
    }
    fn on_key(slf: &Bound<'_, Self>, key: &str) -> R<bool> {
        if key == "escape" {
            slf.call_method0("close")?;
        }
        Ok(true)
    }
    fn close(slf: &Bound<'_, Self>) -> R<()> {
        Screen::close(slf.downcast::<Screen>()?)?;
        let oc = a(slf.as_any(), "on_close")?;
        if !oc.is_none() {
            oc.call0()?;
        } else {
            a(slf.as_any(), "v")?.call_method0("screen_closed")?;
        }
        Ok(())
    }
}

/// 値の行の ◀ ▶（押すたびに次の値へ）
#[pyclass(module = "alpedia_core")]
pub struct RowStep {
    values: Py<PyList>,
    i: usize,
    val: PyObject,
    y: f64,
    fmt: PyObject,
    on_change: PyObject,
}

#[pymethods]
impl RowStep {
    fn __call__(&mut self, py: Python<'_>, d: i64) -> R<()> {
        let vals = self.values.bind(py);
        let n = vals.len() as i64;
        self.i = (self.i as i64 + d).rem_euclid(n) as usize;
        let v = vals.get_item(self.i)?;
        let (w, h): (f64, f64) = self.val.bind(py).call_method1("set", (self.fmt.bind(py).call1((&v,))?, 0.0225))?.extract()?;
        self.val.bind(py).call_method1("place", (0.17 - w / 2.0, self.y + h / 2.0))?;
        self.on_change.bind(py).call1((v,))?;
        Ok(())
    }
}

/// 値の書き方（"names" は動作の名前の辞書、ほかは format の書式＋単位）
#[pyclass(module = "alpedia_core")]
pub struct Fmt {
    names: PyObject,
    spec: String,
    unit: String,
}

#[pymethods]
impl Fmt {
    fn __call__(&self, py: Python<'_>, v: &Bound<'_, PyAny>) -> R<String> {
        if !self.names.is_none(py) {
            return self.names.bind(py).get_item(v)?.extract();
        }
        Ok(format!("{}{}", pf(v, &self.spec)?, self.unit))
    }
}

fn fmt(py: Python<'_>, spec: &str, unit: &str) -> R<PyObject> {
    Ok(Py::new(py, Fmt { names: py.None(), spec: spec.into(), unit: unit.into() })?.into_any())
}

/// アクチュエーター・ピストン・ヒンジの動作設定
#[pyclass(module = "alpedia_core", extends = Screen, subclass)]
pub struct ActuatorPanel {}

#[pymethods]
impl ActuatorPanel {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> (Self, Screen) {
        (ActuatorPanel {}, Screen::blank(py))
    }
    fn __init__(slf: &Bound<'_, Self>, ui: &Bound<'_, PyAny>, viewer: &Bound<'_, PyAny>, c: &Bound<'_, PyAny>, index: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        Screen::init_base(slf.downcast::<Screen>()?, ui)?;
        let s = slf.as_any();
        s.setattr("v", viewer)?;
        s.setattr("w", viewer.getattr("w")?)?;
        s.setattr("c", c)?;
        s.setattr("cell", c.getattr("cells")?.get_item(index)?)?;
        let d = c.getattr("devs")?.get_item(index)?;
        let st = d.get_item("state")?;
        let reg = c.getattr("reg")?;
        let mut dd = reg.call_method1("device", (d.get_item("def")?,))?;
        if !dd.is_truthy()? {
            let x = PyDict::new_bound(py);
            x.set_item("params", PyDict::new_bound(py))?;
            dd = x.into_any();
        }
        let ac = act(py)?;
        ac.call_method1("init_state", (&st, dd.get_item("params")?))?;
        let mut p0 = st.call_method1("get", ("preset",))?;
        if !p0.is_truthy()? {
            p0 = ac.call_method1("default_preset", (&d, &reg))?;
        }
        let mut p = PyDict::new_bound(py).into_any();
        p.call_method1("update", (p0,))?;
        let actions = ac.call_method1("actions_for", (&d, &reg))?;
        s.setattr("actions", &actions)?;
        if !actions.contains(p.call_method1("get", ("action",))?)? {
            p = ac.call_method1("default_preset", (&d, &reg))?;
        }
        s.setattr("hinge", ac.call_method1("is_hinge_dev", (&d, &reg))?)?;
        let pr = PyDict::new_bound(py);
        pr.set_item("action", p.call_method1("get", ("action", "rotate"))?)?;
        pr.set_item("amount", p.call_method1("get", ("amount", 90.0))?)?;
        pr.set_item("speed", p.call_method1("get", ("speed", 90.0))?)?;
        s.setattr("preset", pr)?;
        s.call_method0("_fix_values")?;
        Ok(())
    }
    #[getter]
    fn index<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let s = slf.as_any();
        a(s, "c")?.getattr("index")?.call_method1("get", (a(s, "cell")?,))
    }
    fn _noop(&self) {}
    fn _fix_values(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let p = a(s, "preset")?;
        let au = aui(py)?;
        let near = au.getattr("_nearest")?;
        let ac = act(py)?;
        let act_ = p.get_item("action")?;
        let hinge = a(s, "hinge")?.is_truthy()?;
        if act_.eq("rotate")? {
            let am = if hinge { ac.getattr("HINGE_AMOUNTS")? } else { au.getattr("ROT_AMOUNTS")? };
            p.set_item("amount", near.call1((am, p.get_item("amount")?, 90.0))?)?;
            p.set_item("speed", near.call1((au.getattr("ROT_SPEEDS")?, p.get_item("speed")?, 90.0))?)?;
        } else if act_.eq("extend")? || act_.eq("retract")? {
            let am = p.get_item("amount")?;
            let am = if am.is_truthy()? { am } else { 1.0f64.into_py(py).into_bound(py) };
            let am: f64 = py.import_bound("builtins")?.getattr("float")?.call1((am,))?.extract()?;
            p.set_item("amount", near.call1((au.getattr("EXT_AMOUNTS")?, am.abs(), 1.0))?)?;
            p.set_item("speed", near.call1((au.getattr("EXT_SPEEDS")?, p.get_item("speed")?, 0.5))?)?;
        }
        Ok(())
    }
    fn build(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.call_method0("clear")?;
        btn(s, "", 0.0, 0.0, 3.0, 2.0, a(s, "_noop")?.unbind(), (0.0, 0.0, 0.0, 0.5), 0.022, None, None)?;
        s.call_method1("panel", (-0.5, 0.39, 0.5, -0.39, (0.06, 0.07, 0.1, 0.97)))?;
        let i = a(s, "index")?;
        let d = if i.is_none() { py.None().into_bound(py) } else { dev_of(s, &i)? };
        let name = if d.is_truthy()? { d.get_item("name")?.str()?.to_string() } else { "アクチュエーター".into() };
        txt(s, format!("{} の動作設定", name), -0.1, 0.35, 0.032, WHITE, (0.5, 0.5))?;
        s.call_method1("button", ("名前を変更", 0.36, 0.35, 0.26, 0.055, a(s, "open_rename")?, (0.6, 0.45, 0.15, 1.0), 0.02))?;
        s.setattr("status", txt(s, "", -0.46, 0.27, 0.0188, GREY, (0.0, 0.0))?)?;
        let y = 0.12;
        let p = a(s, "preset")?;
        let act_ = p.get_item("action")?;
        let hinge = a(s, "hinge")?.is_truthy()?;
        let ac = act(py)?;
        let au = aui(py)?;
        let names = PyDict::new_bound(py);
        names.call_method1("update", (ac.getattr("ACTION_NAMES")?,))?;
        if hinge {
            names.set_item("rotate", "曲げる")?;
        }
        let nf = Py::new(py, Fmt { names: names.into_any().unbind(), spec: String::new(), unit: String::new() })?.into_any();
        Self::_row(slf, "動作", y, &a(s, "actions")?, &act_, nf, a(s, "_set_action")?.unbind())?;
        let setter = |k: &str| -> R<PyObject> { partial(&p.getattr("__setitem__")?, vec![k.into_py(py)], None) };
        if act_.eq("rotate")? {
            let am = if hinge { ac.getattr("HINGE_AMOUNTS")? } else { au.getattr("ROT_AMOUNTS")? };
            Self::_row(slf, if hinge { "曲げる角度" } else { "回転量" }, y - 0.08, &am, &p.get_item("amount")?, fmt(py, "+.0f", "°")?, setter("amount")?)?;
            Self::_row(slf, "速さ", y - 0.16, &au.getattr("ROT_SPEEDS")?, &p.get_item("speed")?, fmt(py, ".0f", "°/秒")?, setter("speed")?)?;
            if hinge {
                let dev = dev_of(s, &i)?;
                let k = PyDict::new_bound(py);
                k.set_item("align", 1)?;
                k.set_item("anchor", (0.5, 0.5))?;
                s.call_method("text", (format!("{}：ブロックの中心を軸に -90〜+90° 振ります。E を押すたびにこの角度と 0° を行き来します。\n倒れる側は置き方で決まります（置くときに見ていた側が +）",
                    ac.call_method1("mode_name", (dev.get_item("state")?, &dev))?.str()?), 0.0, y - 0.23, 0.015, GREY), Some(&k))?;
            }
        } else if act_.eq("extend")? || act_.eq("retract")? {
            Self::_row(slf, "移動量", y - 0.08, &au.getattr("EXT_AMOUNTS")?, &p.get_item("amount")?, fmt(py, ".2f", " ブロック")?, setter("amount")?)?;
            Self::_row(slf, "移動速度", y - 0.16, &au.getattr("EXT_SPEEDS")?, &p.get_item("speed")?, fmt(py, ".2f", " ブロック/秒")?, setter("speed")?)?;
        } else {
            let msg = if act_.eq("detach")? {
                "出力面の先の部分を切り離します"
            } else if act_.eq("none")? {
                "E・操縦席のキー（X）では動かしません（コンピューターのプログラムで動かす物に）"
            } else {
                "出力面の先にあるブロック（別の構築物でも可）とつなぎます"
            };
            txt(s, msg, 0.0, y - 0.11, 0.02, GREY, (0.5, 0.5))?;
        }
        let k = PyDict::new_bound(py);
        k.set_item("align", 1)?;
        k.set_item("anchor", (0.5, 0.5))?;
        s.call_method("text", ("閉じた後、このアクチュエーターを見て E で実行します\n操縦席では X キーで実行／コンピューターからは rotate() extend() など",
            0.0, -0.205, 0.0155, GREY), Some(&k))?;
        s.call_method1("button", ("設定して閉じる", -0.3, -0.3, 0.26, 0.055, a(s, "save_close")?, (0.2, 0.45, 0.3, 1.0), 0.02))?;
        s.call_method1("button", ("今すぐ実行", 0.0, -0.3, 0.26, 0.055, a(s, "run_now")?, (0.2, 0.5, 0.3, 1.0), 0.02))?;
        s.call_method1("button", ("キャンセル", 0.3, -0.3, 0.26, 0.055, a(s, "close")?, (0.3, 0.3, 0.33, 1.0), 0.02))?;
        Ok(())
    }
    fn _row(slf: &Bound<'_, Self>, label: &str, y: f64, values: &Bound<'_, PyAny>, current: &Bound<'_, PyAny>, fmt: PyObject, on_change: PyObject) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let vals = PyList::new_bound(py, values.iter()?.collect::<R<Vec<_>>>()?);
        let idx = if vals.contains(current)? { vals.index(current)? } else { 0 };
        txt(s, label, -0.4, y, 0.0225, WHITE, (0.0, 0.5))?;
        let val = txt(s, fmt.bind(py).call1((vals.get_item(idx)?,))?, 0.17, y, 0.0225, WHITE, (0.5, 0.5))?;
        let step = Py::new(py, RowStep { values: vals.unbind(), i: idx, val: val.unbind(), y, fmt, on_change })?.into_bound(py).into_any();
        s.call_method1("button", ("◀", -0.02, y, 0.05, 0.05, partial(&step, vec![(-1i64).into_py(py)], None)?, BLUE))?;
        s.call_method1("button", ("▶", 0.36, y, 0.05, 0.05, partial(&step, vec![1i64.into_py(py)], None)?, BLUE))?;
        Ok(())
    }
    fn _set_action(slf: &Bound<'_, Self>, v: &Bound<'_, PyAny>) -> R<()> {
        let p = a(slf.as_any(), "preset")?;
        p.set_item("action", v)?;
        if v.eq("rotate")? {
            p.set_item("amount", 90.0)?;
            p.set_item("speed", 90.0)?;
        } else if v.eq("extend")? || v.eq("retract")? {
            p.set_item("amount", 1.0)?;
            p.set_item("speed", 0.5)?;
        }
        slf.call_method0("build")?;
        Ok(())
    }
    fn _store(slf: &Bound<'_, Self>) -> R<bool> {
        let s = slf.as_any();
        let i = a(s, "index")?;
        if i.is_none() {
            return Ok(false);
        }
        let d = PyDict::new_bound(s.py());
        d.call_method1("update", (a(s, "preset")?,))?;
        dev_of(s, &i)?.get_item("state")?.set_item("preset", d)?;
        Ok(true)
    }
    fn save_close(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if s.call_method0("_store")?.is_truthy()? {
            let t = act(py)?.call_method1("preset_text", (dev_of(s, &a(s, "index")?)?.get_item("state")?, a(s, "hinge")?))?;
            a(s, "w")?.call_method1("message", (format!("動作を設定: {}（E で実行）", t.str()?),))?;
        }
        s.call_method0("close")?;
        Ok(())
    }
    fn run_now(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if s.call_method0("_store")?.is_truthy()? {
            let w = a(s, "w")?;
            let net = w.getattr("net").unwrap_or_else(|_| py.None().into_bound(py));
            if !net.is_none() && net.getattr("role")?.eq("client")? {
                net.call_method1("actuator_run", (a(s, "c")?, a(s, "index")?))?;
            } else {
                w.call_method1("message", (act(py)?.call_method1("execute_preset", (&w, a(s, "c")?, a(s, "index")?))?,))?;
            }
        }
        s.call_method0("close")?;
        Ok(())
    }
    fn open_rename(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = a(s, "index")?;
        if !i.is_none() {
            let k = PyDict::new_bound(py);
            k.set_item("on_close", a(s, "build")?)?;
            a(s, "ui")?.call_method("open", (py.get_type_bound::<DevicePanel>(), a(s, "v")?, a(s, "c")?, i), Some(&k))?;
        }
        Ok(())
    }
    fn on_key(slf: &Bound<'_, Self>, key: &str) -> R<bool> {
        if key == "escape" {
            slf.call_method0("save_close")?;
        } else if key == "enter" {
            slf.call_method0("run_now")?;
        }
        Ok(true)
    }
    fn update(slf: &Bound<'_, Self>, _dt: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = a(s, "index")?;
        let status = a(s, "status")?;
        if i.is_none() {
            status.call_method1("set", ("アクチュエーターが失われました", 0.0188, GREY))?;
            return Ok(());
        }
        let c = a(s, "c")?;
        let dev = dev_of(s, &i)?;
        let st = dev.get_item("state")?;
        let cs = c.getattr("status")?;
        let ac = act(py)?;
        let att = if ac.call_method1("is_attached", (&c, &i))?.is_truthy()? { "あり" } else { "なし" };
        let moving = if ac.call_method1("busy", (&st,))?.is_truthy()? { "動作中" } else { "停止" };
        let m = ac.call_method1("motion", (&dev, c.getattr("reg")?))?;
        let g = |k: &str, spec: &str| -> R<String> { pf(&st.call_method1("get", (k, 0))?, spec) };
        let val = if m.eq("piston")? {
            format!("伸び {} ブロック   ", g("ext", ".2f")?)
        } else if m.eq("both")? {
            format!("角度 {}°   伸び {} ブロック   ", g("angle", ".0f")?, g("ext", ".2f")?)
        } else {
            format!("角度 {}°   ", g("angle", ".0f")?)
        };
        let stored: f64 = if cs.is_truthy()? { cs.call_method1("get", ("stored", 0))?.extract()? } else { 0.0 };
        status.call_method1("set", (format!("{}出力面の結合 {}   {}\n電力 {} MJ（動かすには電力が必要）", val, att, moving, pff(py, stored / 1e6, ".2f")?),
            0.0188, GREY))?;
        Ok(())
    }
    fn close(slf: &Bound<'_, Self>) -> R<()> {
        Screen::close(slf.downcast::<Screen>()?)?;
        a(slf.as_any(), "v")?.call_method0("screen_closed")?;
        Ok(())
    }
}
