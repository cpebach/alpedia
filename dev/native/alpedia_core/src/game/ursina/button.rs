//! ursina のまね：ボタン・入力欄

use super::*;

// ---------------------------------------------------------------------------
// ボタン・入力欄
// ---------------------------------------------------------------------------
#[pyclass(module = "alpedia_core", extends = Entity, subclass)]
pub struct Button {
    #[pyo3(get, set)]
    pub(crate) _base: PyObject,
    #[pyo3(get, set)]
    pub(crate) highlight_color: PyObject,
    #[pyo3(get, set)]
    pub(crate) pressed_color: PyObject,
    #[pyo3(get, set)]
    pub(crate) text_entity: PyObject,
    #[pyo3(get, set)]
    pub(crate) _text_size: f64,
    #[pyo3(get, set)]
    pub(crate) on_click: PyObject,
    #[pyo3(get, set)]
    pub(crate) _text_color: PyObject,
}

impl Button {
    pub(crate) fn blank(py: Python<'_>) -> Self {
        Button { _base: py.None(), highlight_color: py.None(), pressed_color: py.None(), text_entity: py.None(), _text_size: 1.0, on_click: py.None(), _text_color: py.None() }
    }
    pub(crate) fn highlight(py: Python<'_>, base: &B<'_>) -> R<PyObject> {
        let v: Vec<f64> = base.iter()?.map(|x| f64of(&x?)).collect::<R<_>>()?;
        let f = |x: f64| 1f64.min(x * 1.25 + 0.05);
        Ok(cls(py, &COLOR)?.call1((f(v[0]), f(v[1]), f(v[2]), base.get_item(3)?))?.unbind())
    }
    pub(crate) fn fit_text(slf: &Bound<'_, Button>) -> R<()> {
        let py = slf.py();
        let te = slf.borrow().text_entity.clone_ref(py).into_bound(py);
        if !te.is_none() {
            let (wx, wy) = Entity::world_scale_(sup(slf)?)?;
            let ts = slf.borrow()._text_size;
            te.setattr("scale", (ts / wx.max(1e-6), ts / wy.max(1e-6), 1))?;
        }
        Ok(())
    }
    /// Button.__init__ の中身（InputField からも）
    pub(crate) fn init(s: &B<'_>, text: Option<&B<'_>>, parent: Option<&B<'_>>, model: Option<&B<'_>>, radius: &B<'_>, color_: Option<&B<'_>>,
            highlight_color: Option<&B<'_>>, pressed_color: Option<&B<'_>>, text_size: &B<'_>, text_color: Option<&B<'_>>, scale: Option<&B<'_>>,
            kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let py = s.py();
        let kw = match kw { Some(k) => k.copy()?, None => PyDict::new_bound(py) };
        for k in ["highlight_scale", "pressed_scale", "highlight_text_color", "icon", "disabled", "text_origin"] {
            if kw.contains(k)? {
                kw.del_item(k)?;
            }
        }
        let on_click = match kw.get_item("on_click")? { Some(c) => { kw.del_item("on_click")?; c.unbind() } None => py.None() };
        let quad = tb(py, "quad");
        let mut model = model.cloned().unwrap_or(quad);
        let qt = py.get_type_bound::<Quad>();
        if model.eq("quad")? && radius.is_truthy()? {
            model = qt.call((), Some(&[("radius", radius.clone()), ("aspect", 1.0f64.into_py(py).into_bound(py))].into_py_dict_bound(py)))?;
        }
        let b = s.downcast::<Button>()?;
        b.borrow_mut()._base = py.None();
        let parent = match parent.filter(|p| !p.is_none()) { Some(p) => p.clone(), None => ui_root(py)? };
        let scale = match scale.filter(|p| !p.is_none()) { Some(p) => p.clone(), None => tb(py, (0.25, 0.075)) };
        let g = |k: &str| -> R<Option<B<'_>>> {
            let v = kw.get_item(k)?;
            if v.is_some() {
                kw.del_item(k)?;
            }
            Ok(v)
        };
        let (te, po, or, ro, en, z, x, y) = (g("texture")?, g("position")?, g("origin")?, g("rotation")?, g("enabled")?, g("z")?, g("x")?, g("y")?);
        let _ = g("eternal")?;
        let o00 = tb(py, (0, 0));
        Entity::init(s, Some(&parent), Some(&model), te.as_ref(), None, po.as_ref(), Some(&scale), Some(or.as_ref().unwrap_or(&o00)), ro.as_ref(),
            en.as_ref(), z.as_ref(), x.as_ref(), y.as_ref(), Some(&kw))?;
        let e = sup(b)?;
        let cur = e.borrow()._model.clone_ref(py).into_bound(py);
        if cur.is_instance(&qt)? {
            let s_ = e.borrow().scl;
            s.setattr("model", qt.call((), Some(&[("radius", radius.clone()), ("aspect", tb(py, s_[0] / s_[1].max(1e-6)))].into_py_dict_bound(py)))?)?;
        }
        let base = match color_.filter(|c| !c.is_none()) { Some(c) => c.clone(), None => color(py, 0.18, 0.18, 0.2, 1.0)? };
        let tup = py.import_bound("builtins")?.getattr("tuple")?;
        let base = color_of(py, &tup.call1((base,))?)?;
        b.borrow_mut()._base = base.clone().unbind();
        let hc = match highlight_color.filter(|c| !c.is_none()) { Some(c) => c.clone().unbind(), None => Self::highlight(py, &base)? };
        s.setattr("highlight_color", &hc)?;
        let pc = match pressed_color.filter(|c| !c.is_none()) { Some(c) => c.clone().unbind(), None => s.getattr("highlight_color")?.unbind() };
        s.setattr("pressed_color", pc)?;
        Entity::set_color_base(e, &base)?;
        {
            let mut bb = b.borrow_mut();
            bb.text_entity = py.None();
            bb._text_size = f64of(text_size)?;
            bb.on_click = on_click;
            bb._text_color = text_color.map(|c| c.clone().unbind()).unwrap_or_else(|| py.None());
        }
        if let Some(t) = text.filter(|t| t.is_truthy().unwrap_or(false)) {
            s.setattr("text", t)?;
        }
        Ok(())
    }
}

#[pymethods]
impl Button {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<PyClassInitializer<Self>> {
        Ok(PyClassInitializer::from(Entity::blank(py, "Button")?).add_subclass(Button::blank(py)))
    }
    #[pyo3(signature = (text=None, parent=None, model=None, radius=None, color=None, highlight_color=None, pressed_color=None, text_size=None,
                        text_color=None, text_origin=None, scale=None, **kw))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, text: Option<&B<'_>>, parent: Option<&B<'_>>, model: Option<&B<'_>>, radius: Option<&B<'_>>, color: Option<&B<'_>>,
                highlight_color: Option<&B<'_>>, pressed_color: Option<&B<'_>>, text_size: Option<&B<'_>>, text_color: Option<&B<'_>>,
                text_origin: Option<&B<'_>>, scale: Option<&B<'_>>, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let _ = text_origin;
        let py = slf.py();
        let r01 = 0.1f64.into_py(py).into_bound(py);
        let one = 1i64.into_py(py).into_bound(py);
        Button::init(slf.as_any(), text, parent, model, radius.unwrap_or(&r01), color, highlight_color, pressed_color, text_size.unwrap_or(&one),
            text_color, scale, kw)
    }
    #[getter]
    pub(crate) fn color(slf: &Bound<'_, Self>) -> R<PyObject> {
        let py = slf.py();
        let b = slf.borrow()._base.clone_ref(py);
        Ok(if b.is_none(py) { sup(slf)?.borrow()._color.clone_ref(py) } else { b })
    }
    #[setter]
    pub(crate) fn set_color(slf: &Bound<'_, Self>, c: &B<'_>) -> R<()> {
        let py = slf.py();
        if slf.borrow()._base.is_none(py) {
            return Entity::set_color_base(sup(slf)?, c);
        }
        let tup = py.import_bound("builtins")?.getattr("tuple")?;
        let base = color_of(py, &tup.call1((c,))?)?;
        slf.borrow_mut()._base = base.clone().unbind();
        let hc = Self::highlight(py, &base)?;
        slf.as_any().setattr("highlight_color", hc)?;
        Entity::set_color_base(sup(slf)?, &base)
    }
    #[getter]
    pub(crate) fn text(slf: &Bound<'_, Self>) -> R<PyObject> {
        let py = slf.py();
        let te = slf.borrow().text_entity.clone_ref(py).into_bound(py);
        Ok(if te.is_none() { "".into_py(py) } else { te.getattr("text")?.unbind() })
    }
    #[setter]
    pub(crate) fn set_text(slf: &Bound<'_, Self>, s: &B<'_>) -> R<()> {
        let py = slf.py();
        if slf.borrow().text_entity.is_none(py) {
            let tc = slf.borrow()._text_color.clone_ref(py);
            let k = PyDict::new_bound(py);
            k.set_item("parent", slf)?;
            k.set_item("text", "")?;
            k.set_item("origin", (0, 0))?;
            k.set_item("z", -0.001)?;
            k.set_item("color", if tc.is_none(py) { color(py, 1.0, 1.0, 1.0, 1.0)?.unbind() } else { tc })?;
            let t = py.get_type_bound::<Text>().call((), Some(&k))?;
            slf.borrow_mut().text_entity = t.unbind();
            slf.as_any().call_method0("_fit_text")?;
        }
        slf.borrow().text_entity.bind(py).setattr("text", s)
    }
    #[getter]
    pub(crate) fn text_size(&self) -> f64 {
        self._text_size
    }
    #[setter]
    pub(crate) fn set_text_size(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        slf.borrow_mut()._text_size = f64of(v)?;
        slf.as_any().call_method0("_fit_text")?;
        Ok(())
    }
    pub(crate) fn _fit_text(slf: &Bound<'_, Self>) -> R<()> {
        Self::fit_text(slf)
    }
    #[getter]
    pub(crate) fn text_color(slf: &Bound<'_, Self>) -> R<PyObject> {
        let py = slf.py();
        let te = slf.borrow().text_entity.clone_ref(py).into_bound(py);
        Ok(if te.is_none() { color(py, 1.0, 1.0, 1.0, 1.0)?.unbind() } else { te.getattr("color")?.unbind() })
    }
    #[setter]
    pub(crate) fn set_text_color(slf: &Bound<'_, Self>, c: &B<'_>) -> R<()> {
        let py = slf.py();
        slf.borrow_mut()._text_color = c.clone().unbind();
        let te = slf.borrow().text_entity.clone_ref(py).into_bound(py);
        if !te.is_none() {
            te.setattr("color", c)?;
        }
        Ok(())
    }
    pub(crate) fn _hover(slf: &Bound<'_, Self>, on: bool) -> R<()> {
        let py = slf.py();
        let c = if on { slf.getattr("highlight_color")? } else { slf.borrow()._base.clone_ref(py).into_bound(py) };
        Entity::set_color_base(sup(slf)?, &c)
    }
}

/// InputField.text_field のまね（text・render・cursor）
#[pyclass(module = "alpedia_core", dict)]
pub struct TextField {
    #[pyo3(get, set)]
    pub(crate) owner: PyObject,
    #[pyo3(get, set)]
    pub(crate) text: PyObject,
    #[pyo3(get, set)]
    pub(crate) cursor: PyObject,
}

#[pyclass(module = "alpedia_core", dict)]
pub struct Cursor {
    #[pyo3(get, set)]
    pub(crate) x: PyObject,
    #[pyo3(get, set)]
    pub(crate) y: PyObject,
}

#[pymethods]
impl TextField {
    pub(crate) fn render(&self, py: Python<'_>) -> R<()> {
        self.owner.bind(py).call_method0("_show")?;
        Ok(())
    }
}

#[pyclass(module = "alpedia_core", extends = Button, subclass)]
pub struct InputField {
    #[pyo3(get, set)]
    pub(crate) text_field: PyObject,
    #[pyo3(get, set)]
    pub(crate) character_limit: PyObject,
    #[pyo3(get, set)]
    pub(crate) limit_content_to: PyObject,
    #[pyo3(get, set)]
    pub(crate) submit_on: PyObject,
    #[pyo3(get, set)]
    pub(crate) on_submit: PyObject,
    #[pyo3(get, set)]
    pub(crate) on_value_changed: PyObject,
    #[pyo3(get, set)]
    pub(crate) _shown: PyObject,
    #[pyo3(get, set)]
    pub(crate) _active: bool,
}

#[pymethods]
impl InputField {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<PyClassInitializer<Self>> {
        Ok(PyClassInitializer::from(Entity::blank(py, "InputField")?).add_subclass(Button::blank(py)).add_subclass(InputField {
            text_field: py.None(), character_limit: 24.into_py(py), limit_content_to: py.None(), submit_on: PyList::empty_bound(py).into_any().unbind(),
            on_submit: py.None(), on_value_changed: py.None(), _shown: py.None(), _active: false,
        }))
    }
    #[pyo3(signature = (default_value=None, label=None, max_lines=1, character_limit=None, text=None, active=None, **kw))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, default_value: Option<&B<'_>>, label: Option<&B<'_>>, max_lines: i64, character_limit: Option<PyObject>,
                text: Option<&B<'_>>, active: Option<&B<'_>>, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let _ = label;
        let py = slf.py();
        let kw = match kw { Some(k) => k.copy()?, None => PyDict::new_bound(py) };
        if !kw.contains("scale")? {
            kw.set_item("scale", (0.5, TEXT_SIZE * 2.0 * max_lines as f64))?;
        }
        if !kw.contains("color")? {
            kw.set_item("color", color(py, 0.1, 0.1, 0.12, 1.0)?)?;
        }
        // Button.__init__(**kw)
        let g = |k: &str| -> R<Option<B<'_>>> {
            let v = kw.get_item(k)?;
            if v.is_some() {
                kw.del_item(k)?;
            }
            Ok(v)
        };
        let (te, pa, mo, ra, co, hc, pc, ts, tc, _to, sc) = (g("text")?, g("parent")?, g("model")?, g("radius")?, g("color")?, g("highlight_color")?,
            g("pressed_color")?, g("text_size")?, g("text_color")?, g("text_origin")?, g("scale")?);
        let r01 = 0.1f64.into_py(py).into_bound(py);
        let one = 1i64.into_py(py).into_bound(py);
        let _ = te;
        Button::init(slf.as_any(), None, pa.as_ref(), mo.as_ref(), ra.as_ref().unwrap_or(&r01), co.as_ref(), hc.as_ref(), pc.as_ref(),
            ts.as_ref().unwrap_or(&one), tc.as_ref(), sc.as_ref(), Some(&kw))?;
        let cur = Py::new(py, Cursor { x: 0.into_py(py), y: 0.into_py(py) })?;
        fn pick<'p>(o: Option<&B<'p>>) -> Option<B<'p>> { o.filter(|v| v.is_truthy().unwrap_or(false)).cloned() }
        let init_text = pick(text).or_else(|| pick(default_value));
        let t = match init_text { Some(t) => t.str()?.into_any().unbind(), None => "".into_py(py) };
        let tf = Py::new(py, TextField { owner: slf.clone().into_any().unbind(), text: t, cursor: cur.into_any() })?;
        {
            let mut b = slf.borrow_mut();
            b.text_field = tf.into_any();
            b.character_limit = character_limit.unwrap_or_else(|| 24.into_py(py));
            b.limit_content_to = py.None();
            b.submit_on = PyList::empty_bound(py).into_any().unbind();
            b.on_submit = py.None();
            b.on_value_changed = py.None();
        }
        let k = PyDict::new_bound(py);
        k.set_item("parent", slf)?;
        k.set_item("text", "")?;
        k.set_item("origin", (-0.5, 0))?;
        k.set_item("x", -0.48)?;
        k.set_item("z", -0.001)?;
        let shown = py.get_type_bound::<Text>().call((), Some(&k))?;
        slf.borrow_mut()._shown = shown.clone().unbind();
        let (wx, wy) = Entity::world_scale_(sup(slf)?)?;
        shown.setattr("scale", (1.0 / wx.max(1e-6), 1.0 / wy.max(1e-6), 1))?;
        slf.as_any().setattr("on_click", slf.getattr("_focus")?)?;
        slf.borrow_mut()._active = false;
        slf.as_any().setattr("active", active.map(|a| a.is_truthy()).transpose()?.unwrap_or(false))
    }
    #[getter]
    pub(crate) fn active(&self) -> bool {
        self._active
    }
    #[setter]
    pub(crate) fn set_active(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let py = slf.py();
        let on = on.is_truthy()?;
        slf.borrow_mut()._active = on;
        if on {
            set_focus(py, slf.as_any())?;
        } else if focus(py)?.is(slf) {
            set_focus(py, &py.None().into_bound(py))?;
        }
        slf.as_any().call_method0("_show")?;
        Ok(())
    }
    pub(crate) fn _focus(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().setattr("active", true)
    }
    #[getter]
    pub(crate) fn value(&self, py: Python<'_>) -> R<PyObject> {
        Ok(self.text_field.bind(py).getattr("text")?.unbind())
    }
    pub(crate) fn _show(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let sh = slf.borrow()._shown.clone_ref(py).into_bound(py);
        if !sh.is_none() {
            let t = slf.borrow().text_field.bind(py).getattr("text")?.str()?.to_string();
            sh.setattr("text", format!("{}{}", t, if slf.borrow()._active { "｜" } else { "" }))?;
        }
        Ok(())
    }
    #[pyo3(name = "type")]
    pub(crate) fn type_(slf: &Bound<'_, Self>, t: &str) -> R<()> {
        let py = slf.py();
        let lim = slf.borrow().limit_content_to.clone_ref(py).into_bound(py);
        let t: String = if lim.is_truthy()? {
            let mut o = String::new();
            for ch in t.chars() {
                if lim.contains(ch.to_string())? {
                    o.push(ch);
                }
            }
            o
        } else {
            t.to_string()
        };
        let tf = slf.borrow().text_field.clone_ref(py).into_bound(py);
        let mut s = format!("{}{}", tf.getattr("text")?.str()?, t);
        let cl = slf.borrow().character_limit.clone_ref(py).into_bound(py);
        if cl.is_truthy()? {
            let n: i64 = cl.extract()?;
            let ch: Vec<char> = s.chars().collect();
            let n = if n < 0 { (ch.len() as i64 + n).max(0) as usize } else { (n as usize).min(ch.len()) };
            s = ch[..n].iter().collect();
        }
        tf.setattr("text", s)?;
        slf.as_any().call_method0("_show")?;
        let ov = slf.borrow().on_value_changed.clone_ref(py).into_bound(py);
        if ov.is_truthy()? {
            ov.call0()?;
        }
        Ok(())
    }
    pub(crate) fn key(slf: &Bound<'_, Self>, k: &str) -> R<bool> {
        let py = slf.py();
        if k == "backspace" {
            let tf = slf.borrow().text_field.clone_ref(py).into_bound(py);
            let mut s = tf.getattr("text")?.str()?.to_string();
            s.pop();
            tf.setattr("text", s)?;
            slf.as_any().call_method0("_show")?;
            return Ok(true);
        }
        let os = slf.borrow().on_submit.clone_ref(py).into_bound(py);
        if (k == "enter" || k == "return") && os.is_truthy()? {
            os.call0()?;
            return Ok(true);
        }
        Ok(k == "space" || k.chars().count() == 1)
    }
}
