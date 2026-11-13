//! 景色の箱（ボクセルではない、軽い建物・崖・街）と、惑星の表面の地図。main/scenery.py の中身。
//!
//! - 地図 Map：惑星の表面の座標 (x, z) [m]。原点の向き up0 と、その点での前 fwd0。
//! - Scenery：箱の集まり（1か所）。世界座標の箱 C (中心)・A (行が軸)・E (半分の大きさ)。
//! - SceneryHost（Python の混ぜ込み用クラス）の当たり判定・視線・押し出しの中身も、ここの関数。

use numpy::{PyArray1, PyArray2, PyArrayMethods, PyReadonlyArray2, PyReadonlyArray3};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

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
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(a: V3) -> V3 {
    let n = dot(a, a).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}
fn axpy(a: V3, k: f64, b: V3) -> V3 {
    [a[0] + k * b[0], a[1] + k * b[1], a[2] + k * b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// v の向きの単位ベクトル（numpy の配列）
#[pyfunction]
pub fn scenery_unit(py: Python<'_>, v: &Bound<'_, PyAny>) -> R<PyObject> {
    Ok(arr(py, unit(v3(v)?)))
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Map {
    #[pyo3(get, set)]
    pl: PyObject,
    /// ストーリーの不時着の跡（惑星の中心から）(n,4)
    #[pyo3(get, set)]
    craters: PyObject,
    #[pyo3(get, set)]
    up0: PyObject,
    #[pyo3(get, set)]
    fwd0: PyObject,
    #[pyo3(get, set)]
    right0: PyObject,
}

impl Map {
    fn vecs(&self, py: Python<'_>) -> R<(V3, V3, V3)> {
        Ok((v3(self.up0.bind(py))?, v3(self.fwd0.bind(py))?, v3(self.right0.bind(py))?))
    }
    fn radius(&self, py: Python<'_>) -> R<f64> {
        self.pl.bind(py).getattr("radius")?.extract()
    }
    fn center(&self, py: Python<'_>) -> R<V3> {
        v3(&self.pl.bind(py).getattr("center")?)
    }
    fn dir_(&self, py: Python<'_>, x: f64, z: f64) -> R<V3> {
        let (u, f, r) = self.vecs(py)?;
        let rad = self.radius(py)?;
        Ok(unit([u[0] * rad + r[0] * x + f[0] * z, u[1] * rad + r[1] * x + f[1] * z, u[2] * rad + r[2] * x + f[2] * z]))
    }
    fn radius_at_(&self, py: Python<'_>, dirs: &[V3]) -> R<Vec<f64>> {
        let d = PyArray1::from_vec_bound(py, dirs.iter().flatten().copied().collect()).reshape([dirs.len(), 3])?;
        let r: Vec<f64> = self.pl.bind(py).call_method1("surface_radius", (d,))?.call_method0("tolist")?.extract()?;
        let cr: PyReadonlyArray2<f64> = self.craters.bind(py).extract()?;
        let ca = cr.as_array();
        let mut r = r;
        for k in 0..ca.nrows() {
            let c = [ca[[k, 0]], ca[[k, 1]], ca[[k, 2]]];
            let cr2 = ca[[k, 3]] * ca[[k, 3]];
            for (i, dd) in dirs.iter().enumerate() {
                let dc = dot(*dd, c);
                let disc = dc * dc - dot(c, c) + cr2;
                let s = disc.max(0.0).sqrt();
                let (lo, hi) = (dc - s, dc + s);
                if disc > 0.0 && r[i] > lo && r[i] <= hi {
                    r[i] = lo;
                }
            }
        }
        Ok(r)
    }
    fn ground_(&self, py: Python<'_>, x: f64, z: f64, lift: f64) -> R<(V3, V3)> {
        let d = self.dir_(py, x, z)?;
        let r = self.radius_at_(py, &[d])?[0];
        Ok((axpy(self.center(py)?, r + lift, d), d))
    }
    fn frame_(&self, py: Python<'_>, x: f64, z: f64, yaw: f64) -> R<[V3; 3]> {
        let up = self.dir_(py, x, z)?;
        let (_, f0, _) = self.vecs(py)?;
        let f = unit(axpy(f0, -dot(f0, up), up));
        let r = cross(up, f);
        let a = yaw.to_radians();
        let f2 = [f[0] * a.cos() - r[0] * a.sin(), f[1] * a.cos() - r[1] * a.sin(), f[2] * a.cos() - r[2] * a.sin()];
        let r2 = cross(up, f2);
        Ok([r2, up, f2])
    }
    fn height_(&self, py: Python<'_>, p: &[V3], up: V3, base: V3) -> R<Vec<f64>> {
        let c = self.center(py)?;
        let dirs: Vec<V3> = p.iter().map(|q| unit(sub(*q, c))).collect();
        let r = self.radius_at_(py, &dirs)?;
        Ok(dirs.iter().zip(r.iter()).map(|(d, rr)| dot(sub(axpy(c, *rr, *d), base), up)).collect())
    }
}

fn frame_np<'py>(py: Python<'py>, f: [V3; 3]) -> R<Bound<'py, PyArray2<f64>>> {
    // 列が 右・上・前
    let mut v = Vec::with_capacity(9);
    for row in 0..3 {
        for col in 0..3 {
            v.push(f[col][row]);
        }
    }
    PyArray1::from_vec_bound(py, v).reshape([3, 3])
}

fn rows3(a: &Bound<'_, PyAny>) -> R<Vec<V3>> {
    let np = crate::game::imp(a.py(), "numpy")?;
    let x: PyReadonlyArray2<f64> = np.getattr("asarray")?.call1((a, "float64"))?.call_method1("reshape", (-1, 3))?.extract()?;
    Ok(x.as_array().rows().into_iter().map(|r| [r[0], r[1], r[2]]).collect())
}

#[pymethods]
impl Map {
    #[new]
    #[pyo3(signature = (planet, up0, fwd0, craters=None))]
    fn new(py: Python<'_>, planet: PyObject, up0: &Bound<'_, PyAny>, fwd0: &Bound<'_, PyAny>, craters: Option<&Bound<'_, PyAny>>) -> R<Self> {
        let np = crate::game::imp(py, "numpy")?;
        let cr = match craters {
            Some(c) => np.getattr("asarray")?.call1((c, "float64"))?.call_method1("reshape", (-1, 4))?,
            None => np.getattr("zeros")?.call1(((0, 4),))?,
        };
        let u = unit(v3(up0)?);
        let f0 = v3(fwd0)?;
        let f = unit(axpy(f0, -dot(f0, u), u));
        let r = cross(u, f);
        Ok(Map { pl: planet, craters: cr.unbind(), up0: arr(py, u), fwd0: arr(py, f), right0: arr(py, r) })
    }

    fn dir(&self, py: Python<'_>, x: f64, z: f64) -> R<PyObject> {
        Ok(arr(py, self.dir_(py, x, z)?))
    }

    /// 方向 (N,3) の地表の半径（元の地形から、不時着の跡を削ったもの。プレイヤーが掘った所は含まない）
    fn radius_at<'py>(&self, py: Python<'py>, dirs: &Bound<'py, PyAny>) -> R<Bound<'py, PyArray1<f64>>> {
        Ok(PyArray1::from_vec_bound(py, self.radius_at_(py, &rows3(dirs)?)?))
    }

    /// 地表の点と、その点の上方向
    #[pyo3(signature = (x, z, lift=0.0))]
    fn ground(&self, py: Python<'_>, x: f64, z: f64, lift: f64) -> R<(PyObject, PyObject)> {
        let (p, d) = self.ground_(py, x, z, lift)?;
        Ok((arr(py, p), arr(py, d)))
    }

    /// (x, z) に立てた向き（列が 右・上・前 のワールド方向）。yaw は左回り（度）
    #[pyo3(signature = (x, z, yaw=0.0))]
    fn frame<'py>(&self, py: Python<'py>, x: f64, z: f64, yaw: f64) -> R<Bound<'py, PyArray2<f64>>> {
        frame_np(py, self.frame_(py, x, z, yaw)?)
    }

    /// 点 P (N,3) の地表の、base から up 方向の高さ
    fn height<'py>(&self, py: Python<'py>, p: &Bound<'py, PyAny>, up: &Bound<'py, PyAny>, base: &Bound<'py, PyAny>) -> R<Bound<'py, PyArray1<f64>>> {
        Ok(PyArray1::from_vec_bound(py, self.height_(py, &rows3(p)?, v3(up)?, v3(base)?)?))
    }
}

fn vec_or_none(py: Python<'_>, c: Option<&Bound<'_, PyAny>>) -> R<PyObject> {
    Ok(match c {
        Some(c) if !c.is_none() => crate::game::imp(py, "numpy")?.getattr("array")?.call1((c, "float64"))?.unbind(),
        _ => py.None(),
    })
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Scenery {
    #[pyo3(get, set)]
    id: PyObject,
    #[pyo3(get, set)]
    map: PyObject,
    #[pyo3(get, set)]
    parts: Py<PyList>,
    #[pyo3(get, set)]
    points: Py<PyDict>,
}

const CONC: V3 = [0.62, 0.62, 0.60];

#[pymethods]
impl Scenery {
    #[new]
    fn new(py: Python<'_>, sid: PyObject, map_: PyObject) -> Self {
        Scenery { id: sid, map: map_, parts: PyList::empty_bound(py).unbind(), points: PyDict::new_bound(py).unbind() }
    }

    /// 地図の (x, z) に、幅 w・高さ h・奥行き d の箱
    #[pyo3(signature = (x, z, w, h, d, y0=0.0, yaw=0.0, color=None, glow=None, group=None, collide=None, blink=None, tag=None,
                        ground=true, pitch=0.0, roll=0.0, sink=0.4))]
    #[allow(clippy::too_many_arguments)]
    fn r#box(&self, py: Python<'_>, x: f64, z: f64, w: f64, h: f64, d: f64, y0: f64, yaw: f64, color: Option<&Bound<'_, PyAny>>,
             glow: Option<&Bound<'_, PyAny>>, group: Option<PyObject>, collide: Option<PyObject>, blink: Option<PyObject>, tag: Option<PyObject>,
             ground: bool, pitch: f64, roll: f64, sink: f64) -> R<usize> {
        let mb = self.map.bind(py);
        let m = mb.downcast::<Map>()?.borrow();
        let f = m.frame_(py, x, z, yaw)?;
        let (rv, up, fv) = (f[0], f[1], f[2]);
        let c0 = m.center(py)?;
        let base = axpy(c0, m.radius(py)?, up);
        let (bottom, top);
        if ground {
            let (hw, hd) = (w / 2.0, d / 2.0);
            let pts = [[0.0, 0.0], [-hw, -hd], [hw, -hd], [-hw, hd], [hw, hd], [-hw, 0.0], [hw, 0.0], [0.0, -hd], [0.0, hd]];
            let p: Vec<V3> = pts.iter().map(|q| axpy(axpy(base, q[0], rv), q[1], fv)).collect();
            let hs = m.height_(py, &p, up, base)?;
            let hc = hs[0];
            let hmin = hs.iter().copied().fold(f64::INFINITY, f64::min);
            bottom = if y0 > 0.05 { hc + y0 } else { hmin.min(hc + y0) - sink };
            top = hc + y0 + h;
        } else {
            let g0 = m.height_(py, &[base], up, base)?[0];
            bottom = g0 + y0;
            top = g0 + y0 + h;
        }
        // R の列が 右・上・前
        let mut rm = [[rv[0], up[0], fv[0]], [rv[1], up[1], fv[1]], [rv[2], up[2], fv[2]]];
        if pitch != 0.0 || roll != 0.0 {
            let (p, r) = (pitch.to_radians(), roll.to_radians());
            let rx = [[1.0, 0.0, 0.0], [0.0, p.cos(), -p.sin()], [0.0, p.sin(), p.cos()]];
            let rz = [[r.cos(), -r.sin(), 0.0], [r.sin(), r.cos(), 0.0], [0.0, 0.0, 1.0]];
            let mm = |a: [[f64; 3]; 3], b: [[f64; 3]; 3]| {
                let mut o = [[0.0; 3]; 3];
                for i in 0..3 {
                    for j in 0..3 {
                        o[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
                    }
                }
                o
            };
            rm = mm(mm(rm, rx), rz);
        }
        let c = axpy(base, (bottom + top) / 2.0, up);
        let a_t: Vec<f64> = (0..3).flat_map(|i| (0..3).map(move |j| rm[j][i])).collect();
        let part = PyDict::new_bound(py);
        part.set_item("c", arr(py, c))?;
        part.set_item("A", PyArray1::from_vec_bound(py, a_t).reshape([3, 3])?)?;
        part.set_item("e", arr(py, [w / 2.0, (top - bottom) / 2.0, d / 2.0]))?;
        part.set_item("color", match color {
            Some(c) => vec_or_none(py, Some(c))?,
            None => arr(py, CONC),
        })?;
        part.set_item("glow", vec_or_none(py, glow)?)?;
        part.set_item("group", group.unwrap_or_else(|| py.None()))?;
        part.set_item("collide", collide.unwrap_or_else(|| true.into_py(py)))?;
        part.set_item("blink", blink.unwrap_or_else(|| py.None()))?;
        part.set_item("tag", tag.unwrap_or_else(|| py.None()))?;
        let parts = self.parts.bind(py);
        parts.append(part)?;
        Ok(parts.len() - 1)
    }

    /// 世界座標の中心 c・向き R（列が 右・上・前）・半分の大きさ e の箱
    #[pyo3(signature = (c, r, e, color=None, glow=None, group=None, collide=None, blink=None, tag=None))]
    #[allow(clippy::too_many_arguments)]
    fn raw(&self, py: Python<'_>, c: &Bound<'_, PyAny>, r: &Bound<'_, PyAny>, e: &Bound<'_, PyAny>, color: Option<&Bound<'_, PyAny>>,
           glow: Option<&Bound<'_, PyAny>>, group: Option<PyObject>, collide: Option<PyObject>, blink: Option<PyObject>, tag: Option<PyObject>) -> R<usize> {
        let np = crate::game::imp(py, "numpy")?;
        let part = PyDict::new_bound(py);
        part.set_item("c", np.getattr("asarray")?.call1((c, "float64"))?)?;
        part.set_item("A", np.getattr("asarray")?.call1((r, "float64"))?.getattr("T")?.call_method0("copy")?)?;
        part.set_item("e", np.getattr("asarray")?.call1((e, "float64"))?)?;
        part.set_item("color", match color {
            Some(c) => vec_or_none(py, Some(c))?,
            None => arr(py, CONC),
        })?;
        part.set_item("glow", vec_or_none(py, glow)?)?;
        part.set_item("group", group.unwrap_or_else(|| py.None()))?;
        part.set_item("collide", collide.unwrap_or_else(|| true.into_py(py)))?;
        part.set_item("blink", blink.unwrap_or_else(|| py.None()))?;
        part.set_item("tag", tag.unwrap_or_else(|| py.None()))?;
        let parts = self.parts.bind(py);
        parts.append(part)?;
        Ok(parts.len() - 1)
    }

    fn point_raw(&self, py: Python<'_>, name: PyObject, p: &Bound<'_, PyAny>) -> R<PyObject> {
        let v = crate::game::imp(py, "numpy")?.getattr("asarray")?.call1((p, "float64"))?;
        self.points.bind(py).set_item(name, &v)?;
        Ok(v.unbind())
    }

    #[pyo3(signature = (name, x, z, y=0.0))]
    fn point(&self, py: Python<'_>, name: PyObject, x: f64, z: f64, y: f64) -> R<PyObject> {
        let mb = self.map.bind(py);
        let m = mb.downcast::<Map>()?.borrow();
        let (p, up) = m.ground_(py, x, z, 0.0)?;
        let v = arr(py, axpy(p, y, up));
        self.points.bind(py).set_item(name, &v)?;
        Ok(v)
    }

    /// 箱を配列（C・A・E・col・center・radius・tags）にまとめる
    fn finish(slf: Bound<'_, Self>) -> R<Bound<'_, Self>> {
        let py = slf.py();
        let np = crate::game::imp(py, "numpy")?;
        let parts = slf.borrow().parts.clone_ref(py).into_bound(py);
        let get = |k: &str| -> R<Bound<'_, PyList>> {
            let l = PyList::empty_bound(py);
            for p in parts.iter() {
                l.append(p.get_item(k)?)?;
            }
            Ok(l)
        };
        let c = np.getattr("array")?.call1((get("c")?,))?;
        let n = parts.len();
        let c = if n == 0 { np.getattr("zeros")?.call1(((0, 3),))? } else { c };
        slf.setattr("C", &c)?;
        slf.setattr("A", np.getattr("array")?.call1((get("A")?,))?)?;
        slf.setattr("E", np.getattr("array")?.call1((get("e")?,))?)?;
        slf.setattr("col", np.getattr("array")?.call1((get("collide")?, np.getattr("bool_")?))?)?;
        let cs = rows3(&c)?;
        let es = rows3(&slf.getattr("E")?)?;
        let mut cen = [0.0; 3];
        for p in &cs {
            for a in 0..3 {
                cen[a] += p[a] / cs.len() as f64;
            }
        }
        if cs.is_empty() {
            cen = [f64::NAN; 3];
        }
        slf.setattr("center", arr(py, cen))?;
        let mut radius = f64::NEG_INFINITY;
        for (p, e) in cs.iter().zip(es.iter()) {
            radius = radius.max(dot(sub(*p, cen), sub(*p, cen)).sqrt() + dot(*e, *e).sqrt());
        }
        slf.setattr("radius", radius + 1.0)?;
        let tags = PyDict::new_bound(py);
        for (k, p) in parts.iter().enumerate() {
            let t = p.get_item("tag")?;
            if t.is_truthy()? {
                tags.set_item(t, k)?;
            }
        }
        slf.setattr("tags", tags)?;
        Ok(slf)
    }

    /// 動かない物（当たり判定の持ち主として）
    fn is_moving(&self) -> bool {
        false
    }

    fn point_velocity(&self, py: Python<'_>, p: &Bound<'_, PyAny>) -> R<PyObject> {
        let np = crate::game::imp(py, "numpy")?;
        Ok(np.getattr("zeros_like")?.call1((np.getattr("asarray")?.call1((p, "float64"))?,))?.unbind())
    }
}

// ---------------------------------------------------------------------------
// SceneryHost の中身
// ---------------------------------------------------------------------------
/// 構築物の範囲 lo〜hi（+margin）にかかる箱：[(景色, C, A, E, 番号)]
#[pyfunction]
pub fn scenery_colliders<'py>(py: Python<'py>, scenes: &Bound<'py, PyAny>, lo: &Bound<'py, PyAny>, hi: &Bound<'py, PyAny>, margin: f64)
    -> R<Bound<'py, PyList>> {
    let (lo, hi) = (v3(lo)?, v3(hi)?);
    let ctr = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let half = dot(sub(hi, lo), sub(hi, lo)).sqrt() / 2.0;
    let out = PyList::empty_bound(py);
    let np = crate::game::imp(py, "numpy")?;
    for s in scenes.iter()? {
        let s = s?;
        let sc = v3(&s.getattr("center")?)?;
        let rad: f64 = s.getattr("radius")?.extract()?;
        if dot(sub(ctr, sc), sub(ctr, sc)).sqrt() > rad + half + margin {
            continue;
        }
        let c: PyReadonlyArray2<f64> = s.getattr("C")?.extract()?;
        let e: PyReadonlyArray2<f64> = s.getattr("E")?.extract()?;
        let col: numpy::PyReadonlyArray1<bool> = s.getattr("col")?.extract()?;
        let (ca, ea, cola) = (c.as_array(), e.as_array(), col.as_array());
        let mut idx: Vec<i64> = Vec::new();
        for k in 0..ca.nrows() {
            if !cola[k] {
                continue;
            }
            let rb = (ea[[k, 0]].powi(2) + ea[[k, 1]].powi(2) + ea[[k, 2]].powi(2)).sqrt();
            if (0..3).all(|a| ca[[k, a]] + rb + margin > lo[a] && ca[[k, a]] - rb - margin < hi[a]) {
                idx.push(k as i64);
            }
        }
        if !idx.is_empty() {
            let ix = PyArray1::from_vec_bound(py, idx);
            let take = |name: &str| -> R<Bound<'py, PyAny>> { s.getattr(name)?.get_item(&ix) };
            out.append((&s, take("C")?, take("A")?, take("E")?, &ix))?;
        }
    }
    let _ = np;
    Ok(out)
}

/// 視線と景色の箱：いちばん手前の (景色, t, 箱の番号, 法線) か None
#[pyfunction]
pub fn scenery_raycast<'py>(py: Python<'py>, scenes: &Bound<'py, PyAny>, o: [f64; 3], d: [f64; 3], max_t: f64)
    -> R<Option<(Bound<'py, PyAny>, f64, i64, PyObject)>> {
    let mut best: Option<(Bound<'py, PyAny>, f64, i64, V3)> = None;
    for s in scenes.iter()? {
        let s = s?;
        let sc = v3(&s.getattr("center")?)?;
        let rad: f64 = s.getattr("radius")?.extract()?;
        if dot(sub(o, sc), sub(o, sc)).sqrt() > rad + max_t {
            continue;
        }
        let c = rows3(&s.getattr("C")?)?;
        if c.is_empty() {
            continue;
        }
        let e = rows3(&s.getattr("E")?)?;
        let a: PyReadonlyArray3<f64> = crate::game::imp(py, "numpy")?.getattr("ascontiguousarray")?.call1((s.getattr("A")?, "float64"))?.extract()?;
        let aa = a.as_array();
        let ax: Vec<[V3; 3]> = (0..c.len()).map(|b| [[aa[[b, 0, 0]], aa[[b, 0, 1]], aa[[b, 0, 2]]], [aa[[b, 1, 0]], aa[[b, 1, 1]], aa[[b, 1, 2]]],
                                                    [aa[[b, 2, 0]], aa[[b, 2, 1]], aa[[b, 2, 2]]]]).collect();
        if let Some((t, k, n)) = crate::geomq::ray_boxes(o, d, &c, &ax, &e, max_t) {
            if best.as_ref().map_or(true, |b| t < b.1) {
                best = Some((s, t, k, n));
            }
        }
    }
    Ok(best.map(|(s, t, k, n)| (s, t, k, arr(py, n))))
}

/// 球（中心 center・半径 r）を景色の箱から押し出す：[(押し出し, 法線, 景色)]。箱は host.colliders で集める（MOD が上書きできる）
#[pyfunction]
pub fn scenery_push_sphere<'py>(py: Python<'py>, host: &Bound<'py, PyAny>, world: &Bound<'py, PyAny>, center: &Bound<'py, PyAny>, r: f64)
    -> R<Bound<'py, PyList>> {
    let c = v3(center)?;
    let lo = arr(py, [c[0] - r, c[1] - r, c[2] - r]);
    let hi = arr(py, [c[0] + r, c[1] + r, c[2] + r]);
    let out = PyList::empty_bound(py);
    for it in host.call_method1("colliders", (world, lo, hi, 1.0))?.iter()? {
        let it = it?;
        let s = it.get_item(0)?;
        let cc = rows3(&it.get_item(1)?)?;
        let ee = rows3(&it.get_item(3)?)?;
        let a: PyReadonlyArray3<f64> = crate::game::imp(py, "numpy")?.getattr("ascontiguousarray")?.call1((it.get_item(2)?, "float64"))?.extract()?;
        let flat: Vec<f64> = a.as_array().iter().copied().collect();
        let cf: Vec<f64> = cc.iter().flatten().copied().collect();
        let ef: Vec<f64> = ee.iter().flatten().copied().collect();
        let (mut dd, mut nn, mut kk) = (vec![0.0; 1], vec![0.0; 3], vec![0i64; 1]);
        crate::kernels::box_sdf(&c, &cf, &flat, &ef, &mut dd, &mut nn, &mut kk);
        if dd[0] < r {
            let n = [nn[0], nn[1], nn[2]];
            out.append((arr(py, [n[0] * (r - dd[0]), n[1] * (r - dd[0]), n[2] * (r - dd[0])]), arr(py, n), s))?;
        }
    }
    Ok(out)
}

/// SceneryHost.raycast の中身：視線が当たった景色の箱の dict（kind・t・scene・part・point・normal・placeable・tag）か None
#[pyfunction]
pub fn scenery_raycast_hit<'py>(py: Python<'py>, host: &Bound<'py, PyAny>, o: &Bound<'py, PyAny>, dvec: &Bound<'py, PyAny>, max_t: f64)
    -> PyResult<Bound<'py, PyAny>> {
    let scenes: Vec<Bound<'py, PyAny>> = host.getattr("scenes")?.iter()?.collect::<PyResult<_>>()?;
    let ov = super::construct::v3(o)?;
    let dv = super::construct::v3(dvec)?;
    let r = scenery_raycast(py, PyList::new_bound(py, &scenes).as_any(), ov, dv, max_t)?;
    let (s, tf, k, n) = match r { Some(r) => r, None => return Ok(py.None().into_bound(py)) };
    let t = tf;
    let d = PyDict::new_bound(py);
    d.set_item("kind", host.getattr("scene_kind")?)?;
    d.set_item("t", &t)?;
    d.set_item("scene", &s)?;
    d.set_item("part", &k)?;
    d.set_item("point", super::construct::a1(py, [ov[0] + dv[0] * tf, ov[1] + dv[1] * tf, ov[2] + dv[2] * tf]))?;
    d.set_item("normal", n)?;
    d.set_item("placeable", true)?;
    d.set_item("tag", s.getattr("parts")?.get_item(&k)?.get_item("tag")?)?;
    Ok(d.into_any())
}
