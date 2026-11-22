//! 構築物：保存の形（dict）との行き来

use super::*;

#[pymethods]
impl Construct {
    pub(crate) fn to_dict<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let o = PyDict::new_bound(py);
        o.set_item("id", Self::g_id(slf)?)?;
        o.set_item("name", Self::g_name(slf)?)?;
        o.set_item("voxel_size", Self::sz(slf))?;
        let reg = Self::g_reg(slf)?;
        let vox = PyList::empty_bound(py);
        let cells = Self::cells_v(slf)?;
        for ((c, m), d) in cells.iter().zip(Self::g_mats(slf)?.iter()?).zip(Self::g_devs(slf)?.iter()?) {
            vox.append(PyList::new_bound(py, [c[0].into_py(py), c[1].into_py(py), c[2].into_py(py), m?.unbind(),
                construct_clean_dev(py, &d?, Some(&reg))?]))?;
        }
        o.set_item("voxels", vox)?;
        let ls = PyList::new_bound(py, Self::g_links(slf)?.iter()?.collect::<R<Vec<_>>>()?);
        ls.sort()?;
        let ll = PyList::empty_bound(py);
        for k in ls.iter() {
            ll.append([PyList::new_bound(py, k.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?),
                       PyList::new_bound(py, k.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?)])?;
        }
        o.set_item("links", ll)?;
        let ex = PyList::empty_bound(py);
        for (k, v) in Self::g_extras(slf)?.downcast::<PyDict>()?.iter() {
            ex.append(PyList::new_bound(py, [PyList::new_bound(py, k.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?).into_any(),
                PyList::new_bound(py, k.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any(), v]))?;
        }
        o.set_item("extras", ex)?;
        o.set_item("storage", Self::g_storage(slf)?)?;
        let ca = PyList::empty_bound(py);
        for (k, v) in Self::g_cavity_air(slf)?.downcast::<PyDict>()?.iter() {
            ca.append(PyList::new_bound(py, [PyList::new_bound(py, k.iter()?.collect::<R<Vec<_>>>()?).into_any(), v]))?;
        }
        o.set_item("cavity_air", ca)?;
        // 現在の形（回転したアーム・伸びたアクチュエーター・変形）を保存
        let (r, t) = Self::fit(slf, None)?;
        let rnd = |x: f64| (x * 1e4).round_ties_even() / 1e4;
        let verts = PyList::empty_bound(py);
        for p in Self::pos_v(slf)? {
            let l = mul_r(sub(p, t), &r);
            verts.append([rnd(l[0]), rnd(l[1]), rnd(l[2])])?;
        }
        o.set_item("vertices", verts)?;
        let rest = v3s(&Self::g_rest(slf)?)?;
        let rl = PyList::empty_bound(py);
        for blk in rest.chunks_exact(8) {
            rl.append(PyList::new_bound(py, blk.iter().map(|q| [rnd(q[0]), rnd(q[1]), rnd(q[2])].into_py(py))))?;
        }
        o.set_item("rest", rl)?;
        let anc = Self::g_anchor(slf)?;
        if anc.is_truthy()? {
            o.set_item("anchor", anc.get_item("to")?)?;
        }
        if Self::t_soft_ok(slf)? {
            o.set_item("soft_ok", true)?;
        }
        Ok(o)
    }

    pub(crate) fn spawn_dict<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let (r, t) = Self::fit(slf, None)?;
        let o = PyDict::new_bound(py);
        o.set_item("position", t.to_vec())?;
        o.set_item("rotation", r.iter().map(|x| x.to_vec()).collect::<Vec<_>>())?;
        o.set_item("velocity", Self::velocity(slf)?.call_method0("tolist")?)?;
        o.set_item("angular_velocity", Self::angular_velocity(slf)?.call_method0("tolist")?)?;
        Ok(o)
    }

    #[classmethod]
    pub(crate) fn from_dict<'py>(cls: &Bound<'py, PyType>, registry: &Bound<'py, PyAny>, data: &Bound<'py, PyAny>, spawn: &Bound<'py, PyAny>)
        -> R<Bound<'py, PyAny>> {
        let py = cls.py();
        let vox = PyList::empty_bound(py);
        for v in data.get_item("voxels")?.iter()? {
            let v = v?;
            vox.append(((v.get_item(0)?, v.get_item(1)?, v.get_item(2)?), v.get_item(3)?, v.get_item(4)?))?;
        }
        let links: PyObject = if data.contains("links")? {
            let s = PySet::empty_bound(py)?;
            for l in data.get_item("links")?.iter()? {
                let l = l?;
                s.add(construct_link_key(py, &l.get_item(0)?, &l.get_item(1)?)?)?;
            }
            s.into_any().unbind()
        } else { py.None() };
        let zero = PyList::new_bound(py, [0, 0, 0]);
        let kw = PyDict::new_bound(py);
        kw.set_item("size", dget(data, "voxel_size", 1.0)?)?;
        kw.set_item("links", links)?;
        kw.set_item("id_", dget(data, "id", py.None())?)?;
        kw.set_item("name", dget(data, "name", "構築物")?)?;
        kw.set_item("defer", true)?;
        let c = cls.call_method("from_voxels", (registry, vox, dget(spawn, "position", &zero)?, dget(spawn, "rotation", py.None())?,
            dget(spawn, "velocity", &zero)?, dget(spawn, "angular_velocity", &zero)?), Some(&kw))?;
        let cs = c.downcast::<Construct>()?;
        let extras = Self::g_extras(cs)?;
        for e in dget(data, "extras", PyList::empty_bound(py))?.iter()? {
            let e = e?;
            let lst = PyList::empty_bound(py);
            for x in e.get_item(2)?.iter()? {
                lst.append(PyTuple::new_bound(py, x?.iter()?.collect::<R<Vec<_>>>()?))?;
            }
            extras.set_item(construct_link_key(py, &e.get_item(0)?, &e.get_item(1)?)?, lst)?;
        }
        Self::s_storage(cs, py.get_type_bound::<PyDict>().call1((dget(data, "storage", PyDict::new_bound(py))?,))?)?;
        let ca = PyDict::new_bound(py);
        for kv in dget(data, "cavity_air", PyList::empty_bound(py))?.iter()? {
            let kv = kv?;
            ca.set_item(PyTuple::new_bound(py, kv.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?), kv.get_item(1)?)?;
        }
        Self::s_cavity_air(cs, ca)?;
        let n = Self::n(cs)?;
        let rot = dget(spawn, "rotation", py.None())?;
        let rm = if rot.is_none() { eye() } else { m3(&rot)? };
        let p0 = v3(&dget(spawn, "position", &zero)?)?;
        let verts = dget(data, "vertices", py.None())?;
        if !verts.is_none() && verts.len()? == 8 * n {
            let vv = v3s(&verts)?;
            let pos: Vec<V3> = vv.iter().map(|v| add(mul_t(*v, &rm), p0)).collect();
            Self::s_pos(cs, a3(py, &pos)?)?;
        }
        let rest = dget(data, "rest", py.None())?;
        if !rest.is_none() && rest.len()? == n {
            Self::s_rest(cs, arr(py, fv(&rest)?, &[n, 8, 3])?)?;
        }
        Self::rebuild(cs)?;
        let w = v3(&dget(spawn, "angular_velocity", &zero)?)?;
        if w.iter().any(|x| *x != 0.0) {
            Self::spin(cs, w)?;
        }
        let anc = dget(data, "anchor", py.None())?;
        if anc.is_truthy()? {
            let d = PyDict::new_bound(py);
            d.set_item("to", anc)?;          // 動く物体（軌道上の街など）に固定。中身は拡張が決める
            Self::s_anchor(cs, d)?;
        }
        if dget(data, "soft_ok", py.None())?.is_truthy()? {
            c.setattr("soft_ok", true)?;
        }
        Ok(c)
    }
}
