//! 起動時のメニュー画面（main/client/menu.py）：ワールドの選択・新規作成・削除・ワールド設定・ゲーム設定。

use pyo3::exceptions::{PyException, PyFileNotFoundError, PyOSError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyTuple};

use super::clo::{clo, clo0, clo1, clos};
use super::imp;
use super::panels::{hook, kw, partial, po};
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

pub const PAGE_SIZE: i64 = 6;
pub const MOD_ROWS: i64 = 6;
pub const NAME_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";
pub const TRASH: &str = ".trash"; // 消しきれなかったワールドの置き場（一覧には出ない。次回起動時に片付ける）

fn lg(py: Python<'_>) -> R<B<'_>> {
    imp(py, "main.log")?.call_method1("get", ("menu",))
}
fn paths(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.paths")?.into_any())
}
fn modmeta(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.modmeta")?.into_any())
}
fn menu_mod(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.menu")?.into_any())
}
fn utf8(py: Python<'_>) -> R<Bound<'_, PyDict>> {
    kw(py, vec![("encoding", po(py, "utf-8"))])
}
fn read_json<'py>(py: Python<'py>, p: &B<'py>) -> R<B<'py>> {
    imp(py, "json")?.call_method1("loads", (p.call_method("read_text", (), Some(&utf8(py)?))?,))
}
fn div<'py>(p: &B<'py>, n: &str) -> R<B<'py>> {
    p.call_method1("__truediv__", (n,))
}
/// Python の format(v, spec)
fn fmt(py: Python<'_>, v: &B<'_>, spec: &str) -> R<String> {
    py.import_bound("builtins")?.getattr("format")?.call1((v, spec))?.extract()
}
fn is_exc(py: Python<'_>, e: &PyErr) -> bool {
    e.is_instance_of::<PyException>(py)
}
fn sorted<'py>(py: Python<'py>, x: &B<'py>) -> R<B<'py>> {
    py.import_bound("builtins")?.getattr("sorted")?.call1((x,))
}
fn strip(o: &B<'_>) -> R<String> {
    o.call_method0("strip")?.extract()
}
/// 入力欄の文字（text_field があればそちら）
fn field_text<'py>(f: &B<'py>) -> R<B<'py>> {
    if f.hasattr("text_field")? {
        f.getattr("text_field")?.getattr("text")
    } else {
        f.getattr("text")
    }
}

#[pyfunction]
pub fn menu_dist_text(py: Python<'_>, v: &B<'_>) -> R<String> {
    if !v.is_truthy()? {
        return Ok("全部".into());
    }
    Ok(format!("{} m", fmt(py, v, ".0f")?))
}

// ---------------------------------------------------------------------------
// ワールド操作（UI に依存しない部分）
// ---------------------------------------------------------------------------
#[pyfunction]
pub fn menu_list_worlds(py: Python<'_>) -> R<Bound<'_, PyList>> {
    let out = PyList::empty_bound(py);
    let saves = paths(py)?.getattr("SAVES")?;
    if !saves.call_method0("exists")?.is_truthy()? {
        return Ok(out);
    }
    let upgrade = imp(py, "main.savefmt")?.getattr("upgrade_world")?;
    for d in saves.call_method0("iterdir")?.iter()? {
        let d = d?;
        let wj = div(&d, "world.json")?;
        let name: String = d.getattr("name")?.extract()?;
        if !d.call_method0("is_dir")?.is_truthy()? || !wj.call_method0("exists")?.is_truthy()? || name.starts_with('.') {
            continue;
        }
        let info = PyDict::new_bound(py);
        info.set_item("name", &name)?;
        info.set_item("constructs", 0)?;
        info.set_item("mods", PyList::empty_bound(py))?;
        info.set_item("mtime", 0.0)?;
        let mut s: B<'_> = PyDict::new_bound(py).into_any();
        let r = (|| -> R<()> {
            s = read_json(py, &wj)?;
            info.set_item("time", s.call_method1("get", ("time", 0.0))?)?;
            let st = s.call_method1("get", ("story",))?;
            if st.is_instance_of::<PyDict>() {
                info.set_item("story", s.get_item("story")?)?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => {}
            Err(e) if is_exc(py, &e) => info.set_item("broken", true)?,
            Err(e) => return Err(e),
        }
        match read_json(py, &div(&d, "constructs.json")?).and_then(|x| x.len()) {
            Ok(n) => info.set_item("constructs", n)?,
            Err(e) if is_exc(py, &e) => {}
            Err(e) => return Err(e),
        }
        match read_json(py, &div(&d, "mods.json")?).and_then(|x| x.call_method1("get", ("enabled", PyList::empty_bound(py)))) {
            Ok(m) => info.set_item("mods", m)?,
            Err(e) if is_exc(py, &e) => {}
            Err(e) => return Err(e),
        }
        let r = upgrade.call1((&s, info.get_item("mods")?))?; // 古い形式のワールドも今の形で表示
        info.set_item("mods", r.get_item(1)?)?;
        info.set_item("mode", r.get_item(0)?.call_method1("get", ("mode", "creative"))?)?;
        let mut mt: Option<B<'_>> = None;
        for f in d.call_method1("glob", ("*.json",))?.iter()? {
            let v = f?.call_method0("stat")?.getattr("st_mtime")?;
            if mt.as_ref().map(|m| v.gt(m)).transpose()?.unwrap_or(true) {
                mt = Some(v);
            }
        }
        let mt = match mt {
            Some(m) => m,
            None => wj.call_method0("stat")?.getattr("st_mtime")?,
        };
        info.set_item("mtime", mt)?;
        out.append(info)?;
    }
    let k = kw(py, vec![("key", imp(py, "operator")?.call_method1("itemgetter", ("mtime",))?.unbind()), ("reverse", po(py, true))])?;
    out.call_method("sort", (), Some(&k))?;
    Ok(out)
}

/// 一覧に出す遊び方：ストーリーなら今の章、ほかはサバイバル／クリエイティブと構築物の数。
#[pyfunction]
pub fn menu_world_mode_text(py: Python<'_>, w: &B<'_>) -> R<String> {
    let st = w.call_method1("get", ("story",))?;
    if st.is_instance_of::<PyDict>() {
        if st.call_method1("get", ("done",))?.is_truthy()? {
            return Ok("ストーリー（クリア）".into());
        }
        if !st.call_method1("get", ("built",))?.is_truthy()? {
            return Ok("ストーリー（はじめから）".into());
        }
        let ch: i64 = match py.import_bound("builtins")?.getattr("int")?.call1((st.call_method1("get", ("ch", 0))?,)) {
            Ok(c) => c.extract()?,
            Err(e) if e.is_instance_of::<PyTypeError>(py) || e.is_instance_of::<PyValueError>(py) => return Ok("ストーリー".into()),
            Err(e) => return Err(e),
        };
        let mut names: Vec<String> = vec!["序章".into()];
        names.extend((1..13).map(|k| format!("第{}章", k)));
        names.push("終章".into());
        names.push("エピローグ".into());
        return Ok(format!("ストーリー：{}", if 0 <= ch && (ch as usize) < names.len() { names[ch as usize].clone() } else { ch.to_string() }));
    }
    let mode = if w.call_method1("get", ("mode",))?.eq("survival")? { "サバイバル" } else { "クリエイティブ" };
    Ok(format!("{}  /  構築物 {}", mode, fmt(py, &w.get_item("constructs")?, "")?))
}

/// [(MOD名, 表示名)]。要素を足す MOD（シナリオは除く）。本体に入っている MOD を先に並べる。
#[pyfunction]
pub fn menu_list_mods(py: Python<'_>) -> R<Bound<'_, PyList>> {
    let out = PyList::empty_bound(py);
    let k = kw(py, vec![("kind", po(py, "addon"))])?;
    for m in modmeta(py)?.getattr("list_all")?.call((), Some(&k))?.iter()? {
        let m = m?;
        let id = m.get_item("id")?;
        out.append((&id, m.call_method1("get", ("name", &id))?))?;
    }
    Ok(out)
}

#[pyfunction]
pub fn menu_read_world_settings<'py>(py: Python<'py>, name: &B<'py>) -> R<Bound<'py, PyTuple>> {
    let d = paths(py)?.call_method1("world_dir", (name,))?;
    let defaults = imp(py, "main.world")?.getattr("DEFAULT_SETTINGS")?;
    let json = imp(py, "json")?;
    let s = json.call_method1("loads", (json.call_method1("dumps", (&defaults,))?,))?;
    let mut raw: B<'_> = PyDict::new_bound(py).into_any();
    match read_json(py, &div(&d, "world.json")?) {
        Ok(r) => raw = r,
        Err(e) if is_exc(py, &e) => {}
        Err(e) => return Err(e),
    }
    let mods = match read_json(py, &div(&d, "mods.json")?).and_then(|x| x.call_method1("get", ("enabled", PyList::empty_bound(py)))) {
        Ok(m) => m,
        Err(e) if is_exc(py, &e) => PyList::empty_bound(py).into_any(),
        Err(e) => return Err(e),
    };
    // 古い形式（creative / cities・サバイバルの MOD）を読み替える
    let r = imp(py, "main.savefmt")?.call_method1("upgrade_world", (raw, mods))?;
    let (raw, mods) = (r.get_item(0)?, r.get_item(1)?);
    s.call_method1("update", (&raw,))?;
    let cl = PyDict::new_bound(py);
    cl.call_method1("update", (defaults.get_item("cleanup")?,))?;
    cl.call_method1("update", (s.call_method1("get", ("cleanup", PyDict::new_bound(py)))?,))?;
    s.set_item("cleanup", cl)?;
    Ok(PyTuple::new_bound(py, [s, mods]))
}

#[pyfunction]
pub fn menu_write_world_settings(py: Python<'_>, name: &B<'_>, settings: &B<'_>, mods: &B<'_>) -> R<()> {
    let d = paths(py)?.call_method1("world_dir", (name,))?;
    let json = imp(py, "json")?;
    let k = kw(py, vec![("ensure_ascii", po(py, false)), ("indent", po(py, 1))])?;
    div(&d, "world.json")?.call_method("write_text", (json.call_method("dumps", (settings,), Some(&k))?,), Some(&utf8(py)?))?;
    let m = PyDict::new_bound(py);
    m.set_item("enabled", PyList::new_bound(py, mods.iter()?.collect::<R<Vec<_>>>()?))?;
    div(&d, "mods.json")?.call_method("write_text", (json.call_method("dumps", (m,), Some(&k))?,), Some(&utf8(py)?))?;
    lg(py)?.call_method1("info", ("ワールド設定を保存: %s", name))?;
    Ok(())
}

/// ワールドを作る（選んだ MOD に依存する MOD も入れる。シナリオなどワールドの形を決める MOD があればその形で）。
#[pyfunction]
#[pyo3(signature = (name, settings_patch, mods, planet="terra"))]
pub fn menu_create_world(py: Python<'_>, name: &B<'_>, settings_patch: &B<'_>, mods: &B<'_>, planet: &str) -> R<()> {
    imp(py, "main.tutorial")?.call_method1("create_world", (name, mods, settings_patch, planet))?;
    Ok(())
}

/// OneDrive などの同期ソフトが一瞬つかんでいて「アクセスが拒否されました」になることがあるので、
/// 読み取り専用を外して少し待ちながら何度かやり直す。
#[pyfunction]
#[pyo3(signature = (d, tries=6))]
pub fn menu_rmtree_retry(py: Python<'_>, d: &B<'_>, tries: i64) -> R<()> {
    let onexc = clo(py, move |a| {
        let py = a.py();
        let (func, path, exc) = (a.get_item(0)?, a.get_item(1)?, a.get_item(2)?);
        if exc.is_instance_of::<PyFileNotFoundError>() {
            return Ok(py.None());
        }
        let os = imp(py, "os")?;
        match os.call_method1("chmod", (&path, imp(py, "stat")?.getattr("S_IWRITE")?)) {
            Ok(_) => {}
            Err(e) if e.is_instance_of::<PyOSError>(py) => {}
            Err(e) => return Err(e),
        }
        let time = imp(py, "time")?;
        for k in 0..tries {
            time.call_method1("sleep", (0.1 * (k + 1) as f64,))?;
            match func.call1((&path,)) {
                Ok(_) => return Ok(py.None()),
                Err(e) if e.is_instance_of::<PyFileNotFoundError>(py) => return Ok(py.None()),
                Err(e) if e.is_instance_of::<PyOSError>(py) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(PyErr::from_value_bound(exc))
    })?;
    let vi: (i64, i64) = imp(py, "sys")?.getattr("version_info")?.get_item(0)?.extract::<i64>().and_then(|a| {
        Ok((a, imp(py, "sys")?.getattr("version_info")?.get_item(1)?.extract::<i64>()?))
    })?;
    let sh = imp(py, "shutil")?;
    if vi >= (3, 12) {
        sh.call_method("rmtree", (d,), Some(&kw(py, vec![("onexc", onexc.unbind())])?))?;
    } else {
        let oe = onexc.unbind();
        let onerror = clo(py, move |a| {
            let py = a.py();
            oe.bind(py).call1((a.get_item(0)?, a.get_item(1)?, a.get_item(2)?.get_item(1)?)).map(|x| x.unbind())
        })?;
        sh.call_method("rmtree", (d,), Some(&kw(py, vec![("onerror", onerror.unbind())])?))?;
    }
    Ok(())
}

/// ワールドを削除する。消しきれないものが残ったら、ゴミ箱フォルダへ移して一覧から消す。
/// 戻り値: True=完全に削除 / False=ゴミ箱に移した（次回起動時に片付ける）
#[pyfunction]
pub fn menu_delete_world(py: Python<'_>, name: &B<'_>) -> R<bool> {
    let p = paths(py)?;
    let d = p.call_method1("world_dir", (name,))?;
    let saves = p.getattr("SAVES")?;
    if !d.call_method0("resolve")?.getattr("parent")?.eq(saves.call_method0("resolve")?)? {
        return Err(PyValueError::new_err("不正なワールド名です"));
    }
    let l = lg(py)?;
    match menu_mod(py)?.call_method1("_rmtree_retry", (&d,)) {
        Ok(_) => {
            l.call_method1("info", ("ワールド削除: %s", name))?;
            return Ok(true);
        }
        Err(e) if e.is_instance_of::<PyOSError>(py) => {
            if !d.call_method0("exists")?.is_truthy()? {
                return Ok(true);
            }
            l.call_method1("warning", ("ワールド %s を消しきれませんでした（%s）。ゴミ箱フォルダへ移します", name, e.value_bound(py)))?;
        }
        Err(e) => return Err(e),
    }
    let trash = div(&saves, TRASH)?;
    trash.call_method("mkdir", (), Some(&kw(py, vec![("exist_ok", po(py, true))])?))?;
    let t: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
    let dst = div(&trash, &format!("{}_{}", name.str()?, t.trunc() as i64))?;
    if let Err(e) = d.call_method1("rename", (&dst,)) {
        if e.is_instance_of::<PyOSError>(py) {
            let ne = PyOSError::new_err(format!("ファイルが使用中です（OneDrive の同期中など）。少し待ってからもう一度削除してください: {}", e.value_bound(py).str()?));
            ne.set_cause(py, Some(e));
            return Err(ne);
        }
        return Err(e);
    }
    l.call_method1("info", ("ワールド %s をゴミ箱フォルダへ移しました: %s", name, dst))?;
    Ok(false)
}

/// 前回消しきれなかったワールドを片付ける（失敗しても気にしない）。
#[pyfunction]
pub fn menu_purge_trash(py: Python<'_>) -> R<()> {
    let trash = div(&paths(py)?.getattr("SAVES")?, TRASH)?;
    if !trash.call_method0("exists")?.is_truthy()? {
        return Ok(());
    }
    let ds: Vec<B<'_>> = trash.call_method0("iterdir")?.iter()?.collect::<R<_>>()?;
    let m = menu_mod(py)?;
    for d in ds {
        match m.call_method("_rmtree_retry", (&d,), Some(&kw(py, vec![("tries", po(py, 2))])?)) {
            Ok(_) => {}
            Err(e) if e.is_instance_of::<PyOSError>(py) => {}
            Err(e) => return Err(e),
        }
    }
    match trash.call_method0("rmdir") {
        Ok(_) => Ok(()),
        Err(e) if e.is_instance_of::<PyOSError>(py) => Ok(()),
        Err(e) => Err(e),
    }
}

#[pyfunction]
pub fn menu_save_config(py: Python<'_>, cfg: &B<'_>) -> R<()> {
    let p = div(&paths(py)?.getattr("CONFIG")?, "settings.json")?;
    let k = kw(py, vec![("indent", po(py, 2)), ("ensure_ascii", po(py, false))])?;
    p.call_method("write_text", (imp(py, "json")?.call_method("dumps", (cfg,), Some(&k))?,), Some(&utf8(py)?))?;
    lg(py)?.call_method1("info", ("ゲーム設定を保存しました",))?;
    Ok(())
}

#[pyfunction]
pub fn menu_default_name(py: Python<'_>) -> R<String> {
    match imp(py, "getpass")?.call_method0("getuser").and_then(|u| u.extract::<String>()) {
        Ok(u) => {
            let u: String = u.chars().take(24).collect();
            Ok(if u.is_empty() { "player".into() } else { u })
        }
        Err(e) if is_exc(py, &e) => Ok("player".into()),
        Err(e) => Err(e),
    }
}

/// 設定のウィンドウ（解像度・全画面）を反映する。prev（前の設定）と同じなら何もしない
/// （手で大きさを変えた・最大化したウィンドウを、設定の保存で元に戻さないため）。
#[pyfunction]
#[pyo3(signature = (cfg, prev=None))]
pub fn menu_apply_window_settings(py: Python<'_>, cfg: &B<'_>, prev: Option<&B<'_>>) -> R<()> {
    let ur = imp(py, "ursina")?;
    let (window, vec2) = (ur.getattr("window")?, ur.getattr("Vec2")?);
    let w = cfg.get_item("window")?;
    if let Some(p) = prev.filter(|p| !p.is_none()) {
        let mut same = true;
        for k in ["width", "height", "fullscreen"] {
            if !p.call_method1("get", (k,))?.eq(w.call_method1("get", (k,))?)? {
                same = false;
                break;
            }
        }
        if same {
            return Ok(());
        }
    }
    let r = (|| -> R<()> {
        let fs = w.call_method1("get", ("fullscreen", false))?.is_truthy()?;
        if window.getattr("fullscreen")?.is_truthy()? != fs {
            window.setattr("fullscreen", fs)?;
        }
        if !w.call_method1("get", ("fullscreen", false))?.is_truthy()? {
            window.setattr("size", vec2.call1((w.call_method1("get", ("width", 1280))?, w.call_method1("get", ("height", 720))?))?)?;
        }
        Ok(())
    })();
    match r {
        Ok(()) => Ok(()),
        Err(e) if is_exc(py, &e) => {
            lg(py)?.call_method1("warning", ("ウィンドウ設定の反映に失敗: %s", e.value_bound(py)))?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// 画面
// ---------------------------------------------------------------------------
#[pyclass(module = "main.client.menu", subclass, dict)]
pub struct MainMenu {}

/// U["color"]
fn color<'py>(s: &B<'py>) -> R<B<'py>> {
    s.getattr("U")?.get_item("color")
}
fn rgb<'py>(s: &B<'py>, r: f64, g: f64, b: f64) -> R<B<'py>> {
    color(s)?.call_method1("rgb", (r, g, b))
}
fn cn<'py>(s: &B<'py>, n: &str) -> R<B<'py>> {
    color(s)?.getattr(n)
}
/// self._text(...)
fn t<'py>(s: &B<'py>, text: impl IntoPy<PyObject>, pos: (f64, f64), scale: f64, origin: (f64, f64), col: Option<B<'py>>) -> R<B<'py>> {
    let py = s.py();
    let u = s.getattr("U")?;
    let col = match col {
        Some(c) => c,
        None => u.get_item("color")?.getattr("white")?,
    };
    u.get_item("Text")?.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind()), ("text", text.into_py(py)), ("position", po(py, pos)),
        ("origin", po(py, origin)), ("scale", po(py, scale)), ("color", col.unbind())])?))
}
const C: (f64, f64) = (0.0, 0.0);
const L: (f64, f64) = (-0.5, 0.0);
/// self._button(...)
fn b<'py>(s: &B<'py>, text: &str, pos: (f64, f64), on_click: &B<'py>, size: (f64, f64), col: Option<B<'py>>, enabled: bool) -> R<B<'py>> {
    let py = s.py();
    let u = s.getattr("U")?;
    let c = match col {
        Some(c) => c,
        None => rgb(s, 0.22, 0.32, 0.45)?,
    };
    let c = if enabled { c } else { rgb(s, 0.2, 0.2, 0.22)? };
    let bt = u.get_item("Button")?.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind()), ("text", po(py, text)), ("position", po(py, pos)),
        ("scale", po(py, size)), ("color", c.unbind()), ("text_size", po(py, 0.8))])?))?;
    let te = bt.getattr("text_entity")?;
    te.setattr("scale", imp(py, "operator")?.call_method1("imul", (te.getattr("scale")?, 1.0))?)?;
    if enabled {
        bt.setattr("on_click", on_click)?;
    } else {
        bt.setattr("text_color", cn(s, "gray")?)?;
    }
    Ok(bt)
}
const BS: (f64, f64) = (0.32, 0.055);
fn panel<'py>(s: &B<'py>, pos: (f64, f64), size: (f64, f64), col: Option<B<'py>>) -> R<B<'py>> {
    let py = s.py();
    let col = match col {
        Some(c) => c,
        None => color(s)?.call_method1("rgba", (1, 1, 1, 0.06))?,
    };
    s.getattr("U")?.get_item("Entity")?.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind()), ("model", po(py, "quad")),
        ("position", po(py, (pos.0, pos.1, 0.01))), ("scale", po(py, size)), ("color", col.unbind())])?))
}
/// d[key] = v にする関数
fn setter<'py>(py: Python<'py>, d: &B<'py>, key: &str) -> R<B<'py>> {
    partial(&d.getattr("__setitem__")?, vec![po(py, key)])
}
/// s を持ち歩く（クロージャの中で使う）
fn keep_(s: &B<'_>) -> Py<PyAny> {
    s.clone().unbind()
}

/// 値の一覧の中で current の位置（無ければいちばん近い数）
fn index_of(py: Python<'_>, values: &Bound<'_, PyList>, current: &B<'_>, slider: bool) -> R<usize> {
    if values.contains(current)? {
        return Ok(values.call_method1("index", (current,))?.extract()?);
    }
    let numeric = !current.is_instance_of::<pyo3::types::PyBool>()
        && (current.is_instance_of::<pyo3::types::PyInt>() || current.is_instance_of::<pyo3::types::PyFloat>());
    if !slider && !numeric {
        return Ok(0);
    }
    let cur = f64of(current)?;
    let mut best = 0usize;
    let mut bd = f64::INFINITY;
    for (i, v) in values.iter().enumerate() {
        let d = if slider && !v.is_truthy()? { 1e18 } else { (f64of(&v)? - cur).abs() };
        if i == 0 || d < bd {
            best = i;
            bd = d;
        }
    }
    let _ = py;
    Ok(best)
}

#[pymethods]
impl MainMenu {
    #[classattr]
    const MOD_ROWS: i64 = MOD_ROWS;

    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        MainMenu {}
    }
    #[pyo3(signature = (cfg, on_play, on_quit, on_join=None))]
    fn __init__(slf: &Bound<'_, Self>, cfg: &B<'_>, on_play: &B<'_>, on_quit: &B<'_>, on_join: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let ur = imp(py, "ursina")?;
        let u = PyDict::new_bound(py);
        for n in ["Entity", "Text", "Button", "InputField", "camera", "color", "window"] {
            u.set_item(n, ur.getattr(n)?)?;
        }
        s.setattr("U", &u)?;
        s.setattr("cfg", cfg)?;
        s.setattr("on_play", on_play)?;
        s.setattr("on_quit", on_quit)?;
        s.setattr("on_join", on_join.map(|x| x.clone().unbind()).unwrap_or_else(|| py.None()))?;
        s.setattr("browser", py.None())?;
        s.setattr("selected", py.None())?;
        s.setattr("page_index", 0)?;
        s.setattr("status", "")?;
        s.setattr("_new", py.None())?; // 新規作成の入力（MOD の画面へ行って戻っても残す）
        s.setattr("_ws", py.None())?; // ワールド設定の入力（同上）
        ur.getattr("window")?.setattr("color", ur.getattr("color")?.call_method1("rgb", (0.07, 0.09, 0.13))?)?;
        ur.getattr("mouse")?.setattr("locked", false)?;
        s.setattr("root", ur.getattr("Entity")?.call((), Some(&kw(py, vec![("parent", ur.getattr("camera")?.getattr("ui")?.unbind())])?))?)?;
        s.setattr("page", py.None())?;
        Self::show_main(slf)
    }

    // --- 部品 ----------------------------------------------------------
    fn _clear(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let kids: Vec<B<'_>> = slf.as_any().getattr("root")?.getattr("children")?.iter()?.collect::<R<_>>()?;
        for c in kids {
            super::client_misc::ui_destroy(py, &c, &PyTuple::empty_bound(py), None)?;
        }
        Ok(())
    }
    #[pyo3(signature = (text, pos, scale=1.0, origin=(0.0, 0.0), col=None))]
    fn _text<'py>(slf: &Bound<'py, Self>, text: &B<'py>, pos: (f64, f64), scale: f64, origin: (f64, f64), col: Option<&B<'py>>) -> R<B<'py>> {
        t(slf.as_any(), text.clone().unbind(), pos, scale, origin, col.filter(|c| c.is_truthy().unwrap_or(false)).cloned())
    }
    #[pyo3(signature = (text, pos, on_click, size=(0.32, 0.055), col=None, enabled=true))]
    fn _button<'py>(slf: &Bound<'py, Self>, text: &str, pos: (f64, f64), on_click: &B<'py>, size: (f64, f64), col: Option<&B<'py>>, enabled: bool) -> R<B<'py>> {
        b(slf.as_any(), text, pos, on_click, size, col.filter(|c| c.is_truthy().unwrap_or(false)).cloned(), enabled)
    }
    #[pyo3(signature = (pos, size, col=None))]
    fn _panel<'py>(slf: &Bound<'py, Self>, pos: (f64, f64), size: (f64, f64), col: Option<&B<'py>>) -> R<B<'py>> {
        panel(slf.as_any(), pos, size, col.filter(|c| c.is_truthy().unwrap_or(false)).cloned())
    }
    /// ラベル < 値 > の行。values: 選択肢のリスト。
    #[pyo3(signature = (label, y, values, current, fmt, on_change, x=0.0))]
    #[allow(clippy::too_many_arguments)]
    fn _stepper(slf: &Bound<'_, Self>, label: &B<'_>, y: f64, values: &B<'_>, current: &B<'_>, fmt: &B<'_>, on_change: &B<'_>, x: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let values = PyList::new_bound(py, values.iter()?.collect::<R<Vec<_>>>()?);
        let idx = index_of(py, &values, current, false)?;
        let state = PyDict::new_bound(py);
        state.set_item("i", idx)?;
        t(s, label.clone().unbind(), (x - 0.38, y), 0.85, L, None)?;
        let val = t(s, fmt.call1((values.get_item(idx)?,))?.unbind(), (x + 0.2, y), 0.85, C, None)?;
        let (st, vl, vs, fm, oc) = (state.into_any().unbind(), val.unbind(), values.into_any().unbind(), fmt.clone().unbind(), on_change.clone().unbind());
        let step = clo1(py, move |d| {
            let py = d.py();
            let (st, vs) = (st.bind(py), vs.bind(py));
            let n = vs.len()? as i64;
            let i: i64 = st.get_item("i")?.extract()?;
            let i = (i + d.extract::<i64>()?).rem_euclid(n);
            st.set_item("i", i)?;
            let v = vs.get_item(i)?;
            vl.bind(py).setattr("text", fm.bind(py).call1((&v,))?)?;
            oc.bind(py).call1((&v,))?;
            Ok(())
        })?;
        b(s, "◀", (x + 0.06, y), &partial(&step, vec![po(py, -1)])?, (0.045, 0.045), None, true)?;
        b(s, "▶", (x + 0.34, y), &partial(&step, vec![po(py, 1)])?, (0.045, 0.045), None, true)?;
        Ok(())
    }
    /// ラベル ◀ ━━●━━ ▶ 値 の行。values：選べる値（小さい順）。棒を押すとその位置の値に、◀ ▶ で1つずつ。
    #[pyo3(signature = (label, y, values, current, fmt, on_change, x=0.0))]
    #[allow(clippy::too_many_arguments)]
    fn _slider(slf: &Bound<'_, Self>, label: &B<'_>, y: f64, values: &B<'_>, current: &B<'_>, fmt: &B<'_>, on_change: &B<'_>, x: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let u = s.getattr("U")?;
        let values = PyList::new_bound(py, values.iter()?.collect::<R<Vec<_>>>()?);
        let idx = index_of(py, &values, current, true)?;
        let n = values.len() as i64;
        let (x0, x1) = (x + 0.04, x + 0.3);
        t(s, label.clone().unbind(), (x - 0.38, y), 0.85, L, None)?;
        let val = t(s, fmt.call1((values.get_item(idx)?,))?.unbind(), (x + 0.43, y), 0.85, C, None)?;
        let bar = u.get_item("Button")?.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind()), ("model", po(py, "quad")),
            ("position", po(py, ((x0 + x1) / 2.0, y))), ("scale", po(py, (x1 - x0, 0.012))), ("color", rgb(s, 0.35, 0.38, 0.42)?.unbind()),
            ("highlight_color", rgb(s, 0.45, 0.5, 0.55)?.unbind())])?))?;
        let knob = u.get_item("Entity")?.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind()), ("model", po(py, "quad")),
            ("position", po(py, (x0, y, -0.01))), ("scale", po(py, (0.014, 0.04))), ("color", rgb(s, 0.9, 0.75, 0.3)?.unbind())])?))?;
        let ctx = std::rc::Rc::new(Slider { i: std::cell::Cell::new(idx as i64), n, x0, x1, val: val.unbind(), values: values.into_any().unbind(),
            fmt: fmt.clone().unbind(), on_change: on_change.clone().unbind(), knob: knob.unbind() });
        let c1 = ctx.clone();
        let set_i = clo1(py, move |i| c1.set_i(i.py(), i.extract()?))?;
        let c2 = ctx.clone();
        bar.setattr("on_click", clo0(py, move |py| {
            let pt = imp(py, "ursina")?.getattr("mouse")?.getattr("point").unwrap_or_else(|_| py.None().into_bound(py));
            if pt.is_none() {
                return Ok(());
            }
            let v = (f64of(&py.import_bound("builtins")?.getattr("float")?.call1((pt.get_item(0)?,))?)? + 0.5) * (c2.n - 1) as f64;
            c2.set_i(py, v.round_ties_even() as i64)
        })?)?;
        let c3 = ctx.clone();
        let c4 = ctx.clone();
        b(s, "◀", (x0 - 0.04, y), &clo0(py, move |py| set_i_rel(py, &c3, -1))?, (0.035, 0.04), None, true)?;
        b(s, "▶", (x1 + 0.04, y), &clo0(py, move |py| set_i_rel(py, &c4, 1))?, (0.035, 0.04), None, true)?;
        let _ = set_i;
        ctx.show(py)
    }
    #[pyo3(signature = (label, y, current, on_change, x=0.0))]
    fn _toggle(slf: &Bound<'_, Self>, label: &B<'_>, y: f64, current: &B<'_>, on_change: &B<'_>, x: f64) -> R<()> {
        let py = slf.py();
        let onoff = clos(py, |v| Ok(if v.is_truthy()? { "ON".into() } else { "OFF".into() }))?;
        let vals = PyList::new_bound(py, [true, false]);
        Self::_stepper(slf, label, y, vals.as_any(), &po(py, current.is_truthy()?).into_bound(py), &onoff, on_change, x)
    }
    fn _status_line(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let st = s.getattr("status")?;
        if st.is_truthy()? {
            t(s, st.unbind(), (0.0, -0.44), 0.8, C, Some(cn(s, "orange")?))?;
        }
        Ok(())
    }

    // --- メイン ---------------------------------------------------------
    fn show_main(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        Self::_clear(slf)?;
        let worlds = menu_list_worlds(py)?;
        let names: Vec<B<'_>> = worlds.iter().map(|w| w.get_item("name")).collect::<R<_>>()?;
        let sel = s.getattr("selected")?;
        let mut found = false;
        for n in &names {
            if n.eq(&sel)? {
                found = true;
            }
        }
        if !found {
            s.setattr("selected", names.first().map(|n| n.clone().unbind()).unwrap_or_else(|| py.None()))?;
        }
        let lgray = || cn(s, "light_gray");
        t(s, "Alpedia", (0.0, 0.44), 2.6, C, None)?;
        t(s, "物理演算ボクセルサンドボックス", (0.0, 0.385), 0.8, C, Some(lgray()?))?;
        // 遊び方を選ぶ（新しく始める）
        let qn = s.getattr("_quick_new")?;
        let modes = [("シナリオ", "物語を選んで始める", s.getattr("show_scenarios")?, rgb(s, 0.45, 0.3, 0.6)?),
            ("サバイバル", "資源を集めて作る・月をめざす", partial(&qn, vec![po(py, "survival")])?, rgb(s, 0.25, 0.45, 0.3)?),
            ("クリエイティブ", "何でも置ける・自由に作る", partial(&qn, vec![po(py, "creative")])?, rgb(s, 0.25, 0.38, 0.55)?)];
        for (k, (title, sub, f, col)) in modes.into_iter().enumerate() {
            let x = -0.52 + k as f64 * 0.35;
            b(s, title, (x, 0.29), &f, (0.32, 0.075), Some(col), true)?;
            t(s, sub, (x, 0.235), 0.6, C, Some(lgray()?))?;
        }
        // 続きから（ワールド一覧）
        panel(s, (-0.17, -0.06), (0.98, 0.44), None)?;
        t(s, "続きから", (-0.64, 0.135), 0.95, L, None)?;
        let nw = worlds.len() as i64;
        let pages = 1.max((nw + PAGE_SIZE - 1) / PAGE_SIZE);
        let pi: i64 = s.getattr("page_index")?.extract()?;
        let pi = pi.min(pages - 1);
        s.setattr("page_index", pi)?;
        let start = pi * PAGE_SIZE;
        if nw == 0 {
            t(s, "まだワールドがありません。上のボタンで始めてください", (-0.17, -0.05), 0.85, C, Some(lgray()?))?;
        }
        let selected = s.getattr("selected")?;
        let time = imp(py, "time")?;
        let select = s.getattr("_select")?;
        for (k, i) in (start..(start + PAGE_SIZE).min(nw)).enumerate() {
            let w = worlds.get_item(i as usize)?;
            let y = 0.075 - k as f64 * 0.062;
            let col = if w.get_item("name")?.eq(&selected)? { rgb(s, 0.25, 0.45, 0.65)? } else { rgb(s, 0.15, 0.17, 0.22)? };
            let when: String = time.call_method1("strftime", ("%Y-%m-%d %H:%M", time.call_method1("localtime", (w.get_item("mtime")?,))?))?.extract()?;
            let wname = w.get_item("name")?.str()?.to_string();
            let label = if w.call_method1("get", ("broken",))?.is_truthy()? {
                format!("{}    （world.json が壊れています）", wname)
            } else {
                format!("{}    {}  /  {}", wname, when, menu_world_mode_text(py, &w)?)
            };
            let bt = b(s, &label, (-0.17, y), &partial(&select, vec![w.get_item("name")?.unbind()])?, (0.94, 0.054), Some(col), true)?;
            let te = bt.getattr("text_entity")?;
            te.setattr("origin", (-0.5, 0))?;
            te.setattr("x", -0.47)?;
        }
        if pages > 1 {
            let pg = s.getattr("_page")?;
            b(s, "▲", (0.27, 0.135), &partial(&pg, vec![po(py, -1)])?, (0.045, 0.04), None, true)?;
            t(s, format!("{}/{}", pi + 1, pages), (0.18, 0.135), 0.75, C, None)?;
            b(s, "▼", (0.27, -0.255), &partial(&pg, vec![po(py, 1)])?, (0.045, 0.04), None, true)?;
        }
        // 操作ボタン
        let has = !selected.is_none();
        let x = 0.55;
        b(s, "遊ぶ", (x, 0.1), &s.getattr("_play")?, (0.3, 0.07), Some(rgb(s, 0.2, 0.5, 0.3)?), has)?;
        let sk = keep_(s);
        b(s, "ワールド設定", (x, 0.02), &clo0(py, move |py| {
            let s = sk.bind(py);
            s.call_method1("show_world_settings", (s.getattr("selected")?,))?;
            Ok(())
        })?, BS, None, has)?;
        let sk = keep_(s);
        b(s, "削除", (x, -0.05), &clo0(py, move |py| {
            let s = sk.bind(py);
            s.call_method1("show_delete", (s.getattr("selected")?,))?;
            Ok(())
        })?, BS, Some(rgb(s, 0.5, 0.2, 0.2)?), has)?;
        b(s, "詳しく設定して作る…", (x, -0.12), &s.getattr("show_new")?, BS, None, true)?;
        b(s, "マルチプレイ（LAN）", (x, -0.19), &s.getattr("show_multiplayer")?, BS, Some(rgb(s, 0.25, 0.35, 0.55)?), true)?;
        b(s, "ゲーム設定", (x, -0.26), &s.getattr("show_game_settings")?, BS, None, true)?;
        b(s, "終了", (x, -0.33), &s.getattr("on_quit")?, BS, Some(rgb(s, 0.3, 0.3, 0.33)?), true)?;
        if has {
            t(s, format!("選択中: {}", selected.str()?), (-0.17, -0.31), 0.8, C, Some(lgray()?))?;
        }
        Self::_status_line(slf)
    }
    fn _free_name(slf: &Bound<'_, Self>, base: &str) -> R<String> {
        let py = slf.py();
        let names: Vec<String> = menu_list_worlds(py)?.iter().map(|w| w.get_item("name")?.extract()).collect::<R<_>>()?;
        let p = paths(py)?;
        let mut k = 1;
        loop {
            let n = format!("{}{}", base, k);
            if !names.contains(&n) && !p.call_method1("world_dir", (&n,))?.call_method0("exists")?.is_truthy()? {
                return Ok(n);
            }
            k += 1;
        }
    }
    /// サバイバル・クリエイティブを、おすすめの設定（惑星ガイア）ですぐ始める。
    fn _quick_new(slf: &Bound<'_, Self>, mode: &str) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let name = Self::_free_name(slf, mode)?;
        // MOD は入れない（街などは「詳しく設定して作る」の MOD の選択で）。完全剛体モード（軽い）
        let patch = PyDict::new_bound(py);
        patch.set_item("mode", mode)?;
        patch.set_item("full_rigid", true)?;
        if mode == "creative" {
            patch.set_item("starter_sample", true)?; // クリエイティブ：見本のロケットを置く
        }
        let k = kw(py, vec![("planet", po(py, "gaia"))])?;
        match menu_mod(py)?.getattr("create_world")?.call((&name, patch, PyList::empty_bound(py)), Some(&k)) {
            Ok(_) => {}
            Err(e) if is_exc(py, &e) => {
                s.setattr("status", format!("作れません: {}", e.value_bound(py).str()?))?;
                return Self::show_main(slf);
            }
            Err(e) => return Err(e),
        }
        s.setattr("selected", &name)?;
        s.getattr("on_play")?.call1((&name,))?;
        Ok(())
    }

    // --- シナリオを選ぶ ---------------------------------------------------
    /// シナリオの MOD（mod.json の "type": "scenario"）の一覧。選ぶと新しいワールドで最初から始める。
    #[pyo3(signature = (page=0))]
    fn show_scenarios(slf: &Bound<'_, Self>, page: i64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        Self::_clear(slf)?;
        let sc = modmeta(py)?.getattr("list_all")?.call((), Some(&kw(py, vec![("kind", po(py, "scenario"))])?))?;
        let sc: Vec<B<'_>> = sc.iter()?.collect::<R<_>>()?;
        let lgray = || cn(s, "light_gray");
        t(s, "シナリオを選ぶ", (0.0, 0.42), 1.8, C, None)?;
        t(s, "選んだシナリオを最初から始めます（途中のシナリオは「続きから」に残ります）", (0.0, 0.36), 0.7, C, Some(lgray()?))?;
        let rows = 4i64;
        let n = sc.len() as i64;
        let pages = 1.max((n + rows - 1) / rows);
        let page = 0.max(page.min(pages - 1));
        if sc.is_empty() {
            t(s, "シナリオがありません（mods/ に \"type\": \"scenario\" の MOD を入れると出ます）", (0.0, 0.1), 0.8, C, Some(lgray()?))?;
        }
        let start = s.getattr("_start_scenario")?;
        for (k, i) in ((page * rows)..((page + 1) * rows).min(n)).enumerate() {
            let m = &sc[i as usize];
            let y = 0.24 - k as f64 * 0.15;
            panel(s, (0.0, y - 0.02), (1.5, 0.13), Some(color(s)?.call_method1("rgba", (1, 1, 1, 0.06))?))?;
            let id = m.get_item("id")?;
            t(s, m.call_method1("get", ("name", &id))?.unbind(), (-0.7, y + 0.02), 1.0, L, None)?;
            let d0 = m.call_method1("get", ("description",))?;
            let desc: Vec<char> = if d0.is_truthy()? { d0.str()?.to_string().replace('\n', " ").chars().collect() } else { vec![] };
            let mut lines: Vec<String> = vec![];
            let mut j = 0;
            while j < desc.len().min(104) {
                lines.push(desc[j..(j + 52).min(desc.len())].iter().collect());
                j += 52;
            }
            if desc.len() > 104 {
                let last = lines.pop().unwrap();
                let mut lc: Vec<char> = last.chars().collect();
                lc.pop();
                lines.push(lc.into_iter().collect::<String>() + "…");
            }
            t(s, lines.join("\n"), (-0.7, y - 0.035), 0.6, L, Some(lgray()?))?;
            let ok = m.get_item("exists")?.is_truthy()? && !m.call_method1("get", ("error",))?.is_truthy()?;
            b(s, "始める", (0.62, y - 0.01), &partial(&start, vec![id.unbind()])?, (0.16, 0.06), Some(rgb(s, 0.45, 0.3, 0.6)?), ok)?;
        }
        if pages > 1 {
            let ss = s.getattr("show_scenarios")?;
            b(s, "◀ 前", (-0.2, -0.36), &partial(&ss, vec![po(py, page - 1)])?, (0.14, 0.05), None, true)?;
            t(s, format!("{}/{}", page + 1, pages), (0.0, -0.36), 0.8, C, None)?;
            b(s, "次 ▶", (0.2, -0.36), &partial(&ss, vec![po(py, page + 1)])?, (0.14, 0.05), None, true)?;
        }
        b(s, "戻る", (0.62, -0.42), &s.getattr("_back")?, (0.2, 0.055), Some(rgb(s, 0.3, 0.3, 0.33)?), true)?;
        Self::_status_line(slf)
    }
    /// シナリオを最初から（新しいワールド）。
    fn _start_scenario(slf: &Bound<'_, Self>, mod_id: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let base = if mod_id.eq("story")? { "story".to_string() } else { mod_id.str()?.to_string() };
        let name = Self::_free_name(slf, &base)?;
        let k = kw(py, vec![("planet", po(py, "gaia"))])?;
        match menu_mod(py)?.getattr("create_world")?.call((&name, PyDict::new_bound(py), PyList::new_bound(py, [mod_id])), Some(&k)) {
            Ok(_) => {}
            Err(e) if is_exc(py, &e) => {
                s.setattr("status", format!("作れません: {}", e.value_bound(py).str()?))?;
                return Self::show_main(slf);
            }
            Err(e) => return Err(e),
        }
        s.setattr("selected", &name)?;
        s.getattr("on_play")?.call1((&name,))?;
        Ok(())
    }

    // --- マルチプレイ（LAN） -------------------------------------------------
    /// LAN の中で公開中のワールドの一覧と、IP アドレスを入れて参加する画面。
    fn show_multiplayer(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let net = imp(py, "main.net")?;
        Self::_clear(slf)?;
        let u = s.getattr("U")?;
        let mp = s.getattr("cfg")?.call_method1("setdefault", ("multiplayer", PyDict::new_bound(py)))?;
        if s.getattr("browser")?.is_none() {
            s.setattr("browser", net.call_method0("Browser")?)?;
        }
        let lgray = || cn(s, "light_gray");
        t(s, "マルチプレイ（LAN）", (0.0, 0.42), 1.8, C, None)?;
        t(s, "名前", (-0.62, 0.32), 0.9, L, None)?;
        let nm = mp.call_method1("get", ("name",))?;
        let nm: PyObject = if nm.is_truthy()? { nm.unbind() } else { po(py, menu_default_name(py)?) };
        let name = u.get_item("InputField")?.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind()), ("default_value", nm),
            ("position", po(py, (-0.2, 0.32))), ("scale", po(py, (0.5, 0.05))), ("character_limit", po(py, 24))])?))?;
        panel(s, (-0.2, 0.03), (0.92, 0.46), None)?;
        t(s, "LAN で公開中のワールド", (-0.64, 0.23), 0.9, L, None)?;
        let rows = u.get_item("Entity")?.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind())])?))?;
        let state = PyDict::new_bound(py);
        state.set_item("key", py.None())?;
        state.set_item("t", 0.0)?;
        let (sk, mpk, namek) = (keep_(s), mp.clone().unbind(), name.unbind());
        let join = clo(py, move |a| {
            let py = a.py();
            let (s, mp) = (sk.bind(py), mpk.bind(py));
            let n = strip(&field_text(namek.bind(py))?)?;
            mp.set_item("name", if n.is_empty() { menu_default_name(py)? } else { n })?;
            match menu_mod(py)?.call_method1("save_config", (s.getattr("cfg")?,)) {
                Ok(_) => {}
                Err(e) if is_exc(py, &e) => {}
                Err(e) => return Err(e),
            }
            s.call_method0("_close_browser")?;
            let oj = s.getattr("on_join")?;
            if oj.is_truthy()? {
                oj.call1((a.get_item(0)?, a.get_item(1)?, mp.get_item("name")?))?;
            }
            Ok(py.None())
        })?;
        let (sk, stk, rk, jk) = (keep_(s), state.clone().into_any().unbind(), rows.clone().unbind(), join.clone().unbind());
        let redraw = clo0(py, move |py| {
            let (s, st, rows) = (sk.bind(py), stk.bind(py), rk.bind(py));
            let u = s.getattr("U")?;
            let br = s.getattr("browser")?;
            let found: Vec<B<'_>> = if br.is_truthy()? { br.call_method0("worlds")?.iter()?.collect::<R<_>>()? } else { vec![] };
            let mut key = vec![];
            for f in &found {
                let info = f.get_item(2)?;
                key.push(PyTuple::new_bound(py, [f.get_item(0)?, f.get_item(1)?, info.call_method1("get", ("world",))?, info.call_method1("get", ("players",))?]));
            }
            let key = PyTuple::new_bound(py, key);
            if key.eq(st.get_item("key")?)? {
                return Ok(());
            }
            st.set_item("key", &key)?;
            let kids: Vec<B<'_>> = rows.getattr("children")?.iter()?.collect::<R<_>>()?;
            for ch in kids {
                super::client_misc::ui_destroy(py, &ch, &PyTuple::empty_bound(py), None)?;
            }
            let lgray = color(s)?.getattr("light_gray")?;
            if found.is_empty() {
                let mut msg = "探しています…（同じ LAN でホストが「LAN に公開」すると、ここに出ます）".to_string();
                if !br.is_none() && br.getattr("error")?.is_truthy()? {
                    msg = format!("LAN の知らせを受け取れません（{}）。下で IP アドレスを入れて参加してください", br.getattr("error")?.str()?);
                }
                u.get_item("Text")?.call((), Some(&kw(py, vec![("parent", rows.clone().unbind()), ("text", po(py, msg)), ("position", po(py, (-0.2, 0.1))),
                    ("origin", po(py, (0, 0))), ("scale", po(py, 0.75)), ("color", lgray.unbind())])?))?;
            }
            for (k, f) in found.iter().take(5).enumerate() {
                let (ip, port, info) = (f.get_item(0)?, f.get_item(1)?, f.get_item(2)?);
                let y = 0.16 - k as f64 * 0.07;
                let g = |k: &str, d: PyObject| -> R<String> { fmt(py, &info.call_method1("get", (k, d))?, "") };
                let label = format!("{}   ホスト {}  /  {} 人  /  {}:{}", g("world", po(py, "?"))?, g("host", po(py, "?"))?, g("players", po(py, 1))?,
                    fmt(py, &ip, "")?, fmt(py, &port, "")?);
                let bt = u.get_item("Button")?.call((), Some(&kw(py, vec![("parent", rows.clone().unbind()), ("text", po(py, label)), ("position", po(py, (-0.2, y))),
                    ("scale", po(py, (0.88, 0.06))), ("color", color(s)?.call_method1("rgb", (0.15, 0.25, 0.35))?.unbind()), ("text_size", po(py, 0.8))])?))?;
                let te = bt.getattr("text_entity")?;
                te.setattr("origin", (-0.5, 0))?;
                te.setattr("x", -0.47)?;
                bt.setattr("on_click", partial(jk.bind(py), vec![ip.unbind(), port.unbind()])?)?;
            }
            Ok(())
        })?;
        let (stk, rdk) = (state.clone().into_any().unbind(), redraw.clone().unbind());
        hook(py, &s.getattr("root")?, None, Some(clo0(py, move |py| {
            let st = stk.bind(py);
            let now: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
            if now - f64of(&st.get_item("t")?)? > 0.5 {
                st.set_item("t", imp(py, "time")?.call_method0("time")?)?;
                rdk.bind(py).call0()?;
            }
            Ok(())
        })?))?;
        redraw.call0()?;
        t(s, "IP アドレスで参加", (-0.64, -0.26), 0.85, L, None)?;
        let addr = u.get_item("InputField")?.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind()),
            ("default_value", mp.call_method1("get", ("last_host", ""))?.unbind()), ("position", po(py, (-0.12, -0.26))), ("scale", po(py, (0.36, 0.05))),
            ("character_limit", po(py, 40))])?))?;
        let (ak, mpk, jk) = (addr.unbind(), mp.clone().unbind(), join.clone().unbind());
        let join_addr = clo0(py, move |py| {
            let txt = field_text(ak.bind(py))?.call_method0("strip")?;
            if !txt.is_truthy()? {
                return Ok(());
            }
            let parts = txt.call_method1("partition", (":",))?;
            let (host, port) = (parts.get_item(0)?, parts.get_item(2)?);
            let mp = mpk.bind(py);
            mp.set_item("last_host", &txt)?;
            let int = py.import_bound("builtins")?.getattr("int")?;
            let p = if port.call_method0("strip")?.call_method0("isdigit")?.is_truthy()? {
                int.call1((&port,))?
            } else {
                int.call1((mp.call_method1("get", ("port", imp(py, "main.net")?.getattr("PORT")?))?,))?
            };
            jk.bind(py).call1((host.call_method0("strip")?, p))?;
            Ok(())
        })?;
        b(s, "参加", (0.18, -0.26), &join_addr, (0.16, 0.05), Some(rgb(s, 0.2, 0.5, 0.3)?), true)?;
        t(s, "ホストになるには：ワールドで遊んで Esc →「LAN に公開」。ワールド・セーブ・MOD はホストの PC のものを使い、\n\
            参加する人には MOD が自動で届きます。保存はホストが行います（参加者は F9 で構築物を自分の PC に出力できます）。", (0.0, -0.36), 0.62, C, Some(lgray()?))?;
        let sk = keep_(s);
        let back = clo0(py, move |py| {
            let s = sk.bind(py);
            s.call_method0("_close_browser")?;
            s.call_method0("_back")?;
            Ok(())
        })?;
        b(s, "戻る", (0.5, 0.42), &back, (0.2, 0.055), Some(rgb(s, 0.3, 0.3, 0.33)?), true)?;
        Self::_status_line(slf)
    }
    fn _close_browser(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let br = s.getattr("browser")?;
        if !br.is_none() {
            br.call_method0("close")?;
            s.setattr("browser", py.None())?;
        }
        Ok(())
    }
    /// 参加の準備中（MOD・ワールドを受け取っている）。
    fn show_joining(slf: &Bound<'_, Self>, joiner: &B<'_>, on_cancel: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        Self::_clear(slf)?;
        t(s, "マルチプレイに参加", (0.0, 0.2), 1.6, C, None)?;
        let msg = t(s, joiner.getattr("status")?.unbind(), (0.0, 0.05), 0.95, C, None)?;
        let (mk, jk) = (msg.unbind(), joiner.clone().unbind());
        hook(py, &s.getattr("root")?, None, Some(clo0(py, move |py| {
            let (m, j) = (mk.bind(py), jk.bind(py));
            let st = j.getattr("status")?;
            if !m.getattr("text")?.eq(&st)? {
                m.setattr("text", st)?;
            }
            Ok(())
        })?))?;
        b(s, "やめる", (0.0, -0.15), on_cancel, (0.25, 0.055), Some(rgb(s, 0.4, 0.3, 0.2)?), true)?;
        Ok(())
    }
    fn _select(slf: &Bound<'_, Self>, name: &B<'_>) -> R<()> {
        let s = slf.as_any();
        if name.eq(s.getattr("selected")?)? {
            // 同じワールドをもう一度押したら開始
            return Self::_play(slf);
        }
        s.setattr("selected", name)?;
        s.setattr("status", "")?;
        Self::show_main(slf)
    }
    fn _page(slf: &Bound<'_, Self>, d: i64) -> R<()> {
        let s = slf.as_any();
        let pi: i64 = s.getattr("page_index")?.extract()?;
        s.setattr("page_index", 0.max(pi + d))?;
        Self::show_main(slf)
    }
    fn _play(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let sel = s.getattr("selected")?;
        if sel.is_truthy()? {
            s.getattr("on_play")?.call1((sel,))?;
        }
        Ok(())
    }
    fn error(slf: &Bound<'_, Self>, msg: &B<'_>) -> R<()> {
        slf.as_any().setattr("status", msg)?;
        Self::show_main(slf)
    }
    /// MOD のエラーでワールドを始められなかったとき：エラーの一覧と、どうすればよいか。
    fn show_load_errors(slf: &Bound<'_, Self>, name: &B<'_>, errors: &B<'_>) -> R<()> {
        let s = slf.as_any();
        Self::_clear(slf)?;
        t(s, format!("「{}」を始められません（MOD のエラー）", name.str()?), (0.0, 0.42), 1.4, C, Some(rgb(s, 1.0, 0.55, 0.45)?))?;
        let errs: Vec<String> = errors.iter()?.map(|e| e?.extract::<String>()).collect::<R<_>>()?;
        let shown = &errs[..errs.len().min(12)];
        let mut lines: Vec<String> = shown.iter().map(|e| {
            let c: Vec<char> = e.chars().collect();
            if c.len() <= 96 { format!("・{}", e) } else { format!("・{}…", c[..94].iter().collect::<String>()) }
        }).collect();
        if errs.len() > shown.len() {
            lines.push(format!("…ほか {} 件（logs/latest.log に全部あります）", errs.len() - shown.len()));
        }
        t(s, lines.join("\n"), (-0.8, 0.32), 0.75, (-0.5, 0.5), None)?;
        t(s, "MOD はワールドを始めるときに一括で読み込みます。エラーがあるとワールドを始めません（セーブは変わっていません）。\n\
            ・MOD を作っている：エラーの場所を直して、もう一度「遊ぶ」を押してください。\n\
            ・MOD を外した／入れ替えた：ワールド設定で元の MOD を入れ直すか、変える前のバックアップに戻してください。\n\
            新しい MOD を作るとき・ワールドの MOD を変えるときは、先にワールド（saves/ のフォルダ）をコピーしておくことをすすめます。",
            (-0.8, -0.16), 0.7, (-0.5, 0.5), Some(cn(s, "light_gray")?))?;
        s.setattr("status", "")?;
        b(s, "戻る", (0.0, -0.42), &s.getattr("show_main")?, (0.25, 0.055), Some(rgb(s, 0.3, 0.3, 0.33)?), true)?;
        Ok(())
    }

    // --- 新規作成 -------------------------------------------------------
    #[pyo3(signature = (keep=false))]
    fn show_new(slf: &Bound<'_, Self>, keep: bool) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        Self::_clear(slf)?;
        let u = s.getattr("U")?;
        if !keep || s.getattr("_new").map(|x| x.is_none()).unwrap_or(true) {
            let names: Vec<String> = menu_list_worlds(py)?.iter().map(|w| w.get_item("name")?.extract()).collect::<R<_>>()?;
            let mut k = 1;
            while names.contains(&format!("world{}", k)) {
                k += 1;
            }
            let d = PyDict::new_bound(py);
            d.set_item("name", format!("world{}", k))?;
            d.set_item("planet", "gaia")?;
            d.set_item("voxel_size", 0.5)?;
            d.set_item("gravity_scale", 1.0)?;
            d.set_item("cleanup", true)?;
            d.set_item("full_rigid", true)?;
            d.set_item("mode", "creative")?;
            d.set_item("mods", PySet::empty_bound(py)?)?;
            s.setattr("_new", d)?;
        }
        let opts = s.getattr("_new")?;
        let lgray = || cn(s, "light_gray");
        t(s, "新しいワールド", (0.0, 0.42), 1.8, C, None)?;
        t(s, "名前（半角英数字・- _）", (-0.38, 0.3), 0.85, L, None)?;
        let field = u.get_item("InputField")?.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind()), ("default_value", opts.get_item("name")?.unbind()),
            ("position", po(py, (0.2, 0.3))), ("scale", po(py, (0.34, 0.05))), ("character_limit", po(py, 32)), ("active", po(py, true))])?))?;
        field.setattr("limit_content_to", NAME_CHARS)?;
        let fk = field.unbind();
        let name_now = clo(py, move |a| Ok(strip(&field_text(fk.bind(a.py()))?)?.into_py(a.py())))?;
        let mm = modmeta(py)?;
        let pre = mm.call_method1("world_preset", (sorted(py, &opts.get_item("mods")?)?,))?;
        let (pre_id, preset) = (pre.get_item(0)?, pre.get_item(1)?);
        let mut y = 0.21;
        let (sk, ok_, nk) = (keep_(s), opts.clone().unbind(), name_now.clone().unbind());
        let redraw = clo0(py, move |py| {
            let opts = ok_.bind(py);
            let n = nk.bind(py).call0()?;
            opts.set_item("name", if n.is_truthy()? { n } else { opts.get_item("name")? })?;
            sk.bind(py).call_method1("show_new", (true,))?;
            Ok(())
        })?;
        let set_redraw = |key: &str| -> R<B<'_>> {
            let (ok_, rk, key) = (opts.clone().unbind(), redraw.clone().unbind(), key.to_string());
            clo1(py, move |v| {
                let py = v.py();
                ok_.bind(py).set_item(&key, v)?;
                rk.bind(py).call0()?;
                Ok(())
            })
        };
        let setter = |key: &str| partial(&opts.getattr("__setitem__")?, vec![po(py, key)]);
        let modes = menu_mod(py)?.getattr("MODES")?.getattr("__getitem__")?;
        let mode = opts.call_method1("get", ("mode", "creative"))?;
        let mvals = PyList::new_bound(py, ["creative", "survival"]);
        Self::_stepper(slf, &po(py, "遊び方").into_bound(py), y, mvals.as_any(), &mode, &modes, &set_redraw("mode")?, 0.0)?;
        t(s, menu_mod(py)?.getattr("MODE_NOTES")?.get_item(&mode)?.unbind(), (0.2, y - 0.035), 0.6, C, Some(lgray()?))?;
        y -= 0.075;
        let fr = opts.get_item("full_rigid")?;
        if preset.is_truthy()? {
            t(s, format!("ワールドの形：{}", mm.call_method1("name_of", (&pre_id,))?.str()?), (-0.38, y), 0.85, L, Some(rgb(s, 1.0, 0.85, 0.45)?))?;
            t(s, preset.call_method1("get", ("note", "惑星・大きさは MOD が決めます"))?.unbind(), (-0.38, y - 0.045), 0.62, L, Some(lgray()?))?;
            y -= 0.07;
            Self::_toggle(slf, &po(py, "完全剛体モード").into_bound(py), y - 0.02, &fr, &set_redraw("full_rigid")?, 0.0)?;
            Self::_rigid_note(slf, (0.2, y - 0.055), fr.is_truthy()?, true, false)?;
            y -= 0.07;
        } else {
            let planets = menu_mod(py)?.getattr("PLANETS")?;
            let pl = PyList::new_bound(py, planets.iter()?.collect::<R<Vec<_>>>()?);
            Self::_stepper(slf, &po(py, "惑星").into_bound(py), y, pl.as_any(), &opts.get_item("planet")?, &planets.getattr("__getitem__")?, &setter("planet")?, 0.0)?;
            t(s, "地球型：山・洞窟・海・池がある", (0.2, y - 0.035), 0.6, C, Some(lgray()?))?;
            Self::_stepper(slf, &po(py, "ボクセルサイズ").into_bound(py), y - 0.07, PyList::new_bound(py, [0.25, 0.5, 1.0, 2.0]).as_any(), &opts.get_item("voxel_size")?,
                &clos(py, |v| Ok(format!("{} m", fmt(v.py(), v, "")?)))?, &setter("voxel_size")?, 0.0)?;
            Self::_stepper(slf, &po(py, "重力倍率").into_bound(py), y - 0.14, PyList::new_bound(py, [0.0, 0.25, 0.5, 1.0, 1.5, 2.0]).as_any(),
                &opts.get_item("gravity_scale")?, &clos(py, |v| Ok(format!("×{}", fmt(v.py(), v, "")?)))?, &setter("gravity_scale")?, 0.0)?;
            Self::_toggle(slf, &po(py, "構築物の自動削除").into_bound(py), y - 0.21, &opts.get_item("cleanup")?, &setter("cleanup")?, 0.0)?;
            Self::_toggle(slf, &po(py, "完全剛体モード").into_bound(py), y - 0.28, &fr, &set_redraw("full_rigid")?, 0.0)?;
            Self::_rigid_note(slf, (0.2, y - 0.315), fr.is_truthy()?, true, false)?;
            y -= 0.30;
        }
        y -= 0.09;
        let (sk, ok_, nk) = (keep_(s), opts.clone().unbind(), name_now.clone().unbind());
        let open_mods = clo0(py, move |py| {
            let (s, opts) = (sk.bind(py), ok_.bind(py));
            let n = nk.bind(py).call0()?;
            opts.set_item("name", if n.is_truthy()? { n } else { opts.get_item("name")? })?;
            let back = imp(py, "functools")?.getattr("partial")?.call((s.getattr("show_new")?,), Some(&kw(py, vec![("keep", po(py, true))])?))?;
            s.call_method1("show_mods", (opts.get_item("mods")?, back, "新しいワールドの MOD"))?;
            Ok(())
        })?;
        Self::_mod_summary(slf, y, &opts.get_item("mods")?, &open_mods)?;
        let (sk, ok_, nk) = (keep_(s), opts.clone().unbind(), name_now.clone().unbind());
        let create = clo1(py, move |play| {
            let py = play.py();
            let (s, opts) = (sk.bind(py), ok_.bind(py));
            let name = nk.bind(py).call0()?;
            if !name.is_truthy()? {
                s.setattr("status", "名前を入力してください")?;
                opts.set_item("name", "")?;
                s.call_method1("show_new", (true,))?;
                return Ok(());
            }
            let patch = PyDict::new_bound(py);
            patch.set_item("mode", opts.call_method1("get", ("mode", "creative"))?)?;
            patch.set_item("voxel_size", opts.get_item("voxel_size")?)?;
            patch.set_item("gravity_scale", opts.get_item("gravity_scale")?)?;
            let cl = PyDict::new_bound(py);
            cl.set_item("enabled", opts.get_item("cleanup")?)?;
            patch.set_item("cleanup", cl)?;
            patch.set_item("full_rigid", opts.call_method1("get", ("full_rigid", true))?.is_truthy()?)?;
            let r = menu_mod(py)?.call_method1("create_world", (&name, patch, sorted(py, &opts.get_item("mods")?)?, opts.get_item("planet")?));
            if let Err(e) = r {
                if !is_exc(py, &e) {
                    return Err(e);
                }
                lg(py)?.call_method("exception", ("ワールド作成失敗",), Some(&kw(py, vec![("exc_info", e.value_bound(py).clone().into_any().unbind())])?))?;
                s.setattr("status", format!("作成できません: {}", e.value_bound(py).str()?))?;
                opts.set_item("name", &name)?;
                s.call_method1("show_new", (true,))?;
                return Ok(());
            }
            s.setattr("_new", py.None())?;
            s.setattr("selected", &name)?;
            s.setattr("status", format!("「{}」を作成しました", name.str()?))?;
            if play.is_truthy()? {
                s.getattr("on_play")?.call1((&name,))?;
            } else {
                s.call_method0("show_main")?;
            }
            Ok(())
        })?;
        b(s, "作成して遊ぶ", (-0.22, -0.36), &partial(&create, vec![po(py, true)])?, BS, Some(rgb(s, 0.2, 0.5, 0.3)?), true)?;
        b(s, "作成", (0.12, -0.36), &partial(&create, vec![po(py, false)])?, BS, None, true)?;
        b(s, "戻る", (0.46, -0.36), &s.getattr("_back")?, (0.2, 0.055), Some(rgb(s, 0.3, 0.3, 0.33)?), true)?;
        Self::_status_line(slf)
    }
    /// 完全剛体モードの説明。OFF（やわらかいブロック）は重くなることがあるので、そのことを目立たせる。
    #[pyo3(signature = (pos, on, centered=false, short=false))]
    fn _rigid_note(slf: &Bound<'_, Self>, pos: (f64, f64), on: bool, centered: bool, short: bool) -> R<()> {
        let s = slf.as_any();
        let (txt, col) = if on {
            (if short { "1つの剛体で軽い（おすすめ）" } else { "ON：構築物は丸ごと1つの剛体（曲がらない・壊れない。軽い。おすすめ）" }, cn(s, "light_gray")?)
        } else {
            (if short { "やわらかい。負荷が増え重くなることも" } else { "OFF：やわらかいブロック（曲がる・ちぎれる）。負荷が増え、重くなることがあります" }, cn(s, "orange")?)
        };
        t(s, txt, pos, 0.58, if centered { C } else { L }, Some(col))?;
        Ok(())
    }
    /// MOD の行：選んでいる MOD の名前と「MOD を選ぶ…」ボタン（MOD の画面へ）。
    fn _mod_summary(slf: &Bound<'_, Self>, y: f64, selected: &B<'_>, open_mods: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let mm = modmeta(py)?;
        t(s, "MOD", (-0.38, y), 0.85, L, None)?;
        let order = mm.call_method1("resolve", (sorted(py, selected)?,))?.get_item(0)?;
        let mut names: Vec<String> = vec![];
        for m in order.iter()? {
            names.push(mm.call_method1("name_of", (m?,))?.str()?.to_string());
        }
        if names.is_empty() {
            names.push("なし".into());
        }
        let txt = names.join("、");
        let c: Vec<char> = txt.chars().collect();
        let txt = if c.len() > 40 { c[..38].iter().collect::<String>() + "…" } else { txt };
        t(s, txt, (-0.25, y), 0.7, L, Some(cn(s, "light_gray")?))?;
        t(s, format!("{} 個", order.len()?), (-0.25, y - 0.035), 0.58, L, Some(cn(s, "gray")?))?;
        b(s, "MOD を選ぶ…", (0.55, y - 0.012), open_mods, (0.22, 0.055), Some(rgb(s, 0.3, 0.38, 0.55)?), true)?;
        Ok(())
    }

    // --- MOD の選択（新規作成・ワールド設定から） --------------------------------
    /// MOD の一覧（説明・必要な MOD）。押すと入れる／外す。入れると必要な MOD も一緒に入る。
    #[pyo3(signature = (selected, done, title="MOD".to_string(), page=0, note="".to_string()))]
    fn show_mods(slf: &Bound<'_, Self>, selected: &B<'_>, done: &B<'_>, title: String, page: i64, note: String) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        Self::_clear(slf)?;
        let mm = modmeta(py)?;
        let mods: Vec<B<'_>> = mm.getattr("list_all")?.call((), Some(&kw(py, vec![("kind", po(py, "addon"))])?))?.iter()?.collect::<R<_>>()?; // シナリオはメニューの「シナリオ」から選ぶ
        let (on_col, off_col) = (rgb(s, 0.25, 0.5, 0.35)?, rgb(s, 0.2, 0.2, 0.24)?);
        let lgray = || cn(s, "light_gray");
        t(s, title.as_str(), (0.0, 0.44), 1.6, C, None)?;
        t(s, "押すと入れる・外す。ほかの MOD が必要な MOD を入れると、必要な MOD も一緒に入ります（★＝必要として入った）", (0.0, 0.385), 0.62, C, Some(lgray()?))?;
        let order = mm.call_method1("resolve", (sorted(py, selected)?,))?.get_item(0)?;
        let mut auto: Vec<B<'_>> = vec![];
        for m in order.iter()? {
            let m = m?;
            if !selected.contains(&m)? {
                auto.push(m);
            }
        }
        let n = mods.len() as i64;
        let pages = 1.max((n + MOD_ROWS - 1) / MOD_ROWS);
        let page = 0.max(page.min(pages - 1));
        let names = PyDict::new_bound(py);
        for m in &mods {
            let id = m.get_item("id")?;
            names.set_item(&id, m.call_method1("get", ("name", &id))?)?;
        }
        let (sk, selk, dk, ordk, nk, tt) = (keep_(s), selected.clone().unbind(), done.clone().unbind(), order.clone().unbind(), names.clone().into_any().unbind(), title.clone());
        let flip = clo1(py, move |mid| {
            let py = mid.py();
            let (s, selected, order, names) = (sk.bind(py), selk.bind(py), ordk.bind(py), nk.bind(py));
            let mm = modmeta(py)?;
            let nm = |m: &B<'_>| -> R<String> { names.call_method1("get", (m, m))?.str().map(|x| x.to_string()) };
            let mut msg = String::new();
            if order.contains(mid)? {
                let mut need = vec![];
                for m in mm.call_method1("dependents", (mid, order))?.iter()? {
                    let m = m?;
                    if order.contains(&m)? {
                        need.push(m);
                    }
                }
                selected.call_method1("discard", (mid,))?;
                if !need.is_empty() {
                    for m in &need {
                        selected.call_method1("discard", (m,))?;
                    }
                    msg = format!("「{}」を使う {} も外しました", nm(mid)?, need.iter().map(nm).collect::<R<Vec<_>>>()?.join("、"));
                }
            } else {
                selected.call_method1("add", (mid,))?;
                let r = mm.call_method1("resolve", (sorted(py, selected)?,))?;
                let (o2, miss) = (r.get_item(0)?, r.get_item(2)?);
                let mut add = vec![];
                for m in o2.iter()? {
                    let m = m?;
                    if !order.contains(&m)? && !m.eq(mid)? {
                        add.push(m);
                    }
                }
                if !add.is_empty() {
                    msg = format!("「{}」に必要な {} も入れました", nm(mid)?, add.iter().map(nm).collect::<R<Vec<_>>>()?.join("、"));
                }
                if miss.is_truthy()? {
                    let mut ms = vec![];
                    for x in miss.iter()? {
                        ms.push(x?.get_item(0)?.str()?.to_string());
                    }
                    msg += &format!("（見つからない MOD：{}）", ms.join("、"));
                }
            }
            s.call_method1("show_mods", (selected, dk.bind(py), tt.as_str(), page, msg))?;
            Ok(())
        })?;
        let auto_has = |m: &B<'_>| -> R<bool> {
            for a in &auto {
                if a.eq(m)? {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        for (k, i) in ((page * MOD_ROWS)..((page + 1) * MOD_ROWS).min(n)).enumerate() {
            let m = &mods[i as usize];
            let y = 0.30 - k as f64 * 0.11;
            let mid = m.get_item("id")?;
            let on = order.contains(&mid)?;
            let mark = format!("{}{}", if auto_has(&mid)? { "★ " } else { "" }, if on { "ON" } else { "OFF" });
            panel(s, (0.0, y - 0.018), (1.62, 0.1), Some(color(s)?.call_method1("rgba", (1, 1, 1, if !on { 0.05 } else { 0.09 }))?))?;
            b(s, &mark, (-0.72, y - 0.012), &partial(&flip, vec![mid.clone().unbind()])?, (0.12, 0.05), Some(if on { on_col.clone() } else { off_col.clone() }), true)?;
            let cat = m.call_method1("get", ("category",))?;
            let cat = if cat.is_truthy()? { format!("［{}］ ", m.get_item("category")?.str()?) } else { String::new() };
            t(s, format!("{}{}", cat, fmt(py, &m.call_method1("get", ("name", &mid))?, "")?), (-0.64, y + 0.012), 0.85, L, None)?;
            let d0 = m.call_method1("get", ("description",))?;
            let desc = if d0.is_truthy()? { d0.str()?.to_string().replace('\n', " ") } else { String::new() };
            let dc: Vec<char> = desc.chars().collect();
            let desc = if dc.len() > 92 { dc[..90].iter().collect::<String>() + "…" } else { desc };
            t(s, desc, (-0.64, y - 0.022), 0.58, L, Some(lgray()?))?;
            let mut extra: Vec<String> = vec![];
            let req = m.get_item("requires")?;
            if req.is_truthy()? {
                let mut rs = vec![];
                for r in req.iter()? {
                    let r = r?;
                    rs.push(match names.get_item(&r)? {
                        Some(x) => x.str()?.to_string(),
                        None => format!("{}（見つかりません）", r.str()?),
                    });
                }
                extra.push(format!("必要な MOD：{}", rs.join("、")));
            }
            if m.call_method1("get", ("world",))?.is_instance_of::<PyDict>() {
                extra.push("新しいワールドを作るときに入れると、このワールドの形になります".into());
            }
            let er = m.call_method1("get", ("error",))?;
            if er.is_truthy()? {
                extra.push(er.str()?.to_string());
            }
            if !extra.is_empty() {
                t(s, extra.join("　"), (-0.64, y - 0.05), 0.56, L, Some(rgb(s, 1.0, 0.82, 0.45)?))?;
            }
        }
        if pages > 1 {
            let sm = s.getattr("show_mods")?;
            b(s, "◀ 前", (-0.2, -0.36), &partial(&sm, vec![selected.clone().unbind(), done.clone().unbind(), po(py, title.as_str()), po(py, page - 1)])?,
                (0.14, 0.05), None, true)?;
            t(s, format!("{}/{}", page + 1, pages), (0.0, -0.36), 0.8, C, None)?;
            b(s, "次 ▶", (0.2, -0.36), &partial(&sm, vec![selected.clone().unbind(), done.clone().unbind(), po(py, title.as_str()), po(py, page + 1)])?,
                (0.14, 0.05), None, true)?;
        }
        if !note.is_empty() {
            t(s, note.as_str(), (0.0, -0.30), 0.7, C, Some(cn(s, "orange")?))?;
        }
        t(s, format!("入る MOD：{} 個", order.len()?), (-0.72, -0.42), 0.7, L, Some(lgray()?))?;
        b(s, "決定", (0.62, -0.42), done, (0.2, 0.055), Some(rgb(s, 0.2, 0.5, 0.3)?), true)?;
        Ok(())
    }
    fn _back(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("status", "")?;
        // 保存せずに戻った：音量を元に
        match imp(py, "main.client.sound").and_then(|m| m.call_method1("get", (s.getattr("cfg")?,))) {
            Ok(_) => {}
            Err(e) if is_exc(py, &e) => {}
            Err(e) => return Err(e),
        }
        s.setattr("_new", py.None())?;
        s.setattr("_ws", py.None())?;
        Self::show_main(slf)
    }

    // --- 削除 -----------------------------------------------------------
    fn show_delete(slf: &Bound<'_, Self>, name: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if !name.is_truthy()? {
            return Ok(());
        }
        Self::_clear(slf)?;
        t(s, "ワールドの削除", (0.0, 0.25), 1.8, C, None)?;
        t(s, format!("「{}」を削除しますか？\nセーブデータ・スクリプト・出力したOBJもすべて消え、元に戻せません。", name.str()?), (0.0, 0.1), 1.0, C, None)?;
        let (sk, nk) = (keep_(s), name.clone().unbind());
        let doit = clo0(py, move |py| {
            let (s, name) = (sk.bind(py), nk.bind(py));
            match menu_mod(py)?.call_method1("delete_world", (name,)) {
                Ok(done) => {
                    s.setattr("status", format!("「{}」を削除しました{}", name.str()?,
                        if done.is_truthy()? { "" } else { "（使用中で消せなかったファイルは次回起動時に片付けます）" }))?;
                    s.setattr("selected", py.None())?;
                }
                Err(e) if is_exc(py, &e) => {
                    lg(py)?.call_method("exception", ("削除失敗",), Some(&kw(py, vec![("exc_info", e.value_bound(py).clone().into_any().unbind())])?))?;
                    s.setattr("status", format!("削除できません: {}", e.value_bound(py).str()?))?;
                }
                Err(e) => return Err(e),
            }
            s.call_method0("show_main")?;
            Ok(())
        })?;
        b(s, "削除する", (-0.18, -0.08), &doit, BS, Some(rgb(s, 0.6, 0.2, 0.2)?), true)?;
        b(s, "やめる", (0.18, -0.08), &s.getattr("_back")?, BS, None, true)?;
        Ok(())
    }

    // --- ワールド設定 ---------------------------------------------------
    #[pyo3(signature = (name, keep=false))]
    fn show_world_settings(slf: &Bound<'_, Self>, name: &B<'_>, keep: bool) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if !name.is_truthy()? {
            return Ok(());
        }
        Self::_clear(slf)?;
        let ws = s.getattr("_ws").unwrap_or_else(|_| py.None().into_bound(py));
        let (st, sel) = if keep && ws.is_truthy()? && ws.get_item(0)?.eq(name)? {
            (ws.get_item(1)?, ws.get_item(2)?)
        } else {
            let r = menu_mod(py)?.call_method1("read_world_settings", (name,))?;
            (r.get_item(0)?, PySet::new_bound(py, &r.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?)?.into_any())
        };
        let cl = st.get_item("cleanup")?;
        let lgray = || cn(s, "light_gray");
        t(s, format!("ワールド設定: {}", name.str()?), (0.0, 0.42), 1.6, C, None)?;
        let mut y = 0.31;
        let (sk, nk, stk, selk) = (keep_(s), name.clone().unbind(), st.clone().unbind(), sel.clone().unbind());
        let redraw = clo0(py, move |py| {
            let s = sk.bind(py);
            s.setattr("_ws", PyTuple::new_bound(py, [nk.clone_ref(py), stk.clone_ref(py), selk.clone_ref(py)]))?;
            s.call_method1("show_world_settings", (nk.bind(py), true))?;
            Ok(())
        })?;
        let set_redraw = |key: &str| -> R<B<'_>> {
            let (stk, rk, key) = (st.clone().unbind(), redraw.clone().unbind(), key.to_string());
            clo1(py, move |v| {
                let py = v.py();
                stk.bind(py).set_item(&key, v)?;
                rk.bind(py).call0()?;
                Ok(())
            })
        };
        let mode = st.call_method1("get", ("mode", "creative"))?;
        Self::_stepper(slf, &po(py, "遊び方").into_bound(py), y, PyList::new_bound(py, ["creative", "survival"]).as_any(), &mode,
            &menu_mod(py)?.getattr("MODES")?.getattr("__getitem__")?, &set_redraw("mode")?, 0.0)?;
        t(s, menu_mod(py)?.getattr("MODE_NOTES")?.get_item(&mode)?.unbind(), (0.43, y), 0.62, L, Some(lgray()?))?;
        let fr = st.call_method1("get", ("full_rigid", true))?;
        Self::_toggle(slf, &po(py, "完全剛体モード").into_bound(py), y - 0.065, &fr, &set_redraw("full_rigid")?, 0.0)?;
        Self::_rigid_note(slf, (0.43, y - 0.065), fr.is_truthy()?, false, true)?;
        y -= 0.065;
        let float = py.import_bound("builtins")?.getattr("float")?;
        let int = py.import_bound("builtins")?.getattr("int")?;
        Self::_stepper(slf, &po(py, "ボクセルサイズ（新規設置）").into_bound(py), y - 0.065, PyList::new_bound(py, [0.25, 0.5, 1.0, 2.0]).as_any(),
            &st.call_method1("get", ("voxel_size", 0.5))?, &clos(py, |v| Ok(format!("{} m", fmt(v.py(), v, "")?)))?, &setter(py, &st, "voxel_size")?, 0.0)?;
        Self::_stepper(slf, &po(py, "重力倍率").into_bound(py), y - 0.13, PyList::new_bound(py, [0.0, 0.25, 0.5, 1.0, 1.5, 2.0]).as_any(),
            &st.call_method1("get", ("gravity_scale", 1.0))?, &clos(py, |v| Ok(format!("×{}", fmt(v.py(), v, "")?)))?, &setter(py, &st, "gravity_scale")?, 0.0)?;
        Self::_toggle(slf, &po(py, "構築物の自動削除").into_bound(py), y - 0.195, &cl.call_method1("get", ("enabled", true))?, &setter(py, &cl, "enabled")?, 0.0)?;
        Self::_stepper(slf, &po(py, "　削除する距離").into_bound(py), y - 0.26, PyList::new_bound(py, [200.0, 500.0, 1000.0, 1500.0, 3000.0, 5000.0, 10000.0]).as_any(),
            &float.call1((cl.call_method1("get", ("distance", 1500))?,))?, &clos(py, |v| Ok(format!("{} m", fmt(v.py(), v, ".0f")?)))?, &setter(py, &cl, "distance")?, 0.0)?;
        Self::_stepper(slf, &po(py, "　離れている時間").into_bound(py), y - 0.325, PyList::new_bound(py, [10.0, 30.0, 60.0, 120.0, 300.0, 600.0]).as_any(),
            &float.call1((cl.call_method1("get", ("seconds", 60))?,))?, &clos(py, |v| Ok(format!("{} 秒", fmt(v.py(), v, ".0f")?)))?, &setter(py, &cl, "seconds")?, 0.0)?;
        Self::_stepper(slf, &po(py, "　対象の最大頂点数").into_bound(py), y - 0.39, PyList::new_bound(py, [80, 200, 400, 800, 1600, 4000]).as_any(),
            &int.call1((cl.call_method1("get", ("max_vertices", 400))?,))?, &clos(py, |v| fmt(v.py(), v, ""))?, &setter(py, &cl, "max_vertices")?, 0.0)?;
        let (sk, nk, stk, selk) = (keep_(s), name.clone().unbind(), st.clone().unbind(), sel.clone().unbind());
        let open_mods = clo0(py, move |py| {
            let s = sk.bind(py);
            s.setattr("_ws", PyTuple::new_bound(py, [nk.clone_ref(py), stk.clone_ref(py), selk.clone_ref(py)]))?;
            let k = kw(py, vec![("keep", po(py, true))])?;
            let back = imp(py, "functools")?.getattr("partial")?.call((s.getattr("show_world_settings")?, nk.clone_ref(py)), Some(&k))?;
            s.call_method1("show_mods", (selk.bind(py), back, format!("ワールド「{}」の MOD", nk.bind(py).str()?)))?;
            Ok(())
        })?;
        Self::_mod_summary(slf, y - 0.465, &sel, &open_mods)?;
        let mut ps = vec![];
        for p in st.call_method1("get", ("planets", PyList::empty_bound(py)))?.iter()? {
            ps.push(p?.call_method1("get", ("def", "?"))?.str()?.to_string());
        }
        t(s, format!("惑星: {}（world.json で編集）", ps.join(", ")), (0.0, y - 0.6), 0.7, C, Some(lgray()?))?;
        t(s, "MOD を入れる・外す前に、ワールド（saves/ のフォルダ）をコピーしておくことをすすめます。\
            使っている MOD を外すと、そのワールドは始められなくなります。", (0.0, y - 0.66), 0.58, C, Some(rgb(s, 1.0, 0.8, 0.45)?))?;
        let (sk, nk, stk, selk) = (keep_(s), name.clone().unbind(), st.clone().unbind(), sel.clone().unbind());
        let save = clo0(py, move |py| {
            let s = sk.bind(py);
            let r = (|| -> R<()> {
                let order = modmeta(py)?.call_method1("resolve", (sorted(py, selk.bind(py))?,))?.get_item(0)?;
                menu_mod(py)?.call_method1("write_world_settings", (nk.bind(py), stk.bind(py), order))?;
                s.setattr("_ws", py.None())?;
                s.setattr("status", "ワールド設定を保存しました")?;
                Ok(())
            })();
            match r {
                Ok(()) => {}
                Err(e) if is_exc(py, &e) => s.setattr("status", format!("保存できません: {}", e.value_bound(py).str()?))?,
                Err(e) => return Err(e),
            }
            s.call_method0("show_main")?;
            Ok(())
        })?;
        b(s, "保存", (0.72, 0.42), &save, (0.2, 0.055), Some(rgb(s, 0.2, 0.5, 0.3)?), true)?;
        b(s, "戻る", (0.72, 0.35), &s.getattr("_back")?, (0.2, 0.055), Some(rgb(s, 0.3, 0.3, 0.33)?), true)?;
        Ok(())
    }

    // --- ゲーム設定 -----------------------------------------------------
    fn show_game_settings(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        Self::_clear(slf)?;
        let json = imp(py, "json")?;
        let cfg = json.call_method1("loads", (json.call_method1("dumps", (s.getattr("cfg")?,))?,))?;
        let (w, g, c, ph) = (cfg.get_item("window")?, cfg.get_item("graphics")?, cfg.get_item("controls")?, cfg.get_item("physics")?);
        t(s, "ゲーム設定", (0.0, 0.42), 1.8, C, None)?;
        let y = 0.31;
        let res = PyTuple::new_bound(py, [w.call_method1("get", ("width", 1280))?, w.call_method1("get", ("height", 720))?]);
        let resl = menu_mod(py)?.getattr("RESOLUTIONS")?;
        let res_opts = if resl.contains(&res)? { resl } else { PyList::new_bound(py, [res.clone().into_any()]).call_method1("__add__", (resl,))? };
        let wk = w.clone().unbind();
        let set_res = clo1(py, move |v| {
            let w = wk.bind(v.py());
            w.set_item("width", v.get_item(0)?)?;
            w.set_item("height", v.get_item(1)?)?;
            Ok(())
        })?;
        Self::_stepper(slf, &po(py, "解像度").into_bound(py), y, &res_opts, res.as_any(),
            &clos(py, |v| Ok(format!("{}×{}", fmt(v.py(), &v.get_item(0)?, "")?, fmt(v.py(), &v.get_item(1)?, "")?)))?, &set_res, 0.0)?;
        let ad = PyDict::new_bound(py);
        ad.set_item("bgm", 60)?;
        ad.set_item("sfx", 80)?;
        let a = cfg.call_method1("setdefault", ("audio", ad))?;
        let pct = PyList::new_bound(py, (0..=100).step_by(10).collect::<Vec<i64>>());
        let int = py.import_bound("builtins")?.getattr("int")?;
        let float = py.import_bound("builtins")?.getattr("float")?;
        let pf = |suffix: &'static str| clos(py, move |v| Ok(format!("{}{}", fmt(v.py(), v, "")?, suffix)));
        let geti = |d: &B<'_>, k: &str, dv: i64| -> R<B<'_>> { int.call1((d.call_method1("get", (k, dv))?,)) };
        let getdist = |d: &B<'_>, k: &str| -> R<B<'_>> {
            let v = d.call_method1("get", (k, 0))?;
            float.call1((if v.is_truthy()? { v } else { po(py, 0).into_bound(py) },))
        };
        let dist = menu_mod(py)?.getattr("_dist_text")?;
        let sv = s.getattr("_set_volume")?;
        let ak = a.clone().unbind();
        let ck = c.clone().unbind();
        let ms = clo1(py, move |v| {
            let py = v.py();
            ck.bind(py).set_item("mouse_sensitivity", py.import_bound("builtins")?.getattr("float")?.call1((v,))?)?;
            Ok(())
        })?;
        let lab = |x: &str| po(py, x).into_bound(py);
        let yy = |k: i64| y - 0.056 * (k + 1) as f64;
        Self::_toggle(slf, &lab("フルスクリーン"), yy(0), &w.call_method1("get", ("fullscreen", false))?, &setter(py, &w, "fullscreen")?, 0.0)?;
        Self::_toggle(slf, &lab("垂直同期（再起動後）"), yy(1), &w.call_method1("get", ("vsync", true))?, &setter(py, &w, "vsync")?, 0.0)?;
        Self::_stepper(slf, &lab("BGM の音量"), yy(2), pct.as_any(), &geti(&a, "bgm", 60)?, &pf(" %")?, &partial(&sv, vec![ak.clone_ref(py), po(py, "bgm")])?, 0.0)?;
        Self::_stepper(slf, &lab("効果音の音量"), yy(3), pct.as_any(), &geti(&a, "sfx", 80)?, &pf(" %")?, &partial(&sv, vec![ak.clone_ref(py), po(py, "sfx")])?, 0.0)?;
        Self::_stepper(slf, &lab("視野角"), yy(4), PyList::new_bound(py, (50..115).step_by(5).collect::<Vec<i64>>()).as_any(), &geti(&g, "fov", 80)?, &pf("°")?,
            &setter(py, &g, "fov")?, 0.0)?;
        Self::_stepper(slf, &lab("描画距離"), yy(5), PyList::new_bound(py, [500, 1000, 2000, 3000, 5000, 10000, 20000]).as_any(), &geti(&g, "view_distance", 3000)?,
            &pf(" m")?, &setter(py, &g, "view_distance")?, 0.0)?;
        Self::_slider(slf, &lab("構築物の読み込み距離"), yy(6), &menu_mod(py)?.getattr("LOAD_STEPS")?, &getdist(&g, "load_distance")?, &dist,
            &setter(py, &g, "load_distance")?, 0.0)?;
        Self::_slider(slf, &lab("機能処理の距離"), yy(7), &menu_mod(py)?.getattr("PROCESS_STEPS")?, &getdist(&ph, "process_distance")?, &dist,
            &setter(py, &ph, "process_distance")?, 0.0)?;
        Self::_stepper(slf, &lab("地表の細かさ"), yy(8), PyList::new_bound(py, [24, 32, 48, 64, 96]).as_any(), &geti(&g, "terrain_patch_cells", 48)?,
            &clos(py, |v| {
                let n: i64 = v.extract()?;
                Ok(format!("{}×{}", n * 2 + 1, n * 2 + 1))
            })?, &setter(py, &g, "terrain_patch_cells")?, 0.0)?;
        Self::_stepper(slf, &lab("マウス感度"), yy(9), PyList::new_bound(py, (10..210).step_by(10).collect::<Vec<i64>>()).as_any(), &geti(&c, "mouse_sensitivity", 60)?,
            &pf("")?, &ms, 0.0)?;
        Self::_stepper(slf, &lab("物理サブステップ（多いほど正確・重い）"), yy(10), PyList::new_bound(py, [1, 2, 3, 4, 6, 8]).as_any(), &geti(&ph, "substeps", 3)?,
            &pf("")?, &setter(py, &ph, "substeps")?, 0.0)?;
        Self::_stepper(slf, &lab("物理反復回数（多いほど硬い・重い）"), yy(11), PyList::new_bound(py, [1, 2, 3, 4, 6]).as_any(), &geti(&ph, "iterations", 2)?,
            &pf("")?, &setter(py, &ph, "iterations")?, 0.0)?;
        Self::_toggle(slf, &lab("仮想ジョイスティック（タッチ操作）"), yy(12), &c.call_method1("get", ("touch", false))?, &setter(py, &c, "touch")?, 0.0)?;
        let (sk, ck) = (keep_(s), c.clone().unbind());
        let keys = clo0(py, move |py| {
            let s = sk.bind(py);
            let kp = imp(py, "main.client.keys_panel")?;
            let (sk2, ck2) = (s.clone().unbind(), ck.clone_ref(py));
            let changed = clo0(py, move |py| {
                let (s, c) = (sk2.bind(py), ck2.bind(py));
                let cc = s.getattr("cfg")?.get_item("controls")?;
                for a in imp(py, "main.client.keys_panel")?.getattr("ACTIONS")?.iter()? {
                    let aid = a?.get_item(0)?;
                    c.set_item(&aid, cc.call_method1("get", (&aid, c.call_method1("get", (&aid,))?))?)?;
                }
                Ok(())
            })?;
            kp.getattr("KeysPanel")?.call((s.getattr("U")?, s.getattr("cfg")?, s.getattr("root")?, py.None()), Some(&kw(py, vec![("on_change", changed.unbind())])?))?;
            Ok(())
        })?;
        b(s, "操作とキー設定", (0.62, -0.09), &keys, (0.2, 0.055), Some(rgb(s, 0.25, 0.35, 0.45)?), true)?;
        t(s, "フォントは config/settings.json で変更できます。読み込み距離より遠い構築物は色の箱で描き、\
            機能処理の距離より遠い構築物は止まります（いちばん右で全部）", (0.0, y - 0.77), 0.55, C, Some(cn(s, "light_gray")?))?;
        let (sk, cfk) = (keep_(s), cfg.clone().unbind());
        let save = clo0(py, move |py| {
            let s = sk.bind(py);
            let me = s.getattr("cfg")?;
            let prev = PyDict::new_bound(py);
            prev.call_method1("update", (me.call_method1("get", ("window", PyDict::new_bound(py)))?,))?;
            me.call_method0("clear")?;
            me.call_method1("update", (cfk.bind(py),))?;
            let r = (|| -> R<()> {
                let m = menu_mod(py)?;
                m.call_method1("save_config", (&me,))?;
                m.call_method1("apply_window_settings", (&me, &prev))?;
                s.setattr("status", "ゲーム設定を保存しました")?;
                Ok(())
            })();
            match r {
                Ok(()) => {}
                Err(e) if is_exc(py, &e) => s.setattr("status", format!("保存できません: {}", e.value_bound(py).str()?))?,
                Err(e) => return Err(e),
            }
            s.call_method0("show_main")?;
            Ok(())
        })?;
        let sk = keep_(s);
        let reset = clo0(py, move |py| {
            let s = sk.bind(py);
            let me = s.getattr("cfg")?;
            me.call_method0("clear")?;
            let json = imp(py, "json")?;
            me.call_method1("update", (json.call_method1("loads", (json.call_method1("dumps", (imp(py, "main.config")?.getattr("DEFAULTS")?,))?,))?,))?;
            s.call_method0("show_game_settings")?;
            Ok(())
        })?;
        b(s, "保存", (0.62, -0.17), &save, (0.2, 0.055), Some(rgb(s, 0.2, 0.5, 0.3)?), true)?;
        b(s, "初期値に戻す", (0.62, -0.25), &reset, (0.2, 0.055), None, true)?;
        b(s, "戻る", (0.62, -0.33), &s.getattr("_back")?, (0.2, 0.055), Some(rgb(s, 0.3, 0.3, 0.33)?), true)?;
        Ok(())
    }
    /// BGM・効果音の音量：選んだらすぐ聞こえ方を変える（保存は「保存」で）。
    fn _set_volume(slf: &Bound<'_, Self>, audio_cfg: &B<'_>, key: &str, v: &B<'_>) -> R<()> {
        let py = slf.py();
        audio_cfg.set_item(key, py.import_bound("builtins")?.getattr("int")?.call1((v,))?)?;
        let r = (|| -> R<()> {
            let sound = imp(py, "main.client.sound")?;
            let d = PyDict::new_bound(py);
            d.set_item("audio", audio_cfg)?;
            sound.call_method1("get", (d,))?;
            if key == "sfx" {
                sound.call_method0("get")?.call_method1("sfx", ("click",))?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => Ok(()),
            Err(e) if is_exc(py, &e) => Ok(()),
            Err(e) => Err(e),
        }
    }
    fn destroy(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        Self::_close_browser(slf)?;
        super::client_misc::ui_destroy(py, &slf.as_any().getattr("root")?, &PyTuple::empty_bound(py), None)
    }
}

/// 棒の値（_slider）
struct Slider {
    i: std::cell::Cell<i64>,
    n: i64,
    x0: f64,
    x1: f64,
    val: PyObject,
    values: PyObject,
    fmt: PyObject,
    on_change: PyObject,
    knob: PyObject,
}
impl Slider {
    fn show(&self, py: Python<'_>) -> R<()> {
        let i = self.i.get();
        self.knob.bind(py).setattr("x", self.x0 + (self.x1 - self.x0) * i as f64 / ((self.n - 1).max(1)) as f64)?;
        self.val.bind(py).setattr("text", self.fmt.bind(py).call1((self.values.bind(py).get_item(i)?,))?)?;
        Ok(())
    }
    fn set_i(&self, py: Python<'_>, i: i64) -> R<()> {
        self.i.set(0.max((self.n - 1).min(i)));
        self.show(py)?;
        self.on_change.bind(py).call1((self.values.bind(py).get_item(self.i.get())?,))?;
        Ok(())
    }
}
fn set_i_rel(py: Python<'_>, c: &std::rc::Rc<Slider>, d: i64) -> R<()> {
    c.set_i(py, c.i.get() + d)
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<MainMenu>()?;
    for f in [wrap_pyfunction_bound!(menu_dist_text, m)?, wrap_pyfunction_bound!(menu_list_worlds, m)?, wrap_pyfunction_bound!(menu_world_mode_text, m)?,
              wrap_pyfunction_bound!(menu_list_mods, m)?, wrap_pyfunction_bound!(menu_read_world_settings, m)?, wrap_pyfunction_bound!(menu_write_world_settings, m)?,
              wrap_pyfunction_bound!(menu_create_world, m)?, wrap_pyfunction_bound!(menu_rmtree_retry, m)?, wrap_pyfunction_bound!(menu_delete_world, m)?,
              wrap_pyfunction_bound!(menu_purge_trash, m)?, wrap_pyfunction_bound!(menu_save_config, m)?, wrap_pyfunction_bound!(menu_default_name, m)?,
              wrap_pyfunction_bound!(menu_apply_window_settings, m)?] {
        m.add_function(f)?;
    }
    Ok(())
}
