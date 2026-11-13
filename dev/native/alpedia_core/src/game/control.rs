//! 構築物を外から読み書きする窓口（スクリプト・ブロックプログラム用）。main/control.py の中身。
//!
//! スクリプトの api（scripting）は構築物の中の配列を直接触らず、必ずこの Port を通す。
//! - 読み取りは値のコピー（リスト・辞書・数値）を返す
//! - 書き込みは決まった項目だけ（WRITABLE）。値は検査してから入れる
//! - 機器は番号（構築物の中のブロックの番号）で指す。番号は毎ティック devices() で取り直す

use numpy::{PyArray1, PyReadonlyArray2};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::game::actuator as act;

type R<T> = PyResult<T>;

#[allow(unexpected_cfgs)]
mod err {
    pyo3::create_exception!(alpedia_core, PortError, pyo3::exceptions::PyValueError);
}
pub use err::PortError;

pub const WRITABLE: [&str; 5] = ["throttle", "ignite", "open", "enabled", "running"];
const DIR_NAMES: [&str; 6] = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"];

fn float_of(v: &Bound<'_, PyAny>) -> R<f64> {
    crate::game::imp(v.py(), "builtins")?.getattr("float")?.call1((v,))?.extract()
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Port {
    #[pyo3(get, name = "_w")]
    w: PyObject,
    #[pyo3(get, name = "_c")]
    c: PyObject,
    #[pyo3(get, set, name = "_pose")]
    pose_cache: PyObject,
    /// 姿勢を測るブロック（コンピューター）：その部分の向きを使う（変形しても傾きが狂わない）
    #[pyo3(get, set)]
    frame: PyObject,
}

impl Port {
    pub fn new_pub(py: Python<'_>, world: PyObject, c: PyObject) -> Self {
        Port { w: world, c, pose_cache: py.None(), frame: py.None() }
    }

    fn dev<'py>(&self, py: Python<'py>, i: i64) -> R<Bound<'py, PyAny>> {
        let devs = self.c.bind(py).getattr("devs")?;
        let n = devs.len()? as i64;
        if !(0..n).contains(&i) || !devs.get_item(i)?.is_truthy()? {
            return Err(PortError::new_err(format!("機器 {i} がありません")));
        }
        devs.get_item(i)
    }
    fn reg<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.c.bind(py).getattr("reg")
    }
    fn act_dev<'py>(&self, py: Python<'py>, i: i64) -> R<Bound<'py, PyAny>> {
        let d = self.dev(py, i)?;
        let dd = self.reg(py)?.call_method1("device", (d.get_item("def")?,))?;
        if !dd.is_truthy()? || !dd.get_item("type")?.eq("actuator")? {
            return Err(PortError::new_err(format!("{} はアクチュエーターではありません", d.get_item("name")?.str()?)));
        }
        crate::game::devstate::devstate_fill(py, "actuator", Some(&dd.get_item("params")?), d.get_item("state")?.downcast::<PyDict>()?)?;
        Ok(d)
    }
    fn can(&self, py: Python<'_>, i: i64, action: &str) -> R<()> {
        match act::act_check_action(self.c.bind(py), i as usize, action) {
            Ok(()) => Ok(()),
            Err(e) if e.is_instance_of::<act::MotionError>(py) => Err(PortError::new_err(e.value_bound(py).str()?.to_string())),
            Err(e) => Err(e),
        }
    }
}

#[pymethods]
impl Port {
    #[new]
    fn new(py: Python<'_>, world: PyObject, c: PyObject) -> Self {
        Port { w: world, c, pose_cache: py.None(), frame: py.None() }
    }

    // ------------------------------------------------------------------
    // 構築物全体
    // ------------------------------------------------------------------
    #[getter]
    fn time(&self, py: Python<'_>) -> R<f64> {
        float_of(&self.w.bind(py).getattr("time")?)
    }

    #[getter]
    fn name(&self, py: Python<'_>) -> R<PyObject> {
        Ok(self.c.bind(py).getattr("name")?.unbind())
    }

    /// (重心 [3], 回転行列 R（列がローカル軸のワールド方向）[3x3], 速度 [3], 角速度 [3])。同じティック中は使い回す
    fn pose(&mut self, py: Python<'_>) -> R<PyObject> {
        if self.pose_cache.is_none(py) {
            let c = self.c.bind(py);
            let f = self.frame.bind(py);
            let use_part = !f.is_none() && c.getattr("rotors")?.is_truthy()? && {
                let k: i64 = f.extract()?;
                let n: i64 = c.getattr("n_vox")?.extract()?;
                0 <= k && k < n
            };
            let fit = if use_part {
                c.call_method1("rigid_fit", (c.call_method1("part_of", (f,))?,))?
            } else {
                c.call_method0("rigid_fit")?
            };
            let rr = fit.get_item(0)?;
            self.pose_cache = (c.call_method0("center_of_mass")?, rr, c.call_method0("velocity")?, c.call_method0("angular_velocity")?).into_py(py);
        }
        Ok(self.pose_cache.clone_ref(py))
    }

    fn mass(&self, py: Python<'_>) -> R<f64> {
        float_of(&self.c.bind(py).call_method0("total_mass")?)
    }

    fn power<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyDict>> {
        let d = PyDict::new_bound(py);
        d.update(self.c.bind(py).getattr("status")?.downcast::<pyo3::types::PyMapping>()?)?;
        Ok(d)
    }

    fn storage<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        crate::game::imp(py, "main.devices")?.getattr("storage_total")?.call1((&self.c,))
    }

    fn cavities<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyList>> {
        let out = PyList::empty_bound(py);
        for it in crate::game::imp(py, "main.devices")?.getattr("cavity_report")?.call1((&self.c,))?.iter()? {
            let it = it?;
            let d = PyDict::new_bound(py);
            d.set_item("cells", it.get_item(1)?)?;
            d.set_item("pressure_kpa", it.get_item(2)?)?;
            out.append(d)?;
        }
        Ok(out)
    }

    /// 操縦席に座っている間に押されているキー（座っていなければ空）
    fn input_keys<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        self.w.bind(py).call_method1("keys_for", (&self.c,))
    }

    /// この構築物の操縦席に誰かが座っているか
    fn seated(&self, py: Python<'_>) -> R<bool> {
        let id = self.c.bind(py).getattr("id")?;
        for p in self.w.bind(py).call_method0("all_players")?.iter()? {
            let s = p?.getattr("seated")?;
            if s.is_truthy()? && s.get_item(0)?.eq(&id)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    // ------------------------------------------------------------------
    // 惑星
    // ------------------------------------------------------------------
    fn _planet<'py>(&mut self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        let p = self.pose(py)?.into_bound(py).get_item(0)?;
        self.w.bind(py).call_method1("nearest_planet", (p,))
    }

    fn up(&mut self, py: Python<'_>) -> R<PyObject> {
        let p = self.pose(py)?.into_bound(py).get_item(0)?;
        let pl = self._planet(py)?;
        if pl.is_none() {
            return Ok(PyArray1::from_vec_bound(py, vec![0.0, 1.0, 0.0]).into_any().unbind());
        }
        Ok(pl.call_method1("up", (p,))?.unbind())
    }

    fn altitude(&mut self, py: Python<'_>) -> R<f64> {
        let pl = self._planet(py)?;
        if pl.is_none() {
            return Ok(f64::INFINITY);
        }
        let p = self.pose(py)?.into_bound(py).get_item(0)?;
        let q = p.call_method1("__getitem__", (py.None(),))?;
        float_of(&pl.call_method1("sdf", (q,))?.get_item(0)?)
    }

    /// 重心の場所の重力加速度（すべての天体の分を足したもの）[3] m/s²
    fn gravity(&mut self, py: Python<'_>) -> R<PyObject> {
        let np = crate::game::imp(py, "numpy")?;
        let p = np.getattr("asarray")?.call1((self.pose(py)?.into_bound(py).get_item(0)?, "float64"))?.call_method1("reshape", (1, 3))?;
        let g = self.w.bind(py).call_method1("gravity", (p,))?;
        Ok(np.getattr("asarray")?.call1((g, "float64"))?.call_method1("reshape", (-1, 3))?.get_item(0)?.unbind())
    }

    fn planet<'py>(&mut self, py: Python<'py>) -> R<Option<Bound<'py, PyDict>>> {
        let pl = self._planet(py)?;
        if pl.is_none() {
            return Ok(None);
        }
        let k = float_of(&self.w.bind(py).getattr("settings")?.call_method1("get", ("gravity_scale", 1.0))?)?;
        let gm = float_of(&pl.getattr("gm")?)?;
        let rad = float_of(&pl.getattr("radius")?)?;
        let d = PyDict::new_bound(py);
        d.set_item("name", pl.getattr("name")?)?;
        d.set_item("center", pl.getattr("center")?.call_method0("tolist")?)?;
        d.set_item("radius", pl.getattr("radius")?)?;
        d.set_item("mu", gm * k)?;
        d.set_item("surface_gravity", gm * k / (rad * rad))?;
        Ok(Some(d))
    }

    fn orbit<'py>(&mut self, py: Python<'py>) -> R<Bound<'py, PyAny>> {
        let p = self.pose(py)?.into_bound(py);
        self.w.bind(py).call_method1("orbit_info", (p.get_item(0)?, p.get_item(2)?))
    }

    // ------------------------------------------------------------------
    // 機器
    // ------------------------------------------------------------------
    /// [(番号, 種類ID, 種類, 名前, 向きの名前)]
    fn devices<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyList>> {
        let out = PyList::empty_bound(py);
        let reg = self.reg(py)?;
        let ext = crate::game::imp(py, "main.extensions")?.getattr("behavior_of")?;
        for (i, d) in self.c.bind(py).getattr("devs")?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? {
                let dd = reg.call_method1("device", (d.get_item("def")?,))?;
                let dir: i64 = d.get_item("dir")?.extract()?;
                out.append((i, d.get_item("def")?, ext.call1((dd,))?, d.get_item("name")?, DIR_NAMES[dir.rem_euclid(6) as usize]))?;
            }
        }
        Ok(out)
    }

    fn devices_name<'py>(&self, py: Python<'py>, i: i64) -> R<Bound<'py, PyAny>> {
        self.dev(py, i)?.get_item("name")
    }

    /// 機器の状態のコピー（"_" で始まる内部用の項目は除く）
    fn device_state<'py>(&self, py: Python<'py>, i: i64) -> R<Bound<'py, PyDict>> {
        let out = PyDict::new_bound(py);
        for (k, v) in self.dev(py, i)?.get_item("state")?.downcast::<PyDict>()?.iter() {
            if !k.str()?.to_string().starts_with('_') {
                out.set_item(k, v)?;
            }
        }
        Ok(out)
    }

    #[pyo3(signature = (i, key, default=None))]
    fn device_get<'py>(&self, py: Python<'py>, i: i64, key: &Bound<'py, PyAny>, default: Option<PyObject>) -> R<PyObject> {
        let v = self.dev(py, i)?.get_item("state")?.call_method1("get", (key, default.unwrap_or_else(|| py.None())))?;
        if let Ok(d) = v.downcast::<PyDict>() {
            return Ok(d.copy()?.into_any().unbind());
        }
        Ok(v.unbind())
    }

    /// 決まった項目だけ書き換える
    fn device_set(&self, py: Python<'_>, i: i64, key: &str, value: &Bound<'_, PyAny>) -> R<()> {
        if !WRITABLE.contains(&key) {
            return Err(PortError::new_err(format!("'{key}' はスクリプトから書き換えられません")));
        }
        let d = self.dev(py, i)?;
        let dd = self.reg(py)?.call_method1("device", (d.get_item("def")?,))?;
        let v: PyObject = match key {
            "throttle" | "open" => float_of(value)?.max(0.0).min(1.0).into_py(py),
            _ => value.is_truthy()?.into_py(py),
        };
        if dd.is_truthy()? {
            let t = dd.get_item("type")?.str()?.to_string();
            if !crate::game::devstate::fields(&t).iter().any(|f| f.name == key) {
                return Ok(()); // その機器には無い項目（例：推進機の enabled）：何もしない
            }
        }
        let st = d.get_item("state")?;
        let lit = st.call_method1("get", ("lit",))?.is_truthy()?;
        if key == "throttle" {
            if v.extract::<f64>(py)? > 0.0 && !lit {
                st.set_item("ignite", true)?;
            }
            return Ok(());
        }
        if key == "ignite" {
            if v.extract::<bool>(py)? && !lit {
                st.set_item("ignite", true)?;
            }
            return Ok(());
        }
        st.set_item(key, v)?;
        Ok(())
    }

    /// MOD の機器（type custom）の state["data"]（そのまま。MOD が書き換えてよい）
    fn custom_data<'py>(&self, py: Python<'py>, i: i64) -> R<Bound<'py, PyAny>> {
        self.dev(py, i)?.get_item("state")?.call_method1("setdefault", ("data", PyDict::new_bound(py)))
    }

    #[getter]
    fn world(&self, py: Python<'_>) -> PyObject {
        self.w.clone_ref(py)
    }

    #[getter]
    fn construct(&self, py: Python<'_>) -> PyObject {
        self.c.clone_ref(py)
    }

    fn device_cell<'py>(&self, py: Python<'py>, i: i64) -> R<Bound<'py, PyAny>> {
        self.dev(py, i)?;
        let cell = self.c.bind(py).getattr("cells")?.get_item(i)?;
        Ok(pyo3::types::PyTuple::new_bound(py, cell.iter()?.collect::<R<Vec<_>>>()?).into_any())
    }

    fn device_dir(&self, py: Python<'_>, i: i64) -> R<i64> {
        let d = self.dev(py, i)?;
        crate::game::imp(py, "builtins")?.getattr("int")?.call1((d.get_item("dir")?,))?.extract()
    }

    /// 構築物の重心から見た、ローカル座標での機器の位置 [x,y,z] (m)
    fn device_offset(&mut self, py: Python<'_>, i: i64) -> R<Vec<f64>> {
        self.dev(py, i)?;
        let pose = self.pose(py)?.into_bound(py);
        let com: Vec<f64> = pose.get_item(0)?.call_method0("tolist")?.extract()?;
        let r: PyReadonlyArray2<f64> = pose.get_item(1)?.extract()?;
        let ra = r.as_array();
        let p: Vec<f64> = self.c.bind(py).call_method1("voxel_center", (i,))?.call_method0("tolist")?.extract()?;
        let d = [p[0] - com[0], p[1] - com[1], p[2] - com[2]];
        Ok((0..3).map(|j| (0..3).map(|k| ra[[k, j]] * d[k]).sum()).collect())
    }

    /// 表面の文字（payload=None で消す）
    fn set_display(&self, py: Python<'_>, i: i64, payload: &Bound<'_, PyAny>) -> R<()> {
        let st = self.dev(py, i)?.get_item("state")?;
        if payload.is_none() {
            st.call_method1("pop", ("display", py.None()))?;
        } else {
            st.set_item("display", payload)?;
        }
        Ok(())
    }

    /// コンピューター自身の状態（スクリプトの memory・無線の受信位置など）。コンピューターの番号だけ
    fn comp_state<'py>(&self, py: Python<'py>, i: i64) -> R<Bound<'py, PyAny>> {
        let d = self.dev(py, i)?;
        let dd = self.reg(py)?.call_method1("device", (d.get_item("def")?,))?;
        if !dd.is_truthy()? || !dd.get_item("type")?.eq("computer")? {
            return Err(PortError::new_err("コンピューターではありません"));
        }
        d.get_item("state")
    }

    // --- アクチュエーター ---------------------------------------------
    fn act_can(&self, py: Python<'_>, i: i64, action: &str) -> R<bool> {
        let d = self.dev(py, i)?;
        let reg = self.reg(py)?;
        let dd = reg.call_method1("device", (d.get_item("def")?,))?;
        Ok(dd.is_truthy()? && dd.get_item("type")?.eq("actuator")? && act::actions_for(act::motion(&d, Some(&reg))?).contains(&action))
    }

    fn act_motion(&self, py: Python<'_>, i: i64) -> R<&'static str> {
        let d = self.act_dev(py, i)?;
        act::motion(&d, Some(&self.reg(py)?))
    }

    #[pyo3(signature = (i, angle, speed=None, absolute=false))]
    fn act_rotate(&self, py: Python<'_>, i: i64, angle: &Bound<'_, PyAny>, speed: Option<&Bound<'_, PyAny>>, absolute: bool) -> R<()> {
        self.act_dev(py, i)?;
        self.can(py, i, "rotate")?;
        act::act_cmd_rotate(self.c.bind(py), i as usize, angle, speed, absolute)
    }

    #[pyo3(signature = (i, amount, speed=None, absolute=false))]
    fn act_extend(&self, py: Python<'_>, i: i64, amount: &Bound<'_, PyAny>, speed: Option<&Bound<'_, PyAny>>, absolute: bool) -> R<()> {
        self.act_dev(py, i)?;
        self.can(py, i, "extend")?;
        act::act_cmd_extend(self.c.bind(py), i as usize, Some(amount), speed, absolute)
    }

    #[pyo3(signature = (i, amount, speed=None))]
    fn act_retract(&self, py: Python<'_>, i: i64, amount: &Bound<'_, PyAny>, speed: Option<&Bound<'_, PyAny>>) -> R<()> {
        self.act_dev(py, i)?;
        self.can(py, i, "retract")?;
        let neg = (-float_of(amount)?).into_py(py).into_bound(py);
        act::act_cmd_extend(self.c.bind(py), i as usize, Some(&neg), speed, false)
    }

    fn act_detach<'py>(&self, py: Python<'py>, i: i64) -> R<Bound<'py, PyAny>> {
        self.act_dev(py, i)?;
        self.w.bind(py).call_method1("actuator_detach", (&self.c, i))
    }

    fn act_attach<'py>(&self, py: Python<'py>, i: i64) -> R<Bound<'py, PyAny>> {
        self.act_dev(py, i)?;
        crate::game::imp(py, "main.actuator")?.getattr("cmd_attach")?.call1((&self.w, &self.c, i))
    }

    fn act_execute(&self, py: Python<'_>, i: i64) -> R<String> {
        self.act_dev(py, i)?;
        act::act_execute_preset(self.w.bind(py), self.c.bind(py), i as usize)
    }

    fn act_attached(&self, py: Python<'_>, i: i64) -> R<bool> {
        self.act_dev(py, i)?;
        act::is_attached(self.c.bind(py), i as usize)
    }

    fn act_output_angle(&self, py: Python<'_>, i: i64) -> R<f64> {
        self.act_dev(py, i)?;
        act::act_output_angle(self.c.bind(py), i as usize, Some(self.w.bind(py)))
    }

    fn act_busy(&self, py: Python<'_>, i: i64) -> R<bool> {
        let d = self.act_dev(py, i)?;
        act::act_busy(&d.get_item("state")?)
    }

    // ------------------------------------------------------------------
    // 無線
    // ------------------------------------------------------------------
    /// (届く距離, 位置) か None
    fn antenna(&self, py: Python<'_>) -> R<PyObject> {
        crate::game::radio::radio_antenna_info(py, self.c.bind(py))
    }

    fn radio_send<'py>(&self, py: Python<'py>, channel: &Bound<'py, PyAny>, data: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        self.w.bind(py).getattr("radio")?.call_method1("send", (&self.c, channel, data))
    }

    fn radio_receive<'py>(&self, py: Python<'py>, comp_i: i64, channel: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let st = self.comp_state(py, comp_i)?;
        self.w.bind(py).getattr("radio")?.call_method1("receive", (&self.c, st, channel))
    }

    #[pyo3(signature = (channel, default=None, max_age=1.0))]
    fn radio_latest<'py>(&self, py: Python<'py>, channel: &Bound<'py, PyAny>, default: Option<PyObject>, max_age: f64) -> R<Bound<'py, PyAny>> {
        self.w.bind(py).getattr("radio")?.call_method1("latest", (&self.c, channel, default.unwrap_or_else(|| py.None()), max_age))
    }

    // ------------------------------------------------------------------
    fn message(&self, py: Python<'_>, text: &Bound<'_, PyAny>) -> R<()> {
        self.w.bind(py).call_method1("message", (text,))?;
        Ok(())
    }
}
