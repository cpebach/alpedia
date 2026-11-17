//! ワールド：近くの構築物・光線・ボクセルとの接触

use crate::game::planet::{pla, Planet};
use super::*;
use crate::game::construct::{con, Construct};

#[pymethods]
impl World {
    /// 点 p から r 以内に外側の球がかかる構築物
    pub(crate) fn constructs_near<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>, r: f64) -> R<Vec<Bound<'py, PyAny>>> {
        let b = Self::_bounds(slf)?;
        let cs: Vec<Bound<'py, PyAny>> = b.get_item(0)?.iter()?.collect::<R<_>>()?;
        if cs.is_empty() {
            return Ok(cs);
        }
        let c = v3s(&b.get_item(1)?)?;
        let rr = fv(&b.get_item(2)?)?;
        let p = v3(p)?;
        Ok(cs.into_iter().enumerate().filter(|(k, _)| norm(sub(c[*k], p)) < rr[*k] + r).map(|(_, x)| x).collect())
    }
    /// 線分 o → o + dvec × length の近くに外側の球がかかる構築物
    pub(crate) fn constructs_on_ray<'py>(slf: &Bound<'py, Self>, o: &Bound<'py, PyAny>, dvec: &Bound<'py, PyAny>, length: f64) -> R<Vec<Bound<'py, PyAny>>> {
        let b = Self::_bounds(slf)?;
        let cs: Vec<Bound<'py, PyAny>> = b.get_item(0)?.iter()?.collect::<R<_>>()?;
        if cs.is_empty() {
            return Ok(cs);
        }
        let c = v3s(&b.get_item(1)?)?;
        let rr = fv(&b.get_item(2)?)?;
        let (o, d) = (v3(o)?, v3(dvec)?);
        Ok(cs.into_iter().enumerate().filter(|(k, _)| {
            let rel = sub(c[*k], o);
            let t = dot(rel, d).clamp(0.0, length);
            norm(sub(rel, [t * d[0], t * d[1], t * d[2]])) < rr[*k]
        }).map(|(_, x)| x).collect())
    }
    /// 球（プレイヤーなど）と構築物の形（球・カプセル・斜面）の接触: [(construct, idx, push, normal)]
    pub(crate) fn voxel_contacts<'py>(slf: &Bound<'py, Self>, center: &Bound<'py, PyAny>, radius: f64) -> R<Bound<'py, PyList>> {
        let py = slf.py();
        let shape = imp(py, "main.shape")?;
        let out = PyList::empty_bound(py);
        let mut ctr = v3(center)?;
        for c in Self::constructs_near(slf, &a1(py, ctr), radius + 1.0)? {
            if Construct::n(con(&c)?)? == 0 {
                continue;
            }
            for _ in 0..2 {
                let res = shape.call_method1("closest_on_shape", (&c, a1(py, ctr), radius))?;
                if res.is_none() {
                    break;
                }
                let dist: f64 = res.get_item(0)?.extract()?;
                if dist >= radius {
                    break;
                }
                let q = v3(&res.get_item(1)?)?;
                let vi: usize = res.get_item(2)?.extract()?;
                let v = sub(ctr, q);
                let l = norm(v);
                let n = if l > 1e-9 { [v[0] / l, v[1] / l, v[2] / l] } else {
                    let vc = v3(&Construct::voxel_centers(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?.get_item(vi)?)?;
                    let d = sub(ctr, vc);
                    let ln = norm(d).max(1e-9);
                    [d[0] / ln, d[1] / ln, d[2] / ln]
                };
                let pen = radius - dist;
                let push = [n[0] * pen, n[1] * pen, n[2] * pen];
                out.append((&c, vi, a1(py, push), a1(py, n)))?;
                ctr = [ctr[0] + push[0], ctr[1] + push[1], ctr[2] + push[2]];
            }
        }
        Ok(out)
    }
    /// 視線の先のボクセル／地形。戻り値 dict または None
    #[pyo3(signature = (origin, direction, max_dist=8.0))]
    pub(crate) fn raycast<'py>(slf: &Bound<'py, Self>, origin: &Bound<'py, PyAny>, direction: &Bound<'py, PyAny>, max_dist: f64) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let o = v3(origin)?;
        let d0 = v3(direction)?;
        let l = norm(d0);
        let d = [d0[0] / l, d0[1] / l, d0[2] / l];
        let core = imp(py, "alpedia_core")?;
        let (oa, da) = (a1(py, o), a1(py, d));
        let mut best: Option<Bound<'py, PyDict>> = None;
        let bt = |b: &Option<Bound<'py, PyDict>>| -> R<Option<f64>> {
            match b { Some(b) => Ok(Some(b.get_item("t")?.unwrap().extract()?)), None => Ok(None) }
        };
        for c in Self::constructs_on_ray(slf, &oa, &da, max_dist)? {
            if Construct::n(con(&c)?)? == 0 {
                continue;
            }
            let hit = core.call_method1("voxel_raycast", (crate::game::construct::cflt(&Construct::centers_cached(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?)?,
                crate::game::construct::cflt(&Construct::frames_cached(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?)?, o.to_vec(), d.to_vec(),
                Construct::g_size(con(&c)?)?.extract::<f64>()?, max_dist))?;
            if !hit.is_none() {
                let t: f64 = hit.get_item(0)?.extract()?;
                if bt(&best)?.map(|b| t < b).unwrap_or(true) {
                    let r = PyDict::new_bound(py);
                    r.set_item("kind", "voxel")?;
                    r.set_item("t", t)?;
                    r.set_item("construct", &c)?;
                    r.set_item("index", hit.get_item(1)?.call_method0("__int__")?)?;
                    r.set_item("face", hit.get_item(2)?.call_method0("__int__")?)?;
                    r.set_item("point", a1(py, [o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t]))?;
                    best = Some(r);
                }
            }
        }
        for pl in Self::g_planets(slf)?.iter()? {
            let pl = pl?;
            if norm(sub(o, v3(&pl.getattr("center")?)?)) > pl.getattr("bound")?.extract::<f64>()? + max_dist {
                continue;
            }
            let r = Planet::raycast(pla(&pl)?, &IntoPy::<PyObject>::into_py(&oa, pl.py()).into_bound(pl.py()), &IntoPy::<PyObject>::into_py(&da, pl.py()).into_bound(pl.py()), IntoPy::<PyObject>::into_py(max_dist, pl.py()).extract::<f64>(pl.py())?).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?;
            if r.is_truthy()? {
                let t: f64 = r.get_item(0)?.extract()?;
                if bt(&best)?.map(|b| t < b).unwrap_or(true) {
                    let x = PyDict::new_bound(py);
                    x.set_item("kind", "terrain")?;
                    x.set_item("t", r.get_item(0)?)?;
                    x.set_item("planet", &pl)?;
                    x.set_item("point", r.get_item(1)?)?;
                    let p1 = v3(&r.get_item(1)?)?;
                    x.set_item("normal", Planet::normal(pla(&pl)?, &IntoPy::<PyObject>::into_py(arr(py, p1.to_vec(), &[1, 3])?, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?.get_item(0)?)?;
                    best = Some(x);
                }
            }
            // 水面（海・池）：地面や構築物より手前にあれば水を指す
            if pl.getattr("has_water")?.is_truthy()? {
                let lim = bt(&best)?.unwrap_or(max_dist);
                let w = Planet::water_raycast(pla(&pl)?, &IntoPy::<PyObject>::into_py(&oa, pl.py()).into_bound(pl.py()), &IntoPy::<PyObject>::into_py(&da, pl.py()).into_bound(pl.py()), IntoPy::<PyObject>::into_py(lim, pl.py()).extract::<f64>(pl.py())?).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?;
                if w.is_truthy()? {
                    let t: f64 = w.get_item(0)?.extract()?;
                    if bt(&best)?.map(|b| t < b).unwrap_or(true) {
                        let x = PyDict::new_bound(py);
                        x.set_item("kind", "water")?;
                        x.set_item("t", w.get_item(0)?)?;
                        x.set_item("planet", &pl)?;
                        x.set_item("point", w.get_item(1)?)?;
                        x.set_item("normal", Planet::up(pla(&pl)?, &IntoPy::<PyObject>::into_py(w.get_item(1)?, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?)?;
                        best = Some(x);
                    }
                }
            }
        }
        let mut res: Bound<'py, PyAny> = match &best { Some(b) => b.clone().into_any(), None => py.None().into_bound(py) };
        for a in Self::addons_v(slf)? {
            let lim = if res.is_none() { max_dist } else { res.get_item("t")?.extract()? };
            let r = a.call_method1("raycast", (slf, &oa, &da, lim))?;
            if r.is_truthy()? && (res.is_none() || r.get_item("t")?.extract::<f64>()? < res.get_item("t")?.extract::<f64>()?) {
                res = r;
            }
        }
        Ok(res)
    }
}
