//! 近い点の組を探す（格子で区切って、隣の格子だけ調べる）。

use std::collections::HashMap;

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

#[inline]
fn within(a: &[f64], b: &[f64], r2: f64) -> bool {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= r2
}

fn scan(pa: &[f64], pb: &[f64], r: f64, same: bool) -> Vec<(usize, usize)> {
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
                            if (!same || j > i) && within(pi, &pb[3 * j..3 * j + 3], r2) {
                                out.push((i, j));
                            }
                        }
                    }
                }
            }
        }
    }
    out.sort_unstable();
    out
}

/// 点の集合の中で、距離 r 以内の組 (i < j)。番号の順に並べる。
pub fn within_set(p: &[f64], r: f64) -> Vec<(usize, usize)> {
    scan(p, p, r, true)
}

/// 2つの点の集合の間で、距離 r 以内の組 (A の番号, B の番号)。番号の順に並べる。
pub fn between_sets(a: &[f64], b: &[f64], r: f64) -> Vec<(usize, usize)> {
    scan(a, b, r, false)
}
