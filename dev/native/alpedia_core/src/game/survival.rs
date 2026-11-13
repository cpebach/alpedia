//! サバイバル（本体の遊び方の1つ）：ブロックを置くのに資源を払い、掘って・精錬して資源を集める。main/survival.py の中身。
//!
//! world.json の "mode" が "survival" のワールドで、ワールドの拡張の先頭に入る（MOD ではない）。
//! "mode" が無い・"creative" ならクリエイティブ（資源なしで何でも置ける・タンクは満タン）。

use pyo3::prelude::*;
use pyo3::types::{PyDict, PySet, PyTuple};

use crate::game::hooks::Addon;

type R<T> = PyResult<T>;

/// 1回掘ると手に入る資源（岩のブロック5個・鋼鉄のブロック1個分）
pub const DIG_AMOUNT: f64 = 10.0;
/// 1回くむと手に入る水（海や池の水は減らない）
pub const WATER_AMOUNT: f64 = 20.0;
/// 最初の所持品（詰まないように：精錬機・手回し発電機・太陽光パネル・タンクが作れて、少し飛べる）
pub const STARTER_KIT: [(&str, f64); 4] = [("iron", 250.0), ("silica", 40.0), ("parts", 12.0), ("fuel", 60.0)];
/// サバイバルの目標（上から順に。画面の左上に出す）：(ID, 表示)
pub const GOALS: [(&str, &str); 7] = [
    ("ore", "地面を掘って鉱石を 30 集める（左クリック）"),
    ("refiner", "精錬機を置く（I → 機器 → 精錬機）"),
    ("iron", "精錬機に鉱石を入れて、鉄を受け取る（精錬機を E）"),
    ("fuel", "燃料を 100 ためる（硫黄鉱石・水を精錬。タンクの分も数える）"),
    ("fly", "推進機の乗り物に乗って、高さ 50 m まで飛ぶ"),
    ("orbit", "軌道に乗る（近点が地表より上）"),
    ("moon", "月に降り立つ"),
];

fn goal_text(g: &str) -> &'static str {
    GOALS.iter().find(|x| x.0 == g).map(|x| x.1).unwrap_or("")
}

fn fget(d: &Bound<'_, PyAny>, k: &str) -> R<f64> {
    d.call_method1("get", (k, 0.0))?.extract()
}

fn fmt0(v: f64) -> String {
    format!("{:.0}", v)
}

/// ワールドの遊び方："survival" か "creative"
#[pyfunction]
pub fn survival_mode_of(settings: &Bound<'_, PyAny>) -> R<&'static str> {
    if !settings.is_none() && settings.is_truthy()? && settings.call_method1("get", ("mode",))?.eq("survival")? {
        return Ok("survival");
    }
    Ok("creative")
}

#[pyclass(module = "alpedia_core", extends = Addon, subclass, dict)]
pub struct SurvivalRules {
    goal_t: f64,
}

fn st<'py>(world: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let py = world.py();
    world.getattr("settings")?.call_method1("setdefault", ("survival", PyDict::new_bound(py)))?.call_method1("setdefault", ("goals", PyDict::new_bound(py)))
}

fn message(world: &Bound<'_, PyAny>, text: String) -> R<()> {
    world.call_method1("message", (text,))?;
    Ok(())
}

fn res_name(world: &Bound<'_, PyAny>, k: &Bound<'_, PyAny>) -> R<String> {
    Ok(world.getattr("registry")?.call_method1("res_name", (k,))?.str()?.to_string())
}

fn first_part(t: &str) -> &str {
    t.split('（').next().unwrap_or(t)
}

fn check_goals(world: &Bound<'_, PyAny>) -> R<()> {
    let py = world.py();
    let s = st(world)?;
    let p = world.getattr("player")?;
    let inv = p.getattr("inventory")?;
    let done = |g: &str| -> R<()> {
        if !s.call_method1("get", (g,))?.is_truthy()? {
            s.set_item(g, true)?;
            message(world, format!("◎ 目標達成：{}", first_part(goal_text(g))))?;
        }
        Ok(())
    };
    if fget(&inv, "ore")? >= 30.0 {
        done("ore")?;
    }
    let cons: Vec<Bound<'_, PyAny>> = world.getattr("constructs")?.call_method0("values")?.iter()?.collect::<R<_>>()?;
    if !s.call_method1("get", ("refiner",))?.is_truthy()? {
        let mut any = false;
        'outer: for c in &cons {
            for d in c.getattr("devs")?.iter()? {
                let d = d?;
                if d.is_truthy()? && d.get_item("def")?.eq("refiner")? {
                    any = true;
                    break 'outer;
                }
            }
        }
        if any {
            done("refiner")?;
        }
    }
    let mut fuel = fget(&inv, "fuel")?;
    for c in &cons {
        for d in c.getattr("devs")?.iter()? {
            let d = d?;
            if d.is_truthy()? && d.get_item("def")?.eq("tank")? {
                let cont = d.get_item("state")?.call_method1("get", ("contents", PyDict::new_bound(py)))?;
                fuel += py.import_bound("builtins")?.getattr("float")?.call1((cont.call_method1("get", ("fuel", 0.0))?,))?.extract::<f64>()?;
            }
        }
    }
    if fuel >= 100.0 {
        done("fuel")?;
    }
    let seated = p.getattr("seated")?;
    if seated.is_truthy()? {
        let c = world.getattr("constructs")?.call_method1("get", (seated.get_item(0)?,))?;
        if !c.is_none() {
            let o = world.call_method1("orbit_info", (c.call_method0("center_of_mass")?, c.call_method0("velocity")?))?;
            if o.is_truthy()? {
                if fget(&o, "altitude")? >= 50.0 {
                    done("fly")?;
                }
                let peri: f64 = o.call_method1("get", ("periapsis", -1.0))?.extract()?;
                let apo: f64 = o.call_method1("get", ("apoapsis", 0.0))?.extract()?;
                if peri > 0.0 && apo != f64::INFINITY {
                    done("orbit")?;
                }
            }
        }
    }
    let planets = world.getattr("planets")?;
    if planets.len()? > 1 && p.getattr("on_ground")?.is_truthy()? && !seated.is_truthy()? {
        let pl = world.call_method1("nearest_planet", (p.getattr("pos")?,))?;
        if !pl.is_none() && !pl.is(&planets.get_item(0)?) {
            let name = match pl.getattr("name") {
                Ok(n) => n.str()?.to_string(),
                Err(_) => String::new(),
            };
            if name.contains("ルナ") || name.contains("月") {
                done("moon")?;
            }
        }
    }
    Ok(())
}

#[pymethods]
impl SurvivalRules {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> (Self, Addon) {
        let _ = (args, kwargs);
        (SurvivalRules { goal_t: 0.0 }, Addon {})
    }

    #[classattr]
    fn name() -> &'static str {
        "survival"
    }
    #[classattr]
    fn resource_rules() -> bool {
        true
    }
    #[classattr]
    fn settings_keys(py: Python<'_>) -> Py<PyTuple> {
        PyTuple::new_bound(py, ["survival"]).unbind()
    }
    /// お金は口座にある扱いでなくさない
    #[classattr]
    #[allow(non_snake_case)]
    fn KEEP_ON_RESPAWN(py: Python<'_>) -> Py<PyTuple> {
        PyTuple::new_bound(py, ["credits"]).unbind()
    }

    /// 画面用：[(ID, 表示, 済んだか)]
    fn goals_state(&self, world: &Bound<'_, PyAny>) -> R<Vec<(&'static str, &'static str, bool)>> {
        let s = st(world)?;
        GOALS.iter().map(|(g, t)| Ok((*g, *t, s.call_method1("get", (*g,))?.is_truthy()?))).collect()
    }

    fn before_physics<'py>(&mut self, py: Python<'py>, world: &Bound<'py, PyAny>, dt: f64) -> R<Bound<'py, PySet>> {
        self.goal_t += dt;
        if self.goal_t >= 0.5 {
            self.goal_t = 0.0;
            let _ = check_goals(world);
        }
        PySet::empty_bound(py)
    }

    // --- 読み込み -----------------------------------------------------------
    fn on_load(&self, world: &Bound<'_, PyAny>) -> R<()> {
        let p = world.getattr("player")?;
        if p.getattr("starter_given").map(|v| v.is_truthy().unwrap_or(false)).unwrap_or(false) {
            return Ok(());
        }
        let inv = p.getattr("inventory")?;
        for (k, v) in STARTER_KIT {
            let cur: f64 = fget(&inv, k)?;
            inv.set_item(k, cur.max(v))?;
        }
        p.setattr("starter_given", true)?;
        message(world, "サバイバル開始: 最初の資材を受け取りました（I でインベントリ）".into())
    }

    /// マルチプレイ：参加した人にも最初の資材を（world.player はその人）
    fn on_player_join(&self, world: &Bound<'_, PyAny>, player: PyObject) -> R<()> {
        let _ = player;
        self.on_load(world)
    }

    fn on_respawn(slf: &Bound<'_, Self>, world: &Bound<'_, PyAny>, player: &Bound<'_, PyAny>) -> R<Option<String>> {
        let py = slf.py();
        let keep = slf.getattr("KEEP_ON_RESPAWN")?;
        let inv = player.getattr("inventory")?;
        let lost = PyDict::new_bound(py);
        for it in inv.call_method0("items")?.iter()? {
            let (k, v): (Bound<'_, PyAny>, Bound<'_, PyAny>) = it?.extract()?;
            if v.gt(0)? && !keep.contains(&k)? {
                lost.set_item(k, v)?;
            }
        }
        for k in lost.keys() {
            inv.set_item(k, 0.0)?;
        }
        if lost.is_empty() {
            return Ok(None);
        }
        let t = world.getattr("registry")?.call_method1("res_text", (lost, "、"))?.str()?.to_string();
        Ok(Some(format!("手持ちの資源をなくしました（{t}）。部屋の収納箱に預けた物は無事です")))
    }

    // --- 置く・外す ---------------------------------------------------------
    fn pay(&self, world: &Bound<'_, PyAny>, cost: &Bound<'_, PyAny>) -> R<bool> {
        let py = world.py();
        let inv = world.getattr("player")?.getattr("inventory")?;
        let short = PyDict::new_bound(py);
        for it in cost.call_method0("items")?.iter()? {
            let (k, v): (Bound<'_, PyAny>, Bound<'_, PyAny>) = it?.extract()?;
            let have = inv.call_method1("get", (&k, 0.0))?;
            if have.lt(&v)? {
                short.set_item(&k, v.sub(&have)?)?;
            }
        }
        if !short.is_empty() {
            let t = world.getattr("registry")?.call_method1("res_text", (short, ", "))?.str()?.to_string();
            message(world, format!("資源が足りません（不足: {t}）"))?;
            return Ok(false);
        }
        for it in cost.call_method0("items")?.iter()? {
            let (k, v): (Bound<'_, PyAny>, Bound<'_, PyAny>) = it?.extract()?;
            let cur = inv.get_item(&k)?;
            inv.set_item(&k, cur.sub(v)?)?;
        }
        Ok(true)
    }

    fn refund(&self, world: &Bound<'_, PyAny>, cost: &Bound<'_, PyAny>) -> R<()> {
        let inv = world.getattr("player")?.getattr("inventory")?;
        for it in cost.call_method0("items")?.iter()? {
            let (k, v): (Bound<'_, PyAny>, Bound<'_, PyAny>) = it?.extract()?;
            let cur = inv.call_method1("get", (&k, 0.0))?;
            inv.set_item(&k, cur.add(v)?)?;
        }
        Ok(())
    }

    /// 置いたばかりのバッテリーは空（置き直しで電気が増えないように）
    fn on_device_placed(&self, world: &Bound<'_, PyAny>, c: &Bound<'_, PyAny>, i: usize) -> R<()> {
        let d = c.getattr("devs")?.get_item(i)?;
        let dd = world.getattr("registry")?.call_method1("device", (d.get_item("def")?,))?;
        if dd.is_truthy()? && dd.get_item("type")?.eq("battery")? {
            d.get_item("state")?.set_item("charge", 0.0)?;
        }
        Ok(())
    }

    // --- 集める ---------------------------------------------------------------
    fn _gain(&self, world: &Bound<'_, PyAny>, res: &Bound<'_, PyAny>, amount: f64) -> R<()> {
        let inv = world.getattr("player")?.getattr("inventory")?;
        let cur = inv.call_method1("get", (res, 0.0))?;
        let new = cur.add(amount)?;
        inv.set_item(res, &new)?;
        let total: f64 = new.extract()?;
        message(world, format!("{} +{}（所持 {}）", res_name(world, res)?, fmt0(amount), fmt0(total)))
    }

    fn on_dig(&self, world: &Bound<'_, PyAny>, hit: PyObject, resource: &Bound<'_, PyAny>) -> R<()> {
        let _ = hit;
        self._gain(world, resource, DIG_AMOUNT)
    }

    fn on_water(&self, world: &Bound<'_, PyAny>, hit: PyObject) -> R<bool> {
        let _ = hit;
        let w = pyo3::types::PyString::new_bound(world.py(), "water");
        self._gain(world, w.as_any(), WATER_AMOUNT)?;
        Ok(true)
    }

    // --- 機器 -----------------------------------------------------------------
    fn interact_device(slf: &Bound<'_, Self>, world: &Bound<'_, PyAny>, c: &Bound<'_, PyAny>, i: PyObject, d: &Bound<'_, PyAny>,
                       dd: &Bound<'_, PyAny>) -> R<Option<String>> {
        let _ = i;
        if !dd.is_truthy()? {
            return Ok(None);
        }
        let t = dd.get_item("type")?;
        if t.eq("tank")? {
            return Ok(Some(fill_tank(world, d, dd)?));
        }
        if t.eq("processor")? {
            return exchange(world, c, d, dd);
        }
        let _ = slf;
        Ok(None)
    }

    /// 手持ちの燃料が要る（噴かしている間だけ減る）
    fn jetpack(&self, py: Python<'_>, world: &Bound<'_, PyAny>, p: &Bound<'_, PyAny>, dt: f64, thrust_dir: &Bound<'_, PyAny>)
        -> R<(bool, PyObject)> {
        let rate: f64 = py.import_bound("builtins")?.getattr("float")?.call1((p.getattr("defn")?.call_method1("get", ("jetpack_fuel_rate", 1.0))?,))?.extract()?;
        let usef = rate * dt;
        let np = py.import_bound("numpy")?;
        let inv = p.getattr("inventory")?;
        let td: Vec<f64> = np.getattr("asarray")?.call1((thrust_dir, "float64"))?.call_method0("ravel")?.call_method0("tolist")?.extract()?;
        let up: Vec<f64> = np.getattr("asarray")?.call1((p.getattr("up")?, "float64"))?.call_method0("ravel")?.call_method0("tolist")?.extract()?;
        let have = fget(&inv, "fuel")?;
        if p.getattr("on_ground")?.is_truthy()? && td.iter().zip(up.iter()).map(|(a, b)| a * b).sum::<f64>() <= 0.0 {
            return Ok((have > 0.0, np.getattr("zeros")?.call1((3,))?.unbind()));   // 地上を歩いているだけなら減らない
        }
        if td.iter().map(|x| x * x).sum::<f64>().sqrt() > 0.0 {
            if have <= 0.0 {
                if !p.getattr("out_of_fuel")?.is_truthy()? {
                    message(world, "燃料がないのでジェットパックが使えません（硫黄鉱石や水を精錬して燃料に）".into())?;
                }
                p.setattr("out_of_fuel", true)?;
                return Ok((false, thrust_dir.clone().unbind()));
            }
            inv.set_item("fuel", (have - usef).max(0.0))?;
            p.setattr("out_of_fuel", false)?;
            return Ok((true, thrust_dir.clone().unbind()));
        }
        Ok((have > 0.0, thrust_dir.clone().unbind()))
    }
}

/// 手持ちの燃料をタンクに入れる（入りきらない分は手元に残る）
fn fill_tank(world: &Bound<'_, PyAny>, d: &Bound<'_, PyAny>, dd: &Bound<'_, PyAny>) -> R<String> {
    let py = world.py();
    let p = world.getattr("player")?;
    let fl = py.import_bound("builtins")?.getattr("float")?;
    let cap: f64 = fl.call1((dd.get_item("params")?.call_method1("get", ("capacity", 0))?,))?.extract()?;
    let cont = d.get_item("state")?.call_method1("setdefault", ("contents", PyDict::new_bound(py)))?;
    let inv = p.getattr("inventory")?;
    let have = fget(&inv, "fuel")?;
    let mut total = 0.0;
    for v in cont.call_method0("values")?.iter()? {
        total += v?.extract::<f64>()?;
    }
    let room = (cap - total).max(0.0);
    let name = d.get_item("name")?.str()?.to_string();
    if have <= 0.0 {
        let mut parts = Vec::new();
        for it in cont.call_method0("items")?.iter()? {
            let (k, v): (Bound<'_, PyAny>, f64) = it?.extract()?;
            if v != 0.0 {
                parts.push(format!("{} {}", res_name(world, &k)?, fmt0(v)));
            }
        }
        let now = if parts.is_empty() { "空".to_string() } else { parts.join(", ") };
        return Ok(format!("{name}: {now} / {}（燃料を持っていません。硫黄鉱石や水を精錬機で燃料に）", fmt0(cap)));
    }
    if room <= 0.0 {
        return Ok(format!("{name}: 満タンです"));
    }
    let mv = have.min(room);
    let cur = fget(&cont, "fuel")?;
    cont.set_item("fuel", cur + mv)?;
    inv.set_item("fuel", have - mv)?;
    Ok(format!("{name}: 燃料 {} を入れました（タンク {}/{}、手持ち {}）", fmt0(mv), fmt0(total + mv), fmt0(cap), fmt0(have - mv)))
}

/// 精錬機・加工機：所持品の原料を入れ、できた物を受け取る（何も動かなければ None：ふつうの操作へ）
fn exchange(world: &Bound<'_, PyAny>, c: &Bound<'_, PyAny>, d: &Bound<'_, PyAny>, dd: &Bound<'_, PyAny>) -> R<Option<String>> {
    let py = world.py();
    let dev = py.import_bound("main.devices")?;
    let p = world.getattr("player")?;
    let inv = p.getattr("inventory")?;
    let mut moved: Vec<String> = Vec::new();
    let store = d.get_item("state")?.call_method1("setdefault", ("store", PyDict::new_bound(py)))?;
    for k in dev.getattr("proc_inputs")?.call1((dd,))?.iter()? {
        let k = k?;
        let amt = inv.call_method1("pop", (&k, 0.0))?;
        if amt.is_truthy()? {
            let cur = store.call_method1("get", (&k, 0.0))?;
            store.set_item(&k, cur.add(&amt)?)?;
            moved.push(format!("{} {} 投入", res_name(world, &k)?, fmt0(amt.extract()?)));
        }
    }
    for k in dev.getattr("proc_outputs")?.call1((dd,))?.iter()? {
        let k = k?;
        let amt: f64 = store.call_method1("pop", (&k, 0.0))?.extract()?;
        if amt >= 1e-6 {
            let cur = inv.call_method1("get", (&k, 0.0))?;
            inv.set_item(&k, cur.add(amt)?)?;
            moved.push(format!("{} {} 回収", res_name(world, &k)?, fmt0(amt)));
            let s = st(world)?;
            if k.eq("iron")? && !s.call_method1("get", ("iron",))?.is_truthy()? {
                s.set_item("iron", true)?;
                message(world, format!("◎ 目標達成：{}", first_part(goal_text("iron"))))?;
            }
        }
    }
    if moved.is_empty() {
        return Ok(None);
    }
    let stt = c.getattr("status")?;
    let stt = if stt.is_truthy()? { stt } else { PyDict::new_bound(py).into_any() };
    let stored: f64 = stt.call_method1("get", ("stored", 0))?.extract()?;
    let prod: f64 = stt.call_method1("get", ("production", 0))?.extract()?;
    let hint = if stored > 1.0 || prod > 0.0 { "" } else { "（電気がありません：手回し発電機・太陽光パネル・燃えている推進機をつなげて）" };
    Ok(Some(format!("{}: {}{hint}", d.get_item("name")?.str()?, moved.join(", "))))
}
