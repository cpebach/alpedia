//! 構築物：1 ティックの中身（接触・物理コアとのやり取り）

use crate::game::planet::{pla, Planet};
use crate::game::PyDictOps;
use super::*;

#[pymethods]
impl Construct {
    // ------------------------------------------------------------------ 1サブステップ（Rust の Body）
    /// 単独で1サブステップ進める（テスト用）
    #[pyo3(signature = (h, iters, gravity_fn, planets, gscale=1.0))]
    pub(crate) fn substep(slf: &Bound<'_, Self>, h: f64, iters: i64, gravity_fn: &Bound<'_, PyAny>, planets: &Bound<'_, PyAny>,
               gscale: Option<f64>) -> R<bool> {
        Self::substep_begin(slf, h, iters, gravity_fn, planets, gscale)?;
        Self::substep_end(slf, h)
    }
    /// 1サブステップの前半。gscale：重力倍率（重力は惑星から Rust で計算する。gravity_fn は昔の名残り）
    #[pyo3(signature = (h, iters, gravity_fn, planets, gscale=1.0))]
    pub(crate) fn substep_begin(slf: &Bound<'_, Self>, h: f64, iters: i64, gravity_fn: &Bound<'_, PyAny>, planets: &Bound<'_, PyAny>,
                     gscale: Option<f64>) -> R<()> {
        let _ = gravity_fn;
        if Self::n(slf)? == 0 {
            return Ok(());
        }
        let gs = gscale.unwrap_or(1.0);
        if Self::mode_is(slf, &["art"]) {
            Self::_native_art_begin(slf, h, planets, gs)?;
        } else {
            Self::_native_begin(slf, h, iters, planets, gs)?;
        }
        Ok(())
    }
    pub(crate) fn substep_end(slf: &Bound<'_, Self>, h: f64) -> R<bool> {
        if Self::n(slf)? == 0 {
            return Ok(false);
        }
        if Self::mode_is(slf, &["art"]) {
            if Self::cache_get(slf, |c| &c.nb_art).and_then(|v| v.map(|v| v.is_truthy()).transpose().map(|v| v.unwrap_or(false)))? {
                Self::cache_put(slf, |c| &mut c.nb_art, false)?;
                Self::_native_art_end(slf, h)?;
            }
            return Ok(false);
        }
        if Self::t_nb_used(slf)? {
            Self::s_nb_used(slf, false)?;
            return Self::_native_end(slf, h);
        }
        Ok(false)
    }

    /// 気密の空洞セルと、その6方向で接するボクセルの番号（浮力を伝える先）
    pub(crate) fn _cavity_neighbors<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let net = match opt(slf.as_any(), "_net")? {
            Some(n) if n.getattr("cavities")?.is_truthy()? => n,
            _ => return Ok(py.None().into_bound(py)),
        };
        // 形が変わって配管・空洞をまだ数え直していない（分かれた直後など）：古い番号を使わないように、このティックは浮力なし
        if slf.borrow().dirty {
            return Ok(py.None().into_bound(py));
        }
        let ver = Self::ver(slf);
        if let Some(c) = Self::cache_get(slf, |c| &c.cav)? {
            if c.get_item(0)?.is(&net) && c.get_item(2)?.eq(ver)? {
                return c.get_item(1);
            }
        }
        let index = Self::g_index(slf)?;
        let mut nb: Vec<i64> = vec![];
        for cav in net.getattr("cavities")?.iter()? {
            for cell in cav?.get_item("cells")?.iter()? {
                let c = cell_of(&cell?)?;
                let mut row = [-1i64; 6];
                for (d, du) in DIRS.iter().enumerate() {
                    // その向きで最初に当たるブロック（空洞のまん中のセルも、まわりの壁に浮力を渡す。
                    // となりだけを見ていたときは、壁に触れていないセルの浮力が無くなっていた）
                    for k in 1..=64 {
                        let v: i64 = index.dget2(tup3(py, [c[0] + du[0] * k, c[1] + du[1] * k, c[2] + du[2] * k]), -1)?.extract()?;
                        if v >= 0 {
                            row[d] = v * 128 + k;          // ブロックの番号 × 128 ＋ 距離（セルの数）
                            break;
                        }
                    }
                }
                if row.iter().any(|x| *x >= 0) {
                    nb.extend(row);
                }
            }
        }
        let data: Bound<'py, PyAny> = if nb.is_empty() { py.None().into_bound(py) } else {
            let k = nb.len() / 6;
            PyTuple::new_bound(py, [iarr(py, nb, &[k, 6])?]).into_any()
        };
        Self::cache_put(slf, |c| &mut c.cav, (net, &data, ver))?;
        Ok(data)
    }

    /// 各頂点の当たり判定に使う「芯」（点・線・三角形）を選ぶ（Rust の build_contacts）
    pub(crate) fn _build_contacts(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let cells: Vec<i64> = Self::cells_v(slf)?.into_iter().flatten().collect();
        let n = cells.len() / 3;
        let topo = Self::g_topo(slf)?;
        let segs = iv(&topo.get_item("segs")?)?;
        let tris = iv(&topo.get_item("tris")?)?;
        let (ns, nt) = (segs.len() / 2, tris.len() / 3);
        let r = core(py)?.call_method1("build_contacts", (iarr(py, cells, &[n, 3])?, contig(&Self::g_pos(slf)?)?,
            Self::voxel_frames(slf)?, Self::voxel_centers(slf)?, iarr(py, segs, &[ns, 2])?, iarr(py, tris, &[nt, 3])?))?;
        Self::s_ctype(slf, r.get_item(0)?)?;
        Self::s_cidx(slf, r.get_item(1)?)
    }
    /// 各頂点に対応する芯の最近点（この点から半径 r が見た目の表面）
    pub(crate) fn contact_cores<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        core(slf.py())?.call_method1("contact_cores", (Self::g_pos(slf)?, Self::g_ctype(slf)?, Self::g_cidx(slf)?))
    }

    /// 地面から高い頂点を省くための (上向き, その方向の地表半径)。重心が惑星の中心なら (None, 0)
    pub(crate) fn _ground_ref<'py>(slf: &Bound<'py, Self>, pl: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let p = Self::pos_v(slf)?;
        let com = sub(mean8(&p), v3(&pl.getattr("center")?)?);
        let cn = norm(com);
        if cn <= 1e-6 {
            return Ok(PyTuple::new_bound(py, [py.None(), 0.0f64.into_py(py)]));
        }
        let up = [com[0] / cn, com[1] / cn, com[2] / cn];
        if let Some(gc) = Self::cache_get(slf, |c| &c.ground)? {
            if gc.get_item(0)?.is(pl) {
                let u0 = v3(&gc.get_item(1)?)?;
                let dmax = (0..3).map(|k| (u0[k] - up[k]).abs()).fold(f64::NEG_INFINITY, f64::max);
                let rad: f64 = pl.getattr("radius")?.extract()?;
                if dmax * rad < 0.5 {
                    return Ok(PyTuple::new_bound(py, [a1(py, up).unbind(), gc.get_item(2)?.unbind()]));
                }
            }
        }
        let rs: f64 = Planet::surface_radius(pla(&pl)?, &IntoPy::<PyObject>::into_py(arr(py, up.to_vec(), &[1, 3])?, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?.get_item(0)?.extract()?;
        let upa = a1(py, up);
        Self::cache_put(slf, |c| &mut c.ground, (pl, &upa, rs))?;
        Ok(PyTuple::new_bound(py, [upa.unbind(), rs.into_py(py)]))
    }

    /// 地面との当たり判定（Rust）。collect なら押し戻さずに (頂点, めり込み, 法線) を返す
    #[pyo3(signature = (pl, collect=false))]
    pub(crate) fn _collide_planet<'py>(slf: &Bound<'py, Self>, pl: &Bound<'py, PyAny>, collect: bool) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let nat = Planet::_nat(pla(&pl)?).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?;
        let k = Self::contact_cores(slf)?;
        let r = Self::sz(slf) * 0.5;
        let gr = Self::_ground_ref(slf, pl)?;
        let up = gr.get_item(0)?;
        let upl: PyObject = if up.is_none() { py.None() } else { PyList::new_bound(py, v3(&up)?).into_any().unbind() };
        Self::_contig(slf, &["pos"])?;
        let args: Vec<PyObject> = vec![Self::g_pos(slf)?.unbind(), contig(&Self::g_prev(slf)?)?.unbind(),
            contig(&Self::g_vel(slf)?)?.unbind(), contig(&k)?.unbind(), contig(&Self::g_fric(slf)?)?.unbind(), r.into_py(py),
            pl.getattr("bound")?.extract::<f64>()?.into_py(py), pl.getattr("lipschitz")?.extract::<f64>()?.into_py(py), upl,
            gr.get_item(1)?.unbind(), Self::mode_is(slf, &["rigid"]).into_py(py),
            slf.get_type().getattr("IMPACT_SPEED")?.extract::<f64>()?.into_py(py), CONTACT_SOFT.into_py(py), (!collect).into_py(py)];
        let res = nat.call_method1("collide", PyTuple::new_bound(py, args))?;
        if res.get_item(0)?.is_truthy()? {
            slf.borrow_mut().want_soft = true;
        }
        let hi = res.get_item(1)?;
        if collect && hi.len()? > 0 {
            return Ok(PyTuple::new_bound(py, [hi, res.get_item(2)?, res.get_item(3)?]).into_any());
        }
        Ok(py.None().into_bound(py))
    }

    /// 拡張が出した向き付きの箱（街の建物など）との当たり判定。
    /// 強くぶつかった速さと、動いている物体を押した力積は ext_events に記録する（拡張が after_physics で使う）
    #[pyo3(signature = (bs, h, collect=false))]
    pub(crate) fn _collide_boxes<'py>(slf: &Bound<'py, Self>, bs: &Bound<'py, PyAny>, h: f64, collect: bool) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let none = || py.None().into_bound(py);
        let (owner, c, a, e, idx) = (bs.get_item(0)?, bs.get_item(1)?, bs.get_item(2)?, bs.get_item(3)?, bs.get_item(4)?);
        let kc = v3s(&Self::contact_cores(slf)?)?;
        let r = Self::sz(slf) * 0.5;
        let cv = v3s(&c)?;
        let ev = v3s(&e)?;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for (q, cc) in cv.iter().enumerate() {
            let rb = norm(ev[q]);
            for k in 0..3 {
                lo[k] = lo[k].min(cc[k] - rb);
                hi[k] = hi[k].max(cc[k] + rb);
            }
        }
        for k in 0..3 {
            lo[k] -= r;
            hi[k] += r;
        }
        let sel: Vec<usize> = (0..kc.len()).filter(|&i| (0..3).all(|k| kc[i][k] > lo[k] && kc[i][k] < hi[k])).collect();
        if sel.is_empty() {
            return Ok(none());
        }
        let ks: Vec<V3> = sel.iter().map(|&i| kc[i]).collect();
        let res = core(py)?.call_method1("box_sdf", (a3(py, &ks)?, contig(&c)?, contig(&a)?, contig(&e)?))?;
        let d = fv(&res.get_item(0)?)?;
        let nn = v3s(&res.get_item(1)?)?;
        let kk = iv(&res.get_item(2)?)?;
        let idxv = iv(&idx)?;
        let (mut hi_, mut dep, mut n, mut parts) = (vec![], vec![], vec![], vec![]);
        for q in 0..sel.len() {
            let dd = d[q] - r;
            if dd < 0.0 {
                hi_.push(sel[q]);
                dep.push(dd);
                n.push(nn[q]);
                parts.push(idxv[kk[q] as usize]);
            }
        }
        if hi_.is_empty() {
            return Ok(none());
        }
        let pos_o = Self::g_pos(slf)?;
        let mut pos = v3s(&pos_o)?;
        let vel = Self::vel_v(slf)?;
        let moving = owner.call_method0("is_moving")?.is_truthy()?;
        let ph: Vec<V3> = hi_.iter().map(|&i| pos[i]).collect();
        let vs: Vec<V3> = if moving { v3s(&owner.call_method1("point_velocity", (a3(py, &ph)?,))?)? } else { vec![[0.0; 3]; hi_.len()] };
        let vn: Vec<f64> = (0..hi_.len()).map(|q| -dot(sub(vel[hi_[q]], vs[q]), n[q])).collect();
        let evs = Self::g_ext_events(slf)?;
        if vn.iter().any(|v| *v >= 7.0) {        // 建物の損傷（いちばん強く当たった速さ）
            let best = PyDict::new_bound(py);
            for (pa, v) in parts.iter().zip(&vn) {
                let cur: f64 = best.get_item(pa)?.map(|x| x.extract()).transpose()?.unwrap_or(0.0);
                if *v > cur {
                    best.set_item(pa, v)?;
                }
            }
            evs.call_method1("append", ((&owner, best, py.None(), py.None()),))?;
        }
        if collect {
            let nh = hi_.len();
            return Ok(PyTuple::new_bound(py, [iarr(py, hi_.iter().map(|x| *x as i64).collect(), &[nh])?,
                arr(py, dep.iter().map(|x| -x).collect(), &[nh])?, a3(py, &n)?, a3(py, &vs)?]).into_any());
        }
        let prev = v3s(&Self::g_prev(slf)?)?;
        let fric = fv(&Self::g_fric(slf)?)?;
        let mass = Self::mass_v(slf)?;
        let (mut jv, mut pts) = (vec![], vec![]);
        for q in 0..hi_.len() {
            let i = hi_[q];
            let corr = scl(n[q], -dep[q]);
            pos[i] = add(pos[i], corr);
            let disp = sub(sub(pos[i], prev[i]), scl(vs[q], h));
            let tang = sub(disp, scl(n[q], dot(disp, n[q])));
            let fr = fric[i].clamp(0.0, 1.0);
            pos[i] = sub(pos[i], scl(tang, fr));
            if moving {
                let j = scl(sub(corr, scl(tang, fr)), mass[i] / h);
                jv.push(scl(j, -1.0));
                pts.push(pos[i]);
            }
        }
        write_back(slf, "pos", &pos_o, pos.iter().flatten().copied().collect(), &[pos.len(), 3])?;
        if moving {
            evs.call_method1("append", ((&owner, py.None(), a3(py, &jv)?, a3(py, &pts)?),))?;
        }
        Ok(none())
    }

    /// この構築物の Rust の Body（形が変わったら作り直す）
    pub(crate) fn _native_body<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let reg = Self::g_reg(slf)?;
        let rotors: Vec<Bound<'py, PyAny>> = Self::g_rotors(slf)?.iter()?.collect::<R<_>>()?;
        let mut hk = vec![];
        for r in &rotors {
            match crate::game::actuator::act_hinge_of(&r.get_item("dev")?, Some(&reg))? {
                Some((a, b)) => hk.extend([a as i64, b as i64]),
                None => hk.extend([-1, -1]),
            }
        }
        let key = PyTuple::new_bound(py, [Self::ver(slf).into_py(py), Self::n(slf)?.into_py(py),
            PyTuple::new_bound(py, &hk).into_py(py), slf.borrow().mass_ver.into_py(py)]);
        if let Some(c) = Self::cache_hit(slf, |c| &c.nb, key.as_any())? {
            return c.get_item(1);
        }
        let nr = rotors.len();
        let (mut ra, mut rva, mut rvb, mut rbr) = (vec![], vec![], vec![], vec![]);
        for r in &rotors {
            let a: i64 = r.get_item("a")?.extract()?;
            let b: i64 = r.get_item("b")?.extract()?;
            let d: usize = r.get_item("d")?.extract()?;
            ra.push(a);
            for (ca, cb) in joint_pairs(d) {
                rva.push(8 * a + ca as i64);
                rvb.push(8 * b + cb as i64);
            }
            rbr.push(r.get_item("br")?.extract::<f64>()?);
        }
        let i = |k: &str| -> R<Bound<'py, PyAny>> { cint(&Self::g(slf, k)?) };
        let f = |k: &str| -> R<Bound<'py, PyAny>> { cflt(&Self::g(slf, k)?) };
        let args = PyTuple::new_bound(py, [Self::sz(slf).into_py(py).into_bound(py), i("ja")?, i("jb")?, f("jr")?, f("jk")?, f("jbrk")?,
            f("jcount")?, f("inv_mass")?, f("mass")?, f("damp")?, f("fric")?, f("v_stiff")?, f("v_plast")?, f("v_yield")?,
            f("v_density")?, i("ctype")?, i("cidx")?, iarr(py, ra, &[nr])?, iarr(py, rva, &[nr, 4])?, iarr(py, rvb, &[nr, 4])?,
            arr(py, rbr, &[nr])?, iarr(py, hk, &[nr, 2])?]);
        // まとまりの形合わせ（大きな構築物の固さ）：セルと、まとまりに入れてよいブロック（アクチュエーターは入れない）
        let n = Self::n(slf)?;
        let mut cells = Vec::with_capacity(3 * n);
        for c in Self::g_cells(slf)?.iter()? {
            for x in c?.iter()?.take(3) {
                cells.push(x?.call_method0("__int__")?.extract::<i64>()?);
            }
        }
        let mut ok = vec![true; n];
        for i in Self::actuator_indices(slf)?.iter()? {
            let i: usize = i?.extract()?;
            if i < n {
                ok[i] = false;
            }
        }
        let kw = PyDict::new_bound(py);
        if cells.len() == 3 * n {
            kw.set_item("cells", iarr(py, cells, &[n, 3])?)?;
            kw.set_item("cluster_ok", ok)?;
        }
        let body = core(py)?.getattr("Body")?.call(args, Some(&kw))?;
        Self::cache_put(slf, |c| &mut c.nb, (key, &body))?;
        Ok(body)
    }

    /// まとまりの基準の並び（Body ごと。作り直した Body では NaN から：最初の計算で今の形から作る）
    pub(crate) fn _crest<'py>(slf: &Bound<'py, Self>, body: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        if let Some(c) = Self::cache_get(slf, |c| &c.crest)? {
            if c.get_item(0)?.is(body) {
                return c.get_item(1);
            }
        }
        let m: usize = body.getattr("n_cluster_members")?.extract()?;
        let a = arr(py, vec![f64::NAN; 3 * m.max(1)], &[3 * m.max(1)])?;
        let a = if m == 0 { arr(py, vec![], &[0])? } else { a };
        Self::cache_put(slf, |c| &mut c.crest, (body, &a))?;
        Ok(a)
    }

    /// 構築物どうしの押し出しに使う Rust の Proxy（形が変わったら作り直す）
    pub(crate) fn native_proxy<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let vinv = Self::collision_inv(slf)?;
        let anch: PyObject = if Self::anchored_b(slf)? { PyTuple::new_bound(py, fv(&vinv)?).into_py(py) } else { py.None() };
        let key = PyTuple::new_bound(py, [Self::ver(slf).into_py(py), Self::n(slf)?.into_py(py), anch]);
        if let Some(c) = Self::cache_hit(slf, |c| &c.nproxy, key.as_any())? {
            return c.get_item(1);
        }
        let w = Self::g_prox_w(slf)?.call_method0("tocsr")?;
        w.call_method0("sort_indices")?;
        let n = Self::n(slf)?;
        let index = Self::g_index(slf)?;
        let mut pairs = vec![];
        for k in Self::link_list(slf, true)? {
            if let (Some(a), Some(b)) = (Self::idx_of(&index, &k.get_item(0)?)?, Self::idx_of(&index, &k.get_item(1)?)?) {
                pairs.extend([a as i64, b as i64]);
            }
        }
        let np_ = pairs.len() / 2;
        let inv = cflt(&w.call_method1("__matmul__", (&vinv,))?)?;
        let obj = core(py)?.getattr("Proxy")?.call1((n, Self::sz(slf), cint(&w.getattr("indptr")?)?, cint(&w.getattr("indices")?)?,
            cflt(&w.getattr("data")?)?, inv, iarr(py, pairs, &[np_, 2])?, cflt(&Self::g_mass(slf)?)?))?;
        Self::cache_put(slf, |c| &mut c.nproxy, (key, &obj))?;
        Ok(obj)
    }

    pub(crate) fn _rotor_angles(slf: &Bound<'_, Self>) -> R<Vec<f64>> {
        let mut out = vec![];
        for r in Self::g_rotors(slf)?.iter()? {
            let a: f64 = r?.get_item("dev")?.get_item("state")?.dget2("angle", 0.0)?.extract()?;
            out.push(a.to_radians());
        }
        Ok(out)
    }

    /// Rust に渡す配列を、連続した書き込める float64 にそろえる
    #[pyo3(signature = (*names), name = "_contig")]
    pub(crate) fn _contig_py(slf: &Bound<'_, Self>, names: Vec<String>) -> R<()> {
        let v: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        Self::_contig(slf, &v)
    }

    pub(crate) fn _native_begin(slf: &Bound<'_, Self>, h: f64, iters: i64, planets: &Bound<'_, PyAny>, gscale: f64) -> R<bool> {
        let py = slf.py();
        let body = Self::_native_body(slf)?;
        let pls = construct_native_planets(py, planets)?.into_bound(py);
        let rigid = Self::mode_is(slf, &["rigid"]);
        let full = rigid && Self::t_full_rigid(slf)?;
        Self::ensure_prev(slf)?;
        Self::_contig(slf, &["pos", "vel", "_prev", "force"])?;
        let cav = Self::cav_arg(slf, &pls)?;
        let soft = CONTACT_SOFT;
        let imp_s: f64 = slf.get_type().getattr("IMPACT_SPEED")?.extract()?;
        let (pos, vel, prev, force) = (Self::g_pos(slf)?, Self::g_vel(slf)?, Self::g_prev(slf)?, Self::g_force(slf)?);
        if rigid {
            let rb = Self::rb_req(slf)?;
            rb.borrow_mut(py).contig(py)?;
            let (rr, rib) = { let b = rb.borrow(py); (b.r.clone_ref(py).into_any(), b.ib.clone_ref(py).into_any()) };
            let defer = full && Self::g_ext_colliders(slf)?.is_truthy()?;
            let args: Vec<PyObject> = vec![pos.unbind(), vel.unbind(), prev.unbind(), force.unbind(), rr,
                rib, pls.clone().unbind(), gscale.into_py(py), h.into_py(py), cav.unbind(), full.into_py(py),
                imp_s.into_py(py), soft.into_py(py)];
            let mut a = args;
            if defer {
                a.push(true.into_py(py));
            }
            let want = body.call_method1("rigid_begin", PyTuple::new_bound(py, a))?;
            if want.is_truthy()? {
                slf.borrow_mut().want_soft = true;
            }
            if defer {
                // 完全剛体で MOD の箱（街の建物など）に触れうるとき：箱との接触は MOD のプログラムで集め、地面と一緒に解く
                Self::_native_full_contacts(slf, &body, &pls, h)?;
                Self::s_nb_used(slf, true)?;
                return Ok(true);
            }
        } else {
            Self::_ensure_lastR(slf)?;
            Self::_contig(slf, &["rest", "act_scale", "_last_R", "_quat"])?;
            let pmin: usize = crate::game::construct::PARALLEL_MIN_VOXELS;
            let par = Self::n(slf)? >= pmin && !slf.borrow().in_pool;
            let args: Vec<PyObject> = vec![pos.unbind(), vel.unbind(), prev.unbind(), force.unbind(), Self::g_rest(slf)?.unbind(),
                Self::g_act_scale(slf)?.unbind(), Self::g_last_r(slf)?.unbind(), Self::g_quat(slf)?.unbind(), pls.clone().unbind(),
                gscale.into_py(py), h.into_py(py), iters.into_py(py), Self::_rotor_angles(slf)?.into_py(py), par.into_py(py), cav.unbind(),
                soft.into_py(py), Self::_crest(slf, &body)?.unbind()];
            body.call_method1("soft_begin", PyTuple::new_bound(py, args))?;
        }
        let ecs: Vec<Bound<'_, PyAny>> = Self::g_ext_colliders(slf)?.iter()?.collect::<R<_>>()?;
        for bs in ecs {
            Self::_collide_boxes(slf, &bs, h, false)?;
        }
        Self::s_nb_used(slf, true)?;
        Ok(true)
    }

}

impl Construct {
    /// world._native_tick に渡す、この構築物の1ティック分（Rust の型）。まとめて計算できないときは None
    /// groups：(箱 → 番号の dict, 箱のまとまりの list)。ここで箱を足す
    pub(crate) fn tick_job(slf: &Bound<'_, Self>, groups: &Bound<'_, PyAny>, planets_ok: bool) -> R<Option<crate::TickJobIn>> {
        let py = slf.py();
        if !planets_ok || Self::n(slf)? == 0 || !Self::mode_is(slf, &["soft", "rigid", "art"]) {
            return Ok(None);
        }
        let body = Self::_native_body(slf)?;
        if body.is_none() {
            return Ok(None);
        }
        let (idx, lst) = (groups.get_item(0)?, groups.get_item(1)?);
        let mut boxes = vec![];
        for bs in Self::g_ext_colliders(slf)?.iter()? {
            let bs = bs?;
            if bs.get_item(0)?.call_method0("is_moving")?.is_truthy()? {
                return Ok(None);          // 動いている街：サブステップごとに進めて押した反動を拡張に渡す
            }
            let c = bs.get_item(1)?;
            let cid = c.as_ptr() as usize;
            let k = idx.dget1(cid)?;
            let k: usize = if k.is_none() {
                let k = lst.len()?;
                idx.set_item(cid, k)?;
                lst.call_method1("append", ((cflt(&c)?, cflt(&bs.get_item(2)?)?, cflt(&bs.get_item(3)?)?, cint(&bs.get_item(4)?)?),))?;
                k
            } else {
                k.extract()?
            };
            boxes.push(k);
        }
        Self::ensure_prev(slf)?;
        Self::_contig(slf, &["pos", "vel", "_prev", "force"])?;
        let pmin: usize = crate::game::construct::PARALLEL_MIN_VOXELS;
        let fa_of = |o: Bound<'_, PyAny>| -> R<FA> { o.extract() };
        let (kind, art, a, b, v);
        if Self::mode_is(slf, &["art"]) {
            let at = Self::art_fresh(slf)?;
            if at.borrow().nat.is_none(py) {
                return Ok(None);
            }
            Self::_art_contig(slf)?;
            let (ext_want, ext_speed) = Self::ext_targets(slf, &at)?;
            let (want, spd) = Self::rot_targets(&at)?;
            let ab = at.borrow();
            kind = 3;
            art = Some(ab.nat.extract(py)?);
            a = [Some(ab.x.clone_ref(py)), Some(ab.r.clone_ref(py)), Some(ab.vl.clone_ref(py)), Some(ab.applied.clone_ref(py)),
                 Some(ab.ext_applied.clone_ref(py)), None];
            b = [Some(ab.vl0.clone_ref(py)), None, None];
            v = [want, spd, ext_want, ext_speed];
        } else if Self::mode_is(slf, &["rigid"]) {
            let rb = match Self::rb(slf)? {
                Some(r) => r,
                None => return Ok(None),
            };
            rb.borrow_mut(py).contig(py)?;
            let rbb = rb.borrow(py);
            kind = if Self::t_full_rigid(slf)? { 2 } else { 1 };
            art = None;
            a = [Some(rbb.r.clone_ref(py)), Some(rbb.x.clone_ref(py)), None, None, None, None];
            b = [Some(rbb.ib.clone_ref(py)), Some(rbb.local.clone_ref(py)), None];
            v = [vec![], vec![], vec![], vec![]];
        } else {
            Self::_ensure_lastR(slf)?;
            Self::_contig(slf, &["rest", "act_scale", "_last_R", "_quat"])?;
            let crest = Self::_crest(slf, &body)?;
            let me = slf.borrow();
            kind = 0;
            art = None;
            a = [Some(me.rest.clone_ref(py)), Some(me.last_r.clone_ref(py)), Some(me.quat.clone_ref(py)),
                 if crest.is_none() { None } else { Some(fa_of(crest)?) }, None, None];
            b = [Some(me.act_scale.clone_ref(py)), None, None];
            drop(me);
            v = [Self::_rotor_angles(slf)?, vec![], vec![], vec![]];
        }
        let proxy = Self::native_proxy(slf)?.extract()?;
        let parallel = Self::n(slf)? >= pmin;
        let me = slf.try_borrow()?;
        Ok(Some(crate::TickJobIn {
            kind,
            body: body.extract()?,
            proxy,
            art,
            pos: me.pos.clone_ref(py),
            vel: me.vel.clone_ref(py),
            prev: me.prev.clone_ref(py),
            force: me.force.clone_ref(py),
            cav: None,
            a,
            b,
            v,
            parallel,
            boxes,
            settle_cap: 0.0,
        }))
    }

}

#[pymethods]
impl Construct {
    /// Rust でまとめて計算した間にちぎれた結合を外す（_native_end と同じ）
    pub(crate) fn tick_broken(slf: &Bound<'_, Self>, broken: &Bound<'_, PyAny>, rot: &Bound<'_, PyAny>) -> R<()> {
        let rot: Vec<usize> = iv(rot)?.into_iter().map(|x| x as usize).collect();
        let broken: Vec<usize> = iv(broken)?.into_iter().map(|x| x as usize).collect();
        Self::drop_broken(slf, &broken, &rot)?;
        Self::rebuild(slf)
    }

    // ---- 関節つき剛体（完全剛体モードでアクチュエーターが動く間）の Rust 版 ----
    pub(crate) fn _native_art_topo<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let a = Self::art_req(slf)?.into_bound(py);
        let ex: Vec<Bound<'py, PyAny>> = a.borrow().exts.bind(py).clone().into_any().iter()?.collect::<R<_>>()?;
        let ne = ex.len();
        let mut rows = vec![];
        let mut nbs = vec![];
        let mut e0 = vec![];
        for e in &ex {
            rows.extend(iv(&e.get_item(1)?)?);
            nbs.extend(fv(&e.get_item(2)?)?);
            e0.push(e.get_item(3)?.extract::<f64>()?);
        }
        let i = |k: &str, w: usize| -> R<Bound<'py, PyAny>> {
            let v = iv(&a.as_any().get_item(k)?)?;
            let n = if w == 0 { v.len() } else { v.len() / w };
            iarr(py, v, &if w == 0 { vec![n] } else { vec![n, w] })
        };
        let cls = slf.get_type();
        let kw = PyDict::new_bound(py);
        if bv(&a.borrow().fixed.bind(py).clone().into_any())?.iter().any(|x| *x) {
            kw.set_item("fixed", cint(&a.borrow().fixed.bind(py).clone().into_any())?)?;
            kw.set_item("xfix", cflt(a.borrow().xfix.as_ref().ok_or_else(|| PyValueError::new_err("Xfix"))?.bind(py).as_any())?)?;
        }
        let args = PyTuple::new_bound(py, [i("vg", 0)?, cflt(&a.borrow().inv_m.bind(py).clone().into_any())?, cflt(&a.borrow().iinv_b.bind(py).clone().into_any())?, i("ra", 0)?, i("rb", 0)?,
            i("A", 4)?, i("B", 4)?, i("C", 8)?, iarr(py, rows, &[ne, 4])?, arr(py, nbs, &[ne, 3])?, arr(py, e0, &[ne])?,
            cls.getattr("ART_ITERS")?, cls.getattr("ART_CONTACTS")?, cls.getattr("MOTOR_STEP_X")?.call_method0("__float__")?,
            cls.getattr("MOTOR_HOLD_DEG")?.call_method0("__float__")?, i("HK", 2)?]);
        core(py)?.getattr("ArtTopo")?.call(args, if kw.is_empty() { None } else { Some(&kw) })
    }

    pub(crate) fn _art_contig(slf: &Bound<'_, Self>) -> R<()> {
        Self::art_req(slf)?.borrow_mut(slf.py()).contig(slf.py())
    }

    /// 関節つき剛体の1サブステップの前半（Rust）。MOD の箱との接触だけ集めて渡す（MOD のプログラムを呼ぶため）
    pub(crate) fn _native_art_begin(slf: &Bound<'_, Self>, h: f64, planets: &Bound<'_, PyAny>, gscale: f64) -> R<bool> {
        let py = slf.py();
        let a = Self::art_fresh(slf)?;
        let nat = a.borrow().nat.bind(py).clone().into_any();
        let body = Self::_native_body(slf)?;
        let pls = construct_native_planets(py, planets)?.into_bound(py);
        Self::ensure_prev(slf)?;
        Self::_contig(slf, &["pos", "vel", "_prev", "force"])?;
        Self::_art_contig(slf)?;
        let cav = Self::cav_arg(slf, &pls)?;
        let (ext_want, ext_speed) = Self::ext_targets(slf, &a)?;
        let (pos, vel, prev, force) = (Self::g_pos(slf)?, Self::g_vel(slf)?, Self::g_prev(slf)?, Self::g_force(slf)?);
        let args: Vec<PyObject> = vec![body.clone().unbind(), pos.clone().unbind(), vel.unbind(), prev.clone().unbind(), force.unbind(),
            a.borrow().x.bind(py).clone().into_any().unbind(), a.borrow().r.bind(py).clone().into_any().unbind(), a.borrow().vl.bind(py).clone().into_any().unbind(), a.borrow().vl0.bind(py).clone().into_any().unbind(),
            a.borrow().ext_applied.bind(py).clone().into_any().unbind(), ext_want.into_py(py), ext_speed.into_py(py), pls.clone().unbind(), gscale.into_py(py),
            h.into_py(py), cav.unbind()];
        let pr = nat.call_method1("predict", PyTuple::new_bound(py, args))?;
        let (rows, d) = (pr.get_item(0)?, pr.get_item(1)?);
        let defo = PyList::empty_bound(py);
        if rows.len()? > 0 {
            defo.append((rows, d))?;
        }
        a.borrow_mut().defo = defo.into_any().unbind();
        // MOD の箱（街の建物など）との接触は、MOD のプログラムを呼ぶので集める
        let (boxes, cat) = Self::collect_boxes(slf, h)?;
        let (want, spd) = Self::rot_targets(&a)?;
        let args: Vec<PyObject> = vec![body.unbind(), pos.unbind(), prev.unbind(), a.borrow().x.bind(py).clone().into_any().unbind(), a.borrow().r.bind(py).clone().into_any().unbind(),
            a.borrow().vl.bind(py).clone().into_any().unbind(), a.borrow().applied.bind(py).clone().into_any().unbind(), want.into_py(py), spd.into_py(py), pls.unbind(), h.into_py(py),
            cat[0].clone_ref(py), cat[1].clone_ref(py), cat[2].clone_ref(py), cat[3].clone_ref(py), cat[4].clone_ref(py)];
        let sv = nat.call_method1("solve", PyTuple::new_bound(py, args))?;
        a.borrow_mut().contacts = (sv.get_item(0)?, sv.get_item(1)?, sv.get_item(2)?, sv.get_item(3)?).into_py(py);
        // 動いている街（ステーション）を押した分の反動
        let pts = v3s(&Self::g_pos(slf)?)?;
        let cp = iv(&sv.get_item(4)?)?;
        let pts_at: Vec<V3> = cp.iter().map(|&i| pts[i as usize]).collect();
        Self::push_back_events(slf, &boxes, &sv.get_item(5)?, &sv.get_item(3)?, &sv.get_item(2)?, &pts_at, h)?;
        Self::cache_put(slf, |c| &mut c.nb_art, true)?;
        Ok(true)
    }

    pub(crate) fn _native_art_end(slf: &Bound<'_, Self>, h: f64) -> R<()> {
        let py = slf.py();
        let a = Self::art_req(slf)?.into_bound(py);
        Self::_contig(slf, &["pos", "vel", "_prev"])?;
        Self::_art_contig(slf)?;
        let defo = a.borrow().defo.bind(py).clone();
        let (rows, d) = if defo.is_truthy()? {
            let e = defo.get_item(0)?;
            (e.get_item(0)?, e.get_item(1)?)
        } else {
            (iarr(py, vec![], &[0])?, zeros(py, &[0, 3])?)
        };
        let ct = a.borrow().contacts.bind(py).clone();
        let ct: Vec<Bound<'_, PyAny>> = if ct.is_none() {
            vec![iarr(py, vec![], &[0])?, zeros(py, &[0, 3])?, zeros(py, &[0, 3])?, zeros(py, &[0])?]
        } else {
            ct.iter()?.collect::<R<_>>()?
        };
        let args: Vec<PyObject> = vec![Self::_native_body(slf)?.unbind(), Self::g_pos(slf)?.unbind(), Self::g_vel(slf)?.unbind(),
            Self::g_prev(slf)?.unbind(), a.borrow().x.bind(py).clone().into_any().unbind(), a.borrow().r.bind(py).clone().into_any().unbind(), a.borrow().vl.bind(py).clone().into_any().unbind(), rows.unbind(),
            d.unbind(), ct[0].clone().unbind(), ct[1].clone().unbind(), ct[2].clone().unbind(), ct[3].clone().unbind(), h.into_py(py)];
        a.borrow().nat.bind(py).clone().into_any().call_method1("end", PyTuple::new_bound(py, args))?;
        let mut b = a.borrow_mut();
        b.defo = PyList::empty_bound(py).into_any().unbind();
        b.contacts = py.None();
        Ok(())
    }

    pub(crate) fn _native_full_contacts(slf: &Bound<'_, Self>, body: &Bound<'_, PyAny>, pls: &Bound<'_, PyAny>, h: f64) -> R<()> {
        let py = slf.py();
        let (boxes, cat) = Self::collect_boxes(slf, h)?;
        let imp_s: f64 = slf.get_type().getattr("IMPACT_SPEED")?.extract()?;
        let args: Vec<PyObject> = vec![Self::g_pos(slf)?.unbind(), Self::g_prev(slf)?.unbind(), Self::g_vel(slf)?.unbind(),
            pls.clone().unbind(), h.into_py(py), imp_s.into_py(py), CONTACT_SOFT.into_py(py), cat[0].clone_ref(py), cat[1].clone_ref(py),
            cat[2].clone_ref(py), cat[3].clone_ref(py), cat[4].clone_ref(py)];
        let r = body.call_method1("rigid_contacts", PyTuple::new_bound(py, args))?;
        if r.get_item(0)?.is_truthy()? {
            slf.borrow_mut().want_soft = true;
        }
        let pts = v3s(&r.get_item(5)?)?;
        Self::push_back_events(slf, &boxes, &r.get_item(4)?, &r.get_item(3)?, &r.get_item(2)?, &pts, h)
    }

    pub(crate) fn _native_end(slf: &Bound<'_, Self>, h: f64) -> R<bool> {
        let py = slf.py();
        if Self::mode_is(slf, &["rigid"]) {
            let rb = Self::rb_req(slf)?;
            rb.borrow_mut(py).contig(py)?;
            Self::_contig(slf, &["pos", "vel", "_prev"])?;
            let local = rb.borrow(py).local.clone_ref(py);
            let r = core(py)?.call_method1("rigid_end", (Self::g_pos(slf)?, Self::g_vel(slf)?, Self::g_prev(slf)?,
                local, cflt(&Self::g_mass(slf)?)?, h))?;
            let mut b = rb.borrow_mut(py);
            b.r = to_fa(&r.get_item(0)?)?;
            b.x = to_fa(&r.get_item(1)?)?;
            return Ok(false);
        }
        let body = Self::_native_body(slf)?;
        Self::_contig(slf, &["rest"])?;
        let last_r = match Self::o_last_r(slf)? {
            Some(v) => v,
            None => zeros(py, &[0])?,
        };
        let r = body.call_method1("soft_end", (Self::g_pos(slf)?, Self::g_vel(slf)?, Self::g_prev(slf)?, Self::g_rest(slf)?,
            Self::g_act_scale(slf)?, last_r, h, Self::_rotor_angles(slf)?, Self::_crest(slf, &body)?))?;
        let broken: Vec<usize> = iv(&r.get_item(0)?)?.into_iter().map(|x| x as usize).collect();
        let rot: Vec<usize> = iv(&r.get_item(1)?)?.into_iter().map(|x| x as usize).collect();
        let broke = !broken.is_empty() || !rot.is_empty();
        Self::drop_broken(slf, &broken, &rot)?;
        if broke {
            Self::rebuild(slf)?;
        }
        Ok(broke)
    }

    #[allow(non_snake_case)]
    pub(crate) fn _ensure_lastR(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let n = Self::n(slf)?;
        let need = match Self::o_last_r(slf)? {
            Some(v) => v.len()? != n,
            None => true,
        };
        if need {
            let e = eye();
            let v: Vec<f64> = (0..n).flat_map(|_| e.iter().flatten().copied().collect::<Vec<_>>()).collect();
            Self::s_last_r(slf, arr(py, v, &[n, 3, 3])?)?;
        }
        if Self::cache_get(slf, |c| &c.allvox)?.map(|v| v.len().map(|l| l != n)).transpose()?.unwrap_or(true) {
            Self::cache_put(slf, |c| &mut c.allvox, iarr(py, (0..n as i64).collect(), &[n])?)?;
        }
        if Self::o_quat(slf)?.map(|v| v.len().map(|l| l != n)).transpose()?.unwrap_or(true) {
            let v: Vec<f64> = (0..n).flat_map(|_| [1.0, 0.0, 0.0, 0.0]).collect();
            Self::s_quat(slf, arr(py, v, &[n, 4])?)?;
        }
        Ok(())
    }
}
