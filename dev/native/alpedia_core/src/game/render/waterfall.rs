//! 描画：滝（A-07）。名所の崖や湖の段から落ちる水の幕（絵が下へ流れる）と、滝つぼのしぶき。
//!
//! 置き場所は惑星の形（PlanetShape.waterfalls）が決める。惑星ローカルの決まった形なので、最初に 1 回だけ作る。

use super::*;

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn norm(a: [f64; 3]) -> [f64; 3] {
    let n = dot(a, a).sqrt().max(1e-12);
    [a[0] / n, a[1] / n, a[2] / n]
}
fn madd(a: [f64; 3], b: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

/// 水の絵の 1 回の長さ (m)
const TILE: f64 = 36.0;
const ROWS: usize = 16;

/// (水の幕, しぶき) のメッシュ（頂点 12 個ずつ・三角形）
#[allow(clippy::type_complexity)]
pub fn build(falls: &[[f64; 7]], sun: [f64; 3], amb: f64) -> ((Vec<f32>, Vec<i64>), (Vec<f32>, Vec<i64>)) {
    let (mut v, mut t) = (vec![], vec![]);
    let (mut mv, mut mt) = (vec![], vec![]);
    let mut put = |v: &mut Vec<f32>, p: [f64; 3], n: [f64; 3], c: [f64; 3], a: f64, uv: (f64, f64)| {
        v.extend_from_slice(&[p[0] as f32, p[1] as f32, p[2] as f32, n[0] as f32, n[1] as f32, n[2] as f32,
                              c[0] as f32, c[1] as f32, c[2] as f32, a as f32, uv.0 as f32, uv.1 as f32]);
    };
    for f in falls {
        let a = [f[0], f[1], f[2]];
        let b = [f[3], f[4], f[5]];
        let w = f[6];
        let up = norm(a);
        let ab = sub(b, a);
        let drop = -dot(ab, up);                         // 落差
        if drop < 1.0 {
            continue;
        }
        let hz = madd(ab, up, drop);                     // 横に進む分
        let hd = dot(hz, hz).sqrt();
        let out = if hd > 1e-3 { norm(hz) } else {
            let x = if up[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
            norm(cross(x, up))
        };
        let side = norm(cross(up, out));
        let col = shade_rs(sun, amb, up, [0.92, 0.96, 1.0]);
        let col = [col[0].max(0.55), col[1].max(0.6), col[2].max(0.65)];
        // 幕：上から下へ。水は崖から前へ飛び出してから落ちる（横は落ちた深さの平方根に比例）。下ほど広がる
        let base = (v.len() / 12) as i64;
        let mut along = 0.0;
        let mut prev = a;
        for k in 0..=ROWS {
            let s = k as f64 / ROWS as f64;
            let c = madd(madd(madd(a, up, -drop * s), out, hd * s.sqrt() + 1.5), up, 0.0);
            along += dot(sub(c, prev), sub(c, prev)).sqrt();
            prev = c;
            let half = w * 0.5 * (1.0 + 0.6 * s);
            let alpha = 0.92 - 0.25 * s;
            let vv = -along / TILE;                       // v = 1 が絵の上。下へ行くほど小さく
            put(&mut v, madd(c, side, -half), out, col, alpha, (0.0, vv));
            put(&mut v, madd(c, side, half), out, col, alpha, (0.5, vv));
        }
        for k in 0..ROWS as i64 {
            let i = base + 2 * k;
            t.extend_from_slice(&[i, i + 1, i + 3, i, i + 3, i + 2]);
        }
        // しぶき：滝つぼに十字の板を 3 枚
        let sz = w * 1.8;
        let hh = (w * 1.1).min(drop * 0.8).max(4.0);
        let foot = madd(b, up, -hh * 0.15);
        for j in 0..3 {
            let ang = j as f64 * std::f64::consts::PI / 3.0;
            let e = norm(madd(side.map(|x| x * ang.cos()), out, ang.sin()));
            let i = (mv.len() / 12) as i64;
            let mcol = [col[0] + 0.05, col[1] + 0.05, col[2] + 0.05];
            for (sx, sy) in [(-0.5, 0.0), (0.5, 0.0), (0.5, 1.0), (-0.5, 1.0)] {
                let p = madd(madd(foot, e, sx * sz), up, sy * hh);
                put(&mut mv, p, up, mcol, 0.8, (0.5 + (sx + 0.5) * 0.5, sy));
            }
            mt.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
        }
    }
    ((v, t), (mv, mt))
}
