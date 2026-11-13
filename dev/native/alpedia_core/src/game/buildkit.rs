//! ブロックで建物・船・飾りを組み立てる道具（シナリオ・場所の MOD が使う）。main/buildkit.py の中身。
//!
//! Kit にセル（整数の (x, y, z)。+Y が上、+Z が前）ごとに素材・機器を置いていき、voxels() で構築物のボクセルの
//! リスト [(セル, 素材, 機器)] にする。あとから置いた物が前の物を上書きする。clear で消す。
//! セルの辞書 v は MOD が直接読み書きするので Python の dict のまま持つ。

use std::collections::{BTreeSet, HashMap, HashSet};

use numpy::{PyArray1, PyArrayMethods, PyReadonlyArray1, PyReadonlyArray2};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

type R<T> = PyResult<T>;
type Cell = (i64, i64, i64);

const N6: [Cell; 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];

fn int_of(v: &Bound<'_, PyAny>) -> R<i64> {
    crate::game::imp(v.py(), "builtins")?.getattr("int")?.call1((v,))?.extract()
}

fn facing(f: &str) -> R<i64> {
    Ok(match f {
        "+x" => 0,
        "-x" => 1,
        "+y" => 2,
        "-y" => 3,
        "+z" => 4,
        "-z" => 5,
        _ => return Err(pyo3::exceptions::PyKeyError::new_err(f.to_string())),
    })
}

/// 前の向き f → (横の向き, 前の向き)（セルの差）
fn axes(f: &str) -> R<(Cell, Cell)> {
    let fw: Cell = match f {
        "+z" => (0, 0, 1),
        "-z" => (0, 0, -1),
        "+x" => (1, 0, 0),
        "-x" => (-1, 0, 0),
        _ => return Err(pyo3::exceptions::PyKeyError::new_err(f.to_string())),
    };
    let ax = if fw.0 != 0 { (fw.2, 0, -fw.0) } else { (if fw.2 > 0 { 1 } else { -1 }, 0, 0) };
    Ok((ax, fw))
}

fn add(p: Cell, d: Cell, k: i64) -> Cell {
    (p.0 + d.0 * k, p.1 + d.1 * k, p.2 + d.2 * k)
}

fn rng(a: f64, b: f64) -> std::ops::RangeInclusive<i64> {
    (a.min(b) as i64)..=(a.max(b) as i64)
}

fn inside(c: Cell, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> bool {
    let (x, y, z) = (c.0 as f64, c.1 as f64, c.2 as f64);
    x0.min(x1) <= x && x <= x0.max(x1) && y0.min(y1) <= y && y <= y0.max(y1) && z0.min(z1) <= z && z <= z0.max(z1)
}

fn dict_copy<'py>(d: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let o = PyDict::new_bound(d.py());
    o.update(d.downcast::<pyo3::types::PyMapping>()?)?;
    Ok(o.into_any())
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Kit {
    /// セル (x, y, z) → (素材, 機器 dict か None)
    #[pyo3(get, set)]
    v: Py<PyDict>,
}

impl Kit {
    fn set(&self, py: Python<'_>, c: Cell, mat: &Bound<'_, PyAny>, dev: Option<&Bound<'_, PyAny>>) -> R<()> {
        let dv = match dev {
            Some(d) if d.is_truthy()? => dict_copy(d)?.unbind(),
            _ => py.None(),
        };
        self.v.bind(py).set_item(c, (mat, dv))?;
        Ok(())
    }

    fn set_s(&self, py: Python<'_>, c: Cell, mat: &str) -> R<()> {
        self.v.bind(py).set_item(c, (mat, py.None()))?;
        Ok(())
    }

    fn cells(&self, py: Python<'_>) -> R<Vec<Cell>> {
        self.v.bind(py).keys().iter().map(|k| k.extract::<Cell>()).collect()
    }
}

#[pymethods]
impl Kit {
    #[new]
    fn new(py: Python<'_>) -> Self {
        Kit { v: PyDict::new_bound(py).unbind() }
    }

    #[pyo3(signature = (x, y, z, mat, dev=None))]
    fn put<'py>(slf: Bound<'py, Self>, x: &Bound<'py, PyAny>, y: &Bound<'py, PyAny>, z: &Bound<'py, PyAny>, mat: &Bound<'py, PyAny>,
                dev: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, Self>> {
        let c = (int_of(x)?, int_of(y)?, int_of(z)?);
        slf.borrow().set(slf.py(), c, mat, dev)?;
        Ok(slf)
    }

    fn get<'py>(&self, py: Python<'py>, x: &Bound<'py, PyAny>, y: &Bound<'py, PyAny>, z: &Bound<'py, PyAny>) -> R<PyObject> {
        let c = (int_of(x)?, int_of(y)?, int_of(z)?);
        Ok(self.v.bind(py).get_item(c)?.map(|x| x.unbind()).unwrap_or_else(|| py.None()))
    }

    fn clear(slf: Bound<'_, Self>, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> R<Bound<'_, Self>> {
        let py = slf.py();
        {
            let me = slf.borrow();
            let v = me.v.bind(py);
            for c in me.cells(py)? {
                if inside(c, x0, y0, z0, x1, y1, z1) {
                    v.del_item(c)?;
                }
            }
        }
        Ok(slf)
    }

    /// 両端を含む箱を詰める
    fn fill<'py>(slf: Bound<'py, Self>, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64, mat: &Bound<'py, PyAny>) -> R<Bound<'py, Self>> {
        let py = slf.py();
        {
            let me = slf.borrow();
            let v = me.v.bind(py);
            for x in rng(x0, x1) {
                for y in rng(y0, y1) {
                    for z in rng(z0, z1) {
                        v.set_item((x, y, z), (mat, py.None()))?;
                    }
                }
            }
        }
        Ok(slf)
    }

    /// 壁・床・天井だけの箱（中は空き）。floor・roof が None なら壁の素材、False なら作らない
    #[pyo3(signature = (x0, y0, z0, x1, y1, z1, wall, floor=None, roof=None))]
    #[allow(clippy::too_many_arguments)]
    fn shell<'py>(slf: Bound<'py, Self>, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64, wall: &Bound<'py, PyAny>,
                  floor: Option<&Bound<'py, PyAny>>, roof: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, Self>> {
        let py = slf.py();
        let is_false = |o: Option<&Bound<'py, PyAny>>| -> bool {
            o.map_or(false, |x| x.is_instance_of::<pyo3::types::PyBool>() && !x.is_truthy().unwrap_or(true))
        };
        let pick = |o: Option<&Bound<'py, PyAny>>| -> R<Bound<'py, PyAny>> {
            Ok(match o {
                Some(x) if x.is_truthy()? => x.clone(),
                _ => wall.clone(),
            })
        };
        let (fl, rf) = (is_false(floor), is_false(roof));
        let (fm, rm) = (pick(floor)?, pick(roof)?);
        {
            let me = slf.borrow();
            let v = me.v.bind(py);
            for x in rng(x0, x1) {
                for y in rng(y0, y1) {
                    for z in rng(z0, z1) {
                        let (xf, yf, zf) = (x as f64, y as f64, z as f64);
                        let side = xf == x0 || xf == x1 || zf == z0 || zf == z1;
                        if yf == y0 && !fl {
                            v.set_item((x, y, z), (&fm, py.None()))?;
                        } else if yf == y1 && !rf {
                            v.set_item((x, y, z), (&rm, py.None()))?;
                        } else if side && y0 <= yf && yf <= y1 {
                            v.set_item((x, y, z), (wall, py.None()))?;
                        }
                    }
                }
            }
        }
        Ok(slf)
    }

    /// 範囲の中の今あるブロックの素材を替える（only：その素材のものだけ）
    #[pyo3(signature = (mat, x0, y0, z0, x1, y1, z1, only=None))]
    #[allow(clippy::too_many_arguments)]
    fn paint<'py>(slf: Bound<'py, Self>, mat: &Bound<'py, PyAny>, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64,
                  only: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, Self>> {
        let py = slf.py();
        {
            let me = slf.borrow();
            let v = me.v.bind(py);
            let items: Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)> = v.iter().collect();
            for (k, val) in items {
                let c: Cell = k.extract()?;
                let m = val.get_item(0)?;
                let d = val.get_item(1)?;
                if inside(c, x0, y0, z0, x1, y1, z1) && !d.is_truthy()? && only.map_or(Ok(true), |o| if o.is_none() { Ok(true) } else { m.eq(o) })? {
                    v.set_item(c, (mat, py.None()))?;
                }
            }
        }
        Ok(slf)
    }

    /// 壁に出入り口（範囲を空ける）
    fn door(slf: Bound<'_, Self>, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> R<Bound<'_, Self>> {
        Kit::clear(slf, x0, y0, z0, x1, y1, z1)
    }

    /// 範囲の壁をガラスに
    #[pyo3(signature = (x0, y0, z0, x1, y1, z1, mat=None))]
    #[allow(clippy::too_many_arguments)]
    fn windows<'py>(slf: Bound<'py, Self>, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64, mat: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, Self>> {
        let py = slf.py();
        let m = match mat {
            Some(m) => m.clone(),
            None => pyo3::types::PyString::new_bound(py, "glass").into_any(),
        };
        Kit::paint(slf, &m, x0, y0, z0, x1, y1, z1, None)
    }

    /// 半球の殻（中心 (cx, cy, cz)、半径 r セル）
    #[pyo3(signature = (cx, cy, cz, r, mat, thick=1.0, cut_floor=true))]
    #[allow(clippy::too_many_arguments)]
    fn dome<'py>(slf: Bound<'py, Self>, cx: f64, cy: f64, cz: f64, r: f64, mat: &Bound<'py, PyAny>, thick: f64, cut_floor: bool) -> R<Bound<'py, Self>> {
        let (cx, cy, cz) = (cx as i64, cy as i64, cz as i64);
        let _ = cut_floor;
        let py = slf.py();
        {
            let me = slf.borrow();
            let v = me.v.bind(py);
            let rr = r.ceil() as i64;
            for x in -rr..=rr {
                for y in 0..=rr {
                    for z in -rr..=rr {
                        let d = ((x * x + y * y + z * z) as f64).sqrt();
                        if r - thick <= d && d <= r {
                            v.set_item((cx + x, cy + y, cz + z), (mat, py.None()))?;
                        }
                    }
                }
            }
        }
        Ok(slf)
    }

    /// だ円体（半径 a・b・c セル。X・Y・Z の向き）。殻だけ（solid で詰める）
    #[pyo3(signature = (cx, cy, cz, a, b, c, mat, thick=1.2, solid=false))]
    #[allow(clippy::too_many_arguments)]
    fn ellipsoid<'py>(slf: Bound<'py, Self>, cx: f64, cy: f64, cz: f64, a: f64, b: f64, c: f64, mat: &Bound<'py, PyAny>, thick: f64,
                      solid: bool) -> R<Bound<'py, Self>> {
        let (cx, cy, cz) = (cx as i64, cy as i64, cz as i64);
        let py = slf.py();
        {
            let me = slf.borrow();
            let v = me.v.bind(py);
            let (aa, bb, cc) = (a.ceil() as i64, b.ceil() as i64, c.ceil() as i64);
            for x in -aa..=aa {
                for y in -bb..=bb {
                    for z in -cc..=cc {
                        let (xf, yf, zf) = (x as f64, y as f64, z as f64);
                        let q = (xf / a).powi(2) + (yf / b).powi(2) + (zf / c).powi(2);
                        if q > 1.0 {
                            continue;
                        }
                        if !solid {
                            let s = a.min(b).min(c);
                            let inner = (xf / (a - thick).max(0.5)).powi(2) + (yf / (b - thick).max(0.5)).powi(2)
                                + (zf / (c - thick).max(0.5)).powi(2);
                            if inner <= 1.0 && s > thick {
                                continue;
                            }
                        }
                        v.set_item((cx + x, cy + y, cz + z), (mat, py.None()))?;
                    }
                }
            }
        }
        Ok(slf)
    }

    #[pyo3(signature = (cx, y0, cz, r, h, mat, hollow=false))]
    #[allow(clippy::too_many_arguments)]
    fn cylinder<'py>(slf: Bound<'py, Self>, cx: f64, y0: f64, cz: f64, r: f64, h: i64, mat: &Bound<'py, PyAny>, hollow: bool) -> R<Bound<'py, Self>> {
        let (cx, y0, cz) = (cx as i64, y0 as i64, cz as i64);
        let py = slf.py();
        {
            let me = slf.borrow();
            let v = me.v.bind(py);
            let rr = r.ceil() as i64;
            for x in -rr..=rr {
                for z in -rr..=rr {
                    let d = (x as f64).hypot(z as f64);
                    if d > r || (hollow && d < r - 1.0) {
                        continue;
                    }
                    for y in y0..y0 + h {
                        v.set_item((cx + x, y, cz + z), (mat, py.None()))?;
                    }
                }
            }
        }
        Ok(slf)
    }

    #[pyo3(signature = (other, dx=0, dy=0, dz=0))]
    fn merge<'py>(slf: Bound<'py, Self>, other: &Bound<'py, PyAny>, dx: i64, dy: i64, dz: i64) -> R<Bound<'py, Self>> {
        let py = slf.py();
        {
            let me = slf.borrow();
            let v = me.v.bind(py);
            let ov = other.getattr("v")?;
            for it in ov.call_method0("items")?.iter()? {
                let (k, md): (Cell, Bound<'py, PyAny>) = it?.extract()?;
                v.set_item((k.0 + dx, k.1 + dy, k.2 + dz), md)?;
            }
        }
        Ok(slf)
    }

    /// 構築物のボクセルのリスト [(セル, 素材, 機器)]（セルの順）
    fn voxels<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyList>> {
        let v = self.v.bind(py);
        let mut items: Vec<(Cell, Bound<'py, PyAny>)> = v.iter().map(|(k, x)| Ok((k.extract::<Cell>()?, x))).collect::<R<_>>()?;
        items.sort_by_key(|x| x.0);
        let out = PyList::empty_bound(py);
        for (c, md) in items {
            let m = md.get_item(0)?;
            let d = md.get_item(1)?;
            let d = if d.is_truthy()? { dict_copy(&d)? } else { py.None().into_bound(py) };
            out.append((c, m, d))?;
        }
        Ok(out)
    }

    fn __len__(&self, py: Python<'_>) -> usize {
        self.v.bind(py).len()
    }

    // --- 家具（向き：f は前の向き "+z"・"-z"・"+x"・"-x"） ---------------------
    /// 机：天板（横 w）と両端の脚
    #[pyo3(signature = (x, y, z, f="+z", mat="wood", w=3))]
    fn desk<'py>(slf: Bound<'py, Self>, x: f64, y: f64, z: f64, f: &str, mat: &str, w: i64) -> R<Bound<'py, Self>> {
        let (x, y, z) = (x as i64, y as i64, z as i64);
        let py = slf.py();
        let (ax, _) = axes(f)?;
        {
            let me = slf.borrow();
            for k in 0..w {
                let p = add((x, y, z), ax, k);
                me.set_s(py, (p.0, p.1 + 1, p.2), mat)?;
            }
            for k in [0, w - 1] {
                me.set_s(py, add((x, y, z), ax, k), mat)?;
            }
        }
        Ok(slf)
    }

    /// 椅子：座面と背もたれ（f の向きに座る）
    #[pyo3(signature = (x, y, z, f="+z", mat="hull_dark"))]
    fn chair<'py>(slf: Bound<'py, Self>, x: f64, y: f64, z: f64, f: &str, mat: &str) -> R<Bound<'py, Self>> {
        let (x, y, z) = (x as i64, y as i64, z as i64);
        let py = slf.py();
        let (_, fw) = axes(f)?;
        {
            let me = slf.borrow();
            me.set_s(py, (x, y, z), mat)?;
            let b = add((x, y, z), fw, -1);
            me.set_s(py, (b.0, y + 1, b.2), mat)?;
        }
        Ok(slf)
    }

    #[pyo3(signature = (x, y, z, w=2, d=2, mat="wood"))]
    fn table<'py>(slf: Bound<'py, Self>, x: f64, y: f64, z: f64, w: i64, d: i64, mat: &str) -> R<Bound<'py, Self>> {
        let (x, y, z) = (x as i64, y as i64, z as i64);
        let py = slf.py();
        {
            let me = slf.borrow();
            for i in 0..w {
                for j in 0..d {
                    me.set_s(py, (x + i, y + 1, z + j), mat)?;
                }
            }
            me.set_s(py, (x, y, z), mat)?;
            me.set_s(py, (x + w - 1, y, z + d - 1), mat)?;
        }
        Ok(slf)
    }

    #[pyo3(signature = (x, y, z, f="+z", w=3, mat="carpet_red"))]
    fn sofa<'py>(slf: Bound<'py, Self>, x: f64, y: f64, z: f64, f: &str, w: i64, mat: &str) -> R<Bound<'py, Self>> {
        let (x, y, z) = (x as i64, y as i64, z as i64);
        let py = slf.py();
        let (ax, fw) = axes(f)?;
        {
            let me = slf.borrow();
            for k in 0..w {
                let p = add((x, y, z), ax, k);
                me.set_s(py, p, mat)?;
                let b = add(p, fw, -1);
                me.set_s(py, (b.0, y, b.2), mat)?;
                me.set_s(py, (b.0, y + 1, b.2), mat)?;
            }
        }
        Ok(slf)
    }

    /// ベッド：1×3 の敷き布団と枕元の板
    #[pyo3(signature = (x, y, z, f="+z", mat="cloth_white", frame="wood"))]
    fn bed<'py>(slf: Bound<'py, Self>, x: f64, y: f64, z: f64, f: &str, mat: &str, frame: &str) -> R<Bound<'py, Self>> {
        let (x, y, z) = (x as i64, y as i64, z as i64);
        let py = slf.py();
        let (_, fw) = axes(f)?;
        {
            let me = slf.borrow();
            for k in 0..3 {
                me.set_s(py, add((x, y, z), fw, k), mat)?;
            }
            let h = add((x, y, z), fw, -1);
            me.set_s(py, h, frame)?;
            me.set_s(py, (h.0, y + 1, h.2), frame)?;
        }
        Ok(slf)
    }

    /// 棚：背板と、段ごとに品物
    #[pyo3(signature = (x, y, z, f="+z", w=3, h=4, mat="wood", goods=None))]
    #[allow(clippy::too_many_arguments)]
    fn shelf<'py>(slf: Bound<'py, Self>, x: f64, y: f64, z: f64, f: &str, w: i64, h: i64, mat: &str, goods: Option<Vec<String>>) -> R<Bound<'py, Self>> {
        let (x, y, z) = (x as i64, y as i64, z as i64);
        let py = slf.py();
        let goods = goods.unwrap_or_else(|| ["hull_red", "hull_blue", "hull_yellow", "cloth_white"].iter().map(|s| s.to_string()).collect());
        let (ax, _) = axes(f)?;
        {
            let me = slf.borrow();
            for k in 0..w {
                let p = add((x, y, z), ax, k);
                for j in 0..h {
                    let m = if j % 2 == 0 { mat.to_string() } else { goods[((k + j) as usize) % goods.len()].clone() };
                    me.set_s(py, (p.0, y + j, p.2), &m)?;
                }
            }
        }
        Ok(slf)
    }

    /// 操作盤：台と、表示つきのコンピューター（f の向きに表示）
    #[pyo3(signature = (x, y, z, f="+z", text="", mat="hull_dark", width=1.2, color=(0.6, 1.0, 0.8)))]
    #[allow(clippy::too_many_arguments)]
    fn console<'py>(slf: Bound<'py, Self>, x: f64, y: f64, z: f64, f: &str, text: &str, mat: &str, width: f64, color: (f64, f64, f64))
        -> R<Bound<'py, Self>> {
        let (x, y, z) = (x as i64, y as i64, z as i64);
        let py = slf.py();
        let face = facing(f)?;
        {
            let me = slf.borrow();
            me.set_s(py, (x, y, z), mat)?;
            let st = PyDict::new_bound(py);
            if !text.is_empty() {
                let disp = PyDict::new_bound(py);
                disp.set_item("text", text)?;
                disp.set_item("face", face)?;
                disp.set_item("width", width)?;
                disp.set_item("color", vec![color.0, color.1, color.2])?;
                disp.set_item("background", vec![0.02, 0.05, 0.06, 0.92])?;
                st.set_item("display", disp)?;
            }
            let dev = PyDict::new_bound(py);
            dev.set_item("def", "computer")?;
            dev.set_item("dir", face)?;
            dev.set_item("name", "操作盤")?;
            dev.set_item("state", st)?;
            me.set(py, (x, y + 1, z), &pyo3::types::PyString::new_bound(py, "steel").into_any(), Some(dev.as_any()))?;
        }
        Ok(slf)
    }

    /// 植木鉢・畑：土の箱と、その上の植物
    #[pyo3(signature = (x, y, z, w=2, d=1, h=2))]
    fn planter<'py>(slf: Bound<'py, Self>, x: f64, y: f64, z: f64, w: i64, d: i64, h: i64) -> R<Bound<'py, Self>> {
        let (x, y, z) = (x as i64, y as i64, z as i64);
        let py = slf.py();
        {
            let me = slf.borrow();
            for i in 0..w {
                for j in 0..d {
                    me.set_s(py, (x + i, y, z + j), "soil")?;
                    for k in 1..h {
                        me.set_s(py, (x + i, y + k, z + j), "plant")?;
                    }
                }
            }
        }
        Ok(slf)
    }

    #[pyo3(signature = (x, y, z, n=3, mat="wood"))]
    fn crates<'py>(slf: Bound<'py, Self>, x: f64, y: f64, z: f64, n: usize, mat: &str) -> R<Bound<'py, Self>> {
        let (x, y, z) = (x as i64, y as i64, z as i64);
        let py = slf.py();
        let spots = [(0, 0, 0), (1, 0, 0), (0, 0, 1), (0, 1, 0), (1, 0, 1)];
        {
            let me = slf.borrow();
            for (dx, dy, dz) in spots.iter().take(n) {
                me.set_s(py, (x + dx, y + dy, z + dz), mat)?;
            }
        }
        Ok(slf)
    }

    /// ライト（電力で光るブロック）。color：光の色、group：まとめて点ける名前、blink：点滅の周期（秒）
    #[pyo3(signature = (x, y, z, color=None, group=None, blink=None, on=None, name=None))]
    #[allow(clippy::too_many_arguments)]
    fn light<'py>(slf: Bound<'py, Self>, x: &Bound<'py, PyAny>, y: &Bound<'py, PyAny>, z: &Bound<'py, PyAny>, color: Option<&Bound<'py, PyAny>>,
                  group: Option<&Bound<'py, PyAny>>, blink: Option<&Bound<'py, PyAny>>, on: Option<&Bound<'py, PyAny>>, name: Option<&Bound<'py, PyAny>>)
        -> R<Bound<'py, Self>> {
        let py = slf.py();
        let st = PyDict::new_bound(py);
        st.set_item("enabled", on.map_or(Ok(true), |o| o.is_truthy())?)?;
        if let Some(c) = color.filter(|c| !c.is_none()) {
            let fl = crate::game::imp(py, "builtins")?.getattr("float")?;
            let l: Vec<f64> = c.iter()?.map(|v| fl.call1((v?,))?.extract::<f64>()).collect::<R<_>>()?;
            st.set_item("color", l)?;
        }
        if let Some(g) = group.filter(|g| g.is_truthy().unwrap_or(false)) {
            st.set_item("group", g)?;
        }
        if let Some(b) = blink.filter(|b| b.is_truthy().unwrap_or(false)) {
            st.set_item("blink", crate::game::imp(py, "builtins")?.getattr("float")?.call1((b,))?)?;
        }
        let dev = PyDict::new_bound(py);
        dev.set_item("def", "light")?;
        dev.set_item("dir", 2)?;
        match name {
            Some(n) => dev.set_item("name", n)?,
            None => dev.set_item("name", "明かり")?,
        }
        dev.set_item("state", st)?;
        Kit::put(slf, x, y, z, &pyo3::types::PyString::new_bound(py, "glass").into_any(), Some(dev.as_any()))
    }

    /// 明かりなどの電源（満タンのバッテリー）
    #[pyo3(signature = (x, y, z, charge=2.0e7))]
    fn battery<'py>(slf: Bound<'py, Self>, x: &Bound<'py, PyAny>, y: &Bound<'py, PyAny>, z: &Bound<'py, PyAny>, charge: f64) -> R<Bound<'py, Self>> {
        let py = slf.py();
        let st = PyDict::new_bound(py);
        st.set_item("charge", charge)?;
        let dev = PyDict::new_bound(py);
        dev.set_item("def", "battery")?;
        dev.set_item("dir", 2)?;
        dev.set_item("name", "バッテリー")?;
        dev.set_item("state", st)?;
        Kit::put(slf, x, y, z, &pyo3::types::PyString::new_bound(py, "steel").into_any(), Some(dev.as_any()))
    }

    #[pyo3(signature = (x0, y, z0, x1, z1, mat=None))]
    #[allow(clippy::too_many_arguments)]
    fn rug<'py>(slf: Bound<'py, Self>, x0: f64, y: f64, z0: f64, x1: f64, z1: f64, mat: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, Self>> {
        let py = slf.py();
        let m = match mat {
            Some(m) => m.clone(),
            None => pyo3::types::PyString::new_bound(py, "carpet_red").into_any(),
        };
        Kit::fill(slf, x0, y, z0, x1, y, z1, &m)
    }
}

// ---------------------------------------------------------------------------
// アンカー
// ---------------------------------------------------------------------------
struct Vox<'py> {
    cell: Cell,
    mat: Bound<'py, PyAny>,
    dev: Bound<'py, PyAny>,
}

fn read_vox<'py>(v: &Bound<'py, PyAny>) -> R<Vec<Vox<'py>>> {
    let mut out = Vec::new();
    for it in v.iter()? {
        let it = it?;
        let c = it.get_item(0)?;
        let cell: Cell = (c.get_item(0)?.extract()?, c.get_item(1)?.extract()?, c.get_item(2)?.extract()?);
        out.push(Vox { cell, mat: it.get_item(1)?, dev: it.get_item(2)? });
    }
    Ok(out)
}

fn dev_def_is(d: &Bound<'_, PyAny>, names: &[&str]) -> R<bool> {
    if !d.is_truthy()? {
        return Ok(false);
    }
    let def = d.call_method1("get", ("def",))?;
    for n in names {
        if def.eq(*n)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// アクチュエーターに動かされない部分（どのアクチュエーターの出力の先でもない部分）のセル
fn root_cells_of(vox: &[Vox<'_>]) -> R<HashSet<Cell>> {
    let cells: HashSet<Cell> = vox.iter().map(|v| v.cell).collect();
    let mut outs: Vec<(Cell, Cell)> = Vec::new();
    for v in vox {
        if dev_def_is(&v.dev, &["actuator", "piston", "hinge"])? {
            let d = int_of(&v.dev.call_method1("get", ("dir", 2))?)?;
            let n = N6[d.rem_euclid(6) as usize];
            outs.retain(|x| x.0 != v.cell);
            outs.push((v.cell, (v.cell.0 + n.0, v.cell.1 + n.1, v.cell.2 + n.2)));
        }
    }
    let cut: HashSet<(Cell, Cell)> = outs.iter().flat_map(|&(a, b)| [(a, b), (b, a)]).collect();
    let mut idx: HashMap<Cell, usize> = HashMap::new();
    let list: Vec<Cell> = cells.iter().copied().collect();
    for (k, c) in list.iter().enumerate() {
        idx.insert(*c, k);
    }
    let mut parent: Vec<usize> = (0..list.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for (k, c) in list.iter().enumerate() {
        for n in N6 {
            let nb = (c.0 + n.0, c.1 + n.1, c.2 + n.2);
            if let Some(&j) = idx.get(&nb) {
                if !cut.contains(&(*c, nb)) {
                    let (a, b) = (find(&mut parent, k), find(&mut parent, j));
                    parent[a] = b;
                }
            }
        }
    }
    let driven: HashSet<usize> = outs.iter().filter_map(|(_, b)| idx.get(b).copied()).map(|j| find(&mut parent, j)).collect();
    let roots: Vec<usize> = (0..list.len()).map(|k| find(&mut parent, k)).collect();
    Ok(list.iter().enumerate().filter(|(k, _)| !driven.contains(&roots[*k])).map(|(_, c)| *c).collect())
}

#[pyfunction]
pub fn bk_root_cells(v: &Bound<'_, PyAny>) -> R<HashSet<Cell>> {
    root_cells_of(&read_vox(v)?)
}

fn pick_corners(foot: &[(i64, i64)], step: i64) -> BTreeSet<(i64, i64)> {
    let xs: Vec<i64> = foot.iter().map(|c| c.0).collect();
    let zs: Vec<i64> = foot.iter().map(|c| c.1).collect();
    let (x0, x1) = (*xs.iter().min().unwrap(), *xs.iter().max().unwrap());
    let (z0, z1) = (*zs.iter().min().unwrap(), *zs.iter().max().unwrap());
    let mut pick: BTreeSet<(i64, i64)> = foot.iter().copied().filter(|c| (c.0 - x0).rem_euclid(step) == 0 && (c.1 - z0).rem_euclid(step) == 0).collect();
    for cx in [x0, x1] {
        for cz in [z0, z1] {
            // 四隅にいちばん近い所（同じ近さなら並びの最初）
            let mut best = foot[0];
            let mut bd = i64::MAX;
            for c in foot {
                let d = (c.0 - cx).pow(2) + (c.1 - cz).pow(2);
                if d < bd {
                    bd = d;
                    best = *c;
                }
            }
            pick.insert(best);
        }
    }
    pick
}

/// V の足もと（いちばん下の段で、アクチュエーターに動かされない部分）の下に置くアンカーのセル
fn anchor_cells_of(vox: &[Vox<'_>], step: i64) -> R<Vec<Cell>> {
    let root = root_cells_of(vox)?;
    if root.is_empty() {
        return Ok(Vec::new());
    }
    let y0 = root.iter().map(|c| c.1).min().unwrap();
    let mut foot: Vec<Cell> = root.iter().copied().filter(|c| c.1 == y0).collect();
    foot.sort();
    let f2: Vec<(i64, i64)> = foot.iter().map(|c| (c.0, c.2)).collect();
    let pick = pick_corners(&f2, step);
    Ok(pick.into_iter().map(|(x, z)| (x, y0 - 1, z)).collect())
}

#[pyfunction]
#[pyo3(signature = (v, step=2))]
pub fn bk_anchor_cells(v: &Bound<'_, PyAny>, step: i64) -> R<Vec<Cell>> {
    anchor_cells_of(&read_vox(v)?, step)
}

fn anchor_dev(py: Python<'_>) -> R<Bound<'_, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("def", "anchor")?;
    d.set_item("dir", 2)?;
    d.set_item("name", "アンカー")?;
    Ok(d)
}

/// V にアンカーを足したもの（足もとの下に埋める）
#[pyfunction]
#[pyo3(signature = (v, step=2))]
pub fn bk_anchored<'py>(py: Python<'py>, v: &Bound<'py, PyAny>, step: i64) -> R<Bound<'py, PyList>> {
    let vox = read_vox(v)?;
    let out = PyList::new_bound(py, v.iter()?.collect::<R<Vec<_>>>()?);
    for c in anchor_cells_of(&vox, step)? {
        out.append((c, "steel", anchor_dev(py)?))?;
    }
    Ok(out)
}

/// V の中のセル cells をアンカーに置き換えたもの（宇宙の構築物など、下に地面がないとき）
#[pyfunction]
pub fn bk_anchored_inside<'py>(py: Python<'py>, v: &Bound<'py, PyAny>, cells: &Bound<'py, PyAny>) -> R<Bound<'py, PyList>> {
    let mut set: HashSet<Cell> = HashSet::new();
    for c in cells.iter()? {
        let c = c?;
        set.insert((c.get_item(0)?.extract()?, c.get_item(1)?.extract()?, c.get_item(2)?.extract()?));
    }
    let out = PyList::empty_bound(py);
    for it in v.iter()? {
        let it = it?;
        let c = it.get_item(0)?;
        let cell: Cell = (c.get_item(0)?.extract()?, c.get_item(1)?.extract()?, c.get_item(2)?.extract()?);
        let d = it.get_item(2)?;
        if set.contains(&cell) && !d.is_truthy()? {
            out.append((c, "steel", anchor_dev(py)?))?;
        } else {
            out.append((c, it.get_item(1)?, d))?;
        }
    }
    Ok(out)
}

/// 真ん中あたりの、機器の無いブロック1つをアンカーに（宇宙に浮かぶ船など）
#[pyfunction]
pub fn bk_anchor_center<'py>(py: Python<'py>, v: &Bound<'py, PyAny>) -> R<Bound<'py, PyList>> {
    let vox = read_vox(v)?;
    let cells: Vec<Cell> = vox.iter().filter(|x| !x.dev.is_truthy().unwrap_or(true)).map(|x| x.cell).collect();
    if cells.is_empty() {
        return Ok(PyList::new_bound(py, v.iter()?.collect::<R<Vec<_>>>()?));
    }
    let n = cells.len() as f64;
    let m = cells.iter().fold([0.0; 3], |a, c| [a[0] + c.0 as f64, a[1] + c.1 as f64, a[2] + c.2 as f64]);
    let m = [m[0] / n, m[1] / n, m[2] / n];
    let mut best = 0;
    let mut bd = f64::INFINITY;
    for (k, c) in cells.iter().enumerate() {
        let d = ((c.0 as f64 - m[0]).powi(2) + (c.1 as f64 - m[1]).powi(2) + (c.2 as f64 - m[2]).powi(2)).sqrt();
        if d < bd {
            bd = d;
            best = k;
        }
    }
    let one = PyList::new_bound(py, [cells[best]]);
    bk_anchored_inside(py, v, one.as_any())
}

/// 足もとと地面のすき間を土台のブロックで埋め、その下にアンカーを置き直した V（坂に置いても浮かない）
#[pyfunction]
#[pyo3(signature = (v, r, t, size, sdf, mat=None, step=2))]
#[allow(clippy::too_many_arguments)]
pub fn bk_grounded<'py>(py: Python<'py>, v: &Bound<'py, PyAny>, r: PyReadonlyArray2<'py, f64>, t: PyReadonlyArray1<'py, f64>, size: f64,
                        sdf: &Bound<'py, PyAny>, mat: Option<&Bound<'py, PyAny>>, step: i64) -> R<Bound<'py, PyList>> {
    let all = read_vox(v)?;
    let base: Vec<&Vox<'py>> = all.iter().filter(|x| !dev_def_is(&x.dev, &["anchor"]).unwrap_or(false)).collect();
    let base_owned: Vec<Vox<'py>> = base.iter().map(|x| Vox { cell: x.cell, mat: x.mat.clone(), dev: x.dev.clone() }).collect();
    let root = root_cells_of(&base_owned)?;
    if root.is_empty() {
        return Ok(PyList::new_bound(py, v.iter()?.collect::<R<Vec<_>>>()?));
    }
    let y0 = root.iter().map(|c| c.1).min().unwrap();
    let mats: HashMap<Cell, Bound<'py, PyAny>> = base_owned.iter().map(|x| (x.cell, x.mat.clone())).collect();
    let mut foot: Vec<Cell> = root.iter().copied().filter(|c| c.1 == y0).collect();
    foot.sort();
    let ra = r.as_array();
    let ta = t.as_array();
    let mut pts = Vec::with_capacity(foot.len() * 3);
    for (x, _, z) in &foot {
        let l = [(*x as f64 + 0.5) * size, y0 as f64 * size, (*z as f64 + 0.5) * size];
        for a in 0..3 {
            pts.push(ra[[a, 0]] * l[0] + ra[[a, 1]] * l[1] + ra[[a, 2]] * l[2] + ta[a]);
        }
    }
    let p = PyArray1::from_vec_bound(py, pts).reshape([foot.len(), 3])?;
    let h: Vec<f64> = crate::game::imp(py, "numpy")?.getattr("asarray")?.call1((sdf.call1((p,))?, "float64"))?
        .call_method0("ravel")?.call_method0("tolist")?.extract()?;
    let out = PyList::empty_bound(py);
    for x in &base_owned {
        out.append((x.cell, &x.mat, &x.dev))?;
    }
    let mut deep: HashMap<(i64, i64), i64> = HashMap::new();
    for ((x, _, z), hh) in foot.iter().zip(h.iter()) {
        let n = (hh.max(0.0) / size - 0.15).ceil() as i64;
        for k in 1..=n {
            let m = match mat.filter(|m| m.is_truthy().unwrap_or(false)) {
                Some(m) => m.clone(),
                None => mats.get(&(*x, y0, *z)).cloned().unwrap_or_else(|| pyo3::types::PyString::new_bound(py, "concrete").into_any()),
            };
            out.append(((*x, y0 - k, *z), m, py.None()))?;
        }
        deep.insert((*x, *z), y0 - n);
    }
    let f2: Vec<(i64, i64)> = foot.iter().map(|c| (c.0, c.2)).collect();
    for (x, z) in pick_corners(&f2, step) {
        out.append(((x, deep[&(x, z)] - 1, z), "steel", anchor_dev(py)?))?;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 明かりのまとまり
// ---------------------------------------------------------------------------
/// 構築物 c のライト（group が名前ならそのまとまりだけ）の機器のリスト
#[pyfunction]
#[pyo3(signature = (c, group=None))]
pub fn bk_light_devices<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, group: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyList>> {
    let key = (c.getattr("version")?, c.getattr("n_vox")?);
    let key = PyTuple::new_bound(py, [key.0, key.1]);
    let cache = c.getattr("_lgrp_cache").ok().filter(|x| !x.is_none());
    let ids: Bound<'py, PyAny> = match cache {
        Some(cc) if cc.get_item(0)?.eq(&key)? => cc.get_item(1)?,
        _ => {
            let devs = c.getattr("devs")?;
            let reg = c.getattr("reg")?;
            let ids = PyList::empty_bound(py);
            for (i, d) in devs.iter()?.enumerate() {
                let d = d?;
                if !d.is_truthy()? {
                    continue;
                }
                let dd = reg.call_method1("device", (d.get_item("def")?,))?;
                if dd.is_truthy()? && dd.call_method1("get", ("type",))?.eq("light")? {
                    ids.append(i)?;
                }
            }
            c.setattr("_lgrp_cache", (key, &ids))?;
            ids.into_any()
        }
    };
    let devs = c.getattr("devs")?;
    let out = PyList::empty_bound(py);
    for i in ids.iter()? {
        let d = devs.get_item(i?)?;
        let ok = match group {
            Some(g) if !g.is_none() => d.get_item("state")?.call_method1("get", ("group",))?.eq(g)?,
            _ => true,
        };
        if ok {
            out.append(d)?;
        }
    }
    Ok(out)
}

/// まとまり group のライトを全部点ける・消す
#[pyfunction]
pub fn bk_set_light_group(py: Python<'_>, world: &Bound<'_, PyAny>, group: &Bound<'_, PyAny>, on: &Bound<'_, PyAny>) -> R<usize> {
    let on = on.is_truthy()?;
    let mut n = 0;
    for c in world.getattr("constructs")?.call_method0("values")?.iter()? {
        let c = c?;
        for d in bk_light_devices(py, &c, Some(group))?.iter() {
            let st = d.get_item("state")?;
            if st.call_method1("get", ("enabled", true))?.is_truthy()? != on {
                st.set_item("enabled", on)?;
                c.setattr("_slow_dt", 1.0)?;
                n += 1;
            }
        }
    }
    Ok(n)
}
