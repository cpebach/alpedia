//! 描画：光の層（LightLayer）・遠くの物の層（LodLayer）

use crate::game::construct::{con, Construct};
use super::*;

// ---------------------------------------------------------------- LightLayer
/// ライト（電力で光るブロック）：点いている物の上に、影をつけない明るい色の箱を重ねる（少し大きく）。
/// 点滅（blink）もここで。読み込み済み（近く）の構築物だけ
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct LightLayer {
    /// mesh（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_mesh: Option<PyObject>,
    /// n（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_n: Option<PyObject>,
}

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
        LightLayer { a_mesh: None, a_n: None }
    }
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>) -> R<()> {
        let py = slf.py();
        let m = dmesh(py, parent, "lights")?;
        let n = m.getattr("np")?;
        n.call_method1("setTransparency", (true,))?;
        n.call_method1("setDepthOffset", (1,))?;
        Self::sa_mesh(slf, m)?;
        Self::sa_n(slf, -1)?;
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
            let devs = Construct::g_devs(con(&c)?)?;
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
                let dd = or_dict(crate::game::registry::dev_dict(&reg, &d.get_item("def")?)?)?;
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
        let mesh = Self::ga_mesh(slf)?;
        let n_old: i64 = Self::ga_n(slf)?.extract()?;
        if ps.is_empty() {
            if n_old != 0 {
                mesh.call_method1("set", (zeros12(py)?,))?;
                Self::sa_n(slf, 0)?;
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
        Self::sa_n(slf, ps.len())?;
        Ok(())
    }
    pub(crate) fn remove(slf: &Bound<'_, Self>) -> R<()> {
        Self::ga_mesh(slf)?.call_method0("remove")?;
        Ok(())
    }
}

// ---------------------------------------------------------------- LodLayer
/// 読み込み距離より遠い構築物を、それっぽい色の箱でまとめて描く（ブロックのメッシュを作らない・記号も看板も出さない）。
/// 止まっている物は1つのメッシュにまとめ（入れ替わったときだけ作り直す）、動いている物は毎フレーム作る
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct LodLayer {
    /// base（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_base: Option<PyObject>,
    /// key（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_key: Option<PyObject>,
    /// moving（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_moving: Option<PyObject>,
    /// n_moving（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_n_moving: Option<PyObject>,
}

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
        LodLayer { a_base: None, a_key: None, a_moving: None, a_n_moving: None }
    }
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>) -> R<()> {
        let py = slf.py();
        slf.as_any().setattr("static", dmesh(py, parent, "lod_static")?)?;
        Self::sa_moving(slf, dmesh(py, parent, "lod_moving")?)?;
        Self::sa_key(slf, py.None())?;
        Self::sa_base(slf, np(py)?.call_method1("zeros", (3,))?)?;
        Self::sa_n_moving(slf, 0)?;
        Ok(())
    }
    pub(crate) fn update(slf: &Bound<'_, Self>, constructs: &B<'_>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let mut statics = vec![];
        let mut moving = vec![];
        for c in constructs.iter()? {
            let c = c?;
            if c.getattr("is_static")?.is_truthy()? || Construct::g_asleep(con(&c)?)?.is_truthy()? {
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
        if !key.eq(Self::ga_key(slf)?)? {
            Self::sa_key(slf, &key)?;
            let o = v3(origin)?;
            let base = np3(py, o);
            Self::sa_base(slf, &base)?;
            let mut vs = vec![];
            for c in &statics {
                vs.push(rm.call_method1("lod_vertices", (c, &base))?);
            }
            let v = concat12(py, vs)?;
            let n = v.len()?;
            let t: PyObject = if n > 0 { qtris(py, n / 4)?.unbind() } else { py.None() };
            slf.as_any().getattr("static")?.call_method1("set", (v, t))?;
        }
        let b = v3(&Self::ga_base(slf)?)?;
        let o = v3(origin)?;
        slf.as_any().getattr("static")?.getattr("np")?.call_method1("setPos", (b[0] - o[0], b[1] - o[1], b[2] - o[2]))?;
        let nm: i64 = Self::ga_n_moving(slf)?.extract()?;
        if !moving.is_empty() || nm != 0 {
            let mut vs = vec![];
            for c in &moving {
                vs.push(rm.call_method1("lod_vertices", (c, origin))?);
            }
            let v = concat12(py, vs)?;
            let n = v.len()?;
            let t: PyObject = if n > 0 { qtris(py, n / 4)?.unbind() } else { py.None() };
            Self::ga_moving(slf)?.call_method1("set", (v, t))?;
            Self::sa_n_moving(slf, moving.len())?;
        }
        Ok(())
    }
    pub(crate) fn remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().getattr("static")?.call_method0("remove")?;
        Self::ga_moving(slf)?.call_method0("remove")?;
        Ok(())
    }
}

// ---------------------------------------------------------------- LodLayer の欄の読み書き（C-01。前は __dict__）
// 入れていない欄を読むと、前と同じく AttributeError（getattr(x, 名前, 既定値) も前と同じに動く）
macro_rules! lodlayer_attr_fields {
    ($($g:ident $s:ident $pg:ident $ps:ident $f:ident $name:literal),* $(,)?) => {
        #[allow(dead_code, non_snake_case)]
        impl LodLayer { $(
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
        impl LodLayer { $(
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
lodlayer_attr_fields!(
    ga_base sa_base pga_base psa_base a_base "base", ga_key sa_key pga_key psa_key a_key "key", ga_moving sa_moving pga_moving psa_moving a_moving "moving", ga_n_moving sa_n_moving pga_n_moving psa_n_moving a_n_moving "n_moving"
);

// ---------------------------------------------------------------- LightLayer の欄の読み書き（C-01。前は __dict__）
// 入れていない欄を読むと、前と同じく AttributeError（getattr(x, 名前, 既定値) も前と同じに動く）
macro_rules! lightlayer_attr_fields {
    ($($g:ident $s:ident $pg:ident $ps:ident $f:ident $name:literal),* $(,)?) => {
        #[allow(dead_code, non_snake_case)]
        impl LightLayer { $(
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
        impl LightLayer { $(
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
lightlayer_attr_fields!(
    ga_mesh sa_mesh pga_mesh psa_mesh a_mesh "mesh", ga_n sa_n pga_n psa_n a_n "n"
);
