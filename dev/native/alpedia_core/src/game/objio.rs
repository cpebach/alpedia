//! 構築物の OBJ 出力・読み込み（main/objio.py）。
//! 出力：<名前>.obj（形）・<名前>.mtl（色）・<名前>.alp.json（素材の対応・ボクセルと機器の完全なデータ）。
//! 読み込み：.alp.json に "voxels" があればそのまま、無ければ OBJ をボクセル化して素材を割り当てる。

use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{IntoPyDict, PyDict, PyList, PySet, PyString, PyTuple};

use super::construct::{fv, iv, v3s, R};
use super::imp;

fn log(py: Python<'_>) -> R<Bound<'_, PyAny>> {
    imp(py, "main.log")?.call_method1("get", ("obj",))
}
fn div<'py>(p: &Bound<'py, PyAny>, s: impl IntoPy<PyObject>) -> R<Bound<'py, PyAny>> {
    p.call_method1("__truediv__", (s,))
}
fn utf8(py: Python<'_>) -> Bound<'_, PyDict> {
    [("encoding", "utf-8")].into_py_dict_bound(py)
}
fn read_text(p: &Bound<'_, PyAny>) -> R<String> {
    p.call_method("read_text", (), Some(&utf8(p.py())))?.extract()
}
fn write_text(p: &Bound<'_, PyAny>, s: &str) -> R<()> {
    p.call_method("write_text", (s,), Some(&utf8(p.py())))?;
    Ok(())
}
fn exists(p: &Bound<'_, PyAny>) -> R<bool> {
    p.call_method0("exists")?.is_truthy()
}
fn path<'py>(py: Python<'py>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    imp(py, "pathlib")?.getattr("Path")?.call1((p,))
}
fn getd<'py>(d: &Bound<'py, PyAny>, k: &str, dflt: impl IntoPy<PyObject>) -> R<Bound<'py, PyAny>> {
    d.call_method1("get", (k, dflt))
}
fn safe_name(f: &str) -> bool {
    !f.is_empty() && !f.contains('/') && !f.contains('\\') && !f.contains("..")
}

/// 機器が使うプログラムのファイル名（コンピューターの .py とブロックの .blocks.json）
#[pyfunction]
pub fn objio_script_files(devs: &Bound<'_, PyAny>) -> R<Vec<String>> {
    let py = devs.py();
    let mut out = std::collections::BTreeSet::new();
    for d in devs.iter()? {
        let d = d?;
        if !d.is_truthy()? {
            continue;
        }
        let st = getd(&d, "state", PyDict::new_bound(py))?;
        for k in ["script", "blocks"] {
            let f = st.call_method1("get", (k,))?;
            if let Ok(s) = f.downcast::<PyString>() {
                let s = s.to_string();
                if safe_name(&s) {
                    out.insert(s);
                }
            }
        }
    }
    Ok(out.into_iter().collect())
}

fn py_fmt(x: f64, prec: usize) -> String {
    format!("{x:.prec$}")
}

#[pyfunction]
#[pyo3(signature = (c, out_dir, name, scripts_dir=None))]
pub fn objio_export_construct<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, out_dir: &Bound<'py, PyAny>, name: &str,
                                   scripts_dir: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyAny>> {
    out_dir.call_method("mkdir", (), Some(&[("parents", true), ("exist_ok", true)].into_py_dict_bound(py)))?;
    let fit = c.call_method0("rigid_fit")?;
    let rm = super::construct::m3(&fit.get_item(0)?)?;
    let t = super::construct::v3(&fit.get_item(1)?)?;
    let mesh = super::shape::shape_build_mesh(py, c, None)?;
    let cname = c.getattr("name")?.str()?.to_string();
    let mut lines = vec![format!("# Alpedia construct: {cname}"), format!("mtllib {name}.mtl"), format!("o {name}")];
    let mats = c.getattr("mats")?;
    let used = PyList::empty_bound(py);
    if !mesh.is_none() {
        // 見た目どおりの丸い形（球＋結合部を埋めた形）で出力
        let sk = imp(py, "alpedia_core")?.call_method1("skin", (c.getattr("pos")?, mesh.get_item("vidx")?, mesh.get_item("w")?))?;
        let mv: Vec<[f64; 3]> = v3s(&sk)?.into_iter().map(|p| super::construct::mul_r(super::construct::sub(p, t), &rm)).collect();
        let owner = iv(&mesh.get_item("owner")?)?;
        let faces: Vec<[i64; 3]> = iv(&mesh.get_item("faces")?)?.chunks_exact(3).map(|f| [f[0], f[1], f[2]]).collect();
        let face_mat: Vec<Bound<'py, PyAny>> = faces.iter().map(|f| mats.get_item(owner[f[0] as usize] as usize)).collect::<R<_>>()?;
        let s = PySet::new_bound(py, &face_mat)?;
        let sorted = imp(py, "builtins")?.call_method1("sorted", (s,))?;
        for m in sorted.iter()? {
            used.append(m?)?;
        }
        for v in &mv {
            lines.push(format!("v {} {} {}", py_fmt(v[0], 5), py_fmt(v[1], 5), py_fmt(v[2], 5)));
        }
        for m in used.iter() {
            lines.push(format!("usemtl {}", m.str()?));
            for (f, fm) in faces.iter().zip(&face_mat) {
                if fm.eq(&m)? {
                    lines.push(format!("f {} {} {}", f[0] + 1, f[1] + 1, f[2] + 1));
                }
            }
        }
    }
    let obj_path = div(out_dir, format!("{name}.obj"))?;
    write_text(&obj_path, &(lines.join("\n") + "\n"))?;
    let reg = c.getattr("reg")?;
    let mut mtl: Vec<String> = vec![];
    for m in used.iter() {
        let col = super::construct::v3(&reg.call_method1("material", (&m,))?.get_item("color")?)?;
        mtl.push(format!("newmtl {}", m.str()?));
        mtl.push(format!("Kd {} {} {}", py_fmt(col[0], 3), py_fmt(col[1], 3), py_fmt(col[2], 3)));
        mtl.push(String::new());
    }
    write_text(&div(out_dir, format!("{name}.mtl"))?, &mtl.join("\n"))?;
    let meta = PyDict::new_bound(py);
    meta.set_item("format", "alpedia-construct")?;
    meta.set_item("version", 1)?;
    meta.set_item("name", c.getattr("name")?)?;
    meta.set_item("obj", format!("{name}.obj"))?;
    meta.set_item("voxel_size", c.getattr("size")?)?;
    meta.set_item("scale", 1.0)?;
    let md = PyDict::new_bound(py);
    for m in used.iter() {
        md.set_item(&m, &m)?;
    }
    meta.set_item("materials", md)?;
    meta.set_item("default_material", if used.is_empty() { PyString::new_bound(py, "steel").into_any() } else { used.get_item(0)? })?;
    let vox = PyList::empty_bound(py);
    for ((cl, m), d) in c.getattr("cells")?.iter()?.zip(mats.iter()?).zip(c.getattr("devs")?.iter()?) {
        let cl = cl?;
        vox.append(PyList::new_bound(py, [cl.get_item(0)?.unbind(), cl.get_item(1)?.unbind(), cl.get_item(2)?.unbind(), m?.unbind(),
            super::construct::construct_clean_dev(py, &d?, Some(&reg))?]))?;
    }
    meta.set_item("voxels", vox)?;
    let ls = PyList::new_bound(py, c.getattr("links")?.iter()?.collect::<R<Vec<_>>>()?);
    ls.sort()?;
    let ll = PyList::empty_bound(py);
    for k in ls.iter() {
        ll.append([PyList::new_bound(py, k.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?),
                   PyList::new_bound(py, k.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?)])?;
    }
    meta.set_item("links", ll)?;
    meta.set_item("storage", c.getattr("storage")?)?;
    // 使っているプログラムも一緒に入れておく（別のワールドや他の人に渡しても動くように）
    if let Some(sd) = scripts_dir {
        if !sd.is_none() {
            let scripts = PyDict::new_bound(py);
            for f in objio_script_files(&c.getattr("devs")?)? {
                let fp = div(sd, &f)?;
                if exists(&fp)? {
                    scripts.set_item(&f, read_text(&fp)?)?;
                    if let Some(stem) = f.strip_suffix(".blocks.json") {      // ブロックから変換した .py も
                        let pyf = div(sd, format!("{stem}.py"))?;
                        if exists(&pyf)? {
                            scripts.set_item(pyf.getattr("name")?, read_text(&pyf)?)?;
                        }
                    }
                }
            }
            if !scripts.is_empty() {
                meta.set_item("scripts", scripts)?;
            }
        }
    }
    let kw = [("ensure_ascii", false.into_py(py)), ("indent", 1.into_py(py))].into_py_dict_bound(py);
    let text: String = imp(py, "json")?.call_method("dumps", (&meta,), Some(&kw))?.extract()?;
    write_text(&div(out_dir, format!("{name}.alp.json"))?, &text)?;
    log(py)?.call_method1("info", ("OBJ 出力: %s（ボクセル %d、面 %d）", &obj_path, c.getattr("n_vox")?, c.getattr("face_idx")?.len()?))?;
    Ok(obj_path)
}

/// OBJ ファイル → (頂点 (V,3), 三角形 (T,3), 三角形ごとの素材名のリスト)
#[pyfunction]
pub fn objio_parse_obj<'py>(py: Python<'py>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
    let p = path(py, p)?;
    let text = p.call_method("read_text", (), Some(&[("encoding", "utf-8"), ("errors", "replace")].into_py_dict_bound(py)))?;
    let r = imp(py, "alpedia_core")?.call_method1("obj_parse", (text,))?;
    let names = r.get_item(3)?;
    let tm = PyList::empty_bound(py);
    for k in iv(&r.get_item(2)?)? {
        tm.append(names.get_item(k)?)?;
    }
    Ok(PyTuple::new_bound(py, [r.get_item(0)?, r.get_item(1)?, tm.into_any()]))
}

/// 閉じたメッシュの内部を判定してボクセル化（x方向レイの交差回数）
#[pyfunction]
#[pyo3(signature = (verts, tris, tri_mat, size, mapping, default_material, max_cells=200000))]
#[allow(clippy::too_many_arguments)]
pub fn objio_voxelize<'py>(py: Python<'py>, verts: &Bound<'py, PyAny>, tris: &Bound<'py, PyAny>, tri_mat: &Bound<'py, PyAny>, size: f64,
                           mapping: &Bound<'py, PyAny>, default_material: &Bound<'py, PyAny>, max_cells: i64) -> R<Bound<'py, PyList>> {
    let out = PyList::empty_bound(py);
    if tris.len()? == 0 {
        return Ok(out);
    }
    let v = v3s(verts)?;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for q in &v {
        for a in 0..3 {
            lo[a] = lo[a].min(q[a]);
            hi[a] = hi[a].max(q[a]);
        }
    }
    let n: Vec<i64> = (0..3).map(|a| (((hi[a] - lo[a]) / size).ceil() as i64).max(1)).collect();
    if n[0] * n[1] * n[2] > max_cells {
        return Err(pyo3::exceptions::PyValueError::new_err(format!("大きすぎます（[{}, {}, {}] セル）。voxel_size か scale を調整してください",
            n[0], n[1], n[2])));
    }
    let np = super::construct::np(py)?;
    let t = np.call_method1("ascontiguousarray", (tris, "int64"))?.call_method1("reshape", (-1, 3))?;
    let r = imp(py, "alpedia_core")?.call_method1("voxelize", (super::construct::a3(py, &v)?, t, size, n.clone()))?;
    let mats: Vec<Bound<'py, PyAny>> = tri_mat.iter()?.map(|m| mapping.call_method1("get", (m?, default_material))).collect::<R<_>>()?;
    let cells = iv(&r.get_item(0)?)?;
    let ti = iv(&r.get_item(1)?)?;
    for (c, t) in cells.chunks_exact(3).zip(ti) {
        out.append((PyTuple::new_bound(py, [c[0], c[1], c[2]]), &mats[t as usize], py.None()))?;
    }
    Ok(out)
}

/// (.obj / .alp.json のどちらか) → (meta, obj_path, meta_path)
#[pyfunction]
pub fn objio_resolve<'py>(py: Python<'py>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
    let p = path(py, p)?;
    let suffix: String = p.getattr("suffix")?.extract()?;
    let (meta_path, mut obj_path) = if suffix.to_lowercase() == ".obj" {
        (div(&p.getattr("parent")?, format!("{}.alp.json", p.getattr("stem")?.str()?))?, p.clone())
    } else {
        (p.clone(), py.None().into_bound(py))
    };
    let meta = if exists(&meta_path)? {
        let m = imp(py, "json")?.call_method1("loads", (read_text(&meta_path)?,))?;
        if obj_path.is_none() {
            let o = getd(&m, "obj", py.None())?;
            if o.is_truthy()? {
                obj_path = div(&meta_path.getattr("parent")?, o)?;
            }
        }
        m
    } else { PyDict::new_bound(py).into_any() };
    Ok(PyTuple::new_bound(py, [meta, obj_path, meta_path]))
}

static OBJ_CACHE: GILOnceCell<Py<PyDict>> = GILOnceCell::new();

#[pyfunction]
pub fn objio_parse_cached<'py>(py: Python<'py>, obj_path: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let st = obj_path.call_method0("stat")?;
    let key = PyTuple::new_bound(py, [obj_path.str()?.into_any(), st.getattr("st_mtime")?, st.getattr("st_size")?]);
    let cache = OBJ_CACHE.get_or_init(py, || PyDict::new_bound(py).unbind()).bind(py);
    if let Some(h) = cache.get_item(&key)? {
        return Ok(h);
    }
    let h = objio_parse_obj(py, obj_path)?;
    cache.clear();
    cache.set_item(&key, &h)?;
    Ok(h.into_any())
}

/// 読み込みメニュー用の下調べ。kind: "voxels"（ゲーム内で作ったボクセルデータ）/ "obj"（外部の OBJ をボクセル化）
#[pyfunction]
pub fn objio_inspect<'py>(py: Python<'py>, registry: &Bound<'py, PyAny>, p: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let r = objio_resolve(py, p)?;
    let (meta, obj_path, meta_path) = (r.get_item(0)?, r.get_item(1)?, r.get_item(2)?);
    let src = if obj_path.is_truthy()? { obj_path.clone() } else { meta_path.clone() };
    let info = PyDict::new_bound(py);
    info.set_item("name", getd(&meta, "name", src.getattr("stem")?)?)?;
    info.set_item("meta", &meta)?;
    info.set_item("obj_path", &obj_path)?;
    info.set_item("meta_path", &meta_path)?;
    let req = PyList::empty_bound(py);
    for x in getd(&meta, "requires", PyList::empty_bound(py))?.iter()? {
        let x = x?;
        if x.is_instance_of::<PyString>() {
            req.append(x)?;
        }
    }
    info.set_item("requires", req)?;
    info.set_item("description", getd(&meta, "description", "")?)?;
    if getd(&meta, "voxels", py.None())?.is_truthy()? {
        info.set_item("kind", "voxels")?;
        info.set_item("voxel_size", getd(&meta, "voxel_size", 0.5)?.call_method0("__float__")?)?;
        return Ok(info);
    }
    if obj_path.is_none() || !exists(&obj_path)? {
        return Err(pyo3::exceptions::PyFileNotFoundError::new_err(format!("OBJ が見つかりません: {}", obj_path.str()?)));
    }
    let h = objio_parse_cached(py, &obj_path)?;
    let (verts, tris, tri_mat) = (h.get_item(0)?, h.get_item(1)?, h.get_item(2)?);
    if tris.len()? == 0 {
        return Err(pyo3::exceptions::PyValueError::new_err("面（f）がありません。閉じた立体の OBJ が必要です"));
    }
    let v = v3s(&verts)?;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for q in &v {
        for a in 0..3 {
            lo[a] = lo[a].min(q[a]);
            hi[a] = hi[a].max(q[a]);
        }
    }
    info.set_item("kind", "obj")?;
    info.set_item("extent", super::construct::a1(py, [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]))?;
    info.set_item("n_tris", tris.len()?)?;
    info.set_item("groups", imp(py, "builtins")?.call_method1("sorted", (PySet::new_bound(py, &tri_mat.iter()?.collect::<R<Vec<_>>>()?)?,))?)?;
    info.set_item("scale", getd(&meta, "scale", 1.0)?.call_method0("__float__")?)?;
    info.set_item("voxel_size", getd(&meta, "voxel_size", py.None())?)?;
    info.set_item("default_material", getd(&meta, "default_material", py.None())?)?;
    info.set_item("mapping", mapping_of(registry, &meta)?)?;
    Ok(info)
}

fn mapping_of<'py>(registry: &Bound<'py, PyAny>, meta: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let py = registry.py();
    let m = PyDict::new_bound(py);
    for (k, v) in getd(meta, "materials", PyDict::new_bound(py))?.downcast::<PyDict>()?.iter() {
        if registry.call_method1("get", ("materials", &v))?.is_truthy()? {
            m.set_item(k, v)?;
        }
    }
    Ok(m)
}

/// 戻り値: dict(name, size, voxels, links, storage)。外部 OBJ のときは voxel_size・scale・default_material を上書きできる
#[pyfunction]
#[pyo3(signature = (registry, obj_or_json, voxel_size=None, scale=None, default_material=None, max_cells=200000))]
pub fn objio_import_construct<'py>(py: Python<'py>, registry: &Bound<'py, PyAny>, obj_or_json: &Bound<'py, PyAny>, voxel_size: Option<f64>,
                                   scale: Option<f64>, default_material: Option<&Bound<'py, PyAny>>, max_cells: i64) -> R<Bound<'py, PyDict>> {
    let r = objio_resolve(py, obj_or_json)?;
    let (meta, obj_path, meta_path) = (r.get_item(0)?, r.get_item(1)?, r.get_item(2)?);
    let mut size: f64 = getd(&meta, "voxel_size", 1.0)?.extract()?;
    let src = if obj_path.is_truthy()? { obj_path.clone() } else { meta_path.clone() };
    let name = getd(&meta, "name", src.getattr("stem")?)?;
    let lg = log(py)?;
    let out = PyDict::new_bound(py);
    let mv = getd(&meta, "voxels", py.None())?;
    if mv.is_truthy()? {
        let vox = PyList::empty_bound(py);
        let devs = PyList::empty_bound(py);
        for v in mv.iter()? {
            let v = v?;
            let m3 = v.get_item(3)?;
            let mat = if registry.call_method1("get", ("materials", &m3))?.is_truthy()? { m3 } else { getd(&meta, "default_material", "steel")? };
            let mut dev = if v.len()? > 4 { v.get_item(4)? } else { py.None().into_bound(py) };
            if dev.is_truthy()? && !registry.call_method1("device", (dev.call_method1("get", ("def",))?,))?.is_truthy()? {
                lg.call_method1("warning", ("未知の機器 %s を素材ボクセルに置き換えます", dev.call_method1("get", ("def",))?))?;
                dev = py.None().into_bound(py);
            }
            devs.append(&dev)?;
            vox.append((PyTuple::new_bound(py, [v.get_item(0)?, v.get_item(1)?, v.get_item(2)?]), mat, dev))?;
        }
        let ml = getd(&meta, "links", py.None())?;
        let links: PyObject = if ml.is_truthy()? {
            let s = PySet::empty_bound(py)?;
            for l in ml.iter()? {
                let l = l?;
                s.add(super::construct::construct_link_key(py, &l.get_item(0)?, &l.get_item(1)?)?)?;
            }
            s.into_any().unbind()
        } else { py.None() };
        lg.call_method1("info", ("読み込み（ボクセルデータ）: %s %d ボクセル", &name, vox.len()))?;
        let scripts = py.get_type_bound::<PyDict>().call1((getd(&meta, "scripts", py.None()).and_then(|s| if s.is_truthy()? { Ok(s) } else { Ok(PyDict::new_bound(py).into_any()) })?,))?;
        // 古い出力などでプログラムが入っていなければ、出力元のワールドの scripts から探す
        let srcd = div(&meta_path.getattr("parent")?.getattr("parent")?, "scripts")?;
        for f in objio_script_files(&devs)? {
            if !scripts.contains(&f)? && exists(&div(&srcd, &f)?)? {
                scripts.set_item(&f, read_text(&div(&srcd, &f)?)?)?;
                if let Some(stem) = f.strip_suffix(".blocks.json") {
                    let pf = div(&srcd, format!("{stem}.py"))?;
                    if exists(&pf)? {
                        scripts.set_item(format!("{stem}.py"), read_text(&pf)?)?;
                    }
                }
            }
        }
        out.set_item("name", &name)?;
        out.set_item("size", size)?;
        out.set_item("voxels", vox)?;
        out.set_item("links", links)?;
        out.set_item("storage", getd(&meta, "storage", PyDict::new_bound(py))?)?;
        out.set_item("scripts", scripts)?;
        return Ok(out);
    }
    if obj_path.is_none() || !exists(&obj_path)? {
        return Err(pyo3::exceptions::PyFileNotFoundError::new_err(format!("OBJ が見つかりません: {}", obj_path.str()?)));
    }
    let h = objio_parse_cached(py, &obj_path)?;
    let (verts, tris, tri_mat) = (h.get_item(0)?, h.get_item(1)?, h.get_item(2)?);
    if let Some(v) = voxel_size {
        size = v;
    }
    let sc = match scale { Some(s) => s, None => getd(&meta, "scale", 1.0)?.extract()? };
    let vs: Vec<f64> = fv(&verts)?.into_iter().map(|x| x * sc).collect();
    let nv = vs.len() / 3;
    let verts = super::construct::arr(py, vs, &[nv, 3])?;
    let mapping = mapping_of(registry, &meta)?;
    let mut default = match default_material {
        Some(d) if d.is_truthy()? => d.clone(),
        _ => getd(&meta, "default_material", "steel")?,
    };
    if !registry.call_method1("get", ("materials", &default))?.is_truthy()? {
        default = PyString::new_bound(py, "steel").into_any();
    }
    let vox = objio_voxelize(py, &verts, &tris, &tri_mat, size, mapping.as_any(), &default, max_cells)?;
    lg.call_method1("info", ("読み込み（OBJボクセル化）: %s 三角形 %d → %d ボクセル", obj_path.getattr("name")?, tris.len()?, vox.len()))?;
    out.set_item("name", &name)?;
    out.set_item("size", size)?;
    out.set_item("voxels", vox)?;
    out.set_item("links", py.None())?;
    out.set_item("storage", PyDict::new_bound(py))?;
    Ok(out)
}

/// 設計図（最初から入っている構築物）の置き場：[(グループ名, フォルダ)]。本体の sauce/blueprints と、入っている MOD の blueprints/
#[pyfunction]
#[pyo3(signature = (mods=None))]
pub fn objio_blueprint_dirs<'py>(py: Python<'py>, mods: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyList>> {
    let paths = imp(py, "main.paths")?;
    let modmeta = imp(py, "main.modmeta")?;
    let out = PyList::empty_bound(py);
    out.append(("設計図", div(&paths.getattr("SAUCE")?, "blueprints")?))?;
    if let Some(ms) = mods {
        for m in ms.iter()? {
            let m = m?;
            let d = div(&div(&paths.getattr("MODS")?, &m)?, "blueprints")?;
            if d.call_method0("is_dir")?.is_truthy()? {
                out.append((format!("設計図（{}）", modmeta.call_method1("name_of", (&m,))?.str()?), d))?;
            }
        }
    }
    Ok(out)
}

fn scan<'py>(py: Python<'py>, d: &Bound<'py, PyAny>) -> R<Vec<Bound<'py, PyAny>>> {
    if !exists(d)? {
        return Ok(vec![]);
    }
    let mut v: Vec<Bound<'py, PyAny>> = d.call_method1("glob", ("*.obj",))?.iter()?.collect::<R<_>>()?;
    for p in d.call_method1("glob", ("*.alp.json",))?.iter()? {
        let p = p?;
        let n: String = p.getattr("name")?.extract()?;
        let stem: String = n.chars().take(n.chars().count().saturating_sub(9)).collect();
        if !exists(&div(d, format!("{stem}.obj"))?)? {
            v.push(p);
        }
    }
    let mut keyed: Vec<(String, Bound<'py, PyAny>)> = v.into_iter().map(|p| Ok((p.getattr("name")?.call_method0("lower")?.extract::<String>()?, p)))
        .collect::<R<_>>()?;
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    let _ = py;
    Ok(keyed.into_iter().map(|x| x.1).collect())
}

/// 読み込めるファイルの一覧: [(グループ名, Path)]
#[pyfunction]
#[pyo3(signature = (world_dir, saves_dir, mods=None))]
pub fn objio_list_sources<'py>(py: Python<'py>, world_dir: &Bound<'py, PyAny>, saves_dir: &Bound<'py, PyAny>, mods: Option<&Bound<'py, PyAny>>)
    -> R<Bound<'py, PyList>> {
    let out = PyList::empty_bound(py);
    for gd in objio_blueprint_dirs(py, mods)?.iter() {
        let (g, d) = (gd.get_item(0)?, gd.get_item(1)?);
        for p in scan(py, &d)? {
            out.append((&g, p))?;
        }
    }
    for p in scan(py, &div(world_dir, "imports")?)? {
        out.append(("imports", p))?;
    }
    for p in scan(py, &div(world_dir, "exports")?)? {
        out.append(("exports", p))?;
    }
    let mp = div(&div(saves_dir, ".net")?, "exports")?;          // マルチプレイで参加中に F9 で出力したもの（自分の PC）
    for p in scan(py, &mp)? {
        out.append(("マルチプレイで出力", p))?;
    }
    if exists(saves_dir)? {
        let wr = world_dir.call_method0("resolve")?;
        let mut ds: Vec<Bound<'py, PyAny>> = saves_dir.call_method0("iterdir")?.iter()?.collect::<R<_>>()?;
        let sorted = imp(py, "builtins")?.call_method1("sorted", (PyList::new_bound(py, &ds),))?;
        ds = sorted.iter()?.collect::<R<_>>()?;
        for wd in ds {
            let n: String = wd.getattr("name")?.extract()?;
            if wd.call_method0("is_dir")?.is_truthy()? && !n.starts_with('.') && !wd.call_method0("resolve")?.eq(&wr)? {
                for p in scan(py, &div(&wd, "exports")?)? {
                    out.append((format!("{n} の exports"), p))?;
                }
            }
        }
    }
    Ok(out)
}
