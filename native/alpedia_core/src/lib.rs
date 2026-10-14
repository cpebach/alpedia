//! alpedia_core：Alpedia の物理コア（Rust）。Python からは `import alpedia_core` で使う。
//!
//! - kernels：構築物の形合わせ・結合・当たり判定の芯・描画用の頂点と法線・向き付きの箱
//! - art：完全剛体モードの関節つき剛体
//! - planet：惑星の地形（距離場・タイルのキャッシュ・洞窟・池）
//!
//! 配列は numpy の C 順（行優先）で連続したものを受け取る。重い計算の間は Python の GIL を離すので、
//! Python 側のスレッドから同時に呼べる（構築物ごとの並列計算）。

mod art;
mod kernels;
mod planet;

use std::collections::HashMap;
use std::sync::RwLock;

use numpy::{PyArray1, PyArray2, PyArrayMethods, PyReadonlyArrayDyn, PyReadwriteArrayDyn, PyUntypedArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use rayon::prelude::*;

type RoF<'py> = PyReadonlyArrayDyn<'py, f64>;
type RoI<'py> = PyReadonlyArrayDyn<'py, i64>;
type RwF<'py> = PyReadwriteArrayDyn<'py, f64>;

fn ro<'a, T: numpy::Element>(a: &'a PyReadonlyArrayDyn<'_, T>, name: &str) -> PyResult<&'a [T]> {
    a.as_slice().map_err(|_| PyValueError::new_err(format!("{name}: 連続した配列ではありません")))
}

fn rw<'a>(a: &'a mut PyReadwriteArrayDyn<'_, f64>, name: &str) -> PyResult<&'a mut [f64]> {
    a.as_slice_mut().map_err(|_| PyValueError::new_err(format!("{name}: 連続した書き込める配列ではありません")))
}

fn n_rows(a: &PyReadonlyArrayDyn<'_, f64>) -> usize {
    a.shape().first().copied().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// 構築物
// ---------------------------------------------------------------------------
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn shape_match(
    py: Python<'_>, mut pos: RwF, rest: RoF, act_scale: RoF, stiff: RoF, vox: RoI, mut last_r: RwF, mut quat: RwF,
    parallel: bool,
) -> PyResult<()> {
    let (p, r, s, k, v) = (rw(&mut pos, "pos")?, ro(&rest, "rest")?, ro(&act_scale, "act_scale")?, ro(&stiff, "stiff")?, ro(&vox, "vox")?);
    let (lr, q) = (rw(&mut last_r, "lastR")?, rw(&mut quat, "quat")?);
    let nvox = k.len();
    if v.iter().any(|&i| i < 0 || i as usize >= nvox) || p.len() < 24 * nvox || lr.len() < 9 * nvox || q.len() < 4 * nvox {
        return Err(PyValueError::new_err("shape_match: 配列の大きさが合いません"));
    }
    py.allow_threads(|| kernels::shape_match(p, r, s, k, v, lr, q, parallel));
    Ok(())
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn solve_joints(
    py: Python<'_>, mut pos: RwF, ja: RoI, jb: RoI, jr: RoF, jk: RoF, inv_mass: RoF, jcount: RoF,
) -> PyResult<()> {
    let p = rw(&mut pos, "pos")?;
    let (a, b, r, k, m, c) = (ro(&ja, "ja")?, ro(&jb, "jb")?, ro(&jr, "jr")?, ro(&jk, "jk")?, ro(&inv_mass, "inv_mass")?, ro(&jcount, "jcount")?);
    let nv = p.len() / 3;
    if a.iter().chain(b.iter()).any(|&i| i < 0 || i as usize >= nv) || m.len() < nv || c.len() < nv {
        return Err(PyValueError::new_err("solve_joints: 配列の大きさが合いません"));
    }
    py.allow_threads(|| kernels::solve_joints(p, a, b, r, k, m, c));
    Ok(())
}

#[pyfunction]
fn contact_cores<'py>(py: Python<'py>, pos: RoF<'py>, ctype: RoI<'py>, cidx: RoI<'py>) -> PyResult<Bound<'py, PyArray2<f64>>> {
    let (p, t, c) = (ro(&pos, "pos")?, ro(&ctype, "ctype")?, ro(&cidx, "cidx")?);
    let nv = p.len() / 3;
    let n = nv / 8;
    if t.len() < nv || c.len() < 3 * nv || c.iter().any(|&i| i < 0 || i as usize >= n.max(1)) {
        return Err(PyValueError::new_err("contact_cores: 配列の大きさが合いません"));
    }
    let mut out = vec![0.0; nv * 3];
    py.allow_threads(|| kernels::contact_cores(p, t, c, &mut out));
    Ok(PyArray1::from_vec_bound(py, out).reshape([nv, 3])?)
}

#[pyfunction]
fn skin<'py>(py: Python<'py>, pos: RoF<'py>, vidx: RoI<'py>, w: RoF<'py>) -> PyResult<Bound<'py, PyArray2<f64>>> {
    let (p, v, ww) = (ro(&pos, "pos")?, ro(&vidx, "vidx")?, ro(&w, "w")?);
    let m = v.len() / 8;
    let nv = p.len() / 3;
    if ww.len() < 8 * m || v.iter().any(|&i| i < 0 || i as usize >= nv) {
        return Err(PyValueError::new_err("skin: 配列の大きさが合いません"));
    }
    let mut out = vec![0.0; m * 3];
    py.allow_threads(|| kernels::skin(p, v, ww, &mut out));
    Ok(PyArray1::from_vec_bound(py, out).reshape([m, 3])?)
}

#[pyfunction]
fn vertex_normals<'py>(py: Python<'py>, v: RoF<'py>, faces: RoI<'py>) -> PyResult<Bound<'py, PyArray2<f64>>> {
    let (vv, f) = (ro(&v, "v")?, ro(&faces, "faces")?);
    let n = vv.len() / 3;
    if f.iter().any(|&i| i < 0 || i as usize >= n) {
        return Err(PyValueError::new_err("vertex_normals: 面の番号が範囲外です"));
    }
    let mut out = vec![0.0; n * 3];
    py.allow_threads(|| kernels::vertex_normals(vv, f, &mut out));
    Ok(PyArray1::from_vec_bound(py, out).reshape([n, 3])?)
}

#[pyfunction]
#[allow(clippy::type_complexity)]
fn box_sdf<'py>(
    py: Python<'py>, p: RoF<'py>, c: RoF<'py>, a: RoF<'py>, e: RoF<'py>,
) -> PyResult<(Bound<'py, PyArray1<f64>>, Bound<'py, PyArray2<f64>>, Bound<'py, PyArray1<i64>>)> {
    let (pp, cc, aa, ee) = (ro(&p, "P")?, ro(&c, "C")?, ro(&a, "A")?, ro(&e, "E")?);
    let n = pp.len() / 3;
    let nb = cc.len() / 3;
    if nb == 0 || aa.len() < 9 * nb || ee.len() < 3 * nb {
        return Err(PyValueError::new_err("box_sdf: 箱の配列の大きさが合いません"));
    }
    let (mut d, mut nn, mut k) = (vec![0.0; n], vec![0.0; 3 * n], vec![0i64; n]);
    py.allow_threads(|| kernels::box_sdf(pp, cc, aa, ee, &mut d, &mut nn, &mut k));
    Ok((PyArray1::from_vec_bound(py, d), PyArray1::from_vec_bound(py, nn).reshape([n, 3])?, PyArray1::from_vec_bound(py, k)))
}

// ---------------------------------------------------------------------------
// 完全剛体モード
// ---------------------------------------------------------------------------
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn art_solve(
    py: Python<'_>, mut x: RwF, mut r: RwF, inv_m: RoF, iinv_b: RoF, ra: RoI, rb: RoI, la: RoF, lb: RoF, lc: RoF,
    ang: RoF, cap: RoF, cg: RoI, cl: RoF, cn: RoF, cd: RoF, cp: RoF, cprev: RoF, cvs: RoF, cmu: RoF, h: f64,
    iters: i64, mut lam_n: RwF,
) -> PyResult<()> {
    let (xx, rr) = (rw(&mut x, "X")?, rw(&mut r, "R")?);
    let ln = rw(&mut lam_n, "lamN")?;
    let (im, ib, a, b) = (ro(&inv_m, "invM")?, ro(&iinv_b, "IinvB")?, ro(&ra, "ra")?, ro(&rb, "rb")?);
    let (la_, lb_, lc_, an, cp_) = (ro(&la, "LA")?, ro(&lb, "LB")?, ro(&lc, "LC")?, ro(&ang, "ang")?, ro(&cap, "cap")?);
    let (g, l, n, d, p0) = (ro(&cg, "cg")?, ro(&cl, "cl")?, ro(&cn, "cn")?, ro(&cd, "cd")?, ro(&cp, "cp")?);
    let (pv, vs, mu) = (ro(&cprev, "cprev")?, ro(&cvs, "cvs")?, ro(&cmu, "cmu")?);
    let ng = im.len();
    let nr = a.len();
    let nc = g.len();
    let ok = xx.len() >= 3 * ng && rr.len() >= 9 * ng && ib.len() >= 9 * ng
        && a.iter().chain(b.iter()).chain(g.iter()).all(|&i| i >= 0 && (i as usize) < ng)
        && b.len() == nr && la_.len() >= 12 * nr && lb_.len() >= 12 * nr && lc_.len() >= 3 * nr && an.len() >= nr && cp_.len() >= nr
        && l.len() >= 3 * nc && n.len() >= 3 * nc && d.len() >= nc && p0.len() >= 3 * nc && pv.len() >= 3 * nc
        && vs.len() >= 3 * nc && mu.len() >= nc && ln.len() >= nc;
    if !ok {
        return Err(PyValueError::new_err("art_solve: 配列の大きさが合いません"));
    }
    py.allow_threads(|| art::art_solve(xx, rr, im, ib, a, b, la_, lb_, lc_, an, cp_, g, l, n, d, p0, pv, vs, mu, h, iters, ln));
    Ok(())
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn art_contact_vel(
    py: Python<'_>, mut v: RwF, mut w: RwF, r: RoF, inv_m: RoF, iinv_b: RoF, cg: RoI, cl: RoF, cn: RoF, lam_n: RoF,
    iters: i64,
) -> PyResult<()> {
    let (vv, ww) = (rw(&mut v, "V")?, rw(&mut w, "W")?);
    let (rr, im, ib, g, l, n, ln) = (ro(&r, "R")?, ro(&inv_m, "invM")?, ro(&iinv_b, "IinvB")?, ro(&cg, "cg")?, ro(&cl, "cl")?, ro(&cn, "cn")?, ro(&lam_n, "lamN")?);
    let ng = im.len();
    let nc = g.len();
    if vv.len() < 3 * ng || ww.len() < 3 * ng || rr.len() < 9 * ng || ib.len() < 9 * ng || g.iter().any(|&i| i < 0 || i as usize >= ng)
        || l.len() < 3 * nc || n.len() < 3 * nc || ln.len() < nc
    {
        return Err(PyValueError::new_err("art_contact_vel: 配列の大きさが合いません"));
    }
    py.allow_threads(|| art::art_contact_vel(vv, ww, rr, im, ib, g, l, n, ln, iters));
    Ok(())
}

#[pyfunction]
fn art_write(py: Python<'_>, mut pos: RwF, x: RoF, r: RoF, gv: RoI, vg: RoI, vl: RoF) -> PyResult<()> {
    let p = rw(&mut pos, "pos")?;
    let (xx, rr, a, b, l) = (ro(&x, "X")?, ro(&r, "R")?, ro(&gv, "gv")?, ro(&vg, "vg")?, ro(&vl, "vl")?);
    let (nv, ng) = (p.len() / 3, xx.len() / 3);
    if a.iter().any(|&i| i < 0 || i as usize >= nv) || b.iter().any(|&i| i < 0 || i as usize >= ng) || b.len() < a.len()
        || l.len() < 3 * a.len() || rr.len() < 9 * ng
    {
        return Err(PyValueError::new_err("art_write: 配列の大きさが合いません"));
    }
    py.allow_threads(|| art::art_write(p, xx, rr, a, b, l));
    Ok(())
}

// ---------------------------------------------------------------------------
// 重力（惑星ごとの和）
// ---------------------------------------------------------------------------
#[pyfunction]
fn gravity<'py>(py: Python<'py>, p: RoF<'py>, centers: RoF<'py>, gm: RoF<'py>, radius: RoF<'py>) -> PyResult<Bound<'py, PyArray2<f64>>> {
    let (pp, c, g, r) = (ro(&p, "p")?, ro(&centers, "centers")?, ro(&gm, "gm")?, ro(&radius, "radius")?);
    let n = pp.len() / 3;
    let np_ = g.len();
    if c.len() < 3 * np_ || r.len() < np_ {
        return Err(PyValueError::new_err("gravity: 惑星の配列の大きさが合いません"));
    }
    let mut out = vec![0.0; 3 * n];
    py.allow_threads(|| {
        for i in 0..n {
            for k in 0..np_ {
                let q = [c[3 * k] - pp[3 * i], c[3 * k + 1] - pp[3 * i + 1], c[3 * k + 2] - pp[3 * i + 2]];
                let r2 = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).max((r[k] * 0.5).powi(2));
                let s = g[k] / (r2 * r2.sqrt());
                for a in 0..3 {
                    out[3 * i + a] += q[a] * s;
                }
            }
        }
    });
    Ok(PyArray1::from_vec_bound(py, out).reshape([n, 3])?)
}

// ---------------------------------------------------------------------------
// 惑星の地形
// ---------------------------------------------------------------------------
#[pyclass(frozen)]
struct PlanetShape {
    shape: planet::Shape,
    center: [f64; 3],
    craters: RwLock<Vec<[f64; 4]>>,
}

fn rows3(a: &[f64]) -> Vec<[f64; 3]> {
    a.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect()
}

/// 点の数が多いときだけ並列にする
fn map_points<T: Send, F: Fn([f64; 3]) -> T + Sync>(pts: &[f64], f: F) -> Vec<T> {
    if pts.len() / 3 >= 512 {
        pts.par_chunks_exact(3).map(|c| f([c[0], c[1], c[2]])).collect()
    } else {
        pts.chunks_exact(3).map(|c| f([c[0], c[1], c[2]])).collect()
    }
}

#[pymethods]
impl PlanetShape {
    /// earth：地球型のときの値（dict）。sea, mountain_h, hill_h, flat_r, spawn_h, ponds_*, cave_*, cell, ck, cstart, cend, ccap
    #[new]
    #[pyo3(signature = (radius, amp, freq, seed, center, earth=None))]
    fn new(radius: f64, amp: f64, freq: f64, seed: i64, center: [f64; 3], earth: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let e = match earth {
            None => None,
            Some(d) => {
                let f = |k: &str| -> PyResult<f64> {
                    d.get_item(k)?.ok_or_else(|| PyValueError::new_err(format!("earth に {k} がありません")))?.extract()
                };
                let arr = |k: &str| -> PyResult<Vec<f64>> {
                    let v = d.get_item(k)?.ok_or_else(|| PyValueError::new_err(format!("earth に {k} がありません")))?;
                    let a: PyReadonlyArrayDyn<f64> = v.extract()?;
                    Ok(a.as_slice().map_err(|_| PyValueError::new_err(format!("{k}: 連続した配列ではありません")))?.to_vec())
                };
                let arri = |k: &str| -> PyResult<Vec<i64>> {
                    let v = d.get_item(k)?.ok_or_else(|| PyValueError::new_err(format!("earth に {k} がありません")))?;
                    let a: PyReadonlyArrayDyn<i64> = v.extract()?;
                    Ok(a.as_slice().map_err(|_| PyValueError::new_err(format!("{k}: 連続した配列ではありません")))?.to_vec())
                };
                let ck = arri("ck")?;
                let (cs, ce) = (arri("cstart")?, arri("cend")?);
                let mut index = HashMap::with_capacity(ck.len());
                for i in 0..ck.len() {
                    index.insert(ck[i], (cs[i] as usize, ce[i] as usize));
                }
                let ncap = arr("cave_ra")?.len();
                let cap: Vec<usize> = arri("ccap")?.into_iter().map(|x| x as usize).collect();
                if cap.iter().any(|&x| x >= ncap) {
                    return Err(PyValueError::new_err("洞窟の番号が範囲外です"));
                }
                Some(planet::Earth {
                    sea: f("sea")?,
                    mountain_h: f("mountain_h")?,
                    hill_h: f("hill_h")?,
                    flat_r: f("flat_r")?,
                    spawn_h: f("spawn_h")?,
                    ponds_dir: rows3(&arr("ponds_dir")?),
                    ponds_rad: arr("ponds_rad")?,
                    ponds_depth: arr("ponds_depth")?,
                    ponds_level: arr("ponds_level")?,
                    cave_a: rows3(&arr("cave_a")?),
                    cave_b: rows3(&arr("cave_b")?),
                    cave_ra: arr("cave_ra")?,
                    cave_rb: arr("cave_rb")?,
                    cell: f("cell")?,
                    cave_index: index,
                    cave_cap: cap,
                })
            }
        };
        Ok(PlanetShape { shape: planet::Shape::new(radius, amp, freq, seed, e), center, craters: RwLock::new(Vec::new()) })
    }

    /// 掘削跡（惑星ローカル座標の [x, y, z, 半径]）を入れ替える
    fn set_craters(&self, craters: RoF) -> PyResult<()> {
        let c = ro(&craters, "craters")?;
        *self.craters.write().unwrap() = c.chunks_exact(4).map(|v| [v[0], v[1], v[2], v[3]]).collect();
        Ok(())
    }

    /// 単位方向 (N,3) → タイルから引いた (地表半径, 水面半径) (N,2)
    fn sample<'py>(&self, py: Python<'py>, dirs: RoF<'py>) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let d = ro(&dirs, "dirs")?;
        let n = d.len() / 3;
        let out: Vec<f64> = py.allow_threads(|| map_points(d, |p| self.shape.sample(p)).into_iter().flatten().collect());
        Ok(PyArray1::from_vec_bound(py, out).reshape([n, 2])?)
    }

    fn surface_radius_exact<'py>(&self, py: Python<'py>, dirs: RoF<'py>) -> PyResult<Bound<'py, PyArray1<f64>>> {
        let d = rows3(ro(&dirs, "dirs")?);
        let out = py.allow_threads(|| self.shape.surface_radius_exact(&d));
        Ok(PyArray1::from_vec_bound(py, out))
    }

    /// 地球型：池を除いた高さ
    fn height0<'py>(&self, py: Python<'py>, dirs: RoF<'py>) -> PyResult<Bound<'py, PyArray1<f64>>> {
        let d = ro(&dirs, "dirs")?;
        let e = self.shape.earth.as_ref().ok_or_else(|| PyValueError::new_err("地球型ではありません"))?;
        let out = py.allow_threads(|| map_points(d, |p| self.shape.height0(e, p)));
        Ok(PyArray1::from_vec_bound(py, out))
    }

    /// 世界座標 (N,3) → 符号付き距離（負＝地中）
    fn sdf<'py>(&self, py: Python<'py>, p: RoF<'py>) -> PyResult<Bound<'py, PyArray1<f64>>> {
        let pp = ro(&p, "p")?;
        let cr = self.craters.read().unwrap().clone();
        let c = self.center;
        let out = py.allow_threads(|| map_points(pp, |q| self.shape.sdf([q[0] - c[0], q[1] - c[1], q[2] - c[2]], &cr)));
        Ok(PyArray1::from_vec_bound(py, out))
    }

    /// 世界座標 (N,3) → (距離 (N,), 勾配 (N,3))
    #[pyo3(signature = (p, eps=0.05))]
    fn sdf_grad<'py>(&self, py: Python<'py>, p: RoF<'py>, eps: f64) -> PyResult<(Bound<'py, PyArray1<f64>>, Bound<'py, PyArray2<f64>>)> {
        let pp = ro(&p, "p")?;
        let n = n_rows(&p);
        let cr = self.craters.read().unwrap().clone();
        let c = self.center;
        let res = py.allow_threads(|| map_points(pp, |q| self.shape.sdf_grad([q[0] - c[0], q[1] - c[1], q[2] - c[2]], &cr, eps)));
        let d: Vec<f64> = res.iter().map(|x| x.0).collect();
        let g: Vec<f64> = res.iter().flat_map(|x| x.1).collect();
        Ok((PyArray1::from_vec_bound(py, d), PyArray1::from_vec_bound(py, g).reshape([n, 3])?))
    }

    /// 構築物の頂点と地面の接触（construct._collide_planet と同じ）。
    /// cores：各頂点の芯の最近点（内部の頂点は NaN）、r：ボクセルの半分。up と rs があれば、地面から高い頂点は調べない。
    /// apply なら pos を押し戻して摩擦をかける。戻り値 (速くぶつかりそうか, 当たった頂点, めり込み, 法線)
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    #[pyo3(signature = (pos, prev, vel, cores, fric, r, bound, lipschitz, up, rs, rigid, impact_speed, soft, apply))]
    fn collide<'py>(
        &self, py: Python<'py>, mut pos: RwF<'py>, prev: RoF<'py>, vel: RoF<'py>, cores: RoF<'py>, fric: RoF<'py>, r: f64,
        bound: f64, lipschitz: f64, up: Option<[f64; 3]>, rs: f64, rigid: bool, impact_speed: f64, soft: f64, apply: bool,
    ) -> PyResult<(bool, Bound<'py, PyArray1<i64>>, Bound<'py, PyArray1<f64>>, Bound<'py, PyArray2<f64>>)> {
        let p = rw(&mut pos, "pos")?;
        let (pv, vl, k, fr) = (ro(&prev, "prev")?, ro(&vel, "vel")?, ro(&cores, "cores")?, ro(&fric, "fric")?);
        let nv = p.len() / 3;
        if pv.len() < 3 * nv || vl.len() < 3 * nv || k.len() < 3 * nv || fr.len() < nv {
            return Err(PyValueError::new_err("collide: 配列の大きさが合いません"));
        }
        let cr = self.craters.read().unwrap().clone();
        let c = self.center;
        let res = py.allow_threads(|| {
            let lim2 = (bound + r) * (bound + r);
            let lip = lipschitz.max(0.2);
            // 調べる頂点（地面に近いものだけ）
            let near: Vec<usize> = (0..nv)
                .filter(|&i| {
                    let rel = [k[3 * i] - c[0], k[3 * i + 1] - c[1], k[3 * i + 2] - c[2]];
                    let r2 = rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2];
                    if !(r2 < lim2) {
                        return false;          // NaN（内部の頂点）もここで外れる
                    }
                    match up {
                        None => true,
                        Some(u) => {
                            let rad = r2.sqrt();
                            let along = rel[0] * u[0] + rel[1] * u[1] + rel[2] * u[2];
                            let lat = (rad * rad - along * along).max(0.0).sqrt();
                            (rad - rs) < r + 0.5 + lat / lip
                        }
                    }
                })
                .collect();
            let eval = |&i: &usize| {
                let q = [k[3 * i] - c[0], k[3 * i + 1] - c[1], k[3 * i + 2] - c[2]];
                let (d, g) = self.shape.sdf_grad(q, &cr, 0.05);
                (i, d - r, g)
            };
            let hits: Vec<(usize, f64, [f64; 3])> =
                if near.len() >= 256 { near.par_iter().map(eval).collect() } else { near.iter().map(eval).collect() };
            let mut want_soft = false;
            if rigid {
                for &(i, d, g) in &hits {
                    let vn = -(vl[3 * i] * g[0] + vl[3 * i + 1] * g[1] + vl[3 * i + 2] * g[2]);
                    if vn > impact_speed && d < vn * 0.12 + 0.05 {
                        want_soft = true;
                    }
                }
            }
            let hit: Vec<(usize, f64, [f64; 3])> = hits.into_iter().filter(|h| h.1 < 0.0).collect();
            if apply {
                for &(i, d, n) in &hit {
                    for a in 0..3 {
                        p[3 * i + a] -= n[a] * (d * soft);
                    }
                    let disp = [p[3 * i] - pv[3 * i], p[3 * i + 1] - pv[3 * i + 1], p[3 * i + 2] - pv[3 * i + 2]];
                    let dn = disp[0] * n[0] + disp[1] * n[1] + disp[2] * n[2];
                    let f = fr[i].clamp(0.0, 1.0);
                    for a in 0..3 {
                        p[3 * i + a] -= (disp[a] - n[a] * dn) * f;
                    }
                }
            }
            (want_soft, hit)
        });
        let (want_soft, hit) = res;
        let m = hit.len();
        let idx: Vec<i64> = hit.iter().map(|h| h.0 as i64).collect();
        let dep: Vec<f64> = hit.iter().map(|h| -h.1).collect();
        let nrm: Vec<f64> = hit.iter().flat_map(|h| h.2).collect();
        Ok((want_soft, PyArray1::from_vec_bound(py, idx), PyArray1::from_vec_bound(py, dep),
            PyArray1::from_vec_bound(py, nrm).reshape([m, 3])?))
    }

    /// 惑星ローカル座標 (N,3) → (洞窟までの距離 (N,), 向き (N,3))
    fn cave_dist<'py>(&self, py: Python<'py>, q: RoF<'py>) -> PyResult<(Bound<'py, PyArray1<f64>>, Bound<'py, PyArray2<f64>>)> {
        let qq = ro(&q, "q")?;
        let n = n_rows(&q);
        let res = py.allow_threads(|| map_points(qq, |x| self.shape.cave_dist(x)));
        let d: Vec<f64> = res.iter().map(|x| x.0).collect();
        let g: Vec<f64> = res.iter().flat_map(|x| x.1).collect();
        Ok((PyArray1::from_vec_bound(py, d), PyArray1::from_vec_bound(py, g).reshape([n, 3])?))
    }
}

// ---------------------------------------------------------------------------
// 近い点の組（構築物どうしの押し出し用。scipy の cKDTree の代わり）
// ---------------------------------------------------------------------------
fn grid_key(p: &[f64], cell: f64) -> (i64, i64, i64) {
    ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64, (p[2] / cell).floor() as i64)
}

fn build_grid(pts: &[f64], cell: f64) -> HashMap<(i64, i64, i64), Vec<usize>> {
    let mut g: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, p) in pts.chunks_exact(3).enumerate() {
        if p.iter().all(|x| x.is_finite()) {
            g.entry(grid_key(p, cell)).or_default().push(i);
        }
    }
    g
}

fn within(a: &[f64], b: &[f64], r2: f64) -> bool {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= r2
}

/// 点の集合の中で、距離 r 以内の組 (i < j)。i, j の順に並べて返す。
#[pyfunction]
fn pairs_within<'py>(py: Python<'py>, pts: RoF<'py>, r: f64) -> PyResult<(Bound<'py, PyArray1<i64>>, Bound<'py, PyArray1<i64>>)> {
    let p = ro(&pts, "pts")?;
    let mut out: Vec<(usize, usize)> = py.allow_threads(|| {
        let cell = r.max(1e-9);
        let g = build_grid(p, cell);
        let mut out = Vec::new();
        let r2 = r * r;
        for (i, pi) in p.chunks_exact(3).enumerate() {
            if !pi.iter().all(|x| x.is_finite()) {
                continue;
            }
            let k = grid_key(pi, cell);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(list) = g.get(&(k.0 + dx, k.1 + dy, k.2 + dz)) {
                            for &j in list {
                                if j > i && within(pi, &p[3 * j..3 * j + 3], r2) {
                                    out.push((i, j));
                                }
                            }
                        }
                    }
                }
            }
        }
        out
    });
    out.sort_unstable();
    let (a, b): (Vec<i64>, Vec<i64>) = out.into_iter().map(|(i, j)| (i as i64, j as i64)).unzip();
    Ok((PyArray1::from_vec_bound(py, a), PyArray1::from_vec_bound(py, b)))
}

/// 2つの点の集合の間で、距離 r 以内の組 (A の番号, B の番号)。
#[pyfunction]
fn pairs_between<'py>(py: Python<'py>, a: RoF<'py>, b: RoF<'py>, r: f64) -> PyResult<(Bound<'py, PyArray1<i64>>, Bound<'py, PyArray1<i64>>)> {
    let (pa, pb) = (ro(&a, "a")?, ro(&b, "b")?);
    let mut out: Vec<(usize, usize)> = py.allow_threads(|| {
        let cell = r.max(1e-9);
        let g = build_grid(pb, cell);
        let r2 = r * r;
        let mut out = Vec::new();
        for (i, pi) in pa.chunks_exact(3).enumerate() {
            if !pi.iter().all(|x| x.is_finite()) {
                continue;
            }
            let k = grid_key(pi, cell);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(list) = g.get(&(k.0 + dx, k.1 + dy, k.2 + dz)) {
                            for &j in list {
                                if within(pi, &pb[3 * j..3 * j + 3], r2) {
                                    out.push((i, j));
                                }
                            }
                        }
                    }
                }
            }
        }
        out
    });
    out.sort_unstable();
    let (x, y): (Vec<i64>, Vec<i64>) = out.into_iter().map(|(i, j)| (i as i64, j as i64)).unzip();
    Ok((PyArray1::from_vec_bound(py, x), PyArray1::from_vec_bound(py, y)))
}

/// 読み込めたかの確認用
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pymodule]
fn alpedia_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(shape_match, m)?)?;
    m.add_function(wrap_pyfunction!(solve_joints, m)?)?;
    m.add_function(wrap_pyfunction!(contact_cores, m)?)?;
    m.add_function(wrap_pyfunction!(skin, m)?)?;
    m.add_function(wrap_pyfunction!(vertex_normals, m)?)?;
    m.add_function(wrap_pyfunction!(box_sdf, m)?)?;
    m.add_function(wrap_pyfunction!(art_solve, m)?)?;
    m.add_function(wrap_pyfunction!(art_contact_vel, m)?)?;
    m.add_function(wrap_pyfunction!(art_write, m)?)?;
    m.add_function(wrap_pyfunction!(gravity, m)?)?;
    m.add_function(wrap_pyfunction!(pairs_within, m)?)?;
    m.add_function(wrap_pyfunction!(pairs_between, m)?)?;
    m.add_class::<PlanetShape>()?;
    Ok(())
}
