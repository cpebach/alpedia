//! 描画：構築物の見た目（ConstructView）

use super::*;

// ---------------------------------------------------------------- ConstructView
/// 1つの構築物の描画（テクスチャごとにメッシュを分ける）と、その機器の記号・看板。
///
/// 全部を1つの親ノード（root）の下に置く。剛体・静止中の構築物は、形・記号・看板を構築物の向きで一度だけ作り、
/// 毎フレームは親ノードの位置と向き（行列1つ）だけを動かす。ブロックごとに動いている構築物は毎フレーム作り直す。
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct ConstructView {}

pub(crate) fn remove_groups(s: &B<'_>) -> R<()> {
    for g in s.getattr("groups")?.iter()? {
        g?.get_item("mesh")?.call_method0("remove")?;
    }
    s.setattr("groups", PyList::empty_bound(s.py()))
}

#[pymethods]
impl ConstructView {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        ConstructView {}
    }
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, loader: &B<'_>, construct: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("c", construct)?;
        let root = gfx(py)?.getattr("RsNode")?.call1((parent,))?;
        s.setattr("root", &root)?;
        s.setattr("_parent0", parent)?;
        s.setattr("parent", &root)?;
        s.setattr("loader", loader)?;
        s.setattr("version", -1)?;
        s.setattr("groups", PyList::empty_bound(py))?;
        s.setattr("displays", PyDict::new_bound(py))?;
        s.setattr("_devs_ver", py.None())?;
        s.setattr("_mat_key", py.None())?;
        s.setattr("visible", true)?;
        Ok(())
    }
    // -- 共通 --
    pub(crate) fn set_visible(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let s = slf.as_any();
        if !on.eq(s.getattr("visible")?)? {
            s.setattr("visible", on)?;
            s.getattr("root")?.call_method0(if on.is_truthy()? { "show" } else { "hide" })?;
        }
        Ok(())
    }
    /// 機器のあるボクセルの番号（形が変わったときだけ数え直す）
    pub(crate) fn _dev_ids<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let c = s.getattr("c")?;
        let ver = c.getattr("version")?;
        if !s.getattr("_devs_ver")?.eq(&ver)? {
            s.setattr("_devs_ver", &ver)?;
            let l = PyList::empty_bound(py);
            for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
                if d?.is_truthy()? {
                    l.append(i)?;
                }
            }
            s.setattr("_devs", l)?;
        }
        s.getattr("_devs")
    }
    pub(crate) fn _firing<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let c = s.getattr("c")?;
        let n: usize = c.getattr("n_vox")?.extract()?;
        let mut f = vec![0.0f64; n];
        let devs = c.getattr("devs")?;
        for i in Self::_dev_ids(slf)?.iter()? {
            let i: usize = i?.extract()?;
            let d = devs.get_item(i)?;
            if d.is_truthy()? {
                f[i] = f64of(&get(&d.get_item("state")?, "firing", 0.0)?)?;
            }
        }
        Ok(f64_1(py, f))
    }
    /// M：4x4（行ベクトル形式）か None（単位行列）
    pub(crate) fn _set_root_mat(slf: &Bound<'_, Self>, m: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let root = s.getattr("root")?;
        if m.is_none() {
            if s.getattr("_mat_key")?.is_none() {
                return Ok(());
            }
            s.setattr("_mat_key", py.None())?;
            root.call_method1("setMat", (np(py)?.call_method1("eye", (4,))?.call_method0("ravel")?.call_method0("tolist")?,))?;
            return Ok(());
        }
        let key = m.call_method0("tobytes")?;
        if s.getattr("_mat_key")?.eq(&key)? {
            return Ok(());
        }
        s.setattr("_mat_key", key)?;
        root.call_method1("setMat", (m.call_method0("ravel")?.call_method0("tolist")?,))?;
        Ok(())
    }
    /// 形のメッシュが作れない（ブロックが無い）とき：何も描かない
    pub(crate) fn _rebuild(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        remove_groups(s)?;
        s.setattr("version", s.getattr("c")?.getattr("version")?)
    }
    pub(crate) fn _rebuild_blob(slf: &Bound<'_, Self>) -> R<bool> {
        let py = slf.py();
        let s = slf.as_any();
        let shape = imp(py, "main.shape")?;
        let rm = rmod(py)?;
        remove_groups(s)?;
        let c = s.getattr("c")?;
        let m = shape.call_method1("mesh_for", (&c,))?;
        s.setattr("blob", &m)?;
        s.setattr("version", c.getattr("version")?)?;
        if m.is_none() {
            return Ok(false);
        }
        let reg = c.getattr("reg")?;
        let n_vox: usize = c.getattr("n_vox")?.extract()?;
        let devs = c.getattr("devs")?;
        let mats = c.getattr("mats")?;
        let dev_tex = rm.getattr("DEVICE_TEXTURE")?;
        let mut col_v: Vec<[f64; 3]> = Vec::with_capacity(n_vox);
        let mut tex_v: Vec<(B<'_>, f64)> = Vec::with_capacity(n_vox);
        for i in 0..n_vox {
            let d = devs.get_item(i)?;
            if d.is_truthy()? {
                let dd = or_dict(reg.call_method1("device", (d.get_item("def")?,))?)?;
                col_v.push(v3(&get(&dd, "color", vec![1, 0, 1])?)?);
                let t = get(&dd, "texture", py.None())?;
                tex_v.push((if t.is_truthy()? { t } else { dev_tex.clone() }, 1.0));
            } else {
                let mt = reg.call_method1("material", (mats.get_item(i)?,))?;
                col_v.push(v3(&mt.get_item("color")?)?);
                // 半透明の素材（ガラス：opacity）は別のメッシュにして、透かして描く
                tex_v.push((get(&mt, "texture", py.None())?, f64of(&get(&mt, "opacity", 1.0)?)?));
            }
        }
        let npm = np(py)?;
        let (i64t, f64t) = (npm.getattr("int64")?, npm.getattr("float64")?);
        let owner_o = m.get_item("owner")?;
        let faces_o = m.get_item("faces")?;
        m.set_item("faces64", npm.call_method1("ascontiguousarray", (&faces_o, &i64t))?)?;
        m.set_item("vidx64", npm.call_method1("ascontiguousarray", (m.get_item("vidx")?, &i64t))?)?;
        m.set_item("w64", npm.call_method1("ascontiguousarray", (m.get_item("w")?, &f64t))?)?;
        let rest_o = m.get_item("rest")?;
        let rn = shape.call_method1("vertex_normals", (&rest_o, &faces_o))?;
        let rest = a2(&rest_o, 3)?;
        let (faces, nf) = i2(&faces_o, 3)?;
        let owner = i1(&owner_o)?;
        // テクスチャ座標：三角形ごとに、静止時の面の向きにいちばん近い軸の面へ投影する（ブロック1つでテクスチャ1枚）
        let mut fax = vec![0i64; nf];
        for f in 0..nf {
            let (a, b, cc) = (rest.r3(faces[3 * f] as usize), rest.r3(faces[3 * f + 1] as usize), rest.r3(faces[3 * f + 2] as usize));
            let n = cross([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]]);
            let ab = [n[0].abs(), n[1].abs(), n[2].abs()];
            let mut k = 0;
            for j in 1..3 {
                if ab[j] > ab[k] {
                    k = j;
                }
            }
            fax[f] = k as i64;
        }
        let style = rm.getattr("STYLE")?;
        let ao: Vec<f64> = if get(&style, "ao", true)?.is_truthy()? {
            a1(&rm.call_method1("block_ao", (&c, &rest_o, &rn, &owner_o))?)?
        } else {
            vec![1.0; rest.n]
        };
        // テクスチャごとに三角形を分ける（最初に出てきた順）
        let groups = PyDict::new_bound(py);
        let mut order: Vec<B<'_>> = vec![];
        let mut lists: Vec<Vec<usize>> = vec![];
        for f in 0..nf {
            let o = owner[faces[3 * f] as usize] as usize;
            let key = PyTuple::new_bound(py, [tex_v[o].0.clone(), tb(py, tex_v[o].1)]);
            match groups.get_item(&key)? {
                Some(ix) => {
                    let ix: usize = ix.extract()?;
                    lists[ix].push(f);
                }
                None => {
                    groups.set_item(&key, order.len())?;
                    order.push(key.into_any());
                    lists.push(vec![f]);
                }
            }
        }
        let edges = if get(&style, "edges", true)?.is_truthy()? { rm.call_method1("load_texture", (s.getattr("loader")?, rm.getattr("EDGE_TEXTURE")?))? } else { none(py) };
        let root = s.getattr("root")?;
        let cid = c.getattr("id")?;
        let out_groups = s.getattr("groups")?;
        for (key, fl) in order.iter().zip(lists.iter()) {
            let (tex, alpha) = (key.get_item(0)?, f64of(&key.get_item(1)?)?);
            let mut keys: Vec<i64> = Vec::with_capacity(fl.len() * 3);
            for &f in fl {
                for j in 0..3 {
                    keys.push(faces[3 * f + j] * 3 + fax[f]);
                }
            }
            let mut uk = keys.clone();
            uk.sort_unstable();
            uk.dedup();
            let inv: Vec<i64> = keys.iter().map(|k| uk.binary_search(k).unwrap() as i64).collect();
            let verts: Vec<i64> = uk.iter().map(|k| k.div_euclid(3)).collect();
            let axv: Vec<i64> = uk.iter().map(|k| k.rem_euclid(3)).collect();
            let mesh = dmesh(py, &root, &format!("c{}", cid.str()?))?;
            let mnp = mesh.getattr("np")?;
            if alpha < 0.999 {
                mnp.call_method1("setTransparency", (true,))?;
                mnp.call_method1("setDepthWrite", (false,))?;
                mnp.call_method1("setBin", ("transparent", 4))?;
            }
            let t = rm.call_method1("load_texture", (s.getattr("loader")?, &tex))?;
            if t.is_truthy()? {
                mnp.call_method1("setTexture", (&t, 1))?;
            }
            if !edges.is_none() {
                mnp.call_method1("setTexture", (rm.call_method0("_edge_stage")?, &edges, 1))?;
            }
            let mut cols = Vec::with_capacity(verts.len() * 3);
            let mut uv = Vec::with_capacity(verts.len() * 2);
            let mut own = Vec::with_capacity(verts.len());
            for (j, &vv) in verts.iter().enumerate() {
                let vv = vv as usize;
                let o = owner[vv];
                let cc = col_v[o as usize];
                for k in 0..3 {
                    cols.push(cc[k] * ao[vv]);
                }
                let r = rest.r3(vv);
                let a = axv[j];
                uv.push((if a == 0 { r[2] } else { r[0] }) as f32);
                uv.push((if a == 1 { r[2] } else { r[1] }) as f32);
                own.push(o);
            }
            let g = PyDict::new_bound(py);
            g.set_item("mesh", &mesh)?;
            g.set_item("verts", i64_1(py, verts))?;
            g.set_item("tris", i64_2(py, inv, 3)?)?;
            g.set_item("cols", f64_2(py, cols, 3)?)?;
            g.set_item("uv", f32_2(py, uv, 2)?)?;
            g.set_item("owner", i64_1(py, own))?;
            g.set_item("new", true)?;
            g.set_item("alpha", alpha)?;
            out_groups.call_method1("append", (g,))?;
        }
        Ok(true)
    }
    pub(crate) fn _set_mesh(slf: &Bound<'_, Self>, g: &B<'_>, v: &B<'_>) -> R<()> {
        let _ = slf;
        if g.get_item("new")?.is_truthy()? {
            g.get_item("mesh")?.call_method1("set", (v, g.get_item("tris")?))?;
            g.set_item("new", false)?;
        } else {
            g.get_item("mesh")?.call_method1("set", (v,))?;
        }
        Ok(())
    }
    /// 剛体・静止中の構築物：形は作り直さず（メッシュの使い回し）、位置と向きだけ動かす。
    /// 陰影（色）は向きが少し変わったときや噴射の状態が変わったときだけ付け直す
    pub(crate) fn _update_rigid(slf: &Bound<'_, Self>, origin: &B<'_>, tf: &B<'_>, highlight_flames: &B<'_>) -> R<()> {
        let _ = origin;
        let py = slf.py();
        let s = slf.as_any();
        let shape = imp(py, "main.shape")?;
        let c = s.getattr("c")?;
        let (rr, x, epoch) = (tf.get_item(0)?, tf.get_item(1)?, tf.get_item(2)?);
        let npm = np(py)?;
        let mut firing = none(py);
        let devs = c.getattr("devs")?;
        if highlight_flames.is_truthy()? {
            let thr = c.call_method1("dev_ids", ("thruster",))?; // 噴射の色は推進機だけ
            if thr.is_truthy()? {
                let round = py.import_bound("builtins")?.getattr("round")?;
                let mut v = vec![];
                for i in thr.iter()? {
                    let f = f64of(&get(&devs.get_item(i?)?.get_item("state")?, "firing", 0.0)?)?;
                    v.push(round.call1((f, 2))?);
                }
                firing = PyTuple::new_bound(py, v).into_any();
            }
        }
        let groups: Vec<B<'_>> = s.getattr("groups")?.iter()?.collect::<R<_>>()?;
        let any_new = |gs: &Vec<B<'_>>| -> R<bool> {
            for g in gs {
                if g.get_item("new")?.is_truthy()? {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        let mut b = gd(s, "_baked", none(py));
        if !b.is_none() && b.get_item("epoch")?.eq(&epoch)? && b.get_item("firing")?.eq(&firing)? && !b.get_item("R")?.is_none()
            && npm.call_method1("array_equal", (b.get_item("R")?, &rr))?.is_truthy()? && !any_new(&groups)? {
            return Ok(()); // 止まっている物：作り直す物は無い（位置は親の行列で）
        }
        if b.is_none() || !b.get_item("epoch")?.eq(&epoch)? {
            let blob = s.getattr("blob")?;
            let v = shape.call_method1("skin", (&c, &blob))?;
            let local = v.call_method1("__sub__", (&x,))?.call_method1("__matmul__", (&rr,))?; // 構築物の向きでの形
            let nl = shape.call_method1("vertex_normals", (&local, blob.get_item("faces")?))?;
            let nb = PyDict::new_bound(py);
            nb.set_item("epoch", &epoch)?;
            nb.set_item("local", local)?;
            nb.set_item("nl", nl)?;
            nb.set_item("R", py.None())?;
            nb.set_item("firing", py.None())?;
            s.setattr("_baked", &nb)?;
            b = nb.into_any();
            for g in &groups {
                g.set_item("new", true)?;
            }
        }
        let br = b.get_item("R")?;
        let need_color = br.is_none() || !b.get_item("firing")?.eq(&firing)? || {
            let tr = f64of(&br.getattr("T")?.call_method1("__matmul__", (&rr,))?.call_method0("trace")?)?;
            tr < 1.0 + 2.0 * (4.0f64).to_radians().cos()
        };
        if need_color || any_new(&groups)? {
            b.set_item("R", rr.call_method0("copy")?)?;
            b.set_item("firing", &firing)?;
            let n_vox: usize = c.getattr("n_vox")?.extract()?;
            let mut fl_all: Option<Vec<f64>> = None;
            if !firing.is_none() {
                let fv: Vec<f64> = firing.iter()?.map(|x| f64of(&x?)).collect::<R<_>>()?;
                if fv.iter().any(|x| *x != 0.0) {
                    let mut fa = vec![0.0; n_vox];
                    for (k, i) in c.call_method1("dev_ids", ("thruster",))?.iter()?.enumerate() {
                        let i: usize = i?.extract()?;
                        fa[i] = fv[k];
                    }
                    fl_all = Some(fa);
                }
            }
            let local = a2(&b.get_item("local")?, 3)?;
            let nl = a2(&b.get_item("nl")?, 3)?;
            let rm3 = f3x3(&rr)?;
            let s_ = sun(py)?;
            let amb = ambient(py)?;
            for g in &groups {
                let vs = i1(&g.get_item("verts")?)?;
                let cols = a2(&g.get_item("cols")?, 3)?;
                let uv: Vec<f32> = {
                    let u = g.get_item("uv")?;
                    let r: numpy::PyReadonlyArray2<f32> = npm.call_method1("ascontiguousarray", (u, npm.getattr("float32")?))?.extract()?;
                    r.as_slice()?.to_vec()
                };
                let own = i1(&g.get_item("owner")?)?;
                let alpha = f64of(&g.get_item("alpha")?)? as f32;
                let mut out = vec![0f32; vs.len() * 12];
                for (j, &vi) in vs.iter().enumerate() {
                    let vi = vi as usize;
                    let n = nl.r3(vi);
                    // nl[vs] @ R.T
                    let nw = [dot(n, rm3[0]), dot(n, rm3[1]), dot(n, rm3[2])];
                    let mut col = shade_rs(s_, amb, nw, cols.r3(j));
                    if let Some(fa) = &fl_all {
                        let fl = fa[own[j] as usize];
                        let fire = [1.0, 0.75, 0.25];
                        for k in 0..3 {
                            col[k] = col[k] * (1.0 - fl * 0.6) + fire[k] * fl;
                        }
                    }
                    let r = &mut out[j * 12..j * 12 + 12];
                    let p = local.r3(vi);
                    for k in 0..3 {
                        r[k] = p[k] as f32;
                        r[3 + k] = n[k] as f32;
                        r[6 + k] = col[k] as f32;
                    }
                    r[9] = alpha;
                    r[10] = uv[2 * j];
                    r[11] = uv[2 * j + 1];
                }
                Self::_set_mesh(slf, g, &f32_2(py, out, 12)?)?;
            }
        }
        Ok(())
    }
    pub(crate) fn _update_blob(slf: &Bound<'_, Self>, origin: &B<'_>, highlight_flames: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let c = s.getattr("c")?;
        let m = s.getattr("blob")?;
        let firing = if highlight_flames.is_truthy()? { Self::_firing(slf)? } else { none(py) };
        let npm = np(py)?;
        let pos = npm.call_method1("ascontiguousarray", (c.getattr("pos")?, npm.getattr("float64")?))?;
        let groups: Vec<B<'_>> = s.getattr("groups")?.iter()?.collect::<R<_>>()?;
        let gl = PyList::empty_bound(py);
        for g in &groups {
            gl.append((g.get_item("verts")?, g.get_item("cols")?, g.get_item("uv")?, g.get_item("owner")?))?;
        }
        let o: Vec<f64> = a1(origin)?;
        let outs = native(py)?.call_method1("mesh_pack", (pos, m.get_item("vidx64")?, m.get_item("w64")?, m.get_item("faces64")?, o, gl, sun(py)?.to_vec(),
            ambient(py)?, firing))?;
        for (g, v) in groups.iter().zip(outs.iter()?) {
            let mut v = v?;
            let alpha = f64of(&g.get_item("alpha")?)?;
            if alpha < 0.999 {
                v = npm.call_method("array", (v,), Some(&[("dtype", npm.getattr("float32")?), ("copy", tb(py, true))].into_py_dict_bound(py)))?;
                let sl = PyTuple::new_bound(py, [py.import_bound("builtins")?.getattr("slice")?.call1((py.None(),))?, tb(py, 9)]);
                v.set_item(sl, alpha)?;
            }
            Self::_set_mesh(slf, g, &v)?;
        }
        Ok(())
    }
    /// 形を描く。戻り値：剛体・静止中なら (R, x, 版)、ブロックごとなら None（記号・看板の位置合わせに使う）。
    /// static_root・anchor：止まっている物は、基準点 anchor からの位置で static_root の下に置く
    #[pyo3(signature = (origin, highlight_flames=None, static_root=None, anchor=None))]
    pub(crate) fn update<'py>(slf: &Bound<'py, Self>, origin: &B<'py>, highlight_flames: Option<&B<'py>>, static_root: Option<&B<'py>>, anchor: Option<&B<'py>>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let hf = highlight_flames.cloned().unwrap_or_else(|| tb(py, true));
        let sr = static_root.filter(|x| !x.is_none());
        let c = s.getattr("c")?;
        let want_parent = match sr { Some(x) => x.clone(), None => s.getattr("_parent0")? };
        let root = s.getattr("root")?;
        if !root.getattr("parent")?.is(&want_parent) {
            root.setattr("parent", &want_parent)?;
            s.setattr("_mat_key", "reparent")?; // 行列は必ず付け直す
        }
        let base = if sr.is_some() { anchor.cloned().unwrap_or_else(|| none(py)) } else { origin.clone() };
        s.setattr("_tf", py.None())?;
        if !s.getattr("version")?.eq(c.getattr("version")?)? {
            if !Self::_rebuild_blob(slf)? {
                s.setattr("blob", py.None())?;
                Self::_rebuild(slf)?;
            }
            s.setattr("_baked", py.None())?;
        }
        let tf = if c.hasattr("render_transform")? { c.call_method0("render_transform")? } else { none(py) };
        if !tf.is_none() {
            let (rr, x) = (tf.get_item(0)?, tf.get_item(1)?);
            // 行ベクトル形式：p' = p R^T + (x - origin)
            let r3 = f3x3(&rr)?;
            let xv = v3(&x)?;
            let bv = v3(&base)?;
            let mut mm = vec![0.0f64; 16];
            for i in 0..3 {
                for j in 0..3 {
                    mm[i * 4 + j] = r3[j][i];
                }
            }
            mm[15] = 1.0;
            for j in 0..3 {
                mm[12 + j] = xv[j] - bv[j];
            }
            Self::_set_root_mat(slf, &f64_2(py, mm, 4)?)?;
        } else {
            Self::_set_root_mat(slf, &none(py))?;
        }
        s.setattr("_tf", &tf)?;
        if !gd(s, "blob", none(py)).is_none() {
            if !tf.is_none() {
                s.call_method1("_update_rigid", (origin, &tf, &hf))?;
            } else {
                s.setattr("_baked", py.None())?;
                s.call_method1("_update_blob", (origin, &hf))?;
            }
        }
        Ok(tf)
    }
    /// 記号・看板用の頂点位置：剛体・静止中は構築物の向きでの位置（版ごとに1回だけ計算）、それ以外は None
    pub(crate) fn _local_pos<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let tf = s.getattr("_tf")?;
        if tf.is_none() {
            return Ok(none(py));
        }
        let (rr, x, epoch) = (tf.get_item(0)?, tf.get_item(1)?, tf.get_item(2)?);
        let lp = gd(s, "_lp", none(py));
        if lp.is_none() || !lp.get_item(0)?.eq(&epoch)? {
            let l = s.getattr("c")?.getattr("pos")?.call_method1("__sub__", (&x,))?.call_method1("__matmul__", (&rr,))?.call_method1("reshape", (-1, 8, 3))?;
            let t = PyTuple::new_bound(py, [epoch, l]);
            s.setattr("_lp", &t)?;
            return t.get_item(1);
        }
        lp.get_item(1)
    }
    pub(crate) fn update_decals(slf: &Bound<'_, Self>, origin: &B<'_>, cam_pos: &B<'_>, max_dist: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if !s.hasattr("decals")? {
            s.setattr("decals", rmod(py)?.getattr("DecalView")?.call1((s.getattr("root")?, s.getattr("c")?, max_dist))?)?;
        }
        let d = s.getattr("decals")?;
        d.setattr("max_dist", max_dist)?;
        d.call_method1("update", (origin, cam_pos, s))?;
        Ok(())
    }
    pub(crate) fn hide_decals(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        if s.hasattr("decals")? && s.getattr("decals")?.getattr("visible")?.is_truthy()? {
            let d = s.getattr("decals")?;
            d.getattr("mesh")?.getattr("np")?.call_method0("hide")?;
            d.setattr("visible", false)?;
        }
        Ok(())
    }
    /// コンピューターの表示（看板）
    pub(crate) fn update_displays(slf: &Bound<'_, Self>, origin: &B<'_>, far: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let c = s.getattr("c")?;
        let want = PyDict::new_bound(py);
        let lim2 = far.min(400.0).powi(2);
        let devs = c.getattr("devs")?;
        let pos = c.getattr("pos")?;
        let o = v3(origin)?;
        let rm = rmod(py)?;
        for i in Self::_dev_ids(slf)?.iter()? {
            let i: i64 = i?.extract()?;
            let d = devs.get_item(i)?;
            if !d.is_truthy()? {
                continue;
            }
            let disp = get(&d.get_item("state")?, "display", py.None())?;
            if !disp.is_truthy()? || !get(&disp, "text", py.None())?.is_truthy()? {
                continue;
            }
            let p0 = v3(&pos.get_item(8 * i)?)?;
            let q = [p0[0] - o[0], p0[1] - o[1], p0[2] - o[2]];
            if dot(q, q) > lim2 {
                continue;
            }
            for f in rm.call_method1("display_faces", (&c, i, &disp))?.iter()? {
                want.set_item((i, f?), &disp)?;
            }
        }
        let displays = s.getattr("displays")?;
        let keys: Vec<B<'_>> = displays.iter()?.collect::<R<_>>()?;
        for k in keys {
            if !want.contains(&k)? {
                displays.call_method1("pop", (&k,))?.call_method0("remove")?;
            }
        }
        if want.is_empty() {
            return Ok(());
        }
        let l = Self::_local_pos(slf)?;
        let tf = s.getattr("_tf")?;
        let key = if tf.is_none() { none(py) } else { tf.get_item(2)? };
        let size = f64of(&c.getattr("size")?)?;
        let root = s.getattr("root")?;
        for (k, disp) in want.iter() {
            let (i, f): (i64, i64) = k.extract()?;
            let mut v = displays.call_method1("get", (&k,))?;
            if v.is_none() {
                v = gfx(py)?.getattr("RsDisplayView")?.call1((&root,))?;
                displays.set_item(&k, &v)?;
            }
            v.call_method1("set", (&disp, size))?;
            if !l.is_none() {
                if v.getattr("placed")?.eq(&key)? {
                    continue;
                }
                let p8 = a2(&l.get_item(i)?, 3)?;
                let (ctr, right, up, fwd) = face_frame_rs(py, &p8.d, frames8_rs(&p8.d), f, size)?;
                v.call_method1("place", (np3(py, ctr), np3(py, right), np3(py, up), np3(py, fwd)))?;
                v.setattr("placed", &key)?;
            } else {
                let p8 = a2(&pos.get_item(pyo3::types::PySlice::new_bound(py, (8 * i) as isize, (8 * i + 8) as isize, 1))?, 3)?;
                let (ctr, right, up, fwd) = face_frame_rs(py, &p8.d, frames8_rs(&p8.d), f, size)?;
                v.call_method1("place", (np3(py, [ctr[0] - o[0], ctr[1] - o[1], ctr[2] - o[2]]), np3(py, right), np3(py, up), np3(py, fwd)))?;
                v.setattr("placed", py.None())?;
            }
        }
        Ok(())
    }
    pub(crate) fn remove(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        remove_groups(s)?;
        if s.hasattr("decals")? {
            s.getattr("decals")?.call_method0("remove")?;
        }
        for v in s.getattr("displays")?.call_method0("values")?.iter()? {
            v?.call_method0("remove")?;
        }
        s.setattr("displays", PyDict::new_bound(py))?;
        crate::game::client_misc::ui_destroy(py, &s.getattr("root")?, &PyTuple::empty_bound(py), None)
    }
}
