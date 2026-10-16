//! 構築物どうし（と、同じ構築物の結合していないボクセルどうし）の押し出し（world._voxel_collisions と同じ計算）。
//!
//! 形は「当たり判定の点」の球で近似する：ボクセル中心・結合の中点・三角形の重心（重み行列 W で作る）。
//! 同じ構築物の中はボクセル中心どうしだけ、別の構築物どうしは外枠が重なる組だけ調べる。

use std::collections::HashSet;

use crate::pairs;

/// 構築物1つの、形が変わるまで変わらない値
pub struct Proxy {
    pub n: usize,                     // ボクセル数（当たり判定の点の最初の n 個がボクセル中心）
    pub size: f64,
    pub indptr: Vec<usize>,           // 重み行列 W（点 × ボクセル、CSR）
    pub indices: Vec<usize>,
    pub data: Vec<f64>,
    pub inv: Vec<f64>,                // 点ごとの質量の逆数（W × ボクセルの質量の逆数）
    pub links: HashSet<(usize, usize)>, // 結合しているボクセルの組（小さい番号, 大きい番号）
    pub mass: Vec<f64>,               // 頂点ごとの質量（速度の計算用）
}

impl Proxy {
    pub fn points(&self) -> usize {
        self.indptr.len() - 1
    }
}

/// 1回の計算に渡す構築物
pub struct Item<'a> {
    pub p: &'a Proxy,
    pub pos: &'a mut [f64],
    pub vel: &'a [f64],
    pub active: bool,
    pub rigid: bool,
}

/// 結果：押し出されて起きる眠っていた構築物の番号、動いている物と触れているか、剛体のまま速くぶつかったか
pub struct Outcome {
    pub woke: Vec<usize>,
    pub touch: Vec<bool>,
    pub want_soft: Vec<bool>,
}

fn velocity(it: &Item) -> [f64; 3] {
    if !it.active {
        return [0.0; 3];
    }
    let (mut v, mut m) = ([0.0; 3], 0.0);
    for (i, &mi) in it.p.mass.iter().enumerate() {
        for a in 0..3 {
            v[a] += it.vel[3 * i + a] * mi;
        }
        m += mi;
    }
    if m > 0.0 {
        [v[0] / m, v[1] / m, v[2] / m]
    } else {
        [0.0; 3]
    }
}

/// 当たり判定の点（W × ボクセル中心）
fn proxy_points(it: &Item) -> Vec<f64> {
    let p = it.p;
    let mut c = vec![0.0; 3 * p.n];
    for i in 0..p.n {
        for k in 0..8 {
            for a in 0..3 {
                c[3 * i + a] += it.pos[3 * (8 * i + k) + a];
            }
        }
        for a in 0..3 {
            c[3 * i + a] /= 8.0;
        }
    }
    let np_ = p.points();
    let mut x = vec![0.0; 3 * np_];
    for r in 0..np_ {
        for q in p.indptr[r]..p.indptr[r + 1] {
            let (j, w) = (p.indices[q], p.data[q]);
            for a in 0..3 {
                x[3 * r + a] += w * c[3 * j + a];
            }
        }
    }
    x
}

pub fn solve(items: &mut [Item], n_active: usize) -> Outcome {
    let nc = items.len();
    let mut out = Outcome { woke: Vec::new(), touch: vec![false; nc], want_soft: vec![false; nc] };
    if n_active == 0 {
        return out;
    }
    let pts: Vec<Vec<f64>> = items.iter().map(proxy_points).collect();
    if pts.iter().map(|x| x.len() / 3).sum::<usize>() < 2 {
        return out;
    }
    // 近い点の組 (構築物 a, 点 a, 構築物 b, 点 b)
    let mut pr: Vec<(usize, usize, usize, usize)> = Vec::new();
    for k in 0..n_active {
        let n = items[k].p.n;
        if n > 1 {
            for (i, j) in pairs::within_set(&pts[k][..3 * n], 0.92 * items[k].p.size) {
                pr.push((k, i, k, j));
            }
        }
    }
    let bbox: Vec<([f64; 3], [f64; 3])> = pts
        .iter()
        .map(|x| {
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for p in x.chunks_exact(3) {
                for a in 0..3 {
                    lo[a] = lo[a].min(p[a]);
                    hi[a] = hi[a].max(p[a]);
                }
            }
            (lo, hi)
        })
        .collect();
    for k1 in 0..nc {
        for k2 in k1 + 1..nc {
            if k1 >= n_active && k2 >= n_active {
                continue; // 眠っているものどうしは調べない
            }
            if pts[k1].is_empty() || pts[k2].is_empty() {
                continue;
            }
            let r = 0.46 * (items[k1].p.size + items[k2].p.size);
            let (l1, h1) = bbox[k1];
            let (l2, h2) = bbox[k2];
            if !(0..3).all(|a| l1[a] <= h2[a] + r && l2[a] <= h1[a] + r) {
                continue;
            }
            for (i, j) in pairs::between_sets(&pts[k1], &pts[k2], r) {
                pr.push((k1, i, k2, j));
            }
        }
    }
    // めり込んでいる組だけ（同じ構築物で結合しているボクセルどうしは除く）
    struct Hit {
        ka: usize,
        ia: usize,
        kb: usize,
        ib: usize,
        n: [f64; 3],
        pen: f64,
    }
    let mut hits: Vec<Hit> = Vec::new();
    for (ka, ia, kb, ib) in pr {
        let pa = &pts[ka][3 * ia..3 * ia + 3];
        let pb = &pts[kb][3 * ib..3 * ib + 3];
        let d = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let mind = 0.46 * (items[ka].p.size + items[kb].p.size);
        if !(dist < mind) {
            continue;
        }
        if ka == kb {
            let key = if ia < ib { (ia, ib) } else { (ib, ia) };
            if items[ka].p.links.contains(&key) {
                continue;
            }
        }
        let l = dist.max(1e-9);
        hits.push(Hit { ka, ia, kb, ib, n: [d[0] / l, d[1] / l, d[2] / l], pen: mind - dist });
    }
    if hits.is_empty() {
        return out;
    }
    // 別の構築物どうしの接触：動いている相手と触れている間は静止扱いにしない。
    // 剛体として動いている構築物が速くぶつかったら、ブロックごとの計算に戻す（壊れ方を残す）
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let vels: Vec<[f64; 3]> = items.iter().map(velocity).collect();
    for h in &hits {
        if h.ka == h.kb || !seen.insert((h.ka, h.kb)) {
            continue;
        }
        let (va, vb) = (vels[h.ka], vels[h.kb]);
        let rel = ((va[0] - vb[0]).powi(2) + (va[1] - vb[1]).powi(2) + (va[2] - vb[2]).powi(2)).sqrt();
        if rel > 0.05 {
            out.touch[h.ka] = true;
            out.touch[h.kb] = true;
        }
        if rel > 2.0 {
            for o in [h.ka, h.kb] {
                if items[o].rigid {
                    out.want_soft[o] = true;
                }
            }
        }
    }
    // 押し出す（点に重みを持つボクセルの8頂点を動かす）
    fn shift(it: &mut Item, prow: usize, v: [f64; 3]) {
        let p = it.p;
        for q in p.indptr[prow]..p.indptr[prow + 1] {
            let j = p.indices[q];
            for k in 0..8 {
                for a in 0..3 {
                    it.pos[3 * (8 * j + k) + a] += v[a];
                }
            }
        }
    }
    let mut woke = vec![false; nc];
    for h in &hits {
        let corr = [h.n[0] * h.pen, h.n[1] * h.pen, h.n[2] * h.pen];
        let neg = [-corr[0], -corr[1], -corr[2]];
        if h.ka >= n_active && h.pen < 0.01 * items[h.ka].p.size {
            shift(&mut items[h.kb], h.ib, corr); // 眠っている側は動かさない
            continue;
        }
        if h.kb >= n_active && h.pen < 0.01 * items[h.kb].p.size {
            shift(&mut items[h.ka], h.ia, neg);
            continue;
        }
        let (wa, wb) = (items[h.ka].p.inv[h.ia], items[h.kb].p.inv[h.ib]);
        let ws = wa + wb;
        shift(&mut items[h.ka], h.ia, [neg[0] * wa / ws, neg[1] * wa / ws, neg[2] * wa / ws]);
        shift(&mut items[h.kb], h.ib, [corr[0] * wb / ws, corr[1] * wb / ws, corr[2] * wb / ws]);
        if h.ka >= n_active {
            woke[h.ka] = true;
        }
        if h.kb >= n_active {
            woke[h.kb] = true;
        }
    }
    out.woke = (0..nc).filter(|&k| woke[k]).collect();
    out
}
