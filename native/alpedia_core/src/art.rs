//! 完全剛体モードの関節つき剛体（fastphys.py の art_solve / art_contact_vel / art_write と同じ）。
//!
//! 部分ごとの姿勢：X（部分数×3）・R（部分数×9、行優先）。逆質量 inv_m、物体座標の逆慣性 iinv_b（部分数×9）。

use crate::kernels::polar;

#[inline]
fn rot_left(r: &mut [f64], px: f64, py: f64, pz: f64) {
    let th = (px * px + py * py + pz * pz).sqrt();
    if th < 1e-14 {
        return;
    }
    let (kx, ky, kz) = (px / th, py / th, pz / th);
    let (s, c) = (th.sin(), th.cos());
    let t = 1.0 - c;
    let m = [
        [c + kx * kx * t, kx * ky * t - kz * s, kx * kz * t + ky * s],
        [ky * kx * t + kz * s, c + ky * ky * t, ky * kz * t - kx * s],
        [kz * kx * t - ky * s, kz * ky * t + kx * s, c + kz * kz * t],
    ];
    for j in 0..3 {
        let (r0, r1, r2) = (r[j], r[3 + j], r[6 + j]);
        r[j] = m[0][0] * r0 + m[0][1] * r1 + m[0][2] * r2;
        r[3 + j] = m[1][0] * r0 + m[1][1] * r1 + m[1][2] * r2;
        r[6 + j] = m[2][0] * r0 + m[2][1] * r1 + m[2][2] * r2;
    }
}

/// ワールドの逆慣性 × v（= R Iinv_b R^T v）
#[inline]
fn iw(r: &[f64], ib: &[f64], g: usize, vx: f64, vy: f64, vz: f64) -> (f64, f64, f64) {
    let rg = &r[9 * g..9 * g + 9];
    let i = &ib[9 * g..9 * g + 9];
    let ax = rg[0] * vx + rg[3] * vy + rg[6] * vz;
    let ay = rg[1] * vx + rg[4] * vy + rg[7] * vz;
    let az = rg[2] * vx + rg[5] * vy + rg[8] * vz;
    let bx = i[0] * ax + i[1] * ay + i[2] * az;
    let by = i[3] * ax + i[4] * ay + i[5] * az;
    let bz = i[6] * ax + i[7] * ay + i[8] * az;
    (
        rg[0] * bx + rg[1] * by + rg[2] * bz,
        rg[3] * bx + rg[4] * by + rg[5] * bz,
        rg[6] * bx + rg[7] * by + rg[8] * bz,
    )
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn apply_pt(
    x: &mut [f64], r: &mut [f64], inv_m: &[f64], ib: &[f64], g: usize, rx: f64, ry: f64, rz: f64, px: f64, py: f64,
    pz: f64, sgn: f64,
) {
    x[3 * g] += sgn * px * inv_m[g];
    x[3 * g + 1] += sgn * py * inv_m[g];
    x[3 * g + 2] += sgn * pz * inv_m[g];
    let tx = ry * pz - rz * py;
    let ty = rz * px - rx * pz;
    let tz = rx * py - ry * px;
    let (phx, phy, phz) = iw(r, ib, g, tx, ty, tz);
    rot_left(&mut r[9 * g..9 * g + 9], sgn * phx, sgn * phy, sgn * phz);
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn gen_w(r: &[f64], inv_m: &[f64], ib: &[f64], g: usize, rx: f64, ry: f64, rz: f64, nx: f64, ny: f64, nz: f64) -> f64 {
    let cx = ry * nz - rz * ny;
    let cy = rz * nx - rx * nz;
    let cz = rx * ny - ry * nx;
    let (ix, iy, iz) = iw(r, ib, g, cx, cy, cz);
    inv_m[g] + cx * ix + cy * iy + cz * iz
}

#[inline]
fn ang_w(r: &[f64], ib: &[f64], g: usize, ux: f64, uy: f64, uz: f64) -> f64 {
    let (ix, iy, iz) = iw(r, ib, g, ux, uy, uz);
    ux * ix + uy * iy + uz * iz
}

#[inline]
fn local_to_world(x: &[f64], r: &[f64], g: usize, l: &[f64]) -> [f64; 3] {
    let rg = &r[9 * g..9 * g + 9];
    [
        x[3 * g] + rg[0] * l[0] + rg[1] * l[1] + rg[2] * l[2],
        x[3 * g + 1] + rg[3] * l[0] + rg[4] * l[1] + rg[5] * l[2],
        x[3 * g + 2] + rg[6] * l[0] + rg[7] * l[1] + rg[8] * l[2],
    ]
}

/// 関節（アクチュエーター）と接触を、部分ごとの剛体の位置補正として反復で解く。
#[allow(clippy::too_many_arguments)]
pub fn art_solve(
    x: &mut [f64], r: &mut [f64], inv_m: &[f64], ib: &[f64], ra: &[i64], rb: &[i64], la: &[f64], lb: &[f64],
    lc: &[f64], ang: &[f64], cap: &[f64], cg: &[i64], cl: &[f64], cn: &[f64], cd: &[f64], cp: &[f64],
    cprev: &[f64], cvs: &[f64], cmu: &[f64], h: f64, iters: i64, lam_n: &mut [f64],
) {
    let nr = ra.len();
    let nc = cg.len();
    let mut lam_t = vec![0.0; nc];
    let mut used = vec![0.0; nr];
    let mut pa = [[0.0; 3]; 4];
    let mut tg = [[0.0; 3]; 4];
    let mut pb = [[0.0; 3]; 4];
    for _ in 0..iters {
        // --- 関節 ---
        for q in 0..nr {
            let (a, b) = (ra[q] as usize, rb[q] as usize);
            for k in 0..4 {
                pa[k] = local_to_world(x, r, a, &la[12 * q + 3 * k..12 * q + 3 * k + 3]);
            }
            let cx = (pa[0][0] + pa[1][0] + pa[2][0] + pa[3][0]) * 0.25;
            let cy = (pa[0][1] + pa[1][1] + pa[2][1] + pa[3][1]) * 0.25;
            let cz = (pa[0][2] + pa[1][2] + pa[2][2] + pa[3][2]) * 0.25;
            let ac = local_to_world(x, r, a, &lc[3 * q..3 * q + 3]);
            let (mut nx, mut ny, mut nz) = (cx - ac[0], cy - ac[1], cz - ac[2]);
            let l = (nx * nx + ny * ny + nz * nz).sqrt();
            if l < 1e-12 {
                continue;
            }
            nx /= l;
            ny /= l;
            nz /= l;
            let (cs, sn) = (ang[q].cos(), ang[q].sin());
            for k in 0..4 {
                let (vx, vy, vz) = (pa[k][0] - cx, pa[k][1] - cy, pa[k][2] - cz);
                let dot = vx * nx + vy * ny + vz * nz;
                let crx = ny * vz - nz * vy;
                let cry = nz * vx - nx * vz;
                let crz = nx * vy - ny * vx;
                tg[k] = [
                    cx + vx * cs + crx * sn + nx * dot * (1.0 - cs),
                    cy + vy * cs + cry * sn + ny * dot * (1.0 - cs),
                    cz + vz * cs + crz * sn + nz * dot * (1.0 - cs),
                ];
            }
            let (mut bcx, mut bcy, mut bcz) = (0.0, 0.0, 0.0);
            for k in 0..4 {
                pb[k] = local_to_world(x, r, b, &lb[12 * q + 3 * k..12 * q + 3 * k + 3]);
                bcx += pb[k][0] * 0.25;
                bcy += pb[k][1] * 0.25;
                bcz += pb[k][2] * 0.25;
            }
            // (1) 向き：出力面の4隅を目標の4隅に重ねる回転
            let mut hm = [[0.0; 3]; 3];
            for k in 0..4 {
                let u = [pb[k][0] - bcx, pb[k][1] - bcy, pb[k][2] - bcz];
                let v = [tg[k][0] - cx, tg[k][1] - cy, tg[k][2] - cz];
                for i in 0..3 {
                    for j in 0..3 {
                        hm[i][j] += u[i] * v[j];
                    }
                }
            }
            let ht = [[hm[0][0], hm[1][0], hm[2][0]], [hm[0][1], hm[1][1], hm[2][1]], [hm[0][2], hm[1][2], hm[2][2]]];
            let qm = polar(&ht);
            let cth = ((qm[0][0] + qm[1][1] + qm[2][2] - 1.0) * 0.5).clamp(-1.0, 1.0);
            let th = cth.acos();
            let (mut fx, mut fy, mut fz) = (qm[2][1] - qm[1][2], qm[0][2] - qm[2][0], qm[1][0] - qm[0][1]);
            let fl = (fx * fx + fy * fy + fz * fz).sqrt();
            if fl > 1e-12 && th > 1e-12 {
                fx = fx / fl * th;
                fy = fy / fl * th;
                fz = fz / fl * th;
            } else {
                fx = 0.0;
                fy = 0.0;
                fz = 0.0;
            }
            let mut tw = fx * nx + fy * ny + fz * nz;
            let (bx, by, bz) = (fx - nx * tw, fy - ny * tw, fz - nz * tw);
            // (1a) 軸の傾き
            let bl = (bx * bx + by * by + bz * bz).sqrt();
            if bl > 1e-12 {
                let (ux, uy, uz) = (bx / bl, by / bl, bz / bl);
                let lam = bl / (ang_w(r, ib, a, ux, uy, uz) + ang_w(r, ib, b, ux, uy, uz)).max(1e-30);
                let (pbx, pby, pbz) = iw(r, ib, b, ux * lam, uy * lam, uz * lam);
                let (pax, pay, paz) = iw(r, ib, a, ux * lam, uy * lam, uz * lam);
                rot_left(&mut r[9 * b..9 * b + 9], pbx, pby, pbz);
                rot_left(&mut r[9 * a..9 * a + 9], -pax, -pay, -paz);
            }
            // (1b) ねじれ（モーター。1サブステップの上限 cap）
            let room = (cap[q] - used[q]).max(0.0);
            tw = tw.clamp(-room, room);
            used[q] += tw.abs();
            if tw.abs() > 1e-14 {
                let lam = tw / (ang_w(r, ib, a, nx, ny, nz) + ang_w(r, ib, b, nx, ny, nz)).max(1e-30);
                let (pbx, pby, pbz) = iw(r, ib, b, nx * lam, ny * lam, nz * lam);
                let (pax, pay, paz) = iw(r, ib, a, nx * lam, ny * lam, nz * lam);
                rot_left(&mut r[9 * b..9 * b + 9], pbx, pby, pbz);
                rot_left(&mut r[9 * a..9 * a + 9], -pax, -pay, -paz);
            }
            let (mut bcx, mut bcy, mut bcz) = (0.0, 0.0, 0.0);
            for k in 0..4 {
                pb[k] = local_to_world(x, r, b, &lb[12 * q + 3 * k..12 * q + 3 * k + 3]);
                bcx += pb[k][0] * 0.25;
                bcy += pb[k][1] * 0.25;
                bcz += pb[k][2] * 0.25;
            }
            // (2) つなぎ目
            let (dx, dy, dz) = (cx - bcx, cy - bcy, cz - bcz);
            let d = (dx * dx + dy * dy + dz * dz).sqrt();
            if d < 1e-10 {
                continue;
            }
            let (ux, uy, uz) = (dx / d, dy / d, dz / d);
            let (rax, ray, raz) = (bcx - x[3 * a], bcy - x[3 * a + 1], bcz - x[3 * a + 2]);
            let (rbx, rby, rbz) = (bcx - x[3 * b], bcy - x[3 * b + 1], bcz - x[3 * b + 2]);
            let w = gen_w(r, inv_m, ib, a, rax, ray, raz, ux, uy, uz) + gen_w(r, inv_m, ib, b, rbx, rby, rbz, ux, uy, uz);
            let lam = d / w;
            apply_pt(x, r, inv_m, ib, b, rbx, rby, rbz, ux * lam, uy * lam, uz * lam, 1.0);
            apply_pt(x, r, inv_m, ib, a, rax, ray, raz, ux * lam, uy * lam, uz * lam, -1.0);
        }
        // --- 地面・建物との接触 ---
        for k in 0..nc {
            let g = cg[k] as usize;
            let p = local_to_world(x, r, g, &cl[3 * k..3 * k + 3]);
            let (mut px, mut py, mut pz) = (p[0], p[1], p[2]);
            let (nx, ny, nz) = (cn[3 * k], cn[3 * k + 1], cn[3 * k + 2]);
            let pen = cd[k] - ((px - cp[3 * k]) * nx + (py - cp[3 * k + 1]) * ny + (pz - cp[3 * k + 2]) * nz);
            if pen > 0.0 {
                let (rx, ry, rz) = (px - x[3 * g], py - x[3 * g + 1], pz - x[3 * g + 2]);
                let w = gen_w(r, inv_m, ib, g, rx, ry, rz, nx, ny, nz);
                let lam = pen / w;
                lam_n[k] += lam;
                apply_pt(x, r, inv_m, ib, g, rx, ry, rz, nx * lam, ny * lam, nz * lam, 1.0);
                px += nx * pen;
                py += ny * pen;
                pz += nz * pen;
            }
            if lam_n[k] <= 0.0 {
                continue;
            }
            let dx = px - cprev[3 * k] - cvs[3 * k] * h;
            let dy = py - cprev[3 * k + 1] - cvs[3 * k + 1] * h;
            let dz = pz - cprev[3 * k + 2] - cvs[3 * k + 2] * h;
            let dn = dx * nx + dy * ny + dz * nz;
            let (tx, ty, tz) = (dx - nx * dn, dy - ny * dn, dz - nz * dn);
            let tl = (tx * tx + ty * ty + tz * tz).sqrt();
            if tl < 1e-10 {
                continue;
            }
            let (ux, uy, uz) = (-tx / tl, -ty / tl, -tz / tl);
            let (rx, ry, rz) = (px - x[3 * g], py - x[3 * g + 1], pz - x[3 * g + 2]);
            let w = gen_w(r, inv_m, ib, g, rx, ry, rz, ux, uy, uz);
            let mut lam = tl * cmu[k] / w;
            let lim = 1.5 * lam_n[k] - lam_t[k];
            if lim <= 0.0 {
                continue;
            }
            if lam > lim {
                lam = lim;
            }
            lam_t[k] += lam;
            apply_pt(x, r, inv_m, ib, g, rx, ry, rz, ux * lam, uy * lam, uz * lam, 1.0);
        }
    }
}

/// 接触点の速度の後始末（離れる向きの速度を消す、はね返り 0）。
#[allow(clippy::too_many_arguments)]
pub fn art_contact_vel(
    v: &mut [f64], w: &mut [f64], r: &[f64], inv_m: &[f64], ib: &[f64], cg: &[i64], cl: &[f64], cn: &[f64],
    lam_n: &[f64], iters: i64,
) {
    for _ in 0..iters {
        for k in 0..cg.len() {
            if lam_n[k] <= 0.0 {
                continue;
            }
            let g = cg[k] as usize;
            let rg = &r[9 * g..9 * g + 9];
            let l = &cl[3 * k..3 * k + 3];
            let rx = rg[0] * l[0] + rg[1] * l[1] + rg[2] * l[2];
            let ry = rg[3] * l[0] + rg[4] * l[1] + rg[5] * l[2];
            let rz = rg[6] * l[0] + rg[7] * l[1] + rg[8] * l[2];
            let vx = v[3 * g] + w[3 * g + 1] * rz - w[3 * g + 2] * ry;
            let vy = v[3 * g + 1] + w[3 * g + 2] * rx - w[3 * g] * rz;
            let vz = v[3 * g + 2] + w[3 * g] * ry - w[3 * g + 1] * rx;
            let (nx, ny, nz) = (cn[3 * k], cn[3 * k + 1], cn[3 * k + 2]);
            let vn = vx * nx + vy * ny + vz * nz;
            if vn <= 0.0 {
                continue;
            }
            let gw = gen_w(r, inv_m, ib, g, rx, ry, rz, nx, ny, nz);
            let j = -vn / gw;
            v[3 * g] += nx * j * inv_m[g];
            v[3 * g + 1] += ny * j * inv_m[g];
            v[3 * g + 2] += nz * j * inv_m[g];
            let tx = ry * nz - rz * ny;
            let ty = rz * nx - rx * nz;
            let tz = rx * ny - ry * nx;
            let (ix, iy, iz) = iw(r, ib, g, tx * j, ty * j, tz * j);
            w[3 * g] += ix;
            w[3 * g + 1] += iy;
            w[3 * g + 2] += iz;
        }
    }
}

/// 部分ごとの姿勢から頂点の位置を作る。
pub fn art_write(pos: &mut [f64], x: &[f64], r: &[f64], gv: &[i64], vg: &[i64], vl: &[f64]) {
    for k in 0..gv.len() {
        let v = gv[k] as usize;
        let p = local_to_world(x, r, vg[k] as usize, &vl[3 * k..3 * k + 3]);
        pos[3 * v..3 * v + 3].copy_from_slice(&p);
    }
}
