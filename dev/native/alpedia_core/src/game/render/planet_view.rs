//! 描画：星（PlanetView）と地表の区画（TerrainPatch）

use super::*;

// ---------------------------------------------------------------- PlanetView
/// 遠景用の粗い球（キューブスフィア）
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct PlanetView {}

#[pymethods]
impl PlanetView {
    /// 詳細な地表の下に沈める深さ (m)
    #[classattr]
    #[allow(non_snake_case)]
    pub(crate) fn SINK() -> f64 {
        60.0
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        PlanetView {}
    }
    #[pyo3(signature = (parent, loader, planet, n=48))]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, loader: &B<'_>, planet: &B<'_>, n: usize) -> R<()> {
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
    pub(crate) fn _sunk<'py>(slf: &Bound<'py, Self>, frame: &B<'py>) -> R<B<'py>> {
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
    pub(crate) fn update(slf: &Bound<'_, Self>, origin: &B<'_>, player_pos: Option<&B<'_>>, patch_frame: Option<&B<'_>>) -> R<()> {
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
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        TerrainPatch {}
    }
    #[pyo3(signature = (parent, loader, cells=48, step=1.0))]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, loader: &B<'_>, cells: i64, step: f64) -> R<()> {
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
    pub(crate) fn frame_for<'py>(slf: &Bound<'py, Self>, planet: &B<'py>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        if !s.getattr("planet")?.is(planet) || !gd(s, "shown", tb(py, false)).is_truthy()? || s.getattr("anchor")?.is_none() {
            return Ok(none(py));
        }
        s.getattr("frame")
    }
    #[pyo3(signature = (planet, player_pos, origin, carve=None))]
    pub(crate) fn update(slf: &Bound<'_, Self>, planet: &B<'_>, player_pos: &B<'_>, origin: &B<'_>, carve: Option<&B<'_>>) -> R<()> {
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
    pub(crate) fn _build(slf: &Bound<'_, Self>, pl: &B<'_>, player_pos: &B<'_>, carve: Option<&B<'_>>) -> R<()> {
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
            // 掘った穴に満ちた水：穴の上の格子点（少し外まで）は、その水面の高さにする（外側は地面の下で見えない）
            let mut wr = wr.clone();
            let mut wet: Vec<bool> = (0..nv).map(|i| wr[i] > r_nat[i]).collect();
            let fl: Vec<[f64; 5]> = match pl.getattr("floods_now") { Ok(v) => v.extract().unwrap_or_default(), Err(_) => vec![] };
            for f in &fl {
                let fc = [f[0], f[1], f[2]];
                let lc = rnorm(&fc).max(1e-9);
                let fd = [fc[0] / lc, fc[1] / lc, fc[2] / lc];
                let cosr = ((f[3] + step * 1.5) / lc).min(1.0).asin().cos();
                for i in 0..nv {
                    let dd = d[i * 3] * fd[0] + d[i * 3 + 1] * fd[1] + d[i * 3 + 2] * fd[2];
                    if dd >= cosr {
                        wr[i] = wr[i].max(f[4]);
                        wet[i] = true;
                    }
                }
            }
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
