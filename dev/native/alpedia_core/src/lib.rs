//! alpedia_core：Alpedia の物理コア（Rust）。Python からは `import alpedia_core` で使う。
//!
//! - kernels：構築物の形合わせ・結合・当たり判定の芯・描画用の頂点と法線・向き付きの箱
//! - art：完全剛体モードの関節つき剛体
//! - planet：惑星の地形（距離場・タイルのキャッシュ・洞窟・池）
//!
//! 配列は numpy の C 順（行優先）で連続したものを受け取る。重い計算の間は Python の GIL を離すので、
//! Python 側のスレッドから同時に呼べる（構築物ごとの並列計算）。

mod art;
mod artstep;
mod body;
mod kernels;
mod mesh;
mod pairs;
mod contacts;
mod devices;
mod planet;
mod tick;
mod geomx;
mod mc_tables;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use numpy::{PyArray1, PyArray2, PyArrayMethods, PyReadonlyArray2, PyReadonlyArrayDyn, PyReadwriteArrayDyn, PyUntypedArrayMethods};
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

/// ヒンジの辺 (R,2)。None なら全部 -1（回転）。番号は 0〜3 か -1
fn hinge_rows(hk: &Option<RoI>, nr: usize) -> PyResult<Vec<i64>> {
    match hk {
        None => Ok(vec![-1; 2 * nr]),
        Some(a) => {
            let v = ro(a, "hk")?.to_vec();
            need(
                v.len() == 2 * nr
                    && v.chunks(2).all(|p| (p[0] == -1 && p[1] == -1) || (0..4).contains(&p[0]) && (0..4).contains(&p[1])),
                "ヒンジの辺 hk",
            )?;
            Ok(v)
        }
    }
}

// ---------------------------------------------------------------------------
// 完全剛体モード
// ---------------------------------------------------------------------------
/// hk (R,2)：関節ごとのヒンジの倒れる側の辺（出力面の4隅のうち2つ）。省略・-1 なら回転
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (x, r, inv_m, iinv_b, ra, rb, la, lb, lc, ang, cap, cg, cl, cn, cd, cp, cprev, cvs, cmu, h, iters, lam_n, hk=None))]
fn art_solve(
    py: Python<'_>, mut x: RwF, mut r: RwF, inv_m: RoF, iinv_b: RoF, ra: RoI, rb: RoI, la: RoF, lb: RoF, lc: RoF,
    ang: RoF, cap: RoF, cg: RoI, cl: RoF, cn: RoF, cd: RoF, cp: RoF, cprev: RoF, cvs: RoF, cmu: RoF, h: f64,
    iters: i64, mut lam_n: RwF, hk: Option<RoI>,
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
    let hkv = hinge_rows(&hk, nr)?;
    py.allow_threads(|| {
        art::art_solve(xx, rr, im, ib, a, b, la_, lb_, lc_, an, cp_, g, l, n, d, p0, pv, vs, mu, h, iters, ln, &hkv)
    });
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
    shape: Arc<planet::Shape>,
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
        Ok(PlanetShape { shape: Arc::new(planet::Shape::new(radius, amp, freq, seed, e)), center, craters: RwLock::new(Vec::new()) })
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
// 構築物1つの1サブステップをまとめて（段階B）
// ---------------------------------------------------------------------------
/// 構築物の「形が変わるまで変わらない値」。Python の rebuild ごとに作り直す。
#[pyclass(frozen)]
struct Body {
    t: body::Topo,
}

/// planets：[(PlanetShape, bound, lipschitz, gm, radius, has_water), ...]（世界の全部の惑星）
fn planet_ctx(planets: &Bound<'_, pyo3::types::PyList>) -> PyResult<Vec<body::PlanetCtx>> {
    let mut out = Vec::with_capacity(planets.len());
    for it in planets.iter() {
        let (ps, bound, lipschitz, gm, radius, has_water): (Bound<'_, PlanetShape>, f64, f64, f64, f64, bool) = it.extract()?;
        let p = ps.get();
        out.push(body::PlanetCtx {
            shape: p.shape.clone(),
            center: p.center,
            craters: p.craters.read().unwrap().clone(),
            bound,
            lipschitz,
            gm,
            radius,
            has_water,
        });
    }
    Ok(out)
}

fn cav_rows(cav: &Option<RoI<'_>>) -> PyResult<Vec<[i64; 6]>> {
    match cav {
        None => Ok(Vec::new()),
        Some(a) => Ok(ro(a, "cav")?.chunks_exact(6).map(|c| [c[0], c[1], c[2], c[3], c[4], c[5]]).collect()),
    }
}

fn need(ok: bool, what: &str) -> PyResult<()> {
    if ok {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!("{what}: 配列の大きさが合いません")))
    }
}

#[pymethods]
impl Body {
    /// rotor_avox (R,)、rotor_a / rotor_b (R,4)：回転ジョイントの土台のボクセル・土台側と出力側の4頂点、rotor_br (R,)：ちぎれる距離、
    /// rotor_hk (R,2)：ヒンジの倒れる側の辺（rotor_a の4つのうち2つ。省略・-1 なら回転）
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (size, ja, jb, jr, jk, jbrk, jcount, inv_mass, mass, damp, fric, stiff, plast, yield_, density, ctype, cidx,
                        rotor_avox, rotor_a, rotor_b, rotor_br, rotor_hk=None))]
    fn new(
        size: f64, ja: RoI, jb: RoI, jr: RoF, jk: RoF, jbrk: RoF, jcount: RoF, inv_mass: RoF, mass: RoF, damp: RoF, fric: RoF,
        stiff: RoF, plast: RoF, yield_: RoF, density: RoF, ctype: RoI, cidx: RoI, rotor_avox: RoI, rotor_a: RoI, rotor_b: RoI,
        rotor_br: RoF, rotor_hk: Option<RoI>,
    ) -> PyResult<Self> {
        let nvox = ro(&stiff, "stiff")?.len();
        let nv = 8 * nvox;
        let (ja, jb) = (ro(&ja, "ja")?.to_vec(), ro(&jb, "jb")?.to_vec());
        let nj = ja.len();
        need(jb.len() == nj && ja.iter().chain(jb.iter()).all(|&i| i >= 0 && (i as usize) < nv), "Body 結合")?;
        let ra = ro(&rotor_avox, "rotor_avox")?;
        let (rva, rvb, rbr) = (ro(&rotor_a, "rotor_a")?, ro(&rotor_b, "rotor_b")?, ro(&rotor_br, "rotor_br")?);
        let nr = ra.len();
        need(rva.len() == 4 * nr && rvb.len() == 4 * nr && rbr.len() == nr, "Body 回転ジョイント")?;
        need(
            ra.iter().all(|&i| i >= 0 && (i as usize) < nvox)
                && rva.iter().chain(rvb.iter()).all(|&i| i >= 0 && (i as usize) < nv),
            "Body 回転ジョイントの番号",
        )?;
        let rhk = hinge_rows(&rotor_hk, nr)?;
        let rotors = (0..nr)
            .map(|q| body::Rotor {
                hk: [rhk[2 * q], rhk[2 * q + 1]],
                a_vox: ra[q] as usize,
                a: [rva[4 * q] as usize, rva[4 * q + 1] as usize, rva[4 * q + 2] as usize, rva[4 * q + 3] as usize],
                b: [rvb[4 * q] as usize, rvb[4 * q + 1] as usize, rvb[4 * q + 2] as usize, rvb[4 * q + 3] as usize],
                br: rbr[q],
            })
            .collect();
        let t = body::Topo {
            size,
            nvox,
            ja,
            jb,
            jr: ro(&jr, "jr")?.to_vec(),
            jk: ro(&jk, "jk")?.to_vec(),
            jbrk: ro(&jbrk, "jbrk")?.to_vec(),
            jcount: ro(&jcount, "jcount")?.to_vec(),
            inv_mass: ro(&inv_mass, "inv_mass")?.to_vec(),
            mass: ro(&mass, "mass")?.to_vec(),
            damp: ro(&damp, "damp")?.to_vec(),
            fric: ro(&fric, "fric")?.to_vec(),
            stiff: ro(&stiff, "stiff")?.to_vec(),
            plast: ro(&plast, "plast")?.to_vec(),
            yield_: ro(&yield_, "yield")?.to_vec(),
            density: ro(&density, "density")?.to_vec(),
            ctype: ro(&ctype, "ctype")?.to_vec(),
            cidx: ro(&cidx, "cidx")?.to_vec(),
            rotors,
        };
        need(
            t.jr.len() == nj && t.jk.len() == nj && t.jbrk.len() == nj && t.jcount.len() == nv && t.inv_mass.len() == nv
                && t.mass.len() == nv && t.damp.len() == nv && t.fric.len() == nv && t.plast.len() == nvox
                && t.yield_.len() == nvox && t.density.len() == nvox && t.ctype.len() == nv && t.cidx.len() == 3 * nv,
            "Body",
        )?;
        Ok(Body { t })
    }

    /// ブロックごとの計算の1サブステップの前半（重力・浮力・減衰・積分・形合わせ・結合・回転ジョイント・地面）。
    /// angles：回転ジョイントの角度（ラジアン）。cav：気密の空洞の6方向のボクセル番号 (K,6) か None
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (pos, vel, prev, force, rest, act_scale, last_r, quat, planets, scale, h, iters, angles, parallel, cav, soft))]
    fn soft_begin(
        &self, py: Python<'_>, mut pos: RwF, mut vel: RwF, mut prev: RwF, force: RoF, rest: RoF, act_scale: RoF,
        mut last_r: RwF, mut quat: RwF, planets: &Bound<'_, pyo3::types::PyList>, scale: f64, h: f64, iters: i64,
        angles: Vec<f64>, parallel: bool, cav: Option<RoI>, soft: f64,
    ) -> PyResult<()> {
        let t = &self.t;
        let pls = planet_ctx(planets)?;
        let cv = cav_rows(&cav)?;
        let (p, v, pv) = (rw(&mut pos, "pos")?, rw(&mut vel, "vel")?, rw(&mut prev, "prev")?);
        let (f, r, s) = (ro(&force, "force")?, ro(&rest, "rest")?, ro(&act_scale, "act_scale")?);
        let (lr, q) = (rw(&mut last_r, "lastR")?, rw(&mut quat, "quat")?);
        let nv = 8 * t.nvox;
        need(
            p.len() == 3 * nv && v.len() == 3 * nv && pv.len() == 3 * nv && f.len() == 3 * nv && r.len() == 3 * nv
                && s.len() == 3 * t.nvox && lr.len() == 9 * t.nvox && q.len() == 4 * t.nvox && angles.len() == t.rotors.len()
                && cv.iter().flatten().all(|&j| j < t.nvox as i64),
            "soft_begin",
        )?;
        py.allow_threads(|| body::soft_begin(p, v, pv, f, r, s, lr, q, t, &pls, scale, h, iters, &angles, parallel, &cv, soft));
        Ok(())
    }

    /// 後半：速度・塑性変形・ちぎれの判定。戻り値 (ちぎれた結合の番号, ちぎれた回転ジョイントの番号)
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn soft_end<'py>(
        &self, py: Python<'py>, pos: RoF<'py>, mut vel: RwF<'py>, prev: RoF<'py>, mut rest: RwF<'py>, act_scale: RoF<'py>,
        last_r: RoF<'py>, h: f64, angles: Vec<f64>,
    ) -> PyResult<(Bound<'py, PyArray1<i64>>, Bound<'py, PyArray1<i64>>)> {
        let t = &self.t;
        let (p, pv, s, lr) = (ro(&pos, "pos")?, ro(&prev, "prev")?, ro(&act_scale, "act_scale")?, ro(&last_r, "lastR")?);
        let (v, r) = (rw(&mut vel, "vel")?, rw(&mut rest, "rest")?);
        let nv = 8 * t.nvox;
        need(
            p.len() == 3 * nv && v.len() == 3 * nv && pv.len() == 3 * nv && r.len() == 3 * nv && s.len() == 3 * t.nvox
                && (lr.is_empty() || lr.len() == 9 * t.nvox) && angles.len() == t.rotors.len(),
            "soft_end",
        )?;
        let (br, rot) = py.allow_threads(|| body::soft_end(p, v, pv, r, s, lr, t, h, &angles));
        Ok((PyArray1::from_vec_bound(py, br), PyArray1::from_vec_bound(py, rot)))
    }

    /// 剛体の1サブステップの前半。full：完全剛体モード（接触をまとめて解く）。戻り値：速くぶつかりそうか
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (pos, vel, prev, force, rot, ib, planets, scale, h, cav, full, impact_speed, soft, defer=false))]
    fn rigid_begin(
        &self, py: Python<'_>, mut pos: RwF, mut vel: RwF, mut prev: RwF, force: RoF, rot: RoF, ib: RoF,
        planets: &Bound<'_, pyo3::types::PyList>, scale: f64, h: f64, cav: Option<RoI>, full: bool, impact_speed: f64, soft: f64,
        defer: bool,
    ) -> PyResult<bool> {
        let t = &self.t;
        let pls = planet_ctx(planets)?;
        let cv = cav_rows(&cav)?;
        let (p, v, pv) = (rw(&mut pos, "pos")?, rw(&mut vel, "vel")?, rw(&mut prev, "prev")?);
        let (f, rr, ii) = (ro(&force, "force")?, ro(&rot, "R")?, ro(&ib, "Ib")?);
        let nv = 8 * t.nvox;
        need(
            p.len() == 3 * nv && v.len() == 3 * nv && pv.len() == 3 * nv && f.len() == 3 * nv && rr.len() == 9 && ii.len() == 9
                && cv.iter().flatten().all(|&j| j < t.nvox as i64),
            "rigid_begin",
        )?;
        Ok(py.allow_threads(|| body::rigid_begin(p, v, pv, f, rr, ii, t, &pls, scale, h, &cv, full, impact_speed, soft, defer)))
    }

    /// 完全剛体モードの接触（地面＋MOD の箱）をまとめて解く（rigid_begin を defer=True で呼んだあと）。
    /// boxes：(P, D, N, VS, CI)。戻り値 (速くぶつかりそうか, 使った頂点, 法線, 押し返し, 箱の番号, 解く前の位置)
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn rigid_contacts<'py>(
        &self, py: Python<'py>, mut pos: RwF<'py>, prev: RoF<'py>, vel: RoF<'py>, planets: &Bound<'py, pyo3::types::PyList>,
        h: f64, impact_speed: f64, soft: f64, bp: RoI<'py>, bd: RoF<'py>, bn: RoF<'py>, bvs: RoF<'py>, bci: RoI<'py>,
    ) -> PyResult<(
        bool, Bound<'py, PyArray1<i64>>, Bound<'py, PyArray2<f64>>, Bound<'py, PyArray1<f64>>, Bound<'py, PyArray1<i64>>,
        Bound<'py, PyArray2<f64>>,
    )> {
        let t = &self.t;
        let pls = planet_ctx(planets)?;
        let nv = 8 * t.nvox;
        let p = rw(&mut pos, "pos")?;
        let (pv, vl) = (ro(&prev, "prev")?, ro(&vel, "vel")?);
        let (bpv, bdv, bnv, bvv, bcv) = (ro(&bp, "bp")?, ro(&bd, "bd")?, ro(&bn, "bn")?, ro(&bvs, "bvs")?, ro(&bci, "bci")?);
        let nb = bpv.len();
        need(
            p.len() == 3 * nv && pv.len() == 3 * nv && vl.len() == 3 * nv && bdv.len() == nb && bnv.len() == 3 * nb
                && bvv.len() == 3 * nb && bcv.len() == nb && bpv.iter().all(|&i| i >= 0 && (i as usize) < nv),
            "rigid_contacts",
        )?;
        let boxes: Vec<body::RContact> = (0..nb)
            .map(|k| {
                (bpv[k] as usize, bdv[k], [bnv[3 * k], bnv[3 * k + 1], bnv[3 * k + 2]],
                 [bvv[3 * k], bvv[3 * k + 1], bvv[3 * k + 2]], bcv[k])
            })
            .collect();
        let (want, used) = py.allow_threads(|| body::full_contacts(p, pv, vl, t, &pls, h, impact_speed, soft, boxes));
        let m = used.len();
        Ok((
            want,
            PyArray1::from_vec_bound(py, used.iter().map(|u| u.0 .0 as i64).collect()),
            PyArray1::from_vec_bound(py, used.iter().flat_map(|u| u.0 .2).collect()).reshape([m, 3])?,
            PyArray1::from_vec_bound(py, used.iter().map(|u| u.1).collect()),
            PyArray1::from_vec_bound(py, used.iter().map(|u| u.0 .4).collect()),
            PyArray1::from_vec_bound(py, used.iter().flat_map(|u| u.2).collect()).reshape([m, 3])?,
        ))
    }
}

/// 剛体の形に戻して速度を出す（construct._rigid_fit_pos と速度）。戻り値 (R (3,3), 重心 (3,))
#[pyfunction]
fn rigid_end<'py>(
    py: Python<'py>, mut pos: RwF<'py>, mut vel: RwF<'py>, prev: RoF<'py>, local: RoF<'py>, mass: RoF<'py>, h: f64,
) -> PyResult<(Bound<'py, PyArray2<f64>>, Bound<'py, PyArray1<f64>>)> {
    let (p, v) = (rw(&mut pos, "pos")?, rw(&mut vel, "vel")?);
    let (pv, l, m) = (ro(&prev, "prev")?, ro(&local, "local")?, ro(&mass, "mass")?);
    let nv = m.len();
    need(p.len() == 3 * nv && v.len() == 3 * nv && pv.len() == 3 * nv && l.len() == 3 * nv && nv > 0, "rigid_end")?;
    let (r, x) = py.allow_threads(|| body::rigid_end(p, v, pv, l, m, h));
    Ok((PyArray1::from_vec_bound(py, r.to_vec()).reshape([3, 3])?, PyArray1::from_vec_bound(py, x.to_vec())))
}

/// 非負の最小二乗（テスト用）
#[pyfunction]
fn nnls<'py>(py: Python<'py>, a: PyReadonlyArray2<'py, f64>, b: RoF<'py>) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let s = a.shape();
    let (n, m) = (s[0], s[1]);
    let am = nalgebra::DMatrix::from_row_slice(n, m, a.as_slice().map_err(|_| PyValueError::new_err("a: 連続した配列ではありません"))?);
    let bv = nalgebra::DVector::from_column_slice(ro(&b, "b")?);
    let x = body::nnls(&am, &bv, 200).ok_or_else(|| PyValueError::new_err("nnls: 解けません"))?;
    Ok(PyArray1::from_vec_bound(py, x.as_slice().to_vec()))
}




// ---------------------------------------------------------------------------
// 完全剛体モードの関節つき剛体の1サブステップ
// ---------------------------------------------------------------------------
/// 部分の分け方など（construct._art_build で作り直す）
#[pyclass(frozen)]
struct ArtTopo {
    at: artstep::ArtTopo,
}

fn idx_rows(a: &RoI, name: &str, w: usize, lim: usize) -> PyResult<Vec<usize>> {
    let v = ro(a, name)?;
    need(v.len() % w.max(1) == 0 && v.iter().all(|&i| i >= 0 && (i as usize) < lim), name)?;
    Ok(v.iter().map(|&i| i as usize).collect())
}

#[pymethods]
impl ArtTopo {
    /// vg：頂点ごとの部分の番号。inv_m・iinv_b：部分ごとの逆質量と物体座標の逆慣性 (G,3,3)。
    /// ra/rb：回転関節の土台側・出力側の部分、a/b (R,4)：出力面の4頂点、c (R,8)：アクチュエーターの8頂点。
    /// ext_rows (E,4)・ext_nb (E,3)・ext_e0 (E,)：押し出し。hk (R,2)：ヒンジの倒れる側の辺（省略・-1 なら回転）
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (vg, inv_m, iinv_b, ra, rb, a, b, c, ext_rows, ext_nb, ext_e0, iters, max_contacts, motor_step_x,
                        motor_hold_deg, hk=None, fixed=None, xfix=None))]
    fn new(
        vg: RoI, inv_m: RoF, iinv_b: RoF, ra: RoI, rb: RoI, a: RoI, b: RoI, c: RoI, ext_rows: RoI, ext_nb: RoF, ext_e0: RoF,
        iters: i64, max_contacts: usize, motor_step_x: f64, motor_hold_deg: f64, hk: Option<RoI>, fixed: Option<RoI>,
        xfix: Option<RoF>,
    ) -> PyResult<Self> {
        let vgv = ro(&vg, "vg")?.to_vec();
        let im = ro(&inv_m, "inv_m")?.to_vec();
        let g = im.len();
        let nv = vgv.len();
        need(vgv.iter().all(|&k| k >= 0 && (k as usize) < g), "ArtTopo vg")?;
        let ib = ro(&iinv_b, "iinv_b")?.to_vec();
        need(ib.len() == 9 * g, "ArtTopo iinv_b")?;
        let (rav, rbv) = (ro(&ra, "ra")?.to_vec(), ro(&rb, "rb")?.to_vec());
        need(rav.len() == rbv.len() && rav.iter().chain(rbv.iter()).all(|&k| k >= 0 && (k as usize) < g), "ArtTopo ra/rb")?;
        let nr = rav.len();
        let (av, bv, cv) = (idx_rows(&a, "a", 4, nv)?, idx_rows(&b, "b", 4, nv)?, idx_rows(&c, "c", 8, nv)?);
        need(av.len() == 4 * nr && bv.len() == 4 * nr && cv.len() == 8 * nr, "ArtTopo a/b/c")?;
        let er = idx_rows(&ext_rows, "ext_rows", 4, nv)?;
        let (en, e0) = (ro(&ext_nb, "ext_nb")?, ro(&ext_e0, "ext_e0")?);
        let ne = e0.len();
        need(er.len() == 4 * ne && en.len() == 3 * ne, "ArtTopo ext")?;
        let exts = (0..ne)
            .map(|k| artstep::Ext {
                rows: [er[4 * k], er[4 * k + 1], er[4 * k + 2], er[4 * k + 3]],
                nb: [en[3 * k], en[3 * k + 1], en[3 * k + 2]],
                e0: e0[k],
            })
            .collect();
        // fixed (G,)：アンカーで固定された部分（0/1）、xfix (G,3)：その基準点
        let fx: Vec<bool> = match &fixed {
            Some(f) => ro(f, "fixed")?.iter().map(|&q| q != 0).collect(),
            None => Vec::new(),
        };
        need(fx.is_empty() || fx.len() == g, "ArtTopo fixed")?;
        let xf: Vec<f64> = match &xfix {
            Some(q) => ro(q, "xfix")?.to_vec(),
            None => Vec::new(),
        };
        need(fx.is_empty() || xf.len() == 3 * g, "ArtTopo xfix")?;
        let ims: Vec<f64> = (0..g).map(|k| if !fx.is_empty() && fx[k] { 0.0 } else { im[k] }).collect();
        let ibs: Vec<f64> = (0..9 * g).map(|j| if !fx.is_empty() && fx[j / 9] { 0.0 } else { ib[j] }).collect();
        Ok(ArtTopo {
            at: artstep::ArtTopo {
                g, vg: vgv, inv_m: im, iinv_b: ib, ra: rav, rb: rbv, a: av, b: bv, c: cv, exts, hk: hinge_rows(&hk, nr)?, iters,
                max_contacts,
                motor_step_x, motor_hold_deg, fixed: fx, xfix: xf, inv_m_s: ims, iinv_b_s: ibs,
            },
        })
    }

    /// 前半の1（力・押し出し・部分ごとに位置を進める）。戻り値 (形が変わった頂点, その変化 (k,3))
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    #[pyo3(signature = (body, pos, vel, prev, force, x, r, vl, vl0, ext_applied, ext_want, ext_speed, planets, scale, h, cav))]
    fn predict<'py>(
        &self, py: Python<'py>, body: &Bound<'py, Body>, mut pos: RwF<'py>, mut vel: RwF<'py>, mut prev: RwF<'py>,
        force: RoF<'py>, mut x: RwF<'py>, mut r: RwF<'py>, mut vl: RwF<'py>, vl0: RoF<'py>, mut ext_applied: RwF<'py>,
        ext_want: Vec<f64>, ext_speed: Vec<f64>, planets: &Bound<'py, pyo3::types::PyList>, scale: f64, h: f64,
        cav: Option<RoI<'py>>,
    ) -> PyResult<(Bound<'py, PyArray1<i64>>, Bound<'py, PyArray2<f64>>)> {
        let (t, at) = (&body.get().t, &self.at);
        let pls = planet_ctx(planets)?;
        let cv = cav_rows(&cav)?;
        let nv = 8 * t.nvox;
        let (p, v, pv) = (rw(&mut pos, "pos")?, rw(&mut vel, "vel")?, rw(&mut prev, "prev")?);
        let (xx, rr, vv) = (rw(&mut x, "X")?, rw(&mut r, "R")?, rw(&mut vl, "vl")?);
        let (f, v0) = (ro(&force, "force")?, ro(&vl0, "vl0")?);
        let ea = rw(&mut ext_applied, "ext_applied")?;
        let ne = at.exts.len();
        need(
            at.vg.len() == nv && p.len() == 3 * nv && v.len() == 3 * nv && pv.len() == 3 * nv && f.len() == 3 * nv
                && vv.len() == 3 * nv && v0.len() == 3 * nv && xx.len() == 3 * at.g && rr.len() == 9 * at.g
                && ea.len() == ne && ext_want.len() == ne && ext_speed.len() == ne
                && cv.iter().flatten().all(|&j| j < t.nvox as i64),
            "art predict",
        )?;
        let (rows, d) = py.allow_threads(|| {
            artstep::predict(p, v, pv, f, t, at, xx, rr, vv, v0, ea, &ext_want, &ext_speed, &pls, scale, h, &cv)
        });
        let k = rows.len();
        Ok((
            PyArray1::from_vec_bound(py, rows.into_iter().map(|i| i as i64).collect()),
            PyArray1::from_vec_bound(py, d.into_iter().flatten().collect()).reshape([k, 3])?,
        ))
    }

    /// 前半の2（接触を集めて関節と接触を解く）。boxes：MOD の箱との接触 (P, D, N, VS, CI)。
    /// 戻り値 (部分の番号 cg, 部分の座標での接触点 cl, 法線 N, 押し返し lam, 頂点 P, 箱の番号 CI)
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    #[pyo3(signature = (body, pos, prev, x, r, vl, applied, want, rot_speed, planets, h, bp, bd, bn, bvs, bci))]
    fn solve<'py>(
        &self, py: Python<'py>, body: &Bound<'py, Body>, mut pos: RwF<'py>, prev: RoF<'py>, mut x: RwF<'py>,
        mut r: RwF<'py>, vl: RoF<'py>, mut applied: RwF<'py>, want: Vec<f64>, rot_speed: Vec<f64>,
        planets: &Bound<'py, pyo3::types::PyList>, h: f64, bp: RoI<'py>, bd: RoF<'py>, bn: RoF<'py>, bvs: RoF<'py>,
        bci: RoI<'py>,
    ) -> PyResult<(
        Bound<'py, PyArray1<i64>>, Bound<'py, PyArray2<f64>>, Bound<'py, PyArray2<f64>>, Bound<'py, PyArray1<f64>>,
        Bound<'py, PyArray1<i64>>, Bound<'py, PyArray1<i64>>,
    )> {
        let (t, at) = (&body.get().t, &self.at);
        let pls = planet_ctx(planets)?;
        let nv = 8 * t.nvox;
        let p = rw(&mut pos, "pos")?;
        let (xx, rr, ap) = (rw(&mut x, "X")?, rw(&mut r, "R")?, rw(&mut applied, "applied")?);
        let (pv, vv) = (ro(&prev, "prev")?, ro(&vl, "vl")?);
        let (bpv, bdv, bnv, bvv, bcv) = (ro(&bp, "bp")?, ro(&bd, "bd")?, ro(&bn, "bn")?, ro(&bvs, "bvs")?, ro(&bci, "bci")?);
        let nb = bpv.len();
        let nr = at.ra.len();
        need(
            at.vg.len() == nv && p.len() == 3 * nv && pv.len() == 3 * nv && vv.len() == 3 * nv && xx.len() == 3 * at.g
                && rr.len() == 9 * at.g && ap.len() == nr && want.len() == nr && rot_speed.len() == nr
                && bdv.len() == nb && bnv.len() == 3 * nb && bvv.len() == 3 * nb && bcv.len() == nb
                && bpv.iter().all(|&i| i >= 0 && (i as usize) < nv),
            "art solve",
        )?;
        let boxes: Vec<artstep::Contact> = (0..nb)
            .map(|k| artstep::Contact {
                p: bpv[k] as usize,
                d: bdv[k],
                n: [bnv[3 * k], bnv[3 * k + 1], bnv[3 * k + 2]],
                vs: [bvv[3 * k], bvv[3 * k + 1], bvv[3 * k + 2]],
                ci: bcv[k],
            })
            .collect();
        let (kept, cl, lam) =
            py.allow_threads(|| artstep::solve(p, pv, t, at, xx, rr, vv, ap, &want, &rot_speed, &pls, h, &boxes));
        let nc = kept.len();
        let cg: Vec<i64> = kept.iter().map(|c| at.vg[c.p]).collect();
        let cn: Vec<f64> = kept.iter().flat_map(|c| c.n).collect();
        let cp: Vec<i64> = kept.iter().map(|c| c.p as i64).collect();
        let ci: Vec<i64> = kept.iter().map(|c| c.ci).collect();
        Ok((
            PyArray1::from_vec_bound(py, cg),
            PyArray1::from_vec_bound(py, cl).reshape([nc, 3])?,
            PyArray1::from_vec_bound(py, cn).reshape([nc, 3])?,
            PyArray1::from_vec_bound(py, lam),
            PyArray1::from_vec_bound(py, cp),
            PyArray1::from_vec_bound(py, ci),
        ))
    }

    /// 後半（部分ごとの剛体の動きに直して速度を求める）
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (body, pos, vel, prev, x, r, vl, defo_rows, defo_d, cg, cl, cn, lam, h))]
    fn end<'py>(
        &self, py: Python<'py>, body: &Bound<'py, Body>, mut pos: RwF<'py>, mut vel: RwF<'py>, prev: RoF<'py>,
        mut x: RwF<'py>, mut r: RwF<'py>, vl: RoF<'py>, defo_rows: RoI<'py>, defo_d: RoF<'py>, cg: RoI<'py>,
        cl: RoF<'py>, cn: RoF<'py>, lam: RoF<'py>, h: f64,
    ) -> PyResult<()> {
        let (t, at) = (&body.get().t, &self.at);
        let nv = 8 * t.nvox;
        let (p, v) = (rw(&mut pos, "pos")?, rw(&mut vel, "vel")?);
        let (xx, rr) = (rw(&mut x, "X")?, rw(&mut r, "R")?);
        let (pv, vv) = (ro(&prev, "prev")?, ro(&vl, "vl")?);
        let (dr, dd) = (ro(&defo_rows, "defo_rows")?, ro(&defo_d, "defo_d")?);
        let (cgv, clv, cnv, lv) = (ro(&cg, "cg")?, ro(&cl, "cl")?, ro(&cn, "cn")?, ro(&lam, "lam")?);
        let nc = cgv.len();
        need(
            at.vg.len() == nv && p.len() == 3 * nv && v.len() == 3 * nv && pv.len() == 3 * nv && vv.len() == 3 * nv
                && xx.len() == 3 * at.g && rr.len() == 9 * at.g && dd.len() == 3 * dr.len()
                && dr.iter().all(|&i| i >= 0 && (i as usize) < nv) && clv.len() == 3 * nc && cnv.len() == 3 * nc
                && lv.len() == nc && cgv.iter().all(|&g| g >= 0 && (g as usize) < at.g),
            "art end",
        )?;
        let defo: Vec<(usize, [f64; 3])> =
            (0..dr.len()).map(|k| (dr[k] as usize, [dd[3 * k], dd[3 * k + 1], dd[3 * k + 2]])).collect();
        py.allow_threads(|| artstep::end(p, v, pv, t, at, xx, rr, vv, &defo, cgv, clv, cnv, lv, h));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 構築物どうしの押し出し
// ---------------------------------------------------------------------------
/// 構築物1つの当たり判定の点の作り方など（形が変わったら作り直す）
#[pyclass(frozen)]
struct Proxy {
    p: contacts::Proxy,
}

#[pymethods]
impl Proxy {
    /// indptr/indices/data：重み行列 W（点 × ボクセル、CSR）。inv：点ごとの質量の逆数。links：結合しているボクセルの組 (L,2)。
    /// mass：頂点ごとの質量
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(n: usize, size: f64, indptr: RoI, indices: RoI, data: RoF, inv: RoF, links: RoI, mass: RoF) -> PyResult<Self> {
        let ip: Vec<usize> = ro(&indptr, "indptr")?.iter().map(|&x| x.max(0) as usize).collect();
        let ix: Vec<usize> = ro(&indices, "indices")?.iter().map(|&x| x.max(0) as usize).collect();
        let dt = ro(&data, "data")?.to_vec();
        let iv = ro(&inv, "inv")?.to_vec();
        let lk = ro(&links, "links")?;
        let ms = ro(&mass, "mass")?.to_vec();
        let np_ = ip.len().saturating_sub(1);
        need(
            !ip.is_empty() && np_ >= n && ip.windows(2).all(|w| w[0] <= w[1]) && *ip.last().unwrap() == ix.len()
                && dt.len() == ix.len() && ix.iter().all(|&j| j < n) && iv.len() == np_ && lk.len() % 2 == 0
                && ms.len() == 8 * n,
            "Proxy",
        )?;
        let links = lk.chunks_exact(2).map(|c| {
            let (a, b) = (c[0].max(0) as usize, c[1].max(0) as usize);
            if a < b { (a, b) } else { (b, a) }
        }).collect();
        Ok(Proxy { p: contacts::Proxy { n, size, indptr: ip, indices: ix, data: dt, inv: iv, links, mass: ms } })
    }
}

/// 構築物どうしの押し出し。items：[(Proxy, pos（書き換える）, vel, 剛体か), ...]（最初の n_active 個が動いている構築物）。
/// 戻り値 (起きた眠っていた構築物の番号, 動いている物と触れているか, 速くぶつかった剛体か)
#[pyfunction]
#[pyo3(name = "contacts")]
fn push_apart<'py>(
    py: Python<'py>, items: Vec<(Bound<'py, Proxy>, RwF<'py>, RoF<'py>, bool)>, n_active: usize,
) -> PyResult<(Vec<usize>, Vec<bool>, Vec<bool>)> {
    let mut items = items;
    let mut list = Vec::with_capacity(items.len());
    for (k, (px, pos, vel, rigid)) in items.iter_mut().enumerate() {
        let p = &px.get().p;
        let pv = rw(pos, "pos")?;
        let vl = ro(vel, "vel")?;
        need(pv.len() == 24 * p.n && vl.len() == 24 * p.n, "contacts")?;
        list.push(contacts::Item { p, pos: pv, vel: vl, active: k < n_active, rigid: *rigid, pts: None });
    }
    let na = n_active.min(list.len());
    let o = py.allow_threads(|| contacts::solve(&mut list, na));
    Ok((o.woke, o.touch, o.want_soft))
}

// ---------------------------------------------------------------------------
// 1ティックの物理をまとめて（world.step のサブステップの繰り返し）
// ---------------------------------------------------------------------------
fn opt_rw<'py>(d: &Bound<'py, PyDict>, k: &str) -> PyResult<Option<RwF<'py>>> {
    match d.get_item(k)? {
        Some(v) if !v.is_none() => Ok(Some(v.extract()?)),
        _ => Ok(None),
    }
}

fn opt_ro<'py, T: numpy::Element>(d: &Bound<'py, PyDict>, k: &str) -> PyResult<Option<PyReadonlyArrayDyn<'py, T>>> {
    match d.get_item(k)? {
        Some(v) if !v.is_none() => Ok(Some(v.extract()?)),
        _ => Ok(None),
    }
}

fn req<'py, T: FromPyObject<'py>>(d: &Bound<'py, PyDict>, k: &str) -> PyResult<T> {
    match d.get_item(k)? {
        Some(v) => v.extract(),
        None => Err(PyValueError::new_err(format!("step_tick: {k} がありません"))),
    }
}

/// Python から受け取った、構築物1つ分の配列（借りている間は numpy の配列を書き換えられる）
struct JobArgs<'py> {
    kind: i64,
    body: Bound<'py, Body>,
    proxy: Bound<'py, Proxy>,
    art: Option<Bound<'py, ArtTopo>>,
    pos: RwF<'py>,
    vel: RwF<'py>,
    prev: RwF<'py>,
    force: RoF<'py>,
    cav: Option<RoI<'py>>,
    a: [Option<RwF<'py>>; 6], // soft：rest, last_r, quat / rigid：rb_r, rb_x / art：x, r, vl, applied, ext_applied（順に）
    b: [Option<RoF<'py>>; 3], // soft：act_scale / rigid：rb_ib, local / art：vl0
    v: [Vec<f64>; 4],         // soft：angles / art：want, rot_speed, ext_want, ext_speed
    parallel: bool,
    full: bool,
    boxes: Vec<usize>,
    contact_rigid: bool,
    settle_cap: f64,
}

fn take<'a>(o: &'a mut Option<RwF<'_>>, name: &str) -> PyResult<&'a mut [f64]> {
    match o {
        Some(a) => rw(a, name),
        None => Err(PyValueError::new_err(format!("step_tick: {name} がありません"))),
    }
}

fn take_ro<'a>(o: &'a Option<RoF<'_>>, name: &str) -> PyResult<&'a [f64]> {
    match o {
        Some(a) => ro(a, name),
        None => Err(PyValueError::new_err(format!("step_tick: {name} がありません"))),
    }
}

/// 1ティックの物理をまとめて進める。
/// jobs：動いている構築物ごとの dict（kind 0 ブロックごと / 1 剛体 / 2 完全剛体 / 3 関節つき剛体）。
/// sleepers：[(Proxy, pos, vel, 剛体か, 固定か, 起きてよいか)]。groups：MOD の箱 [(C, A, E, 部分)]。
/// 戻り値 (進めたサブステップの数, 起きた眠っていた物, 触れているか（jobs＋sleepers）, ブロックごとに戻したいか,
///         [(job, ちぎれた結合, ちぎれた回転ジョイント)], [(job, 箱のまとまり, 部分, 速さ, サブステップ)])
#[pyfunction]
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn step_tick<'py>(
    py: Python<'py>, jobs: Vec<Bound<'py, PyDict>>, sleepers: Vec<(Bound<'py, Proxy>, RwF<'py>, RoF<'py>, bool, bool, bool)>,
    groups: Vec<(RoF<'py>, RoF<'py>, RoF<'py>, RoI<'py>)>, planets: &Bound<'py, pyo3::types::PyList>, scale: f64, h: f64,
    substeps: usize, iters: i64, impact_speed: f64, soft: f64,
) -> PyResult<(usize, Vec<usize>, Vec<bool>, Vec<bool>, Vec<(usize, Vec<i64>, Vec<i64>)>, Vec<tick::Hit>)> {
    let pls = planet_ctx(planets)?;
    let mut gs = Vec::with_capacity(groups.len());
    for (c, a, e, p) in &groups {
        let (c, a, e, p) = (ro(c, "C")?.to_vec(), ro(a, "A")?.to_vec(), ro(e, "E")?.to_vec(), ro(p, "part")?.to_vec());
        let n = c.len() / 3;
        need(n > 0 && a.len() == 9 * n && e.len() == 3 * n && p.len() == n, "step_tick 箱")?;
        gs.push(tick::BoxGroup::new(c, a, e, p));
    }
    let mut args = Vec::with_capacity(jobs.len());
    for d in &jobs {
        let kind: i64 = req(d, "kind")?;
        let (a, b, v) = match kind {
            0 => (
                [opt_rw(d, "rest")?, opt_rw(d, "last_r")?, opt_rw(d, "quat")?, None, None, None],
                [opt_ro(d, "act_scale")?, None, None],
                [req(d, "angles")?, Vec::new(), Vec::new(), Vec::new()],
            ),
            1 | 2 => ([opt_rw(d, "rb_r")?, opt_rw(d, "rb_x")?, None, None, None, None], [opt_ro(d, "rb_ib")?, opt_ro(d, "local")?, None],
                      [Vec::new(), Vec::new(), Vec::new(), Vec::new()]),
            3 => (
                [opt_rw(d, "x")?, opt_rw(d, "r")?, opt_rw(d, "vl")?, opt_rw(d, "applied")?, opt_rw(d, "ext_applied")?, None],
                [opt_ro(d, "vl0")?, None, None],
                [req(d, "want")?, req(d, "rot_speed")?, req(d, "ext_want")?, req(d, "ext_speed")?],
            ),
            _ => return Err(PyValueError::new_err("step_tick: kind")),
        };
        let boxes: Vec<usize> = req(d, "boxes")?;
        need(boxes.iter().all(|&g| g < gs.len()), "step_tick boxes")?;
        args.push(JobArgs {
            kind,
            body: req(d, "body")?,
            proxy: req(d, "proxy")?,
            art: if kind == 3 { Some(req(d, "topo")?) } else { None },
            pos: req(d, "pos")?,
            vel: req(d, "vel")?,
            prev: req(d, "prev")?,
            force: req(d, "force")?,
            cav: opt_ro(d, "cav")?,
            a,
            b,
            v,
            parallel: req(d, "parallel")?,
            full: kind == 2,
            boxes,
            contact_rigid: kind == 1 || kind == 2,
            settle_cap: req(d, "settle_cap")?,
        });
    }
    let mut list = Vec::with_capacity(args.len());
    for ja in args.iter_mut() {
        let t = &ja.body.get().t;
        let nv = 8 * t.nvox;
        let cav = cav_rows(&ja.cav)?;
        let pos = rw(&mut ja.pos, "pos")?;
        let vel = rw(&mut ja.vel, "vel")?;
        let prev = rw(&mut ja.prev, "prev")?;
        let force = ro(&ja.force, "force")?;
        need(pos.len() == 3 * nv && vel.len() == 3 * nv && prev.len() == 3 * nv && force.len() == 3 * nv
             && cav.iter().flatten().all(|&j| j < t.nvox as i64) && ja.proxy.get().p.n == t.nvox, "step_tick 構築物")?;
        let [a0, a1, a2, a3, a4, _] = &mut ja.a;
        let [b0, b1, _] = &ja.b;
        let part = match ja.kind {
            0 => {
                let s = tick::SoftPart {
                    rest: take(a0, "rest")?,
                    last_r: take(a1, "last_r")?,
                    quat: take(a2, "quat")?,
                    act_scale: take_ro(b0, "act_scale")?,
                    angles: std::mem::take(&mut ja.v[0]),
                    parallel: ja.parallel,
                };
                need(s.rest.len() == 3 * nv && s.act_scale.len() == 3 * t.nvox && s.last_r.len() == 9 * t.nvox
                     && s.quat.len() == 4 * t.nvox && s.angles.len() == t.rotors.len(), "step_tick ブロックごと")?;
                tick::Part::Soft(s)
            }
            1 | 2 => {
                let r = tick::RigidPart {
                    full: ja.full,
                    rb_r: take(a0, "rb_r")?,
                    rb_x: take(a1, "rb_x")?,
                    rb_ib: take_ro(b0, "rb_ib")?,
                    local: take_ro(b1, "local")?,
                };
                need(r.rb_r.len() == 9 && r.rb_x.len() == 3 && r.rb_ib.len() == 9 && r.local.len() == 3 * nv && nv > 0,
                     "step_tick 剛体")?;
                tick::Part::Rigid(r)
            }
            _ => {
                let at = &ja.art.as_ref().unwrap().get().at;
                let [w0, w1, w2, w3] = &mut ja.v;
                let p = tick::ArtPart {
                    at,
                    x: take(a0, "x")?,
                    r: take(a1, "r")?,
                    vl: take(a2, "vl")?,
                    applied: take(a3, "applied")?,
                    ext_applied: take(a4, "ext_applied")?,
                    vl0: take_ro(b0, "vl0")?,
                    want: std::mem::take(w0),
                    rot_speed: std::mem::take(w1),
                    ext_want: std::mem::take(w2),
                    ext_speed: std::mem::take(w3),
                };
                let (nr, ne) = (at.ra.len(), at.exts.len());
                need(at.vg.len() == nv && p.x.len() == 3 * at.g && p.r.len() == 9 * at.g && p.vl.len() == 3 * nv
                     && p.vl0.len() == 3 * nv && p.applied.len() == nr && p.want.len() == nr && p.rot_speed.len() == nr
                     && p.ext_applied.len() == ne && p.ext_want.len() == ne && p.ext_speed.len() == ne, "step_tick 関節つき剛体")?;
                tick::Part::Art(p)
            }
        };
        list.push(tick::Job {
            t,
            proxy: &ja.proxy.get().p,
            pos,
            vel,
            prev,
            force,
            cav,
            part,
            boxes: ja.boxes.clone(),
            contact_rigid: ja.contact_rigid,
            settle_cap: ja.settle_cap,
        });
    }
    let mut sl = sleepers;
    let mut slist = Vec::with_capacity(sl.len());
    for (px, pos, vel, rigid, fixed, can_wake) in sl.iter_mut() {
        let p = &px.get().p;
        let pv = rw(pos, "pos")?;
        let vl = ro(vel, "vel")?;
        need(pv.len() == 24 * p.n && vl.len() == 24 * p.n, "step_tick 眠っている物")?;
        slist.push(tick::Sleeper { proxy: p, pos: pv, vel: vl, rigid: *rigid, fixed: *fixed, can_wake: *can_wake });
    }
    let o = py.allow_threads(|| tick::run(&mut list, &mut slist, &gs, &pls, scale, h, substeps, iters, impact_speed, soft));
    Ok((o.done, o.woke, o.touch, o.want_soft, o.broken, o.hits))
}

// ---------------------------------------------------------------------------
// 機器の1ティック（devices.update）
// ---------------------------------------------------------------------------
fn dget<'py, T: FromPyObject<'py>>(d: &Bound<'py, PyDict>, k: &str, dflt: T) -> PyResult<T> {
    match d.get_item(k)? {
        Some(v) if !v.is_none() => v.extract(),
        _ => Ok(dflt),
    }
}

/// 構築物1つの機器のまとめ（形が変わるまで変わらない：種類・数値の設定・配管の区画とバルブ）
#[pyclass(frozen)]
struct DevPlan {
    p: devices::Plan,
}

#[pymethods]
impl DevPlan {
    /// devs：機器ごとの dict（vox, type, name, dir, electric, power_use, capacity, output, fuel, fuel_rate, thrust,
    /// power_output, ignite_energy, max_flow, key, recipes [(in, out, time, power)], inputs [(物, 量)], outputs [物],
    /// rate, target_kpa, cavity）。pipe_of {ボクセル: 区画}、pipes [[ボクセル]]、valve_adj [(バルブ, [(バルブか, 番号)])]、
    /// seg_adj {区画: [バルブ]}、fluids [流れる物]、cav_volume [空洞の体積]
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        devs: Vec<Bound<'_, PyDict>>, pipe_of: HashMap<usize, usize>, pipes: Vec<Vec<usize>>,
        valve_adj: Vec<(usize, Vec<(bool, usize)>)>, seg_adj: HashMap<usize, Vec<usize>>, fluids: Vec<String>,
        cav_volume: Vec<f64>,
    ) -> PyResult<Self> {
        let mut out = Vec::with_capacity(devs.len());
        let mut at = HashMap::new();
        for (k, d) in devs.iter().enumerate() {
            let rl: Vec<Bound<'_, PyAny>> = dget(d, "recipes", Vec::new())?;
            let mut rs = Vec::new();
            for r in rl {
                let (i_, o_, t_, p_): (Vec<(String, f64)>, Vec<(String, f64)>, f64, Option<f64>) = r.extract()?;
                rs.push(devices::Recipe { inp: i_, out: o_, time: t_, power: p_ });
            }
            let t: String = dget(d, "type", String::new())?;
            let dv = devices::Dev {
                vox: dget(d, "vox", 0usize)?,
                t: devices::T::of(&t),
                name: dget(d, "name", String::new())?,
                dir: dget(d, "dir", 0i64)?,
                electric: dget(d, "electric", false)?,
                power_use: dget(d, "power_use", 0.0)?,
                capacity: dget(d, "capacity", 0.0)?,
                output: dget(d, "output", 0.0)?,
                fuel: dget(d, "fuel", "fuel".to_string())?,
                fuel_rate: dget(d, "fuel_rate", 0.0)?,
                thrust: dget(d, "thrust", 0.0)?,
                power_output: dget(d, "power_output", 0.0)?,
                ignite_energy: dget(d, "ignite_energy", 0.0)?,
                max_flow: dget(d, "max_flow", 6.0)?,
                key: dget(d, "key", None)?,
                recipes: rs,
                inputs: dget(d, "inputs", Vec::new())?,
                outputs: dget(d, "outputs", Vec::new())?,
                rate: dget(d, "rate", 1.0)?,
                target_kpa: dget(d, "target_kpa", 101.3)?,
                cavity: dget(d, "cavity", None)?,
            };
            need(dv.cavity.map(|c| c < cav_volume.len()).unwrap_or(true), "DevPlan cavity")?;
            at.insert(dv.vox, k);
            out.push(dv);
        }
        need(pipe_of.values().all(|&s| s < pipes.len()), "DevPlan pipes")?;
        let valves: Vec<usize> = valve_adj.iter().map(|x| x.0).collect();
        let va = valve_adj
            .into_iter()
            .map(|(v, l)| (v, l.into_iter().map(|(isv, i)| if isv { devices::Node::V(i) } else { devices::Node::S(i) }).collect()))
            .collect();
        Ok(DevPlan {
            p: devices::Plan { devs: out, at, pipe_of, pipes, valve_adj: va, valves, seg_adj, fluids: fluids.into_iter().collect(), cav_volume },
        })
    }

    /// 1ティック。states：devs と同じ順の state の dict。force・cav_air は書き換える。
    /// 戻り値 (production, demand, stored, capacity, ratio, 電力が来た機器のボクセル, 知らせ)
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn update(
        &self, states: Vec<Bound<'_, PyDict>>, dt: f64, keys: Vec<String>, seated: bool, pos: RoF, mass: RoF, mut force: RwF,
        mut cav_air: RwF,
    ) -> PyResult<(f64, f64, f64, f64, f64, Vec<usize>, Vec<String>)> {
        let p = &self.p;
        need(states.len() == p.devs.len(), "DevPlan.update states")?;
        let (ps, ms) = (ro(&pos, "pos")?, ro(&mass, "mass")?);
        let f = rw(&mut force, "force")?;
        let ca = rw(&mut cav_air, "cav_air")?;
        need(ps.len() == 3 * ms.len() && f.len() == ps.len() && ca.len() == p.cav_volume.len()
             && p.devs.iter().all(|d| 8 * d.vox + 8 <= ms.len()), "DevPlan.update")?;
        let ks = keys.into_iter().collect();
        let r = devices::update(p, &states, dt, &ks, seated, ps, ms, f, ca)?;
        Ok((r.production, r.demand, r.stored, r.capacity, r.ratio, r.powered, r.notes))
    }
}

// ---------------------------------------------------------------------------
// 描画の準備（遠景の箱・記号の四角）
// ---------------------------------------------------------------------------
const BOX_FACES: [[usize; 4]; 6] = [[1, 3, 7, 5], [0, 4, 6, 2], [2, 6, 7, 3], [0, 1, 5, 4], [4, 5, 7, 6], [0, 2, 3, 1]];
const BOX_N: [[f64; 3]; 6] = [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];

fn shade1(n: [f64; 3], c: [f64; 3], sun: [f64; 3], amb: f64) -> [f64; 3] {
    let lam = (n[0] * sun[0] + n[1] * sun[1] + n[2] * sun[2]).clamp(0.0, 1.0);
    let k = amb + (1.0 - amb) * lam;
    [c[0] * k, c[1] * k, c[2] * k]
}

/// 遠景の箱（render.lod_vertices）：格子の座標の箱（中心 C・半分の大きさ H・色）を、R・t でワールドに置き base を引いた
/// 頂点 (箱×24, 12) float32
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn lod_vertices<'py>(
    py: Python<'py>, c: RoF<'py>, hh: RoF<'py>, col: RoF<'py>, r: RoF<'py>, t: [f64; 3], base: [f64; 3], size: f64, sun: [f64; 3],
    ambient: f64,
) -> PyResult<Bound<'py, PyArray2<f32>>> {
    let (cc, hv, co, rr) = (ro(&c, "C")?, ro(&hh, "H")?, ro(&col, "col")?, ro(&r, "R")?);
    let k = cc.len() / 3;
    need(hv.len() == 3 * k && co.len() == 3 * k && rr.len() == 9, "lod_vertices")?;
    let mut out = vec![0f32; k * 24 * 12];
    let rot = |v: [f64; 3]| [rr[0] * v[0] + rr[1] * v[1] + rr[2] * v[2], rr[3] * v[0] + rr[4] * v[1] + rr[5] * v[2],
                             rr[6] * v[0] + rr[7] * v[1] + rr[8] * v[2]];
    let nrm: Vec<[f64; 3]> = BOX_N.iter().map(|&n| rot(n)).collect();
    for b in 0..k {
        let mut w = [[0.0; 3]; 8];
        for (q, wq) in w.iter_mut().enumerate() {
            let bits = [(q & 1) as f64 * 2.0 - 1.0, ((q >> 1) & 1) as f64 * 2.0 - 1.0, ((q >> 2) & 1) as f64 * 2.0 - 1.0];
            let l = [(cc[3 * b] + hv[3 * b] * bits[0]) * size, (cc[3 * b + 1] + hv[3 * b + 1] * bits[1]) * size,
                     (cc[3 * b + 2] + hv[3 * b + 2] * bits[2]) * size];
            let g = rot(l);
            *wq = [g[0] + t[0] - base[0], g[1] + t[1] - base[1], g[2] + t[2] - base[2]];
        }
        let colr = [co[3 * b], co[3 * b + 1], co[3 * b + 2]];
        for (f, face) in BOX_FACES.iter().enumerate() {
            let n = nrm[f];
            let sh = shade1(n, colr, sun, ambient);
            for (j, &vi) in face.iter().enumerate() {
                let o = ((b * 6 + f) * 4 + j) * 12;
                for a in 0..3 {
                    out[o + a] = w[vi][a] as f32;
                    out[o + 3 + a] = n[a] as f32;
                    out[o + 6 + a] = sh[a] as f32;
                }
                out[o + 9] = 1.0;
            }
        }
    }
    Ok(PyArray1::from_vec_bound(py, out).reshape([k * 24, 12])?)
}

/// 記号の四角（render.DecalView._write）：p8 (D,8,3) の頂点、nl・al (D,3) はブロックの向きでの面の法線と上、uv (D,4)。
/// 戻り値 (D×4, 12) float32
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn decal_quads<'py>(
    py: Python<'py>, p8: RoF<'py>, nl: RoF<'py>, al: RoF<'py>, uv: PyReadonlyArrayDyn<'py, f32>, size: f64, origin: [f64; 3],
    sun: [f64; 3], ambient: f64,
) -> PyResult<Bound<'py, PyArray2<f32>>> {
    let (p, n_, a_) = (ro(&p8, "P8")?, ro(&nl, "nl")?, ro(&al, "al")?);
    let u = uv.as_slice().map_err(|_| PyValueError::new_err("uv: 連続した配列ではありません"))?;
    let d = p.len() / 24;
    need(n_.len() == 3 * d && a_.len() == 3 * d && u.len() == 4 * d, "decal_quads")?;
    let mut out = vec![0f32; d * 4 * 12];
    let h = size * 0.31;
    for i in 0..d {
        let q = |k: usize, a: usize| p[24 * i + 3 * k + a];
        let mut cen = [0.0; 3];
        for k in 0..8 {
            for a in 0..3 {
                cen[a] += q(k, a) / 8.0;
            }
        }
        let axis = |hi: [usize; 4], lo: [usize; 4]| {
            let mut v = [0.0; 3];
            for a in 0..3 {
                for j in 0..4 {
                    v[a] += (q(hi[j], a) - q(lo[j], a)) / 4.0;
                }
            }
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
            [v[0] / l, v[1] / l, v[2] / l]
        };
        let f = [axis([1, 3, 5, 7], [0, 2, 4, 6]), axis([2, 3, 6, 7], [0, 1, 4, 5]), axis([4, 5, 6, 7], [0, 1, 2, 3])];
        let mut n = [0.0; 3];
        let mut up = [0.0; 3];
        for k in 0..3 {
            for a in 0..3 {
                n[a] += n_[3 * i + k] * f[k][a];
                up[a] += a_[3 * i + k] * f[k][a];
            }
        }
        let r = body::cross(up, n);
        let pc = [cen[0] + n[0] * (size * 0.5 * 1.012) - origin[0], cen[1] + n[1] * (size * 0.5 * 1.012) - origin[1],
                  cen[2] + n[2] * (size * 0.5 * 1.012) - origin[2]];
        let sgn = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        let uvs = [(u[4 * i], u[4 * i + 1]), (u[4 * i + 2], u[4 * i + 1]), (u[4 * i + 2], u[4 * i + 3]), (u[4 * i], u[4 * i + 3])];
        let sh = shade1(n, [1.0, 1.0, 1.0], sun, ambient);
        for j in 0..4 {
            let o = (i * 4 + j) * 12;
            for a in 0..3 {
                out[o + a] = (pc[a] + r[a] * h * sgn[j].0 + up[a] * h * sgn[j].1) as f32;
                out[o + 3 + a] = n[a] as f32;
                out[o + 6 + a] = sh[a] as f32;
            }
            out[o + 9] = 1.0;
            out[o + 10] = uvs[j].0;
            out[o + 11] = uvs[j].1;
        }
    }
    Ok(PyArray1::from_vec_bound(py, out).reshape([d * 4, 12])?)
}

// ---------------------------------------------------------------------------
// 描画用のメッシュ
// ---------------------------------------------------------------------------
type MeshGroup<'py> = (RoI<'py>, RoF<'py>, PyReadonlyArrayDyn<'py, f32>, RoI<'py>);

/// 構築物の表面メッシュの頂点を、テクスチャのまとまりごとに (k,12) float32 で返す。
/// groups：[(頂点番号, 色 (k,3), UV (k,2) float32, 持ち主のボクセル), ...]、firing：ボクセルごとの噴射（無ければ None）
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (pos, vidx, w, faces, origin, groups, sun, ambient, firing=None))]
fn mesh_pack<'py>(
    py: Python<'py>, pos: RoF<'py>, vidx: RoI<'py>, w: RoF<'py>, faces: RoI<'py>, origin: [f64; 3], groups: Vec<MeshGroup<'py>>,
    sun: [f64; 3], ambient: f64, firing: Option<RoF<'py>>,
) -> PyResult<Vec<Bound<'py, PyArray2<f32>>>> {
    let (p, vi, ww, f) = (ro(&pos, "pos")?, ro(&vidx, "vidx")?, ro(&w, "w")?, ro(&faces, "faces")?);
    let nv = p.len() / 3;
    let m = vi.len() / 8;
    if ww.len() != vi.len() || vi.iter().any(|&i| i < 0 || i as usize >= nv) || f.iter().any(|&i| i < 0 || i as usize >= m) {
        return Err(PyValueError::new_err("mesh_pack: 配列の大きさが合いません"));
    }
    let fr = match &firing {
        None => None,
        Some(a) => Some(ro(a, "firing")?),
    };
    let nvox = nv / 8;
    let mut gs = Vec::with_capacity(groups.len());
    for (a, b, c, d) in &groups {
        let (verts, cols) = (ro(a, "verts")?, ro(b, "cols")?);
        let uv = c.as_slice().map_err(|_| PyValueError::new_err("uv: 連続した配列ではありません"))?;
        let owner = ro(d, "owner")?;
        let k = verts.len();
        if cols.len() != 3 * k || uv.len() != 2 * k || owner.len() != k || verts.iter().any(|&i| i < 0 || i as usize >= m)
            || (fr.is_some() && owner.iter().any(|&o| o < 0 || o as usize >= fr.unwrap().len()))
            || owner.iter().any(|&o| o < 0 || o as usize >= nvox.max(1))
        {
            return Err(PyValueError::new_err("mesh_pack: まとまりの配列の大きさが合いません"));
        }
        gs.push(mesh::Group { verts, cols, uv, owner });
    }
    let outs = py.allow_threads(|| mesh::pack(p, vi, ww, f, origin, &gs, sun, ambient, fr));
    outs.into_iter()
        .map(|o| {
            let k = o.len() / 12;
            Ok(PyArray1::from_vec_bound(py, o).reshape([k, 12])?)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 近い点の組（構築物どうしの押し出し用。scipy の cKDTree の代わり）
// ---------------------------------------------------------------------------
/// 点の集合の中で、距離 r 以内の組 (i < j)。i, j の順に並べて返す。
#[pyfunction]
fn pairs_within<'py>(py: Python<'py>, pts: RoF<'py>, r: f64) -> PyResult<(Bound<'py, PyArray1<i64>>, Bound<'py, PyArray1<i64>>)> {
    let p = ro(&pts, "pts")?;
    let out = py.allow_threads(|| pairs::within_set(p, r));
    let (a, b): (Vec<i64>, Vec<i64>) = out.into_iter().map(|(i, j)| (i as i64, j as i64)).unzip();
    Ok((PyArray1::from_vec_bound(py, a), PyArray1::from_vec_bound(py, b)))
}

/// 2つの点の集合の間で、距離 r 以内の組 (A の番号, B の番号)。
#[pyfunction]
fn pairs_between<'py>(py: Python<'py>, a: RoF<'py>, b: RoF<'py>, r: f64) -> PyResult<(Bound<'py, PyArray1<i64>>, Bound<'py, PyArray1<i64>>)> {
    let (pa, pb) = (ro(&a, "a")?, ro(&b, "b")?);
    let out = py.allow_threads(|| pairs::between_sets(pa, pb, r));
    let (x, y): (Vec<i64>, Vec<i64>) = out.into_iter().map(|(i, j)| (i as i64, j as i64)).unzip();
    Ok((PyArray1::from_vec_bound(py, x), PyArray1::from_vec_bound(py, y)))
}

/// 読み込めたかの確認用
/// 等値面（Marching Cubes）：vol (nx, ny, nz) の level の面。戻り値 (頂点 (n,3)、三角形 (m,3))。
#[pyfunction]
#[pyo3(signature = (vol, level=0.0, spacing=(1.0, 1.0, 1.0)))]
fn marching_cubes<'py>(py: Python<'py>, vol: PyReadonlyArrayDyn<'py, f32>, level: f32, spacing: (f64, f64, f64))
    -> PyResult<(Bound<'py, PyArray2<f64>>, Bound<'py, PyArray2<i64>>)> {
    let sh = vol.shape().to_vec();
    if sh.len() != 3 {
        return Err(PyValueError::new_err("marching_cubes: 3次元の配列にしてください"));
    }
    let data = vol.as_slice().map_err(|_| PyValueError::new_err("marching_cubes: 連続した配列ではありません"))?.to_vec();
    let dims = [sh[0], sh[1], sh[2]];
    let (v, f) = py.allow_threads(|| geomx::marching_cubes(&data, dims, level, [spacing.0, spacing.1, spacing.2]));
    let nv = v.len();
    let nf = f.len();
    let va = PyArray1::from_vec_bound(py, v.into_iter().flatten().collect()).reshape([nv, 3])?;
    let fa = PyArray1::from_vec_bound(py, f.into_iter().flatten().collect()).reshape([nf, 3])?;
    Ok((va, fa))
}

/// 空いている所（solid が偽）を6近傍のつながりごとに番号付け（1 から）。戻り値 (番号の配列（形は solid と同じ）, 個数)。
#[pyfunction]
fn label_empty<'py>(py: Python<'py>, solid: PyReadonlyArrayDyn<'py, bool>) -> PyResult<(Bound<'py, PyArray1<i32>>, i32)> {
    let sh = solid.shape().to_vec();
    if sh.len() != 3 {
        return Err(PyValueError::new_err("label_empty: 3次元の配列にしてください"));
    }
    let data = solid.as_slice().map_err(|_| PyValueError::new_err("label_empty: 連続した配列ではありません"))?.to_vec();
    let (lab, n) = py.allow_threads(|| geomx::label_empty(&data, [sh[0], sh[1], sh[2]]));
    Ok((PyArray1::from_vec_bound(py, lab), n))
}

fn pts3(a: &PyReadonlyArray2<'_, f64>, name: &str) -> PyResult<Vec<[f64; 3]>> {
    let v = a.as_array();
    if v.ncols() != 3 {
        return Err(PyValueError::new_err(format!("{name}: (n,3) にしてください")));
    }
    Ok(v.rows().into_iter().map(|r| [r[0], r[1], r[2]]).collect())
}

/// 点ごとに、いちばん近い候補の番号。
#[pyfunction]
fn nearest<'py>(py: Python<'py>, cands: PyReadonlyArray2<'py, f64>, pts: PyReadonlyArray2<'py, f64>) -> PyResult<Bound<'py, PyArray1<i64>>> {
    let c = pts3(&cands, "nearest")?;
    let p = pts3(&pts, "nearest")?;
    let r = py.allow_threads(|| geomx::nearest(&c, &p));
    Ok(PyArray1::from_vec_bound(py, r))
}

/// 小さな点の集まりの凸包の三角形 (m,3)。
#[pyfunction]
fn convex_hull<'py>(py: Python<'py>, pts: PyReadonlyArray2<'py, f64>) -> PyResult<Bound<'py, PyArray2<i64>>> {
    let p = pts3(&pts, "convex_hull")?;
    let t = geomx::convex_hull(&p);
    let n = t.len();
    Ok(PyArray1::from_vec_bound(py, t.into_iter().flatten().collect()).reshape([n, 3])?)
}

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
    m.add("HINGE", true)?; // ヒンジの関節（hk）を解ける版
    m.add("ART_FIXED", true)?; // アンカーで固定された部分のある関節つき剛体（fixed・xfix）を解ける版
    m.add_function(wrap_pyfunction!(art_solve, m)?)?;
    m.add_function(wrap_pyfunction!(art_contact_vel, m)?)?;
    m.add_function(wrap_pyfunction!(art_write, m)?)?;
    m.add_function(wrap_pyfunction!(gravity, m)?)?;
    m.add_function(wrap_pyfunction!(pairs_within, m)?)?;
    m.add_function(wrap_pyfunction!(pairs_between, m)?)?;
    m.add_class::<PlanetShape>()?;
    m.add_class::<Body>()?;
    m.add_function(wrap_pyfunction!(rigid_end, m)?)?;
    m.add_function(wrap_pyfunction!(nnls, m)?)?;
    m.add_function(wrap_pyfunction!(marching_cubes, m)?)?;
    m.add_function(wrap_pyfunction!(label_empty, m)?)?;
    m.add_function(wrap_pyfunction!(nearest, m)?)?;
    m.add_function(wrap_pyfunction!(convex_hull, m)?)?;
    m.add_function(wrap_pyfunction!(mesh_pack, m)?)?;
    m.add_class::<Proxy>()?;
    m.add_function(wrap_pyfunction!(step_tick, m)?)?;
    m.add_class::<DevPlan>()?;
    m.add_function(wrap_pyfunction!(lod_vertices, m)?)?;
    m.add_function(wrap_pyfunction!(decal_quads, m)?)?;
    m.add_class::<ArtTopo>()?;
    m.add_function(wrap_pyfunction!(push_apart, m)?)?;
    Ok(())
}
