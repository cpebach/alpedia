//! Rust の単体テスト（Python に頼らない計算だけ）。動かし方は native/README.md の「テスト」。
//!
//! ゲームの流れ全体のテストは dev/tests/（Python）にある。ここは、その下の計算（形合わせ・結合・まとまり・
//! 空気の抵抗・重力・惑星の形・掘った穴の水）が式どおりかを、数値で確かめる。

use std::sync::Arc;

use crate::body::{self, PlanetCtx, Topo};
use crate::{cluster, kernels, planet};

const EPS: f64 = 1e-9;

/// 1辺 s の立方体の8頂点（中心が原点。kernels の頂点の順は問わない：同じ順で比べる）
fn cube(s: f64) -> Vec<f64> {
    let h = s * 0.5;
    let mut v = vec![];
    for x in [-h, h] {
        for y in [-h, h] {
            for z in [-h, h] {
                v.extend([x, y, z]);
            }
        }
    }
    v
}

fn rot_z(deg: f64) -> [[f64; 3]; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

fn apply(r: &[[f64; 3]; 3], t: [f64; 3], p: &[f64]) -> Vec<f64> {
    p.chunks_exact(3)
        .flat_map(|q| (0..3).map(move |a| r[a][0] * q[0] + r[a][1] * q[1] + r[a][2] * q[2] + t[a]))
        .collect()
}

fn max_diff(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max)
}

fn dist(p: &[f64], i: usize, j: usize) -> f64 {
    ((p[3 * i] - p[3 * j]).powi(2) + (p[3 * i + 1] - p[3 * j + 1]).powi(2) + (p[3 * i + 2] - p[3 * j + 2]).powi(2)).sqrt()
}

/// テスト用の構築物の形（nvox 個のブロック、結合なし、質量は頂点ごとに m）
fn topo(nvox: usize, size: f64, m: f64) -> Topo {
    let nv = 8 * nvox;
    Topo {
        size,
        nvox,
        ja: vec![],
        jb: vec![],
        jr: vec![],
        jk: vec![],
        jbrk: vec![],
        jcount: vec![1.0; nv],
        inv_mass: vec![1.0 / m; nv],
        mass: vec![m; nv],
        damp: vec![0.0; nv],
        fric: vec![0.5; nv],
        stiff: vec![1.0; nvox],
        plast: vec![0.0; nvox],
        yield_: vec![1.0; nvox],
        density: vec![m * 8.0 / size.powi(3); nvox],
        ctype: vec![0; nv],
        cidx: vec![0; 3 * nv],
        rotors: vec![],
        clusters: Default::default(),
        drag_area: vec![],
    }
}

/// 半径 r・重力定数 gm の、でこぼこの無い惑星（中心は原点）
fn planet(r: f64, gm: f64, air: f64) -> PlanetCtx {
    PlanetCtx {
        shape: Arc::new(planet::Shape::new(r, 0.0, 1.0, 1, None)),
        center: [0.0; 3],
        craters: vec![],
        floods: vec![],
        bound: r + 2.0,
        lipschitz: 1.0,
        gm,
        radius: r,
        has_water: false,
        air,
        air_h: (r * 0.25).max(30.0),
        soi: 0.0,
        acc: [0.0; 3],
    }
}

// ---------------------------------------------------------------- kernels：形合わせ・結合

#[test]
fn polar_returns_the_rotation_of_a_rotated_and_stretched_matrix() {
    let r = rot_z(30.0);
    // A = R × diag(2, 1, 0.5)
    let h = [
        [r[0][0] * 2.0, r[0][1], r[0][2] * 0.5],
        [r[1][0] * 2.0, r[1][1], r[1][2] * 0.5],
        [r[2][0] * 2.0, r[2][1], r[2][2] * 0.5],
    ];
    let p = kernels::polar(&h);
    for a in 0..3 {
        for b in 0..3 {
            assert!((p[a][b] - r[a][b]).abs() < 1e-9, "polar {p:?} != {r:?}");
        }
    }
}

#[test]
fn shape_match_keeps_a_rigidly_moved_block() {
    let rest = cube(0.5);
    let mut pos = apply(&rot_z(40.0), [3.0, -1.0, 2.0], &rest);
    let before = pos.clone();
    let (mut lr, mut q) = (vec![0.0; 9], vec![1.0, 0.0, 0.0, 0.0]);
    kernels::shape_match(&mut pos, &rest, &[1.0; 3], &[1.0], &[0], &mut lr, &mut q, false);
    assert!(max_diff(&pos, &before) < 1e-6, "動かしただけのブロックが形合わせで変わった（回転は反復で 1e-7 ほどまで求める）");
    // 求めた回転は z 軸まわり 40°
    let r = rot_z(40.0);
    for a in 0..3 {
        for b in 0..3 {
            assert!((lr[3 * a + b] - r[a][b]).abs() < 1e-6);
        }
    }
}

#[test]
fn shape_match_restores_a_squashed_block() {
    let rest = cube(0.5);
    let mut pos = rest.clone();
    for k in 0..8 {
        pos[3 * k + 1] *= 0.6; // y を 6 割につぶす
    }
    let (mut lr, mut q) = (vec![0.0; 9], vec![1.0, 0.0, 0.0, 0.0]);
    kernels::shape_match(&mut pos, &rest, &[1.0; 3], &[1.0], &[0], &mut lr, &mut q, false);
    // 固さ 1 なら元の形（辺の長さ 0.5）に戻る
    assert!((dist(&pos, 0, 1) - 0.5).abs() < 1e-9);
    assert!((dist(&pos, 0, 2) - 0.5).abs() < 1e-9);
    assert!((dist(&pos, 0, 4) - 0.5).abs() < 1e-9);
}

#[test]
fn joints_pull_matching_vertices_together() {
    // 2つの頂点（質量は同じ）を、距離 0 の結合でつなぐ：半分ずつ寄る
    let mut pos = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    kernels::solve_joints(&mut pos, &[0], &[1], &[0.0], &[1.0], &[1.0, 1.0], &[1.0, 1.0]);
    assert!(max_diff(&pos, &[0.5, 0.0, 0.0, 0.5, 0.0, 0.0]) < EPS);
}

// ---------------------------------------------------------------- cluster：まとまりの形合わせ（L-03）

/// 一列に n 個（x 方向）のブロック。となりどうしは向かい合う4頂点を結合でつなぐ
fn row(n: usize, size: f64) -> (Vec<i64>, Vec<i64>, Vec<i64>, Vec<f64>) {
    let cells: Vec<i64> = (0..n).flat_map(|i| [i as i64, 0, 0]).collect();
    let mut pos = vec![];
    for i in 0..n {
        let c = cube(size);
        for k in 0..8 {
            pos.extend([c[3 * k] + i as f64 * size, c[3 * k + 1], c[3 * k + 2]]);
        }
    }
    let (mut ja, mut jb) = (vec![], vec![]);
    for i in 0..n.saturating_sub(1) {
        // cube() の順で x = +h の頂点は 4..8、x = -h は 0..4
        for k in 0..4 {
            ja.push((8 * i + 4 + k) as i64);
            jb.push((8 * (i + 1) + k) as i64);
        }
    }
    (cells, ja, jb, pos)
}

fn row_clusters(n: usize, plast: f64, yld: f64) -> (cluster::Clusters, Vec<f64>) {
    let (cells, ja, jb, pos) = row(n, 0.5);
    let ones = vec![1.0; n];
    let cl = cluster::build(n, &cells, &vec![true; n], &ja, &jb, &vec![1.0; 8 * n], &ones, &vec![plast; n], &vec![yld; n]);
    (cl, pos)
}

fn centers(pos: &[f64]) -> Vec<[f64; 3]> {
    pos.chunks_exact(24)
        .map(|b| {
            let mut c = [0.0; 3];
            for k in 0..8 {
                for a in 0..3 {
                    c[a] += b[3 * k + a] / 8.0;
                }
            }
            c
        })
        .collect()
}

#[test]
fn clusters_cover_every_block_once_per_set_and_include_the_whole() {
    let n = 12;
    let (cl, _) = row_clusters(n, 0.0, 1.0);
    assert!(!cl.is_empty());
    let mut seen = vec![false; n];
    let mut sets = std::collections::HashSet::new();
    let mut whole = false;
    for k in 0..cl.start.len() - 1 {
        let m: Vec<usize> = cl.vox[cl.start[k]..cl.start[k + 1]].to_vec();
        assert!(m.len() >= 2);
        assert!(sets.insert(m.clone()), "同じまとまりが2回ある");
        whole |= m.len() == n;
        for v in m {
            seen[v] = true;
        }
    }
    assert!(seen.iter().all(|&x| x));
    assert!(whole, "全体を含むまとまりが無い（大きな構築物に固さが伝わらない）");
}

#[test]
fn clusters_skip_blocks_that_are_not_allowed() {
    // まん中（アクチュエーター）を外すと、左右はつながらない：両方にまたがるまとまりは無い
    let n = 9;
    let (cells, ja, jb, _) = row(n, 0.5);
    let mut ok = vec![true; n];
    ok[4] = false;
    let ones = vec![1.0; n];
    let cl = cluster::build(n, &cells, &ok, &ja, &jb, &vec![1.0; 8 * n], &ones, &ones, &ones);
    for k in 0..cl.start.len() - 1 {
        let m = &cl.vox[cl.start[k]..cl.start[k + 1]];
        assert!(!m.contains(&4));
        assert!(m.iter().all(|&v| v < 4) || m.iter().all(|&v| v > 4), "アクチュエーターをまたいだ: {m:?}");
    }
}

#[test]
fn cluster_solve_keeps_rigid_motion_and_straightens_a_bent_row() {
    let n = 10;
    let (cl, pos0) = row_clusters(n, 0.0, 1.0);
    let mut crest = vec![f64::NAN; 3 * cl.n_members()];
    cluster::init_rest(&pos0, &mut crest, &cl);
    assert!(crest.iter().all(|x| x.is_finite()));
    // 丸ごと回して動かしただけなら、何も変えない
    let mut pos = apply(&rot_z(25.0), [1.0, 2.0, 3.0], &pos0);
    let moved = pos.clone();
    cluster::solve(&mut pos, &crest, &cl);
    assert!(max_diff(&pos, &moved) < 1e-9);
    // 真ん中から先を下げて曲げる → 何回か解くと、まっすぐに近づく
    let mut pos = pos0.clone();
    for b in n / 2..n {
        for k in 0..8 {
            pos[24 * b + 3 * k + 1] -= 0.05 * (b - n / 2 + 1) as f64;
        }
    }
    let bend = |p: &[f64]| centers(p).iter().map(|c| c[1].abs()).fold(0.0, f64::max);
    let before = bend(&pos);
    for _ in 0..6 {
        cluster::solve(&mut pos, &crest, &cl);
    }
    // 全体が少し回ってもよいので、「まっすぐさ」は端と端を結ぶ線からのずれで測る
    let c = centers(&pos);
    let (a, b) = (c[0], c[n - 1]);
    let dev = c.iter().map(|p| {
        let t = ((p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1])) / ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2));
        let q = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
    }).fold(0.0, f64::max);
    assert!(dev < before * 0.2, "曲がりが戻らない（前 {before:.3}、後 {dev:.3}）");
}

#[test]
fn cluster_plasticity_only_past_the_yield() {
    let n = 8;
    let bent = |pos0: &[f64]| {
        let mut p = pos0.to_vec();
        for b in n / 2..n {
            for k in 0..8 {
                p[24 * b + 3 * k + 1] -= 0.04 * (b - n / 2 + 1) as f64;
            }
        }
        p
    };
    // 降伏が大きい：基準の並びは変わらない
    let (cl, pos0) = row_clusters(n, 1.0, 10.0);
    let mut crest = vec![f64::NAN; 3 * cl.n_members()];
    cluster::init_rest(&pos0, &mut crest, &cl);
    let c0 = crest.clone();
    cluster::plasticity(&bent(&pos0), &mut crest, &cl, 0.5, 1.0 / 180.0);
    assert!(max_diff(&crest, &c0) < EPS);
    // 降伏が小さい：曲げた形のほうへ寄る
    let (cl, pos0) = row_clusters(n, 1.0, 0.001);
    let mut crest = vec![f64::NAN; 3 * cl.n_members()];
    cluster::init_rest(&pos0, &mut crest, &cl);
    let c0 = crest.clone();
    cluster::plasticity(&bent(&pos0), &mut crest, &cl, 0.5, 1.0 / 180.0);
    assert!(max_diff(&crest, &c0) > 1e-4);
}

// ---------------------------------------------------------------- body：空気の抵抗・重力（L-02）

#[test]
fn drag_area_is_a_quarter_of_the_open_surface() {
    let s = 0.5;
    // 1つだけ：6面 → s²×6/4 を8頂点に
    let a = body::drag_areas(&[0, 0, 0], 1, s);
    assert!((a.iter().sum::<f64>() - 6.0 * s * s / 4.0).abs() < EPS);
    // 2つ並べる：それぞれ 5 面
    let a = body::drag_areas(&[0, 0, 0, 1, 0, 0], 2, s);
    assert!((a[..8].iter().sum::<f64>() - 5.0 * s * s / 4.0).abs() < EPS);
    assert!((a[8..].iter().sum::<f64>() - 5.0 * s * s / 4.0).abs() < EPS);
    // 3×3×3 のまん中は外に出ていない
    let cells: Vec<i64> = (0..27).flat_map(|i| [i % 3, (i / 3) % 3, i / 9]).collect();
    let a = body::drag_areas(&cells, 27, s);
    assert_eq!(a[8 * 13..8 * 14].iter().sum::<f64>(), 0.0);
}

#[test]
fn air_thins_with_height_and_is_absent_without_atmosphere() {
    let pl = planet(600.0, 9.81 * 600.0 * 600.0, 1.225);
    let at = |h: f64| pl.air_density([0.0, 600.0 + h, 0.0]);
    assert!((at(0.0) - 1.225).abs() < EPS);
    assert!((at(pl.air_h) - 1.225 / std::f64::consts::E).abs() < 1e-9);
    assert!(at(100.0) > at(200.0));
    assert_eq!(planet(160.0, 1.0, 0.0).air_density([0.0, 160.0, 0.0]), 0.0);
}

#[test]
fn gravity_follows_the_inverse_square() {
    let gm = 9.81 * 600.0 * 600.0;
    let pls = [planet(600.0, gm, 0.0)];
    let g1 = body::gravity(&pls, [0.0, 600.0, 0.0], 1.0);
    let g2 = body::gravity(&pls, [0.0, 1200.0, 0.0], 1.0);
    assert!((g1[1] + 9.81).abs() < 1e-9, "地表で 9.81 m/s² 下向き：{g1:?}");
    assert!((g2[1] * 4.0 - g1[1]).abs() < 1e-9);
}

#[test]
fn falling_blocks_slow_down_in_air_but_not_in_vacuum() {
    let gm = 9.81 * 600.0 * 600.0;
    let mut t = topo(1, 0.5, 75.0 / 8.0); // 木のブロック 1 つ（75 kg）
    t.drag_area = body::drag_areas(&[0, 0, 0], 1, 0.5);
    let pos: Vec<f64> = cube(0.5).chunks_exact(3).flat_map(|p| [p[0], p[1] + 610.0, p[2]]).collect();
    let fall = |air: f64| {
        let pls = [planet(600.0, gm, air)];
        let mut vel = vec![0.0; 24];
        for k in 0..8 {
            vel[3 * k + 1] = -40.0; // 下へ 40 m/s
        }
        body::integrate_forces(&pos, &mut vel, &vec![0.0; 24], &t, &pls, 1.0, 1.0 / 60.0, &[]);
        vel[1]
    };
    let (v_air, v_vac) = (fall(1.225), fall(0.0));
    let p0 = [pos[0], pos[1], pos[2]];
    let g = body::gravity(&[planet(600.0, gm, 0.0)], p0, 1.0);
    let v1 = -40.0 + g[1] / 60.0;
    assert!((v_vac - v1).abs() < 1e-9, "真空では重力だけ：{v_vac}");
    // 空気の抵抗 ½ρCdA|v|v ÷ m だけ遅い（A = 6×0.25/4、m = 75。頂点ごとに 1/8 ずつ）
    let k = 0.5 * planet(600.0, gm, 1.225).air_density(p0) * body::DRAG_CD * 0.375 * v1.abs() / 75.0;
    assert!((v_air - v1 * (1.0 - k / 60.0)).abs() < 1e-9, "空気 {v_air}、真空 {v_vac}");
    assert!(v_air > v_vac, "空気の中のほうが遅い");
}

// ---------------------------------------------------------------- 惑星の形・掘った穴の水（L-01）

#[test]
fn smooth_planet_distance_and_craters() {
    let sh = planet::Shape::new(100.0, 0.0, 1.0, 1, None);
    assert!((sh.sdf([0.0, 103.0, 0.0], &[]) - 3.0).abs() < 1e-6);
    assert!((sh.sdf([0.0, 97.0, 0.0], &[]) + 3.0).abs() < 1e-6);
    // 地表のすぐ下を、半径 2 の掘削跡で掘ると、そこは空気（距離が正）になる
    let cr = [[0.0, 99.0, 0.0, 2.0]];
    assert!(sh.sdf([0.0, 99.0, 0.0], &cr) > 0.0);
    assert!(sh.sdf([0.0, 95.0, 0.0], &cr) < 0.0);
}

#[test]
fn flooded_hole_depth() {
    // 中心 (0, 99, 0)・半径 2 の穴に、水面の半径 100（地表と同じ高さ）まで水
    let f = [[0.0, 99.0, 0.0, 2.0, 100.0]];
    assert!((crate::flood_depth(&f, [0.0, 98.5, 0.0]) - 1.5).abs() < 1e-9);
    assert!(crate::flood_depth(&f, [0.0, 100.5, 0.0]) < 0.0, "水面より上");
    assert!(crate::flood_depth(&f, [5.0, 99.0, 0.0]) < 0.0, "穴の外");
    assert!(crate::flood_depth(&[], [0.0, 99.0, 0.0]) < 0.0);
}
