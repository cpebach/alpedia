//! 小さなモジュールの中身（main/extensions.py・tutorial.py・app.py・content.py・tutorial_rod.py・tutorial_drone.py・
//! geom.py・winperf.py）。文章（README・ゲーム内のプログラム・看板）は Python 側の表のまま。

use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{IntoPyDict, PyDict, PyList, PyTuple};

use super::imp;

type R<T> = PyResult<T>;

fn json(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "json")
}
fn deep<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let j = json(o.py())?;
    j.call_method1("loads", (j.call_method1("dumps", (o,))?,))
}
fn dumps(py: Python<'_>, o: &Bound<'_, PyAny>) -> R<String> {
    let kw = [("ensure_ascii", false.into_py(py)), ("indent", 1.into_py(py))].into_py_dict_bound(py);
    json(py)?.call_method("dumps", (o,), Some(&kw))?.extract()
}
fn div<'py>(p: &Bound<'py, PyAny>, s: impl IntoPy<PyObject>) -> R<Bound<'py, PyAny>> {
    p.call_method1("__truediv__", (s,))
}
fn write_text(p: &Bound<'_, PyAny>, s: &str) -> R<()> {
    p.call_method("write_text", (s,), Some(&[("encoding", "utf-8")].into_py_dict_bound(p.py())))?;
    Ok(())
}

// ---------------------------------------------------------------- extensions：MOD が足す機器のふるまい・スクリプトの口

static DEV_METHODS: GILOnceCell<Py<PyDict>> = GILOnceCell::new();
static DEV_PROPS: GILOnceCell<Py<PyDict>> = GILOnceCell::new();
static API_FUNCS: GILOnceCell<Py<PyDict>> = GILOnceCell::new();

fn table<'py>(py: Python<'py>, c: &GILOnceCell<Py<PyDict>>) -> Bound<'py, PyDict> {
    c.get_or_init(py, || PyDict::new_bound(py).unbind()).bind(py).clone()
}
/// behavior → {名前: fn}
#[pyfunction]
pub fn ext_device_methods(py: Python<'_>) -> Bound<'_, PyDict> {
    table(py, &DEV_METHODS)
}
/// behavior → {名前: fn}
#[pyfunction]
pub fn ext_device_props(py: Python<'_>) -> Bound<'_, PyDict> {
    table(py, &DEV_PROPS)
}
/// 名前 → fn
#[pyfunction]
pub fn ext_api_funcs(py: Python<'_>) -> Bound<'_, PyDict> {
    table(py, &API_FUNCS)
}
#[pyfunction]
pub fn ext_reset(py: Python<'_>) -> R<()> {
    for t in [&DEV_METHODS, &DEV_PROPS, &API_FUNCS] {
        table(py, t).clear();
    }
    imp(py, "main.blocks")?.call_method0("reset_ext")?;
    Ok(())
}
#[pyfunction]
#[pyo3(signature = (behavior, methods=None, props=None))]
pub fn ext_register_device(py: Python<'_>, behavior: &Bound<'_, PyAny>, methods: Option<&Bound<'_, PyAny>>, props: Option<&Bound<'_, PyAny>>) -> R<()> {
    for (t, v) in [(&DEV_METHODS, methods), (&DEV_PROPS, props)] {
        let d = table(py, t).call_method1("setdefault", (behavior, PyDict::new_bound(py)))?;
        if let Some(v) = v.filter(|v| !v.is_none()) {
            if v.is_truthy()? {
                d.call_method1("update", (v,))?;
            }
        }
    }
    Ok(())
}
#[pyfunction]
pub fn ext_register_api(py: Python<'_>, name: &Bound<'_, PyAny>, f: &Bound<'_, PyAny>) -> R<()> {
    table(py, &API_FUNCS).set_item(name, f)
}
/// 機器の定義 → スクリプトから見た種類（custom なら behavior）
#[pyfunction]
pub fn ext_behavior_of(py: Python<'_>, dd: &Bound<'_, PyAny>) -> R<PyObject> {
    if !dd.is_truthy()? {
        return Ok("?".into_py(py));
    }
    if dd.call_method1("get", ("type",))?.eq("custom")? {
        let b = dd.call_method1("get", ("behavior",))?;
        return Ok(if b.is_truthy()? { b.str()?.into_any().unbind() } else { "custom".into_py(py) });
    }
    Ok(dd.get_item("type")?.unbind())
}

// ---------------------------------------------------------------- tutorial：新しいワールドを作る

/// ワールドを作る。mods：選んだ MOD（依存する MOD はここで足す）。
/// 選んだ MOD に "world"（ワールドの形）を持つものがあれば、その形で作る（惑星の選択は使わない）
#[pyfunction]
#[pyo3(signature = (name, mods=None, settings_patch=None, planet="terra"))]
pub fn tut_create_world<'py>(py: Python<'py>, name: &Bound<'py, PyAny>, mods: Option<&Bound<'py, PyAny>>, settings_patch: Option<&Bound<'py, PyAny>>,
                             planet: &str) -> R<Bound<'py, PyAny>> {
    let paths = imp(py, "main.paths")?;
    let modmeta = imp(py, "main.modmeta")?;
    let lg = imp(py, "main.log")?.call_method1("get", ("newworld",))?;
    let wmod = imp(py, "main.world")?;
    let wdir = paths.call_method1("world_dir", (name,))?;
    if wdir.call_method0("exists")?.is_truthy()? {
        return Err(pyo3::exceptions::PyFileExistsError::new_err(format!("ワールド「{}」は既にあります", name.str()?)));
    }
    let ml = PyList::empty_bound(py);
    if let Some(m) = mods {
        for x in m.iter()? {
            ml.append(x?)?;
        }
    }
    let r = modmeta.call_method1("resolve", (ml,))?;
    let (mods, missing) = (r.get_item(0)?, r.get_item(2)?);
    if missing.is_truthy()? {
        lg.call_method1("warning", ("依存する MOD が見つかりません: %s", &missing))?;
    }
    for sub in ["scripts", "exports", "imports"] {
        div(&wdir, sub)?.call_method("mkdir", (), Some(&[("parents", true), ("exist_ok", true)].into_py_dict_bound(py)))?;
    }
    let ds = wmod.getattr("DEFAULT_SETTINGS")?;
    let s = deep(&ds)?;
    s.set_item("name", name)?;
    s.set_item("format", imp(py, "main.savefmt")?.getattr("FORMAT")?)?;
    let bodies = |main: &Bound<'py, PyAny>, moon: &Bound<'py, PyAny>, ast: &Bound<'py, PyAny>| -> R<Bound<'py, PyAny>> {
        wmod.call_method1("default_bodies", (main, moon, ast))
    };
    s.set_item("planets", wmod.call_method1("default_bodies", (planet,))?)?;
    let patch = PyDict::new_bound(py);
    if let Some(p) = settings_patch.filter(|p| !p.is_none()) {
        if p.is_truthy()? {
            patch.update(p.downcast::<PyDict>()?.as_mapping())?;
        }
    }
    let wp = modmeta.call_method1("world_preset", (&mods,))?;
    let (pre_id, preset) = (wp.get_item(0)?, wp.get_item(1)?);
    let cleanup = s.get_item("cleanup")?;
    if preset.is_truthy()? {
        lg.call_method1("info", ("MOD '%s' のワールドの形で作ります", &pre_id))?;
        let pp = preset.call_method1("get", ("planets",))?;
        if pp.is_instance_of::<PyList>() {
            s.set_item("planets", deep(&pp)?)?;
        } else {
            let g = |k: &str, d: PyObject| -> R<Bound<'py, PyAny>> { preset.call_method1("get", (k, d)) };
            let b = bodies(&g("main", "terra".into_py(py))?, &g("moon", true.into_py(py))?, &g("asteroids", true.into_py(py))?)?;
            let mo = preset.call_method1("get", ("main_override",))?;
            if mo.is_instance_of::<PyDict>() {
                b.get_item(0)?.set_item("override", py.get_type_bound::<PyDict>().call1((mo,))?)?;
            }
            s.set_item("planets", b)?;
        }
        for k in ["voxel_size", "gravity_scale"] {
            patch.call_method1("pop", (k, py.None()))?;                 // 舞台はこの大きさで作ってある
        }
        let cl = patch.call_method1("pop", ("cleanup", py.None()))?;
        let pset = preset.call_method1("get", ("settings", PyDict::new_bound(py)))?;
        s.call_method1("update", (deep(&pset)?,))?;
        let pset_cl = if pset.is_truthy()? { pset.call_method1("get", ("cleanup",))? } else { py.None().into_bound(py) };
        if cl.is_truthy()? && !pset_cl.is_truthy()? {
            s.get_item("cleanup")?.call_method1("update", (cl,))?;
        }
    } else if patch.contains("cleanup")? {
        cleanup.call_method1("update", (patch.call_method1("pop", ("cleanup",))?,))?;
    }
    s.call_method1("update", (&patch,))?;
    let cl = PyDict::new_bound(py);
    cl.update(ds.get_item("cleanup")?.downcast::<PyDict>()?.as_mapping())?;
    cl.update(s.call_method1("get", ("cleanup", PyDict::new_bound(py)))?.downcast::<PyDict>()?.as_mapping())?;
    s.set_item("cleanup", cl)?;
    write_text(&div(&wdir, "world.json")?, &dumps(py, &s)?)?;
    write_text(&div(&wdir, "constructs.json")?, "[]")?;
    write_text(&div(&wdir, "spawn.json")?, "{}")?;
    let md = PyDict::new_bound(py);
    md.set_item("enabled", PyList::new_bound(py, mods.iter()?.collect::<R<Vec<_>>>()?))?;
    write_text(&div(&wdir, "mods.json")?, &dumps(py, md.as_any())?)?;
    let readme: String = imp(py, "main.content")?.getattr("SCRIPT_README")?.extract()?;
    write_text(&div(&div(&wdir, "scripts")?, "README.txt")?, &readme)?;
    lg.call_method1("info", ("ワールド作成: %s（MOD %s）", name, &mods))?;
    Ok(wdir)
}

/// シナリオモードのワールドを作る（テスト・初回起動用）
#[pyfunction]
#[pyo3(signature = (name=None))]
pub fn tut_create_tutorial<'py>(py: Python<'py>, name: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
    let t = imp(py, "main.tutorial")?;
    let n = match name { Some(n) => n.clone(), None => t.getattr("SCENARIO_WORLD")? };
    tut_create_world(py, &n, Some(PyList::new_bound(py, [t.getattr("STORY_MOD")?]).as_any()), None, "terra")
}

/// シナリオ（物語）のワールドか
#[pyfunction]
pub fn tut_is_story_world(py: Python<'_>, name: &Bound<'_, PyAny>) -> R<bool> {
    let r = (|| -> R<bool> {
        let p = div(&imp(py, "main.paths")?.call_method1("world_dir", (name,))?, "world.json")?;
        let t = p.call_method("read_text", (), Some(&[("encoding", "utf-8")].into_py_dict_bound(py)))?;
        let s = json(py)?.call_method1("loads", (t,))?;
        Ok(s.call_method1("get", ("story",))?.is_instance_of::<PyDict>())
    })();
    match r {
        Ok(v) => Ok(v),
        Err(e) if e.is_instance_of::<pyo3::exceptions::PyException>(py) => Ok(false),
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------- app：起動

#[pyfunction]
pub fn app_create_empty_world<'py>(py: Python<'py>, name: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    tut_create_world(py, name, Some(PyList::empty_bound(py).as_any()), None, "terra")
}

#[pyfunction]
#[pyo3(signature = (name, new=false))]
pub fn app_prepare_world<'py>(py: Python<'py>, name: &Bound<'py, PyAny>, new: bool) -> R<Bound<'py, PyAny>> {
    let wdir = imp(py, "main.paths")?.call_method1("world_dir", (name,))?;
    if !wdir.call_method0("exists")?.is_truthy()? {
        let t = imp(py, "main.tutorial")?;
        let sc = t.getattr("SCENARIO_WORLD")?;
        if (name.eq("tutorial")? || name.eq(&sc)?) && !new {
            t.call_method1("create_tutorial", (name,))?;
        } else {
            imp(py, "main.app")?.call_method1("create_empty_world", (name,))?;
        }
    }
    imp(py, "main.world")?.getattr("World")
}

/// 物理コア（lib/alpedia_core）があるか。無ければ作り方を出して終わる
#[pyfunction]
pub fn app_check_core(py: Python<'_>) -> R<()> {
    let n = imp(py, "main.native")?;
    if n.call_method0("load")?.is_none() {
        return Err(PyErr::from_value_bound(py.get_type_bound::<pyo3::exceptions::PySystemExit>().call1((n.getattr("reason")?,))?));
    }
    Ok(())
}

/// 画面（lib/alpedia_view）が使えるか
#[pyfunction]
pub fn app_rust_view_ok(py: Python<'_>, lg: &Bound<'_, PyAny>) -> R<bool> {
    let lib = imp(py, "main.native")?.getattr("LIB")?.str()?;
    let path = imp(py, "sys")?.getattr("path")?;
    if !path.contains(&lib)? {
        path.call_method1("insert", (0, &lib))?;
    }
    match imp(py, "importlib")?.call_method1("import_module", ("alpedia_view",)) {
        Ok(_) => Ok(true),
        Err(e) if e.is_instance_of::<pyo3::exceptions::PyImportError>(py) => {
            lg.call_method1("error", ("画面（lib/alpedia_view）がありません（%s）。Rust を入れて native\\build_windows.bat で作ってください", e.value_bound(py)))?;
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

#[pyfunction]
pub fn app_main(py: Python<'_>, args: &Bound<'_, PyAny>) -> R<()> {
    let paths = imp(py, "main.paths")?;
    paths.call_method0("ensure_dirs")?;
    let cfg = imp(py, "main.config")?.call_method0("load")?;
    let lg = imp(py, "main.log")?.call_method1("setup", (cfg.call_method1("get", ("log_level", "INFO"))?,))?;
    imp(py, "main.winperf")?.call_method1("boost", (&cfg,))?;
    app_check_core(py)?;
    let world = args.getattr("world")?;
    let new = match args.getattr("new") { Ok(v) => v.is_truthy()?, Err(_) => false };
    if !args.getattr("headless")?.is_truthy()? {
        if !app_rust_view_ok(py, &lg)? {
            return Err(PyErr::from_value_bound(py.get_type_bound::<pyo3::exceptions::PySystemExit>()
                .call1(("画面（lib/alpedia_view）がありません。native\\build_windows.bat で作ってください",))?));
        }
        lg.call_method1("info", ("Alpedia 起動",))?;
        let run_client = imp(py, "main.client_rs.client")?.getattr("run_client")?;
        if world.is_truthy()? {
            app_prepare_world(py, &world, new)?;
        } else {
            let mut any = false;
            for d in paths.getattr("SAVES")?.call_method0("iterdir")?.iter()? {
                let d = d?;
                if d.call_method0("is_dir")?.is_truthy()? && div(&d, "world.json")?.call_method0("exists")?.is_truthy()? {
                    any = true;
                    break;
                }
            }
            if !any {
                tut_create_tutorial(py, None)?;           // 初回はシナリオのワールドを用意
            }
        }
        run_client.call1((&cfg, &world))?;
        return Ok(());
    }
    let name = if world.is_truthy()? { world.clone() } else { cfg.get_item("game")?.call_method1("get", ("default_world", "scenario"))? };
    lg.call_method1("info", ("Alpedia 起動: ワールド '%s'（ヘッドレス）", &name))?;
    let w = (|| -> R<Bound<'_, PyAny>> {
        let wcls = app_prepare_world(py, &name, new)?;
        wcls.call1((&name, &cfg))?.call_method0("load")
    })();
    let w = match w {
        Ok(w) => w,
        Err(e) => {
            let kw = PyDict::new_bound(py);
            kw.set_item("exc_info", e.value_bound(py))?;
            lg.call_method("exception", ("ワールドの読み込みに失敗しました",), Some(&kw))?;
            return Err(e);
        }
    };
    app_run_headless(py, &w, &cfg, args.getattr("steps")?.extract()?)
}

#[pyfunction]
pub fn app_run_headless(py: Python<'_>, world: &Bound<'_, PyAny>, cfg: &Bound<'_, PyAny>, steps: i64) -> R<()> {
    let lg = imp(py, "main.log")?.call_method1("get", ("headless",))?;
    let rate: f64 = cfg.get_item("physics")?.call_method1("get", ("tick_rate", 60))?.extract()?;
    let dt = 1.0 / rate;
    let t0 = std::time::Instant::now();
    for k in 0..steps {
        world.call_method1("step", (dt,))?;
        if k % 120 == 0 {
            let cs = world.getattr("constructs")?;
            let mut nv = 0i64;
            for c in cs.call_method0("values")?.iter()? {
                nv += c?.getattr("n_vert")?.extract::<i64>()?;
            }
            lg.call_method1("info", ("t=%.1fs 構築物 %d 頂点 %d", world.getattr("time")?, cs.len()?, nv))?;
        }
    }
    let el = t0.elapsed().as_secs_f64();
    lg.call_method1("info", ("%d ステップ %.2f 秒（1ステップ %.1f ms）", steps, el, el / steps.max(1) as f64 * 1000.0))?;
    for c in world.getattr("constructs")?.call_method0("values")?.iter()? {
        let c = c?;
        let v = super::construct::v3(&c.call_method0("velocity")?)?;
        let com = super::construct::v3(&c.call_method0("center_of_mass")?)?;
        let r1 = |x: f64| (x * 10.0).round_ties_even() / 10.0;
        lg.call_method1("info", ("  %s: ボクセル %d, 速度 %.2f m/s, 位置 %s", c.getattr("name")?, c.getattr("n_vox")?,
            super::construct::norm(v), vec![r1(com[0]), r1(com[1]), r1(com[2])]))?;
    }
    Ok(())
}

// ---------------------------------------------------------------- content・チュートリアルの舞台

/// 上方向 up に立てた回転行列（列が ローカル x, y, z のワールド方向）
#[pyfunction]
pub fn content_rot_for<'py>(py: Python<'py>, up: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    use super::construct::{cross, norm, v3};
    let u = v3(up)?;
    let l = norm(u);
    let u = [u[0] / l, u[1] / l, u[2] / l];
    let f = cross([1.0, 0.0, 0.0], u);
    let lf = norm(f);
    let f = [f[0] / lf, f[1] / lf, f[2] / lf];
    let r = cross(u, f);
    super::construct::mat(py, &[[r[0], u[0], f[0]], [r[1], u[1], f[1]], [r[2], u[2], f[2]]])
}

fn d<'py>(py: Python<'py>, kv: &[(&str, PyObject)]) -> R<Bound<'py, PyDict>> {
    let o = PyDict::new_bound(py);
    for (k, v) in kv {
        o.set_item(*k, v)?;
    }
    Ok(o)
}
fn vox<'py>(py: Python<'py>, c: [i64; 3], m: &str, dev: Option<Bound<'py, PyDict>>) -> Bound<'py, PyTuple> {
    let dv: PyObject = match dev { Some(x) => x.into_any().unbind(), None => py.None() };
    PyTuple::new_bound(py, [PyTuple::new_bound(py, c).into_any().unbind(), m.into_py(py), dv])
}
fn dev<'py>(py: Python<'py>, def: &str, dir: i64, name: Option<&str>, state: Option<Bound<'py, PyDict>>) -> R<Bound<'py, PyDict>> {
    let o = PyDict::new_bound(py);
    o.set_item("def", def)?;
    o.set_item("dir", dir)?;
    if let Some(n) = name {
        o.set_item("name", n)?;
    }
    if let Some(s) = state {
        o.set_item("state", s)?;
    }
    Ok(o)
}

fn devval(py: Python<'_>, prop: &str) -> R<PyObject> {
    Ok(d(py, &[("type", "devval".into_py(py)), ("dev", "姿勢モーター".into_py(py)), ("prop", prop.into_py(py))])?.into_any().unbind())
}
fn num(py: Python<'_>, v: f64) -> R<PyObject> {
    let n: PyObject = if v.fract() == 0.0 { (v as i64).into_py(py) } else { v.into_py(py) };
    Ok(d(py, &[("type", "num".into_py(py)), ("v", n)])?.into_any().unbind())
}
fn new_stmt<'py>(py: Python<'py>, kind: &str) -> R<Bound<'py, PyAny>> {
    imp(py, "main.blocks")?.call_method1("new_stmt", (kind,))
}
fn display_stmt<'py>(py: Python<'py>, prefix: &str) -> R<Bound<'py, PyAny>> {
    let s = new_stmt(py, "display")?;
    let text = d(py, &[("type", "join".into_py(py)), ("a", d(py, &[("type", "text".into_py(py)), ("v", prefix.into_py(py))])?.into_any().unbind()),
        ("b", d(py, &[("type", "func".into_py(py)), ("fn", "四捨五入".into_py(py)), ("a", devval(py, "出力の向き")?)])?.into_any().unbind())])?;
    s.set_item("text", text)?;
    s.set_item("width", d(py, &[("type", "num".into_py(py)), ("v", 1.3.into_py(py))])?)?;
    Ok(s)
}
fn start_with_target<'py>(py: Python<'py>) -> R<Bound<'py, PyAny>> {
    let st = new_stmt(py, "on_start")?;
    let a = new_stmt(py, "set_var")?;
    a.set_item("var", "目標")?;
    a.set_item("value", num(py, 0.0)?)?;
    st.get_item("body")?.call_method1("append", (a,))?;
    Ok(st)
}

/// 課題：棒の向きを表示するだけ
#[pyfunction]
pub fn rod_starter_program(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    let p = imp(py, "main.blocks")?.call_method0("new_program")?;
    let st = start_with_target(py)?;
    let tk = new_stmt(py, "on_tick")?;
    tk.get_item("body")?.call_method1("append", (display_stmt(py, "課題  棒の向き ")?,))?;
    p.get_item("scripts")?.call_method1("extend", (vec![st, tk],))?;
    Ok(p)
}

/// お手本：誤差のぶんだけ姿勢モーターを回す
#[pyfunction]
pub fn rod_solution_program(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    let p = imp(py, "main.blocks")?.call_method0("new_program")?;
    let st = start_with_target(py)?;
    let tk = new_stmt(py, "on_tick")?;
    let e = new_stmt(py, "set_var")?;
    e.set_item("var", "誤差")?;
    e.set_item("value", d(py, &[("type", "op".into_py(py)), ("op", "-".into_py(py)),
        ("a", d(py, &[("type", "var".into_py(py)), ("name", "目標".into_py(py))])?.into_any().unbind()), ("b", devval(py, "出力の向き")?)])?)?;
    let r = new_stmt(py, "act_rotate_to")?;
    r.set_item("dev", "姿勢モーター")?;
    r.set_item("angle", d(py, &[("type", "op".into_py(py)), ("op", "+".into_py(py)), ("a", devval(py, "角度")?),
        ("b", d(py, &[("type", "var".into_py(py)), ("name", "誤差".into_py(py))])?.into_any().unbind())])?)?;
    r.set_item("speed", num(py, 180.0)?)?;
    tk.get_item("body")?.call_method1("extend", (vec![e, r, display_stmt(py, "お手本  棒の向き ")?],))?;
    p.get_item("scripts")?.call_method1("extend", (vec![st, tk],))?;
    Ok(p)
}

#[pyfunction]
pub fn rod_apparatus<'py>(py: Python<'py>, ctrl_name: &str, blocks_file: &Bound<'py, PyAny>, script_file: &Bound<'py, PyAny>) -> R<Bound<'py, PyList>> {
    let v = PyList::empty_bound(py);
    for x in 0..5 {
        for z in 0..3 {
            v.append(vox(py, [x, 0, z], "rock", None))?;
        }
    }
    v.append(vox(py, [2, 1, 0], "steel", Some(dev(py, "battery", 2, None, None)?)))?;
    v.append(vox(py, [2, 2, 0], "steel", Some(dev(py, "actuator", 4, Some("揺らしモーター"), None)?)))?;
    v.append(vox(py, [0, 1, 0], "steel", Some(dev(py, "computer", 5, Some("揺らし役"),
        Some(d(py, &[("script", "sway.py".into_py(py)), ("running", true.into_py(py))])?))?)))?;
    v.append(vox(py, [4, 1, 0], "steel", Some(dev(py, "computer", 5, Some(ctrl_name),
        Some(d(py, &[("script", script_file.clone().unbind()), ("blocks", blocks_file.clone().unbind()), ("running", true.into_py(py))])?))?)))?;
    v.append(vox(py, [4, 1, 2], "steel", Some(dev(py, "seat", 2, Some("操縦席"), None)?)))?;
    // 揺れる台（揺らしモーターの出力）
    for x in [1, 2, 3] {
        v.append(vox(py, [x, 2, 1], "aluminum", None))?;
    }
    // 姿勢モーター（台の上、出力は +Z）と棒
    let preset = d(py, &[("action", "rotate".into_py(py)), ("amount", 15.0.into_py(py)), ("speed", 45.0.into_py(py))])?;
    v.append(vox(py, [2, 3, 1], "steel", Some(dev(py, "actuator", 4, Some("姿勢モーター"), Some(d(py, &[("preset", preset.into_any().unbind())])?))?)))?;
    for y in 3..8 {
        v.append(vox(py, [2, y, 2], "wood", None))?;           // 軽い木の棒（速く振っても折れにくい）
    }
    v.append(vox(py, [2, 8, 2], "rubber", None))?;
    Ok(v)
}

/// 3×3×3 のドローン。下の段の四隅が推進機、その真上にバルブ、真ん中の段の十字に燃料タンク・バッテリー・コンピューター、上にアンテナ
#[pyfunction]
pub fn drone_voxels(py: Python<'_>) -> R<Bound<'_, PyList>> {
    let v = PyList::empty_bound(py);
    let corners = [((0, 0), "後左"), ((2, 0), "後右"), ((0, 2), "前左"), ((2, 2), "前右")];     // 前 = +Z、右 = +X
    for x in 0..3 {
        for z in 0..3 {
            if let Some((_, nm)) = corners.iter().find(|c| c.0 == (x, z)) {
                v.append(vox(py, [x, 0, z], "steel", Some(dev(py, "thruster", 2, Some(&format!("推進機 {nm}")), None)?)))?;
                v.append(vox(py, [x, 1, z], "steel", Some(dev(py, "valve", 2, Some(&format!("バルブ {nm}")),
                    Some(d(py, &[("open", 0.0.into_py(py))])?))?)))?;
            } else {
                v.append(vox(py, [x, 0, z], "aluminum", None))?;
            }
        }
    }
    let fuel = || -> R<Bound<'_, PyDict>> {
        let c = d(py, &[("fuel", 800.0.into_py(py))])?;
        d(py, &[("contents", c.into_any().unbind())])
    };
    let mid: Vec<((i64, i64), Bound<'_, PyDict>)> = vec![
        ((1, 1), dev(py, "tank", 2, Some("燃料タンク"), Some(fuel()?))?),
        ((1, 0), dev(py, "tank", 2, Some("燃料タンク"), Some(fuel()?))?),
        ((1, 2), dev(py, "tank", 2, Some("燃料タンク"), Some(fuel()?))?),
        ((0, 1), dev(py, "battery", 2, None, None)?),
        ((2, 1), dev(py, "computer", 0, Some("ドローン制御"), Some(d(py, &[("script", "drone.py".into_py(py)), ("running", true.into_py(py))])?))?),
    ];
    for ((x, z), dv) in mid {
        v.append(vox(py, [x, 1, z], "steel", Some(dv)))?;
    }
    v.append(vox(py, [1, 2, 1], "aluminum", Some(dev(py, "antenna", 2, Some("アンテナ"), None)?)))?;
    Ok(v)
}

/// 操縦台：岩の台の上に操縦席（前 = +Z）、正面の台の上にコンピューターの画面、横にアンテナとバッテリー
#[pyfunction]
pub fn drone_remote_voxels(py: Python<'_>) -> R<Bound<'_, PyList>> {
    let v = PyList::empty_bound(py);
    for x in 0..3 {
        for z in 0..3 {
            v.append(vox(py, [x, 0, z], "rock", None))?;
        }
    }
    v.append(vox(py, [1, 1, 0], "steel", Some(dev(py, "seat", 4, Some("操縦席"), None)?)))?;
    v.append(vox(py, [1, 1, 2], "aluminum", None))?;           // 画面の台
    v.append(vox(py, [1, 2, 2], "steel", Some(dev(py, "computer", 5, Some("送信機"),
        Some(d(py, &[("script", "drone_remote.py".into_py(py)), ("running", true.into_py(py))])?))?)))?;
    v.append(vox(py, [0, 1, 2], "steel", Some(dev(py, "battery", 2, None, None)?)))?;
    v.append(vox(py, [2, 1, 2], "aluminum", Some(dev(py, "antenna", 2, Some("アンテナ"), None)?)))?;
    Ok(v)
}

// ---------------------------------------------------------------- geom：向き付きの箱（配列の形をそろえて Rust の計算へ）

fn f64r<'py>(py: Python<'py>, a: &Bound<'py, PyAny>, shape: &[i64]) -> R<Bound<'py, PyAny>> {
    imp(py, "numpy")?.call_method1("ascontiguousarray", (a, "float64"))?.call_method1("reshape", (PyTuple::new_bound(py, shape),))
}
/// P (N,3) と箱 (B) → 一番近い箱までの距離 d (N,)・外向きの向き n (N,3)・箱の番号 k (N,)
#[pyfunction]
pub fn geom_box_sdf<'py>(py: Python<'py>, p: &Bound<'py, PyAny>, c: &Bound<'py, PyAny>, a: &Bound<'py, PyAny>, e: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    imp(py, "alpedia_core")?.call_method1("box_sdf", (f64r(py, p, &[-1, 3])?, f64r(py, c, &[-1, 3])?, f64r(py, a, &[-1, 3, 3])?, f64r(py, e, &[-1, 3])?))
}
/// レイと箱の交差：(t, 箱の番号, 法線) または None
#[pyfunction]
pub fn geom_ray_boxes<'py>(py: Python<'py>, o: &Bound<'py, PyAny>, dvec: &Bound<'py, PyAny>, c: &Bound<'py, PyAny>, a: &Bound<'py, PyAny>,
                           e: &Bound<'py, PyAny>, max_t: f64) -> R<Bound<'py, PyAny>> {
    if c.len()? == 0 {
        return Ok(py.None().into_bound(py));
    }
    let r = imp(py, "alpedia_core")?.call_method1("ray_boxes", (super::construct::v3(o)?.to_vec(), super::construct::v3(dvec)?.to_vec(),
        f64r(py, c, &[-1, 3])?, f64r(py, a, &[-1, 3, 3])?, f64r(py, e, &[-1, 3])?, max_t))?;
    if r.is_none() {
        return Ok(r);
    }
    Ok(PyTuple::new_bound(py, [r.get_item(0)?, r.get_item(1)?.call_method0("__int__")?, imp(py, "numpy")?.call_method1("asarray", (r.get_item(2)?,))?]).into_any())
}

// ---------------------------------------------------------------- winperf：Windows で速く動かす設定

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    #[repr(C)]
    struct Ppts {
        version: u32,
        control_mask: u32,
        state_mask: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn SetPriorityClass(h: *mut c_void, class: u32) -> i32;
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(m: *mut c_void, name: *const u8) -> *mut c_void;
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
    pub fn boost() -> Vec<&'static str> {
        let mut done = vec![];
        unsafe {
            let proc_ = GetCurrentProcess();
            let k32 = GetModuleHandleW(wide("kernel32.dll").as_ptr());
            if !k32.is_null() {
                let f = GetProcAddress(k32, b"SetProcessInformation\0".as_ptr());
                if !f.is_null() {
                    type F = unsafe extern "system" fn(*mut c_void, i32, *mut c_void, u32) -> i32;
                    let f: F = std::mem::transmute(f);
                    let mut st = Ppts { version: 1, control_mask: 0x1 | 0x4, state_mask: 0 };
                    if f(proc_, 4, &mut st as *mut Ppts as *mut c_void, std::mem::size_of::<Ppts>() as u32) != 0 {   // 4 = ProcessPowerThrottling
                        done.push("省電力の速度制限を解除");
                    }
                }
            }
            if SetPriorityClass(proc_, 0x8000) != 0 {               // ABOVE_NORMAL_PRIORITY_CLASS
                done.push("優先度：通常以上");
            }
            let mm = LoadLibraryW(wide("winmm.dll").as_ptr());
            if !mm.is_null() {
                let f = GetProcAddress(mm, b"timeBeginPeriod\0".as_ptr());
                if !f.is_null() {
                    type F = unsafe extern "system" fn(u32) -> u32;
                    let f: F = std::mem::transmute(f);
                    if f(1) == 0 {
                        done.push("タイマー 1 ms");
                    }
                }
            }
        }
        done
    }
}

/// Windows で速く動かす設定（起動時に1回）：省電力の速度制限を切る・優先度を通常以上・タイマー 1 ms。Windows 以外では何もしない
#[pyfunction]
#[pyo3(signature = (cfg=None))]
pub fn winperf_boost(py: Python<'_>, cfg: Option<&Bound<'_, PyAny>>) -> R<Vec<&'static str>> {
    if let Some(c) = cfg.filter(|c| !c.is_none()) {
        let p = c.call_method1("get", ("performance", PyDict::new_bound(py)))?;
        if !p.call_method1("get", ("windows_boost", true))?.is_truthy()? {
            return Ok(vec![]);
        }
    }
    #[cfg(windows)]
    let done = win::boost();
    #[cfg(not(windows))]
    let done: Vec<&'static str> = vec![];
    if !done.is_empty() {
        imp(py, "main.log")?.call_method1("get", ("winperf",))?.call_method1("info", ("Windows の設定: %s", done.join("・")))?;
    }
    Ok(done)
}
