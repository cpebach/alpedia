//! アイテム（素材・機器）のアイコン画像を作る（main/client/icons.py）。
//!
//! ブロックは球の形なので、アイコンも陰影付きの球にする（球の計算は Rust）。
//! 素材はその色とテクスチャ、機器は機器の色に種類ごとのマーク（炎・タンクの帯・画面など）を重ねる（線や形は PIL で描く）。
//! 画像は起動中だけメモリに置く（ファイルは作らない）。

use std::f64::consts::PI;

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::imp;
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

fn tb<T: IntoPy<PyObject>>(py: Python<'_>, x: T) -> B<'_> {
    x.into_py(py).into_bound(py)
}
fn icons(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.icons")?.into_any())
}
fn pil(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "PIL.Image")?.into_any())
}
fn is_exc(py: Python<'_>, e: &PyErr) -> bool {
    e.is_instance_of::<PyException>(py)
}
type C4 = (i64, i64, i64, i64);

/// PIL の ImageDraw で、画像の真ん中を原点・1 が画像の幅の座標で描く
struct Draw<'py> {
    d: B<'py>,
    c: f64,
    s: f64,
}
impl<'py> Draw<'py> {
    fn new(img: &B<'py>) -> R<Self> {
        let py = img.py();
        let s: f64 = img.getattr("size")?.get_item(0)?.extract()?;
        Ok(Draw { d: imp(py, "PIL.ImageDraw")?.call_method1("Draw", (img,))?, c: s / 2.0, s })
    }
    fn p(&self, x: f64, y: f64) -> (f64, f64) {
        (self.c + x * self.s, self.c - y * self.s)
    }
    fn ps(&self, pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
        pts.iter().map(|&(x, y)| self.p(x, y)).collect()
    }
    fn call(&self, name: &str, xy: PyObject, kw: Vec<(&str, PyObject)>) -> R<()> {
        let py = self.d.py();
        let k = PyDict::new_bound(py);
        for (a, b) in kw {
            k.set_item(a, b)?;
        }
        self.d.call_method(name, (xy,), Some(&k))?;
        Ok(())
    }
    fn polygon(&self, pts: &[(f64, f64)], fill: C4) -> R<()> {
        let py = self.d.py();
        self.call("polygon", self.ps(pts).into_py(py), vec![("fill", fill.into_py(py))])
    }
    fn rect(&self, a: (f64, f64), b: (f64, f64), fill: Option<C4>, outline: Option<C4>, width: Option<i64>) -> R<()> {
        let py = self.d.py();
        let mut kw = vec![];
        if let Some(f) = fill {
            kw.push(("fill", f.into_py(py)));
        }
        if let Some(o) = outline {
            kw.push(("outline", o.into_py(py)));
        }
        if let Some(w) = width {
            kw.push(("width", w.into_py(py)));
        }
        self.call("rectangle", vec![self.p(a.0, a.1), self.p(b.0, b.1)].into_py(py), kw)
    }
    fn ellipse(&self, a: (f64, f64), b: (f64, f64), fill: Option<C4>, outline: Option<C4>, width: Option<i64>) -> R<()> {
        let py = self.d.py();
        let mut kw = vec![];
        if let Some(f) = fill {
            kw.push(("fill", f.into_py(py)));
        }
        if let Some(o) = outline {
            kw.push(("outline", o.into_py(py)));
        }
        if let Some(w) = width {
            kw.push(("width", w.into_py(py)));
        }
        self.call("ellipse", vec![self.p(a.0, a.1), self.p(b.0, b.1)].into_py(py), kw)
    }
    fn line(&self, pts: &[(f64, f64)], fill: C4, width: i64) -> R<()> {
        let py = self.d.py();
        self.call("line", self.ps(pts).into_py(py), vec![("fill", fill.into_py(py)), ("width", width.into_py(py))])
    }
    fn arc(&self, a: (f64, f64), b: (f64, f64), start: f64, end: f64, fill: C4, width: i64) -> R<()> {
        let py = self.d.py();
        self.call("arc", vec![self.p(a.0, a.1), self.p(b.0, b.1)].into_py(py),
            vec![("start", start.into_py(py)), ("end", end.into_py(py)), ("fill", fill.into_py(py)), ("width", width.into_py(py))])
    }
}

/// 陰影付きの球（RGBA の PIL 画像）
#[pyfunction]
#[pyo3(signature = (size, color, tex=None, alpha=1.0, shiny=0.25))]
pub fn icons_sphere<'py>(py: Python<'py>, size: usize, color: &B<'py>, tex: Option<&B<'py>>, alpha: f64, shiny: f64) -> R<B<'py>> {
    let s = size;
    let lt: Vec<f64> = icons(py)?.getattr("LIGHT")?.iter()?.map(|x| f64of(&x?)).collect::<R<_>>()?;
    let col: Vec<f64> = color.iter()?.take(3).map(|x| f64of(&x?)).collect::<R<_>>()?;
    let texd = match tex.filter(|t| !t.is_none()) {
        Some(t) => {
            let np = imp(py, "numpy")?;
            let a = np.call_method1("ascontiguousarray", (t, np.getattr("float64")?))?;
            let shape: Vec<usize> = a.getattr("shape")?.extract()?;
            let r: numpy::PyReadonlyArrayDyn<f64> = a.extract()?;
            Some((r.as_slice()?.to_vec(), shape[0], shape[1], if shape.len() > 2 { shape[2] } else { 1 }))
        }
        None => None,
    };
    let rr = 0.86;
    let sf = s as f64;
    let mut out = vec![0u8; s * s * 4];
    for yi in 0..s {
        for xi in 0..s {
            let (x, y) = (xi as f64, yi as f64);
            let nx = (x + 0.5) / sf * 2.0 - 1.0;
            let ny = 1.0 - (y + 0.5) / sf * 2.0;
            let (nx, ny) = (nx / rr, ny / rr);
            let r2 = nx * nx + ny * ny;
            let inside = r2 <= 1.0;
            let nz = (1.0 - r2).clamp(0.0, 1.0).sqrt();
            let lam = (nx * lt[0] + ny * lt[1] + nz * lt[2]).clamp(0.0, 1.0);
            // 鏡面反射（視線は +z）
            let rz = 2.0 * lam * nz - lt[2];
            let spec = rz.clamp(0.0, 1.0).powf(24.0) * shiny;
            let mut base = [col[0], col[1], col[2]];
            if let Some((td, th, tw, tc)) = &texd {
                let u = ((nx.atan2(nz)) / (2.0 * PI) + 0.5) * 2.0;
                let v = (ny * 0.5 + 0.5) * 1.0;
                let pm = |a: f64| { let m = a % 1.0; if m != 0.0 && m < 0.0 { m + 1.0 } else { m } };
                let ui = (pm(u) * (*tw as f64 - 1.0)) as i64;
                let vi = ((1.0 - pm(v)) * (*th as f64 - 1.0)) as i64;
                let (ui, vi) = (ui.rem_euclid(*tw as i64) as usize, vi.rem_euclid(*th as i64) as usize);
                for k in 0..3 {
                    base[k] *= td[(vi * tw + ui) * tc + k.min(tc - 1)];
                }
            }
            let kk = 0.32 + 0.68 * lam;
            let edge = ((1.0 - r2.sqrt()) * sf * rr * 0.5).clamp(0.0, 1.0);
            let a = if inside { edge * alpha } else { 0.0 };
            let o = &mut out[(yi * s + xi) * 4..(yi * s + xi) * 4 + 4];
            for k in 0..3 {
                o[k] = ((base[k] * kk + spec).clamp(0.0, 1.0) * 255.0) as u8;
            }
            o[3] = (a * 255.0) as u8;
        }
    }
    let bytes = pyo3::types::PyBytes::new_bound(py, &out);
    pil(py)?.call_method1("frombytes", ("RGBA", (s, s), bytes))
}

/// 機器の種類ごとのマーク
#[pyfunction]
#[pyo3(signature = (img, kind, col=(255, 255, 255, 230)))]
pub fn icons_mark<'py>(py: Python<'py>, img: &B<'py>, kind: &B<'py>, col: C4) -> R<B<'py>> {
    let dr = Draw::new(img)?;
    let s = dr.s as i64;
    let w = 2.max(s.div_euclid(16));
    let dark = (25, 25, 30, 220);
    let k: String = if kind.is_none() { String::new() } else { kind.str()?.to_string() };
    let hw = 1.max(w.div_euclid(2));
    match k.as_str() {
        "thruster" => {
            dr.polygon(&[(-0.16, 0.12), (0.16, 0.12), (0.24, -0.1), (-0.24, -0.1)], dark)?;
            dr.polygon(&[(-0.18, -0.12), (0.18, -0.12), (0.0, -0.4)], (255, 150, 40, 240))?;
            dr.polygon(&[(-0.09, -0.12), (0.09, -0.12), (0.0, -0.3)], (255, 235, 120, 250))?;
        }
        "tank" => {
            for yy in [0.16, -0.08] {
                dr.rect((-0.3, yy + 0.04), (0.3, yy - 0.04), Some((60, 60, 70, 170)), None, None)?;
            }
            dr.polygon(&[(0.0, 0.34), (-0.09, 0.2), (0.09, 0.2)], (40, 40, 50, 210))?;
            dr.ellipse((-0.09, 0.26), (0.09, 0.1), Some((40, 40, 50, 210)), None, None)?;
        }
        "pipe" => {
            dr.rect((-0.36, 0.07), (0.36, -0.07), Some(dark), None, None)?;
            dr.rect((-0.07, 0.36), (0.07, -0.36), Some(dark), None, None)?;
        }
        "valve" => {
            dr.polygon(&[(-0.26, 0.2), (0.0, 0.0), (-0.26, -0.2)], col)?;
            dr.polygon(&[(0.26, 0.2), (0.0, 0.0), (0.26, -0.2)], col)?;
            dr.line(&[(0.0, 0.0), (0.0, 0.3)], col, w)?;
        }
        "battery" => {
            dr.rect((-0.24, 0.14), (0.24, -0.14), None, Some(col), Some(w))?;
            dr.rect((0.24, 0.06), (0.3, -0.06), Some(col), None, None)?;
            dr.line(&[(-0.12, 0.0), (0.0, 0.0)], col, w)?;
            dr.line(&[(-0.06, 0.06), (-0.06, -0.06)], col, w)?;
        }
        "generator" => {
            dr.polygon(&[(0.06, 0.34), (-0.18, -0.02), (0.0, -0.02), (-0.08, -0.34), (0.2, 0.06), (0.02, 0.06)], (255, 255, 255, 240))?;
        }
        "solar" => {
            dr.rect((-0.3, 0.22), (0.3, -0.22), Some((30, 50, 120, 235)), Some((210, 220, 240, 240)), Some(hw))?;
            for xx in [-0.1, 0.1] {
                dr.line(&[(xx, 0.22), (xx, -0.22)], (210, 220, 240, 200), hw)?;
            }
            dr.line(&[(-0.3, 0.0), (0.3, 0.0)], (210, 220, 240, 200), hw)?;
            dr.ellipse((0.18, 0.4), (0.36, 0.24), Some((255, 210, 60, 245)), None, None)?;
        }
        "crank" => {
            dr.ellipse((-0.2, 0.2), (0.2, -0.2), None, Some(col), Some(w))?;
            dr.line(&[(0.0, 0.0), (0.28, 0.2)], col, w)?;
            dr.ellipse((0.24, 0.26), (0.34, 0.16), Some(col), None, None)?;
        }
        "computer" => {
            dr.rect((-0.28, 0.2), (0.28, -0.16), Some((15, 25, 20, 235)), Some((200, 220, 230, 240)), None)?;
            for (kk, yy) in [0.1, 0.02, -0.06].iter().enumerate() {
                dr.line(&[(-0.2, *yy), (0.05 + 0.07 * (kk % 2) as f64, *yy)], (90, 255, 140, 240), hw)?;
            }
        }
        "actuator" => {
            dr.arc((-0.26, 0.26), (0.26, -0.26), 200.0, 500.0, col, w)?;
            dr.polygon(&[(0.26, 0.08), (0.36, -0.06), (0.16, -0.06)], col)?;
        }
        "piston" => {
            // ピストン：筒と、押し出す棒・矢印
            dr.rect((-0.32, 0.14), (-0.02, -0.14), None, Some(col), Some(w))?;
            dr.rect((-0.02, 0.05), (0.2, -0.05), Some(col), None, None)?;
            dr.polygon(&[(0.2, 0.16), (0.36, 0.0), (0.2, -0.16)], col)?;
        }
        "hinge" => {
            // ヒンジ：2枚の板と軸
            dr.polygon(&[(-0.3, 0.06), (-0.04, 0.06), (-0.04, -0.26), (-0.3, -0.26)], col)?;
            dr.polygon(&[(0.04, 0.06), (0.12, 0.3), (0.34, 0.22), (0.26, -0.02)], col)?;
            dr.ellipse((-0.08, 0.1), (0.08, -0.06), Some((255, 190, 70, 245)), None, None)?;
        }
        "light" => {
            // ライト：電球と光
            dr.ellipse((-0.16, 0.26), (0.16, -0.06), Some((255, 230, 150, 240)), None, None)?;
            dr.rect((-0.08, -0.06), (0.08, -0.2), Some(col), None, None)?;
            for kk in 0..7 {
                let a = PI * (0.15 + 0.7 * kk as f64 / 6.0);
                dr.line(&[(0.24 * a.cos(), 0.1 + 0.24 * a.sin()), (0.34 * a.cos(), 0.1 + 0.34 * a.sin())], (255, 230, 150, 230), w)?;
            }
        }
        "anchor" => {
            // アンカー：いかり（輪・軸・横棒・下の弧と爪）
            dr.ellipse((-0.07, 0.38), (0.07, 0.24), None, Some(col), Some(w))?;
            dr.line(&[(0.0, 0.24), (0.0, -0.3)], col, w)?;
            dr.line(&[(-0.16, 0.14), (0.16, 0.14)], col, w)?;
            dr.arc((-0.3, 0.0), (0.3, -0.36), 0.0, 180.0, col, w)?;
            dr.polygon(&[(-0.36, -0.1), (-0.24, -0.12), (-0.3, -0.22)], col)?;
            dr.polygon(&[(0.36, -0.1), (0.24, -0.12), (0.3, -0.22)], col)?;
        }
        "bend" => {
            // ヒンジの曲がる側：曲がった矢印
            dr.arc((-0.28, 0.28), (0.28, -0.28), 180.0, 300.0, col, w + 1)?;
            dr.polygon(&[(0.1, -0.34), (0.3, -0.22), (0.08, -0.12)], (255, 190, 70, 245))?;
        }
        "processor" => {
            for kk in 0..8 {
                let a = kk as f64 * PI / 4.0;
                dr.line(&[(0.18 * a.cos(), 0.18 * a.sin()), (0.3 * a.cos(), 0.3 * a.sin())], col, w + 1)?;
            }
            dr.ellipse((-0.2, 0.2), (0.2, -0.2), Some(col), None, None)?;
            dr.ellipse((-0.08, 0.08), (0.08, -0.08), Some(dark), None, None)?;
        }
        "seat" => {
            dr.rect((-0.2, 0.3), (-0.1, -0.12), Some(col), None, None)?;
            dr.rect((-0.2, -0.02), (0.22, -0.12), Some(col), None, None)?;
            dr.rect((0.14, -0.12), (0.22, -0.3), Some(col), None, None)?;
        }
        "air_vent" => {
            for yy in [0.14, 0.0, -0.14] {
                dr.rect((-0.26, yy + 0.035), (0.26, yy - 0.035), Some(dark), None, None)?;
            }
        }
        "antenna" => {
            dr.line(&[(0.0, -0.3), (0.0, 0.16)], col, w)?;
            dr.ellipse((-0.05, 0.21), (0.05, 0.11), Some(col), None, None)?;
            for r in [0.16, 0.27] {
                dr.arc((-r, 0.16 + r), (r, 0.16 - r), 200.0, 340.0, col, 1.max(w.div_euclid(2) + 1))?;
                dr.arc((-r, 0.16 + r), (r, 0.16 - r), 20.0, 160.0, col, 1.max(w.div_euclid(2) + 1))?;
            }
        }
        "turret" => {
            // 砲塔：台と砲身
            dr.ellipse((-0.22, 0.04), (0.22, -0.3), Some(dark), None, None)?;
            dr.rect((-0.05, 0.36), (0.05, -0.1), Some(col), None, None)?;
        }
        "radar" => {
            // レーダー：皿と電波
            dr.call("pieslice", vec![dr.p(-0.3, 0.3), dr.p(0.3, -0.3)].into_py(py), vec![("start", 200.into_py(py)), ("end", 340.into_py(py)), ("fill", col.into_py(py))])?;
            dr.line(&[(0.0, -0.06), (0.0, -0.32)], col, w)?;
            for r in [0.16, 0.28] {
                dr.arc((-r, 0.22 + r), (r, 0.22 - r), 220.0, 320.0, (120, 255, 160, 230), hw)?;
            }
        }
        "launcher" => {
            // ミサイル発射機：ミサイルの形
            dr.polygon(&[(-0.07, -0.26), (0.07, -0.26), (0.07, 0.18), (0.0, 0.34), (-0.07, 0.18)], col)?;
            dr.polygon(&[(-0.07, -0.26), (-0.18, -0.34), (-0.07, -0.12)], (255, 90, 60, 240))?;
            dr.polygon(&[(0.07, -0.26), (0.18, -0.34), (0.07, -0.12)], (255, 90, 60, 240))?;
        }
        "pump" => {
            // ポンプ：羽根車と、流す向きの矢印
            dr.ellipse((-0.22, 0.22), (0.22, -0.22), None, Some(col), Some(w))?;
            for kk in 0..3 {
                let a = kk as f64 * 2.0 * PI / 3.0;
                dr.line(&[(0.0, 0.0), (0.17 * a.cos(), 0.17 * a.sin())], col, w)?;
            }
            dr.polygon(&[(0.26, 0.1), (0.4, 0.0), (0.26, -0.1)], (120, 255, 200, 240))?;
        }
        // ---- 面ごとの印（向きのある機器の、出力の面・反対の面。ブロックの表面の記号だけで使う） ----
        "nozzle" => {
            // 推進機の噴射口（後ろの面）：正面から見たノズル。金属の縁・暗いのど・奥の炎の色
            dr.ellipse((-0.36, 0.36), (0.36, -0.36), Some((170, 175, 185, 245)), Some((230, 232, 238, 250)), Some(hw))?;
            dr.ellipse((-0.28, 0.28), (0.28, -0.28), Some((105, 110, 120, 250)), None, None)?;
            dr.ellipse((-0.2, 0.2), (0.2, -0.2), Some((22, 22, 28, 255)), None, None)?;
            dr.ellipse((-0.11, 0.11), (0.11, -0.11), Some((255, 120, 30, 250)), None, None)?;
            dr.ellipse((-0.05, 0.05), (0.05, -0.05), Some((255, 235, 140, 255)), None, None)?;
            dr.arc((-0.32, 0.32), (0.32, -0.32), 200.0, 250.0, (255, 255, 255, 200), hw)?;
        }
        "thrust_face" => {
            // 推進機の押す面（向きの面）：取り付けの板と、「この向きへ押す」の ⊙（こちらへ向かってくる矢印の先）
            dr.rect((-0.3, 0.3), (0.3, -0.3), None, Some(col), Some(w))?;
            for (x, y) in [(-0.21, 0.21), (0.21, 0.21), (-0.21, -0.21), (0.21, -0.21)] {
                dr.ellipse((x - 0.035, y + 0.035), (x + 0.035, y - 0.035), Some(col), None, None)?;
            }
            dr.ellipse((-0.15, 0.15), (0.15, -0.15), None, Some((255, 170, 60, 250)), Some(w))?;
            dr.ellipse((-0.055, 0.055), (0.055, -0.055), Some((255, 170, 60, 250)), None, None)?;
        }
        "shaft" => {
            // ローターの出力の面：軸（キーの平らな所つき）と、回る向きの矢印
            dr.ellipse((-0.15, 0.15), (0.15, -0.15), Some((215, 220, 230, 250)), Some((60, 65, 75, 250)), Some(hw))?;
            dr.rect((-0.15, 0.15), (0.15, 0.09), Some((40, 44, 52, 250)), None, None)?;     // キーの平らな所
            dr.ellipse((-0.04, 0.04), (0.04, -0.04), Some((40, 44, 52, 250)), None, None)?;
            dr.arc((-0.3, 0.3), (0.3, -0.3), 200.0, 320.0, col, w)?;
            dr.arc((-0.3, 0.3), (0.3, -0.3), 20.0, 140.0, col, w)?;
            dr.polygon(&[(0.15, -0.33), (0.3, -0.24), (0.12, -0.18)], col)?;
            dr.polygon(&[(-0.15, 0.33), (-0.3, 0.24), (-0.12, 0.18)], col)?;
        }
        "rod" => {
            // ピストンの出力の面：押し出す棒の先（ブッシュの輪と、⊙）
            dr.ellipse((-0.3, 0.3), (0.3, -0.3), None, Some(col), Some(w))?;
            dr.ellipse((-0.17, 0.17), (0.17, -0.17), Some((215, 220, 230, 250)), Some((60, 65, 75, 250)), Some(hw))?;
            dr.ellipse((-0.06, 0.06), (0.06, -0.06), Some((40, 44, 52, 250)), None, None)?;
            for a in [45.0f64, 135.0, 225.0, 315.0] {
                let (c_, s_) = (a.to_radians().cos(), a.to_radians().sin());
                dr.line(&[(0.2 * c_, 0.2 * s_), (0.29 * c_, 0.29 * s_)], col, w)?;
            }
        }
        "hinge_face" => {
            // ヒンジの出力の面：板と、曲がる軸のピン
            dr.rect((-0.3, 0.28), (0.3, -0.28), None, Some(col), Some(w))?;
            dr.rect((-0.34, 0.06), (0.34, -0.06), Some((255, 190, 70, 245)), None, None)?;
            for x in [-0.34, 0.34] {
                dr.ellipse((x - 0.06, 0.06), (x + 0.06, -0.06), Some((255, 210, 120, 250)), None, None)?;
            }
        }
        "mount" => {
            // アクチュエーターの反対の面（土台に付ける面）：取り付けの板とボルト
            dr.rect((-0.28, 0.28), (0.28, -0.28), Some((120, 125, 135, 200)), Some(col), Some(hw))?;
            for (x, y) in [(-0.19, 0.19), (0.19, 0.19), (-0.19, -0.19), (0.19, -0.19)] {
                dr.ellipse((x - 0.045, y + 0.045), (x + 0.045, y - 0.045), Some((235, 238, 244, 250)), Some((50, 52, 60, 250)), Some(1))?;
            }
        }
        _ => {
            dr.polygon(&[(0.0, 0.22), (0.22, 0.0), (0.0, -0.22), (-0.22, 0.0)], col)?;
        }
    }
    Ok(img.clone())
}

fn texture_array<'py>(py: Python<'py>, path: &B<'py>) -> R<B<'py>> {
    icons(py)?.call_method1("_texture_array", (path,))
}

#[pyfunction]
pub fn icons_texture_array<'py>(py: Python<'py>, path: &B<'py>) -> R<B<'py>> {
    if !path.is_truthy()? {
        return Ok(py.None().into_bound(py));
    }
    let cache = icons(py)?.getattr("_src_cache")?;
    if cache.contains(path)? {
        return cache.get_item(path);
    }
    let a = match pil(py)?.call_method1("open", (path,)).and_then(|im| im.call_method1("convert", ("RGB",))).and_then(|im| {
        let np = imp(py, "numpy")?;
        let k = PyDict::new_bound(py);
        k.set_item("dtype", np.getattr("float64")?)?;
        np.call_method("asarray", (im,), Some(&k))?.call_method1("__truediv__", (255.0,))
    }) {
        Ok(a) => a,
        Err(e) if is_exc(py, &e) => py.None().into_bound(py),
        Err(e) => return Err(e),
    };
    cache.set_item(path, &a)?;
    Ok(a)
}

#[pyfunction]
#[pyo3(signature = (reg, item, size=96))]
pub fn icons_item_image<'py>(py: Python<'py>, reg: &B<'py>, item: &B<'py>, size: usize) -> R<B<'py>> {
    let key = PyTuple::new_bound(py, [item.clone(), tb(py, size)]);
    let cache = icons(py)?.getattr("_img_cache")?;
    if cache.contains(&key)? {
        return cache.get_item(&key);
    }
    let (kind, id_) = (item.get_item(0)?, item.get_item(1)?);
    let img = if kind.eq("material")? {
        let m = reg.call_method1("material", (&id_,))?;
        let m = if m.is_truthy()? { m } else { PyDict::new_bound(py).into_any() };
        let alpha = if id_.eq("glass")? || id_.eq("water")? { 0.72 } else { 1.0 };
        let shiny = if ["steel", "aluminum", "glass", "titanium"].iter().any(|x| id_.eq(*x).unwrap_or(false)) { 0.5 } else { 0.18 };
        let tex = texture_array(py, &m.call_method1("get", ("texture",))?)?;
        icons_sphere(py, size, &m.call_method1("get", ("color", vec![0.7, 0.7, 0.7]))?, Some(&tex), alpha, shiny)?
    } else {
        let dd = reg.call_method1("device", (&id_,))?;
        let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
        let tex = texture_array(py, &dd.call_method1("get", ("texture",))?)?;
        let img = icons_sphere(py, size, &dd.call_method1("get", ("color", vec![0.6, 0.6, 0.6]))?, Some(&tex), 1.0, 0.3)?;
        let mark = dd.call_method1("get", ("mark",))?;
        let k = if mark.is_truthy()? { mark } else { dd.call_method1("get", ("type",))? };
        icons_mark(py, &img, &k, (255, 255, 255, 230))?
    };
    cache.set_item(&key, &img)?;
    Ok(img)
}

/// PIL 画像 → 画面（Rust）のテクスチャ
#[pyfunction]
#[pyo3(signature = (img, name="tex"))]
pub fn icons_panda_texture<'py>(py: Python<'py>, img: &B<'py>, name: &str) -> R<B<'py>> {
    let im = img.call_method1("convert", ("RGBA",))?;
    imp(py, "main.client.gfx")?.call_method1("tex_rgba", (im.getattr("width")?, im.getattr("height")?, im.call_method0("tobytes")?, name))
}

/// アイテムのアイコンのテクスチャ（キャッシュ）
#[pyfunction]
#[pyo3(signature = (reg, item, size=96))]
pub fn icons_item_texture<'py>(py: Python<'py>, reg: &B<'py>, item: &B<'py>, size: usize) -> R<B<'py>> {
    let key = PyTuple::new_bound(py, [item.clone(), tb(py, size)]);
    let cache = icons(py)?.getattr("_tex_cache")?;
    let t = cache.call_method1("get", (&key,))?;
    if !t.is_none() {
        return Ok(t);
    }
    let t = icons_panda_texture(py, &icons_item_image(py, reg, item, size)?, "icon")?;
    cache.set_item(&key, &t)?;
    Ok(t)
}

#[pyfunction]
pub fn icons_item_name<'py>(py: Python<'py>, reg: &B<'py>, item: &B<'py>) -> R<B<'py>> {
    let (kind, id_) = (item.get_item(0)?, item.get_item(1)?);
    let d = if kind.eq("material")? { reg.call_method1("material", (&id_,))? } else { reg.call_method1("device", (&id_,))? };
    let d = if d.is_truthy()? { d } else { PyDict::new_bound(py).into_any() };
    d.call_method1("get", ("name", id_))
}

fn img_bytes(img: &B<'_>) -> R<(Vec<u8>, usize, usize)> {
    let (w, h): (usize, usize) = img.getattr("size")?.extract()?;
    let b: Vec<u8> = img.call_method0("tobytes")?.extract()?;
    Ok((b, w, h))
}
fn from_bytes<'py>(py: Python<'py>, b: Vec<u8>, w: usize, h: usize) -> R<B<'py>> {
    pil(py)?.call_method1("frombytes", ("RGBA", (w, h), pyo3::types::PyBytes::new_bound(py, &b)))
}

/// 透明な背景に、半透明の丸い台座＋種類のマーク。
/// side：上の縁に向きを示す三角（▲の方向が機器の向き）・front：機器が向いている面（輪で囲む）・back：向きの反対側の面（マークを薄く）
#[pyfunction]
#[pyo3(signature = (kind, variant, s=None))]
pub fn icons_decal_image<'py>(py: Python<'py>, kind: &str, variant: &str, s: Option<usize>) -> R<B<'py>> {
    let s = match s { Some(v) => v, None => icons(py)?.getattr("DECAL_CELL")?.extract()? };
    let sf = s as f64;
    let mut img = pil(py)?.call_method1("new", ("RGBA", (s, s), (0, 0, 0, 0)))?;
    let d = imp(py, "PIL.ImageDraw")?.call_method1("Draw", (&img,))?;
    let c = sf / 2.0;
    let rr = sf * 0.40;
    let k = PyDict::new_bound(py);
    k.set_item("fill", (10, 12, 18, 120))?;
    d.call_method("ellipse", (vec![c - rr, c - rr, c + rr, c + rr],), Some(&k))?;
    let kd: B<'_> = if kind != "other" { tb(py, kind) } else { py.None().into_bound(py) };
    icons_mark(py, &img, &kd, (255, 255, 255, 230))?;
    // 暗い色をわざと使う記号（画面・ノズルののど・軸のキーなど）は明るくしない
    if !["computer", "nozzle", "shaft", "rod", "mount"].contains(&kind) {
        // 暗い色で描いた部分（配管・タンクの帯など）は暗い台座の上で見えないので明るくする
        let (mut b, w, h) = img_bytes(&img)?;
        for px in b.chunks_mut(4) {
            let sum = px[0] as i64 + px[1] as i64 + px[2] as i64;
            if px[3] >= 160 && (sum as f64) / 3.0 < 100.0 {
                px[0] = 228;
                px[1] = 230;
                px[2] = 236;
            }
        }
        img = from_bytes(py, b, w, h)?;
    }
    let d = imp(py, "PIL.ImageDraw")?.call_method1("Draw", (&img,))?;
    if variant == "side" {
        let tri = [(c, sf * 0.015), (c - sf * 0.11, sf * 0.13), (c + sf * 0.11, sf * 0.13)];
        let k = PyDict::new_bound(py);
        k.set_item("fill", (0, 0, 0, 180))?;
        d.call_method("polygon", (tri.iter().map(|&(x, y)| (x, y + 1.0)).collect::<Vec<_>>(),), Some(&k))?;
        let k = PyDict::new_bound(py);
        k.set_item("fill", (255, 225, 90, 245))?;
        d.call_method("polygon", (tri.to_vec(),), Some(&k))?;
    } else if variant == "front" {
        let w = 3.max(s as i64 / 20);
        let wf = w as f64;
        let k = PyDict::new_bound(py);
        k.set_item("outline", (255, 225, 90, 245))?;
        k.set_item("width", w)?;
        d.call_method("ellipse", (vec![c - rr - wf, c - rr - wf, c + rr + wf, c + rr + wf],), Some(&k))?;
    } else if variant == "back" {
        let (mut b, w, h) = img_bytes(&img)?;
        for px in b.chunks_mut(4) {
            px[3] = (px[3] as f64 * 0.45) as u8;
        }
        img = from_bytes(py, b, w, h)?;
    }
    Ok(img)
}

/// (PIL 画像, {(種類, 変化): (u0, v0, u1, v1)})。種類ごと・変化ごとに1マス
#[pyfunction]
pub fn icons_decal_atlas(py: Python<'_>) -> R<B<'_>> {
    let m = icons(py)?;
    let cur = m.getattr("_atlas")?;
    if !cur.is_none() {
        return Ok(cur);
    }
    let types: Vec<String> = m.getattr("DECAL_TYPES")?.extract()?;
    let vars: Vec<String> = m.getattr("DECAL_VARIANTS")?.extract()?;
    let (cols, rows) = (vars.len(), types.len());
    let s: usize = m.getattr("DECAL_CELL")?.extract()?;
    let img = pil(py)?.call_method1("new", ("RGBA", (cols * s, rows * s), (0, 0, 0, 0)))?;
    let uv = PyDict::new_bound(py);
    for (r, kind) in types.iter().enumerate() {
        for (k, var) in vars.iter().enumerate() {
            img.call_method1("paste", (m.call_method1("_decal_image", (kind, var))?, (k * s, r * s)))?;
            let pad = 1.0 / (cols * s) as f64;
            let (u0, u1) = (k as f64 / cols as f64 + pad, (k + 1) as f64 / cols as f64 - pad);
            // 画像の上が v=1（テクスチャ座標は下が 0）
            let v1 = 1.0 - r as f64 / rows as f64 - pad;
            let v0 = 1.0 - (r + 1) as f64 / rows as f64 + pad;
            uv.set_item((kind, var), (u0, v0, u1, v1))?;
        }
    }
    let at = PyTuple::new_bound(py, [img, uv.into_any()]);
    m.setattr("_atlas", &at)?;
    Ok(at.into_any())
}

/// 画面のボタン（タッチ操作）のアイコン：丸い下地に白い絵
#[pyfunction]
#[pyo3(signature = (name, size=128, bg=(20, 26, 36, 170), fg=(240, 244, 250, 255), ring=(255, 255, 255, 90)))]
pub fn icons_ui_image<'py>(py: Python<'py>, name: &str, size: usize, bg: C4, fg: C4, ring: C4) -> R<B<'py>> {
    let s = size * 2; // 大きく描いて縮める（ふちをなめらかに）
    let sf = s as f64;
    let img = pil(py)?.call_method1("new", ("RGBA", (s, s), (0, 0, 0, 0)))?;
    let dr = Draw::new(&img)?;
    let w = 3.max(s as i64 / 22);
    {
        let k = PyDict::new_bound(py);
        k.set_item("fill", bg)?;
        k.set_item("outline", ring)?;
        k.set_item("width", 2.max(s as i64 / 64))?;
        dr.d.call_method("ellipse", (vec![sf * 0.03, sf * 0.03, sf * 0.97, sf * 0.97],), Some(&k))?;
    }
    let line = |pts: &[(f64, f64)], width: i64| -> R<()> {
        let k = PyDict::new_bound(py);
        k.set_item("fill", fg)?;
        k.set_item("width", width)?;
        k.set_item("joint", "curve")?;
        dr.d.call_method("line", (dr.ps(pts),), Some(&k))?;
        Ok(())
    };
    let poly = |pts: &[(f64, f64)], fill: C4| dr.polygon(pts, fill);
    match name {
        "menu" => {
            for yy in [0.13, 0.0, -0.13] {
                line(&[(-0.2, yy), (0.2, yy)], w)?;
            }
        }
        "inventory" => {
            for i in 0..3 {
                for j in 0..3 {
                    let (x0, y0) = (-0.2 + i as f64 * 0.14, 0.2 - j as f64 * 0.14);
                    dr.rect((x0, y0), (x0 + 0.1, y0 - 0.1), Some(fg), None, None)?;
                }
            }
        }
        "map" => {
            poly(&[(-0.26, 0.18), (-0.09, 0.24), (0.09, 0.18), (0.26, 0.24), (0.26, -0.18), (0.09, -0.24), (-0.09, -0.18), (-0.26, -0.24)], fg)?;
            dr.line(&[(-0.09, 0.24), (-0.09, -0.18)], bg, w.div_euclid(2) + 1)?;
            dr.line(&[(0.09, 0.18), (0.09, -0.24)], bg, w.div_euclid(2) + 1)?;
        }
        "marker" => {
            poly(&[(0.0, 0.26), (0.17, 0.0), (0.0, -0.26), (-0.17, 0.0)], fg)?;
            dr.ellipse((-0.06, 0.06), (0.06, -0.06), Some(bg), None, None)?;
        }
        "view" => {
            dr.ellipse((-0.27, 0.15), (0.27, -0.15), None, Some(fg), Some(w))?;
            dr.ellipse((-0.09, 0.09), (0.09, -0.09), Some(fg), None, None)?;
        }
        "jump" => {
            line(&[(-0.2, -0.04), (0.0, 0.16), (0.2, -0.04)], w + 2)?;
            line(&[(-0.2, -0.2), (0.0, 0.0), (0.2, -0.2)], w + 2)?;
        }
        "down" => {
            line(&[(-0.2, 0.04), (0.0, -0.16), (0.2, 0.04)], w + 2)?;
            line(&[(-0.2, 0.2), (0.0, 0.0), (0.2, 0.2)], w + 2)?;
        }
        "jetpack" => {
            poly(&[(0.0, 0.3), (0.1, 0.12), (0.1, -0.1), (-0.1, -0.1), (-0.1, 0.12)], fg)?;
            poly(&[(-0.1, -0.02), (-0.2, -0.14), (-0.1, -0.1)], fg)?;
            poly(&[(0.1, -0.02), (0.2, -0.14), (0.1, -0.1)], fg)?;
            poly(&[(-0.07, -0.13), (0.07, -0.13), (0.0, -0.3)], (255, 170, 60, 255))?;
        }
        "place" => {
            dr.rect((-0.2, 0.2), (0.2, -0.2), None, Some(fg), Some(w))?;
            line(&[(0.0, 0.11), (0.0, -0.11)], w)?;
            line(&[(-0.11, 0.0), (0.11, 0.0)], w)?;
        }
        "remove" => {
            dr.arc((-0.26, 0.3), (0.26, -0.06), 200.0, 340.0, fg, w + 2)?;
            line(&[(0.0, 0.12), (-0.04, -0.28)], w + 1)?;
        }
        "rotate" => {
            dr.arc((-0.2, 0.2), (0.2, -0.2), 40.0, 330.0, fg, w + 1)?;
            poly(&[(0.2, 0.0), (0.27, 0.12), (0.1, 0.1)], fg)?;
        }
        "new" => {
            dr.rect((-0.14, 0.14), (0.14, -0.14), None, Some(fg), Some(w))?;
            for (a, b) in [((0.0, 0.3), (0.0, 0.2)), ((0.0, -0.2), (0.0, -0.3)), ((-0.3, 0.0), (-0.2, 0.0)), ((0.2, 0.0), (0.3, 0.0))] {
                line(&[a, b], w)?;
            }
        }
        "interact" => {
            dr.ellipse((-0.2, 0.2), (0.2, -0.2), None, Some(fg), Some(w))?;
            dr.ellipse((-0.07, 0.07), (0.07, -0.07), Some(fg), None, None)?;
        }
        "settings" => {
            for k in 0..8 {
                let a = k as f64 * PI / 4.0;
                line(&[(0.14 * a.cos(), 0.14 * a.sin()), (0.26 * a.cos(), 0.26 * a.sin())], w + 3)?;
            }
            dr.ellipse((-0.17, 0.17), (0.17, -0.17), Some(fg), None, None)?;
            dr.ellipse((-0.07, 0.07), (0.07, -0.07), Some(bg), None, None)?;
        }
        "import" | "export" => {
            dr.line(&[(-0.24, -0.06), (-0.24, -0.24), (0.24, -0.24), (0.24, -0.06)], fg, w)?;
            if name == "import" {
                line(&[(0.0, 0.26), (0.0, -0.06)], w)?;
                poly(&[(-0.11, 0.02), (0.11, 0.02), (0.0, -0.14)], fg)?;
            } else {
                line(&[(0.0, -0.14), (0.0, 0.16)], w)?;
                poly(&[(-0.11, 0.1), (0.11, 0.1), (0.0, 0.27)], fg)?;
            }
        }
        "save" => {
            dr.rect((-0.22, 0.22), (0.22, -0.22), Some(fg), None, None)?;
            dr.rect((-0.12, 0.22), (0.1, 0.06), Some(bg), None, None)?;
            dr.rect((-0.14, -0.04), (0.14, -0.18), Some(bg), None, None)?;
        }
        "exit" => {
            dr.line(&[(0.02, 0.24), (-0.2, 0.24), (-0.2, -0.24), (0.02, -0.24)], fg, w)?;
            line(&[(-0.06, 0.0), (0.24, 0.0)], w)?;
            poly(&[(0.14, 0.1), (0.27, 0.0), (0.14, -0.1)], fg)?;
        }
        "camera" => {
            let k = PyDict::new_bound(py);
            k.set_item("radius", sf * 0.04)?;
            k.set_item("fill", fg)?;
            dr.d.call_method("rounded_rectangle", (vec![dr.p(-0.26, 0.14), dr.p(0.26, -0.2)],), Some(&k))?;
            dr.rect((-0.1, 0.2), (0.08, 0.13), Some(fg), None, None)?;
            dr.ellipse((-0.12, 0.09), (0.12, -0.15), Some(bg), None, None)?;
            dr.ellipse((-0.07, 0.04), (0.07, -0.1), Some(fg), None, None)?;
        }
        n if n == "key" || n.starts_with("key:") => {
            let k = PyDict::new_bound(py);
            k.set_item("radius", sf * 0.05)?;
            k.set_item("outline", fg)?;
            k.set_item("width", w)?;
            dr.d.call_method("rounded_rectangle", (vec![dr.p(-0.24, 0.18), dr.p(0.24, -0.18)],), Some(&k))?;
            let label = n.strip_prefix("key:").unwrap_or("");
            if !label.is_empty() {
                // キーの文字（X など）を真ん中に
                let fsize = (sf * if label.chars().count() <= 2 { 0.22 } else { 0.12 }) as i64;
                let font = match imp(py, "PIL.ImageFont").and_then(|f| {
                    let k = PyDict::new_bound(py);
                    k.set_item("size", fsize)?;
                    f.call_method("load_default", (), Some(&k))
                }) {
                    Ok(f) => f,
                    Err(e) if is_exc(py, &e) => py.None().into_bound(py),
                    Err(e) => return Err(e),
                };
                let k = PyDict::new_bound(py);
                k.set_item("fill", fg)?;
                k.set_item("font", font)?;
                k.set_item("anchor", "mm")?;
                let c = sf / 2.0;
                dr.d.call_method("text", ((c, c), label), Some(&k))?;
            }
        }
        _ => {}
    }
    img.call_method1("resize", ((size, size), pil(py)?.getattr("LANCZOS")?))
}

#[pyfunction]
#[pyo3(signature = (name, size=128))]
pub fn icons_ui_texture<'py>(py: Python<'py>, name: &str, size: usize) -> R<B<'py>> {
    let cache = icons(py)?.getattr("_ui_cache")?;
    let key = (name, size);
    let t = cache.call_method1("get", (key,))?;
    if !t.is_none() {
        return Ok(t);
    }
    let img = icons(py)?.call_method1("ui_image", (name, size))?;
    let t = icons_panda_texture(py, &img, "icon")?;
    cache.set_item(key, &t)?;
    let _ = PyList::empty_bound(py);
    Ok(t)
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    for f in [wrap_pyfunction_bound!(icons_sphere, m)?, wrap_pyfunction_bound!(icons_mark, m)?, wrap_pyfunction_bound!(icons_texture_array, m)?,
              wrap_pyfunction_bound!(icons_item_image, m)?, wrap_pyfunction_bound!(icons_panda_texture, m)?, wrap_pyfunction_bound!(icons_item_texture, m)?,
              wrap_pyfunction_bound!(icons_item_name, m)?, wrap_pyfunction_bound!(icons_decal_image, m)?, wrap_pyfunction_bound!(icons_decal_atlas, m)?,
              wrap_pyfunction_bound!(icons_ui_image, m)?, wrap_pyfunction_bound!(icons_ui_texture, m)?] {
        m.add_function(f)?;
    }
    Ok(())
}
