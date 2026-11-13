//! 惑星（main/planet.py の Planet、main/terrain_earth.py の EarthPlanet）。
//! 地形の距離場そのものは lib.rs の PlanetShape。ここは惑星の値・鉱床・色・地球型の池と洞窟の生成。

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::construct::{arr, cross, dot, fv, iarr, norm, opt, sub, v3, v3s, R, V3};
use super::imp;

const SPAWN_DIR: V3 = [0.0, 1.0, 0.0];

fn core(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "alpedia_core")
}
fn np(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "numpy")
}
fn rows3<'py>(py: Python<'py>, v: &[V3]) -> R<Bound<'py, PyAny>> {
    super::construct::a3(py, v)
}
fn shape_of(o: &Bound<'_, PyAny>) -> R<Vec<usize>> {
    np(o.py())?.call_method1("shape", (o,))?.extract()
}
fn ss(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn unit(v: V3) -> V3 {
    let l = norm(v);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// np.cross より速い外積（(N,3) と (N,3) / (3,) の組み合わせ）
#[pyfunction]
pub fn planet_fcross<'py>(py: Python<'py>, a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let (sa, sb) = (shape_of(a)?, shape_of(b)?);
    let (va, vb) = (v3s(a)?, v3s(b)?);
    let n = va.len().max(vb.len());
    let out: Vec<V3> = (0..n).map(|i| cross(va[if va.len() == 1 { 0 } else { i }], vb[if vb.len() == 1 { 0 } else { i }])).collect();
    let sh = if sa.len() >= sb.len() { sa } else { sb };
    arr(py, out.into_iter().flatten().collect(), &sh)
}

fn noise_of<'py>(py: Python<'py>, p: &Bound<'py, PyAny>, seed: i64, octaves: i64) -> R<Bound<'py, PyAny>> {
    let a = np(py)?.call_method1("asarray", (p, "float64"))?;
    let sh = shape_of(&a)?;
    let q = np(py)?.call_method1("ascontiguousarray", (a.call_method1("reshape", (-1, 3))?,))?;
    let out = core(py)?.call_method1("noise", (q, seed, octaves))?;
    out.call_method1("reshape", (PyTuple::new_bound(py, &sh[..sh.len() - 1]),))
}
/// p: (..., 3) → (...) 0..1 の値ノイズ
#[pyfunction]
#[pyo3(signature = (p, seed=0))]
pub fn planet_value_noise<'py>(py: Python<'py>, p: &Bound<'py, PyAny>, seed: i64) -> R<Bound<'py, PyAny>> {
    noise_of(py, p, seed, 0)
}
/// 値ノイズを octaves 段重ねたもの（0..1）
#[pyfunction]
#[pyo3(signature = (p, seed=0, octaves=3))]
pub fn planet_fbm<'py>(py: Python<'py>, p: &Bound<'py, PyAny>, seed: i64, octaves: i64) -> R<Bound<'py, PyAny>> {
    noise_of(py, p, seed, octaves)
}
#[pyfunction]
pub fn planet_smoothstep<'py>(py: Python<'py>, a: f64, b: f64, x: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    if let Ok(v) = x.extract::<f64>() {
        if !x.hasattr("shape")? {
            return Ok(ss(a, b, v).into_py(py).into_bound(py));
        }
    }
    let a_ = np(py)?.call_method1("asarray", (x, "float64"))?;
    let sh = shape_of(&a_)?;
    let v: Vec<f64> = fv(&a_)?.into_iter().map(|t| ss(a, b, t)).collect();
    arr(py, v, &sh)
}

/// 定義の "terrain" に応じた惑星を作る（sphere = 球体、earth = 山・洞窟・水のある地球型）
#[pyfunction]
#[pyo3(signature = (pdef, position, overrides=None))]
pub fn planet_make<'py>(py: Python<'py>, pdef: &Bound<'py, PyAny>, position: &Bound<'py, PyAny>, overrides: Option<&Bound<'py, PyAny>>)
    -> R<Bound<'py, PyAny>> {
    let d = merged(py, pdef, overrides)?;
    let ov: PyObject = overrides.map(|o| o.clone().unbind()).unwrap_or_else(|| py.None());
    if d.call_method1("get", ("terrain",))?.eq("earth")? {
        return imp(py, "main.terrain_earth")?.getattr("EarthPlanet")?.call1((pdef, position, ov));
    }
    imp(py, "main.planet")?.getattr("Planet")?.call1((pdef, position, ov))
}

fn merged<'py>(py: Python<'py>, pdef: &Bound<'py, PyAny>, overrides: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.update(pdef.downcast::<PyDict>()?.as_mapping())?;
    if let Some(o) = overrides {
        if o.is_truthy()? {
            d.update(o.downcast::<PyDict>()?.as_mapping())?;
        }
    }
    Ok(d)
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Planet {
    #[pyo3(get, set)]
    pub defn: PyObject,
    #[pyo3(get, set)]
    pub id: PyObject,
    #[pyo3(get, set)]
    pub name: PyObject,
    #[pyo3(get, set)]
    pub center: PyObject,
    #[pyo3(get, set)]
    pub radius: f64,
    #[pyo3(get, set)]
    pub gm: f64,
    #[pyo3(get, set)]
    pub amp: f64,
    #[pyo3(get, set)]
    pub freq: f64,
    #[pyo3(get, set)]
    pub seed: i64,
    #[pyo3(get, set)]
    pub craters: PyObject,
    #[pyo3(get, set)]
    pub version: i64,
    #[pyo3(get, set)]
    pub bound: f64,
    #[pyo3(get, set)]
    pub lipschitz: f64,
    nat: Option<PyObject>,
    nat_ncr: i64,
}

impl Planet {
    fn base(py: Python<'_>, pdef: &Bound<'_, PyAny>, position: &Bound<'_, PyAny>, overrides: Option<&Bound<'_, PyAny>>, lip: f64) -> R<Self> {
        let d = merged(py, pdef, overrides)?;
        let f = |k: &str| -> R<f64> { d.get_item(k)?.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(k.to_string()))?.extract() };
        let id = d.get_item("id")?.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("id"))?;
        let radius = f("radius")?;
        let amp = f("noise_amp")?;
        let lipschitz = match d.get_item("lipschitz")? { Some(v) => v.extract()?, None => lip };
        Ok(Planet {
            name: d.call_method1("get", ("name", &id))?.unbind(),
            id: id.unbind(),
            center: super::construct::a1(py, v3(position)?).unbind(),
            radius, gm: f("surface_gravity")? * radius.powf(2.0), amp, freq: f("noise_freq")?,
            seed: d.get_item("seed")?.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("seed"))?.call_method0("__int__")?.extract()?,
            craters: PyList::empty_bound(py).into_any().unbind(),
            version: 0, bound: radius + amp * 1.5 + 2.0, lipschitz,
            defn: d.into_any().unbind(), nat: None, nat_ncr: -1,
        })
    }
    fn ctr(slf: &Bound<'_, Self>) -> R<V3> {
        let py = slf.py();
        let c = slf.borrow().center.clone_ref(py);
        v3(c.bind(py))
    }
    fn dirs_of(p: &[V3], c: V3) -> (Vec<V3>, Vec<f64>) {
        let mut d = Vec::with_capacity(p.len());
        let mut r = Vec::with_capacity(p.len());
        for q in p {
            let v = sub(*q, c);
            let l = norm(v);
            let m = l.max(1e-9);
            d.push([v[0] / m, v[1] / m, v[2] / m]);
            r.push(l);
        }
        (d, r)
    }
    fn sample_v(slf: &Bound<'_, Self>, dirs: &[V3]) -> R<Vec<[f64; 2]>> {
        let py = slf.py();
        let s = Self::_nat(slf)?.call_method1("sample", (rows3(py, dirs)?,))?;
        Ok(fv(&s)?.chunks_exact(2).map(|x| [x[0], x[1]]).collect())
    }
    fn defn<'py>(slf: &Bound<'py, Self>) -> Bound<'py, PyAny> {
        slf.borrow().defn.clone_ref(slf.py()).into_bound(slf.py())
    }
    /// 鉱床の重み：[(定義, 重み)]
    fn deposit_weights<'py>(slf: &Bound<'py, Self>, dirs: &[V3], h: &[f64]) -> R<Vec<(Bound<'py, PyAny>, Vec<f64>)>> {
        let py = slf.py();
        let seed = slf.borrow().seed;
        let mut out = vec![];
        let deps = Self::defn(slf).call_method1("get", ("deposits", PyList::empty_bound(py)))?;
        for (k, dep) in deps.iter()?.enumerate() {
            let dep = dep?;
            let g = |key: &str, dflt: f64| -> R<f64> { dep.call_method1("get", (key, dflt))?.extract() };
            let th = g("threshold", 0.6)?;
            let fr = g("freq", 40.0)?;
            let add_k = 13.0 * k as f64;
            let pts: Vec<V3> = dirs.iter().map(|d| [d[0] * fr + 17.0 + add_k, d[1] * fr + 17.0 + add_k, d[2] * fr + 17.0 + add_k]).collect();
            let ds: i64 = dep.call_method1("get", ("seed", 50 + k as i64))?.call_method0("__int__")?.extract()?;
            let n = fv(&core(py)?.call_method1("noise", (rows3(py, &pts)?, seed + ds, 3))?)?;
            let mut w: Vec<f64> = n.iter().map(|x| ss(th, th + 0.035, *x)).collect();
            let mh = dep.call_method1("get", ("min_height",))?;
            if !mh.is_none() {
                let mh: f64 = mh.extract()?;
                for (wi, hi) in w.iter_mut().zip(h) {
                    *wi *= ss(mh - 2.0, mh + 2.0, *hi);
                }
            }
            out.push((dep, w));
        }
        Ok(out)
    }
    fn tint(slf: &Bound<'_, Self>, dirs: &[V3], h: &[f64], mut col: Vec<V3>) -> R<Vec<V3>> {
        for (dep, w) in Self::deposit_weights(slf, dirs, h)? {
            if dep.contains("color")? {
                let c = v3(&dep.get_item("color")?)?;
                let st: f64 = dep.call_method1("get", ("strength", 0.85))?.extract()?;
                for (q, cl) in col.iter_mut().enumerate() {
                    let wq = w[q] * st;
                    for a in 0..3 {
                        cl[a] = cl[a] * (1.0 - wq) + c[a] * wq;
                    }
                }
            }
        }
        Ok(col)
    }
    fn base_col(slf: &Bound<'_, Self>, n: usize) -> R<Vec<V3>> {
        let c = v3(&Self::defn(slf).get_item("color")?)?;
        Ok(vec![c; n])
    }
}

#[pymethods]
impl Planet {
    #[new]
    #[pyo3(signature = (pdef, position, overrides=None))]
    fn new(py: Python<'_>, pdef: &Bound<'_, PyAny>, position: &Bound<'_, PyAny>, overrides: Option<&Bound<'_, PyAny>>) -> R<Self> {
        Self::base(py, pdef, position, overrides, 0.8)
    }
    #[classattr]
    const TILE: i64 = 32;
    #[classattr]
    const RES: f64 = 0.25;
    #[getter]
    fn has_water(&self) -> bool {
        false
    }
    #[getter]
    fn has_caves(&self) -> bool {
        false
    }

    /// Rust 側に渡す地形の値：(radius, amp, freq, seed, earth の dict か None)
    fn _native_params(slf: &Bound<'_, Self>) -> (f64, f64, f64, i64, Option<PyObject>) {
        let b = slf.borrow();
        (b.radius, b.amp, b.freq, b.seed, None)
    }
    /// Rust の地形。掘削跡が増えていたら渡し直す
    fn _nat<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let has = slf.borrow().nat.is_some();
        if !has {
            let p = slf.call_method0("_native_params")?;
            let c = Self::ctr(slf)?;
            let s = core(py)?.getattr("PlanetShape")?.call1((p.get_item(0)?, p.get_item(1)?, p.get_item(2)?,
                p.get_item(3)?.call_method0("__int__")?, c.to_vec(), p.get_item(4)?))?;
            let mut b = slf.borrow_mut();
            b.nat = Some(s.unbind());
            b.nat_ncr = -1;
        }
        let s = slf.borrow().nat.as_ref().unwrap().clone_ref(py).into_bound(py);
        let cr = slf.borrow().craters.clone_ref(py).into_bound(py);
        let n = cr.len()? as i64;
        if slf.borrow().nat_ncr != n {
            let a = np(py)?.call_method1("asarray", (&cr, "float64"))?.call_method1("reshape", (-1, 4))?;
            s.call_method1("set_craters", (np(py)?.call_method1("ascontiguousarray", (a,))?,))?;
            slf.borrow_mut().nat_ncr = n;
        }
        Ok(s)
    }
    /// 単位方向 (N,3) における地表半径（ノイズを直接計算）
    fn surface_radius_exact<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        Self::_nat(slf)?.call_method1("surface_radius_exact", (rows3(slf.py(), &v3s(dirs)?)?,))
    }
    /// キューブマップ状のタイルにキャッシュした値（地表半径・水面半径）を双線形補間
    fn _sample<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        Self::_nat(slf)?.call_method1("sample", (rows3(slf.py(), &v3s(dirs)?)?,))
    }
    fn surface_radius<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let v: Vec<f64> = Self::sample_v(slf, &v3s(dirs)?)?.iter().map(|x| x[0]).collect();
        let n = v.len();
        arr(slf.py(), v, &[n])
    }
    /// 水面の半径（水が無い所は 0）
    fn water_radius<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let v: Vec<f64> = Self::sample_v(slf, &v3s(dirs)?)?.iter().map(|x| x[1]).collect();
        let n = v.len();
        arr(slf.py(), v, &[n])
    }
    /// タイルに保存する値 (N,2)：地表半径・水面半径（補間しない値）
    fn _tile_values<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        Self::_nat(slf)?.call_method1("tile_values", (rows3(slf.py(), &v3s(dirs)?)?,))
    }
    /// 遠景用：半径と色
    fn far_samples<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let r = Self::surface_radius_exact(slf, dirs)?;
        let d = v3s(dirs)?;
        let rad = slf.borrow().radius;
        let h: Vec<f64> = fv(&r)?.iter().map(|x| x - rad).collect();
        let col = Self::tint(slf, &d, &h, Self::base_col(slf, d.len())?)?;
        Ok(PyTuple::new_bound(py, [r, rows3(py, &col)?]))
    }
    /// 近景の地表の色（頂点ごと）
    fn surface_color<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>, r: &Bound<'py, PyAny>, normals: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let _ = normals;
        let d = v3s(dirs)?;
        let rad = slf.borrow().radius;
        let h: Vec<f64> = fv(r)?.iter().map(|x| x - rad).collect();
        let col = Self::tint(slf, &d, &h, Self::base_col(slf, d.len())?)?;
        rows3(slf.py(), &col)
    }
    fn _deposit_weights<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>, h: &Bound<'py, PyAny>) -> R<Bound<'py, PyList>> {
        let py = slf.py();
        let out = PyList::empty_bound(py);
        for (dep, w) in Self::deposit_weights(slf, &v3s(dirs)?, &fv(h)?)? {
            let n = w.len();
            out.append((dep, arr(py, w, &[n])?))?;
        }
        Ok(out)
    }
    fn deposit_tint<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>, h: &Bound<'py, PyAny>, col: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let c = Self::tint(slf, &v3s(dirs)?, &fv(h)?, v3s(col)?)?;
        rows3(slf.py(), &c)
    }
    /// 地中（掘った穴・洞窟の壁）の色を鉱床に合わせて変える。既定では何もしない
    fn underground_tint<'py>(slf: &Bound<'py, Self>, local_pts: &Bound<'py, PyAny>, col: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let _ = (slf, local_pts);
        Ok(col.clone())
    }
    /// その場所を掘ったときに出る資源の ID
    fn resource_at<'py>(slf: &Bound<'py, Self>, world_point: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let q = sub(v3(world_point)?, Self::ctr(slf)?);
        let r = norm(q).max(1e-9);
        let d = [q[0] / r, q[1] / r, q[2] / r];
        let rad = slf.borrow().radius;
        let h = Self::sample_v(slf, &[d])?[0][0] - rad;
        for (dep, w) in Self::deposit_weights(slf, &[d], &[h])? {
            if w[0] > 0.5 {
                return dep.get_item("resource");
            }
        }
        let _ = py;
        Self::defn(slf).call_method1("get", ("dig_resource", "ore"))
    }
    /// 洞窟（くり抜き）までの距離（惑星ローカル座標）。洞窟の無い惑星は None
    #[pyo3(signature = (q, grad=false))]
    fn cave_dist<'py>(slf: &Bound<'py, Self>, q: &Bound<'py, PyAny>, grad: bool) -> R<Bound<'py, PyAny>> {
        let _ = (q, grad);
        Ok(slf.py().None().into_bound(slf.py()))
    }
    /// 世界座標 (N,3) の点の水深（水面からの深さ。水の外は負）
    fn water_depth<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let (d, r) = Self::dirs_of(&v3s(p)?, Self::ctr(slf)?);
        let s = Self::sample_v(slf, &d)?;
        let v: Vec<f64> = s.iter().zip(&r).map(|(s, r)| if s[1] > s[0] { s[1] - r } else { -1e9 }).collect();
        let n = v.len();
        arr(slf.py(), v, &[n])
    }
    /// 世界座標 (N,3) → 符号付き距離（負=地中）
    fn sdf<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        Self::_nat(slf)?.call_method1("sdf", (rows3(slf.py(), &v3s(p)?)?,))
    }
    /// (地形の距離, 掘削跡込みの地形の距離, 洞窟の距離 or None)。描画用
    fn sdf_parts<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let c = Self::ctr(slf)?;
        let pv = v3s(p)?;
        let (d, r) = Self::dirs_of(&pv, c);
        let s = Self::sample_v(slf, &d)?;
        let h: Vec<f64> = r.iter().zip(&s).map(|(r, s)| r - s[0]).collect();
        let cr = slf.borrow().craters.clone_ref(py).into_bound(py);
        let mut dd = h.clone();
        if cr.is_truthy()? {
            let cv: Vec<[f64; 4]> = fv(&cr)?.chunks_exact(4).map(|x| [x[0], x[1], x[2], x[3]]).collect();
            for (q, pp) in pv.iter().enumerate() {
                let ql = sub(*pp, c);
                let mut m = f64::INFINITY;
                for k in &cv {
                    m = m.min(norm(sub(ql, [k[0], k[1], k[2]])) - k[3]);
                }
                dd[q] = h[q].max(-m);
            }
        }
        let n = h.len();
        let cv: Bound<'py, PyAny> = if slf.getattr("has_caves")?.is_truthy()? {
            let ql: Vec<V3> = pv.iter().map(|x| sub(*x, c)).collect();
            slf.call_method1("cave_dist", (rows3(py, &ql)?,))?
        } else { py.None().into_bound(py) };
        Ok(PyTuple::new_bound(py, [arr(py, h, &[n])?, arr(py, dd, &[n])?, cv]))
    }
    /// 距離と勾配（法線）を同時に求める
    #[pyo3(signature = (p, eps=0.05))]
    fn sdf_grad<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>, eps: f64) -> R<Bound<'py, PyAny>> {
        Self::_nat(slf)?.call_method1("sdf_grad", (rows3(slf.py(), &v3s(p)?)?, eps))
    }
    fn normal<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        slf.call_method1("sdf_grad", (p,))?.get_item(1)
    }
    fn up<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let v = sub(v3(p)?, Self::ctr(slf)?);
        let l = norm(v).max(1e-9);
        Ok(super::construct::a1(slf.py(), [v[0] / l, v[1] / l, v[2] / l]))
    }
    fn gravity<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let c = Self::ctr(slf)?;
        let (gm, rad) = { let b = slf.borrow(); (b.gm, b.radius) };
        let out: Vec<V3> = v3s(p)?.iter().map(|x| {
            let q = sub(c, *x);
            let r2 = dot(q, q).max((rad * 0.5).powf(2.0));
            let k = gm / (r2 * r2.sqrt());
            [q[0] * k, q[1] * k, q[2] * k]
        }).collect();
        rows3(slf.py(), &out)
    }
    fn dig(slf: &Bound<'_, Self>, world_point: &Bound<'_, PyAny>, radius: f64) -> R<()> {
        let l = sub(v3(world_point)?, Self::ctr(slf)?);
        let py = slf.py();
        slf.borrow().craters.bind(py).call_method1("append", (vec![l[0], l[1], l[2], radius],))?;
        slf.borrow_mut().version += 1;
        Ok(())
    }
    /// 球面行進で地表との交点 (t, 点) か None
    fn raycast<'py>(slf: &Bound<'py, Self>, origin: &Bound<'py, PyAny>, direction: &Bound<'py, PyAny>, max_dist: f64) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let lip = slf.getattr("lipschitz")?.extract::<f64>()?;
        let r = Self::_nat(slf)?.call_method1("raycast", (v3(origin)?.to_vec(), v3(direction)?.to_vec(), max_dist, lip))?;
        if r.is_none() {
            return Ok(r);
        }
        Ok(PyTuple::new_bound(py, [r.get_item(0)?, np(py)?.call_method1("asarray", (r.get_item(1)?,))?]).into_any())
    }
    /// 水面との交点（目が水中にあるときは None）
    fn water_raycast<'py>(slf: &Bound<'py, Self>, origin: &Bound<'py, PyAny>, direction: &Bound<'py, PyAny>, max_dist: f64) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        if !slf.getattr("has_water")?.is_truthy()? {
            return Ok(py.None().into_bound(py));
        }
        let r = Self::_nat(slf)?.call_method1("water_raycast", (v3(origin)?.to_vec(), v3(direction)?.to_vec(), max_dist))?;
        if r.is_none() {
            return Ok(r);
        }
        Ok(PyTuple::new_bound(py, [r.get_item(0)?, np(py)?.call_method1("asarray", (r.get_item(1)?,))?]).into_any())
    }
    fn to_dict<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let d = PyDict::new_bound(py);
        d.set_item("def", Self::defn(slf).get_item("id")?)?;
        d.set_item("position", Self::ctr(slf)?.to_vec())?;
        d.set_item("craters", slf.borrow().craters.clone_ref(py))?;
        Ok(d)
    }
}

// ---------------------------------------------------------------- 地球型

fn tangent(d: V3) -> (V3, V3) {
    let rf = if d[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let t1 = unit(cross(d, rf));
    let t2 = cross(d, t1);
    (t1, t2)
}

#[pyfunction]
pub fn earth_tangent_frame<'py>(py: Python<'py>, d: &Bound<'py, PyAny>) -> R<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let sh = shape_of(d)?;
    let (a, b): (Vec<V3>, Vec<V3>) = v3s(d)?.into_iter().map(tangent).unzip();
    Ok((arr(py, a.into_iter().flatten().collect(), &sh)?, arr(py, b.into_iter().flatten().collect(), &sh)?))
}

fn fib(n: usize, seed: f64) -> Vec<V3> {
    let g = std::f64::consts::PI * (1.0 + 5f64.powf(0.5));
    (0..n).map(|i| {
        let k = i as f64 + 0.5;
        let phi = (1.0 - 2.0 * k / n as f64).acos();
        let th = g * k + seed * 0.37;
        [th.cos() * phi.sin(), phi.cos(), th.sin() * phi.sin()]
    }).collect()
}
#[pyfunction]
pub fn earth_fib_sphere<'py>(py: Python<'py>, n: usize, seed: f64) -> R<Bound<'py, PyAny>> {
    rows3(py, &fib(n, seed))
}

/// 点とカプセル（端の半径 ra→rb）の距離と最近点
fn seg_dist(p: V3, a: V3, b: V3, ra: f64, rb: f64) -> (f64, V3, V3, f64) {
    let ab = sub(b, a);
    let t = (dot(sub(p, a), ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
    let c = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
    let v = sub(p, c);
    let l = norm(v);
    (l - (ra + (rb - ra) * t), c, v, l)
}
#[pyfunction]
pub fn earth_seg_dist<'py>(py: Python<'py>, p: &Bound<'py, PyAny>, a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>, ra: &Bound<'py, PyAny>,
                           rb: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
    let (p, a, b, ra, rb) = (v3s(p)?, v3s(a)?, v3s(b)?, fv(ra)?, fv(rb)?);
    let n = p.len();
    let (mut d, mut c, mut v, mut l) = (vec![], vec![], vec![], vec![]);
    for i in 0..n {
        let r = seg_dist(p[i], a[i], b[i], ra[i], rb[i]);
        d.push(r.0);
        c.push(r.1);
        v.push(r.2);
        l.push(r.3);
    }
    Ok(PyTuple::new_bound(py, [arr(py, d, &[n])?, rows3(py, &c)?, rows3(py, &v)?, arr(py, l, &[n])?]))
}

fn key3(g: [i64; 3]) -> i64 {
    let g = [g[0] + 4096, g[1] + 4096, g[2] + 4096];
    (g[0] * 8192 + g[1]) * 8192 + g[2]
}

/// 地球型の地形の値（池・洞窟まで）
struct Earth {
    sea: f64,
    mountain_h: f64,
    hill_h: f64,
    flat_r: f64,
    spawn_h: f64,
    n_caves: i64,
    n_ponds: i64,
    ponds: Vec<(V3, f64, f64, f64)>,
    cave_a: Vec<V3>,
    cave_b: Vec<V3>,
    cave_ra: Vec<f64>,
    cave_rb: Vec<f64>,
    cave_id: Vec<i64>,
    ck: Vec<i64>,
    cstart: Vec<i64>,
    cend: Vec<i64>,
    ccap: Vec<i64>,
}

const CELL: f64 = 8.0;
const MARGIN: f64 = 3.0;

fn earth_params<'py>(py: Python<'py>, e: &Earth, empty: bool) -> R<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("sea", e.sea)?;
    d.set_item("mountain_h", e.mountain_h)?;
    d.set_item("hill_h", e.hill_h)?;
    d.set_item("flat_r", e.flat_r)?;
    d.set_item("spawn_h", e.spawn_h)?;
    d.set_item("cell", CELL)?;
    let f1 = |v: Vec<f64>| -> R<Bound<'py, PyAny>> { let n = v.len(); arr(py, v, &[n]) };
    let i1 = |v: &Vec<i64>| -> R<Bound<'py, PyAny>> { iarr(py, if empty { vec![] } else { v.clone() }, &[if empty { 0 } else { v.len() }]) };
    let pd: Vec<V3> = if empty { vec![] } else { e.ponds.iter().map(|p| p.0).collect() };
    d.set_item("ponds_dir", rows3(py, &pd)?)?;
    let pf = |k: usize| -> Vec<f64> { if empty { vec![] } else { e.ponds.iter().map(|p| [p.1, p.2, p.3][k]).collect() } };
    d.set_item("ponds_rad", f1(pf(0))?)?;
    d.set_item("ponds_depth", f1(pf(1))?)?;
    d.set_item("ponds_level", f1(pf(2))?)?;
    d.set_item("cave_a", rows3(py, if empty { &[] } else { &e.cave_a })?)?;
    d.set_item("cave_b", rows3(py, if empty { &[] } else { &e.cave_b })?)?;
    d.set_item("cave_ra", f1(if empty { vec![] } else { e.cave_ra.clone() })?)?;
    d.set_item("cave_rb", f1(if empty { vec![] } else { e.cave_rb.clone() })?)?;
    d.set_item("ck", i1(&e.ck)?)?;
    d.set_item("cstart", i1(&e.cstart)?)?;
    d.set_item("cend", i1(&e.cend)?)?;
    d.set_item("ccap", i1(&e.ccap)?)?;
    Ok(d)
}

struct Bare<'py> {
    shape: Bound<'py, PyAny>,
}
impl<'py> Bare<'py> {
    fn h0(&self, dirs: &[V3]) -> R<Vec<f64>> {
        fv(&self.shape.call_method1("height0", (rows3(self.shape.py(), dirs)?,))?)
    }
}

fn rng<'py>(py: Python<'py>, seed: i64) -> R<Bound<'py, PyAny>> {
    np(py)?.getattr("random")?.call_method1("default_rng", (seed,))
}
fn shuffled(rs: &Bound<'_, PyAny>, v: Vec<V3>) -> R<Vec<V3>> {
    let py = rs.py();
    let a = rows3(py, &v)?;
    rs.call_method1("shuffle", (&a,))?;
    v3s(&a)
}
fn ang(dirs: &[V3], rf: V3, radius: f64) -> Vec<f64> {
    dirs.iter().map(|d| dot(*d, rf).clamp(-1.0, 1.0).acos() * radius).collect()
}
fn dir_at(radius: f64, ox: f64, oz: f64) -> V3 {
    unit([ox, radius, oz])
}

fn make_ponds(py: Python<'_>, e: &mut Earth, bare: &Bare<'_>, radius: f64, seed: i64) -> R<()> {
    let rs = rng(py, seed * 7919 + 13)?;
    let cand = fib(9000, seed as f64);
    let h = bare.h0(&cand)?;
    let a_sp = ang(&cand, SPAWN_DIR, radius);
    let ok: Vec<V3> = (0..cand.len()).filter(|&i| h[i] > e.sea + 1.5 && h[i] < e.mountain_h * 0.6 && a_sp[i] > e.flat_r * 2.2)
        .map(|i| cand[i]).collect();
    let mut cand = shuffled(&rs, ok)?;
    cand.truncate((e.n_ponds * 3).max(0) as usize);
    let mut p: Vec<(V3, f64, f64)> = vec![(dir_at(radius, -20.0, 16.0), 6.5, 1.6)];     // 出現地点のそばの池
    for c in cand {
        let r1: f64 = rs.call_method1("uniform", (4, 12))?.extract()?;
        let r2: f64 = rs.call_method1("uniform", (1.0, 3.0))?.extract()?;
        p.push((c, r1, r2));
    }
    let step = 2.0 * std::f64::consts::PI / 12.0;
    for (c, rad, depth) in p {
        let (t1, t2) = tangent(c);
        let mut ring = vec![];
        for f in [1.0, 1.4, 1.9] {
            let s = rad * f / radius;
            for i in 0..12 {
                let a = i as f64 * step;
                let (ca, sa) = (a.cos(), a.sin());
                let off = [(ca * t1[0] + sa * t2[0]) * s, (ca * t1[1] + sa * t2[1]) * s, (ca * t1[2] + sa * t2[2]) * s];
                ring.push(unit([c[0] + off[0], c[1] + off[1], c[2] + off[2]]));
            }
        }
        let hr = bare.h0(&ring)?;
        let mn = hr.iter().copied().fold(f64::INFINITY, f64::min);
        let mx = hr.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let level = mn - 0.35;
        if !e.ponds.is_empty() && mx - mn > 2.2 {            // 斜面すぎる所には作らない（出現地点のそばは必ず作る）
            continue;
        }
        e.ponds.push((c, rad, depth, level));
        if e.ponds.len() as i64 > e.n_ponds {
            break;
        }
    }
    Ok(())
}

fn make_caves(py: Python<'_>, e: &mut Earth, bare: &Bare<'_>, radius: f64, seed: i64) -> R<()> {
    let rs = rng(py, seed * 104729 + 7)?;
    let cand = fib(4000, (seed + 1) as f64);
    let h = bare.h0(&cand)?;
    let a_sp = ang(&cand, SPAWN_DIR, radius);
    let ok: Vec<V3> = (0..cand.len()).filter(|&i| h[i] > e.sea + 2.0 && a_sp[i] > e.flat_r * 1.6).map(|i| cand[i]).collect();
    let cand = shuffled(&rs, ok)?;
    let mut starts = vec![dir_at(radius, 30.0, -48.0)];         // 出現地点のそばの洞窟
    starts.extend(cand.into_iter().take(e.n_caves.max(0) as usize));
    let k = starts.len();
    let fvec = |o: Bound<'_, PyAny>| -> R<Vec<f64>> { fv(&o) };
    let tau = 2.0 * std::f64::consts::PI;
    let mut yaw = fvec(rs.call_method1("uniform", (0, tau, k))?)?;
    yaw[0] = (-1.0f64).atan2(0.3);          // 出現地点から離れる向き
    let mut steps: Vec<i64> = super::construct::iv(&rs.call_method1("integers", (18, 42, k))?)?;
    steps[0] = 34;
    let mut depth_goal = fvec(rs.call_method1("uniform", (7, 16, k))?)?;
    depth_goal[0] = 9.0;
    let mut rad = fvec(rs.call_method1("uniform", (1.8, 2.6, k))?)?;
    rad[0] = 2.3;
    let h0 = bare.h0(&starts)?;
    let mut pos: Vec<V3> = starts.iter().zip(&h0).map(|(s, h)| { let m = radius + h + 0.6; [s[0] * m, s[1] * m, s[2] * m] }).collect();
    let mut r_prev = rad.clone();
    let mut alive = vec![true; k];
    let smax = steps.iter().copied().max().unwrap_or(0);
    for s in 0..smax {
        for i in 0..k {
            alive[i] &= s < steps[i];
        }
        if !alive.iter().any(|x| *x) {
            break;
        }
        let up: Vec<V3> = pos.iter().map(|p| unit(*p)).collect();
        let tf: Vec<(V3, V3)> = up.iter().map(|u| tangent(*u)).collect();
        let dy = fvec(rs.call_method1("normal", (0, 0.35, k))?)?;
        for i in 0..k {
            yaw[i] += dy[i];
        }
        let hs: Vec<f64> = bare.h0(&up)?.into_iter().map(|h| radius + h).collect();
        let rn = fvec(rs.call_method1("normal", (0, 0.12, k))?)?;
        let mut new = pos.clone();
        let mut r_new = vec![0.0; k];
        for i in 0..k {
            let depth = hs[i] - norm(pos[i]);
            let mut pitch = if s < 4 { -0.55 } else { ((depth - depth_goal[i]) * 0.06).clamp(-0.5, 0.3) };
            if s >= 4 && depth < r_prev[i] + 2.5 {
                pitch = -0.45;
            }
            let (t1, t2) = tf[i];
            let (cy, sy) = (yaw[i].cos(), yaw[i].sin());
            let fwd = [cy * t1[0] + sy * t2[0], cy * t1[1] + sy * t2[1], cy * t1[2] + sy * t2[2]];
            let (cp, sp) = (pitch.cos(), pitch.sin());
            let dv = [fwd[0] * cp + up[i][0] * sp, fwd[1] * cp + up[i][1] * sp, fwd[2] * cp + up[i][2] * sp];
            r_new[i] = (r_prev[i] * rn[i].exp()).clamp(1.5, 3.4);
            let st = 2.2 + r_new[i] * 0.3;
            new[i] = [pos[i][0] + dv[0] * st, pos[i][1] + dv[1] * st, pos[i][2] + dv[2] * st];
        }
        for i in 0..k {
            if alive[i] {
                e.cave_a.push(pos[i]);
                e.cave_b.push(new[i]);
                e.cave_ra.push(r_prev[i]);
                e.cave_rb.push(r_new[i]);
                e.cave_id.push(i as i64);
            }
        }
        for i in 0..k {
            if alive[i] {
                pos[i] = new[i];
                r_prev[i] = r_new[i];
            }
        }
    }
    // 終点は広間に
    for i in 0..k {
        let r: f64 = rs.call_method0("random")?.extract()?;
        if r < 0.5 || i == 0 {
            let rr: f64 = rs.call_method1("uniform", (3.2, 4.8))?.extract()?;
            e.cave_a.push(pos[i]);
            e.cave_b.push([pos[i][0] + 0.01, pos[i][1] + 0.01, pos[i][2] + 0.01]);
            e.cave_ra.push(rr);
            e.cave_rb.push(rr);
            e.cave_id.push(i as i64);
        }
    }
    index_caves(e);
    Ok(())
}

fn index_caves(e: &mut Earth) {
    let mut keys = vec![];
    let mut caps = vec![];
    for k in 0..e.cave_a.len() {
        let m = e.cave_ra[k].max(e.cave_rb[k]) + MARGIN;
        let (a, b) = (e.cave_a[k], e.cave_b[k]);
        let i0: Vec<i64> = (0..3).map(|j| ((a[j].min(b[j]) - m) / CELL).floor() as i64).collect();
        let i1: Vec<i64> = (0..3).map(|j| ((a[j].max(b[j]) + m) / CELL).floor() as i64).collect();
        for x in i0[0]..=i1[0] {
            for y in i0[1]..=i1[1] {
                for z in i0[2]..=i1[2] {
                    keys.push(key3([x, y, z]));
                    caps.push(k as i64);
                }
            }
        }
    }
    let mut o: Vec<usize> = (0..keys.len()).collect();
    o.sort_by_key(|&i| keys[i]);
    let ks: Vec<i64> = o.iter().map(|&i| keys[i]).collect();
    e.ccap = o.iter().map(|&i| caps[i]).collect();
    e.ck.clear();
    e.cstart.clear();
    for (i, &kk) in ks.iter().enumerate() {
        if i == 0 || ks[i - 1] != kk {
            e.ck.push(kk);
            e.cstart.push(i as i64);
        }
    }
    e.cend = e.cstart[1..].to_vec();
    e.cend.push(ks.len() as i64);
}

#[pyclass(module = "alpedia_core", extends = Planet, subclass, dict)]
pub struct EarthPlanet {
    #[pyo3(get, set)]
    sea: f64,
    #[pyo3(get, set)]
    mountain_h: f64,
    #[pyo3(get, set)]
    hill_h: f64,
    #[pyo3(get, set)]
    flat_r: f64,
    #[pyo3(get, set)]
    spawn_h: f64,
    #[pyo3(get, set)]
    n_caves: i64,
    #[pyo3(get, set)]
    n_ponds: i64,
    #[pyo3(get, set)]
    ponds_dir: PyObject,
    #[pyo3(get, set)]
    ponds_rad: PyObject,
    #[pyo3(get, set)]
    ponds_depth: PyObject,
    #[pyo3(get, set)]
    ponds_level: PyObject,
    #[pyo3(get, set)]
    cave_a: PyObject,
    #[pyo3(get, set)]
    cave_b: PyObject,
    #[pyo3(get, set)]
    cave_ra: PyObject,
    #[pyo3(get, set)]
    cave_rb: PyObject,
    #[pyo3(get, set)]
    cave_id: PyObject,
    #[pyo3(get, set, name = "_ck")]
    ck: PyObject,
    #[pyo3(get, set, name = "_cstart")]
    cstart: PyObject,
    #[pyo3(get, set, name = "_cend")]
    cend: PyObject,
    #[pyo3(get, set, name = "_ccap")]
    ccap: PyObject,
    #[pyo3(get, set, name = "_bare")]
    bare: PyObject,
}

fn sup<'py>(s: &Bound<'py, EarthPlanet>) -> Bound<'py, Planet> {
    s.as_any().downcast::<Planet>().expect("EarthPlanet は Planet").clone()
}

impl EarthPlanet {
    fn earth(slf: &Bound<'_, Self>) -> R<Earth> {
        let py = slf.py();
        let b = slf.borrow();
        let g = |o: &PyObject| fv(o.bind(py));
        let gi = |o: &PyObject| super::construct::iv(o.bind(py));
        let pd = v3s(b.ponds_dir.bind(py))?;
        let (pr, pdp, pl) = (g(&b.ponds_rad)?, g(&b.ponds_depth)?, g(&b.ponds_level)?);
        Ok(Earth {
            sea: b.sea, mountain_h: b.mountain_h, hill_h: b.hill_h, flat_r: b.flat_r, spawn_h: b.spawn_h, n_caves: b.n_caves,
            n_ponds: b.n_ponds,
            ponds: (0..pd.len()).map(|i| (pd[i], pr[i], pdp[i], pl[i])).collect(),
            cave_a: v3s(b.cave_a.bind(py))?, cave_b: v3s(b.cave_b.bind(py))?, cave_ra: g(&b.cave_ra)?, cave_rb: g(&b.cave_rb)?,
            cave_id: gi(&b.cave_id)?, ck: gi(&b.ck)?, cstart: gi(&b.cstart)?, cend: gi(&b.cend)?, ccap: gi(&b.ccap)?,
        })
    }
    fn sample_v(slf: &Bound<'_, Self>, dirs: &[V3]) -> R<Vec<[f64; 2]>> {
        Planet::sample_v(&sup(slf), dirs)
    }
    fn pal(&self, h: &[f64], slope: &[f64], water_h: Option<&[f64]>) -> Vec<V3> {
        let grass = [0.36, 0.55, 0.26];
        let dry = [0.52, 0.55, 0.3];
        let sand = [0.78, 0.72, 0.5];
        let rock = [0.47, 0.44, 0.41];
        let snow = [0.93, 0.94, 0.96];
        let mud = [0.42, 0.36, 0.27];
        let mh = self.mountain_h;
        (0..h.len()).map(|i| {
            let (hh, sl) = (h[i], slope[i]);
            let mix = |col: V3, other: V3, t: f64| -> V3 { [col[0] * (1.0 - t) + other[0] * t, col[1] * (1.0 - t) + other[1] * t, col[2] * (1.0 - t) + other[2] * t] };
            let t = ss(8.0, 30.0, hh);
            let mut col = [grass[0] * (1.0 - t) + dry[0] * t, grass[1] * (1.0 - t) + dry[1] * t, grass[2] * (1.0 - t) + dry[2] * t];
            col = mix(col, sand, ss(1.6, 0.6, hh - self.sea));
            col = mix(col, rock, ss(0.45, 0.65, sl));
            col = mix(col, rock, ss(0.45 * mh, 0.6 * mh, hh));
            let sn = ss(mh * 0.82, mh * 0.95, hh) * (1.0 - ss(0.55, 0.75, sl));
            col = mix(col, snow, sn);
            if let Some(w) = water_h {
                col = mix(col, mud, ss(-0.2, 0.4, w[i] - hh));
            }
            col
        }).collect()
    }
    fn cave_d(slf: &Bound<'_, Self>, q: &[V3]) -> R<Vec<f64>> {
        let py = slf.py();
        let r = Planet::_nat(&sup(slf))?.call_method1("cave_dist", (rows3(py, q)?,))?;
        fv(&r.get_item(0)?)
    }
}

#[pymethods]
impl EarthPlanet {
    #[new]
    #[pyo3(signature = (pdef, position, overrides=None))]
    fn new(py: Python<'_>, pdef: &Bound<'_, PyAny>, position: &Bound<'_, PyAny>, overrides: Option<&Bound<'_, PyAny>>)
        -> R<PyClassInitializer<Self>> {
        let mut base = Planet::base(py, pdef, position, overrides, 0.45)?;
        let d = base.defn.bind(py).clone();
        let g = |k: &str, dflt: f64| -> R<f64> { d.call_method1("get", (k, dflt))?.extract() };
        let gi = |k: &str, dflt: i64| -> R<i64> { d.call_method1("get", (k, dflt))?.call_method0("__int__")?.extract() };
        let mut e = Earth {
            sea: g("sea_level", 0.0)?, mountain_h: g("mountain_height", 45.0)?, hill_h: g("hill_height", 4.0)?,
            flat_r: g("spawn_flat_radius", 38.0)?, spawn_h: g("spawn_height", 3.0)?, n_caves: gi("caves", 220)?, n_ponds: gi("ponds", 260)?,
            ponds: vec![], cave_a: vec![], cave_b: vec![], cave_ra: vec![], cave_rb: vec![], cave_id: vec![],
            ck: vec![], cstart: vec![], cend: vec![], ccap: vec![],
        };
        base.bound = base.radius + e.mountain_h * 1.4 + e.hill_h * 2.0 + 20.0;
        base.amp = e.mountain_h;
        // 池・洞窟を作る間の高さは、それらの無い地形で計算する
        let c = v3(base.center.bind(py))?;
        let shape = core(py)?.getattr("PlanetShape")?.call1((base.radius, base.amp, base.freq, base.seed, c.to_vec(), earth_params(py, &e, true)?))?;
        let bare = Bare { shape };
        make_ponds(py, &mut e, &bare, base.radius, base.seed)?;
        make_caves(py, &mut e, &bare, base.radius, base.seed)?;
        let f1 = |v: Vec<f64>| -> R<PyObject> { let n = v.len(); Ok(arr(py, v, &[n])?.unbind()) };
        let i1 = |v: &Vec<i64>| -> R<PyObject> { Ok(iarr(py, v.clone(), &[v.len()])?.unbind()) };
        let pd: Vec<V3> = e.ponds.iter().map(|p| p.0).collect();
        let me = EarthPlanet {
            sea: e.sea, mountain_h: e.mountain_h, hill_h: e.hill_h, flat_r: e.flat_r, spawn_h: e.spawn_h, n_caves: e.n_caves, n_ponds: e.n_ponds,
            ponds_dir: rows3(py, &pd)?.unbind(),
            ponds_rad: f1(e.ponds.iter().map(|p| p.1).collect())?,
            ponds_depth: f1(e.ponds.iter().map(|p| p.2).collect())?,
            ponds_level: f1(e.ponds.iter().map(|p| p.3).collect())?,
            cave_a: rows3(py, &e.cave_a)?.unbind(), cave_b: rows3(py, &e.cave_b)?.unbind(),
            cave_ra: f1(e.cave_ra.clone())?, cave_rb: f1(e.cave_rb.clone())?, cave_id: i1(&e.cave_id)?,
            ck: i1(&e.ck)?, cstart: i1(&e.cstart)?, cend: i1(&e.cend)?, ccap: i1(&e.ccap)?, bare: py.None(),
        };
        Ok(PyClassInitializer::from(base).add_subclass(me))
    }
    #[classattr]
    const CELL: f64 = CELL;
    #[classattr]
    const MARGIN: f64 = MARGIN;
    #[getter]
    fn has_water(&self) -> bool {
        true
    }
    #[getter]
    fn has_caves(&self) -> bool {
        true
    }

    fn _ang<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>, rf: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let v = ang(&v3s(dirs)?, v3(rf)?, sup(slf).borrow().radius);
        let n = v.len();
        arr(slf.py(), v, &[n])
    }
    /// 池を除いた地表の高さ（半径からの m）
    fn height0<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        Planet::_nat(&sup(slf))?.call_method1("height0", (rows3(slf.py(), &v3s(dirs)?)?,))
    }
    /// 出現地点から地表に沿って (x, z) m 離れた方向
    fn _dir_at<'py>(slf: &Bound<'py, Self>, ox: f64, oz: f64) -> Bound<'py, PyAny> {
        super::construct::a1(slf.py(), dir_at(sup(slf).borrow().radius, ox, oz))
    }
    fn _index_caves(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let mut e = Self::earth(slf)?;
        index_caves(&mut e);
        let i1 = |v: &Vec<i64>| -> R<PyObject> { Ok(iarr(py, v.clone(), &[v.len()])?.unbind()) };
        let mut b = slf.borrow_mut();
        b.ck = i1(&e.ck)?;
        b.cstart = i1(&e.cstart)?;
        b.cend = i1(&e.cend)?;
        b.ccap = i1(&e.ccap)?;
        b.bare = py.None();
        Ok(())
    }
    #[staticmethod]
    fn _key<'py>(py: Python<'py>, g: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let sh = shape_of(g)?;
        let v = super::construct::iv(g)?;
        let out: Vec<i64> = v.chunks_exact(3).map(|x| key3([x[0], x[1], x[2]])).collect();
        iarr(py, out, &sh[..sh.len() - 1])
    }
    /// Rust の地形（PlanetShape）に渡す地球型の値。empty：池・洞窟の無い地形
    #[pyo3(signature = (empty=false))]
    fn _earth_params<'py>(slf: &Bound<'py, Self>, empty: bool) -> R<Bound<'py, PyDict>> {
        earth_params(slf.py(), &Self::earth(slf)?, empty)
    }
    fn _native_params<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let (r, a, f, s) = { let p = &sup(slf).borrow(); (p.radius, p.amp, p.freq, p.seed) };
        Ok(PyTuple::new_bound(py, [r.into_py(py), a.into_py(py), f.into_py(py), s.into_py(py), Self::_earth_params(slf, false)?.into_any().unbind()]))
    }
    #[pyo3(signature = (q, grad=false))]
    fn cave_dist<'py>(slf: &Bound<'py, Self>, q: &Bound<'py, PyAny>, grad: bool) -> R<Bound<'py, PyAny>> {
        let r = Planet::_nat(&sup(slf))?.call_method1("cave_dist", (rows3(slf.py(), &v3s(q)?)?,))?;
        if grad { Ok(r) } else { r.get_item(0) }
    }
    /// center_local の周り radius 以内にあるカプセルの番号
    fn caves_near<'py>(slf: &Bound<'py, Self>, center_local: &Bound<'py, PyAny>, radius: f64) -> R<Bound<'py, PyAny>> {
        let c = v3(center_local)?;
        let e = Self::earth(slf)?;
        let out: Vec<i64> = (0..e.cave_a.len()).filter(|&k| seg_dist(c, e.cave_a[k], e.cave_b[k], e.cave_ra[k], e.cave_rb[k]).0 < radius)
            .map(|k| k as i64).collect();
        let n = out.len();
        iarr(slf.py(), out, &[n])
    }
    fn water_depth<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let (d, r) = Planet::dirs_of(&v3s(p)?, Planet::ctr(&sup(slf))?);
        let s = Self::sample_v(slf, &d)?;
        let v: Vec<f64> = s.iter().zip(&r).map(|(s, r)| if s[1] > s[0] { s[1] - r } else { -1e9 }).collect();
        let n = v.len();
        arr(slf.py(), v, &[n])
    }
    /// 高さ（m）と傾き（0=平ら〜1=垂直）から地表の色
    #[pyo3(signature = (h, slope, water_h=None))]
    fn palette<'py>(slf: &Bound<'py, Self>, h: &Bound<'py, PyAny>, slope: &Bound<'py, PyAny>, water_h: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
        let w = match water_h { Some(w) if !w.is_none() => Some(fv(w)?), _ => None };
        let col = slf.borrow().pal(&fv(h)?, &fv(slope)?, w.as_deref());
        rows3(slf.py(), &col)
    }
    fn surface_color<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>, r: &Bound<'py, PyAny>, normals: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let d = v3s(dirs)?;
        let n = v3s(normals)?;
        let rad = sup(slf).borrow().radius;
        let h: Vec<f64> = fv(r)?.iter().map(|x| x - rad).collect();
        let slope: Vec<f64> = d.iter().zip(&n).map(|(d, n)| ((1.0 - dot(*n, *d).clamp(0.0, 1.0)) * 3.3).clamp(0.0, 1.0)).collect();
        let w: Vec<f64> = Self::sample_v(slf, &d)?.iter().map(|s| s[1] - rad).collect();
        let col = slf.borrow().pal(&h, &slope, Some(&w));
        let col = Planet::tint(&sup(slf), &d, &h, col)?;
        rows3(slf.py(), &col)
    }
    fn _cave_res<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        Planet::defn(&sup(slf)).call_method1("get", ("cave_resource",))
    }
    fn resource_at<'py>(slf: &Bound<'py, Self>, world_point: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let res = Self::_cave_res(slf)?;
        if res.is_truthy()? {
            let q = sub(v3(world_point)?, Planet::ctr(&sup(slf))?);
            let lim: f64 = Planet::defn(&sup(slf)).call_method1("get", ("cave_resource_dist", 3.0))?.extract()?;
            if Self::cave_d(slf, &[q])?[0] < lim {
                return Ok(res);
            }
        }
        Planet::resource_at(&sup(slf), world_point)
    }
    /// 洞窟の壁のまわりに、鉱石の粒をまだらに見せる（掘るとその鉱石が出る範囲の目印）
    fn underground_tint<'py>(slf: &Bound<'py, Self>, local_pts: &Bound<'py, PyAny>, col: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let res = Self::_cave_res(slf)?;
        let defn = Planet::defn(&sup(slf));
        let c = defn.call_method1("get", ("cave_resource_color",))?;
        let pts = v3s(local_pts)?;
        if !res.is_truthy()? || c.is_none() || pts.is_empty() {
            return Ok(col.clone());
        }
        let c = v3(&c)?;
        let d = Self::cave_d(slf, &pts)?;
        let lim: f64 = defn.call_method1("get", ("cave_resource_dist", 3.0))?.extract()?;
        let seed = &sup(slf).borrow().seed;
        let q: Vec<V3> = pts.iter().map(|p| [p[0] * 1.1 + 5.0, p[1] * 1.1 + 5.0, p[2] * 1.1 + 5.0]).collect();
        let nz = fv(&core(py)?.call_method1("noise", (rows3(py, &q)?, seed + 77, 2))?)?;
        let cl = v3s(col)?;
        let out: Vec<V3> = (0..pts.len()).map(|i| {
            let near = 1.0 - ss(lim - 0.5, lim, d[i]);
            let spots = ss(0.55, 0.6, nz[i]);
            let w = near * (0.25 + 0.65 * spots);
            [cl[i][0] * (1.0 - w) + c[0] * w, cl[i][1] * (1.0 - w) + c[1] * w, cl[i][2] * (1.0 - w) + c[2] * w]
        }).collect();
        rows3(py, &out)
    }
    fn far_samples<'py>(slf: &Bound<'py, Self>, dirs: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let d = v3s(dirs)?;
        let rad = sup(slf).borrow().radius;
        let h = fv(&Self::height0(slf, dirs)?)?;          // 遠景では池は省略（海だけ）
        let sea = slf.borrow().sea;
        // 傾きは近くの高さとの差から
        let e = 3.0 / rad;
        let d2: Vec<V3> = d.iter().map(|x| { let (t1, _) = tangent(*x); unit([x[0] + t1[0] * e, x[1] + t1[1] * e, x[2] + t1[2] * e]) }).collect();
        let h2 = fv(&Self::height0(slf, &rows3(py, &d2)?)?)?;
        let slope: Vec<f64> = h2.iter().zip(&h).map(|(a, b)| ((a - b).abs() / 3.0).clamp(0.0, 1.0)).collect();
        let col = slf.borrow().pal(&h, &slope, None);
        let mut col = Planet::tint(&sup(slf), &d, &h, col)?;
        let mut r = vec![0.0; d.len()];
        for i in 0..d.len() {
            let wet = sea > h[i];
            if wet {
                col[i] = [0.16, 0.33, 0.55];
            }
            r[i] = rad + if wet { sea } else { h[i] };
        }
        let n = r.len();
        Ok(PyTuple::new_bound(py, [arr(py, r, &[n])?, rows3(py, &col)?]))
    }
}

#[allow(dead_code)]
fn _unused(o: &Bound<'_, PyAny>) -> R<()> {
    let _ = opt(o, "x")?;
    Ok(())
}
