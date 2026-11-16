//! 構築物：数・重心・速さ・形・ボクセルの場所などを調べる

use super::*;

#[pymethods]
impl Construct {
    // ------------------------------------------------------------------ 物理の値
    #[getter]
    pub(crate) fn n_vox(slf: &Bound<'_, Self>) -> R<usize> {
        Self::n(slf)
    }
    #[getter]
    pub(crate) fn n_vert(slf: &Bound<'_, Self>) -> R<usize> {
        Ok(8 * Self::n(slf)?)
    }
    pub(crate) fn center_of_mass<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let m = Self::mass_v(slf)?;
        if m.is_empty() {
            return Ok(a1(slf.py(), [0.0; 3]));
        }
        Ok(a1(slf.py(), wmean(&Self::pos_v(slf)?, &m)))
    }
    pub(crate) fn velocity<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let m = Self::mass_v(slf)?;
        if m.is_empty() {
            return Ok(a1(slf.py(), [0.0; 3]));
        }
        Ok(a1(slf.py(), wmean(&Self::vel_v(slf)?, &m)))
    }
    pub(crate) fn total_mass(slf: &Bound<'_, Self>) -> R<f64> {
        Ok(pw_sum(&Self::mass_v(slf)?))
    }

    /// アンカーのブロックの番号の配列（形が変わったときだけ数え直す）
    pub(crate) fn anchor_index<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let key = Self::vkey(slf)?;
        if let Some(c) = Self::cache_hit(slf, |c| &c.anchor, key.as_any())? {
            return c.get_item(1);
        }
        let reg = Self::g_reg(slf)?;
        let mut idx = vec![];
        for (i, d) in Self::g_devs(slf)?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? && dev_type(&reg, &d)?.as_deref() == Some("anchor") {
                idx.push(i as i64);
            }
        }
        let n = idx.len();
        let a = iarr(py, idx, &[n])?;
        Self::cache_put(slf, |c| &mut c.anchor, (key, &a))?;
        Ok(a)
    }
    #[getter]
    pub(crate) fn anchored(slf: &Bound<'_, Self>) -> R<bool> {
        Self::anchored_b(slf)
    }

    /// アクチュエーター i の両側（本体の部分と出力の先の部分）が、どちらもアンカーで固定されている（動かせない）か
    pub(crate) fn actuator_locked(slf: &Bound<'_, Self>, i: usize) -> R<bool> {
        let anc = iv(&Self::anchor_index(slf)?)?;
        if anc.is_empty() || !Self::g_devs(slf)?.get_item(i)?.is_truthy()? {
            return Ok(false);
        }
        let oc = crate::game::actuator::act_output_cell(slf.as_any(), i)?;
        let j = match Self::idx_of(&Self::g_index(slf)?, oc.as_any())? {
            Some(j) => j,
            None => return Ok(false),
        };
        Self::part_of(slf, i)?;                // 部分の番号を最新に
        let label = iv(&Self::cache_get(slf, |c| &c.part)?.ok_or_else(|| PyAttributeError::new_err("_part_cache"))?.get_item(1)?)?;
        let fixed: std::collections::HashSet<i64> = anc.iter().map(|a| label[*a as usize]).collect();
        Ok(fixed.contains(&label[i]) && fixed.contains(&label[j]))
    }

    /// 形と置き場所が変わっていなければ同じになる値（止まっている物の計算の使い回しに）
    pub(crate) fn _pose_key<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let n = Self::n(slf)?;
        let v = Self::ver(slf);
        if n == 0 {
            return Ok(PyTuple::new_bound(py, [v.into_py(py), 0.into_py(py)]));
        }
        let p = Self::g_pos(slf)?;
        let f = |r: isize, c: usize| -> R<f64> { p.get_item((r, c))?.extract() };
        let mode = slf.borrow().mode.clone();
        Ok(PyTuple::new_bound(py, [v.into_py(py), n.into_py(py), mode.into_py(py), f(0, 0)?.into_py(py), f(0, 1)?.into_py(py),
            f(-1, 2)?.into_py(py), f(-1, 0)?.into_py(py)]))
    }

    /// ブロックの中心。止まっている物（アンカーで固定・眠っている）は計算を使い回す
    pub(crate) fn centers_cached<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        if !Self::still(slf)? {
            return Self::voxel_centers(slf);
        }
        let k = Self::_pose_key(slf)?;
        if let Some(c) = Self::cache_hit(slf, |c| &c.ctr, k.as_any())? {
            return c.get_item(1);
        }
        let v = Self::voxel_centers(slf)?;
        Self::cache_put(slf, |c| &mut c.ctr, (k, &v))?;
        Ok(v)
    }
    pub(crate) fn frames_cached<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        if !Self::still(slf)? {
            return Self::voxel_frames(slf);
        }
        let k = Self::_pose_key(slf)?;
        if let Some(c) = Self::cache_hit(slf, |c| &c.frm, k.as_any())? {
            return c.get_item(1);
        }
        let v = Self::voxel_frames(slf)?;
        Self::cache_put(slf, |c| &mut c.frm, (k, &v))?;
        Ok(v)
    }

    /// 外側の球 (中心, 半径)。止まっている物は使い回す
    pub(crate) fn bounds<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let k: Bound<'py, PyAny> = if Self::still(slf)? { Self::_pose_key(slf)?.into_any() } else { py.None().into_bound(py) };
        if !k.is_none() {
            if let Some(c) = Self::cache_hit(slf, |c| &c.bnd, &k)? {
                return c.get_item(1);
            }
        }
        let b = if Self::n(slf)? == 0 {
            PyTuple::new_bound(py, [a1(py, [0.0; 3]), 0.0f64.into_py(py).into_bound(py)])
        } else {
            let p = Self::pos_v(slf)?;
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for q in &p {
                for k in 0..3 {
                    lo[k] = lo[k].min(q[k]);
                    hi[k] = hi[k].max(q[k]);
                }
            }
            let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
            PyTuple::new_bound(py, [a1(py, c), (norm(sub(hi, lo)) * 0.5).into_py(py).into_bound(py)])
        };
        Self::cache_put(slf, |c| &mut c.bnd, (k, &b))?;
        Ok(b.into_any())
    }

    /// 止まっていて、毎ティック処理する機器が無い。slow_ok：ライトのように、ときどきまとめて処理すれば足りる機器だけなら True
    #[pyo3(signature = (slow_ok=false))]
    pub(crate) fn is_quiet(slf: &Bound<'_, Self>, slow_ok: bool) -> R<bool> {
        let py = slf.py();
        if !Self::is_static_b(slf)? {
            return Ok(false);
        }
        let key = Self::vkey(slf)?;
        let cache = match Self::cache_hit(slf, |c| &c.quiet, key.as_any())? {
            Some(c) => c,
            None => {
                let reg = Self::g_reg(slf)?;
                let cls = slf.get_type();
                let passive_s = cls.getattr("PASSIVE_DEVICES")?;
                let slow_s = cls.getattr("SLOW_DEVICES")?;
                let (mut passive, mut slow, mut comps) = (true, true, vec![]);
                for (i, d) in Self::g_devs(slf)?.iter()?.enumerate() {
                    let d = d?;
                    if !d.is_truthy()? {
                        continue;
                    }
                    let t = dev_type(&reg, &d)?;
                    let tp = t.clone().into_py(py);
                    let inp = passive_s.contains(&tp)?;
                    passive &= inp;
                    slow &= inp || slow_s.contains(&tp)?;
                    if t.as_deref() == Some("computer") {
                        comps.push(i);
                    }
                }
                let c = PyTuple::new_bound(py, [key.into_any(), passive.into_py(py).into_bound(py),
                    PyList::new_bound(py, comps).into_any(), slow.into_py(py).into_bound(py)]);
                Self::cache_put(slf, |c| &mut c.quiet, &c)?;
                c.into_any()
            }
        };
        if !cache.get_item(if slow_ok { 3 } else { 1 })?.is_truthy()? {
            return Ok(false);
        }
        let devs = Self::g_devs(slf)?;
        for i in cache.get_item(2)?.iter()? {
            if devs.get_item(i?)?.get_item("state")?.call_method1("get", ("running",))?.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    #[getter]
    pub(crate) fn has_devices(slf: &Bound<'_, Self>) -> R<bool> {
        let key = Self::vkey(slf)?;
        if let Some(c) = Self::cache_hit(slf, |c| &c.hasdev, key.as_any())? {
            return c.get_item(1)?.is_truthy();
        }
        let mut any = false;
        for d in Self::g_devs(slf)?.iter()? {
            if d?.is_truthy()? {
                any = true;
                break;
            }
        }
        Self::cache_put(slf, |c| &mut c.hasdev, (key, any))?;
        Ok(any)
    }

    /// (R, t)：格子の座標（セル × 大きさ）→ ワールド。剛体なら剛体の姿勢、止まっている物は使い回す
    pub(crate) fn lattice_pose<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let still = Self::still(slf)?;
        if Self::mode_is(slf, &["rigid", "rails"]) && slf.borrow().rb.is_some() && !still {
            return Self::rigid_fit(slf, None);
        }
        let k: Bound<'py, PyAny> = if still { Self::_pose_key(slf)?.into_any() } else { py.None().into_bound(py) };
        if !k.is_none() {
            if let Some(c) = Self::cache_hit(slf, |c| &c.lat, &k)? {
                return c.get_item(1);
            }
        }
        let out = Self::rigid_fit(slf, None)?;
        Self::cache_put(slf, |c| &mut c.lat, (k, &out))?;
        Ok(out)
    }

    /// 遠くから見せる「それっぽい色の箱」：(中心 (k,3), 半分の大きさ (k,3), 色 (k,3))。格子の座標（セル単位）
    pub(crate) fn lod_boxes<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let key = Self::vkey(slf)?;
        if let Some(c) = Self::cache_hit(slf, |c| &c.lod, key.as_any())? {
            return c.get_item(1);
        }
        let n = Self::n(slf)?;
        let out = if n == 0 {
            PyTuple::new_bound(py, [zeros(py, &[0, 3])?, zeros(py, &[0, 3])?, zeros(py, &[0, 3])?])
        } else {
            let cells = Self::cells_v(slf)?;
            let reg = Self::g_reg(slf)?;
            let lc: i64 = slf.get_type().getattr("LOD_CELL")?.extract()?;
            let mats: Vec<Bound<'_, PyAny>> = Self::g_mats(slf)?.iter()?.collect::<R<_>>()?;
            let devs: Vec<Bound<'_, PyAny>> = Self::g_devs(slf)?.iter()?.collect::<R<_>>()?;
            let grey = PyList::new_bound(py, [0.6, 0.6, 0.6]);
            let mut cols = vec![];
            let mut mid: Vec<(String, String)> = vec![];
            for (m, d) in mats.iter().zip(&devs) {
                let src = if d.is_truthy()? { reg.call_method1("device", (d.get_item("def")?,))? } else { reg.call_method1("material", (m,))? };
                let src = if src.is_truthy()? { src } else { PyDict::new_bound(py).into_any() };
                cols.push(v3(&dget(&src, "color", &grey)?)?);
                let dd = if d.is_truthy()? { d.call_method1("get", ("def",))?.repr()?.to_string() } else { "None".into() };
                mid.push((m.repr()?.to_string(), dd));
            }
            let key_of = |c: &[i64; 3]| [c[0].div_euclid(lc), c[1].div_euclid(lc), c[2].div_euclid(lc)];
            let mut uniq: Vec<[i64; 3]> = cells.iter().map(key_of).collect();
            uniq.sort();
            uniq.dedup();
            let inv: Vec<usize> = cells.iter().map(|c| uniq.binary_search(&key_of(c)).unwrap()).collect();
            let gn = uniq.len();
            let mut lo = vec![[i64::MAX; 3]; gn];
            let mut hi = vec![[i64::MIN; 3]; gn];
            for (c, &g) in cells.iter().zip(&inv) {
                for k in 0..3 {
                    lo[g][k] = lo[g][k].min(c[k]);
                    hi[g][k] = hi[g][k].max(c[k] + 1);
                }
            }
            let mut col = vec![[0.0; 3]; gn];
            for g in 0..gn {
                let sel: Vec<usize> = (0..n).filter(|&i| inv[i] == g).collect();
                let mut cnt: Vec<(&(String, String), usize)> = vec![];
                for &i in &sel {
                    match cnt.iter_mut().find(|x| x.0 == &mid[i]) {
                        Some(x) => x.1 += 1,
                        None => cnt.push((&mid[i], 1)),
                    }
                }
                let mx = cnt.iter().map(|x| x.1).max().unwrap_or(0);
                let best: Vec<&(String, String)> = cnt.iter().filter(|x| x.1 == mx).map(|x| x.0).collect();
                let pick: Vec<V3> = sel.iter().filter(|&&i| best.contains(&&mid[i])).map(|&i| cols[i]).collect();
                col[g] = mean8(&pick);
            }
            let c: Vec<V3> = (0..gn).map(|g| { let mut o = [0.0; 3]; for k in 0..3 { o[k] = (lo[g][k] + hi[g][k]) as f64 * 0.5; } o }).collect();
            let e: Vec<V3> = (0..gn).map(|g| { let mut o = [0.0; 3]; for k in 0..3 { o[k] = (hi[g][k] - lo[g][k]) as f64 * 0.5; } o }).collect();
            PyTuple::new_bound(py, [a3(py, &c)?, a3(py, &e)?, a3(py, &col)?])
        };
        Self::cache_put(slf, |c| &mut c.lod, (key, &out))?;
        Ok(out.into_any())
    }

    /// 構築物どうしの押し出しに使う、ブロックごとの質量の逆数。アンカーで固定されたブロックはほぼ 0
    pub(crate) fn collision_inv<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let im = fv(&Self::g_inv_mass(slf)?)?;
        let mut v: Vec<f64> = im.iter().step_by(8).copied().collect();
        if Self::anchored_b(slf)? {
            let fi: f64 = slf.get_type().getattr("FIXED_INV")?.extract()?;
            let n = Self::n(slf)?;
            let mut done = false;
            if Self::mode_is(slf, &["art"]) {
                if let Some(a) = Self::art_state(slf)? {
                    let a = a.borrow(py);
                    if a.n == n as i64 {
                        let fixed = bv(a.fixed.bind(py).as_any())?;
                        if fixed.iter().any(|x| *x) {
                            let gid = iv(a.gid.bind(py).as_any())?;
                            for (i, g) in gid.iter().enumerate() {
                                if fixed[*g as usize] {
                                    v[i] = fi;
                                }
                            }
                            done = true;
                        }
                    }
                }
            }
            if !done {
                v.iter_mut().for_each(|x| *x = fi);
            }
        }
        let n = v.len();
        arr(py, v, &[n])
    }

    /// 物理計算をせず、その場に止まっている（アンカーで固定・アクチュエーターも止まっている）
    #[getter]
    pub(crate) fn is_static(slf: &Bound<'_, Self>) -> R<bool> {
        Self::is_static_b(slf)
    }

    /// ブロック i の中心（ワールド座標）
    pub(crate) fn voxel_center<'py>(slf: &Bound<'py, Self>, i: usize) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        // そのブロックの 8 頂点だけを読む（毎フレーム音などから呼ばれ、全部の頂点を写すと大きな構築物で重い）
        let pa = slf.borrow().pos.clone_ref(py);
        if let Ok(r) = pa.bind(py).extract::<numpy::PyReadonlyArrayDyn<f64>>() {
            if let Ok(s) = r.as_slice() {
                if s.len() >= 24 * (i + 1) {
                    let mut q = [[0.0; 3]; 8];
                    for (k, row) in q.iter_mut().enumerate() {
                        row.copy_from_slice(&s[3 * (8 * i + k)..3 * (8 * i + k) + 3]);
                    }
                    return Ok(a1(py, mean8(&q)));
                }
            }
        }
        let p = Self::pos_v(slf)?;
        Ok(a1(py, mean8(&p[8 * i..8 * i + 8])))
    }
    pub(crate) fn voxel_centers<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        a3(slf.py(), &Self::centers(slf)?)
    }
    /// 各ボクセルのローカル軸 (n,3,3)。行が x,y,z 軸（単位ベクトル）
    pub(crate) fn voxel_frames<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let p = Self::pos_v(slf)?;
        let n = p.len() / 8;
        let mut out = Vec::with_capacity(9 * n);
        for b in p.chunks_exact(8) {
            for (hi, lo) in [([1, 3, 5, 7], [0, 2, 4, 6]), ([2, 3, 6, 7], [0, 1, 4, 5]), ([4, 5, 6, 7], [0, 1, 2, 3])] {
                let mut s = [0.0; 3];
                for q in 0..4 {
                    let d = sub(b[hi[q]], b[lo[q]]);
                    for k in 0..3 {
                        s[k] += d[k];
                    }
                }
                let e = [s[0] / 4.0, s[1] / 4.0, s[2] / 4.0];
                let l = ((0.0 + e[0] * e[0]) + e[1] * e[1] + e[2] * e[2]).sqrt().max(1e-9);
                out.extend([e[0] / l, e[1] / l, e[2] / l]);
            }
        }
        arr(slf.py(), out, &[n, 3, 3])
    }
}
