//! プレイヤー（描画とは独立した物理・所持品）。main/player.py の中身。
//!
//! 値（位置・速度・所持品など）は Python から読み書きされるので、Python の値のまま持つ（属性）。
//! 毎ティックの動き（重力・水・ジェットパック・歩く・地面や建物・ブロックとの当たり）は Rust で計算する。

use numpy::{PyArray1, PyArrayMethods};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyTuple};

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

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Player {
    #[pyo3(get, set)]
    defn: PyObject,
    /// 足元の球の中心
    #[pyo3(get, set)]
    pos: PyObject,
    #[pyo3(get, set)]
    vel: PyObject,
    #[pyo3(get, set)]
    up: PyObject,
    #[pyo3(get, set)]
    on_ground: PyObject,
    #[pyo3(get, set)]
    ground_vel: PyObject,
    /// (construct_id, cell)
    #[pyo3(get, set)]
    seated: PyObject,
    #[pyo3(get, set)]
    inventory: PyObject,
    /// アイテムバー（9枠）: [["material"|"device", id] or None]
    #[pyo3(get, set)]
    hotbar: PyObject,
    #[pyo3(get, set)]
    jetpack: PyObject,
    #[pyo3(get, set)]
    mass: PyObject,
    #[pyo3(get, set)]
    in_water: PyObject,
    /// サバイバルの最初の資材を受け取ったか
    #[pyo3(get, set)]
    starter_given: PyObject,
    /// 最初のクレジットを受け取ったか
    #[pyo3(get, set)]
    credits_given: PyObject,
    #[pyo3(get, set)]
    out_of_fuel: PyObject,
    // マルチプレイ・描画用
    #[pyo3(get, set)]
    name: PyObject,
    #[pyo3(get, set)]
    pid: PyObject,
    /// ホスト：この人に送るメッセージ
    #[pyo3(get, set)]
    msgs: PyObject,
    /// 参加者が操縦席で押しているキー
    #[pyo3(get, set)]
    input_keys: PyObject,
    /// ジェットパックで飛んでいる
    #[pyo3(get, set)]
    flying: PyObject,
    #[pyo3(get, set)]
    thrust: PyObject,
    #[pyo3(get, set)]
    look_fwd: PyObject,
    #[pyo3(get, set)]
    pitch: PyObject,
    /// 乗っている構築物 (ID, セル)（体重を伝える）
    #[pyo3(get, set)]
    stand: PyObject,
    /// MOD のモーションの名前（カメラモードで選ぶ）
    #[pyo3(get, set)]
    emote: PyObject,
    #[pyo3(get, set)]
    emote_t0: PyObject,
    /// 体の向き（カメラモードで決める。None ならふだんどおり）
    #[pyo3(get, set)]
    face_dir: PyObject,
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
        let z = || arr(py, [0.0; 3]);
        Ok(Player {
            defn: pdef,
            pos: arr(py, v3(pos)?),
            vel: z(),
            up: arr(py, [0.0, 1.0, 0.0]),
            on_ground: false.into_py(py),
            ground_vel: z(),
            seated: py.None(),
            inventory: PyDict::new_bound(py).into_any().unbind(),
            hotbar: py.None(),
            jetpack: false.into_py(py),
            mass: 80.0.into_py(py),
            in_water: 0.0.into_py(py),
            starter_given: false.into_py(py),
            credits_given: false.into_py(py),
            out_of_fuel: false.into_py(py),
            name: "".into_py(py),
            pid: 0.into_py(py),
            msgs: PyList::empty_bound(py).into_any().unbind(),
            input_keys: PySet::empty_bound(py)?.into_any().unbind(),
            flying: false.into_py(py),
            thrust: z(),
            look_fwd: arr(py, [0.0, 0.0, 1.0]),
            pitch: 0.0.into_py(py),
            stand: py.None(),
            emote: py.None(),
            emote_t0: 0.0.into_py(py),
            face_dir: py.None(),
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
        d.set_item("jetpack", self.jetpack.bind(py))?;
        d.set_item("def", self.defn.bind(py).get_item("id")?)?;
        d.set_item("starter_given", self.starter_given.bind(py))?;
        d.set_item("credits_given", self.credits_given.bind(py))?;
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
        self.pos = np.getattr("asarray")?.call((d.call_method1("get", ("position", self.pos.bind(py)))?,), Some(&kw))?.unbind();
        self.vel = np.getattr("asarray")?.call((d.call_method1("get", ("velocity", vec![0, 0, 0]))?,), Some(&kw))?.unbind();
        let inv = PyDict::new_bound(py);
        inv.update(d.call_method1("get", ("inventory", PyDict::new_bound(py)))?.downcast::<pyo3::types::PyMapping>()?)?;
        self.inventory = inv.into_any().unbind();
        let s = d.call_method1("get", ("seated",))?;
        self.seated = if s.is_truthy()? {
            PyTuple::new_bound(py, [s.get_item(0)?, PyTuple::new_bound(py, s.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any()]).into_any().unbind()
        } else {
            py.None()
        };
        self.jetpack = d.call_method1("get", ("jetpack", false))?.is_truthy()?.into_py(py);
        self.starter_given = d.call_method1("get", ("starter_given", false))?.is_truthy()?.into_py(py);
        self.credits_given = d.call_method1("get", ("credits_given", false))?.is_truthy()?.into_py(py);
        let hb = d.call_method1("get", ("hotbar",))?;
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
    fn update(slf: &Bound<'_, Self>, world: &Bound<'_, PyAny>, dt: f64, r#move: &Bound<'_, PyAny>, jump: &Bound<'_, PyAny>,
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
        let pl = world.call_method1("nearest_planet", (arr(py, pos),))?;
        if !pl.is_none() {
            let tu = v3(&pl.call_method1("up", (arr(py, pos),))?)?;
            up = add(up, mul(sub(tu, up), (dt * 5.0).min(1.0)));
            let l = norm(up);
            up = mul(up, 1.0 / l);
            set("up", up)?;
        }
        let seated = slf.getattr("seated")?;
        if seated.is_truthy()? {
            let c = world.getattr("constructs")?.call_method1("get", (seated.get_item(0)?,))?;
            let i = if c.is_truthy()? { c.getattr("index")?.call_method1("get", (seated.get_item(1)?,))? } else { py.None().into_bound(py) };
            if i.is_none() {
                slf.setattr("seated", py.None())?;
            } else {
                let i: usize = i.extract()?;
                let cp: numpy::PyReadonlyArray2<f64> = c.getattr("pos")?.extract()?;
                let cv: numpy::PyReadonlyArray2<f64> = c.getattr("vel")?.extract()?;
                let (ap, av) = (cp.as_array(), cv.as_array());
                let mut ctr = [0.0; 3];
                let mut vv = [0.0; 3];
                for k in 0..8 {
                    for a in 0..3 {
                        ctr[a] += ap[[8 * i + k, a]] / 8.0;
                        vv[a] += av[[8 * i + k, a]] / 8.0;
                    }
                }
                let fr = c.call_method0("voxel_frames")?.get_item(i)?;
                let f1 = v3(&fr.get_item(1)?)?;
                let size = float_of(&c.getattr("size")?)?;
                set("pos", add(ctr, mul(f1, size * 0.5 + radius)))?;
                set("vel", vv)?;
                set("up", f1)?;
                return Ok(());
            }
        }
        // ジェットパック（燃料などの決まりは MOD が決める。サバイバルでは手持ちの燃料が要る）
        let jr = world.call_method1("jetpack", (slf, dt, thrust_dir))?;
        let flying = jr.get_item(0)?.is_truthy()?;
        let td = v3(&jr.get_item(1)?)?;
        slf.setattr("flying", flying)?;
        set("thrust", td)?;
        let g = v3(&world.call_method1("gravity", (pos_np(pos)?,))?.get_item(0)?)?;
        let mut vel = add(get("vel")?, mul(g, dt));
        // 水中：浮力と抵抗、泳ぐ
        let mut in_water = 0.0;
        let on_ground_prev = slf.getattr("on_ground")?.is_truthy()?;
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
        slf.setattr("in_water", in_water)?;
        if !pl.is_none() && !on_ground_prev && in_water <= 0.0 {
            // 空気の抵抗（構築物と同じ大気：気圧に比例、高さとともに薄くなる）。人の Cd × 面積 ≒ 0.7 m²（終端速度 約 55 m/s）
            let kpa = match pl.getattr("defn")?.call_method1("get", ("atmosphere_pressure", 101.3)) {
                Ok(v) if !v.is_none() => float_of(&v).unwrap_or(101.3),
                _ => 101.3,
            };
            let prad = float_of(&pl.getattr("radius")?)?;
            let alt = (norm(sub(pos, v3(&pl.getattr("center")?)?)) - prad).max(0.0);
            let rho = 1.225 * (kpa / 101.3).max(0.0) * (-alt / (prad * 0.25).max(30.0)).exp();
            let m = float_of(&slf.getattr("mass")?).unwrap_or(80.0).max(1.0);
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
        slf.setattr("stand", py.None())?;
        let r = radius;
        // 惑星
        for p in world.getattr("planets")?.iter()? {
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
        let addons = world.getattr("addons").ok();
        if addons.as_ref().map_or(Ok(false), |a| a.is_truthy())? {
            let was_ground = slf.getattr("_ext_ground").map(|v| v.is_truthy().unwrap_or(false)).unwrap_or(false);
            let mut blocked = false;
            slf.setattr("_ext_ground", false)?;
            for off in [0.0, height - 2.0 * r] {
                let center = add(pos, mul(up, off));
                for it in world.call_method1("push_sphere", (arr(py, center), r))?.iter()? {
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
                        slf.setattr("_ext_ground", true)?;
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
                        for it in world.call_method1("push_sphere", (arr(py, add(probe, mul(up, off))), r * 0.98))?.iter()? {
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
        let mass = float_of(&slf.getattr("mass")?)?;
        for off in [0.0, height - 2.0 * r] {
            let center = add(pos, mul(up, off));
            for it in world.call_method1("voxel_contacts", (arr(py, center), r))?.iter()? {
                let it = it?;
                let c = it.get_item(0)?;
                let i: usize = it.get_item(1)?.extract()?;
                let push = v3(&it.get_item(2)?)?;
                let n = v3(&it.get_item(3)?)?;
                pos = add(pos, push);
                let cv: numpy::PyReadonlyArray2<f64> = c.getattr("vel")?.extract()?;
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
                    slf.setattr("stand", (c.getattr("id")?, c.getattr("cells")?.get_item(i)?))?;
                    // 体重を構築物に伝える
                    let gn = norm(v3(&world.call_method1("gravity", (pos_np(center)?,))?.get_item(0)?)?);
                    let f = c.getattr("force")?;
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
        slf.setattr("on_ground", on_ground)?;
        set("ground_vel", ground_vel)?;
        Ok(())
    }
}
