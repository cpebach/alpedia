//! ursina のまね：部品（Entity）

use super::*;

// ---------------------------------------------------------------------------
// 部品
// ---------------------------------------------------------------------------

/// ursina の Entity（画面の上の 2D の部品）
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Entity {
    #[pyo3(get, set)]
    pub(crate) _parent: PyObject,
    #[pyo3(get, set)]
    pub(crate) children: Py<PyList>,
    #[pyo3(get, set)]
    pub(crate) _seq: i64,
    #[pyo3(get, set)]
    pub(crate) _node: PyObject,
    #[pyo3(get, set)]
    pub(crate) _mesh: PyObject,
    #[pyo3(get, set)]
    pub(crate) _model: PyObject,
    #[pyo3(get, set)]
    pub(crate) _texture: PyObject,
    #[pyo3(get, set)]
    pub(crate) _color: PyObject,
    pub(crate) pos: [f64; 3],
    pub(crate) scl: [f64; 3],
    #[pyo3(get, set)]
    pub(crate) _origin: (f64, f64),
    #[pyo3(get, set)]
    pub(crate) _rot: f64,
    #[pyo3(get, set)]
    pub(crate) _enabled: bool,
    #[pyo3(get, set)]
    pub(crate) _alpha: f64,
    #[pyo3(get, set)]
    pub(crate) name: PyObject,
    #[pyo3(get, set)]
    pub(crate) ignore_input: PyObject,
    #[pyo3(get, set)]
    pub(crate) collider: PyObject,
    #[pyo3(get, set)]
    pub(crate) _alive: bool,
    #[pyo3(get, set)]
    pub(crate) _model_dirty: bool,
}

pub(crate) type B<'py> = Bound<'py, PyAny>;

pub(crate) fn sup<'a, 'py, T>(s: &'a Bound<'py, T>) -> R<&'a Bound<'py, Entity>> {
    Ok(s.as_any().downcast::<Entity>()?)
}

pub(crate) fn ent<'a, 'py>(o: &'a Bound<'py, PyAny>) -> R<&'a Bound<'py, Entity>> {
    Ok(o.downcast::<Entity>()?)
}

impl Entity {
    pub fn blank(py: Python<'_>, name: &str) -> R<Self> {
        Ok(Entity {
            _parent: py.None(), children: PyList::empty_bound(py).unbind(), _seq: 0, _node: py.None(), _mesh: py.None(), _model: py.None(),
            _texture: py.None(), _color: color(py, 1.0, 1.0, 1.0, 1.0)?.unbind(), pos: [0.0; 3], scl: [1.0; 3], _origin: (0.0, 0.0), _rot: 0.0,
            _enabled: true, _alpha: 1.0, name: name.into_py(py), ignore_input: false.into_py(py), collider: py.None(), _alive: true, _model_dirty: false,
        })
    }
    /// Entity.__init__（Python の子クラスの super().__init__ もここ）
    #[allow(clippy::too_many_arguments)]
    pub fn init(s: &B<'_>, parent: Option<&B<'_>>, model: Option<&B<'_>>, texture: Option<&B<'_>>, color_: Option<&B<'_>>, position: Option<&B<'_>>,
                scale: Option<&B<'_>>, origin: Option<&B<'_>>, rotation: Option<&B<'_>>, enabled: Option<&B<'_>>, z: Option<&B<'_>>, x: Option<&B<'_>>,
                y: Option<&B<'_>>, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let py = s.py();
        let nn = |o: Option<&B<'_>>| o.filter(|v| !v.is_none()).is_some();
        let kw = match kw { Some(k) => k.copy()?, None => PyDict::new_bound(py) };
        let tname = s.get_type().getattr("__name__")?;
        let name = match kw.get_item("name")? { Some(n) => { kw.del_item("name")?; n } None => tname };
        let collider = match kw.get_item("collider")? { Some(n) => { kw.del_item("collider")?; n.unbind() } None => py.None() };
        {
            let e = ent(s)?;
            let mut b = e.borrow_mut();
            b._parent = py.None();
            b.children = PyList::empty_bound(py).unbind();
            b._seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
            b._node = py.None();
            b._mesh = py.None();
            b._model = py.None();
            b._texture = py.None();
            b._color = color(py, 1.0, 1.0, 1.0, 1.0)?.unbind();
            b.pos = match position { Some(p) => it3(&vec3_of(py, p)?)?, None => [0.0; 3] };
            b.scl = [1.0; 3];
            b._origin = (0.0, 0.0);
            b._rot = 0.0;
            b._enabled = true;
            b._alpha = 1.0;
            b.name = name.unbind();
            b.ignore_input = false.into_py(py);
            b.collider = collider;
            b._alive = true;
        }
        let parent = match parent.filter(|p| !p.is_none()) { Some(p) => p.clone(), None => st(py)?.scene.bind(py).clone() };
        s.setattr("parent", parent)?;
        if nn(scale) {
            s.setattr("scale", scale.unwrap())?;
        }
        match origin {
            Some(o) if !o.is_none() => s.setattr("origin", o)?,
            Some(_) => {}
            None => s.setattr("origin", (0, 0))?,
        }
        if nn(rotation) {
            s.setattr("rotation", rotation.unwrap())?;
        }
        if nn(x) {
            s.setattr("x", x.unwrap())?;
        }
        if nn(y) {
            s.setattr("y", y.unwrap())?;
        }
        if nn(z) {
            s.setattr("z", z.unwrap())?;
        }
        if nn(color_) {
            s.setattr("color", color_.unwrap())?;
        }
        if nn(texture) {
            s.setattr("texture", texture.unwrap())?;
        }
        s.setattr("model", model.map(|m| m.clone()).unwrap_or_else(|| py.None().into_bound(py)))?;
        s.setattr("enabled", enabled.map(|m| m.clone()).unwrap_or_else(|| true.into_py(py).into_bound(py)))?;
        for (k, v) in kw.iter() {
            let _ = s.setattr(k.downcast::<pyo3::types::PyString>()?.clone(), v);
        }
        st(py)?.entities.bind(py).append(s)?;
        Ok(())
    }
    pub(crate) fn touch(s: &Bound<'_, Entity>) -> R<()> {
        if s.borrow()._alive {
            st(s.py())?.dirty.bind(s.py()).add(s)?;
        }
        Ok(())
    }
    pub(crate) fn parent_of<'py>(s: &Bound<'py, Entity>) -> Bound<'py, PyAny> {
        s.borrow()._parent.clone_ref(s.py()).into_bound(s.py())
    }
    pub(crate) fn node_of<'py>(s: &Bound<'py, Entity>) -> R<Bound<'py, PyAny>> {
        let py = s.py();
        let n = s.borrow()._node.clone_ref(py).into_bound(py);
        if !n.is_none() {
            return Ok(n);
        }
        let p = Self::parent_of(s);
        let pn = if p.is_none() { None } else { Some(p.call_method0("node")?) };
        let node = RsNode::create(py, pn.as_ref())?.into_any();
        s.borrow_mut()._node = node.clone().unbind();
        s.as_any().call_method0("_apply")?;
        Ok(node)
    }
    pub(crate) fn is_ui_(s: &Bound<'_, Entity>) -> R<bool> {
        let py = s.py();
        let ui = ui_root(py)?;
        let mut p = s.clone().into_any();
        while !p.is_none() {
            if p.is(&ui) {
                return Ok(true);
            }
            p = Self::parent_of(ent(&p)?);
        }
        Ok(false)
    }
    /// 自分から親へ（camera.ui の手前まで）
    pub(crate) fn chain<'py>(s: &Bound<'py, Entity>) -> R<Vec<Bound<'py, Entity>>> {
        let ui = ui_root(s.py())?;
        let mut out = vec![];
        let mut p = s.clone().into_any();
        while !p.is_none() && !p.is(&ui) {
            let e = ent(&p)?.clone();
            p = Self::parent_of(&e);
            out.push(e);
        }
        Ok(out)
    }
    pub(crate) fn world_z(s: &Bound<'_, Entity>) -> R<f64> {
        let mut z = 0.0;
        for e in Self::chain(s)? {
            z += e.borrow().pos[2];
        }
        Ok(z)
    }
    pub(crate) fn order(s: &Bound<'_, Entity>) -> R<i64> {
        let v = (-Self::world_z(s)? * 2000.0).min(2000.0).max(-2000.0);
        Ok((v.trunc() as i64) * 100000 + s.borrow()._seq.rem_euclid(100000))
    }
    pub(crate) fn world_xy(s: &Bound<'_, Entity>) -> R<(f64, f64)> {
        let (mut px, mut py_) = (0.0, 0.0);
        for e in Self::chain(s)? {
            let b = e.borrow();
            let r = (-b._rot).to_radians();
            let (c, sn) = (r.cos(), r.sin());
            let (qx, qy) = (px * b.scl[0], py_ * b.scl[1]);
            px = qx * c - qy * sn + b.pos[0];
            py_ = qx * sn + qy * c + b.pos[1];
        }
        Ok((px, py_))
    }
    pub(crate) fn world_scale_(s: &Bound<'_, Entity>) -> R<(f64, f64)> {
        let (mut sx, mut sy) = (1.0, 1.0);
        for e in Self::chain(s)? {
            let b = e.borrow();
            sx *= b.scl[0];
            sy *= b.scl[1];
        }
        Ok((sx, sy))
    }
    pub fn set_color_base(s: &Bound<'_, Entity>, c: &B<'_>) -> R<()> {
        let py = s.py();
        let c = py.import_bound("builtins")?.getattr("tuple")?.call1((c,))?;
        s.borrow_mut()._color = color_of(py, &c)?.unbind();
        if !s.borrow()._mesh.is_none(py) {
            s.as_any().call_method0("_paint")?;
        }
        Ok(())
    }
    pub(crate) fn set_origin_base(s: &Bound<'_, Entity>, v: &B<'_>) -> R<()> {
        let o0 = f64of(&v.get_item(0)?)?;
        let o1 = if v.len()? > 1 { f64of(&v.get_item(1)?)? } else { 0.0 };
        {
            let mut b = s.borrow_mut();
            b._origin = (o0, o1);
        }
        Ok(())
    }
    pub(crate) fn mouse_clear_if(py: Python<'_>, s: &B<'_>) -> R<()> {
        let m = mouse(py)?;
        if m.borrow().hovered_entity.bind(py).is(s) {
            m.borrow_mut().hovered_entity = py.None();
        }
        Ok(())
    }
}

#[pymethods]
impl Entity {
    #[classattr]
    pub(crate) fn default_shader(py: Python<'_>) -> PyObject {
        py.None()
    }
    #[classattr]
    #[allow(non_upper_case_globals)]
    const visible_self: bool = true;

    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<Self> {
        Entity::blank(py, "Entity")
    }
    #[pyo3(signature = (parent=None, model=None, texture=None, color=None, position=None, scale=None, origin=None, rotation=None, enabled=None,
                        z=None, x=None, y=None, eternal=None, **kw))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: Option<&B<'_>>, model: Option<&B<'_>>, texture: Option<&B<'_>>, color: Option<&B<'_>>,
                position: Option<&B<'_>>, scale: Option<&B<'_>>, origin: Option<&B<'_>>, rotation: Option<&B<'_>>, enabled: Option<&B<'_>>,
                z: Option<&B<'_>>, x: Option<&B<'_>>, y: Option<&B<'_>>, eternal: Option<&B<'_>>, kw: Option<&Bound<'_, PyDict>>) -> R<()> {
        let _ = eternal;
        let py = slf.py();
        let o00 = tb(py, (0, 0));
        Entity::init(slf.as_any(), parent, model, texture, color, position, scale, Some(origin.unwrap_or(&o00)), rotation, enabled, z, x, y, kw)
    }
    // ---- 親子 ----
    #[getter]
    pub(crate) fn parent(&self, py: Python<'_>) -> PyObject {
        self._parent.clone_ref(py)
    }
    #[setter(parent)]
    pub(crate) fn set_parent_p(slf: &Bound<'_, Self>, p: &B<'_>) -> R<()> {
        let py = slf.py();
        let old = Self::parent_of(slf);
        if !old.is_none() {
            let ch = old.getattr("children")?;
            if ch.contains(slf)? {
                ch.call_method1("remove", (slf,))?;
            }
        }
        slf.borrow_mut()._parent = p.clone().unbind();
        if !p.is_none() {
            p.getattr("children")?.call_method1("append", (slf,))?;
        }
        let n = slf.borrow()._node.clone_ref(py).into_bound(py);
        if !n.is_none() && !p.is_none() {
            n.setattr("parent", p.call_method0("node")?)?;
        }
        Self::touch(slf)
    }
    pub(crate) fn node<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        Self::node_of(slf)
    }
    #[getter]
    pub(crate) fn is_ui(slf: &Bound<'_, Self>) -> R<bool> {
        Self::is_ui_(slf)
    }
    // ---- 位置・大きさ ----
    pub(crate) fn _touch(slf: &Bound<'_, Self>) -> R<()> {
        Self::touch(slf)
    }
    /// ノードの行列（位置・回転・大きさ）と、形の z の順番
    pub(crate) fn _apply(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let n = slf.borrow()._node.clone_ref(py).into_bound(py);
        if n.is_none() || !slf.borrow()._alive {
            return Ok(());
        }
        let (m, en) = {
            let b = slf.borrow();
            let [sx, sy, sz] = b.scl;
            let r = (-b._rot).to_radians();
            let (c, s) = (r.cos(), r.sin());
            let [x, y, _] = b.pos;
            (vec![sx * c, sx * s, 0.0, 0.0, -sy * s, sy * c, 0.0, 0.0, 0.0, 0.0, sz, 0.0, x, y, 0.0, 1.0], b._enabled)
        };
        n.call_method1("setMat", (m,))?;
        let on = en && slf.getattr("visible_self")?.is_truthy()?;
        n.call_method0(if on { "show" } else { "hide" })?;
        Ok(())
    }
    pub(crate) fn _world_z(slf: &Bound<'_, Self>) -> R<f64> {
        Self::world_z(slf)
    }
    /// 画面の上の描く順（z が小さいほど後＝手前、同じなら作った順）
    pub(crate) fn _order(slf: &Bound<'_, Self>) -> R<i64> {
        Self::order(slf)
    }
    #[getter]
    pub(crate) fn position<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let p = slf.borrow().pos;
        vec3(slf.py(), p[0], p[1], p[2])
    }
    #[setter]
    pub(crate) fn set_position(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let v = vec3_of(slf.py(), v)?;
        Self::_set_pos(slf, &v)
    }
    #[getter]
    pub(crate) fn world_position<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let (x, y) = Self::world_xy(slf)?;
        vec3(slf.py(), x, y, Self::world_z(slf)?)
    }
    pub(crate) fn _set_pos(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let n = v.len()?;
        {
            let mut b = slf.borrow_mut();
            if n == 3 {
                b.pos = it3(v)?;
            } else {
                b.pos[0] = f64of(&v.get_item(0)?)?;
                b.pos[1] = f64of(&v.get_item(1)?)?;
            }
        }
        Self::touch(slf)
    }
    pub(crate) fn _seti(slf: &Bound<'_, Self>, i: usize, v: &B<'_>) -> R<()> {
        slf.borrow_mut().pos[i] = f64of(v)?;
        Self::touch(slf)
    }
    #[getter]
    pub(crate) fn _pos(&self) -> Vec<f64> {
        self.pos.to_vec()
    }
    #[setter(_pos)]
    pub(crate) fn set_pos_raw(&mut self, v: Vec<f64>) {
        for (k, x) in v.iter().take(3).enumerate() {
            self.pos[k] = *x;
        }
    }
    #[getter]
    pub(crate) fn _scale(&self) -> Vec<f64> {
        self.scl.to_vec()
    }
    #[setter(_scale)]
    pub(crate) fn set_scale_raw(&mut self, v: Vec<f64>) {
        for (k, x) in v.iter().take(3).enumerate() {
            self.scl[k] = *x;
        }
    }
    #[getter]
    pub(crate) fn x(&self) -> f64 {
        self.pos[0]
    }
    #[setter]
    pub(crate) fn set_x(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_seti(slf, 0, v)
    }
    #[getter]
    pub(crate) fn y(&self) -> f64 {
        self.pos[1]
    }
    #[setter]
    pub(crate) fn set_y(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_seti(slf, 1, v)
    }
    #[getter]
    pub(crate) fn z(&self) -> f64 {
        self.pos[2]
    }
    #[setter]
    pub(crate) fn set_z(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_seti(slf, 2, v)
    }
    #[getter]
    pub(crate) fn scale<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let s = slf.borrow().scl;
        vec3(slf.py(), s[0], s[1], s[2])
    }
    #[setter]
    pub(crate) fn set_scale(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let py = slf.py();
        if is_num(v) {
            let f = f64of(v)?;
            slf.borrow_mut().scl = [f; 3];
        } else {
            let l: Vec<B<'_>> = py.import_bound("builtins")?.getattr("list")?.call1((v,))?.iter()?.collect::<R<_>>()?;
            let z = if l.len() > 2 { f64of(&l[2])? } else { 1.0 };
            slf.borrow_mut().scl = [f64of(&l[0])?, f64of(&l[1])?, z];
        }
        Self::touch(slf)
    }
    pub(crate) fn _sets(slf: &Bound<'_, Self>, i: usize, v: &B<'_>) -> R<()> {
        slf.borrow_mut().scl[i] = f64of(v)?;
        Self::touch(slf)
    }
    #[getter]
    pub(crate) fn scale_x(&self) -> f64 {
        self.scl[0]
    }
    #[setter]
    pub(crate) fn set_scale_x(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_sets(slf, 0, v)
    }
    #[getter]
    pub(crate) fn scale_y(&self) -> f64 {
        self.scl[1]
    }
    #[setter]
    pub(crate) fn set_scale_y(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_sets(slf, 1, v)
    }
    #[getter]
    pub(crate) fn scale_z(&self) -> f64 {
        self.scl[2]
    }
    #[setter]
    pub(crate) fn set_scale_z(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::_sets(slf, 2, v)
    }
    #[getter]
    pub(crate) fn world_scale<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let (sx, sy) = Self::world_scale_(slf)?;
        vec3(slf.py(), sx, sy, 1.0)
    }
    #[setter]
    pub(crate) fn set_world_scale(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let py = slf.py();
        let (a, b) = if is_num(v) { let f = f64of(v)?; (f, f) } else { (f64of(&v.get_item(0)?)?, f64of(&v.get_item(1)?)?) };
        let p = Self::parent_of(slf);
        let (px, py_) = if p.is_none() { (1.0, 1.0) } else { it2(&p.getattr("world_scale")?)? };
        slf.as_any().setattr("scale", tb(py, (a / px.max(1e-9), b / py_.max(1e-9), 1)))
    }
    #[getter]
    pub(crate) fn world_scale_x(slf: &Bound<'_, Self>) -> R<f64> {
        Ok(Self::world_scale_(slf)?.0)
    }
    #[getter]
    pub(crate) fn world_scale_y(slf: &Bound<'_, Self>) -> R<f64> {
        Ok(Self::world_scale_(slf)?.1)
    }
    #[getter]
    pub(crate) fn rotation<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        vec3(slf.py(), 0.0, 0.0, slf.borrow()._rot)
    }
    #[setter]
    pub(crate) fn set_rotation(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let r = if is_seq(v) { f64of(&v.get_item(2)?)? } else { f64of(v)? };
        slf.borrow_mut()._rot = r;
        Self::touch(slf)
    }
    #[getter]
    pub(crate) fn rotation_z(&self) -> f64 {
        self._rot
    }
    #[setter]
    pub(crate) fn set_rotation_z(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        slf.as_any().setattr("rotation", v)
    }
    #[getter]
    pub(crate) fn origin<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let o = slf.borrow()._origin;
        vec2(slf.py(), o.0, o.1)
    }
    #[setter]
    pub(crate) fn set_origin(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        Self::set_origin_base(slf, v)?;
        slf.borrow_mut()._model_dirty = true;
        Self::touch(slf)
    }
    #[getter]
    pub(crate) fn origin_x(&self) -> f64 {
        self._origin.0
    }
    #[setter]
    pub(crate) fn set_origin_x(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let o = slf.borrow()._origin;
        slf.as_any().setattr("origin", (v, o.1))
    }
    #[getter]
    pub(crate) fn origin_y(&self) -> f64 {
        self._origin.1
    }
    #[setter]
    pub(crate) fn set_origin_y(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let o = slf.borrow()._origin;
        slf.as_any().setattr("origin", (o.0, v))
    }
    /// 画面の上の (x, y)（親の位置・大きさ・回転を含める。回転は z 軸だけ）
    pub(crate) fn _world_xy(slf: &Bound<'_, Self>) -> R<(f64, f64)> {
        Self::world_xy(slf)
    }
    /// 形（quad など）の画面の上の外枠 (x0, y0, x1, y1)。回転は無視
    pub(crate) fn _world_box(slf: &Bound<'_, Self>) -> R<(f64, f64, f64, f64)> {
        let (wx, wy) = Self::world_scale_(slf)?;
        let (cx, cy) = Self::world_xy(slf)?;
        let (ox, oy) = slf.borrow()._origin;
        let x0 = cx + (-0.5 - ox) * wx;
        let x1 = cx + (0.5 - ox) * wx;
        let y0 = cy + (-0.5 - oy) * wy;
        let y1 = cy + (0.5 - oy) * wy;
        Ok((x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)))
    }
    // ---- 見た目 ----
    #[getter]
    pub(crate) fn model(&self, py: Python<'_>) -> PyObject {
        self._model.clone_ref(py)
    }
    #[setter]
    pub(crate) fn set_model(slf: &Bound<'_, Self>, m: &B<'_>) -> R<()> {
        let py = slf.py();
        let m = if let Ok(s) = m.extract::<String>() {
            let ms = models(py)?;
            match ms.get_item(&s)? { Some(x) => x.unbind(), None => if s.is_empty() { py.None() } else { ms.get_item("quad")?.unwrap().unbind() } }
        } else {
            m.clone().unbind()
        };
        {
            let mut b = slf.borrow_mut();
            b._model = m;
            b._model_dirty = true;
        }
        slf.as_any().call_method0("_build_mesh")?;
        Ok(())
    }
    #[getter]
    pub(crate) fn texture(&self, py: Python<'_>) -> PyObject {
        self._texture.clone_ref(py)
    }
    #[setter]
    pub(crate) fn set_texture(slf: &Bound<'_, Self>, t: &B<'_>) -> R<()> {
        let py = slf.py();
        slf.borrow_mut()._texture = t.clone().unbind();
        let m = slf.borrow()._mesh.clone_ref(py).into_bound(py);
        if !m.is_none() {
            let r = tex_of(py, t)?;
            let id: PyObject = if r.is_none(py) { (-1i64).into_py(py) } else { r.bind(py).getattr("id")?.unbind() };
            view(py)?.call_method1("mesh_tex", (m.getattr("mid")?, 0, id))?;
        }
        Ok(())
    }
    #[getter]
    pub(crate) fn color(&self, py: Python<'_>) -> PyObject {
        self._color.clone_ref(py)
    }
    #[setter]
    pub(crate) fn set_color(slf: &Bound<'_, Self>, c: &B<'_>) -> R<()> {
        Self::set_color_base(slf, c)
    }
    #[getter]
    pub(crate) fn alpha(&self, py: Python<'_>) -> R<PyObject> {
        Ok(self._color.bind(py).get_item(3)?.unbind())
    }
    #[setter]
    pub(crate) fn set_alpha(slf: &Bound<'_, Self>, v: &B<'_>) -> R<()> {
        let py = slf.py();
        let c = slf.borrow()._color.clone_ref(py).into_bound(py);
        slf.as_any().setattr("color", (c.get_item(0)?, c.get_item(1)?, c.get_item(2)?, v))
    }
    pub(crate) fn _build_mesh(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        if !slf.borrow()._alive {
            return Ok(());
        }
        if slf.borrow()._model.is_none(py) || !Self::is_ui_(slf)? {
            // 3D の部品はまだ描かない（画面の上だけ）
            let m = slf.borrow()._mesh.clone_ref(py).into_bound(py);
            if !m.is_none() {
                m.call_method0("remove")?;
                slf.borrow_mut()._mesh = py.None();
            }
            return Ok(());
        }
        if slf.borrow()._mesh.is_none(py) {
            let node = Self::node_of(slf)?;
            let m = RsDynamicMesh::create(py, Some(&node))?;
            view(py)?.call_method1("mesh_overlay", (m.getattr("mid")?, Self::is_ui_(slf)?))?;
            slf.borrow_mut()._mesh = m.into_any().unbind();
        }
        slf.as_any().call_method("_paint", (), Some(&[("new", true)].into_py_dict_bound(py)))?;
        let t = slf.borrow()._texture.clone_ref(py);
        slf.as_any().setattr("texture", t)?;
        Self::touch(slf)
    }
    #[pyo3(signature = (new=false))]
    pub(crate) fn _paint(slf: &Bound<'_, Self>, new: bool) -> R<()> {
        let py = slf.py();
        let model = slf.borrow()._model.clone_ref(py).into_bound(py);
        let arr = model.call_method0("arrays")?;
        let np = imp(py, "numpy")?;
        let f64t = np.getattr("float64")?;
        let va = np.call_method1("ascontiguousarray", (arr.get_item(0)?, &f64t))?;
        let uva = np.call_method1("ascontiguousarray", (arr.get_item(1)?, &f64t))?;
        let t = arr.get_item(2)?;
        let vr: numpy::PyReadonlyArray2<f64> = va.extract()?;
        let uvr: numpy::PyReadonlyArray2<f64> = uva.extract()?;
        let v = vr.as_array();
        let uv = uvr.as_array();
        let n = v.nrows();
        let (ox, oy) = slf.borrow()._origin;
        let base: Vec<f64> = slf.borrow()._color.bind(py).iter()?.map(|x| f64of(&x?)).collect::<R<_>>()?;
        let cols = model.getattr("colors").unwrap_or_else(|_| py.None().into_bound(py));
        let mut out = vec![0f32; n * 12];
        let use_cols = !cols.is_none() && cols.len()? == n;
        let mut cv: Vec<[f64; 4]> = vec![];
        if use_cols {
            for c in cols.iter()? {
                let c = c?;
                let l: Vec<f64> = c.iter()?.map(|x| f64of(&x?)).collect::<R<_>>()?;
                cv.push(if l.len() >= 4 { [l[0], l[1], l[2], l[3]] } else { [l[0], l[1], l[2], 1.0] });
            }
        }
        for i in 0..n {
            let r = &mut out[i * 12..i * 12 + 12];
            r[0] = (v[[i, 0]] - ox) as f32;
            r[1] = (v[[i, 1]] - oy) as f32;
            r[5] = -1.0;
            for k in 0..4 {
                r[6 + k] = if use_cols { (cv[i][k] * base[k]) as f32 } else { base[k] as f32 };
            }
            r[10] = uv[[i, 0]] as f32;
            r[11] = uv[[i, 1]] as f32;
        }
        let a = numpy::PyArray1::from_vec_bound(py, out);
        let a = numpy::PyArrayMethods::reshape(&a, [n, 12])?;
        let mesh = slf.borrow()._mesh.clone_ref(py).into_bound(py);
        let nr: i64 = mesh.getattr("n_rows")?.extract()?;
        let tt: PyObject = if new || nr != n as i64 { t.unbind() } else { py.None() };
        mesh.call_method1("set", (a, tt))?;
        mesh.setattr("n_rows", n)?;
        Ok(())
    }
    pub(crate) fn _sort(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let m = slf.borrow()._mesh.clone_ref(py).into_bound(py);
        if !m.is_none() && slf.borrow()._alive {
            let b = BIN0 + Self::order(slf)?.div_euclid(100000) * 1000 + slf.borrow()._seq.rem_euclid(1000);
            view(py)?.call_method1("mesh_props", (m.getattr("mid")?, true, false, true, 0, b))?;
            if slf.borrow()._model_dirty {
                slf.borrow_mut()._model_dirty = false;
                slf.as_any().call_method0("_paint")?;
            }
        }
        Ok(())
    }
    // ---- 表示・入力 ----
    #[getter]
    pub(crate) fn enabled(&self) -> bool {
        self._enabled
    }
    #[setter]
    pub(crate) fn set_enabled(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let py = slf.py();
        let on = on.is_truthy()?;
        slf.borrow_mut()._enabled = on;
        Self::touch(slf)?;
        if !on {
            let m = mouse(py)?;
            if !m.borrow().hovered_entity.is_none(py) {
                m.borrow_mut().hovered_entity = py.None();
            }
        }
        Ok(())
    }
    #[getter]
    pub(crate) fn visible(&self) -> bool {
        self._enabled
    }
    #[setter]
    pub(crate) fn set_visible(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        Self::set_enabled(slf, on)
    }
    pub(crate) fn active_chain(slf: &Bound<'_, Self>) -> R<bool> {
        let mut p = slf.clone().into_any();
        while !p.is_none() {
            let e = ent(&p)?.clone();
            {
                let b = e.borrow();
                if !b._enabled || !b._alive {
                    return Ok(false);
                }
            }
            p = Self::parent_of(&e);
        }
        Ok(true)
    }
    pub(crate) fn show(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().setattr("enabled", true)
    }
    pub(crate) fn hide(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().setattr("enabled", false)
    }
    #[getter]
    pub(crate) fn hovered(slf: &Bound<'_, Self>) -> R<bool> {
        let py = slf.py();
        Ok(mouse(py)?.borrow().hovered_entity.bind(py).is(slf))
    }
    pub(crate) fn disable(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().setattr("enabled", false)
    }
    pub(crate) fn enable(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().setattr("enabled", true)
    }
    // Panda3D の NodePath の物はまねるだけ
    #[pyo3(name = "setTwoSided", signature = (*_a))]
    pub(crate) fn set_two_sided(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setDepthOffset", signature = (*_a))]
    pub(crate) fn set_depth_offset(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setBin", signature = (*_a))]
    pub(crate) fn set_bin(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setLightOff", signature = (*_a))]
    pub(crate) fn set_light_off(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setShaderOff", signature = (*_a))]
    pub(crate) fn set_shader_off(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setDepthTest", signature = (*_a))]
    pub(crate) fn set_depth_test(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setDepthWrite", signature = (*_a))]
    pub(crate) fn set_depth_write(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setTransparency", signature = (*_a))]
    pub(crate) fn set_transparency(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "set_two_sided", signature = (*_a))]
    pub(crate) fn set_two_sided2(&self, _a: &Bound<'_, PyTuple>) {}
    #[pyo3(name = "setColorScale", signature = (r, g, b, a=1.0))]
    pub(crate) fn set_color_scale(slf: &Bound<'_, Self>, r: f64, g: f64, b: f64, a: f64) -> R<()> {
        let py = slf.py();
        let n = Self::node_of(slf)?;
        view(py)?.call_method1("node_tint", (n.getattr("id")?, r, g, b, a))?;
        Ok(())
    }
    #[pyo3(signature = (v, *_a, **_k))]
    pub(crate) fn animate_scale(slf: &Bound<'_, Self>, v: &B<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<()> {
        slf.as_any().setattr("scale", v)
    }
    #[pyo3(signature = (v, *_a, **_k))]
    pub(crate) fn animate_color(slf: &Bound<'_, Self>, v: &B<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> R<()> {
        slf.as_any().setattr("color", v)
    }
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn fade_out(&self, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) {}
    pub(crate) fn _destroy(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        if !slf.borrow()._alive {
            return Ok(());
        }
        slf.borrow_mut()._alive = false;
        let ch: Vec<B<'_>> = slf.borrow().children.bind(py).iter().collect();
        for c in ch {
            c.call_method0("_destroy")?;
        }
        let n = slf.borrow()._node.clone_ref(py).into_bound(py);
        if !n.is_none() {
            n.call_method0("removeNode")?;
            slf.borrow_mut()._node = py.None();
        }
        slf.borrow_mut()._mesh = py.None();
        let p = Self::parent_of(slf);
        if !p.is_none() {
            let ch = p.getattr("children")?;
            if ch.contains(slf)? {
                ch.call_method1("remove", (slf,))?;
            }
        }
        let s = st(py)?;
        let es = s.entities.bind(py);
        if es.contains(slf)? {
            es.call_method1("remove", (slf,))?;
        }
        s.dirty.bind(py).discard(slf)?;
        Self::mouse_clear_if(py, slf.as_any())?;
        if focus(py)?.is(slf) {
            set_focus(py, &py.None().into_bound(py))?;
        }
        Ok(())
    }
}

#[pyfunction]
#[pyo3(signature = (e, delay=None))]
pub fn ursina_destroy(py: Python<'_>, e: &B<'_>, delay: Option<&B<'_>>) -> R<()> {
    if e.is_none() {
        return Ok(());
    }
    if let Some(d) = delay.filter(|d| d.is_truthy().unwrap_or(false)) {
        let k = PyDict::new_bound(py);
        let _ = &k;
        return ursina_invoke(py, &imp(py, "alpedia_core")?.getattr("ursina_destroy")?, &PyTuple::new_bound(py, [e]), Some(d), None);
    }
    if e.is_instance_of::<Entity>() {
        e.call_method0("_destroy")?;
    }
    Ok(())
}
