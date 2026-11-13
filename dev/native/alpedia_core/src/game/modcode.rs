//! MOD のプログラム（mods/<MOD名>/code/）を読み込む（main/modcode.py）。
//! プログラムは alpedia_mods.<MOD名> というパッケージとして読み込む。ワールドを始めるときに一括で読み込み、
//! エラーがあればまとめて出してワールドは始めない。画面用のモジュールは画面を作るときまで後回しにする。

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{IntoPyDict, PyDict, PyList, PyTuple};

use super::imp;

type R<T> = PyResult<T>;

const PREFIX: &str = "alpedia_mods";
const CLIENT_LIBS: [&str; 4] = ["ursina", "panda3d", "direct", "main.client"];

fn log(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    imp(py, "main.log")?.call_method1("get", ("modcode",))
}
fn mods_dir(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    imp(py, "main.paths")?.getattr("MODS")
}
fn div<'py>(p: &Bound<'py, PyAny>, s: impl IntoPy<PyObject>) -> R<Bound<'py, PyAny>> {
    p.call_method1("__truediv__", (s,))
}
fn sys_modules(py: Python<'_>) -> R<Bound<'_, PyDict>> {
    Ok(imp(py, "sys")?.getattr("modules")?.downcast_into::<PyDict>()?)
}

/// MOD（定義・プログラム・テクスチャ）や、ワールドが使っている MOD の中身に問題があり、ワールドを始められない
#[pyclass(extends = PyException, module = "alpedia_core", subclass)]
pub struct ModLoadError {
    #[pyo3(get)]
    errors: PyObject,
}

#[pymethods]
impl ModLoadError {
    #[new]
    #[pyo3(signature = (errors, *_a))]
    fn new(py: Python<'_>, errors: &Bound<'_, PyAny>, _a: &Bound<'_, PyTuple>) -> R<Self> {
        Ok(ModLoadError { errors: PyList::new_bound(py, errors.iter()?.collect::<R<Vec<_>>>()?).into_any().unbind() })
    }
    fn __str__(&self, py: Python<'_>) -> R<String> {
        let v: Vec<String> = self.errors.bind(py).iter()?.map(|x| Ok(x?.str()?.to_string())).collect::<R<_>>()?;
        Ok(format!("MOD の読み込みでエラーがあります:\n{}", v.join("\n")))
    }
}

static DEFERRED: GILOnceCell<Py<PyList>> = GILOnceCell::new();

/// 画面が無いと読み込めないモジュール（画面を作るときに読み込む）：[(MOD, モジュール名, 相対パス, code/)]
#[pyfunction]
pub fn modcode_deferred(py: Python<'_>) -> Bound<'_, PyList> {
    DEFERRED.get_or_init(py, || PyList::empty_bound(py).unbind()).bind(py).clone()
}

/// 読み込み済みの MOD のプログラムを捨てる（次のワールドを始めるときに読み直すため）
#[pyfunction]
pub fn modcode_reset(py: Python<'_>) -> R<()> {
    let mods = sys_modules(py)?;
    let pre = format!("{PREFIX}.");
    let names: Vec<String> = mods.keys().iter().filter_map(|k| k.extract::<String>().ok())
        .filter(|n| n == PREFIX || n.starts_with(&pre)).collect();
    for n in names {
        mods.del_item(n)?;
    }
    imp(py, "main.extensions")?.call_method0("reset")?;      // MOD が足した機器のふるまい・ブロック（読み込むときにもう一度登録される）
    Ok(())
}

/// 例外が起きた MOD の中の場所（ファイル名と行）。文法の誤りはメッセージに場所が入っているので付けない
#[pyfunction]
pub fn modcode_where(py: Python<'_>, e: &Bound<'_, PyAny>, root: &Bound<'_, PyAny>) -> R<String> {
    if e.is_instance(&py.get_type_bound::<pyo3::exceptions::PySyntaxError>())? {
        return Ok(String::new());
    }
    let tb = imp(py, "traceback")?.call_method1("extract_tb", (e.getattr("__traceback__")?,))?;
    let frames: Vec<Bound<'_, PyAny>> = tb.iter()?.collect::<R<_>>()?;
    let rr = root.call_method0("resolve")?;
    for fr in frames.iter().rev() {
        let rel = (|| -> R<Bound<'_, PyAny>> {
            let p = imp(py, "main.paths")?.getattr("Path")?.call1((fr.getattr("filename")?,))?.call_method0("resolve")?;
            p.call_method1("relative_to", (&rr,))
        })();
        match rel {
            Ok(r) => return Ok(format!("（{} {} 行目）", r.str()?, fr.getattr("lineno")?.str()?)),
            Err(err) if err.is_instance_of::<pyo3::exceptions::PyValueError>(py) || err.is_instance_of::<pyo3::exceptions::PyOSError>(py) => continue,
            Err(err) => return Err(err),
        }
    }
    Ok(String::new())
}

fn err_text(py: Python<'_>, e: &PyErr) -> R<String> {
    let v = e.value_bound(py);
    Ok(format!("{}: {}", v.get_type().name()?, v.str()?))
}

/// 画面用のライブラリ（ursina・panda3d・main.client）が無くて読み込めなかったか
fn needs_client(py: Python<'_>, e: &PyErr) -> R<bool> {
    if !e.is_instance_of::<pyo3::exceptions::PyImportError>(py) {
        return Ok(false);
    }
    let v = e.value_bound(py);
    let name = match v.getattr("name") { Ok(n) if !n.is_none() => n.str()?.to_string(), _ => String::new() };
    let txt = format!("{name} {}", v.str()?);
    Ok(CLIENT_LIBS.iter().any(|lib| txt.contains(lib)))
}

/// 画面のライブラリが読み込める状態か（ゲームの画面から起動しているか）
#[pyfunction]
pub fn modcode_client_ready(py: Python<'_>) -> R<bool> {
    sys_modules(py)?.contains("ursina")
}

/// 画面の無い所で MOD を読み込む間、画面用のライブラリを読み込ませない（そのモジュールは後回しになる）
#[pyclass(module = "alpedia_core")]
pub struct ClientGuard;

#[pymethods]
impl ClientGuard {
    #[new]
    fn new() -> Self {
        ClientGuard
    }
    #[pyo3(signature = (name, path=None, target=None))]
    fn find_spec(&self, py: Python<'_>, name: &str, path: Option<PyObject>, target: Option<PyObject>) -> R<PyObject> {
        let _ = (path, target);
        if CLIENT_LIBS.iter().any(|b| name == *b || name.starts_with(&format!("{b}."))) {
            let e = py.get_type_bound::<pyo3::exceptions::PyImportError>().call(
                (format!("画面用のライブラリ {name} は画面を作るときに読み込みます"),), Some(&[("name", name)].into_py_dict_bound(py)))?;
            return Err(PyErr::from_value_bound(e));
        }
        Ok(py.None())
    }
}

fn log_exc(py: Python<'_>, e: PyErr, msg: &str, args: impl IntoPy<Py<PyTuple>>) -> R<()> {
    let a: Py<PyTuple> = args.into_py(py);
    let mut v: Vec<PyObject> = vec![msg.into_py(py)];
    v.extend(a.bind(py).iter().map(|x| x.unbind()));
    let kw = PyDict::new_bound(py);
    kw.set_item("exc_info", e.value_bound(py))?;
    log(py)?.call_method("exception", PyTuple::new_bound(py, v), Some(&kw))?;
    Ok(())
}

/// MOD のプログラムを全部読み込む（code/ の中の .py をすべて import）。エラーは errors に足す
#[pyfunction]
pub fn modcode_load(py: Python<'_>, mod_ids: &Bound<'_, PyAny>, errors: &Bound<'_, PyAny>) -> R<()> {
    modcode_reset(py)?;
    let def = modcode_deferred(py);
    def.call_method0("clear")?;
    let guard = if modcode_client_ready(py)? { None } else { Some(Bound::new(py, ClientGuard)?) };
    let mp = imp(py, "sys")?.getattr("meta_path")?;
    if let Some(g) = &guard {
        mp.call_method1("insert", (0, g))?;
    }
    let r = load_inner(py, mod_ids, errors);
    if let Some(g) = &guard {
        mp.call_method1("remove", (g,))?;
    }
    r
}

fn load_inner(py: Python<'_>, mod_ids: &Bound<'_, PyAny>, errors: &Bound<'_, PyAny>) -> R<()> {
    let importlib = imp(py, "importlib")?;
    for mid in mod_ids.iter()? {
        let mid = mid?;
        let ms: String = mid.str()?.to_string();
        let d = div(&div(&mods_dir(py)?, &mid)?, "code")?;
        if !div(&d, "__init__.py")?.call_method0("exists")?.is_truthy()? {
            continue;
        }
        if let Err(e) = modcode_package(py, &ms) {
            if !e.is_instance_of::<PyException>(py) {
                return Err(e);
            }
            let msg = format!("MOD '{ms}' のプログラムを読み込めません: {}{}", err_text(py, &e)?, modcode_where(py, e.value_bound(py).as_any(), &d)?);
            log_exc(py, e, "MOD '%s' のプログラムを読み込めません", (&mid,))?;
            errors.call_method1("append", (msg,))?;
            continue;
        }
        let files = imp(py, "builtins")?.call_method1("sorted", (d.call_method1("rglob", ("*.py",))?,))?;
        for f in files.iter()? {
            let f = f?;
            let rel = f.call_method1("relative_to", (&d,))?.call_method1("with_suffix", ("",))?;
            let rparts: Vec<String> = rel.getattr("parts")?.extract()?;
            let parts: Vec<&String> = rparts.iter().filter(|p| p.as_str() != "__init__").collect();
            if parts.is_empty() || rparts.iter().any(|p| p == "__pycache__") {
                continue;
            }
            let name = format!("{PREFIX}.{ms}.{}", parts.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("."));
            if let Err(e) = importlib.call_method1("import_module", (&name,)) {
                if !e.is_instance_of::<PyException>(py) {
                    return Err(e);
                }
                if needs_client(py, &e)? && !modcode_client_ready(py)? {
                    modcode_deferred(py).append((&mid, &name, &rel, &d))?;
                    continue;
                }
                let msg = format!("MOD '{ms}' の {}.py を読み込めません: {}{}", rel.call_method0("as_posix")?.str()?, err_text(py, &e)?,
                    modcode_where(py, e.value_bound(py).as_any(), &d)?);
                log_exc(py, e, "MOD '%s' の %s を読み込めません", (&mid, f.getattr("name")?))?;
                errors.call_method1("append", (msg,))?;
            }
        }
    }
    Ok(())
}

/// 後回しにした画面用のモジュールを読み込む（画面を作るとき。プレイ中には読みに行かない）
#[pyfunction]
pub fn modcode_load_client(py: Python<'_>, errors: &Bound<'_, PyAny>) -> R<()> {
    let def = modcode_deferred(py);
    let items: Vec<Bound<'_, PyAny>> = def.iter().collect();
    for it in items {
        let (mid, name, rel, d) = (it.get_item(0)?, it.get_item(1)?, it.get_item(2)?, it.get_item(3)?);
        if let Err(e) = imp(py, "importlib")?.call_method1("import_module", (&name,)) {
            if !e.is_instance_of::<PyException>(py) {
                return Err(e);
            }
            let msg = format!("MOD '{}' の {}.py を読み込めません: {}{}", mid.str()?, rel.call_method0("as_posix")?.str()?, err_text(py, &e)?,
                modcode_where(py, e.value_bound(py).as_any(), &d)?);
            log_exc(py, e, "MOD '%s' の %s を読み込めません", (&mid, &name))?;
            errors.call_method1("append", (msg,))?;
        }
    }
    def.call_method0("clear")?;
    Ok(())
}

/// MOD のプログラムのパッケージ（code/ が無い MOD は None）
#[pyfunction]
#[pyo3(name = "modcode_package")]
pub fn modcode_package_py(py: Python<'_>, mod_id: &str) -> R<PyObject> {
    modcode_package(py, mod_id)
}
pub fn modcode_package(py: Python<'_>, mod_id: &str) -> R<PyObject> {
    let d = div(&div(&mods_dir(py)?, mod_id)?, "code")?;
    let init = div(&d, "__init__.py")?;
    if !init.call_method0("exists")?.is_truthy()? {
        return Ok(py.None());
    }
    let name = format!("{PREFIX}.{mod_id}");
    let mods = sys_modules(py)?;
    if let Some(m) = mods.get_item(&name)? {
        return Ok(m.unbind());
    }
    if !mods.contains(PREFIX)? {
        let root = imp(py, "types")?.getattr("ModuleType")?.call1((PREFIX,))?;
        root.setattr("__path__", PyList::empty_bound(py))?;
        mods.set_item(PREFIX, root)?;
    }
    let util = imp(py, "importlib.util")?;
    let kw = [("submodule_search_locations", PyList::new_bound(py, [d.str()?]))].into_py_dict_bound(py);
    let spec = util.call_method("spec_from_file_location", (&name, &init), Some(&kw))?;
    let m = util.call_method1("module_from_spec", (&spec,))?;
    mods.set_item(&name, &m)?;
    if let Err(e) = spec.getattr("loader")?.call_method1("exec_module", (&m,)) {
        mods.call_method1("pop", (&name, py.None()))?;
        return Err(e);
    }
    Ok(m.unbind())
}

/// MOD のワールド側の拡張を作る（プログラムを持たない MOD は飛ばす）
#[pyfunction]
#[pyo3(signature = (mod_ids, errors=None))]
pub fn modcode_world_addons<'py>(py: Python<'py>, mod_ids: &Bound<'py, PyAny>, errors: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyList>> {
    let out = PyList::empty_bound(py);
    for mid in mod_ids.iter()? {
        let mid = mid?;
        let ms: String = mid.str()?.to_string();
        let r = (|| -> R<()> {
            let pkg = modcode_package(py, &ms)?.into_bound(py);
            if pkg.is_none() || !pkg.hasattr("world_addon")? {
                return Ok(());
            }
            let a = pkg.call_method0("world_addon")?;
            a.setattr("mod_id", &mid)?;
            out.append(&a)?;
            log(py)?.call_method1("info", ("MOD のプログラムを読み込み: %s（%s）", &mid, a.get_type().name()?))?;
            Ok(())
        })();
        if let Err(e) = r {
            if !e.is_instance_of::<PyException>(py) {
                return Err(e);
            }
            let wh = modcode_where(py, e.value_bound(py).as_any(), &div(&div(&mods_dir(py)?, &mid)?, "code")?)?;
            let msg = format!("MOD '{ms}' のプログラムを読み込めません: {}{wh}", err_text(py, &e)?);
            log_exc(py, e, "MOD '%s' のプログラムを読み込めません", (&mid,))?;
            if let Some(er) = errors.filter(|x| !x.is_none()) {
                er.call_method1("append", (msg,))?;
            }
        }
    }
    Ok(out)
}

/// ワールドに入っている拡張に合わせて、画面側の拡張を作る。errors があればエラーをそこに足す
#[pyfunction]
#[pyo3(signature = (game, errors=None))]
pub fn modcode_client_addons<'py>(py: Python<'py>, game: &Bound<'py, PyAny>, errors: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyList>> {
    let out = PyList::empty_bound(py);
    let mods = sys_modules(py)?;
    for a in game.getattr("world")?.getattr("addons")?.iter()? {
        let a = a?;
        let mid = match a.getattr("mod_id") { Ok(m) if m.is_truthy()? => m, _ => continue };
        let pkg = match mods.get_item(format!("{PREFIX}.{}", mid.str()?))? { Some(p) => p, None => continue };
        if !pkg.hasattr("client_addon")? {
            continue;
        }
        match pkg.call_method1("client_addon", (game,)) {
            Ok(c) => {
                if !c.is_none() {
                    out.append(c)?;
                }
            }
            Err(e) if e.is_instance_of::<PyException>(py) => {
                let wh = modcode_where(py, e.value_bound(py).as_any(), &div(&div(&mods_dir(py)?, &mid)?, "code")?)?;
                let msg = format!("MOD '{}' の画面を作れません: {}{wh}", mid.str()?, err_text(py, &e)?);
                log_exc(py, e, "MOD '%s' の画面を作れません", (&mid,))?;
                match errors.filter(|x| !x.is_none()) {
                    Some(er) => {
                        er.call_method1("append", (msg,))?;
                    }
                    None => {
                        game.getattr("world")?.call_method1("message", (format!("⚠ MOD '{}' の画面を作れませんでした（logs/latest.log）", mid.str()?),))?;
                    }
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}
