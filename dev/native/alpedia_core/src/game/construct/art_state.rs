//! 構築物：関節つき剛体の状態（前は dict だった art。mode "art" の間だけ）。物理コアがその場で書き換える配列は
//! numpy の配列（型つき）のまま持つ。Python・テストからは art["X"] のように今までと同じ形でも読める。

use super::*;

#[pyclass(module = "alpedia_core")]
pub struct ArtState {
    /// 作ったときの構築物の版・ボクセルの数（変わったら作り直す）
    #[pyo3(get, set)]
    pub version: i64,
    #[pyo3(get, set)]
    pub n: i64,
    /// ボクセル → 部分、頂点 → 部分、部分の数
    #[pyo3(get, set)]
    pub gid: IA,
    #[pyo3(get, set)]
    pub vg: IA,
    #[pyo3(get, set, name = "G")]
    pub g: i64,
    /// 頂点の、部分の重心からの位置（最初・今）
    #[pyo3(get, set)]
    pub vl0: FA,
    #[pyo3(get, set)]
    pub vl: FA,
    /// 部分の質量の逆数・慣性の逆数（体の向き）・慣性
    #[pyo3(get, set, name = "invM")]
    pub inv_m: FA,
    #[pyo3(get, set, name = "IinvB")]
    pub iinv_b: FA,
    #[pyo3(get, set, name = "Ib")]
    pub ib: FA,
    /// 部分の重心・向き
    #[pyo3(get, set, name = "X")]
    pub x: FA,
    #[pyo3(get, set, name = "R")]
    pub r: FA,
    /// 押し出しのアクチュエーター：[(ボクセル, 面の頂点, 向き, 伸び)]
    #[pyo3(get, set)]
    pub exts: Py<PyList>,
    /// 回転ジョイント：部分 a・b、面の頂点 A・B、アクチュエーターの頂点 C、角度、ヒンジ
    #[pyo3(get, set)]
    pub ra: IA,
    #[pyo3(get, set)]
    pub rb: IA,
    #[pyo3(get, set, name = "A")]
    pub a: IA,
    #[pyo3(get, set, name = "B")]
    pub b: IA,
    #[pyo3(get, set, name = "C")]
    pub c: IA,
    #[pyo3(get, set)]
    pub devs: Py<PyList>,
    #[pyo3(get, set)]
    pub applied: FA,
    #[pyo3(get, set)]
    pub ext_applied: FA,
    #[pyo3(get, set)]
    pub gv: IA,
    #[pyo3(get, set, name = "HK")]
    pub hk: IA,
    /// アンカーで固定の部分。固定があるときだけ、固定の部分を動かさない値（invM_s・IinvB_s）と固定の位置（Xfix）
    #[pyo3(get, set)]
    pub fixed: BA,
    #[pyo3(get, set, name = "invM_s")]
    pub inv_m_s: Option<FA>,
    #[pyo3(get, set, name = "IinvB_s")]
    pub iinv_b_s: Option<FA>,
    #[pyo3(get, set, name = "Xfix")]
    pub xfix: Option<FA>,
    /// 物理コアの ArtTopo（作れないときは None）
    #[pyo3(get, set)]
    pub nat: PyObject,
    /// 1 ティックの途中の値（形の変わり・接触）
    #[pyo3(get, set)]
    pub defo: PyObject,
    #[pyo3(get, set)]
    pub contacts: PyObject,
}

#[pymethods]
impl ArtState {
    fn __getitem__(&self, py: Python<'_>, k: &str) -> R<PyObject> {
        self.item(py, k)?.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(k.to_string()))
    }
    #[pyo3(signature = (k, default=None))]
    fn get(&self, py: Python<'_>, k: &str, default: Option<PyObject>) -> R<PyObject> {
        Ok(match self.item(py, k)? {
            Some(v) if !v.is_none(py) => v,
            _ => default.unwrap_or_else(|| py.None()),
        })
    }
    fn __contains__(&self, py: Python<'_>, k: &str) -> R<bool> {
        Ok(self.item(py, k)?.is_some())
    }
}

impl ArtState {
    fn item(&self, py: Python<'_>, k: &str) -> R<Option<PyObject>> {
        let f = |a: &FA| Some(a.clone_ref(py).into_any());
        let i = |a: &IA| Some(a.clone_ref(py).into_any());
        Ok(match k {
            "version" => Some(self.version.into_py(py)),
            "n" => Some(self.n.into_py(py)),
            "gid" => i(&self.gid),
            "vg" => i(&self.vg),
            "G" => Some(self.g.into_py(py)),
            "vl0" => f(&self.vl0),
            "vl" => f(&self.vl),
            "invM" => f(&self.inv_m),
            "IinvB" => f(&self.iinv_b),
            "Ib" => f(&self.ib),
            "X" => f(&self.x),
            "R" => f(&self.r),
            "exts" => Some(self.exts.clone_ref(py).into_any()),
            "ra" => i(&self.ra),
            "rb" => i(&self.rb),
            "A" => i(&self.a),
            "B" => i(&self.b),
            "C" => i(&self.c),
            "devs" => Some(self.devs.clone_ref(py).into_any()),
            "applied" => f(&self.applied),
            "ext_applied" => f(&self.ext_applied),
            "gv" => i(&self.gv),
            "HK" => i(&self.hk),
            "fixed" => Some(self.fixed.clone_ref(py).into_any()),
            "invM_s" => self.inv_m_s.as_ref().and_then(f),
            "IinvB_s" => self.iinv_b_s.as_ref().and_then(f),
            "Xfix" => self.xfix.as_ref().and_then(f),
            "nat" => Some(self.nat.clone_ref(py)),
            "defo" => Some(self.defo.clone_ref(py)),
            "contacts" => Some(self.contacts.clone_ref(py)),
            _ => None,
        })
    }
    /// 物理コアに渡す前に、書き換える配列を C の並びに
    pub fn contig(&mut self, py: Python<'_>) -> R<()> {
        for a in [&mut self.x, &mut self.r, &mut self.vl, &mut self.vl0, &mut self.applied, &mut self.ext_applied,
                  &mut self.inv_m, &mut self.iinv_b] {
            if let Some(v) = fix_contig(a.bind(py).as_any())? {
                *a = to_fa(&v)?;
            }
        }
        Ok(())
    }
    /// 固定の部分があるか
    pub fn any_fixed(&self, py: Python<'_>) -> R<bool> {
        Ok(self.fixed.bind(py).readonly().as_array().iter().any(|x| *x))
    }
}

impl Construct {
    /// 関節つき剛体の状態（無ければ None）
    pub fn art_state(slf: &Bound<'_, Self>) -> R<Option<Py<ArtState>>> {
        Ok(slf.try_borrow()?.art.as_ref().map(|r| r.clone_ref(slf.py())))
    }
    pub fn art_req(slf: &Bound<'_, Self>) -> R<Py<ArtState>> {
        Self::art_state(slf)?.ok_or_else(|| PyValueError::new_err("関節つき剛体の状態（art）がありません"))
    }
}
