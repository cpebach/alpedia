//! 構築物1つの物理の1サブステップ（construct.py の substep_begin / substep_end と同じ計算）。
//!
//! 状態（頂点の位置・速度・基準形など）は Python の numpy 配列のまま持ち、ここではその場で書き換える。
//! 結合・回転ジョイント・質量などの「形が変わるまで変わらない値」は Topo にまとめて持つ（Python の rebuild ごとに作り直す）。
//! 街などの MOD が出す当たり判定の箱は Python 側で処理する（MOD のプログラムを呼ぶ必要があるため）。

use std::sync::Arc;

use nalgebra::{DMatrix, DVector, Matrix3, Vector3};
use rayon::prelude::*;

use crate::kernels;
use crate::planet::Shape;

/// 惑星1つの、このティックでの値
pub struct PlanetCtx {
    pub shape: Arc<Shape>,
    pub center: [f64; 3],
    pub craters: Vec<[f64; 4]>,
    pub bound: f64,
    pub lipschitz: f64,
    pub gm: f64,
    pub radius: f64,
    pub has_water: bool,
}

impl PlanetCtx {
    fn local(&self, p: [f64; 3]) -> [f64; 3] {
        [p[0] - self.center[0], p[1] - self.center[1], p[2] - self.center[2]]
    }

    /// 水深（水面からの深さ。水の外は -1e9）
    pub fn water_depth(&self, p: [f64; 3]) -> f64 {
        let q = self.local(p);
        let r = dot(q, q).sqrt();
        let ri = r.max(1e-9);
        let s = self.shape.sample([q[0] / ri, q[1] / ri, q[2] / ri]);
        if s[1] > s[0] {
            s[1] - r
        } else {
            -1e9
        }
    }
}

/// 回転ジョイント（アクチュエーターの出力面）
pub struct Rotor {
    pub a_vox: usize,
    pub a: [usize; 4],
    pub b: [usize; 4],
    pub br: f64,
}

/// 形が変わるまで変わらない値
pub struct Topo {
    pub size: f64,
    pub nvox: usize,
    pub ja: Vec<i64>,
    pub jb: Vec<i64>,
    pub jr: Vec<f64>,
    pub jk: Vec<f64>,
    pub jbrk: Vec<f64>,
    pub jcount: Vec<f64>,
    pub inv_mass: Vec<f64>,
    pub mass: Vec<f64>,
    pub damp: Vec<f64>,
    pub fric: Vec<f64>,
    pub stiff: Vec<f64>,
    pub plast: Vec<f64>,
    pub yield_: Vec<f64>,
    pub density: Vec<f64>,
    pub ctype: Vec<i64>,
    pub cidx: Vec<i64>,
    pub rotors: Vec<Rotor>,
}

#[inline]
pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[inline]
fn v3(s: &[f64], i: usize) -> [f64; 3] {
    [s[3 * i], s[3 * i + 1], s[3 * i + 2]]
}

/// 全部の惑星の重力の和（world.gravity と同じ。scale は重力倍率）
pub fn gravity(planets: &[PlanetCtx], p: [f64; 3], scale: f64) -> [f64; 3] {
    let mut g = [0.0; 3];
    for pl in planets {
        let q = [pl.center[0] - p[0], pl.center[1] - p[1], pl.center[2] - p[2]];
        let r2 = dot(q, q).max((pl.radius * 0.5).powi(2));
        let s = pl.gm / (r2 * r2.sqrt());
        for a in 0..3 {
            g[a] += q[a] * s;
        }
    }
    [g[0] * scale, g[1] * scale, g[2] * scale]
}

/// 構築物が惑星の近くにあるか（construct._near_planet）
fn near_planet(pos: &[f64], nvox: usize, size: f64, pl: &PlanetCtx) -> bool {
    let lim = (pl.bound + size * 2.0).powi(2);
    let mut m = f64::INFINITY;
    for i in 0..nvox {
        let q = pl.local(v3(pos, 8 * i));
        m = m.min(dot(q, q));
    }
    m < lim
}

/// 地面との接触（construct._collide_planet）。戻り値 (速くぶつかりそうか, [(頂点, 距離-r, 法線)] 当たったものだけ)
#[allow(clippy::too_many_arguments)]
pub fn collide_planet(
    p: &mut [f64], pv: &[f64], vl: &[f64], k: &[f64], fr: &[f64], r: f64, pl: &PlanetCtx, up: Option<[f64; 3]>, rs: f64,
    rigid: bool, impact_speed: f64, soft: f64, apply: bool,
) -> (bool, Vec<(usize, f64, [f64; 3])>) {
    let nv = p.len() / 3;
    let c = pl.center;
    let lim2 = (pl.bound + r) * (pl.bound + r);
    let lip = pl.lipschitz.max(0.2);
    let near: Vec<usize> = (0..nv)
        .filter(|&i| {
            let rel = [k[3 * i] - c[0], k[3 * i + 1] - c[1], k[3 * i + 2] - c[2]];
            let r2 = dot(rel, rel);
            if !(r2 < lim2) {
                return false; // NaN（内部の頂点）もここで外れる
            }
            match up {
                None => true,
                Some(u) => {
                    let rad = r2.sqrt();
                    let along = dot(rel, u);
                    let lat = (rad * rad - along * along).max(0.0).sqrt();
                    (rad - rs) < r + 0.5 + lat / lip
                }
            }
        })
        .collect();
    let eval = |&i: &usize| {
        let q = [k[3 * i] - c[0], k[3 * i + 1] - c[1], k[3 * i + 2] - c[2]];
        let (d, g) = pl.shape.sdf_grad(q, &pl.craters, 0.05);
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
            let dn = dot(disp, n);
            let f = fr[i].clamp(0.0, 1.0);
            for a in 0..3 {
                p[3 * i + a] -= (disp[a] - n[a] * dn) * f;
            }
        }
    }
    (want_soft, hit)
}

/// 重心から見て「上」と、その方向の地表半径（地面から高い頂点を省くため）
fn ground_ref(pos: &[f64], pl: &PlanetCtx) -> (Option<[f64; 3]>, f64) {
    let nv = pos.len() / 3;
    let mut m = [0.0; 3];
    for i in 0..nv {
        for a in 0..3 {
            m[a] += pos[3 * i + a];
        }
    }
    let com = pl.local([m[0] / nv as f64, m[1] / nv as f64, m[2] / nv as f64]);
    let cn = dot(com, com).sqrt();
    if cn <= 1e-6 {
        return (None, 0.0);
    }
    let up = [com[0] / cn, com[1] / cn, com[2] / cn];
    (Some(up), pl.shape.sample(up)[0])
}

fn cores(pos: &[f64], t: &Topo) -> Vec<f64> {
    let mut k = vec![0.0; pos.len()];
    kernels::contact_cores(pos, &t.ctype, &t.cidx, &mut k);
    k
}

/// 水の浮力と抵抗（construct._buoyancy）。cav は気密の空洞セルの6方向のボクセル番号（-1 は無し）
fn buoyancy(pos: &[f64], vel: &mut [f64], t: &Topo, pl: &PlanetCtx, planets: &[PlanetCtx], scale: f64, h: f64, cav: &[[i64; 6]]) {
    let n = t.nvox;
    let size = t.size;
    let cen: Vec<[f64; 3]> = (0..n)
        .map(|i| {
            let mut c = [0.0; 3];
            for k in 0..8 {
                for a in 0..3 {
                    c[a] += pos[3 * (8 * i + k) + a];
                }
            }
            [c[0] / 8.0, c[1] / 8.0, c[2] / 8.0]
        })
        .collect();
    let wd: Vec<f64> = cen.iter().map(|&c| pl.water_depth(c)).collect();
    let frac: Vec<f64> = wd.iter().map(|&w| (w / size + 0.5).clamp(0.0, 1.0)).collect();
    let wmax = wd.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if !frac.iter().any(|&f| f > 0.0) && wmax < -2.0 * size {
        return;
    }
    let vol = size.powi(3);
    let mut force: Vec<[f64; 3]> = (0..n)
        .map(|i| {
            let g = gravity(planets, cen[i], scale);
            let s = 1000.0 * vol * frac[i];
            [-g[0] * s, -g[1] * s, -g[2] * s]
        })
        .collect();
    if !cav.is_empty() {
        let mut ccs = Vec::with_capacity(cav.len());
        for row in cav {
            let (mut c, mut cnt) = ([0.0; 3], 0.0);
            for &j in row {
                if j >= 0 {
                    for a in 0..3 {
                        c[a] += cen[j as usize][a];
                    }
                    cnt += 1.0;
                }
            }
            ccs.push(([c[0] / cnt, c[1] / cnt, c[2] / cnt], cnt));
        }
        let f2: Vec<f64> = ccs.iter().map(|(c, _)| (pl.water_depth(*c) / size + 0.5).clamp(0.0, 1.0)).collect();
        if f2.iter().any(|&f| f > 0.0) {
            for (k, row) in cav.iter().enumerate() {
                let (c, cnt) = ccs[k];
                let g = gravity(planets, c, scale);
                let s = 1000.0 * vol * f2[k] / cnt;
                for &j in row {
                    if j >= 0 {
                        for a in 0..3 {
                            force[j as usize][a] += -g[a] * s;
                        }
                    }
                }
            }
        }
    }
    let total: f64 = force.iter().map(|f| f[0].abs() + f[1].abs() + f[2].abs()).sum();
    if !(total > 0.0) {
        return;
    }
    for i in 0..n {
        let m = t.density[i] * vol;
        let drag = 1.0 - (1.5 * h * frac[i]).min(1.0);
        for k in 0..8 {
            for a in 0..3 {
                let v = &mut vel[3 * (8 * i + k) + a];
                *v += force[i][a] / m * h;
                *v *= drag;
            }
        }
    }
}

/// 重力・外力・浮力で速度を進める（ブロックごと・剛体の両方の最初の部分）
#[allow(clippy::too_many_arguments)]
fn integrate_forces(
    pos: &[f64], vel: &mut [f64], force: &[f64], t: &Topo, planets: &[PlanetCtx], scale: f64, h: f64, cav: &[[i64; 6]],
) {
    let nv = pos.len() / 3;
    for i in 0..nv {
        let g = gravity(planets, v3(pos, i), scale);
        for a in 0..3 {
            vel[3 * i + a] += (g[a] + force[3 * i + a] * t.inv_mass[i]) * h;
        }
    }
    for pl in planets {
        if pl.has_water && near_planet(pos, t.nvox, t.size, pl) {
            buoyancy(pos, vel, t, pl, planets, scale, h, cav);
        }
    }
}

/// 回転ジョイントの目標の位置（construct._rotor_targets）
fn rotor_target(pos: &[f64], r: &Rotor, angle: f64) -> [[f64; 3]; 4] {
    let pa: Vec<[f64; 3]> = r.a.iter().map(|&i| v3(pos, i)).collect();
    let mut c = [0.0; 3];
    for p in &pa {
        for a in 0..3 {
            c[a] += p[a];
        }
    }
    for x in c.iter_mut() {
        *x /= 4.0;
    }
    let mut va = [0.0; 3];
    for k in 0..8 {
        for a in 0..3 {
            va[a] += pos[3 * (8 * r.a_vox + k) + a];
        }
    }
    let mut n = [c[0] - va[0] / 8.0, c[1] - va[1] / 8.0, c[2] - va[2] / 8.0];
    let nl = dot(n, n).sqrt().max(1e-9);
    for x in n.iter_mut() {
        *x /= nl;
    }
    let (cs, sn) = (angle.cos(), angle.sin());
    let mut out = [[0.0; 3]; 4];
    for k in 0..4 {
        let v = [pa[k][0] - c[0], pa[k][1] - c[1], pa[k][2] - c[2]];
        let cr = cross(n, v);
        let vn = dot(v, n);
        for a in 0..3 {
            out[k][a] = c[a] + v[a] * cs + cr[a] * sn + n[a] * vn * (1.0 - cs);
        }
    }
    out
}

fn solve_rotors(pos: &mut [f64], t: &Topo, angles: &[f64]) {
    for (q, r) in t.rotors.iter().enumerate() {
        let tg = rotor_target(pos, r, angles[q]);
        let mut err = [[0.0; 3]; 4];
        let mut mean = [0.0; 3];
        for k in 0..4 {
            for a in 0..3 {
                err[k][a] = pos[3 * r.b[k] + a] - tg[k][a];
                mean[a] += err[k][a] / 4.0;
            }
        }
        let (wa, wb) = (t.inv_mass[r.a[0]], t.inv_mass[r.b[0]]);
        let ka = wa / (wa + wb);
        for k in 0..4 {
            for a in 0..3 {
                pos[3 * r.b[k] + a] -= (err[k][a] - mean[a]) + mean[a] * (1.0 - ka);
            }
        }
        for k in 0..4 {
            for a in 0..3 {
                pos[3 * r.a[k] + a] += mean[a] * ka;
            }
        }
    }
}

/// ブロックごとの計算の1サブステップの前半（construct.substep_begin の soft の部分。MOD の箱は除く）
#[allow(clippy::too_many_arguments)]
pub fn soft_begin(
    pos: &mut [f64], vel: &mut [f64], prev: &mut [f64], force: &[f64], rest: &[f64], act_scale: &[f64], last_r: &mut [f64],
    quat: &mut [f64], t: &Topo, planets: &[PlanetCtx], scale: f64, h: f64, iters: i64, angles: &[f64], parallel: bool,
    cav: &[[i64; 6]], soft: f64,
) {
    integrate_forces(pos, vel, force, t, planets, scale, h, cav);
    let nv = pos.len() / 3;
    // 減衰は構築物全体の並進運動にはかけない
    let msum: f64 = t.mass.iter().sum();
    let mut vm = [0.0; 3];
    for i in 0..nv {
        for a in 0..3 {
            vm[a] += vel[3 * i + a] * t.mass[i];
        }
    }
    for x in vm.iter_mut() {
        *x /= msum;
    }
    for i in 0..nv {
        let f = (1.0 - t.damp[i] * h).max(0.0);
        for a in 0..3 {
            vel[3 * i + a] = vm[a] + (vel[3 * i + a] - vm[a]) * f;
        }
    }
    prev.copy_from_slice(pos);
    for i in 0..3 * nv {
        pos[i] += vel[i] * h;
    }
    let iters = if t.rotors.is_empty() { iters } else { iters * 2 };
    let near: Vec<&PlanetCtx> = planets.iter().filter(|pl| near_planet(pos, t.nvox, t.size, pl)).collect();
    let vox: Vec<i64> = (0..t.nvox as i64).collect();
    for _ in 0..iters {
        kernels::shape_match(pos, rest, act_scale, &t.stiff, &vox, last_r, quat, parallel);
        if !t.ja.is_empty() {
            kernels::solve_joints(pos, &t.ja, &t.jb, &t.jr, &t.jk, &t.inv_mass, &t.jcount);
        }
        if !t.rotors.is_empty() {
            solve_rotors(pos, t, angles);
        }
    }
    let r = t.size * 0.5;
    for pl in near {
        let k = cores(pos, t);
        let (up, rs) = ground_ref(pos, pl);
        collide_planet(pos, prev, vel, &k, &t.fric, r, pl, up, rs, false, 0.0, soft, true);
    }
}

/// ブロックごとの計算の1サブステップの後半：速度・塑性変形・ちぎれの判定。
/// 戻り値 (ちぎれた結合の番号, ちぎれた回転ジョイントの番号)
#[allow(clippy::too_many_arguments)]
pub fn soft_end(
    pos: &[f64], vel: &mut [f64], prev: &[f64], rest: &mut [f64], act_scale: &[f64], last_r: &[f64], t: &Topo, h: f64,
    angles: &[f64],
) -> (Vec<i64>, Vec<i64>) {
    for i in 0..pos.len() {
        vel[i] = (pos[i] - prev[i]) / h;
    }
    plasticity(pos, rest, act_scale, last_r, t, h);
    let mut rot = Vec::new();
    for (q, r) in t.rotors.iter().enumerate() {
        let tg = rotor_target(pos, r, angles[q]);
        let mut m: f64 = 0.0;
        for k in 0..4 {
            let d = [pos[3 * r.b[k]] - tg[k][0], pos[3 * r.b[k] + 1] - tg[k][1], pos[3 * r.b[k] + 2] - tg[k][2]];
            m = m.max(dot(d, d).sqrt());
        }
        if m > r.br {
            rot.push(q as i64);
        }
    }
    let mut br = Vec::new();
    for j in 0..t.ja.len() {
        let (a, b) = (t.ja[j] as usize, t.jb[j] as usize);
        let d = [pos[3 * b] - pos[3 * a], pos[3 * b + 1] - pos[3 * a + 1], pos[3 * b + 2] - pos[3 * a + 2]];
        if dot(d, d).sqrt() > t.jbrk[j] {
            br.push(j as i64);
        }
    }
    (br, rot)
}

/// 曲がったまま戻らない変形（construct._plasticity）
fn plasticity(pos: &[f64], rest: &mut [f64], act_scale: &[f64], last_r: &[f64], t: &Topo, h: f64) {
    if !t.plast.iter().any(|&p| p > 0.0) || last_r.len() != 9 * t.nvox {
        return;
    }
    let mut any = false;
    let mut upd: Vec<(usize, [[f64; 3]; 8], f64)> = Vec::new();
    for i in 0..t.nvox {
        if !(t.plast[i] > 0.0) {
            continue;
        }
        let mut c = [0.0; 3];
        for k in 0..8 {
            for a in 0..3 {
                c[a] += pos[3 * (8 * i + k) + a];
            }
        }
        for x in c.iter_mut() {
            *x /= 8.0;
        }
        let rr = &last_r[9 * i..9 * i + 9];
        let mut local = [[0.0; 3]; 8];
        let mut lm = [0.0; 3];
        for k in 0..8 {
            let x = [pos[3 * (8 * i + k)] - c[0], pos[3 * (8 * i + k) + 1] - c[1], pos[3 * (8 * i + k) + 2] - c[2]];
            for a in 0..3 {
                // R^T x（R は行優先：R[j][a] = rr[3j+a]）
                local[k][a] = (rr[a] * x[0] + rr[3 + a] * x[1] + rr[6 + a] * x[2]) / act_scale[3 * i + a];
                lm[a] += local[k][a];
            }
        }
        let mut diff = [[0.0; 3]; 8];
        let mut strain: f64 = 0.0;
        for k in 0..8 {
            for a in 0..3 {
                diff[k][a] = local[k][a] - lm[a] / 8.0 - rest[24 * i + 3 * k + a];
            }
            strain = strain.max(dot(diff[k], diff[k]).sqrt());
        }
        if strain / t.size > t.yield_[i] {
            any = true;
            upd.push((i, diff, (t.plast[i] * 20.0 * h).clamp(0.0, 1.0)));
        }
    }
    if !any {
        return;
    }
    for (i, diff, rate) in upd {
        for k in 0..8 {
            for a in 0..3 {
                rest[24 * i + 3 * k + a] += diff[k][a] * rate;
            }
        }
    }
    let lim = t.size * 1.2;
    for x in rest.iter_mut() {
        *x = x.clamp(-lim, lim);
    }
}

// ---------------------------------------------------------------------------
// 剛体として動いている構築物
// ---------------------------------------------------------------------------
fn mass_center(pos: &[f64], mass: &[f64]) -> ([f64; 3], f64) {
    let mut x = [0.0; 3];
    let mut msum = 0.0;
    for i in 0..mass.len() {
        for a in 0..3 {
            x[a] += pos[3 * i + a] * mass[i];
        }
        msum += mass[i];
    }
    ([x[0] / msum, x[1] / msum, x[2] / msum], msum)
}

fn m3(r: &[f64]) -> Matrix3<f64> {
    Matrix3::new(r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7], r[8])
}

/// 剛体の1サブステップの前半（construct._rigid_begin）。full なら完全剛体モードの接触（最小二乗）。
/// 戻り値：速くぶつかりそうか
#[allow(clippy::too_many_arguments)]
pub fn rigid_begin(
    pos: &mut [f64], vel: &mut [f64], prev: &mut [f64], force: &[f64], rb_r: &[f64], rb_ib: &[f64], t: &Topo,
    planets: &[PlanetCtx], scale: f64, h: f64, cav: &[[i64; 6]], full: bool, impact_speed: f64, soft: f64,
) -> bool {
    integrate_forces(pos, vel, force, t, planets, scale, h, cav);
    let nv = pos.len() / 3;
    let (x, msum) = mass_center(pos, &t.mass);
    let (v, _) = mass_center(vel, &t.mass);
    let mut l = [0.0; 3];
    for i in 0..nv {
        let r = [pos[3 * i] - x[0], pos[3 * i + 1] - x[1], pos[3 * i + 2] - x[2]];
        let dv = [vel[3 * i] - v[0], vel[3 * i + 1] - v[1], vel[3 * i + 2] - v[2]];
        let c = cross(r, dv);
        for a in 0..3 {
            l[a] += c[a] * t.mass[i];
        }
    }
    let _ = msum;
    let rm = m3(rb_r);
    let iw = rm * m3(rb_ib) * rm.transpose();
    let reg = Matrix3::identity() * (1e-9 * iw.trace().max(1e-9));
    let w = (iw + reg).lu().solve(&Vector3::new(l[0], l[1], l[2])).unwrap_or_else(Vector3::zeros);
    let w = [w[0], w[1], w[2]];
    for i in 0..nv {
        let r = [pos[3 * i] - x[0], pos[3 * i + 1] - x[1], pos[3 * i + 2] - x[2]];
        let c = cross(w, r);
        for a in 0..3 {
            vel[3 * i + a] = v[a] + c[a];
        }
    }
    prev.copy_from_slice(pos);
    for i in 0..3 * nv {
        pos[i] += vel[i] * h;
    }
    let near: Vec<&PlanetCtx> = planets.iter().filter(|pl| near_planet(pos, t.nvox, t.size, pl)).collect();
    let r = t.size * 0.5;
    let mut want = false;
    if full {
        let mut pts: Vec<(usize, f64, [f64; 3])> = Vec::new();
        for pl in &near {
            let k = cores(pos, t);
            let (up, rs) = ground_ref(pos, pl);
            let (ws, hit) = collide_planet(pos, prev, vel, &k, &t.fric, r, pl, up, rs, true, impact_speed, soft, false);
            want |= ws;
            pts.extend(hit.into_iter().map(|(i, d, n)| (i, -d, n)));
        }
        if !pts.is_empty() {
            rigid_contacts(pos, prev, t, pts, h);
        }
        return want;
    }
    for pl in near {
        let k = cores(pos, t);
        let (up, rs) = ground_ref(pos, pl);
        let (ws, _) = collide_planet(pos, prev, vel, &k, &t.fric, r, pl, up, rs, true, impact_speed, soft, true);
        want |= ws;
    }
    want
}

/// 完全剛体モードの地面との接触（construct._rigid_ground_contacts の惑星の部分）
fn rigid_contacts(pos: &mut [f64], prev: &[f64], t: &Topo, mut pts: Vec<(usize, f64, [f64; 3])>, h: f64) {
    let _ = h;
    // 深い所から 32 点まで
    let mut order: Vec<usize> = (0..pts.len()).collect();
    order.sort_by(|&a, &b| pts[b].1.partial_cmp(&pts[a].1).unwrap_or(std::cmp::Ordering::Equal));
    order.truncate(32);
    pts = order.into_iter().map(|i| pts[i]).collect();
    let nv = pos.len() / 3;
    let (x, msum) = mass_center(pos, &t.mass);
    let mut ii: Matrix3<f64> = Matrix3::zeros();
    let mut trace_part = 0.0;
    for i in 0..nv {
        let r = [pos[3 * i] - x[0], pos[3 * i + 1] - x[1], pos[3 * i + 2] - x[2]];
        trace_part += t.mass[i] * dot(r, r);
        for a in 0..3 {
            for b in 0..3 {
                ii[(a, b)] -= t.mass[i] * r[a] * r[b];
            }
        }
    }
    for a in 0..3 {
        ii[(a, a)] += trace_part;
    }
    let reg = Matrix3::identity() * (1e-6 * ii.trace().max(1e-6));
    let iinv = (ii + reg).try_inverse().unwrap_or_else(Matrix3::zeros);
    let n = pts.len();
    let rk: Vec<Vector3<f64>> = pts
        .iter()
        .map(|&(i, _, _)| Vector3::new(pos[3 * i] - x[0], pos[3 * i + 1] - x[1], pos[3 * i + 2] - x[2]))
        .collect();
    let nn: Vec<Vector3<f64>> = pts.iter().map(|p| Vector3::new(p.2[0], p.2[1], p.2[2])).collect();
    let rn: Vec<Vector3<f64>> = (0..n).map(|k| rk[k].cross(&nn[k])).collect();
    let mut a = DMatrix::zeros(n, n);
    for i in 0..n {
        for j in 0..n {
            a[(i, j)] = nn[i].dot(&nn[j]) / msum + rn[i].dot(&(iinv * rn[j]));
        }
    }
    let d = DVector::from_iterator(n, pts.iter().map(|p| p.1));
    let lam = nnls(&a, &d, 200).unwrap_or_else(|| {
        let mut l = DVector::zeros(n);
        let wk = 1.0 / msum + rn[0].dot(&(iinv * rn[0]));
        l[0] = d[0] / wk;
        l
    });
    let mut dx = Vector3::zeros();
    let mut dth = Vector3::zeros();
    for k in 0..n {
        dx += nn[k] * lam[k];
        dth += rn[k] * lam[k];
    }
    dx /= msum;
    dth = iinv * dth;
    // 摩擦
    let base: Vec<Vector3<f64>> = pts
        .iter()
        .map(|&(i, _, _)| Vector3::new(pos[3 * i] - prev[3 * i], pos[3 * i + 1] - prev[3 * i + 1], pos[3 * i + 2] - prev[3 * i + 2]))
        .collect();
    let fr: Vec<f64> = pts.iter().map(|&(i, _, _)| t.fric[i].clamp(0.0, 1.0)).collect();
    for _ in 0..2 {
        for k in 0..n {
            if lam[k] <= 0.0 {
                continue;
            }
            let disp = base[k] + dx + dth.cross(&rk[k]);
            let tt = disp - nn[k] * nn[k].dot(&disp);
            let tl = tt.norm();
            if tl < 1e-9 {
                continue;
            }
            let tn = tt / tl;
            let rt = rk[k].cross(&tn);
            let wt = 1.0 / msum + rt.dot(&(iinv * rt));
            let l = (tl * fr[k] / wt).min(lam[k] * 1.5);
            dx -= tn * (l / msum);
            dth -= iinv * rt * l;
        }
    }
    let th = dth.norm();
    let rw = if th > 1e-12 {
        let k = dth / th;
        let kx = Matrix3::new(0.0, -k[2], k[1], k[2], 0.0, -k[0], -k[1], k[0], 0.0);
        Matrix3::identity() + kx * th.sin() + kx * kx * (1.0 - th.cos())
    } else {
        Matrix3::identity()
    };
    for i in 0..nv {
        let r = Vector3::new(pos[3 * i] - x[0], pos[3 * i + 1] - x[1], pos[3 * i + 2] - x[2]);
        let r2 = rw * r;
        for a in 0..3 {
            pos[3 * i + a] = x[a] + dx[a] + r2[a];
        }
    }
}

/// 非負の最小二乗（Lawson–Hanson）：min |A x - b|, x >= 0
pub fn nnls(a: &DMatrix<f64>, b: &DVector<f64>, maxiter: usize) -> Option<DVector<f64>> {
    let n = a.ncols();
    let mut x = DVector::zeros(n);
    let mut passive = vec![false; n];
    let tol = 10.0 * f64::EPSILON * a.norm() * (n.max(1) as f64);
    let solve = |passive: &[bool]| -> Option<DVector<f64>> {
        let idx: Vec<usize> = (0..n).filter(|&i| passive[i]).collect();
        let sub = a.select_columns(&idx);
        let z = sub.clone().svd(true, true).solve(b, 1e-14).ok()?;
        let mut full = DVector::zeros(n);
        for (k, &i) in idx.iter().enumerate() {
            full[i] = z[k];
        }
        Some(full)
    };
    let mut it = 0;
    loop {
        let w = a.transpose() * (b - a * &x);
        let mut best = None;
        let mut bw = tol;
        for i in 0..n {
            if !passive[i] && w[i] > bw {
                bw = w[i];
                best = Some(i);
            }
        }
        let Some(j) = best else { break };
        passive[j] = true;
        loop {
            it += 1;
            if it > maxiter * n.max(1) {
                return Some(x);
            }
            let z = solve(&passive)?;
            if (0..n).all(|i| !passive[i] || z[i] > 0.0) {
                x = z;
                break;
            }
            let mut alpha = f64::INFINITY;
            for i in 0..n {
                if passive[i] && z[i] <= 0.0 {
                    let den = x[i] - z[i];
                    if den > 0.0 {
                        alpha = alpha.min(x[i] / den);
                    }
                }
            }
            if !alpha.is_finite() {
                alpha = 0.0;
            }
            x = &x + (&z - &x) * alpha;
            for i in 0..n {
                if passive[i] && x[i] <= tol {
                    passive[i] = false;
                    x[i] = 0.0;
                }
            }
        }
    }
    Some(x)
}

/// 剛体の形に戻す（construct._rigid_fit_pos）と速度。戻り値 (R 行優先9個, 重心)
pub fn rigid_end(pos: &mut [f64], vel: &mut [f64], prev: &[f64], local: &[f64], mass: &[f64], h: f64) -> ([f64; 9], [f64; 3]) {
    let nv = pos.len() / 3;
    let (x, _) = mass_center(pos, mass);
    let mut hm: Matrix3<f64> = Matrix3::zeros();
    for i in 0..nv {
        for a in 0..3 {
            for b in 0..3 {
                hm[(a, b)] += local[3 * i + a] * mass[i] * (pos[3 * i + b] - x[b]);
            }
        }
    }
    let svd = hm.svd(true, true);
    let u = svd.u.unwrap();
    let mut vt = svd.v_t.unwrap();
    let mut r = vt.transpose() * u.transpose();
    if r.determinant() < 0.0 {
        let s = svd.singular_values;
        let mut k = 0;
        for i in 1..3 {
            if s[i] < s[k] {
                k = i;
            }
        }
        for c in 0..3 {
            vt[(k, c)] = -vt[(k, c)];
        }
        r = vt.transpose() * u.transpose();
    }
    for i in 0..nv {
        let l = Vector3::new(local[3 * i], local[3 * i + 1], local[3 * i + 2]);
        let p = r * l;
        for a in 0..3 {
            pos[3 * i + a] = x[a] + p[a];
            vel[3 * i + a] = (pos[3 * i + a] - prev[3 * i + a]) / h;
        }
    }
    let mut out = [0.0; 9];
    for a in 0..3 {
        for b in 0..3 {
            out[3 * a + b] = r[(a, b)];
        }
    }
    (out, x)
}
