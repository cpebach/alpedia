//! コンピューター（Python スクリプト）の画面（main/client/script_panel.py）：ファイルの選択・実行・停止・エラーと出力の表示。
//!
//! スクリプトの編集はゲームの外のエディタ（メモ帳・VS Code など）で行う。
//! 「エディタで開く」でファイルを開き、保存するとゲームが自動で読み込み直す（実行中でも次の tick から新しいコード）。

use crate::game::PyPathOps;
use crate::game::PyDictOps;
use crate::game::construct::{con, Construct};
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::imp;
use super::panels::{hook, kw, partial, po, ursina};
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

fn lg(py: Python<'_>) -> R<B<'_>> {
    imp(py, "main.log")?.dget1("script_panel")
}
fn sp(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.script_panel")?.into_any())
}
fn utf8(py: Python<'_>) -> R<Bound<'_, PyDict>> {
    let k = PyDict::new_bound(py);
    k.set_item("encoding", "utf-8")?;
    Ok(k)
}
fn is_win(py: Python<'_>) -> R<bool> {
    let p: String = imp(py, "sys")?.getattr("platform")?.extract()?;
    Ok(p.starts_with("win"))
}
fn platform(py: Python<'_>) -> R<String> {
    imp(py, "sys")?.getattr("platform")?.extract()
}
fn popen<'py>(py: Python<'py>, args: B<'py>, flags: Option<B<'py>>) -> R<()> {
    let sub = imp(py, "subprocess")?;
    match flags {
        Some(f) => {
            let k = PyDict::new_bound(py);
            k.set_item("creationflags", f)?;
            sub.getattr("Popen")?.call((args,), Some(&k))?;
        }
        None => {
            sub.getattr("Popen")?.call1((args,))?;
        }
    }
    Ok(())
}

/// OS の既定のアプリでファイル・フォルダを開く
#[pyfunction]
pub fn sp_open_external(py: Python<'_>, path: &B<'_>) -> R<()> {
    let p = path.str()?.to_string();
    if is_win(py)? {
        imp(py, "os")?.call_method1("startfile", (p,))?;
    } else if platform(py)? == "darwin" {
        popen(py, PyList::new_bound(py, ["open", p.as_str()]).into_any(), None)?;
    } else {
        popen(py, PyList::new_bound(py, ["xdg-open", p.as_str()]).into_any(), None)?;
    }
    Ok(())
}

/// Windows で使えるエディタ（見つかった順）
#[pyfunction]
pub fn sp_win_editors(py: Python<'_>) -> R<Vec<Vec<String>>> {
    let which = imp(py, "shutil")?.getattr("which")?;
    let mut out: Vec<Vec<String>> = vec![];
    for exe in ["code.cmd", "code"] {
        let w = which.call1((exe,))?;
        if w.is_truthy()? {
            out.push(vec![w.extract()?]);
            break;
        }
    }
    let local: String = imp(py, "os")?.getattr("environ")?.dget2("LOCALAPPDATA", "")?.extract()?;
    let vs: String = imp(py, "os.path")?.call_method1("join", (local.as_str(), "Programs", "Microsoft VS Code", "Code.exe"))?.extract()?;
    if out.is_empty() && !local.is_empty() && imp(py, "os.path")?.call_method1("exists", (vs.as_str(),))?.is_truthy()? {
        out.push(vec![vs]);
    }
    out.push(vec!["notepad.exe".into()]);
    Ok(out)
}

/// テキストエディタでファイルを開く。
/// Windows では .py を既定のアプリで「開く」と Python で実行されてしまうので、エディタを指定して開く。
/// command（設定 game.editor）があればそれを使い、無ければ VS Code、無ければメモ帳
#[pyfunction]
#[pyo3(signature = (path, command=""))]
pub fn sp_open_in_editor(py: Python<'_>, path: &B<'_>, command: &str) -> R<()> {
    let p = path.str()?.to_string();
    let win = is_win(py)?;
    let sub = imp(py, "subprocess")?;
    let flags = if win { sub.getattr("CREATE_NO_WINDOW").unwrap_or_else(|_| po(py, 0).into_bound(py)) } else { po(py, 0).into_bound(py) };
    if !command.is_empty() {
        let k = PyDict::new_bound(py);
        k.set_item("posix", !win)?;
        let parts: Vec<String> = imp(py, "shlex")?.call_method("split", (command,), Some(&k))?.extract()?;
        let mut args: Vec<String> = parts.iter().map(|a| a.trim_matches('"').to_string()).collect();
        args.push(p);
        popen(py, PyList::new_bound(py, args).into_any(), Some(flags))?;
        return Ok(());
    }
    if win {
        let mut err: Option<PyErr> = None;
        for cmd in sp_win_editors(py)? {
            let mut args = cmd.clone();
            args.push(p.clone());
            match popen(py, PyList::new_bound(py, args).into_any(), Some(flags.clone())) {
                Ok(()) => return Ok(()),
                Err(e) if e.is_instance_of::<pyo3::exceptions::PyOSError>(py) => err = Some(e),
                Err(e) => return Err(e),
            }
        }
        return Err(err.unwrap());
    }
    if platform(py)? == "darwin" {
        popen(py, PyList::new_bound(py, ["open", "-t", p.as_str()]).into_any(), None)
    } else {
        popen(py, PyList::new_bound(py, ["xdg-open", p.as_str()]).into_any(), None)
    }
}

#[pyfunction]
pub fn sp_wrap(s: &B<'_>, n: usize) -> R<String> {
    let s: String = s.str()?.to_string();
    let ch: Vec<char> = s.chars().collect();
    if ch.len() <= n {
        return Ok(s);
    }
    Ok(ch.chunks(n).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("\n  "))
}

#[pyclass(module = "main.client.script_panel", subclass, dict)]
#[allow(non_snake_case)]
pub struct ScriptPanel {
    /// U（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_U: Option<PyObject>,
    /// _status_time（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_status_time: Option<PyObject>,
    /// c（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_c: Option<PyObject>,
    /// cell（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_cell: Option<PyObject>,
    /// game（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_game: Option<PyObject>,
    /// info（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_info: Option<PyObject>,
    /// keys（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_keys: Option<PyObject>,
    /// log_text（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_log_text: Option<PyObject>,
    /// modal（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_modal: Option<PyObject>,
    /// py_btn（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_py_btn: Option<PyObject>,
    /// root（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_root: Option<PyObject>,
    /// status（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_status: Option<PyObject>,
    /// title（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_title: Option<PyObject>,
    /// world（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_world: Option<PyObject>,
}

#[pymethods]
impl ScriptPanel {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        ScriptPanel { a_U: None, a_status_time: None, a_c: None, a_cell: None, a_game: None, a_info: None, a_keys: None, a_log_text: None, a_modal: None, a_py_btn: None, a_root: None, a_status: None, a_title: None, a_world: None }
    }
    fn __init__(slf: &Bound<'_, Self>, game: &B<'_>, construct: &B<'_>, index: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let ur = ursina(py)?;
        let (entity, text, button, camera, color, mouse) = (ur.getattr("Entity")?, ur.getattr("Text")?, ur.getattr("Button")?, ur.getattr("camera")?,
            ur.getattr("color")?, ur.getattr("mouse")?);
        Self::sa_game(slf, game)?;
        Self::sa_world(slf, game.getattr("world")?)?;
        Self::sa_c(slf, construct)?;
        Self::sa_cell(slf, Construct::g_cells(con(&construct)?)?.get_item(index)?)?;
        Self::sa_modal(slf, py.None())?;
        let u = PyDict::new_bound(py);
        u.set_item("Entity", &entity)?;
        u.set_item("Text", &text)?;
        u.set_item("Button", &button)?;
        u.set_item("color", &color)?;
        Self::sa_U(slf, u)?;
        mouse.setattr("locked", false)?;
        let root = entity.call((), Some(&kw(py, vec![("parent", camera.getattr("ui")?.unbind())])?))?;
        Self::sa_root(slf, &root)?;
        let r_ = || root.clone().unbind();
        let rgb = |r: f64, g: f64, b: f64| -> R<PyObject> { Ok(color.call_method1("rgb", (r, g, b))?.unbind()) };
        let lgray = || -> R<PyObject> { Ok(color.getattr("light_gray")?.unbind()) };
        entity.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, (3, 2))), ("color", color.call_method1("rgba", (0.02, 0.03, 0.05, 0.92))?.unbind()),
            ("z", po(py, 0.2))])?))?;
        Self::sa_title(slf, text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (-0.86, 0.475))), ("origin", po(py, (-0.5, 0.5))),
            ("scale", po(py, 1.1))])?))?)?;
        text.call((), Some(&kw(py, vec![("parent", r_()), ("position", po(py, (-0.86, -0.455))), ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.65)), ("color", lgray()?),
            ("text", po(py, "スクリプトはゲームの外のエディタで編集します。保存するとゲームが自動で読み込み直します / Esc 閉じる"))])?))?;
        Self::sa_info(slf, text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (-0.84, 0.40))), ("origin", po(py, (-0.5, 0.5))),
            ("scale", po(py, 0.75))])?))?)?;
        text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "出力（print）")), ("position", po(py, (-0.84, 0.05))), ("origin", po(py, (-0.5, 0.5))), ("scale", po(py, 0.7))])?))?;
        Self::sa_log_text(slf, text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (-0.84, 0.01))), ("origin", po(py, (-0.5, 0.5))),
            ("scale", po(py, 0.62)), ("color", lgray()?)])?))?)?;
        let x = 0.55;
        let btn = |t: &str, pos: (f64, f64), f: B<'_>, w: f64, col: Option<PyObject>| -> R<B<'_>> {
            let b = button.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, t)), ("position", po(py, pos)), ("scale", po(py, (w, 0.045))),
                ("color", match col { Some(c) => c, None => rgb(0.22, 0.32, 0.45)? }), ("text_size", po(py, 0.75))])?))?;
            b.setattr("on_click", f)?;
            Ok(b)
        };
        let m = |n: &str| s.getattr(n);
        btn("このコンピューターの名前を変更", (x, 0.40), m("open_rename")?, 0.42, None)?;
        btn("ブロックで編集（ノードプログラミング）", (x, 0.33), m("open_blocks")?, 0.42, Some(rgb(0.75, 0.5, 0.15)?))?;
        Self::sa_py_btn(slf, btn("Python モードにする", (x, 0.26), m("to_python")?, 0.42, Some(rgb(0.45, 0.35, 0.6)?))?)?;
        btn("◀ 前のファイル", (x - 0.11, 0.17), partial(&m("cycle_file")?, vec![po(py, -1)])?, 0.2, None)?;
        btn("次のファイル ▶", (x + 0.11, 0.17), partial(&m("cycle_file")?, vec![po(py, 1)])?, 0.2, None)?;
        btn("エディタで開く", (x - 0.11, 0.11), m("open_in_editor")?, 0.2, Some(rgb(0.2, 0.45, 0.3)?))?;
        btn("新規ファイル", (x + 0.11, 0.11), m("new_file")?, 0.2, None)?;
        btn("実行", (x - 0.11, 0.05), m("run")?, 0.2, Some(rgb(0.2, 0.5, 0.3)?))?;
        btn("停止", (x + 0.11, 0.05), m("stop")?, 0.2, Some(rgb(0.5, 0.25, 0.2)?))?;
        btn("scripts フォルダを開く", (x - 0.11, -0.01), m("_open_scripts")?, 0.2, None)?;
        btn("API 一覧（README）", (x + 0.11, -0.01), m("_open_readme")?, 0.2, None)?;
        btn("閉じる", (x, -0.08), m("close")?, 0.42, Some(rgb(0.3, 0.3, 0.33)?))?;
        Self::sa_status(slf, text.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("position", po(py, (-0.84, -0.40))), ("origin", po(py, (-0.5, 0.5))),
            ("scale", po(py, 0.75)), ("color", color.getattr("orange")?.unbind())])?))?)?;
        Self::sa_keys(slf, hook(py, &root, Some(m("_on_key")?), Some(m("update")?))?)?;
        Ok(())
    }
    pub(crate) fn _on_key(slf: &Bound<'_, Self>, key: &B<'_>) -> R<()> {
        if Self::ga_modal(slf)?.is_none() && key.eq("escape")? {
            Self::close(slf).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?;
        }
        Ok(())
    }
    pub(crate) fn _open_scripts(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        s.call_method1("_open", (Self::scripts_dir(slf).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?,))?;
        Ok(())
    }
    pub(crate) fn _open_readme(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        s.call_method1("_open", (Self::readme(slf).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?,))?;
        Ok(())
    }
    #[getter]
    fn index<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        Self::ga_c(slf)?.getattr("index")?.dget1(Self::ga_cell(slf)?)
    }
    pub(crate) fn device<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let i = Self::index(slf)?;
        if i.is_none() {
            return Ok(py.None().into_bound(py));
        }
        Self::ga_c(slf)?.getattr("devs")?.get_item(i)
    }
    pub(crate) fn scripts_dir<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let d = Self::ga_world(slf)?.getattr("dir")?.call_method1("__truediv__", ("scripts",))?;
        let k = PyDict::new_bound(py);
        k.set_item("exist_ok", true)?;
        d.call_method("mkdir", (), Some(&k))?;
        Ok(d)
    }
    pub(crate) fn readme<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let txt = imp(py, "main.content")?.getattr("SCRIPT_README")?;
        let p = Self::scripts_dir(slf)?.call_method1("__truediv__", ("README.txt",))?;
        if !p.pexists()?.is_truthy()? || !p.call_method("read_text", (), Some(&utf8(py)?))?.eq(&txt)? {
            // 古い版の説明は書き直す
            p.call_method("write_text", (&txt,), Some(&utf8(py)?))?;
        }
        Ok(p)
    }
    pub(crate) fn set_status(slf: &Bound<'_, Self>, msg: &B<'_>) -> R<()> {
        let py = slf.py();
        Self::ga_status(slf)?.setattr("text", msg)?;
        Self::sa_status_time(slf, imp(py, "time")?.call_method0("time")?)?;
        Ok(())
    }
    #[pyo3(signature = (path, edit=false))]
    pub(crate) fn _open(slf: &Bound<'_, Self>, path: &B<'_>, edit: bool) -> R<()> {
        let py = slf.py();
        let r = if edit {
            let ed = Self::ga_game(slf)?.getattr("cfg")?.dget2("game", PyDict::new_bound(py))?.dget2("editor", "")?;
            let cmd = if ed.is_truthy()? { ed.str()?.to_string() } else { String::new() };
            sp(py)?.call_method1("open_in_editor", (path, cmd)).map(|_| ())
        } else {
            sp(py)?.call_method1("open_external", (path,)).map(|_| ())
        };
        match r {
            Ok(()) => Self::set_status(slf, &po(py, format!("{} を開きました", path.getattr("name")?.str()?)).into_bound(py)),
            Err(e) if e.is_instance_of::<PyException>(py) => {
                lg(py)?.call_method1("warning", ("開けませんでした: %s (%s)", path, e.value_bound(py)))?;
                Self::set_status(slf, &po(py, format!("開けませんでした: {}", path.str()?)).into_bound(py))
            }
            Err(e) => Err(e),
        }
    }
    pub(crate) fn _set_script(slf: &Bound<'_, Self>, name: &B<'_>) -> R<()> {
        let py = slf.py();
        let d = Self::device(slf)?;
        if d.is_none() {
            return Ok(());
        }
        let st = d.get_item("state")?;
        st.set_item("script", name)?;
        for k in ["_env", "error", "error_line"] {
            st.dpop2(k, py.None())?;
        }
        Self::ga_world(slf)?.getattr("runner")?.call_method1("invalidate", (name,))?;
        Ok(())
    }
    pub(crate) fn cycle_file(slf: &Bound<'_, Self>, step: i64) -> R<()> {
        let py = slf.py();
        let mut files: Vec<String> = vec![];
        for p in Self::scripts_dir(slf)?.call_method1("glob", ("*.py",))?.iter()? {
            files.push(p?.getattr("name")?.extract()?);
        }
        files.sort();
        if files.is_empty() {
            return Self::set_status(slf, &po(py, "scripts フォルダに .py がありません（新規ファイルで作れます）").into_bound(py));
        }
        let d = Self::device(slf)?;
        let cur: String = if d.is_truthy()? { d.dget2("state", PyDict::new_bound(py))?.dget2("script", "")?.str()?.to_string() } else { String::new() };
        let i = files.iter().position(|f| *f == cur).map(|x| x as i64).unwrap_or(-1);
        let name = files[(i + step).rem_euclid(files.len() as i64) as usize].clone();
        Self::_set_script(slf, &name.clone().into_py(py).into_bound(py))?;
        Self::set_status(slf, &po(py, format!("{} をこのコンピューターに設定しました", name)).into_bound(py))
    }
    pub(crate) fn new_file(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let d = Self::scripts_dir(slf)?;
        let mut k = 1;
        while d.call_method1("__truediv__", (format!("script{}.py", k),))?.pexists()?.is_truthy()? {
            k += 1;
        }
        let name = format!("script{}.py", k);
        d.call_method1("__truediv__", (&name,))?.call_method("write_text", (sp(py)?.getattr("NEW_TEMPLATE")?,), Some(&utf8(py)?))?;
        Self::_set_script(slf, &name.clone().into_py(py).into_bound(py))?;
        Self::set_status(slf, &po(py, format!("{} を作って設定しました（エディタで開いて編集）", name)).into_bound(py))
    }
    pub(crate) fn open_in_editor(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let d = Self::device(slf)?;
        let mut name = if d.is_truthy()? { d.dget2("state", PyDict::new_bound(py))?.dget2("script", "")? } else { po(py, "").into_bound(py) };
        if !name.is_truthy()? {
            Self::new_file(slf)?;
            name = Self::device(slf)?.get_item("state")?.get_item("script")?;
        }
        let p = match imp(py, "main.scripting")?.call_method1("script_path", (Self::ga_world(slf)?.getattr("dir")?, &name)) {
            Ok(p) => p,
            Err(e) if e.is_instance_of::<pyo3::exceptions::PyValueError>(py) => return Self::set_status(slf, &e.value_bound(py).str()?.into_any()),
            Err(e) => return Err(e),
        };
        if !p.pexists()?.is_truthy()? {
            p.call_method("write_text", (sp(py)?.getattr("NEW_TEMPLATE")?,), Some(&utf8(py)?))?;
        }
        Self::_open(slf, &p, true)
    }
    pub(crate) fn run(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let d = Self::device(slf)?;
        if d.is_none() {
            return Ok(());
        }
        let st = d.get_item("state")?;
        let name = st.dget2("script", "")?;
        let r = (|| -> R<()> {
            let p = imp(py, "main.scripting")?.call_method1("script_path", (Self::ga_world(slf)?.getattr("dir")?, &name))?;
            let src = p.call_method("read_text", (), Some(&utf8(py)?))?;
            py.import_bound("builtins")?.getattr("compile")?.call1((src, &name, "exec"))?;
            Ok(())
        })();
        if let Err(e) = r {
            if e.is_instance_of::<pyo3::exceptions::PySyntaxError>(py) {
                let v = e.value_bound(py);
                let (msg, ln) = (v.getattr("msg")?, v.getattr("lineno")?);
                st.set_item("error", format!("SyntaxError: {}（{} 行目）", msg.str()?, ln.str()?))?;
                st.set_item("error_line", &ln)?;
                return Self::set_status(slf, &po(py, format!("構文エラー {} 行目: {}", ln.str()?, msg.str()?)).into_bound(py));
            }
            if e.is_instance_of::<pyo3::exceptions::PyOSError>(py) || e.is_instance_of::<pyo3::exceptions::PyValueError>(py) {
                return Self::set_status(slf, &po(py, format!("スクリプトを読めません: {}", e.value_bound(py).str()?)).into_bound(py));
            }
            return Err(e);
        }
        for k in ["_env", "error", "error_line"] {
            st.dpop2(k, py.None())?;
        }
        st.set_item("running", true)?;
        Self::set_status(slf, &po(py, format!("{} を実行しました", name.str()?)).into_bound(py))
    }
    pub(crate) fn stop(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let d = Self::device(slf)?;
        if d.is_truthy()? {
            d.get_item("state")?.set_item("running", false)?;
            Self::set_status(slf, &po(py, "停止しました").into_bound(py))?;
        }
        Ok(())
    }
    pub(crate) fn to_python(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let d = Self::device(slf)?;
        if d.is_truthy()? && d.get_item("state")?.dpop2("blocks", py.None())?.is_truthy()? {
            Self::set_status(slf, &po(py, "Python モードにしました（ブロックから作ったコードを、そのまま Python として使います）").into_bound(py))?;
        }
        Ok(())
    }
    pub(crate) fn open_blocks(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let (game, c, i) = (Self::ga_game(slf)?, Self::ga_c(slf)?, Self::index(slf)?);
        Self::close(slf).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?;
        if i.is_none() {
            return Ok(());
        }
        game.getattr("U")?.get_item("mouse")?.setattr("locked", false)?;
        game.getattr("ui_root")?.setattr("enabled", false)?;
        game.setattr("editor", imp(py, "main.client.block_editor")?.getattr("BlockEditor")?.call1((&game, c, i))?)?;
        Ok(())
    }
    pub(crate) fn open_rename(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let i = Self::index(slf)?;
        if i.is_none() {
            return Ok(());
        }
        let k = PyDict::new_bound(py);
        k.set_item("owner", s)?;
        imp(py, "main.client.device_ui")?.getattr("DevicePanel")?.call((Self::ga_game(slf)?, Self::ga_c(slf)?, i), Some(&k))?;
        Ok(())
    }
    pub(crate) fn close(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        super::client_misc::ui_destroy(py, &Self::ga_root(slf)?, &PyTuple::empty_bound(py), None)?;
        Self::ga_game(slf)?.call_method0("editor_closed")?;
        Ok(())
    }
    pub(crate) fn update(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let d = Self::device(slf)?;
        let c = Self::ga_c(slf)?;
        if d.is_none() {
            Self::ga_title(slf)?.setattr("text", "コンピューターが失われました")?;
            return Ok(());
        }
        let st = d.get_item("state")?;
        let g = |k: &str| st.dget1(k);
        let running = if g("running")?.is_truthy()? { "実行中" } else { "停止中" };
        let blocks = g("blocks")?.is_truthy()?;
        let mode = if blocks { "ブロック" } else { "Python" };
        Self::ga_py_btn(slf)?.setattr("enabled", blocks)?;
        Self::ga_title(slf)?.setattr("text", format!("{}  —  {}", d.get_item("name")?.str()?, c.getattr("name")?.str()?))?;
        let cs = c.getattr("status")?;
        let cs = if cs.is_truthy()? { cs } else { PyDict::new_bound(py).into_any() };
        let fmt = py.import_bound("builtins")?.getattr("format")?;
        let f = |v: f64, spec: &str| -> R<String> { fmt.call1((v, spec))?.extract() };
        let gv = |k: &str, d: f64| -> R<f64> { f64of(&cs.dget2(k, d)?) };
        let script = g("script")?;
        let sname = if script.is_truthy()? { script.str()?.to_string() } else { String::new() };
        let mut info = vec![
            format!("スクリプト: {}（{}）", if sname.is_empty() { "未設定" } else { sname.as_str() }, mode),
            format!("状態: {}", running),
            format!("電力: {} / {} MJ  (供給 {}%)", f(gv("stored", 0.0)? / 1e6, ".2f")?, f(gv("capacity", 0.0)? / 1e6, ".1f")?, f(gv("ratio", 1.0)? * 100.0, ".0f")?),
            format!("場所: scripts/{}", sname),
        ];
        let err = g("error")?;
        if err.is_truthy()? {
            info.push("エラー:".into());
            info.push(format!("  {}", sp_wrap(&st.get_item("error")?, 56)?));
        }
        Self::ga_info(slf)?.setattr("text", info.join("\n").replace('<', "＜").replace('>', "＞"))?;
        let logs = st.dget2("_log", PyList::empty_bound(py))?;
        let n = logs.len()?;
        let mut lines = vec![];
        for (k, t) in logs.iter()?.enumerate() {
            if k + 12 >= n {
                lines.push(sp_wrap(&t?, 64)?);
            }
        }
        Self::ga_log_text(slf)?.setattr("text", lines.join("\n").replace('<', "＜").replace('>', "＞"))?;
        let stt = Self::ga_status(slf)?;
        let now: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
        let t0 = Self::ga_status_time(slf).map(|x| f64of(&x).unwrap_or(0.0)).unwrap_or(0.0);
        if stt.getattr("text")?.is_truthy()? && now - t0 > 8.0 {
            stt.setattr("text", "")?;
        }
        Ok(())
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<ScriptPanel>()?;
    for f in [wrap_pyfunction_bound!(sp_open_external, m)?, wrap_pyfunction_bound!(sp_win_editors, m)?, wrap_pyfunction_bound!(sp_open_in_editor, m)?,
              wrap_pyfunction_bound!(sp_wrap, m)?] {
        m.add_function(f)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- ScriptPanel の欄の読み書き（C-01。前は __dict__）
// 入れていない欄を読むと、前と同じく AttributeError（getattr(x, 名前, 既定値) も前と同じに動く）
macro_rules! scriptpanel_attr_fields {
    ($($g:ident $s:ident $pg:ident $ps:ident $f:ident $name:literal),* $(,)?) => {
        #[allow(dead_code, non_snake_case)]
        impl ScriptPanel { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
                match &slf.try_borrow()?.$f {
                    Some(v) => Ok(v.bind(slf.py()).clone()),
                    None => Err(pyo3::exceptions::PyAttributeError::new_err($name)),
                }
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> PyResult<()> {
                let v = v.into_py(slf.py());
                slf.try_borrow_mut()?.$f = Some(v);
                Ok(())
            }
        )* }
        #[pymethods]
        #[allow(non_snake_case)]
        impl ScriptPanel { $(
            #[getter($name)]
            fn $pg(slf: &Bound<'_, Self>) -> PyResult<PyObject> {
                Ok(Self::$g(slf)?.unbind())
            }
            #[setter($name)]
            fn $ps(&mut self, v: PyObject) {
                self.$f = Some(v);
            }
        )* }
    };
}
scriptpanel_attr_fields!(
    ga_U sa_U pga_U psa_U a_U "U", ga_status_time sa_status_time pga_status_time psa_status_time a_status_time "_status_time", ga_c sa_c pga_c psa_c a_c "c", ga_cell sa_cell pga_cell psa_cell a_cell "cell", ga_game sa_game pga_game psa_game a_game "game", ga_info sa_info pga_info psa_info a_info "info", ga_keys sa_keys pga_keys psa_keys a_keys "keys", ga_log_text sa_log_text pga_log_text psa_log_text a_log_text "log_text", ga_modal sa_modal pga_modal psa_modal a_modal "modal", ga_py_btn sa_py_btn pga_py_btn psa_py_btn a_py_btn "py_btn", ga_root sa_root pga_root psa_root a_root "root", ga_status sa_status pga_status psa_status a_status "status", ga_title sa_title pga_title psa_title a_title "title", ga_world sa_world pga_world psa_world a_world "world"
);
