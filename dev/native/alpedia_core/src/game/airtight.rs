//! 気密判定：ボクセルで完全に囲まれた空洞を探す。main/airtight.py の中身。

use pyo3::prelude::*;
use pyo3::types::{PySet, PyTuple};

use crate::geomx;

pub const AIR_DENSITY_1ATM: f64 = 1.2; // kg/m^3
pub const ATM_KPA: f64 = 101.3;

/// cells → 空洞ごとのセルの番号（外部と繋がっていない空のセル群。ラベルの順）
pub fn find_cavities(cells: &[[i64; 3]]) -> Vec<Vec<[i64; 3]>> {
    if cells.len() < 6 {
        return Vec::new();
    }
    let mut lo = [i64::MAX; 3];
    let mut hi = [i64::MIN; 3];
    for c in cells {
        for a in 0..3 {
            lo[a] = lo[a].min(c[a] - 1);
            hi[a] = hi[a].max(c[a] + 1);
        }
    }
    let dims = [(hi[0] - lo[0] + 1) as usize, (hi[1] - lo[1] + 1) as usize, (hi[2] - lo[2] + 1) as usize];
    let n = dims[0] * dims[1] * dims[2];
    if n > 4_000_000 {
        return Vec::new(); // 大きすぎる場合は省略
    }
    let idx = |x: usize, y: usize, z: usize| (x * dims[1] + y) * dims[2] + z;
    let mut solid = vec![false; n];
    for c in cells {
        solid[idx((c[0] - lo[0]) as usize, (c[1] - lo[1]) as usize, (c[2] - lo[2]) as usize)] = true;
    }
    let (lab, count) = geomx::label_empty(&solid, dims);
    if count <= 1 {
        return Vec::new();
    }
    let mut border = vec![false; count as usize + 1];
    for x in 0..dims[0] {
        for y in 0..dims[1] {
            for z in 0..dims[2] {
                if x == 0 || y == 0 || z == 0 || x == dims[0] - 1 || y == dims[1] - 1 || z == dims[2] - 1 {
                    border[lab[idx(x, y, z)] as usize] = true;
                }
            }
        }
    }
    let mut out: Vec<Vec<[i64; 3]>> = vec![Vec::new(); count as usize + 1];
    for x in 0..dims[0] {
        for y in 0..dims[1] {
            for z in 0..dims[2] {
                let k = lab[idx(x, y, z)] as usize;
                if k > 0 && !border[k] {
                    out[k].push([x as i64 + lo[0], y as i64 + lo[1], z as i64 + lo[2]]);
                }
            }
        }
    }
    (1..=count as usize).filter(|&k| !border[k]).map(|k| std::mem::take(&mut out[k])).collect()
}

/// cells: [(i,j,k)] → [set(空洞セル)]
#[pyfunction]
pub fn airtight_find_cavities<'py>(py: Python<'py>, cells: Vec<(i64, i64, i64)>) -> PyResult<Vec<Bound<'py, PySet>>> {
    let c: Vec<[i64; 3]> = cells.into_iter().map(|(a, b, c)| [a, b, c]).collect();
    let cav = py.allow_threads(|| find_cavities(&c));
    cav.into_iter()
        .map(|cells| {
            let items: Vec<Bound<'py, PyTuple>> = cells.iter().map(|p| PyTuple::new_bound(py, p)).collect();
            PySet::new_bound(py, &items)
        })
        .collect()
}

/// 空気の量 (kg) と体積 (m³) → 気圧 (kPa)
#[pyfunction]
pub fn airtight_pressure_kpa(air_kg: f64, volume_m3: f64) -> f64 {
    if volume_m3 <= 0.0 {
        return 0.0;
    }
    air_kg / volume_m3 / AIR_DENSITY_1ATM * ATM_KPA
}

/// 気圧 (kPa) にするのに要る空気の量 (kg)
#[pyfunction]
pub fn airtight_air_for_pressure(kpa: f64, volume_m3: f64) -> f64 {
    kpa / ATM_KPA * AIR_DENSITY_1ATM * volume_m3
}
