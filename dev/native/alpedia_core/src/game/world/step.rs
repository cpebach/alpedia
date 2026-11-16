//! ワールド：1 ティック（step）とアドオンの呼び出し

use super::*;

#[pymethods]
impl World {
    // ------------------------------------------------------------------ シミュレーション
    #[pyo3(signature = (dt, r#move=None, jump=None, thrust_dir=None))]
    pub(crate) fn step(slf: &Bound<'_, Self>, dt: f64, r#move: Option<&Bound<'_, PyAny>>, jump: Option<&Bound<'_, PyAny>>,
            thrust_dir: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        let z = || a1(py, [0.0; 3]);
        let mv = r#move.map(|x| x.clone()).unwrap_or_else(z);
        let jp = jump.map(|x| x.clone()).unwrap_or_else(|| false.into_py(py).into_bound(py));
        let td = thrust_dir.map(|x| x.clone()).unwrap_or_else(z);
        let mut pt = std::time::Instant::now();
        slf.borrow_mut().time += dt;
        let time = slf.borrow().time;
        let ambient = 0.0;
        let seats = PyDict::new_bound(py);
        let lp = Self::g(slf, "local_player")?;
        for p in Self::all_players(slf)? {
            let s = p.getattr("seated")?;
            if s.is_truthy()? {
                let k = if p.is(&lp) { Self::g(slf, "input_keys")? } else { p.getattr("input_keys")? };
                seats.call_method1("setdefault", (s.get_item(0)?, PySet::empty_bound(py)?))?.call_method1("update", (k,))?;
            }
        }
        // 構築物の外側の球（問い合わせで使い回す）。全部の作り直しは 0.25 秒ごと
        let bt: f64 = match Self::dict_get(slf, "_bnd_t")? { Some(v) => v.extract()?, None => -1.0 };
        if time - bt >= 0.25 {
            Self::set(slf, "_bnd_t", time)?;
            Self::set(slf, "_bnd", py.None())?;
        }
        // 機能処理の距離より遠い物は、機器・プログラム・物理を止める（遠いかどうかは 0.25 秒ごとに見直す）
        let constructs = Self::g(slf, "constructs")?;
        let ft: f64 = match opt(slf.as_any(), "_far_t")? { Some(v) => v.extract()?, None => -1.0 };
        let fnn: i64 = match opt(slf.as_any(), "_far_n")? { Some(v) => v.extract()?, None => -1 };
        if time - ft >= 0.25 || fnn != constructs.len()? as i64 {
            Self::set(slf, "_far_t", time)?;
            Self::set(slf, "_far_n", constructs.len()?)?;
            let f = Self::far_ids(slf)?;
            Self::set(slf, "_far", f)?;
        }
        let far = Self::g(slf, "_far")?;
        let runner = Self::runner(slf)?.into_bound(py);
        prof_add(7, &mut pt);
        for c in Self::cons_v(slf)? {
            let cid = c.getattr("id")?;
            if far.contains(&cid)? {
                continue;
            }
            let dirty = truthy(&c, "dirty")?;
            if (!c.getattr("has_devices")?.is_truthy()? || c.call_method0("is_quiet")?.is_truthy()?) && !dirty {
                // 機器の無い物・固定されていて毎ティック処理する機器の無い物は機器・プログラムの処理を飛ばす
                zero_inplace(&c.getattr("force")?)?;
                continue;
            }
            let mut cdt = dt;
            let in_seat = seats.contains(&cid)?;
            if !dirty && !in_seat && c.call_method("is_quiet", (), Some(&[("slow_ok", true)].into_py_dict_bound(py)))?.is_truthy()? {
                // 明かりだけの固定された建物：0.25 秒ごとにまとめて（点いているか・電気の消費）
                zero_inplace(&c.getattr("force")?)?;
                let sd: f64 = match opt(&c, "_slow_dt")? { Some(v) => v.extract()?, None => 0.0 } + dt;
                c.setattr("_slow_dt", sd)?;
                if sd < 0.25 {
                    continue;
                }
                cdt = sd;
                c.setattr("_slow_dt", 0.0)?;
            }
            let keys = PyFrozenSet::new_bound(py, &match seats.get_item(&cid)? {
                Some(s) => s.iter()?.collect::<R<Vec<_>>>()?,
                None => vec![],
            })?;
            prof_add(7, &mut pt);
            crate::game::devices::dev_update(py, &c, cdt, Some(keys.as_any()), in_seat, ambient)?;
            prof_add(0, &mut pt);
            if let Some(n) = opt(&c, "notes")? {
                if n.is_truthy()? {
                    for m in n.iter()? {
                        Self::message(slf, &m?.str()?.to_string())?;
                    }
                    c.setattr("notes", PyList::empty_bound(py))?;
                }
            }
            runner.call_method1("tick", (&c, cdt))?;
            prof_add(1, &mut pt);
        }
        prof_add(7, &mut pt);
        Self::g(slf, "player")?.call_method1("update", (slf, dt, &mv, &jp, &td))?;
        for p in Self::g(slf, "remote")?.call_method0("values")?.iter()? {
            // 参加者の体は参加者の PC が動かす。ジェットパックの燃料と体重だけここで
            let p = p?;
            if p.getattr("flying")?.is_truthy()? && p.getattr("jetpack")?.is_truthy()? {
                let mut ap = Self::as_player(slf, p.clone().unbind());
                ap.__enter__(py)?;
                let r = Self::jetpack(slf, &p, dt, &p.getattr("thrust")?);
                ap.__exit__(py, &PyTuple::empty_bound(py))?;
                r?;
            }
            let st = p.getattr("stand")?;
            if st.is_truthy()? {
                let c = constructs.call_method1("get", (st.get_item(0)?,))?;
                if c.is_truthy()? {
                    let key = PyTuple::new_bound(py, st.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?);
                    let i = c.getattr("index")?.call_method1("get", (key,))?;
                    if !i.is_none() {
                        let i: usize = i.extract()?;
                        let pp = v3(&p.getattr("pos")?)?;
                        let gn = norm(v3(&Self::gravity(slf, &arr(py, pp.to_vec(), &[1, 3])?)?.get_item(0)?)?);
                        let up = v3(&p.getattr("up")?)?;
                        let m: f64 = p.getattr("mass")?.extract()?;
                        let add = [-up[0] * m * gn / 8.0, -up[1] * m * gn / 8.0, -up[2] * m * gn / 8.0];
                        let f = c.getattr("force")?;
                        if !with_rw(&f, |s| {
                            for q in 8 * i..8 * i + 8 {
                                for k in 0..3 {
                                    s[3 * q + k] += add[k];
                                }
                            }
                        })? {
                            let mut fv_ = fv(&f)?;
                            for q in 8 * i..8 * i + 8 {
                                for k in 0..3 {
                                    fv_[3 * q + k] += add[k];
                                }
                            }
                            let n = fv_.len() / 3;
                            c.setattr("force", arr(py, fv_, &[n, 3])?)?;
                        }
                    }
                }
            }
        }
        prof_add(2, &mut pt);
        let substeps = slf.borrow().substeps;
        let iters = slf.borrow().iters;
        let h = dt / substeps as f64;
        // 拡張（軌道上の街など）が位置を決める構築物は、物理計算しない
        let anchored_ids = PySet::empty_bound(py)?;
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("before_physics", (slf, dt))?;
            if r.is_truthy()? {
                anchored_ids.call_method1("__ior__", (py.get_type_bound::<PySet>().call1((r,))?,))?;
            }
        }
        let mut moved: Vec<Bound<'_, PyAny>> = vec![];
        for k in anchored_ids.iter() {
            if constructs.contains(&k)? {
                moved.push(constructs.get_item(&k)?);
            }
        }
        // 機能処理の距離より遠い物は止めておく（当たり判定の相手にはなる）。レールに乗っている物は軌道だけ進める
        for cid in far.iter()? {
            let cid = cid?;
            if !constructs.get_item(&cid)?.getattr("mode")?.eq("rails")? {
                anchored_ids.add(cid)?;
            }
        }
        prof_add(3, &mut pt);
        let mut cons: Vec<Bound<'_, PyAny>> = vec![];
        let mut sleeping: Vec<Bound<'_, PyAny>> = vec![];
        let gravity_fn = slf.getattr("gravity")?;
        for c in Self::cons_v(slf)? {
            if c.getattr("n_vox")?.extract::<usize>()? == 0 {
                continue;
            }
            let cid = c.getattr("id")?;
            if anchored_ids.contains(&cid)? {
                sleeping.push(c);
                continue;
            }
            let mode: String = c.getattr("mode")?.extract()?;
            if mode == "rigid" && c.getattr("anchored")?.is_truthy()? && !c.call_method0("_actuator_busy")?.is_truthy()? {
                // アンカーで固定されて止まっている物（建物など）：物理計算しない（ほかの物には当たる）
                c.setattr("_touch_moving", false)?;
                anchored_ids.add(&cid)?;
                let v = c.getattr("vel")?;
                if any_nonzero(&v)? {
                    zero_inplace(&v)?;
                }
                sleeping.push(c);
                continue;
            }
            if c.getattr("asleep")?.is_truthy()? && any_nonzero(&c.getattr("force")?)? {
                c.call_method0("wake")?;
            }
            c.setattr("_touch_moving", false)?;
            c.call_method1("update_mode", (dt, slf))?;            // 軽量化：剛体化・レール（遠方）の切り替え
            if c.getattr("is_static")?.is_truthy()? {             // アンカーで固定（アクチュエーターが止まっている間）
                anchored_ids.add(&cid)?;
                zero_inplace(&c.getattr("vel")?)?;
                sleeping.push(c);
            } else if c.getattr("asleep")?.is_truthy()? {
                sleeping.push(c);
            } else if c.getattr("mode")?.eq("rails")? {
                c.call_method1("rails_step", (dt, &gravity_fn))?;    // レールに乗っている構築物は重心の軌道だけを進める
                moved.push(c);
            } else {
                cons.push(c);
            }
        }
        prof_add(21, &mut pt);
        for c in &cons {
            Self::_prep_ext(slf, c, dt)?;
        }
        prof_add(22, &mut pt);
        sleeping = Self::_sleeping_near(slf, cons.clone(), sleeping, dt)?;
        prof_add(23, &mut pt);
        let broken = PySet::empty_bound(py)?;
        let mut fixed_pos: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)> = vec![];
        for c in &sleeping {
            if c.getattr("is_static")?.is_truthy()? {        // アンカーで固定：押されても戻す
                fixed_pos.push((c.clone(), c.getattr("pos")?.call_method0("copy")?));
            }
        }
        let settle_acc = fget(&phys(&Self::g(slf, "cfg")?)?, "merge_settle_acc", 30.0)?;
        let mut done = 0i64;
        let fp = PyList::new_bound(py, fixed_pos.iter().map(|(a, b)| PyTuple::new_bound(py, [a, b])));
        prof_add(3, &mut pt);
        let res = Self::_native_tick(slf, cons.clone(), sleeping.clone(), h, &anchored_ids, settle_acc, &fp)?;
        if !res.is_none() {                    // Rust でまとめて進めた（途中で止まったら、残りはここで今までどおり）
            done = res.get_item(0)?.extract()?;
            let woke: Vec<Bound<'_, PyAny>> = res.get_item(1)?.iter()?.collect::<R<_>>()?;
            broken.call_method1("__ior__", (res.get_item(2)?,))?;
            if !woke.is_empty() {
                Self::wake_in(slf, &woke, dt)?;
                cons.extend(woke);
                sleeping = Self::still_sleeping(sleeping, &anchored_ids)?;
            }
        }
        prof_add(4, &mut pt);
        let gs = Self::gscale(slf)?;
        for _ in 0..(substeps - done).max(0) {
            let mut settling = vec![];
            for c in &cons {
                let st: f64 = match opt(c, "_settle")? { Some(v) => v.extract()?, None => 0.0 };
                if st > 0.0 && c.getattr("vel")?.getattr("shape")?.eq(c.getattr("pos")?.getattr("shape")?)? {
                    let rb = c.getattr("rb")?;
                    let rb0: PyObject = if rb.is_truthy()? {
                        (rb.get_item("v")?.call_method0("copy")?, rb.get_item("w")?.call_method0("copy")?).into_py(py)
                    } else { rb.unbind() };
                    settling.push((c.clone(), c.getattr("vel")?.call_method0("copy")?, rb0));
                }
            }
            let pls = Self::g(slf, "planets")?;
            let begin = PyCFunction::new_closure_bound(py, None, None, {
                let gf = gravity_fn.clone().unbind();
                let pls = pls.clone().unbind();
                move |a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>| -> R<PyObject> {
                    let py = a.py();
                    a.get_item(0)?.call_method1("substep_begin", (h, iters, gf.bind(py), pls.bind(py), gs))?;
                    Ok(py.None())
                }
            })?;
            Self::_each(slf, cons.clone(), begin.as_any())?;
            let woke = Self::_voxel_collisions(slf, cons.clone(), sleeping.clone())?;
            let mut wk = vec![];
            for c in woke {
                if !anchored_ids.contains(c.getattr("id")?)? {
                    wk.push(c);
                }
            }
            for (c, p0) in &fixed_pos {
                let p = c.getattr("pos")?;
                let src = fv(p0)?;
                if !with_rw(&p, |s| s.copy_from_slice(&src))? {
                    p.set_item(pyo3::types::PySlice::full_bound(py), p0)?;
                }
            }
            let end = PyCFunction::new_closure_bound(py, None, None, move |a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>| -> R<PyObject> {
                a.get_item(0)?.call_method1("substep_end", (h,)).map(|x| x.unbind())
            })?;
            let ends = Self::_each(slf, cons.clone(), end.as_any())?;
            for (c, v0, rb0) in &settling {
                world_limit_dv(c, v0, rb0.bind(py), settle_acc * h)?;
            }
            for (c, b) in cons.iter().zip(ends.iter()) {
                if b.bind(py).is_truthy()? {
                    broken.add(c.getattr("id")?)?;
                }
            }
            if !wk.is_empty() {
                Self::wake_in(slf, &wk, dt)?;
                cons.extend(wk);
                sleeping = Self::still_sleeping(sleeping, &anchored_ids)?;
            }
        }
        prof_add(5, &mut pt);
        for c in &cons {
            c.call_method1("update_sleep", (dt,))?;
            let st: f64 = match opt(c, "_settle")? { Some(v) => v.extract()?, None => 0.0 };
            if st > 0.0 {
                c.setattr("_settle", (st - dt).max(0.0))?;
            }
        }
        let mut cm = cons.clone();
        cm.extend(moved);
        Self::_patch_bounds(slf, cm, &broken)?;
        let cl = PyList::new_bound(py, &cons);
        for a in Self::addons_v(slf)? {
            a.call_method1("after_physics", (slf, &cl))?;
        }
        for c in &cons {
            c.setattr("ext_events", PyList::empty_bound(py))?;
        }
        prof_add(6, &mut pt);
        for cid in broken.iter() {
            let c = constructs.call_method1("get", (&cid,))?;
            if c.is_none() {
                continue;
            }
            let parts = c.call_method0("split")?;
            for part in parts.iter()? {
                Self::add_construct(slf, &part?)?;
            }
            if parts.len()? > 0 {
                log(py)?.call_method1("info", ("構築物 %s が %d 個に分離", c.getattr("name")?, parts.len()? + 1))?;
            }
        }
        let mut empty = vec![];
        for (k, c) in constructs.downcast::<PyDict>()?.iter() {
            if c.getattr("n_vox")?.extract::<usize>()? == 0 {
                empty.push(k);
            }
        }
        for k in empty {
            constructs.del_item(k)?;
        }
        let timers = Self::g(slf, "_timers")?;
        let rb_t: f64 = timers.get_item("rebond")?.extract::<f64>()? + dt;
        timers.set_item("rebond", rb_t)?;
        if rb_t > 0.25 {
            timers.set_item("rebond", 0.0)?;
            for c in Self::cons_v(slf)? {
                c.call_method0("try_rebond")?;
            }
        }
        for pl in Self::g(slf, "planets")?.iter()? {
            let pl = pl?;
            if pl.hasattr("update_water")? {
                pl.call_method1("update_water", (dt,))?;    // 掘った穴に水が満ちる（水のある惑星）
            }
        }
        let cu_t: f64 = timers.get_item("cleanup")?.extract::<f64>()? + dt;
        timers.set_item("cleanup", cu_t)?;
        let cl = Self::settings(slf)?.get_item("cleanup")?;
        if cu_t >= fget(&cl, "check_interval", 5.0)? {
            Self::_cleanup(slf, cu_t)?;
            timers.set_item("cleanup", 0.0)?;
        }
        prof_add(7, &mut pt);
        Ok(())
    }

    // ------------------------------------------------------------------ 拡張（hooks.Addon）
    /// クリエイティブ（資源を使わない）か。資源のルールを持つ MOD（サバイバルなど）が入っていれば False
    #[getter]
    pub(crate) fn creative(slf: &Bound<'_, Self>) -> R<bool> {
        for a in Self::addons_v(slf)? {
            if a.getattr("resource_rules")?.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// 名前で拡張を探す（無ければ None）
    pub(crate) fn addon<'py>(slf: &Bound<'py, Self>, name: &str) -> R<Bound<'py, PyAny>> {
        for a in Self::addons_v(slf)? {
            if a.getattr("name")?.eq(name)? {
                return Ok(a);
            }
        }
        Ok(slf.py().None().into_bound(slf.py()))
    }
    /// 拡張の状態を変える操作（街での売り買いなど）。参加者なら自分の PC でもすぐに行い、ホストへも送る
    #[pyo3(signature = (name, path, *args))]
    pub(crate) fn addon_call<'py>(slf: &Bound<'py, Self>, name: &str, path: &str, args: &Bound<'py, PyTuple>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let a = Self::addon(slf, name)?;
        if a.is_none() {
            return Ok(a);
        }
        let mut f = a.clone();
        for part in path.split('.') {
            f = f.getattr(part)?;
        }
        let res = f.call1(args.clone())?;
        let net = Self::g(slf, "net")?;
        if !net.is_none() && match opt(&net, "role")? { Some(r) => r.eq("client")?, None => false } {
            let enc = PyList::empty_bound(py);
            for x in args.iter() {
                let r = a.call_method1("net_ref_of", (&x,))?;
                if r.is_none() {
                    enc.append(x)?;
                } else {
                    let d = PyDict::new_bound(py);
                    d.set_item("__ref__", PyList::new_bound(py, r.iter()?.collect::<R<Vec<_>>>()?))?;
                    enc.append(d)?;
                }
            }
            let m = PyDict::new_bound(py);
            m.set_item("t", "call")?;
            m.set_item("addon", name)?;
            m.set_item("path", path)?;
            m.set_item("args", enc)?;
            net.call_method1("send", (m,))?;
        }
        Ok(res)
    }
    /// ホスト：参加者からの addon_call を行う（world.player はその人にしてから呼ぶ）
    pub(crate) fn run_addon_call<'py>(slf: &Bound<'py, Self>, name: &str, path: &str, args: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let a = Self::addon(slf, name)?;
        if a.is_none() {
            return Ok(a);
        }
        let mut f = a.clone();
        for part in path.split('.') {
            if part.starts_with('_') {
                return Ok(py.None().into_bound(py));
            }
            f = f.getattr(part)?;
        }
        let real = PyList::empty_bound(py);
        for x in args.iter()? {
            let x = x?;
            if x.is_instance_of::<PyDict>() && x.contains("__ref__")? {
                let r = PyTuple::new_bound(py, std::iter::once(slf.clone().into_any()).chain(x.get_item("__ref__")?.iter()?.collect::<R<Vec<_>>>()?)
                    .collect::<Vec<_>>());
                real.append(a.getattr("net_ref")?.call1(r)?)?;
            } else {
                real.append(x)?;
            }
        }
        f.call1(PyTuple::new_bound(py, real))
    }
}
