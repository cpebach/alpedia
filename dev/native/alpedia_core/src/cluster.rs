//! ブロックごとの計算（やわらかいブロックのモード）の「まとまりの形合わせ」。
//!
//! ブロックは自分の形（8頂点）を保ち、となりのブロックとは頂点どうしでつながるだけなので、反復の回数が少ないと
//! 固さが遠くまで伝わらず、大きな構築物ほどやわらかくなっていた（長い梁がたわむ・高い柱が自分の重さで崩れる）。
//! そこで、近くのつながったブロックのまとまり（2・4・8…セルの箱の中で、つながっている物。箱は半分ずらした組も使う）
//! ごとに、ブロックの中心の並びを1つの剛体として形合わせする。大きいまとまりが遠くまで固さを伝える。
//!
//! まとまりの基準の並び（crest）は、初めて使うときの今の形から取る（読み込んだセーブの曲がりはそのまま保つ）。
//! 降伏を超えて曲げられたまとまりは、基準の並びを今の形へ寄せる（曲がったまま戻らない）。
//! アクチュエーターのブロックはまとまりに入れない（伸び縮み・回転を止めないため）。

use std::collections::HashMap;

use nalgebra::Matrix3;

/// まとまりの組（形が変わるまで変わらない）
#[derive(Default, Clone)]
pub struct Clusters {
    /// まとまり k のブロックは vox[start[k]..start[k+1]]
    pub start: Vec<usize>,
    pub vox: Vec<usize>,
    /// まとまりごとの固さ・塑性・降伏（ブロックの値の平均）
    pub stiff: Vec<f64>,
    pub plast: Vec<f64>,
    pub yield_: Vec<f64>,
    /// ブロックごとの質量（8頂点の和）
    pub vmass: Vec<f64>,
}

impl Clusters {
    pub fn n_members(&self) -> usize {
        self.vox.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vox.is_empty()
    }

    fn n(&self) -> usize {
        self.start.len().saturating_sub(1)
    }
}

/// まとまりを作る。cells：ブロックのセル (nvox*3)、ok：まとまりに入れてよいブロック、ja/jb：頂点の結合
#[allow(clippy::too_many_arguments)]
pub fn build(
    nvox: usize, cells: &[i64], ok: &[bool], ja: &[i64], jb: &[i64], mass: &[f64], stiff: &[f64], plast: &[f64], yield_: &[f64],
) -> Clusters {
    let mut out = Clusters::default();
    if nvox < 3 || cells.len() != 3 * nvox || ok.len() != nvox || mass.len() != 8 * nvox {
        return out;
    }
    // となりどうし（結合でつながったブロック）
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); nvox];
    for (&a, &b) in ja.iter().zip(jb.iter()) {
        let (va, vb) = (a as usize / 8, b as usize / 8);
        if va != vb && ok[va] && ok[vb] && !adj[va].contains(&vb) {
            adj[va].push(vb);
            adj[vb].push(va);
        }
    }
    let mut lo = [i64::MAX; 3];
    let mut hi = [i64::MIN; 3];
    for i in 0..nvox {
        for a in 0..3 {
            lo[a] = lo[a].min(cells[3 * i + a]);
            hi[a] = hi[a].max(cells[3 * i + a]);
        }
    }
    let extent = (0..3).map(|a| hi[a] - lo[a] + 1).max().unwrap_or(1);
    let mut seen: HashMap<Vec<usize>, ()> = HashMap::new();
    let mut mark = vec![usize::MAX; nvox];
    let mut stamp = 0usize;
    out.start.push(0);
    let mut s: i64 = 2;
    loop {
        let offs: &[i64] = if s >= 2 * extent { &[0] } else { &[0, 1] };
        for &o in offs {
            let off = o * s / 2;
            let mut boxes: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
            for i in 0..nvox {
                if !ok[i] {
                    continue;
                }
                let k = [
                    (cells[3 * i] - lo[0] + off).div_euclid(s),
                    (cells[3 * i + 1] - lo[1] + off).div_euclid(s),
                    (cells[3 * i + 2] - lo[2] + off).div_euclid(s),
                ];
                boxes.entry(k).or_default().push(i);
            }
            let mut keys: Vec<[i64; 3]> = boxes.keys().copied().collect();
            keys.sort();
            for k in keys {
                let members = &boxes[&k];
                if members.len() < 2 {
                    continue;
                }
                // 箱の中でつながっている物ごとに
                stamp += 1;
                let inbox = stamp;
                for &i in members {
                    mark[i] = inbox;
                }
                for &seed in members {
                    if mark[seed] != inbox {
                        continue;
                    }
                    stamp += 1;
                    let comp_id = stamp;
                    let mut comp = vec![seed];
                    mark[seed] = comp_id;
                    let mut q = 0;
                    while q < comp.len() {
                        let v = comp[q];
                        q += 1;
                        for &u in &adj[v] {
                            if mark[u] == inbox {
                                mark[u] = comp_id;
                                comp.push(u);
                            }
                        }
                    }
                    if comp.len() < 2 {
                        continue;
                    }
                    comp.sort_unstable();
                    if seen.insert(comp.clone(), ()).is_some() {
                        continue;
                    }
                    let n = comp.len() as f64;
                    out.stiff.push(comp.iter().map(|&i| stiff[i]).sum::<f64>() / n);
                    out.plast.push(comp.iter().map(|&i| plast[i]).sum::<f64>() / n);
                    out.yield_.push(comp.iter().map(|&i| yield_[i]).sum::<f64>() / n);
                    out.vox.extend_from_slice(&comp);
                    out.start.push(out.vox.len());
                }
            }
        }
        if s >= 2 * extent {
            break;
        }
        s *= 2;
    }
    out.vmass = (0..nvox).map(|i| mass[8 * i..8 * i + 8].iter().sum()).collect();
    out
}

fn centers(pos: &[f64], nvox: usize) -> Vec<[f64; 3]> {
    (0..nvox)
        .map(|i| {
            let mut c = [0.0; 3];
            for k in 0..8 {
                for a in 0..3 {
                    c[a] += pos[3 * (8 * i + k) + a];
                }
            }
            [c[0] / 8.0, c[1] / 8.0, c[2] / 8.0]
        })
        .collect()
}

/// 基準の並びが無ければ（NaN）、今の形から作る
pub fn init_rest(pos: &[f64], crest: &mut [f64], cl: &Clusters) {
    if crest.len() != 3 * cl.n_members() || cl.is_empty() || !crest[0].is_nan() {
        return;
    }
    let c = centers(pos, cl.vmass.len());
    for (j, &v) in cl.vox.iter().enumerate() {
        crest[3 * j..3 * j + 3].copy_from_slice(&c[v]);
    }
}

/// まとまり k の重心・基準の重心・回転（基準 → 今）
fn fit(c: &[[f64; 3]], crest: &[f64], cl: &Clusters, k: usize) -> ([f64; 3], [f64; 3], Matrix3<f64>) {
    let (s0, s1) = (cl.start[k], cl.start[k + 1]);
    let mut m = 0.0;
    let mut x = [0.0; 3];
    let mut r0 = [0.0; 3];
    for j in s0..s1 {
        let v = cl.vox[j];
        let w = cl.vmass[v];
        m += w;
        for a in 0..3 {
            x[a] += w * c[v][a];
            r0[a] += w * crest[3 * j + a];
        }
    }
    for a in 0..3 {
        x[a] /= m;
        r0[a] /= m;
    }
    let mut am = Matrix3::<f64>::zeros();
    for j in s0..s1 {
        let v = cl.vox[j];
        let w = cl.vmass[v];
        for a in 0..3 {
            for b in 0..3 {
                am[(a, b)] += w * (c[v][a] - x[a]) * (crest[3 * j + b] - r0[b]);
            }
        }
    }
    // 極分解（A = R S）の R
    let svd = am.svd(true, true);
    let (u, vt) = (svd.u.unwrap(), svd.v_t.unwrap());
    let mut r = u * vt;
    if r.determinant() < 0.0 {
        let s = svd.singular_values;
        let mut mi = 0;
        for i in 1..3 {
            if s[i] < s[mi] {
                mi = i;
            }
        }
        let mut u2 = u;
        for row in 0..3 {
            u2[(row, mi)] = -u2[(row, mi)];
        }
        r = u2 * vt;
    }
    (x, r0, r)
}

/// 反復1回：まとまりごとに形合わせし、ブロックの8頂点をまとめて動かす（重なりは平均）
pub fn solve(pos: &mut [f64], crest: &[f64], cl: &Clusters) {
    if cl.is_empty() || crest.len() != 3 * cl.n_members() || crest[0].is_nan() {
        return;
    }
    let nvox = cl.vmass.len();
    let c = centers(pos, nvox);
    let mut d = vec![[0.0; 3]; nvox];
    let mut cnt = vec![0u32; nvox];
    for k in 0..cl.n() {
        let st = cl.stiff[k];
        if !(st > 0.0) {
            continue;
        }
        let (x, r0, r) = fit(&c, crest, cl, k);
        for j in cl.start[k]..cl.start[k + 1] {
            let v = cl.vox[j];
            let q = r * nalgebra::Vector3::new(crest[3 * j] - r0[0], crest[3 * j + 1] - r0[1], crest[3 * j + 2] - r0[2]);
            for a in 0..3 {
                d[v][a] += st * (x[a] + q[a] - c[v][a]);
            }
            cnt[v] += 1;
        }
    }
    for v in 0..nvox {
        if cnt[v] == 0 {
            continue;
        }
        let f = 1.0 / cnt[v] as f64;
        let dv = [d[v][0] * f, d[v][1] * f, d[v][2] * f];
        for k in 0..8 {
            for a in 0..3 {
                pos[3 * (8 * v + k) + a] += dv[a];
            }
        }
    }
}

/// 降伏を超えて曲がったまとまりは、基準の並びを今の形へ寄せる（曲がったまま戻らない）
pub fn plasticity(pos: &[f64], crest: &mut [f64], cl: &Clusters, size: f64, h: f64) {
    if cl.is_empty() || crest.len() != 3 * cl.n_members() || crest[0].is_nan() {
        return;
    }
    let c = centers(pos, cl.vmass.len());
    for k in 0..cl.n() {
        if !(cl.plast[k] > 0.0) {
            continue;
        }
        let (x, r0, r) = fit(&c, crest, cl, k);
        let rt = r.transpose();
        let (s0, s1) = (cl.start[k], cl.start[k + 1]);
        let mut diff = Vec::with_capacity(s1 - s0);
        let mut strain: f64 = 0.0;
        let mut rad: f64 = size;
        for j in s0..s1 {
            let v = cl.vox[j];
            let l = rt * nalgebra::Vector3::new(c[v][0] - x[0], c[v][1] - x[1], c[v][2] - x[2]);
            let e = [crest[3 * j] - r0[0], crest[3 * j + 1] - r0[1], crest[3 * j + 2] - r0[2]];
            let dd = [l[0] - e[0], l[1] - e[1], l[2] - e[2]];
            strain = strain.max((dd[0] * dd[0] + dd[1] * dd[1] + dd[2] * dd[2]).sqrt());
            rad = rad.max((e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt());
            diff.push(dd);
        }
        if strain / rad <= cl.yield_[k] {
            continue;
        }
        let rate = (cl.plast[k] * 20.0 * h).clamp(0.0, 1.0);
        for (q, j) in (s0..s1).enumerate() {
            for a in 0..3 {
                crest[3 * j + a] += diff[q][a] * rate;
            }
        }
    }
}
