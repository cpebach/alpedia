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

// ---------------------------------------------------------------- LightLayer
/// ライト（電力で光るブロック）：点いている物の上に、影をつけない明るい色の箱を重ねる（少し大きく）。
/// 点滅（blink）もここで。読み込み済み（近く）の構築物だけ
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct LightLayer {}

#[pymethods]
impl LightLayer {
    #[classattr]
    #[allow(non_snake_case)]
    fn GROW() -> f64 {
        1.06
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        LightLayer {}
    }
    fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let m = dmesh(py, parent, "lights")?;
        let n = m.getattr("np")?;
        n.call_method1("setTransparency", (true,))?;
        n.call_method1("setDepthOffset", (1,))?;
        s.setattr("mesh", m)?;
        s.setattr("n", -1)?;
        Ok(())
    }
    fn update(slf: &Bound<'_, Self>, constructs: &B<'_>, origin: &B<'_>, now: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let grow = f64of(&s.getattr("GROW")?)?;
        let o = v3(origin)?;
        let mut ps: Vec<[f64; 24]> = vec![];
        let mut cols: Vec<[f64; 3]> = vec![];
        let rm = rmod(py)?;
        for c in constructs.iter()? {
            let c = c?;
            let ids = rm.call_method1("light_ids", (&c,))?;
            if !ids.is_truthy()? {
                continue;
            }
            let devs = c.getattr("devs")?;
            let reg = c.getattr("reg")?;
            let pos = c.getattr("pos")?;
            for i in ids.iter()? {
                let i: i64 = i?.extract()?;
                let d = devs.get_item(i)?;
                let st = d.get_item("state")?;
                if !get(&st, "active", py.None())?.is_truthy()? {
                    continue;
                }
                let bl = get(&st, "blink", py.None())?;
                if bl.is_truthy()? {
                    let b = f64of(&bl)?;
                    let m = f64of(&tb(py, now).call_method1("__mod__", (b,))?)?;
                    if m > b * 0.5 {
                        continue;
                    }
                }
                let dd = or_dict(reg.call_method1("device", (d.get_item("def")?,))?)?;
                let mut col = get(&st, "color", py.None())?;
                if !col.is_truthy()? {
                    col = get(&get(&dd, "params", PyDict::new_bound(py))?, "color", py.None())?;
                }
                let col = if col.is_truthy()? { v3(&col)? } else { [1.0, 0.86, 0.55] };
                let v8 = a2(&pos.get_item(pyo3::types::PySlice::new_bound(py, (8 * i) as isize, (8 * i + 8) as isize, 1))?, 3)?;
                let mut ctr = [0.0; 3];
                for k in 0..3 {
                    let mut sm = 0.0;
                    for j in 0..8 {
                        sm += v8.d[j * 3 + k];
                    }
                    ctr[k] = sm / 8.0;
                }
                let mut p = [0.0; 24];
                for j in 0..8 {
                    for k in 0..3 {
                        p[j * 3 + k] = (v8.d[j * 3 + k] - ctr[k]) * grow + ctr[k] - o[k];
                    }
                }
                ps.push(p);
                cols.push(col);
            }
        }
        let mesh = s.getattr("mesh")?;
        let n_old: i64 = s.getattr("n")?.extract()?;
        if ps.is_empty() {
            if n_old != 0 {
                mesh.call_method1("set", (zeros12(py)?,))?;
                s.setattr("n", 0)?;
            }
            return Ok(());
        }
        let mut out = vec![0f32; ps.len() * 24 * 12];
        for (b, p) in ps.iter().enumerate() {
            for (f, face) in BOX_FACES.iter().enumerate() {
                for (q, &vi) in face.iter().enumerate() {
                    let r = &mut out[((b * 6 + f) * 4 + q) * 12..((b * 6 + f) * 4 + q) * 12 + 12];
                    for k in 0..3 {
                        r[k] = p[vi * 3 + k] as f32;
                        r[6 + k] = cols[b][k] as f32;
                    }
                    r[9] = 0.92;
                }
            }
        }
        let t: PyObject = if ps.len() as i64 != n_old { qtris(py, ps.len() * 6)?.unbind() } else { py.None() };
        mesh.call_method1("set", (f32_2(py, out, 12)?, t))?;
        s.setattr("n", ps.len())?;
        Ok(())
    }
    fn remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().getattr("mesh")?.call_method0("remove")?;
        Ok(())
    }
}

// ---------------------------------------------------------------- LodLayer
/// 読み込み距離より遠い構築物を、それっぽい色の箱でまとめて描く（ブロックのメッシュを作らない・記号も看板も出さない）。
/// 止まっている物は1つのメッシュにまとめ（入れ替わったときだけ作り直す）、動いている物は毎フレーム作る
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct LodLayer {}

fn concat12<'py>(py: Python<'py>, vs: Vec<B<'py>>) -> R<B<'py>> {
    if vs.is_empty() {
        return zeros12(py);
    }
    np(py)?.call_method1("concatenate", (PyList::new_bound(py, vs),))
}

#[pymethods]
impl LodLayer {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        LodLayer {}
    }
    fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("static", dmesh(py, parent, "lod_static")?)?;
        s.setattr("moving", dmesh(py, parent, "lod_moving")?)?;
        s.setattr("key", py.None())?;
        s.setattr("base", np(py)?.call_method1("zeros", (3,))?)?;
        s.setattr("n_moving", 0)?;
        Ok(())
    }
    fn update(slf: &Bound<'_, Self>, constructs: &B<'_>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let mut statics = vec![];
        let mut moving = vec![];
        for c in constructs.iter()? {
            let c = c?;
            if c.getattr("is_static")?.is_truthy()? || c.getattr("asleep")?.is_truthy()? {
                statics.push(c);
            } else {
                moving.push(c);
            }
        }
        let pairs = PyList::empty_bound(py);
        for c in &statics {
            pairs.append((c.getattr("id")?, c.call_method0("_pose_key")?))?;
        }
        let key = py.import_bound("builtins")?.getattr("tuple")?.call1((py.import_bound("builtins")?.getattr("sorted")?.call1((pairs,))?,))?;
        let rm = rmod(py)?;
        if !key.eq(s.getattr("key")?)? {
            s.setattr("key", &key)?;
            let o = v3(origin)?;
            let base = np3(py, o);
            s.setattr("base", &base)?;
            let mut vs = vec![];
            for c in &statics {
                vs.push(rm.call_method1("lod_vertices", (c, &base))?);
            }
            let v = concat12(py, vs)?;
            let n = v.len()?;
            let t: PyObject = if n > 0 { qtris(py, n / 4)?.unbind() } else { py.None() };
            s.getattr("static")?.call_method1("set", (v, t))?;
        }
        let b = v3(&s.getattr("base")?)?;
        let o = v3(origin)?;
        s.getattr("static")?.getattr("np")?.call_method1("setPos", (b[0] - o[0], b[1] - o[1], b[2] - o[2]))?;
        let nm: i64 = s.getattr("n_moving")?.extract()?;
        if !moving.is_empty() || nm != 0 {
            let mut vs = vec![];
            for c in &moving {
                vs.push(rm.call_method1("lod_vertices", (c, origin))?);
            }
            let v = concat12(py, vs)?;
            let n = v.len()?;
            let t: PyObject = if n > 0 { qtris(py, n / 4)?.unbind() } else { py.None() };
            s.getattr("moving")?.call_method1("set", (v, t))?;
            s.setattr("n_moving", moving.len())?;
        }
        Ok(())
    }
    fn remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().getattr("static")?.call_method0("remove")?;
        slf.as_any().getattr("moving")?.call_method0("remove")?;
        Ok(())
    }
}

// ---------------------------------------------------------------- ConstructView
/// 1つの構築物の描画（テクスチャごとにメッシュを分ける）と、その機器の記号・看板。
///
/// 全部を1つの親ノード（root）の下に置く。剛体・静止中の構築物は、形・記号・看板を構築物の向きで一度だけ作り、
/// 毎フレームは親ノードの位置と向き（行列1つ）だけを動かす。ブロックごとに動いている構築物は毎フレーム作り直す。
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct ConstructView {}

fn remove_groups(s: &B<'_>) -> R<()> {
    for g in s.getattr("groups")?.iter()? {
        g?.get_item("mesh")?.call_method0("remove")?;
    }
    s.setattr("groups", PyList::empty_bound(s.py()))
}

#[pymethods]
impl ConstructView {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        ConstructView {}
    }
    fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, loader: &B<'_>, construct: &B<'_>) -> R<()> {
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
    fn set_visible(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let s = slf.as_any();
        if !on.eq(s.getattr("visible")?)? {
            s.setattr("visible", on)?;
            s.getattr("root")?.call_method0(if on.is_truthy()? { "show" } else { "hide" })?;
        }
        Ok(())
    }
    /// 機器のあるボクセルの番号（形が変わったときだけ数え直す）
    fn _dev_ids<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
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
    fn _firing<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
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
    fn _set_root_mat(slf: &Bound<'_, Self>, m: &B<'_>) -> R<()> {
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
    fn _rebuild(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        remove_groups(s)?;
        s.setattr("version", s.getattr("c")?.getattr("version")?)
    }
    fn _rebuild_blob(slf: &Bound<'_, Self>) -> R<bool> {
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
    fn _set_mesh(slf: &Bound<'_, Self>, g: &B<'_>, v: &B<'_>) -> R<()> {
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
    fn _update_rigid(slf: &Bound<'_, Self>, origin: &B<'_>, tf: &B<'_>, highlight_flames: &B<'_>) -> R<()> {
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
    fn _update_blob(slf: &Bound<'_, Self>, origin: &B<'_>, highlight_flames: &B<'_>) -> R<()> {
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
    fn update<'py>(slf: &Bound<'py, Self>, origin: &B<'py>, highlight_flames: Option<&B<'py>>, static_root: Option<&B<'py>>, anchor: Option<&B<'py>>) -> R<B<'py>> {
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
    fn _local_pos<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
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
    fn update_decals(slf: &Bound<'_, Self>, origin: &B<'_>, cam_pos: &B<'_>, max_dist: &B<'_>) -> R<()> {
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
    fn hide_decals(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        if s.hasattr("decals")? && s.getattr("decals")?.getattr("visible")?.is_truthy()? {
            let d = s.getattr("decals")?;
            d.getattr("mesh")?.getattr("np")?.call_method0("hide")?;
            d.setattr("visible", false)?;
        }
        Ok(())
    }
    /// コンピューターの表示（看板）
    fn update_displays(slf: &Bound<'_, Self>, origin: &B<'_>, far: f64) -> R<()> {
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
    fn remove(slf: &Bound<'_, Self>) -> R<()> {
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
        super::client_misc::ui_destroy(py, &s.getattr("root")?, &PyTuple::empty_bound(py), None)
    }
}

// ---------------------------------------------------------------- DecalView
/// 機器ブロックの表面に種類の記号を貼る。向きのある機器は、横の面の ▲ が機器の向き、
/// 向いている面は黄色い輪、反対側の面は薄い記号になる。遠くの構築物では表示しない（負荷対策）
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct DecalView {}

#[pymethods]
impl DecalView {
    #[classattr]
    fn _texture(py: Python<'_>) -> PyObject {
        py.None()
    }
    #[classattr]
    fn _uv(py: Python<'_>) -> PyObject {
        py.None()
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        DecalView {}
    }
    #[pyo3(signature = (parent, construct, max_dist=None))]
    fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, construct: &B<'_>, max_dist: Option<&B<'_>>) -> R<()> {
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
    fn _rebuild(slf: &Bound<'_, Self>) -> R<()> {
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
    fn _kinds(py: Python<'_>) -> R<B<'_>> {
        imp(py, "main.client.icons")?.getattr("DECAL_TYPES")
    }
    fn _display_sig<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
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
    fn update(slf: &Bound<'_, Self>, origin: &B<'_>, cam_pos: &B<'_>, view: Option<&B<'_>>) -> R<()> {
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
    fn _write(slf: &Bound<'_, Self>, p8: &B<'_>, origin: &B<'_>) -> R<()> {
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
    fn remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.as_any().getattr("mesh")?.call_method0("remove")?;
        Ok(())
    }
}

// ---------------------------------------------------------------- PlanetView
/// 遠景用の粗い球（キューブスフィア）
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct PlanetView {}

#[pymethods]
impl PlanetView {
    /// 詳細な地表の下に沈める深さ (m)
    #[classattr]
    #[allow(non_snake_case)]
    fn SINK() -> f64 {
        60.0
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        PlanetView {}
    }
    #[pyo3(signature = (parent, loader, planet, n=48))]
    fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, loader: &B<'_>, planet: &B<'_>, n: usize) -> R<()> {
        let _ = loader;
        let py = slf.py();
        let s = slf.as_any();
        let n = if planet.getattr("has_water")?.is_truthy()? { 96 } else { n }; // 山や海岸線が見えるように細かく
        s.setattr("p", planet)?;
        let root = parent.call_method1("attachNewNode", (format!("planet_{}", planet.getattr("id")?.str()?),))?;
        s.setattr("root", &root)?;
        let mesh = dmesh(py, &root, "far")?;
        s.setattr("mesh", &mesh)?;
        let step = 2.0 / n as f64;
        let g: Vec<f64> = (0..=n).map(|i| if i == n { 1.0 } else { i as f64 * step + -1.0 }).collect();
        let m1 = n + 1;
        let mut dirs: Vec<f64> = Vec::with_capacity(6 * m1 * m1 * 3);
        let mut tris: Vec<i64> = vec![];
        for face in 0..6usize {
            let (major, neg) = (face / 2, face % 2);
            let off = (face * m1 * m1) as i64;
            for a in 0..m1 {
                for b in 0..m1 {
                    let mut d = [0.0; 3];
                    d[major] = if neg == 1 { -1.0 } else { 1.0 };
                    d[(major + 1) % 3] = g[a];
                    d[(major + 2) % 3] = g[b];
                    let nn = rnorm(&d);
                    dirs.extend_from_slice(&[d[0] / nn, d[1] / nn, d[2] / nn]);
                }
            }
            for a in 0..n {
                for b in 0..n {
                    let i = (a * m1 + b) as i64;
                    let nn = n as i64;
                    tris.extend_from_slice(&[i + off, i + nn + 1 + off, i + nn + 2 + off, i + off, i + nn + 2 + off, i + 1 + off]);
                }
            }
        }
        let nv = dirs.len() / 3;
        let dirs_o = f64_2(py, dirs.clone(), 3)?;
        let fs = planet.call_method1("far_samples", (&dirs_o,))?;
        let r = a1(&fs.get_item(0)?)?;
        let col = a2(&fs.get_item(1)?, 3)?;
        let mut pos = vec![0.0; nv * 3];
        for i in 0..nv {
            let rr = r[i] - 0.6;
            for k in 0..3 {
                pos[i * 3 + k] = dirs[i * 3 + k] * rr;
            }
        }
        let pos_o = f64_2(py, pos.clone(), 3)?;
        let tris_o = i64_2(py, tris.clone(), 3)?;
        let nrm = a2(&imp(py, "main.shape")?.call_method1("vertex_normals", (&pos_o, &tris_o))?, 3)?;
        let s_ = sun(py)?;
        let amb = ambient(py)?;
        let mut base = vec![0f32; nv * 12];
        let mut normals = vec![0.0f64; nv * 3];
        for i in 0..nv {
            let dv = [dirs[i * 3], dirs[i * 3 + 1], dirs[i * 3 + 2]];
            let mut nv3 = nrm.r3(i);
            let e = nv3[0] * dv[0] + nv3[1] * dv[1] + nv3[2] * dv[2];
            let sg = (e + 1e-9).signum() * if e + 1e-9 == 0.0 { 0.0 } else { 1.0 };
            nv3 = [nv3[0] * sg, nv3[1] * sg, nv3[2] * sg];
            let e2 = nv3[0] * dv[0] + nv3[1] * dv[1] + nv3[2] * dv[2];
            if e2 <= 0.2 {
                nv3 = dv;
            }
            normals[i * 3..i * 3 + 3].copy_from_slice(&nv3);
            let sh = shade_rs(s_, amb, nv3, col.r3(i));
            let rr = &mut base[i * 12..i * 12 + 12];
            for k in 0..3 {
                rr[k] = pos[i * 3 + k] as f32;
                rr[3 + k] = nv3[k] as f32;
                rr[6 + k] = sh[k] as f32;
            }
            rr[9] = 1.0;
        }
        let base_o = f32_2(py, base, 12)?;
        s.setattr("base", &base_o)?;
        s.setattr("tris", &tris_o)?;
        s.setattr("dirs", &dirs_o)?;
        s.setattr("pos", &pos_o)?;
        s.setattr("cut", py.None())?;
        s.setattr("frame_key", py.None())?;
        let mut e = 0.0f64;
        for t in tris.chunks(3) {
            let (a, c) = (t[0] as usize, t[2] as usize);
            let d = [pos[a * 3] - pos[c * 3], pos[a * 3 + 1] - pos[c * 3 + 1], pos[a * 3 + 2] - pos[c * 3 + 2]];
            e = e.max(rnorm(&d));
        }
        // プレイヤーのまわりは詳細な地表があるので、遠景の球をくり抜く（洞窟や穴を覆わないように）
        s.setattr("CUT", (46.0 - e).max(0.0))?;
        mesh.call_method1("set", (&base_o, &tris_o))?;
        let _ = normals;
        Ok(())
    }
    /// 詳細な地表（TerrainPatch）の真下にある遠景の頂点を深く沈めた頂点データ。
    /// 沈めるのは、その頂点につながる三角形が全部詳細な地表の範囲に収まる頂点だけ（範囲の外に段差やすき間を作らない）
    fn _sunk<'py>(slf: &Bound<'py, Self>, frame: &B<'py>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let base = s.getattr("base")?;
        if frame.is_none() {
            return Ok(base);
        }
        let (up, t1, t2, r0, half) = (v3(&frame.get_item(0)?)?, v3(&frame.get_item(1)?)?, v3(&frame.get_item(2)?)?, f64of(&frame.get_item(3)?)?, f64of(&frame.get_item(4)?)?);
        let q_o = s.getattr("pos")?;
        let c = matvec(py, &q_o, up)?;
        let qa = matvec(py, &q_o, t1)?;
        let qb = matvec(py, &q_o, t2)?;
        let nv = c.len();
        let mut inside = vec![false; nv];
        let mut any = false;
        for i in 0..nv {
            let a = qa[i] * r0 / c[i];
            let b = qb[i] * r0 / c[i];
            inside[i] = c[i] > 0.0 && a.abs() < half && b.abs() < half;
            any |= inside[i];
        }
        if !any {
            return Ok(base);
        }
        let (tris, nt) = i2(&s.getattr("tris")?, 3)?;
        let mut bad = vec![false; nv];
        for t in 0..nt {
            let tr = &tris[3 * t..3 * t + 3];
            if !(inside[tr[0] as usize] && inside[tr[1] as usize] && inside[tr[2] as usize]) {
                for &v in tr {
                    bad[v as usize] = true;
                }
            }
        }
        let sink: Vec<bool> = (0..nv).map(|i| inside[i] && !bad[i]).collect();
        if !sink.iter().any(|x| *x) {
            return Ok(base);
        }
        let q = a2(&q_o, 3)?;
        let dirs = a2(&s.getattr("dirs")?, 3)?;
        let sk = f64of(&s.getattr("SINK")?)?;
        let npm = np(py)?;
        let v = base.call_method0("copy")?;
        let mut vv: Vec<f32> = {
            let r: numpy::PyReadonlyArray2<f32> = npm.call_method1("ascontiguousarray", (&v,))?.extract()?;
            r.as_slice()?.to_vec()
        };
        for i in 0..nv {
            if sink[i] {
                for k in 0..3 {
                    vv[i * 12 + k] = (q.d[i * 3 + k] - dirs.d[i * 3 + k] * sk) as f32;
                }
            }
        }
        f32_2(py, vv, 12)
    }
    /// patch_frame：この惑星の詳細な地表の範囲 (上, 接線1, 接線2, 半径, 半幅) か None
    #[pyo3(signature = (origin, player_pos=None, patch_frame=None))]
    fn update(slf: &Bound<'_, Self>, origin: &B<'_>, player_pos: Option<&B<'_>>, patch_frame: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let p = s.getattr("p")?;
        let ctr = v3(&p.getattr("center")?)?;
        let o = v3(origin)?;
        s.getattr("root")?.call_method1("setPos", (ctr[0] - o[0], ctr[1] - o[1], ctr[2] - o[2]))?;
        let pf = patch_frame.filter(|x| !x.is_none());
        let fkey = match pf {
            None => none(py),
            Some(f) => {
                let mut v: Vec<f64> = vec![];
                v.extend(v3(&f.get_item(0)?)?);
                v.extend(v3(&f.get_item(1)?)?);
                v.push(f64of(&f.get_item(3)?)?);
                v.push(f64of(&f.get_item(4)?)?);
                // np.round(x, 6)：x×10⁶ を偶数への丸め → ÷10⁶
                let r: Vec<PyObject> = v.iter().map(|x| ((x * 1e6).round_ties_even() / 1e6).into_py(py)).collect();
                PyTuple::new_bound(py, r).into_any()
            }
        };
        if !fkey.eq(s.getattr("frame_key")?)? {
            s.setattr("frame_key", &fkey)?;
            s.setattr("verts", Self::_sunk(slf, &pf.cloned().unwrap_or_else(|| none(py)))?)?;
            s.setattr("cut", "redo")?;
        }
        let radius = f64of(&p.getattr("radius")?)?;
        let mut cut: Option<[f64; 3]> = None;
        if let Some(pp) = player_pos.filter(|x| !x.is_none()) {
            let pv = v3(pp)?;
            let rel = [pv[0] - ctr[0], pv[1] - ctr[1], pv[2] - ctr[2]];
            let rn = npnorm(py, rel)?;
            if rn - radius < 250.0 {
                let d = rn.max(1e-9);
                cut = Some([rel[0] / d, rel[1] / d, rel[2] / d]);
            }
        }
        let cur = s.getattr("cut")?;
        if cut.is_none() && cur.is_none() {
            return Ok(());
        }
        let redo = cur.is_instance_of::<pyo3::types::PyString>();
        if !redo {
            if let Some(cv) = cut {
                if !cur.is_none() {
                    let cc = v3(&cur)?;
                    if npnorm(py, [cv[0] - cc[0], cv[1] - cc[1], cv[2] - cc[2]])? * radius < 4.0 {
                        return Ok(());
                    }
                }
            }
        }
        s.setattr("cut", match cut { Some(c) => np3(py, c), None => none(py) })?;
        let tris_o = s.getattr("tris")?;
        let tris_out = if let Some(cv) = cut {
            let dirs = a2(&s.getattr("dirs")?, 3)?;
            let cutr = f64of(&s.getattr("CUT")?)?;
            let near: Vec<bool> = (0..dirs.n).map(|i| {
                let d = dirs.r3(i);
                rnorm(&[d[0] - cv[0], d[1] - cv[1], d[2] - cv[2]]) * radius < cutr
            }).collect();
            let (tris, nt) = i2(&tris_o, 3)?;
            let mut keep = Vec::with_capacity(tris.len());
            for t in 0..nt {
                let tr = &tris[3 * t..3 * t + 3];
                if !(near[tr[0] as usize] || near[tr[1] as usize] || near[tr[2] as usize]) {
                    keep.extend_from_slice(tr);
                }
            }
            i64_2(py, keep, 3)?
        } else {
            tris_o
        };
        let verts = gd(s, "verts", s.getattr("base")?);
        s.getattr("mesh")?.call_method1("set", (verts, tris_out))?;
        Ok(())
    }
}

// ---------------------------------------------------------------- TerrainPatch
/// プレイヤー周辺の詳細な地表（掘削跡も反映）と水面
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct TerrainPatch {}

#[pymethods]
impl TerrainPatch {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        TerrainPatch {}
    }
    #[pyo3(signature = (parent, loader, cells=48, step=1.0))]
    fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, loader: &B<'_>, cells: i64, step: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let root = parent.call_method1("attachNewNode", ("terrain_patch",))?;
        s.setattr("root", &root)?;
        s.setattr("mesh", dmesh(py, &root, "patch")?)?;
        let w = dmesh(py, &root, "water")?;
        let n = w.getattr("np")?;
        n.call_method1("setTransparency", (true,))?;
        n.call_method1("setDepthWrite", (false,))?;
        n.call_method1("setBin", ("transparent", 10))?;
        s.setattr("water", w)?;
        s.setattr("loader", loader)?;
        s.setattr("cells", cells)?;
        s.setattr("step", step)?;
        s.setattr("anchor", py.None())?;
        s.setattr("planet", py.None())?;
        s.setattr("version", -1)?;
        s.setattr("carve_version", -1)?;
        s.setattr("center", np(py)?.call_method1("zeros", (3,))?)?;
        Ok(())
    }
    /// この惑星の上に詳細な地表を出しているなら (上, 接線1, 接線2, 半径, 半幅)（遠景の球を沈める範囲）
    fn frame_for<'py>(slf: &Bound<'py, Self>, planet: &B<'py>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        if !s.getattr("planet")?.is(planet) || !gd(s, "shown", tb(py, false)).is_truthy()? || s.getattr("anchor")?.is_none() {
            return Ok(none(py));
        }
        s.getattr("frame")
    }
    #[pyo3(signature = (planet, player_pos, origin, carve=None))]
    fn update(slf: &Bound<'_, Self>, planet: &B<'_>, player_pos: &B<'_>, origin: &B<'_>, carve: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let root = s.getattr("root")?;
        let pp = v3(player_pos)?;
        if planet.is_none() || {
            let c = v3(&planet.getattr("center")?)?;
            npnorm(py, [pp[0] - c[0], pp[1] - c[1], pp[2] - c[2]])? - f64of(&planet.getattr("radius")?)? > 400.0
        } {
            root.call_method0("hide")?;
            s.setattr("shown", false)?;
            return Ok(());
        }
        s.setattr("shown", true)?;
        root.call_method0("show")?;
        let carve = carve.filter(|c| !c.is_none());
        let cv = match carve { Some(c) => c.getattr("version")?, None => tb(py, -1) };
        let anchor = s.getattr("anchor")?;
        let need = !s.getattr("planet")?.is(planet) || anchor.is_none() || !s.getattr("version")?.eq(planet.getattr("version")?)?
            || !cv.eq(s.getattr("carve_version")?)? || {
                let a = v3(&anchor)?;
                npnorm(py, [pp[0] - a[0], pp[1] - a[1], pp[2] - a[2]])? > f64of(&s.getattr("step")?)? * f64of(&s.getattr("cells")?)? * 0.25
            };
        if need {
            s.call_method1("_build", (planet, player_pos, carve.cloned().unwrap_or_else(|| none(py))))?;
        }
        let c = v3(&s.getattr("center")?)?;
        let o = v3(origin)?;
        root.call_method1("setPos", (c[0] - o[0], c[1] - o[1], c[2] - o[2]))?;
        Ok(())
    }
    #[pyo3(signature = (pl, player_pos, carve=None))]
    fn _build(slf: &Bound<'_, Self>, pl: &B<'_>, player_pos: &B<'_>, carve: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let npm = np(py)?;
        let carve = carve.filter(|c| !c.is_none());
        s.setattr("planet", pl)?;
        s.setattr("version", pl.getattr("version")?)?;
        s.setattr("carve_version", match carve { Some(c) => c.getattr("version")?, None => tb(py, -1) })?;
        let use_carve = match carve { Some(c) => c.getattr("enabled")?.is_truthy()?, None => false };
        let up_o = pl.call_method1("up", (player_pos,))?;
        let up = v3(&up_o)?;
        let r0 = f64of(&pl.call_method1("surface_radius", (up_o.get_item(py.None())?,))?.get_item(0)?)?;
        // 格子をスナップしてちらつきを防ぐ
        let rf = if up[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let mut t1 = cross(up, rf);
        let n1 = npnorm(py, t1)?;
        t1 = [t1[0] / n1, t1[1] / n1, t1[2] / n1];
        let t2 = cross(up, t1);
        let n: i64 = s.getattr("cells")?.extract()?;
        let step = f64of(&s.getattr("step")?)?;
        s.setattr("frame", (np3(py, up), np3(py, t1), np3(py, t2), r0, n as f64 * step))?;
        let m = (2 * n + 1) as usize;
        let g: Vec<f64> = (-n..=n).map(|k| k as f64 * step).collect();
        let nv = m * m;
        let mut d = vec![0.0f64; nv * 3];
        for a in 0..m {
            for b in 0..m {
                let (gx, gy) = (g[a], g[b]);
                let i = a * m + b;
                let mut v = [0.0; 3];
                for k in 0..3 {
                    v[k] = up[k] * r0 + t1[k] * gx + t2[k] * gy;
                }
                let nn = rnorm(&v);
                for k in 0..3 {
                    d[i * 3 + k] = v[k] / nn;
                }
            }
        }
        let d_o = f64_2(py, d.clone(), 3)?;
        let sm = a2(&pl.call_method1("_sample", (&d_o,))?, 2)?;
        let mut r: Vec<f64> = (0..nv).map(|i| sm.d[i * 2]).collect();
        let wr: Vec<f64> = (0..nv).map(|i| sm.d[i * 2 + 1]).collect();
        let r_nat = r.clone();
        let center = v3(&pl.getattr("center")?)?;
        let craters = pl.getattr("craters")?;
        let has_craters = craters.is_truthy()?;
        if has_craters && !use_carve {
            let pts: Vec<f64> = (0..nv * 3).map(|j| center[j % 3] + d[j] * r[j / 3]).collect();
            let sv = a1(&pl.call_method1("sdf", (f64_2(py, pts, 3)?,))?)?;
            let idx: Vec<usize> = (0..nv).filter(|&i| sv[i] > 1e-4).collect();
            if !idx.is_empty() {
                let mut maxr = f64::NEG_INFINITY;
                for c in craters.iter()? {
                    maxr = maxr.max(f64of(&c?.get_item(3)?)?);
                }
                let mut lo: Vec<f64> = idx.iter().map(|&i| r[i] - 2.2 * maxr).collect();
                let mut hi: Vec<f64> = idx.iter().map(|&i| r[i]).collect();
                for _ in 0..16 {
                    let mid: Vec<f64> = lo.iter().zip(hi.iter()).map(|(a, b)| (a + b) * 0.5).collect();
                    let pts: Vec<f64> = (0..idx.len() * 3).map(|j| center[j % 3] + d[idx[j / 3] * 3 + j % 3] * mid[j / 3]).collect();
                    let s2 = a1(&pl.call_method1("sdf", (f64_2(py, pts, 3)?,))?)?;
                    for k in 0..idx.len() {
                        if s2[k] > 0.0 {
                            hi[k] = mid[k];
                        } else {
                            lo[k] = mid[k];
                        }
                    }
                }
                for (k, &i) in idx.iter().enumerate() {
                    r[i] = lo[k];
                }
            }
        }
        let mut inside = vec![false; nv];
        if use_carve && carve.unwrap().getattr("active")?.is_truthy()? {
            let lp: Vec<f64> = (0..nv * 3).map(|j| d[j] * r[j / 3]).collect();
            let ins = carve.unwrap().call_method1("contains", (f64_2(py, lp, 3)?,))?;
            for (i, x) in ins.iter()?.enumerate() {
                inside[i] = x?.is_truthy()?;
            }
            for i in 0..nv {
                r[i] -= if inside[i] { 0.08 } else { 0.0 }; // 掘削・洞窟の詳細メッシュを優先して見せる
            }
        }
        let pos: Vec<f64> = (0..nv * 3).map(|j| center[j % 3] + d[j] * r[j / 3]).collect();
        s.setattr("anchor", player_pos.call_method0("copy")?)?;
        let cen = [center[0] + up[0] * r0, center[1] + up[1] * r0, center[2] + up[2] * r0];
        s.setattr("center", np3(py, cen))?;
        let rel: Vec<f64> = (0..nv * 3).map(|j| pos[j] - cen[j % 3]).collect();
        // np.gradient（中は中心差分、端は片側差分）
        let at = |a: usize, b: usize, k: usize| rel[(a * m + b) * 3 + k];
        let grad = |a: usize, b: usize, k: usize, axis: usize| -> f64 {
            let (i, len) = if axis == 0 { (a, m) } else { (b, m) };
            let f = |j: usize| if axis == 0 { at(j, b, k) } else { at(a, j, k) };
            if len < 2 {
                0.0
            } else if i == 0 {
                f(1) - f(0)
            } else if i == len - 1 {
                f(len - 1) - f(len - 2)
            } else {
                (f(i + 1) - f(i - 1)) / 2.0
            }
        };
        let mut nrm = vec![0.0f64; nv * 3];
        for a in 0..m {
            for b in 0..m {
                let ex = [grad(a, b, 0, 0), grad(a, b, 1, 0), grad(a, b, 2, 0)];
                let ey = [grad(a, b, 0, 1), grad(a, b, 1, 1), grad(a, b, 2, 1)];
                let mut c = cross(ex, ey);
                let nn = rnorm(&c).max(1e-9);
                c = [c[0] / nn, c[1] / nn, c[2] / nn];
                let i = a * m + b;
                let sdot = c[0] * d[i * 3] + c[1] * d[i * 3 + 1] + c[2] * d[i * 3 + 2];
                let sg = if sdot > 0.0 { 1.0 } else if sdot < 0.0 { -1.0 } else { 0.0 };
                for k in 0..3 {
                    nrm[i * 3 + k] = c[k] * sg;
                }
            }
        }
        let nrm_o = f64_2(py, nrm.clone(), 3)?;
        let mut col = a2(&pl.call_method1("surface_color", (&d_o, f64_1(py, r_nat.clone()), &nrm_o))?, 3)?;
        // 掘った所は土の色に
        if has_craters && !use_carve {
            let dirt = [0.45, 0.33, 0.22];
            for i in 0..nv {
                let depth = ((r_nat[i] - r[i]) / 1.0).clamp(0.0, 1.0);
                for k in 0..3 {
                    col.d[i * 3 + k] = col.d[i * 3 + k] * (1.0 - depth) + dirt[k] * depth;
                }
            }
        }
        let s_ = sun(py)?;
        let amb = ambient(py)?;
        let mut v = vec![0f32; nv * 12];
        for a in 0..m {
            for b in 0..m {
                let i = a * m + b;
                let nn = [nrm[i * 3], nrm[i * 3 + 1], nrm[i * 3 + 2]];
                let sh = shade_rs(s_, amb, nn, col.r3(i));
                let rr = &mut v[i * 12..i * 12 + 12];
                for k in 0..3 {
                    rr[k] = rel[i * 3 + k] as f32;
                    rr[3 + k] = nn[k] as f32;
                    rr[6 + k] = sh[k] as f32;
                }
                rr[9] = 1.0;
                rr[10] = (g[a] / 4.0) as f32;
                rr[11] = (g[b] / 4.0) as f32;
            }
        }
        let mut quads: Vec<[i64; 4]> = vec![];
        for a in 0..m - 1 {
            for b in 0..m - 1 {
                let i = (a * m + b) as i64;
                let mm = m as i64;
                quads.push([i, i + mm, i + mm + 1, i + 1]);
            }
        }
        if inside.iter().any(|x| *x) {
            quads.retain(|q| !q.iter().all(|&k| inside[k as usize]));
        }
        let tris: Vec<i64> = quads.iter().flat_map(|q| [q[0], q[1], q[2], q[0], q[2], q[3]]).collect();
        let mesh = s.getattr("mesh")?;
        mesh.call_method1("set", (f32_2(py, v, 12)?, i64_2(py, tris, 3)?))?;
        let rm = rmod(py)?;
        let tex = rm.call_method1("load_texture", (s.getattr("loader")?, get(&pl.getattr("defn")?, "texture", py.None())?))?;
        if tex.is_truthy()? {
            mesh.getattr("np")?.call_method1("setTexture", (tex, 1))?;
        }
        // 水面
        let water = s.getattr("water")?;
        if pl.getattr("has_water")?.is_truthy()? {
            let wet: Vec<bool> = (0..nv).map(|i| wr[i] > r_nat[i]).collect();
            let wq: Vec<[i64; 4]> = quads.iter().filter(|q| q.iter().any(|&k| wet[k as usize])).cloned().collect();
            if !wq.is_empty() {
                let sh_c = [0.2, 0.42, 0.75];
                let mut wv = vec![0f32; nv * 12];
                for i in 0..nv {
                    let dv = [d[i * 3], d[i * 3 + 1], d[i * 3 + 2]];
                    let sh = shade_rs(s_, amb, dv, sh_c);
                    let rr = &mut wv[i * 12..i * 12 + 12];
                    for k in 0..3 {
                        rr[k] = (center[k] + dv[k] * wr[i] - cen[k]) as f32;
                        rr[3 + k] = dv[k] as f32;
                        rr[6 + k] = sh[k] as f32;
                    }
                    rr[9] = 0.72;
                }
                let wt: Vec<i64> = wq.iter().flat_map(|q| [q[0], q[1], q[2], q[0], q[2], q[3]]).collect();
                water.call_method1("set", (f32_2(py, wv, 12)?, i64_2(py, wt, 3)?))?;
                water.getattr("np")?.call_method0("show")?;
            } else {
                water.getattr("np")?.call_method0("hide")?;
            }
        } else {
            water.getattr("np")?.call_method0("hide")?;
        }
        let _ = npm;
        Ok(())
    }
}

// ---------------------------------------------------------------- CarveView
/// 洞窟や掘削跡のまわりを、距離場から作った詳細なメッシュで描く（8 m の立方体チャンクごと）。
/// 高さ場の地表（TerrainPatch）では表せない横穴・洞窟の中を見せる
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct CarveView {}

fn ckey(py: Python<'_>, k: [i64; 3]) -> B<'_> {
    PyTuple::new_bound(py, k).into_any()
}

#[pymethods]
impl CarveView {
    #[classattr]
    #[allow(non_snake_case)]
    fn CH() -> f64 {
        8.0
    }
    #[classattr]
    #[allow(non_snake_case)]
    fn RES() -> f64 {
        0.5
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        CarveView {}
    }
    #[pyo3(signature = (parent, loader, view_range=56.0))]
    fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, loader: &B<'_>, view_range: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("enabled", true)?;
        let root = parent.call_method1("attachNewNode", ("carve",))?;
        s.setattr("root", &root)?;
        s.setattr("mesh", dmesh(py, &root, "carve")?)?;
        s.setattr("loader", loader)?;
        s.setattr("range", view_range)?;
        s.setattr("cache", PyDict::new_bound(py))?; // (チャンクの位置) → (verts, v, tris) or None
        s.setattr("active", PySet::empty_bound(py)?)?; // 描画中のチャンク（惑星ローカルの整数座標）
        s.setattr("version", 0)?;
        s.setattr("planet", py.None())?;
        s.setattr("anchor", py.None())?;
        s.setattr("pver", -1)?;
        s.setattr("n_craters", 0)?;
        s.setattr("pending", PyList::empty_bound(py))?;
        s.setattr("dirty", false)?;
        Ok(())
    }
    /// 惑星ローカル座標の点が描画中のチャンクに入っているか
    fn contains<'py>(slf: &Bound<'py, Self>, local_pts: &B<'py>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let active = s.getattr("active")?;
        let p = a2(local_pts, 3)?;
        let npm = np(py)?;
        if !active.is_truthy()? {
            return npm.call_method1("zeros", (p.n, py.import_bound("builtins")?.getattr("bool")?));
        }
        let ch = f64of(&s.getattr("CH")?)?;
        let mut keys = std::collections::HashSet::new();
        for k in active.iter()? {
            let k: (i64, i64, i64) = k?.extract()?;
            keys.insert(k);
        }
        let out: Vec<bool> = (0..p.n).map(|i| {
            let r = p.r3(i);
            keys.contains(&((r[0] / ch).floor() as i64, (r[1] / ch).floor() as i64, (r[2] / ch).floor() as i64))
        }).collect();
        Ok(numpy::PyArray1::from_vec_bound(py, out).into_any())
    }
    fn _wanted<'py>(slf: &Bound<'py, Self>, pl: &B<'py>, player_pos: &B<'py>) -> R<Bound<'py, PySet>> {
        let py = slf.py();
        let s = slf.as_any();
        let pp = v3(player_pos)?;
        let c0 = v3(&pl.getattr("center")?)?;
        let local = [pp[0] - c0[0], pp[1] - c0[1], pp[2] - c0[2]];
        let rng = f64of(&s.getattr("range")?)?;
        let ch = f64of(&s.getattr("CH")?)?;
        let mut boxes: Vec<([f64; 3], [f64; 3])> = vec![];
        if pl.getattr("has_caves")?.is_truthy()? {
            let (ca, cb, ra, rb) = (pl.getattr("cave_a")?, pl.getattr("cave_b")?, pl.getattr("cave_ra")?, pl.getattr("cave_rb")?);
            for k in pl.call_method1("caves_near", (np3(py, local), rng))?.iter()? {
                let k = k?;
                let (a, b) = (v3(&ca.get_item(&k)?)?, v3(&cb.get_item(&k)?)?);
                let rr = f64of(&ra.get_item(&k)?)?.max(f64of(&rb.get_item(&k)?)?) + 1.0;
                boxes.push(([a[0].min(b[0]) - rr, a[1].min(b[1]) - rr, a[2].min(b[2]) - rr], [a[0].max(b[0]) + rr, a[1].max(b[1]) + rr, a[2].max(b[2]) + rr]));
            }
        }
        for c in pl.getattr("craters")?.iter()? {
            let c = c?;
            let cc = [f64of(&c.get_item(0)?)?, f64of(&c.get_item(1)?)?, f64of(&c.get_item(2)?)?];
            let cr = f64of(&c.get_item(3)?)?;
            if npnorm(py, [cc[0] - local[0], cc[1] - local[1], cc[2] - local[2]])? < rng + cr {
                boxes.push(([cc[0] - cr - 1.0, cc[1] - cr - 1.0, cc[2] - cr - 1.0], [cc[0] + cr + 1.0, cc[1] + cr + 1.0, cc[2] + cr + 1.0]));
            }
        }
        let fl = |x: f64| (x / ch).floor() as i64;
        let lo_v = [fl(local[0] - rng), fl(local[1] - rng), fl(local[2] - rng)];
        let hi_v = [fl(local[0] + rng), fl(local[1] + rng), fl(local[2] + rng)];
        let want = PySet::empty_bound(py)?;
        let mut keys: Vec<[i64; 3]> = vec![];
        let mut seen = std::collections::HashSet::new();
        for (lo, hi) in boxes {
            let i0 = [fl(lo[0]).max(lo_v[0]), fl(lo[1]).max(lo_v[1]), fl(lo[2]).max(lo_v[2])];
            let i1 = [fl(hi[0]).min(hi_v[0]), fl(hi[1]).min(hi_v[1]), fl(hi[2]).min(hi_v[2])];
            if (0..3).any(|k| i1[k] < i0[k]) {
                continue;
            }
            for x in i0[0]..=i1[0] {
                for y in i0[1]..=i1[1] {
                    for z in i0[2]..=i1[2] {
                        if seen.insert([x, y, z]) {
                            keys.push([x, y, z]);
                            want.add(ckey(py, [x, y, z]))?;
                        }
                    }
                }
            }
        }
        if keys.len() > 900 {
            // 多すぎるときは近い順に
            let sorted = py.import_bound("builtins")?.getattr("sorted")?;
            let keyf = Py::new(py, ChunkDist { local, ch })?;
            let l = sorted.call((&want,), Some(&[("key", keyf)].into_py_dict_bound(py)))?;
            let out = PySet::empty_bound(py)?;
            for k in l.iter()?.take(900) {
                out.add(k?)?;
            }
            return Ok(out);
        }
        Ok(want)
    }
    fn _mesh_chunk<'py>(slf: &Bound<'py, Self>, pl: &B<'py>, key: &B<'py>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let shape = imp(py, "main.shape")?;
        let npm = np(py)?;
        let ch = f64of(&s.getattr("CH")?)?;
        let res = f64of(&s.getattr("RES")?)?;
        let n = (ch / res).round() as usize + 1;
        let k3: [i64; 3] = { let v: Vec<i64> = key.extract()?; [v[0], v[1], v[2]] };
        let o = [k3[0] as f64 * ch, k3[1] as f64 * ch, k3[2] as f64 * ch];
        let g: Vec<f64> = (0..n).map(|i| i as f64 * res).collect();
        let c0 = v3(&pl.getattr("center")?)?;
        // numpy では (格子 + o) + center の順
        let mut pts2 = Vec::with_capacity(n * n * n * 3);
        for a in 0..n {
            for b in 0..n {
                for c in 0..n {
                    let p = [g[a] + o[0], g[b] + o[1], g[c] + o[2]];
                    pts2.extend_from_slice(&[p[0] + c0[0], p[1] + c0[1], p[2] + c0[2]]);
                }
            }
        }
        let parts = pl.call_method1("sdf_parts", (f64_2(py, pts2, 3)?,))?;
        let (dd, cc) = (a1(&parts.get_item(1)?)?, parts.get_item(2)?);
        let sv: Vec<f64> = if cc.is_none() {
            dd
        } else {
            let cv = a1(&cc)?;
            dd.iter().zip(cv.iter()).map(|(a, b)| a.max(-b)).collect()
        };
        let mn = sv.iter().cloned().fold(f64::INFINITY, f64::min);
        let mx = sv.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        if mn > 0.0 || mx < 0.0 {
            return Ok(none(py));
        }
        let vol = numpy::PyArrayMethods::reshape(&numpy::PyArray1::from_vec_bound(py, sv.iter().map(|x| *x as f32).collect::<Vec<f32>>()), [n, n, n])?;
        let mc = match shape.call_method("marching_cubes", (vol, 0.0), Some(&[("spacing", (res, res, res))].into_py_dict_bound(py))) {
            Ok(r) => r,
            Err(e) if is_exc(py, &e) => return Ok(none(py)),
            Err(e) => return Err(e),
        };
        let (verts_o, faces_o) = (mc.get_item(0)?, mc.get_item(1)?);
        if faces_o.len()? == 0 {
            return Ok(none(py));
        }
        let vr = a2(&verts_o, 3)?;
        let nv = vr.n;
        let verts: Vec<f64> = (0..nv * 3).map(|j| vr.d[j] + o[j % 3]).collect();
        let wp: Vec<f64> = (0..nv * 3).map(|j| verts[j] + c0[j % 3]).collect();
        let wp_o = f64_2(py, wp, 3)?;
        let nrm_o = pl.call_method1("sdf_grad", (&wp_o,))?.get_item(1)?;
        let nrm = a2(&nrm_o, 3)?;
        let hv = a1(&pl.call_method1("sdf_parts", (&wp_o,))?.get_item(0)?)?;
        let depth: Vec<f64> = hv.iter().map(|h| (-h).max(0.0)).collect();
        let vn: Vec<f64> = (0..nv).map(|i| rnorm(&verts[i * 3..i * 3 + 3])).collect();
        let dirs: Vec<f64> = (0..nv * 3).map(|j| verts[j] / vn[j / 3]).collect();
        let radii: Vec<f64> = (0..nv).map(|i| vn[i] + depth[i]).collect();
        let top = a2(&pl.call_method1("surface_color", (f64_2(py, dirs, 3)?, f64_1(py, radii), &nrm_o))?, 3)?;
        let dirt = [0.42, 0.34, 0.26];
        let stone = [0.44, 0.42, 0.4];
        let mut col = vec![0.0f64; nv * 3];
        for i in 0..nv {
            let t = ((depth[i] - 1.5) / 4.0).clamp(0.0, 1.0);
            let k = (depth[i] / 0.6).clamp(0.0, 1.0);
            for c in 0..3 {
                let under = dirt[c] * (1.0 - t) + stone[c] * t;
                col[i * 3 + c] = top.d[i * 3 + c] * (1.0 - k) + under * k;
            }
        }
        let col = a2(&pl.call_method1("underground_tint", (f64_2(py, verts.clone(), 3)?, f64_2(py, col, 3)?))?, 3)?;
        // 地中は日が当たらないので、向きによる陰影を弱めて少し暗くする（真っ暗にはしない）
        let s_ = sun(py)?;
        let amb = ambient(py)?;
        let mut v = vec![0f32; nv * 12];
        for i in 0..nv {
            let nn = nrm.r3(i);
            let deep = ((depth[i] - 1.0) / 6.0).clamp(0.0, 1.0);
            let sh = shade_rs(s_, amb, nn, col.r3(i));
            let lam = dot(nn, s_).clamp(0.0, 1.0);
            let rr = &mut v[i * 12..i * 12 + 12];
            for c in 0..3 {
                rr[3 + c] = nn[c] as f32;
                rr[6 + c] = (sh[c] * (1.0 - deep) + col.d[i * 3 + c] * (0.62 + 0.18 * lam) * deep) as f32;
            }
            rr[9] = 1.0;
            let ab = [nn[0].abs(), nn[1].abs(), nn[2].abs()];
            let mut ax = 0;
            for j in 1..3 {
                if ab[j] > ab[ax] {
                    ax = j;
                }
            }
            let u = if ax == 0 { verts[i * 3 + 1] } else { verts[i * 3] };
            let w = if ax == 2 { verts[i * 3 + 1] } else { verts[i * 3 + 2] };
            rr[10] = (u / 4.0) as f32;
            rr[11] = (w / 4.0) as f32;
        }
        let faces = npm.call_method1("ascontiguousarray", (faces_o, npm.getattr("int64")?))?;
        Ok(PyTuple::new_bound(py, [f64_2(py, verts, 3)?, f32_2(py, v, 12)?, faces]).into_any())
    }
    fn update(slf: &Bound<'_, Self>, pl: &B<'_>, player_pos: &B<'_>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let root = s.getattr("root")?;
        let pp = v3(player_pos)?;
        let far = pl.is_none() || {
            let c = v3(&pl.getattr("center")?)?;
            npnorm(py, [pp[0] - c[0], pp[1] - c[1], pp[2] - c[2]])? - f64of(&pl.getattr("radius")?)? > 300.0
        };
        if !s.getattr("enabled")?.is_truthy()? || far {
            if s.getattr("active")?.is_truthy()? {
                s.setattr("active", PySet::empty_bound(py)?)?;
                let v: i64 = s.getattr("version")?.extract()?;
                s.setattr("version", v + 1)?;
            }
            root.call_method0("hide")?;
            return Ok(());
        }
        root.call_method0("show")?;
        if !pl.is(&s.getattr("planet")?) {
            s.setattr("planet", pl)?;
            s.setattr("anchor", py.None())?;
            s.setattr("cache", PyDict::new_bound(py))?;
            s.setattr("n_craters", 0)?;
            s.setattr("active", PySet::empty_bound(py)?)?;
        }
        let ch = f64of(&s.getattr("CH")?)?;
        // 新しい掘削跡が触れるチャンクは作り直す
        let craters: Vec<B<'_>> = pl.getattr("craters")?.iter()?.collect::<R<_>>()?;
        let nc: usize = s.getattr("n_craters")?.extract()?;
        if craters.len() != nc {
            let cache = s.getattr("cache")?;
            for c in &craters[nc.min(craters.len())..] {
                let cc = [f64of(&c.get_item(0)?)?, f64of(&c.get_item(1)?)?, f64of(&c.get_item(2)?)?];
                let cr = f64of(&c.get_item(3)?)?;
                let keys: Vec<B<'_>> = cache.iter()?.collect::<R<_>>()?;
                for key in keys {
                    let k: Vec<i64> = key.extract()?;
                    let lo: Vec<f64> = (0..3).map(|j| k[j] as f64 * ch - cr - 1.0).collect();
                    let hi: Vec<f64> = (0..3).map(|j| lo[j] + ch + 2.0 * cr + 2.0).collect();
                    if (0..3).all(|j| cc[j] >= lo[j] && cc[j] <= hi[j]) {
                        cache.call_method1("pop", (&key,))?;
                    }
                }
            }
            s.setattr("n_craters", craters.len())?;
            s.setattr("anchor", py.None())?;
        }
        let anchor = s.getattr("anchor")?;
        let moved = anchor.is_none() || {
            let a = v3(&anchor)?;
            npnorm(py, [pp[0] - a[0], pp[1] - a[1], pp[2] - a[2]])? > 6.0
        };
        if moved {
            s.setattr("anchor", player_pos.call_method0("copy")?)?;
            let want = Self::_wanted(slf, pl, player_pos)?;
            let cache = s.getattr("cache")?;
            let c0 = v3(&pl.getattr("center")?)?;
            let local = [pp[0] - c0[0], pp[1] - c0[1], pp[2] - c0[2]];
            let l = PyList::empty_bound(py);
            for k in want.iter() {
                if !cache.contains(&k)? {
                    l.append(k)?;
                }
            }
            let keyf = Py::new(py, ChunkDist { local, ch })?;
            let pend = py.import_bound("builtins")?.getattr("sorted")?.call((l,), Some(&[("key", keyf)].into_py_dict_bound(py)))?;
            s.setattr("pending", pend)?;
            s.setattr("want", want)?;
            s.setattr("dirty", true)?;
        }
        // 1フレームに作るチャンクは時間で制限（カクつき防止）
        let time = imp(py, "time")?;
        let pc = || -> R<f64> { time.call_method0("perf_counter")?.extract() };
        let t0 = pc()?;
        let pending = s.getattr("pending")?;
        let cache = s.getattr("cache")?;
        while pending.is_truthy()? && pc()? - t0 < 0.012 {
            let k = pending.call_method1("pop", (0,))?;
            if !cache.contains(&k)? {
                cache.set_item(&k, Self::_mesh_chunk(slf, pl, &k)?)?;
            }
            s.setattr("dirty", true)?;
        }
        if s.getattr("dirty")?.is_truthy()? && (!pending.is_truthy()? || pc()? - f64of(&gd(s, "_last_asm", tb(py, 0.0)))? > 0.4) {
            s.setattr("_last_asm", pc()?)?;
            s.call_method0("_assemble")?;
        }
        let c = v3(&pl.getattr("center")?)?;
        let o = v3(origin)?;
        root.call_method1("setPos", (c[0] - o[0], c[1] - o[1], c[2] - o[2]))?;
        Ok(())
    }
    fn _assemble(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("dirty", false)?;
        let mut vs: Vec<f32> = vec![];
        let mut ts: Vec<i64> = vec![];
        let mut n: i64 = 0;
        let active = PySet::empty_bound(py)?;
        let cache = s.getattr("cache")?;
        let want = s.getattr("want")?;
        let npm = np(py)?;
        for k in want.iter()? {
            let k = k?;
            let m = cache.call_method1("get", (&k,))?;
            if m.is_none() {
                continue;
            }
            active.add(&k)?;
            let verts = a2(&m.get_item(0)?, 3)?;
            let v: Vec<f32> = {
                let r: numpy::PyReadonlyArray2<f32> = npm.call_method1("ascontiguousarray", (m.get_item(1)?, npm.getattr("float32")?))?.extract()?;
                r.as_slice()?.to_vec()
            };
            let (f, _) = i2(&m.get_item(2)?, 3)?;
            let base = vs.len();
            vs.extend_from_slice(&v);
            for i in 0..verts.n {
                for c in 0..3 {
                    vs[base + i * 12 + c] = verts.d[i * 3 + c] as f32;
                }
            }
            ts.extend(f.iter().map(|x| x + n));
            n += verts.n as i64;
        }
        let mesh = s.getattr("mesh")?;
        if !vs.is_empty() {
            mesh.call_method1("set", (f32_2(py, vs, 12)?, i64_2(py, ts, 3)?))?;
            let rm = rmod(py)?;
            let tex = rm.call_method1("load_texture", (s.getattr("loader")?, get(&s.getattr("planet")?.getattr("defn")?, "texture", py.None())?))?;
            if tex.is_truthy()? {
                mesh.getattr("np")?.call_method1("setTexture", (tex, 1))?;
            }
            mesh.getattr("np")?.call_method0("show")?;
        } else {
            mesh.getattr("np")?.call_method0("hide")?;
        }
        // 画面外のキャッシュを捨てる
        if cache.len()? > 2500 {
            let keys: Vec<B<'_>> = cache.iter()?.collect::<R<_>>()?;
            for k in keys {
                if !want.contains(&k)? {
                    cache.call_method1("pop", (&k,))?;
                }
            }
        }
        if !active.eq(s.getattr("active")?)? {
            s.setattr("active", active)?;
            let v: i64 = s.getattr("version")?.extract()?;
            s.setattr("version", v + 1)?;
        }
        let _ = fmt_err;
        Ok(())
    }
}

/// チャンクの近さ（並べ替えのキー）
#[pyclass(module = "alpedia_core")]
pub struct ChunkDist {
    local: [f64; 3],
    ch: f64,
}
#[pymethods]
impl ChunkDist {
    fn __call__(&self, py: Python<'_>, k: &B<'_>) -> R<f64> {
        let v: Vec<i64> = k.extract()?;
        let p: [f64; 3] = std::array::from_fn(|j| (v[j] as f64 + 0.5) * self.ch - self.local[j]);
        npnorm(py, p)
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    for f in [wrap_pyfunction_bound!(render_load_texture, m)?, wrap_pyfunction_bound!(render_dynamic_mesh, m)?, wrap_pyfunction_bound!(render_quad_tris, m)?,
              wrap_pyfunction_bound!(render_shade, m)?, wrap_pyfunction_bound!(render_project_uv, m)?, wrap_pyfunction_bound!(render_block_ao, m)?,
              wrap_pyfunction_bound!(render_frames8, m)?, wrap_pyfunction_bound!(render_face_frame_p, m)?, wrap_pyfunction_bound!(render_face_frame, m)?,
              wrap_pyfunction_bound!(render_display_faces, m)?, wrap_pyfunction_bound!(render_lod_vertices, m)?, wrap_pyfunction_bound!(render_light_ids, m)?,
              wrap_pyfunction_bound!(render_preload, m)?, wrap_pyfunction_bound!(render_edge_stage, m)?] {
        m.add_function(f)?;
    }
    m.add_class::<LightLayer>()?;
    m.add_class::<LodLayer>()?;
    m.add_class::<ConstructView>()?;
    m.add_class::<DecalView>()?;
    m.add_class::<PlanetView>()?;
    m.add_class::<TerrainPatch>()?;
    m.add_class::<CarveView>()?;
    m.add_class::<ChunkDist>()?;
    Ok(())
}
