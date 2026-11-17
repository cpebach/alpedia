//! プレイヤー（描画とは独立した物理・所持品）。main/player.py の中身。
//!
//! 値（位置・速度・所持品など）は Python から読み書きされるので、Python の値のまま持つ（属性）。
//! 毎ティックの動き（重力・水・ジェットパック・歩く・地面や建物・ブロックとの当たり）は Rust で計算する。

use crate::game::planet::{pla, Planet};
use crate::game::PyDictOps;
use crate::game::world::{wld, World};
use numpy::{PyArray1, PyArrayMethods};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyTuple};
use super::construct::{con, fa, to_fa, Construct, FA};

type R<T> = PyResult<T>;
type V3 = [f64; 3];

fn v3(o: &Bound<'_, PyAny>) -> R<V3> {
    let v: Vec<f64> = crate::game::imp(o.py(), "numpy")?.getattr("asarray")?.call1((o, "float64"))?.call_method0("ravel")?.call_method0("tolist")?.extract()?;
    if v.len() < 3 {
        return Err(pyo3::exceptions::PyValueError::new_err("3 つの値が要ります"));
    }
    Ok([v[0], v[1], v[2]])
}

fn arr(py: Python<'_>, v: V3) -> PyObject {
    PyArray1::from_vec_bound(py, v.to_vec()).into_any().unbind()
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

fn float_of(v: &Bound<'_, PyAny>) -> R<f64> {
    crate::game::imp(v.py(), "builtins")?.getattr("float")?.call1((v,))?.extract()
}

/// Python の値として持っているプレイヤーを、型つきで見る（違う物ならエラー）
#[inline]
pub fn ply<'a, 'py>(p: &'a Bound<'py, PyAny>) -> R<&'a Bound<'py, Player>> {
    Ok(p.downcast::<Player>()?)
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Player {
    #[pyo3(get, set)]
    pub defn: PyObject,
    /// 足元の球の中心
    #[pyo3(get, set)]
    pub pos: FA,
    #[pyo3(get, set)]
    pub vel: FA,
    /// どの公転する惑星と一緒に動いているか（-1 は無し、-2 はまだ決めていない。L-08）
    #[pyo3(get, set)]
    pub frame: i64,
    #[pyo3(get, set)]
    pub up: FA,
    #[pyo3(get, set)]
    pub on_ground: bool,
    #[pyo3(get, set)]
    pub ground_vel: FA,
    /// (construct_id, cell)
    #[pyo3(get, set)]
    pub seated: PyObject,
    #[pyo3(get, set)]
    pub inventory: PyObject,
    /// アイテムバー（9枠）: [["material"|"device", id] or None]
    #[pyo3(get, set)]
    pub hotbar: PyObject,
    #[pyo3(get, set)]
    pub jetpack: bool,
    #[pyo3(get, set)]
    pub mass: f64,
    #[pyo3(get, set)]
    pub in_water: f64,
    /// サバイバルの最初の資材を受け取ったか
    #[pyo3(get, set)]
    pub starter_given: bool,
    /// 最初のクレジットを受け取ったか
    #[pyo3(get, set)]
    pub credits_given: bool,
    #[pyo3(get, set)]
    pub out_of_fuel: bool,
    // マルチプレイ・描画用
    #[pyo3(get, set)]
    pub name: PyObject,
    #[pyo3(get, set)]
    pub pid: PyObject,
    /// ホスト：この人に送るメッセージ
    #[pyo3(get, set)]
    pub msgs: PyObject,
    /// 参加者が操縦席で押しているキー
    #[pyo3(get, set)]
    pub input_keys: PyObject,
    /// ジェットパックで飛んでいる
    #[pyo3(get, set)]
    pub flying: bool,
    #[pyo3(get, set)]
    pub thrust: FA,
    #[pyo3(get, set)]
    pub look_fwd: FA,
    #[pyo3(get, set)]
    pub pitch: f64,
    /// 乗っている構築物 (ID, セル)（体重を伝える）
    #[pyo3(get, set)]
    pub stand: PyObject,
    /// MOD のモーションの名前（カメラモードで選ぶ）
    #[pyo3(get, set)]
    pub emote: PyObject,
    #[pyo3(get, set)]
    pub emote_t0: f64,
    /// 体の向き（カメラモードで決める。None ならふだんどおり）
    #[pyo3(get, set)]
    pub face_dir: Option<FA>,
    /// MOD の地面（街の床など）に立っている
    #[pyo3(get, set, name = "_ext_ground")]
    pub ext_ground: bool,
}

impl Player {
    fn radius_(&self, py: Python<'_>) -> R<f64> {
        float_of(&self.defn.bind(py).get_item("radius")?)
    }
    fn height_(&self, py: Python<'_>) -> R<f64> {
        float_of(&self.defn.bind(py).get_item("height")?)
    }
}

#[pymethods]
impl Player {
    #[new]
    fn new(py: Python<'_>, pdef: PyObject, pos: &Bound<'_, PyAny>) -> R<Self> {
        let z = || fa(py, vec![0.0; 3], &[3]);
        Ok(Player {
            defn: pdef,
            pos: fa(py, v3(pos)?.to_vec(), &[3])?,
            vel: z()?,
            frame: -2,
            up: fa(py, vec![0.0, 1.0, 0.0], &[3])?,
            on_ground: false,
            ground_vel: z()?,
            seated: py.None(),
            inventory: PyDict::new_bound(py).into_any().unbind(),
            hotbar: py.None(),
            jetpack: false,
            mass: 80.0,
            in_water: 0.0,
            starter_given: false,
            credits_given: false,
            out_of_fuel: false,
            name: "".into_py(py),
            pid: 0.into_py(py),
            msgs: PyList::empty_bound(py).into_any().unbind(),
            input_keys: PySet::empty_bound(py)?.into_any().unbind(),
            flying: false,
            thrust: z()?,
            look_fwd: fa(py, vec![0.0, 0.0, 1.0], &[3])?,
            pitch: 0.0,
            stand: py.None(),
            emote: py.None(),
            emote_t0: 0.0,
            face_dir: None,
            ext_ground: false,
        })
    }

    #[getter]
    fn radius(&self, py: Python<'_>) -> R<f64> {
        self.radius_(py)
    }

    #[getter]
    fn height(&self, py: Python<'_>) -> R<f64> {
        self.height_(py)
    }

    fn eye(&self, py: Python<'_>) -> R<PyObject> {
        let (p, u) = (v3(self.pos.bind(py))?, v3(self.up.bind(py))?);
        let k = self.height_(py)? - self.radius_(py)? - 0.15;
        Ok(arr(py, add(p, mul(u, k))))
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyDict>> {
        let d = PyDict::new_bound(py);
        d.set_item("position", self.pos.bind(py).call_method0("tolist")?)?;
        d.set_item("velocity", self.vel.bind(py).call_method0("tolist")?)?;
        d.set_item("inventory", self.inventory.bind(py))?;
        let s = self.seated.bind(py);
        if s.is_truthy()? {
            let l = PyList::new_bound(py, [s.get_item(0)?, PyList::new_bound(py, s.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any()]);
            d.set_item("seated", l)?;
        } else {
            d.set_item("seated", py.None())?;
        }
        d.set_item("jetpack", self.jetpack)?;
        d.set_item("def", self.defn.bind(py).get_item("id")?)?;
        d.set_item("starter_given", self.starter_given)?;
        d.set_item("credits_given", self.credits_given)?;
        let hb = self.hotbar.bind(py);
        if hb.is_truthy()? {
            let l = PyList::empty_bound(py);
            for x in hb.iter()? {
                let x = x?;
                if x.is_truthy()? {
                    l.append(PyList::new_bound(py, x.iter()?.collect::<R<Vec<_>>>()?))?;
                } else {
                    l.append(py.None())?;
                }
            }
            d.set_item("hotbar", l)?;
        } else {
            d.set_item("hotbar", py.None())?;
        }
        Ok(d)
    }

    fn load_dict(&mut self, py: Python<'_>, d: &Bound<'_, PyAny>) -> R<()> {
        let np = crate::game::imp(py, "numpy")?;
        let kw = PyDict::new_bound(py);
        kw.set_item("dtype", np.getattr("float64")?)?;
        self.pos = to_fa(&np.getattr("asarray")?.call((d.dget2("position", self.pos.bind(py))?,), Some(&kw))?)?;
        self.vel = to_fa(&np.getattr("asarray")?.call((d.dget2("velocity", vec![0, 0, 0])?,), Some(&kw))?)?;
        let inv = PyDict::new_bound(py);
        inv.update(d.dget2("inventory", PyDict::new_bound(py))?.downcast::<pyo3::types::PyMapping>()?)?;
        self.inventory = inv.into_any().unbind();
        let s = d.dget1("seated")?;
        self.seated = if s.is_truthy()? {
            PyTuple::new_bound(py, [s.get_item(0)?, PyTuple::new_bound(py, s.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any()]).into_any().unbind()
        } else {
            py.None()
        };
        self.jetpack = d.dget2("jetpack", false)?.is_truthy()?;
        self.starter_given = d.dget2("starter_given", false)?.is_truthy()?;
        self.credits_given = d.dget2("credits_given", false)?.is_truthy()?;
        let hb = d.dget1("hotbar")?;
        self.hotbar = if hb.is_truthy()? {
            let l = PyList::empty_bound(py);
            for x in hb.iter()? {
                let x = x?;
                if x.is_truthy()? {
                    l.append(PyTuple::new_bound(py, x.iter()?.collect::<R<Vec<_>>>()?))?;
                } else {
                    l.append(py.None())?;
                }
            }
            l.into_any().unbind()
        } else {
            py.None()
        };
        Ok(())
    }

    /// move: 地表に沿った希望速度（ワールド座標）、thrust_dir: ジェットパック方向
    pub(crate) fn update(slf: &Bound<'_, Self>, world: &Bound<'_, PyAny>, dt: f64, r#move: &Bound<'_, PyAny>, jump: &Bound<'_, PyAny>,
              thrust_dir: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let mv = v3(r#move)?;
        let jump = jump.is_truthy()?;
        let (radius, height, defn) = {
            let me = slf.borrow();
            (me.radius_(py)?, me.height_(py)?, me.defn.clone_ref(py))
        };
        let get = |k: &str| -> R<V3> { v3(&slf.getattr(k)?) };
        let set = |k: &str, v: V3| -> R<()> { slf.setattr(k, arr(py, v)) };
        let mut pos = get("pos")?;
        let mut up = get("up")?;
        let pos_np = |p: V3| -> R<Bound<'_, PyAny>> { Ok(PyArray1::from_vec_bound(py, p.to_vec()).reshape([1, 3])?.into_any()) };
        let pl = World::nearest_planet(wld(&world)?, &IntoPy::<PyObject>::into_py(arr(py, pos), world.py()).into_bound(world.py())).map(|v| IntoPy::<PyObject>::into_py(v, world.py()).into_bound(world.py()))?;
        if !pl.is_none() {
            let tu = v3(&Planet::up(pla(&pl)?, &IntoPy::<PyObject>::into_py(arr(py, pos), pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?)?;
            up = add(up, mul(sub(tu, up), (dt * 5.0).min(1.0)));
            let l = norm(up);
            up = mul(up, 1.0 / l);
            set("up", up)?;
        }
        let seated = slf.try_borrow()?.seated.bind(py).clone();
        if seated.is_truthy()? {
            let c = World::g_constructs(wld(&world)?)?.dget1(seated.get_item(0)?)?;
            let i = if c.is_truthy()? { Construct::g_index(con(&c)?)?.dget1(seated.get_item(1)?)? } else { py.None().into_bound(py) };
            if i.is_none() {
                slf.try_borrow_mut()?.seated = py.None();
            } else {
                let i: usize = i.extract()?;
                let cp: numpy::PyReadonlyArray2<f64> = Construct::g_pos(con(&c)?)?.extract()?;
                let cv: numpy::PyReadonlyArray2<f64> = Construct::g_vel(con(&c)?)?.extract()?;
                let (ap, av) = (cp.as_array(), cv.as_array());
                let mut ctr = [0.0; 3];
                let mut vv = [0.0; 3];
                for k in 0..8 {
                    for a in 0..3 {
                        ctr[a] += ap[[8 * i + k, a]] / 8.0;
                        vv[a] += av[[8 * i + k, a]] / 8.0;
                    }
                }
                let fr = Construct::voxel_frames(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?.get_item(i)?;
                let f1 = v3(&fr.get_item(1)?)?;
                let size = con(&c)?.borrow().size;
                set("pos", add(ctr, mul(f1, size * 0.5 + radius)))?;
                set("vel", vv)?;
                set("up", f1)?;
                return Ok(());
            }
        }
        // ジェットパック（燃料などの決まりは MOD が決める。サバイバルでは手持ちの燃料が要る）
        let jr = World::jetpack(wld(&world)?, &IntoPy::<PyObject>::into_py(slf, world.py()).into_bound(world.py()), IntoPy::<PyObject>::into_py(dt, world.py()).extract::<f64>(world.py())?, &IntoPy::<PyObject>::into_py(thrust_dir, world.py()).into_bound(world.py())).map(|v| IntoPy::<PyObject>::into_py(v, world.py()).into_bound(world.py()))?;
        let flying = jr.get_item(0)?.is_truthy()?;
        let td = v3(&jr.get_item(1)?)?;
        slf.try_borrow_mut()?.flying = flying;
        set("thrust", td)?;
        let g = v3(&World::gravity(wld(&world)?, &IntoPy::<PyObject>::into_py(pos_np(pos)?, world.py()).into_bound(world.py())).map(|v| IntoPy::<PyObject>::into_py(v, world.py()).into_bound(world.py()))?.get_item(0)?)?;
        let mut vel = add(get("vel")?, mul(g, dt));
        // 水中：浮力と抵抗、泳ぐ
        let mut in_water = 0.0;
        let on_ground_prev = slf.try_borrow()?.on_ground;
        if !pl.is_none() && pl.getattr("has_water")?.is_truthy()? {
            let wd = float_of(&pl.call_method1("water_depth", (pos_np(pos)?,))?.get_item(0)?)?;
            let frac = ((wd + radius) / (height * 0.8)).clamp(0.0, 1.0);
            in_water = frac;
            if frac > 0.0 {
                vel = sub(vel, mul(g, dt * frac * 0.97));
                vel = mul(vel, (1.0 - dt * 2.5 * frac).max(0.0));
                if !flying {
                    let vert = dot(td, up);
                    vel = add(vel, mul(up, vert * 9.0 * dt * (frac * 1.5).min(1.0)));
                    if !on_ground_prev {
                        vel = add(vel, mul(mv, 6.0 * dt * frac));
                    }
                }
            }
        }
        slf.try_borrow_mut()?.in_water = in_water;
        if !pl.is_none() && !on_ground_prev && in_water <= 0.0 {
            // 空気の抵抗（構築物と同じ大気：気圧に比例、高さとともに薄くなる）。人の Cd × 面積 ≒ 0.7 m²（終端速度 約 55 m/s）
            let kpa = match pl.getattr("defn")?.dget2("atmosphere_pressure", 101.3) {
                Ok(v) if !v.is_none() => float_of(&v).unwrap_or(101.3),
                _ => 101.3,
            };
            let prad = float_of(&pl.getattr("radius")?)?;
            let alt = (norm(sub(pos, v3(&pl.getattr("center")?)?)) - prad).max(0.0);
            let rho = 1.225 * (kpa / 101.3).max(0.0) * (-alt / (prad * 0.25).max(30.0)).exp();
            let m = slf.try_borrow()?.mass.max(1.0);
            let s = norm(vel);
            let f = (0.5 * rho * 0.7 * s / m * dt).min(0.9);
            vel = mul(vel, 1.0 - f);
        }
        let d = defn.bind(py);
        if flying && norm(td) > 0.0 {
            vel = add(vel, mul(td, float_of(&d.get_item("jetpack_accel")?)? * dt));
        }
        if on_ground_prev {
            let gv = get("ground_vel")?;
            let rel = sub(vel, gv);
            let rel_t = sub(rel, mul(up, dot(rel, up)));
            let want = mul(mv, float_of(&d.get_item("walk_speed")?)?);
            let change = sub(want, rel_t);
            let k = (dt * 12.0).min(1.0);
            vel = add(vel, mul(change, k));
            if jump {
                vel = add(vel, mul(up, float_of(&d.get_item("jump_speed")?)?));
            }
        } else if !flying && norm(mv) > 0.0 {
            vel = add(vel, mul(mv, 2.0 * dt)); // 空中での僅かな制御
        }
        pos = add(pos, mul(vel, dt));
        let mut on_ground = false;
        let mut ground_vel = [0.0; 3];
        slf.try_borrow_mut()?.stand = py.None();
        let r = radius;
        // 惑星
        for p in World::g_planets(wld(&world)?)?.iter()? {
            let p = p?;
            let res = p.call_method1("sdf_grad", (pos_np(pos)?,))?;
            let dd = float_of(&res.get_item(0)?.get_item(0)?)?;
            let n = v3(&res.get_item(1)?.get_item(0)?)?;
            if dd < r {
                pos = add(pos, mul(n, r - dd));
                let vn = dot(vel, n);
                if vn < 0.0 {
                    vel = sub(vel, mul(n, vn));
                }
                if dot(n, up) > 0.5 {
                    on_ground = true;
                    vel = mul(vel, (1.0 - dt * 2.0).max(0.0));
                }
            }
        }
        // 拡張の物体（街の建物など。動いているステーションの上では、足元の建物と一緒に動く）
        let addons = World::g_addons(wld(&world)?).ok();
        if addons.as_ref().map_or(Ok(false), |a| a.is_truthy())? {
            let was_ground = slf.try_borrow()?.ext_ground;
            let mut blocked = false;
            slf.try_borrow_mut()?.ext_ground = false;
            for off in [0.0, height - 2.0 * r] {
                let center = add(pos, mul(up, off));
                for it in World::push_sphere(wld(&world)?, &IntoPy::<PyObject>::into_py(arr(py, center), world.py()).into_bound(world.py()), IntoPy::<PyObject>::into_py(r, world.py()).extract::<f64>(world.py())?).map(|v| IntoPy::<PyObject>::into_py(v, world.py()).into_bound(world.py()))?.iter()? {
                    let it = it?;
                    let push = v3(&it.get_item(0)?)?;
                    let n = v3(&it.get_item(1)?)?;
                    let body = it.get_item(2)?;
                    pos = add(pos, push);
                    let vb = if body.call_method0("is_moving")?.is_truthy()? {
                        v3(&body.call_method1("point_velocity", (arr(py, pos),))?)?
                    } else {
                        [0.0; 3]
                    };
                    let vn = dot(sub(vel, vb), n);
                    if vn < 0.0 {
                        vel = sub(vel, mul(n, vn));
                    }
                    // 段の角に乗ったとき（法線が斜め）も足場とみなす
                    if off == 0.0 && dot(n, up) > 0.3 {
                        on_ground = true;
                        slf.try_borrow_mut()?.ext_ground = true;
                        ground_vel = vb;
                    }
                    if off == 0.0 && dot(n, up).abs() < 0.6 && dot(n, mv) < -0.3 {
                        blocked = true;
                    }
                }
            }
            // 低い段差（STEP_UP まで）は歩くだけで上れる
            if blocked && (was_ground || on_ground || in_water > 0.0) && norm(mv) > 0.0 {
                let m = mul(mv, 1.0 / norm(mv));
                for h in [0.15, 0.3, 0.45, 0.6, 0.72] {
                    let probe = add(add(pos, mul(up, h)), mul(m, 0.12));
                    let mut clear = true;
                    for off in [0.0, height - 2.0 * r] {
                        for it in World::push_sphere(wld(&world)?, &IntoPy::<PyObject>::into_py(arr(py, add(probe, mul(up, off))), world.py()).into_bound(world.py()), IntoPy::<PyObject>::into_py(r * 0.98, world.py()).extract::<f64>(world.py())?).map(|v| IntoPy::<PyObject>::into_py(v, world.py()).into_bound(world.py()))?.iter()? {
                            let n = v3(&it?.get_item(1)?)?;
                            if dot(n, up) < 0.5 || off > 0.0 {
                                clear = false;
                                break;
                            }
                        }
                        if !clear {
                            break;
                        }
                    }
                    if clear {
                        pos = add(add(pos, mul(up, h)), mul(m, 0.08));
                        on_ground = true;
                        let vu = dot(vel, up);
                        if vu < 0.0 {
                            vel = sub(vel, mul(up, vu));
                        }
                        break;
                    }
                }
            }
        }
        // ボクセル（足元と頭の2球）
        let mass = slf.try_borrow()?.mass;
        for off in [0.0, height - 2.0 * r] {
            let center = add(pos, mul(up, off));
            for it in World::voxel_contacts(wld(&world)?, &IntoPy::<PyObject>::into_py(arr(py, center), world.py()).into_bound(world.py()), IntoPy::<PyObject>::into_py(r, world.py()).extract::<f64>(world.py())?).map(|v| IntoPy::<PyObject>::into_py(v, world.py()).into_bound(world.py()))?.iter()? {
                let it = it?;
                let c = it.get_item(0)?;
                let i: usize = it.get_item(1)?.extract()?;
                let push = v3(&it.get_item(2)?)?;
                let n = v3(&it.get_item(3)?)?;
                pos = add(pos, push);
                let cv: numpy::PyReadonlyArray2<f64> = Construct::g_vel(con(&c)?)?.extract()?;
                let av = cv.as_array();
                let mut vv = [0.0; 3];
                for k in 0..8 {
                    for a in 0..3 {
                        vv[a] += av[[8 * i + k, a]] / 8.0;
                    }
                }
                drop(cv);
                let rel = sub(vel, vv);
                let vn = dot(rel, n);
                if vn < 0.0 {
                    vel = sub(vel, mul(n, vn));
                }
                if off == 0.0 && dot(n, up) > 0.5 {
                    on_ground = true;
                    ground_vel = vv;
                    slf.try_borrow_mut()?.stand = (con(&c)?.borrow().id.clone_ref(py), Construct::g_cells(con(&c)?)?.get_item(i)?).into_py(py);
                    // 体重を構築物に伝える
                    let gn = norm(v3(&World::gravity(wld(&world)?, &IntoPy::<PyObject>::into_py(pos_np(center)?, world.py()).into_bound(world.py())).map(|v| IntoPy::<PyObject>::into_py(v, world.py()).into_bound(world.py()))?.get_item(0)?)?);
                    let f = Construct::g_force(con(&c)?)?;
                    let mut fa: numpy::PyReadwriteArray2<f64> = f.extract()?;
                    let mut a = fa.as_array_mut();
                    for k in 0..8 {
                        for ax in 0..3 {
                            a[[8 * i + k, ax]] += -n[ax] * mass * gn / 8.0;
                        }
                    }
                }
            }
        }
        set("pos", pos)?;
        set("vel", vel)?;
        slf.try_borrow_mut()?.on_ground = on_ground;
        set("ground_vel", ground_vel)?;
        Ok(())
    }
}

// ---------------------------------------------------------------- 欄の読み書き（C-01）
macro_rules! player_py_fields {
    ($($g:ident $s:ident $f:ident),* $(,)?) => {
        #[allow(dead_code)]
        impl Player { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
                Ok(slf.try_borrow()?.$f.bind(slf.py()).clone().into_any())
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> R<()> {
                let py = slf.py();
                let t = v.into_py(py).extract(py)?;
                slf.try_borrow_mut()?.$f = t;
                Ok(())
            }
        )* }
    };
}
macro_rules! player_val_fields {
    ($($g:ident $s:ident $f:ident),* $(,)?) => {
        #[allow(dead_code)]
        impl Player { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
                Ok(slf.try_borrow()?.$f.clone().into_py(slf.py()).into_bound(slf.py()))
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> R<()> {
                let py = slf.py();
                let t = v.into_py(py).extract(py)?;
                slf.try_borrow_mut()?.$f = t;
                Ok(())
            }
        )* }
    };
}
player_py_fields!(
    g_defn s_defn defn, g_pos s_pos pos, g_vel s_vel vel, g_up s_up up, g_ground_vel s_ground_vel ground_vel, g_seated s_seated seated, g_inventory s_inventory inventory, g_hotbar s_hotbar hotbar, g_name s_name name, g_pid s_pid pid, g_msgs s_msgs msgs, g_input_keys s_input_keys input_keys, g_thrust s_thrust thrust, g_look_fwd s_look_fwd look_fwd, g_stand s_stand stand, g_emote s_emote emote
);
player_val_fields!(
    g_on_ground s_on_ground on_ground, g_jetpack s_jetpack jetpack, g_mass s_mass mass, g_in_water s_in_water in_water, g_starter_given s_starter_given starter_given, g_credits_given s_credits_given credits_given, g_out_of_fuel s_out_of_fuel out_of_fuel, g_flying s_flying flying, g_pitch s_pitch pitch, g_ext_ground s_ext_ground ext_ground, g_emote_t0 s_emote_t0 emote_t0
);
impl Player {
    pub fn g_face_dir<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        Ok(match &slf.try_borrow()?.face_dir {
            Some(a) => a.bind(py).clone().into_any(),
            None => py.None().into_bound(py),
        })
    }
    pub fn s_face_dir(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> R<()> {
        let py = slf.py();
        let o = v.into_py(py);
        let t = if o.is_none(py) { None } else { Some(to_fa(o.bind(py))?) };
        slf.try_borrow_mut()?.face_dir = t;
        Ok(())
    }
}
