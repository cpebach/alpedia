//! 描画の下まわり（main/client/gfx.py）：Rust の画面（alpedia_view.View）のノード・メッシュ・テクスチャ・文字。
//! RsNode は Panda3D の NodePath のうち、描画で使う分だけをまねる（親子・行列・表示・テクスチャ・半透明など）。

use std::sync::Mutex;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::imp;

type R<T> = PyResult<T>;

static VIEW: Mutex<Option<Py<PyAny>>> = Mutex::new(None);

/// 今の画面（alpedia_view.View）
pub fn view(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    match VIEW.lock().unwrap().as_ref() {
        Some(v) => Ok(v.clone_ref(py).into_bound(py)),
        None => Err(pyo3::exceptions::PyRuntimeError::new_err("画面がまだありません（gfx.start）")),
    }
}

#[pyfunction]
pub fn gfx_active() -> bool {
    VIEW.lock().unwrap().is_some()
}

/// Rust の画面を開く。使えなければ例外
#[pyfunction]
#[pyo3(signature = (width=1280.0, height=720.0, title="Alpedia", vsync=None))]
pub fn gfx_start<'py>(py: Python<'py>, width: f64, height: f64, title: &str, vsync: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
    let vs = match vsync { Some(v) => v.is_truthy()?, None => true };
    let v = imp(py, "alpedia_view")?.getattr("View")?.call1((width as i64, height as i64, title, vs))?;
    gfx_set_view(py, &v)?;
    Ok(v)
}

/// 画面を差し替える（gfx.VIEW も合わせる）
#[pyfunction]
pub fn gfx_set_view(py: Python<'_>, v: &Bound<'_, PyAny>) -> R<()> {
    *VIEW.lock().unwrap() = if v.is_none() { None } else { Some(v.clone().unbind()) };
    imp(py, "main.client.gfx")?.setattr("VIEW", v)?;
    Ok(())
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct RsTex {
    #[pyo3(get, set)]
    id: i64,
    #[pyo3(get, set)]
    path: PyObject,
}

#[pymethods]
impl RsTex {
    #[new]
    fn new(tid: i64, path: PyObject) -> Self {
        RsTex { id: tid, path }
    }
    // Panda3D のテクスチャの設定はまねるだけ
    #[pyo3(signature = (*_a))]
    #[pyo3(name = "setMagfilter")]
    fn set_magfilter(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(signature = (*_a))]
    #[pyo3(name = "setMinfilter")]
    fn set_minfilter(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(signature = (*_a))]
    #[pyo3(name = "setWrapU")]
    fn set_wrap_u(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(signature = (*_a))]
    #[pyo3(name = "setWrapV")]
    fn set_wrap_v(&self, _a: &Bound<'_, PyTuple>) {}
}

static TEX: pyo3::sync::GILOnceCell<Py<PyDict>> = pyo3::sync::GILOnceCell::new();

#[pyfunction]
#[pyo3(signature = (path, nearest=true))]
pub fn gfx_tex(py: Python<'_>, path: &Bound<'_, PyAny>, nearest: bool) -> R<PyObject> {
    if !path.is_truthy()? {
        return Ok(py.None());
    }
    let key = (path.str()?, nearest);
    let cache = TEX.get_or_init(py, || PyDict::new_bound(py).unbind()).bind(py);
    if let Some(t) = cache.get_item(&key)? {
        return Ok(t.unbind());
    }
    let tid: i64 = view(py)?.call_method1("tex_load", (path.str()?, nearest))?.extract()?;
    let t: PyObject = if tid >= 0 { Py::new(py, RsTex { id: tid, path: path.clone().unbind() })?.into_any() } else { py.None() };
    cache.set_item(key, &t)?;
    Ok(t)
}

#[pyfunction]
#[pyo3(signature = (w, h, data, name=None, nearest=false))]
pub fn gfx_tex_rgba(py: Python<'_>, w: &Bound<'_, PyAny>, h: &Bound<'_, PyAny>, data: &Bound<'_, PyAny>, name: Option<PyObject>, nearest: bool) -> R<RsTex> {
    let name = name.unwrap_or_else(|| "tex".into_py(py));
    let b = imp(py, "builtins")?.call_method1("bytes", (data,))?;
    let tid: i64 = view(py)?.call_method1("tex_rgba", (w.call_method0("__int__")?, h.call_method0("__int__")?, b, nearest))?.extract()?;
    Ok(RsTex { id: tid, path: name })
}

/// Rust の画面のノード（NodePath のまね）
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct RsNode {
    #[pyo3(get, name = "_parent")]
    parent: PyObject,
    #[pyo3(get, set)]
    id: i64,
    #[pyo3(get, set)]
    children: Py<PyList>,
    #[pyo3(get, set)]
    meshes: Py<PyList>,
    #[pyo3(get, set)]
    props: Py<PyDict>,
    #[pyo3(get, set)]
    tex: Py<PyList>,
    #[pyo3(get, set)]
    alive: bool,
    #[pyo3(get, set)]
    enabled: bool,
}

fn is_node(o: &Bound<'_, PyAny>) -> bool {
    o.is_instance_of::<RsNode>()
}

impl RsNode {
    pub fn make(py: Python<'_>, parent: Option<&Bound<'_, PyAny>>) -> R<Self> {
        let p = parent.filter(|p| is_node(p));
        let pid: i64 = match p { Some(p) => p.getattr("id")?.extract()?, None => 0 };
        let id: i64 = view(py)?.call_method1("node_new", (pid,))?.extract()?;
        Ok(RsNode {
            parent: parent.map(|p| p.clone().unbind()).unwrap_or_else(|| py.None()), id,
            children: PyList::empty_bound(py).unbind(), meshes: PyList::empty_bound(py).unbind(), props: PyDict::new_bound(py).unbind(),
            tex: PyList::new_bound(py, [py.None(), py.None()]).unbind(), alive: true, enabled: true,
        })
    }
    pub fn create<'py>(py: Python<'py>, parent: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, RsNode>> {
        let n = Bound::new(py, Self::make(py, parent)?)?;
        if let Some(p) = parent.filter(|p| is_node(p)) {
            p.getattr("children")?.call_method1("append", (&n,))?;
        }
        Ok(n)
    }
    fn effective<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let par = slf.borrow().parent.clone_ref(py).into_bound(py);
        let out = if is_node(&par) { Self::effective(par.downcast::<RsNode>()?)?.copy()? } else { PyDict::new_bound(py) };
        out.update(slf.borrow().props.bind(py).as_mapping())?;
        Ok(out)
    }
    fn tex_eff<'py>(slf: &Bound<'py, Self>, slot: usize) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let t = slf.borrow().tex.bind(py).get_item(slot)?;
        if !t.is_none() {
            return Ok(t);
        }
        let par = slf.borrow().parent.clone_ref(py).into_bound(py);
        if is_node(&par) {
            Self::tex_eff(par.downcast::<RsNode>()?, slot)
        } else {
            Ok(py.None().into_bound(py))
        }
    }
    pub fn apply(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let v = view(py)?;
        let p = Self::effective(slf)?;
        let g = |k: &str, d: PyObject| -> R<Bound<'_, PyAny>> { Ok(p.get_item(k)?.unwrap_or_else(|| d.into_bound(py))) };
        let meshes: Vec<Bound<'_, PyAny>> = slf.borrow().meshes.bind(py).iter().collect();
        if !meshes.is_empty() {
            let (blend, dw, ts, off, bin, add) = (g("blend", false.into_py(py))?.is_truthy()?, g("depth_write", true.into_py(py))?.is_truthy()?,
                g("two_sided", true.into_py(py))?.is_truthy()?, g("depth_offset", 0.into_py(py))?, g("bin", 0.into_py(py))?, g("add", false.into_py(py))?.is_truthy()?);
            let t0 = Self::tex_eff(slf, 0)?;
            let t1 = Self::tex_eff(slf, 1)?;
            let tid = |t: &Bound<'_, PyAny>| -> R<i64> { if t.is_none() { Ok(-1) } else { t.getattr("id")?.extract() } };
            let (i0, i1) = (tid(&t0)?, tid(&t1)?);
            for m in &meshes {
                v.call_method1("mesh_props", (m, blend, dw, ts, &off, &bin))?;
                v.call_method1("mesh_add", (m, add))?;
                v.call_method1("mesh_tex", (m, 0, i0))?;
                v.call_method1("mesh_tex", (m, 1, i1))?;
            }
        }
        let ch: Vec<Bound<'_, PyAny>> = slf.borrow().children.bind(py).iter().collect();
        for c in ch {
            if let Ok(c) = c.downcast::<RsNode>() {
                Self::apply(c)?;
            } else {
                c.call_method0("_apply")?;
            }
        }
        Ok(())
    }
    fn set_props(slf: &Bound<'_, Self>, k: &str, v: PyObject) -> R<()> {
        slf.borrow().props.bind(slf.py()).set_item(k, v)?;
        Self::apply(slf)
    }
    fn mark_dead(slf: &Bound<'_, Self>) {
        slf.borrow_mut().alive = false;
        let py = slf.py();
        let ch: Vec<Bound<'_, PyAny>> = slf.borrow().children.bind(py).iter().collect();
        for c in ch {
            if let Ok(c) = c.downcast::<RsNode>() {
                Self::mark_dead(c);
            }
        }
    }
}

#[pymethods]
impl RsNode {
    #[new]
    #[pyo3(signature = (parent=None, name=None))]
    fn new(py: Python<'_>, parent: Option<&Bound<'_, PyAny>>, name: Option<PyObject>) -> R<Self> {
        let _ = name;
        Self::make(py, parent)
    }
    /// 作ったあと：親の子の一覧に入れる（Python から RsNode(...) で作ったとき）
    #[pyo3(signature = (parent=None, name=None))]
    fn __init__(slf: &Bound<'_, Self>, parent: Option<&Bound<'_, PyAny>>, name: Option<PyObject>) -> R<()> {
        let _ = name;
        if let Some(p) = parent.filter(|p| is_node(p)) {
            let ch = p.getattr("children")?;
            if !ch.contains(slf)? {
                ch.call_method1("append", (slf,))?;
            }
        }
        Ok(())
    }
    // ---- 親子 ----
    #[pyo3(name = "attachNewNode", signature = (name=None))]
    fn attach_new_node<'py>(slf: &Bound<'py, Self>, name: Option<PyObject>) -> R<Bound<'py, RsNode>> {
        let _ = name;
        Self::create(slf.py(), Some(slf.as_any()))
    }
    #[getter]
    fn parent(&self, py: Python<'_>) -> PyObject {
        self.parent.clone_ref(py)
    }
    #[setter]
    fn set_parent(slf: &Bound<'_, Self>, p: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let old = slf.borrow().parent.clone_ref(py).into_bound(py);
        if p.is(&old) {
            return Ok(());
        }
        if is_node(&old) {
            let ch = old.getattr("children")?;
            if ch.contains(slf)? {
                ch.call_method1("remove", (slf,))?;
            }
        }
        slf.borrow_mut().parent = p.clone().unbind();
        let id = slf.borrow().id;
        if is_node(p) {
            p.getattr("children")?.call_method1("append", (slf,))?;
            view(py)?.call_method1("node_parent", (id, p.getattr("id")?))?;
        } else {
            view(py)?.call_method1("node_parent", (id, 0))?;
        }
        Ok(())
    }
    #[pyo3(name = "reparentTo")]
    fn reparent_to(slf: &Bound<'_, Self>, p: &Bound<'_, PyAny>) -> R<()> {
        Self::set_parent(slf, p)
    }
    #[pyo3(name = "removeNode")]
    fn remove_node(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        if !slf.borrow().alive {
            return Ok(());
        }
        let id = slf.borrow().id;
        view(py)?.call_method1("node_remove", (id,))?;
        Self::mark_dead(slf);
        let par = slf.borrow().parent.clone_ref(py).into_bound(py);
        if is_node(&par) {
            let ch = par.getattr("children")?;
            if ch.contains(slf)? {
                ch.call_method1("remove", (slf,))?;
            }
        }
        Ok(())
    }
    fn _mark_dead(slf: &Bound<'_, Self>) {
        Self::mark_dead(slf)
    }
    // ---- 位置 ----
    #[pyo3(name = "setMat")]
    fn set_mat(slf: &Bound<'_, Self>, m: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let mut flat: Vec<f64> = vec![];
        let mut nested = false;
        for x in m.iter()? {
            let x = x?;
            match x.extract::<f64>() {
                Ok(f) => flat.push(f),
                Err(_) => {
                    nested = true;
                    break;
                }
            }
        }
        if nested {
            flat.clear();
            for row in m.iter()? {
                for x in row?.iter()? {
                    flat.push(x?.extract()?);
                }
            }
        }
        view(py)?.call_method1("node_mat", (slf.borrow().id, flat))?;
        Ok(())
    }
    #[pyo3(name = "setPos", signature = (x, y=None, z=None))]
    fn set_pos(slf: &Bound<'_, Self>, x: &Bound<'_, PyAny>, y: Option<f64>, z: Option<f64>) -> R<()> {
        let (a, b, c) = match (y, z) {
            (Some(y), Some(z)) => (x.extract::<f64>()?, y, z),
            _ => {
                let v = super::construct::v3(x)?;
                (v[0], v[1], v[2])
            }
        };
        view(slf.py())?.call_method1("node_pos", (slf.borrow().id, a, b, c))?;
        Ok(())
    }
    fn show(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow_mut().enabled = true;
        view(slf.py())?.call_method1("node_show", (slf.borrow().id, true))?;
        Ok(())
    }
    fn hide(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow_mut().enabled = false;
        view(slf.py())?.call_method1("node_show", (slf.borrow().id, false))?;
        Ok(())
    }
    #[pyo3(name = "setColorScale", signature = (r, g, b, a=1.0))]
    fn set_color_scale(slf: &Bound<'_, Self>, r: f64, g: f64, b: f64, a: f64) -> R<()> {
        view(slf.py())?.call_method1("node_tint", (slf.borrow().id, r, g, b, a))?;
        Ok(())
    }
    // ---- 見え方 ----
    fn _apply(slf: &Bound<'_, Self>) -> R<()> {
        Self::apply(slf)
    }
    fn _effective<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        Self::effective(slf)
    }
    fn _tex_eff<'py>(slf: &Bound<'py, Self>, slot: usize) -> R<Bound<'py, PyAny>> {
        Self::tex_eff(slf, slot)
    }
    #[pyo3(signature = (**kw))]
    fn _set(slf: &Bound<'_, Self>, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        if let Some(k) = kw {
            slf.borrow().props.bind(slf.py()).update(k.as_mapping())?;
        }
        Self::apply(slf)
    }
    #[pyo3(name = "setTransparency", signature = (on=None, *_a))]
    fn set_transparency(slf: &Bound<'_, Self>, on: Option<&Bound<'_, PyAny>>, _a: &Bound<'_, PyTuple>) -> R<()> {
        let on = match on { Some(o) => o.is_truthy()?, None => true };
        Self::set_props(slf, "blend", on.into_py(slf.py()))
    }
    #[pyo3(name = "setDepthWrite", signature = (on, *_a))]
    fn set_depth_write(slf: &Bound<'_, Self>, on: &Bound<'_, PyAny>, _a: &Bound<'_, PyTuple>) -> R<()> {
        Self::set_props(slf, "depth_write", on.is_truthy()?.into_py(slf.py()))
    }
    #[pyo3(name = "setTwoSided", signature = (on, *_a))]
    fn set_two_sided(slf: &Bound<'_, Self>, on: &Bound<'_, PyAny>, _a: &Bound<'_, PyTuple>) -> R<()> {
        Self::set_props(slf, "two_sided", on.is_truthy()?.into_py(slf.py()))
    }
    #[pyo3(name = "setDepthOffset", signature = (n, *_a))]
    fn set_depth_offset(slf: &Bound<'_, Self>, n: &Bound<'_, PyAny>, _a: &Bound<'_, PyTuple>) -> R<()> {
        Self::set_props(slf, "depth_offset", n.call_method0("__int__")?.unbind())
    }
    #[pyo3(name = "setBin", signature = (name, n=0, *_a))]
    fn set_bin(slf: &Bound<'_, Self>, name: &Bound<'_, PyAny>, n: i64, _a: &Bound<'_, PyTuple>) -> R<()> {
        let base = if name.eq("fixed")? { 100 } else { 0 };
        Self::set_props(slf, "bin", (base + n).into_py(slf.py()))
    }
    /// 足し算で重ねる（光る物）
    #[pyo3(name = "setBlendAdd", signature = (on=true))]
    fn set_blend_add(slf: &Bound<'_, Self>, on: bool) -> R<()> {
        Self::set_props(slf, "add", on.into_py(slf.py()))
    }
    /// Panda3D の ColorBlendAttrib.make(M_add, …) をまねる（足し算で重ねる）。ほかは何もしない
    #[pyo3(name = "setAttrib", signature = (attr, *_a))]
    fn set_attrib(slf: &Bound<'_, Self>, attr: &Bound<'_, PyAny>, _a: &Bound<'_, PyTuple>) -> R<()> {
        if let Ok(args) = attr.getattr("args") {
            if args.is_truthy()? && args.get_item(0)?.eq(1)? {
                return Self::set_blend_add(slf, true);
            }
        }
        Ok(())
    }
    #[pyo3(name = "setTexture", signature = (*args))]
    fn set_texture(slf: &Bound<'_, Self>, args: &Bound<'_, PyTuple>) -> R<()> {
        let py = slf.py();
        let tex = slf.borrow().tex.clone_ref(py);
        if args.len() >= 2 && args.get_item(0)?.eq("edges")? {
            tex.bind(py).set_item(1, args.get_item(1)?)?;
        } else if !args.is_empty() && args.get_item(0)?.is_instance_of::<RsTex>() {
            tex.bind(py).set_item(0, args.get_item(0)?)?;
        }
        Self::apply(slf)
    }
    #[pyo3(name = "clearTexture", signature = (*_a))]
    fn clear_texture(slf: &Bound<'_, Self>, _a: &Bound<'_, PyTuple>) -> R<()> {
        let py = slf.py();
        slf.borrow_mut().tex = PyList::new_bound(py, [py.None(), py.None()]).unbind();
        Self::apply(slf)
    }
    // Panda3D の設定でまねるだけの物
    #[pyo3(name = "setLightOff", signature = (*_a))]
    fn set_light_off(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setShaderOff", signature = (*_a))]
    fn set_shader_off(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setFinal", signature = (*_a))]
    fn set_final(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setBounds", signature = (*_a))]
    fn set_bounds(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "clearBounds", signature = (*_a))]
    fn clear_bounds(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setColor", signature = (*_a))]
    fn set_color(&self, _a: &Bound<'_, PyTuple>) {}
}

/// render.DynamicMesh の Rust 版：頂点 (N,12) と三角形を numpy 配列から設定する
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct RsDynamicMesh {
    #[pyo3(get, set)]
    np: PyObject,
    #[pyo3(get, set)]
    node: PyObject,
    #[pyo3(get, set)]
    mid: i64,
    #[pyo3(get, set)]
    n_rows: i64,
}

impl RsDynamicMesh {
    pub fn create<'py>(py: Python<'py>, parent: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, RsDynamicMesh>> {
        let n = RsNode::create(py, parent.filter(|p| is_node(p)))?;
        let mid: i64 = view(py)?.call_method1("mesh_new", (n.borrow().id,))?.extract()?;
        n.borrow().meshes.bind(py).append(mid)?;
        RsNode::apply(&n)?;
        Bound::new(py, RsDynamicMesh { np: n.clone().into_any().unbind(), node: n.into_any().unbind(), mid, n_rows: -1 })
    }
}

#[pymethods]
impl RsDynamicMesh {
    #[new]
    #[pyo3(signature = (parent=None, name=None))]
    fn new(py: Python<'_>, parent: Option<&Bound<'_, PyAny>>, name: Option<PyObject>) -> R<Self> {
        let _ = name;
        let m = Self::create(py, parent)?;
        let b = m.borrow();
        Ok(RsDynamicMesh { np: b.np.clone_ref(py), node: b.node.clone_ref(py), mid: b.mid, n_rows: -1 })
    }
    #[pyo3(signature = (verts, tris=None))]
    fn set(slf: &Bound<'_, Self>, verts: &Bound<'_, PyAny>, tris: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        let np = imp(py, "numpy")?;
        let v = np.call_method1("ascontiguousarray", (verts, np.getattr("float32")?))?;
        let mid = slf.borrow().mid;
        let n = v.len()?;
        if n == 0 {
            view(py)?.call_method1("mesh_set", (mid, np.call_method1("zeros", ((0, 12), "float32"))?, np.call_method1("zeros", ((0, 3), "int64"))?))?;
            slf.borrow_mut().n_rows = 0;
            return Ok(());
        }
        let tris = tris.filter(|t| !t.is_none());
        if tris.is_none() && n as i64 != slf.borrow().n_rows {
            return Err(pyo3::exceptions::PyValueError::new_err("頂点数が変わる時は tris が必要です"));
        }
        let t: PyObject = match tris {
            Some(t) => np.call_method1("ascontiguousarray", (t, "int64"))?.call_method1("reshape", (-1, 3))?.unbind(),
            None => py.None(),
        };
        view(py)?.call_method1("mesh_set", (mid, v.call_method1("reshape", (-1, 12))?, t))?;
        slf.borrow_mut().n_rows = n as i64;
        Ok(())
    }
    fn remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow().np.bind(slf.py()).call_method0("removeNode")?;
        Ok(())
    }
}

// ---------------------------------------------------------------- 文字・板

static FONT_OK: Mutex<Option<bool>> = Mutex::new(None);

/// フォントを読む（1回だけ）。path が無ければ、いつもの候補から探す
#[pyfunction]
#[pyo3(signature = (path=None))]
pub fn gfx_font(py: Python<'_>, path: Option<&Bound<'_, PyAny>>) -> R<bool> {
    if let Some(v) = *FONT_OK.lock().unwrap() {
        return Ok(v);
    }
    let mut cands: Vec<Bound<'_, PyAny>> = vec![];
    if let Some(p) = path.filter(|p| p.is_truthy().unwrap_or(false)) {
        cands.push(p.clone());
    }
    for f in imp(py, "main.client.game")?.getattr("FONT_CANDIDATES")?.iter()? {
        cands.push(f?);
    }
    let pathcls = imp(py, "pathlib")?.getattr("Path")?;
    for f in cands {
        if f.is_truthy()? && pathcls.call1((&f,))?.call_method0("exists")?.is_truthy()? {
            if view(py)?.call_method1("font_load", (f.str()?, 0, 40.0)).is_ok() {
                *FONT_OK.lock().unwrap() = Some(true);
                return Ok(true);
            }
        }
    }
    *FONT_OK.lock().unwrap() = Some(false);
    Ok(false)
}

fn quad_verts(x0: f64, y0: f64, x1: f64, y1: f64, rgba: [f64; 4], z: f64) -> Vec<f32> {
    let mut v = vec![0f32; 48];
    let pts = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for k in 0..4 {
        let r = &mut v[12 * k..12 * k + 12];
        r[0] = pts[k][0] as f32;
        r[1] = pts[k][1] as f32;
        r[2] = z as f32;
        r[5] = -1.0;
        for c in 0..4 {
            r[6 + c] = rgba[c] as f32;
        }
        r[10] = uv[k][0] as f32;
        r[11] = uv[k][1] as f32;
    }
    v
}
fn rgba4(o: &Bound<'_, PyAny>) -> R<[f64; 4]> {
    let v: Vec<f64> = o.iter()?.map(|x| x?.extract::<f64>()).collect::<R<_>>()?;
    Ok([v[0], v[1], v[2], if v.len() > 3 { v[3] } else { 1.0 }])
}
pub fn set_quad(py: Python<'_>, mesh: &Bound<'_, PyAny>, x0: f64, y0: f64, x1: f64, y1: f64, rgba: [f64; 4], z: f64, uv: bool) -> R<()> {
    let mut v = quad_verts(x0, y0, x1, y1, rgba, z);
    if !uv {
        for k in 0..4 {
            v[12 * k + 10] = 0.0;
            v[12 * k + 11] = 0.0;
        }
    }
    let va = numpy::PyArray1::from_vec_bound(py, v);
    let va = numpy::PyArrayMethods::reshape(&va, [4, 12])?;
    let t = numpy::PyArray1::from_vec_bound(py, vec![0i64, 1, 2, 0, 2, 3]);
    let t = numpy::PyArrayMethods::reshape(&t, [2, 3])?;
    mesh.call_method1("set", (va, t))?;
    Ok(())
}

/// メッシュに色の四角を1つ（文字の下地など）
#[pyfunction]
#[pyo3(signature = (mesh, x0, y0, x1, y1, rgba, z=0.0))]
pub fn gfx_quad(py: Python<'_>, mesh: &Bound<'_, PyAny>, x0: f64, y0: f64, x1: f64, y1: f64, rgba: &Bound<'_, PyAny>, z: f64) -> R<()> {
    let c = rgba4(rgba)?;
    set_quad(py, mesh, x0, y0, x1, y1, c, z, false)?;
    view(py)?.call_method1("mesh_props", (mesh.getattr("mid")?, c[3] < 0.999, true, true, 0, 0))?;
    Ok(())
}

/// 文字（と下地）。overlay なら画面の上（縦の高さ 1、真ん中が原点）、そうでなければ親ノードの座標で
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct RsText {
    #[pyo3(get, set)]
    node: PyObject,
    #[pyo3(get, set)]
    bg: PyObject,
    #[pyo3(get, set)]
    text: PyObject,
    #[pyo3(get, set)]
    overlay: bool,
    #[pyo3(get, set)]
    bgc: PyObject,
    #[pyo3(get, set)]
    bin: i64,
    #[pyo3(get, set)]
    key: PyObject,
    #[pyo3(get, set)]
    size: (f64, f64),
}

#[pymethods]
impl RsText {
    #[new]
    #[pyo3(signature = (parent=None, overlay=true, bg=None, bin=0))]
    fn new(py: Python<'_>, parent: Option<&Bound<'_, PyAny>>, overlay: bool, bg: Option<PyObject>, bin: i64) -> R<Self> {
        let parent = parent.filter(|p| !p.is_none());
        let node = RsNode::create(py, parent)?;
        let bgm: PyObject = match &bg { Some(b) if !b.is_none(py) => RsDynamicMesh::create(py, Some(node.as_any()))?.into_any().unbind(), _ => py.None() };
        let tm = RsDynamicMesh::create(py, Some(node.as_any()))?;
        let v = view(py)?;
        for m in [bgm.bind(py), tm.as_any()] {
            if !m.is_none() {
                v.call_method1("mesh_overlay", (m.getattr("mid")?, overlay))?;
            }
        }
        Ok(RsText { node: node.into_any().unbind(), bg: bgm, text: tm.into_any().unbind(), overlay, bgc: bg.unwrap_or_else(|| py.None()), bin,
            key: py.None(), size: (0.0, 0.0) })
    }
    /// 文字を変える（同じなら何もしない）。戻り値 (幅, 高さ)
    #[pyo3(signature = (text, size=0.025, color=None, max_width=0.0, align=0, pad=0.008))]
    fn set(slf: &Bound<'_, Self>, text: &Bound<'_, PyAny>, size: f64, color: Option<&Bound<'_, PyAny>>, max_width: f64, align: i64, pad: f64)
        -> R<(f64, f64)> {
        let py = slf.py();
        let col: Vec<f64> = match color { Some(c) => c.iter()?.map(|x| x?.extract::<f64>()).collect::<R<_>>()?, None => vec![1.0, 1.0, 1.0, 1.0] };
        let key = PyTuple::new_bound(py, [text.clone().unbind(), size.into_py(py), PyTuple::new_bound(py, &col).into_py(py), max_width.into_py(py), align.into_py(py)]);
        if key.eq(slf.borrow().key.bind(py))? {
            return Ok(slf.borrow().size);
        }
        slf.borrow_mut().key = key.into_any().unbind();
        let (tm, bg, overlay, bin) = { let b = slf.borrow(); (b.text.clone_ref(py).into_bound(py), b.bg.clone_ref(py).into_bound(py), b.overlay, b.bin) };
        let v = view(py)?;
        let tmid = tm.getattr("mid")?;
        if !gfx_font(py, None)? || !text.is_truthy()? {
            let np = imp(py, "numpy")?;
            v.call_method1("mesh_set", (&tmid, np.call_method1("zeros", ((0, 12), "float32"))?, np.call_method1("zeros", ((0, 3), "int64"))?))?;
            if !bg.is_none() {
                bg.getattr("np")?.call_method0("hide")?;
            }
            slf.borrow_mut().size = (0.0, 0.0);
            return Ok((0.0, 0.0));
        }
        let (w, h): (f64, f64) = v.call_method1("text_mesh", (&tmid, text.str()?, size, col, max_width, align))?.extract()?;
        v.call_method1("mesh_props", (&tmid, true, false, true, if overlay { 0 } else { 2 }, bin + 1))?;
        if !bg.is_none() {
            bg.getattr("np")?.call_method0("show")?;
            let bgc = rgba4(slf.borrow().bgc.bind(py))?;
            gfx_quad(py, &bg, -pad, pad, w + pad, -h - pad, &PyTuple::new_bound(py, bgc), 0.0)?;
            v.call_method1("mesh_props", (bg.getattr("mid")?, bgc[3] < 0.999, !overlay, true, 0, bin))?;
        }
        slf.borrow_mut().size = (w, h);
        Ok((w, h))
    }
    fn place(slf: &Bound<'_, Self>, x: f64, y: f64) -> R<()> {
        slf.borrow().node.bind(slf.py()).call_method1("setPos", (x, y, 0.0))?;
        Ok(())
    }
    #[pyo3(signature = (on=true))]
    fn show(slf: &Bound<'_, Self>, on: bool) -> R<()> {
        slf.borrow().node.bind(slf.py()).call_method0(if on { "show" } else { "hide" })?;
        Ok(())
    }
    fn remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow().node.bind(slf.py()).call_method0("removeNode")?;
        Ok(())
    }
}

/// コンピューターの表示（看板）の Rust の画面版
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct RsDisplayView {
    #[pyo3(get, set)]
    root: PyObject,
    #[pyo3(get, set)]
    inner: PyObject,
    #[pyo3(get, set)]
    bg: PyObject,
    #[pyo3(get, set)]
    txt: PyObject,
    #[pyo3(get, set)]
    key: PyObject,
    #[pyo3(get, set)]
    placed: PyObject,
}

#[pymethods]
impl RsDisplayView {
    #[new]
    #[pyo3(signature = (parent=None))]
    fn new(py: Python<'_>, parent: Option<&Bound<'_, PyAny>>) -> R<Self> {
        let root = RsNode::create(py, parent)?;
        let inner = RsNode::create(py, Some(root.as_any()))?;
        let bg = RsDynamicMesh::create(py, Some(root.as_any()))?;
        let txt = Py::new(py, RsText::new(py, Some(inner.as_any()), false, None, 0)?)?;
        Ok(RsDisplayView { root: root.into_any().unbind(), inner: inner.into_any().unbind(), bg: bg.into_any().unbind(), txt: txt.into_any(),
            key: py.None(), placed: py.None() })
    }
    fn set(slf: &Bound<'_, Self>, disp: &Bound<'_, PyAny>, voxel_size: f64) -> R<()> {
        let py = slf.py();
        let tup = |o: Bound<'_, PyAny>| -> R<Bound<'_, PyTuple>> { Ok(PyTuple::new_bound(py, o.iter()?.collect::<R<Vec<_>>>()?)) };
        let width = disp.call_method1("get", ("width",))?;
        let key = PyTuple::new_bound(py, [disp.get_item("text")?, width.clone(), tup(disp.call_method1("get", ("color", PyTuple::empty_bound(py)))?)?.into_any(),
            tup(disp.call_method1("get", ("background", PyTuple::empty_bound(py)))?)?.into_any(), voxel_size.into_py(py).into_bound(py)]);
        if key.eq(slf.borrow().key.bind(py))? {
            return Ok(());
        }
        slf.borrow_mut().key = key.into_any().unbind();
        let col = rgba4(&disp.call_method1("get", ("color", vec![1, 1, 1]))?)?;
        let bgc = rgba4(&disp.call_method1("get", ("background", vec![0.04, 0.05, 0.08, 0.92]))?)?;
        let txt = slf.borrow().txt.clone_ref(py).into_bound(py);
        let (tw, th): (f64, f64) = txt.call_method1("set", (disp.get_item("text")?, 1.0, (col[0], col[1], col[2], 1.0)))?.extract()?;
        let (tw, th) = (tw.max(1e-6), th.max(1e-6));
        let w_target = if width.is_truthy()? { width.extract::<f64>()? } else { voxel_size * 0.96 };
        let mut k = w_target * 0.9 / tw;
        let mut h = th * k / 0.88;
        if width.is_none() {
            if h > voxel_size * 0.96 {
                k *= voxel_size * 0.96 / h;
                h = voxel_size * 0.96;
            }
            h = h.max(voxel_size * 0.96);
        }
        // 文字：左揃え・全体は中央（文字の座標は 1行目の上が 0、下へ負）。看板の表は -Z の向き
        let m = vec![k, 0.0, 0.0, 0.0, 0.0, k, 0.0, 0.0, 0.0, 0.0, k, 0.0, -tw * k / 2.0, th * k / 2.0, -0.004, 1.0];
        slf.borrow().inner.bind(py).call_method1("setMat", (m,))?;
        gfx_quad(py, slf.borrow().bg.bind(py), -w_target / 2.0, -h / 2.0, w_target / 2.0, h / 2.0, &PyTuple::new_bound(py, [bgc[0], bgc[1], bgc[2], 1.0]), 0.0)
    }
    fn place(slf: &Bound<'_, Self>, center: &Bound<'_, PyAny>, right: &Bound<'_, PyAny>, up: &Bound<'_, PyAny>, fwd: &Bound<'_, PyAny>) -> R<()> {
        use super::construct::v3;
        let (c, r, u, f) = (v3(center)?, v3(right)?, v3(up)?, v3(fwd)?);
        slf.borrow().root.bind(slf.py()).call_method1("setMat", (vec![r[0], r[1], r[2], 0.0, u[0], u[1], u[2], 0.0, f[0], f[1], f[2], 0.0, c[0], c[1], c[2], 1.0],))?;
        Ok(())
    }
    fn remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow().root.bind(slf.py()).call_method0("removeNode")?;
        Ok(())
    }
}

pub const BOX_FACES: [[usize; 4]; 6] = [[1, 3, 7, 5], [0, 4, 6, 2], [2, 6, 7, 3], [0, 1, 5, 4], [4, 5, 7, 6], [0, 2, 3, 1]];
pub const BOX_N: [[f64; 3]; 6] = [[1., 0., 0.], [-1., 0., 0.], [0., 1., 0.], [0., -1., 0.], [0., 0., 1.], [0., 0., -1.]];

/// 箱（-0.5〜0.5）の頂点 (24,12) と三角形。陰影は面ごとに少し変える
pub fn box_verts(rgba: [f64; 4], lo: f64, hi: f64) -> (Vec<f32>, Vec<i64>) {
    let mut v = vec![0f32; 24 * 12];
    let shade = [0.85, 0.7, 1.0, 0.55, 0.9, 0.75];
    for f in 0..6 {
        for (j, &k) in BOX_FACES[f].iter().enumerate() {
            let r = &mut v[(f * 4 + j) * 12..(f * 4 + j) * 12 + 12];
            for a in 0..3 {
                let bit = ((k >> a) & 1) as f64 * 2.0 - 1.0;
                r[a] = ((bit * 0.5 + 0.5) * (hi - lo) + lo) as f32;
                r[3 + a] = BOX_N[f][a] as f32;
                r[6 + a] = (rgba[a] * shade[f]) as f32;
            }
            r[9] = rgba[3] as f32;
        }
    }
    let mut t = vec![];
    for q in 0..6i64 {
        t.extend([q * 4, q * 4 + 1, q * 4 + 2]);
    }
    for q in 0..6i64 {
        t.extend([q * 4, q * 4 + 2, q * 4 + 3]);
    }
    (v, t)
}

#[pyfunction]
#[pyo3(signature = (rgba, lo=-0.5, hi=0.5))]
pub fn gfx_box_vertices<'py>(py: Python<'py>, rgba: &Bound<'py, PyAny>, lo: f64, hi: f64) -> R<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let (v, t) = box_verts(rgba4(rgba)?, lo, hi);
    Ok((numpy::PyArrayMethods::reshape(&numpy::PyArray1::from_vec_bound(py, v), [24, 12])?.into_any(),
        numpy::PyArrayMethods::reshape(&numpy::PyArray1::from_vec_bound(py, t), [12, 3])?.into_any()))
}

/// 枠だけの箱（12 本の細い棒。単位の箱 -0.5〜0.5 を、ノードの行列で置く）。戻り値はノード
#[pyfunction]
#[pyo3(signature = (parent, rgba, thick=0.03))]
pub fn gfx_wire_box<'py>(py: Python<'py>, parent: Option<&Bound<'py, PyAny>>, rgba: &Bound<'py, PyAny>, thick: f64) -> R<Bound<'py, RsNode>> {
    let node = RsNode::create(py, parent)?;
    let c4 = rgba4(rgba)?;
    let (bv, bt) = box_verts(c4, -0.5, 0.5);
    let (mut vv, mut tt) = (vec![], vec![]);
    let h = 0.5;
    let mut nbox = 0i64;
    for ax in 0..3 {
        for s1 in [-h, h] {
            for s2 in [-h, h] {
                let mut c = [0.0; 3];
                c[(ax + 1) % 3] = s1;
                c[(ax + 2) % 3] = s2;
                let mut e = [thick; 3];
                e[ax] = 1.0 + thick;
                let mut v = bv.clone();
                for r in v.chunks_exact_mut(12) {
                    for a in 0..3 {
                        r[a] = (r[a] as f64 * e[a] + c[a]) as f32;
                    }
                }
                tt.extend(bt.iter().map(|x| x + nbox * 24));
                vv.extend(v);
                nbox += 1;
            }
        }
    }
    let m = RsDynamicMesh::create(py, Some(node.as_any()))?;
    let nv = vv.len() / 12;
    let nt = tt.len() / 3;
    RsDynamicMesh::set(&m, numpy::PyArrayMethods::reshape(&numpy::PyArray1::from_vec_bound(py, vv), [nv, 12])?.as_any(),
        Some(numpy::PyArrayMethods::reshape(&numpy::PyArray1::from_vec_bound(py, tt), [nt, 3])?.as_any()))?;
    view(py)?.call_method1("mesh_props", (m.borrow().mid, c4[3] < 0.999, true, true, 1, 0))?;
    Ok(node)
}

/// ノードを (中心, 軸 (3,3) 行が x・y・z, 大きさ (3,)) に置く
#[pyfunction]
pub fn gfx_set_box(node: &Bound<'_, PyAny>, center: &Bound<'_, PyAny>, axes: &Bound<'_, PyAny>, scale: &Bound<'_, PyAny>) -> R<()> {
    use super::construct::{m3, v3};
    let a = m3(axes)?;
    let s = v3(scale)?;
    let c = v3(center)?;
    let m: Vec<f64> = vec![a[0][0] * s[0], a[0][1] * s[0], a[0][2] * s[0], 0.0, a[1][0] * s[1], a[1][1] * s[1], a[1][2] * s[1], 0.0,
        a[2][0] * s[2], a[2][1] * s[2], a[2][2] * s[2], 0.0, c[0], c[1], c[2], 1.0];
    node.call_method1("setMat", (m,))?;
    Ok(())
}
