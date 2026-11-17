//! 画面：狙い・構築物の見た目・名札・アイテムバー・重ねて出す物

use crate::game::planet::{pla, Planet};
use crate::game::world::{wld, World};
use crate::game::construct::{con, Construct};
use crate::game::PyDictOps;
use super::*;

#[pymethods]
impl Viewer {
    /// 狙っている物：枠と説明（今までの画面と同じ play.aim）
    pub(crate) fn _aim(slf: &Bound<'_, Self>, origin: &B<'_>, look: &B<'_>) -> R<()> {
        let py = slf.py();
        let play = imp(py, "main.client.play")?;
        let w = Self::ga_w(slf)?;
        let p = ga(&w, "player")?;
        let hit = if ga(&p, "seated")?.is_truthy()? {
            none(py)
        } else {
            World::raycast(wld(&w)?, &IntoPy::<PyObject>::into_py(p.call_method0("eye")?, w.py()).into_bound(w.py()), &IntoPy::<PyObject>::into_py(look, w.py()).into_bound(w.py()), IntoPy::<PyObject>::into_py(f64of(&get(&ga(&p, "defn")?, "reach", 8.0)?)?, w.py()).extract::<f64>(w.py())?).map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?
        };
        Self::sa_hit(slf, &hit)?;
        let gx = gfx(py)?;
        let (hl, gh) = (Self::ga_highlight(slf)?, Self::ga_ghost(slf)?);
        if hit.is_truthy()? {
            let k = hit.get_item("kind")?;
            if !(k.eq("voxel")? || k.eq("terrain")? || k.eq("water")?) {
                let f = acall(py, "aim", vec![hit.clone().unbind()])?;
                let r = Self::_each_addon(slf, &f, true)?; // MOD の物（街の建物・NPC など）
                if r.is_truthy()? {
                    let (txt, placeable) = (r.get_item(0)?, r.get_item(1)?.is_truthy()?);
                    hl.call_method0("hide")?;
                    if placeable {
                        let size = f64of(&get(&ga(&w, "settings")?, "voxel_size", 0.5)?)?;
                        let lv = Self::look_vectors(slf)?;
                        let (fwd, right) = (v3(&lv.get_item(0)?)?, v3(&lv.get_item(1)?)?);
                        let nrm = v3(&hit.get_item("normal")?)?;
                        let c = sub(add(v3(&hit.get_item("point")?)?, mul(nrm, size * 0.55)), v3(origin)?);
                        let np = imp(py, "numpy")?;
                        let ax = np.call_method1("stack", (vec![np3(py, right), np3(py, nrm), np3(py, fwd)],))?;
                        gx.call_method1("set_box", (&gh, np3(py, c), ax, np.call_method1("full", (3, size))?))?;
                        gh.call_method0("show")?;
                    } else {
                        gh.call_method0("hide")?;
                    }
                    Self::sa_aim_txt(slf, txt)?;
                    if placeable {
                        hit.set_item("placeable", true)?;
                    }
                    return Ok(());
                }
            }
        }
        let a = play.call_method1("aim", (&w, &hit, Self::look_vectors(slf)?))?;
        for (node, key) in [(&hl, "highlight"), (&gh, "ghost")] {
            let bx = a.get_item(key)?;
            if bx.is_none() {
                node.call_method0("hide")?;
            } else {
                gx.call_method1("set_box", (node, bx.get_item(0)?.call_method1("__sub__", (origin,))?, bx.get_item(1)?, bx.get_item(2)?))?;
                node.call_method0("show")?;
            }
        }
        Self::sa_aim_txt(slf, a.get_item("text")?)?;
        Ok(())
    }
    /// ほかの人（マルチプレイ）の体：MOD（VRM の見た目など）か、定義の色の箱
    pub(crate) fn _bodies(slf: &Bound<'_, Self>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = Self::ga_w(slf)?;
        let seen = PySet::empty_bound(py)?;
        let net = gd(&w, "net", none(py));
        let mut others: Vec<B<'_>> = vec![];
        if !net.is_none() {
            let o = gd(&net, "others", none(py));
            others = if o.is_callable() { o.call0()?.iter()?.collect::<R<_>>()? } else { o.call_method0("values")?.iter()?.collect::<R<_>>()? };
        }
        let mut list: Vec<(String, B<'_>)> = vec![("me".into(), ga(&w, "local_player")?)];
        for p in others {
            list.push((format!("p{}", ga(&p, "pid")?.str()?), p));
        }
        let bodies = Self::ga_bodies(slf)?;
        let game = Self::ga_game(slf)?;
        let errs = Self::ga_addon_err(slf)?;
        let ov = v3(origin)?;
        for (key, p) in list {
            seen.add(&key)?;
            let mut b = bodies.dget1(&key)?;
            let defid = get(&ga(&p, "defn")?, "id", py.None())?;
            if !b.is_none() {
                let bd = if b.hasattr("_defid")? { ga(&b, "_defid")? } else { none(py) };
                if !(bd.is_none() || bd.eq(&defid)?) {
                    b.call_method0("destroy")?; // 見た目を変えた：作り直す
                    bodies.set_item(&key, py.None())?;
                    b = none(py);
                }
            }
            if b.is_none() {
                let f = acall(py, "player_body", vec![p.clone().unbind()])?;
                let mb = Self::_each_addon(slf, &f, true)?; // MOD が体の見た目を引き受ける
                if !mb.is_none() {
                    bodies.set_item(&key, &mb)?;
                    b = mb;
                }
            }
            if b.is_none() {
                b = imp(py, "main.client.game")?.getattr("BoxBody")?.call1((&game, &p))?; // MOD が引き受けなければ定義の色の箱
                bodies.set_item(&key, &b)?;
            }
            if !b.hasattr("_defid")? {
                if let Err(e) = b.setattr("_defid", &defid) {
                    if !e.is_instance_of::<pyo3::exceptions::PyAttributeError>(py) {
                        return Err(e);
                    }
                }
            }
            let shown = if key == "me" {
                // 自分：三人称・カメラモードのときだけ
                Self::ga_third_person(slf)?.is_truthy()? || !ga(&game, "cam_mode")?.is_none()
            } else {
                let q = sub(v3(&ga(&p, "pos")?)?, ov);
                let sh = dot(q, q) < 500.0f64.powi(2);
                s.call_method1("_label", (&key, &p, origin, sh))?;
                sh
            };
            if let Err(e) = b.call_method1("update", (origin, shown, 1.0 / 60.0)) {
                if !is_exc(py, &e) {
                    return Err(e);
                }
                let k = PyTuple::new_bound(py, [tb(py, "body"), b.get_type().getattr("__name__")?]);
                if !errs.contains(&k)? {
                    errs.call_method1("add", (&k,))?;
                    log_exc(py, &e, "体の見た目でエラー", vec![])?;
                }
            }
        }
        let keys: Vec<B<'_>> = bodies.iter()?.collect::<R<_>>()?;
        for k in keys {
            if !seen.contains(&k)? {
                bodies.dpop1(&k)?.call_method0("destroy")?;
            }
        }
        let labels = Self::ga_labels(slf)?;
        let keys: Vec<B<'_>> = labels.iter()?.collect::<R<_>>()?;
        for k in keys {
            if !seen.contains(&k)? || k.eq("me")? {
                labels.dpop1(&k)?.call_method0("remove")?;
            }
        }
        Ok(())
    }
    /// ほかの人の頭の上の名前（いつもカメラの方を向く）
    pub(crate) fn _label(slf: &Bound<'_, Self>, key: &B<'_>, p: &B<'_>, origin: &B<'_>, shown: bool) -> R<()> {
        let py = slf.py();
        let labels = Self::ga_labels(slf)?;
        let mut t = labels.dget1(key)?;
        if t.is_none() {
            t = rs_text(py, Self::ga_root(slf)?, false, Some(BG), None)?;
            labels.set_item(key, &t)?;
        }
        let nm = ga(p, "name")?;
        let nm = if nm.is_truthy()? { nm } else { tb(py, "?") };
        let wh = t.call_method1("set", (nm, 0.22))?;
        let (w_, h_) = (f64of(&wh.get_item(0)?)?, f64of(&wh.get_item(1)?)?);
        t.call_method1("show", (shown,))?;
        if !shown {
            return Ok(());
        }
        let lv = Self::look_vectors(slf)?;
        let (right, look) = (v3(&lv.get_item(1)?)?, v3(&lv.get_item(3)?)?);
        let cu = cross(look, right); // カメラの上
        let c = sub(add(v3(&ga(p, "pos")?)?, mul(v3(&ga(p, "up")?)?, f64of(&ga(p, "height")?)? - f64of(&ga(p, "radius")?)? + 0.35)), v3(origin)?);
        let o = add(sub(c, mul(right, w_ / 2.0)), mul(cu, h_ / 2.0));
        let m = vec![right[0], right[1], right[2], 0.0, cu[0], cu[1], cu[2], 0.0, -look[0], -look[1], -look[2], 0.0, o[0], o[1], o[2], 1.0];
        ga(&t, "node")?.call_method1("setMat", (m,))?;
        Ok(())
    }
    /// 画面下のアイテムバー（アイコン・選んでいる枠は黄色。今までの画面と同じ並べ方）
    pub(crate) fn _hotbar(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let slots: i64 = imp(py, "main.client.inventory")?.getattr("SLOTS")?.extract()?;
        let u = imp(py, "main.client_rs.ui")?;
        let w = Self::ga_w(slf)?;
        let hb = ga(&ga(&w, "player")?, "hotbar")?;
        let items: Vec<B<'_>> = hb.iter()?.collect::<R<_>>()?;
        let mut allowed = vec![];
        for it in &items {
            allowed.push(it.is_truthy()? && World::item_allowed(wld(&w)?, &IntoPy::<PyObject>::into_py(it, w.py()).into_bound(w.py())).map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?.is_truthy()?);
        }
        let sel = Self::ga_sel(slf)?;
        let state = PyTuple::new_bound(py, [PyTuple::new_bound(py, &items).into_any(), sel.clone(), PyTuple::new_bound(py, &allowed).into_any()]);
        let cur = Self::ga_hotbar(slf)?;
        if !cur.is_none() && cur.get_item(0)?.eq(&state)? {
            return Ok(());
        }
        if !cur.is_none() {
            cur.get_item(1)?.call_method0("removeNode")?;
        }
        let gx = gfx(py)?;
        let root = gx.getattr("RsNode")?.call1((py.None(), "hotbar"))?;
        let (sw, gap, y) = (0.07, 0.008, -0.448);
        let selk: i64 = sel.extract()?;
        let view = gview(py)?;
        for k in 0..slots {
            let x = (k as f64 - (slots - 1) as f64 / 2.0) * (sw + gap);
            let sz = sw * if k == selk { 1.12 } else { 1.0 };
            let col = if k == selk { (1.0, 0.82, 0.25, 0.9) } else { (0.0, 0.0, 0.0, 0.5) };
            u.call_method1("rect_mesh", (&root, x - sz / 2.0, y - sz / 2.0, x + sz / 2.0, y + sz / 2.0, col))?;
            let it = &items[k as usize];
            if it.is_truthy()? {
                let tex = u.call_method1("item_tex", (ga(&w, "registry")?, it))?;
                let m = u.call_method1("rect_mesh", (&root, x - sw * 0.44, y - sw * 0.44, x + sw * 0.44, y + sw * 0.44,
                    (1.0, 1.0, 1.0, if allowed[k as usize] { 1.0 } else { 0.18 }), tex))?;
                view.call_method1("mesh_props", (m.getattr("mid")?, true, false, true, 0, 11))?;
            }
            let t = rs_text(py, root.clone(), true, None, Some(11))?;
            t.call_method1("set", ((k + 1).to_string(), 0.0138, (1.0, 1.0, 1.0, 0.8)))?;
            t.call_method1("place", (x - sw / 2.0 + 0.005, y + sw / 2.0 - 0.003))?;
        }
        if !gd(s, "_hud_shown", tb(py, true)).is_truthy()? {
            root.call_method0("hide")?;
        }
        Self::sa_hotbar(slf, PyTuple::new_bound(py, [state.into_any(), root]))?;
        Ok(())
    }
    /// 画面の上の文字（左上：場所・高度・速度・重さ、左下：知らせ）。プログラムなどの画面を開いている間は隠す
    pub(crate) fn _overlay(slf: &Bound<'_, Self>, ww: f64, hh: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = Self::ga_w(slf)?;
        let cm = ga(&Self::ga_game(slf)?, "cam_mode")?;
        let shown = Self::ga_editor(slf)?.is_none() && (cm.is_none() || ga(&cm, "ui_on")?.is_truthy()?);
        let texts = [Self::ga_hud(slf)?, Self::ga_msg(slf)?, Self::ga_aim_text(slf)?, Self::ga_item_text(slf)?];
        if shown != gd(s, "_hud_shown", tb(py, true)).is_truthy()? {
            Self::sa_hud_shown(slf, shown)?;
            for t in &texts {
                t.call_method1("show", (shown,))?;
            }
            ga(&Self::ga_cross(slf)?, "np")?.call_method0(if shown { "show" } else { "hide" })?;
            let hb = Self::ga_hotbar(slf)?;
            if !hb.is_none() {
                hb.get_item(1)?.call_method0(if shown { "show" } else { "hide" })?;
            }
        }
        if !shown {
            return Ok(());
        }
        let p = ga(&w, "player")?;
        let pl = World::nearest_planet(wld(&w)?, &IntoPy::<PyObject>::into_py(ga(&p, "pos")?, w.py()).into_bound(w.py())).map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?;
        let pos = ga(&p, "pos")?;
        let alt = if pl.is_truthy()? { f64of(&Planet::sdf(pla(&pl)?, &IntoPy::<PyObject>::into_py(pos.get_item(py.None())?, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?.get_item(0)?)? } else { 0.0 };
        let pname = if pl.is_truthy()? { ga(&pl, "name")?.str()?.to_string() } else { "宇宙".into() };
        let vel = norm(v3(&ga(&p, "vel")?)?);
        let mut lines = vec![format!("{}  高度 {} m  速度 {} m/s", pname, pf(py, alt, ".1f")?, pf(py, vel, ".1f")?)];
        if ga(&p, "jetpack")?.is_truthy()? {
            lines.push("ジェットパック ON".into());
        }
        let seated = ga(&p, "seated")?;
        let cs = ga(&w, "constructs")?;
        if seated.is_truthy()? && cs.contains(seated.get_item(0)?)? {
            let c = cs.get_item(seated.get_item(0)?)?;
            let cv = norm(v3(&Construct::velocity(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?)?);
            lines.push(format!("操縦中: {}  速度 {} m/s", ga(&c, "name")?.str()?, pf(py, cv, ".1f")?));
        }
        let net = gd(&w, "net", none(py));
        if !net.is_none() && ga(&net, "role")?.eq("client")? {
            lines.push(format!("マルチプレイ参加中（ホスト {}）  {} 人", ga(&net, "host_name")?.str()?, 1 + ga(&net, "others")?.len()?));
        } else if !net.is_none() {
            let a = net.call_method0("addresses")?.get_item(pyo3::types::PySlice::new_bound(py, 0, 1, 1))?;
            let j = PyString::new_bound(py, "  ").call_method1("join", (a,))?;
            let locked = net.call_method0("_info")?.dget2("locked", false)?.is_truthy()?;
            lines.push(format!("LAN に公開中 {}  {} 人{}", j.str()?, 1 + ga(&w, "remote")?.len()?, if locked { "（合言葉あり）" } else { "" }));
        }
        let ft = gd(s, "fps_text", tb(py, ""));
        if ft.is_truthy()? && gd(s, "debug_hud", tb(py, false)).is_truthy()? {
            // F3：処理の重さ
            lines.push(ft.str()?.to_string());
            let fd = gd(s, "fps_detail", tb(py, ""));
            if fd.is_truthy()? {
                lines.push(format!("  内訳 ms:{}", fd.str()?));
            }
        }
        if gd(s, "show_help", tb(py, false)).is_truthy()? {
            // F1：操作の説明
            let help: String = imp(py, "main.client.game")?.getattr("HELP")?.extract()?;
            lines.push(help.trim_end().to_string());
            for a in Self::ga_addons(slf)?.iter()? {
                let ht = gd(&a?, "help_text", tb(py, ""));
                if ht.is_truthy()? {
                    lines.push(ht.str()?.to_string());
                }
            }
        }
        if Self::_touch_mode(slf)? {
            // タッチ操作：画面のボタンとジョイスティック
        } else if !Self::ga_grabbed(slf)?.is_truthy()? && !ga(&Self::ga_ui(slf)?, "stack")?.is_truthy()? {
            lines.push("クリックで操作（マウスで見回す）・Esc でメニュー・I でインベントリ".into());
        }
        let aspect = ww / hh.max(1.0);
        let (hud, msg, aim_text, item_text) = (&texts[0], &texts[1], &texts[2], &texts[3]);
        hud.call_method1("set", (lines.join("\n"), 0.026))?;
        hud.call_method1("place", (-aspect / 2.0 + 0.015, 0.5 - 0.015))?;
        let now: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
        let all: Vec<B<'_>> = ga(&w, "msgs")?.iter()?.collect::<R<_>>()?;
        let mut msgs: Vec<String> = vec![];
        for m in all.iter().skip(all.len().saturating_sub(5)) {
            if now - f64of(&m.get_item(0)?)? < 8.0 {
                msgs.push(m.get_item(1)?.str()?.to_string());
            }
        }
        let k = PyDict::new_bound(py);
        k.set_item("max_width", aspect * 0.6)?;
        let h = f64of(&msg.call_method("set", (msgs.join("\n"), 0.024), Some(&k))?.get_item(1)?)?;
        msg.call_method1("show", (!msgs.is_empty(),))?;
        msg.call_method1("place", (-aspect / 2.0 + 0.015, -0.5 + 0.115 + h))?; // アイテムバーより上
        let at = gd(s, "_aim_txt", tb(py, ""));
        let k = PyDict::new_bound(py);
        k.set_item("max_width", aspect * 0.5)?;
        let tw = f64of(&aim_text.call_method("set", (&at, 0.022), Some(&k))?.get_item(0)?)?;
        aim_text.call_method1("show", (at.is_truthy()?,))?;
        aim_text.call_method1("place", (-tw / 2.0, -0.03))?;
        let it = imp(py, "main.client.play")?.call_method1("item_text", (&w, ga(&p, "hotbar")?.get_item(Self::ga_sel(slf)?)?, Self::ga_sel(slf)?, Self::ga_dir_index(slf)?))?;
        let r = item_text.call_method1("set", (it, 0.022))?;
        let (tw, th) = (f64of(&r.get_item(0)?)?, f64of(&r.get_item(1)?)?);
        item_text.call_method1("place", (-tw / 2.0, -0.5 + 0.13 + th))?;
        Self::_hotbar(slf)?;
        let st: Vec<B<'_>> = ga(&Self::ga_ui(slf)?, "stack")?.iter()?.collect::<R<_>>()?;
        for x in st {
            x.call_method1("update", (0.0,))?;
        }
        Ok(())
    }
}
