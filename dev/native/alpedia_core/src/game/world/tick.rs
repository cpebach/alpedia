//! ワールド：物理の中身（並べて動かす・ぶつかり・眠り・遠くの物）

use crate::game::player::{ply, Player};
use super::*;
use crate::game::construct::{con, Construct};
use numpy::PyUntypedArrayMethods;

#[pymethods]
impl World {
    /// このティックで当たりそうな、拡張の当たり判定の箱を集めておく
    pub(crate) fn _prep_ext(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, dt: f64) -> R<()> {
        let py = slf.py();
        Construct::s_ext_events(con(&c)?, PyList::empty_bound(py))?;
        Construct::s_ext_colliders(con(&c)?, PyList::empty_bound(py))?;
        let addons = Self::addons_v(slf)?;
        if addons.is_empty() {
            return Ok(());
        }
        let vel = fv(&Construct::g_vel(con(&c)?)?)?;
        let vmax = vel.iter().map(|x| x.abs()).fold(f64::NEG_INFINITY, |a, b| if b.is_nan() || a.is_nan() { f64::NAN } else { a.max(b) });
        let sp = vmax * dt + Construct::g_size(con(&c)?)?.extract::<f64>()? * 2.0;
        let pos = v3s(&Construct::g_pos(con(&c)?)?)?;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let (lo, hi) = (a1(py, lo), a1(py, hi));
        let ec = Construct::g_ext_colliders(con(&c)?)?;
        for a in addons {
            ec.call_method1("extend", (a.call_method1("colliders", (slf, &lo, &hi, sp))?,))?;
        }
        Ok(())
    }
    /// 拡張の物体（街の建物など）から球を押し出す：[(押し出し, 法線, owner)]
    pub(crate) fn push_sphere<'py>(slf: &Bound<'py, Self>, center: &Bound<'py, PyAny>, r: f64) -> R<Bound<'py, PyList>> {
        let out = PyList::empty_bound(slf.py());
        for a in Self::addons_v(slf)? {
            for x in a.call_method1("push_sphere", (slf, center, r))?.iter()? {
                out.append(x?)?;
            }
        }
        Ok(out)
    }
    /// 構築物ごとの計算を、複数の CPU コアで同時に進める（Rust の計算は GIL を離すので並列に動く）
    pub(crate) fn _each(slf: &Bound<'_, Self>, cons: Vec<Bound<'_, PyAny>>, f: &Bound<'_, PyAny>) -> R<Vec<PyObject>> {
        let py = slf.py();
        let workers = Self::_workers(slf)?;
        if workers <= 1 || cons.len() < 2 {
            return cons.iter().map(|c| f.call1((c,)).map(|x| x.unbind())).collect();
        }
        let pmin: usize = crate::game::construct::PARALLEL_MIN_VOXELS;
        let mut out: Vec<Option<PyObject>> = (0..cons.len()).map(|_| None).collect();
        let mut small = vec![];
        for (k, c) in cons.iter().enumerate() {
            // 剛体・関節つき剛体は Python 側の部分が多く、スレッドに分けても速くならない
            if Construct::n(con(&c)?)? >= pmin || !Construct::g_mode(con(&c)?)?.eq("soft")? {
                out[k] = Some(f.call1((c,))?.unbind());
            } else {
                small.push(k);
            }
        }
        if small.len() == 1 {
            out[small[0]] = Some(f.call1((&cons[small[0]],))?.unbind());
        } else if !small.is_empty() {
            let pool = Self::_pool(slf)?;
            let fo = f.clone().unbind();
            let run = PyCFunction::new_closure_bound(py, None, None, move |a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>| -> R<PyObject> {
                let py = a.py();
                let c = a.get_item(0)?;
                c.setattr("_in_pool", true)?;
                let r = fo.bind(py).call1((&c,));
                c.setattr("_in_pool", false)?;
                r.map(|x| x.unbind())
            })?;
            let items = PyList::new_bound(py, small.iter().map(|&k| cons[k].clone()));
            let res = pool.call_method1("map", (run, items))?;
            for (k, r) in small.iter().zip(res.iter()?) {
                out[*k] = Some(r?.unbind());
            }
        }
        Ok(out.into_iter().map(|x| x.unwrap_or_else(|| py.None())).collect())
    }
    pub(crate) fn _workers(slf: &Bound<'_, Self>) -> R<i64> {
        let py = slf.py();
        let t = getd(&phys(&Self::g_cfg(slf)?)?, "threads", 0)?;
        let n: i64 = if t.is_truthy()? { t.call_method0("__int__")?.extract()? } else { 0 };
        if n > 0 {
            return Ok(n);
        }
        let c = imp(py, "os")?.call_method0("cpu_count")?;
        let c: i64 = if c.is_truthy()? { c.extract()? } else { 2 };
        Ok((c - 1).max(1))
    }
    pub(crate) fn _pool<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        if let Some(p) = Self::cache_get(slf, |c| &c.pool_obj)? {
            return Ok(p);
        }
        let kw = PyDict::new_bound(py);
        kw.set_item("max_workers", Self::_workers(slf)?)?;
        kw.set_item("thread_name_prefix", "phys")?;
        let p = imp(py, "concurrent.futures")?.getattr("ThreadPoolExecutor")?.call((), Some(&kw))?;
        Self::cache_put(slf, |c| &mut c.pool_obj, &p)?;
        Ok(p)
    }
    /// 構築物どうし（と、同じ構築物の結合していないボクセルどうし）の押し出し。押し出された眠っている構築物のリストを返す
    #[pyo3(signature = (active, sleeping=Vec::new()))]
    pub(crate) fn _voxel_collisions<'py>(slf: &Bound<'py, Self>, active: Vec<Bound<'py, PyAny>>, sleeping: Vec<Bound<'py, PyAny>>) -> R<Vec<Bound<'py, PyAny>>> {
        if active.is_empty() {
            return Ok(vec![]);
        }
        let core = imp(slf.py(), "alpedia_core")?;
        Self::_voxel_collisions_native(slf, &core, active, sleeping)
    }
    /// 1ティックの物理（サブステップの繰り返し）を Rust でまとめて進める。(進めたサブステップの数, 起きた物, ちぎれた構築物の ID)。
    /// まとめて進められないとき（動いている街に触れうる物がある）は None
    pub(crate) fn _native_tick<'py>(slf: &Bound<'py, Self>, cons: Vec<Bound<'py, PyAny>>, sleeping: Vec<Bound<'py, PyAny>>, h: f64,
                         anchored_ids: &Bound<'py, PyAny>, settle_acc: f64, fixed_pos: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let none = || py.None().into_bound(py);
        if cons.is_empty() {
            return Ok(none());
        }
        let mut pt = std::time::Instant::now();
        let pls = crate::game::construct::construct_native_planets(py, &Self::g_planets(slf)?)?.into_bound(py);
        let gidx = PyDict::new_bound(py);
        let glst = PyList::empty_bound(py);
        let groups = PyTuple::new_bound(py, [gidx.as_any(), glst.as_any()]);
        use crate::game::construct::Construct;
        let mut jobs = Vec::with_capacity(cons.len());
        for c in &cons {
            let cc = c.downcast::<Construct>()?;
            let mut j = match Construct::tick_job(cc, &groups, true)? {
                Some(j) => j,
                None => return Ok(none()),
            };
            // 気密の空洞の浮力（水のある惑星）。まとめて計算するときも、1つずつ計算するときと同じに
            let cav = Construct::cav_arg(cc, &pls)?;
            j.cav = if cav.is_none() { None } else { Some(cav.extract()?) };
            let me = cc.borrow();
            let ok = me.settle > 0.0 && me.vel.bind(py).shape() == me.pos.bind(py).shape();
            j.settle_cap = if ok { settle_acc * h } else { 0.0 };
            jobs.push(j);
        }
        prof_add(8, &mut pt);
        let fixed: Vec<usize> = fixed_pos.iter()?.map(|x| Ok(x?.get_item(0)?.as_ptr() as usize)).collect::<R<_>>()?;
        let mut sl: Vec<crate::SleeperIn> = Vec::with_capacity(sleeping.len());
        for c in &sleeping {
            let cc = c.downcast::<Construct>()?;
            Construct::_contig(cc, &["pos", "vel"])?;
            let px = Construct::native_proxy(cc)?.extract()?;
            let me = cc.borrow();
            sl.push((px, me.pos.clone_ref(py), me.vel.clone_ref(py), me.mode == "rigid",
                fixed.contains(&(c.as_ptr() as usize)), !anchored_ids.contains(me.id.bind(py))?));
        }
        prof_add(9, &mut pt);
        let gl: Vec<crate::BoxGroupIn> = glst.iter().map(|g| g.extract()).collect::<R<_>>()?;
        let (substeps, iters) = (slf.borrow().substeps, slf.borrow().iters);
        let out = crate::step_tick_native(py, jobs, sl, gl, pls.downcast::<PyList>()?, Self::gscale(slf)?, h, substeps.max(0) as usize,
            iters, crate::game::construct::IMPACT_SPEED, crate::game::construct::CONTACT_SOFT)?;
        let r = IntoPy::<PyObject>::into_py(out, py).into_bound(py);
        prof_add(10, &mut pt);
        let (done, woke, touch, want, broken, hits) = (r.get_item(0)?, r.get_item(1)?, r.get_item(2)?, r.get_item(3)?, r.get_item(4)?, r.get_item(5)?);
        let allc: Vec<&Bound<'_, PyAny>> = cons.iter().chain(sleeping.iter()).collect();
        for ((c, t), w) in allc.iter().zip(touch.iter()?).zip(want.iter()?) {
            if t?.is_truthy()? {
                Construct::s_touch_moving(con(&c)?, true)?;
            }
            if w?.is_truthy()? {
                Construct::s_want_soft(con(&c)?, true)?;
            }
        }
        let broke = PySet::empty_bound(py)?;
        for b in broken.iter()? {
            let b = b?;
            let k: usize = b.get_item(0)?.extract()?;
            cons[k].call_method1("tick_broken", (b.get_item(1)?, b.get_item(2)?))?;
            broke.add(cons[k].getattr("id")?)?;
        }
        if hits.is_truthy()? {
            let owners = PyDict::new_bound(py);
            for c in &cons {
                for e in Construct::g_ext_colliders(con(&c)?)?.iter()? {
                    let e = e?;
                    owners.set_item(e.get_item(1)?.as_ptr() as usize, e.get_item(0)?)?;
                }
            }
            let gowner = PyDict::new_bound(py);
            for (i, k) in gidx.iter() {
                gowner.set_item(k, owners.get_item(i)?)?;
            }
            // (k, g, stp) ごとに {部分: 速さ}
            let mut ev: Vec<((i64, i64, i64), Bound<'_, PyDict>)> = vec![];
            for x in hits.iter()? {
                let x = x?;
                let key: (i64, i64, i64) = (x.get_item(0)?.extract()?, x.get_item(1)?.extract()?, x.get_item(4)?.extract()?);
                let d = match ev.iter().find(|e| e.0 == key) {
                    Some(e) => e.1.clone(),
                    None => {
                        let d = PyDict::new_bound(py);
                        ev.push((key, d.clone()));
                        d
                    }
                };
                d.set_item(x.get_item(2)?.call_method0("__int__")?, x.get_item(3)?.extract::<f64>()?)?;
            }
            ev.sort_by_key(|e| e.0 .2);
            for ((k, g, _), best) in ev {
                cons[k as usize].getattr("ext_events")?.call_method1("append", ((gowner.get_item(g)?, best, py.None(), py.None()),))?;
            }
        }
        for c in &cons {
            if let Ok(c) = c.downcast::<crate::game::construct::Construct>() {
                if crate::game::construct::Construct::mode_is(c, &["art"]) {
                    if let Some(a) = crate::game::construct::Construct::art_state(c)? {
                        let mut b = a.borrow_mut(py);
                        b.defo = PyList::empty_bound(py).into_any().unbind();
                        b.contacts = py.None();
                    }
                }
            }
        }
        let wl = PyList::empty_bound(py);
        for q in woke.iter()? {
            wl.append(&sleeping[q?.extract::<usize>()?])?;
        }
        prof_add(11, &mut pt);
        Ok(PyTuple::new_bound(py, [done, wl.into_any(), broke.into_any()]).into_any())
    }
    /// _voxel_collisions の Rust 版（同じ計算を1回の呼び出しで）
    pub(crate) fn _voxel_collisions_native<'py>(slf: &Bound<'py, Self>, core: &Bound<'py, PyAny>, active: Vec<Bound<'py, PyAny>>,
                                     sleeping: Vec<Bound<'py, PyAny>>) -> R<Vec<Bound<'py, PyAny>>> {
        let py = slf.py();
        let na = active.len();
        let cons: Vec<Bound<'py, PyAny>> = active.into_iter().chain(sleeping).collect();
        let items = PyList::empty_bound(py);
        for c in &cons {
            c.call_method1("_contig", ("pos", "vel"))?;
            items.append((c.call_method0("native_proxy")?, Construct::g_pos(con(&c)?)?, Construct::g_vel(con(&c)?)?, Construct::g_mode(con(&c)?)?.eq("rigid")?))?;
        }
        let r = core.call_method1("contacts", (items, na))?;
        for ((c, t), w) in cons.iter().zip(r.get_item(1)?.iter()?).zip(r.get_item(2)?.iter()?) {
            if t?.is_truthy()? {
                Construct::s_touch_moving(con(&c)?, true)?;
            }
            if w?.is_truthy()? {
                Construct::s_want_soft(con(&c)?, true)?;
            }
        }
        r.get_item(0)?.iter()?.map(|k| Ok(cons[k?.extract::<usize>()?].clone())).collect()
    }
    pub(crate) fn _cleanup(slf: &Bound<'_, Self>, elapsed: f64) -> R<()> {
        let py = slf.py();
        let cl = Self::settings(slf)?.get_item("cleanup")?;
        if !getd(&cl, "enabled", true)?.is_truthy()? {
            return Ok(());
        }
        let constructs = Self::g_constructs(slf)?;
        let items: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)> = constructs.downcast::<PyDict>()?.iter().collect();
        let dist: f64 = cl.get_item("distance")?.extract()?;
        let maxv: i64 = cl.get_item("max_vertices")?.call_method0("__int__")?.extract()?;
        let secs: f64 = cl.get_item("seconds")?.extract()?;
        for (cid, c) in items {
            if Self::seated_in(slf, &c)? {
                Construct::s_far_time(con(&c)?, 0.0)?;
                continue;
            }
            let far = Self::player_distance(slf, &c.call_method0("center_of_mass")?)? > dist;
            if far && !Construct::g_powered(con(&c)?)?.is_truthy()? && c.getattr("n_vert")?.extract::<i64>()? <= maxv {
                let ft: f64 = Construct::g_far_time(con(&c)?)?.extract::<f64>()? + elapsed;
                Construct::s_far_time(con(&c)?, ft)?;
                if ft >= secs {
                    constructs.del_item(&cid)?;
                    log(py)?.call_method1("info", ("自動削除: %s（頂点 %d, 距離 %.0f m）", Construct::g_name(con(&c)?)?, c.getattr("n_vert")?,
                        Self::player_distance(slf, &c.call_method0("center_of_mass")?)?))?;
                }
            } else {
                Construct::s_far_time(con(&c)?, 0.0)?;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------ 問い合わせ
    /// すべての構築物の外側の球（このティックの間は使い回す）：(構築物のリスト, 中心 (n,3), 半径 (n,))
    pub(crate) fn _bounds<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let n = Self::g_constructs(slf)?.len()?;
        let b = match Self::cache_get(slf, |c| &c.bnd)? {
            Some(b) if b.get_item(0)?.eq(n)? => b,
            _ => {
                let cs = PyList::empty_bound(py);
                let (mut c, mut r0, mut rr) = (vec![], vec![], vec![]);
                for x in Self::cons_v(slf)? {
                    if x.getattr("n_vox")?.extract::<usize>()? == 0 {
                        continue;
                    }
                    let bb = x.call_method0("bounds")?;
                    c.extend(v3(&bb.get_item(0)?)?);
                    let r: f64 = bb.get_item(1)?.extract()?;
                    r0.push(r);
                    rr.push(r + x.getattr("size")?.extract::<f64>()?);
                    cs.append(x)?;
                }
                let k = cs.len();
                let b = PyTuple::new_bound(py, [n.into_py(py), cs.into_any().unbind(), arr(py, c, &[k, 3])?.unbind(), arr(py, rr, &[k])?.unbind(),
                    arr(py, r0, &[k])?.unbind()]);
                Self::cache_put(slf, |c| &mut c.bnd, &b)?;
                b.into_any()
            }
        };
        Ok(PyTuple::new_bound(py, [b.get_item(1)?, b.get_item(2)?, b.get_item(3)?]))
    }
    /// 物理のあと：動いた物の外側の球だけ直す（全部を作り直さない）。形が変わった・ちぎれた物があれば作り直す
    pub(crate) fn _patch_bounds(slf: &Bound<'_, Self>, moved: Vec<Bound<'_, PyAny>>, broken: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let b = match Self::cache_get(slf, |c| &c.bnd)? {
            Some(b) if !b.is_none() => b,
            _ => {
                Self::cache_put(slf, |c| &mut c.bnd, py.None())?;
                return Ok(());
            }
        };
        if broken.is_truthy()? || !b.get_item(0)?.eq(Self::g_constructs(slf)?.len()?)? {
            Self::cache_put(slf, |c| &mut c.bnd, py.None())?;
            return Ok(());
        }
        let at = Self::bnd_at(slf, &b.get_item(1)?)?;
        let (c_o, rr_o, r0_o) = (b.get_item(2)?, b.get_item(3)?, b.get_item(4)?);
        for c in moved {
            let k = at.call_method1("get", (c.as_ptr() as usize,))?;
            if k.is_none() || Construct::n(con(&c)?)? == 0 {
                Self::cache_put(slf, |c| &mut c.bnd, py.None())?;
                return Ok(());
            }
            let k: usize = k.extract()?;
            let bb = c.call_method0("bounds")?;
            let ctr = v3(&bb.get_item(0)?)?;
            let r: f64 = bb.get_item(1)?.extract()?;
            let sz: f64 = Construct::g_size(con(&c)?)?.extract()?;
            with_rw(&c_o, |s| s[3 * k..3 * k + 3].copy_from_slice(&ctr))?;
            with_rw(&r0_o, |s| s[k] = r)?;
            with_rw(&rr_o, |s| s[k] = r + sz)?;
        }
        Ok(())
    }
    /// (構築物のリスト, 中心 (n,3), 半径＋大きさ (n,), 半径 (n,))（描画の距離の計算用）
    pub(crate) fn _bounds_arrays<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        Self::_bounds(slf)?;
        let b = Self::cache_get(slf, |c| &c.bnd)?.ok_or_else(|| PyAttributeError::new_err("_bnd"))?;
        Ok(PyTuple::new_bound(slf.py(), [b.get_item(1)?, b.get_item(2)?, b.get_item(3)?, b.get_item(4)?]))
    }
    /// 機能処理の距離（m）。0 なら全部（距離に関係なく処理する）
    pub(crate) fn process_distance(slf: &Bound<'_, Self>) -> R<f64> {
        let v = getd(&phys(&Self::g_cfg(slf)?)?, "process_distance", 0)?;
        Ok(if v.is_truthy()? { v.extract()? } else { 0.0 })
    }
    /// どのプレイヤーからも機能処理の距離より遠い構築物の ID（座っている物・最近いた物は近い扱い）
    pub(crate) fn far_ids<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PySet>> {
        let py = slf.py();
        let lim = Self::process_distance(slf)?;
        let out = PySet::empty_bound(py)?;
        if lim <= 0.0 || Self::g_constructs(slf)?.len()? == 0 {
            return Ok(out);
        }
        let b = Self::_bounds(slf)?;
        let cs: Vec<Bound<'_, PyAny>> = b.get_item(0)?.iter()?.collect::<R<_>>()?;
        if cs.is_empty() {
            return Ok(out);
        }
        let c = v3s(&b.get_item(1)?)?;
        let rr = fv(&b.get_item(2)?)?;
        let players = Self::all_players(slf)?;
        let pp: Vec<V3> = players.iter().map(|p| v3(&p.getattr("pos")?)).collect::<R<_>>()?;
        let seated = PySet::empty_bound(py)?;
        for p in &players {
            let s = Player::g_seated(ply(&p)?)?;
            if s.is_truthy()? {
                seated.add(s.get_item(0)?)?;
            }
        }
        let prev = match Self::cache_get(slf, |c| &c.far_prev)? { Some(p) => p, None => PySet::empty_bound(py)?.into_any() };
        for (k, x) in cs.iter().enumerate() {
            let id = x.getattr("id")?;
            if seated.contains(&id)? {
                continue;
            }
            let d = pp.iter().map(|p| norm(sub(c[k], *p))).fold(f64::INFINITY, f64::min) - rr[k];
            // 境目で行ったり来たりしないように、止まっていた物は少し近づくまで止めたまま
            if d > lim || (prev.contains(&id)? && d > lim * 0.9) {
                out.add(id)?;
            }
        }
        Self::cache_put(slf, |c| &mut c.far_prev, &out)?;
        Ok(out)
    }
    /// 押し出しの相手にする止まっている構築物：動いている物のどれかの外側の球にかかる物だけ
    pub(crate) fn _sleeping_near<'py>(slf: &Bound<'py, Self>, cons: Vec<Bound<'py, PyAny>>, sleeping: Vec<Bound<'py, PyAny>>, dt: f64)
        -> R<Vec<Bound<'py, PyAny>>> {
        if cons.is_empty() || sleeping.is_empty() {
            return Ok(if cons.is_empty() { vec![] } else { sleeping });
        }
        let mut ac = vec![];
        let mut ar = vec![];
        for c in &cons {
            let b = c.call_method0("bounds")?;
            ac.push(v3(&b.get_item(0)?)?);
            let vmax = fv(&Construct::g_vel(con(&c)?)?)?.iter().map(|x| x.abs()).fold(f64::NEG_INFINITY, f64::max);
            ar.push(b.get_item(1)?.extract::<f64>()? + vmax * dt * 2.0 + Construct::g_size(con(&c)?)?.extract::<f64>()? * 2.0);
        }
        let b = Self::_bounds(slf)?;
        let at = Self::bnd_at(slf, &b.get_item(0)?)?;
        let mut ix = vec![];
        let mut all = true;
        for c in &sleeping {
            let k = at.call_method1("get", (c.as_ptr() as usize,))?;
            if k.is_none() {
                all = false;
                break;
            }
            ix.push(k.extract::<usize>()?);
        }
        let (bc, br): (Vec<V3>, Vec<f64>) = if !all {
            let mut bc = vec![];
            let mut br = vec![];
            for c in &sleeping {
                let b = c.call_method0("bounds")?;
                bc.push(v3(&b.get_item(0)?)?);
                br.push(b.get_item(1)?.extract::<f64>()?);
            }
            (bc, br)
        } else {
            let c = v3s(&b.get_item(1)?)?;
            let r0 = fv(&Self::cache_get(slf, |c| &c.bnd)?.ok_or_else(|| PyAttributeError::new_err("_bnd"))?.get_item(4)?)?;
            (ix.iter().map(|&k| c[k]).collect(), ix.iter().map(|&k| r0[k]).collect())
        };
        let mut out = vec![];
        for (q, c) in sleeping.into_iter().enumerate() {
            let hit = (0..ac.len()).any(|a| {
                let d = sub(bc[q], ac[a]);
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() < ar[a] + br[q]
            });
            if hit {
                out.push(c);
            }
        }
        Ok(out)
    }
}
