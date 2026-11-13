//! MOD の情報（mods/<MOD名>/mod.json）と、MOD どうしの依存関係。main/modmeta.py の中身。
//! 設定ファイル（config/settings.json）の読み書き（main/config.py の中身）もここ。

use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::net::{parse_json, val_to_py};

type R<T> = PyResult<T>;

pub const TYPES: [&str; 3] = ["addon", "scenario", "assets"];
/// 前の版では MOD だった本体の遊び方（mods/ に残っていても一覧に出さない）
pub const BUILTIN_MODES: [&str; 1] = ["survival"];

fn root<'py>(py: Python<'py>, mods_dir: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
    match mods_dir.filter(|d| d.is_truthy().unwrap_or(false)) {
        Some(d) => Ok(d.clone()),
        None => crate::game::imp(py, "main.paths")?.getattr("MODS"),
    }
}

fn read_json_file<'py>(py: Python<'py>, p: &PathBuf) -> Result<Bound<'py, PyAny>, String> {
    let b = std::fs::read(p).map_err(|e| e.to_string())?;
    let t = String::from_utf8_lossy(&b).into_owned();
    let v = parse_json(t.trim_start_matches('\u{feff}').as_bytes())?;
    val_to_py(py, &v, &[]).map(|x| x.into_bound(py)).map_err(|e| e.to_string())
}

/// mod.json の中身（無い・読めない MOD は最低限の項目だけ）
#[pyfunction]
#[pyo3(signature = (mod_id, mods_dir=None))]
pub fn modmeta_read<'py>(py: Python<'py>, mod_id: &str, mods_dir: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyDict>> {
    let d = root(py, mods_dir)?.call_method1("__truediv__", (mod_id,))?;
    let dp = PathBuf::from(d.str()?.to_string());
    let meta = PyDict::new_bound(py);
    meta.set_item("id", mod_id)?;
    meta.set_item("name", mod_id)?;
    meta.set_item("description", "")?;
    meta.set_item("requires", PyList::empty_bound(py))?;
    meta.set_item("builtin", false)?;
    let mj = dp.join("mod.json");
    if mj.exists() {
        match read_json_file(py, &mj) {
            Ok(raw) => {
                if let Ok(rd) = raw.downcast::<PyDict>() {
                    meta.update(rd.as_mapping())?;
                }
            }
            Err(e) => meta.set_item("error", format!("mod.json が読めません: {e}"))?,
        }
    }
    meta.set_item("id", mod_id)?; // フォルダの名前が ID
    let req = meta.get_item("requires")?;
    let reqs = PyList::empty_bound(py);
    if let Some(r) = req {
        if r.is_truthy()? {
            if let Ok(l) = r.downcast::<PyList>() {
                for x in l.iter() {
                    if let Ok(s) = x.extract::<String>() {
                        if s != mod_id {
                            reqs.append(s)?;
                        }
                    }
                }
            }
        }
    }
    meta.set_item("requires", reqs)?;
    meta.set_item("exists", dp.is_dir())?;
    let t = meta.get_item("type")?;
    let ok = t.map(|t| t.extract::<String>().map(|s| TYPES.contains(&s.as_str())).unwrap_or(false)).unwrap_or(false);
    if !ok {
        meta.set_item("type", "addon")?;
    }
    Ok(meta)
}

/// mods/ にある MOD の情報のリスト（kind="addon" / "scenario" でその種類だけ）。本体に入っている MOD を先に
#[pyfunction]
#[pyo3(signature = (mods_dir=None, kind=None))]
pub fn modmeta_list_all<'py>(py: Python<'py>, mods_dir: Option<&Bound<'py, PyAny>>, kind: Option<&str>) -> R<Bound<'py, PyList>> {
    let rt = root(py, mods_dir)?;
    let rp = PathBuf::from(rt.str()?.to_string());
    let out = PyList::empty_bound(py);
    if !rp.exists() {
        return Ok(out);
    }
    let mut names: Vec<String> = std::fs::read_dir(&rp)
        .map(|it| it.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    #[cfg(windows)]
    names.sort_by_key(|n| n.to_lowercase());
    #[cfg(not(windows))]
    names.sort();
    let mut metas: Vec<(bool, String, String, Bound<'py, PyDict>)> = Vec::new();
    for n in names {
        if n.starts_with('.') || n.starts_with('_') || BUILTIN_MODES.contains(&n.as_str()) {
            continue;
        }
        let m = modmeta_read(py, &n, Some(&rt))?;
        if kind.map_or(true, |k| m.get_item("type").ok().flatten().map(|t| t.eq(k).unwrap_or(false)).unwrap_or(false)) {
            let builtin = m.get_item("builtin")?.map(|b| b.is_truthy().unwrap_or(false)).unwrap_or(false);
            let cat = match m.get_item("category")? {
                Some(c) => c.str()?.to_string(),
                None => String::new(),
            };
            metas.push((!builtin, cat, n, m));
        }
    }
    metas.sort_by(|a, b| (a.0, &a.1, &a.2).cmp(&(b.0, &b.1, &b.2)));
    for (_, _, _, m) in metas {
        out.append(m)?;
    }
    Ok(out)
}

#[pyfunction]
#[pyo3(signature = (mod_id, mods_dir=None))]
pub fn modmeta_is_scenario(py: Python<'_>, mod_id: &str, mods_dir: Option<&Bound<'_, PyAny>>) -> R<bool> {
    modmeta_read(py, mod_id, mods_dir)?.get_item("type")?.map_or(Ok(false), |t| t.eq("scenario"))
}

/// 選んだ MOD に、依存する MOD を足して読み込む順に並べる（依存する方が先）。
/// 戻り値：(並べた MOD のリスト, 足した MOD のリスト, 見つからない MOD の [(MOD, 必要とした MOD)])
#[pyfunction]
#[pyo3(signature = (enabled, mods_dir=None))]
pub fn modmeta_resolve(py: Python<'_>, enabled: Vec<String>, mods_dir: Option<&Bound<'_, PyAny>>)
    -> R<(Vec<String>, Vec<String>, Vec<(String, String)>)> {
    let mut chosen: Vec<String> = Vec::new();
    for e in enabled {
        if !chosen.contains(&e) {
            chosen.push(e);
        }
    }
    struct St {
        order: Vec<String>,
        added: Vec<String>,
        missing: Vec<(String, String)>,
        visiting: Vec<String>,
    }
    fn visit(py: Python<'_>, st: &mut St, mid: &str, parent: Option<&str>, chosen: &[String], md: Option<&Bound<'_, PyAny>>) -> R<()> {
        if st.order.iter().any(|x| x == mid) {
            return Ok(());
        }
        let meta = modmeta_read(py, mid, md)?;
        if !meta.get_item("exists")?.map_or(Ok(false), |x| x.is_truthy())? {
            match parent {
                Some(p) => st.missing.push((mid.to_string(), p.to_string())),
                None => {
                    if !st.order.iter().any(|x| x == mid) {
                        st.order.push(mid.to_string()); // 選ばれているが無い：読み込みのときにエラーにする
                    }
                }
            }
            return Ok(());
        }
        if st.visiting.iter().any(|x| x == mid) {
            return Ok(()); // 循環：そのまま並べる
        }
        st.visiting.push(mid.to_string());
        let reqs: Vec<String> = meta.get_item("requires")?.map(|r| r.extract()).transpose()?.unwrap_or_default();
        for r in reqs {
            visit(py, st, &r, Some(mid), chosen, md)?;
        }
        st.visiting.retain(|x| x != mid);
        if !st.order.iter().any(|x| x == mid) {
            st.order.push(mid.to_string());
            if !chosen.iter().any(|x| x == mid) {
                st.added.push(mid.to_string());
            }
        }
        Ok(())
    }
    let mut st = St { order: Vec::new(), added: Vec::new(), missing: Vec::new(), visiting: Vec::new() };
    for m in &chosen {
        visit(py, &mut st, m, None, &chosen, mods_dir)?;
    }
    Ok((st.order, st.added, st.missing))
}

/// selected の中で mod_id を（間接的にでも）必要とする MOD
#[pyfunction]
#[pyo3(signature = (mod_id, selected, mods_dir=None))]
pub fn modmeta_dependents(py: Python<'_>, mod_id: &str, selected: Vec<String>, mods_dir: Option<&Bound<'_, PyAny>>) -> R<Vec<String>> {
    let mut out = Vec::new();
    for m in selected {
        if m == mod_id {
            continue;
        }
        let (order, _, _) = modmeta_resolve(py, vec![m.clone()], mods_dir)?;
        if order.iter().any(|x| x == mod_id) {
            out.push(m);
        }
    }
    Ok(out)
}

#[pyfunction]
#[pyo3(signature = (mod_id, mods_dir=None))]
pub fn modmeta_name_of<'py>(py: Python<'py>, mod_id: &str, mods_dir: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
    modmeta_read(py, mod_id, mods_dir)?.as_any().call_method1("get", ("name", mod_id))
}

/// 選んだ MOD のうち、ワールドの形を決めるもの（最初の1つ）：(MOD の ID, world の設定) か (None, None)
#[pyfunction]
#[pyo3(signature = (mods, mods_dir=None))]
pub fn modmeta_world_preset(py: Python<'_>, mods: Vec<String>, mods_dir: Option<&Bound<'_, PyAny>>) -> R<(PyObject, PyObject)> {
    for m in mods {
        if let Some(w) = modmeta_read(py, &m, mods_dir)?.get_item("world")? {
            if w.is_instance_of::<PyDict>() {
                return Ok((m.into_py(py), w.unbind()));
            }
        }
    }
    Ok((py.None(), py.None()))
}

// ---------------------------------------------------------------------------
// 設定（config/settings.json）
// ---------------------------------------------------------------------------
fn merge<'py>(base: &Bound<'py, PyAny>, over: &Bound<'py, PyDict>) -> R<Bound<'py, PyDict>> {
    let out = crate::game::registry::deep_copy(base)?.downcast_into::<PyDict>()?;
    for (k, v) in over.iter() {
        let cur = out.get_item(&k)?;
        match (v.downcast::<PyDict>(), cur.as_ref().map(|c| c.downcast::<PyDict>())) {
            (Ok(vd), Some(Ok(cd))) => {
                let m = merge(cd.as_any(), vd)?;
                out.set_item(&k, m)?;
            }
            _ => out.set_item(&k, v)?,
        }
    }
    Ok(out)
}

#[pyfunction]
pub fn config_merge<'py>(base: &Bound<'py, PyAny>, over: &Bound<'py, PyDict>) -> R<Bound<'py, PyDict>> {
    merge(base, over)
}

/// config/settings.json を読む（無ければ既定値で作る）。defaults は既定値の dict
#[pyfunction]
pub fn config_load<'py>(py: Python<'py>, defaults: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let paths = crate::game::imp(py, "main.paths")?;
    let cdir = PathBuf::from(paths.getattr("CONFIG")?.str()?.to_string());
    let p = cdir.join("settings.json");
    let _ = std::fs::create_dir_all(&cdir);
    let mut data = PyDict::new_bound(py);
    if p.exists() {
        match read_json_file(py, &p) {
            Ok(v) => {
                if let Ok(d) = v.downcast_into::<PyDict>() {
                    data = d;
                }
            }
            Err(e) => {
                crate::game::imp(py, "logging")?.getattr("getLogger")?.call1(("alpedia.config",))?
                    .call_method1("error", ("settings.json の読み込みに失敗: %s（既定値を使用）", e))?;
            }
        }
    }
    let cfg = merge(defaults, &data)?;
    if !p.exists() {
        let kw = PyDict::new_bound(py);
        kw.set_item("indent", 2)?;
        kw.set_item("ensure_ascii", false)?;
        let txt: String = crate::game::imp(py, "json")?.getattr("dumps")?.call((&cfg,), Some(&kw))?.extract()?;
        let _ = std::fs::write(&p, txt);
    }
    Ok(cfg)
}
