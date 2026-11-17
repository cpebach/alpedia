//! 描画：掘った所（CarveView）・区画の距離（ChunkDist）

use crate::game::planet::{pla, Planet};
use crate::game::PyDictOps;
use super::*;

// ---------------------------------------------------------------- CarveView
/// 洞窟や掘削跡のまわりを、距離場から作った詳細なメッシュで描く（8 m の立方体チャンクごと）。
/// 高さ場の地表（TerrainPatch）では表せない横穴・洞窟の中を見せる
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct CarveView {
    /// _last_asm（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_last_asm: Option<PyObject>,
    /// active（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_active: Option<PyObject>,
    /// anchor（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_anchor: Option<PyObject>,
    /// cache（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_cache: Option<PyObject>,
    /// dirty（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_dirty: Option<PyObject>,
    /// enabled（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_enabled: Option<PyObject>,
    /// loader（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_loader: Option<PyObject>,
    /// mesh（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_mesh: Option<PyObject>,
    /// n_craters（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_n_craters: Option<PyObject>,
    /// pending（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_pending: Option<PyObject>,
    /// planet（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_planet: Option<PyObject>,
    /// pver（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_pver: Option<PyObject>,
    /// range（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_range: Option<PyObject>,
    /// root（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_root: Option<PyObject>,
    /// version（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_version: Option<PyObject>,
    /// want（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_want: Option<PyObject>,
}

pub(crate) fn ckey(py: Python<'_>, k: [i64; 3]) -> B<'_> {
    PyTuple::new_bound(py, k).into_any()
}

#[pymethods]
impl CarveView {
    #[classattr]
    #[allow(non_snake_case)]
    pub(crate) fn CH() -> f64 {
        8.0
    }
    #[classattr]
    #[allow(non_snake_case)]
    pub(crate) fn RES() -> f64 {
        0.5
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        CarveView { a_last_asm: None, a_active: None, a_anchor: None, a_cache: None, a_dirty: None, a_enabled: None, a_loader: None, a_mesh: None, a_n_craters: None, a_pending: None, a_planet: None, a_pver: None, a_range: None, a_root: None, a_version: None, a_want: None }
    }
    #[pyo3(signature = (parent, loader, view_range=56.0))]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, loader: &B<'_>, view_range: f64) -> R<()> {
        let py = slf.py();
        Self::sa_enabled(slf, true)?;
        let root = parent.call_method1("attachNewNode", ("carve",))?;
        Self::sa_root(slf, &root)?;
        Self::sa_mesh(slf, dmesh(py, &root, "carve")?)?;
        Self::sa_loader(slf, loader)?;
        Self::sa_range(slf, view_range)?;
        Self::sa_cache(slf, PyDict::new_bound(py))?; // (チャンクの位置) → (verts, v, tris) or None
        Self::sa_active(slf, PySet::empty_bound(py)?)?; // 描画中のチャンク（惑星ローカルの整数座標）
        Self::sa_version(slf, 0)?;
        Self::sa_planet(slf, py.None())?;
        Self::sa_anchor(slf, py.None())?;
        Self::sa_pver(slf, -1)?;
        Self::sa_n_craters(slf, 0)?;
        Self::sa_pending(slf, PyList::empty_bound(py))?;
        Self::sa_dirty(slf, false)?;
        Ok(())
    }
    /// 惑星ローカル座標の点が描画中のチャンクに入っているか
    pub(crate) fn contains<'py>(slf: &Bound<'py, Self>, local_pts: &B<'py>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let active = Self::ga_active(slf)?;
        let p = a2(local_pts, 3)?;
        let npm = np(py)?;
        if !active.is_truthy()? {
            return npm.call_method1("zeros", (p.n, py.import_bound("builtins")?.getattr("bool")?));
        }
        let ch = f64of(&s.getattr("CH")?)?;
        let mut keys = std::collections::HashSet::new();
        for k in active.iter()? {
            let k: (i64, i64, i64) = k?.extract()?;
            keys.insert(k);
        }
        let out: Vec<bool> = (0..p.n).map(|i| {
            let r = p.r3(i);
            keys.contains(&((r[0] / ch).floor() as i64, (r[1] / ch).floor() as i64, (r[2] / ch).floor() as i64))
        }).collect();
        Ok(numpy::PyArray1::from_vec_bound(py, out).into_any())
    }
    pub(crate) fn _wanted<'py>(slf: &Bound<'py, Self>, pl: &B<'py>, player_pos: &B<'py>) -> R<Bound<'py, PySet>> {
        let py = slf.py();
        let s = slf.as_any();
        let pp = v3(player_pos)?;
        let c0 = v3(&pl.getattr("center")?)?;
        let local = [pp[0] - c0[0], pp[1] - c0[1], pp[2] - c0[2]];
        let rng = f64of(&Self::ga_range(slf)?)?;
        let ch = f64of(&s.getattr("CH")?)?;
        let mut boxes: Vec<([f64; 3], [f64; 3])> = vec![];
        if pl.getattr("has_caves")?.is_truthy()? {
            let (ca, cb, ra, rb) = (pl.getattr("cave_a")?, pl.getattr("cave_b")?, pl.getattr("cave_ra")?, pl.getattr("cave_rb")?);
            for k in pl.call_method1("caves_near", (np3(py, local), rng))?.iter()? {
                let k = k?;
                let (a, b) = (v3(&ca.get_item(&k)?)?, v3(&cb.get_item(&k)?)?);
                let rr = f64of(&ra.get_item(&k)?)?.max(f64of(&rb.get_item(&k)?)?) + 1.0;
                boxes.push(([a[0].min(b[0]) - rr, a[1].min(b[1]) - rr, a[2].min(b[2]) - rr], [a[0].max(b[0]) + rr, a[1].max(b[1]) + rr, a[2].max(b[2]) + rr]));
            }
        }
        for c in pl.getattr("craters")?.iter()? {
            let c = c?;
            let cc = [f64of(&c.get_item(0)?)?, f64of(&c.get_item(1)?)?, f64of(&c.get_item(2)?)?];
            let cr = f64of(&c.get_item(3)?)?;
            if npnorm(py, [cc[0] - local[0], cc[1] - local[1], cc[2] - local[2]])? < rng + cr {
                boxes.push(([cc[0] - cr - 1.0, cc[1] - cr - 1.0, cc[2] - cr - 1.0], [cc[0] + cr + 1.0, cc[1] + cr + 1.0, cc[2] + cr + 1.0]));
            }
        }
        let fl = |x: f64| (x / ch).floor() as i64;
        let lo_v = [fl(local[0] - rng), fl(local[1] - rng), fl(local[2] - rng)];
        let hi_v = [fl(local[0] + rng), fl(local[1] + rng), fl(local[2] + rng)];
        let want = PySet::empty_bound(py)?;
        let mut keys: Vec<[i64; 3]> = vec![];
        let mut seen = std::collections::HashSet::new();
        for (lo, hi) in boxes {
            let i0 = [fl(lo[0]).max(lo_v[0]), fl(lo[1]).max(lo_v[1]), fl(lo[2]).max(lo_v[2])];
            let i1 = [fl(hi[0]).min(hi_v[0]), fl(hi[1]).min(hi_v[1]), fl(hi[2]).min(hi_v[2])];
            if (0..3).any(|k| i1[k] < i0[k]) {
                continue;
            }
            for x in i0[0]..=i1[0] {
                for y in i0[1]..=i1[1] {
                    for z in i0[2]..=i1[2] {
                        if seen.insert([x, y, z]) {
                            keys.push([x, y, z]);
                            want.add(ckey(py, [x, y, z]))?;
                        }
                    }
                }
            }
        }
        if keys.len() > 900 {
            // 多すぎるときは近い順に
            let sorted = py.import_bound("builtins")?.getattr("sorted")?;
            let keyf = Py::new(py, ChunkDist { local, ch })?;
            let l = sorted.call((&want,), Some(&[("key", keyf)].into_py_dict_bound(py)))?;
            let out = PySet::empty_bound(py)?;
            for k in l.iter()?.take(900) {
                out.add(k?)?;
            }
            return Ok(out);
        }
        Ok(want)
    }
    pub(crate) fn _mesh_chunk<'py>(slf: &Bound<'py, Self>, pl: &B<'py>, key: &B<'py>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let shape = imp(py, "main.shape")?;
        let npm = np(py)?;
        let ch = f64of(&s.getattr("CH")?)?;
        let res = f64of(&s.getattr("RES")?)?;
        let n = (ch / res).round() as usize + 1;
        let k3: [i64; 3] = { let v: Vec<i64> = key.extract()?; [v[0], v[1], v[2]] };
        let o = [k3[0] as f64 * ch, k3[1] as f64 * ch, k3[2] as f64 * ch];
        let g: Vec<f64> = (0..n).map(|i| i as f64 * res).collect();
        let c0 = v3(&pl.getattr("center")?)?;
        // numpy では (格子 + o) + center の順
        let mut pts2 = Vec::with_capacity(n * n * n * 3);
        for a in 0..n {
            for b in 0..n {
                for c in 0..n {
                    let p = [g[a] + o[0], g[b] + o[1], g[c] + o[2]];
                    pts2.extend_from_slice(&[p[0] + c0[0], p[1] + c0[1], p[2] + c0[2]]);
                }
            }
        }
        let parts = Planet::sdf_parts(pla(&pl)?, &IntoPy::<PyObject>::into_py(f64_2(py, pts2, 3)?, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?;
        let (dd, cc) = (a1(&parts.get_item(1)?)?, parts.get_item(2)?);
        let sv: Vec<f64> = if cc.is_none() {
            dd
        } else {
            let cv = a1(&cc)?;
            dd.iter().zip(cv.iter()).map(|(a, b)| a.max(-b)).collect()
        };
        let mn = sv.iter().cloned().fold(f64::INFINITY, f64::min);
        let mx = sv.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        if mn > 0.0 || mx < 0.0 {
            return Ok(none(py));
        }
        let vol = numpy::PyArrayMethods::reshape(&numpy::PyArray1::from_vec_bound(py, sv.iter().map(|x| *x as f32).collect::<Vec<f32>>()), [n, n, n])?;
        let mc = match shape.call_method("marching_cubes", (vol, 0.0), Some(&[("spacing", (res, res, res))].into_py_dict_bound(py))) {
            Ok(r) => r,
            Err(e) if is_exc(py, &e) => return Ok(none(py)),
            Err(e) => return Err(e),
        };
        let (verts_o, faces_o) = (mc.get_item(0)?, mc.get_item(1)?);
        if faces_o.len()? == 0 {
            return Ok(none(py));
        }
        let vr = a2(&verts_o, 3)?;
        let nv = vr.n;
        let verts: Vec<f64> = (0..nv * 3).map(|j| vr.d[j] + o[j % 3]).collect();
        let wp: Vec<f64> = (0..nv * 3).map(|j| verts[j] + c0[j % 3]).collect();
        let wp_o = f64_2(py, wp, 3)?;
        let nrm_o = Planet::sdf_grad(pla(&pl)?, &IntoPy::<PyObject>::into_py(&wp_o, pl.py()).into_bound(pl.py()), 0.05).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?.get_item(1)?;
        let nrm = a2(&nrm_o, 3)?;
        let hv = a1(&Planet::sdf_parts(pla(&pl)?, &IntoPy::<PyObject>::into_py(&wp_o, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?.get_item(0)?)?;
        let depth: Vec<f64> = hv.iter().map(|h| (-h).max(0.0)).collect();
        let vn: Vec<f64> = (0..nv).map(|i| rnorm(&verts[i * 3..i * 3 + 3])).collect();
        let dirs: Vec<f64> = (0..nv * 3).map(|j| verts[j] / vn[j / 3]).collect();
        let radii: Vec<f64> = (0..nv).map(|i| vn[i] + depth[i]).collect();
        let top = a2(&pl.call_method1("surface_color", (f64_2(py, dirs, 3)?, f64_1(py, radii), &nrm_o,))?, 3)?;
        let dirt = [0.42, 0.34, 0.26];
        let stone = [0.44, 0.42, 0.4];
        let mut col = vec![0.0f64; nv * 3];
        for i in 0..nv {
            let t = ((depth[i] - 1.5) / 4.0).clamp(0.0, 1.0);
            let k = (depth[i] / 0.6).clamp(0.0, 1.0);
            for c in 0..3 {
                let under = dirt[c] * (1.0 - t) + stone[c] * t;
                col[i * 3 + c] = top.d[i * 3 + c] * (1.0 - k) + under * k;
            }
        }
        let col = a2(&pl.call_method1("underground_tint", (f64_2(py, verts.clone(), 3)?, f64_2(py, col, 3)?,))?, 3)?;
        // 地中は日が当たらないので、向きによる陰影を弱めて少し暗くする（真っ暗にはしない）
        let s_ = sun(py)?;
        let amb = ambient(py)?;
        let mut v = vec![0f32; nv * 12];
        for i in 0..nv {
            let nn = nrm.r3(i);
            let deep = ((depth[i] - 1.0) / 6.0).clamp(0.0, 1.0);
            let sh = shade_rs(s_, amb, nn, col.r3(i));
            let lam = dot(nn, s_).clamp(0.0, 1.0);
            let rr = &mut v[i * 12..i * 12 + 12];
            for c in 0..3 {
                rr[3 + c] = nn[c] as f32;
                rr[6 + c] = (sh[c] * (1.0 - deep) + col.d[i * 3 + c] * (0.62 + 0.18 * lam) * deep) as f32;
            }
            rr[9] = 1.0;
            let ab = [nn[0].abs(), nn[1].abs(), nn[2].abs()];
            let mut ax = 0;
            for j in 1..3 {
                if ab[j] > ab[ax] {
                    ax = j;
                }
            }
            let u = if ax == 0 { verts[i * 3 + 1] } else { verts[i * 3] };
            let w = if ax == 2 { verts[i * 3 + 1] } else { verts[i * 3 + 2] };
            rr[10] = (u / 4.0) as f32;
            rr[11] = (w / 4.0) as f32;
        }
        let faces = npm.call_method1("ascontiguousarray", (faces_o, npm.getattr("int64")?))?;
        Ok(PyTuple::new_bound(py, [f64_2(py, verts, 3)?, f32_2(py, v, 12)?, faces]).into_any())
    }
    pub(crate) fn update(slf: &Bound<'_, Self>, pl: &B<'_>, player_pos: &B<'_>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let root = Self::ga_root(slf)?;
        let pp = v3(player_pos)?;
        let far = pl.is_none() || {
            let c = v3(&pl.getattr("center")?)?;
            npnorm(py, [pp[0] - c[0], pp[1] - c[1], pp[2] - c[2]])? - f64of(&pl.getattr("radius")?)? > 300.0
        };
        if !Self::ga_enabled(slf)?.is_truthy()? || far {
            if Self::ga_active(slf)?.is_truthy()? {
                Self::sa_active(slf, PySet::empty_bound(py)?)?;
                let v: i64 = Self::ga_version(slf)?.extract()?;
                Self::sa_version(slf, v + 1)?;
            }
            root.call_method0("hide")?;
            return Ok(());
        }
        root.call_method0("show")?;
        if !pl.is(&Self::ga_planet(slf)?) {
            Self::sa_planet(slf, pl)?;
            Self::sa_anchor(slf, py.None())?;
            Self::sa_cache(slf, PyDict::new_bound(py))?;
            Self::sa_n_craters(slf, 0)?;
            Self::sa_active(slf, PySet::empty_bound(py)?)?;
        }
        let ch = f64of(&s.getattr("CH")?)?;
        // 新しい掘削跡が触れるチャンクは作り直す
        let craters: Vec<B<'_>> = pl.getattr("craters")?.iter()?.collect::<R<_>>()?;
        let nc: usize = Self::ga_n_craters(slf)?.extract()?;
        if craters.len() != nc {
            let cache = Self::ga_cache(slf)?;
            for c in &craters[nc.min(craters.len())..] {
                let cc = [f64of(&c.get_item(0)?)?, f64of(&c.get_item(1)?)?, f64of(&c.get_item(2)?)?];
                let cr = f64of(&c.get_item(3)?)?;
                let keys: Vec<B<'_>> = cache.iter()?.collect::<R<_>>()?;
                for key in keys {
                    let k: Vec<i64> = key.extract()?;
                    let lo: Vec<f64> = (0..3).map(|j| k[j] as f64 * ch - cr - 1.0).collect();
                    let hi: Vec<f64> = (0..3).map(|j| lo[j] + ch + 2.0 * cr + 2.0).collect();
                    if (0..3).all(|j| cc[j] >= lo[j] && cc[j] <= hi[j]) {
                        cache.dpop1(&key)?;
                    }
                }
            }
            Self::sa_n_craters(slf, craters.len())?;
            Self::sa_anchor(slf, py.None())?;
        }
        let anchor = Self::ga_anchor(slf)?;
        // anchor は惑星ローカル（惑星が公転で動いても選び直さない。L-08）
        let pc = v3(&pl.getattr("center")?)?;
        let moved = anchor.is_none() || {
            let a = v3(&anchor)?;
            npnorm(py, [pp[0] - pc[0] - a[0], pp[1] - pc[1] - a[1], pp[2] - pc[2] - a[2]])? > 6.0
        };
        if moved {
            Self::sa_anchor(slf, np(py)?.call_method1("array", (vec![pp[0] - pc[0], pp[1] - pc[1], pp[2] - pc[2]],))?)?;
            let want = Self::_wanted(slf, pl, player_pos)?;
            let cache = Self::ga_cache(slf)?;
            let c0 = v3(&pl.getattr("center")?)?;
            let local = [pp[0] - c0[0], pp[1] - c0[1], pp[2] - c0[2]];
            let l = PyList::empty_bound(py);
            for k in want.iter() {
                if !cache.contains(&k)? {
                    l.append(k)?;
                }
            }
            let keyf = Py::new(py, ChunkDist { local, ch })?;
            let pend = py.import_bound("builtins")?.getattr("sorted")?.call((l,), Some(&[("key", keyf)].into_py_dict_bound(py)))?;
            Self::sa_pending(slf, pend)?;
            Self::sa_want(slf, want)?;
            Self::sa_dirty(slf, true)?;
        }
        // 1フレームに作るチャンクは時間で制限（カクつき防止）
        let time = imp(py, "time")?;
        let pc = || -> R<f64> { time.call_method0("perf_counter")?.extract() };
        let t0 = pc()?;
        let pending = Self::ga_pending(slf)?;
        let cache = Self::ga_cache(slf)?;
        while pending.is_truthy()? && pc()? - t0 < 0.012 {
            let k = pending.dpop1(0)?;
            if !cache.contains(&k)? {
                cache.set_item(&k, Self::_mesh_chunk(slf, pl, &k)?)?;
            }
            Self::sa_dirty(slf, true)?;
        }
        if Self::ga_dirty(slf)?.is_truthy()? && (!pending.is_truthy()? || pc()? - f64of(&gd(s, "_last_asm", tb(py, 0.0)))? > 0.4) {
            Self::sa_last_asm(slf, pc()?)?;
            Self::_assemble(slf).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?;
        }
        let c = v3(&pl.getattr("center")?)?;
        let o = v3(origin)?;
        root.call_method1("setPos", (c[0] - o[0], c[1] - o[1], c[2] - o[2]))?;
        Ok(())
    }
    pub(crate) fn _assemble(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        Self::sa_dirty(slf, false)?;
        let mut vs: Vec<f32> = vec![];
        let mut ts: Vec<i64> = vec![];
        let mut n: i64 = 0;
        let active = PySet::empty_bound(py)?;
        let cache = Self::ga_cache(slf)?;
        let want = Self::ga_want(slf)?;
        let npm = np(py)?;
        for k in want.iter()? {
            let k = k?;
            let m = cache.dget1(&k)?;
            if m.is_none() {
                continue;
            }
            active.add(&k)?;
            let verts = a2(&m.get_item(0)?, 3)?;
            let v: Vec<f32> = {
                let r: numpy::PyReadonlyArray2<f32> = npm.call_method1("ascontiguousarray", (m.get_item(1)?, npm.getattr("float32")?))?.extract()?;
                r.as_slice()?.to_vec()
            };
            let (f, _) = i2(&m.get_item(2)?, 3)?;
            let base = vs.len();
            vs.extend_from_slice(&v);
            for i in 0..verts.n {
                for c in 0..3 {
                    vs[base + i * 12 + c] = verts.d[i * 3 + c] as f32;
                }
            }
            ts.extend(f.iter().map(|x| x + n));
            n += verts.n as i64;
        }
        let mesh = Self::ga_mesh(slf)?;
        if !vs.is_empty() {
            mesh.call_method1("set", (f32_2(py, vs, 12)?, i64_2(py, ts, 3)?))?;
            let rm = rmod(py)?;
            let tex = rm.call_method1("load_texture", (Self::ga_loader(slf)?, get(&Self::ga_planet(slf)?.getattr("defn")?, "texture", py.None())?))?;
            if tex.is_truthy()? {
                mesh.getattr("np")?.call_method1("setTexture", (tex, 1))?;
            }
            mesh.getattr("np")?.call_method0("show")?;
        } else {
            mesh.getattr("np")?.call_method0("hide")?;
        }
        // 画面外のキャッシュを捨てる
        if cache.len()? > 2500 {
            let keys: Vec<B<'_>> = cache.iter()?.collect::<R<_>>()?;
            for k in keys {
                if !want.contains(&k)? {
                    cache.dpop1(&k)?;
                }
            }
        }
        if !active.eq(Self::ga_active(slf)?)? {
            Self::sa_active(slf, active)?;
            let v: i64 = Self::ga_version(slf)?.extract()?;
            Self::sa_version(slf, v + 1)?;
        }
        let _ = fmt_err;
        Ok(())
    }
}

/// チャンクの近さ（並べ替えのキー）
#[pyclass(module = "alpedia_core")]
pub struct ChunkDist {
    local: [f64; 3],
    ch: f64,
}
#[pymethods]
impl ChunkDist {
    pub(crate) fn __call__(&self, py: Python<'_>, k: &B<'_>) -> R<f64> {
        let v: Vec<i64> = k.extract()?;
        let p: [f64; 3] = std::array::from_fn(|j| (v[j] as f64 + 0.5) * self.ch - self.local[j]);
        npnorm(py, p)
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    for f in [wrap_pyfunction_bound!(render_load_texture, m)?, wrap_pyfunction_bound!(render_dynamic_mesh, m)?, wrap_pyfunction_bound!(render_quad_tris, m)?,
              wrap_pyfunction_bound!(render_shade, m)?, wrap_pyfunction_bound!(render_project_uv, m)?, wrap_pyfunction_bound!(render_block_ao, m)?,
              wrap_pyfunction_bound!(render_frames8, m)?, wrap_pyfunction_bound!(render_face_frame_p, m)?, wrap_pyfunction_bound!(render_face_frame, m)?,
              wrap_pyfunction_bound!(render_display_faces, m)?, wrap_pyfunction_bound!(render_lod_vertices, m)?, wrap_pyfunction_bound!(render_light_ids, m)?,
              wrap_pyfunction_bound!(render_preload, m)?, wrap_pyfunction_bound!(render_edge_stage, m)?] {
        m.add_function(f)?;
    }
    m.add_class::<LightLayer>()?;
    m.add_class::<LodLayer>()?;
    m.add_class::<ConstructView>()?;
    m.add_class::<DecalView>()?;
    m.add_class::<PlanetView>()?;
    m.add_class::<TerrainPatch>()?;
    m.add_class::<CarveView>()?;
    m.add_class::<ChunkDist>()?;
    Ok(())
}

// ---------------------------------------------------------------- CarveView の欄の読み書き（C-01。前は __dict__）
// 入れていない欄を読むと、前と同じく AttributeError（getattr(x, 名前, 既定値) も前と同じに動く）
macro_rules! carveview_attr_fields {
    ($($g:ident $s:ident $pg:ident $ps:ident $f:ident $name:literal),* $(,)?) => {
        #[allow(dead_code, non_snake_case)]
        impl CarveView { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
                match &slf.try_borrow()?.$f {
                    Some(v) => Ok(v.bind(slf.py()).clone()),
                    None => Err(pyo3::exceptions::PyAttributeError::new_err($name)),
                }
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> PyResult<()> {
                let v = v.into_py(slf.py());
                slf.try_borrow_mut()?.$f = Some(v);
                Ok(())
            }
        )* }
        #[pymethods]
        #[allow(non_snake_case)]
        impl CarveView { $(
            #[getter($name)]
            fn $pg(slf: &Bound<'_, Self>) -> PyResult<PyObject> {
                Ok(Self::$g(slf)?.unbind())
            }
            #[setter($name)]
            fn $ps(&mut self, v: PyObject) {
                self.$f = Some(v);
            }
        )* }
    };
}
carveview_attr_fields!(
    ga_last_asm sa_last_asm pga_last_asm psa_last_asm a_last_asm "_last_asm", ga_active sa_active pga_active psa_active a_active "active", ga_anchor sa_anchor pga_anchor psa_anchor a_anchor "anchor", ga_cache sa_cache pga_cache psa_cache a_cache "cache", ga_dirty sa_dirty pga_dirty psa_dirty a_dirty "dirty", ga_enabled sa_enabled pga_enabled psa_enabled a_enabled "enabled", ga_loader sa_loader pga_loader psa_loader a_loader "loader", ga_mesh sa_mesh pga_mesh psa_mesh a_mesh "mesh", ga_n_craters sa_n_craters pga_n_craters psa_n_craters a_n_craters "n_craters", ga_pending sa_pending pga_pending psa_pending a_pending "pending", ga_planet sa_planet pga_planet psa_planet a_planet "planet", ga_pver sa_pver pga_pver psa_pver a_pver "pver", ga_range sa_range pga_range psa_range a_range "range", ga_root sa_root pga_root psa_root a_root "root", ga_version sa_version pga_version psa_version a_version "version", ga_want sa_want pga_want psa_want a_want "want"
);
