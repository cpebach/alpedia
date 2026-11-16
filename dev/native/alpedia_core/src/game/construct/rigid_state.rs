//! 構築物：剛体・レールの間の状態（前は dict だった rb）。物理コアが R・x をその場で書き換えるので、配列は numpy の配列（型つき）
//! のまま持つ。Python・ほかのモジュールからは rb["R"] のように今までと同じ形でも読める。

use super::*;

#[pyclass(module = "alpedia_core")]
pub struct RigidState {
    /// 向き（3×3）
    #[pyo3(get, set, name = "R")]
    pub r: FA,
    /// 重心（3）
    #[pyo3(get, set)]
    pub x: FA,
    /// 速度（3）
    #[pyo3(get, set)]
    pub v: FA,
    /// 角速度（3）
    #[pyo3(get, set)]
    pub w: FA,
    /// 慣性テンソル（体の向き。3×3）
    #[pyo3(get, set, name = "Ib")]
    pub ib: FA,
    /// 頂点の、重心からの位置（体の向き。8n×3）
    #[pyo3(get, set)]
    pub local: FA,
    /// 重心からいちばん遠い頂点までの距離
    #[pyo3(get, set)]
    pub ext: f64,
}

#[pymethods]
impl RigidState {
    fn __getitem__(&self, py: Python<'_>, k: &str) -> R<PyObject> {
        Ok(match k {
            "R" => self.r.clone_ref(py).into_any(),
            "x" => self.x.clone_ref(py).into_any(),
            "v" => self.v.clone_ref(py).into_any(),
            "w" => self.w.clone_ref(py).into_any(),
            "Ib" => self.ib.clone_ref(py).into_any(),
            "local" => self.local.clone_ref(py).into_any(),
            "ext" => self.ext.into_py(py),
            _ => return Err(pyo3::exceptions::PyKeyError::new_err(k.to_string())),
        })
    }
    fn __setitem__(&mut self, k: &str, v: &Bound<'_, PyAny>) -> R<()> {
        match k {
            "R" => self.r = to_fa(v)?,
            "x" => self.x = to_fa(v)?,
            "v" => self.v = to_fa(v)?,
            "w" => self.w = to_fa(v)?,
            "Ib" => self.ib = to_fa(v)?,
            "local" => self.local = to_fa(v)?,
            "ext" => self.ext = v.extract()?,
            _ => return Err(pyo3::exceptions::PyKeyError::new_err(k.to_string())),
        }
        Ok(())
    }
    fn __contains__(&self, k: &str) -> bool {
        matches!(k, "R" | "x" | "v" | "w" | "Ib" | "local" | "ext")
    }
}

impl Construct {
    /// 剛体の状態（無ければ None）
    pub fn rb(slf: &Bound<'_, Self>) -> R<Option<Py<RigidState>>> {
        Ok(slf.try_borrow()?.rb.as_ref().map(|r| r.clone_ref(slf.py())))
    }
    /// 剛体の状態（無ければエラー）
    pub fn rb_req(slf: &Bound<'_, Self>) -> R<Py<RigidState>> {
        Self::rb(slf)?.ok_or_else(|| PyValueError::new_err("剛体の状態（rb）がありません"))
    }
}

impl RigidState {
    /// 物理コアに渡す前に、配列を C の並びに（並んでいない物だけ写す）
    pub fn contig(&mut self, py: Python<'_>) -> R<()> {
        for a in [&mut self.r, &mut self.x, &mut self.ib, &mut self.local] {
            if let Some(v) = fix_contig(a.bind(py).as_any())? {
                *a = to_fa(&v)?;
            }
        }
        Ok(())
    }
    pub fn new(py: Python<'_>, local: &[V3], r: &M3, x: V3, v: V3, w: V3, ib: &M3) -> R<Self> {
        Ok(RigidState {
            r: m3a(py, r)?,
            x: v3a(py, x)?,
            v: v3a(py, v)?,
            w: v3a(py, w)?,
            ib: m3a(py, ib)?,
            local: fa(py, local.iter().flatten().copied().collect(), &[local.len(), 3])?,
            ext: local.iter().map(|l| norm(*l)).fold(f64::NEG_INFINITY, f64::max),
        })
    }
}

pub type M3 = [[f64; 3]; 3];

/// 3 つの数の配列
pub fn v3a(py: Python<'_>, v: V3) -> R<FA> {
    fa(py, v.to_vec(), &[3])
}
/// 3×3 の配列
pub fn m3a(py: Python<'_>, m: &M3) -> R<FA> {
    fa(py, m.iter().flatten().copied().collect(), &[3, 3])
}
/// 配列（型つき）の中身を読む
pub fn fa_vec(py: Python<'_>, a: &FA) -> R<Vec<f64>> {
    let r = a.bind(py).readonly();
    Ok(match r.as_slice() {
        Ok(s) => s.to_vec(),
        Err(_) => r.as_array().iter().copied().collect(),
    })
}
pub fn fa_v3(py: Python<'_>, a: &FA) -> R<V3> {
    let v = fa_vec(py, a)?;
    if v.len() < 3 {
        return Err(PyValueError::new_err("3つの数が必要です"));
    }
    Ok([v[0], v[1], v[2]])
}
pub fn fa_m3(py: Python<'_>, a: &FA) -> R<M3> {
    let v = fa_vec(py, a)?;
    if v.len() < 9 {
        return Err(PyValueError::new_err("3×3 の行列が必要です"));
    }
    Ok([[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]])
}
pub fn fa_v3s(py: Python<'_>, a: &FA) -> R<Vec<V3>> {
    Ok(fa_vec(py, a)?.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect())
}
