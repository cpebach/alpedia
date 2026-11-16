//! ワールド：Rust の中だけで使う道具

use super::*;

impl World {
    pub(crate) fn wake_in(slf: &Bound<'_, Self>, woke: &[Bound<'_, PyAny>], dt: f64) -> R<()> {
        for c in woke {
            c.call_method0("wake")?;
            c.setattr("_prev", c.getattr("pos")?.call_method0("copy")?)?;
            Self::_prep_ext(slf, c, dt)?;
        }
        Ok(())
    }
    pub(crate) fn still_sleeping<'py>(sleeping: Vec<Bound<'py, PyAny>>, anchored: &Bound<'py, PyAny>) -> R<Vec<Bound<'py, PyAny>>> {
        let mut out = vec![];
        for c in sleeping {
            if c.getattr("asleep")?.is_truthy()? || anchored.contains(c.getattr("id")?)? {
                out.push(c);
            }
        }
        Ok(out)
    }
    /// 外側の球のリストでの番号 {id(c): k}（リストが変わったときだけ作り直す）
    pub(crate) fn bnd_at<'py>(slf: &Bound<'py, Self>, cs: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
        let py = slf.py();
        if let Some(at) = Self::dict_get(slf, "_bnd_at")? {
            if !at.is_none() && at.get_item(0)?.is(cs) {
                return at.get_item(1);
            }
        }
        let m = PyDict::new_bound(py);
        for (k, c) in cs.iter()?.enumerate() {
            m.set_item(c?.as_ptr() as usize, k)?;
        }
        Self::set(slf, "_bnd_at", (cs, &m))?;
        Ok(m.into_any())
    }
    /// 置くときの向き：列が (右, 上, 前)
    pub(crate) fn frame_of(up: V3, look: V3) -> R<[[f64; 3]; 3]> {
        let mut fwd = sub(look, [up[0] * dot(look, up), up[1] * dot(look, up), up[2] * dot(look, up)]);
        if norm(fwd) < 1e-6 {
            fwd = cross(up, [1.0, 0.0, 0.0]);
        }
        let l = norm(fwd);
        fwd = [fwd[0] / l, fwd[1] / l, fwd[2] / l];
        let right = cross(up, fwd);
        Ok([[right[0], up[0], fwd[0]], [right[1], up[1], fwd[1]], [right[2], up[2], fwd[2]]])
    }
    pub(crate) fn frame_of_import(up: V3, look: V3) -> [[f64; 3]; 3] {
        let mut fwd = sub(look, [up[0] * dot(look, up), up[1] * dot(look, up), up[2] * dot(look, up)]);
        fwd = if norm(fwd) > 1e-6 { let l = norm(fwd); [fwd[0] / l, fwd[1] / l, fwd[2] / l] } else { cross(up, [1.0, 0.0, 0.0]) };
        let right = cross(up, fwd);
        [[right[0], up[0], fwd[0]], [right[1], up[1], fwd[1]], [right[2], up[2], fwd[2]]]
    }
}

#[allow(dead_code)]
pub(crate) fn _unused(py: Python<'_>) -> R<()> {
    let _ = (iv, PyAttributeError::new_err(""), a3(py, &[])?);
    Ok(())
}
