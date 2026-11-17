//! ワールド：切り離し（多段式ロケットの段を切り離す。A-01）
//!
//! 切り離し（decoupler）は向きのある機器。キー（定義の params.key、ふつうは G）を押すたびに、まだ使っていない切り離しのうち
//! 操縦席からいちばん遠い 1 つ（同じ名前の切り離しはまとめて）が、出力面の結合を外す。外れた部分は別の構築物になり、
//! 互いに少し離れる向きに押される。1 回きり（使った切り離しはそのまま残る）。プログラムからは state["fire"] = True でも外せる。

use super::*;
use crate::game::construct::{con, Construct};

/// 分かれた部分を押し離す速さ（m/s）
const PUSH: f64 = 1.5;

impl World {
    /// 座っている人のキー（keys）で切り離しを使う
    pub(crate) fn fire_decouplers(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, keys: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let ids: Vec<usize> = Construct::dev_ids(con(c)?, Some(&IntoPy::<PyObject>::into_py("decoupler", py).into_bound(py)))?.extract()?;
        if ids.is_empty() {
            return Ok(());
        }
        let reg = Construct::g_reg(con(c)?)?;
        let devs = Construct::g_devs(con(c)?)?;
        // 押した瞬間だけ（押し続けても 1 段ずつ）
        let mut pressed = false;
        let mut scripted: Vec<usize> = vec![];
        let mut key = "g".to_string();
        for &i in &ids {
            let d = devs.get_item(i)?;
            let dd = crate::game::registry::dev_dict(&reg, &d.get_item("def")?)?;
            if let Ok(k) = dd.dget2("params", PyDict::new_bound(py))?.dget1("key") {
                if !k.is_none() {
                    key = k.extract()?;
                }
            }
            let st = d.dsetdefault("state", PyDict::new_bound(py))?;
            if st.dpop2("fire", false)?.is_truthy()? && !st.dget2("fired", false)?.is_truthy()? {
                scripted.push(i);
            }
        }
        let down = keys.contains(&key)?;
        let was: bool = c.getattr("_decouple_key").and_then(|x| x.extract()).unwrap_or(false);
        c.setattr("_decouple_key", down)?;
        if down && !was {
            pressed = true;
        }
        let mut fire: Vec<usize> = scripted;
        if pressed {
            // 操縦席からいちばん遠い、まだ使っていない切り離し（同じ名前の物も）
            let seat: Vec<usize> = Construct::dev_ids(con(c)?, Some(&IntoPy::<PyObject>::into_py("seat", py).into_bound(py)))?.extract()?;
            let centers = Construct::voxel_centers(con(c)?)?;
            let at = |i: usize| -> R<V3> { v3(&centers.get_item(i)?) };
            let s0 = match seat.first() { Some(&s) => at(s)?, None => v3(&Construct::center_of_mass(con(c)?)?)? };
            let mut best: Option<(f64, usize)> = None;
            for &i in &ids {
                let d = devs.get_item(i)?;
                if d.get_item("state")?.dget2("fired", false)?.is_truthy()? {
                    continue;
                }
                let q = sub(at(i)?, s0);
                let dist = dot(q, q);
                if best.map_or(true, |b| dist > b.0) {
                    best = Some((dist, i));
                }
            }
            if let Some((_, b)) = best {
                let name = devs.get_item(b)?.dget2("name", "")?;
                for &i in &ids {
                    let d = devs.get_item(i)?;
                    if i == b || (name.is_truthy()? && d.dget2("name", "")?.eq(&name)? && !d.get_item("state")?.dget2("fired", false)?.is_truthy()?) {
                        fire.push(i);
                    }
                }
            }
        }
        if fire.is_empty() {
            return Ok(());
        }
        fire.sort_unstable();
        fire.dedup();
        let com0 = v3(&Construct::center_of_mass(con(c)?)?)?;
        let mut any = false;
        for i in fire {
            devs.get_item(i)?.get_item("state")?.set_item("fired", true)?;
            any |= crate::game::actuator::act_cmd_detach(c, i)?;
        }
        if !any {
            return Ok(());
        }
        for part in Construct::split(con(c)?)?.iter() {
            Self::add_construct(slf, &part)?;
            // 分かれた部分を互いに離れる向きに押す
            let pc = v3(&Construct::center_of_mass(con(&part)?)?)?;
            let d = sub(pc, com0);
            let n = norm(d);
            if n > 1e-6 {
                let dv = [d[0] / n * PUSH, d[1] / n * PUSH, d[2] / n * PUSH];
                Self::con_add_vel(con(&part)?, dv)?;
                Self::con_add_vel(con(c)?, [-dv[0] * 0.3, -dv[1] * 0.3, -dv[2] * 0.3])?;
            }
            Construct::wake(con(&part)?, false)?;
        }
        Construct::wake(con(c)?, false)?;
        Self::message(slf, "切り離しました")?;
        Ok(())
    }
}
