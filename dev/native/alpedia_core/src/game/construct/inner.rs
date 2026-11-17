//! 構築物：Rust の中だけで使う道具（剛体の動き・関節の組み・接触の箱など）

use crate::game::PyDictOps;
use super::*;

pub struct RigidMotion {
    pub x: V3,
    pub v: V3,
    pub w: V3,
    pub i: [[f64; 3]; 3],
    pub r: Vec<V3>,
}

/// np.ascontiguousarray（すでにそうならそのまま。numpy を呼ぶのは作り直すときだけ）
pub fn contig<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    if let Ok(a) = o.downcast::<numpy::PyUntypedArray>() {
        if a.is_c_contiguous() {
            return Ok(o.clone());
        }
    }
    np(o.py())?.call_method1("ascontiguousarray", (o,))
}
/// np.ascontiguousarray(o, "float64")
pub fn cflt<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    if let Ok(a) = o.downcast::<PyArrayDyn<f64>>() {
        if a.is_c_contiguous() {
            return Ok(o.clone());
        }
    }
    np(o.py())?.call_method1("ascontiguousarray", (o, "float64"))
}
/// np.ascontiguousarray(o, "int64")
pub fn cint<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    if let Ok(a) = o.downcast::<PyArrayDyn<i64>>() {
        if a.is_c_contiguous() {
            return Ok(o.clone());
        }
    }
    np(o.py())?.call_method1("ascontiguousarray", (o, "int64"))
}
/// np.asarray(o, "float64")（すでに float64 の配列ならそのまま）
pub fn as_f64<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    if o.downcast::<PyArrayDyn<f64>>().is_ok() {
        return Ok(o.clone());
    }
    np(o.py())?.call_method1("asarray", (o, "float64"))
}
/// np.asarray(o, "int64")
pub fn as_i64<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    if o.downcast::<PyArrayDyn<i64>>().is_ok() {
        return Ok(o.clone());
    }
    np(o.py())?.call_method1("asarray", (o, "int64"))
}
/// 連続した書き込める float64 でなければ作り直した配列（そうなら None）
pub fn fix_contig<'py>(o: &Bound<'py, PyAny>) -> R<Option<Bound<'py, PyAny>>> {
    if let Ok(a) = o.downcast::<numpy::PyArrayDyn<f64>>() {
        if a.is_c_contiguous() && a.getattr("flags")?.getattr("writeable")?.is_truthy()? {
            return Ok(None);
        }
    }
    Ok(Some(np(o.py())?.call_method1("array", (o, "float64"))?.call_method1("copy", ("C",))?))
}
/// 計算した値を元の配列に書き戻す（書き込めない配列なら作り直して入れる）
pub(crate) fn write_back(slf: &Bound<'_, Construct>, name: &str, o: &Bound<'_, PyAny>, data: Vec<f64>, shape: &[usize]) -> R<()> {
    let mut done = false;
    if let Ok(mut a) = o.extract::<PyReadwriteArrayDyn<f64>>() {
        if a.len() == data.len() {
            if let Ok(s) = a.as_slice_mut() {
                s.copy_from_slice(&data);
                done = true;
            } else {
                for (d, v) in a.as_array_mut().iter_mut().zip(&data) {
                    *d = *v;
                }
                done = true;
            }
        }
    }
    if !done {
        slf.as_any().setattr(name, arr(slf.py(), data, shape)?)?;
    }
    Ok(())
}

impl Construct {
    pub fn rigid_motion(slf: &Bound<'_, Self>) -> R<RigidMotion> {
        let m = Self::mass_v(slf)?;
        let pos = Self::pos_v(slf)?;
        let vel = Self::vel_v(slf)?;
        let x = wmean(&pos, &m);
        let v = wmean(&vel, &m);
        let r: Vec<V3> = pos.iter().map(|p| sub(*p, x)).collect();
        let mut l = [0.0; 3];
        let mut ii = [[0.0; 3]; 3];
        let mut mrr = Vec::with_capacity(m.len());
        for q in 0..r.len() {
            let c = cross(r[q], sub(vel[q], v));
            for k in 0..3 {
                l[k] += c[k] * m[q];
            }
            mrr.push(m[q] * dot(r[q], r[q]));
            for j in 0..3 {
                for k in 0..3 {
                    ii[j][k] -= m[q] * r[q][j] * r[q][k];
                }
            }
        }
        let s = pw_sum(&mrr);
        for k in 0..3 {
            ii[k][k] += s;
        }
        let tr = ii[0][0] + ii[1][1] + ii[2][2];
        let mut a = ii;
        for k in 0..3 {
            a[k][k] += 1e-9 * tr.max(1e-9);
        }
        let w = solve3(&a, l).unwrap_or([f64::NAN; 3]);
        Ok(RigidMotion { x, v, w, i: ii, r })
    }

    /// vel += w × (pos - 重心)
    pub(crate) fn spin(slf: &Bound<'_, Self>, w: V3) -> R<()> {
        let m = Self::mass_v(slf)?;
        let pos = Self::pos_v(slf)?;
        let com = wmean(&pos, &m);
        let vel_o = Self::g_vel(slf)?;
        let mut vel = v3s(&vel_o)?;
        for (v, p) in vel.iter_mut().zip(&pos) {
            *v = add(*v, cross(w, sub(*p, com)));
        }
        let n = vel.len();
        write_back(slf, "vel", &vel_o, vel.iter().flatten().copied().collect(), &[n, 3])
    }

    pub fn fit(slf: &Bound<'_, Self>, idx: Option<&[usize]>) -> R<([[f64; 3]; 3], V3)> {
        let cells = Self::cells_v(slf)?;
        let pos = Self::pos_v(slf)?;
        let size = Self::sz(slf);
        let sel: Vec<usize> = match idx { Some(i) => i.to_vec(), None => (0..cells.len()).collect() };
        let mut local = Vec::with_capacity(8 * sel.len());
        let mut pp = Vec::with_capacity(8 * sel.len());
        for &i in &sel {
            for (c, cb) in CORNER_BITS.iter().enumerate() {
                local.push([(cells[i][0] as f64 + cb[0]) * size, (cells[i][1] as f64 + cb[1]) * size, (cells[i][2] as f64 + cb[2]) * size]);
                pp.push(pos[8 * i + c]);
            }
        }
        let lc = mean8(&local);
        let pc = mean8(&pp);
        let mut h = nalgebra::Matrix3::<f64>::zeros();
        for (l, p) in local.iter().zip(&pp) {
            let (a, b) = (sub(*l, lc), sub(*p, pc));
            for i in 0..3 {
                for j in 0..3 {
                    h[(i, j)] += a[i] * b[j];
                }
            }
        }
        let svd = nalgebra::linalg::SVD::new(h, true, true);
        let (u, mut vt) = (svd.u.unwrap(), svd.v_t.unwrap());
        let mut r = vt.transpose() * u.transpose();
        if r.determinant() < 0.0 {
            let sv = svd.singular_values;
            let kmin = (0..3).min_by(|&a, &b| sv[a].partial_cmp(&sv[b]).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
            for j in 0..3 {
                vt[(kmin, j)] *= -1.0;
            }
            r = vt.transpose() * u.transpose();
        }
        let rm = [[r[(0, 0)], r[(0, 1)], r[(0, 2)]], [r[(1, 0)], r[(1, 1)], r[(1, 2)]], [r[(2, 0)], r[(2, 1)], r[(2, 2)]]];
        let t = sub(pc, mul_t(lc, &rm));
        Ok((rm, t))
    }

    pub fn _contig(slf: &Bound<'_, Self>, names: &[&str]) -> R<()> {
        for k in names {
            let a = Self::g(slf, k)?;
            if let Some(v) = fix_contig(&a)? {
                Self::set(slf, k, v)?;
            }
        }
        Ok(())
    }
    pub(crate) fn ensure_prev(slf: &Bound<'_, Self>) -> R<()> {
        let pos = Self::g_pos(slf)?;
        let ok = match Self::o_prev(slf)? {
            Some(p) => p.getattr("shape")?.eq(pos.getattr("shape")?)?,
            None => false,
        };
        if !ok {
            Self::s_prev(slf, pos.call_method0("copy")?)?;
        }
        Ok(())
    }
    pub fn cav_arg<'py>(slf: &Bound<'py, Self>, pls: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let mut water = false;
        for p in pls.iter()? {
            if p?.get_item(5)?.is_truthy()? {
                water = true;
                break;
            }
        }
        if !water {
            return Ok(py.None().into_bound(py));
        }
        let c = Self::_cavity_neighbors(slf)?;
        if c.is_none() {
            return Ok(c);
        }
        cint(&c.get_item(0)?)
    }
    pub(crate) fn art_fresh<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, ArtState>> {
        let py = slf.py();
        let fresh = match Self::art_state(slf)? {
            Some(a) => {
                let b = a.borrow(py);
                b.version == Self::ver(slf) && b.n == Self::n(slf)? as i64
            }
            None => false,
        };
        if !fresh {
            Self::_art_build(slf)?;
        }
        Ok(Self::art_req(slf)?.into_bound(py))
    }
    pub(crate) fn ext_targets(slf: &Bound<'_, Self>, a: &Bound<'_, ArtState>) -> R<(Vec<f64>, Vec<f64>)> {
        let py = slf.py();
        let devs = Self::g_devs(slf)?;
        let (mut w, mut s) = (vec![], vec![]);
        let exts = a.borrow().exts.clone_ref(py);
        for e in exts.bind(py).iter() {
            let d = devs.get_item(e.get_item(0)?)?;
            let st = if d.is_truthy()? { d.get_item("state")? } else { PyDict::new_bound(py).into_any() };
            w.push(st.dget2("ext", e.get_item(3)?)?.extract::<f64>()?);
            s.push(st.dget2("ext_speed", 0.5)?.extract::<f64>()?);
        }
        Ok((w, s))
    }
    pub(crate) fn rot_targets(a: &Bound<'_, ArtState>) -> R<(Vec<f64>, Vec<f64>)> {
        let (mut w, mut s) = (vec![], vec![]);
        let devs = a.borrow().devs.clone_ref(a.py());
        for dv in devs.bind(a.py()).iter() {
            let st = dv.get_item("state")?;
            w.push(st.dget2("angle", 0.0)?.extract::<f64>()?);
            s.push(st.dget2("rot_speed", 90.0)?.extract::<f64>()?);
        }
        Ok((w, s))
    }
    /// MOD の箱との接触を集める：(箱のリスト, [頂点, 深さ, 法線, 箱の速さ, 箱の番号])
    pub(crate) fn collect_boxes<'py>(slf: &Bound<'py, Self>, h: f64) -> R<(Vec<Bound<'py, PyAny>>, Vec<PyObject>)> {
        let py = slf.py();
        let boxes: Vec<Bound<'py, PyAny>> = Self::g_ext_colliders(slf)?.iter()?.collect::<R<_>>()?;
        let (mut p, mut d, mut n, mut vs, mut ci) = (vec![], vec![], vec![], vec![], vec![]);
        for (bi, bs) in boxes.iter().enumerate() {
            let res = Self::_collide_boxes(slf, bs, h, true)?;
            if res.is_truthy()? {
                let pp = iv(&res.get_item(0)?)?;
                ci.extend(std::iter::repeat(bi as i64).take(pp.len()));
                p.extend(pp);
                d.extend(fv(&res.get_item(1)?)?);
                n.extend(fv(&res.get_item(2)?)?);
                vs.extend(fv(&res.get_item(3)?)?);
            }
        }
        let k = p.len();
        Ok((boxes, vec![iarr(py, p, &[k])?.unbind(), arr(py, d, &[k])?.unbind(), arr(py, n, &[k, 3])?.unbind(),
            arr(py, vs, &[k, 3])?.unbind(), iarr(py, ci, &[k])?.unbind()]))
    }
    /// 動いている街（ステーション）を押した分の反動を ext_events に
    pub(crate) fn push_back_events(slf: &Bound<'_, Self>, boxes: &[Bound<'_, PyAny>], ci: &Bound<'_, PyAny>, lam: &Bound<'_, PyAny>,
                        cn: &Bound<'_, PyAny>, pts: &[V3], h: f64) -> R<()> {
        let py = slf.py();
        let ci = iv(ci)?;
        if ci.is_empty() || !ci.iter().any(|x| *x >= 0) {
            return Ok(());
        }
        let lam = fv(lam)?;
        let cn = v3s(cn)?;
        let mut ids: Vec<i64> = ci.iter().copied().filter(|x| *x >= 0).collect();
        ids.sort();
        ids.dedup();
        let evs = Self::g_ext_events(slf)?;
        for bi in ids {
            let sel: Vec<usize> = (0..ci.len()).filter(|&q| ci[q] == bi && lam[q] > 0.0).collect();
            if sel.is_empty() {
                continue;
            }
            let j: Vec<V3> = sel.iter().map(|&q| scl(cn[q], -(lam[q] / h))).collect();
            let p: Vec<V3> = sel.iter().map(|&q| pts[q]).collect();
            evs.call_method1("append", ((boxes[bi as usize].get_item(0)?, py.None(), a3(py, &j)?, a3(py, &p)?),))?;
        }
        Ok(())
    }
    pub(crate) fn drop_broken(slf: &Bound<'_, Self>, broken: &[usize], rot: &[usize]) -> R<()> {
        let links = Self::g_links(slf)?;
        let rotors = Self::g_rotors(slf)?;
        for &q in rot {
            links.call_method1("discard", (rotors.get_item(q)?.get_item("key")?,))?;
        }
        let jkey = Self::g_jkey(slf)?;
        let extras = Self::g_extras(slf)?;
        let py = slf.py();
        for &j in broken {
            let key = jkey.get_item(j)?;
            if key.get_item(0)?.eq("x")? {
                extras.dpop2(key.downcast::<PyTuple>()?.get_slice(1, 3), py.None())?;
            } else {
                links.call_method1("discard", (key,))?;
            }
        }
        Ok(())
    }
    pub(crate) fn art_groups(slf: &Bound<'_, Self>) -> R<Vec<usize>> {
        let n = Self::n(slf)?;
        let mut parent: Vec<usize> = (0..n).collect();
        let rk = Self::rotor_keys(slf)?;
        let index = Self::g_index(slf)?;
        for k in Self::link_list(slf, true)? {
            if rk.contains(&k)? {
                continue;
            }
            if let (Some(a), Some(b)) = (Self::idx_of(&index, &k.get_item(0)?)?, Self::idx_of(&index, &k.get_item(1)?)?) {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb {
                    parent[ra] = rb;
                }
            }
        }
        let roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
        let mut u = roots.clone();
        u.sort();
        u.dedup();
        Ok(roots.iter().map(|r| u.binary_search(r).unwrap()).collect())
    }
    pub(crate) fn apply_keep(slf: &Bound<'_, Self>, keep: &[bool]) -> R<()> {
        let py = slf.py();
        for k in ["cells", "mats", "devs"] {
            let l = Self::g(slf, k)?;
            let nl = PyList::empty_bound(py);
            for (x, kp) in l.iter()?.zip(keep) {
                if *kp {
                    nl.append(x?)?;
                }
            }
            Self::set(slf, k, nl)?;
        }
        let m = keep.iter().filter(|x| **x).count();
        let pos = Self::pos_v(slf)?;
        let vel = Self::vel_v(slf)?;
        let rest = fv(&Self::g_rest(slf)?)?;
        let asc = fv(&Self::g_act_scale(slf)?)?;
        let (mut p, mut v, mut r, mut a) = (vec![], vec![], vec![], vec![]);
        for (i, kp) in keep.iter().enumerate() {
            if *kp {
                p.extend_from_slice(&pos[8 * i..8 * i + 8]);
                v.extend_from_slice(&vel[8 * i..8 * i + 8]);
                r.extend_from_slice(&rest[24 * i..24 * i + 24]);
                a.extend_from_slice(&asc[3 * i..3 * i + 3]);
            }
        }
        Self::s_pos(slf, a3(py, &p)?)?;
        Self::s_vel(slf, a3(py, &v)?)?;
        Self::s_rest(slf, arr(py, r, &[m, 8, 3])?)?;
        Self::s_act_scale(slf, arr(py, a, &[m, 3])?)?;
        Self::s_force(slf, zeros(py, &[8 * m, 3])?)
    }
}

/// vel[:] = 0 の代わり（書き込めない配列のとき）
pub(crate) struct PySliceAll;
impl PySliceAll {
    pub(crate) fn all(py: Python<'_>) -> R<Bound<'_, PyAny>> {
        Ok(pyo3::types::PySlice::full_bound(py).into_any())
    }
}
