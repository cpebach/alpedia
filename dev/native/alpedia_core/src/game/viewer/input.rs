//! 画面：視線・入力・クリック・画面を開く・キー

use crate::game::player::{ply, Player};
use crate::game::construct::{con, Construct};
use super::*;

#[pymethods]
impl Viewer {
    // ---- 視点 ----
    pub(crate) fn look_vectors<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let up_o = ga(&ga(&Self::ga_w(slf)?, "player")?, "up")?;
        let up = v3(&up_o)?;
        let refv = || if up[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let yo = Self::ga_yaw_fwd(slf)?;
        let yaw = if yo.is_none() {
            let r = refv();
            sub(r, mul(up, dot(r, up)))
        } else {
            v3(&yo)?
        };
        let mut f = sub(yaw, mul(up, dot(yaw, up)));
        if norm(f) < 1e-3 {
            let r = refv();
            f = sub(r, mul(up, dot(r, up)));
        }
        let n = norm(f).max(1e-9);
        f = [f[0] / n, f[1] / n, f[2] / n];
        let fo = np3(py, f);
        Self::sa_yaw_fwd(slf, &fo)?;
        let pitch = f64of(&Self::ga_pitch(slf)?)?;
        let look = add(mul(f, pitch.cos()), mul(up, pitch.sin()));
        Ok(PyTuple::new_bound(py, [fo, np3(py, cross(up, f)), up_o, np3(py, look)]))
    }
    pub(crate) fn _input(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let c = Self::ga_cfg(slf)?.get_item("controls")?;
        let (mut dx, mut dy) = (0.0f64, 0.0f64);
        let view = Self::ga_view(slf)?;
        let game = Self::ga_game(slf)?;
        let sh = Self::ga_shim(slf)?;
        let ui = Self::ga_ui(slf)?;
        let held = Self::ga_held(slf)?;
        let evs = view.call_method0("pump")?;
        for ev in evs.iter()? {
            let ev = ev?;
            let (kind_o, a, b, n) = (ev.get_item(0)?, ev.get_item(1)?, ev.get_item(2)?, ev.get_item(3)?);
            let kind: String = kind_o.extract()?;
            let cm = ga(&game, "cam_mode")?;
            let editor_none = Self::ga_editor(slf)?.is_none();
            let stack = ga(&ui, "stack")?.is_truthy()?;
            if !cm.is_none() && editor_none && !stack {
                // カメラモード：パネルのボタン・ドラッグは ursina のまねへ、キーとホイールはカメラモードへ
                sh.call_method1("event", (&view, &kind_o, &a, &b, &n))?;
                if let Some(k) = kind.strip_prefix("key:") {
                    if a.is_truthy()? {
                        held.call_method1("add", (k,))?;
                        if !n.is_truthy()? {
                            cm.call_method1("on_key", (keymap(k),))?;
                        }
                    } else {
                        held.call_method1("discard", (k,))?;
                    }
                } else if kind == "wheel" && a.is_truthy()? {
                    cm.call_method1("on_key", (if f64of(&a)? > 0.0 { "scroll up" } else { "scroll down" },))?;
                } else if kind == "close" {
                    Self::sa_quit(slf, true)?;
                    Self::sa_close_app(slf, true)?;
                }
                continue;
            }
            if !editor_none && sh.call_method1("event", (&view, &kind_o, &a, &b, &n))?.is_truthy()? {
                continue;
            }
            if kind.starts_with("touch:") {
                // 指：画面のボタン・ジョイスティック・なぞって視点
                let (ww, hh): (f64, f64) = (f64of(&view.call_method0("size")?.get_item(0)?)?, f64of(&view.call_method0("size")?.get_item(1)?)?);
                ga(&ga(&game, "touch_hud")?, "ptr")?.call_method1("feed", (&kind_o, (f64of(&a)? - ww / 2.0) / hh, (hh / 2.0 - f64of(&b)?) / hh, &n))?;
                continue;
            }
            let grabbed = Self::ga_grabbed(slf)?.is_truthy()?;
            if editor_none && !grabbed && !stack {
                // 画面の上のボタン（目標の欄・画面のボタン）
                if kind == "cursor" {
                    sh.call_method1("event", (&view, &kind_o, &a, &b, &n))?;
                } else if kind == "button" && n.eq(0)? {
                    let m = ga(&sh, "mouse")?;
                    let (mx, my) = (ga(&m, "x")?, ga(&m, "y")?);
                    let pass = !a.is_truthy()? || sh.call_method1("_hit", (&mx, &my))?.is_truthy()?
                        || ga(&game, "touch_hud")?.call_method1("over_ui", (&mx, &my))?.is_truthy()? || s.call_method0("_touch_mode")?.is_truthy()?;
                    if pass {
                        sh.call_method1("event", (&view, &kind_o, &a, &b, &n))?; // ボタンは指と同じ（ursina のまねの mouse.left）
                        continue;
                    }
                }
            }
            if ui.call_method1("event", (&kind_o, &a, &b, &n))?.is_truthy()? {
                continue;
            }
            if kind == "close" {
                Self::sa_quit(slf, true)?;
                Self::sa_close_app(slf, true)?;
            } else if let Some(k) = kind.strip_prefix("key:") {
                if k == "alt" {
                    // Alt を押している間はカーソルを出す（画面のボタンを押せる）
                    if a.is_truthy()? && Self::ga_grabbed(slf)?.is_truthy()? {
                        Self::sa_alt_free(slf, true)?;
                        s.call_method1("set_grab", (false,))?;
                    } else if !a.is_truthy()? && gd(s, "_alt_free", tb(py, false)).is_truthy()? {
                        Self::sa_alt_free(slf, false)?;
                        if !ga(&ui, "stack")?.is_truthy()? && Self::ga_editor(slf)?.is_none() && ga(&game, "cam_mode")?.is_none() {
                            s.call_method1("set_grab", (true,))?;
                        }
                    }
                }
                if a.is_truthy()? {
                    held.call_method1("add", (k,))?;
                    if !n.is_truthy()? {
                        s.call_method1("_key", (k, &c))?;
                    }
                } else {
                    held.call_method1("discard", (k,))?;
                }
            } else if kind == "mouse" {
                dx += f64of(&a)?;
                dy += f64of(&b)?;
            } else if kind == "button" && a.is_truthy()? && !Self::ga_grabbed(slf)?.is_truthy()? {
                if !s.call_method0("_touch_mode")?.is_truthy()? {
                    s.call_method1("set_grab", (true,))?;
                }
            } else if kind == "button" && a.is_truthy()? {
                s.call_method1("_click", (&n,))?;
            } else if kind == "wheel" && a.is_truthy()? {
                let sel: i64 = Self::ga_sel(slf)?.extract()?;
                Self::sa_sel(slf, (sel + if f64of(&a)? > 0.0 { -1 } else { 1 }).rem_euclid(9))?;
            } else if kind == "focus" && !a.is_truthy()? {
                held.call_method0("clear")?;
            }
        }
        if dx != 0.0 || dy != 0.0 {
            let sens = f64of(&get(&c, "mouse_sensitivity", 60.0)?)? / 60.0 * 0.0025;
            Self::look_vectors(slf)?;
            let up = v3(&ga(&ga(&Self::ga_w(slf)?, "player")?, "up")?)?;
            let f = v3(&Self::ga_yaw_fwd(slf)?)?;
            let ang = dx * sens;
            Self::sa_yaw_fwd(slf, np3(py, add(mul(f, ang.cos()), mul(cross(up, f), ang.sin()))))?;
            let p = f64of(&Self::ga_pitch(slf)?)?;
            Self::sa_pitch(slf, (p - dy * sens).clamp(-1.55, 1.55))?;
        }
        Ok(())
    }
    /// 左：壊す・掘る、右：置く
    pub(crate) fn _click(slf: &Bound<'_, Self>, button: i64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = Self::ga_w(slf)?;
        let seated = ga(&ga(&w, "player")?, "seated")?.is_truthy()?;
        let hit = Self::ga_hit(slf)?;
        if seated || hit.is_none() {
            if button == 1 && !seated {
                s.call_method1("_place", (py.None(),))?;
            }
            return Ok(());
        }
        let kind = hit.get_item("kind")?;
        if button == 0 {
            if kind.eq("voxel")? {
                if !w.call_method1("remove", (&hit,))?.is(&tb(py, false)) {
                    s.call_method1("sfx", ("break",))?;
                }
            } else if (kind.eq("terrain")? || kind.eq("water")?) && !w.call_method1("dig", (&hit,))?.is(&tb(py, false)) {
                s.call_method1("sfx", ("dig",))?;
            }
        } else if button == 1 {
            s.call_method1("_place", (&hit,))?;
        }
        Ok(())
    }
    pub(crate) fn _place(slf: &Bound<'_, Self>, hit: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = Self::ga_w(slf)?;
        let item = ga(&ga(&w, "player")?, "hotbar")?.get_item(Self::ga_sel(slf)?)?;
        if item.is_none() {
            w.call_method1("message", ("アイテムバーのこの枠は空です",))?;
            return Ok(());
        }
        let look = Self::look_vectors(slf)?.get_item(3)?;
        let ok = w.call_method1("place", (hit, &item, Self::ga_dir_index(slf)?, look))?;
        let name = if !ok.is(&tb(py, false)) { if item.get_item(0)?.eq("device")? { "device" } else { "place" } } else { "error" };
        s.call_method1("sfx", (name,))?;
        Ok(())
    }
    /// 一時停止メニュー・インベントリを開く（マウスを出して、歩くのを止める）
    pub(crate) fn open_screen(slf: &Bound<'_, Self>, name: &str) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let sc = imp(py, "main.client_rs.screens")?;
        Self::ga_held(slf)?.call_method0("clear")?;
        s.call_method1("set_grab", (false,))?;
        ga(&Self::ga_game(slf)?, "ui_root")?.setattr("enabled", false)?; // MOD の画面の部品（目標の欄など）は隠す
        let cls = match name {
            "pause" => sc.getattr("Pause")?,
            "inventory" => sc.getattr("Inventory")?,
            _ => return Err(pyo3::exceptions::PyKeyError::new_err(name.to_string())),
        };
        Self::ga_ui(slf)?.call_method1("open", (cls, s))?;
        Ok(())
    }
    /// P：見ている機器の設定（アクチュエーター・ほかの機器）。コンピューターはプログラムの画面
    pub(crate) fn open_editor(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = Self::ga_w(slf)?;
        let h = Self::ga_hit(slf)?;
        let ok = h.is_truthy()? && h.get_item("kind")?.eq("voxel")? && h.get_item("construct")?.getattr("devs")?.get_item(h.get_item("index")?)?.is_truthy()?;
        if !ok {
            w.call_method1("message", ("機器を見ながら P を押すと設定を開けます",))?;
            return Ok(());
        }
        let (c, i) = (h.get_item("construct")?, h.get_item("index")?);
        let d = Construct::g_devs(con(&c)?)?.get_item(&i)?;
        let dd = c.getattr("reg")?.call_method1("device", (d.get_item("def")?,))?;
        let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
        Self::ga_held(slf)?.call_method0("clear")?;
        if get(&dd, "type", py.None())?.eq("computer")? {
            s.call_method1("open_program", (&c, &i))?;
            return Ok(());
        }
        s.call_method1("set_grab", (false,))?;
        ga(&Self::ga_game(slf)?, "ui_root")?.setattr("enabled", false)?;
        let sd = imp(py, "main.client_rs.screens_dev")?;
        let cls = if imp(py, "main.actuator")?.call_method1("is_actuator", (&c, &i))?.is_truthy()? { sd.getattr("ActuatorPanel")? } else { sd.getattr("DevicePanel")? };
        Self::ga_ui(slf)?.call_method1("open", (cls, s, &c, &i))?;
        Ok(())
    }
    /// コンピューターのプログラムの画面（ScriptPanel・BlockEditor を ursina のまねで動かす）
    pub(crate) fn open_program(slf: &Bound<'_, Self>, c: &B<'_>, i: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.call_method1("set_grab", (false,))?;
        let game = Self::ga_game(slf)?;
        ga(&game, "ui_root")?.setattr("enabled", false)?;
        let blocks = get(&Construct::g_devs(con(&c)?)?.get_item(i)?.get_item("state")?, "blocks", py.None())?;
        let ed = if blocks.is_truthy()? {
            imp(py, "main.client.block_editor")?.getattr("BlockEditor")?.call1((&game, c, i))?
        } else {
            imp(py, "main.client.script_panel")?.getattr("ScriptPanel")?.call1((&game, c, i))?
        };
        Self::sa_editor(slf, ed)?;
        Ok(())
    }
    pub(crate) fn _touch_mode(slf: &Bound<'_, Self>) -> R<bool> {
        get(&ga(slf.as_any(), "cfg")?.get_item("controls")?, "touch", false)?.is_truthy()
    }
    /// マウスで視点を回す（カーソルを隠して閉じ込める）か。タッチ操作の ON のときは閉じ込めない
    pub(crate) fn set_grab(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let on = on.is_truthy()? && !Self::_touch_mode(slf)?;
        Self::sa_grabbed(slf, on)?;
        Self::ga_game(slf)?.setattr("mouse_captured", on)?;
        Self::ga_view(slf)?.call_method1("grab", (on,))?;
        Ok(())
    }
    /// 画面の UI 座標 (x, y) を通る視線：(始点, 向き)。タッチで触った所を狙うのに使う
    pub(crate) fn screen_ray<'py>(slf: &Bound<'py, Self>, x: f64, y: f64) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let s = slf.as_any();
        let cam = gd(s, "_cam", none(py));
        if cam.is_none() {
            let look = Self::look_vectors(slf)?.get_item(3)?;
            return Ok(PyTuple::new_bound(py, [ga(&Self::ga_w(slf)?, "player")?.call_method0("eye")?, look]));
        }
        let origin = cam.get_item(0)?;
        let look = v3(&cam.get_item(1)?)?;
        let up = v3(&cam.get_item(2)?)?;
        let vfov = f64of(&cam.get_item(3)?)?;
        let mut right = cross(look, up); // 画面の右（カメラの向きから）
        let n = norm(right).max(1e-9);
        right = [right[0] / n, right[1] / n, right[2] / n];
        let cu = cross(right, look);
        let t = (vfov.to_radians() / 2.0).tan();
        let d = add(add(look, mul(right, x * 2.0 * t)), mul(cu, y * 2.0 * t));
        let dn = norm(d);
        let np = imp(py, "numpy")?;
        let o = np.call_method1("asarray", (origin, py.import_bound("builtins")?.getattr("float")?))?;
        Ok(PyTuple::new_bound(py, [o, np3(py, [d[0] / dn, d[1] / dn, d[2] / dn])]))
    }
    /// タッチ：画面の (x, y) を触った。kind = "tap"（使う／置く）・"hold"（長押し：壊す・掘る）
    pub(crate) fn touch_action(slf: &Bound<'_, Self>, kind: &str, x: f64, y: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = Self::ga_w(slf)?;
        let p = ga(&w, "player")?;
        if ga(&p, "seated")?.is_truthy()? || !Self::ga_editor(slf)?.is_none() || ga(&Self::ga_ui(slf)?, "stack")?.is_truthy()? {
            return Ok(());
        }
        let r = Self::screen_ray(slf, x, y)?;
        let (o, d) = (r.get_item(0)?, r.get_item(1)?);
        let eye = v3(&p.call_method0("eye")?)?;
        let reach = f64of(&get(&ga(&p, "defn")?, "reach", 8.0)?)? + norm(sub(v3(&o)?, eye));
        let hit = w.call_method1("raycast", (&o, &d, reach))?;
        let f = tb(py, false);
        if kind == "hold" {
            if hit.is_truthy()? && hit.get_item("kind")?.eq("voxel")? {
                if !w.call_method1("remove", (&hit,))?.is(&f) {
                    s.call_method1("sfx", ("break",))?;
                }
            } else if hit.is_truthy()? && (hit.get_item("kind")?.eq("terrain")? || hit.get_item("kind")?.eq("water")?) && !w.call_method1("dig", (&hit,))?.is(&f) {
                s.call_method1("sfx", ("dig",))?;
            }
            return Ok(());
        }
        if hit.is_truthy()? && Self::_usable(slf, &hit)? {
            s.call_method1("do_interact", (&hit,))?;
            return Ok(());
        }
        if hit.is_truthy()? {
            let k = hit.get_item("kind")?;
            if k.eq("voxel")? || k.eq("terrain")? || k.eq("water")? || get(&hit, "placeable", py.None())?.is_truthy()? {
                s.call_method1("_place", (&hit,))?;
            }
        }
        Ok(())
    }
    /// LAN に公開する（一時停止メニューから）
    pub(crate) fn start_hosting(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let w = Self::ga_w(slf)?;
        let cfg = Self::ga_cfg(slf)?;
        if let Err(e) = imp(py, "main.netserver")?.getattr("Host")?.call1((&w, &cfg)) {
            if e.is_instance_of::<pyo3::exceptions::PyOSError>(py) {
                log_exc(py, &e, "LAN に公開できません", vec![])?;
                let port = get(&get(&cfg, "multiplayer", PyDict::new_bound(py))?, "port", py.None())?;
                w.call_method1("message", (format!("LAN に公開できません（ポート {} が使えない？）: {}", port.str()?, e.value_bound(py).str()?),))?;
                return Ok(());
            }
            return Err(e);
        }
        let a = w.getattr("net")?.call_method0("addresses")?.get_item(pyo3::types::PySlice::new_bound(py, 0, 2, 1))?;
        let j = PyString::new_bound(py, "  ").call_method1("join", (a,))?;
        w.call_method1("message", (format!("LAN に公開しました: {}（参加する人はメニューの「マルチプレイ」から）", j.str()?),))?;
        Ok(())
    }
    pub(crate) fn screen_closed(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let stack = ga(&Self::ga_ui(slf)?, "stack")?.is_truthy()?;
        let game = Self::ga_game(slf)?;
        if !stack && Self::ga_editor(slf)?.is_none() && ga(&game, "cam_mode")?.is_none() {
            ga(&game, "ui_root")?.setattr("enabled", true)?;
        }
        if !stack {
            s.call_method1("set_grab", (true,))?;
        }
        Ok(())
    }
    pub(crate) fn _key(slf: &Bound<'_, Self>, k: &str, c: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = Self::ga_w(slf)?;
        let seat_keys = imp(py, "main.client.game")?.getattr("SEAT_KEYS")?;
        let p = ga(&w, "player")?;
        let ck = |name: &str, d: &str| -> R<bool> { Ok(get(c, name, d)?.eq(k)?) };
        let ci = |name: &str| -> R<bool> { c.get_item(name)?.eq(k) };
        if ga(&p, "seated")?.is_truthy()? && k != "escape" {
            let kk = keymap(k);
            let in_seat = seat_keys.contains(kk)?;
            if in_seat || kk == "x" {
                w.call_method1("key_pressed", (kk,))?;
            }
            if ci("interact")? {
                w.call_method1("interact", (py.None(),))?;
                s.call_method1("sfx", ("seat",))?;
            } else if ck("third_person", "v")? {
                let t = Self::ga_third_person(slf)?.is_truthy()?;
                Self::sa_third_person(slf, !t)?;
            } else if ck("save", "f5")? {
                w.call_method0("save")?;
            } else if ck("export", "f9")? {
                s.call_method0("export_obj")?;
            } else if !in_seat {
                let f = acall(py, "on_key", vec![kk.into_py(py)])?;
                Self::_each_addon(slf, &f, true)?; // MOD のキー（座っている間も使えるもの）
            }
            return Ok(());
        }
        if k.chars().count() == 1 && "123456789".contains(k) {
            Self::sa_sel(slf, k.parse::<i64>().unwrap() - 1)?;
        } else if ci("rotate")? {
            let d: i64 = Self::ga_dir_index(slf)?.extract()?;
            Self::sa_dir_index(slf, (d + 1).rem_euclid(6))?;
        } else if ci("interact")? {
            let mut hit = Self::ga_hit(slf)?;
            if !Self::_usable(slf, &hit)? {
                // 見ている物が使えなければ、近くの MOD の物（受付・お店など）
                for t in Self::nearby_targets(slf, 2.6)?.iter()? {
                    let t = t?;
                    if get(&t, "kind", py.None())?.eq("e")? {
                        hit = t.get_item("hit")?;
                        break;
                    }
                }
            }
            if !hit.is_none() {
                s.call_method1("do_interact", (&hit,))?;
            }
        } else if ci("new_construct")? {
            s.call_method1("_place", (py.None(),))?;
        }
        if k == "escape" {
            s.call_method1("open_screen", ("pause",))?;
        } else if ck("inventory", "i")? {
            s.call_method1("open_screen", ("inventory",))?;
        } else if ck("edit_script", "p")? {
            s.call_method0("open_editor")?;
        } else if ck("jetpack", "f")? {
            let j = ga(&p, "jetpack")?.is_truthy()?;
            Player::s_jetpack(ply(&p)?, !j)?;
        } else if ck("save", "f5")? {
            w.call_method0("save")?;
        } else if ck("third_person", "v")? {
            let t = Self::ga_third_person(slf)?.is_truthy()?;
            Self::sa_third_person(slf, !t)?;
        } else if ck("toggle_help", "f1")? {
            let t = gd(s, "show_help", tb(py, false)).is_truthy()?;
            Self::sa_show_help(slf, !t)?;
        } else if ck("debug_hud", "f3")? {
            let t = gd(s, "debug_hud", tb(py, false)).is_truthy()?;
            Self::sa_debug_hud(slf, !t)?;
        } else if k == "f11" {
            let v = Self::ga_view(slf)?;
            let fs = v.call_method0("fullscreen")?.is_truthy()?;
            v.call_method1("set_fullscreen", (!fs,))?;
        } else if ck("camera_mode", "c")? {
            Self::ga_game(slf)?.call_method0("enter_camera")?;
        } else if ck("export", "f9")? {
            s.call_method0("export_obj")?;
        } else if ck("import", "f8")? {
            Self::ga_game(slf)?.call_method0("open_import")?;
        } else if !(ci("forward")? || ci("back")? || ci("left")? || ci("right")? || ci("jump")?) {
            let f = acall(py, "on_key", vec![k.into_py(py)])?;
            Self::_each_addon(slf, &f, true)?;
        }
        Ok(())
    }
}
