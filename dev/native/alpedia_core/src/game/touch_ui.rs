//! 画面のボタン（タッチ・マウスで押せる操作）と、仮想ジョイスティック（main/client/touch_ui.py）。
//!
//! - 右上：メニュー・持ち物・マップなどの小さなボタン（いつも出ている）
//! - 右下：ジャンプ・掘る・置くなどの操作ボタン（操縦席では「降りる」と、機器に割り当てたキー）
//! - 右の中ほど：近くにある、触れるもの（操縦席・コンピューター・お店・NPC など）のボタン（近づくと出る）
//! - 左下：仮想ジョイスティック（ゲーム設定・一時停止メニューで ON にしたとき）。画面のほかの所をなぞると視点が回る
//!
//! 指（タッチ）は画面（Rust）が届ける指の出来事を、指が無ければマウスの左ボタン（1本の指）を使う。
//! キーボード・マウスで遊ぶときは、Alt を押している間カーソルが出るので、ボタンをクリックできる。

use pyo3::exceptions::{PyException, PyOSError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};

use super::imp;
use super::panels::{kw, po};
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

pub const SMALL: f64 = 0.034; // 右上のボタンの半径（画面の高さ = 1）
pub const BIG: f64 = 0.058;
pub const MID: f64 = 0.046;
pub const STICK_R: f64 = 0.11; // ジョイスティックの半径
pub const HOLD_TIME: f64 = 0.45; // この秒数より長く、指を動かさずに触っていると長押し
pub const TAP_TIME: f64 = 0.3; // これより短く離せばタップ
pub const MOVE_TOL: f64 = 0.012; // これより動いたら、なぞる（視点）
pub const REPEAT: f64 = 0.22; // 長押しのくり返しの間隔

fn lg(py: Python<'_>) -> R<B<'_>> {
    imp(py, "main.log")?.call_method1("get", ("touch",))
}
fn mono(py: Python<'_>) -> R<f64> {
    imp(py, "time")?.call_method0("monotonic")?.extract()
}
fn ui_texture<'py>(py: Python<'py>, name: &str) -> R<B<'py>> {
    imp(py, "main.client.icons")?.call_method1("ui_texture", (name,))
}
fn log_exc(py: Python<'_>, e: &PyErr, msg: &str, args: Vec<PyObject>) -> R<()> {
    let k = PyDict::new_bound(py);
    k.set_item("exc_info", e.value_bound(py))?;
    let mut a: Vec<PyObject> = vec![po(py, msg)];
    a.extend(args);
    lg(py)?.call_method("exception", PyTuple::new_bound(py, a), Some(&k))?;
    Ok(())
}
/// 例外なら記録して、世界に「⚠ エラー」を出す
fn guard(py: Python<'_>, game: &B<'_>, r: R<()>, msg: &str, args: Vec<PyObject>) -> R<()> {
    match r {
        Ok(()) => Ok(()),
        Err(e) if e.is_instance_of::<PyException>(py) => {
            log_exc(py, &e, msg, args)?;
            game.getattr("world")?.call_method1("message", (format!("⚠ エラー: {}", e.value_bound(py).str()?),))?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn key_name_s(k: &str) -> String {
    match k {
        "" => String::new(),
        "space" => "Space".into(),
        "left control" => "Ctrl".into(),
        "left shift" => "Shift".into(),
        "escape" => "Esc".into(),
        "left alt" => "Alt".into(),
        "up arrow" => "↑".into(),
        "down arrow" => "↓".into(),
        "left arrow" => "←".into(),
        "right arrow" => "→".into(),
        "tab" => "Tab".into(),
        "enter" => "Enter".into(),
        "backspace" => "BS".into(),
        _ if k.chars().count() <= 3 => k.to_uppercase(),
        _ => k.into(),
    }
}
/// キーの表示名（space → Space、e → E など）
#[pyfunction]
pub fn key_name<'py>(py: Python<'py>, k: &B<'py>) -> R<B<'py>> {
    if !k.is_truthy()? {
        return Ok(po(py, "").into_bound(py));
    }
    if let Ok(s) = k.downcast::<PyString>() {
        return Ok(po(py, key_name_s(&s.to_string())).into_bound(py));
    }
    imp(py, "main.client.touch_ui")?.getattr("KEY_NAMES")?.call_method1("get", (k, k.call_method0("upper")?))
}
fn seat_label(k: &str) -> Option<&'static str> {
    match k {
        "space" => Some("上昇"),
        "left control" | "left shift" => Some("下降"),
        _ => None,
    }
}

// ---------------------------------------------------------------- Pointers
/// いま画面に触れている指（またはマウスの左ボタン）：{番号: (x, y)}（画面の UI 座標）。
/// 画面（viewer）が feed で指の出来事を渡す。
#[pyclass(module = "main.client.touch_ui", name = "Pointers", subclass, dict)]
pub struct TPointers {
    #[pyo3(get, set)]
    game: PyObject,
    #[pyo3(get, set)]
    touches: PyObject,
    #[pyo3(get, set)]
    touch_t: f64,
    #[pyo3(get, set)]
    multi: bool,
}

#[pymethods]
impl TPointers {
    #[new]
    fn new(py: Python<'_>, game: PyObject) -> Self {
        TPointers { game, touches: PyDict::new_bound(py).into_any().unbind(), touch_t: 0.0, multi: true }
    }
    /// kind：touch:start / touch:move / touch:end、(x, y) は UI 座標。
    fn feed(&mut self, py: Python<'_>, kind: &str, x: PyObject, y: PyObject, n: PyObject) -> R<()> {
        let t = self.touches.bind(py);
        if kind == "touch:end" {
            t.call_method1("pop", (n, py.None()))?;
        } else {
            t.set_item(n, (x, y))?;
        }
        self.touch_t = mono(py)?;
        Ok(())
    }
    fn current<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyDict>> {
        let t = self.touches.bind(py);
        let out = PyDict::new_bound(py);
        if t.is_truthy()? && mono(py)? - self.touch_t < 5.0 {
            for it in t.call_method0("items")?.iter()? {
                let it = it?;
                out.set_item(("t", it.get_item(0)?), it.get_item(1)?)?;
            }
            return Ok(out);
        }
        t.call_method0("clear")?;
        let mouse = imp(py, "ursina")?.getattr("mouse")?;
        if mouse.getattr("left")?.is_truthy()? {
            let cap = self.game.bind(py).getattr("mouse_captured").map(|c| c.is_truthy()).unwrap_or(Ok(false))?;
            if !cap {
                out.set_item("m", self.mouse(py)?)?;
            }
        }
        Ok(out)
    }
    fn mouse<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyTuple>> {
        let mouse = imp(py, "ursina")?.getattr("mouse")?;
        Ok(PyTuple::new_bound(py, [mouse.getattr("x")?, mouse.getattr("y")?]))
    }
}

// ---------------------------------------------------------------- Btn
#[pyclass(module = "main.client.touch_ui", subclass, dict)]
pub struct Btn {
    #[pyo3(get, set)]
    id: PyObject,
    #[pyo3(get, set)]
    r: f64,
    #[pyo3(get, set)]
    kind: String,
    #[pyo3(get, set)]
    key: PyObject,
    #[pyo3(get, set)]
    on_tap: PyObject,
    #[pyo3(get, set)]
    press_key: bool,
    #[pyo3(get, set)]
    color: PyObject,
    #[pyo3(get, set)]
    label: PyObject,
    #[pyo3(get, set)]
    x: f64,
    #[pyo3(get, set)]
    y: f64,
    #[pyo3(get, set)]
    visible: bool,
    #[pyo3(get, set)]
    pressed: bool,
    #[pyo3(get, set)]
    root: PyObject,
    #[pyo3(get, set)]
    img: PyObject,
    #[pyo3(get, set)]
    text: PyObject,
}

#[pymethods]
impl Btn {
    #[new]
    #[pyo3(signature = (hud, bid, icon, label, r, kind="tap", key=None, on_tap=None, press_key=false))]
    #[allow(clippy::too_many_arguments)]
    fn new(py: Python<'_>, hud: &B<'_>, bid: PyObject, icon: &str, label: PyObject, r: f64, kind: &str, key: Option<PyObject>, on_tap: Option<PyObject>,
           press_key: bool) -> R<Self> {
        let u = hud.getattr("U")?;
        let (entity, color) = (u.get_item("Entity")?, u.get_item("color")?);
        let root = entity.call((), Some(&kw(py, vec![("parent", hud.getattr("root")?.unbind()), ("enabled", po(py, false))])?))?;
        let img = entity.call((), Some(&kw(py, vec![("parent", root.clone().unbind()), ("model", po(py, "quad")), ("texture", ui_texture(py, icon)?.unbind()),
            ("scale", po(py, 2.0 * r))])?))?;
        let text = u.get_item("Text")?.call((), Some(&kw(py, vec![("parent", root.clone().unbind()), ("text", label.clone_ref(py)), ("origin", po(py, (0, 0.5))),
            ("position", po(py, (0.0, -r - 0.004))), ("scale", po(py, if r < MID { 0.55 } else { 0.62 })), ("color", color.call_method1("rgba", (1, 1, 1, 0.92))?.unbind())])?))?;
        Ok(Btn { id: bid, r, kind: kind.into(), key: key.unwrap_or_else(|| py.None()), on_tap: on_tap.unwrap_or_else(|| py.None()), press_key, color: color.unbind(), label,
            x: 0.0, y: 0.0, visible: false, pressed: false, root: root.unbind(), img: img.unbind(), text: text.unbind() })
    }
    #[pyo3(signature = (x, y, visible=true, label=None))]
    fn place(&mut self, py: Python<'_>, x: f64, y: f64, visible: bool, label: Option<&B<'_>>) -> R<()> {
        self.x = x;
        self.y = y;
        let root = self.root.bind(py);
        root.setattr("position", (x, y, 0))?;
        if let Some(l) = label.filter(|l| !l.is_none()) {
            let t = self.text.bind(py);
            if !l.eq(t.getattr("text")?)? {
                t.setattr("text", l)?;
            }
        }
        if visible != self.visible {
            self.visible = visible;
            root.setattr("enabled", visible)?;
            if !visible {
                self.pressed = false;
            }
        }
        Ok(())
    }
    fn hit(&self, x: f64, y: f64) -> bool {
        self.visible && (x - self.x).powi(2) + (y - self.y).powi(2) <= (self.r * 1.15).powi(2)
    }
    fn show_pressed(&self, py: Python<'_>, on: bool) -> R<()> {
        let img = self.img.bind(py);
        img.setattr("scale", 2.0 * self.r * if on { 0.9 } else { 1.0 })?;
        let c = self.color.bind(py);
        img.setattr("color", if on { c.call_method1("rgb", (0.7, 0.88, 1.0))? } else { c.getattr("white")? })?;
        Ok(())
    }
}

// ---------------------------------------------------------------- Pill
/// 近くのもの（お店・操縦席など）のボタン：横長の札。
#[pyclass(module = "main.client.touch_ui", subclass, dict)]
pub struct Pill {
    #[pyo3(get, set)]
    w: f64,
    #[pyo3(get, set)]
    h: f64,
    #[pyo3(get, set)]
    root: PyObject,
    #[pyo3(get, set)]
    bg: PyObject,
    #[pyo3(get, set)]
    icon: PyObject,
    #[pyo3(get, set)]
    text: PyObject,
    #[pyo3(get, set)]
    gear_bg: PyObject,
    #[pyo3(get, set)]
    gear: PyObject,
    #[pyo3(get, set)]
    target: PyObject,
    #[pyo3(get, set)]
    x: f64,
    #[pyo3(get, set)]
    y: f64,
}

#[pymethods]
impl Pill {
    #[new]
    fn new(py: Python<'_>, hud: &B<'_>) -> R<Self> {
        let u = hud.getattr("U")?;
        let (entity, color) = (u.get_item("Entity")?, u.get_item("color")?);
        let (w, h) = (0.34, 0.052);
        let root = entity.call((), Some(&kw(py, vec![("parent", hud.getattr("root")?.unbind()), ("enabled", po(py, false))])?))?;
        let r_ = || root.clone().unbind();
        let dark = || -> R<PyObject> { Ok(color.call_method1("rgba", (0.08, 0.1, 0.14, 0.78))?.unbind()) };
        let bg = entity.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, (w, h))), ("color", dark()?)])?))?;
        let icon = entity.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, h * 0.85)), ("x", po(py, -w / 2.0 + h * 0.55)),
            ("texture", ui_texture(py, "interact")?.unbind())])?))?;
        let text = u.get_item("Text")?.call((), Some(&kw(py, vec![("parent", r_()), ("text", po(py, "")), ("origin", po(py, (-0.5, 0))), ("x", po(py, -w / 2.0 + h * 1.15)),
            ("scale", po(py, 0.68))])?))?;
        // 右端の歯車：その機器の設定（P）
        let gear_bg = entity.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, (h, h))), ("x", po(py, w / 2.0 + h * 0.55)),
            ("color", dark()?)])?))?;
        let gear = entity.call((), Some(&kw(py, vec![("parent", r_()), ("model", po(py, "quad")), ("scale", po(py, h * 0.8)), ("x", po(py, w / 2.0 + h * 0.55)),
            ("texture", ui_texture(py, "settings")?.unbind())])?))?;
        Ok(Pill { w, h, root: root.unbind(), bg: bg.unbind(), icon: icon.unbind(), text: text.unbind(), gear_bg: gear_bg.unbind(), gear: gear.unbind(),
            target: py.None(), x: 0.0, y: 0.0 })
    }
    fn set(&mut self, py: Python<'_>, target: &B<'_>, x: f64, y: f64, key_hint: &B<'_>) -> R<()> {
        self.target = target.clone().unbind();
        self.x = x;
        self.y = y;
        let root = self.root.bind(py);
        root.setattr("position", (x, y, 0))?;
        let ic = target.call_method1("get", ("icon", "interact"))?;
        self.icon.bind(py).setattr("texture", imp(py, "main.client.icons")?.call_method1("ui_texture", (ic,))?)?;
        let fmt = py.import_bound("builtins")?.getattr("format")?;
        let mut txt: String = fmt.call1((target.get_item("label")?,))?.extract()?;
        if key_hint.is_truthy()? {
            txt += &format!("  [{}]", fmt.call1((key_hint,))?.extract::<String>()?);
        }
        let t = self.text.bind(py);
        if !t.getattr("text")?.eq(&txt)? {
            t.setattr("text", &txt)?;
        }
        let on = target.call_method1("get", ("settings",))?.is_truthy()?;
        self.gear_bg.bind(py).setattr("enabled", on)?;
        self.gear.bind(py).setattr("enabled", on)?;
        root.setattr("enabled", true)?;
        Ok(())
    }
    fn hide(&mut self, py: Python<'_>) -> R<()> {
        self.root.bind(py).setattr("enabled", false)?;
        self.target = py.None();
        Ok(())
    }
    /// 押した所："main"（使う）・"gear"（設定）・None。
    fn hit(&self, py: Python<'_>, x: f64, y: f64) -> R<Option<&'static str>> {
        if !self.root.bind(py).getattr("enabled")?.is_truthy()? || (y - self.y).abs() > self.h / 2.0 {
            return Ok(None);
        }
        if (x - self.x).abs() <= self.w / 2.0 {
            return Ok(Some("main"));
        }
        let gx = self.x + self.w / 2.0 + self.h * 0.55;
        if self.gear.bind(py).getattr("enabled")?.is_truthy()? && (x - gx).abs() <= self.h / 2.0 {
            return Ok(Some("gear"));
        }
        Ok(None)
    }
}

// ---------------------------------------------------------------- ボタンの操作
/// ボタンを押したときの操作（0：設定のキー、1：そのままのキー、2：降りる、3：設定を開く）
#[pyclass(module = "alpedia_core", name = "_TouchTap")]
pub struct TouchTap {
    kind: u8,
    g: PyObject,
    k: PyObject,
}
#[pymethods]
impl TouchTap {
    fn __call__(&self, py: Python<'_>) -> R<()> {
        let g = self.g.bind(py);
        match self.kind {
            0 => {
                let c = g.getattr("cfg")?.get_item("controls")?;
                g.call_method1("input", (c.call_method1("get", (self.k.bind(py), self.k.bind(py)))?,))?;
            }
            1 => {
                g.call_method1("input", (self.k.bind(py),))?;
            }
            2 => {
                g.getattr("world")?.call_method1("interact", (py.None(),))?;
            }
            _ => {
                g.call_method0("open_editor")?;
            }
        }
        Ok(())
    }
}
fn tap(py: Python<'_>, kind: u8, g: &B<'_>, k: &str) -> R<PyObject> {
    Ok(Py::new(py, TouchTap { kind, g: g.clone().unbind(), k: po(py, k) })?.into_any())
}

// ---------------------------------------------------------------- TouchHUD
#[pyclass(module = "main.client.touch_ui", subclass, dict)]
pub struct TouchHUD {}

fn win<'py>(s: &B<'py>) -> R<B<'py>> {
    s.getattr("U")?.get_item("window")
}
fn wx(w: &B<'_>, side: &str, c: &str) -> R<f64> {
    f64of(&w.getattr(side)?.getattr(c)?)
}

#[pymethods]
impl TouchHUD {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        TouchHUD {}
    }
    fn __init__(slf: &Bound<'_, Self>, game: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("game", game)?;
        let u = game.getattr("U")?;
        s.setattr("U", &u)?;
        let (entity, color) = (u.get_item("Entity")?, u.get_item("color")?);
        s.setattr("root", entity.call((), Some(&kw(py, vec![("parent", game.getattr("ui_root")?.unbind())])?))?)?;
        s.setattr("ptr", Py::new(py, TPointers::new(py, game.clone().unbind()))?)?;
        let g = game;
        let bt = py.get_type_bound::<Btn>();
        let b = |bid: &str, icon: &str, label: &str, r: f64, kind: &str, key: Option<&str>, on_tap: Option<PyObject>| -> R<B<'_>> {
            let k = PyDict::new_bound(py);
            k.set_item("kind", kind)?;
            k.set_item("key", key)?;
            k.set_item("on_tap", on_tap)?;
            bt.call((s, bid, icon, label, r), Some(&k))
        };
        let top = PyList::empty_bound(py);
        for (id, label, ot) in [("menu", "メニュー", tap(py, 1, g, "escape")?), ("inventory", "持ち物", tap(py, 0, g, "inventory")?),
            ("map", "マップ", tap(py, 0, g, "city_map")?), ("marker", "目印", tap(py, 0, g, "city_marker")?), ("view", "視点", tap(py, 0, g, "third_person")?),
            ("camera", "カメラ", g.getattr("enter_camera")?.unbind()), ("import", "読み込み", tap(py, 0, g, "import")?),
            ("export", "出力", tap(py, 0, g, "export")?), ("save", "保存", tap(py, 0, g, "save")?)] {
            top.append(b(id, id, label, SMALL, "tap", None, Some(ot))?)?;
        }
        s.setattr("top", top)?;
        let act = PyDict::new_bound(py);
        act.set_item("jump", b("jump", "jump", "ジャンプ", BIG, "hold", Some("jump"), None)?)?;
        act.set_item("down", b("down", "down", "下へ", MID, "hold", Some("down"), None)?)?;
        act.set_item("jetpack", b("jetpack", "jetpack", "ジェット", MID, "tap", None, Some(tap(py, 0, g, "jetpack")?))?)?;
        act.set_item("remove", b("remove", "remove", "掘る・壊す", MID, "repeat", None, Some(g.getattr("do_remove")?.unbind()))?)?;
        act.set_item("place", b("place", "place", "置く", MID, "repeat", None, Some(g.getattr("do_place")?.unbind()))?)?;
        act.set_item("rotate", b("rotate", "rotate", "向き", SMALL, "tap", None, Some(tap(py, 0, g, "rotate")?))?)?;
        act.set_item("new", b("new", "new", "空中に置く", SMALL, "tap", None, Some(tap(py, 0, g, "new_construct")?))?)?;
        act.set_item("exit", b("exit", "exit", "降りる", MID, "tap", None, Some(tap(py, 2, g, "")?))?)?;
        // 照準の先の物を使う（E）・設定（P）。見ている物が使えるときだけ出る
        act.set_item("use", b("use", "interact", "使う", MID, "tap", None, Some(tap(py, 0, g, "interact")?))?)?;
        act.set_item("settings", b("settings", "settings", "設定", SMALL, "tap", None, Some(tap(py, 3, g, "")?))?)?;
        s.setattr("act", act)?;
        s.setattr("seat_btns", PyDict::new_bound(py))?; // 操縦席：機器に割り当てたキー → ボタン
        let pills = PyList::empty_bound(py);
        for _ in 0..4 {
            pills.append(Py::new(py, Pill::new(py, s)?)?)?;
        }
        s.setattr("pills", pills)?;
        s.setattr("targets", PyList::empty_bound(py))?;
        s.setattr("_t_scan", 0.0)?;
        // ジョイスティック
        s.setattr("stick_bg", entity.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind()), ("model", po(py, "circle")), ("scale", po(py, STICK_R * 2.0)),
            ("color", color.call_method1("rgba", (0, 0, 0, 0.28))?.unbind()), ("enabled", po(py, false))])?))?)?;
        s.setattr("stick_knob", entity.call((), Some(&kw(py, vec![("parent", s.getattr("root")?.unbind()), ("model", po(py, "circle")), ("scale", po(py, STICK_R * 0.8)),
            ("color", color.call_method1("rgba", (1, 1, 1, 0.45))?.unbind()), ("enabled", po(py, false))])?))?)?;
        s.setattr("stick_home", (0.0, 0.0))?;
        let np = imp(py, "numpy")?;
        s.setattr("stick", np.call_method1("zeros", (2,))?)?; // -1..1（右, 前）
        s.setattr("look", np.call_method1("zeros", (2,))?)?; // なぞった量（画面の高さ = 1）
        s.setattr("vkeys", PyDict::new_bound(py))?; // ボタンで押しているキー（押している間 1）
        s.setattr("own", PyDict::new_bound(py))?; // 指 → 何に使っているか
        s.setattr("visible", true)?;
        Ok(())
    }

    // ------------------------------------------------------------------
    #[getter]
    fn touch_mode(slf: &Bound<'_, Self>) -> R<bool> {
        slf.as_any().getattr("game")?.getattr("cfg")?.get_item("controls")?.call_method1("get", ("touch", false))?.is_truthy()
    }
    fn key_hint(slf: &Bound<'_, Self>, action: &B<'_>) -> R<PyObject> {
        let py = slf.py();
        if Self::touch_mode(slf)? {
            return Ok(po(py, ""));
        }
        let k = slf.as_any().getattr("game")?.getattr("cfg")?.get_item("controls")?.call_method1("get", (action, ""))?;
        Ok(key_name(py, &k)?.unbind())
    }
    fn over_ui(slf: &Bound<'_, Self>, x: f64, y: f64) -> R<bool> {
        let py = slf.py();
        for b in Self::_all_btns(slf)? {
            if b.borrow().hit(x, y) {
                return Ok(true);
            }
        }
        for p in slf.as_any().getattr("pills")?.iter()? {
            if p?.downcast::<Pill>()?.borrow().hit(py, x, y)?.is_some() {
                return Ok(true);
            }
        }
        Ok(Self::touch_mode(slf)? && Self::_in_stick_zone(slf, x, y)?)
    }
    fn mouse_over_ui(slf: &Bound<'_, Self>) -> R<bool> {
        let py = slf.py();
        let r = (|| -> R<bool> {
            let m = slf.as_any().getattr("ptr")?.call_method0("mouse")?;
            Self::over_ui(slf, f64of(&m.get_item(0)?)?, f64of(&m.get_item(1)?)?)
        })();
        match r {
            Ok(v) => Ok(v),
            Err(e) if e.is_instance_of::<PyException>(py) => Ok(false),
            Err(e) => Err(e),
        }
    }
    fn _all_btns<'py>(slf: &Bound<'py, Self>) -> R<Vec<Bound<'py, Btn>>> {
        let s = slf.as_any();
        let mut out = vec![];
        for b in s.getattr("top")?.iter()? {
            out.push(b?.downcast_into::<Btn>()?);
        }
        for b in s.getattr("act")?.call_method0("values")?.iter()? {
            out.push(b?.downcast_into::<Btn>()?);
        }
        for b in s.getattr("seat_btns")?.call_method0("values")?.iter()? {
            out.push(b?.downcast_into::<Btn>()?);
        }
        Ok(out)
    }
    fn _in_stick_zone(slf: &Bound<'_, Self>, x: f64, y: f64) -> R<bool> {
        let w = win(slf.as_any())?;
        Ok(x < wx(&w, "left", "x")? + 0.62 && y < 0.05)
    }

    // ------------------------------------------------------------------
    fn layout(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let g = s.getattr("game")?;
        let w = g.getattr("world")?;
        let wn = win(s)?;
        let (right, top) = (wx(&wn, "right", "x")?, wx(&wn, "top", "y")?);
        let player = w.getattr("player")?;
        let seated = !player.getattr("seated")?.is_none();
        let city = w.call_method1("addon", ("city",))?;
        let has_city = !city.is_none() && city.getattr("enabled").map(|e| e.is_truthy()).unwrap_or(Ok(false))?;
        let net = w.getattr("net")?;
        if !net.is_none() {
            net.call_method0("can_save")?;
        }
        let touch = Self::touch_mode(slf)?;
        // 読み込み・出力・保存は一時停止メニューの中（F8・F9・F5 のキーはそのまま使える）
        let show = |id: &str| match id {
            "menu" | "view" | "camera" => true,
            "inventory" => !seated,
            "map" | "marker" => has_city,
            _ => false,
        };
        fn hint(id: &str) -> &str {
            match id {
            "menu" => "Esc",
            "map" => "city_map",
            "marker" => "city_marker",
            "view" => "third_person",
            "camera" => "camera_mode",
            x => x,
        }
        }
        let hs = |a: &str| -> R<String> { Ok(Self::key_hint(slf, &po(py, a).into_bound(py))?.bind(py).str()?.to_string()) };
        let mut x = right - 0.045;
        for b in s.getattr("top")?.iter()? {
            let b = b?.downcast_into::<Btn>()?;
            let id: String = b.borrow().id.bind(py).extract()?;
            if !show(&id) {
                b.borrow_mut().place(py, 0.0, 0.0, false, None)?;
                continue;
            }
            let h = if id == "menu" && !touch { "Esc".to_string() } else { hs(hint(&id))? };
            let label = b.borrow().label.bind(py).str()?.to_string();
            let txt = label + &if !h.is_empty() && id != "menu" { format!(" {}", h) } else if !h.is_empty() { " Esc".to_string() } else { String::new() };
            b.borrow_mut().place(py, x, top - 0.045, true, Some(&po(py, txt).into_bound(py)))?;
            x -= SMALL * 2.0 + 0.03;
        }
        // 右下の操作
        let (bx, by) = (right - 0.1, -0.36);
        let a = s.getattr("act")?;
        let jet = player.getattr("jetpack")?.is_truthy()?;
        let flying = jet || f64of(&player.getattr("in_water")?)? > 0.3;
        let h = g.getattr("hit")?;
        let aim_use = touch && h.is_truthy()? && g.call_method1("_usable", (&h,))?.is_truthy()?; // キーボードでは E・P があるので出さない
        let aim_set = touch && h.is_truthy()? && h.call_method1("get", ("kind",))?.eq("voxel")?
            && h.get_item("construct")?.getattr("devs")?.get_item(h.get_item("index")?)?.is_truthy()?;
        let ns = !seated;
        let lay: [(&str, f64, f64, bool); 10] = [
            ("jump", bx, by, ns), ("down", bx, by + 0.15, ns && flying),
            ("jetpack", bx - 0.02, if flying { by + 0.27 } else { by + 0.15 }, ns),
            ("remove", bx - 0.13, by + 0.07, ns), ("place", bx - 0.15, by - 0.06, ns),
            ("rotate", bx - 0.25, by + 0.02, ns), ("new", bx - 0.25, by - 0.08, ns),
            ("exit", bx, by, seated),
            ("use", bx - 0.13, by + 0.2, ns && aim_use),
            ("settings", bx - 0.25, by + 0.14, ns && aim_set),
        ];
        fn keys(k: &str) -> Option<&str> {
            match k {
            "jump" | "down" | "jetpack" | "rotate" => Some(k),
            "new" => Some("new_construct"),
            "exit" | "use" => Some("interact"),
            "settings" => Some("edit_script"),
            _ => None,
        }
        }
        for (k, xx, yy, vis) in lay {
            let h = match keys(k) {
                Some(a) => hs(a)?,
                None if touch => String::new(),
                None => match k {
                    "remove" => "左クリック".into(),
                    "place" => "右クリック".into(),
                    _ => String::new(),
                },
            };
            let b = a.get_item(k)?.downcast_into::<Btn>()?;
            let label = b.borrow().label.bind(py).str()?.to_string();
            let txt = label + &if h.is_empty() { String::new() } else { format!(" {}", h) };
            b.borrow_mut().place(py, xx, yy, vis, Some(&po(py, txt).into_bound(py)))?;
        }
        let jb = a.get_item("jetpack")?.downcast_into::<Btn>()?;
        if jb.borrow().visible {
            let on = if player.getattr("jetpack")?.is_truthy()? { "ON" } else { "OFF" };
            let hk = hs("jetpack")?;
            jb.borrow().text.bind(py).setattr("text", format!("ジェット {}", on) + &if hk.is_empty() { String::new() } else { format!(" {}", hk) })?;
        }
        Self::_layout_seat(slf, seated, bx, by)
    }
    /// 操縦席：機器に割り当てたキーのボタン（押している間 そのキーを押したことになる）。
    fn _layout_seat(slf: &Bound<'_, Self>, seated: bool, bx: f64, by: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let mut want: Vec<(String, Vec<String>)> = vec![];
        fn add(want: &mut Vec<(String, Vec<String>)>, k: &str, name: Option<String>) {
            let i = match want.iter().position(|(kk, _)| kk == k) {
                Some(i) => i,
                None => {
                    want.push((k.to_string(), vec![]));
                    want.len() - 1
                }
            };
            if let Some(n) = name {
                want[i].1.push(n);
            }
        }
        if seated {
            let w = s.getattr("game")?.getattr("world")?;
            let c = w.getattr("constructs")?.call_method1("get", (w.getattr("player")?.getattr("seated")?.get_item(0)?,))?;
            if !c.is_none() {
                let reg = c.getattr("reg")?;
                for d in c.getattr("devs")?.iter()? {
                    let d = d?;
                    if !d.is_truthy()? {
                        continue;
                    }
                    let dd = reg.call_method1("device", (d.get_item("def")?,))?;
                    let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
                    let pr = dd.call_method1("get", ("params",))?;
                    let pr = if pr.is_truthy()? { pr } else { PyDict::new_bound(py).into_any() };
                    let mut k = pr.call_method1("get", ("key",))?;
                    if dd.call_method1("get", ("type",))?.eq("actuator")? {
                        let pre = d.get_item("state")?.call_method1("get", ("preset",))?;
                        let pre = if pre.is_truthy()? { pre } else { PyDict::new_bound(py).into_any() };
                        if pre.call_method1("get", ("action",))?.eq("none")? {
                            k = py.None().into_bound(py); // 動作「なし」のアクチュエーターはキーで動かさない
                        }
                    }
                    if k.is_truthy()? {
                        add(&mut want, &k.extract::<String>()?, Some(d.get_item("name")?.str()?.to_string()));
                    }
                }
                for k in Self::_program_keys(slf, &c)? {
                    add(&mut want, &k, None); // プログラムが読むキー（ドローンの X など）
                }
            }
            for k in ["space", "left control", "left shift"] {
                add(&mut want, k, None);
            }
        }
        let sb = s.getattr("seat_btns")?.downcast_into::<PyDict>()?;
        let old: Vec<String> = sb.keys().iter().map(|k| k.extract()).collect::<R<_>>()?;
        for k in old {
            if !want.iter().any(|(kk, _)| *kk == k) {
                let b = sb.call_method1("pop", (&k,))?;
                super::client_misc::ui_destroy(py, &b.getattr("root")?, &PyTuple::empty_bound(py), None)?;
            }
        }
        want.sort_by(|a, b| (a.1.is_empty(), &a.0).cmp(&(b.1.is_empty(), &b.0)));
        for (n, (k, names)) in want.iter().enumerate() {
            let kn = key_name_s(k);
            let mut label = match names.first() {
                Some(n0) => n0.clone(),
                None => seat_label(k).map(|x| x.to_string()).unwrap_or_else(|| kn.clone()),
            };
            if names.len() > 1 {
                label += &format!(" 他{}", names.len() - 1);
            }
            let mut label: String = label.chars().take(8).collect();
            if !names.is_empty() || seat_label(k).is_some() {
                label += &format!(" {}", kn);
            }
            let b = match sb.get_item(k)? {
                Some(b) => b,
                None => {
                    let icon = match k.as_str() {
                        "space" => "jump".to_string(),
                        "left control" | "left shift" => "down".to_string(),
                        _ => format!("key:{}", kn),
                    };
                    let kk = PyDict::new_bound(py);
                    kk.set_item("kind", "hold")?;
                    kk.set_item("key", k)?;
                    kk.set_item("press_key", true)?;
                    let b = py.get_type_bound::<Btn>().call((s, format!("seat:{}", k), icon, &label, MID), Some(&kk))?;
                    sb.set_item(k, &b)?;
                    b
                }
            };
            let (col, row) = ((n % 3) as f64, (n / 3) as f64);
            b.downcast_into::<Btn>()?.borrow_mut().place(py, bx - 0.13 * (col + 1.0) + 0.02 * row, by + 0.12 * row + 0.04 * col, true,
                Some(&po(py, label).into_bound(py)))?;
        }
        Ok(())
    }
    /// 構築物のコンピューターのプログラムが読んでいるキー（ブロックの「キー □」・Python の "x" in api.input など）。
    fn _program_keys(slf: &Bound<'_, Self>, c: &B<'_>) -> R<Vec<String>> {
        let py = slf.py();
        let s = slf.as_any();
        let w = s.getattr("game")?.getattr("world")?;
        let mut out: Vec<String> = vec![];
        let cache = match s.getattr("_keys_cache") {
            Ok(x) => x,
            Err(_) => {
                let d = PyDict::new_bound(py).into_any();
                s.setattr("_keys_cache", &d)?;
                d
            }
        };
        let reg = c.getattr("reg")?;
        for d in c.getattr("devs")?.iter()? {
            let d = d?;
            if !d.is_truthy()? {
                continue;
            }
            let dd = reg.call_method1("device", (d.get_item("def")?,))?;
            let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
            if !dd.call_method1("get", ("type",))?.eq("computer")? {
                continue;
            }
            let st = d.call_method1("get", ("state", PyDict::new_bound(py)))?;
            let bl = st.call_method1("get", ("blocks",))?;
            let name = if bl.is_truthy()? { bl } else { st.call_method1("get", ("script",))? };
            if !name.is_truthy()? {
                continue;
            }
            let path = w.getattr("dir")?.call_method1("__truediv__", ("scripts",))?.call_method1("__truediv__", (&name,))?;
            let mt = match path.call_method0("stat") {
                Ok(st) => st.getattr("st_mtime")?,
                Err(e) if e.is_instance_of::<PyOSError>(py) => continue,
                Err(e) => return Err(e),
            };
            let ps = path.str()?;
            let got = cache.call_method1("get", (&ps,))?;
            let got = if got.is_none() || !got.get_item(0)?.eq(&mt)? {
                let keys = PyList::empty_bound(py);
                let r = (|| -> R<()> {
                    let k = PyDict::new_bound(py);
                    k.set_item("encoding", "utf-8")?;
                    let text = path.call_method("read_text", (), Some(&k))?;
                    if name.call_method1("endswith", (".json",))?.is_truthy()? {
                        fn walk(x: &B<'_>, keys: &Bound<'_, PyList>) -> R<()> {
                            if let Ok(d) = x.downcast::<PyDict>() {
                                let t = d.call_method1("get", ("type",))?;
                                let kk = d.call_method1("get", ("key",))?;
                                if (t.eq("on_key")? || t.eq("on_key_down")? || t.eq("key")?) && kk.is_instance_of::<PyString>() {
                                    keys.append(d.get_item("key")?)?;
                                }
                                for v in d.values().iter() {
                                    walk(&v, keys)?;
                                }
                            } else if let Ok(l) = x.downcast::<PyList>() {
                                for v in l.iter() {
                                    walk(&v, keys)?;
                                }
                            }
                            Ok(())
                        }
                        walk(&imp(py, "json")?.call_method1("loads", (&text,))?, &keys)?;
                    } else {
                        let found = imp(py, "re")?.call_method1("findall", (
                            r#"["']([a-z0-9]|space|left shift|left control|up arrow|down arrow|left arrow|right arrow)["']\s+in\s+api\.input"#, &text))?;
                        keys.call_method1("extend", (found,))?;
                    }
                    Ok(())
                })();
                let keys = match r {
                    Ok(()) => keys,
                    Err(e) if e.is_instance_of::<PyException>(py) => PyList::empty_bound(py),
                    Err(e) => return Err(e),
                };
                let t = PyTuple::new_bound(py, [mt.unbind(), keys.into_any().unbind()]);
                cache.set_item(&ps, &t)?;
                t.into_any()
            } else {
                got
            };
            for k in got.get_item(1)?.iter()? {
                let k: String = k?.extract()?;
                if !out.contains(&k) && !["w", "a", "s", "d"].contains(&k.as_str()) {
                    out.push(k); // W/A/S/D はスティック
                }
            }
        }
        Ok(out)
    }

    // ------------------------------------------------------------------
    fn update(slf: &Bound<'_, Self>, _dt: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let g = s.getattr("game")?;
        let active = g.getattr("editor")?.is_none() && !g.getattr("paused")?.is_truthy()?;
        if !s.getattr("visible")?.eq(active)? {
            s.setattr("visible", active)?;
            s.getattr("root")?.setattr("enabled", active)?;
            if !active {
                Self::_release_all(slf)?;
            }
        }
        if !active {
            return Ok(());
        }
        Self::layout(slf)?;
        let now = mono(py)?;
        if now - f64of(&s.getattr("_t_scan")?)? > 0.2 {
            s.setattr("_t_scan", now)?;
            s.setattr("targets", g.call_method0("nearby_targets")?)?;
            Self::_layout_pills(slf)?;
        }
        Self::_pointers(slf)?;
        Self::_draw_stick(slf)
    }
    fn _layout_pills(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let x = wx(&win(s)?, "right", "x")? - 0.26;
        let key = Self::key_hint(slf, &po(py, "interact").into_bound(py))?.into_bound(py);
        let pkey = Self::key_hint(slf, &po(py, "edit_script").into_bound(py))?.into_bound(py);
        let targets = s.getattr("targets")?;
        let n = targets.len()?;
        let empty = po(py, "").into_bound(py);
        for (k, p) in s.getattr("pills")?.iter()?.enumerate() {
            let p = p?.downcast_into::<Pill>()?;
            if k < n {
                let t = targets.get_item(k)?;
                let hint = if k == 0 || t.call_method1("get", ("kind",))?.eq("p")? {
                    if t.call_method1("get", ("kind", "e"))?.eq("e")? { &key } else { &pkey }
                } else {
                    &empty
                };
                p.borrow_mut().set(py, &t, x, 0.12 - k as f64 * 0.065, hint)?;
            } else {
                p.borrow_mut().hide(py)?;
            }
        }
        Ok(())
    }
    fn _pointers(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let g = s.getattr("game")?;
        let pts = s.getattr("ptr")?.call_method0("current")?;
        let touch = Self::touch_mode(slf)?;
        let own = s.getattr("own")?.downcast_into::<PyDict>()?;
        // 離した指
        let gone: Vec<B<'_>> = own.keys().iter().filter(|p| !pts.contains(p).unwrap_or(false)).collect();
        for pid in gone {
            let o = own.call_method1("pop", (&pid,))?;
            Self::_release(slf, &pid, &o, &pts)?;
        }
        for it in pts.call_method0("items")?.iter()? {
            let it = it?;
            let pid = it.get_item(0)?;
            let xy = it.get_item(1)?;
            let (x, y) = (f64of(&xy.get_item(0)?)?, f64of(&xy.get_item(1)?)?);
            let o = match own.get_item(&pid)? {
                Some(o) => o,
                None => {
                    let o = Self::_grab(slf, &pid, x, y, touch)?;
                    own.set_item(&pid, o)?;
                    continue;
                }
            };
            let kind: String = o.get_item(0)?.extract()?;
            let now = mono(py)?;
            if kind == "btn" {
                let b = o.get_item(1)?.downcast_into::<Btn>()?;
                let hit = b.borrow().hit(x, y);
                b.borrow().show_pressed(py, hit)?;
                if b.borrow().kind == "repeat" && b.borrow().hit(x, y) && now - f64of(&o.get_item(2)?)? >= HOLD_TIME && now >= f64of(&o.get_item(3)?)? {
                    o.set_item(3, now + REPEAT)?; // 長押し：くり返す
                    o.set_item(4, true)?;
                    Self::_tap(slf, b.as_any())?;
                }
            } else if kind == "stick" {
                let home = s.getattr("stick_home")?;
                let (hx, hy) = (f64of(&home.get_item(0)?)?, f64of(&home.get_item(1)?)?);
                let np = imp(py, "numpy")?;
                let v = np.call_method1("array", (vec![x - hx, y - hy],))?.call_method1("__truediv__", (STICK_R,))?;
                let n = f64of(&np.getattr("linalg")?.call_method1("norm", (&v,))?)?;
                s.setattr("stick", if n > 1.0 { v.call_method1("__truediv__", (n,))? } else { v })?;
            } else if kind == "look" {
                let l = o.get_item(1)?;
                let (lx, ly) = (f64of(&l.get_item(0)?)?, f64of(&l.get_item(1)?)?);
                let s0 = o.get_item(2)?;
                let (sx, sy) = (f64of(&s0.get_item(0)?)?, f64of(&s0.get_item(1)?)?);
                if !o.get_item(4)?.is_truthy()? && (x - sx).powi(2) + (y - sy).powi(2) > MOVE_TOL.powi(2) {
                    o.set_item(4, true)?; // 動いた：なぞって視点を回す（長押しにはしない）
                }
                if o.get_item(4)?.is_truthy()? {
                    s.getattr("look")?.call_method1("__iadd__", ((x - lx, y - ly),))?;
                } else if now - f64of(&o.get_item(3)?)? >= HOLD_TIME && now >= f64of(&o.get_item(5)?)? {
                    o.set_item(5, now + REPEAT)?; // 長押し：触っている所を壊す・掘る（くり返す）
                    o.set_item(6, true)?;
                    Self::_world_touch(slf, "hold", x, y)?;
                }
                o.set_item(1, (x, y))?;
            }
        }
        let mut any_stick = false;
        for o in own.values().iter() {
            if o.get_item(0)?.eq("stick")? {
                any_stick = true;
            }
        }
        if !any_stick {
            s.getattr("stick")?.set_item(imp(py, "builtins")?.getattr("slice")?.call1((py.None(),))?, 0)?;
        }
        g.setattr("vkeys", s.getattr("vkeys")?)?;
        Ok(())
    }
    fn _grab<'py>(slf: &Bound<'py, Self>, _pid: &B<'py>, x: f64, y: f64, touch: bool) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        for b in Self::_all_btns(slf)? {
            if b.borrow().hit(x, y) {
                b.borrow_mut().pressed = true;
                b.borrow().show_pressed(py, true)?;
                if b.borrow().kind == "hold" {
                    let bk = b.borrow().key.clone_ref(py).into_bound(py);
                    let g = s.getattr("game")?;
                    let k = g.getattr("cfg")?.get_item("controls")?.call_method1("get", (&bk, &bk))?;
                    s.getattr("vkeys")?.set_item(&k, 1)?;
                    if b.borrow().press_key {
                        g.call_method1("input", (&k,))?;
                    }
                }
                let now = mono(py)?;
                return Ok(PyList::new_bound(py, [po(py, "btn"), b.into_any().unbind(), po(py, now), po(py, now + HOLD_TIME), po(py, false)]).into_any());
            }
        }
        for p in s.getattr("pills")?.iter()? {
            let p = p?.downcast_into::<Pill>()?;
            let h = p.borrow().hit(py, x, y)?;
            if let Some(h) = h {
                let t = p.borrow().target.clone_ref(py);
                return Ok(PyList::new_bound(py, [po(py, "pill"), p.into_any().unbind(), t, po(py, h == "gear")]).into_any());
            }
        }
        if touch && Self::_in_stick_zone(slf, x, y)? {
            let w = win(s)?;
            let (l, b) = (wx(&w, "left", "x")?, wx(&w, "bottom", "y")?);
            let hx = pmin(pmax(x, l + STICK_R + 0.03), l + 0.62 - STICK_R);
            let hy = pmin(pmax(y, b + STICK_R + 0.03), 0.05 - STICK_R);
            s.setattr("stick_home", (hx, hy))?;
            return Ok(PyList::new_bound(py, [po(py, "stick")]).into_any());
        }
        if touch {
            let now = mono(py)?;
            // [種類, 前の位置, 触り始めの位置, 触り始めの時刻, 動いたか, 次の長押しの時刻, 長押しをしたか]
            return Ok(PyList::new_bound(py, [po(py, "look"), po(py, (x, y)), po(py, (x, y)), po(py, now), po(py, false), po(py, now + HOLD_TIME), po(py, false)]).into_any());
        }
        Ok(PyList::new_bound(py, [po(py, "none")]).into_any())
    }
    fn _tap(slf: &Bound<'_, Self>, b: &B<'_>) -> R<()> {
        let py = slf.py();
        let r = b.getattr("on_tap")?.call0().map(|_| ());
        guard(py, &slf.as_any().getattr("game")?, r, "ボタン %s の操作でエラー", vec![b.getattr("id")?.unbind()])
    }
    fn _world_touch(slf: &Bound<'_, Self>, kind: &str, x: f64, y: f64) -> R<()> {
        let py = slf.py();
        let g = slf.as_any().getattr("game")?;
        let r = g.call_method1("touch_action", (kind, x, y)).map(|_| ());
        guard(py, &g, r, "タッチの操作でエラー", vec![])
    }
    fn _release(slf: &Bound<'_, Self>, _pid: &B<'_>, o: &B<'_>, _pts: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let kind: String = o.get_item(0)?.extract()?;
        if kind == "btn" {
            let b = o.get_item(1)?.downcast_into::<Btn>()?;
            b.borrow().show_pressed(py, false)?;
            b.borrow_mut().pressed = false;
            let bk = b.borrow().key.clone_ref(py).into_bound(py);
            if b.borrow().kind == "hold" {
                let k = s.getattr("game")?.getattr("cfg")?.get_item("controls")?.call_method1("get", (&bk, &bk))?;
                let mut other = false;
                for x in s.getattr("own")?.call_method0("values")?.iter()? {
                    let x = x?;
                    if x.get_item(0)?.eq("btn")? {
                        let xb = x.get_item(1)?;
                        if !xb.is(b.as_any()) && xb.getattr("key")?.eq(&bk)? {
                            other = true;
                            break;
                        }
                    }
                }
                if !other {
                    s.getattr("vkeys")?.call_method1("pop", (k, py.None()))?;
                }
            } else if !b.borrow().on_tap.is_none(py) && !(b.borrow().kind == "repeat" && o.get_item(4)?.is_truthy()?) {
                Self::_tap(slf, b.as_any())?; // 長押しでくり返したあとは、離したときにはもうしない
            }
        } else if kind == "look" {
            if !o.get_item(4)?.is_truthy()? && !o.get_item(6)?.is_truthy()? && mono(py)? - f64of(&o.get_item(3)?)? <= TAP_TIME {
                let p = o.get_item(1)?;
                Self::_world_touch(slf, "tap", f64of(&p.get_item(0)?)?, f64of(&p.get_item(1)?)?)?; // タップ：触った物を使う（使えなければ置く）
            }
        } else if kind == "pill" && !o.get_item(2)?.is_none() {
            let g = s.getattr("game")?;
            let k = PyDict::new_bound(py);
            k.set_item("settings", o.get_item(3)?)?;
            let r = g.call_method("do_target", (o.get_item(2)?,), Some(&k)).map(|_| ());
            guard(py, &g, r, "近くのものの操作でエラー", vec![])?;
        }
        Ok(())
    }
    fn _release_all(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let own = s.getattr("own")?;
        for o in own.call_method0("values")?.iter()? {
            let o = o?;
            if o.get_item(0)?.eq("btn")? {
                let b = o.get_item(1)?.downcast_into::<Btn>()?;
                b.borrow().show_pressed(py, false)?;
                b.borrow_mut().pressed = false;
            }
        }
        own.call_method0("clear")?;
        s.getattr("vkeys")?.call_method0("clear")?;
        let all = imp(py, "builtins")?.getattr("slice")?.call1((py.None(),))?;
        s.getattr("stick")?.set_item(&all, 0)?;
        s.getattr("look")?.set_item(&all, 0)?;
        s.getattr("game")?.setattr("vkeys", s.getattr("vkeys")?)?;
        Ok(())
    }
    fn take_look<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let l = slf.as_any().getattr("look")?;
        let d = l.call_method0("copy")?;
        l.set_item(imp(py, "builtins")?.getattr("slice")?.call1((py.None(),))?, 0)?;
        Ok(d)
    }
    fn _draw_stick(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let on = Self::touch_mode(slf)?;
        s.getattr("stick_bg")?.setattr("enabled", on)?;
        s.getattr("stick_knob")?.setattr("enabled", on)?;
        if !on {
            return Ok(());
        }
        let w = win(s)?;
        let mut any_stick = false;
        for o in s.getattr("own")?.call_method0("values")?.iter()? {
            if o?.get_item(0)?.eq("stick")? {
                any_stick = true;
            }
        }
        if !any_stick {
            s.setattr("stick_home", (wx(&w, "left", "x")? + 0.2, wx(&w, "bottom", "y")? + 0.2))?;
        }
        let home = s.getattr("stick_home")?;
        let (hx, hy) = (f64of(&home.get_item(0)?)?, f64of(&home.get_item(1)?)?);
        s.getattr("stick_bg")?.setattr("position", (hx, hy, 0.01))?;
        let st = s.getattr("stick")?;
        let (s0, s1) = (f64of(&st.get_item(0)?)?, f64of(&st.get_item(1)?)?);
        s.getattr("stick_knob")?.setattr("position", (hx + s0 * STICK_R * 0.7, hy + s1 * STICK_R * 0.7, 0))?;
        Ok(())
    }
    fn destroy(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        super::client_misc::ui_destroy(py, &slf.as_any().getattr("root")?, &PyTuple::empty_bound(py), None)
    }
}

/// Python の max(a, b)（等しければ a）
fn pmax(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}
/// Python の min(a, b)（等しければ a）
fn pmin(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<TPointers>()?;
    m.add_class::<Btn>()?;
    m.add_class::<Pill>()?;
    m.add_class::<TouchTap>()?;
    m.add_class::<TouchHUD>()?;
    m.add_function(wrap_pyfunction_bound!(key_name, m)?)?;
    Ok(())
}
