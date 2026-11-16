//! ワールド：ジェットパック・触る・キー・書き出しと読み込み

use super::*;

#[pymethods]
impl World {
    /// ジェットパックを使えるか（燃料などは MOD が決める）：(使えるか, 噴射の向き)
    pub(crate) fn jetpack<'py>(slf: &Bound<'py, Self>, player: &Bound<'py, PyAny>, dt: f64, thrust_dir: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
        let mut flying = player.getattr("jetpack")?;
        let mut td = thrust_dir.clone();
        for a in Self::addons_v(slf)? {
            if flying.is_truthy()? {
                let r = a.call_method1("jetpack", (slf, player, dt, &td))?;
                flying = r.get_item(0)?;
                td = r.get_item(1)?;
            }
        }
        Ok(PyTuple::new_bound(slf.py(), [flying, td]))
    }
    pub(crate) fn interact(slf: &Bound<'_, Self>, hit: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        let p = Self::g(slf, "player")?;
        if p.getattr("seated")?.is_truthy()? {
            p.setattr("seated", py.None())?;
            return Self::message(slf, "操縦席から降りました");
        }
        if !hit.is_truthy()? || !hit.get_item("kind")?.eq("voxel")? {
            return Ok(());
        }
        let c = hit.get_item("construct")?;
        let i: usize = hit.get_item("index")?.extract()?;
        let d = c.getattr("devs")?.get_item(i)?;
        let reg = Self::g(slf, "registry")?;
        let cname = c.getattr("name")?.str()?.to_string();
        if d.is_truthy()? {
            let dd = reg.call_method1("device", (d.get_item("def")?,))?;
            let dtype: Option<String> = if dd.is_truthy()? { Some(dd.get_item("type")?.extract()?) } else { None };
            if dtype.as_deref() == Some("seat") {
                p.setattr("seated", (c.getattr("id")?, c.getattr("cells")?.get_item(i)?))?;
                return Self::message(slf, &format!("{cname} の操縦席に座りました（E で降りる）"));
            }
            for a in Self::addons_v(slf)? {
                let msg = a.call_method1("interact_device", (slf, &c, i, &d, &dd))?;
                if msg.is_truthy()? {
                    return Self::message(slf, &msg.str()?.to_string());
                }
            }
            if dtype.as_deref() == Some("tank") {
                // E でタンクの中身を 燃料満タン → 空気満タン → 空 と切り替え
                let cap: f64 = getd(&dd.get_item("params")?, "capacity", 0)?.extract()?;
                let cont = d.get_item("state")?.call_method1("setdefault", ("contents", PyDict::new_bound(py)))?;
                let dn = d.get_item("name")?.str()?.to_string();
                if fget(&cont, "fuel", 0.0)? >= cap * 0.99 {
                    cont.call_method0("clear")?;
                    cont.set_item("air", cap)?;
                    Self::message(slf, &format!("{dn}: 空気を満タンにしました（E で空に）"))?;
                } else if fget(&cont, "air", 0.0)? >= cap * 0.99 {
                    cont.call_method0("clear")?;
                    Self::message(slf, &format!("{dn}: 空にしました（E で燃料満タン）"))?;
                } else {
                    cont.call_method0("clear")?;
                    cont.set_item("fuel", cap)?;
                    Self::message(slf, &format!("{dn}: 燃料を満タンにしました（E で空気に）"))?;
                }
                return Ok(());
            }
            if dtype.as_deref() == Some("actuator") {
                let m = crate::game::actuator::act_execute_preset(slf.as_any(), &c, i)?;
                return Self::message(slf, &m);
            }
            if let Some(m) = crate::game::devices::dev_interact(py, &c, i, Some(p.clone().unbind()))? {
                if !m.is_empty() {
                    Self::message(slf, &m)?;
                }
            }
            return Ok(());
        }
        for a in Self::addons_v(slf)? {
            let msg = a.call_method1("interact", (slf, &c, i))?;
            if msg.is_truthy()? {
                return Self::message(slf, &msg.str()?.to_string());
            }
        }
        let m = reg.call_method1("material", (c.getattr("mats")?.get_item(i)?,))?;
        let tm: f64 = c.call_method0("total_mass")?.extract()?;
        Self::message(slf, &format!("{cname}: {} / ボクセル {} / 質量 {:.1} t", m.get_item("name")?.str()?, c.getattr("n_vox")?, tm / 1000.0))
    }
    /// 操縦席で押されたキー：キー割り当てのあるアクチュエータを切り替える
    pub(crate) fn key_pressed(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> R<()> {
        let s = Self::g(slf, "player")?.getattr("seated")?;
        if !s.is_truthy()? {
            return Ok(());
        }
        let c = Self::g(slf, "constructs")?.call_method1("get", (s.get_item(0)?,))?;
        if !c.is_truthy()? {
            return Ok(());
        }
        let reg = Self::g(slf, "registry")?;
        for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
            let d = d?;
            if !d.is_truthy()? {
                continue;
            }
            let dd = reg.call_method1("device", (d.get_item("def")?,))?;
            if dd.is_truthy()? && dd.get_item("type")?.eq("actuator")? && getd(&dd.get_item("params")?, "key", slf.py().None())?.eq(key)? {
                let pr = getd(&d.get_item("state")?, "preset", slf.py().None())?;
                if pr.is_truthy()? && getd(&pr, "action", slf.py().None())?.eq("none")? {
                    continue;               // 動作「なし」：キーでは動かさない（プログラムが動かす）
                }
                let m = crate::game::actuator::act_execute_preset(slf.as_any(), &c, i)?;
                Self::message(slf, &m)?;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------ OBJ
    pub(crate) fn export_obj<'py>(slf: &Bound<'py, Self>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let cname: String = c.getattr("name")?.str()?.to_string();
        let id: String = c.getattr("id")?.str()?.to_string();
        let safe: String = cname.chars().map(|ch| if ch.is_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' }).collect();
        let name = format!("{safe}_{}", id.chars().take(4).collect::<String>());
        let ed = Self::g(slf, "exports_dir")?;
        let kw = PyDict::new_bound(py);
        kw.set_item("scripts_dir", Self::g(slf, "dir")?.call_method1("__truediv__", ("scripts",))?)?;
        let path = imp(py, "main.objio")?.call_method("export_construct", (c, &ed, name), Some(&kw))?;
        Self::message(slf, &format!("OBJ 出力: {}/{}", ed.getattr("name")?.str()?, path.getattr("name")?.str()?))?;
        Ok(path)
    }
    /// ボクセル一覧を置くのに必要な資源（機器は機器の費用、それ以外は素材の費用）
    pub(crate) fn voxels_cost<'py>(slf: &Bound<'py, Self>, voxels: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
        let py = slf.py();
        let total = PyDict::new_bound(py);
        for v in voxels.iter()? {
            let v = v?;
            let (mat, dev) = (v.get_item(1)?, v.get_item(2)?);
            let item: (&str, Bound<'_, PyAny>) = if dev.is_truthy()? { ("device", dev.get_item("def")?) } else { ("material", mat) };
            let item: PyObject = item.into_py(py);
            let cost = Self::item_cost(slf, item.bind(py))?;
            for (k, x) in cost.downcast::<PyDict>()?.iter() {
                let cur: f64 = total.get_item(&k)?.map(|v| v.extract()).transpose()?.unwrap_or(0.0);
                total.set_item(k, cur + x.extract::<f64>()?)?;
            }
        }
        Ok(total)
    }
    #[pyo3(signature = (path, position=None, look_dir=None, **opts))]
    pub(crate) fn import_obj<'py>(slf: &Bound<'py, Self>, path: &Bound<'py, PyAny>, position: Option<&Bound<'py, PyAny>>, look_dir: Option<&Bound<'py, PyAny>>,
                       opts: Option<&Bound<'py, PyDict>>) -> R<Bound<'py, PyAny>> {
        let data = imp(slf.py(), "main.objio")?.call_method("import_construct", (Self::g(slf, "registry")?, path), opts)?;
        Self::place_import(slf, &data, position, look_dir)
    }
    /// 読み込んだ構築物（objio.import_construct の結果）をプレイヤーの前に置く。サバイバルでは資源を払う
    #[pyo3(signature = (data, position=None, look_dir=None))]
    pub(crate) fn place_import<'py>(slf: &Bound<'py, Self>, data: &Bound<'py, PyAny>, position: Option<&Bound<'py, PyAny>>, look_dir: Option<&Bound<'py, PyAny>>)
        -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let none = py.None().into_bound(py);
        let vox = data.get_item("voxels")?;
        if !vox.is_truthy()? {
            Self::message(slf, "ボクセルが0個でした")?;
            return Ok(none);
        }
        if !Self::pay(slf, Self::voxels_cost(slf, &vox)?.as_any())? {
            return Ok(none);
        }
        let size: f64 = data.get_item("size")?.extract()?;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in vox.iter()? {
            let c = v3(&v?.get_item(0)?)?;
            for k in 0..3 {
                lo[k] = lo[k].min(c[k]);
                hi[k] = hi[k].max(c[k]);
            }
        }
        let player = Self::g(slf, "player")?;
        let pp = player.getattr("pos")?;
        let pl = Self::nearest_planet(slf, &pp)?;
        let up = if pl.is_truthy()? { v3(&pl.call_method1("up", (&pp,))?)? } else { [0.0, 1.0, 0.0] };
        let look = match look_dir { Some(l) if !l.is_none() => v3(l)?, _ => [0.0, 0.0, 1.0] };
        let rm = Self::frame_of_import(up, look);
        let fwd = [rm[0][2], rm[1][2], rm[2][2]];
        let ext = [(hi[0] - lo[0] + 1.0) * size, (hi[1] - lo[1] + 1.0) * size, (hi[2] - lo[2] + 1.0) * size];
        let position = match position {
            Some(p) if !p.is_none() => v3(p)?,
            _ => {
                let em = ext[0].max(ext[1]).max(ext[2]);
                let p0 = v3(&pp)?;
                let mut base = [p0[0] + fwd[0] * (em * 0.7 + 3.0), p0[1] + fwd[1] * (em * 0.7 + 3.0), p0[2] + fwd[2] * (em * 0.7 + 3.0)];
                if !pl.is_none() {
                    // 地表の上に置く
                    let dd: f64 = pl.call_method1("sdf", (arr(py, base.to_vec(), &[1, 3])?,))?.get_item(0)?.extract()?;
                    for k in 0..3 {
                        base[k] = base[k] - up[k] * dd + up[k] * 0.5;
                    }
                }
                base
            }
        };
        let lmin = [lo[0] * size, lo[1] * size, lo[2] * size];
        let co = [ext[0] / 2.0 + lmin[0], 0.0 + lmin[1], ext[2] / 2.0 + lmin[2]];
        let origin = sub(position, crate::game::construct::mul_t(co, &rm));
        let kw = PyDict::new_bound(py);
        kw.set_item("size", size)?;
        kw.set_item("links", data.get_item("links")?)?;
        kw.set_item("name", data.get_item("name")?)?;
        let c = imp(py, "main.construct")?.getattr("Construct")?.call_method("from_voxels",
            (Self::g(slf, "registry")?, &vox, a1(py, origin), crate::game::construct::mat(py, &rm)?), Some(&kw))?;
        c.setattr("storage", py.get_type_bound::<PyDict>().call1((getd(data, "storage", PyDict::new_bound(py))?,))?)?;
        Self::add_construct(slf, &c)?;
        // 一緒に入っていたプログラムを scripts へ（同じ名前で中身が違うものがあれば上書きしない）
        let sdir = Self::g(slf, "dir")?.call_method1("__truediv__", ("scripts",))?;
        sdir.call_method("mkdir", (), Some(&[("exist_ok", true)].into_py_dict_bound(py)))?;
        let enc = [("encoding", "utf-8")].into_py_dict_bound(py);
        let mut kept = vec![];
        let sc = getd(data, "scripts", py.None())?;
        if sc.is_truthy()? {
            for (f, text) in sc.downcast::<PyDict>()?.iter() {
                let fs: String = f.str()?.to_string();
                if fs.contains('/') || fs.contains('\\') || fs.contains("..") {
                    continue;
                }
                let fp = sdir.call_method1("__truediv__", (&f,))?;
                if !fp.call_method0("exists")?.is_truthy()? {
                    fp.call_method("write_text", (&text,), Some(&enc))?;
                } else if !fp.call_method("read_text", (), Some(&enc))?.eq(&text)? {
                    kept.push(fs);
                }
            }
        }
        if !kept.is_empty() {
            Self::message(slf, &format!("同じ名前のプログラムが既にあるので、そちらを使います: {}", kept.join(", ")))?;
        }
        Self::message(slf, &format!("読み込み: {}（{} ボクセル）", data.get_item("name")?.str()?, c.getattr("n_vox")?))?;
        Ok(c)
    }
}
