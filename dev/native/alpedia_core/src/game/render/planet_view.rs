//! 描画：星（PlanetView）と地表の区画（TerrainPatch）

use crate::game::planet::{pla, Planet};
use super::*;

// ---------------------------------------------------------------- PlanetView
/// 遠景用の粗い球（キューブスフィア）
#[pyclass(module = "main.client.render", subclass, dict)]
#[allow(non_snake_case)]
pub struct PlanetView {
    /// CUT（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_CUT: Option<PyObject>,
    /// base（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_base: Option<PyObject>,
    /// cut（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_cut: Option<PyObject>,
    /// dirs（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_dirs: Option<PyObject>,
    /// frame_key（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_frame_key: Option<PyObject>,
    /// mesh（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_mesh: Option<PyObject>,
    /// p（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_p: Option<PyObject>,
    /// pos（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_pos: Option<PyObject>,
    /// root（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_root: Option<PyObject>,
    /// tris（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_tris: Option<PyObject>,
    /// verts（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_verts: Option<PyObject>,
    /// 距離で細かさを変える遠景（A-01）
    pub lod: Option<Box<super::planet_lod::Lod>>,
    /// 遠景の内側（詳細な地表のまわり。planet_lod.rs）のメッシュ
    pub lod_inner: Option<PyObject>,
    /// 草・木（A-02。定義に "vegetation": true の惑星だけ）：(草のメッシュ, 木のメッシュ)
    pub veg: Option<Box<super::vegetation::Veg>>,
    pub veg_mesh: Option<(PyObject, PyObject)>,
}

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
        PlanetView { a_CUT: None, a_base: None, a_cut: None, a_dirs: None, a_frame_key: None, a_mesh: None, a_p: None, a_pos: None, a_root: None, a_tris: None, a_verts: None, lod: None, lod_inner: None, veg: None, veg_mesh: None }
    }
    #[pyo3(signature = (parent, loader, planet, n=48))]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, loader: &B<'_>, planet: &B<'_>, n: usize) -> R<()> {
        let _ = (loader, n);
        let py = slf.py();
        Self::sa_p(slf, planet)?;
        let root = parent.call_method1("attachNewNode", (format!("planet_{}", planet.getattr("id")?.str()?),))?;
        Self::sa_root(slf, &root)?;
        let mesh = dmesh(py, &root, "far")?;
        Self::sa_mesh(slf, &mesh)?;
        slf.borrow_mut().lod_inner = Some(dmesh(py, &root, "far_in")?.unbind());
        Self::sa_frame_key(slf, py.None())?;
        let radius = f64of(&planet.getattr("radius")?)?;
        slf.borrow_mut().lod = Some(Box::new(super::planet_lod::Lod::new(radius)));
        // 草・木
        if get(&planet.getattr("defn")?, "vegetation", false.into_py(py))?.is_truthy()? {
            let path = imp(py, "main.paths")?.getattr("TEXTURES")?.call_method1("__truediv__", ("plants.png",))?.str()?;
            let tex = rmod(py)?.call_method1("load_texture", (py.None(), path))?;
            let mk = |name: &str| -> R<PyObject> {
                let m = dmesh(py, &root, name)?;
                let n = m.getattr("np")?;
                n.call_method1("setTwoSided", (true,))?;
                if tex.is_truthy()? {
                    n.call_method1("setTexture", (&tex, 1))?;
                }
                if let Ok(mid) = m.getattr("mid") {
                    let _ = crate::game::gfx::view(py)?.call_method1("mesh_cutoff", (mid, 0.5));
                }
                Ok(m.unbind())
            };
            let (g, t) = (mk("grass")?, mk("trees")?);
            // 風で揺らす（草はよく、木は梢だけ少し）。前の画面（alpedia_view）では無いので、無ければ何もしない
            for (m, k) in [(&g, 0.12f32), (&t, 0.35f32)] {
                if let Ok(mid) = m.bind(py).getattr("mid") {
                    let _ = crate::game::gfx::view(py)?.call_method1("mesh_wind", (mid, k));
                }
            }
            let mut b = slf.borrow_mut();
            b.veg = Some(Box::default());
            b.veg_mesh = Some((g, t));
        }
        // 滝（A-07）：名所の崖・湖の段。惑星ローカルの決まった形なので 1 回だけ作る
        if let Ok(nat) = planet.call_method0("_nat") {
            if let Ok(ps) = nat.downcast::<crate::PlanetShape>() {
                let shape = ps.get().shape.clone();
                let falls = py.allow_threads(|| shape.waterfalls().clone());
                if !falls.is_empty() {
                    let ((v, t), (mv, mt)) = super::waterfall::build(&falls, sun(py)?, ambient(py)?);
                    let path = imp(py, "main.paths")?.getattr("TEXTURES")?.call_method1("__truediv__", ("waterfall.png",))?.str()?;
                    let tex = rmod(py)?.call_method1("load_texture", (py.None(), path))?;
                    for (name, v, t, flow) in [("waterfall", v, t, 0.55f32), ("waterfall_mist", mv, mt, 0.0f32)] {
                        let m = dmesh(py, &root, name)?;
                        let n = m.getattr("np")?;
                        n.call_method1("setTwoSided", (true,))?;
                        n.call_method1("setTransparency", (true,))?;
                        n.call_method1("setDepthWrite", (false,))?;
                        if tex.is_truthy()? {
                            n.call_method1("setTexture", (&tex, 1))?;
                        }
                        m.call_method1("set", (f32_2(py, v, 12)?, i64_2(py, t, 3)?))?;
                        // 水の絵を下へ流す（前の画面では無いので、無ければそのまま）
                        if flow > 0.0 {
                            if let Ok(mid) = m.getattr("mid") {
                                let _ = crate::game::gfx::view(py)?.call_method1("mesh_flow", (mid, flow));
                            }
                        }
                    }
                }
            }
        }
        // 最初は遠くから見た形（いちばん粗い区画）
        let far = [0.0, 0.0, radius * 1e3];
        Self::_lod_update(slf, far, None)?;
        Ok(())
    }
    /// 描画の重さの内訳 [(名前, 秒)] を取り出して 0 に戻す（開発用）
    #[staticmethod]
    pub(crate) fn prof_take() -> Vec<(String, f64)> {
        let mut p = super::RPROF.lock().unwrap_or_else(|e| e.into_inner());
        let out = super::RPROF_PARTS.iter().zip(p.iter()).map(|(k, v)| (k.to_string(), *v)).collect();
        *p = [0.0; 10];
        out
    }
    /// patch_frame：この惑星の詳細な地表の範囲 (上, 接線1, 接線2, 半径, 半幅) か None
    #[pyo3(signature = (origin, player_pos=None, patch_frame=None))]
    pub(crate) fn update(slf: &Bound<'_, Self>, origin: &B<'_>, player_pos: Option<&B<'_>>, patch_frame: Option<&B<'_>>) -> R<()> {
        let p = Self::ga_p(slf)?;
        let ctr = v3(&p.getattr("center")?)?;
        let o = v3(origin)?;
        Self::ga_root(slf)?.call_method1("setPos", (ctr[0] - o[0], ctr[1] - o[1], ctr[2] - o[2]))?;
        let cam = match player_pos.filter(|x| !x.is_none()) { Some(pp) => v3(pp)?, None => o };
        let cam = [cam[0] - ctr[0], cam[1] - ctr[1], cam[2] - ctr[2]];
        let frame = match patch_frame.filter(|x| !x.is_none()) {
            None => None,
            Some(f) => Some((v3(&f.get_item(0)?)?, v3(&f.get_item(1)?)?, v3(&f.get_item(2)?)?, f64of(&f.get_item(3)?)?, f64of(&f.get_item(4)?)?)),
        };
        Self::_lod_update(slf, cam, frame)?;
        Self::_veg_update(slf, cam)
    }
}

impl PlanetView {
    fn _veg_update(slf: &Bound<'_, Self>, cam: [f64; 3]) -> R<()> {
        let py = slf.py();
        let p = Self::ga_p(slf)?;
        let mut veg = match slf.borrow_mut().veg.take() { Some(v) => v, None => return Ok(()) };
        let res = veg.update(&p, cam);
        slf.borrow_mut().veg = Some(veg);
        let (g, t) = res?;
        let meshes = slf.borrow().veg_mesh.as_ref().map(|(a, b)| (a.clone_ref(py), b.clone_ref(py)));
        let mut pt = std::time::Instant::now();
        if let Some((gm, tm)) = meshes {
            for (data, m) in [(g, gm), (t, tm)] {
                if let Some((v, tr)) = data {
                    m.bind(py).call_method1("set", (f32_2(py, v, 12)?, i64_2(py, tr, 3)?))?;
                }
            }
        }
        super::rprof(5, &mut pt);
        Ok(())
    }
    fn _lod_update(slf: &Bound<'_, Self>, cam: [f64; 3], frame: Option<([f64; 3], [f64; 3], [f64; 3], f64, f64)>) -> R<()> {
        let py = slf.py();
        let p = Self::ga_p(slf)?;
        let mut lod = match slf.borrow_mut().lod.take() { Some(l) => l, None => return Ok(()) };
        let res = lod.update(&p, cam, frame);
        slf.borrow_mut().lod = Some(lod);
        let (outer, inner) = res?;
        let mut pt = std::time::Instant::now();
        if let Some((v, t)) = outer {
            Self::ga_mesh(slf)?.call_method1("set", (f32_2(py, v, 12)?, i64_2(py, t, 3)?))?;
        }
        if let Some((v, t)) = inner {
            let m = slf.borrow().lod_inner.as_ref().map(|m| m.clone_ref(py));
            if let Some(m) = m {
                m.bind(py).call_method1("set", (f32_2(py, v, 12)?, i64_2(py, t, 3)?))?;
            }
        }
        super::rprof(2, &mut pt);
        Ok(())
    }
}

// ---------------------------------------------------------------- TerrainPatch
/// プレイヤー周辺の詳細な地表（掘削跡も反映）と水面
#[pyclass(module = "main.client.render", subclass, dict)]
pub struct TerrainPatch {
    /// anchor（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_anchor: Option<PyObject>,
    /// carve_version（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_carve_version: Option<PyObject>,
    /// cells（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_cells: Option<PyObject>,
    /// center（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_center: Option<PyObject>,
    /// frame（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_frame: Option<PyObject>,
    /// loader（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_loader: Option<PyObject>,
    /// mesh（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_mesh: Option<PyObject>,
    /// planet（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_planet: Option<PyObject>,
    /// root（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_root: Option<PyObject>,
    /// shown（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_shown: Option<PyObject>,
    /// step（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_step: Option<PyObject>,
    /// version（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_version: Option<PyObject>,
    /// water（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_water: Option<PyObject>,
}

#[pymethods]
impl TerrainPatch {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    pub(crate) fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        TerrainPatch { a_anchor: None, a_carve_version: None, a_cells: None, a_center: None, a_frame: None, a_loader: None, a_mesh: None, a_planet: None, a_root: None, a_shown: None, a_step: None, a_version: None, a_water: None }
    }
    #[pyo3(signature = (parent, loader, cells=48, step=1.0))]
    pub(crate) fn __init__(slf: &Bound<'_, Self>, parent: &B<'_>, loader: &B<'_>, cells: i64, step: f64) -> R<()> {
        let py = slf.py();
        let root = parent.call_method1("attachNewNode", ("terrain_patch",))?;
        Self::sa_root(slf, &root)?;
        Self::sa_mesh(slf, dmesh(py, &root, "patch")?)?;
        let w = dmesh(py, &root, "water")?;
        let n = w.getattr("np")?;
        n.call_method1("setTransparency", (true,))?;
        n.call_method1("setDepthWrite", (false,))?;
        n.call_method1("setBin", ("transparent", 10))?;
        Self::sa_water(slf, w)?;
        Self::sa_loader(slf, loader)?;
        Self::sa_cells(slf, cells)?;
        Self::sa_step(slf, step)?;
        Self::sa_anchor(slf, py.None())?;
        Self::sa_planet(slf, py.None())?;
        Self::sa_version(slf, -1)?;
        Self::sa_carve_version(slf, -1)?;
        Self::sa_center(slf, np(py)?.call_method1("zeros", (3,))?)?;
        Ok(())
    }
    /// この惑星の上に詳細な地表を出しているなら (上, 接線1, 接線2, 半径, 半幅)（遠景の球を沈める範囲）
    pub(crate) fn frame_for<'py>(slf: &Bound<'py, Self>, planet: &B<'py>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        if !Self::ga_planet(slf)?.is(planet) || !gd(s, "shown", tb(py, false)).is_truthy()? || Self::ga_anchor(slf)?.is_none() {
            return Ok(none(py));
        }
        Self::ga_frame(slf)
    }
    #[pyo3(signature = (planet, player_pos, origin, carve=None))]
    pub(crate) fn update(slf: &Bound<'_, Self>, planet: &B<'_>, player_pos: &B<'_>, origin: &B<'_>, carve: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let root = Self::ga_root(slf)?;
        let pp = v3(player_pos)?;
        if planet.is_none() || {
            let c = v3(&planet.getattr("center")?)?;
            npnorm(py, [pp[0] - c[0], pp[1] - c[1], pp[2] - c[2]])? - f64of(&planet.getattr("radius")?)? > 400.0
        } {
            root.call_method0("hide")?;
            Self::sa_shown(slf, false)?;
            return Ok(());
        }
        Self::sa_shown(slf, true)?;
        root.call_method0("show")?;
        let carve = carve.filter(|c| !c.is_none());
        let cv = match carve { Some(c) => c.getattr("version")?, None => tb(py, -1) };
        let anchor = Self::ga_anchor(slf)?;
        let need = !Self::ga_planet(slf)?.is(planet) || anchor.is_none() || !Self::ga_version(slf)?.eq(planet.getattr("version")?)?
            || !cv.eq(Self::ga_carve_version(slf)?)? || {
                // anchor・center は惑星ローカル（惑星が公転で動いても描き直さない。L-08）
                let a = v3(&anchor)?;
                let pc = v3(&planet.getattr("center")?)?;
                npnorm(py, [pp[0] - pc[0] - a[0], pp[1] - pc[1] - a[1], pp[2] - pc[2] - a[2]])? > f64of(&Self::ga_step(slf)?)? * f64of(&Self::ga_cells(slf)?)? * 0.25
            };
        if need {
            let mut pt = std::time::Instant::now();
            s.call_method1("_build", (planet, player_pos, carve.cloned().unwrap_or_else(|| none(py))))?;
            super::rprof(6, &mut pt);
            if let Ok(mut p) = super::RPROF.lock() {
                p[7] += 1.0;
            }
        }
        let cl = v3(&Self::ga_center(slf)?)?;
        let pc = v3(&planet.getattr("center")?)?;
        let c = [cl[0] + pc[0], cl[1] + pc[1], cl[2] + pc[2]];
        let o = v3(origin)?;
        root.call_method1("setPos", (c[0] - o[0], c[1] - o[1], c[2] - o[2]))?;
        Ok(())
    }
    #[pyo3(signature = (pl, player_pos, carve=None))]
    pub(crate) fn _build(slf: &Bound<'_, Self>, pl: &B<'_>, player_pos: &B<'_>, carve: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let npm = np(py)?;
        let carve = carve.filter(|c| !c.is_none());
        Self::sa_planet(slf, pl)?;
        Self::sa_version(slf, pl.getattr("version")?)?;
        Self::sa_carve_version(slf, match carve { Some(c) => c.getattr("version")?, None => tb(py, -1) })?;
        let use_carve = match carve { Some(c) => c.getattr("enabled")?.is_truthy()?, None => false };
        let up_o = Planet::up(pla(&pl)?, &IntoPy::<PyObject>::into_py(player_pos, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?;
        let up = v3(&up_o)?;
        let r0 = f64of(&Planet::surface_radius(pla(&pl)?, &IntoPy::<PyObject>::into_py(up_o.get_item(py.None())?, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?.get_item(0)?)?;
        // 格子をスナップしてちらつきを防ぐ
        let rf = if up[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let mut t1 = cross(up, rf);
        let n1 = npnorm(py, t1)?;
        t1 = [t1[0] / n1, t1[1] / n1, t1[2] / n1];
        let t2 = cross(up, t1);
        let n: i64 = Self::ga_cells(slf)?.extract()?;
        let step = f64of(&Self::ga_step(slf)?)?;
        Self::sa_frame(slf, (np3(py, up), np3(py, t1), np3(py, t2), r0, n as f64 * step))?;
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
        let mut pt = std::time::Instant::now();
        // 高さは地形から直接（当たり判定用の細かいタイル（0.25 m）を作らない。値はタイルと同じ。高速移動で止まらないように）
        let sm = a2(&pl.call_method1("_tile_values", (&d_o,))?, 2)?;
        super::rprof(8, &mut pt);
        let mut r: Vec<f64> = (0..nv).map(|i| sm.d[i * 2]).collect();
        let wr: Vec<f64> = (0..nv).map(|i| sm.d[i * 2 + 1]).collect();
        let r_nat = r.clone();
        let center = v3(&pl.getattr("center")?)?;
        let craters = pl.getattr("craters")?;
        let has_craters = craters.is_truthy()?;
        if has_craters && !use_carve {
            let pts: Vec<f64> = (0..nv * 3).map(|j| center[j % 3] + d[j] * r[j / 3]).collect();
            let sv = a1(&Planet::sdf(pla(&pl)?, &IntoPy::<PyObject>::into_py(f64_2(py, pts, 3)?, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?)?;
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
                    let s2 = a1(&Planet::sdf(pla(&pl)?, &IntoPy::<PyObject>::into_py(f64_2(py, pts, 3)?, pl.py()).into_bound(pl.py())).map(|v| IntoPy::<PyObject>::into_py(v, pl.py()).into_bound(pl.py()))?)?;
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
        let ppv = v3(player_pos)?;
        Self::sa_anchor(slf, np3(py, [ppv[0] - center[0], ppv[1] - center[1], ppv[2] - center[2]]))?;
        let cen = [center[0] + up[0] * r0, center[1] + up[1] * r0, center[2] + up[2] * r0];
        Self::sa_center(slf, np3(py, [cen[0] - center[0], cen[1] - center[1], cen[2] - center[2]]))?;
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
        let mut pt = std::time::Instant::now();
        let mut col = a2(&pl.call_method1("surface_color", (&d_o, f64_1(py, r_nat.clone()), &nrm_o,))?, 3)?;
        super::rprof(9, &mut pt);
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
        let mesh = Self::ga_mesh(slf)?;
        mesh.call_method1("set", (f32_2(py, v, 12)?, i64_2(py, tris, 3)?))?;
        let rm = rmod(py)?;
        let tex = rm.call_method1("load_texture", (Self::ga_loader(slf)?, get(&pl.getattr("defn")?, "texture", py.None())?))?;
        if tex.is_truthy()? {
            mesh.getattr("np")?.call_method1("setTexture", (tex, 1))?;
        }
        // 水面
        let water = Self::ga_water(slf)?;
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

// ---------------------------------------------------------------- TerrainPatch の欄の読み書き（C-01。前は __dict__）
// 入れていない欄を読むと、前と同じく AttributeError（getattr(x, 名前, 既定値) も前と同じに動く）
macro_rules! terrainpatch_attr_fields {
    ($($g:ident $s:ident $pg:ident $ps:ident $f:ident $name:literal),* $(,)?) => {
        #[allow(dead_code, non_snake_case)]
        impl TerrainPatch { $(
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
        impl TerrainPatch { $(
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
terrainpatch_attr_fields!(
    ga_anchor sa_anchor pga_anchor psa_anchor a_anchor "anchor", ga_carve_version sa_carve_version pga_carve_version psa_carve_version a_carve_version "carve_version", ga_cells sa_cells pga_cells psa_cells a_cells "cells", ga_center sa_center pga_center psa_center a_center "center", ga_frame sa_frame pga_frame psa_frame a_frame "frame", ga_loader sa_loader pga_loader psa_loader a_loader "loader", ga_mesh sa_mesh pga_mesh psa_mesh a_mesh "mesh", ga_planet sa_planet pga_planet psa_planet a_planet "planet", ga_root sa_root pga_root psa_root a_root "root", ga_shown sa_shown pga_shown psa_shown a_shown "shown", ga_step sa_step pga_step psa_step a_step "step", ga_version sa_version pga_version psa_version a_version "version", ga_water sa_water pga_water psa_water a_water "water"
);

// ---------------------------------------------------------------- PlanetView の欄の読み書き（C-01。前は __dict__）
// 入れていない欄を読むと、前と同じく AttributeError（getattr(x, 名前, 既定値) も前と同じに動く）
macro_rules! planetview_attr_fields {
    ($($g:ident $s:ident $pg:ident $ps:ident $f:ident $name:literal),* $(,)?) => {
        #[allow(dead_code, non_snake_case)]
        impl PlanetView { $(
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
        impl PlanetView { $(
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
planetview_attr_fields!(
    ga_CUT sa_CUT pga_CUT psa_CUT a_CUT "CUT", ga_base sa_base pga_base psa_base a_base "base", ga_cut sa_cut pga_cut psa_cut a_cut "cut", ga_dirs sa_dirs pga_dirs psa_dirs a_dirs "dirs", ga_frame_key sa_frame_key pga_frame_key psa_frame_key a_frame_key "frame_key", ga_mesh sa_mesh pga_mesh psa_mesh a_mesh "mesh", ga_p sa_p pga_p psa_p a_p "p", ga_pos sa_pos pga_pos psa_pos a_pos "pos", ga_root sa_root pga_root psa_root a_root "root", ga_tris sa_tris pga_tris psa_tris a_tris "tris", ga_verts sa_verts pga_verts psa_verts a_verts "verts"
);
