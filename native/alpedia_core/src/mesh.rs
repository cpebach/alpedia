//! 描画用：構築物の表面メッシュの頂点（位置・法線・色・UV）を一度に作る（client/render.py の _update_blob と同じ）。

use crate::kernels;

/// 1つのテクスチャのまとまり
pub struct Group<'a> {
    pub verts: &'a [i64],
    pub cols: &'a [f64],
    pub uv: &'a [f32],
    pub owner: &'a [i64],
}

/// 戻り値：まとまりごとの頂点 (k×12, float32)
#[allow(clippy::too_many_arguments)]
pub fn pack(
    pos: &[f64], vidx: &[i64], w: &[f64], faces: &[i64], origin: [f64; 3], groups: &[Group], sun: [f64; 3], ambient: f64,
    firing: Option<&[f64]>,
) -> Vec<Vec<f32>> {
    let m = vidx.len() / 8;
    let mut v = vec![0.0; 3 * m];
    kernels::skin(pos, vidx, w, &mut v);
    for i in 0..m {
        for a in 0..3 {
            v[3 * i + a] -= origin[a];
        }
    }
    let mut n = vec![0.0; 3 * m];
    kernels::vertex_normals(&v, faces, &mut n);
    let fire = firing.filter(|f| f.iter().any(|&x| x != 0.0));
    groups
        .iter()
        .map(|g| {
            let mut out = vec![0f32; 12 * g.verts.len()];
            for (k, &vi) in g.verts.iter().enumerate() {
                let i = vi as usize;
                let nn = [n[3 * i], n[3 * i + 1], n[3 * i + 2]];
                let lam = (nn[0] * sun[0] + nn[1] * sun[1] + nn[2] * sun[2]).clamp(0.0, 1.0);
                let kk = ambient + (1.0 - ambient) * lam;
                let mut col = [g.cols[3 * k] * kk, g.cols[3 * k + 1] * kk, g.cols[3 * k + 2] * kk];
                if let Some(f) = fire {
                    let fl = f[g.owner[k] as usize];
                    let hot = [1.0, 0.75, 0.25];
                    for a in 0..3 {
                        col[a] = col[a] * (1.0 - fl * 0.6) + hot[a] * fl;
                    }
                }
                let o = &mut out[12 * k..12 * k + 12];
                for a in 0..3 {
                    o[a] = v[3 * i + a] as f32;
                    o[3 + a] = nn[a] as f32;
                    o[6 + a] = col[a] as f32;
                }
                o[9] = 1.0;
                o[10] = g.uv[2 * k];
                o[11] = g.uv[2 * k + 1];
            }
            out
        })
        .collect()
}
