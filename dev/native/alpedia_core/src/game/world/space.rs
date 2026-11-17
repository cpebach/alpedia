//! ワールド：重力・軌道・構築物の出し入れ

use crate::game::PyDictOps;
use crate::game::player::{ply, Player};
use super::*;
use crate::game::construct::{con, Construct};

#[pymethods]
impl World {
    /// 重力（全部の惑星の和 × 重力倍率）。公転する惑星の影響圏の中では、その惑星の加速度を引く（物は惑星と一緒に運ばれるので。L-08）
    pub(crate) fn gravity<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let np = crate::game::construct::np(py)?;
        let pa = crate::game::construct::as_f64(&p)?;
        let shape = pa.getattr("shape")?;
        let pts = crate::game::construct::fv(&crate::game::construct::cflt(&pa)?)?;
        let pd = Self::planet_data(slf)?;
        let gs = Self::gscale(slf)?;
        let mut out = vec![0.0; pts.len()];
        for (i, q) in pts.chunks_exact(3).enumerate() {
            let g = grav_at(&pd, [q[0], q[1], q[2]], gs);
            out[3 * i..3 * i + 3].copy_from_slice(&g);
        }
        let n = out.len();
        np.call_method1("reshape", (arr(py, out, &[n])?, shape))
    }
    pub(crate) fn nearest_planet<'py>(slf: &Bound<'py, Self>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let p = v3(p)?;
        let mut best: Bound<'py, PyAny> = py.None().into_bound(py);
        let mut bd = 1e300;
        for pl in Self::g_planets(slf)?.iter()? {
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
        for part in Construct::split(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?.iter()? {
            let part = part?;
            Self::add_construct(slf, &part)?;
            part.call_method0("wake")?;
        }
        Construct::wake(con(&c)?, false).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?;
        Ok(true)
    }
    /// other を c に取り込む（座っているプレイヤーの参照も付け替える）。片方だけがアンカーで固定されているときは
    /// 固定されていない側を、固定されている側の格子の位置に合わせる
    #[pyo3(signature = (c, other, cell_map, xform=None))]
    pub(crate) fn merge_constructs(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, other: &Bound<'_, PyAny>, cell_map: &Bound<'_, PyAny>,
                        xform: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        let n0: i64 = Construct::n(con(&c)?)?.into_py(py).into_bound(py).extract()?;
        let ca = c.getattr("anchored")?.is_truthy()?;
        let oa = other.getattr("anchored")?.is_truthy()?;
        Construct::absorb(con(&c)?, &IntoPy::<PyObject>::into_py(other, c.py()).into_bound(c.py()), &IntoPy::<PyObject>::into_py(cell_map, c.py()).into_bound(c.py()), Some(&IntoPy::<PyObject>::into_py(xform.map(|x| x.clone().unbind()).unwrap_or_else(|| py.None()), c.py()).into_bound(c.py()))).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?;
        if ca != oa {
            let n1: i64 = Construct::n(con(&c)?)?.into_py(py).into_bound(py).extract()?;
            let k: Vec<i64> = if ca { (0..n0).collect() } else { (n0..n1).collect() };
            let nk = k.len();
            Construct::snap_to_lattice(con(&c)?, &IntoPy::<PyObject>::into_py(crate::game::construct::iarr(py, k, &[nk])?, c.py()).into_bound(c.py())).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?;
        }
        // 結合した直後は、めり込みの押し出しで吹き飛ばないように、しばらく速さの変化をおさえる
        let cur: f64 = con(c)?.borrow().settle;
        let t = fget(&phys(&Self::g_cfg(slf)?)?, "merge_settle_time", 1.0)?;
        Construct::s_settle(con(&c)?, cur.max(t))?;
        let oid = other.getattr("id")?;
        for p in Self::all_players(slf)? {
            if Self::is_seated_on(&p, &oid)? {
                let s = Player::g_seated(ply(&p)?)?;
                Player::s_seated(ply(&p)?, (Construct::g_id(con(&c)?)?, cell_map.get_item(s.get_item(1)?)?))?;
            }
        }
        Self::g_constructs(slf)?.dpop2(&oid, py.None())?;
        log(py)?.call_method1("info", ("構築物を結合: %s ← %s", Construct::g_name(con(&c)?)?, other.getattr("name")?))?;
        Ok(())
    }
    pub(crate) fn seated_in(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>) -> R<bool> {
        let cid = Construct::g_id(con(&c)?)?;
        for p in Self::all_players(slf)? {
            let s = Player::g_seated(ply(&p)?)?;
            if !s.is_none() && s.get_item(0)?.eq(&cid)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(crate) fn add_construct<'py>(slf: &Bound<'py, Self>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        Self::g_constructs(slf)?.set_item(Construct::g_id(con(&c)?)?, c)?;
        Ok(c.clone())
    }
}

/// 重力の計算に使う惑星の値：(中心, gm, 半径, 影響圏, 加速度)
pub(crate) type PData = Vec<(V3, f64, f64, f64, V3)>;

impl World {
    pub(crate) fn planet_data(slf: &Bound<'_, Self>) -> R<PData> {
        let mut out = vec![];
        for x in Self::g_planets(slf)?.iter()? {
            let x = x?;
            let b = crate::game::planet::pla(&x)?.borrow();
            let py = slf.py();
            out.push((v3(b.center.bind(py))?, b.gm, b.radius, b.soi, b.oacc));
        }
        Ok(out)
    }
}

/// 点 p の重力（body.rs の gravity と同じ）
pub(crate) fn grav_at(pd: &PData, p: V3, gs: f64) -> V3 {
    let mut g = [0.0; 3];
    for (c, gm, r, _, _) in pd {
        let q = sub(*c, p);
        let r2 = dot(q, q).max((r * 0.5).powi(2));
        let s = gm / (r2 * r2.sqrt());
        for a in 0..3 {
            g[a] += q[a] * s;
        }
    }
    let off = match frame_index(pd, p) { Some(i) => pd[i].4, None => [0.0; 3] };
    [g[0] * gs - off[0], g[1] * gs - off[1], g[2] * gs - off[2]]
}

/// 点 p がどの公転する惑星の影響圏にあるか（いちばん小さい影響圏）
pub(crate) fn frame_index(pd: &PData, p: V3) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, (c, _, _, soi, _)) in pd.iter().enumerate() {
        if *soi > 0.0 {
            let q = sub(p, *c);
            if dot(q, q) < soi * soi && best.map_or(true, |b| *soi < pd[b].3) {
                best = Some(i);
            }
        }
    }
    best
}

impl World {
    /// 点 p の空気の密度（地表 101.3 kPa を 1。高さ（半径の 1/4、30 m 以上ごと）で 1/e。全部の惑星の和。A-01 のジェット用）
    pub(crate) fn air_at(slf: &Bound<'_, Self>, p: V3) -> R<f64> {
        let py = slf.py();
        let mut a = 0.0;
        for x in Self::g_planets(slf)?.iter()? {
            let x = x?;
            let b = crate::game::planet::pla(&x)?.borrow();
            let kpa: f64 = match b.defn.bind(py).dget2("atmosphere_pressure", 101.3) {
                Ok(v) if !v.is_none() => v.extract().unwrap_or(101.3),
                _ => 101.3,
            };
            if kpa <= 0.0 {
                continue;
            }
            let alt = (norm(sub(p, v3(b.center.bind(py))?)) - b.radius).max(0.0);
            let h = (b.radius * 0.25).max(30.0);
            if alt < h * 30.0 {
                a += kpa / 101.3 * (-alt / h).exp();
            }
        }
        Ok(a)
    }
}
