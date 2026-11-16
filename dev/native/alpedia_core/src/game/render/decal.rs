//! 描画：地面の跡（DecalView）

use crate::game::construct::{con, Construct};
use super::*;

// ---------------------------------------------------------------- DecalView
/// 機器ブロックの表面に種類の記号を貼る。向きのある機器は、横の面の ▲ が機器の向き、
/// 向いている面は黄色い輪、反対側の面は薄い記号になる。遠くの構築物では表示しない（負荷対策）
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct DecalView {
    /// _new（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_new: Option<PyObject>,
    /// _placed（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_placed: Option<PyObject>,
    /// _sig（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_sig: Option<PyObject>,
    /// c（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_c: Option<PyObject>,
    /// items（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_items: Option<PyObject>,
    /// max_dist（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_max_dist: Option<PyObject>,
    /// mesh（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_mesh: Option<PyObject>,
    /// version（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_version: Option<PyObject>,
    /// visible（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_visible: Option<PyObject>,
}

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
        DecalView { a_new: None, a_placed: None, a_sig: None, a_c: None, a_items: None, a_max_dist: None, a_mesh: None, a_version: None, a_visible: None }
    }
    #[pyo3(signature = (parent, construct, max_dist=None))]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, construct: &B<'_>, max_dist: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        Self::sa_c(slf, construct)?;
        Self::sa_max_dist(slf, max_dist.cloned().unwrap_or_else(|| tb(py, 30.0)))?;
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
        Self::sa_mesh(slf, &mesh)?;
        Self::sa_version(slf, -1)?;
        Self::sa_items(slf, py.None())?;
        Self::sa_visible(slf, false)?;
        Self::sa_placed(slf, py.None())?;
        Ok(())
    }
    pub(crate) fn _rebuild(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let dirs = imp(py, "main.construct")?.getattr("DIRS")?;
        let dirv: Vec<[i64; 3]> = (0..6).map(|k| -> R<[i64; 3]> { let v: Vec<i64> = dirs.get_item(k)?.call_method0("tolist")?.extract()?; Ok([v[0], v[1], v[2]]) }).collect::<R<_>>()?;
        let c = Self::ga_c(slf)?;
        let reg = c.getattr("reg")?;
        let act = imp(py, "main.actuator")?;
        let kinds = s.call_method0("_kinds")?;
        let uvmap = s.getattr("_uv")?;
        let rm = rmod(py)?;
        let index = Construct::g_index(con(&c)?)?;
        let cells = Construct::g_cells(con(&c)?)?;
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
        for (i, d) in Construct::g_devs(con(&c)?)?.iter()?.enumerate() {
            let d = d?;
            if !d.is_truthy()? {
                continue;
            }
            let dd = or_dict(crate::game::registry::dev_dict(&reg, &d.get_item("def")?)?)?;
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
        Self::sa_version(slf, c.getattr("version")?)?;
        Self::sa_placed(slf, py.None())?;
        if vox.is_empty() {
            Self::sa_items(slf, py.None())?;
            Self::ga_mesh(slf)?.call_method1("set", (zeros12(py)?,))?;
            return Ok(());
        }
        let nu = uvs.len() / vox.len();
        let uva = {
            let a = numpy::PyArray1::from_vec_bound(py, uvs);
            if nu == 1 { a.into_any() } else { numpy::PyArrayMethods::reshape(&a, [vox.len(), nu])?.into_any() }
        };
        Self::sa_items(slf, PyTuple::new_bound(py, [i64_1(py, vox), f64_2(py, nrm, 3)?, f64_2(py, upv, 3)?, uva]))?;
        Self::sa_new(slf, true)?;
        Ok(())
    }
    #[staticmethod]
    pub(crate) fn _kinds(py: Python<'_>) -> R<B<'_>> {
        imp(py, "main.client.icons")?.getattr("DECAL_TYPES")
    }
    pub(crate) fn _display_sig<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let c = Self::ga_c(slf)?;
        let devs = Construct::g_devs(con(&c)?)?;
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
        let c = Self::ga_c(slf)?;
        let ver_diff = !Self::ga_version(slf)?.eq(c.getattr("version")?)?;
        if !Self::ga_items(slf)?.is_none() || ver_diff {
            let sig = Self::_display_sig(slf)?;
            if ver_diff || !sig.eq(gd(s, "_sig", none(py)))? {
                Self::sa_sig(slf, &sig)?;
                s.call_method0("_rebuild")?;
            }
        }
        if Self::ga_items(slf)?.is_none() {
            return Ok(());
        }
        // 遠くの構築物では記号を出さない（外側の球で大まかに、近ければブロックごとに）
        let md = f64of(&Self::ga_max_dist(slf)?)?;
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
        let mesh = Self::ga_mesh(slf)?;
        if !near {
            if Self::ga_visible(slf)?.is_truthy()? {
                mesh.getattr("np")?.call_method0("hide")?;
                Self::sa_visible(slf, false)?;
            }
            return Ok(());
        }
        if !Self::ga_visible(slf)?.is_truthy()? {
            mesh.getattr("np")?.call_method0("show")?;
            Self::sa_visible(slf, true)?;
        }
        let l = match view.filter(|v| !v.is_none()) { Some(v) => v.call_method0("_local_pos")?, None => none(py) };
        if !l.is_none() {
            // 剛体・静止中：構築物の向きで一度だけ作る（位置と向きは親ノードの行列で動く）
            let key = view.unwrap().getattr("_tf")?.get_item(2)?;
            if Self::ga_placed(slf)?.eq(&key)? {
                return Ok(());
            }
            Self::sa_placed(slf, key)?;
            s.call_method1("_write", (l, np(py)?.call_method1("zeros", (3,))?))?;
        } else {
            Self::sa_placed(slf, py.None())?;
            s.call_method1("_write", (c.getattr("pos")?.call_method1("reshape", (-1, 8, 3))?, origin))?;
        }
        Ok(())
    }
    pub(crate) fn _write(slf: &Bound<'_, Self>, p8: &B<'_>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let c = Self::ga_c(slf)?;
        let it = Self::ga_items(slf)?;
        let (vox, nl, al, uv) = (it.get_item(0)?, it.get_item(1)?, it.get_item(2)?, it.get_item(3)?);
        let sel = p8.get_item(&vox)?;
        let npm = np(py)?;
        let (f64t, f32t) = (npm.getattr("float64")?, npm.getattr("float32")?);
        let nvox = vox.len()?;
        let v = native(py)?.call_method1("decal_quads", (npm.call_method1("ascontiguousarray", (sel, &f64t))?, npm.call_method1("ascontiguousarray", (nl, &f64t))?,
            npm.call_method1("ascontiguousarray", (al, &f64t))?, npm.call_method1("ascontiguousarray", (uv, &f32t))?, f64of(&c.getattr("size")?)?, a1(origin)?,
            sun(py)?.to_vec(), ambient(py)?))?;
        let mesh = Self::ga_mesh(slf)?;
        if gd(s, "_new", tb(py, false)).is_truthy()? {
            mesh.call_method1("set", (v, qtris(py, nvox)?))?;
            Self::sa_new(slf, false)?;
        } else {
            mesh.call_method1("set", (v,))?;
        }
        Ok(())
    }
    pub(crate) fn remove(slf: &Bound<'_, Self>) -> R<()> {
        Self::ga_mesh(slf)?.call_method0("remove")?;
        Ok(())
    }
}

// ---------------------------------------------------------------- DecalView の欄の読み書き（C-01。前は __dict__）
// 入れていない欄を読むと、前と同じく AttributeError（getattr(x, 名前, 既定値) も前と同じに動く）
macro_rules! decalview_attr_fields {
    ($($g:ident $s:ident $pg:ident $ps:ident $f:ident $name:literal),* $(,)?) => {
        #[allow(dead_code, non_snake_case)]
        impl DecalView { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
                match &slf.try_borrow()?.$f {
                    Some(v) => Ok(v.bind(slf.py()).clone()),
                    None => Err(pyo3::exceptions::PyAttributeError::new_err($name)),
                }
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> PyResult<()> {
                let v = v.into_py(slf.py());
                slf.try_borrow_mut()?.$f = Some(v);
                Ok(())
            }
        )* }
        #[pymethods]
        #[allow(non_snake_case)]
        impl DecalView { $(
            #[getter($name)]
            fn $pg(slf: &Bound<'_, Self>) -> PyResult<PyObject> {
                Ok(Self::$g(slf)?.unbind())
            }
            #[setter($name)]
            fn $ps(&mut self, v: PyObject) {
                self.$f = Some(v);
            }
        )* }
    };
}
decalview_attr_fields!(
    ga_new sa_new pga_new psa_new a_new "_new", ga_placed sa_placed pga_placed psa_placed a_placed "_placed", ga_sig sa_sig pga_sig psa_sig a_sig "_sig", ga_c sa_c pga_c psa_c a_c "c", ga_items sa_items pga_items psa_items a_items "items", ga_max_dist sa_max_dist pga_max_dist psa_max_dist a_max_dist "max_dist", ga_mesh sa_mesh pga_mesh psa_mesh a_mesh "mesh", ga_version sa_version pga_version psa_version a_version "version", ga_visible sa_visible pga_visible psa_visible a_visible "visible"
);
