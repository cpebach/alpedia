//! ワールド：惑星の公転（L-08）
//!
//! 公転する惑星（月・小惑星）は、親の惑星（ふつうは planets の最初）のまわりを円軌道で回る。位置は時刻だけで決まる
//! （保存・通信で送るのは時刻だけでよい）。
//! 公転する惑星の影響圏の中の物（構築物・プレイヤー）は、毎ティック惑星と同じだけ動かす（惑星に止まった座標で計算する）。
//! そのため影響圏の中の速度は惑星に対する速度で、重力からは惑星の加速度を引く（body.rs の gravity・space.rs の grav_at）。
//! 影響圏を出入りするときは、世界から見た速度が変わらないように惑星の速度を足し引きする。

use super::space::{frame_index, PData};
use super::*;
use crate::game::construct::{con, Construct};
use crate::game::planet::{pla, Orbit};

/// 公転していない（親を持たない）とき・まだ決めていないときの印
const NO_FRAME: i64 = -1;
const UNKNOWN: i64 = -2;

fn add3(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// 配列の行（3 つずつ）すべてに d を足す（その場で。書けない配列なら作り直して返す）
fn shift_rows<'py>(py: Python<'py>, a: &Bound<'py, PyAny>, d: V3) -> R<Option<Bound<'py, PyAny>>> {
    if a.is_none() {
        return Ok(None);
    }
    if with_rw(a, |s| for q in s.chunks_exact_mut(3) { for k in 0..3 { q[k] += d[k]; } })? {
        return Ok(None);
    }
    let mut v = fv(a)?;
    for q in v.chunks_exact_mut(3) {
        for k in 0..3 {
            q[k] += d[k];
        }
    }
    let shape = a.getattr("shape")?;
    Ok(Some(crate::game::construct::np(py)?.call_method1("reshape", (arr(py, v.clone(), &[v.len()])?, shape))?))
}

impl World {
    /// 読み込んだとき：settings の planets の "orbit" から公転を決める。
    /// "orbit" が無い（前のセーブ）ときは、最初の惑星以外は最初の惑星のまわりを回る。そのときの基準の時刻は今の時刻
    /// （保存してあった位置が今の位置）。"orbit": false なら公転しない
    pub(crate) fn setup_orbits(slf: &Bound<'_, Self>, entries: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let time = slf.borrow().time;
        let gs = Self::gscale(slf)?;
        let pls: Vec<Bound<'_, PyAny>> = Self::g_planets(slf)?.iter()?.collect::<R<_>>()?;
        for (i, pl) in pls.iter().enumerate() {
            let ent = entries.get_item(i)?;
            let o = getd(&ent, "orbit", py.None())?;
            if o.is_instance_of::<pyo3::types::PyBool>() && !o.is_truthy()? {
                continue;
            }
            let (parent, axis, t0): (usize, Option<V3>, f64) = if o.is_instance_of::<PyDict>() {
                let ax = getd(&o, "axis", py.None())?;
                (getd(&o, "around", 0)?.extract()?, if ax.is_none() { None } else { Some(v3(&ax)?) }, fget(&o, "t0", time)?)
            } else if i == 0 {
                continue;
            } else {
                (0, None, time)
            };
            if parent >= i {
                continue;       // 親は先に並んでいる惑星だけ
            }
            let (c0, gm, bound) = { let b = pla(pl)?.borrow(); (b.center0, b.gm, b.bound) };
            let (pc0, pgm) = { let b = pla(&pls[parent])?.borrow(); (b.center0, b.gm) };
            let rel0 = sub(c0, pc0);
            let r = norm(rel0);
            if r < 1e-6 {
                continue;
            }
            // 軸：指定（無ければ上 +Y）から公転の面に垂直な成分だけを取る
            let u = [rel0[0] / r, rel0[1] / r, rel0[2] / r];
            let mut ax = axis.unwrap_or([0.0, 1.0, 0.0]);
            let k = dot(ax, u);
            ax = [ax[0] - u[0] * k, ax[1] - u[1] * k, ax[2] - u[2] * k];
            if norm(ax) < 1e-6 {
                ax = cross(u, [0.0, 0.0, 1.0]);
                if norm(ax) < 1e-6 {
                    ax = cross(u, [1.0, 0.0, 0.0]);
                }
            }
            let n = norm(ax);
            let ax = [ax[0] / n, ax[1] / n, ax[2] / n];
            let omega = (gs * (pgm + gm) / r.powi(3)).sqrt();
            let soi = (r * (gm / pgm.max(1e-9)).powf(0.4)).max(bound + 100.0).min(r * 0.5);
            let mut b = pla(pl)?.borrow_mut();
            b.orbit = Some(Orbit { parent, axis: ax, omega, t0, rel0 });
            b.soi = soi;
        }
        Self::orbit_tick(slf, false)
    }

    /// 公転を今の時刻に進める。carry なら影響圏の中の物を惑星と一緒に動かす（step の始めに）
    pub(crate) fn orbit_tick(slf: &Bound<'_, Self>, carry: bool) -> R<()> {
        let py = slf.py();
        let pls: Vec<Bound<'_, PyAny>> = Self::g_planets(slf)?.iter()?.collect::<R<_>>()?;
        if !pls.iter().any(|p| pla(p).map(|b| b.borrow().orbit.is_some()).unwrap_or(false)) {
            return Ok(());
        }
        let time = slf.borrow().time;
        let old: PData = Self::planet_data(slf)?;
        let n = pls.len();
        let mut st: Vec<(V3, V3, V3)> = Vec::with_capacity(n);     // 新しい (中心, 速度, 加速度)
        let mut delta = vec![[0.0; 3]; n];
        for (i, pl) in pls.iter().enumerate() {
            let o = pla(pl)?.borrow().orbit;
            let s = match o {
                Some(o) if o.parent < i => {
                    let (pc, pv, pa) = st[o.parent];
                    let (r, v, a) = o.at(time);
                    (add3(pc, r), add3(pv, v), add3(pa, a))
                }
                _ => (old[i].0, [0.0; 3], [0.0; 3]),
            };
            delta[i] = sub(s.0, old[i].0);
            if o.is_some() {
                {
                    let mut b = pla(pl)?.borrow_mut();
                    b.ovel = s.1;
                    b.oacc = s.2;
                }
                Planet::move_center(pla(pl)?, s.0)?;
            }
            st.push(s);
        }
        if !carry {
            return Ok(());
        }
        // MOD の景色の箱（月の港など）
        for a in Self::g_addons(slf)?.iter()? {
            let a = a?;
            if let Ok(sc) = a.getattr("scenes") {
                for s in sc.iter()? {
                    let s = s?;
                    if let Ok(s) = s.downcast::<crate::game::scenery::Scenery>() {
                        crate::game::scenery::Scenery::follow(s)?;
                    }
                }
            }
        }
        let vel_of = |f: i64| -> V3 { if f >= 0 { st[f as usize].1 } else { [0.0; 3] } };
        // 構築物
        for c in Self::cons_v(slf)? {
            let cc = con(&c)?;
            if Construct::n(cc)? == 0 {
                continue;
            }
            let p0 = { let p = Construct::g_pos(cc)?; v3(&p.get_item(0)?)? };
            let f = frame_index(&old, p0).map(|i| i as i64).unwrap_or(NO_FRAME);
            let pf = cc.borrow().frame;
            if pf != UNKNOWN && pf != f {
                let dv = sub(vel_of(pf), vel_of(f));
                Self::con_add_vel(cc, dv)?;
            }
            cc.borrow_mut().frame = f;
            if f >= 0 {
                let d = delta[f as usize];
                if d != [0.0; 3] {
                    Self::con_translate(cc, d)?;
                }
            }
        }
        // プレイヤー（ネットの相手も）
        for p in Self::all_players(slf)? {
            if p.is_none() {
                continue;
            }
            let pos = p.getattr("pos")?;
            let p0 = v3(&pos)?;
            let f = frame_index(&old, p0).map(|i| i as i64).unwrap_or(NO_FRAME);
            let pf: i64 = p.getattr("frame").and_then(|x| x.extract()).unwrap_or(UNKNOWN);
            if pf != UNKNOWN && pf != f {
                let dv = sub(vel_of(pf), vel_of(f));
                if let Ok(v) = p.getattr("vel") {
                    if let Some(nv) = shift_rows(py, &v, dv)? {
                        p.setattr("vel", nv)?;
                    }
                }
            }
            let _ = p.setattr("frame", f);
            if f >= 0 {
                let d = delta[f as usize];
                if let Some(np) = shift_rows(py, &pos, d)? {
                    p.setattr("pos", np)?;
                }
            }
        }
        Ok(())
    }

    /// 構築物を d だけ平行に動かす（頂点・前の位置・剛体の重心・関節の部分の重心）
    pub(crate) fn con_translate(c: &Bound<'_, Construct>, d: V3) -> R<()> {
        let py = c.py();
        let pos = Construct::g_pos(c)?;
        if let Some(a) = shift_rows(py, &pos, d)? {
            Construct::s_pos(c, a)?;
        }
        let prev = c.borrow().prev.bind(py).clone().into_any();
        if prev.len().unwrap_or(0) > 0 {
            if let Some(a) = shift_rows(py, &prev, d)? {
                c.borrow_mut().prev = a.extract::<crate::game::construct::FA>()?;
            }
        }
        let rb = c.borrow().rb.as_ref().map(|r| r.clone_ref(py));
        if let Some(rb) = rb {
            let x = rb.borrow(py).x.bind(py).clone().into_any();
            if let Some(a) = shift_rows(py, &x, d)? {
                rb.borrow_mut(py).x = a.extract::<crate::game::construct::FA>()?;
            }
        }
        let art = c.borrow().art.as_ref().map(|r| r.clone_ref(py));
        if let Some(art) = art {
            let x = art.borrow(py).x.bind(py).clone().into_any();
            if let Some(a) = shift_rows(py, &x, d)? {
                art.borrow_mut(py).x = a.extract::<crate::game::construct::FA>()?;
            }
        }
        // 眠っている物の描画の基準は位置から取り直す
        c.borrow_mut().cache.sleep_anchor = None;
        Ok(())
    }

    /// 構築物の速度に dv を足す（影響圏を出入りしたとき）
    pub(crate) fn con_add_vel(c: &Bound<'_, Construct>, dv: V3) -> R<()> {
        let py = c.py();
        let vel = Construct::g_vel(c)?;
        if let Some(a) = shift_rows(py, &vel, dv)? {
            Construct::s_vel(c, a)?;
        }
        let rb = c.borrow().rb.as_ref().map(|r| r.clone_ref(py));
        if let Some(rb) = rb {
            let v = rb.borrow(py).v.bind(py).clone().into_any();
            if let Some(a) = shift_rows(py, &v, dv)? {
                rb.borrow_mut(py).v = a.extract::<crate::game::construct::FA>()?;
            }
        }
        let mut b = c.borrow_mut();
        b.asleep = false;
        b.sleep_epoch += 1;
        Ok(())
    }
}
