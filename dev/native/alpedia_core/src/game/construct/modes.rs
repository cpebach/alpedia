//! 構築物：動き方の切り替え（剛体・関節・レール・乗り物の枠）

use crate::game::world::{wld, World};
use super::*;

#[pymethods]
impl Construct {
    // ------------------------------------------------------------------ 軽量化 1：剛体化
    /// 剛体化してよい構築物か（液体・粘土など柔らかい素材、動作中のアクチュエーターは不可）
    pub(crate) fn _rigid_allowed(slf: &Bound<'_, Self>) -> R<bool> {
        if Self::n(slf)? < 2 {
            return Ok(false);
        }
        if fv(&Self::g_v_stiff(slf)?)?.iter().any(|x| *x < 0.5) || bv(&Self::g_v_rebond(slf)?)?.iter().any(|x| *x) {
            return Ok(false);
        }
        let reg = Self::g_reg(slf)?;
        for d in Self::g_devs(slf)?.iter()? {
            let d = d?;
            if d.is_truthy()? && dev_type(&reg, &d)?.as_deref() == Some("actuator") && crate::game::actuator::act_busy(&d.get_item("state")?)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// 重力以外の力による全体の加速度 (m/s²)
    pub(crate) fn body_acceleration(slf: &Bound<'_, Self>) -> R<f64> {
        let f = v3s(&Self::g_force(slf)?)?;
        let mut s = [0.0; 3];
        for q in &f {
            for k in 0..3 {
                s[k] += q[k];
            }
        }
        Ok(norm(s) / pw_sum(&Self::mass_v(slf)?).max(1e-9))
    }
    /// (重心, 重心速度, 角速度, 慣性テンソル, 重心からの位置)
    pub(crate) fn _rigid_motion<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let rm = Self::rigid_motion(slf)?;
        Ok(PyTuple::new_bound(py, [a1(py, rm.x), a1(py, rm.v), a1(py, rm.w), mat(py, &rm.i)?, a3(py, &rm.r)?]))
    }
    /// 形がほとんど変わっていない（剛体として動いている）か
    pub(crate) fn calm_for_rigid(slf: &Bound<'_, Self>) -> R<bool> {
        let rm = Self::rigid_motion(slf)?;
        let vel = Self::vel_v(slf)?;
        let mut dmax = 0.0f64;
        let mut ext2 = f64::NEG_INFINITY;
        for (q, r) in rm.r.iter().enumerate() {
            let d = sub(vel[q], add(rm.v, cross(rm.w, *r)));
            for k in 0..3 {
                dmax = dmax.max(d[k].abs());
            }
            ext2 = ext2.max(dot(*r, *r));
        }
        let ext = ext2.sqrt();
        let c = |k: &str| -> R<f64> { slf.get_type().getattr(k)?.extract() };
        let acc_in = c("RIGID_ACC_IN")?;
        Ok(dmax < c("RIGID_DEFORM_V")? && dot(rm.w, rm.w) * ext < acc_in && Self::body_acceleration(slf)? < acc_in)
    }
    pub(crate) fn enter_rigid(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let rm = Self::rigid_motion(slf)?;
        let rb = Py::new(py, RigidState::new(py, &rm.r, &eye(), rm.x, rm.v, rm.w, &rm.i)?)?;
        let vel: Vec<V3> = rm.r.iter().map(|r| add(rm.v, cross(rm.w, *r))).collect();
        Self::s_vel(slf, a3(py, &vel)?)?;
        let mut b = slf.borrow_mut();
        b.rb = Some(rb);
        b.mode = "rigid".into();
        b.rb_epoch += 1;
        b.want_soft = false;
        Ok(())
    }
    pub(crate) fn leave_rigid(slf: &Bound<'_, Self>) -> R<()> {
        let mut b = slf.borrow_mut();
        b.mode = "soft".into();
        b.rb = None;
        b.calm_t = 0.0;
        b.want_soft = false;
        Ok(())
    }

    /// 毎ティック：ブロックごと ⇔ 剛体 ⇔ レール（遠方の軌道計算）を切り替える
    pub(crate) fn update_mode(slf: &Bound<'_, Self>, dt: f64, world: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        if Self::n(slf)? == 0 || slf.borrow().asleep {
            return Ok(());
        }
        let phys = World::g_cfg(wld(&world)?)?.call_method1("get", ("physics", PyDict::new_bound(py)))?;
        // 完全剛体モードでも soft_ok の構築物（物語の見せ場の落石・木箱など）は、曲がる・ちぎれる
        let mut full = World::g_settings(wld(&world)?)?.call_method1("get", ("full_rigid", false))?.is_truthy()?
            && !Self::t_soft_ok(slf)?;
        if Self::anchored_b(slf)? {          // アンカーで固定：柔らかいブロックのモードでも剛体（＋関節）として扱う
            full = true;
        }
        Self::s_full_rigid(slf, full)?;
        if full {
            return Self::_update_full_rigid(slf, world, &phys, dt);
        }
        if Self::mode_is(slf, &["art"]) {       // 完全剛体モードを切ったとき
            slf.borrow_mut().mode = "soft".into();
            slf.try_borrow_mut()?.art = None;
        }
        let c = |k: &str| -> R<f64> { slf.get_type().getattr(k)?.extract() };
        let acc = Self::body_acceleration(slf)?;
        let pg = |k: &str, d: bool| -> R<bool> { phys.call_method1("get", (k, d))?.is_truthy() };
        let rails_d: f64 = phys.call_method1("get", ("rails_distance", 400.0))?.extract()?;
        if Self::mode_is(slf, &["rigid", "rails"]) {
            let mut out = slf.borrow().want_soft || acc > c("RIGID_ACC_OUT")? || !Self::_rigid_allowed(slf)?;
            if !out && Self::mode_is(slf, &["rigid"]) {
                let rm = Self::rigid_motion(slf)?;
                let ext: f64 = Self::rb(slf)?.map(|r| r.borrow(py).ext).unwrap_or(0.0);
                out = dot(rm.w, rm.w) * ext > c("RIGID_ACC_OUT")?;
            }
            if out {
                if Self::mode_is(slf, &["rails"]) {
                    Self::leave_rails(slf)?;
                }
                return Self::leave_rigid(slf);
            }
        }
        if Self::mode_is(slf, &["soft"]) {
            if !pg("rigid_enabled", true)? || slf.borrow().want_soft {
                let mut b = slf.borrow_mut();
                b.want_soft = false;
                b.calm_t = 0.0;
                return Ok(());
            }
            if Self::_rigid_allowed(slf)? && Self::calm_for_rigid(slf)? {
                slf.borrow_mut().calm_t += dt;
                if slf.borrow().calm_t >= c("RIGID_CALM_T")? {
                    Self::enter_rigid(slf)?;
                }
            } else {
                slf.borrow_mut().calm_t = 0.0;
            }
        }
        // 軽量化 2：プレイヤーから遠い構築物は「レール」に乗せる
        if Self::mode_is(slf, &["rigid"]) && pg("rails_enabled", true)? {
            if Self::_rails_ok(slf, world, rails_d)? {
                Self::enter_rails(slf)?;
            }
        } else if Self::mode_is(slf, &["rails"]) && !Self::_rails_ok(slf, world, rails_d * 0.9)? {
            Self::leave_rails(slf)?;
        }
        Ok(())
    }

    /// 機器のあるブロックの番号（kind が種類なら、その種類だけ）。形が変わったときだけ数え直す
    #[pyo3(signature = (kind=None))]
    pub(crate) fn dev_ids<'py>(slf: &Bound<'py, Self>, kind: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let key = Self::vkey(slf)?;
        let by = match Self::cache_hit(slf, |c| &c.devid, key.as_any())? {
            Some(c) => c.get_item(1)?,
            None => {
                let by = PyDict::new_bound(py);
                let all = PyList::empty_bound(py);
                by.set_item(py.None(), &all)?;
                let reg = Self::g_reg(slf)?;
                for (i, d) in Self::g_devs(slf)?.iter()?.enumerate() {
                    let d = d?;
                    if d.is_truthy()? {
                        all.append(i)?;
                        let t = dev_type(&reg, &d)?.into_py(py);
                        by.call_method1("setdefault", (t, PyList::empty_bound(py)))?.call_method1("append", (i,))?;
                    }
                }
                Self::cache_put(slf, |c| &mut c.devid, (key, &by))?;
                by.into_any()
            }
        };
        by.call_method1("get", (kind.map(|k| k.clone().unbind()).unwrap_or_else(|| py.None()), PyTuple::empty_bound(py)))
    }
    /// アクチュエーター（ヒンジなども）の番号（形が変わったときだけ数え直す）
    pub(crate) fn actuator_indices<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let key = Self::vkey(slf)?;
        if let Some(c) = Self::cache_hit(slf, |c| &c.act, key.as_any())? {
            return c.get_item(1);
        }
        let reg = Self::g_reg(slf)?;
        let out = PyList::empty_bound(py);
        for (i, d) in Self::g_devs(slf)?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? && dev_type(&reg, &d)?.as_deref() == Some("actuator") {
                out.append(i)?;
            }
        }
        Self::cache_put(slf, |c| &mut c.act, (key, &out))?;
        Ok(out.into_any())
    }
    pub(crate) fn _actuator_busy(slf: &Bound<'_, Self>) -> R<bool> {
        let devs = Self::g_devs(slf)?;
        for i in Self::actuator_indices(slf)?.iter()? {
            if crate::game::actuator::act_busy(&devs.get_item(i?)?.get_item("state")?)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 完全剛体モード：構築物は丸ごと1つの剛体。アクチュエーターが動いている間は部分ごとの剛体を関節でつないで動かす（mode "art"）
    #[pyo3(signature = (world, phys, dt=0.0))]
    pub(crate) fn _update_full_rigid(slf: &Bound<'_, Self>, world: &Bound<'_, PyAny>, phys: &Bound<'_, PyAny>, dt: f64) -> R<()> {
        slf.borrow_mut().want_soft = false;          // 衝突などによる「ブロックごとに戻す」要求は無視する
        if Self::_actuator_busy(slf)? {
            Self::cache_put(slf, |c| &mut c.art_hold, slf.get_type().getattr("ART_HOLD")?)?;
            if Self::mode_is(slf, &["rails"]) {
                Self::leave_rails(slf)?;
            }
            if !Self::mode_is(slf, &["art"]) {
                Self::enter_art(slf)?;
            }
            return Ok(());
        }
        if Self::mode_is(slf, &["art"]) {
            let hold: f64 = match Self::cache_get(slf, |c| &c.art_hold)? { Some(v) => v.extract()?, None => 0.0 };
            if hold > 0.0 {
                Self::cache_put(slf, |c| &mut c.art_hold, hold - dt)?;
                return Ok(());
            }
        }
        if Self::mode_is(slf, &["soft", "art"]) {
            Self::enter_rigid(slf)?;
        }
        if Self::anchored_b(slf)? {
            if Self::mode_is(slf, &["rails"]) {
                Self::leave_rails(slf)?;
            }
            return Ok(());
        }
        let rails_d: f64 = phys.call_method1("get", ("rails_distance", 400.0))?.extract()?;
        if Self::mode_is(slf, &["rigid"]) && phys.call_method1("get", ("rails_enabled", true))?.is_truthy()? {
            if Self::_rails_ok(slf, world, rails_d)? {
                Self::enter_rails(slf)?;
            }
        } else if Self::mode_is(slf, &["rails"]) && !Self::_rails_ok(slf, world, rails_d * 0.9)? {
            Self::leave_rails(slf)?;
        }
        Ok(())
    }

    pub(crate) fn _art_groups<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let g = Self::art_groups(slf)?;
        let n = g.len();
        iarr(slf.py(), g.into_iter().map(|x| x as i64).collect(), &[n])
    }

    pub(crate) fn _art_build(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let gid = Self::art_groups(slf)?;
        let gn = gid.iter().max().map(|x| x + 1).unwrap_or(0);
        let vg: Vec<usize> = gid.iter().flat_map(|g| std::iter::repeat(*g).take(8)).collect();
        let m = Self::mass_v(slf)?;
        let pos = Self::pos_v(slf)?;
        let mut mm = vec![0.0; gn];
        let mut x = vec![[0.0; 3]; gn];
        for (q, &g) in vg.iter().enumerate() {
            mm[g] += m[q];
        }
        for k in 0..3 {
            for (q, &g) in vg.iter().enumerate() {
                x[g][k] += m[q] * pos[q][k];
            }
        }
        for g in 0..gn {
            for k in 0..3 {
                x[g][k] /= mm[g];
            }
        }
        let vl: Vec<V3> = pos.iter().zip(&vg).map(|(p, &g)| sub(*p, x[g])).collect();
        let mut ib = vec![[[0.0; 3]; 3]; gn];
        for i in 0..3 {
            for j in 0..3 {
                for (q, &g) in vg.iter().enumerate() {
                    let rr = dot(vl[q], vl[q]);
                    ib[g][i][j] += m[q] * ((if i == j { rr } else { 0.0 }) - vl[q][i] * vl[q][j]);
                }
            }
        }
        let iinv: Vec<[[f64; 3]; 3]> = ib.iter().map(|b| {
            let tr = b[0][0] + b[1][1] + b[2][2];
            let e = 1e-6 * tr.max(1e-6);
            let mut a = *b;
            for k in 0..3 {
                a[k][k] += e;
            }
            inv3(&a)
        }).collect();
        let reg = Self::g_reg(slf)?;
        let exts = PyList::empty_bound(py);
        for (i, d) in Self::g_devs(slf)?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? && crate::game::actuator::act_is_actuator(slf.as_any(), i)? {
                let dir: i64 = dget(&d, "dir", 2)?.call_method0("__int__")?.extract()?;
                let jp = joint_pairs(dir.rem_euclid(6) as usize);
                let rows: Vec<i64> = jp.iter().map(|p| (8 * i + p.0) as i64).collect();
                let face: Vec<V3> = rows.iter().map(|&r| pos[r as usize]).collect();
                let mut nb = sub(mean8(&face), mean8(&pos[8 * i..8 * i + 8]));
                let l = norm(nb).max(1e-9);
                nb = [nb[0] / l, nb[1] / l, nb[2] / l];
                let e0: f64 = d.get_item("state")?.call_method1("get", ("ext", 0.0))?.extract()?;
                exts.append((i, iarr(py, rows, &[4])?, a1(py, nb), e0))?;
            }
        }
        // アンカーのある部分は固定（動かない）。固定どうしをつなぐアクチュエーターは動かせない
        let mut fixed = vec![false; gn];
        for a in iv(&Self::anchor_index(slf)?)? {
            fixed[gid[a as usize]] = true;
        }
        let (mut ra, mut rbv, mut av, mut bvv, mut cv, mut hk) = (vec![], vec![], vec![], vec![], vec![], vec![]);
        let devs = PyList::empty_bound(py);
        let mut applied = vec![];
        for r in Self::g_rotors(slf)?.iter()? {
            let r = r?;
            let ri: usize = r.get_item("a")?.extract()?;
            let rj: usize = r.get_item("b")?.extract()?;
            let (a, b) = (gid[ri], gid[rj]);
            if a == b || (fixed[a] && fixed[b]) {
                continue;                // 輪になっていて回せない（固定されたまま）
            }
            let d: usize = r.get_item("d")?.extract()?;
            ra.push(a as i64);
            rbv.push(b as i64);
            for (ca, cb) in joint_pairs(d) {
                av.push((8 * ri + ca) as i64);
                bvv.push((8 * rj + cb) as i64);
            }
            cv.extend((8 * ri..8 * ri + 8).map(|x| x as i64));
            let dv = r.get_item("dev")?;
            applied.push(dv.get_item("state")?.call_method1("get", ("angle", 0.0))?.extract::<f64>()?);
            match crate::game::actuator::act_hinge_of(&dv, Some(&reg))? {
                Some((p, q)) => hk.extend([p as i64, q as i64]),
                None => hk.extend([-1, -1]),
            }
            devs.append(dv)?;
        }
        let nr = ra.len();
        let nv = vg.len();
        let invm: Vec<f64> = mm.iter().map(|m| 1.0 / m).collect();
        let flat = |v: &[[[f64; 3]; 3]]| -> Vec<f64> { v.iter().flat_map(|m| m.iter().flatten().copied().collect::<Vec<_>>()).collect() };
        let ext_applied: Vec<f64> = exts.iter().map(|e| e.get_item(3)?.extract::<f64>()).collect::<R<_>>()?;
        let ne = ext_applied.len();
        let any_fixed = fixed.iter().any(|x| *x);
        let flat3 = |v: &[V3]| -> Vec<f64> { v.iter().flatten().copied().collect() };
        let art = ArtState {
            version: Self::ver(slf),
            n: Self::n(slf)? as i64,
            gid: ia(py, gid.iter().map(|x| *x as i64).collect(), &[gid.len()])?,
            vg: ia(py, vg.iter().map(|x| *x as i64).collect(), &[nv])?,
            g: gn as i64,
            vl0: fa(py, flat3(&vl), &[nv, 3])?,
            vl: fa(py, flat3(&vl), &[nv, 3])?,
            inv_m: fa(py, invm.clone(), &[gn])?,
            iinv_b: fa(py, flat(&iinv), &[gn, 3, 3])?,
            ib: fa(py, flat(&ib), &[gn, 3, 3])?,
            x: fa(py, flat3(&x), &[gn, 3])?,
            r: fa(py, flat(&vec![eye(); gn]), &[gn, 3, 3])?,
            exts: exts.unbind(),
            ra: ia(py, ra, &[nr])?,
            rb: ia(py, rbv, &[nr])?,
            a: ia(py, av, &[nr, 4])?,
            b: ia(py, bvv, &[nr, 4])?,
            c: ia(py, cv, &[nr, 8])?,
            devs: devs.unbind(),
            applied: fa(py, applied, &[nr])?,
            ext_applied: fa(py, ext_applied, &[ne])?,
            gv: ia(py, (0..nv as i64).collect(), &[nv])?,
            hk: ia(py, hk, &[nr, 2])?,
            fixed: PyArray1::from_vec_bound(py, fixed.clone()).reshape(IxDyn(&[gn]))?.unbind(),
            // 固定の部分は質量・慣性を無限大として解く（関節は動く側だけを動かす）。今の姿勢をそのまま保つ
            inv_m_s: if any_fixed {
                Some(fa(py, invm.iter().zip(&fixed).map(|(v, f)| if *f { 0.0 } else { *v }).collect(), &[gn])?)
            } else { None },
            iinv_b_s: if any_fixed {
                let z: Vec<[[f64; 3]; 3]> = iinv.iter().zip(&fixed).map(|(v, f)| if *f { [[0.0; 3]; 3] } else { *v }).collect();
                Some(fa(py, flat(&z), &[gn, 3, 3])?)
            } else { None },
            xfix: if any_fixed { Some(fa(py, flat3(&x), &[gn, 3])?) } else { None },
            nat: py.None(),
            defo: py.None(),
            contacts: py.None(),
        };
        slf.try_borrow_mut()?.art = Some(Py::new(py, art)?);
        let nat = Self::_native_art_topo(slf)?.unbind();
        Self::art_req(slf)?.borrow_mut(py).nat = nat;
        Ok(())
    }

    pub(crate) fn enter_art(slf: &Bound<'_, Self>) -> R<()> {
        Self::_art_build(slf)?;
        let mut b = slf.borrow_mut();
        b.mode = "art".into();
        b.rb = None;
        b.want_soft = false;
        Ok(())
    }

    // ------------------------------------------------------------------ 軽量化 2：レール
    pub(crate) fn _rails_ok(slf: &Bound<'_, Self>, world: &Bound<'_, PyAny>, dist: f64) -> R<bool> {
        let py = slf.py();
        let rb = Self::rb(slf)?;
        let x = match &rb { None => Self::center_of_mass(slf)?, Some(r) => r.borrow(py).x.bind(py).clone().into_any() };
        if world.call_method1("player_distance", (&x,))?.extract::<f64>()? < dist {
            return Ok(false);
        }
        if fv(&Self::g_force(slf)?)?.iter().any(|v| *v != 0.0) {
            return Ok(false);
        }
        // 地面の近くでは当たり判定が必要なのでレールに乗せない
        let pl = world.call_method1("nearest_planet", (&x,))?;
        if !pl.is_none() {
            let xv = v3(&x)?;
            let alt: f64 = pl.call_method1("sdf", (arr(py, xv.to_vec(), &[1, 3])?,))?.get_item(0)?.extract()?;
            let ext: f64 = match &rb { None => 10.0, Some(r) => r.borrow(py).ext };
            if alt < ext + 15.0 {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn enter_rails(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        slf.borrow_mut().mode = "rails".into();
        let rm = Self::rigid_motion(slf)?;
        if let Some(rb) = Self::rb(slf)? {
            let mut r = rb.borrow_mut(py);
            r.x = v3a(py, rm.x)?;
            r.v = v3a(py, rm.v)?;
            r.w = v3a(py, rm.w)?;
        }
        Self::_rails_sync(slf)
    }
    pub(crate) fn leave_rails(slf: &Bound<'_, Self>) -> R<()> {
        Self::_rails_sync(slf)?;
        slf.borrow_mut().mode = "rigid".into();
        Ok(())
    }
    /// 重心は重力だけで（速度ベルレ法）、回転は一定の角速度で進める
    #[pyo3(signature = (dt, gravity_fn, sub=4))]
    pub(crate) fn rails_step(slf: &Bound<'_, Self>, dt: f64, gravity_fn: &Bound<'_, PyAny>, sub: i64) -> R<()> {
        let py = slf.py();
        let rb = Self::rb_req(slf)?;
        let h = dt / sub as f64;
        let mut x = fa_v3(py, &rb.borrow(py).x)?;
        let mut v = fa_v3(py, &rb.borrow(py).v)?;
        let grav = |x: V3| -> R<V3> { v3(&gravity_fn.call1((arr(py, x.to_vec(), &[1, 3])?,))?.get_item(0)?) };
        let mut a = grav(x)?;
        for _ in 0..sub {
            v = add(v, scl(a, h * 0.5));
            x = add(x, scl(v, h));
            a = grav(x)?;
            v = add(v, scl(a, h * 0.5));
        }
        rb.borrow_mut(py).x = v3a(py, x)?;
        rb.borrow_mut(py).v = v3a(py, v)?;
        let w = fa_v3(py, &rb.borrow(py).w)?;
        let wn = norm(w);
        if wn > 1e-9 {
            let k = [w[0] / wn, w[1] / wn, w[2] / wn];
            let th = wn * dt;
            let kx = [[0.0, -k[2], k[1]], [k[2], 0.0, -k[0]], [-k[1], k[0], 0.0]];
            let kk = matmul(&kx, &kx);
            let mut rw = eye();
            for i in 0..3 {
                for j in 0..3 {
                    rw[i][j] = rw[i][j] + th.sin() * kx[i][j] + (1.0 - th.cos()) * kk[i][j];
                }
            }
            let r0 = fa_m3(py, &rb.borrow(py).r)?;
            rb.borrow_mut(py).r = m3a(py, &matmul(&rw, &r0))?;
        }
        Self::_rails_sync(slf)
    }
    pub(crate) fn _rails_sync(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let rb = Self::rb_req(slf)?;
        let (r, x, v, w, loc) = {
            let b = rb.borrow(py);
            (fa_m3(py, &b.r)?, fa_v3(py, &b.x)?, fa_v3(py, &b.v)?, fa_v3(py, &b.w)?, fa_v3s(py, &b.local)?)
        };
        let rr: Vec<V3> = loc.iter().map(|l| mul_t(*l, &r)).collect();
        Self::s_pos(slf, a3(py, &rr.iter().map(|q| add(x, *q)).collect::<Vec<_>>())?)?;
        Self::s_vel(slf, a3(py, &rr.iter().map(|q| add(v, cross(w, *q))).collect::<Vec<_>>())?)
    }

    // ------------------------------------------------------------------ 外から位置を決める（拡張が、動く物体に固定した構築物を一緒に動かす）
    /// 座標系 (R, x) に今の形のまま固定する（形が変わったときだけ取り直す）。物理計算は止まる（mode="rails"）
    pub(crate) fn hold_in_frame(slf: &Bound<'_, Self>, r: &Bound<'_, PyAny>, x: &Bound<'_, PyAny>, v: &Bound<'_, PyAny>, w: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let f = Self::cache_get(slf, |c| &c.frame_hold)?;
        let fresh = match &f {
            None => true,
            Some(f) => !f.get_item("ver")?.eq(Self::ver(slf))? || slf.borrow().rb.is_none() || !Self::mode_is(slf, &["rails"]),
        };
        if fresh {
            let rm = m3(r)?;
            let xv = v3(x)?;
            let local: Vec<V3> = Self::pos_v(slf)?.iter().map(|p| mul_r(sub(*p, xv), &rm)).collect();
            let fh = PyDict::new_bound(py);
            fh.set_item("ver", Self::ver(slf))?;
            Self::cache_put(slf, |c| &mut c.frame_hold, fh)?;
            let rb = Py::new(py, RigidState::new(py, &local, &rm, xv, v3(v)?, v3(w)?, &eye())?)?;
            let mut b = slf.borrow_mut();
            b.mode = "rails".into();
            b.rb = Some(rb);
            b.rb_epoch += 1;
        }
        Ok(())
    }
    /// 固定した座標系が (R, x, 速度 v, 角速度 w) に動いた：一緒に動かす
    pub(crate) fn move_with_frame(slf: &Bound<'_, Self>, r: &Bound<'_, PyAny>, x: &Bound<'_, PyAny>, v: &Bound<'_, PyAny>, w: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        Self::hold_in_frame(slf, r, x, v, w)?;
        {
            let rb = Self::rb_req(slf)?;
            let mut b = rb.borrow_mut(py);
            b.r = m3a(py, &m3(r)?)?;
            b.x = v3a(py, v3(x)?)?;
            b.v = v3a(py, v3(v)?)?;
            b.w = v3a(py, v3(w)?)?;
        }
        Self::_rails_sync(slf)?;
        let mut b = slf.borrow_mut();
        b.asleep = false;
        b.sleep_epoch += 1;
        Ok(())
    }
    /// 固定をやめて、ふつうの物理計算に戻す
    pub(crate) fn release_frame(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        Self::cache_put(slf, |c| &mut c.frame_hold, py.None())?;
        let mut b = slf.borrow_mut();
        if b.mode == "rails" {
            b.mode = "soft".into();
            b.rb = None;
        }
        Ok(())
    }
    /// 描画用：剛体・レール・静止中なら (R, x, 版)。ブロックごとに動いている間は None
    pub(crate) fn render_transform<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        if let (true, Some(rb)) = (Self::mode_is(slf, &["rigid", "rails"]), Self::rb(slf)?) {
            let tag: PyObject = ("rb", slf.borrow().rb_epoch, Self::ver(slf)).into_py(py);
            let b = rb.borrow(py);
            return Ok(PyTuple::new_bound(py, [b.r.bind(py).clone().into_any(), b.x.bind(py).clone().into_any(), tag.into_bound(py)]).into_any());
        }
        if slf.borrow().asleep {
            let ep = slf.borrow().sleep_epoch;
            let ok = match Self::cache_get(slf, |c| &c.sleep_anchor)? {
                Some(a) => a.get_item(0)?.eq(ep)?,
                None => false,
            };
            if !ok {
                let p0 = Self::pos_v(slf)?.first().copied().unwrap_or([0.0; 3]);
                Self::cache_put(slf, |c| &mut c.sleep_anchor, (ep, a1(py, p0)))?;
            }
            let anc = Self::cache_get(slf, |c| &c.sleep_anchor)?.ok_or_else(|| PyAttributeError::new_err("_sleep_anchor"))?.get_item(1)?;
            let tag: PyObject = ("sleep", ep, Self::ver(slf)).into_py(py);
            return Ok(PyTuple::new_bound(py, [mat(py, &eye())?, anc, tag.into_bound(py)]).into_any());
        }
        Ok(py.None().into_bound(py))
    }
}
