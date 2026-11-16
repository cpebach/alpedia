//! ワールド：ブロックを置く・消す・掘る・崩れる

use super::*;

#[pymethods]
impl World {
    // ------------------------------------------------------------------ 編集
    /// 置く・読み込む費用を払う（資源のルールを持つ MOD が決める。払えなければ False）
    pub(crate) fn pay(slf: &Bound<'_, Self>, cost: &Bound<'_, PyAny>) -> R<bool> {
        let py = slf.py();
        for a in Self::addons_v(slf)? {
            if !a.call_method1("pay", (slf, py.get_type_bound::<PyDict>().call1((cost,))?))?.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn refund(slf: &Bound<'_, Self>, cost: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        for a in Self::addons_v(slf)? {
            a.call_method1("refund", (slf, py.get_type_bound::<PyDict>().call1((cost,))?))?;
        }
        Ok(())
    }
    #[pyo3(signature = (c, i=None))]
    pub(crate) fn _device_placed(slf: &Bound<'_, Self>, c: &Bound<'_, PyAny>, i: Option<usize>) -> R<()> {
        let py = slf.py();
        let i = match i { Some(i) => i, None => return Ok(()) };
        let d = c.getattr("devs")?.get_item(i)?;
        if !d.is_truthy()? {
            return Ok(());
        }
        if Self::creative(slf)? {                // クリエイティブ：置いたタンクは燃料満タン（E で中身を切り替え）
            let dd = Self::g(slf, "registry")?.call_method1("device", (d.get_item("def")?,))?;
            let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
            if getd(&dd, "type", py.None())?.eq("tank")? {
                let cap: f64 = getd(&getd(&dd, "params", PyDict::new_bound(py))?, "capacity", 0)?.extract()?;
                let ct = PyDict::new_bound(py);
                ct.set_item("fuel", cap)?;
                d.get_item("state")?.set_item("contents", ct)?;
            }
        }
        for a in Self::addons_v(slf)? {
            a.call_method1("on_device_placed", (slf, c, i))?;
        }
        Ok(())
    }
    /// そのアイテムを今使えるか（MOD が絞っていれば従う）
    pub(crate) fn item_allowed(slf: &Bound<'_, Self>, item: &Bound<'_, PyAny>) -> R<bool> {
        let t = PyTuple::new_bound(slf.py(), item.iter()?.collect::<R<Vec<_>>>()?);
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("item_allowed", (slf, &t))?;
            if r.is_instance_of::<pyo3::types::PyBool>() && !r.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn blueprints_allowed(slf: &Bound<'_, Self>) -> R<bool> {
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("blueprints_allowed", (slf,))?;
            if r.is_instance_of::<pyo3::types::PyBool>() && !r.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// ブロックのパレットに出す種類（None なら全部）
    pub(crate) fn block_categories<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let mut out: Option<Bound<'py, PyAny>> = None;
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("block_categories", (slf,))?;
            if !r.is_none() {
                let s = py.get_type_bound::<PySet>().call1((r,))?;
                out = Some(match out { None => s, Some(o) => o.call_method1("__and__", (s,))? });
            }
        }
        Ok(out.unwrap_or_else(|| py.None().into_bound(py)))
    }
    pub(crate) fn block_allowed(slf: &Bound<'_, Self>, kind: &Bound<'_, PyAny>) -> R<bool> {
        for a in Self::addons_v(slf)? {
            let r = a.call_method1("block_allowed", (slf, kind))?;
            if r.is_instance_of::<pyo3::types::PyBool>() && !r.is_truthy()? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn item_cost<'py>(slf: &Bound<'py, Self>, item: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        let (kind, id) = (item.get_item(0)?, item.get_item(1)?);
        let reg = Self::g(slf, "registry")?;
        let dict = py.get_type_bound::<PyDict>();
        if kind.eq("material")? {
            return dict.call1((getd(&reg.call_method1("material", (id,))?, "cost", PyDict::new_bound(py))?,));
        }
        let dd = reg.call_method1("device", (id,))?;
        if dd.is_truthy()? {
            dict.call1((getd(&dd, "cost", PyDict::new_bound(py))?,))
        } else {
            Ok(PyDict::new_bound(py).into_any())
        }
    }
    pub(crate) fn place(slf: &Bound<'_, Self>, hit: &Bound<'_, PyAny>, item: &Bound<'_, PyAny>, dir_index: &Bound<'_, PyAny>, look_dir: &Bound<'_, PyAny>)
        -> R<bool> {
        let py = slf.py();
        let (kind, id) = (item.get_item(0)?, item.get_item(1)?);
        if !Self::item_allowed(slf, item)? {
            Self::message(slf, "それは今はまだ使えません（物語が進むと使えるようになります）")?;
            return Ok(false);
        }
        let (mat, dev): (Bound<'_, PyAny>, Bound<'_, PyAny>) = if kind.eq("material")? {
            (id.clone(), py.None().into_bound(py))
        } else {
            let dd = Self::g(slf, "registry")?.call_method1("device", (&id,))?;
            let d = PyDict::new_bound(py);
            d.set_item("def", &id)?;
            d.set_item("dir", dir_index)?;
            (dd.get_item("material")?, d.into_any())
        };
        let look = v3(look_dir)?;
        if hit.is_truthy()? && hit.get_item("kind")?.eq("voxel")? {
            let c = hit.get_item("construct")?;
            let i: usize = hit.get_item("index")?.extract()?;
            let face: usize = hit.get_item("face")?.extract()?;
            let cell0 = crate::game::construct::cell_of(&c.getattr("cells")?.get_item(i)?)?;
            let du = crate::game::construct::DIRS[face];
            let cell = [cell0[0] + du[0], cell0[1] + du[1], cell0[2] + du[2]];
            let ct = crate::game::construct::tup3(py, cell);
            if c.getattr("index")?.contains(&ct)? {
                Self::message(slf, "そこには既にボクセルがあります")?;
                return Ok(false);
            }
            let f = c.call_method0("voxel_frames")?.get_item(i)?;
            Self::_hinge_side(slf, &dev, &f, look_dir)?;
            let fm = crate::game::construct::m3(&f)?;
            let (axis, sign) = (face / 2, if face % 2 == 0 { 1.0 } else { -1.0 });
            let size: f64 = c.getattr("size")?.extract()?;
            let pos = v3s(&c.getattr("pos")?)?;
            let m = crate::game::construct::mean8(&pos[8 * i..8 * i + 8]);
            let ctr = [m[0] + fm[axis][0] * sign * size, m[1] + fm[axis][1] * sign * size, m[2] + fm[axis][2] * sign * size];
            let mut verts = vec![];
            for cb in crate::game::construct::CORNER_BITS.iter() {
                let l = [(cb[0] - 0.5) * size, (cb[1] - 0.5) * size, (cb[2] - 0.5) * size];
                let w = crate::game::construct::mul_r(l, &fm);
                verts.push([ctr[0] + w[0], ctr[1] + w[1], ctr[2] + w[2]]);
            }
            if Self::_blocked(slf, &a1(py, ctr), size)? {
                return Ok(false);
            }
            if !Self::pay(slf, &Self::item_cost(slf, item)?)? {
                return Ok(false);
            }
            c.call_method1("add_voxel", (&ct, &mat, &dev, a3(py, &verts)?))?;
            let ni = c.getattr("index")?.call_method1("get", (&ct,))?;
            Self::_device_placed(slf, &c, if ni.is_none() { None } else { Some(ni.extract()?) })?;
            return Ok(true);
        }
        // 何もない所・地面 → 新しい構築物
        let size = fget(&Self::settings(slf)?, "voxel_size", 0.5)?;
        let (up, ctr) = if hit.is_truthy()? && (hit.get_item("kind")?.eq("terrain")? || hit.get_item("kind")?.eq("water")?
            || getd(hit, "placeable", py.None())?.is_truthy()?) {
            let up = v3(&hit.get_item("normal")?)?;
            let p = v3(&hit.get_item("point")?)?;
            (up, [p[0] + up[0] * size * 0.55, p[1] + up[1] * size * 0.55, p[2] + up[2] * size * 0.55])
        } else {
            let player = Self::g(slf, "player")?;
            let pp = player.getattr("pos")?;
            let pl = Self::nearest_planet(slf, &pp)?;
            let up = if pl.is_truthy()? { v3(&pl.call_method1("up", (&pp,))?)? } else { [0.0, 1.0, 0.0] };
            let e = v3(&player.call_method0("eye")?)?;
            (up, [e[0] + look[0] * 3.0, e[1] + look[1] * 3.0, e[2] + look[2] * 3.0])
        };
        if Self::_blocked(slf, &a1(py, ctr), size)? {
            return Ok(false);
        }
        let rm = Self::frame_of(up, look)?;
        let rt = [[rm[0][0], rm[1][0], rm[2][0]], [rm[0][1], rm[1][1], rm[2][1]], [rm[0][2], rm[1][2], rm[2][2]]];
        Self::_hinge_side(slf, &dev, &crate::game::construct::mat(py, &rt)?, look_dir)?;
        if !Self::pay(slf, &Self::item_cost(slf, item)?)? {
            return Ok(false);
        }
        let off = crate::game::construct::mul_t([0.5 * size, 0.5 * size, 0.5 * size], &rm);
        let origin = sub(ctr, off);
        let vox = PyList::new_bound(py, [PyTuple::new_bound(py, [PyTuple::new_bound(py, [0, 0, 0]).into_any(), mat.clone(), dev.clone()])]);
        let kw = PyDict::new_bound(py);
        kw.set_item("size", size)?;
        kw.set_item("name", "構築物")?;
        let c = imp(py, "main.construct")?.getattr("Construct")?.call_method("from_voxels",
            (Self::g(slf, "registry")?, vox, a1(py, origin), crate::game::construct::mat(py, &rm)?), Some(&kw))?;
        Self::add_construct(slf, &c)?;
        Self::_device_placed(slf, &c, Some(0))?;
        if hit.is_truthy()? && getd(hit, "placeable", py.None())?.is_truthy()? {
            for a in Self::addons_v(slf)? {
                a.call_method1("on_placed", (slf, hit, &c))?;
            }
        }
        Ok(true)
    }
    /// ヒンジを置くとき：曲がる側を、置くときに見ている向きで決める
    pub(crate) fn _hinge_side(slf: &Bound<'_, Self>, dev: &Bound<'_, PyAny>, frame_rows: &Bound<'_, PyAny>, look_dir: &Bound<'_, PyAny>) -> R<()> {
        let py = slf.py();
        if !dev.is_truthy()? {
            return Ok(());
        }
        if !crate::game::actuator::act_is_hinge_dev(dev, Some(&Self::g(slf, "registry")?))? {
            return Ok(());
        }
        let d: i64 = getd(dev, "dir", 2)?.call_method0("__int__")?.extract()?;
        let fr = crate::game::construct::np(py)?.call_method1("ascontiguousarray", (frame_rows, "float64"))?;
        let side = crate::game::actuator::act_side_toward(fr.extract()?, d, v3(look_dir)?)?;
        dev.call_method1("setdefault", ("state", PyDict::new_bound(py)))?.set_item("hinge", side)?;
        let names = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"];
        Self::message(slf, &format!("ヒンジ：+ の角度で {} 側へ倒れます（置くときに見ていた側。-90〜+90°。R で出力の向き）", names[side]))
    }
    pub(crate) fn _blocked(slf: &Bound<'_, Self>, ctr: &Bound<'_, PyAny>, size: f64) -> R<bool> {
        let c = v3(ctr)?;
        let me = Self::g(slf, "player")?;
        for p in Self::all_players(slf)? {
            let pos = v3(&p.getattr("pos")?)?;
            let up = v3(&p.getattr("up")?)?;
            let h: f64 = p.getattr("height")?.extract()?;
            let r: f64 = p.getattr("radius")?.extract()?;
            for off in [0.0, h - 2.0 * r] {
                if norm(sub([pos[0] + up[0] * off, pos[1] + up[1] * off, pos[2] + up[2] * off], c)) < size * 0.5 + r * 0.9 {
                    let msg = if p.is(&me) { "自分の位置には置けません".to_string() } else { format!("{} がいる所には置けません", p.getattr("name")?.str()?) };
                    Self::message(slf, &msg)?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    pub(crate) fn remove(slf: &Bound<'_, Self>, hit: &Bound<'_, PyAny>) -> R<bool> {
        let py = slf.py();
        if !hit.is_truthy()? || !hit.get_item("kind")?.eq("voxel")? {
            return Ok(false);
        }
        let c = hit.get_item("construct")?;
        let i: usize = hit.get_item("index")?.extract()?;
        let mat = c.getattr("mats")?.get_item(i)?;
        let dev = c.getattr("devs")?.get_item(i)?;
        let cell = c.getattr("cells")?.get_item(i)?;
        let cid = c.getattr("id")?;
        for p in Self::all_players(slf)? {
            let s = p.getattr("seated")?;
            if s.is_truthy()? && s.get_item(0)?.eq(&cid)? && s.get_item(1)?.eq(&cell)? {
                p.setattr("seated", py.None())?;
            }
        }
        c.call_method1("remove_voxels", (vec![i],))?;
        let item: (&str, Bound<'_, PyAny>) = if dev.is_truthy()? { ("device", dev.get_item("def")?) } else { ("material", mat) };
        let item: PyObject = item.into_py(py);
        Self::refund(slf, &Self::item_cost(slf, item.bind(py))?)?;
        if c.getattr("n_vox")?.extract::<usize>()? == 0 {
            Self::g(slf, "constructs")?.del_item(&cid)?;
        } else {
            for part in c.call_method0("split")?.iter()? {
                Self::add_construct(slf, &part?)?;
            }
        }
        Ok(true)
    }
    /// 左クリック：地面を掘る（資源は MOD が受け取る）。水面は MOD が使えば使う
    #[pyo3(signature = (hit, radius=0.8))]
    pub(crate) fn dig(slf: &Bound<'_, Self>, hit: &Bound<'_, PyAny>, radius: f64) -> R<bool> {
        if hit.is_truthy()? && hit.get_item("kind")?.eq("water")? {
            for a in Self::addons_v(slf)? {
                if a.call_method1("on_water", (slf, hit))?.is_truthy()? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if !hit.is_truthy()? || !hit.get_item("kind")?.eq("terrain")? {
            return Ok(false);
        }
        let pl = hit.get_item("planet")?;
        let res = pl.call_method1("resource_at", (hit.get_item("point")?,))?;      // 掘る前の場所で判定
        if pl.hasattr("update_water")? {
            pl.call_method1("update_water", (0.0,))?;    // 今までの穴の水を確かめておく（新しい穴は底から満ちる）
        }
        pl.call_method1("dig", (hit.get_item("point")?, radius))?;
        Self::_cave_in(slf, &pl, &hit.get_item("point")?, radius)?;
        for a in Self::addons_v(slf)? {
            a.call_method1("on_dig", (slf, hit, &res))?;
        }
        Ok(true)
    }
    /// 掘った穴の上が薄い（地中を掘って天井が薄い・深く掘って穴の縁が張り出した）とき、上を崩す：
    /// 穴の上に縦の掘削跡を足して地表まで開け、崩れた土を土のブロックのかたまり（構築物）として落とす
    pub(crate) fn _cave_in(slf: &Bound<'_, Self>, pl: &Bound<'_, PyAny>, point: &Bound<'_, PyAny>, radius: f64) -> R<()> {
        let py = slf.py();
        let ctr = v3(&pl.getattr("center")?)?;
        let p = v3(point)?;
        let c = sub(p, ctr);
        let rc = norm(c);
        if rc < 1e-6 || radius <= 0.0 {
            return Ok(());
        }
        let up = [c[0] / rc, c[1] / rc, c[2] / rc];
        let surf: f64 = pl.call_method1("surface_radius_exact", (a1(py, up).call_method1("reshape", (1, 3))?,))?.get_item(0)?.extract()?;
        let roof = surf - (rc + radius);          // 穴の上の土の厚さ（負なら地表に開いている）
        let depth = surf - rc;                    // 穴の中心の深さ
        let roof_min = (radius * 1.2).clamp(1.0, 3.0);
        let thin_roof = roof > 0.0 && roof < roof_min;
        let overhang = roof <= 0.0 && depth > radius * 0.5;
        if !(thin_roof || overhang) {
            return Ok(());
        }
        // 上がまだ土か（すでに掘ってあれば崩す物は無い）
        let rf = if up[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let t1 = { let v = cross(up, rf); let l = norm(v); [v[0] / l, v[1] / l, v[2] / l] };
        let t2 = cross(up, t1);
        let top = surf + 0.2;
        let rr = radius * 0.9;
        // 崩れる所の土（0.5 m おきの点で、今は土の所）
        let mut pts: Vec<V3> = vec![];
        let mut h = rc + radius * 0.5;
        while h < top {
            let mut x = -rr;
            while x <= rr {
                let mut y = -rr;
                while y <= rr {
                    if x * x + y * y <= rr * rr {
                        pts.push([ctr[0] + up[0] * h + t1[0] * x + t2[0] * y, ctr[1] + up[1] * h + t1[1] * x + t2[1] * y,
                                  ctr[2] + up[2] * h + t1[2] * x + t2[2] * y]);
                    }
                    y += 0.5;
                }
                x += 0.5;
            }
            h += 0.5;
        }
        if pts.is_empty() {
            return Ok(());
        }
        let flat: Vec<f64> = pts.iter().flatten().copied().collect();
        let n = pts.len();
        let sd: Vec<f64> = pl.call_method1("sdf", (crate::game::construct::arr(py, flat, &[n, 3])?,))?.extract()?;
        let solid: Vec<V3> = pts.iter().zip(sd.iter()).filter(|(_, d)| **d < -0.1).map(|(q, _)| *q).collect();
        if solid.len() < 2 {
            return Ok(());
        }
        // 縦に掘削跡を重ねて地表まで開ける
        let step = rr * 0.8;
        let mut k = 1.0;
        loop {
            let h = rc + step * k;
            let q = [ctr[0] + up[0] * h, ctr[1] + up[1] * h, ctr[2] + up[2] * h];
            pl.call_method1("dig", (a1(py, q), rr))?;
            if h - rr > top {
                break;
            }
            k += 1.0;
            if k > 40.0 {
                break;
            }
        }
        // 崩れた土：開けた縦穴の真ん中に、下から積んで落とす（壁にめり込まないように）。穴が太ければ 2×2×2 のかたまり
        // （1 m 角）、細ければ1ブロック。数は崩れた土の量まで（多すぎるときは 8 個まで）
        let big = rr >= 0.8;
        let per = if big { 8 } else { 1 };
        let edge = if big { 1.0 } else { 0.5 };
        let n_chunk = ((solid.len() + per - 1) / per).clamp(1, 8);
        let rot = crate::game::misc::content_rot_for(py, &a1(py, up))?;
        let soil = if Self::g(slf, "registry")?.call_method1("material", ("soil",)).map(|m| m.is_truthy().unwrap_or(false)).unwrap_or(false) {
            "soil"
        } else {
            "clay"
        };
        let h0 = rc - radius * 0.5;
        for q in 0..n_chunk {
            let h = h0 + q as f64 * (edge + 0.15);
            if h + edge > top + 2.0 {
                break;
            }
            // かたまりの真ん中が縦穴の軸に来るように（from_voxels の原点はブロック (0,0,0) の中心）
            let half = (edge - 0.5) * 0.5;
            let off = v3(&rot.call_method1("dot", (a1(py, [half, half, half]),))?)?;
            let o = [ctr[0] + up[0] * h - off[0], ctr[1] + up[1] * h - off[1], ctr[2] + up[2] * h - off[2]];
            let vox = PyList::empty_bound(py);
            let k = if big { 2 } else { 1 };
            for x in 0..k {
                for y in 0..k {
                    for z in 0..k {
                        vox.append((PyTuple::new_bound(py, [x, y, z]), soil, py.None()))?;
                    }
                }
            }
            let kw = PyDict::new_bound(py);
            kw.set_item("size", 0.5)?;
            kw.set_item("name", "崩れた土")?;
            let cn = imp(py, "main.construct")?.getattr("Construct")?.call_method("from_voxels",
                (Self::g(slf, "registry")?, vox, a1(py, o), &rot), Some(&kw))?;
            Self::add_construct(slf, &cn)?;
        }
        Self::message(slf, "掘った穴の上が崩れた")?;
        Ok(())
    }
}
