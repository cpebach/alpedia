//! 描画の材料（main/client/render.py）：構築物・惑星・地表・看板などの頂点を作り、Rust の画面（gfx）のメッシュに渡す。
//!
//! 浮動原点：全ての描画位置は「カメラ（プレイヤーの目）からの相対座標」にしてから
//! float32 に変換するので、世界がどれだけ広くても精度が落ちない。
//!
//! 部品（ConstructView など）の値は Python の属性（ほかの部品・MOD が読む）。計算は Rust。

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyTuple};

use super::imp;
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

// ---------------------------------------------------------------- 小さな道具
fn tb<T: IntoPy<PyObject>>(py: Python<'_>, x: T) -> B<'_> {
    x.into_py(py).into_bound(py)
}
fn none(py: Python<'_>) -> B<'_> {
    py.None().into_bound(py)
}
fn gd<'py>(o: &B<'py>, k: &str, d: B<'py>) -> B<'py> {
    o.getattr(k).unwrap_or(d)
}
fn get<'py>(d: &B<'py>, k: &str, dflt: impl IntoPy<PyObject>) -> R<B<'py>> {
    let py = d.py();
    d.call_method1("get", (k, dflt.into_py(py)))
}
fn or_dict<'py>(o: B<'py>) -> R<B<'py>> {
    let py = o.py();
    Ok(if o.is_truthy()? { o } else { PyDict::new_bound(py).into_any() })
}
fn np(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "numpy")?.into_any())
}
fn rmod(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.render")?.into_any())
}
fn gfx(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.gfx")?.into_any())
}
fn native(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "alpedia_core")?.into_any())
}
fn is_exc(py: Python<'_>, e: &PyErr) -> bool {
    e.is_instance_of::<PyException>(py)
}
fn sun(py: Python<'_>) -> R<[f64; 3]> {
    v3(&rmod(py)?.getattr("SUN")?)
}
fn ambient(py: Python<'_>) -> R<f64> {
    f64of(&rmod(py)?.getattr("AMBIENT")?)
}
fn v3(o: &B<'_>) -> R<[f64; 3]> {
    Ok([f64of(&o.get_item(0)?)?, f64of(&o.get_item(1)?)?, f64of(&o.get_item(2)?)?])
}
fn np3(py: Python<'_>, v: [f64; 3]) -> B<'_> {
    numpy::PyArray1::from_vec_bound(py, v.to_vec()).into_any()
}
/// 配列（float64・行ごと）
pub struct A2 {
    pub d: Vec<f64>,
    pub n: usize,
    pub m: usize,
}
impl A2 {
    fn row(&self, i: usize) -> &[f64] {
        &self.d[i * self.m..i * self.m + self.m]
    }
    fn r3(&self, i: usize) -> [f64; 3] {
        let r = self.row(i);
        [r[0], r[1], r[2]]
    }
}
fn a2(o: &B<'_>, m: usize) -> R<A2> {
    let py = o.py();
    let npm = np(py)?;
    let a = npm.call_method1("ascontiguousarray", (o, npm.getattr("float64")?))?.call_method1("reshape", (-1i64, m as i64))?;
    let r: numpy::PyReadonlyArray2<f64> = a.extract()?;
    let n = r.as_array().nrows();
    Ok(A2 { d: r.as_slice()?.to_vec(), n, m })
}
fn a1(o: &B<'_>) -> R<Vec<f64>> {
    let py = o.py();
    let npm = np(py)?;
    let a = npm.call_method1("ascontiguousarray", (o, npm.getattr("float64")?))?.call_method1("reshape", (-1i64,))?;
    let r: numpy::PyReadonlyArray1<f64> = a.extract()?;
    Ok(r.as_slice()?.to_vec())
}
fn i2(o: &B<'_>, m: usize) -> R<(Vec<i64>, usize)> {
    let py = o.py();
    let npm = np(py)?;
    let a = npm.call_method1("ascontiguousarray", (o, npm.getattr("int64")?))?.call_method1("reshape", (-1i64, m as i64))?;
    let r: numpy::PyReadonlyArray2<i64> = a.extract()?;
    let n = r.as_array().nrows();
    Ok((r.as_slice()?.to_vec(), n))
}
fn i1(o: &B<'_>) -> R<Vec<i64>> {
    let py = o.py();
    let npm = np(py)?;
    let a = npm.call_method1("ascontiguousarray", (o, npm.getattr("int64")?))?.call_method1("reshape", (-1i64,))?;
    let r: numpy::PyReadonlyArray1<i64> = a.extract()?;
    Ok(r.as_slice()?.to_vec())
}
fn f32_2(py: Python<'_>, d: Vec<f32>, m: usize) -> R<B<'_>> {
    let n = d.len() / m.max(1);
    let a = numpy::PyArray1::from_vec_bound(py, d);
    Ok(numpy::PyArrayMethods::reshape(&a, [n, m])?.into_any())
}
fn f64_2(py: Python<'_>, d: Vec<f64>, m: usize) -> R<B<'_>> {
    let n = d.len() / m.max(1);
    let a = numpy::PyArray1::from_vec_bound(py, d);
    Ok(numpy::PyArrayMethods::reshape(&a, [n, m])?.into_any())
}
fn i64_2(py: Python<'_>, d: Vec<i64>, m: usize) -> R<B<'_>> {
    let n = d.len() / m.max(1);
    let a = numpy::PyArray1::from_vec_bound(py, d);
    Ok(numpy::PyArrayMethods::reshape(&a, [n, m])?.into_any())
}
fn i64_1(py: Python<'_>, d: Vec<i64>) -> B<'_> {
    numpy::PyArray1::from_vec_bound(py, d).into_any()
}
fn f64_1(py: Python<'_>, d: Vec<f64>) -> B<'_> {
    numpy::PyArray1::from_vec_bound(py, d).into_any()
}
fn zeros12(py: Python<'_>) -> R<B<'_>> {
    f32_2(py, vec![], 12)
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn rnorm(a: &[f64]) -> f64 {
    // numpy の norm(axis=1) と同じ（順に足す）
    let mut s = 0.0;
    for x in a {
        s += x * x;
    }
    s.sqrt()
}
/// 1次元の norm（numpy と同じ値にするため numpy で）
fn npnorm(py: Python<'_>, v: [f64; 3]) -> R<f64> {
    f64of(&np(py)?.getattr("linalg")?.call_method1("norm", (np3(py, v),))?)
}
/// (n,3) @ (3,)（numpy と同じ値にするため numpy で）
fn matvec(py: Python<'_>, q: &B<'_>, v: [f64; 3]) -> R<Vec<f64>> {
    a1(&q.call_method1("__matmul__", (np3(py, v),))?)
}
fn fmt_err(py: Python<'_>, e: &PyErr) -> String {
    e.value_bound(py).str().map(|s| s.to_string()).unwrap_or_default()
}

// ---------------------------------------------------------------- 関数
/// 画像ファイルのテクスチャ（loader は昔の名残り。使わない）
#[pyfunction]
pub fn render_load_texture<'py>(py: Python<'py>, loader: &B<'py>, path: &B<'py>) -> R<B<'py>> {
    let _ = loader;
    if !path.is_truthy()? {
        return Ok(none(py));
    }
    gfx(py)?.call_method1("tex", (path,))
}

/// 頂点（位置・法線・色・UV、(N,12) float32）と三角形を numpy 配列から設定できるメッシュ（gfx.RsDynamicMesh）
#[pyfunction]
#[pyo3(signature = (parent, name="mesh"))]
pub fn render_dynamic_mesh<'py>(py: Python<'py>, parent: &B<'py>, name: &str) -> R<B<'py>> {
    gfx(py)?.getattr("RsDynamicMesh")?.call1((parent, name))
}
fn dmesh<'py>(py: Python<'py>, parent: &B<'py>, name: &str) -> R<B<'py>> {
    rmod(py)?.call_method1("DynamicMesh", (parent, name))
}

#[pyfunction]
pub fn render_quad_tris(py: Python<'_>, nq: i64) -> R<B<'_>> {
    let mut v = Vec::with_capacity((nq.max(0) * 6) as usize);
    for q in 0..nq.max(0) {
        let b = q * 4;
        v.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    i64_2(py, v, 3)
}
fn qtris(py: Python<'_>, nq: usize) -> R<B<'_>> {
    render_quad_tris(py, nq as i64)
}

/// 陰影：colors × (AMBIENT + (1 − AMBIENT)·clip(法線·SUN, 0, 1))
#[pyfunction]
pub fn render_shade<'py>(py: Python<'py>, normals: &B<'py>, colors: &B<'py>) -> R<B<'py>> {
    let n = a2(normals, 3)?;
    let s = sun(py)?;
    let amb = ambient(py)?;
    let k: Vec<f64> = (0..n.n).map(|i| {
        let lam = dot(n.r3(i), s).clamp(0.0, 1.0);
        amb + (1.0 - amb) * lam
    }).collect();
    let kk = numpy::PyArrayMethods::reshape(&numpy::PyArray1::from_vec_bound(py, k), [n.n, 1])?;
    np(py)?.call_method1("multiply", (colors, kk))
}
fn shade_rs(s: [f64; 3], amb: f64, n: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let lam = dot(n, s).clamp(0.0, 1.0);
    let k = amb + (1.0 - amb) * lam;
    [c[0] * k, c[1] * k, c[2] * k]
}

/// セル単位の位置を、軸 axis（0/1/2）に垂直な面へ投影したテクスチャ座標（1セルで 0〜1）。横の面は v が上（+Y）
#[pyfunction]
pub fn render_project_uv<'py>(py: Python<'py>, p: &B<'py>, axis: &B<'py>) -> R<B<'py>> {
    let p = a2(p, 3)?;
    let ax = i1(axis)?;
    let mut out = Vec::with_capacity(p.n * 2);
    for i in 0..p.n {
        let r = p.row(i);
        let a = ax[if ax.len() == 1 { 0 } else { i }];
        out.push(if a == 0 { r[2] } else { r[0] });
        out.push(if a == 1 { r[2] } else { r[1] });
    }
    f64_2(py, out, 2)
}

/// 角の暗さ（環境光の遮り）：頂点の外側の半球に、ほかのブロックがどれだけあるか（形が変わったときだけ）。
/// owner があれば、回転ジョイントの向こう側（別に動く部分）のブロックは数えない
#[pyfunction]
#[pyo3(signature = (c, p, n, owner=None, strength=0.42))]
pub fn render_block_ao<'py>(py: Python<'py>, c: &B<'py>, p: &B<'py>, n: &B<'py>, owner: Option<&B<'py>>, strength: f64) -> R<B<'py>> {
    let npm = np(py)?;
    if p.len()? == 0 {
        return npm.call_method1("ones", (0,));
    }
    let owner = owner.filter(|o| !o.is_none());
    let mut gid = none(py);
    if owner.is_some() && gd(c, "rotors", none(py)).is_truthy()? && c.hasattr("_art_groups")? {
        gid = c.call_method0("_art_groups")?;
    }
    let use_ = !gid.is_none() && gid.len()? > 0 && gid.call_method0("max")?.gt(0)?;
    let i64t = npm.getattr("int64")?;
    let f64t = npm.getattr("float64")?;
    let cells = npm.call_method("asarray", (c.getattr("cells")?,), Some(&[("dtype", &i64t)].into_py_dict_bound(py)))?.call_method1("reshape", (-1, 3))?;
    let (g, o) = if use_ {
        (npm.call_method1("ascontiguousarray", (&gid, &i64t))?, npm.call_method1("ascontiguousarray", (owner.unwrap(), &i64t))?)
    } else {
        (none(py), none(py))
    };
    native(py)?.call_method1("block_ao", (cells, npm.call_method1("ascontiguousarray", (p, &f64t))?, npm.call_method1("ascontiguousarray", (n, &f64t))?,
        strength, g, o))
}
use pyo3::types::IntoPyDict;

/// (n,8,3) の頂点 → 各ボクセルのローカル軸 (n,3,3)。construct.voxel_frames と同じ
fn frames8_rs(p8: &[f64]) -> [[f64; 3]; 3] {
    let p = |k: usize, c: usize| p8[k * 3 + c];
    let mean4 = |a: [usize; 4], b: [usize; 4], c: usize| -> f64 {
        let mut s = 0.0;
        for j in 0..4 {
            s += p(a[j], c) - p(b[j], c);
        }
        s / 4.0
    };
    let mut f = [[0.0; 3]; 3];
    for c in 0..3 {
        f[0][c] = mean4([1, 3, 5, 7], [0, 2, 4, 6], c);
        f[1][c] = mean4([2, 3, 6, 7], [0, 1, 4, 5], c);
        f[2][c] = mean4([4, 5, 6, 7], [0, 1, 2, 3], c);
    }
    for r in f.iter_mut() {
        let n = rnorm(r).max(1e-9);
        for x in r.iter_mut() {
            *x /= n;
        }
    }
    f
}
#[pyfunction]
pub fn render_frames8<'py>(py: Python<'py>, p8: &B<'py>) -> R<B<'py>> {
    let a = a2(p8, 24)?;
    let mut out = Vec::with_capacity(a.n * 9);
    for i in 0..a.n {
        let f = frames8_rs(a.row(i));
        for r in f {
            out.extend_from_slice(&r);
        }
    }
    let arr = numpy::PyArray1::from_vec_bound(py, out);
    Ok(numpy::PyArrayMethods::reshape(&arr, [a.n, 3, 3])?.into_any())
}

/// ボクセルの8頂点 P8 とローカル軸 F から、面 d の (中心, 右, 上, 手前向き)
fn face_frame_rs(py: Python<'_>, p8: &[f64], f: [[f64; 3]; 3], d: i64, size: f64) -> R<([f64; 3], [f64; 3], [f64; 3], [f64; 3])> {
    let (axis, sign) = (d.div_euclid(2) as usize, if d.rem_euclid(2) == 0 { 1.0 } else { -1.0 });
    let n = [f[axis][0] * sign, f[axis][1] * sign, f[axis][2] * sign];
    let mut up = if axis == 1 { [f[2][0] * -sign, f[2][1] * -sign, f[2][2] * -sign] } else { f[1] };
    let dd = f64of(&np(py)?.call_method1("dot", (np3(py, up), np3(py, n)))?)?;
    up = [up[0] - n[0] * dd, up[1] - n[1] * dd, up[2] - n[2] * dd];
    let un = npnorm(py, up)?.max(1e-9);
    up = [up[0] / un, up[1] / un, up[2] / un];
    let fwd = [-n[0], -n[1], -n[2]];
    let right = cross(up, fwd);
    let mut ctr = [0.0; 3];
    for c in 0..3 {
        let mut s = 0.0;
        for k in 0..8 {
            s += p8[k * 3 + c];
        }
        ctr[c] = s / 8.0 + n[c] * (size * 0.5 + 0.004);
    }
    Ok((ctr, right, up, fwd))
}
fn ff_tuple(py: Python<'_>, r: ([f64; 3], [f64; 3], [f64; 3], [f64; 3])) -> B<'_> {
    PyTuple::new_bound(py, [np3(py, r.0), np3(py, r.1), np3(py, r.2), np3(py, r.3)]).into_any()
}
fn f3x3(o: &B<'_>) -> R<[[f64; 3]; 3]> {
    let a = a2(o, 3)?;
    Ok([a.r3(0), a.r3(1), a.r3(2)])
}
#[pyfunction]
pub fn render_face_frame_p<'py>(py: Python<'py>, p8: &B<'py>, f: &B<'py>, d: i64, size: f64) -> R<B<'py>> {
    let p = a2(p8, 3)?;
    Ok(ff_tuple(py, face_frame_rs(py, &p.d, f3x3(f)?, d, size)?))
}
#[pyfunction]
pub fn render_face_frame<'py>(py: Python<'py>, c: &B<'py>, i: i64, d: i64, frames: &B<'py>) -> R<B<'py>> {
    let f = f3x3(&frames.get_item(i)?)?;
    let p8 = a2(&c.getattr("pos")?.get_item(pyo3::types::PySlice::new_bound(py, (8 * i) as isize, (8 * i + 8) as isize, 1))?, 3)?;
    Ok(ff_tuple(py, face_frame_rs(py, &p8.d, f, d, f64of(&c.getattr("size")?)?)?))
}

/// 表示する面のリスト（他のボクセルで塞がれた面は除く）
#[pyfunction]
pub fn render_display_faces<'py>(py: Python<'py>, c: &B<'py>, i: &B<'py>, disp: &B<'py>) -> R<Vec<i64>> {
    let cm = imp(py, "main.construct")?;
    let dt = cm.getattr("DIR_TUPLES")?;
    let f = get(disp, "face", 2)?;
    let faces: Vec<i64> = if f.eq("all")? { (0..6).collect() } else { vec![py.import_bound("builtins")?.getattr("int")?.call1((f,))?.extract()?] };
    let cell = c.getattr("cells")?.get_item(i)?;
    let cl: Vec<i64> = cell.extract()?;
    let links = c.getattr("links")?;
    let lk = cm.getattr("link_key")?;
    let mut out = vec![];
    for d in faces {
        let t: Vec<i64> = dt.get_item(d)?.extract()?;
        let nb = (cl[0] + t[0], cl[1] + t[1], cl[2] + t[2]);
        if links.contains(lk.call1((&cell, nb))?)? {
            continue;
        }
        out.push(d);
    }
    Ok(out)
}

/// 構築物 c の遠景の箱（lod_boxes）を、ワールド座標 − base の頂点 (N,12) にする（6面×4頂点ずつ）
#[pyfunction]
pub fn render_lod_vertices<'py>(py: Python<'py>, c: &B<'py>, base: &B<'py>) -> R<B<'py>> {
    let lb = c.call_method0("lod_boxes")?;
    let (cc, h, col) = (lb.get_item(0)?, lb.get_item(1)?, lb.get_item(2)?);
    if cc.len()? == 0 {
        return zeros12(py);
    }
    let lp = c.call_method0("lattice_pose")?;
    let (r, t) = (lp.get_item(0)?, lp.get_item(1)?);
    let npm = np(py)?;
    let f64t = npm.getattr("float64")?;
    let f = |a: &B<'py>| npm.call_method1("ascontiguousarray", (a, &f64t));
    let tl: Vec<f64> = a1(&t)?;
    let bl: Vec<f64> = a1(base)?;
    native(py)?.call_method1("lod_vertices", (f(&cc)?, f(&h)?, f(&col)?, f(&r)?, tl, bl, f64of(&c.getattr("size")?)?, sun(py)?.to_vec(), ambient(py)?))
}

/// 構築物 c のライトの番号（形が変わったときだけ数え直す）
#[pyfunction]
pub fn render_light_ids<'py>(py: Python<'py>, c: &B<'py>) -> R<B<'py>> {
    let cache = gd(c, "_light_cache", none(py));
    let key = PyTuple::new_bound(py, [c.getattr("version")?, c.getattr("n_vox")?]);
    if cache.is_none() || !cache.get_item(0)?.eq(&key)? {
        let reg = c.getattr("reg")?;
        let ids = PyList::empty_bound(py);
        for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
            let d = d?;
            if d.is_truthy()? && get(&or_dict(reg.call_method1("device", (d.get_item("def")?,))?)?, "type", py.None())?.eq("light")? {
                ids.append(i)?;
            }
        }
        let nc = PyTuple::new_bound(py, [key.into_any(), ids.into_any()]);
        c.setattr("_light_cache", &nc)?;
        return nc.get_item(1);
    }
    cache.get_item(1)
}

/// 定義（本体と MOD）が使うテクスチャを全部読み込んでおく。読めないものはエラーのリストで返す。
/// アイコン用の画像もここで読む（プレイ中に MOD のファイルを読みに行かない）
#[pyfunction]
pub fn render_preload<'py>(py: Python<'py>, loader: &B<'py>, registry: &B<'py>) -> R<Vec<String>> {
    let icons = imp(py, "main.client.icons")?;
    let rm = rmod(py)?;
    let mut paths: Vec<B<'_>> = vec![rm.getattr("EDGE_TEXTURE")?, rm.getattr("DEVICE_TEXTURE")?];
    for cat in ["materials", "devices", "planets", "players"] {
        for d in registry.getattr("defs")?.get_item(cat)?.call_method0("values")?.iter()? {
            let d = d?;
            let t = get(&d, "texture", py.None())?;
            if t.is_truthy()? {
                paths.push(t);
            }
        }
    }
    let mut seen: Vec<B<'_>> = vec![];
    let mut errors: Vec<String> = vec![];
    for p in paths {
        if !p.is_truthy()? || seen.iter().any(|x| x.eq(&p).unwrap_or(false)) {
            continue;
        }
        seen.push(p.clone());
        if rm.call_method1("load_texture", (loader, &p))?.is_none() {
            errors.push(format!("テクスチャ '{}' を読み込めません", p.str()?));
        }
        if icons.call_method1("_texture_array", (&p,))?.is_none() {
            errors.push(format!("テクスチャ '{}' を画像として読めません", p.str()?));
        }
    }
    let mut out: Vec<String> = vec![];
    for e in errors {
        if !out.contains(&e) {
            out.push(e);
        }
    }
    Ok(out)
}

/// 輪郭の線を、本来のテクスチャに掛け合わせる2枚目のテクスチャの段
#[pyfunction]
pub fn render_edge_stage(py: Python<'_>) -> R<B<'_>> {
    gfx(py)?.getattr("EDGES")
}

const BOX_FACES: [[usize; 4]; 6] = [[1, 3, 7, 5], [0, 4, 6, 2], [2, 6, 7, 3], [0, 1, 5, 4], [4, 5, 7, 6], [0, 2, 3, 1]];

// 中身は分けて書いている（C-02）
mod layers;
mod construct_view;
mod decal;
mod planet_view;
mod carve;
pub(crate) use self::layers::*;
pub(crate) use self::construct_view::*;
pub(crate) use self::decal::*;
pub(crate) use self::planet_view::*;
pub(crate) use self::carve::*;
