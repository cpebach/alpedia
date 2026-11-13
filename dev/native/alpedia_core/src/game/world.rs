//! ワールド（main/world.py の World）：読み込み・保存・シミュレーションの1ティック・編集操作。
//! 決まった属性は Rust の欄、それ以外（registry・settings・player など）は __dict__（MOD・画面からも同じ名前で読める）。

use pyo3::exceptions::{PyAttributeError, PyFileNotFoundError};
use pyo3::prelude::*;
use pyo3::types::{PyCFunction, PyDict, PyFrozenSet, PyList, PySet, PyString, PyTuple};

use super::construct::{a1, a3, arr, cross, dot, fv, iv, norm, opt, sub, truthy, v3, v3s, with_rw, R, V3};
use super::imp;

fn log<'py>(py: Python<'py>) -> R<Bound<'py, PyAny>> {
    imp(py, "main.log")?.call_method1("get", ("world",))
}
fn wmod(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "main.world")
}
fn json(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "json")
}
fn deep<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    imp(o.py(), "copy")?.call_method1("deepcopy", (o,))
}
fn getd<'py>(d: &Bound<'py, PyAny>, k: &str, dflt: impl IntoPy<PyObject>) -> R<Bound<'py, PyAny>> {
    d.call_method1("get", (k, dflt))
}
fn fget(d: &Bound<'_, PyAny>, k: &str, dflt: f64) -> R<f64> {
    d.call_method1("get", (k, dflt))?.extract()
}
fn phys<'py>(cfg: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    cfg.call_method1("get", ("physics", PyDict::new_bound(cfg.py())))
}
/// 配列の中身を 0 に（その場で）
fn zero_inplace(o: &Bound<'_, PyAny>) -> R<()> {
    if !with_rw(o, |s| s.fill(0.0))? {
        o.set_item(pyo3::types::PySlice::full_bound(o.py()), 0.0)?;
    }
    Ok(())
}
fn any_nonzero(o: &Bound<'_, PyAny>) -> R<bool> {
    Ok(fv(o)?.iter().any(|x| *x != 0.0))
}

#[pyfunction]
#[pyo3(signature = (main="terra", moon=true, asteroids=true))]
pub fn world_default_bodies<'py>(py: Python<'py>, main: &str, moon: bool, asteroids: bool) -> R<Bound<'py, PyList>> {
    let out = PyList::empty_bound(py);
    let d = PyDict::new_bound(py);
    d.set_item("def", main)?;
    d.set_item("position", vec![0, 0, 0])?;
    d.set_item("craters", PyList::empty_bound(py))?;
    out.append(d)?;
    let w = wmod(py)?;
    if moon {
        out.append(deep(&w.getattr("MOON")?)?)?;
    }
    if asteroids {
        for a in deep(&w.getattr("ASTEROIDS")?)?.iter()? {
            out.append(a?)?;
        }
    }
    Ok(out)
}

#[pyfunction]
pub fn world_read_json<'py>(path: &Bound<'py, PyAny>, default: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let py = path.py();
    if !path.call_method0("exists")?.is_truthy()? {
        return Ok(default.clone());
    }
    let res = path.call_method("read_text", (), Some(&[("encoding", "utf-8")].into_py_dict_bound(py)))
        .and_then(|t| json(py)?.call_method1("loads", (t,)));
    match res {
        Ok(v) => Ok(v),
        Err(e) => {
            if e.is_instance_of::<pyo3::exceptions::PyException>(py) {
                log(py)?.call_method1("error", ("%s の読み込みに失敗: %s", path.getattr("name")?, e.value_bound(py)))?;
                Ok(default.clone())
            } else {
                Err(e)
            }
        }
    }
}

#[pyfunction]
pub fn world_write_json(path: &Bound<'_, PyAny>, data: &Bound<'_, PyAny>) -> R<()> {
    let py = path.py();
    let suf: String = path.getattr("suffix")?.extract()?;
    let tmp = path.call_method1("with_suffix", (format!("{suf}.tmp"),))?;
    let kw = [("ensure_ascii", false.into_py(py)), ("indent", 1.into_py(py))].into_py_dict_bound(py);
    let text = json(py)?.call_method("dumps", (data,), Some(&kw))?;
    tmp.call_method("write_text", (text,), Some(&[("encoding", "utf-8")].into_py_dict_bound(py)))?;
    tmp.call_method1("replace", (path,))?;
    Ok(())
}

use pyo3::types::IntoPyDict;

/// 前の版の「燃料を使う発電機」（generator）を配管に置き換える（推進機が発電するようになったため）
#[pyfunction]
pub fn world_drop_fuel_generators(reg: &Bound<'_, PyAny>, cons: &Bound<'_, PyAny>) -> R<i64> {
    let py = reg.py();
    if !reg.call_method1("device", ("generator",))?.is_none() {
        return Ok(0);
    }
    let mut n = 0;
    for cd in cons.iter()? {
        for v in getd(&cd?, "voxels", PyList::empty_bound(py))?.iter()? {
            let v = v?;
            if v.len()? > 4 {
                let d = v.get_item(4)?;
                if d.is_truthy()? && getd(&d, "def", py.None())?.eq("generator")? {
                    let nd = PyDict::new_bound(py);
                    nd.set_item("def", "pipe")?;
                    nd.set_item("dir", getd(&d, "dir", 2)?.call_method0("__int__")?)?;
                    nd.set_item("name", "配管")?;
                    v.set_item(4, nd)?;
                    n += 1;
                }
            }
        }
    }
    if n > 0 {
        log(py)?.call_method1("info", ("燃料の発電機 %d 個を配管に置き換えました", n))?;
    }
    Ok(n)
}

/// ワールドが使っているのに定義が無いもの（MOD を外した・MOD の ID が変わった など）
#[pyfunction]
pub fn world_missing_references(reg: &Bound<'_, PyAny>, settings: &Bound<'_, PyAny>, cons: &Bound<'_, PyAny>, mods: &Bound<'_, PyAny>)
    -> R<Vec<String>> {
    let py = reg.py();
    let _ = mods;
    let mut out = vec![];
    let hint = "（その MOD を入れ直すか、MOD を外す前のバックアップに戻してください）";
    for pd in getd(settings, "planets", PyList::empty_bound(py))?.iter()? {
        let pd = pd?;
        let def = getd(&pd, "def", py.None())?;
        if reg.call_method1("get", ("planets", &def))?.is_none() {
            out.push(format!("惑星 '{}' の定義がありません{hint}", def.str()?));
        }
    }
    let defs = reg.getattr("defs")?;
    let (mdefs, ddefs) = (defs.get_item("materials")?, defs.get_item("devices")?);
    for cd in cons.iter()? {
        let cd = cd?;
        let mut mats: std::collections::BTreeSet<String> = Default::default();
        let mut devs: std::collections::BTreeSet<String> = Default::default();
        for v in getd(&cd, "voxels", PyList::empty_bound(py))?.iter()? {
            let v = v?;
            let n = v.len()?;
            if n > 3 && !mdefs.contains(v.get_item(3)?)? {
                mats.insert(v.get_item(3)?.str()?.to_string());
            }
            let d = if n > 4 { v.get_item(4)? } else { py.None().into_bound(py) };
            if d.is_truthy()? {
                let def = getd(&d, "def", py.None())?;
                if !ddefs.contains(&def)? {
                    devs.insert(def.str()?.to_string());
                }
            }
        }
        let name = getd(&cd, "name", getd(&cd, "id", py.None())?)?.str()?.to_string();
        if !mats.is_empty() {
            out.push(format!("構築物「{name}」の素材 {} の定義がありません{hint}", mats.into_iter().collect::<Vec<_>>().join(", ")));
        }
        if !devs.is_empty() {
            out.push(format!("構築物「{name}」の機器 {} の定義がありません{hint}", devs.into_iter().collect::<Vec<_>>().join(", ")));
        }
    }
    Ok(out)
}

/// 結合した直後：1サブステップの速さの変化を cap（m/s）までにする（全体で同じ割合に縮めるので剛体の動きは剛体のまま）
#[pyfunction]
pub fn world_limit_dv(c: &Bound<'_, PyAny>, v0: &Bound<'_, PyAny>, rb0: &Bound<'_, PyAny>, cap: f64) -> R<()> {
    let py = c.py();
    let vel_o = c.getattr("vel")?;
    if !vel_o.getattr("shape")?.eq(v0.getattr("shape")?)? {
        return Ok(());
    }
    let vel = v3s(&vel_o)?;
    let v0v = v3s(v0)?;
    let mut m2 = f64::NEG_INFINITY;
    let dv: Vec<V3> = vel.iter().zip(&v0v).map(|(a, b)| sub(*a, *b)).collect();
    for d in &dv {
        m2 = m2.max(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
    }
    let m = if dv.is_empty() { 0.0 } else { m2.sqrt() };
    if m <= cap {
        return Ok(());
    }
    let k = cap / m;
    let nv: Vec<f64> = v0v.iter().zip(&dv).flat_map(|(a, d)| [a[0] + d[0] * k, a[1] + d[1] * k, a[2] + d[2] * k]).collect();
    if !with_rw(&vel_o, |s| s.copy_from_slice(&nv))? {
        c.setattr("vel", arr(py, nv, &[dv.len(), 3])?)?;
    }
    let rb = c.getattr("rb")?;
    if rb0.is_truthy()? && !rb.is_none() && c.getattr("mode")?.eq("rigid")? {
        for (q, key) in ["v", "w"].iter().enumerate() {
            let a = v3(&rb0.get_item(q)?)?;
            let b = v3(&rb.get_item(*key)?)?;
            rb.set_item(*key, a1(py, [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]))?;
        }
    }
    Ok(())
}

/// world.as_player(p) の中身（with の間だけ world.player をその人にする）
#[pyclass(module = "alpedia_core")]
pub struct AsPlayer {
    world: PyObject,
    p: PyObject,
    prev: Option<(PyObject, PyObject)>,
}

#[pymethods]
impl AsPlayer {
    fn __enter__(&mut self, py: Python<'_>) -> R<()> {
        let w = self.world.bind(py);
        let p = self.p.bind(py);
        if p.is_none() || p.is(&w.getattr("player")?) {
            return Ok(());
        }
        self.prev = Some((w.getattr("player")?.unbind(), w.getattr("_msg_sink")?.unbind()));
        w.setattr("player", p)?;
        w.setattr("_msg_sink", p.getattr("msgs")?)?;
        Ok(())
    }
    #[pyo3(signature = (*_a))]
    fn __exit__(&mut self, py: Python<'_>, _a: &Bound<'_, PyTuple>) -> R<bool> {
        if let Some((p, s)) = self.prev.take() {
            let w = self.world.bind(py);
            w.setattr("player", p)?;
            w.setattr("_msg_sink", s)?;
        }
        Ok(false)
    }
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct World {
    #[pyo3(get, set)]
    pub name: PyObject,
    #[pyo3(get, set)]
    pub cfg: PyObject,
    #[pyo3(get, set)]
    pub dir: PyObject,
    #[pyo3(get, set)]
    pub exports_dir: PyObject,
    #[pyo3(get, set)]
    pub net: PyObject,
    #[pyo3(get, set)]
    pub remote: PyObject,
    #[pyo3(get, set, name = "_msg_sink")]
    pub msg_sink: PyObject,
    #[pyo3(get, set)]
    pub time: f64,
    #[pyo3(get, set)]
    pub constructs: PyObject,
    #[pyo3(get, set)]
    pub planets: PyObject,
    #[pyo3(get, set)]
    pub msgs: PyObject,
    #[pyo3(get, set)]
    pub input_keys: PyObject,
    #[pyo3(get, set)]
    pub time_scale: f64,
    #[pyo3(set)]
    pub runner: PyObject,
    #[pyo3(set)]
    pub radio: PyObject,
    #[pyo3(get, set, name = "_given_addons")]
    pub given_addons: PyObject,
    #[pyo3(get, set)]
    pub addons: PyObject,
    #[pyo3(get, set)]
    pub mods: PyObject,
    #[pyo3(get, set, name = "_timers")]
    pub timers: PyObject,
    #[pyo3(get, set)]
    pub substeps: i64,
    #[pyo3(get, set)]
    pub iters: i64,
}

impl World {
    fn g<'py>(slf: &Bound<'py, Self>, k: &str) -> R<Bound<'py, PyAny>> {
        slf.as_any().getattr(k)
    }
    fn set(slf: &Bound<'_, Self>, k: &str, v: impl IntoPy<PyObject>) -> R<()> {
        slf.as_any().setattr(k, v.into_py(slf.py()))
    }
    fn addons_v<'py>(slf: &Bound<'py, Self>) -> R<Vec<Bound<'py, PyAny>>> {
        Self::g(slf, "addons")?.iter()?.collect()
    }
    fn cons_v<'py>(slf: &Bound<'py, Self>) -> R<Vec<Bound<'py, PyAny>>> {
        Self::g(slf, "constructs")?.call_method0("values")?.iter()?.collect()
    }
    fn settings<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        Self::g(slf, "settings")
    }
    fn gscale(slf: &Bound<'_, Self>) -> R<f64> {
        fget(&Self::settings(slf)?, "gravity_scale", 1.0)
    }
    fn dict_get<'py>(slf: &Bound<'py, Self>, k: &str) -> R<Option<Bound<'py, PyAny>>> {
        match slf.as_any().getattr("__dict__")?.downcast::<PyDict>()?.get_item(k)? {
            Some(v) => Ok(Some(v)),
            None => Ok(None),
        }
    }
    fn is_seated_on(p: &Bound<'_, PyAny>, cid: &Bound<'_, PyAny>) -> R<bool> {
        let s = p.getattr("seated")?;
        Ok(s.is_truthy()? && s.get_item(0)?.eq(cid)?)
    }
}

#[pymethods]
impl World {
    #[new]
    #[pyo3(signature = (name, cfg, addons=None, directory=None))]
    fn new(py: Python<'_>, name: PyObject, cfg: &Bound<'_, PyAny>, addons: Option<&Bound<'_, PyAny>>, directory: Option<&Bound<'_, PyAny>>)
        -> R<Self> {
        let dir = match directory {
            Some(d) if !d.is_none() => d.clone(),
            _ => imp(py, "main.paths")?.call_method1("world_dir", (&name,))?,
        };
        let exports_dir = dir.call_method1("__truediv__", ("exports",))?;
        let (given, list) = match addons {
            Some(a) if !a.is_none() => {
                let l = PyList::new_bound(py, a.iter()?.collect::<R<Vec<_>>>()?);
                (l.clone().into_any().unbind(), PyList::new_bound(py, l.iter()).into_any().unbind())
            }
            _ => (py.None(), PyList::empty_bound(py).into_any().unbind()),
        };
        let timers = PyDict::new_bound(py);
        timers.set_item("rebond", 0.0)?;
        timers.set_item("cleanup", 0.0)?;
        let ph = phys(cfg)?;
        imp(py, "main.native")?.call_method0("require")?;
        let substeps: i64 = getd(&ph, "substeps", 3)?.call_method0("__int__")?.extract()?;
        let iters: i64 = getd(&ph, "iterations", 2)?.call_method0("__int__")?.extract()?;
        let msgs = imp(py, "collections")?.getattr("deque")?.call((), Some(&[("maxlen", 8)].into_py_dict_bound(py)))?;
        Ok(World {
            name, cfg: cfg.clone().unbind(), dir: dir.unbind(), exports_dir: exports_dir.unbind(), net: py.None(),
            remote: PyDict::new_bound(py).into_any().unbind(), msg_sink: py.None(), time: 0.0,
            constructs: PyDict::new_bound(py).into_any().unbind(), planets: PyList::empty_bound(py).into_any().unbind(),
            msgs: msgs.unbind(), input_keys: PySet::empty_bound(py)?.into_any().unbind(), time_scale: 1.0,
            runner: py.None(), radio: py.None(), given_addons: given, addons: list,
            mods: PyList::empty_bound(py).into_any().unbind(), timers: timers.into_any().unbind(), substeps, iters,
        })
    }

    /// コンピューターのプログラムを動かす係（初めて使うときに作る）
    #[getter]
    fn runner(slf: &Bound<'_, Self>) -> R<PyObject> {
        let py = slf.py();
        if slf.borrow().runner.is_none(py) {
            let r = imp(py, "main.scripting")?.getattr("ScriptRunner")?.call1((slf,))?.unbind();
            slf.borrow_mut().runner = r;
        }
        Ok(slf.borrow().runner.clone_ref(py))
    }
    /// 無線（初めて使うときに作る）
    #[getter]
    fn radio(slf: &Bound<'_, Self>) -> R<PyObject> {
        let py = slf.py();
        if slf.borrow().radio.is_none(py) {
            let r = imp(py, "main.radio")?.getattr("RadioBus")?.call1((slf,))?.unbind();
            slf.borrow_mut().radio = r;
        }
        Ok(slf.borrow().radio.clone_ref(py))
    }

    // ------------------------------------------------------------------ 読み込み・保存
    fn load<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, Self>> {
        let py = slf.py();
        let dir = Self::g(slf, "dir")?;
        let lg = log(py)?;
        if !dir.call_method0("exists")?.is_truthy()? {
            return Err(PyFileNotFoundError::new_err(format!("ワールド '{}' がありません: {}", Self::g(slf, "name")?.str()?, dir.str()?)));
        }
        let div = |k: &str| -> R<Bound<'_, PyAny>> { dir.call_method1("__truediv__", (k,)) };
        for sub_ in ["scripts", "exports", "imports"] {
            div(sub_)?.call_method("mkdir", (), Some(&[("exist_ok", true)].into_py_dict_bound(py)))?;
        }
        let s = world_read_json(&div("world.json")?, PyDict::new_bound(py).as_any())?;
        let dm = PyDict::new_bound(py);
        dm.set_item("enabled", PyList::empty_bound(py))?;
        let mods = getd(&world_read_json(&div("mods.json")?, dm.as_any())?, "enabled", PyList::empty_bound(py))?;
        let savefmt = imp(py, "main.savefmt")?;
        let r = savefmt.call_method1("upgrade_world", (&s, &mods))?;
        let (s, mut mods, upgraded) = (r.get_item(0)?, r.get_item(1)?, r.get_item(2)?.is_truthy()?);
        let given_none = slf.borrow().given_addons.is_none(py);
        let save_mods = |mods: &Bound<'_, PyAny>| -> R<()> {
            let d = PyDict::new_bound(py);
            d.set_item("enabled", PyList::new_bound(py, mods.iter()?.collect::<R<Vec<_>>>()?))?;
            match world_write_json(&div("mods.json")?, d.as_any()) {
                Err(e) if e.is_instance_of::<pyo3::exceptions::PyOSError>(py) => Ok(()),
                x => x,
            }
        };
        if upgraded {
            lg.call_method1("info", ("古い形式のワールドを読み替えました：遊び方 %s、MOD %s", getd(&s, "mode", "creative")?, &mods))?;
            if given_none {
                save_mods(&mods)?;          // 読み替えた MOD の一覧はすぐ書き戻す（world.json は保存のときに）
            }
        }
        // 依存する MOD（mod.json の requires）を足す
        let modcode = imp(py, "main.modcode")?;
        let modmeta = imp(py, "main.modmeta")?;
        let rs = modmeta.call_method1("resolve", (&mods,))?;
        mods = rs.get_item(0)?;
        let added = rs.get_item(1)?;
        let missing = rs.get_item(2)?;
        if added.is_truthy()? && given_none {
            lg.call_method1("info", ("依存する MOD を足しました: %s", &added))?;
            save_mods(&mods)?;
        }
        Self::set(slf, "mods", PyList::new_bound(py, mods.iter()?.collect::<R<Vec<_>>>()?))?;
        // MOD はここで一括で読み込む（定義・プログラムの全ファイル）。エラーがあればワールドを始めない
        let reg = imp(py, "main.registry")?.getattr("Registry")?.call0()?.call_method1("load", (&mods,))?;
        Self::set(slf, "registry", &reg)?;
        let errors = reg.getattr("errors")?;
        for m in missing.iter()? {
            let m = m?;
            let (mid, parent) = (m.get_item(0)?, m.get_item(1)?);
            errors.call_method1("append", (format!("MOD '{}' に必要な MOD '{}' が mods/ にありません",
                modmeta.call_method1("name_of", (parent,))?.str()?, mid.str()?),))?;
        }
        if given_none {
            modcode.call_method1("load", (reg.getattr("loaded_mods")?, &errors))?;
            Self::set(slf, "addons", modcode.call_method1("world_addons", (reg.getattr("loaded_mods")?, &errors))?)?;
        }
        let survival = imp(py, "main.survival")?;
        if survival.call_method1("mode_of", (&s,))?.eq("survival")? && Self::addon(slf, "survival")?.is_none() {
            let sr = survival.getattr("SurvivalRules")?.call0()?;
            Self::g(slf, "addons")?.call_method1("insert", (0, sr))?;      // 本体の遊び方（MOD ではない）
        }
        let extra = PyList::empty_bound(py);
        for a in Self::addons_v(slf)? {
            for k in a.getattr("settings_keys")?.iter()? {
                extra.append(k?)?;
            }
        }
        for msg in savefmt.call_method1("check_world", (&s, &extra))?.iter()? {
            lg.call_method1("warning", ("保存形式: %s", msg?))?;
        }
        let ds = wmod(py)?.getattr("DEFAULT_SETTINGS")?;
        let settings = PyDict::new_bound(py);
        settings.update(ds.downcast::<PyDict>()?.as_mapping())?;
        settings.update(s.downcast::<PyDict>()?.as_mapping())?;
        let cl = PyDict::new_bound(py);
        cl.update(ds.get_item("cleanup")?.downcast::<PyDict>()?.as_mapping())?;
        cl.update(getd(&s, "cleanup", PyDict::new_bound(py))?.downcast::<PyDict>()?.as_mapping())?;
        settings.set_item("cleanup", cl)?;
        Self::set(slf, "settings", &settings)?;
        slf.borrow_mut().time = fget(settings.as_any(), "time", 0.0)?;
        let cons = world_read_json(&div("constructs.json")?, PyList::empty_bound(py).as_any())?;
        let n_gen = world_drop_fuel_generators(&reg, &cons)?;
        for e in world_missing_references(&reg, settings.as_any(), &cons, &Self::g(slf, "mods")?)? {
            errors.call_method1("append", (e,))?;
        }
        if errors.is_truthy()? {
            return Err(PyErr::from_value_bound(modcode.getattr("ModLoadError")?.call1((&errors,))?));
        }
        let planets = PyList::empty_bound(py);
        let mk = imp(py, "main.planet")?.getattr("make_planet")?;
        for pd in settings.get_item("planets")?.unwrap().iter()? {
            let pd = pd?;
            let base = reg.call_method1("get", ("planets", pd.get_item("def")?))?;
            let ov = getd(&pd, "override", py.None())?;
            let p = mk.call1((base, getd(&pd, "position", vec![0, 0, 0])?, &ov))?;
            let cr = PyList::empty_bound(py);
            for c in getd(&pd, "craters", PyList::empty_bound(py))?.iter()? {
                cr.append(PyList::new_bound(py, c?.iter()?.collect::<R<Vec<_>>>()?))?;
            }
            p.setattr("craters", cr)?;
            p.setattr("defn_override", ov)?;
            planets.append(p)?;
        }
        Self::set(slf, "planets", &planets)?;
        let spawn = world_read_json(&div("spawn.json")?, PyDict::new_bound(py).as_any())?;
        for cd in cons.iter()? {
            for m in savefmt.call_method1("check_construct", (cd?,))?.iter()? {
                lg.call_method1("warning", ("保存形式: %s", m?))?;
            }
        }
        for m in savefmt.call_method1("check_spawn", (&spawn,))?.iter()? {
            lg.call_method1("warning", ("保存形式: %s", m?))?;
        }
        let ccls = imp(py, "main.construct")?.getattr("Construct")?;
        let constructs = Self::g(slf, "constructs")?;
        for cd in cons.iter()? {
            let cd = cd?;
            let r = cd.get_item("id").and_then(|id| ccls.call_method1("from_dict", (&reg, &cd, getd(&spawn, &id.str()?.to_string(), PyDict::new_bound(py))?)));
            match r {
                Ok(c) => constructs.set_item(c.getattr("id")?, c)?,
                Err(e) if e.is_instance_of::<pyo3::exceptions::PyException>(py) => {
                    let kw = PyDict::new_bound(py);
                    kw.set_item("exc_info", e.value_bound(py))?;
                    lg.call_method("exception", ("構築物 %s の読み込みに失敗: %s", getd(&cd, "id", py.None())?, e.value_bound(py)), Some(&kw))?;
                }
                Err(e) => return Err(e),
            }
        }
        let pj = world_read_json(&div("player.json")?, py.None().bind(py))?;
        // 見た目：保存されていたもの → ワールド設定の見た目（古い形式）→ 宇宙飛行士 の順に、定義があるもの
        let pjd = if pj.is_truthy()? { pj.clone() } else { PyDict::new_bound(py).into_any() };
        let cands = [getd(&pjd, "def", py.None())?, getd(settings.as_any(), "player", py.None())?, PyString::new_bound(py, "default").into_any()];
        let mut pick = PyString::new_bound(py, "default").into_any();
        for d in cands {
            if d.is_truthy()? && !reg.call_method1("get", ("players", &d))?.is_none() {
                pick = d;
                break;
            }
        }
        let player = Self::make_player(slf, &pick, None)?;
        Self::set(slf, "player", &player)?;
        if pj.is_truthy()? {
            for m in savefmt.call_method1("check_player", (&pj,))?.iter()? {
                lg.call_method1("warning", ("保存形式: %s", m?))?;
            }
            player.call_method1("load_dict", (&pj,))?;
        }
        Self::set(slf, "local_player", &player)?;
        for a in Self::addons_v(slf)? {
            a.call_method1("on_load", (slf,))?;
        }
        if settings.call_method1("pop", ("starter_sample", py.None()))?.is_truthy()? {
            Self::_place_starter_sample(slf)?;
        }
        Self::_refresh_script_readme(slf)?;
        lg.call_method1("info", ("ワールド '%s' を読み込み: 構築物 %d, 惑星 %d, MOD %s", Self::g(slf, "name")?, constructs.len()?,
            planets.len(), &mods))?;
        Self::message(slf, &format!("ワールド「{}」を読み込みました", Self::g(slf, "name")?.str()?))?;
        if added.is_truthy()? && given_none {
            let names: Vec<String> = added.iter()?.map(|m| Ok(modmeta.call_method1("name_of", (m?,))?.str()?.to_string())).collect::<R<_>>()?;
            Self::message(slf, &format!("必要な MOD を足しました: {}", names.join("、")))?;
        }
        if n_gen > 0 {
            Self::message(slf, &format!("燃料の発電機はなくなったので、{n_gen} 個を配管にしました（推進機が燃えている間に発電します）"))?;
        }
        Ok(slf.clone())
    }

    /// scripts/README.txt を今の版の説明に合わせる（古いワールドに古い説明が残らないように）
    fn _refresh_script_readme(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let text = imp(py, "main.content")?.getattr("SCRIPT_README")?;
        let p = Self::g(slf, "dir")?.call_method1("__truediv__", ("scripts",))?.call_method1("__truediv__", ("README.txt",))?;
        let enc = [("encoding", "utf-8")].into_py_dict_bound(py);
        let r: R<()> = (|| {
            if p.getattr("parent")?.call_method0("is_dir")?.is_truthy()?
                && (!p.call_method0("exists")?.is_truthy()? || !p.call_method("read_text", (), Some(&enc))?.eq(&text)?) {
                p.call_method("write_text", (&text,), Some(&enc))?;
            }
            Ok(())
        })();
        match r {
            Err(e) if e.is_instance_of::<pyo3::exceptions::PyOSError>(py) => {
                log(py)?.call_method1("warning", ("README.txt を更新できません: %s", e.value_bound(py)))?;
                Ok(())
            }
            x => x,
        }
    }

    /// 新しく始めたクリエイティブ：出現地点のそばに見本の「はじめのロケット」を置く
    fn _place_starter_sample(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let bp = imp(py, "main.paths")?.getattr("SAUCE")?.call_method1("__truediv__", ("blueprints",))?
            .call_method1("__truediv__", ("starter_rocket.alp.json",))?;
        if !bp.call_method0("exists")?.is_truthy()? {
            return Ok(());
        }
        let r: R<()> = (|| {
            let data = imp(py, "main.objio")?.call_method1("import_construct", (Self::g(slf, "registry")?, &bp))?;
            let ppos = v3(&Self::g(slf, "player")?.getattr("pos")?)?;
            let pl = Self::nearest_planet(slf, &a1(py, ppos))?;
            let up = if pl.is_none() { [0.0, 1.0, 0.0] } else { v3(&pl.call_method1("up", (a1(py, ppos),))?)? };
            let mut side = cross(up, [0.0, 0.0, 1.0]);
            if norm(side) < 1e-6 {
                side = cross(up, [1.0, 0.0, 0.0]);
            }
            let l = norm(side);
            side = [side[0] / l, side[1] / l, side[2] / l];
            let mut pos = [ppos[0] + side[0] * 4.0, ppos[1] + side[1] * 4.0, ppos[2] + side[2] * 4.0 + 3.0];
            if !pl.is_none() {
                let d: f64 = pl.call_method1("sdf", (arr(py, pos.to_vec(), &[1, 3])?,))?.get_item(0)?.extract()?;
                for k in 0..3 {
                    pos[k] = pos[k] - up[k] * d + up[k] * 0.3;
                }
            }
            let c = Self::place_import(slf, &data, Some(&a1(py, pos)), None)?;
            if !c.is_none() {
                Self::message(slf, "見本の「はじめのロケット」を置きました。座って Space で点火（F8 の設計図にもあります）")?;
            }
            Ok(())
        })();
        match r {
            Err(e) if e.is_instance_of::<pyo3::exceptions::PyException>(py) => {
                let kw = PyDict::new_bound(py);
                kw.set_item("exc_info", e.value_bound(py))?;
                log(py)?.call_method("exception", ("見本のロケットを置けません",), Some(&kw))?;
                Ok(())
            }
            x => x,
        }
    }

    /// プレイヤーの見た目の定義。無ければ（MOD を外したなど）宇宙飛行士
    fn player_def<'py>(slf: &Bound<'py, Self>, def_id: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let reg = Self::g(slf, "registry")?;
        let id: Bound<'py, PyAny> = if def_id.is_truthy()? { def_id.clone() } else { PyString::new_bound(py, "default").into_any() };
        let d = reg.call_method1("get", ("players", &id))?;
        if d.is_none() {
            log(py)?.call_method1("warning", ("プレイヤーの見た目 '%s' の定義が無いので、宇宙飛行士にします", def_id))?;
            return reg.call_method1("get", ("players", "default"));
        }
        Ok(d)
    }
    #[pyo3(signature = (def_id, pos=None))]
    fn make_player<'py>(slf: &Bound<'py, Self>, def_id: &Bound<'py, PyAny>, pos: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let p = match pos {
            Some(p) if !p.is_none() => p.clone(),
            _ => Self::_default_spawn(slf)?,
        };
        imp(py, "main.player")?.getattr("Player")?.call1((Self::player_def(slf, def_id)?, p))
    }
    /// このワールドにいるプレイヤー全員（自分＋参加者）
    fn all_players<'py>(slf: &Bound<'py, Self>) -> R<Vec<Bound<'py, PyAny>>> {
        let mut out = vec![Self::g(slf, "local_player")?];
        for p in Self::g(slf, "remote")?.call_method0("values")?.iter()? {
            out.push(p?);
        }
        Ok(out)
    }
    /// 参加者の操作をホストで行う間、world.player をその人にする（with で使う）
    fn as_player(slf: &Bound<'_, Self>, p: PyObject) -> AsPlayer {
        AsPlayer { world: slf.clone().into_any().unbind(), p, prev: None }
    }
    /// いちばん近いプレイヤーまでの距離
    fn player_distance(slf: &Bound<'_, Self>, x: &Bound<'_, PyAny>) -> R<f64> {
        let x = v3(x)?;
        let mut best = f64::INFINITY;
        for p in Self::all_players(slf)? {
            best = best.min(norm(sub(x, v3(&p.getattr("pos")?)?)));
        }
        Ok(best)
    }
    /// 構築物 c の操縦席に座っている人が押しているキー
    fn keys_for<'py>(slf: &Bound<'py, Self>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PySet>> {
        let out = PySet::empty_bound(slf.py())?;
        let lp = Self::g(slf, "local_player")?;
        let cid = c.getattr("id")?;
        for p in Self::all_players(slf)? {
            if Self::is_seated_on(&p, &cid)? {
                let k = if p.is(&lp) { Self::g(slf, "input_keys")? } else { p.getattr("input_keys")? };
                for x in k.iter()? {
                    out.add(x?)?;
                }
            }
        }
        Ok(out)
    }
    /// リスポーン：自宅など（MOD が決める）か最初の出現地点へ戻る
    #[pyo3(signature = (player=None))]
    fn respawn(slf: &Bound<'_, Self>, player: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        let p = match player {
            Some(p) if p.is_truthy()? => p.clone(),
            _ => Self::g(slf, "player")?,
        };
        let mut pos: Option<Bound<'_, PyAny>> = None;
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("respawn_point", (slf, &p))?;
            if !r.is_none() {
                pos = Some(a1(py, v3(&r)?));
                break;
            }
        }
        let pos = match pos { Some(p) => p, None => Self::_default_spawn(slf)? };
        p.setattr("seated", py.None())?;
        p.setattr("jetpack", false)?;
        p.setattr("pos", a1(py, v3(&pos)?))?;
        p.setattr("vel", a1(py, [0.0; 3]))?;
        let pl = Self::nearest_planet(slf, &p.getattr("pos")?)?;
        if !pl.is_none() {
            p.setattr("up", pl.call_method1("up", (p.getattr("pos")?,))?)?;
        }
        let mut msgs = vec![];
        for a in Self::addons_v(slf)? {
            let m = a.call_method1("on_respawn", (slf, &p))?;
            if m.is_truthy()? {
                msgs.push(m.str()?.to_string());
            }
        }
        Self::message(slf, &format!("リスポーンしました{}", if msgs.is_empty() { String::new() } else { format!("：{}", msgs.join(" ")) }))
    }
    fn _default_spawn<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let pls = Self::g(slf, "planets")?;
        if pls.len()? == 0 {
            return Ok(a1(py, [0.0; 3]));
        }
        let p = pls.get_item(0)?;
        let r: f64 = p.call_method1("surface_radius", (arr(py, vec![0.0, 1.0, 0.0], &[1, 3])?,))?.get_item(0)?.extract()?;
        let c = v3(&p.getattr("center")?)?;
        Ok(a1(py, [c[0], c[1] + (r + 2.0), c[2]]))
    }

    /// 保存する中身（ファイル名 → JSON）。保存と、マルチプレイの参加者へ送るのに使う
    fn snapshot<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let st = Self::settings(slf)?;
        st.set_item("time", slf.borrow().time)?;
        st.set_item("format", imp(py, "main.savefmt")?.getattr("FORMAT")?)?;
        for a in Self::addons_v(slf)? {
            a.call_method1("on_save", (slf,))?;
        }
        let pls = PyList::empty_bound(py);
        for pl in Self::g(slf, "planets")?.iter()? {
            let pl = pl?;
            let d = PyDict::new_bound(py);
            d.update(pl.call_method0("to_dict")?.downcast::<PyDict>()?.as_mapping())?;
            if let Some(o) = opt(&pl, "defn_override")? {
                if o.is_truthy()? {
                    d.set_item("override", o)?;
                }
            }
            pls.append(d)?;
        }
        st.set_item("planets", pls)?;
        let out = PyDict::new_bound(py);
        out.set_item("world.json", &st)?;
        let cl = PyList::empty_bound(py);
        let sp = PyDict::new_bound(py);
        for c in Self::cons_v(slf)? {
            cl.append(c.call_method0("to_dict")?)?;
        }
        for c in Self::cons_v(slf)? {
            sp.set_item(c.getattr("id")?, c.call_method0("spawn_dict")?)?;
        }
        out.set_item("constructs.json", cl)?;
        out.set_item("spawn.json", sp)?;
        out.set_item("player.json", Self::g(slf, "local_player")?.call_method0("to_dict")?)?;
        let md = PyDict::new_bound(py);
        md.set_item("enabled", PyList::new_bound(py, Self::g(slf, "mods")?.iter()?.collect::<R<Vec<_>>>()?))?;
        out.set_item("mods.json", md)?;
        Ok(out)
    }
    fn save(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let net = Self::g(slf, "net")?;
        if !net.is_none() && !net.call_method0("can_save")?.is_truthy()? {
            return Self::message(slf, "保存はホストが行います（ワールドはホストの PC にあります）");
        }
        let snap = Self::snapshot(slf)?;
        if !net.is_none() {
            snap.update(net.call_method0("save_extra")?.downcast::<PyDict>()?.as_mapping())?;      // 参加した人の状態（players.json）
        }
        let dir = Self::g(slf, "dir")?;
        for (f, d) in snap.iter() {
            world_write_json(&dir.call_method1("__truediv__", (f,))?, &d)?;
        }
        log(py)?.call_method1("info", ("保存しました: %s", &dir))?;
        Self::message(slf, "保存しました")
    }

    // ------------------------------------------------------------------ 共通
    fn message(slf: &Bound<'_, Self>, text: &str) -> R<()> {
        let py = slf.py();
        let sink = Self::g(slf, "_msg_sink")?;
        let to = if sink.is_none() { Self::g(slf, "msgs")? } else { sink };
        let t: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
        to.call_method1("append", ((t, text),))?;
        Ok(())
    }
    fn gravity<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let np = super::construct::np(py)?;
        let pls: Vec<Bound<'py, PyAny>> = Self::g(slf, "planets")?.iter()?.collect::<R<_>>()?;
        if pls.is_empty() {
            return np.call_method1("zeros_like", (np.call_method1("asarray", (p, "float64"))?,));
        }
        let ok = match opt(slf.as_any(), "_grav_key")? { Some(k) => k.eq(pls.len())?, None => false };
        if !ok {
            let mut c = vec![];
            let mut gm = vec![];
            let mut r = vec![];
            for x in &pls {
                c.extend(v3(&x.getattr("center")?)?);
                gm.push(x.getattr("gm")?.extract::<f64>()?);
                r.push(x.getattr("radius")?.extract::<f64>()?);
            }
            let n = pls.len();
            Self::set(slf, "_grav", (arr(py, c, &[n, 3])?, arr(py, gm, &[n])?, arr(py, r, &[n])?))?;
            Self::set(slf, "_grav_key", n)?;
        }
        let gr = Self::g(slf, "_grav")?;
        let pa = np.call_method1("ascontiguousarray", (p, "float64"))?.call_method1("reshape", (-1, 3))?;
        let g = imp(py, "alpedia_core")?.call_method1("gravity", (pa, gr.get_item(0)?, gr.get_item(1)?, gr.get_item(2)?))?;
        g.call_method1("reshape", (np.call_method1("shape", (p,))?,))?.call_method1("__mul__", (Self::gscale(slf)?,))
    }
    fn nearest_planet<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let p = v3(p)?;
        let mut best: Bound<'py, PyAny> = py.None().into_bound(py);
        let mut bd = 1e300;
        for pl in Self::g(slf, "planets")?.iter()? {
            let pl = pl?;
            let d = (norm(sub(p, v3(&pl.getattr("center")?)?)) - pl.getattr("radius")?.extract::<f64>()?)
                / pl.getattr("gm")?.extract::<f64>()?.max(1e-9).powf(0.5);
            if d < bd {
                best = pl;
                bd = d;
            }
        }
        Ok(best)
    }
    /// 最寄り惑星に対する軌道要素（二体問題の近似）
    fn orbit_info<'py>(slf: &Bound<'py, Self>, pos: &Bound<'py, PyAny>, vel: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let pl = Self::nearest_planet(slf, pos)?;
        if pl.is_none() {
            return Ok(pl);
        }
        let mu = pl.getattr("gm")?.extract::<f64>()? * Self::gscale(slf)?;
        let rad: f64 = pl.getattr("radius")?.extract()?;
        let r = sub(v3(pos)?, v3(&pl.getattr("center")?)?);
        let v = v3(vel)?;
        let rn = norm(r);
        let up = [r[0] / rn, r[1] / rn, r[2] / rn];
        let vr = dot(v, up);
        let vh = norm(sub(v, [up[0] * vr, up[1] * vr, up[2] * vr]));
        let out = PyDict::new_bound(py);
        let inf = f64::INFINITY;
        out.set_item("planet", pl.getattr("name")?)?;
        out.set_item("radius", pl.getattr("radius")?)?;
        out.set_item("altitude", rn - rad)?;
        out.set_item("vertical_speed", vr)?;
        out.set_item("horizontal_speed", vh)?;
        out.set_item("orbital_speed", if mu > 0.0 { (mu / rn).sqrt() } else { 0.0 })?;
        out.set_item("apoapsis", inf)?;
        out.set_item("periapsis", rn - rad)?;
        out.set_item("eccentricity", 1.0)?;
        out.set_item("period", inf)?;
        out.set_item("time_to_apoapsis", inf)?;
        if mu <= 0.0 {
            return Ok(out.into_any());
        }
        let energy = dot(v, v) / 2.0 - mu / rn;
        let h = norm(cross(r, v));
        let e = (1.0 + 2.0 * energy * h * h / (mu * mu)).max(0.0).sqrt();
        out.set_item("eccentricity", e)?;
        if energy < 0.0 {
            let a = -mu / (2.0 * energy);
            out.set_item("apoapsis", a * (1.0 + e) - rad)?;
            out.set_item("periapsis", a * (1.0 - e) - rad)?;
            out.set_item("period", 2.0 * std::f64::consts::PI * (a.powf(3.0) / mu).sqrt())?;
            if e > 1e-6 {
                // 真近点角から遠点までの時間
                let cos_e = ((1.0 - rn / a) / e).clamp(-1.0, 1.0);
                let mut ea = cos_e.acos();
                if vr < 0.0 {
                    ea = 2.0 * std::f64::consts::PI - ea;
                }
                let m = ea - e * ea.sin();
                let n = (mu / a.powf(3.0)).sqrt();
                out.set_item("time_to_apoapsis", (std::f64::consts::PI - m).rem_euclid(2.0 * std::f64::consts::PI) / n)?;
            }
        } else {
            out.set_item("periapsis", (h * h / mu) / (1.0 + e) - rad)?;
        }
        Ok(out.into_any())
    }

    // --- アクチュエーター用 ---
    fn actuator_detach(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, i: usize) -> R<bool> {
        if !super::actuator::act_cmd_detach(c, i)? {
            return Ok(false);
        }
        for part in c.call_method0("split")?.iter()? {
            let part = part?;
            Self::add_construct(slf, &part)?;
            part.call_method0("wake")?;
        }
        c.call_method0("wake")?;
        Ok(true)
    }
    /// other を c に取り込む（座っているプレイヤーの参照も付け替える）。片方だけがアンカーで固定されているときは
    /// 固定されていない側を、固定されている側の格子の位置に合わせる
    #[pyo3(signature = (c, other, cell_map, xform=None))]
    fn merge_constructs(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, other: &Bound<'_, PyAny>, cell_map: &Bound<'_, PyAny>,
                        xform: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        let n0: i64 = c.getattr("n_vox")?.extract()?;
        let ca = c.getattr("anchored")?.is_truthy()?;
        let oa = other.getattr("anchored")?.is_truthy()?;
        c.call_method1("absorb", (other, cell_map, xform.map(|x| x.clone().unbind()).unwrap_or_else(|| py.None())))?;
        if ca != oa {
            let n1: i64 = c.getattr("n_vox")?.extract()?;
            let k: Vec<i64> = if ca { (0..n0).collect() } else { (n0..n1).collect() };
            let nk = k.len();
            c.call_method1("snap_to_lattice", (super::construct::iarr(py, k, &[nk])?,))?;
        }
        // 結合した直後は、めり込みの押し出しで吹き飛ばないように、しばらく速さの変化をおさえる
        let cur: f64 = match opt(c, "_settle")? { Some(v) => v.extract()?, None => 0.0 };
        let t = fget(&phys(&Self::g(slf, "cfg")?)?, "merge_settle_time", 1.0)?;
        c.setattr("_settle", cur.max(t))?;
        let oid = other.getattr("id")?;
        for p in Self::all_players(slf)? {
            if Self::is_seated_on(&p, &oid)? {
                let s = p.getattr("seated")?;
                p.setattr("seated", (c.getattr("id")?, cell_map.get_item(s.get_item(1)?)?))?;
            }
        }
        Self::g(slf, "constructs")?.call_method1("pop", (&oid, py.None()))?;
        log(py)?.call_method1("info", ("構築物を結合: %s ← %s", c.getattr("name")?, other.getattr("name")?))?;
        Ok(())
    }
    fn seated_in(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>) -> R<bool> {
        let cid = c.getattr("id")?;
        for p in Self::all_players(slf)? {
            let s = p.getattr("seated")?;
            if !s.is_none() && s.get_item(0)?.eq(&cid)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn add_construct<'py>(slf: &Bound<'py, Self>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        Self::g(slf, "constructs")?.set_item(c.getattr("id")?, c)?;
        Ok(c.clone())
    }

    // ------------------------------------------------------------------ シミュレーション
    #[pyo3(signature = (dt, r#move=None, jump=None, thrust_dir=None))]
    fn step(slf: &Bound<'_, Self>, dt: f64, r#move: Option<&Bound<'_, PyAny>>, jump: Option<&Bound<'_, PyAny>>,
            thrust_dir: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        let z = || a1(py, [0.0; 3]);
        let mv = r#move.map(|x| x.clone()).unwrap_or_else(z);
        let jp = jump.map(|x| x.clone()).unwrap_or_else(|| false.into_py(py).into_bound(py));
        let td = thrust_dir.map(|x| x.clone()).unwrap_or_else(z);
        slf.borrow_mut().time += dt;
        let time = slf.borrow().time;
        let ambient = 0.0;
        let seats = PyDict::new_bound(py);
        let lp = Self::g(slf, "local_player")?;
        for p in Self::all_players(slf)? {
            let s = p.getattr("seated")?;
            if s.is_truthy()? {
                let k = if p.is(&lp) { Self::g(slf, "input_keys")? } else { p.getattr("input_keys")? };
                seats.call_method1("setdefault", (s.get_item(0)?, PySet::empty_bound(py)?))?.call_method1("update", (k,))?;
            }
        }
        // 構築物の外側の球（問い合わせで使い回す）。全部の作り直しは 0.25 秒ごと
        let bt: f64 = match Self::dict_get(slf, "_bnd_t")? { Some(v) => v.extract()?, None => -1.0 };
        if time - bt >= 0.25 {
            Self::set(slf, "_bnd_t", time)?;
            Self::set(slf, "_bnd", py.None())?;
        }
        // 機能処理の距離より遠い物は、機器・プログラム・物理を止める（遠いかどうかは 0.25 秒ごとに見直す）
        let constructs = Self::g(slf, "constructs")?;
        let ft: f64 = match opt(slf.as_any(), "_far_t")? { Some(v) => v.extract()?, None => -1.0 };
        let fnn: i64 = match opt(slf.as_any(), "_far_n")? { Some(v) => v.extract()?, None => -1 };
        if time - ft >= 0.25 || fnn != constructs.len()? as i64 {
            Self::set(slf, "_far_t", time)?;
            Self::set(slf, "_far_n", constructs.len()?)?;
            let f = Self::far_ids(slf)?;
            Self::set(slf, "_far", f)?;
        }
        let far = Self::g(slf, "_far")?;
        let runner = Self::runner(slf)?.into_bound(py);
        for c in Self::cons_v(slf)? {
            let cid = c.getattr("id")?;
            if far.contains(&cid)? {
                continue;
            }
            let dirty = truthy(&c, "dirty")?;
            if (!c.getattr("has_devices")?.is_truthy()? || c.call_method0("is_quiet")?.is_truthy()?) && !dirty {
                // 機器の無い物・固定されていて毎ティック処理する機器の無い物は機器・プログラムの処理を飛ばす
                zero_inplace(&c.getattr("force")?)?;
                continue;
            }
            let mut cdt = dt;
            let in_seat = seats.contains(&cid)?;
            if !dirty && !in_seat && c.call_method("is_quiet", (), Some(&[("slow_ok", true)].into_py_dict_bound(py)))?.is_truthy()? {
                // 明かりだけの固定された建物：0.25 秒ごとにまとめて（点いているか・電気の消費）
                zero_inplace(&c.getattr("force")?)?;
                let sd: f64 = match opt(&c, "_slow_dt")? { Some(v) => v.extract()?, None => 0.0 } + dt;
                c.setattr("_slow_dt", sd)?;
                if sd < 0.25 {
                    continue;
                }
                cdt = sd;
                c.setattr("_slow_dt", 0.0)?;
            }
            let keys = PyFrozenSet::new_bound(py, &match seats.get_item(&cid)? {
                Some(s) => s.iter()?.collect::<R<Vec<_>>>()?,
                None => vec![],
            })?;
            super::devices::dev_update(py, &c, cdt, Some(keys.as_any()), in_seat, ambient)?;
            if let Some(n) = opt(&c, "notes")? {
                if n.is_truthy()? {
                    for m in n.iter()? {
                        Self::message(slf, &m?.str()?.to_string())?;
                    }
                    c.setattr("notes", PyList::empty_bound(py))?;
                }
            }
            runner.call_method1("tick", (&c, cdt))?;
        }
        Self::g(slf, "player")?.call_method1("update", (slf, dt, &mv, &jp, &td))?;
        for p in Self::g(slf, "remote")?.call_method0("values")?.iter()? {
            // 参加者の体は参加者の PC が動かす。ジェットパックの燃料と体重だけここで
            let p = p?;
            if p.getattr("flying")?.is_truthy()? && p.getattr("jetpack")?.is_truthy()? {
                let mut ap = Self::as_player(slf, p.clone().unbind());
                ap.__enter__(py)?;
                let r = Self::jetpack(slf, &p, dt, &p.getattr("thrust")?);
                ap.__exit__(py, &PyTuple::empty_bound(py))?;
                r?;
            }
            let st = p.getattr("stand")?;
            if st.is_truthy()? {
                let c = constructs.call_method1("get", (st.get_item(0)?,))?;
                if c.is_truthy()? {
                    let key = PyTuple::new_bound(py, st.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?);
                    let i = c.getattr("index")?.call_method1("get", (key,))?;
                    if !i.is_none() {
                        let i: usize = i.extract()?;
                        let pp = v3(&p.getattr("pos")?)?;
                        let gn = norm(v3(&Self::gravity(slf, &arr(py, pp.to_vec(), &[1, 3])?)?.get_item(0)?)?);
                        let up = v3(&p.getattr("up")?)?;
                        let m: f64 = p.getattr("mass")?.extract()?;
                        let add = [-up[0] * m * gn / 8.0, -up[1] * m * gn / 8.0, -up[2] * m * gn / 8.0];
                        let f = c.getattr("force")?;
                        if !with_rw(&f, |s| {
                            for q in 8 * i..8 * i + 8 {
                                for k in 0..3 {
                                    s[3 * q + k] += add[k];
                                }
                            }
                        })? {
                            let mut fv_ = fv(&f)?;
                            for q in 8 * i..8 * i + 8 {
                                for k in 0..3 {
                                    fv_[3 * q + k] += add[k];
                                }
                            }
                            let n = fv_.len() / 3;
                            c.setattr("force", arr(py, fv_, &[n, 3])?)?;
                        }
                    }
                }
            }
        }
        let substeps = slf.borrow().substeps;
        let iters = slf.borrow().iters;
        let h = dt / substeps as f64;
        // 拡張（軌道上の街など）が位置を決める構築物は、物理計算しない
        let anchored_ids = PySet::empty_bound(py)?;
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("before_physics", (slf, dt))?;
            if r.is_truthy()? {
                anchored_ids.call_method1("__ior__", (py.get_type_bound::<PySet>().call1((r,))?,))?;
            }
        }
        let mut moved: Vec<Bound<'_, PyAny>> = vec![];
        for k in anchored_ids.iter() {
            if constructs.contains(&k)? {
                moved.push(constructs.get_item(&k)?);
            }
        }
        // 機能処理の距離より遠い物は止めておく（当たり判定の相手にはなる）。レールに乗っている物は軌道だけ進める
        for cid in far.iter()? {
            let cid = cid?;
            if !constructs.get_item(&cid)?.getattr("mode")?.eq("rails")? {
                anchored_ids.add(cid)?;
            }
        }
        let mut cons: Vec<Bound<'_, PyAny>> = vec![];
        let mut sleeping: Vec<Bound<'_, PyAny>> = vec![];
        let gravity_fn = slf.getattr("gravity")?;
        for c in Self::cons_v(slf)? {
            if c.getattr("n_vox")?.extract::<usize>()? == 0 {
                continue;
            }
            let cid = c.getattr("id")?;
            if anchored_ids.contains(&cid)? {
                sleeping.push(c);
                continue;
            }
            let mode: String = c.getattr("mode")?.extract()?;
            if mode == "rigid" && c.getattr("anchored")?.is_truthy()? && !c.call_method0("_actuator_busy")?.is_truthy()? {
                // アンカーで固定されて止まっている物（建物など）：物理計算しない（ほかの物には当たる）
                c.setattr("_touch_moving", false)?;
                anchored_ids.add(&cid)?;
                let v = c.getattr("vel")?;
                if any_nonzero(&v)? {
                    zero_inplace(&v)?;
                }
                sleeping.push(c);
                continue;
            }
            if c.getattr("asleep")?.is_truthy()? && any_nonzero(&c.getattr("force")?)? {
                c.call_method0("wake")?;
            }
            c.setattr("_touch_moving", false)?;
            c.call_method1("update_mode", (dt, slf))?;            // 軽量化：剛体化・レール（遠方）の切り替え
            if c.getattr("is_static")?.is_truthy()? {             // アンカーで固定（アクチュエーターが止まっている間）
                anchored_ids.add(&cid)?;
                zero_inplace(&c.getattr("vel")?)?;
                sleeping.push(c);
            } else if c.getattr("asleep")?.is_truthy()? {
                sleeping.push(c);
            } else if c.getattr("mode")?.eq("rails")? {
                c.call_method1("rails_step", (dt, &gravity_fn))?;    // レールに乗っている構築物は重心の軌道だけを進める
                moved.push(c);
            } else {
                cons.push(c);
            }
        }
        for c in &cons {
            Self::_prep_ext(slf, c, dt)?;
        }
        sleeping = Self::_sleeping_near(slf, cons.clone(), sleeping, dt)?;
        let broken = PySet::empty_bound(py)?;
        let mut fixed_pos: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)> = vec![];
        for c in &sleeping {
            if c.getattr("is_static")?.is_truthy()? {        // アンカーで固定：押されても戻す
                fixed_pos.push((c.clone(), c.getattr("pos")?.call_method0("copy")?));
            }
        }
        let settle_acc = fget(&phys(&Self::g(slf, "cfg")?)?, "merge_settle_acc", 30.0)?;
        let mut done = 0i64;
        let fp = PyList::new_bound(py, fixed_pos.iter().map(|(a, b)| PyTuple::new_bound(py, [a, b])));
        let res = Self::_native_tick(slf, cons.clone(), sleeping.clone(), h, &anchored_ids, settle_acc, &fp)?;
        if !res.is_none() {                    // Rust でまとめて進めた（途中で止まったら、残りはここで今までどおり）
            done = res.get_item(0)?.extract()?;
            let woke: Vec<Bound<'_, PyAny>> = res.get_item(1)?.iter()?.collect::<R<_>>()?;
            broken.call_method1("__ior__", (res.get_item(2)?,))?;
            if !woke.is_empty() {
                Self::wake_in(slf, &woke, dt)?;
                cons.extend(woke);
                sleeping = Self::still_sleeping(sleeping, &anchored_ids)?;
            }
        }
        let gs = Self::gscale(slf)?;
        for _ in 0..(substeps - done).max(0) {
            let mut settling = vec![];
            for c in &cons {
                let st: f64 = match opt(c, "_settle")? { Some(v) => v.extract()?, None => 0.0 };
                if st > 0.0 && c.getattr("vel")?.getattr("shape")?.eq(c.getattr("pos")?.getattr("shape")?)? {
                    let rb = c.getattr("rb")?;
                    let rb0: PyObject = if rb.is_truthy()? {
                        (rb.get_item("v")?.call_method0("copy")?, rb.get_item("w")?.call_method0("copy")?).into_py(py)
                    } else { rb.unbind() };
                    settling.push((c.clone(), c.getattr("vel")?.call_method0("copy")?, rb0));
                }
            }
            let pls = Self::g(slf, "planets")?;
            let begin = PyCFunction::new_closure_bound(py, None, None, {
                let gf = gravity_fn.clone().unbind();
                let pls = pls.clone().unbind();
                move |a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>| -> R<PyObject> {
                    let py = a.py();
                    a.get_item(0)?.call_method1("substep_begin", (h, iters, gf.bind(py), pls.bind(py), gs))?;
                    Ok(py.None())
                }
            })?;
            Self::_each(slf, cons.clone(), begin.as_any())?;
            let woke = Self::_voxel_collisions(slf, cons.clone(), sleeping.clone())?;
            let mut wk = vec![];
            for c in woke {
                if !anchored_ids.contains(c.getattr("id")?)? {
                    wk.push(c);
                }
            }
            for (c, p0) in &fixed_pos {
                let p = c.getattr("pos")?;
                let src = fv(p0)?;
                if !with_rw(&p, |s| s.copy_from_slice(&src))? {
                    p.set_item(pyo3::types::PySlice::full_bound(py), p0)?;
                }
            }
            let end = PyCFunction::new_closure_bound(py, None, None, move |a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>| -> R<PyObject> {
                a.get_item(0)?.call_method1("substep_end", (h,)).map(|x| x.unbind())
            })?;
            let ends = Self::_each(slf, cons.clone(), end.as_any())?;
            for (c, v0, rb0) in &settling {
                world_limit_dv(c, v0, rb0.bind(py), settle_acc * h)?;
            }
            for (c, b) in cons.iter().zip(ends.iter()) {
                if b.bind(py).is_truthy()? {
                    broken.add(c.getattr("id")?)?;
                }
            }
            if !wk.is_empty() {
                Self::wake_in(slf, &wk, dt)?;
                cons.extend(wk);
                sleeping = Self::still_sleeping(sleeping, &anchored_ids)?;
            }
        }
        for c in &cons {
            c.call_method1("update_sleep", (dt,))?;
            let st: f64 = match opt(c, "_settle")? { Some(v) => v.extract()?, None => 0.0 };
            if st > 0.0 {
                c.setattr("_settle", (st - dt).max(0.0))?;
            }
        }
        let mut cm = cons.clone();
        cm.extend(moved);
        Self::_patch_bounds(slf, cm, &broken)?;
        let cl = PyList::new_bound(py, &cons);
        for a in Self::addons_v(slf)? {
            a.call_method1("after_physics", (slf, &cl))?;
        }
        for c in &cons {
            c.setattr("ext_events", PyList::empty_bound(py))?;
        }
        for cid in broken.iter() {
            let c = constructs.call_method1("get", (&cid,))?;
            if c.is_none() {
                continue;
            }
            let parts = c.call_method0("split")?;
            for part in parts.iter()? {
                Self::add_construct(slf, &part?)?;
            }
            if parts.len()? > 0 {
                log(py)?.call_method1("info", ("構築物 %s が %d 個に分離", c.getattr("name")?, parts.len()? + 1))?;
            }
        }
        let mut empty = vec![];
        for (k, c) in constructs.downcast::<PyDict>()?.iter() {
            if c.getattr("n_vox")?.extract::<usize>()? == 0 {
                empty.push(k);
            }
        }
        for k in empty {
            constructs.del_item(k)?;
        }
        let timers = Self::g(slf, "_timers")?;
        let rb_t: f64 = timers.get_item("rebond")?.extract::<f64>()? + dt;
        timers.set_item("rebond", rb_t)?;
        if rb_t > 0.25 {
            timers.set_item("rebond", 0.0)?;
            for c in Self::cons_v(slf)? {
                c.call_method0("try_rebond")?;
            }
        }
        let cu_t: f64 = timers.get_item("cleanup")?.extract::<f64>()? + dt;
        timers.set_item("cleanup", cu_t)?;
        let cl = Self::settings(slf)?.get_item("cleanup")?;
        if cu_t >= fget(&cl, "check_interval", 5.0)? {
            Self::_cleanup(slf, cu_t)?;
            timers.set_item("cleanup", 0.0)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------ 拡張（hooks.Addon）
    /// クリエイティブ（資源を使わない）か。資源のルールを持つ MOD（サバイバルなど）が入っていれば False
    #[getter]
    fn creative(slf: &Bound<'_, Self>) -> R<bool> {
        for a in Self::addons_v(slf)? {
            if a.getattr("resource_rules")?.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// 名前で拡張を探す（無ければ None）
    fn addon<'py>(slf: &Bound<'py, Self>, name: &str) -> R<Bound<'py, PyAny>> {
        for a in Self::addons_v(slf)? {
            if a.getattr("name")?.eq(name)? {
                return Ok(a);
            }
        }
        Ok(slf.py().None().into_bound(slf.py()))
    }
    /// 拡張の状態を変える操作（街での売り買いなど）。参加者なら自分の PC でもすぐに行い、ホストへも送る
    #[pyo3(signature = (name, path, *args))]
    fn addon_call<'py>(slf: &Bound<'py, Self>, name: &str, path: &str, args: &Bound<'py, PyTuple>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let a = Self::addon(slf, name)?;
        if a.is_none() {
            return Ok(a);
        }
        let mut f = a.clone();
        for part in path.split('.') {
            f = f.getattr(part)?;
        }
        let res = f.call1(args.clone())?;
        let net = Self::g(slf, "net")?;
        if !net.is_none() && match opt(&net, "role")? { Some(r) => r.eq("client")?, None => false } {
            let enc = PyList::empty_bound(py);
            for x in args.iter() {
                let r = a.call_method1("net_ref_of", (&x,))?;
                if r.is_none() {
                    enc.append(x)?;
                } else {
                    let d = PyDict::new_bound(py);
                    d.set_item("__ref__", PyList::new_bound(py, r.iter()?.collect::<R<Vec<_>>>()?))?;
                    enc.append(d)?;
                }
            }
            let m = PyDict::new_bound(py);
            m.set_item("t", "call")?;
            m.set_item("addon", name)?;
            m.set_item("path", path)?;
            m.set_item("args", enc)?;
            net.call_method1("send", (m,))?;
        }
        Ok(res)
    }
    /// ホスト：参加者からの addon_call を行う（world.player はその人にしてから呼ぶ）
    fn run_addon_call<'py>(slf: &Bound<'py, Self>, name: &str, path: &str, args: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let a = Self::addon(slf, name)?;
        if a.is_none() {
            return Ok(a);
        }
        let mut f = a.clone();
        for part in path.split('.') {
            if part.starts_with('_') {
                return Ok(py.None().into_bound(py));
            }
            f = f.getattr(part)?;
        }
        let real = PyList::empty_bound(py);
        for x in args.iter()? {
            let x = x?;
            if x.is_instance_of::<PyDict>() && x.contains("__ref__")? {
                let r = PyTuple::new_bound(py, std::iter::once(slf.clone().into_any()).chain(x.get_item("__ref__")?.iter()?.collect::<R<Vec<_>>>()?)
                    .collect::<Vec<_>>());
                real.append(a.getattr("net_ref")?.call1(r)?)?;
            } else {
                real.append(x)?;
            }
        }
        f.call1(PyTuple::new_bound(py, real))
    }
    /// このティックで当たりそうな、拡張の当たり判定の箱を集めておく
    fn _prep_ext(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, dt: f64) -> R<()> {
        let py = slf.py();
        c.setattr("ext_events", PyList::empty_bound(py))?;
        c.setattr("ext_colliders", PyList::empty_bound(py))?;
        let addons = Self::addons_v(slf)?;
        if addons.is_empty() {
            return Ok(());
        }
        let vel = fv(&c.getattr("vel")?)?;
        let vmax = vel.iter().map(|x| x.abs()).fold(f64::NEG_INFINITY, |a, b| if b.is_nan() || a.is_nan() { f64::NAN } else { a.max(b) });
        let sp = vmax * dt + c.getattr("size")?.extract::<f64>()? * 2.0;
        let pos = v3s(&c.getattr("pos")?)?;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let (lo, hi) = (a1(py, lo), a1(py, hi));
        let ec = c.getattr("ext_colliders")?;
        for a in addons {
            ec.call_method1("extend", (a.call_method1("colliders", (slf, &lo, &hi, sp))?,))?;
        }
        Ok(())
    }
    /// 拡張の物体（街の建物など）から球を押し出す：[(押し出し, 法線, owner)]
    fn push_sphere<'py>(slf: &Bound<'py, Self>, center: &Bound<'py, PyAny>, r: f64) -> R<Bound<'py, PyList>> {
        let out = PyList::empty_bound(slf.py());
        for a in Self::addons_v(slf)? {
            for x in a.call_method1("push_sphere", (slf, center, r))?.iter()? {
                out.append(x?)?;
            }
        }
        Ok(out)
    }
    /// 構築物ごとの計算を、複数の CPU コアで同時に進める（Rust の計算は GIL を離すので並列に動く）
    fn _each(slf: &Bound<'_, Self>, cons: Vec<Bound<'_, PyAny>>, f: &Bound<'_, PyAny>) -> R<Vec<PyObject>> {
        let py = slf.py();
        let workers = Self::_workers(slf)?;
        if workers <= 1 || cons.len() < 2 {
            return cons.iter().map(|c| f.call1((c,)).map(|x| x.unbind())).collect();
        }
        let pmin: usize = imp(py, "main.fastphys")?.getattr("PARALLEL_MIN_VOXELS")?.extract()?;
        let mut out: Vec<Option<PyObject>> = (0..cons.len()).map(|_| None).collect();
        let mut small = vec![];
        for (k, c) in cons.iter().enumerate() {
            // 剛体・関節つき剛体は Python 側の部分が多く、スレッドに分けても速くならない
            if c.getattr("n_vox")?.extract::<usize>()? >= pmin || !c.getattr("mode")?.eq("soft")? {
                out[k] = Some(f.call1((c,))?.unbind());
            } else {
                small.push(k);
            }
        }
        if small.len() == 1 {
            out[small[0]] = Some(f.call1((&cons[small[0]],))?.unbind());
        } else if !small.is_empty() {
            let pool = Self::_pool(slf)?;
            let fo = f.clone().unbind();
            let run = PyCFunction::new_closure_bound(py, None, None, move |a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>| -> R<PyObject> {
                let py = a.py();
                let c = a.get_item(0)?;
                c.setattr("_in_pool", true)?;
                let r = fo.bind(py).call1((&c,));
                c.setattr("_in_pool", false)?;
                r.map(|x| x.unbind())
            })?;
            let items = PyList::new_bound(py, small.iter().map(|&k| cons[k].clone()));
            let res = pool.call_method1("map", (run, items))?;
            for (k, r) in small.iter().zip(res.iter()?) {
                out[*k] = Some(r?.unbind());
            }
        }
        Ok(out.into_iter().map(|x| x.unwrap_or_else(|| py.None())).collect())
    }
    fn _workers(slf: &Bound<'_, Self>) -> R<i64> {
        let py = slf.py();
        let t = getd(&phys(&Self::g(slf, "cfg")?)?, "threads", 0)?;
        let n: i64 = if t.is_truthy()? { t.call_method0("__int__")?.extract()? } else { 0 };
        if n > 0 {
            return Ok(n);
        }
        let c = imp(py, "os")?.call_method0("cpu_count")?;
        let c: i64 = if c.is_truthy()? { c.extract()? } else { 2 };
        Ok((c - 1).max(1))
    }
    fn _pool<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        if let Some(p) = opt(slf.as_any(), "_pool_obj")? {
            return Ok(p);
        }
        let kw = PyDict::new_bound(py);
        kw.set_item("max_workers", Self::_workers(slf)?)?;
        kw.set_item("thread_name_prefix", "phys")?;
        let p = imp(py, "concurrent.futures")?.getattr("ThreadPoolExecutor")?.call((), Some(&kw))?;
        Self::set(slf, "_pool_obj", &p)?;
        Ok(p)
    }
    /// 構築物どうし（と、同じ構築物の結合していないボクセルどうし）の押し出し。押し出された眠っている構築物のリストを返す
    #[pyo3(signature = (active, sleeping=Vec::new()))]
    fn _voxel_collisions<'py>(slf: &Bound<'py, Self>, active: Vec<Bound<'py, PyAny>>, sleeping: Vec<Bound<'py, PyAny>>) -> R<Vec<Bound<'py, PyAny>>> {
        if active.is_empty() {
            return Ok(vec![]);
        }
        let core = imp(slf.py(), "alpedia_core")?;
        Self::_voxel_collisions_native(slf, &core, active, sleeping)
    }
    /// 1ティックの物理（サブステップの繰り返し）を Rust でまとめて進める。(進めたサブステップの数, 起きた物, ちぎれた構築物の ID)。
    /// まとめて進められないとき（動いている街に触れうる物がある）は None
    fn _native_tick<'py>(slf: &Bound<'py, Self>, cons: Vec<Bound<'py, PyAny>>, sleeping: Vec<Bound<'py, PyAny>>, h: f64,
                         anchored_ids: &Bound<'py, PyAny>, settle_acc: f64, fixed_pos: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let none = || py.None().into_bound(py);
        if cons.is_empty() {
            return Ok(none());
        }
        let core = imp(py, "alpedia_core")?;
        let pls = super::construct::construct_native_planets(py, &Self::g(slf, "planets")?)?.into_bound(py);
        let gidx = PyDict::new_bound(py);
        let glst = PyList::empty_bound(py);
        let groups = PyTuple::new_bound(py, [gidx.as_any(), glst.as_any()]);
        let jobs = PyList::empty_bound(py);
        for c in &cons {
            let j = c.call_method1("tick_job", (&groups, true))?;
            if j.is_none() {
                return Ok(none());
            }
            let st: f64 = match opt(c, "_settle")? { Some(v) => v.extract()?, None => 0.0 };
            let ok = st > 0.0 && c.getattr("vel")?.getattr("shape")?.eq(c.getattr("pos")?.getattr("shape")?)?;
            j.set_item("settle_cap", if ok { settle_acc * h } else { 0.0 })?;
            jobs.append(j)?;
        }
        let fixed: Vec<usize> = fixed_pos.iter()?.map(|x| Ok(x?.get_item(0)?.as_ptr() as usize)).collect::<R<_>>()?;
        let sl = PyList::empty_bound(py);
        for c in &sleeping {
            c.call_method1("_contig", ("pos", "vel"))?;
            sl.append((c.call_method0("native_proxy")?, c.getattr("pos")?, c.getattr("vel")?, c.getattr("mode")?.eq("rigid")?,
                fixed.contains(&(c.as_ptr() as usize)), !anchored_ids.contains(c.getattr("id")?)?))?;
        }
        let imp_s: f64 = imp(py, "main.construct")?.getattr("Construct")?.getattr("IMPACT_SPEED")?.extract()?;
        let args: Vec<PyObject> = vec![jobs.into_any().unbind(), sl.into_any().unbind(), glst.clone().into_any().unbind(), pls.unbind(),
            Self::gscale(slf)?.into_py(py), h.into_py(py), slf.borrow().substeps.into_py(py), slf.borrow().iters.into_py(py),
            imp_s.into_py(py), super::construct::CONTACT_SOFT.into_py(py)];
        let r = core.call_method1("step_tick", PyTuple::new_bound(py, args))?;
        let (done, woke, touch, want, broken, hits) = (r.get_item(0)?, r.get_item(1)?, r.get_item(2)?, r.get_item(3)?, r.get_item(4)?, r.get_item(5)?);
        let allc: Vec<&Bound<'_, PyAny>> = cons.iter().chain(sleeping.iter()).collect();
        for ((c, t), w) in allc.iter().zip(touch.iter()?).zip(want.iter()?) {
            if t?.is_truthy()? {
                c.setattr("_touch_moving", true)?;
            }
            if w?.is_truthy()? {
                c.setattr("_want_soft", true)?;
            }
        }
        let broke = PySet::empty_bound(py)?;
        for b in broken.iter()? {
            let b = b?;
            let k: usize = b.get_item(0)?.extract()?;
            cons[k].call_method1("tick_broken", (b.get_item(1)?, b.get_item(2)?))?;
            broke.add(cons[k].getattr("id")?)?;
        }
        if hits.is_truthy()? {
            let owners = PyDict::new_bound(py);
            for c in &cons {
                for e in c.getattr("ext_colliders")?.iter()? {
                    let e = e?;
                    owners.set_item(e.get_item(1)?.as_ptr() as usize, e.get_item(0)?)?;
                }
            }
            let gowner = PyDict::new_bound(py);
            for (i, k) in gidx.iter() {
                gowner.set_item(k, owners.get_item(i)?)?;
            }
            // (k, g, stp) ごとに {部分: 速さ}
            let mut ev: Vec<((i64, i64, i64), Bound<'_, PyDict>)> = vec![];
            for x in hits.iter()? {
                let x = x?;
                let key: (i64, i64, i64) = (x.get_item(0)?.extract()?, x.get_item(1)?.extract()?, x.get_item(4)?.extract()?);
                let d = match ev.iter().find(|e| e.0 == key) {
                    Some(e) => e.1.clone(),
                    None => {
                        let d = PyDict::new_bound(py);
                        ev.push((key, d.clone()));
                        d
                    }
                };
                d.set_item(x.get_item(2)?.call_method0("__int__")?, x.get_item(3)?.extract::<f64>()?)?;
            }
            ev.sort_by_key(|e| e.0 .2);
            for ((k, g, _), best) in ev {
                cons[k as usize].getattr("ext_events")?.call_method1("append", ((gowner.get_item(g)?, best, py.None(), py.None()),))?;
            }
        }
        for c in &cons {
            if c.getattr("mode")?.eq("art")? {
                let a = c.getattr("art")?;
                if !a.is_none() {
                    a.set_item("defo", PyList::empty_bound(py))?;
                    a.set_item("contacts", py.None())?;
                }
            }
        }
        let wl = PyList::empty_bound(py);
        for q in woke.iter()? {
            wl.append(&sleeping[q?.extract::<usize>()?])?;
        }
        Ok(PyTuple::new_bound(py, [done, wl.into_any(), broke.into_any()]).into_any())
    }
    /// _voxel_collisions の Rust 版（同じ計算を1回の呼び出しで）
    fn _voxel_collisions_native<'py>(slf: &Bound<'py, Self>, core: &Bound<'py, PyAny>, active: Vec<Bound<'py, PyAny>>,
                                     sleeping: Vec<Bound<'py, PyAny>>) -> R<Vec<Bound<'py, PyAny>>> {
        let py = slf.py();
        let na = active.len();
        let cons: Vec<Bound<'py, PyAny>> = active.into_iter().chain(sleeping).collect();
        let items = PyList::empty_bound(py);
        for c in &cons {
            c.call_method1("_contig", ("pos", "vel"))?;
            items.append((c.call_method0("native_proxy")?, c.getattr("pos")?, c.getattr("vel")?, c.getattr("mode")?.eq("rigid")?))?;
        }
        let r = core.call_method1("contacts", (items, na))?;
        for ((c, t), w) in cons.iter().zip(r.get_item(1)?.iter()?).zip(r.get_item(2)?.iter()?) {
            if t?.is_truthy()? {
                c.setattr("_touch_moving", true)?;
            }
            if w?.is_truthy()? {
                c.setattr("_want_soft", true)?;
            }
        }
        r.get_item(0)?.iter()?.map(|k| Ok(cons[k?.extract::<usize>()?].clone())).collect()
    }
    fn _cleanup(slf: &Bound<'_, Self>, elapsed: f64) -> R<()> {
        let py = slf.py();
        let cl = Self::settings(slf)?.get_item("cleanup")?;
        if !getd(&cl, "enabled", true)?.is_truthy()? {
            return Ok(());
        }
        let constructs = Self::g(slf, "constructs")?;
        let items: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)> = constructs.downcast::<PyDict>()?.iter().collect();
        let dist: f64 = cl.get_item("distance")?.extract()?;
        let maxv: i64 = cl.get_item("max_vertices")?.call_method0("__int__")?.extract()?;
        let secs: f64 = cl.get_item("seconds")?.extract()?;
        for (cid, c) in items {
            if Self::seated_in(slf, &c)? {
                c.setattr("far_time", 0.0)?;
                continue;
            }
            let far = Self::player_distance(slf, &c.call_method0("center_of_mass")?)? > dist;
            if far && !c.getattr("powered")?.is_truthy()? && c.getattr("n_vert")?.extract::<i64>()? <= maxv {
                let ft: f64 = c.getattr("far_time")?.extract::<f64>()? + elapsed;
                c.setattr("far_time", ft)?;
                if ft >= secs {
                    constructs.del_item(&cid)?;
                    log(py)?.call_method1("info", ("自動削除: %s（頂点 %d, 距離 %.0f m）", c.getattr("name")?, c.getattr("n_vert")?,
                        Self::player_distance(slf, &c.call_method0("center_of_mass")?)?))?;
                }
            } else {
                c.setattr("far_time", 0.0)?;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------ 問い合わせ
    /// すべての構築物の外側の球（このティックの間は使い回す）：(構築物のリスト, 中心 (n,3), 半径 (n,))
    fn _bounds<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let n = Self::g(slf, "constructs")?.len()?;
        let b = match opt(slf.as_any(), "_bnd")? {
            Some(b) if b.get_item(0)?.eq(n)? => b,
            _ => {
                let cs = PyList::empty_bound(py);
                let (mut c, mut r0, mut rr) = (vec![], vec![], vec![]);
                for x in Self::cons_v(slf)? {
                    if x.getattr("n_vox")?.extract::<usize>()? == 0 {
                        continue;
                    }
                    let bb = x.call_method0("bounds")?;
                    c.extend(v3(&bb.get_item(0)?)?);
                    let r: f64 = bb.get_item(1)?.extract()?;
                    r0.push(r);
                    rr.push(r + x.getattr("size")?.extract::<f64>()?);
                    cs.append(x)?;
                }
                let k = cs.len();
                let b = PyTuple::new_bound(py, [n.into_py(py), cs.into_any().unbind(), arr(py, c, &[k, 3])?.unbind(), arr(py, rr, &[k])?.unbind(),
                    arr(py, r0, &[k])?.unbind()]);
                Self::set(slf, "_bnd", &b)?;
                b.into_any()
            }
        };
        Ok(PyTuple::new_bound(py, [b.get_item(1)?, b.get_item(2)?, b.get_item(3)?]))
    }
    /// 物理のあと：動いた物の外側の球だけ直す（全部を作り直さない）。形が変わった・ちぎれた物があれば作り直す
    fn _patch_bounds(slf: &Bound<'_, Self>, moved: Vec<Bound<'_, PyAny>>, broken: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let b = match Self::dict_get(slf, "_bnd")? {
            Some(b) if !b.is_none() => b,
            _ => {
                Self::set(slf, "_bnd", py.None())?;
                return Ok(());
            }
        };
        if broken.is_truthy()? || !b.get_item(0)?.eq(Self::g(slf, "constructs")?.len()?)? {
            Self::set(slf, "_bnd", py.None())?;
            return Ok(());
        }
        let at = Self::bnd_at(slf, &b.get_item(1)?)?;
        let (c_o, rr_o, r0_o) = (b.get_item(2)?, b.get_item(3)?, b.get_item(4)?);
        for c in moved {
            let k = at.call_method1("get", (c.as_ptr() as usize,))?;
            if k.is_none() || c.getattr("n_vox")?.extract::<usize>()? == 0 {
                Self::set(slf, "_bnd", py.None())?;
                return Ok(());
            }
            let k: usize = k.extract()?;
            let bb = c.call_method0("bounds")?;
            let ctr = v3(&bb.get_item(0)?)?;
            let r: f64 = bb.get_item(1)?.extract()?;
            let sz: f64 = c.getattr("size")?.extract()?;
            with_rw(&c_o, |s| s[3 * k..3 * k + 3].copy_from_slice(&ctr))?;
            with_rw(&r0_o, |s| s[k] = r)?;
            with_rw(&rr_o, |s| s[k] = r + sz)?;
        }
        Ok(())
    }
    /// (構築物のリスト, 中心 (n,3), 半径＋大きさ (n,), 半径 (n,))（描画の距離の計算用）
    fn _bounds_arrays<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        Self::_bounds(slf)?;
        let b = Self::g(slf, "_bnd")?;
        Ok(PyTuple::new_bound(slf.py(), [b.get_item(1)?, b.get_item(2)?, b.get_item(3)?, b.get_item(4)?]))
    }
    /// 機能処理の距離（m）。0 なら全部（距離に関係なく処理する）
    fn process_distance(slf: &Bound<'_, Self>) -> R<f64> {
        let v = getd(&phys(&Self::g(slf, "cfg")?)?, "process_distance", 0)?;
        Ok(if v.is_truthy()? { v.extract()? } else { 0.0 })
    }
    /// どのプレイヤーからも機能処理の距離より遠い構築物の ID（座っている物・最近いた物は近い扱い）
    fn far_ids<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PySet>> {
        let py = slf.py();
        let lim = Self::process_distance(slf)?;
        let out = PySet::empty_bound(py)?;
        if lim <= 0.0 || Self::g(slf, "constructs")?.len()? == 0 {
            return Ok(out);
        }
        let b = Self::_bounds(slf)?;
        let cs: Vec<Bound<'_, PyAny>> = b.get_item(0)?.iter()?.collect::<R<_>>()?;
        if cs.is_empty() {
            return Ok(out);
        }
        let c = v3s(&b.get_item(1)?)?;
        let rr = fv(&b.get_item(2)?)?;
        let players = Self::all_players(slf)?;
        let pp: Vec<V3> = players.iter().map(|p| v3(&p.getattr("pos")?)).collect::<R<_>>()?;
        let seated = PySet::empty_bound(py)?;
        for p in &players {
            let s = p.getattr("seated")?;
            if s.is_truthy()? {
                seated.add(s.get_item(0)?)?;
            }
        }
        let prev = match opt(slf.as_any(), "_far_prev")? { Some(p) => p, None => PySet::empty_bound(py)?.into_any() };
        for (k, x) in cs.iter().enumerate() {
            let id = x.getattr("id")?;
            if seated.contains(&id)? {
                continue;
            }
            let d = pp.iter().map(|p| norm(sub(c[k], *p))).fold(f64::INFINITY, f64::min) - rr[k];
            // 境目で行ったり来たりしないように、止まっていた物は少し近づくまで止めたまま
            if d > lim || (prev.contains(&id)? && d > lim * 0.9) {
                out.add(id)?;
            }
        }
        Self::set(slf, "_far_prev", &out)?;
        Ok(out)
    }
    /// 押し出しの相手にする止まっている構築物：動いている物のどれかの外側の球にかかる物だけ
    fn _sleeping_near<'py>(slf: &Bound<'py, Self>, cons: Vec<Bound<'py, PyAny>>, sleeping: Vec<Bound<'py, PyAny>>, dt: f64)
        -> R<Vec<Bound<'py, PyAny>>> {
        if cons.is_empty() || sleeping.is_empty() {
            return Ok(if cons.is_empty() { vec![] } else { sleeping });
        }
        let mut ac = vec![];
        let mut ar = vec![];
        for c in &cons {
            let b = c.call_method0("bounds")?;
            ac.push(v3(&b.get_item(0)?)?);
            let vmax = fv(&c.getattr("vel")?)?.iter().map(|x| x.abs()).fold(f64::NEG_INFINITY, f64::max);
            ar.push(b.get_item(1)?.extract::<f64>()? + vmax * dt * 2.0 + c.getattr("size")?.extract::<f64>()? * 2.0);
        }
        let b = Self::_bounds(slf)?;
        let at = Self::bnd_at(slf, &b.get_item(0)?)?;
        let mut ix = vec![];
        let mut all = true;
        for c in &sleeping {
            let k = at.call_method1("get", (c.as_ptr() as usize,))?;
            if k.is_none() {
                all = false;
                break;
            }
            ix.push(k.extract::<usize>()?);
        }
        let (bc, br): (Vec<V3>, Vec<f64>) = if !all {
            let mut bc = vec![];
            let mut br = vec![];
            for c in &sleeping {
                let b = c.call_method0("bounds")?;
                bc.push(v3(&b.get_item(0)?)?);
                br.push(b.get_item(1)?.extract::<f64>()?);
            }
            (bc, br)
        } else {
            let c = v3s(&b.get_item(1)?)?;
            let r0 = fv(&Self::g(slf, "_bnd")?.get_item(4)?)?;
            (ix.iter().map(|&k| c[k]).collect(), ix.iter().map(|&k| r0[k]).collect())
        };
        let mut out = vec![];
        for (q, c) in sleeping.into_iter().enumerate() {
            let hit = (0..ac.len()).any(|a| {
                let d = sub(bc[q], ac[a]);
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() < ar[a] + br[q]
            });
            if hit {
                out.push(c);
            }
        }
        Ok(out)
    }
    /// 点 p から r 以内に外側の球がかかる構築物
    fn constructs_near<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>, r: f64) -> R<Vec<Bound<'py, PyAny>>> {
        let b = Self::_bounds(slf)?;
        let cs: Vec<Bound<'py, PyAny>> = b.get_item(0)?.iter()?.collect::<R<_>>()?;
        if cs.is_empty() {
            return Ok(cs);
        }
        let c = v3s(&b.get_item(1)?)?;
        let rr = fv(&b.get_item(2)?)?;
        let p = v3(p)?;
        Ok(cs.into_iter().enumerate().filter(|(k, _)| norm(sub(c[*k], p)) < rr[*k] + r).map(|(_, x)| x).collect())
    }
    /// 線分 o → o + dvec × length の近くに外側の球がかかる構築物
    fn constructs_on_ray<'py>(slf: &Bound<'py, Self>, o: &Bound<'py, PyAny>, dvec: &Bound<'py, PyAny>, length: f64) -> R<Vec<Bound<'py, PyAny>>> {
        let b = Self::_bounds(slf)?;
        let cs: Vec<Bound<'py, PyAny>> = b.get_item(0)?.iter()?.collect::<R<_>>()?;
        if cs.is_empty() {
            return Ok(cs);
        }
        let c = v3s(&b.get_item(1)?)?;
        let rr = fv(&b.get_item(2)?)?;
        let (o, d) = (v3(o)?, v3(dvec)?);
        Ok(cs.into_iter().enumerate().filter(|(k, _)| {
            let rel = sub(c[*k], o);
            let t = dot(rel, d).clamp(0.0, length);
            norm(sub(rel, [t * d[0], t * d[1], t * d[2]])) < rr[*k]
        }).map(|(_, x)| x).collect())
    }
    /// 球（プレイヤーなど）と構築物の形（球・カプセル・斜面）の接触: [(construct, idx, push, normal)]
    fn voxel_contacts<'py>(slf: &Bound<'py, Self>, center: &Bound<'py, PyAny>, radius: f64) -> R<Bound<'py, PyList>> {
        let py = slf.py();
        let shape = imp(py, "main.shape")?;
        let out = PyList::empty_bound(py);
        let mut ctr = v3(center)?;
        for c in Self::constructs_near(slf, &a1(py, ctr), radius + 1.0)? {
            if c.getattr("n_vox")?.extract::<usize>()? == 0 {
                continue;
            }
            for _ in 0..2 {
                let res = shape.call_method1("closest_on_shape", (&c, a1(py, ctr), radius))?;
                if res.is_none() {
                    break;
                }
                let dist: f64 = res.get_item(0)?.extract()?;
                if dist >= radius {
                    break;
                }
                let q = v3(&res.get_item(1)?)?;
                let vi: usize = res.get_item(2)?.extract()?;
                let v = sub(ctr, q);
                let l = norm(v);
                let n = if l > 1e-9 { [v[0] / l, v[1] / l, v[2] / l] } else {
                    let vc = v3(&c.call_method0("voxel_centers")?.get_item(vi)?)?;
                    let d = sub(ctr, vc);
                    let ln = norm(d).max(1e-9);
                    [d[0] / ln, d[1] / ln, d[2] / ln]
                };
                let pen = radius - dist;
                let push = [n[0] * pen, n[1] * pen, n[2] * pen];
                out.append((&c, vi, a1(py, push), a1(py, n)))?;
                ctr = [ctr[0] + push[0], ctr[1] + push[1], ctr[2] + push[2]];
            }
        }
        Ok(out)
    }
    /// 視線の先のボクセル／地形。戻り値 dict または None
    #[pyo3(signature = (origin, direction, max_dist=8.0))]
    fn raycast<'py>(slf: &Bound<'py, Self>, origin: &Bound<'py, PyAny>, direction: &Bound<'py, PyAny>, max_dist: f64) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let o = v3(origin)?;
        let d0 = v3(direction)?;
        let l = norm(d0);
        let d = [d0[0] / l, d0[1] / l, d0[2] / l];
        let core = imp(py, "alpedia_core")?;
        let (oa, da) = (a1(py, o), a1(py, d));
        let mut best: Option<Bound<'py, PyDict>> = None;
        let bt = |b: &Option<Bound<'py, PyDict>>| -> R<Option<f64>> {
            match b { Some(b) => Ok(Some(b.get_item("t")?.unwrap().extract()?)), None => Ok(None) }
        };
        for c in Self::constructs_on_ray(slf, &oa, &da, max_dist)? {
            if c.getattr("n_vox")?.extract::<usize>()? == 0 {
                continue;
            }
            let np = super::construct::np(py)?;
            let hit = core.call_method1("voxel_raycast", (np.call_method1("ascontiguousarray", (c.call_method0("centers_cached")?, "float64"))?,
                np.call_method1("ascontiguousarray", (c.call_method0("frames_cached")?, "float64"))?, o.to_vec(), d.to_vec(),
                c.getattr("size")?.extract::<f64>()?, max_dist))?;
            if !hit.is_none() {
                let t: f64 = hit.get_item(0)?.extract()?;
                if bt(&best)?.map(|b| t < b).unwrap_or(true) {
                    let r = PyDict::new_bound(py);
                    r.set_item("kind", "voxel")?;
                    r.set_item("t", t)?;
                    r.set_item("construct", &c)?;
                    r.set_item("index", hit.get_item(1)?.call_method0("__int__")?)?;
                    r.set_item("face", hit.get_item(2)?.call_method0("__int__")?)?;
                    r.set_item("point", a1(py, [o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t]))?;
                    best = Some(r);
                }
            }
        }
        for pl in Self::g(slf, "planets")?.iter()? {
            let pl = pl?;
            if norm(sub(o, v3(&pl.getattr("center")?)?)) > pl.getattr("bound")?.extract::<f64>()? + max_dist {
                continue;
            }
            let r = pl.call_method1("raycast", (&oa, &da, max_dist))?;
            if r.is_truthy()? {
                let t: f64 = r.get_item(0)?.extract()?;
                if bt(&best)?.map(|b| t < b).unwrap_or(true) {
                    let x = PyDict::new_bound(py);
                    x.set_item("kind", "terrain")?;
                    x.set_item("t", r.get_item(0)?)?;
                    x.set_item("planet", &pl)?;
                    x.set_item("point", r.get_item(1)?)?;
                    let p1 = v3(&r.get_item(1)?)?;
                    x.set_item("normal", pl.call_method1("normal", (arr(py, p1.to_vec(), &[1, 3])?,))?.get_item(0)?)?;
                    best = Some(x);
                }
            }
            // 水面（海・池）：地面や構築物より手前にあれば水を指す
            if pl.getattr("has_water")?.is_truthy()? {
                let lim = bt(&best)?.unwrap_or(max_dist);
                let w = pl.call_method1("water_raycast", (&oa, &da, lim))?;
                if w.is_truthy()? {
                    let t: f64 = w.get_item(0)?.extract()?;
                    if bt(&best)?.map(|b| t < b).unwrap_or(true) {
                        let x = PyDict::new_bound(py);
                        x.set_item("kind", "water")?;
                        x.set_item("t", w.get_item(0)?)?;
                        x.set_item("planet", &pl)?;
                        x.set_item("point", w.get_item(1)?)?;
                        x.set_item("normal", pl.call_method1("up", (w.get_item(1)?,))?)?;
                        best = Some(x);
                    }
                }
            }
        }
        let mut res: Bound<'py, PyAny> = match &best { Some(b) => b.clone().into_any(), None => py.None().into_bound(py) };
        for a in Self::addons_v(slf)? {
            let lim = if res.is_none() { max_dist } else { res.get_item("t")?.extract()? };
            let r = a.call_method1("raycast", (slf, &oa, &da, lim))?;
            if r.is_truthy()? && (res.is_none() || r.get_item("t")?.extract::<f64>()? < res.get_item("t")?.extract::<f64>()?) {
                res = r;
            }
        }
        Ok(res)
    }

    // ------------------------------------------------------------------ 編集
    /// 置く・読み込む費用を払う（資源のルールを持つ MOD が決める。払えなければ False）
    fn pay(slf: &Bound<'_, Self>, cost: &Bound<'_, PyAny>) -> R<bool> {
        let py = slf.py();
        for a in Self::addons_v(slf)? {
            if !a.call_method1("pay", (slf, py.get_type_bound::<PyDict>().call1((cost,))?))?.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn refund(slf: &Bound<'_, Self>, cost: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        for a in Self::addons_v(slf)? {
            a.call_method1("refund", (slf, py.get_type_bound::<PyDict>().call1((cost,))?))?;
        }
        Ok(())
    }
    #[pyo3(signature = (c, i=None))]
    fn _device_placed(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, i: Option<usize>) -> R<()> {
        let py = slf.py();
        let i = match i { Some(i) => i, None => return Ok(()) };
        let d = c.getattr("devs")?.get_item(i)?;
        if !d.is_truthy()? {
            return Ok(());
        }
        if Self::creative(slf)? {                // クリエイティブ：置いたタンクは燃料満タン（E で中身を切り替え）
            let dd = Self::g(slf, "registry")?.call_method1("device", (d.get_item("def")?,))?;
            let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
            if getd(&dd, "type", py.None())?.eq("tank")? {
                let cap: f64 = getd(&getd(&dd, "params", PyDict::new_bound(py))?, "capacity", 0)?.extract()?;
                let ct = PyDict::new_bound(py);
                ct.set_item("fuel", cap)?;
                d.get_item("state")?.set_item("contents", ct)?;
            }
        }
        for a in Self::addons_v(slf)? {
            a.call_method1("on_device_placed", (slf, c, i))?;
        }
        Ok(())
    }
    /// そのアイテムを今使えるか（MOD が絞っていれば従う）
    fn item_allowed(slf: &Bound<'_, Self>, item: &Bound<'_, PyAny>) -> R<bool> {
        let t = PyTuple::new_bound(slf.py(), item.iter()?.collect::<R<Vec<_>>>()?);
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("item_allowed", (slf, &t))?;
            if r.is_instance_of::<pyo3::types::PyBool>() && !r.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn blueprints_allowed(slf: &Bound<'_, Self>) -> R<bool> {
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("blueprints_allowed", (slf,))?;
            if r.is_instance_of::<pyo3::types::PyBool>() && !r.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// ブロックのパレットに出す種類（None なら全部）
    fn block_categories<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let mut out: Option<Bound<'py, PyAny>> = None;
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("block_categories", (slf,))?;
            if !r.is_none() {
                let s = py.get_type_bound::<PySet>().call1((r,))?;
                out = Some(match out { None => s, Some(o) => o.call_method1("__and__", (s,))? });
            }
        }
        Ok(out.unwrap_or_else(|| py.None().into_bound(py)))
    }
    fn block_allowed(slf: &Bound<'_, Self>, kind: &Bound<'_, PyAny>) -> R<bool> {
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("block_allowed", (slf, kind))?;
            if r.is_instance_of::<pyo3::types::PyBool>() && !r.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn item_cost<'py>(slf: &Bound<'py, Self>, item: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let (kind, id) = (item.get_item(0)?, item.get_item(1)?);
        let reg = Self::g(slf, "registry")?;
        let dict = py.get_type_bound::<PyDict>();
        if kind.eq("material")? {
            return dict.call1((getd(&reg.call_method1("material", (id,))?, "cost", PyDict::new_bound(py))?,));
        }
        let dd = reg.call_method1("device", (id,))?;
        if dd.is_truthy()? {
            dict.call1((getd(&dd, "cost", PyDict::new_bound(py))?,))
        } else {
            Ok(PyDict::new_bound(py).into_any())
        }
    }
    fn place(slf: &Bound<'_, Self>, hit: &Bound<'_, PyAny>, item: &Bound<'_, PyAny>, dir_index: &Bound<'_, PyAny>, look_dir: &Bound<'_, PyAny>)
        -> R<bool> {
        let py = slf.py();
        let (kind, id) = (item.get_item(0)?, item.get_item(1)?);
        if !Self::item_allowed(slf, item)? {
            Self::message(slf, "それは今はまだ使えません（物語が進むと使えるようになります）")?;
            return Ok(false);
        }
        let (mat, dev): (Bound<'_, PyAny>, Bound<'_, PyAny>) = if kind.eq("material")? {
            (id.clone(), py.None().into_bound(py))
        } else {
            let dd = Self::g(slf, "registry")?.call_method1("device", (&id,))?;
            let d = PyDict::new_bound(py);
            d.set_item("def", &id)?;
            d.set_item("dir", dir_index)?;
            (dd.get_item("material")?, d.into_any())
        };
        let look = v3(look_dir)?;
        if hit.is_truthy()? && hit.get_item("kind")?.eq("voxel")? {
            let c = hit.get_item("construct")?;
            let i: usize = hit.get_item("index")?.extract()?;
            let face: usize = hit.get_item("face")?.extract()?;
            let cell0 = super::construct::cell_of(&c.getattr("cells")?.get_item(i)?)?;
            let du = super::construct::DIRS[face];
            let cell = [cell0[0] + du[0], cell0[1] + du[1], cell0[2] + du[2]];
            let ct = super::construct::tup3(py, cell);
            if c.getattr("index")?.contains(&ct)? {
                Self::message(slf, "そこには既にボクセルがあります")?;
                return Ok(false);
            }
            let f = c.call_method0("voxel_frames")?.get_item(i)?;
            Self::_hinge_side(slf, &dev, &f, look_dir)?;
            let fm = super::construct::m3(&f)?;
            let (axis, sign) = (face / 2, if face % 2 == 0 { 1.0 } else { -1.0 });
            let size: f64 = c.getattr("size")?.extract()?;
            let pos = v3s(&c.getattr("pos")?)?;
            let m = super::construct::mean8(&pos[8 * i..8 * i + 8]);
            let ctr = [m[0] + fm[axis][0] * sign * size, m[1] + fm[axis][1] * sign * size, m[2] + fm[axis][2] * sign * size];
            let mut verts = vec![];
            for cb in super::construct::CORNER_BITS.iter() {
                let l = [(cb[0] - 0.5) * size, (cb[1] - 0.5) * size, (cb[2] - 0.5) * size];
                let w = super::construct::mul_r(l, &fm);
                verts.push([ctr[0] + w[0], ctr[1] + w[1], ctr[2] + w[2]]);
            }
            if Self::_blocked(slf, &a1(py, ctr), size)? {
                return Ok(false);
            }
            if !Self::pay(slf, &Self::item_cost(slf, item)?)? {
                return Ok(false);
            }
            c.call_method1("add_voxel", (&ct, &mat, &dev, a3(py, &verts)?))?;
            let ni = c.getattr("index")?.call_method1("get", (&ct,))?;
            Self::_device_placed(slf, &c, if ni.is_none() { None } else { Some(ni.extract()?) })?;
            return Ok(true);
        }
        // 何もない所・地面 → 新しい構築物
        let size = fget(&Self::settings(slf)?, "voxel_size", 0.5)?;
        let (up, ctr) = if hit.is_truthy()? && (hit.get_item("kind")?.eq("terrain")? || hit.get_item("kind")?.eq("water")?
            || getd(hit, "placeable", py.None())?.is_truthy()?) {
            let up = v3(&hit.get_item("normal")?)?;
            let p = v3(&hit.get_item("point")?)?;
            (up, [p[0] + up[0] * size * 0.55, p[1] + up[1] * size * 0.55, p[2] + up[2] * size * 0.55])
        } else {
            let player = Self::g(slf, "player")?;
            let pp = player.getattr("pos")?;
            let pl = Self::nearest_planet(slf, &pp)?;
            let up = if pl.is_truthy()? { v3(&pl.call_method1("up", (&pp,))?)? } else { [0.0, 1.0, 0.0] };
            let e = v3(&player.call_method0("eye")?)?;
            (up, [e[0] + look[0] * 3.0, e[1] + look[1] * 3.0, e[2] + look[2] * 3.0])
        };
        if Self::_blocked(slf, &a1(py, ctr), size)? {
            return Ok(false);
        }
        let rm = Self::frame_of(up, look)?;
        let rt = [[rm[0][0], rm[1][0], rm[2][0]], [rm[0][1], rm[1][1], rm[2][1]], [rm[0][2], rm[1][2], rm[2][2]]];
        Self::_hinge_side(slf, &dev, &super::construct::mat(py, &rt)?, look_dir)?;
        if !Self::pay(slf, &Self::item_cost(slf, item)?)? {
            return Ok(false);
        }
        let off = super::construct::mul_t([0.5 * size, 0.5 * size, 0.5 * size], &rm);
        let origin = sub(ctr, off);
        let vox = PyList::new_bound(py, [PyTuple::new_bound(py, [PyTuple::new_bound(py, [0, 0, 0]).into_any(), mat.clone(), dev.clone()])]);
        let kw = PyDict::new_bound(py);
        kw.set_item("size", size)?;
        kw.set_item("name", "構築物")?;
        let c = imp(py, "main.construct")?.getattr("Construct")?.call_method("from_voxels",
            (Self::g(slf, "registry")?, vox, a1(py, origin), super::construct::mat(py, &rm)?), Some(&kw))?;
        Self::add_construct(slf, &c)?;
        Self::_device_placed(slf, &c, Some(0))?;
        if hit.is_truthy()? && getd(hit, "placeable", py.None())?.is_truthy()? {
            for a in Self::addons_v(slf)? {
                a.call_method1("on_placed", (slf, hit, &c))?;
            }
        }
        Ok(true)
    }
    /// ヒンジを置くとき：曲がる側を、置くときに見ている向きで決める
    fn _hinge_side(slf: &Bound<'_, Self>, dev: &Bound<'_, PyAny>, frame_rows: &Bound<'_, PyAny>, look_dir: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        if !dev.is_truthy()? {
            return Ok(());
        }
        if !super::actuator::act_is_hinge_dev(dev, Some(&Self::g(slf, "registry")?))? {
            return Ok(());
        }
        let d: i64 = getd(dev, "dir", 2)?.call_method0("__int__")?.extract()?;
        let fr = super::construct::np(py)?.call_method1("ascontiguousarray", (frame_rows, "float64"))?;
        let side = super::actuator::act_side_toward(fr.extract()?, d, v3(look_dir)?)?;
        dev.call_method1("setdefault", ("state", PyDict::new_bound(py)))?.set_item("hinge", side)?;
        let names = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"];
        Self::message(slf, &format!("ヒンジ：+ の角度で {} 側へ倒れます（置くときに見ていた側。-90〜+90°。R で出力の向き）", names[side]))
    }
    fn _blocked(slf: &Bound<'_, Self>, ctr: &Bound<'_, PyAny>, size: f64) -> R<bool> {
        let c = v3(ctr)?;
        let me = Self::g(slf, "player")?;
        for p in Self::all_players(slf)? {
            let pos = v3(&p.getattr("pos")?)?;
            let up = v3(&p.getattr("up")?)?;
            let h: f64 = p.getattr("height")?.extract()?;
            let r: f64 = p.getattr("radius")?.extract()?;
            for off in [0.0, h - 2.0 * r] {
                if norm(sub([pos[0] + up[0] * off, pos[1] + up[1] * off, pos[2] + up[2] * off], c)) < size * 0.5 + r * 0.9 {
                    let msg = if p.is(&me) { "自分の位置には置けません".to_string() } else { format!("{} がいる所には置けません", p.getattr("name")?.str()?) };
                    Self::message(slf, &msg)?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn remove(slf: &Bound<'_, Self>, hit: &Bound<'_, PyAny>) -> R<bool> {
        let py = slf.py();
        if !hit.is_truthy()? || !hit.get_item("kind")?.eq("voxel")? {
            return Ok(false);
        }
        let c = hit.get_item("construct")?;
        let i: usize = hit.get_item("index")?.extract()?;
        let mat = c.getattr("mats")?.get_item(i)?;
        let dev = c.getattr("devs")?.get_item(i)?;
        let cell = c.getattr("cells")?.get_item(i)?;
        let cid = c.getattr("id")?;
        for p in Self::all_players(slf)? {
            let s = p.getattr("seated")?;
            if s.is_truthy()? && s.get_item(0)?.eq(&cid)? && s.get_item(1)?.eq(&cell)? {
                p.setattr("seated", py.None())?;
            }
        }
        c.call_method1("remove_voxels", (vec![i],))?;
        let item: (&str, Bound<'_, PyAny>) = if dev.is_truthy()? { ("device", dev.get_item("def")?) } else { ("material", mat) };
        let item: PyObject = item.into_py(py);
        Self::refund(slf, &Self::item_cost(slf, item.bind(py))?)?;
        if c.getattr("n_vox")?.extract::<usize>()? == 0 {
            Self::g(slf, "constructs")?.del_item(&cid)?;
        } else {
            for part in c.call_method0("split")?.iter()? {
                Self::add_construct(slf, &part?)?;
            }
        }
        Ok(true)
    }
    /// 左クリック：地面を掘る（資源は MOD が受け取る）。水面は MOD が使えば使う
    #[pyo3(signature = (hit, radius=0.8))]
    fn dig(slf: &Bound<'_, Self>, hit: &Bound<'_, PyAny>, radius: f64) -> R<bool> {
        if hit.is_truthy()? && hit.get_item("kind")?.eq("water")? {
            for a in Self::addons_v(slf)? {
                if a.call_method1("on_water", (slf, hit))?.is_truthy()? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if !hit.is_truthy()? || !hit.get_item("kind")?.eq("terrain")? {
            return Ok(false);
        }
        let pl = hit.get_item("planet")?;
        let res = pl.call_method1("resource_at", (hit.get_item("point")?,))?;      // 掘る前の場所で判定
        pl.call_method1("dig", (hit.get_item("point")?, radius))?;
        for a in Self::addons_v(slf)? {
            a.call_method1("on_dig", (slf, hit, &res))?;
        }
        Ok(true)
    }
    /// ジェットパックを使えるか（燃料などは MOD が決める）：(使えるか, 噴射の向き)
    fn jetpack<'py>(slf: &Bound<'py, Self>, player: &Bound<'py, PyAny>, dt: f64, thrust_dir: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
        let mut flying = player.getattr("jetpack")?;
        let mut td = thrust_dir.clone();
        for a in Self::addons_v(slf)? {
            if flying.is_truthy()? {
                let r = a.call_method1("jetpack", (slf, player, dt, &td))?;
                flying = r.get_item(0)?;
                td = r.get_item(1)?;
            }
        }
        Ok(PyTuple::new_bound(slf.py(), [flying, td]))
    }
    fn interact(slf: &Bound<'_, Self>, hit: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let p = Self::g(slf, "player")?;
        if p.getattr("seated")?.is_truthy()? {
            p.setattr("seated", py.None())?;
            return Self::message(slf, "操縦席から降りました");
        }
        if !hit.is_truthy()? || !hit.get_item("kind")?.eq("voxel")? {
            return Ok(());
        }
        let c = hit.get_item("construct")?;
        let i: usize = hit.get_item("index")?.extract()?;
        let d = c.getattr("devs")?.get_item(i)?;
        let reg = Self::g(slf, "registry")?;
        let cname = c.getattr("name")?.str()?.to_string();
        if d.is_truthy()? {
            let dd = reg.call_method1("device", (d.get_item("def")?,))?;
            let dtype: Option<String> = if dd.is_truthy()? { Some(dd.get_item("type")?.extract()?) } else { None };
            if dtype.as_deref() == Some("seat") {
                p.setattr("seated", (c.getattr("id")?, c.getattr("cells")?.get_item(i)?))?;
                return Self::message(slf, &format!("{cname} の操縦席に座りました（E で降りる）"));
            }
            for a in Self::addons_v(slf)? {
                let msg = a.call_method1("interact_device", (slf, &c, i, &d, &dd))?;
                if msg.is_truthy()? {
                    return Self::message(slf, &msg.str()?.to_string());
                }
            }
            if dtype.as_deref() == Some("tank") {
                // E でタンクの中身を 燃料満タン → 空気満タン → 空 と切り替え
                let cap: f64 = getd(&dd.get_item("params")?, "capacity", 0)?.extract()?;
                let cont = d.get_item("state")?.call_method1("setdefault", ("contents", PyDict::new_bound(py)))?;
                let dn = d.get_item("name")?.str()?.to_string();
                if fget(&cont, "fuel", 0.0)? >= cap * 0.99 {
                    cont.call_method0("clear")?;
                    cont.set_item("air", cap)?;
                    Self::message(slf, &format!("{dn}: 空気を満タンにしました（E で空に）"))?;
                } else if fget(&cont, "air", 0.0)? >= cap * 0.99 {
                    cont.call_method0("clear")?;
                    Self::message(slf, &format!("{dn}: 空にしました（E で燃料満タン）"))?;
                } else {
                    cont.call_method0("clear")?;
                    cont.set_item("fuel", cap)?;
                    Self::message(slf, &format!("{dn}: 燃料を満タンにしました（E で空気に）"))?;
                }
                return Ok(());
            }
            if dtype.as_deref() == Some("actuator") {
                let m = super::actuator::act_execute_preset(slf.as_any(), &c, i)?;
                return Self::message(slf, &m);
            }
            if let Some(m) = super::devices::dev_interact(py, &c, i, Some(p.clone().unbind()))? {
                if !m.is_empty() {
                    Self::message(slf, &m)?;
                }
            }
            return Ok(());
        }
        for a in Self::addons_v(slf)? {
            let msg = a.call_method1("interact", (slf, &c, i))?;
            if msg.is_truthy()? {
                return Self::message(slf, &msg.str()?.to_string());
            }
        }
        let m = reg.call_method1("material", (c.getattr("mats")?.get_item(i)?,))?;
        let tm: f64 = c.call_method0("total_mass")?.extract()?;
        Self::message(slf, &format!("{cname}: {} / ボクセル {} / 質量 {:.1} t", m.get_item("name")?.str()?, c.getattr("n_vox")?, tm / 1000.0))
    }
    /// 操縦席で押されたキー：キー割り当てのあるアクチュエータを切り替える
    fn key_pressed(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> R<()> {
        let s = Self::g(slf, "player")?.getattr("seated")?;
        if !s.is_truthy()? {
            return Ok(());
        }
        let c = Self::g(slf, "constructs")?.call_method1("get", (s.get_item(0)?,))?;
        if !c.is_truthy()? {
            return Ok(());
        }
        let reg = Self::g(slf, "registry")?;
        for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
            let d = d?;
            if !d.is_truthy()? {
                continue;
            }
            let dd = reg.call_method1("device", (d.get_item("def")?,))?;
            if dd.is_truthy()? && dd.get_item("type")?.eq("actuator")? && getd(&dd.get_item("params")?, "key", slf.py().None())?.eq(key)? {
                let pr = getd(&d.get_item("state")?, "preset", slf.py().None())?;
                if pr.is_truthy()? && getd(&pr, "action", slf.py().None())?.eq("none")? {
                    continue;               // 動作「なし」：キーでは動かさない（プログラムが動かす）
                }
                let m = super::actuator::act_execute_preset(slf.as_any(), &c, i)?;
                Self::message(slf, &m)?;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------ OBJ
    fn export_obj<'py>(slf: &Bound<'py, Self>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let cname: String = c.getattr("name")?.str()?.to_string();
        let id: String = c.getattr("id")?.str()?.to_string();
        let safe: String = cname.chars().map(|ch| if ch.is_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' }).collect();
        let name = format!("{safe}_{}", id.chars().take(4).collect::<String>());
        let ed = Self::g(slf, "exports_dir")?;
        let kw = PyDict::new_bound(py);
        kw.set_item("scripts_dir", Self::g(slf, "dir")?.call_method1("__truediv__", ("scripts",))?)?;
        let path = imp(py, "main.objio")?.call_method("export_construct", (c, &ed, name), Some(&kw))?;
        Self::message(slf, &format!("OBJ 出力: {}/{}", ed.getattr("name")?.str()?, path.getattr("name")?.str()?))?;
        Ok(path)
    }
    /// ボクセル一覧を置くのに必要な資源（機器は機器の費用、それ以外は素材の費用）
    fn voxels_cost<'py>(slf: &Bound<'py, Self>, voxels: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let total = PyDict::new_bound(py);
        for v in voxels.iter()? {
            let v = v?;
            let (mat, dev) = (v.get_item(1)?, v.get_item(2)?);
            let item: (&str, Bound<'_, PyAny>) = if dev.is_truthy()? { ("device", dev.get_item("def")?) } else { ("material", mat) };
            let item: PyObject = item.into_py(py);
            let cost = Self::item_cost(slf, item.bind(py))?;
            for (k, x) in cost.downcast::<PyDict>()?.iter() {
                let cur: f64 = total.get_item(&k)?.map(|v| v.extract()).transpose()?.unwrap_or(0.0);
                total.set_item(k, cur + x.extract::<f64>()?)?;
            }
        }
        Ok(total)
    }
    #[pyo3(signature = (path, position=None, look_dir=None, **opts))]
    fn import_obj<'py>(slf: &Bound<'py, Self>, path: &Bound<'py, PyAny>, position: Option<&Bound<'py, PyAny>>, look_dir: Option<&Bound<'py, PyAny>>,
                       opts: Option<&Bound<'py, PyDict>>) -> R<Bound<'py, PyAny>> {
        let data = imp(slf.py(), "main.objio")?.call_method("import_construct", (Self::g(slf, "registry")?, path), opts)?;
        Self::place_import(slf, &data, position, look_dir)
    }
    /// 読み込んだ構築物（objio.import_construct の結果）をプレイヤーの前に置く。サバイバルでは資源を払う
    #[pyo3(signature = (data, position=None, look_dir=None))]
    fn place_import<'py>(slf: &Bound<'py, Self>, data: &Bound<'py, PyAny>, position: Option<&Bound<'py, PyAny>>, look_dir: Option<&Bound<'py, PyAny>>)
        -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let none = py.None().into_bound(py);
        let vox = data.get_item("voxels")?;
        if !vox.is_truthy()? {
            Self::message(slf, "ボクセルが0個でした")?;
            return Ok(none);
        }
        if !Self::pay(slf, Self::voxels_cost(slf, &vox)?.as_any())? {
            return Ok(none);
        }
        let size: f64 = data.get_item("size")?.extract()?;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in vox.iter()? {
            let c = v3(&v?.get_item(0)?)?;
            for k in 0..3 {
                lo[k] = lo[k].min(c[k]);
                hi[k] = hi[k].max(c[k]);
            }
        }
        let player = Self::g(slf, "player")?;
        let pp = player.getattr("pos")?;
        let pl = Self::nearest_planet(slf, &pp)?;
        let up = if pl.is_truthy()? { v3(&pl.call_method1("up", (&pp,))?)? } else { [0.0, 1.0, 0.0] };
        let look = match look_dir { Some(l) if !l.is_none() => v3(l)?, _ => [0.0, 0.0, 1.0] };
        let rm = Self::frame_of_import(up, look);
        let fwd = [rm[0][2], rm[1][2], rm[2][2]];
        let ext = [(hi[0] - lo[0] + 1.0) * size, (hi[1] - lo[1] + 1.0) * size, (hi[2] - lo[2] + 1.0) * size];
        let position = match position {
            Some(p) if !p.is_none() => v3(p)?,
            _ => {
                let em = ext[0].max(ext[1]).max(ext[2]);
                let p0 = v3(&pp)?;
                let mut base = [p0[0] + fwd[0] * (em * 0.7 + 3.0), p0[1] + fwd[1] * (em * 0.7 + 3.0), p0[2] + fwd[2] * (em * 0.7 + 3.0)];
                if !pl.is_none() {
                    // 地表の上に置く
                    let dd: f64 = pl.call_method1("sdf", (arr(py, base.to_vec(), &[1, 3])?,))?.get_item(0)?.extract()?;
                    for k in 0..3 {
                        base[k] = base[k] - up[k] * dd + up[k] * 0.5;
                    }
                }
                base
            }
        };
        let lmin = [lo[0] * size, lo[1] * size, lo[2] * size];
        let co = [ext[0] / 2.0 + lmin[0], 0.0 + lmin[1], ext[2] / 2.0 + lmin[2]];
        let origin = sub(position, super::construct::mul_t(co, &rm));
        let kw = PyDict::new_bound(py);
        kw.set_item("size", size)?;
        kw.set_item("links", data.get_item("links")?)?;
        kw.set_item("name", data.get_item("name")?)?;
        let c = imp(py, "main.construct")?.getattr("Construct")?.call_method("from_voxels",
            (Self::g(slf, "registry")?, &vox, a1(py, origin), super::construct::mat(py, &rm)?), Some(&kw))?;
        c.setattr("storage", py.get_type_bound::<PyDict>().call1((getd(data, "storage", PyDict::new_bound(py))?,))?)?;
        Self::add_construct(slf, &c)?;
        // 一緒に入っていたプログラムを scripts へ（同じ名前で中身が違うものがあれば上書きしない）
        let sdir = Self::g(slf, "dir")?.call_method1("__truediv__", ("scripts",))?;
        sdir.call_method("mkdir", (), Some(&[("exist_ok", true)].into_py_dict_bound(py)))?;
        let enc = [("encoding", "utf-8")].into_py_dict_bound(py);
        let mut kept = vec![];
        let sc = getd(data, "scripts", py.None())?;
        if sc.is_truthy()? {
            for (f, text) in sc.downcast::<PyDict>()?.iter() {
                let fs: String = f.str()?.to_string();
                if fs.contains('/') || fs.contains('\\') || fs.contains("..") {
                    continue;
                }
                let fp = sdir.call_method1("__truediv__", (&f,))?;
                if !fp.call_method0("exists")?.is_truthy()? {
                    fp.call_method("write_text", (&text,), Some(&enc))?;
                } else if !fp.call_method("read_text", (), Some(&enc))?.eq(&text)? {
                    kept.push(fs);
                }
            }
        }
        if !kept.is_empty() {
            Self::message(slf, &format!("同じ名前のプログラムが既にあるので、そちらを使います: {}", kept.join(", ")))?;
        }
        Self::message(slf, &format!("読み込み: {}（{} ボクセル）", data.get_item("name")?.str()?, c.getattr("n_vox")?))?;
        Ok(c)
    }
}

impl World {
    fn wake_in(slf: &Bound<'_, Self>, woke: &[Bound<'_, PyAny>], dt: f64) -> R<()> {
        for c in woke {
            c.call_method0("wake")?;
            c.setattr("_prev", c.getattr("pos")?.call_method0("copy")?)?;
            Self::_prep_ext(slf, c, dt)?;
        }
        Ok(())
    }
    fn still_sleeping<'py>(sleeping: Vec<Bound<'py, PyAny>>, anchored: &Bound<'py, PyAny>) -> R<Vec<Bound<'py, PyAny>>> {
        let mut out = vec![];
        for c in sleeping {
            if c.getattr("asleep")?.is_truthy()? || anchored.contains(c.getattr("id")?)? {
                out.push(c);
            }
        }
        Ok(out)
    }
    /// 外側の球のリストでの番号 {id(c): k}（リストが変わったときだけ作り直す）
    fn bnd_at<'py>(slf: &Bound<'py, Self>, cs: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        if let Some(at) = Self::dict_get(slf, "_bnd_at")? {
            if !at.is_none() && at.get_item(0)?.is(cs) {
                return at.get_item(1);
            }
        }
        let m = PyDict::new_bound(py);
        for (k, c) in cs.iter()?.enumerate() {
            m.set_item(c?.as_ptr() as usize, k)?;
        }
        Self::set(slf, "_bnd_at", (cs, &m))?;
        Ok(m.into_any())
    }
    /// 置くときの向き：列が (右, 上, 前)
    fn frame_of(up: V3, look: V3) -> R<[[f64; 3]; 3]> {
        let mut fwd = sub(look, [up[0] * dot(look, up), up[1] * dot(look, up), up[2] * dot(look, up)]);
        if norm(fwd) < 1e-6 {
            fwd = cross(up, [1.0, 0.0, 0.0]);
        }
        let l = norm(fwd);
        fwd = [fwd[0] / l, fwd[1] / l, fwd[2] / l];
        let right = cross(up, fwd);
        Ok([[right[0], up[0], fwd[0]], [right[1], up[1], fwd[1]], [right[2], up[2], fwd[2]]])
    }
    fn frame_of_import(up: V3, look: V3) -> [[f64; 3]; 3] {
        let mut fwd = sub(look, [up[0] * dot(look, up), up[1] * dot(look, up), up[2] * dot(look, up)]);
        fwd = if norm(fwd) > 1e-6 { let l = norm(fwd); [fwd[0] / l, fwd[1] / l, fwd[2] / l] } else { cross(up, [1.0, 0.0, 0.0]) };
        let right = cross(up, fwd);
        [[right[0], up[0], fwd[0]], [right[1], up[1], fwd[1]], [right[2], up[2], fwd[2]]]
    }
}

#[allow(dead_code)]
fn _unused(py: Python<'_>) -> R<()> {
    let _ = (iv, PyAttributeError::new_err(""), a3(py, &[])?);
    Ok(())
}
