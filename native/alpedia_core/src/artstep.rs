//! 完全剛体モードの関節つき剛体の1サブステップ（construct._art_begin / _art_end と同じ計算）。
//!
//! 部分（アクチュエーターの出力面の結合を除いてつながっているブロックのまとまり）ごとの剛体を、
//! 関節（出力面の4隅を回転角だけ回した位置に合わせる）でつなぐ。部分ごとの姿勢 X・R、形 vl などは
//! Python の numpy 配列のまま持ち、ここでその場で書き換える。街などの MOD の箱との接触は Python で集めて渡す。

use nalgebra::{Matrix3, Vector3};

use crate::art;
use crate::body::{self, PlanetCtx, Topo};

/// 押し出し（アクチュエーターの出力面を押し出す）1つ
pub struct Ext {
    pub rows: [usize; 4],
    pub nb: [f64; 3],
    pub e0: f64,
}

/// 部分の分け方など、形が変わるまで変わらない値
pub struct ArtTopo {
    pub g: usize,
    pub vg: Vec<i64>,
    pub inv_m: Vec<f64>,
    pub iinv_b: Vec<f64>,
    pub ra: Vec<i64>,
    pub rb: Vec<i64>,
    pub a: Vec<usize>, // (R,4)
    pub b: Vec<usize>, // (R,4)
    pub c: Vec<usize>, // (R,8)
    pub exts: Vec<Ext>,
    pub iters: i64,
    pub max_contacts: usize,
    pub motor_step_x: f64,
    pub motor_hold_deg: f64,
}

/// 部分ごとの姿勢の、今の頂点の位置への最適な当てはめ（construct._art_fit）
pub fn fit(pos: &[f64], x: &mut [f64], r: &mut [f64], vl: &[f64], mass: &[f64], at: &ArtTopo) {
    let g = at.g;
    let nv = mass.len();
    let mut sx = vec![0.0; 3 * g];
    for i in 0..nv {
        let k = at.vg[i] as usize;
        for a in 0..3 {
            sx[3 * k + a] += mass[i] * pos[3 * i + a];
        }
    }
    for k in 0..g {
        let m = 1.0 / at.inv_m[k];
        for a in 0..3 {
            x[3 * k + a] = sx[3 * k + a] / m;
        }
    }
    let mut hm = vec![Matrix3::<f64>::zeros(); g];
    for i in 0..nv {
        let k = at.vg[i] as usize;
        for a in 0..3 {
            for b in 0..3 {
                hm[k][(a, b)] += mass[i] * vl[3 * i + a] * (pos[3 * i + b] - x[3 * k + b]);
            }
        }
    }
    for k in 0..g {
        let svd = hm[k].svd(true, true);
        let u = svd.u.unwrap();
        let mut vt = svd.v_t.unwrap();
        let mut rr = vt.transpose() * u.transpose();
        if rr.determinant() < 0.0 {
            let s = svd.singular_values;
            let mut m = 0;
            for i in 1..3 {
                if s[i] < s[m] {
                    m = i;
                }
            }
            for c in 0..3 {
                vt[(m, c)] = -vt[(m, c)];
            }
            rr = vt.transpose() * u.transpose();
        }
        for a in 0..3 {
            for b in 0..3 {
                r[9 * k + 3 * a + b] = rr[(a, b)];
            }
        }
    }
}

/// 部分ごとの姿勢から頂点の位置を作る
pub fn write(pos: &mut [f64], x: &[f64], r: &[f64], vl: &[f64], at: &ArtTopo) {
    let gv: Vec<i64> = (0..at.vg.len() as i64).collect();
    art::art_write(pos, x, r, &gv, &at.vg, vl);
}

/// 部分ごとの速度（並進 V・角速度 W）。運動量・角運動量を保つ
fn part_velocity(pos: &[f64], vel: &[f64], x: &[f64], r: &[f64], mass: &[f64], at: &ArtTopo) -> (Vec<f64>, Vec<f64>) {
    let g = at.g;
    let nv = mass.len();
    let mut v = vec![0.0; 3 * g];
    for i in 0..nv {
        let k = at.vg[i] as usize;
        for a in 0..3 {
            v[3 * k + a] += mass[i] * vel[3 * i + a];
        }
    }
    for k in 0..g {
        let m = 1.0 / at.inv_m[k];
        for a in 0..3 {
            v[3 * k + a] /= m;
        }
    }
    let mut l = vec![0.0; 3 * g];
    for i in 0..nv {
        let k = at.vg[i] as usize;
        let ri = [pos[3 * i] - x[3 * k], pos[3 * i + 1] - x[3 * k + 1], pos[3 * i + 2] - x[3 * k + 2]];
        let dv = [vel[3 * i] - v[3 * k], vel[3 * i + 1] - v[3 * k + 1], vel[3 * i + 2] - v[3 * k + 2]];
        let c = body::cross(ri, dv);
        for a in 0..3 {
            l[3 * k + a] += c[a] * mass[i];
        }
    }
    let mut w = vec![0.0; 3 * g];
    for k in 0..g {
        let rm = body::m3(&r[9 * k..9 * k + 9]);
        let ib = body::m3(&at.iinv_b[9 * k..9 * k + 9]);
        let ww = rm * ib * rm.transpose() * Vector3::new(l[3 * k], l[3 * k + 1], l[3 * k + 2]);
        for a in 0..3 {
            w[3 * k + a] = ww[a];
        }
    }
    (v, w)
}

/// 前半の1：力で速度を進め、押し出しで形を変え、部分ごとの剛体として位置を進める。
/// 戻り値：押し出しで形が変わった頂点と、その変化（後半で速度から除く）
#[allow(clippy::too_many_arguments)]
pub fn predict(
    pos: &mut [f64], vel: &mut [f64], prev: &mut [f64], force: &[f64], t: &Topo, at: &ArtTopo, x: &mut [f64],
    r: &mut [f64], vl: &mut [f64], vl0: &[f64], ext_applied: &mut [f64], ext_want: &[f64], ext_speed: &[f64],
    planets: &[PlanetCtx], scale: f64, h: f64, cav: &[[i64; 6]],
) -> (Vec<usize>, Vec<[f64; 3]>) {
    body::integrate_forces(pos, vel, force, t, planets, scale, h, cav);
    fit(pos, x, r, vl, &t.mass, at);
    let nv = t.mass.len();
    let mut defo = (Vec::new(), Vec::new());
    if !at.exts.is_empty() {
        let mut nvl = vl0.to_vec();
        let mut moved = false;
        for (k, e) in at.exts.iter().enumerate() {
            let spd = ext_speed[k].max(0.01) * h * 1.05;
            let cur = ext_applied[k] + (ext_want[k] - ext_applied[k]).clamp(-spd, spd);
            ext_applied[k] = cur;
            if cur != e.e0 {
                for &row in &e.rows {
                    for a in 0..3 {
                        nvl[3 * row + a] += e.nb[a] * ((cur - e.e0) * t.size);
                    }
                }
                moved = true;
            }
        }
        if moved {
            // 部分の重心が基準点（X）のままになるように、部分ごとに中心を合わせ直す
            let mut lbar = vec![0.0; 3 * at.g];
            for i in 0..nv {
                let k = at.vg[i] as usize;
                for a in 0..3 {
                    lbar[3 * k + a] += t.mass[i] * nvl[3 * i + a];
                }
            }
            for k in 0..at.g {
                let m = 1.0 / at.inv_m[k];
                for a in 0..3 {
                    lbar[3 * k + a] /= m;
                }
            }
            for i in 0..nv {
                let k = at.vg[i] as usize;
                for a in 0..3 {
                    nvl[3 * i + a] -= lbar[3 * k + a];
                }
            }
        }
        for i in 0..nv {
            let d = [nvl[3 * i] - vl[3 * i], nvl[3 * i + 1] - vl[3 * i + 1], nvl[3 * i + 2] - vl[3 * i + 2]];
            if d.iter().any(|&q| q.abs() > 0.0) {
                defo.0.push(i);
                defo.1.push(d);
            }
        }
        if !defo.0.is_empty() {
            vl.copy_from_slice(&nvl);
        }
    }
    let (v, w) = part_velocity(pos, vel, x, r, &t.mass, at);
    prev.copy_from_slice(pos);
    for k in 0..at.g {
        for a in 0..3 {
            x[3 * k + a] += v[3 * k + a] * h;
        }
        art::rot_left(&mut r[9 * k..9 * k + 9], w[3 * k] * h, w[3 * k + 1] * h, w[3 * k + 2] * h);
    }
    write(pos, x, r, vl, at);
    defo
}

/// 接触点 1つ（P：頂点、D：めり込み、N：法線、VS：相手の速度、CI：MOD の箱の番号（地面は -1））
#[derive(Clone, Copy)]
pub struct Contact {
    pub p: usize,
    pub d: f64,
    pub n: [f64; 3],
    pub vs: [f64; 3],
    pub ci: i64,
}

/// 前半の2：地面との接触を集め（MOD の箱の接触は boxes で受け取る）、関節と接触を解いて位置を決める。
/// 戻り値：使った接触点（部分ごとに深い順で上限まで）とそれぞれの押し返しの大きさ
#[allow(clippy::too_many_arguments)]
pub fn solve(
    pos: &mut [f64], prev: &[f64], t: &Topo, at: &ArtTopo, x: &mut [f64], r: &mut [f64], vl: &[f64],
    applied: &mut [f64], want: &[f64], rot_speed: &[f64], planets: &[PlanetCtx], h: f64, boxes: &[Contact],
) -> (Vec<Contact>, Vec<f64>, Vec<f64>) {
    let rad = t.size * 0.5;
    let mut cs: Vec<Contact> = Vec::new();
    for pl in planets {
        if !body::near_planet(pos, t.nvox, t.size, pl) {
            continue;
        }
        let k = body::cores(pos, t);
        let (up, rs) = body::ground_ref(pos, pl);
        let vz = vec![0.0; pos.len()];
        let (_, hit) = body::collide_planet(pos, prev, &vz, &k, &t.fric, rad, pl, up, rs, false, 0.0, 1.0, false);
        cs.extend(hit.into_iter().map(|(i, d, n)| Contact { p: i, d: -d, n, vs: [0.0; 3], ci: -1 }));
    }
    cs.extend_from_slice(boxes);
    // 部分ごとに深い順、上限まで
    let mut idx: Vec<usize> = (0..cs.len()).collect();
    idx.sort_by(|&i, &j| {
        let (gi, gj) = (at.vg[cs[i].p], at.vg[cs[j].p]);
        gi.cmp(&gj).then((-cs[i].d).partial_cmp(&-cs[j].d).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut kept: Vec<Contact> = Vec::new();
    let mut cnt = 0usize;
    let mut last = -1i64;
    for i in idx {
        let g = at.vg[cs[i].p];
        if g != last {
            last = g;
            cnt = 0;
        }
        if cnt < at.max_contacts {
            kept.push(cs[i]);
        }
        cnt += 1;
    }
    let nc = kept.len();
    let cg: Vec<i64> = kept.iter().map(|c| at.vg[c.p]).collect();
    let mut cl = vec![0.0; 3 * nc];
    for (k, c) in kept.iter().enumerate() {
        let g = cg[k] as usize;
        let rg = &r[9 * g..9 * g + 9];
        let d = [pos[3 * c.p] - x[3 * g], pos[3 * c.p + 1] - x[3 * g + 1], pos[3 * c.p + 2] - x[3 * g + 2]];
        for i in 0..3 {
            cl[3 * k + i] = rg[i] * d[0] + rg[3 + i] * d[1] + rg[6 + i] * d[2];
        }
    }
    let nr = at.ra.len();
    let mut la = vec![0.0; 12 * nr];
    let mut lb = vec![0.0; 12 * nr];
    let mut lc = vec![0.0; 3 * nr];
    for q in 0..nr {
        for k in 0..4 {
            for a in 0..3 {
                la[12 * q + 3 * k + a] = vl[3 * at.a[4 * q + k] + a];
                lb[12 * q + 3 * k + a] = vl[3 * at.b[4 * q + k] + a];
            }
        }
        for k in 0..8 {
            for a in 0..3 {
                lc[3 * q + a] += vl[3 * at.c[8 * q + k] + a];
            }
        }
        for a in 0..3 {
            lc[3 * q + a] /= 8.0;
        }
    }
    // 回転角はサブステップごとに少しずつ（回転の速さで）追いかける。1サブステップで直せる角度には上限
    let mut ang = vec![0.0; nr];
    let mut cap = vec![0.0; nr];
    for q in 0..nr {
        let spd = rot_speed[q].max(1.0) * h * 1.05;
        let step = (want[q] - applied[q]).clamp(-spd, spd);
        applied[q] += step;
        ang[q] = applied[q].to_radians();
        cap[q] = (step.abs() * at.motor_step_x + at.motor_hold_deg).to_radians();
    }
    let cn: Vec<f64> = kept.iter().flat_map(|c| c.n).collect();
    let cd: Vec<f64> = kept.iter().map(|c| c.d).collect();
    let cp: Vec<f64> = kept.iter().flat_map(|c| body::v3(pos, c.p)).collect();
    let cprev: Vec<f64> = kept.iter().flat_map(|c| body::v3(prev, c.p)).collect();
    let cvs: Vec<f64> = kept.iter().flat_map(|c| c.vs).collect();
    let cmu: Vec<f64> = kept.iter().map(|c| t.fric[c.p].clamp(0.0, 1.0)).collect();
    let mut lam = vec![0.0; nc];
    art::art_solve(
        x, r, &at.inv_m, &at.iinv_b, &at.ra, &at.rb, &la, &lb, &lc, &ang, &cap, &cg, &cl, &cn, &cd, &cp, &cprev, &cvs,
        &cmu, h, at.iters, &mut lam,
    );
    write(pos, x, r, vl, at);
    (kept, cl, lam)
}

/// 後半：ほかの構築物に押された分なども部分ごとの剛体の動きに直してから、速度を求める
#[allow(clippy::too_many_arguments)]
pub fn end(
    pos: &mut [f64], vel: &mut [f64], prev: &[f64], t: &Topo, at: &ArtTopo, x: &mut [f64], r: &mut [f64], vl: &[f64],
    defo: &[(usize, [f64; 3])], cg: &[i64], cl: &[f64], cn: &[f64], lam: &[f64], h: f64,
) {
    fit(pos, x, r, vl, &t.mass, at);
    write(pos, x, r, vl, at);
    for i in 0..pos.len() {
        vel[i] = (pos[i] - prev[i]) / h;
    }
    // 押し出しによる形の変化は速度に含めない
    for &(row, d) in defo {
        let g = at.vg[row] as usize;
        let rg = &r[9 * g..9 * g + 9];
        for a in 0..3 {
            vel[3 * row + a] -= (rg[3 * a] * d[0] + rg[3 * a + 1] * d[1] + rg[3 * a + 2] * d[2]) / h;
        }
    }
    if cg.is_empty() || !lam.iter().any(|&l| l > 0.0) {
        return;
    }
    // 部分ごとの速度に直し、地面から離れる向きの余計な速度を消してから頂点の速度に戻す
    let (mut v, mut w) = part_velocity(pos, vel, x, r, &t.mass, at);
    art::art_contact_vel(&mut v, &mut w, r, &at.inv_m, &at.iinv_b, cg, cl, cn, lam, 2);
    for i in 0..t.mass.len() {
        let g = at.vg[i] as usize;
        let ri = [pos[3 * i] - x[3 * g], pos[3 * i + 1] - x[3 * g + 1], pos[3 * i + 2] - x[3 * g + 2]];
        let c = body::cross([w[3 * g], w[3 * g + 1], w[3 * g + 2]], ri);
        for a in 0..3 {
            vel[3 * i + a] = v[3 * g + a] + c[a];
        }
    }
}
