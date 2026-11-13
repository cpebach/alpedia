//! Rust の画面の UI の部品（main/client_rs/ui.py）：板・文字・ボタン・画像・入力欄・画面の重なり。
//! 座標は画面の上（縦の高さが 1、真ん中が原点、x 右・y 上）。画面は部品を持ち、build() で作り直すのが基本。

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::gfx::{view, RsDynamicMesh, RsNode};
use pyo3::types::IntoPyDict;
use super::imp;

pub type R<T> = PyResult<T>;

/// obj.name に args を先に渡した関数（ボタンの押したとき などに使う）
pub fn cb<'py>(obj: &Bound<'py, PyAny>, name: &str, args: impl IntoPy<Py<PyTuple>>) -> R<Bound<'py, PyAny>> {
    let py = obj.py();
    let a: Py<PyTuple> = args.into_py(py);
    let mut v: Vec<PyObject> = vec![obj.getattr(name)?.unbind()];
    v.extend(a.bind(py).iter().map(|x| x.unbind()));
    imp(py, "functools")?.getattr("partial")?.call1(PyTuple::new_bound(py, v))
}
fn text_new<'py>(py: Python<'py>, root: &Bound<'py, PyAny>, bin: i64) -> R<Bound<'py, PyAny>> {
    let k = PyDict::new_bound(py);
    k.set_item("overlay", true)?;
    k.set_item("bin", bin)?;
    imp(py, "alpedia_core")?.getattr("RsText")?.call((root,), Some(&k))
}
pub fn rgba(v: [f64; 4]) -> (f64, f64, f64, f64) {
    (v[0], v[1], v[2], v[3])
}

static ICON_TEX: pyo3::sync::GILOnceCell<Py<PyDict>> = pyo3::sync::GILOnceCell::new();

/// アイテムのアイコンのテクスチャ
#[pyfunction]
pub fn ui_item_tex<'py>(py: Python<'py>, reg: &Bound<'py, PyAny>, item: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let key = PyTuple::new_bound(py, item.iter()?.collect::<R<Vec<_>>>()?);
    let cache = ICON_TEX.get_or_init(py, || PyDict::new_bound(py).unbind()).bind(py);
    if let Some(t) = cache.get_item(&key)? {
        return Ok(t);
    }
    let img = imp(py, "main.client.icons")?.call_method1("item_image", (reg, item))?.call_method1("convert", ("RGBA",))?;
    let t = super::gfx::gfx_tex_rgba(py, &img.getattr("width")?, &img.getattr("height")?, &img.call_method0("tobytes")?,
        Some(format!("icon_{}", key.repr()?).into_py(py)), false)?;
    let t = Py::new(py, t)?.into_bound(py).into_any();
    cache.set_item(key, &t)?;
    Ok(t)
}

/// 色の四角（tex があれば画像）
#[pyfunction]
#[pyo3(signature = (node, x0, y0, x1, y1, rgba, tex=None, z=0.0))]
#[allow(clippy::too_many_arguments)]
pub fn ui_rect_mesh<'py>(py: Python<'py>, node: &Bound<'py, PyAny>, x0: f64, y0: f64, x1: f64, y1: f64, rgba: &Bound<'py, PyAny>,
                         tex: Option<&Bound<'py, PyAny>>, z: f64) -> R<Bound<'py, PyAny>> {
    let m = RsDynamicMesh::create(py, Some(node))?;
    let v = view(py)?;
    let mid: i64 = m.getattr("mid")?.extract()?;
    v.call_method1("mesh_overlay", (mid, true))?;
    let c: Vec<f64> = rgba.iter()?.map(|x| x?.extract::<f64>()).collect::<R<_>>()?;
    super::gfx::set_quad(py, m.as_any(), x0, y0, x1, y1, [c[0], c[1], c[2], if c.len() > 3 { c[3] } else { 1.0 }], z, true)?;
    v.call_method1("mesh_props", (mid, true, false, true, 0, 0))?;
    if let Some(t) = tex.filter(|t| !t.is_none()) {
        v.call_method1("mesh_tex", (mid, 0, t.getattr("id")?))?;
    }
    Ok(m.into_any())
}

/// 画面ひとつ（メニュー・インベントリなど）。build() で部品を作り、閉じるときは close()
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Screen {
    #[pyo3(get, set)]
    pub ui: PyObject,
    #[pyo3(get, set)]
    pub root: PyObject,
    #[pyo3(get, set)]
    pub layer: i64,
    #[pyo3(get, set)]
    pub widgets: Py<PyList>,
    #[pyo3(get, set)]
    pub fields: Py<PyList>,
    #[pyo3(get, set)]
    pub hover: PyObject,
}

impl Screen {
    pub fn blank(py: Python<'_>) -> Self {
        Screen { ui: py.None(), root: py.None(), layer: 0, widgets: PyList::empty_bound(py).unbind(), fields: PyList::empty_bound(py).unbind(), hover: py.None() }
    }
    pub fn of<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, Screen>> {
        Ok(o.downcast::<Screen>()?.clone())
    }
    pub fn init_base(slf: &Bound<'_, Screen>, ui: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let root = RsNode::create(py, None)?;
        let mut b = slf.borrow_mut();
        b.ui = ui.clone().unbind();
        b.root = root.into_any().unbind();
        b.layer = 0;
        b.widgets = PyList::empty_bound(py).unbind();
        b.fields = PyList::empty_bound(py).unbind();
        b.hover = py.None();
        Ok(())
    }
    pub fn bin(slf: &Bound<'_, Screen>, k: i64) -> i64 {
        20 + slf.borrow().layer * 10 + k
    }
    fn root<'py>(slf: &Bound<'py, Screen>) -> Bound<'py, PyAny> {
        slf.borrow().root.clone_ref(slf.py()).into_bound(slf.py())
    }
    pub fn add_widget(slf: &Bound<'_, Screen>, b: (f64, f64, f64, f64), click: PyObject, right: PyObject, mesh: PyObject, color: PyObject, info: PyObject) -> R<()> {
        let py = slf.py();
        let d = PyDict::new_bound(py);
        d.set_item("box", b)?;
        d.set_item("click", click)?;
        d.set_item("right", right)?;
        d.set_item("mesh", mesh)?;
        d.set_item("color", color)?;
        d.set_item("info", info)?;
        slf.borrow().widgets.bind(py).append(d)
    }
}

fn tup4(o: Option<&Bound<'_, PyAny>>, d: [f64; 4]) -> R<PyObject> {
    match o {
        Some(c) if !c.is_none() => Ok(c.clone().unbind()),
        _ => Ok(Python::with_gil(|py| rgba(d).into_py(py))),
    }
}

#[pymethods]
impl Screen {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        Screen::blank(py)
    }
    #[pyo3(signature = (ui, *_a, **_k))]
    fn __init__(slf: &Bound<'_, Self>, ui: &Bound<'_, PyAny>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<()> {
        Self::init_base(slf, ui)
    }
    // ---- 部品 ----
    fn clear(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let r = Self::root(slf);
        if !r.is_none() {
            r.call_method0("removeNode")?;
        }
        let root = RsNode::create(py, None)?;
        let mut b = slf.borrow_mut();
        b.root = root.into_any().unbind();
        b.widgets = PyList::empty_bound(py).unbind();
        b.fields = PyList::empty_bound(py).unbind();
        b.hover = py.None();
        Ok(())
    }
    fn _bin(slf: &Bound<'_, Self>, k: i64) -> i64 {
        Self::bin(slf, k)
    }
    #[pyo3(signature = (x0, y0, x1, y1, rgba=None))]
    fn panel<'py>(slf: &Bound<'py, Self>, x0: f64, y0: f64, x1: f64, y1: f64, rgba: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let c = tup4(rgba, [0.07, 0.08, 0.11, 0.96])?;
        let m = ui_rect_mesh(py, &Self::root(slf), x0, y0, x1, y1, c.bind(py), None, 0.0)?;
        view(py)?.call_method1("mesh_props", (m.getattr("mid")?, true, false, true, 0, Self::bin(slf, 0)))?;
        Ok(m)
    }
    /// 文字。anchor：(0,0) 左上・(0.5,0.5) 真ん中 など（文字の大きさに対する置き場所）
    #[pyo3(signature = (s, x, y, size=0.024, color=None, align=0, max_width=0.0, anchor=(0.0, 0.0)))]
    #[allow(clippy::too_many_arguments)]
    fn text<'py>(slf: &Bound<'py, Self>, s: &Bound<'py, PyAny>, x: f64, y: f64, size: f64, color: Option<&Bound<'py, PyAny>>, align: i64,
                 max_width: f64, anchor: (f64, f64)) -> R<(Bound<'py, PyAny>, (f64, f64))> {
        let py = slf.py();
        let t = text_new(py, &Self::root(slf), Self::bin(slf, 3) - 1)?;          // 文字は板・ボタンより後に描く
        let col = tup4(color, [1.0, 1.0, 1.0, 1.0])?;
        let (w, h): (f64, f64) = t.call_method1("set", (s, size, col, max_width, align))?.extract()?;
        t.call_method1("place", (x - w * anchor.0, y + h * anchor.1))?;
        Ok((t.into_any(), (w, h)))
    }
    #[pyo3(signature = (tex, x, y, size, alpha=1.0))]
    fn image<'py>(slf: &Bound<'py, Self>, tex: &Bound<'py, PyAny>, x: f64, y: f64, size: f64, alpha: f64) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let m = ui_rect_mesh(py, &Self::root(slf), x - size / 2.0, y - size / 2.0, x + size / 2.0, y + size / 2.0,
            PyTuple::new_bound(py, [1.0, 1.0, 1.0, alpha]).as_any(), Some(tex), 0.0)?;
        view(py)?.call_method1("mesh_props", (m.getattr("mid")?, true, false, true, 0, Self::bin(slf, 2)))?;
        Ok(m)
    }
    /// ボタン（真ん中が (x, y)）
    #[pyo3(signature = (label, x, y, w, h, on_click, color=None, size=0.022, on_right=None, info=None))]
    #[allow(clippy::too_many_arguments)]
    fn button<'py>(slf: &Bound<'py, Self>, label: &Bound<'py, PyAny>, x: f64, y: f64, w: f64, h: f64, on_click: PyObject,
                   color: Option<&Bound<'py, PyAny>>, size: f64, on_right: Option<PyObject>, info: Option<PyObject>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let col = tup4(color, [0.18, 0.2, 0.25, 1.0])?;
        let m = ui_rect_mesh(py, &Self::root(slf), x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0, col.bind(py), None, 0.0)?;
        view(py)?.call_method1("mesh_props", (m.getattr("mid")?, true, false, true, 0, Self::bin(slf, 1)))?;
        if label.is_truthy()? {
            slf.call_method("text", (label, x, y, size), Some(&[("anchor", (0.5, 0.5))].into_py_dict_bound(py)))?;
        }
        Self::add_widget(slf, (x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0), on_click, on_right.unwrap_or_else(|| py.None()), m.clone().unbind(),
            col, info.unwrap_or_else(|| py.None()))?;
        Ok(m)
    }
    /// 文字の入力欄（左端が x、真ん中の高さが y）。クリックで選ぶ
    #[pyo3(signature = (x, y, w, h, value=None, limit=0, on_submit=None, size=0.024))]
    #[allow(clippy::too_many_arguments)]
    fn field<'py>(slf: &Bound<'py, Self>, x: f64, y: f64, w: f64, h: f64, value: Option<&Bound<'py, PyAny>>, limit: i64, on_submit: Option<PyObject>,
                  size: f64) -> R<Bound<'py, Field>> {
        let py = slf.py();
        let v = match value { Some(v) => v.str()?.to_string(), None => String::new() };
        let f = Field::create(slf, x, y, w, h, v, limit, on_submit.unwrap_or_else(|| py.None()), size)?;
        let bg = f.borrow().bg.clone_ref(py);
        Self::add_widget(slf, (x, y - h / 2.0, x + w, y + h / 2.0), cb(slf.as_any(), "focus_field", (&f,))?.unbind(), py.None(), bg,
            (0.12, 0.13, 0.17, 1.0).into_py(py), py.None())?;
        slf.borrow().fields.bind(py).append(&f)?;
        Self::focus_field(slf, f.as_any())?;
        Ok(f)
    }
    fn focus_field(slf: &Bound<'_, Self>, f: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        for g in slf.borrow().fields.bind(py).iter() {
            g.setattr("focused", g.is(f))?;
            g.call_method0("render")?;
        }
        Ok(())
    }
    #[getter]
    fn focused(slf: &Bound<'_, Self>) -> R<PyObject> {
        let py = slf.py();
        for f in slf.borrow().fields.bind(py).iter() {
            if f.getattr("focused")?.is_truthy()? {
                return Ok(f.unbind());
            }
        }
        Ok(py.None())
    }
    // ---- 入力 ----
    fn hit(slf: &Bound<'_, Self>, x: f64, y: f64) -> R<PyObject> {
        let py = slf.py();
        let ws: Vec<Bound<'_, PyAny>> = slf.borrow().widgets.bind(py).iter().collect();
        for wd in ws.iter().rev() {
            let (x0, y0, x1, y1): (f64, f64, f64, f64) = wd.get_item("box")?.extract()?;
            if x0 <= x && x <= x1 && y0 <= y && y <= y1 {
                return Ok(wd.clone().unbind());
            }
        }
        Ok(py.None())
    }
    fn on_move(slf: &Bound<'_, Self>, x: f64, y: f64) -> R<()> {
        let py = slf.py();
        let h = slf.call_method1("hit", (x, y))?;
        let hov = slf.borrow().hover.clone_ref(py).into_bound(py);
        if !h.is(&hov) {
            let v = view(py)?;
            if !hov.is_none() {
                let np = hov.get_item("mesh")?.getattr("np")?;
                if np.getattr("alive")?.is_truthy()? {
                    v.call_method1("node_tint", (np.getattr("id")?, 1.0, 1.0, 1.0, 1.0))?;
                }
            }
            if !h.is_none() {
                v.call_method1("node_tint", (h.get_item("mesh")?.getattr("np")?.getattr("id")?, 1.45, 1.45, 1.45, 1.0))?;     // マウスが乗ったら明るく
            }
            slf.borrow_mut().hover = h.unbind();
        }
        Ok(())
    }
    fn on_click(slf: &Bound<'_, Self>, x: f64, y: f64, button: i64) -> R<PyObject> {
        let py = slf.py();
        let h = slf.call_method1("hit", (x, y))?;
        if h.is_none() {
            return Ok(slf.call_method1("on_empty_click", (button,))?.unbind());
        }
        let f = h.get_item(if button == 0 { "click" } else { "right" })?;
        if !f.is_none() {
            f.call0()?;
        }
        Ok(true.into_py(py))
    }
    fn on_empty_click(&self, button: i64) -> bool {
        let _ = button;
        false
    }
    /// キー。True なら使った
    fn on_key(&self, key: &Bound<'_, PyAny>) -> bool {
        let _ = key;
        false
    }
    fn on_text(&self, t: &Bound<'_, PyAny>) -> bool {
        let _ = t;
        false
    }
    fn update(&self, dt: f64) {
        let _ = dt;
    }
    fn build(&self) {}
    fn close(slf: &Bound<'_, Self>) -> R<()> {
        Self::root(slf).call_method0("removeNode")?;
        slf.borrow().ui.bind(slf.py()).call_method1("closed", (slf,))?;
        Ok(())
    }
}

/// 文字の入力欄：キーボード（日本語の変換も）で入力、Backspace で消す、Enter で決定
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Field {
    #[pyo3(get, set)]
    scr: PyObject,
    #[pyo3(get, set)]
    value: String,
    #[pyo3(get, set)]
    limit: i64,
    #[pyo3(get, set)]
    on_submit: PyObject,
    #[pyo3(get, set)]
    focused: bool,
    #[pyo3(get, set)]
    bg: PyObject,
    #[pyo3(get, set)]
    txt: PyObject,
    #[pyo3(get, set)]
    x: f64,
    #[pyo3(get, set)]
    y: f64,
    #[pyo3(get, set)]
    size: f64,
}

impl Field {
    #[allow(clippy::too_many_arguments)]
    fn create<'py>(scr: &Bound<'py, Screen>, x: f64, y: f64, w: f64, h: f64, value: String, limit: i64, on_submit: PyObject, size: f64) -> R<Bound<'py, Field>> {
        let py = scr.py();
        let root = Screen::root(scr);
        let bg = ui_rect_mesh(py, &root, x, y - h / 2.0, x + w, y + h / 2.0, PyTuple::new_bound(py, [0.12, 0.13, 0.17, 1.0]).as_any(), None, 0.0)?;
        view(py)?.call_method1("mesh_props", (bg.getattr("mid")?, true, false, true, 0, Screen::bin(scr, 1)))?;
        let txt = text_new(py, &root, Screen::bin(scr, 3) - 1)?;
        let f = Bound::new(py, Field { scr: scr.clone().into_any().unbind(), value, limit, on_submit, focused: false, bg: bg.unbind(),
            txt: txt.into_any().unbind(), x, y, size })?;
        Field::render(&f)?;
        Ok(f)
    }
}

#[pymethods]
impl Field {
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(scr: &Bound<'_, PyAny>, x: f64, y: f64, w: f64, h: f64, value: &Bound<'_, PyAny>, limit: i64, on_submit: PyObject, size: f64) -> R<Self> {
        let f = Self::create(&Screen::of(scr)?, x, y, w, h, value.str()?.to_string(), limit, on_submit, size)?;
        let b = f.borrow();
        let py = scr.py();
        Ok(Field { scr: b.scr.clone_ref(py), value: b.value.clone(), limit: b.limit, on_submit: b.on_submit.clone_ref(py), focused: false,
            bg: b.bg.clone_ref(py), txt: b.txt.clone_ref(py), x: b.x, y: b.y, size: b.size })
    }
    fn render(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let (s, txt, size, x, y) = {
            let b = slf.borrow();
            (format!("{}{}", b.value, if b.focused { "｜" } else { "" }), b.txt.clone_ref(py).into_bound(py), b.size, b.x, b.y)
        };
        let (_w, h): (f64, f64) = txt.call_method1("set", (s, size))?.extract()?;
        txt.call_method1("place", (x + 0.01, y + h / 2.0))?;
        Ok(())
    }
    fn set(slf: &Bound<'_, Self>, s: &str) -> R<()> {
        {
            let mut b = slf.borrow_mut();
            b.value = if b.limit > 0 { s.chars().take(b.limit as usize).collect() } else { s.to_string() };
        }
        Self::render(slf)
    }
    #[pyo3(name = "type")]
    fn type_(slf: &Bound<'_, Self>, t: &str) -> R<()> {
        let v = format!("{}{}", slf.borrow().value, t);
        Self::set(slf, &v)
    }
    fn key(slf: &Bound<'_, Self>, k: &str) -> R<bool> {
        let py = slf.py();
        if k == "backspace" {
            let mut v = slf.borrow().value.clone();
            v.pop();
            Self::set(slf, &v)?;
            return Ok(true);
        }
        let sub = slf.borrow().on_submit.clone_ref(py);
        if k == "enter" && !sub.is_none(py) {
            sub.call0(py)?;
            return Ok(true);
        }
        Ok(false)
    }
}

/// 画面の重なり（いちばん上が入力を受ける）とマウスの位置
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct UI {
    #[pyo3(get, set)]
    view: PyObject,
    #[pyo3(get, set)]
    stack: Py<PyList>,
    #[pyo3(get, set)]
    mouse: (f64, f64),
}

#[pymethods]
impl UI {
    #[new]
    fn new(py: Python<'_>, view: PyObject) -> Self {
        UI { view, stack: PyList::empty_bound(py).unbind(), mouse: (0.0, 0.0) }
    }
    #[getter]
    fn top(&self, py: Python<'_>) -> R<PyObject> {
        let s = self.stack.bind(py);
        Ok(if s.is_empty() { py.None() } else { s.get_item(s.len() - 1)?.unbind() })
    }
    #[pyo3(signature = (scr_cls, *a, **k))]
    fn open<'py>(slf: &Bound<'py, Self>, scr_cls: &Bound<'py, PyAny>, a: &Bound<'py, PyTuple>, k: Option<&Bound<'py, PyDict>>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let mut v: Vec<PyObject> = vec![slf.clone().into_any().unbind()];
        v.extend(a.iter().map(|x| x.unbind()));
        let s = scr_cls.call(PyTuple::new_bound(py, v), k)?;
        let stack = slf.borrow().stack.clone_ref(py);
        s.setattr("layer", stack.bind(py).len())?;
        stack.bind(py).append(&s)?;
        s.call_method0("build")?;
        Ok(s)
    }
    fn closed(&self, py: Python<'_>, s: &Bound<'_, PyAny>) -> R<()> {
        let st = self.stack.bind(py);
        if st.contains(s)? {
            st.call_method1("remove", (s,))?;
        }
        Ok(())
    }
    fn to_ui(&self, py: Python<'_>, px: f64, py_: f64) -> R<(f64, f64)> {
        let (w, h): (f64, f64) = self.view.bind(py).call_method0("size")?.extract()?;
        Ok(((px - w / 2.0) / h, (h / 2.0 - py_) / h))
    }
    /// 入力を渡す。画面が開いていて使ったら True
    fn event(slf: &Bound<'_, Self>, kind: &str, a: &Bound<'_, PyAny>, b: &Bound<'_, PyAny>, n: &Bound<'_, PyAny>) -> R<bool> {
        let py = slf.py();
        let s = slf.borrow().top(py)?.into_bound(py);
        if kind == "cursor" {
            let m = slf.borrow().to_ui(py, a.extract()?, b.extract()?)?;
            slf.borrow_mut().mouse = m;
            if !s.is_none() {
                s.call_method1("on_move", m)?;
            }
            return Ok(!s.is_none());
        }
        if s.is_none() {
            return Ok(false);
        }
        let m = slf.borrow().mouse;
        if kind == "button" {
            if a.is_truthy()? {
                s.call_method1("on_click", (m.0, m.1, n))?;
            }
            return Ok(true);
        }
        if let Some(k) = kind.strip_prefix("key:") {
            if a.is_truthy()? {
                let f = s.getattr("focused")?;
                let used = !f.is_none() && (f.call_method1("key", (k,))?.is_truthy()? || k.chars().count() == 1 || k == "space");
                if !used {
                    s.call_method1("on_key", (k,))?;
                }
            }
            return Ok(true);
        }
        if let Some(t) = kind.strip_prefix("text:") {
            let f = s.getattr("focused")?;
            if !f.is_none() {
                f.call_method1("type", (t,))?;
            } else {
                s.call_method1("on_text", (t,))?;
            }
            return Ok(true);
        }
        Ok(kind == "wheel" || kind == "mouse")
    }
}
