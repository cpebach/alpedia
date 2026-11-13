//! 景色の箱（main/scenery.py の Scenery）を描く（main/client/scenery_view.py）。シナリオ・場所の MOD の画面側が使う。

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::imp;
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

const FACES: [(usize, f64); 6] = [(0, 1.0), (0, -1.0), (1, 1.0), (1, -1.0), (2, 1.0), (2, -1.0)];

fn none(py: Python<'_>) -> B<'_> {
    py.None().into_bound(py)
}
fn sv(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.scenery_view")?.into_any())
}
fn v3(o: &B<'_>) -> R<[f64; 3]> {
    Ok([f64of(&o.get_item(0)?)?, f64of(&o.get_item(1)?)?, f64of(&o.get_item(2)?)?])
}
fn arr(o: &B<'_>, m: usize) -> R<Vec<f64>> {
    let py = o.py();
    let np = imp(py, "numpy")?;
    let a = np.call_method1("ascontiguousarray", (o, np.getattr("float64")?))?.call_method1("reshape", (-1i64, m as i64))?;
    let r: numpy::PyReadonlyArray2<f64> = a.extract()?;
    Ok(r.as_slice()?.to_vec())
}
fn f32_2(py: Python<'_>, d: Vec<f32>, m: usize) -> R<B<'_>> {
    let n = d.len() / m.max(1);
    let a = numpy::PyArray1::from_vec_bound(py, d);
    Ok(numpy::PyArrayMethods::reshape(&a, [n, m])?.into_any())
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn round2(x: f64) -> f64 {
    (x * 100.0).round_ties_even() / 100.0
}

/// 箱1つの6面×4頂点と法線（外から見て表になる回り順）
fn box_quads_rs(c: [f64; 3], a: [[f64; 3]; 3], e: [f64; 3]) -> ([[[f64; 3]; 4]; 6], [[f64; 3]; 6]) {
    let mut v = [[[0.0; 3]; 4]; 6];
    let mut nn = [[0.0; 3]; 6];
    for (f, &(ax, sg)) in FACES.iter().enumerate() {
        let (a1, a2) = ((ax + 1) % 3, (ax + 2) % 3);
        let n: [f64; 3] = std::array::from_fn(|k| a[ax][k] * sg);
        let u: [f64; 3] = std::array::from_fn(|k| a[a1][k] * e[a1]);
        let w: [f64; 3] = std::array::from_fn(|k| a[a2][k] * e[a2]);
        let cc: [f64; 3] = std::array::from_fn(|k| c[k] + n[k] * e[ax]);
        let q = [
            std::array::from_fn(|k| cc[k] - u[k] - w[k]),
            std::array::from_fn(|k| cc[k] + u[k] - w[k]),
            std::array::from_fn(|k| cc[k] + u[k] + w[k]),
            std::array::from_fn(|k| cc[k] - u[k] + w[k]),
        ];
        let cr = cross(std::array::from_fn(|k| q[1][k] - q[0][k]), std::array::from_fn(|k| q[2][k] - q[0][k]));
        let flip = !(dot(cr, n) < 0.0);
        v[f] = if flip { [q[3], q[2], q[1], q[0]] } else { q };
        nn[f] = n;
    }
    (v, nn)
}

/// 箱 (B) → 各面の4頂点 (B,6,4,3)・法線 (B,6,3)。外から見て表になる回り順
#[pyfunction]
pub fn scenery_box_quads<'py>(py: Python<'py>, c: &B<'py>, a: &B<'py>, e: &B<'py>) -> R<Bound<'py, PyTuple>> {
    let cc = arr(c, 3)?;
    let aa = arr(a, 9)?;
    let ee = arr(e, 3)?;
    let nb = cc.len() / 3;
    let mut vo = Vec::with_capacity(nb * 72);
    let mut no = Vec::with_capacity(nb * 18);
    for b in 0..nb {
        let ax: [[f64; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|k| aa[b * 9 + i * 3 + k]));
        let (v, n) = box_quads_rs([cc[b * 3], cc[b * 3 + 1], cc[b * 3 + 2]], ax, [ee[b * 3], ee[b * 3 + 1], ee[b * 3 + 2]]);
        for f in 0..6 {
            for q in 0..4 {
                vo.extend_from_slice(&v[f][q]);
            }
            no.extend_from_slice(&n[f]);
        }
    }
    let va = numpy::PyArrayMethods::reshape(&numpy::PyArray1::from_vec_bound(py, vo), [nb, 6, 4, 3])?;
    let na = numpy::PyArrayMethods::reshape(&numpy::PyArray1::from_vec_bound(py, no), [nb, 6, 3])?;
    Ok(PyTuple::new_bound(py, [va.into_any(), na.into_any()]))
}

/// 景色の箱の見た目の種類：岩・地面（道）・金属（建物など）・なし（光る物）
#[pyfunction]
pub fn scenery_tex_kind(py: Python<'_>, p: &B<'_>) -> R<&'static str> {
    if !p.get_item("glow")?.is_none() {
        return Ok("plain");
    }
    let s = imp(py, "main.scenery")?;
    let c = v3(&p.get_item("color")?)?;
    let c = c.map(round2);
    let same = |o: B<'_>| -> R<bool> { Ok(v3(&o)?.map(round2) == c) };
    for n in ["ROCK", "ROCK2", "ROCK3", "MOON_G"] {
        if same(s.getattr(n)?)? {
            return Ok("rock");
        }
    }
    if same(s.getattr("CONC")?)? || c == [0.40, 0.42, 0.45].map(round2) {
        return Ok("ground");
    }
    Ok("metal")
}

fn texture<'py>(py: Python<'py>, kind: &str) -> R<B<'py>> {
    let m = sv(py)?;
    let path = imp(py, "main.paths")?.getattr("SAUCE")?.call_method1("__truediv__", ("textures",))?
        .call_method1("__truediv__", (m.getattr("TEXTURES")?.get_item(kind)?.get_item(0)?,))?;
    let pixel = m.getattr("PIXEL")?.contains(kind)?;
    imp(py, "main.client.gfx")?.call_method1("tex", (path.str()?, pixel))
}

/// 景色の箱1か所の描画。ふつうの箱（影つき・模様つき）・明かり（点く／消える）・点滅のメッシュ
#[pyclass(module = "main.client.scenery_view", subclass, dict)]
pub struct SceneView {}

#[pymethods]
impl SceneView {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        SceneView {}
    }
    fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, scene: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("s", scene)?;
        let meshes = PyDict::new_bound(py);
        s.setattr("meshes", &meshes)?;
        let parts: Vec<B<'_>> = scene.getattr("parts")?.iter()?.collect::<R<_>>()?;
        let mut order: Vec<String> = vec![];
        let mut groups: Vec<Vec<usize>> = vec![];
        for (k, p) in parts.iter().enumerate() {
            let key = if !p.get_item("blink")?.is_none() {
                "blink".to_string()
            } else if !p.get_item("group")?.is_none() {
                "lights".to_string()
            } else {
                format!("static:{}", scenery_tex_kind(py, p)?)
            };
            match order.iter().position(|x| *x == key) {
                Some(i) => groups[i].push(k),
                None => {
                    order.push(key);
                    groups.push(vec![k]);
                }
            }
        }
        s.setattr("state", PyDict::new_bound(py))?;
        let m = sv(py)?;
        let textures = m.getattr("TEXTURES")?;
        let rm = imp(py, "main.client.render")?;
        let sid = scene.getattr("id")?.str()?.to_string();
        let cc = arr(&scene.getattr("C")?, 3)?;
        let aa = arr(&scene.getattr("A")?, 9)?;
        let ee = arr(&scene.getattr("E")?, 3)?;
        let center = v3(&scene.getattr("center")?)?;
        for (name, idx) in order.iter().zip(groups.iter()) {
            let mesh = rm.call_method1("DynamicMesh", (parent, format!("scenery_{}_{}", sid, name)))?;
            mesh.getattr("node")?.call_method0("clearBounds")?;
            mesh.getattr("node")?.call_method1("setFinal", (false,))?;
            mesh.getattr("np")?.call_method1("setTwoSided", (false,))?;
            let kind = if let Some((_, k)) = name.split_once(':') { k.to_string() } else { "plain".to_string() };
            let mut tile = 1.0;
            if textures.contains(&kind)? {
                mesh.getattr("np")?.call_method1("setTexture", (texture(py, &kind)?, 1))?;
                tile = f64of(&textures.get_item(&kind)?.get_item(1)?)?;
            }
            let keep = [0usize, 1, 2, 4, 5]; // 下の面は見えない
            let nf = keep.len();
            let nb = idx.len();
            let mut v = vec![0f32; nb * nf * 4 * 12];
            for (bi, &k) in idx.iter().enumerate() {
                let c: [f64; 3] = std::array::from_fn(|j| cc[k * 3 + j] - center[j]);
                let a: [[f64; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| aa[k * 9 + i * 3 + j]));
                let e: [f64; 3] = std::array::from_fn(|j| ee[k * 3 + j]);
                let (vq, nq) = box_quads_rs(c, a, e);
                for (j, &f) in keep.iter().enumerate() {
                    // 模様の座標：面の中の2つの軸に沿った長さ（m）÷ 1枚の大きさ
                    let ax = FACES[f].0;
                    let (a1, a2) = if ax != 1 { (if ax == 0 { 2 } else { 0 }, 1) } else { ((ax + 1) % 3, (ax + 2) % 3) };
                    for q in 0..4 {
                        let p = vq[f][q];
                        let rel: [f64; 3] = std::array::from_fn(|t| p[t] - c[t]);
                        let row = ((bi * nf + j) * 4 + q) * 12;
                        let r = &mut v[row..row + 12];
                        for t in 0..3 {
                            r[t] = p[t] as f32;
                            r[3 + t] = nq[f][t] as f32;
                        }
                        r[9] = 1.0;
                        r[10] = (dot(rel, a[a1]) / tile) as f32;
                        r[11] = (dot(rel, a[a2]) / tile) as f32;
                    }
                }
            }
            let it = PyDict::new_bound(py);
            it.set_item("mesh", &mesh)?;
            it.set_item("idx", numpy::PyArray1::from_vec_bound(py, idx.iter().map(|x| *x as i64).collect::<Vec<i64>>()))?;
            it.set_item("v", f32_2(py, v, 12)?)?;
            it.set_item("nf", nf)?;
            it.set_item("tris", rm.call_method1("quad_tris", (nb * nf,))?)?;
            it.set_item("new", true)?;
            it.set_item("bright", if textures.contains(&kind)? { 1.35 } else { 1.15 })?;
            meshes.set_item(name, it)?;
        }
        for name in &order {
            if name.starts_with("static") {
                s.call_method1("_colors", (name, py.None()))?;
            }
        }
        s.setattr("edges", Self::_edges(slf, parent, scene)?)?;
        Ok(())
    }
    /// ふつうの箱（光らない物）のふちに細い線（景色が平らな色の板に見えないように）。
    /// 線は細い十字の板（辺ごとに2枚）で描く
    fn _edges<'py>(slf: &Bound<'py, Self>, parent: &B<'py>, scene: &B<'py>) -> R<B<'py>> {
        let py = slf.py();
        let parts: Vec<B<'_>> = scene.getattr("parts")?.iter()?.collect::<R<_>>()?;
        let mut idx = vec![];
        for (k, p) in parts.iter().enumerate() {
            if p.get_item("glow")?.is_none() && p.get_item("blink")?.is_none() && p.get_item("group")?.is_none() {
                idx.push(k);
            }
        }
        if idx.is_empty() {
            return Ok(none(py));
        }
        let cc = arr(&scene.getattr("C")?, 3)?;
        let aa = arr(&scene.getattr("A")?, 9)?;
        let ee = arr(&scene.getattr("E")?, 3)?;
        let center = v3(&scene.getattr("center")?)?;
        let mut signs = [[0.0f64; 3]; 8];
        let mut t = 0;
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    signs[t] = [sx, sy, sz];
                    t += 1;
                }
            }
        }
        let mut pairs = vec![];
        for i in 0..8 {
            for j in i + 1..8 {
                if (0..3).filter(|&k| signs[i][k] != signs[j][k]).count() == 1 {
                    pairs.push((i, j));
                }
            }
        }
        let mut quads: Vec<[[f64; 3]; 4]> = vec![];
        for &k in &idx {
            let c: [f64; 3] = std::array::from_fn(|j| cc[k * 3 + j] - center[j]);
            let a: [[f64; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| aa[k * 9 + i * 3 + j]));
            let e: [f64; 3] = std::array::from_fn(|j| ee[k * 3 + j]);
            let emin = e[0].min(e[1]).min(e[2]);
            if emin < 0.04 {
                continue; // 細い棒・板は線を付けない（線だらけになる）
            }
            let w = (emin * 0.02).clamp(0.012, 0.04);
            // A の行が箱の軸（面より少し外に出して、面に埋もれないように）
            let corners: Vec<[f64; 3]> = signs.iter().map(|sg| {
                let q: [f64; 3] = std::array::from_fn(|j| sg[j] * (e[j] + w * 0.5));
                std::array::from_fn(|t| c[t] + (q[0] * a[0][t] + q[1] * a[1][t] + q[2] * a[2][t]))
            }).collect();
            for &(i, j) in &pairs {
                let ax = (0..3).find(|&k| signs[i][k] != signs[j][k]).unwrap();
                for o in [(ax + 1) % 3, (ax + 2) % 3] {
                    // 辺に垂直な2つの向きに板
                    let d: [f64; 3] = std::array::from_fn(|t| a[o][t] * w);
                    let (p0, p1) = (corners[i], corners[j]);
                    quads.push([std::array::from_fn(|t| p0[t] - d[t]), std::array::from_fn(|t| p1[t] - d[t]), std::array::from_fn(|t| p1[t] + d[t]),
                        std::array::from_fn(|t| p0[t] + d[t])]);
                }
            }
        }
        if quads.is_empty() {
            return Ok(none(py));
        }
        let rgba: Vec<f64> = sv(py)?.getattr("EDGE_RGBA")?.iter()?.map(|x| f64of(&x?)).collect::<R<_>>()?;
        let mut v = vec![0f32; quads.len() * 4 * 12];
        for (qi, q) in quads.iter().enumerate() {
            for (j, p) in q.iter().enumerate() {
                let r = &mut v[(qi * 4 + j) * 12..(qi * 4 + j) * 12 + 12];
                for t in 0..3 {
                    r[t] = p[t] as f32;
                }
                r[4] = 1.0;
                for t in 0..4 {
                    r[6 + t] = rgba[t] as f32;
                }
            }
        }
        let rm = imp(py, "main.client.render")?;
        let m = rm.call_method1("DynamicMesh", (parent, format!("scenery_edges_{}", scene.getattr("id")?.str()?)))?;
        let n = m.getattr("np")?;
        n.call_method1("setTransparency", (true,))?;
        n.call_method1("setDepthOffset", (1,))?;
        m.call_method1("set", (f32_2(py, v, 12)?, rm.call_method1("quad_tris", (quads.len(),))?))?;
        Ok(n)
    }
    /// 色を入れる。光る部品は（static なら いつも、lights なら on のとき）光の色で影なし、ほかは影つき
    fn _colors(slf: &Bound<'_, Self>, name: &str, on: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let it = s.getattr("meshes")?.call_method1("get", (name,))?;
        if it.is_none() {
            return Ok(());
        }
        let parts = s.getattr("s")?.getattr("parts")?;
        let nf: usize = it.get_item("nf")?.extract()?;
        let per = nf * 4;
        let stat = name.starts_with("static");
        let on = on.is_truthy()?;
        let bright = f64of(&it.get_item("bright")?)?;
        let rm = imp(py, "main.client.render")?;
        let sun = v3(&rm.getattr("SUN")?)?;
        let amb = f64of(&rm.getattr("AMBIENT")?)?;
        let va = it.get_item("v")?;
        let np = imp(py, "numpy")?;
        let mut v: Vec<f32> = {
            let r: numpy::PyReadonlyArray2<f32> = np.call_method1("ascontiguousarray", (&va,))?.extract()?;
            r.as_slice()?.to_vec()
        };
        let idx: Vec<i64> = it.get_item("idx")?.extract()?;
        for (bi, &k) in idx.iter().enumerate() {
            let p = parts.get_item(k)?;
            let glow = p.get_item("glow")?;
            let glowing = !glow.is_none() && (stat || on);
            let base = if glowing { v3(&glow)? } else { v3(&p.get_item("color")?)? };
            for q in 0..per {
                let row = (bi * per + q) * 12;
                let mut col = base;
                if !glowing {
                    let n = [v[row + 3] as f64, v[row + 4] as f64, v[row + 5] as f64];
                    let lam = dot(n, sun).clamp(0.0, 1.0);
                    let kk = amb + (1.0 - amb) * lam;
                    col = std::array::from_fn(|t| base[t] * kk * bright);
                }
                for t in 0..3 {
                    v[row + 6 + t] = col[t].clamp(0.0, 1.0) as f32;
                }
            }
        }
        let vo = f32_2(py, v, 12)?;
        it.set_item("v", &vo)?;
        let mesh = it.get_item("mesh")?;
        if it.get_item("new")?.is_truthy()? {
            mesh.call_method1("set", (&vo, it.get_item("tris")?))?;
            it.set_item("new", false)?;
        } else {
            mesh.call_method1("set", (&vo,))?;
        }
        Ok(())
    }
    fn update(slf: &Bound<'_, Self>, origin: &B<'_>, lights_on: &B<'_>, t: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let c = v3(&s.getattr("s")?.getattr("center")?)?;
        let o = v3(origin)?;
        let pos = [c[0] - o[0], c[1] - o[1], c[2] - o[2]];
        let meshes = s.getattr("meshes")?;
        for it in meshes.call_method0("values")?.iter()? {
            it?.get_item("mesh")?.getattr("np")?.call_method1("setPos", (pos[0], pos[1], pos[2]))?;
        }
        let edges = s.getattr("edges").unwrap_or_else(|_| none(py));
        if !edges.is_none() {
            edges.call_method1("setPos", (pos[0], pos[1], pos[2]))?;
        }
        let state = s.getattr("state")?;
        if !state.call_method1("get", ("lights",))?.eq(lights_on)? {
            state.set_item("lights", lights_on)?;
            s.call_method1("_colors", ("lights", lights_on))?;
        }
        let it = meshes.call_method1("get", ("blink",))?;
        if !it.is_none() && t - f64of(&state.call_method1("get", ("blink_t", -1))?)? > 0.08 {
            state.set_item("blink_t", t)?;
            let parts = s.getattr("s")?.getattr("parts")?;
            let nf: usize = it.get_item("nf")?.extract()?;
            let idx: Vec<i64> = it.get_item("idx")?.extract()?;
            let np = imp(py, "numpy")?;
            let mut v: Vec<f32> = {
                let r: numpy::PyReadonlyArray2<f32> = np.call_method1("ascontiguousarray", (it.get_item("v")?,))?.extract()?;
                r.as_slice()?.to_vec()
            };
            let per = nf * 4;
            for (bi, &k) in idx.iter().enumerate() {
                let p = parts.get_item(k)?;
                let a = 0.5 + 0.5 * (t * 2.0 * std::f64::consts::PI / f64of(&p.get_item("blink")?)? + k as f64 * 0.7).sin();
                let col = v3(&p.get_item("color")?)?;
                let glow = v3(&p.get_item("glow")?)?;
                let cc: [f64; 3] = std::array::from_fn(|j| col[j] + (glow[j] - col[j]) * (0.25 + 0.75 * a));
                for q in 0..per {
                    let row = (bi * per + q) * 12;
                    for j in 0..3 {
                        v[row + 6 + j] = cc[j].clamp(0.0, 1.0) as f32;
                    }
                }
            }
            let vo = f32_2(py, v, 12)?;
            it.set_item("v", &vo)?;
            let tr: PyObject = if it.get_item("new")?.is_truthy()? { it.get_item("tris")?.unbind() } else { py.None() };
            it.get_item("mesh")?.call_method1("set", (vo, tr))?;
            it.set_item("new", false)?;
        }
        Ok(())
    }
    fn remove(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        for it in s.getattr("meshes")?.call_method0("values")?.iter()? {
            it?.get_item("mesh")?.call_method0("remove")?;
        }
        let e = s.getattr("edges").unwrap_or_else(|_| none(py));
        if !e.is_none() {
            e.call_method0("removeNode")?;
            s.setattr("edges", py.None())?;
        }
        let _ = PyList::empty_bound(py);
        Ok(())
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_function(wrap_pyfunction_bound!(scenery_box_quads, m)?)?;
    m.add_function(wrap_pyfunction_bound!(scenery_tex_kind, m)?)?;
    m.add_class::<SceneView>()?;
    Ok(())
}
