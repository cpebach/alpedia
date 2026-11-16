//! ursina のまね：Rust の画面からまわす

use super::*;

// ---------------------------------------------------------------------------
// Rust の画面からまわす
// ---------------------------------------------------------------------------
pub(crate) fn overrides(e: &B<'_>, name: &str) -> R<bool> {
    let t = e.get_type();
    let et = e.py().get_type_bound::<Entity>();
    let a = t.getattr(name).ok();
    let b = et.getattr(name).ok();
    let differ = match (&a, &b) {
        (None, None) => false,
        (Some(x), Some(y)) => !x.is(y),
        _ => true,
    };
    Ok(differ && e.hasattr(name)?)
}

/// 毎フレーム：invoke・update() を呼び、位置・描く順を Rust の画面へ
#[pyfunction]
pub fn ursina_frame(py: Python<'_>, view_: &B<'_>, dt: f64) -> R<()> {
    let s = st(py)?;
    s.time.borrow_mut(py).dt = dt;
    let wh = view_.call_method0("size")?;
    s.window.borrow_mut(py)._size = PyTuple::new_bound(py, wh.iter()?.collect::<R<Vec<_>>>()?).into_any().unbind();
    let now: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
    let inv = s.invokes.bind(py);
    let due: Vec<B<'_>> = inv.iter().filter(|x| x.get_item(0).and_then(|t| t.extract::<f64>()).map(|t| t <= now).unwrap_or(false)).collect();
    for x in due {
        inv.call_method1("remove", (&x,))?;
        let k = x.get_item(3)?;
        if let Err(e) = x.get_item(1)?.call(x.get_item(2)?.downcast::<PyTuple>()?, Some(k.downcast::<PyDict>()?)) {
            log_exc(py, "invoke に失敗しました", e)?;
        }
    }
    let es: Vec<B<'_>> = s.entities.bind(py).iter().collect();
    for e in &es {
        let ee = ent(e)?;
        if ee.borrow()._alive && overrides(e, "update")? && Entity::active_chain(ee)? {
            e.call_method0("update")?;
        }
    }
    let dirty = s.dirty.bind(py);
    let ds: Vec<B<'_>> = dirty.iter().collect();
    for e in &ds {
        if ent(e)?.borrow()._alive {
            e.call_method0("_apply")?;
        }
    }
    if !dirty.is_empty() {
        let es: Vec<B<'_>> = s.entities.bind(py).iter().collect();
        for e in &es {
            let ee = ent(e)?;
            if ee.borrow()._alive && Entity::is_ui_(ee)? {
                e.call_method0("_sort")?;
            }
        }
    }
    dirty.clear();
    Ok(())
}

/// マウスの下のいちばん手前のボタン
pub(crate) fn hit<'py>(py: Python<'py>, x: f64, y: f64) -> R<Option<Bound<'py, PyAny>>> {
    let mut best: Option<((f64, i64), B<'py>)> = None;
    let es: Vec<B<'py>> = st(py)?.entities.bind(py).iter().collect();
    for e in es {
        if !e.is_instance_of::<Button>() {
            continue;
        }
        let ee = ent(&e)?.clone();
        if !ee.borrow()._alive || e.getattr("ignore_input")?.is_truthy()? || !Entity::active_chain(&ee)? || !Entity::is_ui_(&ee)? {
            continue;
        }
        let (x0, y0, x1, y1): (f64, f64, f64, f64) = e.call_method0("_world_box")?.extract()?;
        if x0 <= x && x <= x1 && y0 <= y && y <= y1 {
            let k = (Entity::world_z(&ee)?, -ee.borrow()._seq);
            let better = match &best { None => true, Some((bk, _)) => k.0 < bk.0 || (k.0 == bk.0 && k.1 < bk.1) };
            if better {
                best = Some((k, e));
            }
        }
    }
    Ok(best.map(|b| b.1))
}

#[pyfunction]
pub(crate) fn ursina_hit<'py>(py: Python<'py>, x: f64, y: f64) -> R<PyObject> {
    Ok(hit(py, x, y)?.map(|h| h.unbind()).unwrap_or_else(|| py.None()))
}

/// 入力を ursina の形（"left mouse down" など）にして配る。使った（画面が開いている）なら True
#[pyfunction]
pub fn ursina_event(py: Python<'_>, view_: &B<'_>, kind: &str, a: &B<'_>, b: &B<'_>, n: i64) -> R<bool> {
    let m = mouse(py)?;
    if kind == "cursor" {
        let (w, h): (f64, f64) = it2(&view_.call_method0("size")?)?;
        let (ax, by) = (f64of(a)?, f64of(b)?);
        {
            let mut mb = m.borrow_mut();
            mb.x = (ax - w / 2.0) / h;
            mb.y = (h / 2.0 - by) / h;
        }
        let (mx, my) = { let mb = m.borrow(); (mb.x, mb.y) };
        let hb = hit(py, mx, my)?;
        let old = m.borrow().hovered_entity.clone_ref(py).into_bound(py);
        let same = match &hb { Some(h) => h.is(&old), None => old.is_none() };
        if !same {
            if old.is_instance_of::<Button>() && ent(&old)?.borrow()._alive {
                old.call_method1("_hover", (false,))?;
            }
            if let Some(h) = &hb {
                if h.is_instance_of::<Button>() {
                    h.call_method1("_hover", (true,))?;
                }
            }
            m.borrow_mut().hovered_entity = hb.map(|h| h.unbind()).unwrap_or_else(|| py.None());
        }
        return Ok(true);
    }
    if kind == "button" {
        let down = a.is_truthy()?;
        let name = format!("{} mouse {}", ["left", "right", "middle", "x"][n.clamp(0, 3) as usize], if down { "down" } else { "up" });
        if n == 0 {
            m.borrow_mut().left = down;
        }
        if down && n == 0 {
            let (mx, my) = { let mb = m.borrow(); (mb.x, mb.y) };
            if let Some(h) = hit(py, mx, my)? {
                let f = h.getattr("on_click")?;
                if !f.is_none() && f.is_callable() {
                    f.call0()?;
                }
            }
        }
        send(py, &name)?;
        return Ok(true);
    }
    if kind == "wheel" {
        send(py, if f64of(a)? > 0.0 { "scroll up" } else { "scroll down" })?;
        return Ok(true);
    }
    if let Some(k0) = kind.strip_prefix("key:") {
        let k = match k0 { "control" => "left control", "shift" => "left shift", "alt" => "left alt", x => x };
        let down = a.is_truthy()?;
        st(py)?.held_keys.bind(py).set_item(k, if down { 1 } else { 0 })?;
        let f = focus(py)?;
        if down && !f.is_none() && ent(&f)?.borrow()._alive && f.call_method1("key", (k,))?.is_truthy()? {
            return Ok(true);
        }
        send(py, &if down { k.to_string() } else { format!("{} up", k) })?;
        return Ok(true);
    }
    if let Some(t) = kind.strip_prefix("text:") {
        let f = focus(py)?;
        if !f.is_none() && ent(&f)?.borrow()._alive {
            f.call_method1("type", (t,))?;
        }
        return Ok(true);
    }
    Ok(kind == "mouse")
}

pub(crate) fn send(py: Python<'_>, key: &str) -> R<()> {
    let es: Vec<B<'_>> = st(py)?.entities.bind(py).iter().collect();
    for e in es {
        let ee = ent(&e)?.clone();
        if ee.borrow()._alive && overrides(&e, "input")? && Entity::active_chain(&ee)? {
            if let Err(err) = e.call_method1("input", (key,)) {
                log_exc(py, "input に失敗しました", err)?;
            }
        }
    }
    Ok(())
}

#[pyfunction]
pub(crate) fn ursina_send(py: Python<'_>, key: &str) -> R<()> {
    send(py, key)
}

/// 画面の上に ursina の部品が出ているか
#[pyfunction]
pub fn ursina_any_ui(py: Python<'_>) -> R<bool> {
    let ui = ui_root(py)?;
    for c in ui.getattr("children")?.iter()? {
        let c = c?;
        let e = ent(&c)?;
        if e.borrow()._alive && Entity::active_chain(e)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// ursina_shim のモジュールに入れる名前（部品・全体の状態）
#[pyfunction]
pub fn ursina_names<'py>(py: Python<'py>) -> R<Bound<'py, PyDict>> {
    let s = st(py)?;
    let d = PyDict::new_bound(py);
    d.set_item("Color", cls(py, &COLOR)?)?;
    d.set_item("Vec2", cls(py, &VEC2)?)?;
    d.set_item("Vec3", cls(py, &VEC3)?)?;
    d.set_item("color", s.color.bind(py))?;
    d.set_item("_ColorMod", py.get_type_bound::<ColorMod>())?;
    d.set_item("Mesh", py.get_type_bound::<Mesh>())?;
    d.set_item("Quad", py.get_type_bound::<Quad>())?;
    let ms = models(py)?;
    d.set_item("_QUAD", ms.get_item("quad")?)?;
    d.set_item("_MODELS", ms)?;
    d.set_item("Texture", py.get_type_bound::<Texture>())?;
    d.set_item("Entity", py.get_type_bound::<Entity>())?;
    d.set_item("Text", py.get_type_bound::<Text>())?;
    d.set_item("Button", py.get_type_bound::<Button>())?;
    d.set_item("InputField", py.get_type_bound::<InputField>())?;
    d.set_item("_TextField", py.get_type_bound::<TextField>())?;
    d.set_item("_entities", s.entities.bind(py))?;
    d.set_item("_dirty", s.dirty.bind(py))?;
    d.set_item("_focus", s.focus.bind(py))?;
    d.set_item("_invokes", s.invokes.bind(py))?;
    d.set_item("_Mouse", py.get_type_bound::<Mouse>())?;
    d.set_item("_mouse", s.mouse.bind(py))?;
    d.set_item("mouse", s.mouse.bind(py))?;
    d.set_item("_Window", py.get_type_bound::<Window>())?;
    d.set_item("window", s.window.bind(py))?;
    d.set_item("held_keys", s.held_keys.bind(py))?;
    d.set_item("time_", s.time.bind(py))?;
    d.set_item("time", s.time.bind(py))?;
    d.set_item("application", s.application.bind(py))?;
    d.set_item("_Scene", py.get_type_bound::<Scene>())?;
    d.set_item("scene", s.scene.bind(py))?;
    d.set_item("camera", s.camera.bind(py))?;
    d.set_item("Sequence", py.get_type_bound::<Sequence>())?;
    d.set_item("Audio", py.get_type_bound::<Audio>())?;
    d.set_item("input_handler", Py::new(py, InputHandler {})?)?;
    d.set_item("Ursina", py.get_type_bound::<Ursina>())?;
    d.set_item("BIN0", BIN0)?;
    d.set_item("TEXT_SIZE", TEXT_SIZE)?;
    d.set_item("FONT_K", FONT_K)?;
    let me = imp(py, "alpedia_core")?;
    for (k, f) in [("load_texture", "ursina_load_texture"), ("load_model", "ursina_load_model"), ("_tex_of", "ursina_tex_of"),
                   ("destroy", "ursina_destroy"), ("invoke", "ursina_invoke"), ("frame", "ursina_frame"), ("_hit", "ursina_hit"),
                   ("event", "ursina_event"), ("_send", "ursina_send"), ("any_ui", "ursina_any_ui")] {
        d.set_item(k, me.getattr(f)?)?;
    }
    Ok(d)
}

#[pyfunction]
pub(crate) fn ursina_tex_of(py: Python<'_>, t: &B<'_>) -> R<PyObject> {
    tex_of(py, t)
}

/// sys.modules["ursina"] をこのまねにする（Rust の画面のときだけ）
#[pyfunction]
pub fn ursina_install<'py>(py: Python<'py>, m: &B<'py>) -> R<B<'py>> {
    imp(py, "main.client_rs.panda_compat")?.call_method0("install")?;
    let sm = imp(py, "sys")?.getattr("modules")?;
    sm.set_item("ursina", m)?;
    let ns = PyDict::new_bound(py);
    let c = st(py)?.color.bind(py);
    for k in ["rgb", "rgba", "hsv"] {
        ns.set_item(k, c.getattr(k)?)?;
    }
    for (n, _) in NAMED {
        ns.set_item(*n, c.getattr(*n)?)?;
    }
    sm.set_item("ursina.color", imp(py, "types")?.getattr("SimpleNamespace")?.call((), Some(&ns))?)?;
    Ok(m.clone())
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    for f in [wrap_pyfunction_bound!(_color_new, m)?, wrap_pyfunction_bound!(_vec2_new, m)?, wrap_pyfunction_bound!(_vec3_new, m)?,
              wrap_pyfunction_bound!(_vec2_add, m)?, wrap_pyfunction_bound!(_vec2_sub, m)?, wrap_pyfunction_bound!(_vec2_mul, m)?,
              wrap_pyfunction_bound!(_vec3_add, m)?, wrap_pyfunction_bound!(_vec3_sub, m)?, wrap_pyfunction_bound!(_vec3_mul, m)?,
              wrap_pyfunction_bound!(ursina_load_texture, m)?, wrap_pyfunction_bound!(ursina_load_model, m)?, wrap_pyfunction_bound!(ursina_tex_of, m)?,
              wrap_pyfunction_bound!(ursina_destroy, m)?, wrap_pyfunction_bound!(ursina_invoke, m)?, wrap_pyfunction_bound!(ursina_frame, m)?,
              wrap_pyfunction_bound!(ursina_hit, m)?, wrap_pyfunction_bound!(ursina_event, m)?, wrap_pyfunction_bound!(ursina_send, m)?,
              wrap_pyfunction_bound!(ursina_any_ui, m)?, wrap_pyfunction_bound!(ursina_names, m)?, wrap_pyfunction_bound!(ursina_install, m)?] {
        m.add_function(f)?;
    }
    m.add_class::<BoundFn>()?;
    m.add_class::<ColorMod>()?;
    m.add_class::<Mesh>()?;
    m.add_class::<Quad>()?;
    m.add_class::<Texture>()?;
    m.add_class::<Entity>()?;
    m.add_class::<Text>()?;
    m.add_class::<Button>()?;
    m.add_class::<TextField>()?;
    m.add_class::<Cursor>()?;
    m.add_class::<InputField>()?;
    m.add_class::<Mouse>()?;
    m.add_class::<Window>()?;
    m.add_class::<Time>()?;
    m.add_class::<App>()?;
    m.add_class::<Lens>()?;
    m.add_class::<CamNode>()?;
    m.add_class::<Cam>()?;
    m.add_class::<Base>()?;
    m.add_class::<Scene>()?;
    m.add_class::<Camera>()?;
    m.add_class::<Sequence>()?;
    m.add_class::<Audio>()?;
    m.add_class::<InputHandler>()?;
    m.add_class::<Ursina>()?;
    Ok(())
}
