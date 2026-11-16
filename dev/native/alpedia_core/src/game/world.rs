//! ワールド（main/world.py の World）：読み込み・保存・シミュレーションの1ティック・編集操作。
//! 決まった属性は Rust の欄、それ以外（registry・settings・player など）は __dict__（MOD・画面からも同じ名前で読める）。

use crate::game::player::{ply, Player};
use pyo3::exceptions::{PyAttributeError, PyFileNotFoundError};
use pyo3::prelude::*;
use pyo3::types::{PyCFunction, PyDict, PyFrozenSet, PyList, PySet, PyString, PyTuple};

use super::construct::{a1, a3, arr, con, Construct, cross, dot, fv, iv, norm, opt, sub, v3, v3s, with_rw, R, V3};
use super::imp;

use crate::prof::add as prof_add;
/// 物理の内訳（秒の合計）を返して 0 に戻す
#[pyfunction]
pub fn world_step_prof() -> Vec<(String, f64)> {
    crate::prof::take()
}

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
    // 写さずにその場で見る（大きな建物の速度の配列を毎ティック写していて重かった）
    if let Ok(r) = o.extract::<numpy::PyReadonlyArrayDyn<f64>>() {
        return Ok(match r.as_slice() {
            Ok(s) => s.iter().any(|x| *x != 0.0),
            Err(_) => r.as_array().iter().any(|x| *x != 0.0),
        });
    }
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
    if !crate::game::registry::dev_dict(&reg, &pyo3::types::PyString::new_bound(py, "generator").into_any())?.is_none() {
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
    let vel_o = Construct::g_vel(con(&c)?)?;
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
        Construct::s_vel(con(&c)?, arr(py, nv, &[dv.len(), 3])?)?;
    }
    let rb = c.getattr("rb")?;
    if rb0.is_truthy()? && !rb.is_none() && Construct::g_mode(con(&c)?)?.eq("rigid")? {
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
        self.prev = Some((w.getattr("player")?.unbind(), World::g_msg_sink(wld(&w)?)?.unbind()));
        w.setattr("player", p)?;
        World::s_msg_sink(wld(&w)?, p.getattr("msgs")?)?;
        Ok(())
    }
    #[pyo3(signature = (*_a))]
    fn __exit__(&mut self, py: Python<'_>, _a: &Bound<'_, PyTuple>) -> R<bool> {
        if let Some((p, s)) = self.prev.take() {
            let w = self.world.bind(py);
            w.setattr("player", p)?;
            World::s_msg_sink(wld(&w)?, s)?;
        }
        Ok(false)
    }
}

/// Python の値として持っているワールドを、型つきで見る（違う物ならエラー）
#[inline]
pub fn wld<'a, 'py>(w: &'a Bound<'py, PyAny>) -> R<&'a Bound<'py, World>> {
    Ok(w.downcast::<World>()?)
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
    /// ブロック・素材・機器などの定義（読み込みで作る）
    #[pyo3(get, set)]
    pub registry: PyObject,
    /// ワールドの設定（world.json の settings）
    #[pyo3(get, set)]
    pub settings: PyObject,
    /// 今動かしているプレイヤー（ほかの参加者の操作の間は、その人に入れ替わる）・この PC のプレイヤー
    #[pyo3(get, set)]
    pub player: PyObject,
    #[pyo3(get, set)]
    pub local_player: PyObject,
    /// 計算の覚え（Python からは見えない）
    pub cache: WorldCaches,
}

/// ワールドの計算の覚え
#[derive(Default)]
pub struct WorldCaches {
    pub bnd: Option<PyObject>,
    pub bnd_at: Option<PyObject>,
    pub bnd_t: Option<PyObject>,
    pub grav: Option<PyObject>,
    pub grav_key: Option<PyObject>,
    pub far: Option<PyObject>,
    pub far_t: Option<PyObject>,
    pub far_n: Option<PyObject>,
    pub far_prev: Option<PyObject>,
    pub pool_obj: Option<PyObject>,
}

impl World {
    fn addons_v<'py>(slf: &Bound<'py, Self>) -> R<Vec<Bound<'py, PyAny>>> {
        Self::g_addons(slf)?.iter()?.collect()
    }
    fn cons_v<'py>(slf: &Bound<'py, Self>) -> R<Vec<Bound<'py, PyAny>>> {
        Self::g_constructs(slf)?.call_method0("values")?.iter()?.collect()
    }
    fn settings<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        Self::g_settings(slf)
    }
    fn gscale(slf: &Bound<'_, Self>) -> R<f64> {
        fget(&Self::settings(slf)?, "gravity_scale", 1.0)
    }
    fn is_seated_on(p: &Bound<'_, PyAny>, cid: &Bound<'_, PyAny>) -> R<bool> {
        let s = Player::g_seated(ply(&p)?)?;
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
            registry: py.None(), settings: PyDict::new_bound(py).into_any().unbind(), player: py.None(), local_player: py.None(),
            cache: WorldCaches::default(),
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
        let dir = Self::g_dir(slf)?;
        let lg = log(py)?;
        if !dir.call_method0("exists")?.is_truthy()? {
            return Err(PyFileNotFoundError::new_err(format!("ワールド '{}' がありません: {}", Self::g_name(slf)?.str()?, dir.str()?)));
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
        Self::s_mods(slf, PyList::new_bound(py, mods.iter()?.collect::<R<Vec<_>>>()?))?;
        // MOD はここで一括で読み込む（定義・プログラムの全ファイル）。エラーがあればワールドを始めない
        let reg = imp(py, "main.registry")?.getattr("Registry")?.call0()?.call_method1("load", (&mods,))?;
        Self::s_registry(slf, &reg)?;
        let errors = reg.getattr("errors")?;
        for m in missing.iter()? {
            let m = m?;
            let (mid, parent) = (m.get_item(0)?, m.get_item(1)?);
            errors.call_method1("append", (format!("MOD '{}' に必要な MOD '{}' が mods/ にありません",
                modmeta.call_method1("name_of", (parent,))?.str()?, mid.str()?),))?;
        }
        if given_none {
            modcode.call_method1("load", (reg.getattr("loaded_mods")?, &errors))?;
            Self::s_addons(slf, modcode.call_method1("world_addons", (reg.getattr("loaded_mods")?, &errors))?)?;
        }
        let survival = imp(py, "main.survival")?;
        if survival.call_method1("mode_of", (&s,))?.eq("survival")? && Self::addon(slf, "survival")?.is_none() {
            let sr = survival.getattr("SurvivalRules")?.call0()?;
            Self::g_addons(slf)?.call_method1("insert", (0, sr))?;      // 本体の遊び方（MOD ではない）
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
        Self::s_settings(slf, &settings)?;
        slf.borrow_mut().time = fget(settings.as_any(), "time", 0.0)?;
        let cons = world_read_json(&div("constructs.json")?, PyList::empty_bound(py).as_any())?;
        let n_gen = world_drop_fuel_generators(&reg, &cons)?;
        for e in world_missing_references(&reg, settings.as_any(), &cons, &Self::g_mods(slf)?)? {
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
        Self::s_planets(slf, &planets)?;
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
        let constructs = Self::g_constructs(slf)?;
        for cd in cons.iter()? {
            let cd = cd?;
            let r = cd.get_item("id").and_then(|id| ccls.call_method1("from_dict", (&reg, &cd, getd(&spawn, &id.str()?.to_string(), PyDict::new_bound(py))?)));
            match r {
                Ok(c) => constructs.set_item(Construct::g_id(con(&c)?)?, c)?,
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
        Self::s_player(slf, &player)?;
        if pj.is_truthy()? {
            for m in savefmt.call_method1("check_player", (&pj,))?.iter()? {
                lg.call_method1("warning", ("保存形式: %s", m?))?;
            }
            player.call_method1("load_dict", (&pj,))?;
        }
        Self::s_local_player(slf, &player)?;
        for a in Self::addons_v(slf)? {
            a.call_method1("on_load", (slf,))?;
        }
        if settings.call_method1("pop", ("starter_sample", py.None()))?.is_truthy()? {
            Self::_place_starter_sample(slf)?;
        }
        Self::_refresh_script_readme(slf)?;
        lg.call_method1("info", ("ワールド '%s' を読み込み: 構築物 %d, 惑星 %d, MOD %s", Self::g_name(slf)?, constructs.len()?,
            planets.len(), &mods))?;
        Self::message(slf, &format!("ワールド「{}」を読み込みました", Self::g_name(slf)?.str()?))?;
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
        let p = Self::g_dir(slf)?.call_method1("__truediv__", ("scripts",))?.call_method1("__truediv__", ("README.txt",))?;
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
            let data = imp(py, "main.objio")?.call_method1("import_construct", (Self::g_registry(slf)?, &bp))?;
            let ppos = v3(&Self::g_player(slf)?.getattr("pos")?)?;
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

}

// 中身は分けて書いている（C-02）
mod players;
mod space;
mod step;
mod tick;
mod query;
mod edit;
mod interact;
mod inner;

// ---------------------------------------------------------------- 欄の読み書き（C-01）
// g_欄：値（Python の値として）、s_欄：入れる（型を確かめる）
macro_rules! world_py_fields {
    ($($g:ident $s:ident $f:ident),* $(,)?) => {
        #[allow(dead_code)]
        impl World { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
                Ok(slf.try_borrow()?.$f.bind(slf.py()).clone().into_any())
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> R<()> {
                let py = slf.py();
                let t = v.into_py(py).extract(py)?;
                slf.try_borrow_mut()?.$f = t;
                Ok(())
            }
        )* }
    };
}
macro_rules! world_val_fields {
    ($($g:ident $s:ident $f:ident),* $(,)?) => {
        #[allow(dead_code)]
        impl World { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
                Ok(slf.try_borrow()?.$f.into_py(slf.py()).into_bound(slf.py()))
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> R<()> {
                let py = slf.py();
                let t = v.into_py(py).extract(py)?;
                slf.try_borrow_mut()?.$f = t;
                Ok(())
            }
        )* }
    };
}
world_py_fields!(
    g_addons s_addons addons,
    g_cfg s_cfg cfg,
    g_constructs s_constructs constructs,
    g_dir s_dir dir,
    g_exports_dir s_exports_dir exports_dir,
    g_given_addons s_given_addons given_addons,
    g_input_keys s_input_keys input_keys,
    g_local_player s_local_player local_player,
    g_mods s_mods mods,
    g_msg_sink s_msg_sink msg_sink,
    g_msgs s_msgs msgs,
    g_name s_name name,
    g_net s_net net,
    g_planets s_planets planets,
    g_player s_player player,
    g_registry s_registry registry,
    g_remote s_remote remote,
    g_settings s_settings settings,
    g_timers s_timers timers
);
world_val_fields!(
    g_iters s_iters iters,
    g_substeps s_substeps substeps,
    g_time s_time time,
    g_time_scale s_time_scale time_scale
);

impl World {
    /// 覚えの欄を読む（無い・None なら None）
    pub fn cache_get<'py>(slf: &Bound<'py, Self>, f: impl Fn(&WorldCaches) -> &Option<PyObject>) -> R<Option<Bound<'py, PyAny>>> {
        let b = slf.try_borrow()?;
        Ok(match f(&b.cache) {
            Some(o) if !o.is_none(slf.py()) => Some(o.bind(slf.py()).clone()),
            _ => None,
        })
    }
    pub fn cache_put(slf: &Bound<'_, Self>, f: impl Fn(&mut WorldCaches) -> &mut Option<PyObject>, v: impl IntoPy<PyObject>) -> R<()> {
        let o = v.into_py(slf.py());
        *f(&mut slf.try_borrow_mut()?.cache) = Some(o);
        Ok(())
    }
}
