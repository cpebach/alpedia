//! 描画：地面の跡（DecalView）

use super::*;

// ---------------------------------------------------------------- DecalView
/// 機器ブロックの表面に種類の記号を貼る。向きのある機器は、横の面の ▲ が機器の向き、
/// 向いている面は黄色い輪、反対側の面は薄い記号になる。遠くの構築物では表示しない（負荷対策）
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct DecalView {}

#[pymethods]
impl DecalView {
    #[classattr]
    pub(crate) fn _texture(py: Python<'_>) -> PyObject {
        py.None()
    }
    #[classattr]
    pub(crate) fn _uv(py: Python<'_>) -> PyObject {
        py.None()
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        DecalView {}
    }
    #[pyo3(signature = (parent, construct, max_dist=None))]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, construct: &B<'_>, max_dist: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("c", construct)?;
        s.setattr("max_dist", max_dist.cloned().unwrap_or_else(|| tb(py, 30.0)))?;
        let mesh = dmesh(py, parent, &format!("decal{}", construct.getattr("id")?.str()?))?;
        let n = mesh.getattr("np")?;
        n.call_method1("setTransparency", (true,))?;
        n.call_method1("setDepthOffset", (2,))?;
        n.call_method1("setDepthWrite", (false,))?;
        n.call_method1("setBin", ("transparent", 5))?;
        let cls = rmod(py)?.getattr("DecalView")?;
        if cls.getattr("_texture")?.is_none() {
            let icons = imp(py, "main.client.icons")?;
            let r = icons.call_method0("decal_atlas")?;
            cls.setattr("_uv", r.get_item(1)?)?;
            cls.setattr("_texture", icons.call_method1("panda_texture", (r.get_item(0)?, "decals"))?)?;
        }
        n.call_method1("setTexture", (cls.getattr("_texture")?, 1))?;
        s.setattr("mesh", &mesh)?;
        s.setattr("version", -1)?;
        s.setattr("items", py.None())?;
        s.setattr("visible", false)?;
        s.setattr("_placed", py.None())?;
        Ok(())
    }
    pub(crate) fn _rebuild(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let dirs = imp(py, "main.construct")?.getattr("DIRS")?;
        let dirv: Vec<[i64; 3]> = (0..6).map(|k| -> R<[i64; 3]> { let v: Vec<i64> = dirs.get_item(k)?.call_method0("tolist")?.extract()?; Ok([v[0], v[1], v[2]]) }).collect::<R<_>>()?;
        let c = s.getattr("c")?;
        let reg = c.getattr("reg")?;
        let act = imp(py, "main.actuator")?;
        let kinds = s.call_method0("_kinds")?;
        let uvmap = s.getattr("_uv")?;
        let rm = rmod(py)?;
        let index = c.getattr("index")?;
        let cells = c.getattr("cells")?;
        let (mut vox, mut nrm, mut upv, mut uvs): (Vec<i64>, Vec<f64>, Vec<f64>, Vec<f32>) = (vec![], vec![], vec![], vec![]);
        let mut push = |i: usize, n: [i64; 3], up: [i64; 3], uv: &B<'_>| -> R<()> {
            vox.push(i as i64);
            nrm.extend(n.iter().map(|x| *x as f64));
            upv.extend(up.iter().map(|x| *x as f64));
            for x in uv.iter()? {
                uvs.push(f64of(&x?)? as f32);
            }
            Ok(())
        };
        for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
            let d = d?;
            if !d.is_truthy()? {
                continue;
            }
            let dd = or_dict(reg.call_method1("device", (d.get_item("def")?,))?)?;
            let mark = get(&dd, "mark", py.None())?;
            let mut kind = if mark.is_truthy()? { mark } else { get(&dd, "type", py.None())? };
            if !kinds.contains(&kind)? {
                kind = tb(py, "other");
            }
            let directional = get(&dd, "directional", py.None())?.is_truthy()?;
            let di: i64 = py.import_bound("builtins")?.getattr("int")?.call1((get(&d, "dir", 2)?,))?.extract()?;
            let dv = dirv[di.rem_euclid(6) as usize];
            let bend = if act.call_method1("is_hinge_dev", (&d, &reg))?.is_truthy()? { act.call_method1("hinge_side", (&d,))? } else { none(py) };
            let disp = get(&d.get_item("state")?, "display", py.None())?;
            let disp_faces: Vec<i64> = if disp.is_truthy()? && get(&disp, "text", py.None())?.is_truthy()? {
                rm.call_method1("display_faces", (&c, i, &disp))?.extract()?
            } else {
                vec![]
            };
            let cell: Vec<i64> = cells.get_item(i)?.extract()?;
            for f in 0..6usize {
                let n = dirv[f];
                if index.contains((cell[0] + n[0], cell[1] + n[1], cell[2] + n[2]))? || disp_faces.contains(&(f as i64)) {
                    continue;
                }
                let default_up = if n[1] != 0 { [0, 0, 1] } else { [0, 1, 0] };
                let (var, up) = if directional {
                    let dt = dv[0] * n[0] + dv[1] * n[1] + dv[2] * n[2];
                    if !bend.is_none() && bend.eq(f)? {
                        // ヒンジの曲がる側の面：曲がった矢印
                        push(i, n, dv, &uvmap.get_item(("bend", "plain"))?)?;
                        continue;
                    }
                    if dt == 1 {
                        ("front", default_up)
                    } else if dt == -1 {
                        ("back", default_up)
                    } else {
                        ("side", dv)
                    }
                } else {
                    ("plain", default_up)
                };
                // 推進機・アクチュエーターは、向きの面と反対の面に、その面の役割の印を貼る
                // （推進機：向きの面＝押す面、反対の面＝噴射口のノズル。アクチュエーター：向きの面＝出力の軸・棒・板、反対の面＝土台に付ける板）
                let ks: String = kind.extract().unwrap_or_default();
                let face: Option<(&str, &str)> = match (ks.as_str(), var) {
                    ("thruster", "front") => Some(("thrust_face", "front")),
                    ("thruster", "back") => Some(("nozzle", "plain")),
                    ("actuator", "front") => Some(("shaft", "front")),
                    ("piston", "front") => Some(("rod", "front")),
                    ("hinge", "front") => Some(("hinge_face", "front")),
                    ("actuator" | "piston" | "hinge", "back") => Some(("mount", "plain")),
                    _ => None,
                };
                if let Some((fk, fv)) = face {
                    if let Some(uv) = uvmap.get_item((fk, fv)).ok() {
                        push(i, n, up, &uv)?;
                        continue;
                    }
                }
                push(i, n, up, &uvmap.get_item((&kind, var))?)?;
            }
        }
        s.setattr("version", c.getattr("version")?)?;
        s.setattr("_placed", py.None())?;
        if vox.is_empty() {
            s.setattr("items", py.None())?;
            s.getattr("mesh")?.call_method1("set", (zeros12(py)?,))?;
            return Ok(());
        }
        let nu = uvs.len() / vox.len();
        let uva = {
            let a = numpy::PyArray1::from_vec_bound(py, uvs);
            if nu == 1 { a.into_any() } else { numpy::PyArrayMethods::reshape(&a, [vox.len(), nu])?.into_any() }
        };
        s.setattr("items", PyTuple::new_bound(py, [i64_1(py, vox), f64_2(py, nrm, 3)?, f64_2(py, upv, 3)?, uva]))?;
        s.setattr("_new", true)?;
        Ok(())
    }
    #[staticmethod]
    pub(crate) fn _kinds(py: Python<'_>) -> R<B<'_>> {
        imp(py, "main.client.icons")?.getattr("DECAL_TYPES")
    }
    pub(crate) fn _display_sig<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let c = slf.as_any().getattr("c")?;
        let devs = c.getattr("devs")?;
        let mut out = vec![];
        for i in c.call_method1("dev_ids", ("computer",))?.iter()? {
            let i = i?;
            let disp = or_dict(get(&devs.get_item(&i)?.get_item("state")?, "display", py.None())?)?;
            if get(&disp, "text", py.None())?.is_truthy()? {
                out.push(i);
            }
        }
        Ok(PyTuple::new_bound(py, out).into_any())
    }
    #[pyo3(signature = (origin, cam_pos, view=None))]
    pub(crate) fn update(slf: &Bound<'_, Self>, origin: &B<'_>, cam_pos: &B<'_>, view: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let c = s.getattr("c")?;
        let ver_diff = !s.getattr("version")?.eq(c.getattr("version")?)?;
        if !s.getattr("items")?.is_none() || ver_diff {
            let sig = Self::_display_sig(slf)?;
            if ver_diff || !sig.eq(gd(s, "_sig", none(py)))? {
                s.setattr("_sig", &sig)?;
                s.call_method0("_rebuild")?;
            }
        }
        if s.getattr("items")?.is_none() {
            return Ok(());
        }
        // 遠くの構築物では記号を出さない（外側の球で大まかに、近ければブロックごとに）
        let md = f64of(&s.getattr("max_dist")?)?;
        let bnd = c.call_method0("bounds")?;
        let ctr = v3(&bnd.get_item(0)?)?;
        let rad = f64of(&bnd.get_item(1)?)?;
        let cp = v3(cam_pos)?;
        let near = if npnorm(py, [ctr[0] - cp[0], ctr[1] - cp[1], ctr[2] - cp[2]])? - rad > md {
            false
        } else {
            let pos = a2(&c.getattr("pos")?, 3)?;
            let mut mn = f64::INFINITY;
            let mut i = 0;
            while i < pos.n {
                let p = pos.r3(i);
                let r = [p[0] - cp[0], p[1] - cp[1], p[2] - cp[2]];
                let d = r[0] * r[0] + r[1] * r[1] + r[2] * r[2];
                if d < mn {
                    mn = d;
                }
                i += 8;
            }
            mn < md * md
        };
        let mesh = s.getattr("mesh")?;
        if !near {
            if s.getattr("visible")?.is_truthy()? {
                mesh.getattr("np")?.call_method0("hide")?;
                s.setattr("visible", false)?;
            }
            return Ok(());
        }
        if !s.getattr("visible")?.is_truthy()? {
            mesh.getattr("np")?.call_method0("show")?;
            s.setattr("visible", true)?;
        }
        let l = match view.filter(|v| !v.is_none()) { Some(v) => v.call_method0("_local_pos")?, None => none(py) };
        if !l.is_none() {
            // 剛体・静止中：構築物の向きで一度だけ作る（位置と向きは親ノードの行列で動く）
            let key = view.unwrap().getattr("_tf")?.get_item(2)?;
            if s.getattr("_placed")?.eq(&key)? {
                return Ok(());
            }
            s.setattr("_placed", key)?;
            s.call_method1("_write", (l, np(py)?.call_method1("zeros", (3,))?))?;
        } else {
            s.setattr("_placed", py.None())?;
            s.call_method1("_write", (c.getattr("pos")?.call_method1("reshape", (-1, 8, 3))?, origin))?;
        }
        Ok(())
    }
    pub(crate) fn _write(slf: &Bound<'_, Self>, p8: &B<'_>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let c = s.getattr("c")?;
        let it = s.getattr("items")?;
        let (vox, nl, al, uv) = (it.get_item(0)?, it.get_item(1)?, it.get_item(2)?, it.get_item(3)?);
        let sel = p8.get_item(&vox)?;
        let npm = np(py)?;
        let (f64t, f32t) = (npm.getattr("float64")?, npm.getattr("float32")?);
        let nvox = vox.len()?;
        let v = native(py)?.call_method1("decal_quads", (npm.call_method1("ascontiguousarray", (sel, &f64t))?, npm.call_method1("ascontiguousarray", (nl, &f64t))?,
            npm.call_method1("ascontiguousarray", (al, &f64t))?, npm.call_method1("ascontiguousarray", (uv, &f32t))?, f64of(&c.getattr("size")?)?, a1(origin)?,
            sun(py)?.to_vec(), ambient(py)?))?;
        let mesh = s.getattr("mesh")?;
        if gd(s, "_new", tb(py, false)).is_truthy()? {
            mesh.call_method1("set", (v, qtris(py, nvox)?))?;
            s.setattr("_new", false)?;
        } else {
            mesh.call_method1("set", (v,))?;
        }
        Ok(())
    }
    pub(crate) fn remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().getattr("mesh")?.call_method0("remove")?;
        Ok(())
    }
}
