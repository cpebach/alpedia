//! ワールド：プレイヤー・出てくる場所・保存・メッセージ

use crate::game::planet::{pla, Planet};
use crate::game::PyDictOps;
use crate::game::player::{ply, Player};
use super::*;
use crate::game::construct::{con, Construct};

#[pymethods]
impl World {
    /// プレイヤーの見た目の定義。無ければ（MOD を外したなど）宇宙飛行士
    pub(crate) fn player_def<'py>(slf: &Bound<'py, Self>, def_id: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let reg = Self::g_registry(slf)?;
        let id: Bound<'py, PyAny> = if def_id.is_truthy()? { def_id.clone() } else { PyString::new_bound(py, "default").into_any() };
        let d = reg.dget2("players", &id)?;
        if d.is_none() {
            log(py)?.call_method1("warning", ("プレイヤーの見た目 '%s' の定義が無いので、宇宙飛行士にします", def_id))?;
            return reg.dget2("players", "default");
        }
        Ok(d)
    }
    #[pyo3(signature = (def_id, pos=None))]
    pub(crate) fn make_player<'py>(slf: &Bound<'py, Self>, def_id: &Bound<'py, PyAny>, pos: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let p = match pos {
            Some(p) if !p.is_none() => p.clone(),
            _ => Self::_default_spawn(slf)?,
        };
        imp(py, "main.player")?.getattr("Player")?.call1((Self::player_def(slf, def_id)?, p))
    }
    /// このワールドにいるプレイヤー全員（自分＋参加者）
    pub(crate) fn all_players<'py>(slf: &Bound<'py, Self>) -> R<Vec<Bound<'py, PyAny>>> {
        let mut out = vec![Self::g_local_player(slf)?];
        for p in Self::g_remote(slf)?.call_method0("values")?.iter()? {
            out.push(p?);
        }
        Ok(out)
    }
    /// 参加者の操作をホストで行う間、world.player をその人にする（with で使う）
    pub(crate) fn as_player(slf: &Bound<'_, Self>, p: PyObject) -> AsPlayer {
        AsPlayer { world: slf.clone().into_any().unbind(), p, prev: None }
    }
    /// いちばん近いプレイヤーまでの距離
    pub(crate) fn player_distance(slf: &Bound<'_, Self>, x: &Bound<'_, PyAny>) -> R<f64> {
        let x = v3(x)?;
        let mut best = f64::INFINITY;
        for p in Self::all_players(slf)? {
            best = best.min(norm(sub(x, v3(&p.getattr("pos")?)?)));
        }
        Ok(best)
    }
    /// 構築物 c の操縦席に座っている人が押しているキー
    pub(crate) fn keys_for<'py>(slf: &Bound<'py, Self>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PySet>> {
        let out = PySet::empty_bound(slf.py())?;
        let lp = Self::g_local_player(slf)?;
        let cid = Construct::g_id(con(&c)?)?;
        for p in Self::all_players(slf)? {
            if Self::is_seated_on(&p, &cid)? {
                let k = if p.is(&lp) { Self::g_input_keys(slf)? } else { p.getattr("input_keys")? };
                for x in k.iter()? {
                    out.add(x?)?;
                }
            }
        }
        Ok(out)
    }
    /// リスポーン：自宅など（MOD が決める）か最初の出現地点へ戻る
    #[pyo3(signature = (player=None))]
    pub(crate) fn respawn(slf: &Bound<'_, Self>, player: Option<&Bound<'_, PyAny>>) -> R<()> {
        let py = slf.py();
        let p = match player {
            Some(p) if p.is_truthy()? => p.clone(),
            _ => Self::g_player(slf)?,
        };
        let mut pos: Option<Bound<'_, PyAny>> = None;
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("respawn_point", (slf, &p))?;
            if !r.is_none() {
                pos = Some(a1(py, v3(&r)?));
                break;
            }
        }
        let pos = match pos { Some(p) => p, None => Self::_default_spawn(slf)? };
        Player::s_seated(ply(&p)?, py.None())?;
        Player::s_jetpack(ply(&p)?, false)?;
        p.setattr("pos", a1(py, v3(&pos)?))?;
        p.setattr("vel", a1(py, [0.0; 3]))?;
        let pl = Self::nearest_planet(slf, &p.getattr("pos")?)?;
        if !pl.is_none() {
            p.setattr("up", Planet::up(pla(&pl)?, &IntoPy::<PyObject>::into_py(p.getattr("pos")?, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?)?;
        }
        let mut msgs = vec![];
        for a in Self::addons_v(slf)? {
            let m = a.call_method1("on_respawn", (slf, &p))?;
            if m.is_truthy()? {
                msgs.push(m.str()?.to_string());
            }
        }
        Self::message(slf, &format!("リスポーンしました{}", if msgs.is_empty() { String::new() } else { format!("：{}", msgs.join(" ")) }))
    }
    pub(crate) fn _default_spawn<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let pls = Self::g_planets(slf)?;
        if pls.len()? == 0 {
            return Ok(a1(py, [0.0; 3]));
        }
        let p = pls.get_item(0)?;
        let r: f64 = p.call_method1("surface_radius", (arr(py, vec![0.0, 1.0, 0.0], &[1, 3])?,))?.get_item(0)?.extract()?;
        let c = v3(&p.getattr("center")?)?;
        Ok(a1(py, [c[0], c[1] + (r + 2.0), c[2]]))
    }

    /// 保存する中身（ファイル名 → JSON）。保存と、マルチプレイの参加者へ送るのに使う
    pub(crate) fn snapshot<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let st = Self::settings(slf)?;
        st.set_item("time", slf.borrow().time)?;
        st.set_item("format", imp(py, "main.savefmt")?.getattr("FORMAT")?)?;
        for a in Self::addons_v(slf)? {
            a.call_method1("on_save", (slf,))?;
        }
        let pls = PyList::empty_bound(py);
        for pl in Self::g_planets(slf)?.iter()? {
            let pl = pl?;
            let d = PyDict::new_bound(py);
            d.update(Planet::to_dict(pla(&pl)?).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?.downcast::<PyDict>()?.as_mapping())?;
            if let Some(o) = opt(&pl, "defn_override")? {
                if o.is_truthy()? {
                    d.set_item("override", o)?;
                }
            }
            pls.append(d)?;
        }
        st.set_item("planets", pls)?;
        let out = PyDict::new_bound(py);
        out.set_item("world.json", &st)?;
        let cl = PyList::empty_bound(py);
        let sp = PyDict::new_bound(py);
        for c in Self::cons_v(slf)? {
            cl.append(Construct::to_dict(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?)?;
        }
        for c in Self::cons_v(slf)? {
            sp.set_item(Construct::g_id(con(&c)?)?, Construct::spawn_dict(con(&c)?).map(|v| IntoPy::<PyObject>::into_py(v, c.py()).into_bound(c.py()))?)?;
        }
        out.set_item("constructs.json", cl)?;
        out.set_item("spawn.json", sp)?;
        out.set_item("player.json", Self::g_local_player(slf)?.call_method0("to_dict")?)?;
        let md = PyDict::new_bound(py);
        md.set_item("enabled", PyList::new_bound(py, Self::g_mods(slf)?.iter()?.collect::<R<Vec<_>>>()?))?;
        out.set_item("mods.json", md)?;
        Ok(out)
    }
    pub(crate) fn save(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let net = Self::g_net(slf)?;
        if !net.is_none() && !net.call_method0("can_save")?.is_truthy()? {
            return Self::message(slf, "保存はホストが行います（ワールドはホストの PC にあります）");
        }
        let snap = Self::snapshot(slf)?;
        if !net.is_none() {
            snap.update(net.call_method0("save_extra")?.downcast::<PyDict>()?.as_mapping())?;      // 参加した人の状態（players.json）
        }
        let dir = Self::g_dir(slf)?;
        for (f, d) in snap.iter() {
            world_write_json(&dir.call_method1("__truediv__", (f,))?, &d)?;
        }
        log(py)?.call_method1("info", ("保存しました: %s", &dir))?;
        Self::message(slf, "保存しました")
    }

    // ------------------------------------------------------------------ 共通
    pub(crate) fn message(slf: &Bound<'_, Self>, text: &str) -> R<()> {
        let py = slf.py();
        let sink = Self::g_msg_sink(slf)?;
        let to = if sink.is_none() { Self::g_msgs(slf)? } else { sink };
        let t: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
        to.call_method1("append", ((t, text),))?;
        Ok(())
    }
}
