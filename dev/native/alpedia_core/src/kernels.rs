//! 構築物の物理の計算（Python 版 main/fastphys.py と同じ内容）。
//!
//! 配列は「行優先で詰めた f64 / i64 のスライス」で受け取る（pos は 頂点数×3 など）。
//! 並列化は rayon。ボクセルごとの形合わせは、ボクセルが違えば書き込む頂点が重ならないので並列に回せる。

use nalgebra::Matrix3;
use rayon::prelude::*;

#[inline]
fn quat_to_mat(q: &[f64]) -> [[f64; 3]; 3] {
    let (w0, x, y, z) = (q[0], q[1], q[2], q[3]);
    [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - w0 * z), 2.0 * (x * z + w0 * y)],
        [2.0 * (x * y + w0 * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w0 * x)],
        [2.0 * (x * z - w0 * y), 2.0 * (y * z + w0 * x), 1.0 - 2.0 * (x * x + y * y)],
    ]
}

/// A に最も合う回転（Müller らの反復法。前回の回転 q から始める）。q を更新して行列を返す。
pub fn rotation(a: &[[f64; 3]; 3], q: &mut [f64]) -> [[f64; 3]; 3] {
    for _ in 0..40 {
        let r = quat_to_mat(q);
        let (mut ox, mut oy, mut oz, mut dd) = (0.0, 0.0, 0.0, 0.0);
        for c in 0..3 {
            let (rx, ry, rz) = (r[0][c], r[1][c], r[2][c]);
            let (ax, ay, az) = (a[0][c], a[1][c], a[2][c]);
            ox += ry * az - rz * ay;
            oy += rz * ax - rx * az;
            oz += rx * ay - ry * ax;
            dd += rx * ax + ry * ay + rz * az;
        }
        let inv = 1.0 / (dd.abs() + 1e-9);
        ox *= inv;
        oy *= inv;
        oz *= inv;
        let wn = (ox * ox + oy * oy + oz * oz).sqrt();
        if wn < 1e-9 {
            break;
        }
        let h = 0.5 * wn;
        let sh = h.sin() / wn;
        let (dw, dx, dy, dz) = (h.cos(), ox * sh, oy * sh, oz * sh);
        let (w0, x, y, z) = (q[0], q[1], q[2], q[3]);
        let nw = dw * w0 - dx * x - dy * y - dz * z;
        let nx = dw * x + dx * w0 + dy * z - dz * y;
        let ny = dw * y - dx * z + dy * w0 + dz * x;
        let nz = dw * z + dx * y - dy * x + dz * w0;
        let nq = (nw * nw + nx * nx + ny * ny + nz * nz).sqrt();
        q[0] = nw / nq;
        q[1] = nx / nq;
        q[2] = ny / nq;
        q[3] = nz / nq;
        if wn < 1e-7 {
            break;
        }
    }
    quat_to_mat(q)
}

/// 極分解の回転（行列式が負なら、いちばん小さい特異値の向きを裏返す）。
pub fn polar(h: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let m = Matrix3::new(h[0][0], h[0][1], h[0][2], h[1][0], h[1][1], h[1][2], h[2][0], h[2][1], h[2][2]);
    let svd = m.svd(true, true);
    let mut u = svd.u.unwrap();
    let vt = svd.v_t.unwrap();
    let mut r = u * vt;
    if r.determinant() < 0.0 {
        let s = svd.singular_values;
        let mut k = 0;
        for i in 1..3 {
            if s[i] < s[k] {
                k = i;
            }
        }
        for row in 0..3 {
            u[(row, k)] = -u[(row, k)];
        }
        r = u * vt;
    }
    [[r[(0, 0)], r[(0, 1)], r[(0, 2)]], [r[(1, 0)], r[(1, 1)], r[(1, 2)]], [r[(2, 0)], r[(2, 1)], r[(2, 2)]]]
}

/// 1つのボクセルの形合わせ。p8 はそのボクセルの8頂点（24個）、rest は基準形（24個）。
#[inline]
fn match_one(p8: &mut [f64], rest: &[f64], scale: &[f64], stiff: f64, last_r: &mut [f64], quat: &mut [f64]) {
    let mut c = [0.0; 3];
    for k in 0..8 {
        for a in 0..3 {
            c[a] += p8[3 * k + a];
        }
    }
    for a in 0..3 {
        c[a] /= 8.0;
    }
    let mut q = [[0.0; 3]; 8];
    let mut qm = [0.0; 3];
    for k in 0..8 {
        for a in 0..3 {
            q[k][a] = rest[3 * k + a] * scale[a];
            qm[a] += q[k][a];
        }
    }
    for a in 0..3 {
        qm[a] /= 8.0;
    }
    let mut am = [[0.0; 3]; 3];
    for k in 0..8 {
        for a in 0..3 {
            q[k][a] -= qm[a];
        }
        for a in 0..3 {
            let x = p8[3 * k + a] - c[a];
            for b in 0..3 {
                am[a][b] += x * q[k][b];
            }
        }
    }
    let r = rotation(&am, quat);
    for k in 0..8 {
        for a in 0..3 {
            let g = c[a] + r[a][0] * q[k][0] + r[a][1] * q[k][1] + r[a][2] * q[k][2];
            p8[3 * k + a] += stiff * (g - p8[3 * k + a]);
        }
    }
    for a in 0..3 {
        for b in 0..3 {
            last_r[3 * a + b] = r[a][b];
        }
    }
}

/// 書き込み先が重ならないことが分かっている並列処理用のポインタ。
#[derive(Clone, Copy)]
struct Ptr(*mut f64);
unsafe impl Send for Ptr {}
unsafe impl Sync for Ptr {}
impl Ptr {
    #[inline]
    fn at(self, off: usize) -> *mut f64 {
        unsafe { self.0.add(off) }
    }
}

/// ボクセルごとの形合わせ。vox の番号のボクセルだけ。parallel なら複数コアで。
pub fn shape_match(
    pos: &mut [f64], rest: &[f64], act_scale: &[f64], stiff: &[f64], vox: &[i64], last_r: &mut [f64],
    quat: &mut [f64], parallel: bool,
) {
    if !parallel {
        for &i in vox {
            let i = i as usize;
            match_one(
                &mut pos[24 * i..24 * i + 24], &rest[24 * i..24 * i + 24], &act_scale[3 * i..3 * i + 3], stiff[i],
                &mut last_r[9 * i..9 * i + 9], &mut quat[4 * i..4 * i + 4],
            );
        }
        return;
    }
    let (pp, lp, qp) = (Ptr(pos.as_mut_ptr()), Ptr(last_r.as_mut_ptr()), Ptr(quat.as_mut_ptr()));
    vox.par_iter().for_each(move |&i| {
        let i = i as usize;
        // 安全性：vox の番号は重ならない（ボクセルごとに書き込む範囲が別）
        let (p8, lr, qq) = unsafe {
            (
                std::slice::from_raw_parts_mut(pp.at(24 * i), 24),
                std::slice::from_raw_parts_mut(lp.at(9 * i), 9),
                std::slice::from_raw_parts_mut(qp.at(4 * i), 4),
            )
        };
        match_one(p8, &rest[24 * i..24 * i + 24], &act_scale[3 * i..3 * i + 3], stiff[i], lr, qq);
    });
}

/// 頂点どうしの結合（距離拘束）。
pub fn solve_joints(
    pos: &mut [f64], ja: &[i64], jb: &[i64], jr: &[f64], jk: &[f64], inv_mass: &[f64], jcount: &[f64],
) {
    let nv = pos.len() / 3;
    let mut delta = vec![0.0; nv * 3];
    for j in 0..ja.len() {
        let (a, b) = (ja[j] as usize, jb[j] as usize);
        let dx = pos[3 * b] - pos[3 * a];
        let dy = pos[3 * b + 1] - pos[3 * a + 1];
        let dz = pos[3 * b + 2] - pos[3 * a + 2];
        let l = (dx * dx + dy * dy + dz * dz).sqrt();
        let c = l - jr[j];
        let (wa, wb) = (inv_mass[a], inv_mass[b]);
        let s = c * jk[j] / (wa + wb) / l.max(1e-9);
        delta[3 * a] += s * dx * wa;
        delta[3 * a + 1] += s * dy * wa;
        delta[3 * a + 2] += s * dz * wa;
        delta[3 * b] -= s * dx * wb;
        delta[3 * b + 1] -= s * dy * wb;
        delta[3 * b + 2] -= s * dz * wb;
    }
    for i in 0..nv {
        let f = 1.0 / jcount[i];
        for a in 0..3 {
            pos[3 * i + a] += delta[3 * i + a] * f;
        }
    }
}

// ---------------------------------------------------------------------------
// 当たり判定の芯（点・線分・三角形の最近点）
// ---------------------------------------------------------------------------
#[inline]
fn closest_seg(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> ([f64; 3], f64) {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let den = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    let mut t = ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1] + (p[2] - a[2]) * ab[2]) / den.max(1e-12);
    t = t.clamp(0.0, 1.0);
    let o = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
    let d = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
    (o, (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
}

#[inline]
fn closest_tri(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let mut n = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
    let nn = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
    for x in n.iter_mut() {
        *x /= nn;
    }
    let dn = (p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1] + (p[2] - a[2]) * n[2];
    let pr = [p[0] - n[0] * dn, p[1] - n[1] * dn, p[2] - n[2] * dn];
    let v2 = [pr[0] - a[0], pr[1] - a[1], pr[2] - a[2]];
    let d00 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    let d01 = ab[0] * ac[0] + ab[1] * ac[1] + ab[2] * ac[2];
    let d11 = ac[0] * ac[0] + ac[1] * ac[1] + ac[2] * ac[2];
    let d20 = v2[0] * ab[0] + v2[1] * ab[1] + v2[2] * ab[2];
    let d21 = v2[0] * ac[0] + v2[1] * ac[1] + v2[2] * ac[2];
    let mut den = d00 * d11 - d01 * d01;
    if den.abs() < 1e-12 {
        den = 1e-12;
    }
    let v = (d11 * d20 - d01 * d21) / den;
    let w = (d00 * d21 - d01 * d20) / den;
    let u = 1.0 - v - w;
    if u >= 0.0 && v >= 0.0 && w >= 0.0 {
        return pr;
    }
    let (q1, e1) = closest_seg(p, a, b);
    let (q2, e2) = closest_seg(p, b, c);
    let (q3, e3) = closest_seg(p, c, a);
    if e1 <= e2 && e1 <= e3 {
        q1
    } else if e2 <= e3 {
        q2
    } else {
        q3
    }
}

/// 各頂点の芯の最近点（内部の頂点は NaN）。out は 頂点数×3。
pub fn contact_cores(pos: &[f64], ctype: &[i64], cidx: &[i64], out: &mut [f64]) {
    let nv = pos.len() / 3;
    let n = nv / 8;
    let mut cen = vec![[0.0f64; 3]; n];
    for i in 0..n {
        for k in 0..8 {
            for a in 0..3 {
                cen[i][a] += pos[3 * (8 * i + k) + a];
            }
        }
        for a in 0..3 {
            cen[i][a] /= 8.0;
        }
    }
    for v in 0..nv {
        let t = ctype[v];
        let p = [pos[3 * v], pos[3 * v + 1], pos[3 * v + 2]];
        let k = if t < 0 {
            [f64::NAN; 3]
        } else if t == 0 {
            cen[cidx[3 * v] as usize]
        } else if t == 1 {
            closest_seg(p, cen[cidx[3 * v] as usize], cen[cidx[3 * v + 1] as usize]).0
        } else {
            closest_tri(p, cen[cidx[3 * v] as usize], cen[cidx[3 * v + 1] as usize], cen[cidx[3 * v + 2] as usize])
        };
        out[3 * v..3 * v + 3].copy_from_slice(&k);
    }
}

// ---------------------------------------------------------------------------
// 描画用：変形後の表面の頂点と法線
// ---------------------------------------------------------------------------
pub fn skin(pos: &[f64], vidx: &[i64], w: &[f64], out: &mut [f64]) {
    let m = vidx.len() / 8;
    for i in 0..m {
        let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
        for k in 0..8 {
            let wk = w[8 * i + k];
            let p = vidx[8 * i + k] as usize;
            x += wk * pos[3 * p];
            y += wk * pos[3 * p + 1];
            z += wk * pos[3 * p + 2];
        }
        out[3 * i] = x;
        out[3 * i + 1] = y;
        out[3 * i + 2] = z;
    }
}

pub fn vertex_normals(v: &[f64], faces: &[i64], vn: &mut [f64]) {
    vn.iter_mut().for_each(|x| *x = 0.0);
    for f in 0..faces.len() / 3 {
        let (a, b, c) = (faces[3 * f] as usize, faces[3 * f + 1] as usize, faces[3 * f + 2] as usize);
        let e1 = [v[3 * b] - v[3 * a], v[3 * b + 1] - v[3 * a + 1], v[3 * b + 2] - v[3 * a + 2]];
        let e2 = [v[3 * c] - v[3 * a], v[3 * c + 1] - v[3 * a + 1], v[3 * c + 2] - v[3 * a + 2]];
        let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        for i in [a, b, c] {
            vn[3 * i] += n[0];
            vn[3 * i + 1] += n[1];
            vn[3 * i + 2] += n[2];
        }
    }
    for i in 0..v.len() / 3 {
        let l = (vn[3 * i].powi(2) + vn[3 * i + 1].powi(2) + vn[3 * i + 2].powi(2)).sqrt().max(1e-12);
        vn[3 * i] /= l;
        vn[3 * i + 1] /= l;
        vn[3 * i + 2] /= l;
    }
}

// ---------------------------------------------------------------------------
// 向き付きの箱（geom.py と同じ）
// ---------------------------------------------------------------------------
/// 点 P と箱 → 一番近い箱までの距離 d・外向きの向き n・箱の番号 k
pub fn box_sdf(p: &[f64], c: &[f64], a: &[f64], e: &[f64], d_out: &mut [f64], n_out: &mut [f64], k_out: &mut [i64]) {
    let nb = c.len() / 3;
    for i in 0..p.len() / 3 {
        let pi = [p[3 * i], p[3 * i + 1], p[3 * i + 2]];
        let mut best = f64::INFINITY;
        let mut bk = 0usize;
        let mut bq = [0.0; 3];
        let mut baq = [0.0; 3];
        for b in 0..nb {
            let rel = [pi[0] - c[3 * b], pi[1] - c[3 * b + 1], pi[2] - c[3 * b + 2]];
            let mut q = [0.0; 3];
            let mut aq = [0.0; 3];
            for r in 0..3 {
                q[r] = a[9 * b + 3 * r] * rel[0] + a[9 * b + 3 * r + 1] * rel[1] + a[9 * b + 3 * r + 2] * rel[2];
                aq[r] = q[r].abs() - e[3 * b + r];
            }
            let out = (aq[0].max(0.0).powi(2) + aq[1].max(0.0).powi(2) + aq[2].max(0.0).powi(2)).sqrt();
            let d = out + aq[0].max(aq[1]).max(aq[2]).min(0.0);
            if d < best {
                best = d;
                bk = b;
                bq = q;
                baq = aq;
            }
        }
        let mut g = [0.0; 3];
        if best <= 0.0 {
            let mut ax = 0;
            for r in 1..3 {
                if baq[r] > baq[ax] {
                    ax = r;
                }
            }
            let s = bq[ax].signum();
            g[ax] = if bq[ax] == 0.0 { 1.0 } else { s };
        } else {
            for r in 0..3 {
                g[r] = baq[r].max(0.0) * if bq[r] > 0.0 { 1.0 } else if bq[r] < 0.0 { -1.0 } else { 0.0 };
            }
        }
        let gl = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt().max(1e-12);
        for r in 0..3 {
            g[r] /= gl;
        }
        for j in 0..3 {
            n_out[3 * i + j] = g[0] * a[9 * bk + j] + g[1] * a[9 * bk + 3 + j] + g[2] * a[9 * bk + 6 + j];
        }
        d_out[i] = best;
        k_out[i] = bk as i64;
    }
}
