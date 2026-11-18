//! 画面：描く・終わり・動かす（run）

use crate::game::world::{wld, World};
use crate::game::construct::{con, Construct};
use crate::game::PyDictOps;
use super::*;

#[pymethods]
impl Viewer {
    pub(crate) fn _render(slf: &Bound<'_, Self>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let mut pt = std::time::Instant::now();
        let s = slf.as_any();
        let w = Self::ga_w(slf)?;
        let views = Self::ga_views(slf)?;
        let cons = ga(&w, "constructs")?;
        let vk: Vec<B<'_>> = views.iter()?.collect::<R<_>>()?;
        for cid in vk {
            if !cons.contains(&cid)? {
                views.dpop1(&cid)?.call_method0("remove")?;
            }
        }
        let loaded = Self::ga_loaded(slf)?;
        loaded.call_method1("intersection_update", (py.import_bound("builtins")?.getattr("set")?.call1((&cons,))?,))?;
        let g = Self::ga_cfg(slf)?.get_item("graphics")?;
        let far = f64of(&get(&g, "view_distance", 3000)?)?;
        let decal_dist = f64of(&get(&g, "decal_distance", 30.0)?)?;
        let ld = get(&g, "load_distance", 0)?;
        let load = if ld.is_truthy()? { f64of(&ld)? } else { 0.0 };
        let o = v3(origin)?;
        let anchor = Self::ga_anchor(slf)?;
        if anchor.is_none() || norm(sub(o, v3(&anchor)?)) > 256.0 {
            Self::sa_anchor(slf, np3(py, o))?;
            let av: i64 = Self::ga_anchor_ver(slf)?.extract()?;
            Self::sa_anchor_ver(slf, av + 1)?;
        }
        let an = Self::ga_anchor(slf)?;
        let a3 = v3(&an)?;
        let d0 = sub(a3, o);
        Self::ga_static_root(slf)?.call_method1("setPos", (d0[0], d0[1], d0[2]))?;
        let ba = World::_bounds_arrays(wld(&w)?).map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?;
        let cs: Vec<B<'_>> = ba.get_item(0)?.iter()?.collect::<R<_>>()?;
        let cc = ba.get_item(1)?;
        let rr = ba.get_item(3)?;
        let mut dists = vec![];
        if !cs.is_empty() {
            let np = imp(py, "numpy")?;
            let cr: numpy::PyReadonlyArray2<f64> = np.call_method1("ascontiguousarray", (&cc, np.getattr("float64")?))?.extract()?;
            let rrr: numpy::PyReadonlyArray1<f64> = np.call_method1("ascontiguousarray", (&rr, np.getattr("float64")?))?.extract()?;
            let ca = cr.as_array();
            let ra = rrr.as_array();
            for i in 0..cs.len() {
                let d = [ca[[i, 0]] - o[0], ca[[i, 1]] - o[1], ca[[i, 2]] - o[2]];
                dists.push((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - ra[i]);
            }
        }
        let player = ga(&w, "player")?;
        let seatv = ga(&player, "seated")?;
        let seat = if seatv.is_truthy()? { seatv.get_item(0)? } else { none(py) };
        let lod_list = PyList::empty_bound(py);
        let fno: i64 = gd(s, "frame_no", tb(py, 0)).extract::<i64>()? + 1;
        Self::sa_frame_no(slf, fno)?;
        let time = imp(py, "time")?;
        let budget = f64of(&time.call_method0("perf_counter")?)? + 0.008;
        let shape = imp(py, "main.shape")?;
        let rm = render_mod(py)?;
        let root = Self::ga_root(slf)?;
        let static_root = Self::ga_static_root(slf)?;
        let anchor_ver = Self::ga_anchor_ver(slf)?;
        let hash = py.import_bound("builtins")?.getattr("hash")?;
        scene_prof(0, &mut pt);
        for (c, dist) in cs.iter().zip(dists.iter()) {
            let cid = ga(c, "id")?;
            if !cons.dget1(&cid)?.is(c) {
                continue;
            }
            let mut v = views.dget1(&cid)?;
            if *dist > far {
                if !v.is_none() {
                    v.call_method1("set_visible", (false,))?;
                }
                continue;
            }
            let seated = seat.eq(&cid)?;
            let near = load <= 0.0 || seated || *dist < load || (loaded.contains(&cid)? && *dist < load * 1.1);
            if !near {
                if !v.is_none() {
                    views.dpop1(&cid)?.call_method0("remove")?;
                    c.setattr("_mesh_cache", py.None())?;
                }
                loaded.call_method1("discard", (&cid,))?;
                lod_list.append(c)?;
                continue;
            }
            if v.is_none() {
                // 大きな物・アンカーで固定された建物（街の建物はたくさん一度に出てくる）は、形を裏で作る（できるまでは遠くの見た目の箱）
                let big = ga(c, "n_vox")?.extract::<i64>()? > 150 || Construct::anchored_b(con(&c)?)?;
                if !seated && big && !shape.call_method1("request_mesh", (c,))?.is_truthy()? {
                    lod_list.append(c)?;
                    continue;
                }
                if f64of(&time.call_method0("perf_counter")?)? > budget && !loaded.contains(&cid)? && !seated {
                    lod_list.append(c)?;
                    continue;
                }
                v = rm.getattr("ConstructView")?.call1((&root, py.None(), c))?;
                views.set_item(&cid, &v)?;
            }
            loaded.call_method1("add", (&cid,))?;
            v.call_method1("set_visible", (true,))?;
            let tf = Construct::render_transform(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?;
            // 公転する惑星と一緒に動いている物は毎フレーム置き直す（L-08）
            let static_ = !tf.is_none() && !seated && con(&c)?.borrow().frame < 0 && (ga(c, "is_static")?.is_truthy()? || ga(c, "asleep")?.is_truthy()?)
                && !Construct::dev_ids(con(&c)?, Some(&IntoPy::<PyObject>::into_py("thruster", c.py()).into_bound(c.py()))).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?.is_truthy()?;
            let key = if static_ { PyTuple::new_bound(py, [ga(c, "version")?, tf.get_item(2)?, anchor_ver.clone()]).into_any() } else { none(py) };
            let vd = ga(&v, "__dict__")?;
            if key.is_none() || !get(&vd, "_static_key", py.None())?.eq(&key)? {
                v.setattr("_static_key", &key)?;
                if static_ {
                    let k = PyDict::new_bound(py);
                    k.set_item("static_root", &static_root)?;
                    k.set_item("anchor", &an)?;
                    v.call_method("update", (origin,), Some(&k))?;
                } else {
                    v.call_method1("update", (origin,))?;
                }
                v.call_method1("update_decals", (origin, origin, decal_dist))?;
            }
            let hv: i64 = hash.call1((&cid,))?.extract()?;
            if key.is_none() || (fno as i128 + hv as i128).rem_euclid(6) == 0 {
                // 看板（止まっている物は6フレームに1回）
                v.call_method1("update_displays", (origin, far))?;
            }
        }
        scene_prof(1, &mut pt);
        Self::ga_lod(slf)?.call_method1("update", (lod_list, origin))?;
        scene_prof(2, &mut pt);
        let lc = PyList::empty_bound(py);
        for k in loaded.iter()? {
            let k = k?;
            if cons.contains(&k)? {
                lc.append(cons.get_item(&k)?)?;
            }
        }
        Self::ga_lights(slf)?.call_method1("update", (lc, origin, f64of(&ga(&w, "time")?)?))?;
        scene_prof(3, &mut pt);
        let ppos = ga(&player, "pos")?;
        let npl = World::nearest_planet(wld(&w)?, &IntoPy::<PyObject>::into_py(&ppos, w.py()).into_bound(w.py())).map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?;
        let carve = Self::ga_carve(slf)?;
        carve.call_method1("update", (&npl, &ppos, origin))?;
        let patch = Self::ga_patch(slf)?;
        patch.call_method1("update", (&npl, &ppos, origin, &carve))?;
        for pv in Self::ga_planet_views(slf)?.iter()? {
            let pv = pv?;
            pv.call_method1("update", (origin, &ppos, patch.call_method1("frame_for", (ga(&pv, "p")?,))?))?;
        }
        scene_prof(4, &mut pt);
        let now = time.call_method0("time")?;
        for x in Self::ga_scenes(slf)?.iter()? {
            let x = x?;
            let (a, sv) = (x.get_item(0)?, x.get_item(1)?);
            let on = gd(&a, "alert", tb(py, false)).is_truthy()? || gd(&a, "lights", tb(py, false)).is_truthy()?;
            sv.call_method1("update", (origin, on, &now))?;
        }
        // 空の色
        let mut air = 0.0;
        if !npl.is_none() {
            let p = (f64of(&get(&ga(&npl, "defn")?, "atmosphere_pressure", 101.3)?)? / 101.3).clamp(0.0, 1.0);
            let rad = f64of(&ga(&npl, "radius")?)?;
            let alt = (norm(sub(o, v3(&ga(&npl, "center")?)?)) - rad).max(0.0);
            air = p * (-alt / (rad * 0.25).max(30.0)).exp();
        }
        scene_prof(5, &mut pt);
        let c: Vec<f64> = (0..3).map(|i| SKY_SPACE[i] + (SKY_DAY[i] - SKY_SPACE[i]) * air).collect();
        Self::ga_view(slf)?.call_method1("set_clear", (c[0], c[1], c[2]))?;
        // 空気遠近（A-02 のシェーダー）：遠くの物を地平線の空の色に寄せる。空気が薄いほど弱い（宇宙では無し）
        let haze: Vec<f64> = (0..3).map(|i| c[i] * 0.75 + HAZE[i] * 0.25 * air).collect();
        let _ = Self::ga_view(slf)?.call_method1("set_fog", (haze[0], haze[1], haze[2], air / FOG_VIS));
        Ok(())
    }
    /// 画面から消す（メニューに戻るとき）
    pub(crate) fn destroy(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let snd = Self::ga_snd(slf)?;
        if !snd.is_none() {
            snd.call_method1("set_bgm", (py.None(),))?;
            snd.call_method1("set_override", (py.None(),))?;
            snd.call_method0("stop_loops")?;
            snd.call_method1("update", (10.0,))?; // 曲を止める
        }
        let f = acall(py, "destroy", vec![])?;
        Self::_each_addon(slf, &f, false)?;
        Self::sa_addons(slf, PyList::empty_bound(py))?;
        let r = ga(&Self::ga_game(slf)?, "touch_hud").and_then(|h| h.call_method0("destroy"));
        swallow(py, r, |_| Ok(()))?;
        for b in Self::ga_bodies(slf)?.call_method0("values")?.iter()? {
            let b = b?;
            swallow(py, b.call_method0("destroy"), |_| Ok(()))?;
        }
        Self::sa_bodies(slf, PyDict::new_bound(py))?;
        for t in Self::ga_labels(slf)?.call_method0("values")?.iter()? {
            t?.call_method0("remove")?;
        }
        Self::sa_labels(slf, PyDict::new_bound(py))?;
        for v in Self::ga_views(slf)?.call_method0("values")?.iter()? {
            v?.call_method0("remove")?;
        }
        Self::sa_views(slf, PyDict::new_bound(py))?;
        for n in [Self::ga_root(slf)?, ga(&Self::ga_hud(slf)?, "node")?, ga(&Self::ga_msg(slf)?, "node")?, ga(&Self::ga_aim_text(slf)?, "node")?,
                  ga(&Self::ga_item_text(slf)?, "node")?, ga(&Self::ga_cross(slf)?, "np")?] {
            n.call_method0("removeNode")?;
        }
        let hb = Self::ga_hotbar(slf)?;
        if !hb.is_none() {
            hb.get_item(1)?.call_method0("removeNode")?;
        }
        let st: Vec<B<'_>> = ga(&Self::ga_ui(slf)?, "stack")?.iter()?.collect::<R<_>>()?;
        for x in st {
            x.call_method0("close")?;
        }
        let ed = Self::ga_editor(slf)?;
        if !ed.is_none() && ed.hasattr("close")? {
            swallow(py, ed.call_method0("close"), |_| Ok(()))?;
        }
        Self::ga_shim(slf)?.call_method1("destroy", (ga(&Self::ga_game(slf)?, "ui_root")?,))?;
        Self::ga_view(slf)?.call_method1("grab", (false,))?;
        Ok(())
    }
    pub(crate) fn run(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let t = imp(py, "time")?;
        let mut last: f64 = t.call_method0("perf_counter")?.extract()?;
        while !Self::ga_quit(slf)?.is_truthy()? {
            let now: f64 = t.call_method0("perf_counter")?.extract()?;
            let dt = (now - last).min(0.1);
            last = now;
            Self::frame(slf, IntoPy::<PyObject>::into_py(dt, slf.py()).extract::<f64>(slf.py())?).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?;
        }
        Self::ga_w(slf)?.call_method0("save")?;
        Ok(())
    }
}
