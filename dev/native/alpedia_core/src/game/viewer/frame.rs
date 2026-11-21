//! 画面：1 フレーム（frame）・カメラ・書き出し

use crate::game::world::{wld, World};
use crate::game::PyDictOps;
use crate::game::player::{ply, Player};
use super::*;

/// 1 フレームで物理に使う時間の上限（秒）
const PHYS_BUDGET: f64 = 0.012;
/// 物理が PHYS_BUDGET を超えたとき、超えた時間の何倍だけ物理を休むか（少し超えただけならほとんど休まない）
const PHYS_REST: f64 = 1.5;

#[pymethods]
impl Viewer {
    // ---- 1フレーム ----
    pub(crate) fn frame(slf: &Bound<'_, Self>, dt: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = Self::ga_w(slf)?;
        let cfg = Self::ga_cfg(slf)?;
        let c = cfg.get_item("controls")?;
        Self::_input(slf).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?;
        let game = Self::ga_game(slf)?;
        let cam_mode = ga(&game, "cam_mode")?;
        let hud = ga(&game, "touch_hud")?;
        if cam_mode.is_none() {
            hud.call_method1("update", (dt,))?; // 画面のボタン・ジョイスティック・近くの物（押されたキー → vkeys）
        }
        let vk = ga(&game, "vkeys")?;
        let tl = hud.call_method0("take_look")?; // 画面をなぞった量（タッチ）
        let (tdx, tdy) = (f64of(&tl.get_item(0)?)?, f64of(&tl.get_item(1)?)?);
        let player = ga(&w, "player")?;
        if tdx != 0.0 || tdy != 0.0 {
            let sens = (f64of(&get(&c, "mouse_sensitivity", 60.0)?)? * 1.6).to_radians();
            Self::look_vectors(slf)?;
            let upv = v3(&ga(&player, "up")?)?;
            let ang = tdx * sens;
            let yf = v3(&Self::ga_yaw_fwd(slf)?)?;
            Self::sa_yaw_fwd(slf, np3(py, add(mul(yf, ang.cos()), mul(cross(upv, yf), ang.sin()))))?;
            let p = f64of(&Self::ga_pitch(slf)?)?;
            Self::sa_pitch(slf, (p + tdy * sens).clamp(-1.55, 1.55))?;
        }
        let lv = Self::look_vectors(slf)?;
        let (fwd_o, right_o, up_o, look_o) = (lv.get_item(0)?, lv.get_item(1)?, lv.get_item(2)?, lv.get_item(3)?);
        let (fwd, right, up) = (v3(&fwd_o)?, v3(&right_o)?, v3(&up_o)?);
        let lp = ga(&w, "local_player")?;
        Player::s_look_fwd(ply(&lp)?, &fwd_o)?;
        Player::s_pitch(ply(&lp)?, Self::ga_pitch(slf)?)?;
        let held = PySet::empty_bound(py)?;
        for k in Self::ga_held(slf)?.iter()? {
            let k = k?;
            let ks: String = k.extract()?;
            held.add(keymap(&ks))?;
        }
        for kv in vk.call_method0("items")?.iter()? {
            let kv = kv?;
            if kv.get_item(1)?.is_truthy()? {
                held.add(kv.get_item(0)?)?;
            }
        }
        let hk = |k: &B<'_>| -> R<f64> {
            Ok(if held.contains(k)? || (k.eq("control")? && held.contains("left control")?) { 1.0 } else { 0.0 })
        };
        let st = ga(&hud, "stick")?;
        let (st0, st1) = (f64of(&st.get_item(0)?)?, f64of(&st.get_item(1)?)?);
        let mut mv = [0.0f64; 3];
        let ui = Self::ga_ui(slf)?;
        let busy = ga(&ui, "stack")?.is_truthy()? || !Self::ga_editor(slf)?.is_none() || ga(&game, "hold")?.is_truthy()? || !cam_mode.is_none();
        let seated = ga(&player, "seated")?.is_truthy()?;
        let ci = |n: &str| c.get_item(n);
        if !seated && !busy {
            let a = hk(&ci("forward")?)? - hk(&ci("back")?)? + st1;
            let b = hk(&ci("right")?)? - hk(&ci("left")?)? + st0;
            mv = add(add(mv, mul(fwd, a)), mul(right, b));
            let nm = norm(mv);
            if nm > 1.0 {
                mv = [mv[0] / nm, mv[1] / nm, mv[2] / nm];
            }
        }
        let jump = hk(&ci("jump")?)? != 0.0 && !seated && !busy;
        let thrust = if !busy {
            add(mul(mv, 0.6), mul(up, hk(&ci("jump")?)? - hk(&get(&c, "down", "left control")?)?))
        } else {
            [0.0; 3]
        };
        let seat_keys = imp(py, "main.client.game")?.getattr("SEAT_KEYS")?;
        if seated && !busy {
            let keys = PySet::empty_bound(py)?;
            for k in held.iter() {
                if seat_keys.contains(&k)? {
                    keys.add(k)?;
                }
            }
            for (k, on) in [("forward", st1 > 0.4), ("back", st1 < -0.4), ("right", st0 > 0.4), ("left", st0 < -0.4)] {
                // 操縦席ではスティックを W/A/S/D として使う
                if on {
                    keys.add(ci(k)?)?;
                }
            }
            w.setattr("input_keys", keys)?;
        } else {
            w.setattr("input_keys", PySet::empty_bound(py)?)?;
        }
        let tick = 1.0 / f64of(&get(&cfg.get_item("physics")?, "tick_rate", 60)?)?;
        let mut acc = f64of(&Self::ga_acc(slf)?)? + dt;
        let mut n = 0;
        let time = imp(py, "time")?;
        let pc = || -> R<f64> { time.call_method0("perf_counter")?.extract() };
        let t0 = pc()?;
        let net = gd(&w, "net", none(py));
        let client = !net.is_none() && ga(&net, "role")?.eq("client")?;
        if ga(&game, "hold")?.is_truthy()? && net.is_none() {
            acc = 0.0; // MOD が世界の時間を止めている（ムービーなど）
        }
        if client {
            // マルチプレイに参加中：ホストから届いた世界を当てる
            net.setattr("look", &look_o)?;
            let top = ga(&ui, "top")?;
            let ed = Self::ga_editor(slf)?;
            let edv = if ed.is_truthy()? { ed } else if top.hasattr("c")? { top } else { none(py) };
            let k = PyDict::new_bound(py);
            k.set_item("editor", edv)?;
            net.call_method1("frame", (imp(py, "types")?.getattr("SimpleNamespace")?.call((), Some(&k))?, dt))?;
            let closed = ga(&net, "closed")?;
            if closed.is_truthy()? {
                Self::sa_acc(slf, acc)?;
                let k = PyDict::new_bound(py);
                k.set_item("status", format!("マルチプレイから抜けました：{}", closed.str()?))?;
                game.call_method("_leave", ("menu",), Some(&k))?;
                return Ok(());
            }
        }
        let mvo = np3(py, mv);
        let tho = np3(py, thrust);
        // 物理は 1 フレームに PHYS_BUDGET 秒まで（最低 1 回）。重いときは追いつこうとせず世界の時間が遅くなる
        // （操作の反映・画面が遅れないように。遅れを取り戻そうとして次のフレームも重くなるのを防ぐ）
        // 物理がとても重い（1 回で PHYS_BUDGET を超えた）ときは、超えた分に合わせて物理を休んで画面と操作だけ進める
        // （とても重いときは物理は時間の半分ほどまで。世界の時間はさらに遅くなるが、視点・メニュー・キーは早く反映される）
        let rest_until = f64of(&gd(s, "_phys_rest", tb(py, 0.0)))?;
        if t0 < rest_until {
            acc = 0.0;
        }
        while acc >= tick && n < 3 {
            if n > 0 && pc()? - t0 > PHYS_BUDGET {
                break;
            }
            if client {
                net.call_method1("step", (tick, &mvo, jump && n == 0, &tho))?; // 自分の体だけ動かす
            } else {
                World::step(wld(&w)?, IntoPy::<PyObject>::into_py(tick, w.py()).extract::<f64>(w.py())?, Some(&IntoPy::<PyObject>::into_py(&mvo, w.py()).into_bound(w.py())), Some(&IntoPy::<PyObject>::into_py(jump && n == 0, w.py()).into_bound(w.py())), Some(&IntoPy::<PyObject>::into_py(&tho, w.py()).into_bound(w.py()))).map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?;
            }
            acc -= tick;
            n += 1;
        }
        if acc >= tick {
            acc = 0.0; // 追いつけなかった分は捨てる（時間が遅くなる）
        }
        if n > 0 {
            let now = pc()?;
            let cost = now - t0;
            s.setattr("_phys_rest", if cost > PHYS_BUDGET { now + (cost - PHYS_BUDGET) * PHYS_REST } else { 0.0 })?;
        }
        Self::sa_acc(slf, acc)?;
        if !net.is_none() && !client {
            net.call_method1("tick", (dt,))?; // 参加者へ送る・参加者の操作を行う
        }
        let t1 = pc()?;
        let mut marks: Vec<(&str, f64)> = Vec::with_capacity(8);
        let cam = s.call_method1("_camera", (dt, &look_o, &up_o))?;
        let (origin, cam_look, cam_up) = (cam.get_item(0)?, cam.get_item(1)?, cam.get_item(2)?);
        marks.push(("cam", pc()?));
        // 画面いっぱいを覆う画面（ムービーなど、covers_screen = True）を開いている間は、隠れた世界の用意を飛ばす
        let ed = Self::ga_editor(slf)?;
        let covered = !ed.is_none() && gd(&ed, "covers_screen", tb(py, false)).is_truthy()?;
        if !covered {
            s.call_method1("_render", (&origin,))?;
        }
        marks.push(("scene", pc()?));
        if !covered {
            s.call_method1("_bodies", (&origin,))?;
        }
        marks.push(("bodies", pc()?));
        let g = cfg.get_item("graphics")?;
        let view = Self::ga_view(slf)?;
        let sz = view.call_method0("size")?;
        let (ww, hh) = (sz.get_item(0)?, sz.get_item(1)?);
        let sh = Self::ga_shim(slf)?;
        let fov = if !cam_mode.is_none() { f64of(&ga(&ga(&sh, "camera")?, "fov")?)? } else { f64of(&get(&g, "fov", 80)?)? };
        let hfov = fov.to_radians(); // Panda3D と同じく横の視野角
        let wwf = f64of(&ww)?;
        let hhf = f64of(&hh)?;
        let vfov = (2.0 * ((hfov / 2.0).tan() * hhf / wwf.max(1.0)).atan()).to_degrees();
        let cl = v3(&cam_look)?;
        let cu = v3(&cam_up)?;
        view.call_method1("camera", (vec![0.0, 0.0, 0.0], cl.to_vec(), cu.to_vec(), vfov, 0.05))?;
        Self::sa_cam(slf, PyTuple::new_bound(py, [origin.clone(), np3(py, cl), np3(py, cu), tb(py, vfov)]))?;
        ga(&sh, "camera")?.setattr("fov_pair", (hfov.to_degrees(), vfov))?;
        let far = f64of(&get(&g, "view_distance", 3000)?)?;
        let f = acall(py, "update", vec![origin.clone().unbind(), far.into_py(py)])?;
        Self::_each_addon(slf, &f, false)?;
        marks.push(("mod", pc()?));
        if !cam_mode.is_none() && !ga(&cam_mode, "frame_on")?.is_truthy()? {
            Self::sa_hit(slf, py.None())?;
            Self::ga_highlight(slf)?.call_method0("hide")?;
            Self::ga_ghost(slf)?.call_method0("hide")?;
            Self::sa_aim_txt(slf, "")?;
        } else {
            s.call_method1("_aim", (&origin, &look_o))?;
        }
        Self::_overlay(slf, IntoPy::<PyObject>::into_py(&ww, slf.py()).extract::<f64>(slf.py())?, IntoPy::<PyObject>::into_py(&hh, slf.py()).extract::<f64>(slf.py())?).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?;
        marks.push(("aim_ui", pc()?));
        sh.call_method1("frame", (&view, dt))?; // 今までの画面の部品（プログラムの画面など）
        Self::_update_sound(slf, IntoPy::<PyObject>::into_py(dt, slf.py()).extract::<f64>(slf.py())?).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?;
        marks.push(("shim_snd", pc()?));
        view.call_method0("render")?;
        let t2 = pc()?;
        let perf = Self::ga_perf(slf)?;
        let pi = |k: &str| -> R<f64> { f64of(&perf.get_item(k)?) };
        let frames: i64 = perf.get_item("frames")?.extract::<i64>()? + 1;
        perf.set_item("frames", frames)?;
        perf.set_item("phys", pi("phys")? + (t1 - t0))?;
        perf.set_item("rend", pi("rend")? + (t2 - t1))?;
        marks.push(("gpu", t2));
        {
            // 描画の内訳（F3・ログ用）
            let parts = match perf.get_item("parts") {
                Ok(d) => d,
                Err(_) => { let d = PyDict::new_bound(py).into_any(); perf.set_item("parts", &d)?; d }
            };
            let mut prev = t1;
            for (k, t) in &marks {
                let old: f64 = parts.get_item(*k).ok().map(|v| v.extract().unwrap_or(0.0)).unwrap_or(0.0);
                parts.set_item(*k, old + (t - prev))?;
                prev = *t;
            }
        }
        let ticks: i64 = perf.get_item("ticks")?.extract::<i64>()? + n;
        perf.set_item("ticks", ticks)?;
        let el = t2 - pi("t")?;
        if el >= 1.0 {
            let fr = frames as f64;
            let txt = format!("fps {}  物理 {} ms/回  描画 {} ms", pf(py, fr / el, ".0f")?, pf(py, pi("phys")? / (ticks.max(1) as f64) * 1000.0, ".1f")?,
                pf(py, pi("rend")? / fr * 1000.0, ".1f")?);
            Self::sa_fps_text(slf, &txt)?;
            let mut detail = String::new();
            if let Ok(parts) = perf.get_item("parts") {
                for k in ["cam", "scene", "bodies", "mod", "aim_ui", "shim_snd", "gpu"] {
                    let v: f64 = parts.get_item(k).ok().map(|v| v.extract().unwrap_or(0.0)).unwrap_or(0.0);
                    detail.push_str(&format!(" {} {:.1}", k, v / fr * 1000.0));
                }
            }
            Self::sa_fps_detail(slf, &detail)?; // F3 にはフレームの内訳だけ
            // ログには場面の用意（1フレームあたり）と物理（1回あたり）の内訳も（0.05 ms 未満は省く）
            let mut full = detail.clone();
            {
                let mut p = SCENE_PROF.lock().unwrap_or_else(|e| e.into_inner());
                full.push_str("  場面:");
                for (k, v) in SCENE_PARTS.iter().zip(p.iter()) {
                    if v / fr * 1000.0 >= 0.05 {
                        full.push_str(&format!(" {} {:.1}", k, v / fr * 1000.0));
                    }
                }
                *p = [0.0; 6];
            }
            full.push_str("  物理:");
            for (k, v) in crate::game::world::world_step_prof() {
                let ms = v / (ticks.max(1) as f64) * 1000.0;
                if ms >= 0.05 {
                    full.push_str(&format!(" {} {:.1}", k, ms));
                }
            }
            let pl: i64 = gd(s, "_perf_log", tb(py, 0)).extract::<i64>()? + 1;
            Self::sa_perf_log(slf, pl)?;
            if pl % 10 == 1 {
                // ログは 10 秒ごと
                lg(py)?.call_method1("info", ("重さ: %s（構築物 %d）内訳 ms:%s", &txt, ga(&w, "constructs")?.len()?, &full))?;
            }
            let np = PyDict::new_bound(py);
            np.set_item("frames", 0)?;
            np.set_item("t", t2)?;
            np.set_item("phys", 0.0)?;
            np.set_item("rend", 0.0)?;
            np.set_item("ticks", 0)?;
            Self::sa_perf(slf, np)?;
        }
        Ok(())
    }
    /// カメラの位置・視線・上（今までの画面と同じ：カメラモード・遠隔操作・三人称・一人称）
    pub(crate) fn _camera<'py>(slf: &Bound<'py, Self>, dt: f64, look: &B<'py>, up: &B<'py>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let s = slf.as_any();
        let play = imp(py, "main.client.play")?;
        let w = Self::ga_w(slf)?;
        let player = ga(&w, "player")?;
        let mut origin = player.call_method0("eye")?;
        let seated = !ga(&player, "seated")?.is_none(); // 座ったら三人称（降りたら元に）。V でいつでも切り替え
        let was = Self::ga_was_seated(slf)?.is_truthy()?;
        if seated && !was {
            Self::sa_third_before_seat(slf, Self::ga_third_person(slf)?)?;
            Self::sa_third_person(slf, true)?;
        } else if was && !seated {
            Self::sa_third_person(slf, gd(s, "_third_before_seat", tb(py, false)))?;
        }
        Self::sa_was_seated(slf, seated)?;
        let cm = ga(&Self::ga_game(slf)?, "cam_mode")?;
        if !cm.is_none() {
            let held = Py::new(py, Held::make(s)?)?;
            cm.call_method1("update", (dt, held, PyDict::new_bound(py), imp(py, "numpy")?.call_method1("zeros", (2,))?))?;
            let r = cm.call_method0("camera")?;
            return Ok(PyTuple::new_bound(py, [r.get_item(0)?, r.get_item(1)?, r.get_item(2)?]));
        }
        let tp = Self::ga_third_person(slf)?.is_truthy()?;
        let follow = if tp { play.call_method1("seat_follow", (&w, Self::ga_addons(slf)?))? } else { none(py) };
        let lk = v3(look)?;
        let u = v3(up)?;
        if !follow.is_none() {
            // 遠隔操作：操っている物（ドローンなど）の後ろから
            let com = v3(&follow.get_item(0)?)?;
            let dist = f64of(&follow.get_item(1)?)?;
            return Ok(PyTuple::new_bound(py, [np3(py, add(sub(com, mul(lk, dist)), mul(u, dist * 0.25))), look.clone(), up.clone()]));
        }
        if tp {
            let mut dist = 5.0;
            if seated {
                // 乗り物の大きさに合わせて離れる
                let c = ga(&w, "constructs")?.dget1(ga(&player, "seated")?.get_item(0)?)?;
                if !c.is_none() && ga(&c, "n_vox")?.is_truthy()? {
                    let pos = ga(&c, "pos")?;
                    let ext = f64of(&imp(py, "numpy")?.getattr("linalg")?.call_method1("norm", (pos.call_method1("max", (0,))?.call_method1("__sub__", (pos.call_method1("min", (0,))?,))?,))?)?;
                    dist = (ext * 1.3).clamp(5.0, 40.0);
                }
            }
            origin = play.call_method1("camera_clip", (&w, &origin, np3(py, add(mul(lk, -dist), mul(u, 0.8 + dist * 0.1)))))?;
        }
        Ok(PyTuple::new_bound(py, [origin, look.clone(), up.clone()]))
    }
    /// F9：見ている（座っている）構築物を OBJ で出力
    pub(crate) fn export_obj(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let w = Self::ga_w(slf)?;
        let p = ga(&w, "player")?;
        let mut target = none(py);
        let hit = Self::ga_hit(slf)?;
        if ga(&p, "seated")?.is_truthy()? {
            target = ga(&w, "constructs")?.dget1(ga(&p, "seated")?.get_item(0)?)?;
        } else if hit.is_truthy()? && hit.get_item("kind")?.eq("voxel")? {
            target = hit.get_item("construct")?;
        }
        if target.is_none() {
            World::message(wld(&w)?, "出力する構築物を見てください").map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?;
            return Ok(());
        }
        let r = World::export_obj(wld(&w)?, &IntoPy::<PyObject>::into_py(&target, w.py()).into_bound(w.py())).map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()));
        swallow(py, r, |e| {
            log_exc(py, e, "OBJ 出力失敗", vec![])?;
            World::message(wld(&w)?, &format!("OBJ 出力失敗: {}", e.value_bound(py).str()?)).map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?;
            Ok(())
        })?;
        Ok(())
    }
}
