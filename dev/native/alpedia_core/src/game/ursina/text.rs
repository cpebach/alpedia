//! ursina のまね：文字（Text）

use super::*;

// ---------------------------------------------------------------------------
// 文字
// ---------------------------------------------------------------------------
#[pyclass(module = "alpedia_core", extends = Entity, subclass)]
pub struct Text {
    #[pyo3(get, set)]
    pub(crate) _text: String,
    #[pyo3(get, set)]
    pub(crate) _txt: PyObject,
    #[pyo3(get, set)]
    pub(crate) _wordwrap: i64,
    #[pyo3(get, set)]
    pub(crate) _wh: (f64, f64),
}


impl Text {
    pub(crate) fn wrap(&self, s: &str) -> String {
        if self._wordwrap <= 0 {
            return s.to_string();
        }
        let n = self._wordwrap as usize;
        let mut out: Vec<String> = vec![];
        for line in s.split('\n') {
            let mut ch: Vec<char> = line.chars().collect();
            while ch.len() > n {
                out.push(ch[..n].iter().collect());
                ch = ch[n..].to_vec();
            }
            out.push(ch.iter().collect());
        }
        out.join("\n")
    }
    pub(crate) fn render(slf: &Bound<'_, Text>) -> R<()> {
        let py = slf.py();
        let e = sup(slf)?;
        if !e.borrow()._alive || !Entity::is_ui_(e)? {
            return Ok(());
        }
        if slf.borrow()._txt.is_none(py) {
            let node = Entity::node_of(e)?;
            let k = PyDict::new_bound(py);
            k.set_item("overlay", true)?;
            let t = py.get_type_bound::<RsText>().call((node,), Some(&k))?;
            slf.borrow_mut()._txt = t.unbind();
        }
        let s = { let b = slf.borrow(); b.wrap(&b._text) };
        let ox = e.borrow()._origin.0;
        let al = if ox <= -0.25 { 0 } else if ox >= 0.25 { 2 } else { 1 }; // ursina と同じく、行ごとに origin の x でそろえる
        let txt = slf.borrow()._txt.clone_ref(py).into_bound(py);
        let wh: (f64, f64) = if !s.is_empty() {
            let c = py.import_bound("builtins")?.getattr("tuple")?.call1((e.borrow()._color.bind(py),))?;
            txt.call_method1("set", (&s, TEXT_SIZE * FONT_K, c, 0.0, al))?.extract()?
        } else {
            txt.call_method1("set", ("", TEXT_SIZE * FONT_K))?;
            (0.0, 0.0)
        };
        slf.borrow_mut()._wh = wh;
        Self::place(slf)?;
        Entity::touch(e)
    }
    pub(crate) fn place(slf: &Bound<'_, Text>) -> R<()> {
        let py = slf.py();
        let txt = slf.borrow()._txt.clone_ref(py).into_bound(py);
        if txt.is_none() {
            return Ok(());
        }
        let (w, h) = slf.borrow()._wh;
        let (ox, oy) = sup(slf)?.borrow()._origin;
        txt.getattr("node")?.call_method1("setPos", (-(ox + 0.5) * w, -(oy - 0.5) * h, 0.0))?;
        Ok(())
    }
}

#[pymethods]
impl Text {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const size: f64 = TEXT_SIZE;
    #[classattr]
    pub(crate) fn default_font(py: Python<'_>) -> PyObject {
        py.None()
    }
    #[classattr]
    pub(crate) fn default_monospace_font(py: Python<'_>) -> PyObject {
        py.None()
    }
    #[classattr]
    #[allow(non_upper_case_globals)]
    const default_resolution: i64 = 1080;

    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<(Self, Entity)> {
        Ok((Text { _text: String::new(), _txt: py.None(), _wordwrap: 0, _wh: (0.0, 0.0) }, Entity::blank(py, "Text")?))
    }
    #[pyo3(signature = (text=None, parent=None, origin=None, wordwrap=None, **kw))]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, text: Option<&B<'_>>, parent: Option<&B<'_>>, origin: Option<&B<'_>>, wordwrap: Option<&B<'_>>,
                kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let py = slf.py();
        let ww: i64 = match wordwrap.filter(|w| w.is_truthy().unwrap_or(false)) {
            Some(w) => py.import_bound("builtins")?.getattr("int")?.call1((w,))?.extract()?,
            None => 0,
        };
        {
            let mut b = slf.borrow_mut();
            b._text = String::new();
            b._txt = py.None();
            b._wordwrap = ww;
            b._wh = (0.0, 0.0);
        }
        let kw = match kw { Some(k) => k.copy()?, None => PyDict::new_bound(py) };
        for k in ["start_tag", "end_tag", "font", "resolution", "background", "line_height"] {
            if kw.contains(k)? {
                kw.del_item(k)?;
            }
        }
        let col = match kw.get_item("color")? { Some(c) => { kw.del_item("color")?; c } None => color(py, 1.0, 1.0, 1.0, 1.0)? };
        let parent = match parent.filter(|p| !p.is_none()) { Some(p) => p.clone(), None => ui_root(py)? };
        let origin = match origin { Some(o) => o.clone(), None => tb(py, (-0.5, 0.5)) };
        kw.set_item("parent", parent)?;
        kw.set_item("origin", origin)?;
        // Entity.__init__(**kw)
        let g = |k: &str| -> R<Option<B<'_>>> {
            let v = kw.get_item(k)?;
            if v.is_some() {
                kw.del_item(k)?;
            }
            Ok(v)
        };
        let (pa, mo, te, co, po, sc, or, ro, en, z, x, y) = (g("parent")?, g("model")?, g("texture")?, g("color")?, g("position")?, g("scale")?,
            g("origin")?, g("rotation")?, g("enabled")?, g("z")?, g("x")?, g("y")?);
        let _ = g("eternal")?;
        Entity::init(slf.as_any(), pa.as_ref(), mo.as_ref(), te.as_ref(), co.as_ref(), po.as_ref(), sc.as_ref(), or.as_ref(), ro.as_ref(), en.as_ref(),
            z.as_ref(), x.as_ref(), y.as_ref(), Some(&kw))?;
        sup(slf)?.borrow_mut()._color = color_of(py, &col)?.unbind();
        let t: PyObject = match text { Some(t) => t.clone().unbind(), None => "".into_py(py) };
        slf.as_any().setattr("text", t)
    }
    pub(crate) fn _wrap(&self, s: &str) -> String {
        self.wrap(s)
    }
    #[getter]
    pub(crate) fn text(&self) -> String {
        self._text.clone()
    }
    #[setter]
    pub(crate) fn set_text(slf: &Bound<'_, Self>, s: &B<'_>) -> R<()> {
        let s = if s.is_none() { String::new() } else { s.str()?.to_string() };
        slf.borrow_mut()._text = s;
        slf.as_any().call_method0("_render")?;
        Ok(())
    }
    #[getter]
    pub(crate) fn wordwrap(&self) -> i64 {
        self._wordwrap
    }
    #[setter]
    pub(crate) fn set_wordwrap(slf: &Bound<'_, Self>, n: &B<'_>) -> R<()> {
        let py = slf.py();
        let v: i64 = if n.is_truthy()? { py.import_bound("builtins")?.getattr("int")?.call1((n,))?.extract()? } else { 0 };
        slf.borrow_mut()._wordwrap = v;
        slf.as_any().call_method0("_render")?;
        Ok(())
    }
    pub(crate) fn _render(slf: &Bound<'_, Self>) -> R<()> {
        Self::render(slf)
    }
    pub(crate) fn _place(slf: &Bound<'_, Self>) -> R<()> {
        Self::place(slf)
    }
    #[getter]
    pub(crate) fn color(slf: &Bound<'_, Self>) -> R<PyObject> {
        Ok(sup(slf)?.borrow()._color.clone_ref(slf.py()))
    }
    #[setter]
    pub(crate) fn set_color(slf: &Bound<'_, Self>, c: &B<'_>) -> R<()> {
        let py = slf.py();
        let c = py.import_bound("builtins")?.getattr("tuple")?.call1((c,))?;
        sup(slf)?.borrow_mut()._color = color_of(py, &c)?.unbind();
        let t = slf.borrow()._txt.clone_ref(py).into_bound(py);
        if !t.is_none() {
            t.setattr("key", py.None())?;
            slf.as_any().call_method0("_render")?;
        }
        Ok(())
    }
    #[getter]
    pub(crate) fn origin<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let o = sup(slf)?.borrow()._origin;
        vec2(slf.py(), o.0, o.1)
    }
    #[setter]
    pub(crate) fn set_origin(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let py = slf.py();
        Entity::set_origin_base(sup(slf)?, v)?;
        if !slf.borrow()._txt.is_none(py) {
            slf.as_any().call_method0("_render")?;
        }
        Entity::touch(sup(slf)?)
    }
    #[getter]
    pub(crate) fn width(&self) -> f64 {
        self._wh.0
    }
    #[getter]
    pub(crate) fn height(&self) -> f64 {
        self._wh.1
    }
    pub(crate) fn _sort(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let t = slf.borrow()._txt.clone_ref(py).into_bound(py);
        let e = sup(slf)?;
        if !t.is_none() && e.borrow()._alive {
            let b = BIN0 + Entity::order(e)?.div_euclid(100000) * 1000 + e.borrow()._seq.rem_euclid(1000) + 1;
            view(py)?.call_method1("mesh_props", (t.getattr("text")?.getattr("mid")?, true, false, true, 0, b))?;
        }
        Ok(())
    }
    pub(crate) fn _world_box(slf: &Bound<'_, Self>) -> R<(f64, f64, f64, f64)> {
        let e = sup(slf)?;
        let (wx, wy) = Entity::world_scale_(e)?;
        let (cx, cy) = Entity::world_xy(e)?;
        let (w0, h0) = slf.borrow()._wh;
        let (w, h) = (w0 * wx, h0 * wy);
        let (ox, oy) = e.borrow()._origin;
        let x0 = cx - (ox + 0.5) * w;
        let y1 = cy - (oy - 0.5) * h;
        Ok((x0, y1 - h, x0 + w, y1))
    }
}
