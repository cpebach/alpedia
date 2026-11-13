//! 素材・機器・惑星・プレイヤー・資源の定義を読み込むレジストリ。main/registry.py の中身。
//!
//! 本体の基本定義は sauce/base/<カテゴリ>/*.json、MOD は mods/<MOD名>/<カテゴリ>/*.json に同じ形式で置く。
//! 定義に "extends": "<既存ID>" を書くと、その定義を引き継いで差分だけ上書きできる。

use std::path::{Component, Path, PathBuf};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};

use crate::net::{parse_json, val_to_py};

pub const CATEGORIES: [&str; 5] = ["resources", "materials", "devices", "planets", "players"];

pub const REQUIRED_TYPES: [&str; 16] = [
    "thruster", "tank", "pipe", "valve", "battery", "generator", "crank", "computer", "actuator", "processor", "seat",
    "air_vent", "antenna", "anchor", "light", "custom",
];

/// カテゴリごとの既定値（足りない項目はここから補う）。JSON で書いて Python の値にする
const DEFAULTS_JSON: &str = r#"{
 "resources": {"name": "", "density": 1000.0, "fluid": false, "color": [0.8, 0.8, 0.8]},
 "materials": {"name": "", "color": [0.7, 0.7, 0.7], "texture": null, "opacity": 1.0, "density": 1000.0,
   "stiffness": 0.8, "joint_stiffness": 0.8, "break_distance": 0.05, "yield_strain": 0.1, "plasticity": 0.0,
   "damping": 0.02, "friction": 0.6, "rebond": false, "rebond_distance": 0.2, "cost": {}},
 "devices": {"name": "", "type": "pipe", "material": "steel", "color": [0.5, 0.5, 0.55], "texture": null,
   "power": "none", "power_use": 0.0, "pipe_port": false, "directional": false, "params": {}, "cost": {}},
 "planets": {"name": "", "radius": 500.0, "surface_gravity": 9.81, "color": [0.45, 0.55, 0.35], "texture": null,
   "noise_amp": 8.0, "noise_freq": 3.0, "seed": 0, "dig_resource": "ore", "atmosphere_pressure": 101.3, "terrain": "sphere"},
 "players": {"name": "", "color": [0.9, 0.75, 0.6], "model": null, "texture": null, "height": 1.8, "radius": 0.35,
   "walk_speed": 5.0, "jump_speed": 5.0, "jetpack_accel": 15.0, "jetpack_fuel_rate": 1.0, "reach": 8.0}
}"#;

/// カテゴリごとの既定値（新しい dict）
pub fn defaults(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let v = parse_json(DEFAULTS_JSON.as_bytes()).map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(val_to_py(py, &v, &[])?.into_bound(py).downcast_into::<PyDict>()?)
}

/// JSON の値の深い複製（dict と list だけたどる）
pub fn deep_copy<'py>(v: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let py = v.py();
    if let Ok(d) = v.downcast::<PyDict>() {
        let out = PyDict::new_bound(py);
        for (k, x) in d.iter() {
            out.set_item(k, deep_copy(&x)?)?;
        }
        return Ok(out.into_any());
    }
    if let Ok(l) = v.downcast::<PyList>() {
        let out = PyList::empty_bound(py);
        for x in l.iter() {
            out.append(deep_copy(&x)?)?;
        }
        return Ok(out.into_any());
    }
    if v.is_instance_of::<PyTuple>() || v.is_instance_of::<pyo3::types::PySet>() {
        return py.import_bound("copy")?.getattr("deepcopy")?.call1((v,));
    }
    Ok(v.clone())
}

/// base に over を深く重ねた新しい dict（"extends"・"override" は捨てる）
pub fn deep_merge<'py>(base: &Bound<'py, PyAny>, over: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyDict>> {
    let out = deep_copy(base)?.downcast_into::<PyDict>()?;
    for (k, v) in over.iter() {
        if let Ok(ks) = k.downcast::<PyString>() {
            let s = ks.to_cow()?;
            if s == "extends" || s == "override" {
                continue;
            }
        }
        let cur = out.get_item(&k)?;
        match (v.downcast::<PyDict>(), cur.as_ref().map(|c| c.downcast::<PyDict>())) {
            (Ok(vd), Some(Ok(cd))) => {
                let m = deep_merge(cd.as_any(), vd)?;
                out.set_item(&k, m)?;
            }
            _ => out.set_item(&k, deep_copy(&v)?)?,
        }
    }
    Ok(out)
}

fn logger(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import_bound("logging")?.getattr("getLogger")?.call1(("alpedia.registry",))
}

/// pathlib と同じように "." と重なった区切りを消した文字列
fn norm_path(p: &Path) -> String {
    let mut out = PathBuf::new();
    for c in p.components() {
        if c != Component::CurDir {
            out.push(c.as_os_str());
        }
    }
    out.to_string_lossy().into_owned()
}

fn path_attr(py: Python<'_>, name: &str) -> PyResult<PathBuf> {
    let p = py.import_bound("main.paths")?.getattr(name)?;
    Ok(PathBuf::from(p.str()?.to_string()))
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Registry {
    #[pyo3(get, set)]
    defs: Py<PyDict>,
    #[pyo3(get, set)]
    loaded_mods: Py<PyList>,
    #[pyo3(get, set)]
    errors: Py<PyList>,
}

impl Registry {
    fn err(&self, py: Python<'_>, msg: String) -> PyResult<()> {
        self.errors.bind(py).append(&msg)?;
        logger(py)?.call_method1("error", (msg,))?;
        Ok(())
    }

    fn cat<'py>(&self, py: Python<'py>, cat: &str) -> PyResult<Bound<'py, PyDict>> {
        Ok(self.defs.bind(py).get_item(cat)?.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(cat.to_string()))?.downcast_into::<PyDict>()?)
    }

    fn load_tree(&self, py: Python<'_>, root: &Path, source: &str, allow_override: bool) -> PyResult<usize> {
        let mut count = 0;
        for cat in CATEGORIES {
            let d = root.join(cat);
            if !d.is_dir() {
                continue;
            }
            let mut files: Vec<PathBuf> = std::fs::read_dir(&d)
                .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().map_or(false, |x| x == "json") && p.is_file()).collect())
                .unwrap_or_default();
            #[cfg(windows)]
            files.sort_by_key(|p| p.to_string_lossy().to_lowercase());      // Windows の pathlib と同じ順（大文字・小文字を区別しない）
            #[cfg(not(windows))]
            files.sort();
            for f in files {
                let fname = f.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
                let text = match std::fs::read(&f) {
                    Ok(b) => b,
                    Err(e) => {
                        self.err(py, format!("[{source}] {fname} の JSON 解析に失敗: {e}"))?;
                        continue;
                    }
                };
                let text = String::from_utf8_lossy(&text);
                let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
                let data = match parse_json(text.as_bytes()) {
                    Ok(v) => val_to_py(py, &v, &[])?.into_bound(py),
                    Err(e) => {
                        self.err(py, format!("[{source}] {fname} の JSON 解析に失敗: {e}"))?;
                        continue;
                    }
                };
                let items: Vec<Bound<'_, PyAny>> = match data.downcast::<PyList>() {
                    Ok(l) => l.iter().collect(),
                    Err(_) => vec![data],
                };
                for item in items {
                    if self.add(py, cat, &item, root, source, allow_override, &fname)? {
                        count += 1;
                    }
                }
            }
        }
        Ok(count)
    }

    #[allow(clippy::too_many_arguments)]
    fn add(&self, py: Python<'_>, cat: &str, item: &Bound<'_, PyAny>, root: &Path, source: &str, allow_override: bool, fname: &str)
        -> PyResult<bool> {
        let Some(item) = item.downcast::<PyDict>().ok().filter(|d| d.contains("id").unwrap_or(false)) else {
            self.err(py, format!("[{source}] {cat}/{fname}: 'id' のない定義をスキップ"))?;
            return Ok(false);
        };
        let id = item.get_item("id")?.unwrap();
        let ids = id.str()?.to_string();
        let defs = self.cat(py, cat)?;
        let merged = if let Some(ext) = item.get_item("extends")? {
            match defs.get_item(&ext)? {
                None => {
                    self.err(py, format!("[{source}] {cat}/{ids}: 継承元 '{}' が存在しません", ext.str()?))?;
                    return Ok(false);
                }
                Some(parent) => deep_merge(&parent, item)?,
            }
        } else {
            let d = defaults(py)?;
            deep_merge(&d.get_item(cat)?.unwrap(), item)?
        };
        let over = match item.get_item("override")? {
            Some(v) => v.is_truthy()?,
            None => false,
        };
        if defs.contains(&id)? && !(allow_override || over) {
            self.err(py, format!("[{source}] {cat}/{ids}: 既存IDと重複（上書きするには \"override\": true）"))?;
            return Ok(false);
        }
        if cat == "devices" {
            let t = merged.get_item("type")?;
            let ok = match &t {
                Some(t) => t.extract::<String>().map(|s| REQUIRED_TYPES.contains(&s.as_str())).unwrap_or(false),
                None => false,
            };
            if !ok {
                let tn = match t {
                    Some(t) => t.str()?.to_string(),
                    None => "None".into(),
                };
                self.err(py, format!("[{source}] devices/{ids}: 未知の type '{tn}'"))?;
                return Ok(false);
            }
        }
        // テクスチャ・モデルのパスを解決
        for key in ["texture", "model"] {
            let Some(v) = item.get_item(key)? else { continue };
            if !v.is_truthy()? {
                continue;
            }
            let vs = v.str()?.to_string();
            let vp = Path::new(&vs);
            let mut p: Option<PathBuf> = Some(if vp.is_absolute() { vp.to_path_buf() } else { root.join(vp) });
            if !p.as_ref().unwrap().exists() {
                let p2 = path_attr(py, "SAUCE")?.join(vp);
                if p2.exists() {
                    p = Some(p2);
                } else if cat == "players" {
                    // 人物の見た目（VRM など）が無いだけなら止めない：箱の体で遊べるようにする
                    logger(py)?.call_method1("warning", (format!(
                        "[{source}] {cat}/{ids}: {key} '{vs}' が見つかりません（箱の体で表示します）"),))?;
                    p = None;
                } else {
                    self.err(py, format!("[{source}] {cat}/{ids}: {key} '{vs}' が見つかりません"))?;
                    p = None;
                }
            }
            match p {
                Some(p) => merged.set_item(key, norm_path(&p))?,
                None => merged.set_item(key, py.None())?,
            }
        }
        let name = match merged.get_item("name")? {
            Some(n) if n.is_truthy()? => n,
            _ => id.clone(),
        };
        merged.set_item("name", name)?;
        merged.set_item("_source", source)?;
        defs.set_item(&id, merged)?;
        Ok(true)
    }
}

#[pymethods]
impl Registry {
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        let defs = PyDict::new_bound(py);
        for c in CATEGORIES {
            defs.set_item(c, PyDict::new_bound(py))?;
        }
        Ok(Registry { defs: defs.unbind(), loaded_mods: PyList::empty_bound(py).unbind(), errors: PyList::empty_bound(py).unbind() })
    }

    fn get(&self, py: Python<'_>, cat: &str, id_: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        Ok(self.cat(py, cat)?.get_item(id_)?.map(|x| x.unbind()).unwrap_or_else(|| py.None()))
    }

    fn material(&self, py: Python<'_>, id_: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let m = self.cat(py, "materials")?;
        if let Some(x) = m.get_item(id_)? {
            if x.is_truthy()? {
                return Ok(x.unbind());
            }
        }
        Ok(m.get_item("rock")?.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("rock"))?.unbind())
    }

    fn device(&self, py: Python<'_>, id_: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        Ok(self.cat(py, "devices")?.get_item(id_)?.map(|x| x.unbind()).unwrap_or_else(|| py.None()))
    }

    /// 資源 ID → 表示名（硫黄鉱石 など）
    fn res_name(&self, py: Python<'_>, id_: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        if let Some(r) = self.cat(py, "resources")?.get_item(id_)? {
            if r.is_truthy()? {
                return Ok(r.call_method1("get", ("name", id_))?.unbind());
            }
        }
        Ok(id_.clone().unbind())
    }

    /// {資源ID: 量} → 「鉄 100、機械部品 5」
    #[pyo3(signature = (amounts, sep="、"))]
    fn res_text(&self, py: Python<'_>, amounts: &Bound<'_, PyAny>, sep: &str) -> PyResult<String> {
        let mut parts = Vec::new();
        for it in amounts.call_method0("items")?.iter()? {
            let (k, v): (Bound<'_, PyAny>, Bound<'_, PyAny>) = it?.extract()?;
            if v.is_truthy()? {
                let n = self.res_name(py, &k)?;
                let f = v.call_method1("__format__", (".0f",))?;
                parts.push(format!("{} {}", n.bind(py).str()?, f.str()?));
            }
        }
        Ok(parts.join(sep))
    }

    #[pyo3(signature = (enabled_mods=Vec::new()))]
    fn load(slf: Bound<'_, Self>, enabled_mods: Vec<String>) -> PyResult<Bound<'_, Self>> {
        let py = slf.py();
        let me = slf.borrow();
        me.load_tree(py, &path_attr(py, "BASE_DEFS")?, "base", true)?;
        let mods_root = path_attr(py, "MODS")?;
        let log = logger(py)?;
        for m in &enabled_mods {
            let mdir = mods_root.join(m);
            if !mdir.is_dir() {
                me.err(py, format!("MOD '{m}' が mods/ に見つかりません"))?;
                continue;
            }
            let mut meta = PyDict::new_bound(py);
            let mj = mdir.join("mod.json");
            if mj.exists() {
                let r = std::fs::read(&mj).map_err(|e| e.to_string()).and_then(|b| {
                    let t = String::from_utf8_lossy(&b).into_owned();
                    parse_json(t.trim_start_matches('\u{feff}').as_bytes())
                });
                match r {
                    Ok(v) => {
                        if let Ok(d) = val_to_py(py, &v, &[])?.into_bound(py).downcast_into::<PyDict>() {
                            meta = d;
                        }
                    }
                    Err(e) => {
                        me.err(py, format!("MOD '{m}' の mod.json が読めません: {e}"))?;
                        continue;
                    }
                }
            }
            log.call_method1("info", ("MOD 読み込み開始: %s %s", meta.call_method1("get", ("name", m))?,
                                      meta.call_method1("get", ("version", ""))?))?;
            let n = me.load_tree(py, &mdir, m, false)?;
            me.loaded_mods.bind(py).append(m)?;
            log.call_method1("info", ("MOD 読み込み完了: %s（%d 件）", m, n))?;
        }
        let counts = PyDict::new_bound(py);
        for c in CATEGORIES {
            counts.set_item(c, me.cat(py, c)?.len())?;
        }
        log.call_method1("info", ("定義数: %s", counts))?;
        drop(me);
        Ok(slf)
    }

    /// 建築用の選択肢（素材 → 機器の順）
    fn palette<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let out = PyList::empty_bound(py);
        for k in self.cat(py, "materials")?.keys() {
            out.append(("material", k))?;
        }
        for k in self.cat(py, "devices")?.keys() {
            out.append(("device", k))?;
        }
        Ok(out)
    }
}

#[pyfunction]
pub fn registry_defaults(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    defaults(py)
}

#[pyfunction]
pub fn registry_deep_merge<'py>(base: &Bound<'py, PyAny>, over: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyDict>> {
    deep_merge(base, over)
}
