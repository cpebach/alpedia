//! カメラモード（自撮り・建造物の撮影用）（main/client/camera_mode.py）。「写真を保存」で、パネルを隠した画面を photos/ に PNG で保存する。
//!
//! - 自分の体を三人称で映し、カメラは自分のまわりを回る（ドラッグ・なぞる）、ホイール／ピンチで距離、
//!   WASD・スティックで注目する所を動かす、Space・Ctrl で上下
//! - パネル：画面の表示（UI）・ブロックの枠の表示、モーション、体の向き、視野角、傾き、距離、
//!   パネルを隠す（画面を1回タップ・クリックか H で戻る）、元に戻す、終わる
//! - 遊びは止めない（ほかの人・機械は動き続ける）。自分は動かない

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::imp;
use super::panels::{kw, partial, po, ursina};
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;
type V3 = [f64; 3];

fn lg(py: Python<'_>) -> R<B<'_>> {
    imp(py, "main.log")?.call_method1("get", ("camera",))
}
fn v3(o: &B<'_>) -> R<V3> {
    Ok([f64of(&o.get_item(0)?)?, f64of(&o.get_item(1)?)?, f64of(&o.get_item(2)?)?])
}
fn np3(py: Python<'_>, v: V3) -> B<'_> {
    numpy::PyArray1::from_vec_bound(py, v.to_vec()).into_any()
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: V3, eps: f64) -> V3 {
    let n = norm(a).max(eps);
    [a[0] / n, a[1] / n, a[2] / n]
}
/// 軸 axis のまわりに ang 回す行列
fn rot(axis: V3, ang: f64) -> [[f64; 3]; 3] {
    let a = unit(axis, 1e-12);
    let (x, y, z) = (a[0], a[1], a[2]);
    let k = [[0.0, -z, y], [z, 0.0, -x], [-y, x, 0.0]];
    let mut kk = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            kk[i][j] = k[i][0] * k[0][j] + k[i][1] * k[1][j] + k[i][2] * k[2][j];
        }
    }
    let (s, c) = (ang.sin(), 1.0 - ang.cos());
    let mut m = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = (if i == j { 1.0 } else { 0.0 }) + s * k[i][j] + c * kk[i][j];
        }
    }
    m
}
fn mv(m: [[f64; 3]; 3], v: V3) -> V3 {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}
fn pf(py: Python<'_>, v: f64, spec: &str) -> R<String> {
    py.import_bound("builtins")?.getattr("format")?.call1((v, spec))?.extract()
}
fn pymod(a: f64, b: f64) -> f64 {
    let m = a % b;
    if m != 0.0 && (m < 0.0) != (b < 0.0) { m + b } else { m }
}
fn fg(s: &B<'_>, k: &str) -> R<f64> {
    f64of(&s.getattr(k)?)
}

#[pyclass(module = "main.client.camera_mode", subclass, dict)]
pub struct CameraMode {}

impl CameraMode {
    fn local<'py>(s: &B<'py>) -> R<B<'py>> {
        s.getattr("game")?.getattr("world")?.getattr("local_player")
    }
    fn upn(p: &B<'_>) -> R<V3> {
        Ok(unit(v3(&p.getattr("up")?)?, 0.0))
    }
    /// 体の向き（base_fwd を上に垂直にして、yaw だけ回した物）
    fn fwd_yaw(s: &B<'_>, up: V3, yaw: f64) -> R<V3> {
        let b = v3(&s.getattr("base_fwd")?)?;
        let d = dot(b, up);
        let f = unit([b[0] - up[0] * d, b[1] - up[1] * d, b[2] - up[2] * d], 1e-9);
        Ok(mv(rot(up, -yaw), f))
    }
}

#[pymethods]
impl CameraMode {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        CameraMode {}
    }
    fn __init__(slf: &Bound<'_, Self>, game: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("game", game)?;
        let u = game.getattr("U")?;
        s.setattr("U", &u)?;
        let p = game.getattr("world")?.getattr("local_player")?;
        let lv = game.call_method0("look_vectors")?;
        s.setattr("up0", lv.get_item(2)?.call_method0("copy")?)?;
        s.setattr("yaw", std::f64::consts::PI)?; // 0 = 自分の後ろから。π = 正面から（自撮り）
        s.setattr("pitch", 0.12)?;
        s.setattr("dist", 3.2)?;
        s.setattr("height", 0.62)?; // 注目点の高さ（身長に対する割合）
        s.setattr("pan", imp(py, "numpy")?.call_method1("zeros", (3,))?)?;
        s.setattr("roll", 0.0)?;
        let fov0 = f64of(&u.get_item("camera")?.getattr("fov")?)?;
        s.setattr("fov0", fov0)?;
        s.setattr("fov", fov0)?;
        s.setattr("ui_on", false)?; // ゲームの画面の表示（HUD・アイテムバーなど）
        s.setattr("frame_on", false)?; // ブロックを置く枠・視線の先の枠
        s.setattr("panel_on", true)?;
        s.setattr("base_fwd", lv.get_item(0)?.call_method0("copy")?)?; // 体の向き（モードに入ったときの視線の向き）
        s.setattr("body_yaw", 0.0)?; // 体の向き（0 = モードに入ったときの向き＝カメラの方）
        let em = PyList::new_bound(py, ["（自動）"]);
        for e in game.getattr("emotes")?.iter()? {
            em.append(e?)?;
        }
        s.setattr("emotes", em)?;
        s.setattr("emote_i", 0)?;
        s.setattr("ptr_own", PyDict::new_bound(py))?;
        s.setattr("tap", py.None())?;
        s.setattr("prev_third", game.getattr("third_person")?)?;
        p.setattr("emote", py.None())?;
        game.getattr("ui_root")?.setattr("enabled", false)?;
        game.call_method1("set_mouse_capture", (false,))?;
        s.call_method0("_build")?;
        s.call_method0("_apply_face")?;
        lg(py)?.call_method1("info", ("カメラモード開始",))?;
        Ok(())
    }
    fn _build(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let ur = ursina(py)?;
        let (entity, text, button, color, camera, window) = (ur.getattr("Entity")?, ur.getattr("Text")?, ur.getattr("Button")?, ur.getattr("color")?,
            ur.getattr("camera")?, ur.getattr("window")?);
        let root = entity.call((), Some(&kw(py, vec![("parent", camera.getattr("ui")?.unbind())])?))?;
        s.setattr("root", &root)?;
        let x0 = f64of(&window.getattr("right")?.getattr("x")?)? - 0.2;
        let panel = entity.call((), Some(&kw(py, vec![("parent", root.clone().unbind())])?))?;
        s.setattr("panel", &panel)?;
        let pn = || panel.clone().unbind();
        let rgb = |r: f64, g: f64, b: f64| -> R<PyObject> { Ok(color.call_method1("rgb", (r, g, b))?.unbind()) };
        entity.call((), Some(&kw(py, vec![("parent", pn()), ("model", po(py, "quad")), ("position", po(py, (x0, 0.0, 0.02))), ("scale", po(py, (0.37, 0.9))),
            ("color", color.call_method1("rgba", (0.05, 0.06, 0.09, 0.72))?.unbind())])?))?;
        text.call((), Some(&kw(py, vec![("parent", pn()), ("text", po(py, "カメラモード")), ("position", po(py, (x0, 0.42))), ("origin", po(py, (0, 0))), ("scale", po(py, 1.1))])?))?;
        let rows = PyDict::new_bound(py);
        s.setattr("rows", &rows)?;
        let mut y = 0.35;
        let row = |key: &str, label: &str, fl: B<'_>, fr: Option<B<'_>>, y: &mut f64| -> R<()> {
            text.call((), Some(&kw(py, vec![("parent", pn()), ("text", po(py, label)), ("position", po(py, (x0 - 0.17, *y))), ("origin", po(py, (-0.5, 0))), ("scale", po(py, 0.7))])?))?;
            let t = text.call((), Some(&kw(py, vec![("parent", pn()), ("text", po(py, "")), ("position", po(py, (x0 + 0.07, *y))), ("origin", po(py, (0, 0))), ("scale", po(py, 0.7))])?))?;
            match fr {
                None => {
                    let b = button.call((), Some(&kw(py, vec![("parent", pn()), ("text", po(py, "切替")), ("position", po(py, (x0 + 0.14, *y))), ("scale", po(py, (0.07, 0.04))),
                        ("color", rgb(0.22, 0.32, 0.45)?), ("text_size", po(py, 0.6))])?))?;
                    b.setattr("on_click", fl)?;
                }
                Some(fr) => {
                    for (dx, f, txt) in [(-0.0, fl, "◀"), (0.14, fr, "▶")] {
                        let b = button.call((), Some(&kw(py, vec![("parent", pn()), ("text", po(py, txt)), ("position", po(py, (x0 + dx, *y))), ("scale", po(py, (0.035, 0.035))),
                            ("color", rgb(0.22, 0.32, 0.45)?), ("text_size", po(py, 0.6))])?))?;
                        b.setattr("on_click", f)?;
                    }
                }
            }
            rows.set_item(key, t)?;
            *y -= 0.058;
            Ok(())
        };
        let m = |n: &str, a: Vec<PyObject>| -> R<B<'_>> { partial(&s.getattr(n)?, a) };
        row("ui", "画面の表示", s.getattr("toggle_ui")?, None, &mut y)?;
        row("frame", "ブロックの枠", s.getattr("toggle_frame")?, None, &mut y)?;
        row("emote", "モーション", m("step_emote", vec![po(py, -1)])?, Some(m("step_emote", vec![po(py, 1)])?), &mut y)?;
        row("body", "体の向き", m("turn_body", vec![po(py, -1)])?, Some(m("turn_body", vec![po(py, 1)])?), &mut y)?;
        row("dist", "距離", m("zoom", vec![po(py, 1.0 / 0.85)])?, Some(m("zoom", vec![po(py, 0.85)])?), &mut y)?;
        row("height", "高さ", m("lift", vec![po(py, -1)])?, Some(m("lift", vec![po(py, 1)])?), &mut y)?;
        row("fov", "視野角", m("set_fov", vec![po(py, -5)])?, Some(m("set_fov", vec![po(py, 5)])?), &mut y)?;
        row("roll", "傾き", m("set_roll", vec![po(py, -1)])?, Some(m("set_roll", vec![po(py, 1)])?), &mut y)?;
        let game = s.getattr("game")?;
        for (t, f, col) in [("写真を保存", s.getattr("take_photo")?, rgb(0.25, 0.5, 0.35)?), ("パネルを隠す", s.getattr("hide_panel")?, rgb(0.25, 0.35, 0.45)?),
                            ("カメラを元に戻す", s.getattr("reset")?, rgb(0.3, 0.3, 0.33)?), ("カメラモードを終わる", game.getattr("exit_camera")?, rgb(0.5, 0.25, 0.2)?)] {
            let b = button.call((), Some(&kw(py, vec![("parent", pn()), ("text", po(py, t)), ("position", po(py, (x0, y - 0.01))), ("scale", po(py, (0.3, 0.048))),
                ("color", col), ("text_size", po(py, 0.7))])?))?;
            b.setattr("on_click", f)?;
            y -= 0.06;
        }
        text.call((), Some(&kw(py, vec![("parent", pn()), ("position", po(py, (x0, y - 0.005))), ("origin", po(py, (0, 0.5))), ("scale", po(py, 0.55)),
            ("color", color.getattr("light_gray")?.unbind()),
            ("text", po(py, "ドラッグ・なぞる：カメラが回る\nホイール・2本指：距離\nWASD・スティック：注目する所を動かす\nSpace / Ctrl：上下　H：パネル\nP：写真を保存（photos フォルダ）"))])?))?;
        s.setattr("hint", text.call((), Some(&kw(py, vec![("parent", root.clone().unbind()), ("text", po(py, "画面をタップ（クリック）するとパネルが出ます")), ("origin", po(py, (0, 0))),
            ("position", po(py, (0, -0.45))), ("scale", po(py, 0.7)), ("color", color.call_method1("rgba", (1, 1, 1, 0.0))?.unbind()), ("enabled", po(py, false))])?))?)?;
        s.setattr("_hint_t", 0.0)?;
        s.call_method0("_refresh")?;
        Ok(())
    }
    fn _refresh(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let r = s.getattr("rows")?;
        let set = |k: &str, t: String| -> R<()> { r.get_item(k)?.setattr("text", t) };
        set("ui", (if s.getattr("ui_on")?.is_truthy()? { "表示" } else { "隠す" }).into())?;
        set("frame", (if s.getattr("frame_on")?.is_truthy()? { "表示" } else { "隠す" }).into())?;
        let em: String = s.getattr("emotes")?.get_item(s.getattr("emote_i")?)?.str()?.to_string();
        set("emote", em.chars().take(9).collect())?;
        set("body", format!("{}°", pf(py, pymod(fg(s, "body_yaw")?.to_degrees(), 360.0), ".0f")?))?;
        set("dist", format!("{} m", pf(py, fg(s, "dist")?, ".1f")?))?;
        set("height", format!("{}%", pf(py, fg(s, "height")? * 100.0, ".0f")?))?;
        set("fov", format!("{}°", pf(py, fg(s, "fov")?, ".0f")?))?;
        set("roll", format!("{}°", pf(py, fg(s, "roll")?.to_degrees(), "+.0f")?))?;
        Ok(())
    }
    // --- 操作 ---
    fn toggle_ui(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let on = !s.getattr("ui_on")?.is_truthy()?;
        s.setattr("ui_on", on)?;
        s.getattr("game")?.getattr("ui_root")?.setattr("enabled", on)?;
        s.call_method0("_refresh")?;
        Ok(())
    }
    fn toggle_frame(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let on = !s.getattr("frame_on")?.is_truthy()?;
        s.setattr("frame_on", on)?;
        s.call_method0("_refresh")?;
        Ok(())
    }
    fn step_emote(slf: &Bound<'_, Self>, d: i64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let n = s.getattr("emotes")?.len()? as i64;
        let i = (s.getattr("emote_i")?.extract::<i64>()? + d).rem_euclid(n);
        s.setattr("emote_i", i)?;
        let p = Self::local(s)?;
        p.setattr("emote", if i == 0 { py.None() } else { s.getattr("emotes")?.get_item(i)?.unbind() })?;
        p.setattr("emote_t0", imp(py, "time")?.call_method0("monotonic")?)?;
        s.call_method0("_refresh")?;
        Ok(())
    }
    fn turn_body(slf: &Bound<'_, Self>, d: f64) -> R<()> {
        let s = slf.as_any();
        s.setattr("body_yaw", pymod(fg(s, "body_yaw")? + d * 30f64.to_radians(), 2.0 * std::f64::consts::PI))?;
        s.call_method0("_apply_face")?;
        s.call_method0("_refresh")?;
        Ok(())
    }
    fn _apply_face(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let p = Self::local(s)?;
        let up = Self::upn(&p)?;
        let b = v3(&s.getattr("base_fwd")?)?;
        let d = dot(b, up);
        let f = unit([b[0] - up[0] * d, b[1] - up[1] * d, b[2] - up[2] * d], 1e-9);
        p.setattr("face_dir", np3(py, mv(rot(up, -fg(s, "body_yaw")?), f)))?;
        Ok(())
    }
    fn zoom(slf: &Bound<'_, Self>, k: f64) -> R<()> {
        let s = slf.as_any();
        s.setattr("dist", (fg(s, "dist")? * k).clamp(0.6, 60.0))?;
        s.call_method0("_refresh")?;
        Ok(())
    }
    fn lift(slf: &Bound<'_, Self>, d: f64) -> R<()> {
        let s = slf.as_any();
        s.setattr("height", (fg(s, "height")? + d * 0.1).clamp(-0.2, 1.6))?;
        s.call_method0("_refresh")?;
        Ok(())
    }
    fn set_fov(slf: &Bound<'_, Self>, d: f64) -> R<()> {
        let s = slf.as_any();
        let f = (fg(s, "fov")? + d).clamp(15.0, 120.0);
        s.setattr("fov", f)?;
        s.getattr("U")?.get_item("camera")?.setattr("fov", f)?;
        s.call_method0("_refresh")?;
        Ok(())
    }
    fn set_roll(slf: &Bound<'_, Self>, d: f64) -> R<()> {
        let s = slf.as_any();
        let lim = 45f64.to_radians();
        s.setattr("roll", (fg(s, "roll")? + d * 5f64.to_radians()).clamp(-lim, lim))?;
        s.call_method0("_refresh")?;
        Ok(())
    }
    fn hide_panel(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        s.setattr("panel_on", false)?;
        s.getattr("panel")?.setattr("enabled", false)?;
        let h = s.getattr("hint")?;
        h.setattr("enabled", true)?;
        s.setattr("_hint_t", 2.5)?;
        h.setattr("color", s.getattr("U")?.get_item("color")?.call_method1("rgba", (1, 1, 1, 0.85))?)?;
        Ok(())
    }
    fn show_panel(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        s.setattr("panel_on", true)?;
        s.getattr("panel")?.setattr("enabled", true)?;
        s.getattr("hint")?.setattr("enabled", false)?;
        Ok(())
    }
    /// パネルを隠して1コマ描いてから、画面を photos/<ワールド>_<日時>.png に保存する
    fn take_photo(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if s.getattr("_shooting").map(|x| x.is_truthy().unwrap_or(false)).unwrap_or(false) {
            return Ok(());
        }
        s.setattr("_shooting", true)?;
        let was = s.getattr("panel_on")?.is_truthy()?;
        s.getattr("panel")?.setattr("enabled", false)?;
        s.getattr("hint")?.setattr("enabled", false)?;
        let k = PyDict::new_bound(py);
        k.set_item("delay", 0.12)?;
        ursina(py)?.getattr("invoke")?.call((partial(&s.getattr("_shoot")?, vec![po(py, was)])?,), Some(&k))?;
        Ok(())
    }
    fn _shoot(slf: &Bound<'_, Self>, was: bool) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let r = (|| -> R<()> {
            let d = imp(py, "main.paths")?.getattr("ROOT")?.call_method1("__truediv__", ("photos",))?;
            let k = PyDict::new_bound(py);
            k.set_item("parents", true)?;
            k.set_item("exist_ok", true)?;
            d.call_method("mkdir", (), Some(&k))?;
            let world = s.getattr("game")?.getattr("world")?;
            let ts: String = imp(py, "time")?.call_method1("strftime", ("%Y%m%d_%H%M%S",))?.extract()?;
            let name = format!("{}_{}.png", world.getattr("name")?.str()?, ts);
            let path = d.call_method1("__truediv__", (&name,))?;
            imp(py, "main.client.gfx")?.getattr("VIEW")?.call_method1("screenshot", (path.str()?,))?; // この後に描く1コマを保存する
            world.call_method1("message", (format!("写真を保存しました：photos/{}", name),))?;
            for a in s.getattr("game")?.getattr("addons")?.iter()? {
                let a = a?;
                if let Err(e) = a.call_method1("on_photo", (&path,)) {
                    if !e.is_instance_of::<PyException>(py) {
                        return Err(e);
                    }
                    let k = PyDict::new_bound(py);
                    k.set_item("exc_info", e.value_bound(py))?;
                    lg(py)?.call_method("exception", ("写真の知らせでエラー（%s）", a.getattr("name")?), Some(&k))?;
                }
            }
            Ok(())
        })();
        let res = match r {
            Err(e) if e.is_instance_of::<PyException>(py) => {
                let k = PyDict::new_bound(py);
                k.set_item("exc_info", e.value_bound(py))?;
                lg(py)?.call_method("exception", ("写真を保存できません",), Some(&k))?;
                Ok(())
            }
            x => x,
        };
        let k = PyDict::new_bound(py);
        k.set_item("delay", 0.15)?;
        ursina(py)?.getattr("invoke")?.call((partial(&s.getattr("_restore")?, vec![po(py, was)])?,), Some(&k))?; // 保存する1コマを描いてからパネルを戻す
        res
    }
    fn _restore(slf: &Bound<'_, Self>, was: bool) -> R<()> {
        let s = slf.as_any();
        s.setattr("_shooting", false)?;
        if was {
            s.call_method0("show_panel")?;
        }
        Ok(())
    }
    fn reset(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("yaw", std::f64::consts::PI)?;
        s.setattr("pitch", 0.12)?;
        s.setattr("dist", 3.2)?;
        s.setattr("height", 0.62)?;
        s.setattr("pan", imp(py, "numpy")?.call_method1("zeros", (3,))?)?;
        s.setattr("roll", 0.0)?;
        let f0 = s.getattr("fov0")?;
        s.setattr("fov", &f0)?;
        s.getattr("U")?.get_item("camera")?.setattr("fov", f0)?;
        s.call_method0("_refresh")?;
        Ok(())
    }
    /// カメラモード中のキー。使ったら True
    fn on_key(slf: &Bound<'_, Self>, key: &str) -> R<bool> {
        let s = slf.as_any();
        let pon = s.getattr("panel_on")?.is_truthy()?;
        match key {
            // 終わるのは Esc・「カメラモードを終わる」だけ（C をもう一度押しても終わらない）
            "escape" => {
                if !pon {
                    s.call_method0("show_panel")?;
                } else {
                    s.getattr("game")?.call_method0("exit_camera")?;
                }
            }
            "h" => {
                s.call_method0(if !pon { "show_panel" } else { "hide_panel" })?;
            }
            "p" => {
                s.call_method0("take_photo")?;
            }
            "scroll up" => {
                s.call_method1("zoom", (0.9,))?;
            }
            "scroll down" => {
                s.call_method1("zoom", (1.0 / 0.9,))?;
            }
            _ => {}
        }
        Ok(true) // ほかのキーはゲームの操作に回さない（移動はカメラの方で読む）
    }
    // --- 毎フレーム ---
    fn update(slf: &Bound<'_, Self>, dt: f64, held: &B<'_>, vkeys: &B<'_>, stick: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let c = s.getattr("game")?.getattr("cfg")?.get_item("controls")?;
        let ht = fg(s, "_hint_t")?;
        if ht > 0.0 {
            let ht = ht - dt;
            s.setattr("_hint_t", ht)?;
            if ht <= 0.0 {
                s.getattr("hint")?.setattr("enabled", false)?;
            }
        }
        s.call_method0("_pointers")?;
        // 注目点を動かす（WASD・スティック）
        let p = Self::local(s)?;
        let up = Self::upn(&p)?;
        let ax = s.call_method1("_cam_axes", (np3(py, up),))?;
        let (cf, cr) = (v3(&ax.get_item(0)?)?, v3(&ax.get_item(1)?)?);
        let h = |k: &str| -> R<f64> {
            let key = c.get_item(k)?;
            Ok(if held.get_item(&key)?.is_truthy()? || vkeys.call_method1("get", (&key,))?.is_truthy()? { 1.0 } else { 0.0 })
        };
        let (s0, s1) = (f64of(&stick.get_item(0)?)?, f64of(&stick.get_item(1)?)?);
        let a = h("forward")? - h("back")? + s1;
        let b = h("right")? - h("left")? + s0;
        let mut m: V3 = std::array::from_fn(|k| cf[k] * a + cr[k] * b);
        let ud = h("jump")? - h("down")?;
        for k in 0..3 {
            m[k] += up[k] * ud;
        }
        let sp = 1.5 + fg(s, "dist")? * 0.4;
        let pan = v3(&s.getattr("pan")?)?;
        let mut np_: V3 = std::array::from_fn(|k| pan[k] + m[k] * sp * dt);
        let npm = imp(py, "numpy")?;
        let n = f64of(&npm.getattr("linalg")?.call_method1("norm", (np3(py, np_),))?)?;
        if n > 40.0 {
            for x in np_.iter_mut() {
                *x *= 40.0 / n;
            }
        }
        s.setattr("pan", np3(py, np_))?;
        Ok(())
    }
    /// カメラの前（地面に沿って）と右
    fn _cam_axes<'py>(slf: &Bound<'py, Self>, up: &B<'py>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let s = slf.as_any();
        let u = v3(up)?;
        let f = Self::fwd_yaw(s, u, fg(s, "yaw")?)?;
        let right = cross(u, f);
        Ok(PyTuple::new_bound(py, [np3(py, f), np3(py, right)])) // 画面の奥・右（カメラは注目点の -f 側から f の向きに見る）
    }
    /// ドラッグで回る・2本指で距離・タップでパネルを出す（パネルの上から始めた指は使わない）
    fn _pointers(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let pts = s.getattr("game")?.getattr("touch_hud")?.getattr("ptr")?.call_method0("current")?;
        let pon = s.getattr("panel_on")?.is_truthy()?;
        let over_panel = match ursina(py).and_then(|u| u.getattr("mouse")?.getattr("hovered_entity")) {
            Ok(h) => h.is_truthy()? && pon,
            Err(e) if e.is_instance_of::<PyException>(py) => false,
            Err(e) => return Err(e),
        };
        let own = s.getattr("ptr_own")?;
        let now = || -> R<f64> { imp(py, "time")?.call_method0("monotonic")?.extract() };
        let keys: Vec<B<'_>> = own.iter()?.collect::<R<_>>()?;
        for pid in keys {
            if !pts.contains(&pid)? {
                let o = own.call_method1("pop", (&pid,))?;
                if o.is_truthy()? && !s.getattr("panel_on")?.is_truthy()? && now()? - f64of(&o.get_item("t0")?)? < 0.35 && f64of(&o.get_item("moved")?)? < 0.015 {
                    s.call_method0("show_panel")?; // 画面を1回タップ：パネルを出す
                }
            }
        }
        for kv in pts.call_method0("items")?.iter()? {
            let kv = kv?;
            let (pid, xy) = (kv.get_item(0)?, kv.get_item(1)?);
            let (x, y) = (f64of(&xy.get_item(0)?)?, f64of(&xy.get_item(1)?)?);
            let o = own.call_method1("get", (&pid, false))?;
            if o.is(&false.into_py(py).into_bound(py)) || (o.is_instance_of::<pyo3::types::PyBool>() && !o.is_truthy()?) {
                if over_panel {
                    own.set_item(&pid, py.None())?;
                } else {
                    let d = PyDict::new_bound(py);
                    d.set_item("last", (x, y))?;
                    d.set_item("t0", now()?)?;
                    d.set_item("moved", 0.0)?;
                    own.set_item(&pid, d)?;
                }
                continue;
            }
            if o.is_none() {
                continue;
            }
            let last = o.get_item("last")?;
            let (lx, ly) = (f64of(&last.get_item(0)?)?, f64of(&last.get_item(1)?)?);
            let (dx, dy) = (x - lx, y - ly);
            o.set_item("last", (x, y))?;
            o.set_item("moved", f64of(&o.get_item("moved")?)? + (dx.abs() + dy.abs()))?;
            let live: Vec<B<'_>> = own.call_method0("values")?.iter()?.filter_map(|v| v.ok()).filter(|v| v.is_truthy().unwrap_or(false)).collect();
            if live.len() >= 2 {
                let (a, b) = (live[0].get_item("last")?, live[1].get_item("last")?);
                let d = (f64of(&a.get_item(0)?)? - f64of(&b.get_item(0)?)?).hypot(f64of(&a.get_item(1)?)? - f64of(&b.get_item(1)?)?);
                let pd = o.call_method1("get", ("pinch",))?;
                if pd.is_truthy()? {
                    s.call_method1("zoom", (f64of(&pd)? / d.max(1e-3),))?;
                }
                o.set_item("pinch", d)?;
            } else {
                s.setattr("yaw", fg(s, "yaw")? - dx * 3.0)?;
                s.setattr("pitch", (fg(s, "pitch")? - dy * 2.5).clamp(-1.4, 1.4))?;
            }
        }
        Ok(())
    }
    /// (カメラの位置, 視線, 上, 右) をワールドで
    fn camera<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let s = slf.as_any();
        let p = Self::local(s)?;
        let up = Self::upn(&p)?;
        let f = Self::fwd_yaw(s, up, fg(s, "yaw")?)?;
        let right = cross(up, f);
        let pos0 = v3(&p.getattr("pos")?)?;
        let k = f64of(&p.getattr("height")?)? * fg(s, "height")? - f64of(&p.getattr("radius")?)?;
        let pan = v3(&s.getattr("pan")?)?;
        let focus: V3 = std::array::from_fn(|i| pos0[i] + up[i] * k + pan[i]);
        let (pitch, dist) = (fg(s, "pitch")?, fg(s, "dist")?);
        let d: V3 = std::array::from_fn(|i| (-f[i] * pitch.cos() + up[i] * pitch.sin()) * dist);
        let pos: V3 = std::array::from_fn(|i| focus[i] + d[i]);
        let look = unit(std::array::from_fn(|i| focus[i] - pos[i]), 0.0);
        let ul = dot(up, look);
        let cu = unit(std::array::from_fn(|i| up[i] - look[i] * ul), 1e-9);
        let cu = mv(rot(look, fg(s, "roll")?), cu);
        Ok(PyTuple::new_bound(py, [np3(py, pos), np3(py, look), np3(py, cu), np3(py, right)]))
    }
    fn destroy(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        super::client_misc::ui_destroy(py, &s.getattr("root")?, &PyTuple::empty_bound(py), None)?;
        let g = s.getattr("game")?;
        g.getattr("U")?.get_item("camera")?.setattr("fov", s.getattr("fov0")?)?;
        g.getattr("ui_root")?.setattr("enabled", true)?;
        let p = Self::local(s)?;
        p.setattr("emote", py.None())?;
        p.setattr("face_dir", py.None())?;
        lg(py)?.call_method1("info", ("カメラモード終了",))?;
        Ok(())
    }
}

/// 軸 axis のまわりに ang 回す行列（3×3）
#[pyfunction]
pub fn camera_rot<'py>(py: Python<'py>, axis: &B<'py>, ang: f64) -> R<B<'py>> {
    let m = rot(v3(axis)?, ang);
    let v: Vec<f64> = m.iter().flatten().cloned().collect();
    Ok(numpy::PyArrayMethods::reshape(&numpy::PyArray1::from_vec_bound(py, v), [3, 3])?.into_any())
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<CameraMode>()?;
    m.add_function(wrap_pyfunction_bound!(camera_rot, m)?)?;
    Ok(())
}
