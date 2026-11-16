//! 構築物：ボクセルを足す・消す・分ける・合わせる・向きを直す

use super::*;

#[pymethods]
impl Construct {
    // ------------------------------------------------------------------ 編集
    /// positions: (8,3) 新しいボクセルの頂点位置。隣接ボクセルと近ければ結合する
    pub(crate) fn add_voxel<'py>(slf: &Bound<'py, Self>, cell: &Bound<'py, PyAny>, mat_: &Bound<'py, PyAny>, dev: &Bound<'py, PyAny>,
                      positions: &Bound<'py, PyAny>) -> R<PyObject> {
        let py = slf.py();
        let mut c = [0i64; 3];
        for (q, x) in cell.iter()?.enumerate().take(3) {
            c[q] = x?.call_method0("__int__")?.extract()?;
        }
        let index = Self::g_index(slf)?;
        if index.contains(tup3(py, c))? {
            return Ok(false.into_py(py));
        }
        Self::g_cells(slf)?.call_method1("append", (tup3(py, c),))?;
        Self::g_mats(slf)?.call_method1("append", (mat_,))?;
        let d: PyObject = if dev.is_truthy()? { Self::_init_device(slf, dev)?.into_any().unbind() } else { py.None() };
        Self::g_devs(slf)?.call_method1("append", (d,))?;
        let pnew = v3s(positions)?;
        let mut pos = Self::pos_v(slf)?;
        let mut vel = Self::vel_v(slf)?;
        pos.extend(pnew.iter().copied());
        let n = Self::n(slf)?;
        let mut v = [0.0; 3];
        if n > 1 {
            let ctr = mean8(&pnew);
            let mut best = (f64::INFINITY, 0usize);
            for j in 0..n - 1 {
                let dd = norm(sub(mean8(&pos[8 * j..8 * j + 8]), ctr));
                if dd < best.0 {
                    best = (dd, j);
                }
            }
            v = mean8(&vel[8 * best.1..8 * best.1 + 8]);
        }
        vel.extend(std::iter::repeat(v).take(8));
        Self::s_pos(slf, a3(py, &pos)?)?;
        Self::s_vel(slf, a3(py, &vel)?)?;
        let size = Self::sz(slf);
        let mut rest = fv(&Self::g_rest(slf)?)?;
        for cb in CORNER_BITS.iter() {
            rest.extend(cb.iter().map(|b| (b - 0.5) * size));
        }
        Self::s_rest(slf, arr(py, rest, &[n, 8, 3])?)?;
        let mut asc = fv(&Self::g_act_scale(slf)?)?;
        asc.extend([1.0, 1.0, 1.0]);
        Self::s_act_scale(slf, arr(py, asc, &[n, 3])?)?;
        let i = n - 1;
        let links = Self::g_links(slf)?;
        for d in 0..6 {
            let nb = [c[0] + DIRS[d][0], c[1] + DIRS[d][1], c[2] + DIRS[d][2]];
            let j = match Self::idx_of(&index, tup3(py, nb).as_any())? {
                Some(j) => j,
                None => continue,
            };
            let mut dist = f64::NEG_INFINITY;
            for (ca, cb) in joint_pairs(d) {
                dist = dist.max(norm(sub(pnew[ca], pos[8 * j + cb])));
            }
            if dist < size * 0.35 {
                links.call_method1("add", (link_key_c(py, c, nb),))?;
            }
        }
        Self::s_force(slf, zeros(py, &[8 * n, 3])?)?;
        Self::rebuild(slf)?;
        Ok(i.into_py(py))
    }

    pub(crate) fn remove_voxels<'py>(slf: &Bound<'py, Self>, idxs: &Bound<'py, PyAny>) -> R<Bound<'py, PyList>> {
        let py = slf.py();
        let n = Self::n(slf)?;
        let mut keep = vec![true; n];
        let cl = Self::g_cells(slf)?;
        let removed = PyList::empty_bound(py);
        let idv: Vec<i64> = idxs.iter()?.map(|x| x?.extract::<i64>()).collect::<R<_>>()?;
        for &i in &idv {
            let i = if i < 0 { i + n as i64 } else { i } as usize;
            keep[i] = false;
        }
        for &i in &idv {
            removed.append(cl.get_item(i)?)?;
        }
        Self::apply_keep(slf, &keep)?;
        let rs = PySet::new_bound(py, &removed.iter().collect::<Vec<_>>())?;
        let nl = PySet::empty_bound(py)?;
        for k in Self::g_links(slf)?.iter()? {
            let k = k?;
            if !rs.contains(k.get_item(0)?)? && !rs.contains(k.get_item(1)?)? {
                nl.add(k)?;
            }
        }
        Self::s_links(slf, nl)?;
        Self::rebuild(slf)?;
        Ok(removed)
    }
    pub(crate) fn _apply_keep(slf: &Bound<'_, Self>, keep: &Bound<'_, PyAny>) -> R<()> {
        let k = bv(keep)?;
        Self::apply_keep(slf, &k)
    }

    /// 結合で繋がっているボクセル群に分ける
    pub(crate) fn components(slf: &Bound<'_, Self>) -> R<Vec<Vec<usize>>> {
        let n = Self::n(slf)?;
        let mut parent: Vec<usize> = (0..n).collect();
        let index = Self::g_index(slf)?;
        for k in Self::link_list(slf, true)? {
            let (a, b) = (Self::idx_of(&index, &k.get_item(0)?)?, Self::idx_of(&index, &k.get_item(1)?)?);
            if let (Some(a), Some(b)) = (a, b) {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb {
                    parent[ra] = rb;
                }
            }
        }
        let mut order: Vec<usize> = vec![];
        let mut groups: std::collections::HashMap<usize, Vec<usize>> = Default::default();
        for i in 0..n {
            let r = find(&mut parent, i);
            groups.entry(r).or_insert_with(|| { order.push(r); vec![] }).push(i);
        }
        Ok(order.into_iter().map(|r| groups.remove(&r).unwrap()).collect())
    }

    /// ちぎれた部分を別の構築物に分離する。新しい構築物のリストを返す
    pub(crate) fn split<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyList>> {
        let py = slf.py();
        let mut comps = Self::components(slf)?;
        let out = PyList::empty_bound(py);
        if comps.len() <= 1 {
            return Ok(out);
        }
        comps.sort_by(|a, b| b.len().cmp(&a.len()));
        let cells: Vec<Bound<'py, PyAny>> = Self::g_cells(slf)?.iter()?.collect::<R<_>>()?;
        let mats: Vec<Bound<'py, PyAny>> = Self::g_mats(slf)?.iter()?.collect::<R<_>>()?;
        let devs: Vec<Bound<'py, PyAny>> = Self::g_devs(slf)?.iter()?.collect::<R<_>>()?;
        let pos = Self::pos_v(slf)?;
        let vel = Self::vel_v(slf)?;
        let rest = fv(&Self::g_rest(slf)?)?;
        let asc = fv(&Self::g_act_scale(slf)?)?;
        let soft_ok = Self::t_soft_ok(slf)?;
        let name = format!("{}の破片", Self::g_name(slf)?.str()?);
        let links: Vec<Bound<'py, PyAny>> = Self::g_links(slf)?.iter()?.collect::<R<_>>()?;
        let extras = Self::g_extras(slf)?;
        for comp in &comps[1..] {
            let kw = PyDict::new_bound(py);
            kw.set_item("size", Self::sz(slf))?;
            kw.set_item("name", &name)?;
            let sub_ = py.get_type_bound::<Construct>().call((Self::g_reg(slf)?,), Some(&kw))?;
            let sb = sub_.downcast::<Construct>()?;
            if soft_ok {
                sub_.setattr("soft_ok", true)?;        // 破片も柔らかいまま（完全剛体モードの中の見せ場）
            }
            let sc = PyList::new_bound(py, comp.iter().map(|&i| cells[i].clone()));
            Self::s_cells(sb, &sc)?;
            Self::s_mats(sb, PyList::new_bound(py, comp.iter().map(|&i| mats[i].clone())))?;
            Self::s_devs(sb, PyList::new_bound(py, comp.iter().map(|&i| devs[i].clone())))?;
            let pv: Vec<V3> = comp.iter().flat_map(|&i| pos[8 * i..8 * i + 8].to_vec()).collect();
            let vv: Vec<V3> = comp.iter().flat_map(|&i| vel[8 * i..8 * i + 8].to_vec()).collect();
            Self::s_pos(sb, a3(py, &pv)?)?;
            Self::s_vel(sb, a3(py, &vv)?)?;
            let k = comp.len();
            Self::s_rest(sb, arr(py, comp.iter().flat_map(|&i| rest[24 * i..24 * i + 24].to_vec()).collect(), &[k, 8, 3])?)?;
            Self::s_act_scale(sb, arr(py, comp.iter().flat_map(|&i| asc[3 * i..3 * i + 3].to_vec()).collect(), &[k, 3])?)?;
            let cs = PySet::new_bound(py, &sc.iter().collect::<Vec<_>>())?;
            let sl = PySet::empty_bound(py)?;
            for l in &links {
                if cs.contains(l.get_item(0)?)? && cs.contains(l.get_item(1)?)? {
                    sl.add(l)?;
                }
            }
            Self::s_links(sb, sl)?;
            let se = PyDict::new_bound(py);
            for (kk, v) in extras.downcast::<PyDict>()?.iter() {
                if cs.contains(kk.get_item(0)?)? && cs.contains(kk.get_item(1)?)? {
                    se.set_item(kk, v)?;
                }
            }
            Self::s_extras(sb, se)?;
            Self::s_force(sb, zeros(py, &[8 * k, 3])?)?;
            Self::rebuild(sb)?;
            out.append(sub_)?;
        }
        let mut keep = vec![false; Self::n(slf)?];
        for &i in &comps[0] {
            keep[i] = true;
        }
        Self::apply_keep(slf, &keep)?;
        let cs = PySet::new_bound(py, &Self::g_cells(slf)?.iter()?.collect::<R<Vec<_>>>()?)?;
        let nl = PySet::empty_bound(py)?;
        for l in &links {
            if cs.contains(l.get_item(0)?)? && cs.contains(l.get_item(1)?)? {
                nl.add(l)?;
            }
        }
        Self::s_links(slf, nl)?;
        Self::rebuild(slf)?;
        Ok(out)
    }

    /// 別の構築物を取り込む。cell_map: other のセル → このセルの対応。
    /// xform: {ボクセル番号: (頂点の並べ替え perm, 旧ローカル→新ローカルの回転 M)}（90°単位で向きを揃える時）
    #[pyo3(signature = (other, cell_map, xform=None))]
    pub(crate) fn absorb(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>, cell_map: &Bound<'_, PyAny>, xform: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        let index = Self::g_index(slf)?;
        for c in other.getattr("cells")?.iter()? {
            if index.contains(cell_map.get_item(c?)?)? {
                return Err(PyValueError::new_err("セルが重なっています"));
            }
        }
        if let Some(x) = xform {
            if x.is_truthy()? {
                other.call_method1("_reorient", (x,))?;
            }
        }
        let mapped: Vec<Bound<'_, PyAny>> = other.getattr("cells")?.iter()?.map(|c| cell_map.get_item(c?)).collect::<R<_>>()?;
        Self::g_cells(slf)?.call_method1("extend", (PyList::new_bound(py, mapped),))?;
        Self::g_mats(slf)?.call_method1("extend", (other.getattr("mats")?,))?;
        Self::g_devs(slf)?.call_method1("extend", (other.getattr("devs")?,))?;
        let n = Self::n(slf)?;
        let cat = |k: &str, w: &[usize]| -> R<()> {
            let mut a = fv(&Self::g(slf, k)?)?;
            a.extend(fv(&other.getattr(k)?)?);
            let mut sh = vec![n * if k == "pos" || k == "vel" { 8 } else { 1 }];
            sh.extend_from_slice(w);
            Self::set(slf, k, arr(py, a, &sh)?)
        };
        cat("pos", &[3])?;
        cat("vel", &[3])?;
        cat("rest", &[8, 3])?;
        cat("act_scale", &[3])?;
        let links = Self::g_links(slf)?;
        for k in other.getattr("links")?.iter()? {
            let k = k?;
            let (a, b) = (cell_of(&cell_map.get_item(k.get_item(0)?)?)?, cell_of(&cell_map.get_item(k.get_item(1)?)?)?);
            links.call_method1("add", (link_key_c(py, a, b),))?;
        }
        let extras = Self::g_extras(slf)?;
        for (k, v) in other.getattr("extras")?.downcast::<PyDict>()?.iter() {
            let (a, b) = (cell_of(&cell_map.get_item(k.get_item(0)?)?)?, cell_of(&cell_map.get_item(k.get_item(1)?)?)?);
            extras.set_item(link_key_c(py, a, b), PyList::new_bound(py, v.iter()?.collect::<R<Vec<_>>>()?))?;
        }
        let st = Self::g_storage(slf)?;
        for (k, v) in other.getattr("storage")?.downcast::<PyDict>()?.iter() {
            let cur = st.call_method1("get", (&k, 0.0))?;
            st.set_item(&k, cur.add(v)?)?;
        }
        let ca = Self::g_cavity_air(slf)?;
        for (k, v) in other.getattr("cavity_air")?.downcast::<PyDict>()?.iter() {
            ca.set_item(cell_map.call_method1("get", (&k, &k))?, v)?;
        }
        Self::s_force(slf, zeros(py, &[8 * n, 3])?)?;
        Self::rebuild(slf)
    }

    /// ブロック keep の今の位置から格子の置き方を求め、残りのブロックをその格子の位置に置き直す
    pub(crate) fn snap_to_lattice(slf: &Bound<'_, Self>, keep: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let mut keep: Vec<i64> = iv(keep)?;
        let n = Self::n(slf)?;
        if keep.is_empty() || keep.len() == n {
            return Ok(());
        }
        let ks: std::collections::BTreeSet<i64> = keep.iter().copied().collect();
        let anc: Vec<i64> = iv(&Self::anchor_index(slf)?)?.into_iter().filter(|a| ks.contains(a)).collect::<std::collections::BTreeSet<_>>()
            .into_iter().collect();
        let fit: Vec<i64> = if anc.is_empty() { keep.clone() } else {
            // 固定されている部分（アクチュエーターの先で回っている所は除く）で格子を求める
            let mut s = std::collections::BTreeSet::new();
            for a in &anc {
                s.extend(iv(&Self::part_of(slf, *a as usize)?)?);
            }
            s.into_iter().collect()
        };
        let (r, t) = Self::fit(slf, Some(&fit.iter().map(|x| *x as usize).collect::<Vec<_>>()))?;
        let cells = Self::cells_v(slf)?;
        keep.retain(|_| true);
        let span = keep.iter().map(|&k| cells[k as usize].iter().map(|x| x.abs()).max().unwrap()).max().unwrap() as f64 + 50.0;
        let others: Vec<usize> = (0..n).filter(|i| !ks.contains(&(*i as i64)))
            .filter(|&i| (cells[i].iter().map(|x| x.abs()).max().unwrap() as f64) < span).collect();
        if others.is_empty() {
            return Ok(());
        }
        let size = Self::sz(slf);
        let pos_o = Self::g_pos(slf)?;
        let mut pos = v3s(&pos_o)?;
        let vel_o = Self::g_vel(slf)?;
        let mut vel = v3s(&vel_o)?;
        for &i in &others {
            for (c, cb) in CORNER_BITS.iter().enumerate() {
                let l = [(cells[i][0] as f64 + cb[0]) * size, (cells[i][1] as f64 + cb[1]) * size, (cells[i][2] as f64 + cb[2]) * size];
                pos[8 * i + c] = add(mul_t(l, &r), t);
                vel[8 * i + c] = [0.0; 3];
            }
        }
        write_back(slf, "pos", &pos_o, pos.iter().flatten().copied().collect(), &[8 * n, 3])?;
        write_back(slf, "vel", &vel_o, vel.iter().flatten().copied().collect(), &[8 * n, 3])?;
        if let Some(pr) = Self::o_prev(slf)? {
            if pr.len()? == 8 * n {
                let mut p = v3s(&pr)?;
                for &i in &others {
                    for c in 0..8 {
                        p[8 * i + c] = pos[8 * i + c];
                    }
                }
                write_back(slf, "_prev", &pr, p.iter().flatten().copied().collect(), &[8 * n, 3])?;
            }
        }
        let _ = py;
        Self::wake(slf, true)
    }

    /// ボクセルの頂点番号と向きを、指定の回転で並べ替える（格子に揃えるため）
    pub(crate) fn _reorient(slf: &Bound<'_, Self>, xform: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let pos_o = Self::g_pos(slf)?;
        let vel_o = Self::g_vel(slf)?;
        let rest_o = Self::g_rest(slf)?;
        let asc_o = Self::g_act_scale(slf)?;
        let mut pos = v3s(&pos_o)?;
        let mut vel = v3s(&vel_o)?;
        let mut rest = v3s(&rest_o)?;
        let mut asc = v3s(&asc_o)?;
        let cells = Self::g_cells(slf)?;
        let devs = Self::g_devs(slf)?;
        let pmap = PyDict::new_bound(py);
        for (k, v) in xform.downcast::<PyDict>()?.iter() {
            let k: usize = k.extract()?;
            let perm: Vec<usize> = iv(&v.get_item(0)?)?.into_iter().map(|x| x as usize).collect();
            let m = m3(&v.get_item(1)?)?;
            let (p0, v0, r0) = (pos[8 * k..8 * k + 8].to_vec(), vel[8 * k..8 * k + 8].to_vec(), rest[8 * k..8 * k + 8].to_vec());
            for old in 0..8 {
                pos[8 * k + perm[old]] = p0[old];
                vel[8 * k + perm[old]] = v0[old];
                rest[8 * k + perm[old]] = mul_t(r0[old], &m);
            }
            let am = [[m[0][0].abs(), m[0][1].abs(), m[0][2].abs()], [m[1][0].abs(), m[1][1].abs(), m[1][2].abs()],
                      [m[2][0].abs(), m[2][1].abs(), m[2][2].abs()]];
            asc[k] = mul_t(asc[k], &am);
            pmap.set_item(cells.get_item(k)?, v.get_item(0)?)?;
            let d = devs.get_item(k)?;
            if d.is_truthy()? {
                let di: usize = d.get_item("dir")?.extract()?;
                let dv = DIRS[di];
                let r = mul_t([dv[0] as f64, dv[1] as f64, dv[2] as f64], &m);
                let t = [r[0].round_ties_even() as i64, r[1].round_ties_even() as i64, r[2].round_ties_even() as i64];
                d.set_item("dir", DIRS.iter().position(|x| *x == t).map(|x| x as i64).unwrap_or(-1))?;
            }
        }
        let n = pos.len();
        write_back(slf, "pos", &pos_o, pos.iter().flatten().copied().collect(), &[n, 3])?;
        write_back(slf, "vel", &vel_o, vel.iter().flatten().copied().collect(), &[n, 3])?;
        write_back(slf, "rest", &rest_o, rest.iter().flatten().copied().collect(), &[n / 8, 8, 3])?;
        write_back(slf, "act_scale", &asc_o, asc.iter().flatten().copied().collect(), &[asc.len(), 3])?;
        let extras = Self::g_extras(slf)?;
        let items: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)> = extras.downcast::<PyDict>()?.iter().collect();
        for (key, lst) in items {
            let pa = pmap.get_item(key.get_item(0)?)?;
            let pb = pmap.get_item(key.get_item(1)?)?;
            let nl = PyList::empty_bound(py);
            for e in lst.iter()? {
                let e = e?;
                let (a, b, r) = (e.get_item(0)?, e.get_item(1)?, e.get_item(2)?);
                let a2: PyObject = match &pa { Some(p) => p.get_item(&a)?.call_method0("__int__")?.unbind(), None => a.unbind() };
                let b2: PyObject = match &pb { Some(p) => p.get_item(&b)?.call_method0("__int__")?.unbind(), None => b.unbind() };
                nl.append((a2, b2, r))?;
            }
            extras.set_item(key, nl)?;
        }
        Ok(())
    }

    /// 再結合する素材同士が近づいていたら結合を追加
    pub(crate) fn try_rebond(slf: &Bound<'_, Self>) -> R<bool> {
        let py = slf.py();
        let rbd = bv(&Self::g_v_rebond(slf)?)?;
        if !rbd.iter().any(|x| *x) {
            return Ok(false);
        }
        let idx: Vec<usize> = (0..rbd.len()).filter(|&i| rbd[i]).collect();
        let ctr = Self::centers(slf)?;
        let sel: Vec<V3> = idx.iter().map(|&i| ctr[i]).collect();
        let r = core(py)?.call_method1("pairs_within", (a3(py, &sel)?, Self::sz(slf) * 1.15))?;
        let (pa, pb) = (iv(&r.get_item(0)?)?, iv(&r.get_item(1)?)?);
        let cells = Self::cells_v(slf)?;
        let links = Self::g_links(slf)?;
        let extras = Self::g_extras(slf)?;
        let index = Self::g_index(slf)?;
        let pos = Self::pos_v(slf)?;
        let rdist = fv(&Self::g_v_rebond_dist(slf)?)?;
        let mut added = false;
        for (a, b) in pa.iter().zip(&pb) {
            let (ia, ib) = (idx[*a as usize], idx[*b as usize]);
            let key = link_key_c(py, cells[ia], cells[ib]);
            if links.contains(&key)? || extras.contains(&key)? {
                continue;
            }
            let ia: usize = index.get_item(key.get_item(0)?)?.extract()?;
            let ib: usize = index.get_item(key.get_item(1)?)?.extract()?;
            let mut dd = vec![];
            for ca in 0..8 {
                for cb in 0..8 {
                    dd.push((norm(sub(pos[8 * ia + ca], pos[8 * ib + cb])), ca * 8 + cb));
                }
            }
            let lim = rdist[ia].max(rdist[ib]);
            let mut order = dd.clone();
            order.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
            let lst = PyList::empty_bound(py);
            for (dv, o) in order.iter().take(4) {
                if *dv < lim {
                    lst.append((o / 8, o % 8, dv * 0.5))?;
                }
            }
            if !lst.is_empty() {
                extras.set_item(&key, lst)?;
                added = true;
            }
        }
        if added {
            Self::rebuild(slf)?;
        }
        Ok(added)
    }

    // ------------------------------------------------------------------ 保存
    /// ブロック i と同じ「部分」（アクチュエーターの出力面の結合をたどらずにつながっているブロック）の番号の配列
    pub(crate) fn part_of<'py>(slf: &Bound<'py, Self>, i: usize) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let key = Self::vkey(slf)?;
        let label = match Self::cache_hit(slf, |c| &c.part, key.as_any())? {
            Some(c) => iv(&c.get_item(1)?)?,
            None => {
                let n = Self::n(slf)?;
                let mut parent: Vec<usize> = (0..n).collect();
                let joints = Self::rotor_keys(slf)?;
                let index = Self::g_index(slf)?;
                for k in Self::link_list(slf, true)? {
                    if joints.contains(&k)? {
                        continue;
                    }
                    if let (Some(a), Some(b)) = (Self::idx_of(&index, &k.get_item(0)?)?, Self::idx_of(&index, &k.get_item(1)?)?) {
                        let rb = find(&mut parent, b);
                        let ra = find(&mut parent, a);
                        parent[ra] = rb;
                    }
                }
                let label: Vec<i64> = (0..n).map(|k| find(&mut parent, k) as i64).collect();
                Self::cache_put(slf, |c| &mut c.part, (key, iarr(py, label.clone(), &[n])?))?;
                label
            }
        };
        let li = label[i];
        let out: Vec<i64> = (0..label.len()).filter(|&k| label[k] == li).map(|k| k as i64).collect();
        let m = out.len();
        iarr(py, out, &[m])
    }

    /// 現在の頂点位置にもっとも合う剛体変換 (R, t)：pos ≒ R @ local + t。idx：その番号のブロックだけで合わせる
    #[pyo3(signature = (idx=None))]
    pub(crate) fn rigid_fit<'py>(slf: &Bound<'py, Self>, idx: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let sel = match idx {
            Some(i) if !i.is_none() => Some(iv(i)?.into_iter().map(|x| x as usize).collect::<Vec<_>>()),
            _ => None,
        };
        let (r, t) = Self::fit(slf, sel.as_deref())?;
        Ok(PyTuple::new_bound(py, [mat(py, &r)?, a1(py, t)]).into_any())
    }

    pub(crate) fn angular_velocity<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let m = Self::mass_v(slf)?;
        let pos = Self::pos_v(slf)?;
        let vel = Self::vel_v(slf)?;
        let com = wmean(&pos, &m);
        let vc = wmean(&vel, &m);
        let mut lm = [0.0; 3];
        let mut ii = [[0.0; 3]; 3];
        for q in 0..pos.len() {
            let r = sub(pos[q], com);
            let c = cross(r, sub(vel[q], vc));
            let rr = dot(r, r);
            for k in 0..3 {
                lm[k] += c[k] * m[q];
                for j in 0..3 {
                    ii[k][j] += m[q] * ((if k == j { rr } else { 0.0 }) - r[k] * r[j]);
                }
            }
        }
        for k in 0..3 {
            ii[k][k] += 1e-6;
        }
        Ok(a1(py, solve3(&ii, lm).unwrap_or([0.0; 3])))
    }
}
