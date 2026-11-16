//! 描画：光の層（LightLayer）・遠くの物の層（LodLayer）

use super::*;

// ---------------------------------------------------------------- LightLayer
/// ライト（電力で光るブロック）：点いている物の上に、影をつけない明るい色の箱を重ねる（少し大きく）。
/// 点滅（blink）もここで。読み込み済み（近く）の構築物だけ
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct LightLayer {}

#[pymethods]
impl LightLayer {
    #[classattr]
    #[allow(non_snake_case)]
    pub(crate) fn GROW() -> f64 {
        1.06
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        LightLayer {}
    }
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let m = dmesh(py, parent, "lights")?;
        let n = m.getattr("np")?;
        n.call_method1("setTransparency", (true,))?;
        n.call_method1("setDepthOffset", (1,))?;
        s.setattr("mesh", m)?;
        s.setattr("n", -1)?;
        Ok(())
    }
    pub(crate) fn update(slf: &Bound<'_, Self>, constructs: &B<'_>, origin: &B<'_>, now: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let grow = f64of(&s.getattr("GROW")?)?;
        let o = v3(origin)?;
        let mut ps: Vec<[f64; 24]> = vec![];
        let mut cols: Vec<[f64; 3]> = vec![];
        let rm = rmod(py)?;
        for c in constructs.iter()? {
            let c = c?;
            let ids = rm.call_method1("light_ids", (&c,))?;
            if !ids.is_truthy()? {
                continue;
            }
            let devs = c.getattr("devs")?;
            let reg = c.getattr("reg")?;
            let pos = c.getattr("pos")?;
            for i in ids.iter()? {
                let i: i64 = i?.extract()?;
                let d = devs.get_item(i)?;
                let st = d.get_item("state")?;
                if !get(&st, "active", py.None())?.is_truthy()? {
                    continue;
                }
                let bl = get(&st, "blink", py.None())?;
                if bl.is_truthy()? {
                    let b = f64of(&bl)?;
                    let m = f64of(&tb(py, now).call_method1("__mod__", (b,))?)?;
                    if m > b * 0.5 {
                        continue;
                    }
                }
                let dd = or_dict(reg.call_method1("device", (d.get_item("def")?,))?)?;
                let mut col = get(&st, "color", py.None())?;
                if !col.is_truthy()? {
                    col = get(&get(&dd, "params", PyDict::new_bound(py))?, "color", py.None())?;
                }
                let col = if col.is_truthy()? { v3(&col)? } else { [1.0, 0.86, 0.55] };
                let v8 = a2(&pos.get_item(pyo3::types::PySlice::new_bound(py, (8 * i) as isize, (8 * i + 8) as isize, 1))?, 3)?;
                let mut ctr = [0.0; 3];
                for k in 0..3 {
                    let mut sm = 0.0;
                    for j in 0..8 {
                        sm += v8.d[j * 3 + k];
                    }
                    ctr[k] = sm / 8.0;
                }
                let mut p = [0.0; 24];
                for j in 0..8 {
                    for k in 0..3 {
                        p[j * 3 + k] = (v8.d[j * 3 + k] - ctr[k]) * grow + ctr[k] - o[k];
                    }
                }
                ps.push(p);
                cols.push(col);
            }
        }
        let mesh = s.getattr("mesh")?;
        let n_old: i64 = s.getattr("n")?.extract()?;
        if ps.is_empty() {
            if n_old != 0 {
                mesh.call_method1("set", (zeros12(py)?,))?;
                s.setattr("n", 0)?;
            }
            return Ok(());
        }
        let mut out = vec![0f32; ps.len() * 24 * 12];
        for (b, p) in ps.iter().enumerate() {
            for (f, face) in BOX_FACES.iter().enumerate() {
                for (q, &vi) in face.iter().enumerate() {
                    let r = &mut out[((b * 6 + f) * 4 + q) * 12..((b * 6 + f) * 4 + q) * 12 + 12];
                    for k in 0..3 {
                        r[k] = p[vi * 3 + k] as f32;
                        r[6 + k] = cols[b][k] as f32;
                    }
                    r[9] = 0.92;
                }
            }
        }
        let t: PyObject = if ps.len() as i64 != n_old { qtris(py, ps.len() * 6)?.unbind() } else { py.None() };
        mesh.call_method1("set", (f32_2(py, out, 12)?, t))?;
        s.setattr("n", ps.len())?;
        Ok(())
    }
    pub(crate) fn remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().getattr("mesh")?.call_method0("remove")?;
        Ok(())
    }
}

// ---------------------------------------------------------------- LodLayer
/// 読み込み距離より遠い構築物を、それっぽい色の箱でまとめて描く（ブロックのメッシュを作らない・記号も看板も出さない）。
/// 止まっている物は1つのメッシュにまとめ（入れ替わったときだけ作り直す）、動いている物は毎フレーム作る
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct LodLayer {}

pub(crate) fn concat12<'py>(py: Python<'py>, vs: Vec<B<'py>>) -> R<B<'py>> {
    if vs.is_empty() {
        return zeros12(py);
    }
    np(py)?.call_method1("concatenate", (PyList::new_bound(py, vs),))
}

#[pymethods]
impl LodLayer {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        LodLayer {}
    }
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("static", dmesh(py, parent, "lod_static")?)?;
        s.setattr("moving", dmesh(py, parent, "lod_moving")?)?;
        s.setattr("key", py.None())?;
        s.setattr("base", np(py)?.call_method1("zeros", (3,))?)?;
        s.setattr("n_moving", 0)?;
        Ok(())
    }
    pub(crate) fn update(slf: &Bound<'_, Self>, constructs: &B<'_>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let mut statics = vec![];
        let mut moving = vec![];
        for c in constructs.iter()? {
            let c = c?;
            if c.getattr("is_static")?.is_truthy()? || c.getattr("asleep")?.is_truthy()? {
                statics.push(c);
            } else {
                moving.push(c);
            }
        }
        let pairs = PyList::empty_bound(py);
        for c in &statics {
            pairs.append((c.getattr("id")?, c.call_method0("_pose_key")?))?;
        }
        let key = py.import_bound("builtins")?.getattr("tuple")?.call1((py.import_bound("builtins")?.getattr("sorted")?.call1((pairs,))?,))?;
        let rm = rmod(py)?;
        if !key.eq(s.getattr("key")?)? {
            s.setattr("key", &key)?;
            let o = v3(origin)?;
            let base = np3(py, o);
            s.setattr("base", &base)?;
            let mut vs = vec![];
            for c in &statics {
                vs.push(rm.call_method1("lod_vertices", (c, &base))?);
            }
            let v = concat12(py, vs)?;
            let n = v.len()?;
            let t: PyObject = if n > 0 { qtris(py, n / 4)?.unbind() } else { py.None() };
            s.getattr("static")?.call_method1("set", (v, t))?;
        }
        let b = v3(&s.getattr("base")?)?;
        let o = v3(origin)?;
        s.getattr("static")?.getattr("np")?.call_method1("setPos", (b[0] - o[0], b[1] - o[1], b[2] - o[2]))?;
        let nm: i64 = s.getattr("n_moving")?.extract()?;
        if !moving.is_empty() || nm != 0 {
            let mut vs = vec![];
            for c in &moving {
                vs.push(rm.call_method1("lod_vertices", (c, origin))?);
            }
            let v = concat12(py, vs)?;
            let n = v.len()?;
            let t: PyObject = if n > 0 { qtris(py, n / 4)?.unbind() } else { py.None() };
            s.getattr("moving")?.call_method1("set", (v, t))?;
            s.setattr("n_moving", moving.len())?;
        }
        Ok(())
    }
    pub(crate) fn remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().getattr("static")?.call_method0("remove")?;
        slf.as_any().getattr("moving")?.call_method0("remove")?;
        Ok(())
    }
}
