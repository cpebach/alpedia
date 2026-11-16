//! ワールド：重力・軌道・構築物の出し入れ

use super::*;

#[pymethods]
impl World {
    pub(crate) fn gravity<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let np = crate::game::construct::np(py)?;
        let pls: Vec<Bound<'py, PyAny>> = Self::g(slf, "planets")?.iter()?.collect::<R<_>>()?;
        if pls.is_empty() {
            return np.call_method1("zeros_like", (np.call_method1("asarray", (p, "float64"))?,));
        }
        let ok = match opt(slf.as_any(), "_grav_key")? { Some(k) => k.eq(pls.len())?, None => false };
        if !ok {
            let mut c = vec![];
            let mut gm = vec![];
            let mut r = vec![];
            for x in &pls {
                c.extend(v3(&x.getattr("center")?)?);
                gm.push(x.getattr("gm")?.extract::<f64>()?);
                r.push(x.getattr("radius")?.extract::<f64>()?);
            }
            let n = pls.len();
            Self::set(slf, "_grav", (arr(py, c, &[n, 3])?, arr(py, gm, &[n])?, arr(py, r, &[n])?))?;
            Self::set(slf, "_grav_key", n)?;
        }
        let gr = Self::g(slf, "_grav")?;
        let pa = np.call_method1("ascontiguousarray", (p, "float64"))?.call_method1("reshape", (-1, 3))?;
        let g = imp(py, "alpedia_core")?.call_method1("gravity", (pa, gr.get_item(0)?, gr.get_item(1)?, gr.get_item(2)?))?;
        g.call_method1("reshape", (np.call_method1("shape", (p,))?,))?.call_method1("__mul__", (Self::gscale(slf)?,))
    }
    pub(crate) fn nearest_planet<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let p = v3(p)?;
        let mut best: Bound<'py, PyAny> = py.None().into_bound(py);
        let mut bd = 1e300;
        for pl in Self::g(slf, "planets")?.iter()? {
            let pl = pl?;
            let d = (norm(sub(p, v3(&pl.getattr("center")?)?)) - pl.getattr("radius")?.extract::<f64>()?)
                / pl.getattr("gm")?.extract::<f64>()?.max(1e-9).powf(0.5);
            if d < bd {
                best = pl;
                bd = d;
            }
        }
        Ok(best)
    }
    /// 最寄り惑星に対する軌道要素（二体問題の近似）
    pub(crate) fn orbit_info<'py>(slf: &Bound<'py, Self>, pos: &Bound<'py, PyAny>, vel: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let pl = Self::nearest_planet(slf, pos)?;
        if pl.is_none() {
            return Ok(pl);
        }
        let mu = pl.getattr("gm")?.extract::<f64>()? * Self::gscale(slf)?;
        let rad: f64 = pl.getattr("radius")?.extract()?;
        let r = sub(v3(pos)?, v3(&pl.getattr("center")?)?);
        let v = v3(vel)?;
        let rn = norm(r);
        let up = [r[0] / rn, r[1] / rn, r[2] / rn];
        let vr = dot(v, up);
        let vh = norm(sub(v, [up[0] * vr, up[1] * vr, up[2] * vr]));
        let out = PyDict::new_bound(py);
        let inf = f64::INFINITY;
        out.set_item("planet", pl.getattr("name")?)?;
        out.set_item("radius", pl.getattr("radius")?)?;
        out.set_item("altitude", rn - rad)?;
        out.set_item("vertical_speed", vr)?;
        out.set_item("horizontal_speed", vh)?;
        out.set_item("orbital_speed", if mu > 0.0 { (mu / rn).sqrt() } else { 0.0 })?;
        out.set_item("apoapsis", inf)?;
        out.set_item("periapsis", rn - rad)?;
        out.set_item("eccentricity", 1.0)?;
        out.set_item("period", inf)?;
        out.set_item("time_to_apoapsis", inf)?;
        if mu <= 0.0 {
            return Ok(out.into_any());
        }
        let energy = dot(v, v) / 2.0 - mu / rn;
        let h = norm(cross(r, v));
        let e = (1.0 + 2.0 * energy * h * h / (mu * mu)).max(0.0).sqrt();
        out.set_item("eccentricity", e)?;
        if energy < 0.0 {
            let a = -mu / (2.0 * energy);
            out.set_item("apoapsis", a * (1.0 + e) - rad)?;
            out.set_item("periapsis", a * (1.0 - e) - rad)?;
            out.set_item("period", 2.0 * std::f64::consts::PI * (a.powf(3.0) / mu).sqrt())?;
            if e > 1e-6 {
                // 真近点角から遠点までの時間
                let cos_e = ((1.0 - rn / a) / e).clamp(-1.0, 1.0);
                let mut ea = cos_e.acos();
                if vr < 0.0 {
                    ea = 2.0 * std::f64::consts::PI - ea;
                }
                let m = ea - e * ea.sin();
                let n = (mu / a.powf(3.0)).sqrt();
                out.set_item("time_to_apoapsis", (std::f64::consts::PI - m).rem_euclid(2.0 * std::f64::consts::PI) / n)?;
            }
        } else {
            out.set_item("periapsis", (h * h / mu) / (1.0 + e) - rad)?;
        }
        Ok(out.into_any())
    }

    // --- アクチュエーター用 ---
    pub(crate) fn actuator_detach(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, i: usize) -> R<bool> {
        if !crate::game::actuator::act_cmd_detach(c, i)? {
            return Ok(false);
        }
        for part in c.call_method0("split")?.iter()? {
            let part = part?;
            Self::add_construct(slf, &part)?;
            part.call_method0("wake")?;
        }
        c.call_method0("wake")?;
        Ok(true)
    }
    /// other を c に取り込む（座っているプレイヤーの参照も付け替える）。片方だけがアンカーで固定されているときは
    /// 固定されていない側を、固定されている側の格子の位置に合わせる
    #[pyo3(signature = (c, other, cell_map, xform=None))]
    pub(crate) fn merge_constructs(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, other: &Bound<'_, PyAny>, cell_map: &Bound<'_, PyAny>,
                        xform: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        let n0: i64 = c.getattr("n_vox")?.extract()?;
        let ca = c.getattr("anchored")?.is_truthy()?;
        let oa = other.getattr("anchored")?.is_truthy()?;
        c.call_method1("absorb", (other, cell_map, xform.map(|x| x.clone().unbind()).unwrap_or_else(|| py.None())))?;
        if ca != oa {
            let n1: i64 = c.getattr("n_vox")?.extract()?;
            let k: Vec<i64> = if ca { (0..n0).collect() } else { (n0..n1).collect() };
            let nk = k.len();
            c.call_method1("snap_to_lattice", (crate::game::construct::iarr(py, k, &[nk])?,))?;
        }
        // 結合した直後は、めり込みの押し出しで吹き飛ばないように、しばらく速さの変化をおさえる
        let cur: f64 = match opt(c, "_settle")? { Some(v) => v.extract()?, None => 0.0 };
        let t = fget(&phys(&Self::g(slf, "cfg")?)?, "merge_settle_time", 1.0)?;
        c.setattr("_settle", cur.max(t))?;
        let oid = other.getattr("id")?;
        for p in Self::all_players(slf)? {
            if Self::is_seated_on(&p, &oid)? {
                let s = p.getattr("seated")?;
                p.setattr("seated", (c.getattr("id")?, cell_map.get_item(s.get_item(1)?)?))?;
            }
        }
        Self::g(slf, "constructs")?.call_method1("pop", (&oid, py.None()))?;
        log(py)?.call_method1("info", ("構築物を結合: %s ← %s", c.getattr("name")?, other.getattr("name")?))?;
        Ok(())
    }
    pub(crate) fn seated_in(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>) -> R<bool> {
        let cid = c.getattr("id")?;
        for p in Self::all_players(slf)? {
            let s = p.getattr("seated")?;
            if !s.is_none() && s.get_item(0)?.eq(&cid)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(crate) fn add_construct<'py>(slf: &Bound<'py, Self>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        Self::g(slf, "constructs")?.set_item(c.getattr("id")?, c)?;
        Ok(c.clone())
    }
}
