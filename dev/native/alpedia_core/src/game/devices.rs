//! 機器の動作のまとめ役（main/devices.py の中身）：配管のつながり（区画とバルブ）・空洞・毎ティックの機器の計算の呼び出し・
//! E キーの操作。1ティックの計算そのもの（電力・推進機・配管の流れ・精錬機・通気口）は crate::devices の DevPlan。
//!
//! 推進機：点火のときだけ電気を使い、燃えている間は燃料を使って推力と電気を出す。
//! 精錬機・加工機：原料は配管でつながった所から受け取り、作った物は自分の中にためる。

use crate::game::PyDictOps;
use crate::game::construct::{con, Construct};
use std::collections::HashMap;

use numpy::PyArray1;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyTuple};

type R<T> = PyResult<T>;

const DIRS: [[i64; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

fn float_of(v: &Bound<'_, PyAny>) -> R<f64> {
    crate::game::imp(v.py(), "builtins")?.getattr("float")?.call1((v,))?.extract()
}

/// 構築物ごとのキャッシュ：配管のつながり（区画とバルブ）・空洞
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Networks {
    /// voxel idx -> 区画の番号（バルブは入らない）
    #[pyo3(get, set)]
    pipe_of: Py<PyDict>,
    /// [[voxel idx...]] 区画ごとの機器
    #[pyo3(get, set)]
    pipes: Py<PyList>,
    /// バルブの voxel idx -> となりの区画・バルブ [("s", 区画) / ("v", idx)]
    #[pyo3(get, set)]
    valve_adj: Py<PyDict>,
    /// 区画 -> となりのバルブ [idx]
    #[pyo3(get, set)]
    seg_adj: Py<PyDict>,
    /// [{"cells": set, "rep": cell, "volume": m3}]
    #[pyo3(get, set)]
    cavities: Py<PyList>,
    /// cell -> cavity index
    #[pyo3(get, set)]
    cavity_of: Py<PyDict>,
}

#[pymethods]
impl Networks {
    #[new]
    fn new(py: Python<'_>) -> Self {
        Networks {
            pipe_of: PyDict::new_bound(py).unbind(),
            pipes: PyList::empty_bound(py).unbind(),
            valve_adj: PyDict::new_bound(py).unbind(),
            seg_adj: PyDict::new_bound(py).unbind(),
            cavities: PyList::empty_bound(py).unbind(),
            cavity_of: PyDict::new_bound(py).unbind(),
        }
    }
}

fn dev_def<'py>(c: &Bound<'py, PyAny>, d: &Bound<'py, PyAny>) -> R<Option<Bound<'py, PyAny>>> {
    if !d.is_truthy()? {
        return Ok(None);
    }
    let dd = Construct::g_reg(con(&c)?)?.call_method1("device", (d.get_item("def")?,))?;
    Ok(if dd.is_none() { None } else { Some(dd) })
}

/// 配管のつながり・空洞を数え直す（形が変わったとき）
#[pyfunction]
pub fn dev_rebuild_networks<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, Networks>> {
    let net = Bound::new(py, Networks::new(py))?;
    let devs: Vec<Bound<'py, PyAny>> = Construct::g_devs(con(&c)?)?.iter()?.collect::<R<_>>()?;
    let n: usize = Construct::n(con(&c)?)?.into_py(py).into_bound(py).extract()?;
    let mut kind: Vec<Option<char>> = Vec::with_capacity(n);
    for i in 0..n {
        let k = match devs.get(i) {
            Some(d) => match dev_def(c, d)? {
                Some(dd) if dd.dget1("pipe_port")?.is_truthy()? => {
                    Some(if dd.get_item("type")?.eq("valve")? { 'v' } else { 'p' })
                }
                _ => None,
            },
            None => None,
        };
        kind.push(k);
    }
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let index = Construct::g_index(con(&c)?)?;
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    // 結合（アクチュエーターで格子がずれてつないだ所 extras も含む）：配管の機器どうしなら流れる
    let mut keys: Vec<Bound<'py, PyAny>> = Construct::g_links(con(&c)?)?.iter()?.collect::<R<_>>()?;
    if let Ok(ex) = Construct::g_extras(con(&c)?) {
        keys.extend(ex.iter()?.collect::<R<Vec<_>>>()?);
    }
    for k in keys {
        let (a, b) = (k.get_item(0)?, k.get_item(1)?);
        let (ia, ib) = (index.dget1(a)?, index.dget1(b)?);
        if ia.is_none() || ib.is_none() {
            continue;
        }
        let (ia, ib): (usize, usize) = (ia.extract()?, ib.extract()?);
        if ia >= n || ib >= n || kind[ia].is_none() || kind[ib].is_none() {
            continue;
        }
        if kind[ia] == Some('p') && kind[ib] == Some('p') {
            let (ra, rb) = (find(&mut parent, ia), find(&mut parent, ib));
            if ra != rb {
                parent[ra] = rb;
            }
        } else {
            pairs.push((ia, ib));
        }
    }
    // 区画（最初に出てきた順）
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut gpos: HashMap<usize, usize> = HashMap::new();
    for i in 0..n {
        if kind[i] == Some('p') {
            let r = find(&mut parent, i);
            match gpos.get(&r) {
                Some(&g) => groups[g].1.push(i),
                None => {
                    gpos.insert(r, groups.len());
                    groups.push((r, vec![i]));
                }
            }
        }
    }
    let me = net.borrow();
    let (pipe_of, pipes, valve_adj, seg_adj) = (me.pipe_of.bind(py), me.pipes.bind(py), me.valve_adj.bind(py), me.seg_adj.bind(py));
    let mut pipe_of_r: HashMap<usize, usize> = HashMap::new();
    for (k, (_, members)) in groups.iter().enumerate() {
        pipes.append(members.clone())?;
        seg_adj.set_item(k, PyList::empty_bound(py))?;
        for &m in members {
            pipe_of.set_item(m, k)?;
            pipe_of_r.insert(m, k);
        }
    }
    let mut vadj: Vec<(usize, Vec<(char, usize)>)> = Vec::new();
    let mut vpos: HashMap<usize, usize> = HashMap::new();
    for i in 0..n {
        if kind[i] == Some('v') {
            vpos.insert(i, vadj.len());
            vadj.push((i, Vec::new()));
        }
    }
    let mut sadj: Vec<Vec<usize>> = vec![Vec::new(); groups.len()];
    for (ia, ib) in pairs {
        for (v, o) in [(ia, ib), (ib, ia)] {
            if kind[v] != Some('v') {
                continue;
            }
            let node = if kind[o] == Some('v') { ('v', o) } else { ('s', pipe_of_r[&o]) };
            let list = &mut vadj[vpos[&v]].1;
            if !list.contains(&node) {
                list.push(node);
            }
            if node.0 == 's' && !sadj[node.1].contains(&v) {
                sadj[node.1].push(v);
            }
        }
    }
    for (v, list) in &vadj {
        let l = PyList::empty_bound(py);
        for (k, x) in list {
            l.append((k.to_string(), *x))?;
        }
        valve_adj.set_item(*v, l)?;
    }
    for (k, l) in sadj.iter().enumerate() {
        seg_adj.set_item(k, l.clone())?;
    }
    // 空洞
    let size: f64 = float_of(&Construct::g_size(con(&c)?)?)?;
    let vol1 = size.powi(3);
    let cavity_air = Construct::g_cavity_air(con(&c)?)?;
    let mut old_density: HashMap<(i64, i64, i64), f64> = HashMap::new();
    if let Ok(old) = c.getattr("_net") {
        if old.is_truthy()? {
            for cav in old.getattr("cavities")?.iter()? {
                let cav = cav?;
                let air = float_of(&cavity_air.dget2(cav.get_item("rep")?, 0.0)?)?;
                let dens = air / float_of(&cav.get_item("volume")?)?.max(1e-9);
                for cell in cav.get_item("cells")?.iter()? {
                    old_density.insert(cell?.extract()?, dens);
                }
            }
        }
    }
    let cells: Vec<(i64, i64, i64)> = Construct::g_cells(con(&c)?)?.iter()?.map(|x| -> R<(i64, i64, i64)> {
        let x = x?;
        Ok((x.get_item(0)?.extract()?, x.get_item(1)?.extract()?, x.get_item(2)?.extract()?))
    }).collect::<R<_>>()?;
    let cav_cells: Vec<[i64; 3]> = cells.iter().map(|c| [c.0, c.1, c.2]).collect();
    let cavs = crate::game::airtight::find_cavities(&cav_cells);
    let new_air = PyDict::new_bound(py);
    let cavities = me.cavities.bind(py);
    let cavity_of = me.cavity_of.bind(py);
    for (k, cs) in cavs.iter().enumerate() {
        let rep = cs.iter().copied().min().unwrap();
        let rep_t = (rep[0], rep[1], rep[2]);
        let volume = cs.len() as f64 * vol1;
        let air = if !old_density.is_empty() {
            cs.iter().map(|x| old_density.get(&(x[0], x[1], x[2])).copied().unwrap_or(0.0)).sum::<f64>() * vol1
        } else {
            float_of(&cavity_air.dget2(rep_t, 0.0)?)?
        };
        new_air.set_item(rep_t, air)?;
        let set = PySet::new_bound(py, &cs.iter().map(|x| PyTuple::new_bound(py, x)).collect::<Vec<_>>())?;
        let e = PyDict::new_bound(py);
        e.set_item("cells", &set)?;
        e.set_item("rep", rep_t)?;
        e.set_item("volume", volume)?;
        cavities.append(e)?;
        for x in cs {
            cavity_of.set_item((x[0], x[1], x[2]), k)?;
        }
    }
    drop(me);
    Construct::s_cavity_air(con(&c)?, new_air)?;
    c.setattr("_net", &net)?;
    Construct::s_dirty(con(&c)?, false)?;
    Ok(net)
}

/// その資源が流れる物（燃料・空気・水など）か
#[pyfunction]
pub fn dev_is_fluid(reg: &Bound<'_, PyAny>, res: &Bound<'_, PyAny>) -> R<bool> {
    let py = reg.py();
    let r = reg.getattr("defs")?.dget2("resources", PyDict::new_bound(py))?.dget1(res)?;
    if r.is_truthy()? {
        return r.dget1("fluid")?.is_truthy();
    }
    Ok(res.eq("fuel")? || res.eq("air")? || res.eq("water")?)
}

#[pyfunction]
pub fn dev_valve_opening(st: &Bound<'_, PyAny>) -> R<f64> {
    Ok(float_of(&st.dget2("open", 1.0)?)?.max(0.0).min(1.0))
}

/// 精錬機・加工機が出す物（作る物のうち、自分の原料ではないもの）
#[pyfunction]
pub fn dev_proc_outputs<'py>(py: Python<'py>, dd: &Bound<'py, PyAny>) -> R<Bound<'py, PySet>> {
    let rs = dd.get_item("params")?.dget2("recipes", PyList::empty_bound(py))?;
    let ins = PySet::empty_bound(py)?;
    let outs = PySet::empty_bound(py)?;
    for r in rs.iter()? {
        let r = r?;
        for k in r.dget2("in", PyDict::new_bound(py))?.iter()? {
            ins.add(k?)?;
        }
    }
    for r in rs.iter()? {
        let r = r?;
        for k in r.dget2("out", PyDict::new_bound(py))?.iter()? {
            let k = k?;
            if !ins.contains(&k)? {
                outs.add(k)?;
            }
        }
    }
    Ok(outs)
}

#[pyfunction]
pub fn dev_proc_inputs<'py>(py: Python<'py>, dd: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let rs = dd.get_item("params")?.dget2("recipes", PyList::empty_bound(py))?;
    let out = PyDict::new_bound(py);
    for r in rs.iter()? {
        for it in r?.dget2("in", PyDict::new_bound(py))?.call_method0("items")?.iter()? {
            let (k, v): (Bound<'py, PyAny>, Bound<'py, PyAny>) = it?.extract()?;
            let cur = float_of(&out.as_any().dget2(&k, 0.0)?)?;
            out.set_item(&k, cur.max(float_of(&v)?))?;
        }
    }
    Ok(out)
}

/// 古い関数（区画の中のタンクだけ。バルブの先は見ない）
#[pyfunction]
pub fn dev_tanks_in<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, net_id: &Bound<'py, PyAny>) -> R<Bound<'py, PyList>> {
    let out = PyList::empty_bound(py);
    if net_id.is_none() {
        return Ok(out);
    }
    let devs = Construct::g_devs(con(&c)?)?;
    for i in c.getattr("_net")?.getattr("pipes")?.get_item(net_id)?.iter()? {
        let i = i?;
        let d = devs.get_item(&i)?;
        if let Some(dd) = dev_def(c, &d)? {
            if dd.get_item("type")?.eq("tank")? {
                let cap = float_of(&dd.get_item("params")?.dget2("capacity", 0)?)?;
                out.append((i, d.get_item("state")?.get_item("contents")?, cap))?;
            }
        }
    }
    Ok(out)
}

#[pyfunction]
pub fn dev_fluid_amount(py: Python<'_>, c: &Bound<'_, PyAny>, net_id: &Bound<'_, PyAny>, res: &Bound<'_, PyAny>) -> R<f64> {
    let mut s = 0.0;
    for t in dev_tanks_in(py, c, net_id)?.iter() {
        s += float_of(&t.get_item(1)?.dget2(res, 0.0)?)?;
    }
    Ok(s)
}

/// 構築物の精錬機・加工機に入っている物の合計（表示・スクリプトの api.storage 用）
#[pyfunction]
pub fn dev_storage_total<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let out = PyDict::new_bound(py);
    out.update(Construct::g_storage(con(&c)?)?.downcast::<pyo3::types::PyMapping>()?)?;
    for d in Construct::g_devs(con(&c)?)?.iter()? {
        let d = d?;
        if !d.is_truthy()? {
            continue;
        }
        let st = d.get_item("state")?;
        if st.contains("store")? {
            for it in st.get_item("store")?.call_method0("items")?.iter()? {
                let (k, v): (Bound<'py, PyAny>, Bound<'py, PyAny>) = it?.extract()?;
                if v.gt(1e-9)? {
                    let cur = out.as_any().dget2(&k, 0.0)?;
                    out.set_item(&k, cur.add(v)?)?;
                }
            }
        }
    }
    Ok(out)
}

/// ボクセル i の向き d の軸（世界での向き）
#[pyfunction]
#[pyo3(signature = (c, i, d, frames=None))]
pub fn dev_world_axis<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, i: usize, d: i64, frames: Option<&Bound<'py, PyAny>>) -> R<PyObject> {
    let f = match frames.filter(|f| !f.is_none()) {
        Some(f) => f.get_item(i)?,
        None => Construct::voxel_frames(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?.get_item(i)?,
    };
    let axis = d.div_euclid(2);
    let sign = if d.rem_euclid(2) == 0 { 1.0 } else { -1.0 };
    let row: Vec<f64> = f.get_item(axis)?.call_method0("tolist")?.extract()?;
    Ok(PyArray1::from_vec_bound(py, row.into_iter().map(|x| x * sign).collect()).into_any().unbind())
}

fn adjacent_cavity(c: &Bound<'_, PyAny>, i: usize) -> R<Option<usize>> {
    let cell = Construct::g_cells(con(&c)?)?.get_item(i)?;
    let (x, y, z): (i64, i64, i64) = (cell.get_item(0)?.extract()?, cell.get_item(1)?.extract()?, cell.get_item(2)?.extract()?);
    let cav_of = c.getattr("_net")?.getattr("cavity_of")?;
    for d in DIRS {
        let k = cav_of.dget1((x + d[0], y + d[1], z + d[2]))?;
        if !k.is_none() {
            return Ok(Some(k.extract()?));
        }
    }
    Ok(None)
}

fn note(c: &Bound<'_, PyAny>, text: &Bound<'_, PyAny>) -> R<()> {
    let notes = Construct::g_notes(con(&c)?)?;
    if !notes.contains(text)? {
        notes.call_method1("append", (text,))?;
    }
    Ok(())
}

fn ensure_net<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let has = c.getattr("_net").map(|n| !n.is_none()).unwrap_or(false);
    if Construct::g_dirty(con(&c)?)?.is_truthy()? || !has {
        dev_rebuild_networks(py, c)?;
    }
    c.getattr("_net")
}

/// この構築物の機器のまとめ（形が変わったときだけ作り直す）
fn native_plan<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, net: &Bound<'py, PyAny>, by_type: &[(String, Vec<(usize, Bound<'py, PyAny>, Bound<'py, PyAny>)>)])
    -> R<(Bound<'py, PyAny>, Vec<usize>)> {
    let key = PyTuple::new_bound(py, [Construct::g_version(con(&c)?)?.unbind(), Construct::g_cells(con(&c)?)?.len()?.into_py(py),
                                       (net.as_ptr() as usize).into_py(py)]);
    let cd = c.getattr("__dict__")?;
    let cache = cd.dget1("_devplan")?;
    if !cache.is_none() && cache.get_item(0)?.eq(&key)? {
        return Ok((cache.get_item(1)?, cache.get_item(2)?.extract()?));
    }
    let mut order: Vec<usize> = by_type.iter().flat_map(|(_, l)| l.iter().map(|x| x.0)).collect();
    order.sort();
    let devs_l = PyList::empty_bound(py);
    let mut outs_all: Vec<Bound<'py, PyAny>> = Vec::new();
    let cdevs = Construct::g_devs(con(&c)?)?;
    let reg = Construct::g_reg(con(&c)?)?;
    let fl = |o: Bound<'py, PyAny>| float_of(&o);
    for &i in &order {
        let d = cdevs.get_item(i)?;
        let dd = crate::game::registry::dev_dict(&reg, &d.get_item("def")?)?;
        let p = dd.dget2("params", PyDict::new_bound(py))?;
        let t = dd.get_item("type")?;
        let is_proc = t.eq("processor")?;
        let e = PyDict::new_bound(py);
        let pg = |k: &str, dflt: f64| -> R<f64> { fl(p.dget2(k, dflt)?) };
        e.set_item("vox", i)?;
        e.set_item("type", &t)?;
        e.set_item("name", d.dget2("name", "")?)?;
        e.set_item("dir", crate::game::imp(py, "builtins")?.getattr("int")?.call1((d.dget2("dir", 0)?,))?)?;
        e.set_item("electric", dd.dget1("power")?.eq("electric")?)?;
        e.set_item("power_use", fl(dd.dget2("power_use", 0)?)?)?;
        e.set_item("capacity", pg("capacity", if is_proc { 400.0 } else { 0.0 })?)?;
        e.set_item("output", pg("output", 0.0)?)?;
        e.set_item("fuel", p.dget2("fuel", "fuel")?)?;
        e.set_item("fuel_rate", pg("fuel_rate", 0.0)?)?;
        e.set_item("thrust", pg("thrust", 0.0)?)?;
        e.set_item("power_output", pg("power_output", 0.0)?)?;
        e.set_item("ignite_energy", pg("ignite_energy", 0.0)?)?;
        e.set_item("max_flow", pg("max_flow", 6.0)?)?;
        let key = p.dget1("key")?;
        e.set_item("key", if key.is_truthy()? { key.unbind() } else { py.None() })?;
        e.set_item("air_breathing", p.dget2("air_breathing", false)?.is_truthy()?)?;
        e.set_item("rate", pg("rate", 1.0)?)?;
        e.set_item("target_kpa", pg("target_kpa", 101.3)?)?;
        if is_proc {
            let recipes = PyList::empty_bound(py);
            for r in p.dget2("recipes", PyList::empty_bound(py))?.iter()? {
                let r = r?;
                let conv = |k: &str| -> R<Bound<'py, PyList>> {
                    let l = PyList::empty_bound(py);
                    for it in r.dget2(k, PyDict::new_bound(py))?.call_method0("items")?.iter()? {
                        let (a, b): (Bound<'py, PyAny>, Bound<'py, PyAny>) = it?.extract()?;
                        l.append((a, float_of(&b)?))?;
                    }
                    Ok(l)
                };
                let power: PyObject = if r.contains("power")? { float_of(&r.get_item("power")?)?.into_py(py) } else { py.None() };
                recipes.append((conv("in")?, conv("out")?, fl(r.dget2("time", 1.0)?)?, power))?;
            }
            e.set_item("recipes", recipes)?;
            e.set_item("inputs", PyList::new_bound(py, dev_proc_inputs(py, &dd)?.items().iter()))?;
            let outs = crate::game::imp(py, "builtins")?.getattr("sorted")?.call1((dev_proc_outputs(py, &dd)?,))?;
            for o in outs.iter()? {
                outs_all.push(o?);
            }
            e.set_item("outputs", outs)?;
        }
        if t.eq("air_vent")? {
            e.set_item("cavity", adjacent_cavity(c, i)?)?;
        }
        devs_l.append(e)?;
    }
    let fl_set = PySet::empty_bound(py)?;
    for r in &outs_all {
        if dev_is_fluid(&reg, r)? {
            fl_set.add(r)?;
        }
    }
    for k in ["fuel", "air", "water"] {
        fl_set.add(k)?;
    }
    let fluids = crate::game::imp(py, "builtins")?.getattr("sorted")?.call1((fl_set,))?;
    let va = PyList::empty_bound(py);
    for it in net.getattr("valve_adj")?.call_method0("items")?.iter()? {
        let (v, adj): (Bound<'py, PyAny>, Bound<'py, PyAny>) = it?.extract()?;
        let l = PyList::empty_bound(py);
        for x in adj.iter()? {
            let x = x?;
            l.append((x.get_item(0)?.eq("v")?, x.get_item(1)?))?;
        }
        va.append((v, l))?;
    }
    let pipes = PyList::empty_bound(py);
    for m in net.getattr("pipes")?.iter()? {
        pipes.append(PyList::new_bound(py, m?.iter()?.collect::<R<Vec<_>>>()?))?;
    }
    let seg = PyDict::new_bound(py);
    for it in net.getattr("seg_adj")?.call_method0("items")?.iter()? {
        let (k, v): (Bound<'py, PyAny>, Bound<'py, PyAny>) = it?.extract()?;
        seg.set_item(k, PyList::new_bound(py, v.iter()?.collect::<R<Vec<_>>>()?))?;
    }
    let vols = PyList::empty_bound(py);
    for cv in net.getattr("cavities")?.iter()? {
        vols.append(float_of(&cv?.get_item("volume")?)?)?;
    }
    let pipe_of = PyDict::new_bound(py);
    pipe_of.update(net.getattr("pipe_of")?.downcast::<pyo3::types::PyMapping>()?)?;
    let plan_cls = crate::game::imp(py, "alpedia_core")?.getattr("DevPlan")?;
    let plan = plan_cls.call1((devs_l, pipe_of, pipes, va, seg, fluids, vols))?;
    c.setattr("_devplan", (key, &plan, order.clone()))?;
    Ok((plan, order))
}

/// 機器の1ティック（電力・推進機・配管・精錬機・通気口は DevPlan、アクチュエーターは actuator）
#[pyfunction]
#[pyo3(signature = (c, dt, input_keys=None, seated=false, ambient_kpa=0.0))]
pub fn dev_update(py: Python<'_>, c: &Bound<'_, PyAny>, dt: f64, input_keys: Option<&Bound<'_, PyAny>>, seated: bool, ambient_kpa: f64) -> R<()> {
    let net = ensure_net(py, c)?;
    let force = Construct::g_force(con(&c)?)?;
    force.call_method1("fill", (0.0,))?;
    let n: usize = Construct::n(con(&c)?)?.into_py(py).into_bound(py).extract()?;
    if n == 0 {
        return Ok(());
    }
    let key = PyTuple::new_bound(py, [Construct::g_version(con(&c)?)?.unbind(), Construct::g_cells(con(&c)?)?.len()?.into_py(py)]);
    let cd = c.getattr("__dict__")?;
    let devs = Construct::g_devs(con(&c)?)?;
    let cache = cd.dget1("_bytype_cache")?;
    let idx: Vec<(String, Vec<usize>)> = if !cache.is_none() && cache.get_item(0)?.eq(&key)? {
        cache.get_item(1)?.call_method0("items")?.iter()?.map(|it| it?.extract::<(String, Vec<usize>)>()).collect::<R<_>>()?
    } else {
        // 種類ごとの機器の番号（形が変わったときだけ数え直す）。種類は最初に出てきた順
        let mut idx: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, d) in devs.iter()?.enumerate() {
            let d = d?;
            if let Some(dd) = dev_def(c, &d)? {
                let t: String = dd.get_item("type")?.extract()?;
                match idx.iter_mut().find(|x| x.0 == t) {
                    Some(x) => x.1.push(i),
                    None => idx.push((t, vec![i])),
                }
            }
        }
        let dct = PyDict::new_bound(py);
        for (t, l) in &idx {
            dct.set_item(t, l.clone())?;
        }
        c.setattr("_bytype_cache", (&key, dct))?;
        idx
    };
    let nd = devs.len()?;
    let mut by_type: Vec<(String, Vec<(usize, Bound<'_, PyAny>, Bound<'_, PyAny>)>)> = Vec::new();
    for (t, ids) in idx {
        let mut lst = Vec::new();
        for i in ids {
            let d = if i < nd { devs.get_item(i)? } else { py.None().into_bound(py) };
            if let Some(dd) = dev_def(c, &d)? {
                lst.push((i, d, dd));
            }
        }
        if !lst.is_empty() {
            by_type.push((t, lst));
        }
    }
    let storage = Construct::g_storage(con(&c)?)?;
    if storage.is_truthy()? {
        if let Some((_, procs)) = by_type.iter().find(|x| x.0 == "processor") {
            // 古いセーブ：構築物で共有していた資源は、最初の精錬機・加工機に入れる
            let st0 = procs[0].1.get_item("state")?.dsetdefault("store", PyDict::new_bound(py))?;
            for it in storage.call_method0("items")?.iter()? {
                let (k, v): (Bound<'_, PyAny>, Bound<'_, PyAny>) = it?.extract()?;
                let cur = st0.dget2(&k, 0.0)?;
                st0.set_item(&k, cur.add(v)?)?;
            }
            Construct::s_storage(con(&c)?, PyDict::new_bound(py))?;
        }
    }
    let (plan, order) = native_plan(py, c, &net, &by_type)?;
    // 1ティック
    let states = PyList::empty_bound(py);
    for &i in &order {
        states.append(devs.get_item(i)?.get_item("state")?)?;
    }
    let cav_air = Construct::g_cavity_air(con(&c)?)?;
    let reps: Vec<Bound<'_, PyAny>> = net.getattr("cavities")?.iter()?.map(|cv| cv?.get_item("rep")).collect::<R<_>>()?;
    let mut cav = Vec::with_capacity(reps.len());
    for r in &reps {
        cav.push(float_of(&cav_air.dget2(r, 0.0)?)?);
    }
    let cav_arr = PyArray1::from_vec_bound(py, cav);
    c.call_method1("_contig", ("pos", "force"))?;
    let np = crate::game::imp(py, "numpy")?;
    let mass = np.getattr("ascontiguousarray")?.call1((Construct::g_mass(con(&c)?)?, "float64"))?;
    let keys = PyList::empty_bound(py);
    if let Some(k) = input_keys.filter(|k| !k.is_none()) {
        for x in k.iter()? {
            keys.append(x?.str()?)?;
        }
    }
    // ambient_kpa：その場の空気の密度（地表を 1。ジェット用。A-01）
    let res = plan.call_method1("update", (states, dt, keys, seated, Construct::g_pos(con(&c)?)?, mass, Construct::g_force(con(&c)?)?, &cav_arr, ambient_kpa))?;
    let production: f64 = res.get_item(0)?.extract()?;
    let stored: f64 = res.get_item(2)?.extract()?;
    let ratio: f64 = res.get_item(4)?.extract()?;
    let ca: Vec<f64> = cav_arr.call_method0("tolist")?.extract()?;
    for (r, a) in reps.iter().zip(ca) {
        cav_air.set_item(r, a)?;
    }
    Construct::s_powered(con(&c)?, stored > 1.0 || production > 0.0)?;
    let status = PyDict::new_bound(py);
    status.set_item("production", production)?;
    status.set_item("demand", res.get_item(1)?)?;
    status.set_item("stored", stored)?;
    status.set_item("capacity", res.get_item(3)?)?;
    status.set_item("ratio", ratio)?;
    Construct::s_status(con(&c)?, status)?;
    for n in res.get_item(6)?.iter()? {
        note(c, &n?)?;
    }
    let powered: Vec<usize> = res.get_item(5)?.extract()?;
    if let Some((_, acts)) = by_type.iter().find(|x| x.0 == "actuator") {
        for (i, _, dd) in acts {
            crate::game::actuator::act_step(c, *i, dd, dt, powered.contains(i), ratio)?;
        }
    }
    // タンクの中身の重さ（A-01）：使った・入れた分が全体の重さの 2% を超えたら、頂点の質量を計算し直す
    if let Some((_, tanks)) = by_type.iter().find(|x| x.0 == "tank") {
        let cc = con(&c)?;
        let reg = Construct::g_reg(cc)?;
        let mut m = 0.0;
        for (_, d, _) in tanks {
            if let Ok(Some(st)) = d.get_item("state").map(|s| s.downcast_into::<PyDict>().ok()) {
                if let Some(cont) = st.get_item("contents")? {
                    m += Construct::contents_mass(&reg, &cont)?;
                }
            }
        }
        let (r, tot) = { let b = cc.borrow(); (b.mass_ref, b.mass_total) };
        if (m - r).abs() > (tot * 0.02).max(1.0) {
            Construct::refresh_mass(cc)?;
        }
    }
    Ok(())
}

/// 空洞ごとの (代表のセル, セルの数, 気圧 kPa)
#[pyfunction]
pub fn dev_cavity_report<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyList>> {
    let net = ensure_net(py, c)?;
    let out = PyList::empty_bound(py);
    let air = Construct::g_cavity_air(con(&c)?)?;
    for cav in net.getattr("cavities")?.iter()? {
        let cav = cav?;
        let rep = cav.get_item("rep")?;
        let a = float_of(&air.dget2(&rep, 0.0)?)?;
        let v = float_of(&cav.get_item("volume")?)?;
        out.append((rep, cav.get_item("cells")?.len()?, crate::game::airtight::airtight_pressure_kpa(a, v)))?;
    }
    Ok(out)
}

fn pct0(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
}

/// E キーで機器を操作。メッセージを返す
#[pyfunction]
#[pyo3(signature = (c, i, player=None))]
pub fn dev_interact(py: Python<'_>, c: &Bound<'_, PyAny>, i: usize, player: Option<PyObject>) -> R<Option<String>> {
    let _ = player;
    let d = Construct::g_devs(con(&c)?)?.get_item(i)?;
    if !d.is_truthy()? {
        return Ok(None);
    }
    let dd = Construct::g_reg(con(&c)?)?.call_method1("device", (d.get_item("def")?,))?;
    if !dd.is_truthy()? {
        return Ok(Some(format!("未定義の機器: {}", d.get_item("def")?.str()?)));
    }
    let st = d.get_item("state")?;
    let t: String = dd.get_item("type")?.extract()?;
    let name = d.get_item("name")?.str()?.to_string();
    let g = |k: &str, dflt: PyObject| -> R<Bound<'_, PyAny>> { st.dget2(k, dflt) };
    Ok(Some(match t.as_str() {
        "valve" => {
            let o = dev_valve_opening(&st)?;
            if o > 0.0 {
                st.set_item("set", o)?;
                st.set_item("open", 0.0)?;
            } else {
                let s = float_of(&g("set", 1.0.into_py(py))?)?;
                st.set_item("open", if s != 0.0 { s } else { 1.0 })?;
            }
            let op = float_of(&st.get_item("open")?)?;
            format!("{name}: {}", if op <= 0.0 { "閉".to_string() } else { format!("開（{}。開き具合は P で）", pct0(op)) })
        }
        "anchor" => format!("{name}: {} を惑星に固定しています（ブロック {} 個。アクチュエーターの先は動けます。アンカーとはなれた部分は動きます）",
                            Construct::g_name(con(&c)?)?.str()?, Construct::n(con(&c)?)?.into_py(py).into_bound(py).str()?),
        "crank" => {
            let e = float_of(&dd.get_item("params")?.dget2("energy_per_use", 5000)?)?;
            let cur = float_of(&g("pending", 0.0.into_py(py))?)?;
            st.set_item("pending", cur + e)?;
            format!("{name}: {:.1} kJ 発電", e / 1000.0)
        }
        "computer" => {
            let running = !g("running", false.into_py(py))?.is_truthy()?;
            st.set_item("running", running)?;
            st.dpop2("error", py.None())?;
            let sc = st.dget1("script")?;
            let scs = if sc.is_truthy()? { sc.str()?.to_string() } else { "スクリプト未設定".into() };
            format!("{name}: {}（{scs}）", if running { "起動" } else { "停止" })
        }
        "generator" | "processor" | "air_vent" | "antenna" | "light" | "custom" => {
            if t == "air_vent" && g("enabled", true.into_py(py))?.is_truthy()? {
                let m = if g("mode", "fill".into_py(py))?.eq("fill")? { "drain" } else { "fill" };
                st.set_item("mode", m)?;
                return Ok(Some(format!("{name}: モード {m}")));
            }
            let en = !g("enabled", true.into_py(py))?.is_truthy()?;
            st.set_item("enabled", en)?;
            format!("{name}: {}", if en { "ON" } else { "OFF" })
        }
        "thruster" => {
            if g("lit", false.into_py(py))?.is_truthy()? {
                format!("{name}: 燃焼中（止める・出力を変えるには、つながるバルブで）")
            } else {
                st.set_item("ignite", true)?;
                st.set_item("_ask", true)?; // 結果（点火した・できない理由）は次のティックで知らせる
                return Ok(None);
            }
        }
        "tank" => {
            let mut parts = Vec::new();
            for it in st.get_item("contents")?.call_method0("items")?.iter()? {
                let (k, v): (Bound<'_, PyAny>, Bound<'_, PyAny>) = it?.extract()?;
                parts.push(format!("{} {}kg", k.str()?, v.call_method1("__format__", (".0f",))?));
            }
            format!("{name}: {}", if parts.is_empty() { "空".to_string() } else { parts.join(", ") })
        }
        "battery" => format!("{name}: {:.2} MJ", float_of(&g("charge", 0.into_py(py))?)? / 1e6),
        _ => name,
    }))
}
