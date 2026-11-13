//! セーブデータの決まった形（保存形式）。main/savefmt.py の中身。
//!
//! saves/<ワールド名>/ の各ファイルに入れてよい項目と型をここで決める。読み込み時に check_* で調べ、
//! 形が違う所はログに出す（読める所は読む）。機器の状態の中身は devstate.rs で決める。
//! 形式を変えたら FORMAT を上げ、古い形式の読み替えを書く。

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};

/// 2: クリエイティブ／サバイバル・街の ON/OFF を world.json ではなく MOD（mods.json）で決める
pub const FORMAT: i64 = 2;

#[derive(Clone, Copy, PartialEq)]
enum T {
    Int,
    Num,
    Str,
    Bool,
    Dict,
    List,
    ListOrNone,
}

impl T {
    fn ok(self, v: &Bound<'_, PyAny>) -> bool {
        let is_int = v.is_instance_of::<PyInt>();
        match self {
            T::Int => is_int,
            T::Num => is_int || v.is_instance_of::<PyFloat>(),
            T::Str => v.is_instance_of::<PyString>(),
            T::Bool => v.is_instance_of::<PyBool>(),
            T::Dict => v.is_instance_of::<PyDict>(),
            T::List => v.is_instance_of::<PyList>(),
            T::ListOrNone => v.is_instance_of::<PyList>() || v.is_none(),
        }
    }
    fn numeric(self) -> bool {
        matches!(self, T::Int | T::Num)
    }
}

const WORLD: &[(&str, T)] = &[
    ("format", T::Int), ("name", T::Str), ("voxel_size", T::Num), ("gravity_scale", T::Num), ("cleanup", T::Dict),
    ("player", T::Str), ("planets", T::List), ("time", T::Num), ("full_rigid", T::Bool), ("tutorial", T::Bool),
    ("starter_sample", T::Bool), ("mode", T::Str),
];
const CLEANUP: &[(&str, T)] = &[("enabled", T::Bool), ("distance", T::Num), ("seconds", T::Num), ("max_vertices", T::Int),
                                ("check_interval", T::Num)];
const PLANET: &[(&str, T)] = &[("def", T::Str), ("position", T::List), ("craters", T::List), ("override", T::Dict)];
const CONSTRUCT_REQUIRED: &[(&str, T)] = &[("id", T::Str), ("name", T::Str), ("voxel_size", T::Num), ("voxels", T::List)];
const CONSTRUCT_OPTIONAL: &[(&str, T)] = &[("links", T::List), ("extras", T::List), ("storage", T::Dict), ("cavity_air", T::List),
                                           ("vertices", T::List), ("rest", T::List), ("anchor", T::Str), ("soft_ok", T::Bool)];
const DEVICE: &[(&str, T)] = &[("def", T::Str), ("dir", T::Int), ("name", T::Str), ("state", T::Dict)];
const SPAWN: &[(&str, T)] = &[("position", T::List), ("rotation", T::List), ("velocity", T::List), ("angular_velocity", T::List)];
const PLAYER: &[(&str, T)] = &[("position", T::List), ("velocity", T::List), ("inventory", T::Dict), ("seated", T::ListOrNone),
                               ("jetpack", T::Bool), ("def", T::Str), ("starter_given", T::Bool), ("credits_given", T::Bool),
                               ("hotbar", T::ListOrNone)];
/// 形式 1 までの項目（読み込み時に MOD に読み替える）
const LEGACY_WORLD_KEYS: &[&str] = &["creative", "cities"];

fn type_name(v: &Bound<'_, PyAny>) -> String {
    v.get_type().name().map(|n| n.to_string()).unwrap_or_else(|_| "?".into())
}

fn keys(d: &Bound<'_, PyAny>, spec: &[(&str, T)], wh: &str, required: &[&str], extra: &[String]) -> PyResult<Vec<String>> {
    let mut bad = Vec::new();
    let Ok(d) = d.downcast::<PyDict>() else {
        return Ok(vec![format!("{wh}: 辞書ではありません")]);
    };
    for k in required {
        if !d.contains(*k)? {
            bad.push(format!("{wh}: '{k}' がありません"));
        }
    }
    for (k, v) in d.iter() {
        let ks = k.str()?.to_string();
        let Some(&(_, t)) = spec.iter().find(|(n, _)| k.is_instance_of::<PyString>() && *n == ks) else {
            if !extra.iter().any(|e| *e == ks) {
                bad.push(format!("{wh}: 知らない項目 '{ks}'"));
            }
            continue;
        };
        if (v.is_instance_of::<PyBool>() && t.numeric()) || !t.ok(&v) {
            bad.push(format!("{wh}: '{ks}' の型が違います（{}）", type_name(&v)));
        }
    }
    Ok(bad)
}

/// 古いワールドの読み替え：(settings, mods, 読み替えたか)
#[pyfunction]
pub fn savefmt_upgrade_world<'py>(py: Python<'py>, settings: &Bound<'py, PyDict>, mods: &Bound<'py, PyAny>)
    -> PyResult<(Bound<'py, PyDict>, Bound<'py, PyList>, bool)> {
    let s = settings.copy()?;
    let mut mods: Vec<PyObject> = mods.iter()?.map(|x| x.map(|x| x.unbind())).collect::<PyResult<_>>()?;
    let has = |mods: &Vec<PyObject>, name: &str| mods.iter().any(|m| m.bind(py).eq(name).unwrap_or(false));
    let mut changed = false;
    let fmt: i64 = match s.get_item("format")? {
        Some(v) if v.is_truthy()? => py.import_bound("builtins")?.getattr("int")?.call1((v,))?.extract()?,
        _ => 0,
    };
    if fmt < 2 {
        if let Some(v) = s.get_item("creative")? {
            if v.is_instance_of::<PyBool>() && !v.is_truthy()? {
                s.set_item("mode", "survival")?;
            }
        }
        let cities = match s.get_item("cities")? {
            Some(v) => v.is_truthy()?,
            None => true,                    // 形式 1 までは、項目が無ければ街あり
        };
        if cities && !has(&mods, "city") {
            mods.push("city".into_py(py));
        }
        for k in LEGACY_WORLD_KEYS {
            if s.contains(*k)? {
                s.del_item(*k)?;
            }
        }
        s.set_item("format", FORMAT)?;
        changed = true;
    }
    if has(&mods, "survival") {
        mods.retain(|m| !m.bind(py).eq("survival").unwrap_or(false));
        s.set_item("mode", "survival")?;
        changed = true;
    }
    Ok((s, PyList::new_bound(py, mods), changed))
}

#[pyfunction]
#[pyo3(signature = (s, extra_keys=Vec::new()))]
pub fn savefmt_check_world(s: &Bound<'_, PyAny>, extra_keys: Vec<String>) -> PyResult<Vec<String>> {
    let mut bad = keys(s, WORLD, "world.json", &["planets"], &extra_keys)?;
    if let Ok(d) = s.downcast::<PyDict>() {
        if let Some(c) = d.get_item("cleanup")? {
            if c.is_instance_of::<PyDict>() {
                bad.extend(keys(&c, CLEANUP, "world.json cleanup", &[], &[])?);
            }
        }
        if let Some(pl) = d.get_item("planets")? {
            if pl.is_truthy()? {
                for (i, p) in pl.iter()?.enumerate() {
                    bad.extend(keys(&p?, PLANET, &format!("world.json planets[{i}]"), &["def"], &[])?);
                }
            }
        }
    }
    Ok(bad)
}

#[pyfunction]
pub fn savefmt_check_construct(d: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    let dd = d.downcast::<PyDict>().ok();
    let id = match dd.and_then(|x| x.get_item("id").ok().flatten()) {
        Some(v) => v.str()?.to_string(),
        None => "?".into(),
    };
    let wh = format!("構築物 {id}");
    let spec: Vec<(&str, T)> = CONSTRUCT_REQUIRED.iter().chain(CONSTRUCT_OPTIONAL.iter()).copied().collect();
    let req: Vec<&str> = CONSTRUCT_REQUIRED.iter().map(|x| x.0).collect();
    let mut bad = keys(d, &spec, &wh, &req, &[])?;
    let Some(dd) = dd else { return Ok(bad) };
    let vox = match dd.get_item("voxels")? {
        Some(v) if v.is_truthy()? => v,
        _ => return Ok(bad),
    };
    let dreq: Vec<&str> = DEVICE.iter().map(|x| x.0).collect();
    for (j, v) in vox.iter()?.enumerate() {
        let v = v?;
        let ok = match v.downcast::<PyList>() {
            Ok(l) if l.len() == 5 => {
                (0..3).all(|k| l.get_item(k).map(|x| x.is_instance_of::<PyInt>()).unwrap_or(false))
                    && l.get_item(3)?.is_instance_of::<PyString>()
            }
            _ => false,
        };
        if !ok {
            bad.push(format!("{wh} voxels[{j}]: [x, y, z, 素材, 機器] の形ではありません"));
            continue;
        }
        let dev = v.downcast::<PyList>()?.get_item(4)?;
        if !dev.is_none() {
            bad.extend(keys(&dev, DEVICE, &format!("{wh} voxels[{j}] 機器"), &dreq, &[])?);
        }
    }
    Ok(bad)
}

#[pyfunction]
pub fn savefmt_check_spawn(sp: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    let mut bad = Vec::new();
    if sp.is_none() || !sp.is_truthy()? {
        return Ok(bad);
    }
    for item in sp.call_method0("items")?.iter()? {
        let (cid, d): (Bound<'_, PyAny>, Bound<'_, PyAny>) = item?.extract()?;
        bad.extend(keys(&d, SPAWN, &format!("spawn.json {}", cid.str()?), &[], &[])?);
    }
    Ok(bad)
}

#[pyfunction]
pub fn savefmt_check_player(d: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    keys(d, PLAYER, "player.json", &[], &[])
}
