//! 機器の1ティック（main/devices.py の update と同じ計算）。
//!
//! 電力（手回し・太陽光・バッテリー）、推進機（点火・燃焼・推力・発電）、配管の流れ（区画とバルブ）、
//! 精錬機・加工機、タンク、通気口、アンテナ・ライト・MOD の機器の ON/OFF。
//! 機器の状態は Python の dict（d["state"]）のまま読み書きする（MOD とスクリプトからは今までどおり見える）。
//! アクチュエーターは Python（actuator.step）が、ここで決めた電力の配分のあとに動かす。

use std::collections::{HashMap, HashSet, VecDeque};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyDictMethods};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum T {
    Crank,
    Battery,
    Generator,
    Thruster,
    Computer,
    Actuator,
    Processor,
    AirVent,
    Antenna,
    Light,
    Custom,
    Tank,
    Other,
}

impl T {
    pub fn of(s: &str) -> T {
        match s {
            "crank" => T::Crank,
            "battery" => T::Battery,
            "generator" => T::Generator,
            "thruster" => T::Thruster,
            "computer" => T::Computer,
            "actuator" => T::Actuator,
            "processor" => T::Processor,
            "air_vent" => T::AirVent,
            "antenna" => T::Antenna,
            "light" => T::Light,
            "custom" => T::Custom,
            "tank" => T::Tank,
            _ => T::Other,
        }
    }
}

pub struct Recipe {
    pub inp: Vec<(String, f64)>,
    pub out: Vec<(String, f64)>,
    pub time: f64,
    pub power: Option<f64>,
}

pub struct Dev {
    pub vox: usize,
    pub t: T,
    pub name: String,
    pub dir: i64,
    pub electric: bool,
    pub power_use: f64,
    pub capacity: f64,
    pub output: f64,
    pub fuel: String,
    pub fuel_rate: f64,
    pub thrust: f64,
    pub power_output: f64,
    pub ignite_energy: f64,
    pub max_flow: f64,
    pub key: Option<String>,
    pub recipes: Vec<Recipe>,
    pub inputs: Vec<(String, f64)>,  // proc_inputs
    pub outputs: Vec<String>,        // proc_outputs
    pub rate: f64,                   // 通気口
    pub target_kpa: f64,
    pub cavity: Option<usize>,       // 通気口のとなりの空洞
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Node {
    S(usize),
    V(usize),
}

pub struct Plan {
    pub devs: Vec<Dev>,
    pub at: HashMap<usize, usize>, // ボクセル番号 → devs の番号
    pub pipe_of: HashMap<usize, usize>,
    pub pipes: Vec<Vec<usize>>,
    pub valve_adj: HashMap<usize, Vec<Node>>,
    pub valves: Vec<usize>, // バルブ（valve_adj の順）
    pub seg_adj: HashMap<usize, Vec<usize>>,
    pub fluids: HashSet<String>,
    pub cav_volume: Vec<f64>,
}

const LOCAL_FRACTION: f64 = 0.15;
const ATM_KPA: f64 = 101.3;
const AIR_DENSITY_1ATM: f64 = 1.2;

// ---- dict の読み書き ----
fn gf(st: &Bound<'_, PyDict>, k: &str, dflt: f64) -> f64 {
    match st.get_item(k) {
        Ok(Some(v)) => v.extract::<f64>().unwrap_or(dflt),
        _ => dflt,
    }
}

fn gb(st: &Bound<'_, PyDict>, k: &str, dflt: bool) -> bool {
    match st.get_item(k) {
        Ok(Some(v)) => v.is_truthy().unwrap_or(dflt),
        _ => dflt,
    }
}

fn sf(st: &Bound<'_, PyDict>, k: &str, v: f64) -> PyResult<()> {
    st.set_item(k, v)
}

fn sb(st: &Bound<'_, PyDict>, k: &str, v: bool) -> PyResult<()> {
    st.set_item(k, v)
}

fn pop_truthy(st: &Bound<'_, PyDict>, k: &str) -> PyResult<bool> {
    match st.get_item(k)? {
        Some(v) => {
            let b = v.is_truthy()?;
            st.del_item(k)?;
            Ok(b)
        }
        None => Ok(false),
    }
}

fn pop_f(st: &Bound<'_, PyDict>, k: &str) -> PyResult<f64> {
    match st.get_item(k)? {
        Some(v) => {
            let x = v.extract::<f64>().unwrap_or(0.0);
            st.del_item(k)?;
            Ok(x)
        }
        None => Ok(0.0),
    }
}

/// st.setdefault(k, {})
fn sub<'py>(st: &Bound<'py, PyDict>, k: &str) -> PyResult<Bound<'py, PyDict>> {
    if let Some(v) = st.get_item(k)? {
        if let Ok(d) = v.downcast_into::<PyDict>() {
            return Ok(d);
        }
    }
    let d = PyDict::new_bound(st.py());
    st.set_item(k, &d)?;
    Ok(d)
}

fn dsum(d: &Bound<'_, PyDict>) -> f64 {
    d.iter().map(|(_, v)| v.extract::<f64>().unwrap_or(0.0)).sum()
}

/// 1ティックの流れ（devices.Flow）
struct Flow<'a> {
    p: &'a Plan,
    rem: HashMap<usize, f64>,
    opened: HashSet<usize>,
}

impl<'a> Flow<'a> {
    fn new(p: &'a Plan, st: &[Bound<'_, PyDict>], dt: f64) -> Self {
        let mut rem = HashMap::new();
        let mut opened = HashSet::new();
        for &v in &p.valves {
            let mf = p.at.get(&v).map(|&k| p.devs[k].max_flow).unwrap_or(6.0);
            let o = p.at.get(&v).map(|&k| gf(&st[k], "open", 1.0)).unwrap_or(1.0).clamp(0.0, 1.0);
            let r = o * mf * dt;
            rem.insert(v, r);
            if r > 0.0 {
                opened.insert(v);
            }
        }
        Flow { p, rem, opened }
    }

    /// 機器（ボクセル i）から届く区画を近い順に：[(区画, 通ったバルブ)]
    fn reach(&self, i: usize, by_open: bool) -> Vec<(usize, Vec<usize>)> {
        let p = self.p;
        let start = if let Some(&s) = p.pipe_of.get(&i) {
            Node::S(s)
        } else if p.valve_adj.contains_key(&i) {
            Node::V(i)
        } else {
            return Vec::new();
        };
        let mut seen: HashSet<Node> = HashSet::new();
        seen.insert(start);
        let mut out = Vec::new();
        let mut q: VecDeque<(Node, Vec<usize>)> = VecDeque::new();
        q.push_back((start, Vec::new()));
        while let Some((node, mut path)) = q.pop_front() {
            let nbrs: Vec<Node> = match node {
                Node::S(s) => {
                    out.push((s, path.clone()));
                    p.seg_adj.get(&s).map(|v| v.iter().map(|&x| Node::V(x)).collect()).unwrap_or_default()
                }
                Node::V(v) => {
                    if !path.contains(&v) {
                        path.push(v);
                    }
                    let closed = if by_open { !self.opened.contains(&v) } else { self.rem.get(&v).copied().unwrap_or(0.0) <= 1e-12 };
                    if closed {
                        continue;
                    }
                    p.valve_adj.get(&v).cloned().unwrap_or_default()
                }
            };
            for nb in nbrs {
                if seen.insert(nb) {
                    q.push_back((nb, path.clone()));
                }
            }
        }
        out
    }

    fn cap(&self, path: &[usize]) -> f64 {
        path.iter().map(|v| self.rem.get(v).copied().unwrap_or(0.0)).fold(f64::INFINITY, f64::min)
    }

    fn use_(&mut self, path: &[usize], amount: f64) {
        for v in path {
            if let Some(r) = self.rem.get_mut(v) {
                *r -= amount;
            }
        }
    }

    /// 区画 seg の中で res を出せる所（タンクの中身・精錬機の作った物）
    fn sources<'py>(&self, st: &[Bound<'py, PyDict>], seg: usize, res: &str, tank: bool, proc_: bool, exclude: usize)
        -> PyResult<Vec<Bound<'py, PyDict>>> {
        let mut out = Vec::new();
        for &i in &self.p.pipes[seg] {
            if i == exclude {
                continue;
            }
            let k = match self.p.at.get(&i) {
                Some(&k) => k,
                None => continue,
            };
            let d = &self.p.devs[k];
            if d.t == T::Tank && tank {
                let cont = sub(&st[k], "contents")?;
                if gf(&cont, res, 0.0) > 0.0 {
                    out.push(cont);
                }
            } else if d.t == T::Processor && proc_ && d.outputs.iter().any(|o| o == res) {
                let store = sub(&st[k], "store")?;
                if gf(&store, res, 0.0) > 0.0 {
                    out.push(store);
                }
            }
        }
        Ok(out)
    }

    fn valves_to(&self, st: &[Bound<'_, PyDict>], i: usize, res: &str) -> PyResult<Vec<usize>> {
        for (seg, path) in self.reach(i, false) {
            if !self.sources(st, seg, res, true, true, i)?.is_empty() {
                let mut v = path;
                v.sort_unstable();
                v.dedup();
                return Ok(v);
            }
        }
        Ok(Vec::new())
    }

    fn pull(&mut self, st: &[Bound<'_, PyDict>], i: usize, res: &str, amount: f64, tank: bool, proc_: bool, dry: bool)
        -> PyResult<f64> {
        if amount <= 0.0 {
            return Ok(0.0);
        }
        let mut got = 0.0;
        for (seg, path) in self.reach(i, dry) {
            let cap = if dry { amount - got } else { self.cap(&path).min(amount - got) };
            if cap <= 1e-12 {
                continue;
            }
            let mut took = 0.0;
            for bx in self.sources(st, seg, res, tank, proc_, i)? {
                let have = gf(&bx, res, 0.0);
                let t = have.min(cap - took);
                if t > 0.0 {
                    if !dry {
                        bx.set_item(res, have - t)?;
                    }
                    took += t;
                }
                if took >= cap - 1e-12 {
                    break;
                }
            }
            if took > 0.0 {
                if !dry {
                    self.use_(&path, took);
                }
                got += took;
            }
            if got >= amount - 1e-12 {
                break;
            }
        }
        Ok(got)
    }

    fn put_tanks(&mut self, st: &[Bound<'_, PyDict>], i: usize, res: &str, amount: f64) -> PyResult<f64> {
        let mut put = 0.0;
        for (seg, path) in self.reach(i, false) {
            let cap = self.cap(&path).min(amount - put);
            if cap <= 1e-12 {
                continue;
            }
            let mut done = 0.0;
            for &kv in &self.p.pipes[seg] {
                let k = match self.p.at.get(&kv) {
                    Some(&k) => k,
                    None => continue,
                };
                let d = &self.p.devs[k];
                if kv == i || d.t != T::Tank {
                    continue;
                }
                let cont = sub(&st[k], "contents")?;
                let free = d.capacity - dsum(&cont);
                let add = free.min(cap - done);
                if add > 0.0 {
                    cont.set_item(res, gf(&cont, res, 0.0) + add)?;
                    done += add;
                }
                if done >= cap - 1e-12 {
                    break;
                }
            }
            if done > 0.0 {
                self.use_(&path, done);
                put += done;
            }
            if put >= amount - 1e-12 {
                break;
            }
        }
        Ok(put)
    }
}

fn ready_recipe<'b>(d: &'b Dev, st: &Bound<'_, PyDict>) -> PyResult<Option<(usize, &'b Recipe)>> {
    let rs = &d.recipes;
    if rs.is_empty() {
        return Ok(None);
    }
    let store = match st.get_item("store")? {
        Some(v) => v.downcast_into::<PyDict>().ok(),
        None => None,
    };
    let get = |k: &str| store.as_ref().map(|s| gf(s, k, 0.0)).unwrap_or(0.0);
    let total = store.as_ref().map(dsum).unwrap_or(0.0);
    let start = (gf(st, "next", 0.0) as i64).rem_euclid(rs.len() as i64) as usize;
    for k in 0..rs.len() {
        let j = (start + k) % rs.len();
        let r = &rs[j];
        if !r.inp.iter().all(|(n, v)| get(n) >= *v) {
            continue;
        }
        let after = total - r.inp.iter().map(|x| x.1).sum::<f64>() + r.out.iter().map(|x| x.1).sum::<f64>();
        if after <= d.capacity + 1e-9 {
            return Ok(Some((j, r)));
        }
    }
    Ok(None)
}

fn busy(st: &Bound<'_, PyDict>) -> bool {
    (gf(st, "ext_target", 0.0) - gf(st, "ext", 0.0)).abs() > 1e-4
        || (gf(st, "angle_target", 0.0) - gf(st, "angle", 0.0)).abs() > 1e-3
}

/// ボクセル i の軸（construct.voxel_frames と world_axis）
fn world_axis(pos: &[f64], i: usize, dir: i64) -> [f64; 3] {
    let p = |k: usize, a: usize| pos[3 * (8 * i + k) + a];
    let sets: [([usize; 4], [usize; 4]); 3] =
        [([1, 3, 5, 7], [0, 2, 4, 6]), ([2, 3, 6, 7], [0, 1, 4, 5]), ([4, 5, 6, 7], [0, 1, 2, 3])];
    let ax = (dir / 2) as usize;
    let (hi, lo) = sets[ax.min(2)];
    let mut v = [0.0; 3];
    for a in 0..3 {
        for q in 0..4 {
            v[a] += (p(hi[q], a) - p(lo[q], a)) / 4.0;
        }
    }
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    let s = if dir % 2 == 0 { 1.0 } else { -1.0 };
    [v[0] / n * s, v[1] / n * s, v[2] / n * s]
}

fn apply_point_forces(pos: &[f64], mass: &[f64], force: &mut [f64], forces: &[(usize, [f64; 3])]) {
    use nalgebra::{Matrix3, Vector3};
    let nv = mass.len();
    let msum: f64 = mass.iter().sum();
    let mut com = [0.0; 3];
    for i in 0..nv {
        for a in 0..3 {
            com[a] += pos[3 * i + a] * mass[i];
        }
    }
    for c in com.iter_mut() {
        *c /= msum;
    }
    let mut inertia = Matrix3::identity() * 1e-6;
    for i in 0..nv {
        let r = Vector3::new(pos[3 * i] - com[0], pos[3 * i + 1] - com[1], pos[3 * i + 2] - com[2]);
        inertia += (Matrix3::identity() * r.dot(&r) - r * r.transpose()) * mass[i];
    }
    let iinv = inertia.try_inverse().unwrap_or_else(Matrix3::zeros);
    let mut lin = Vector3::zeros();
    let mut tau = Vector3::zeros();
    for &(i, f) in forces {
        for k in 0..8 {
            for a in 0..3 {
                force[3 * (8 * i + k) + a] += f[a] * LOCAL_FRACTION / 8.0;
            }
        }
        let fr = Vector3::new(f[0], f[1], f[2]) * (1.0 - LOCAL_FRACTION);
        lin += fr;
        let mut c = Vector3::zeros();
        for k in 0..8 {
            for a in 0..3 {
                c[a] += pos[3 * (8 * i + k) + a] / 8.0;
            }
        }
        tau += (c - Vector3::new(com[0], com[1], com[2])).cross(&fr);
    }
    let alpha = iinv * tau;
    for i in 0..nv {
        let r = Vector3::new(pos[3 * i] - com[0], pos[3 * i + 1] - com[1], pos[3 * i + 2] - com[2]);
        let acc = lin / msum + alpha.cross(&r);
        for a in 0..3 {
            force[3 * i + a] += mass[i] * acc[a];
        }
    }
}

pub struct Result_ {
    pub production: f64,
    pub demand: f64,
    pub stored: f64,
    pub capacity: f64,
    pub ratio: f64,
    pub powered: Vec<usize>, // ボクセル番号
    pub notes: Vec<String>,
}

/// devices.update の本体。st：plan.devs と同じ順の state の dict。cav_air：空洞ごとの空気（書き換える）
#[allow(clippy::too_many_arguments)]
pub fn update(
    p: &Plan, st: &[Bound<'_, PyDict>], dt: f64, keys: &HashSet<String>, seated: bool, pos: &[f64], mass: &[f64],
    force: &mut [f64], cav_air: &mut [f64],
) -> PyResult<Result_> {
    let mut notes: Vec<String> = Vec::new();
    let mut note = |s: String| {
        if !notes.contains(&s) {
            notes.push(s);
        }
    };
    let mut flow = Flow::new(p, st, dt);
    let of = |t: T| p.devs.iter().enumerate().filter(move |(_, d)| d.t == t).map(|(k, _)| k);
    // ---- 電力（手回し・太陽光） ----
    let mut crank = 0.0;
    for k in of(T::Crank) {
        crank += pop_f(&st[k], "pending")?;
    }
    let bats: Vec<usize> = of(T::Battery).collect();
    let mut stored: f64 = bats.iter().map(|&k| gf(&st[k], "charge", 0.0)).sum::<f64>() + 0.0; // 空の和の -0 を 0 に
    let capacity: f64 = bats.iter().map(|&k| p.devs[k].capacity).sum::<f64>() + 0.0;
    let mut production = 0.0;
    for k in of(T::Generator) {
        let on = gb(&st[k], "enabled", true);
        sb(&st[k], "active", on)?;
        if on {
            production += p.devs[k].output;
        }
    }
    // ---- 推進機 ----
    let mut ignite_cost = 0.0;
    let pool = stored + crank + production * dt;
    let mut point_forces: Vec<(usize, [f64; 3])> = Vec::new();
    let mut burning: Vec<(usize, f64)> = Vec::new();
    for k in of(T::Thruster) {
        let d = &p.devs[k];
        let s = &st[k];
        let want = pop_truthy(s, "ignite")? || (seated && d.key.as_ref().map(|q| keys.contains(q)).unwrap_or(false));
        let asked = pop_truthy(s, "_ask")?;
        if want && !gb(s, "lit", false) {
            let e = d.ignite_energy;
            if flow.pull(st, d.vox, &d.fuel, 1e-6, true, true, true)? <= 0.0 {
                s.set_item("error", "燃料が届いていません（燃料の入ったタンクとつなぐ・間のバルブを開ける）")?;
            } else if pool - ignite_cost < e {
                s.set_item(
                    "error",
                    format!(
                        "電気が足りず点火できません（点火に {:.0} kJ 要ります。バッテリー・太陽光パネル・手回し発電機を同じ構築物に付ける）",
                        e / 1000.0
                    ),
                )?;
            } else {
                ignite_cost += e;
                sb(s, "lit", true)?;
                if s.contains("error")? {
                    s.del_item("error")?;
                }
                if asked {
                    note(format!("{}: 点火しました", d.name));
                }
            }
            if asked && !gb(s, "lit", false) {
                let err: String = s.get_item("error")?.map(|v| v.extract().unwrap_or_default()).unwrap_or_default();
                note(format!("{}: {}", d.name, err));
            }
        }
        sf(s, "firing", 0.0)?;
        sf(s, "throttle", 0.0)?;
        if gb(s, "lit", false) {
            burning.push((k, d.fuel_rate * dt));
        }
    }
    // 同じバルブの先にいくつも推進機があるときは、バルブを通る量を欲しい量に比べて分ける
    let mut through: HashMap<usize, f64> = HashMap::new();
    let mut vss: Vec<Vec<usize>> = Vec::with_capacity(burning.len());
    for &(k, need) in &burning {
        let vs = flow.valves_to(st, p.devs[k].vox, &p.devs[k].fuel)?;
        for &v in &vs {
            *through.entry(v).or_insert(0.0) += need;
        }
        vss.push(vs);
    }
    let rem0 = flow.rem.clone();
    for (q, &(k, need)) in burning.iter().enumerate() {
        let d = &p.devs[k];
        let s = &st[k];
        let mut quota = need;
        for v in &vss[q] {
            quota = quota.min(rem0.get(v).copied().unwrap_or(0.0) * need / through[v].max(1e-12));
        }
        let got = if quota > 0.0 { flow.pull(st, d.vox, &d.fuel, quota, true, true, false)? } else { 0.0 };
        let frac = if need > 0.0 { got / need } else { 0.0 };
        if frac <= 1e-6 {
            sb(s, "lit", false)?;
            continue;
        }
        let ax = world_axis(pos, d.vox, d.dir);
        point_forces.push((d.vox, [ax[0] * d.thrust * frac, ax[1] * d.thrust * frac, ax[2] * d.thrust * frac]));
        sf(s, "firing", frac)?;
        sf(s, "throttle", frac)?;
        production += d.power_output * frac;
    }
    if !point_forces.is_empty() {
        apply_point_forces(pos, mass, force, &point_forces);
    }
    // ---- 電力の配分 ----
    let mut consumers: Vec<usize> = Vec::new();
    for t in [T::Computer, T::Actuator, T::Processor, T::AirVent, T::Antenna, T::Light, T::Custom] {
        for k in of(t) {
            let d = &p.devs[k];
            if !d.electric {
                continue;
            }
            let s = &st[k];
            let w = match t {
                T::Computer => gb(s, "running", false),
                T::Actuator => busy(s),
                T::Processor => gb(s, "enabled", true) && ready_recipe(d, s)?.is_some(),
                T::AirVent => gb(s, "enabled", true) && d.cavity.is_some(),
                _ => gb(s, "enabled", true),
            };
            if w {
                consumers.push(k);
            }
        }
    }
    let mut demand = 0.0;
    for &k in &consumers {
        let d = &p.devs[k];
        let mut u = d.power_use;
        if d.t == T::Processor {
            if let Some((_, r)) = ready_recipe(d, &st[k])? {
                if let Some(pw) = r.power {
                    u = pw;
                }
            }
        }
        demand += u;
    }
    let avail = production * dt + stored + crank - ignite_cost;
    let ratio = if demand <= 0.0 { 1.0 } else { (avail.max(0.0) / (demand * dt)).min(1.0) };
    let mut energy = production * dt + crank - demand * dt * ratio - ignite_cost;
    if !bats.is_empty() && energy.abs() >= 1e-12 {
        if energy > 0.0 {
            for &k in &bats {
                let ch = gf(&st[k], "charge", 0.0);
                let add = (p.devs[k].capacity - ch).min(energy);
                sf(&st[k], "charge", ch + add)?;
                energy -= add;
                if energy <= 0.0 {
                    break;
                }
            }
        } else {
            let mut need = -energy;
            for &k in &bats {
                let ch = gf(&st[k], "charge", 0.0);
                let take = ch.min(need);
                sf(&st[k], "charge", ch - take)?;
                need -= take;
                if need <= 0.0 {
                    break;
                }
            }
        }
    }
    stored = bats.iter().map(|&k| gf(&st[k], "charge", 0.0)).sum::<f64>() + 0.0;
    let powered: HashSet<usize> = if ratio > 0.0 { consumers.iter().copied().collect() } else { HashSet::new() };
    for k in of(T::Antenna).chain(of(T::Light)) {
        sb(&st[k], "active", powered.contains(&k))?;
    }
    for k in of(T::Custom) {
        let on = gb(&st[k], "enabled", true);
        sb(&st[k], "active", on && (powered.contains(&k) || !p.devs[k].electric))?;
    }
    // ---- 精錬機・加工機 ----
    let procs: Vec<usize> = of(T::Processor).collect();
    for &k in &procs {
        let d = &p.devs[k];
        let store = sub(&st[k], "store")?;
        for (r, amt) in &d.inputs {
            let room = d.capacity - dsum(&store);
            let want = (amt * 2.0 - gf(&store, r, 0.0)).min(room);
            if want > 0.0 {
                let got = flow.pull(st, d.vox, r, want, true, true, false)?;
                if got > 0.0 {
                    store.set_item(r, gf(&store, r, 0.0) + got)?;
                }
            }
        }
    }
    for &k in &procs {
        let d = &p.devs[k];
        let s = &st[k];
        let store = sub(s, "store")?;
        if !powered.contains(&k) {
            sb(s, "working", false)?;
            continue;
        }
        let rr = ready_recipe(d, s)?;
        let (j, r) = match rr {
            None => {
                sb(s, "working", false)?;
                sf(s, "progress", 0.0)?;
                continue;
            }
            Some(x) => x,
        };
        sb(s, "working", true)?;
        let prog = gf(s, "progress", 0.0) + dt * ratio;
        sf(s, "progress", prog)?;
        if prog >= r.time {
            sf(s, "progress", 0.0)?;
            s.set_item("next", ((j + 1) % d.recipes.len().max(1)) as i64)?;
            for (n, a) in &r.inp {
                store.set_item(n, gf(&store, n, 0.0) - a)?;
            }
            for (n, a) in &r.out {
                store.set_item(n, gf(&store, n, 0.0) + a)?;
            }
        }
    }
    // ---- タンク：届く精錬機・加工機の作った燃料など（流れる物）を受け取る ----
    let mut fl_out: Vec<&String> = Vec::new();
    for &k in &procs {
        for r in &p.devs[k].outputs {
            if !fl_out.contains(&r) && p.fluids.contains(r) {
                fl_out.push(r);
            }
        }
    }
    if !fl_out.is_empty() {
        for k in of(T::Tank) {
            let d = &p.devs[k];
            let cont = sub(&st[k], "contents")?;
            let mut free = d.capacity - dsum(&cont);
            if free <= 1e-9 {
                continue;
            }
            for r in &fl_out {
                let got = flow.pull(st, d.vox, r, free, false, true, false)?;
                if got > 0.0 {
                    cont.set_item(r.as_str(), gf(&cont, r, 0.0) + got)?;
                    free -= got;
                }
                if free <= 1e-9 {
                    break;
                }
            }
        }
    }
    // ---- 通気口・気密 ----
    for k in of(T::AirVent) {
        let d = &p.devs[k];
        let s = &st[k];
        sb(s, "cavity", d.cavity.is_some())?;
        let cav = match d.cavity {
            Some(c) if powered.contains(&k) => c,
            _ => continue,
        };
        let air = cav_air[cav];
        let target = d.target_kpa / ATM_KPA * AIR_DENSITY_1ATM * p.cav_volume[cav];
        let rate = d.rate * dt * ratio;
        let mode: String = s.get_item("mode")?.map(|v| v.extract().unwrap_or_else(|_| "fill".into())).unwrap_or_else(|| "fill".into());
        if mode == "fill" && air < target {
            let got = flow.pull(st, d.vox, "air", rate.min(target - air), true, false, false)?;
            cav_air[cav] = air + got;
        } else if mode == "drain" && air > 0.0 {
            let mv = rate.min(air);
            flow.put_tanks(st, d.vox, "air", mv)?;
            cav_air[cav] = air - mv;
        }
    }
    let mut pw: Vec<usize> = powered.iter().map(|&k| p.devs[k].vox).collect();
    pw.sort_unstable();
    Ok(Result_ { production, demand, stored, capacity, ratio, powered: pw, notes })
}
