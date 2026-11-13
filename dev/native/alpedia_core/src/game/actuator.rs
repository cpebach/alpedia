//! アクチュエーター：回転・押し出し・引き込み・分離・結合。main/actuator.py の中身。
//!
//! アクチュエーターは向き（出力面）を持つブロック。
//! - 回転   : 出力面の先につながったブロック（とその先の部分）を、向きを軸に回す
//!            ヒンジ（別の機器）は、ヒンジのブロックの中心を軸に -90〜+90° 振る
//! - 押し出し: 自分が伸びて、出力面の先を押し出す（最大でブロック1つ分）
//! - 引き込み: 伸びた分を引き戻す
//! - 分離   : 出力面の結合を切り離す（先の部分は別の構築物になる）
//! - 結合   : 出力面の先にあるブロック（別の構築物でも可）とつなぐ

use numpy::{PyArray1, PyArray2, PyArrayMethods, PyReadonlyArray2};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

type R<T> = PyResult<T>;

#[allow(unexpected_cfgs)]
mod err {
    pyo3::create_exception!(alpedia_core, MotionError, pyo3::exceptions::PyValueError);
}
pub use err::MotionError;

pub const DIR_TUPLES: [[i64; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
pub const DIR_NAMES: [&str; 6] = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"];
pub const HINGE_MAX: f64 = 90.0;

/// 向き d の面の4隅（構築物の FACE_QUADS の並び）
pub fn face_quad(d: usize) -> [usize; 4] {
    let axis = d / 2;
    let positive = d % 2 == 0;
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut quad = [0usize; 4];
    for (k, (bu, bv)) in [(0, 0), (1, 0), (1, 1), (0, 1)].into_iter().enumerate() {
        let mut bits = [0usize; 3];
        bits[axis] = if positive { 1 } else { 0 };
        bits[u] = bu;
        bits[v] = bv;
        quad[k] = bits[0] + 2 * bits[1] + 4 * bits[2];
    }
    if !positive {
        quad.reverse();
    }
    quad
}

/// リンク a→b（b = a + DIRS[d]）で結合する頂点ペア（construct.JOINT_PAIRS）
pub fn joint_pairs(d: usize) -> [[usize; 2]; 4] {
    let ax = d / 2;
    let q = face_quad(d);
    [[q[0], q[0] ^ (1 << ax)], [q[1], q[1] ^ (1 << ax)], [q[2], q[2] ^ (1 << ax)], [q[3], q[3] ^ (1 << ax)]]
}

fn corner_bit(c: usize, ax: usize) -> usize {
    (c >> ax) & 1
}

fn main_mod<'py>(py: Python<'py>, name: &str) -> R<Bound<'py, PyModule>> {
    py.import_bound(name)
}

fn link_key<'py>(py: Python<'py>, a: &Bound<'py, PyAny>, b: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    main_mod(py, "main.construct")?.getattr("link_key")?.call1((a, b))
}

fn dev_of<'py>(c: &Bound<'py, PyAny>, i: usize) -> R<Bound<'py, PyAny>> {
    c.getattr("devs")?.get_item(i)
}

fn dev_def<'py>(c: &Bound<'py, PyAny>, d: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    c.getattr("reg")?.call_method1("device", (d.get_item("def")?,))
}

fn int_of(v: &Bound<'_, PyAny>) -> R<i64> {
    v.py().import_bound("builtins")?.getattr("int")?.call1((v,))?.extract()
}

fn float_of(v: &Bound<'_, PyAny>) -> R<f64> {
    v.py().import_bound("builtins")?.getattr("float")?.call1((v,))?.extract()
}

fn sget<'py>(st: &Bound<'py, PyAny>, k: &str, default: impl IntoPy<PyObject>) -> R<Bound<'py, PyAny>> {
    st.call_method1("get", (k, default.into_py(st.py())))
}

/// アクチュエーターの種類："rotor" / "piston" / "hinge" / "both"
pub fn motion(dev: &Bound<'_, PyAny>, reg: Option<&Bound<'_, PyAny>>) -> R<&'static str> {
    let py = dev.py();
    if !dev.is_truthy()? {
        return Ok("both");
    }
    let dd = match reg {
        Some(r) if !r.is_none() => r.call_method1("device", (dev.get_item("def")?,))?,
        _ => py.None().into_bound(py),
    };
    let ddd = if dd.is_truthy()? { dd.clone() } else { PyDict::new_bound(py).into_any() };
    let p = ddd.call_method1("get", ("params",))?;
    let p = if p.is_truthy()? { p } else { PyDict::new_bound(py).into_any() };
    let m = p.call_method1("get", ("motion",))?;
    for k in ["rotor", "piston", "hinge", "both"] {
        if m.eq(k)? {
            return Ok(k);
        }
    }
    let def = dev.call_method1("get", ("def",))?;
    if p.call_method1("get", ("hinge",))?.is_truthy()? || def.eq("hinge")? {
        return Ok("hinge");
    }
    if def.eq("piston")? {
        return Ok("piston");
    }
    if def.eq("actuator")? && dd.is_none() {
        return Ok("rotor");
    }
    Ok("both")
}

pub fn actions_for(m: &str) -> &'static [&'static str] {
    match m {
        "rotor" => &["rotate", "detach", "attach", "none"],
        "piston" => &["extend", "retract", "detach", "attach", "none"],
        "hinge" => &["rotate", "detach", "attach", "none"],
        _ => &["rotate", "extend", "retract", "detach", "attach", "none"],
    }
}

fn motion_name(m: &str) -> &'static str {
    match m {
        "rotor" => "ローター",
        "piston" => "ピストン",
        "hinge" => "ヒンジ",
        _ => "アクチュエーター",
    }
}

fn default_preset<'py>(py: Python<'py>, dev: &Bound<'py, PyAny>, reg: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    if motion(dev, reg)? == "piston" {
        d.set_item("action", "extend")?;
        d.set_item("amount", 1.0)?;
        d.set_item("speed", 0.5)?;
    } else {
        d.set_item("action", "rotate")?;
        d.set_item("amount", 90.0)?;
        d.set_item("speed", 90.0)?;
    }
    Ok(d)
}

#[pyfunction]
#[pyo3(signature = (dev, reg=None))]
pub fn act_motion(dev: &Bound<'_, PyAny>, reg: Option<&Bound<'_, PyAny>>) -> R<&'static str> {
    motion(dev, reg)
}

#[pyfunction]
#[pyo3(signature = (dev, reg=None))]
pub fn act_actions_for(dev: &Bound<'_, PyAny>, reg: Option<&Bound<'_, PyAny>>) -> R<Vec<&'static str>> {
    Ok(actions_for(motion(dev, reg)?).to_vec())
}

#[pyfunction]
#[pyo3(signature = (dev, reg=None))]
pub fn act_default_preset<'py>(py: Python<'py>, dev: &Bound<'py, PyAny>, reg: Option<&Bound<'py, PyAny>>) -> R<Bound<'py, PyDict>> {
    default_preset(py, dev, reg)
}

/// 古いセーブの「アクチュエーター」：押し出しを使っていたものはピストンにする
#[pyfunction]
pub fn act_migrate_device<'py>(py: Python<'py>, dev: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    if !dev.is_truthy()? || !dev.call_method1("get", ("def",))?.eq("actuator")? {
        return Ok(dev.clone());
    }
    let st = dev.call_method1("get", ("state",))?;
    let st = if st.is_truthy()? { st } else { PyDict::new_bound(py).into_any() };
    let pre = st.call_method1("get", ("preset",))?;
    let pre = if pre.is_truthy()? { pre } else { PyDict::new_bound(py).into_any() };
    let num = |k: &str| -> R<f64> {
        let v = sget(&st, k, 0)?;
        let v = if v.is_truthy()? { v } else { 0i64.into_py(py).into_bound(py) };
        float_of(&v)
    };
    let act = pre.call_method1("get", ("action",))?;
    if num("ext")?.abs() > 1e-6 || num("ext_target")?.abs() > 1e-6 || act.eq("extend")? || act.eq("retract")? {
        let d = PyDict::new_bound(py);
        d.update(dev.downcast::<pyo3::types::PyMapping>()?)?;
        d.set_item("def", "piston")?;
        return Ok(d.into_any());
    }
    Ok(dev.clone())
}

/// その動作ができなければ MotionError（メッセージ付き）
#[pyfunction]
pub fn act_check_action(c: &Bound<'_, PyAny>, i: usize, action: &str) -> R<()> {
    let d = dev_of(c, i)?;
    let reg = c.getattr("reg")?;
    let m = motion(&d, Some(&reg))?;
    if actions_for(m).contains(&action) {
        return Ok(());
    }
    let what = match action {
        "rotate" => "回せません",
        "extend" => "押し出せません",
        "retract" => "引き込めません",
        _ => "できません",
    };
    let hint = match m {
        "rotor" => "回転・分離・結合だけ。押し出すならピストンを使ってください",
        "piston" => "押し出し・引き込み・分離・結合だけ。回すならローターを使ってください",
        "hinge" => "曲げる・分離・結合だけ",
        _ => "",
    };
    Err(MotionError::new_err(format!("{}（{}）は{what}（{hint}）", d.get_item("name")?.str()?, motion_name(m))))
}

// ---------------------------------------------------------------------------
// ヒンジ
// ---------------------------------------------------------------------------
pub fn hinge_sides(d: usize) -> Vec<usize> {
    (0..6).filter(|k| k / 2 != d / 2).collect()
}

pub fn default_hinge_side(d: usize) -> usize {
    if d / 2 == 1 { 5 } else { 3 }
}

/// 出力面の4隅のうち、辺 side の側にある2つの番号
pub fn hinge_pair(d: usize, side: usize) -> Option<(usize, usize)> {
    let ax = side / 2;
    let want = if side % 2 == 0 { 1 } else { 0 };
    let jp = joint_pairs(d);
    let ks: Vec<usize> = (0..4).filter(|&k| corner_bit(jp[k][0], ax) == want).collect();
    if ks.len() == 2 { Some((ks[0], ks[1])) } else { None }
}

pub fn is_hinge_dev(dev: &Bound<'_, PyAny>, reg: Option<&Bound<'_, PyAny>>) -> R<bool> {
    if !dev.is_truthy()? {
        return Ok(false);
    }
    if dev.call_method1("get", ("def",))?.eq("hinge")? {
        return Ok(true);
    }
    let Some(reg) = reg.filter(|r| !r.is_none()) else { return Ok(false) };
    let dd = reg.call_method1("device", (dev.get_item("def")?,))?;
    if !dd.is_truthy()? {
        return Ok(false);
    }
    let p = dd.call_method1("get", ("params",))?;
    if !p.is_truthy()? {
        return Ok(false);
    }
    p.call_method1("get", ("hinge",))?.is_truthy()
}

fn dev_dir(dev: &Bound<'_, PyAny>) -> R<i64> {
    int_of(&sget(dev, "dir", 2)?)
}

/// ヒンジの曲がる側（向きの番号）。置いたときに決めた側、無ければ既定
pub fn hinge_side(dev: &Bound<'_, PyAny>) -> R<usize> {
    let py = dev.py();
    let d = dev_dir(dev)?;
    let st = sget(dev, "state", PyDict::new_bound(py))?;
    let h = int_of(&sget(&st, "hinge", -1)?)?;
    let du = d.rem_euclid(6) as usize;
    if h >= 0 && hinge_sides(du).contains(&(h as usize)) && (0..6).contains(&d) {
        return Ok(h as usize);
    }
    Ok(default_hinge_side(du))
}

#[pyfunction]
pub fn act_hinge_sides(d: i64) -> Vec<usize> {
    hinge_sides(d.rem_euclid(6) as usize)
}

#[pyfunction]
pub fn act_default_hinge_side(d: i64) -> usize {
    default_hinge_side(d.rem_euclid(6) as usize)
}

#[pyfunction]
pub fn act_hinge_pair(d: i64, side: i64) -> Option<(usize, usize)> {
    hinge_pair(d.rem_euclid(6) as usize, side.rem_euclid(6) as usize)
}

#[pyfunction]
#[pyo3(signature = (dev, reg=None))]
pub fn act_is_hinge_dev(dev: &Bound<'_, PyAny>, reg: Option<&Bound<'_, PyAny>>) -> R<bool> {
    is_hinge_dev(dev, reg)
}

#[pyfunction]
pub fn act_hinge_side(dev: &Bound<'_, PyAny>) -> R<usize> {
    hinge_side(dev)
}

/// ヒンジなら、軸になる辺（出力面の4隅のうち2つの番号）。回転のアクチュエーターなら None
#[pyfunction]
#[pyo3(signature = (dev, reg=None))]
pub fn act_hinge_of(dev: &Bound<'_, PyAny>, reg: Option<&Bound<'_, PyAny>>) -> R<Option<(usize, usize)>> {
    if !is_hinge_dev(dev, reg)? {
        return Ok(None);
    }
    Ok(hinge_pair(dev_dir(dev)?.rem_euclid(6) as usize, hinge_side(dev)?))
}

/// 置くとき：出力の軸に垂直な4つの向きのうち、見ている向き look にいちばん近いもの
#[pyfunction]
pub fn act_side_toward(frame_rows: PyReadonlyArray2<'_, f64>, d: i64, look: [f64; 3]) -> R<usize> {
    let f = frame_rows.as_array();
    let d = d.rem_euclid(6) as usize;
    let (mut best, mut bk) = (-2.0, default_hinge_side(d));
    for k in hinge_sides(d) {
        let s = if k % 2 == 0 { 1.0 } else { -1.0 };
        let dt = (0..3).map(|a| f[[k / 2, a]] * s * look[a]).sum::<f64>();
        if dt > best {
            best = dt;
            bk = k;
        }
    }
    Ok(bk)
}

#[pyfunction]
#[pyo3(signature = (st, dev=None))]
pub fn act_mode_name(st: &Bound<'_, PyAny>, dev: Option<&Bound<'_, PyAny>>) -> R<String> {
    let _ = st;
    if let Some(d) = dev.filter(|d| !d.is_none()) {
        if is_hinge_dev(d, None)? {
            return Ok(format!("ヒンジ（+ で {} 側へ倒れる）", DIR_NAMES[hinge_side(d)?]));
        }
    }
    Ok("回転".into())
}

#[pyfunction]
pub fn act_is_actuator(c: &Bound<'_, PyAny>, i: usize) -> R<bool> {
    let d = dev_of(c, i)?;
    if !d.is_truthy()? {
        return Ok(false);
    }
    let dd = dev_def(c, &d)?;
    if !dd.is_truthy()? {
        return Ok(false);
    }
    dd.call_method1("get", ("type",))?.eq("actuator")
}

pub fn stroke(c: &Bound<'_, PyAny>, i: usize) -> R<f64> {
    let py = c.py();
    let dd = dev_def(c, &dev_of(c, i)?)?;
    let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
    float_of(&sget(&sget(&dd, "params", PyDict::new_bound(py))?, "stroke", 1.0)?)
}

#[pyfunction]
pub fn act_stroke(c: &Bound<'_, PyAny>, i: usize) -> R<f64> {
    stroke(c, i)
}

pub fn output_cell<'py>(c: &Bound<'py, PyAny>, i: usize) -> R<Bound<'py, PyTuple>> {
    let py = c.py();
    let d = dev_of(c, i)?.get_item("dir")?;
    let dt = DIR_TUPLES[d.extract::<i64>()?.rem_euclid(6) as usize];
    let dt = if d.extract::<i64>()? < 0 || d.extract::<i64>()? > 5 {
        let t = main_mod(py, "main.construct")?.getattr("DIR_TUPLES")?.get_item(&d)?;
        [t.get_item(0)?.extract()?, t.get_item(1)?.extract()?, t.get_item(2)?.extract()?]
    } else {
        dt
    };
    let cell = c.getattr("cells")?.get_item(i)?;
    let x: i64 = cell.get_item(0)?.extract()?;
    let y: i64 = cell.get_item(1)?.extract()?;
    let z: i64 = cell.get_item(2)?.extract()?;
    Ok(PyTuple::new_bound(py, [x + dt[0], y + dt[1], z + dt[2]]))
}

#[pyfunction]
pub fn act_output_cell<'py>(c: &Bound<'py, PyAny>, i: usize) -> R<Bound<'py, PyTuple>> {
    output_cell(c, i)
}

pub fn is_attached(c: &Bound<'_, PyAny>, i: usize) -> R<bool> {
    let py = c.py();
    let k = link_key(py, &c.getattr("cells")?.get_item(i)?, output_cell(c, i)?.as_any())?;
    c.getattr("links")?.contains(k)
}

#[pyfunction]
pub fn act_is_attached(c: &Bound<'_, PyAny>, i: usize) -> R<bool> {
    is_attached(c, i)
}

#[pyfunction]
pub fn act_busy(st: &Bound<'_, PyAny>) -> R<bool> {
    let f = |k: &str| -> R<f64> { float_of(&sget(st, k, 0)?) };
    Ok((f("ext_target")? - f("ext")?).abs() > 1e-4 || (f("angle_target")? - f("angle")?).abs() > 1e-3)
}

// ---------------------------------------------------------------------------
// 命令
// ---------------------------------------------------------------------------
fn wake(c: &Bound<'_, PyAny>) -> R<()> {
    let kw = PyDict::new_bound(c.py());
    kw.set_item("soft", true)?;
    c.call_method("wake", (), Some(&kw))?;
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (c, i, angle, speed=None, absolute=false))]
pub fn act_cmd_rotate(c: &Bound<'_, PyAny>, i: usize, angle: &Bound<'_, PyAny>, speed: Option<&Bound<'_, PyAny>>, absolute: bool) -> R<()> {
    let d = dev_of(c, i)?;
    let st = d.get_item("state")?;
    let mut t = if absolute { float_of(angle)? } else { float_of(&sget(&st, "angle_target", 0.0)?)? + float_of(angle)? };
    if is_hinge_dev(&d, Some(&c.getattr("reg")?))? {
        t = t.max(-HINGE_MAX).min(HINGE_MAX);
    }
    st.set_item("angle_target", t)?;
    if let Some(s) = speed.filter(|s| !s.is_none()) {
        st.set_item("rot_speed", float_of(s)?.min(3600.0).max(1.0))?;
    }
    wake(c)
}

#[pyfunction]
#[pyo3(signature = (c, i, amount=None, speed=None, absolute=false))]
pub fn act_cmd_extend(c: &Bound<'_, PyAny>, i: usize, amount: Option<&Bound<'_, PyAny>>, speed: Option<&Bound<'_, PyAny>>, absolute: bool) -> R<()> {
    let st = dev_of(c, i)?.get_item("state")?;
    let mx = stroke(c, i)?;
    let a = match amount {
        Some(a) => float_of(a)?,
        None => 1.0,
    };
    let t = if absolute { a } else { float_of(&sget(&st, "ext_target", 0.0)?)? + a };
    st.set_item("ext_target", t.max(0.0).min(mx))?;
    if let Some(s) = speed.filter(|s| !s.is_none()) {
        st.set_item("ext_speed", float_of(s)?.min(10.0).max(0.01))?;
    }
    wake(c)
}

/// 出力面の結合を外す。外れたら True（分かれた部分の処理は world 側で行う）
#[pyfunction]
pub fn act_cmd_detach(c: &Bound<'_, PyAny>, i: usize) -> R<bool> {
    let py = c.py();
    let cell = c.getattr("cells")?.get_item(i)?;
    let key = link_key(py, &cell, output_cell(c, i)?.as_any())?;
    let links = c.getattr("links")?;
    let extras = c.getattr("extras")?;
    let mut found = links.contains(&key)? || extras.contains(&key)?;
    links.call_method1("discard", (&key,))?;
    extras.call_method1("pop", (&key, py.None()))?;
    // 出力面の頂点につながる追加結合も外す
    let d = dev_of(c, i)?.get_item("dir")?.extract::<i64>()?.rem_euclid(6) as usize;
    let face: Vec<i64> = joint_pairs(d).iter().map(|p| p[0] as i64).collect();
    let keys: Vec<Bound<'_, PyAny>> = PyList::new_bound(py, extras.iter()?.collect::<R<Vec<_>>>()?).iter().collect();
    for k in keys {
        if !k.contains(&cell)? {
            continue;
        }
        let lst = extras.get_item(&k)?;
        let k0 = k.get_item(0)?.eq(&cell)?;
        let k1 = k.get_item(1)?.eq(&cell)?;
        let keep = PyList::empty_bound(py);
        let mut n = 0;
        for e in lst.iter()? {
            let e = e?;
            n += 1;
            let e0: i64 = e.get_item(0)?.extract().unwrap_or(-1);
            let e1: i64 = e.get_item(1)?.extract().unwrap_or(-1);
            if !((k0 && face.contains(&e0)) || (k1 && face.contains(&e1))) {
                keep.append(e)?;
            }
        }
        if keep.len() != n {
            found = true;
            if keep.len() > 0 {
                extras.set_item(&k, keep)?;
            } else {
                extras.del_item(&k)?;
            }
        }
    }
    if found {
        c.call_method0("rebuild")?;
    }
    Ok(found)
}

/// 頂点 idx の位置（構築物の pos から、その行だけ読む）
fn pos_at(c: &Bound<'_, PyAny>, idx: &[usize]) -> R<Vec<[f64; 3]>> {
    let p: PyReadonlyArray2<f64> = c.getattr("pos")?.extract()?;
    let a = p.as_array();
    let n = a.nrows();
    idx.iter()
        .map(|&k| if k < n { Ok([a[[k, 0]], a[[k, 1]], a[[k, 2]]]) } else { Err(pyo3::exceptions::PyIndexError::new_err("頂点の番号が範囲外です")) })
        .collect()
}

/// 出力面の中心・法線・4頂点の位置
pub fn face_info(c: &Bound<'_, PyAny>, i: usize) -> R<([f64; 3], [f64; 3], [[f64; 3]; 4])> {
    let d = dev_of(c, i)?.get_item("dir")?.extract::<i64>()?.rem_euclid(6) as usize;
    let jp = joint_pairs(d);
    let pos = pos_at(c, &(0..8).map(|k| 8 * i + k).collect::<Vec<_>>())?;
    let mut pa = [[0.0; 3]; 4];
    let mut ctr = [0.0; 3];
    for k in 0..4 {
        pa[k] = pos[jp[k][0]];
        for a in 0..3 {
            ctr[a] += pa[k][a];
        }
    }
    let mut cen = [0.0; 3];
    for k in 0..8 {
        for a in 0..3 {
            cen[a] += pos[k][a];
        }
    }
    for a in 0..3 {
        ctr[a] /= 4.0;
        cen[a] /= 8.0;
    }
    let mut n = [ctr[0] - cen[0], ctr[1] - cen[1], ctr[2] - cen[2]];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
    for a in 0..3 {
        n[a] /= l;
    }
    Ok((ctr, n, pa))
}

#[pyfunction]
pub fn act_face_info<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, i: usize)
    -> R<(Bound<'py, PyArray1<f64>>, Bound<'py, PyArray1<f64>>, Bound<'py, PyArray2<f64>>)> {
    let (ctr, n, pa) = face_info(c, i)?;
    Ok((PyArray1::from_vec_bound(py, ctr.to_vec()), PyArray1::from_vec_bound(py, n.to_vec()),
        PyArray1::from_vec_bound(py, pa.iter().flatten().copied().collect()).reshape([4, 3])?))
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// 出力面とその先のブロックの、軸まわりのねじれ角（度）
pub fn measure_twist(c: &Bound<'_, PyAny>, i: usize) -> R<Option<f64>> {
    let oc = output_cell(c, i)?;
    let j = c.getattr("index")?.call_method1("get", (oc,))?;
    if j.is_none() {
        return Ok(None);
    }
    let j: usize = j.extract()?;
    let d = dev_of(c, i)?.get_item("dir")?.extract::<i64>()?.rem_euclid(6) as usize;
    let jp = joint_pairs(d);
    let ia: Vec<usize> = (0..4).map(|k| 8 * i + jp[k][0]).collect();
    let ib: Vec<usize> = (0..4).map(|k| 8 * j + jp[k][1]).collect();
    let (pa_, pb_) = (pos_at(c, &ia)?, pos_at(c, &ib)?);
    let (_, n, _) = face_info(c, i)?;
    let mut ma = [0.0; 3];
    let mut mb = [0.0; 3];
    for k in 0..4 {
        for a in 0..3 {
            ma[a] += pa_[k][a] / 4.0;
            mb[a] += pb_[k][a] / 4.0;
        }
    }
    let (mut s, mut co) = (0.0, 0.0);
    for k in 0..4 {
        let pa = pa_[k];
        let pb = pb_[k];
        let va = [pa[0] - ma[0], pa[1] - ma[1], pa[2] - ma[2]];
        let vb = [pb[0] - mb[0], pb[1] - mb[1], pb[2] - mb[2]];
        s += dot(cross(va, vb), n);
        co += dot(va, vb);
    }
    Ok(Some(s.atan2(co).to_degrees()))
}

#[pyfunction]
pub fn act_measure_twist(c: &Bound<'_, PyAny>, i: usize) -> R<Option<f64>> {
    measure_twist(c, i)
}

/// 結合した直後に、今のねじれ角を回転角として取り込む（急に回らないように）
#[pyfunction]
pub fn act_sync_angle(c: &Bound<'_, PyAny>, i: usize) -> R<()> {
    let d = dev_of(c, i)?;
    let st = d.get_item("state")?;
    if is_hinge_dev(&d, Some(&c.getattr("reg")?))? {
        st.set_item("angle", 0.0)?;
        st.set_item("angle_target", 0.0)?;
        return Ok(());
    }
    let Some(tw) = measure_twist(c, i)? else { return Ok(()) };
    let cur = float_of(&sget(&st, "angle", 0.0)?)?;
    let ang = cur + ((tw - cur + 180.0).rem_euclid(360.0) - 180.0);
    st.set_item("angle", ang)?;
    st.set_item("angle_target", ang)?;
    Ok(())
}

/// 出力側ブロックの向き（度）
#[pyfunction]
#[pyo3(signature = (c, i, world=None))]
pub fn act_output_angle(c: &Bound<'_, PyAny>, i: usize, world: Option<&Bound<'_, PyAny>>) -> R<f64> {
    let py = c.py();
    let j = c.getattr("index")?.call_method1("get", (output_cell(c, i)?,))?;
    if j.is_none() || !is_attached(c, i)? {
        return Ok(0.0);
    }
    let d = dev_of(c, i)?;
    if is_hinge_dev(&d, Some(&c.getattr("reg")?))? {
        return float_of(&sget(&d.get_item("state")?, "angle", 0.0)?);
    }
    let j: usize = j.extract()?;
    let (ctr, n, _) = face_info(c, i)?;
    let fr: numpy::PyReadonlyArray3<f64> = c.call_method0("voxel_frames")?.extract()?;
    let fa = fr.as_array();
    let axis = d.get_item("dir")?.extract::<i64>()?.rem_euclid(6) / 2;
    let row = if axis == 1 { 2 } else { 1 };
    let mut r = [fa[[j, row, 0]], fa[[j, row, 1]], fa[[j, row, 2]]];
    let ctr_py = PyArray1::from_vec_bound(py, ctr.to_vec());
    let mut up = [0.0, 1.0, 0.0];
    if let Some(w) = world.filter(|w| !w.is_none()) {
        let pl = w.call_method1("nearest_planet", (&ctr_py,))?;
        if !pl.is_none() {
            let u: Vec<f64> = pl.call_method1("up", (&ctr_py,))?.extract()?;
            up = [u[0], u[1], u[2]];
        }
    }
    let mut wv = up;
    if dot(n, up).abs() > 0.7 {
        let fit = c.call_method0("rigid_fit")?;
        let rm: numpy::PyReadonlyArray2<f64> = fit.get_item(0)?.extract()?;
        let a = rm.as_array();
        wv = [a[[0, 2]], a[[1, 2]], a[[2, 2]]];
    }
    let rn = dot(r, n);
    let wn = dot(wv, n);
    for a in 0..3 {
        r[a] -= n[a] * rn;
        wv[a] -= n[a] * wn;
    }
    if dot(r, r).sqrt() < 1e-6 || dot(wv, wv).sqrt() < 1e-6 {
        return Ok(0.0);
    }
    Ok(dot(n, cross(wv, r)).atan2(dot(wv, r)).to_degrees())
}

fn fmt(v: &Bound<'_, PyAny>, spec: &str) -> R<String> {
    Ok(v.call_method1("__format__", (spec,))?.to_string())
}

fn pget<'py>(p: &Bound<'py, PyAny>, k: &str, default: f64) -> R<Bound<'py, PyAny>> {
    sget(p, k, default)
}

/// P メニューで設定した動作を実行（E キー・操縦席のキー）
#[pyfunction]
pub fn act_execute_preset(world: &Bound<'_, PyAny>, c: &Bound<'_, PyAny>, i: usize) -> R<String> {
    let py = c.py();
    let dev = dev_of(c, i)?;
    let reg = c.getattr("reg")?;
    let st = dev.get_item("state")?;
    let mut p = st.call_method1("get", ("preset",))?;
    if !p.is_truthy()? {
        p = default_preset(py, &dev, Some(&reg))?.into_any();
    }
    let mut a = sget(&p, "action", "rotate")?.str()?.to_string();
    let m = motion(&dev, Some(&reg))?;
    if !actions_for(m).contains(&a.as_str()) {
        p = default_preset(py, &dev, Some(&reg))?.into_any();
        a = p.get_item("action")?.str()?.to_string();
    }
    let name = dev.get_item("name")?.str()?.to_string();
    let hinge = is_hinge_dev(&dev, Some(&reg))?;
    let speed = p.call_method1("get", ("speed",))?;
    let speed_opt = if speed.is_none() { None } else { Some(&speed) };
    match a.as_str() {
        "none" => Ok(format!("{name}: 設定の動作は「なし」です（コンピューターのプログラムで動かします）")),
        "rotate" if hinge => {
            let mut to = float_of(&pget(&p, "amount", 90.0)?)?;
            if (float_of(&sget(&st, "angle_target", 0.0)?)? - to).abs() < 0.5 {
                to = 0.0;
            }
            act_cmd_rotate(c, i, &to.into_py(py).into_bound(py), speed_opt, true)?;
            Ok(format!("{name}: {}° へ曲げる（{}°/秒）", fmt(&st.get_item("angle_target")?, "+.0f")?,
                       fmt(&pget(&p, "speed", 90.0)?, ".0f")?))
        }
        "rotate" => {
            let amt = pget(&p, "amount", 90.0)?;
            act_cmd_rotate(c, i, &amt, speed_opt, false)?;
            Ok(format!("{name}: {}° 回転（{}°/秒）", fmt(&pget(&p, "amount", 90.0)?, "+.0f")?, fmt(&pget(&p, "speed", 90.0)?, ".0f")?))
        }
        "extend" => {
            let before = st.get_item("ext_target")?;
            act_cmd_extend(c, i, Some(&pget(&p, "amount", 1.0)?), speed_opt, false)?;
            if st.get_item("ext_target")?.eq(&before)? {
                return Ok(format!("{name}: これ以上押し出せません（最大 {:.1} ブロック）", stroke(c, i)?));
            }
            Ok(format!("{name}: {} ブロック押し出し", fmt(&pget(&p, "amount", 1.0)?, ".2f")?))
        }
        "retract" => {
            let before = st.get_item("ext_target")?;
            let amt = -float_of(&pget(&p, "amount", 1.0)?)?;
            act_cmd_extend(c, i, Some(&amt.into_py(py).into_bound(py)), speed_opt, false)?;
            if st.get_item("ext_target")?.eq(&before)? {
                return Ok(format!("{name}: これ以上引き込めません"));
            }
            Ok(format!("{name}: {} ブロック引き込み", fmt(&pget(&p, "amount", 1.0)?, ".2f")?))
        }
        "detach" => {
            let ok = world.call_method1("actuator_detach", (c, i))?.is_truthy()?;
            Ok(format!("{name}: {}", if ok { "分離しました" } else { "結合しているものがありません" }))
        }
        "attach" => {
            let r = py.import_bound("main.actuator")?.getattr("cmd_attach")?.call1((world, c, i))?;
            Ok(format!("{name}: {}", r.str()?))
        }
        _ => Ok(format!("{name}: 不明な動作 {a}")),
    }
}

/// P メニューの表示
#[pyfunction]
#[pyo3(signature = (st, hinge=false))]
pub fn act_preset_text(py: Python<'_>, st: &Bound<'_, PyAny>, hinge: bool) -> R<String> {
    let mut p = st.call_method1("get", ("preset",))?;
    if !p.is_truthy()? {
        let d = PyDict::new_bound(py);
        d.set_item("action", "rotate")?;
        d.set_item("amount", 90.0)?;
        d.set_item("speed", 90.0)?;
        p = d.into_any();
    }
    let a = sget(&p, "action", "rotate")?;
    let names = [("rotate", "回転"), ("extend", "押し出し"), ("retract", "引き込み"), ("detach", "分離"), ("attach", "結合"), ("none", "なし")];
    let an = a.str()?.to_string();
    let name = names.iter().find(|x| x.0 == an).map(|x| x.1.to_string());
    if an == "rotate" {
        if hinge {
            return Ok(format!("{}° と 0° を行き来 / {}°/秒", fmt(&pget(&p, "amount", 90.0)?, "+.0f")?, fmt(&pget(&p, "speed", 90.0)?, ".0f")?));
        }
        return Ok(format!("回転 {}° / {}°/秒", fmt(&pget(&p, "amount", 90.0)?, "+.0f")?, fmt(&pget(&p, "speed", 90.0)?, ".0f")?));
    }
    if an == "extend" || an == "retract" {
        return Ok(format!("{} {}ブロック / {}ブロック/秒", name.unwrap(), fmt(&pget(&p, "amount", 1.0)?, ".2f")?,
                          fmt(&pget(&p, "speed", 0.5)?, ".2f")?));
    }
    Ok(name.unwrap_or(an))
}

/// 毎ティック：角度・伸びを目標へ進め、形（act_scale）に反映する
#[pyfunction]
pub fn act_step(c: &Bound<'_, PyAny>, i: usize, dd: &Bound<'_, PyAny>, dt: f64, powered: bool, ratio: f64) -> R<()> {
    let py = c.py();
    let dev = dev_of(c, i)?;
    let st = dev.get_item("state")?;
    crate::game::devstate::devstate_fill(py, "actuator", Some(&dd.get_item("params")?), st.downcast::<PyDict>()?)?;
    let mut powered = powered;
    let mut moving = false;
    const LOCKED: &str = "両側ともアンカーで固定されていて動かせません";
    if powered && c.getattr("anchored")?.is_truthy()? && c.call_method1("actuator_locked", (i,))?.is_truthy()? {
        st.set_item("angle_target", st.get_item("angle")?)?;
        st.set_item("ext_target", st.get_item("ext")?)?;
        if !st.call_method1("get", ("error",))?.is_truthy()? {
            st.set_item("error", LOCKED)?;
        }
        powered = false;
    } else if st.call_method1("get", ("error",))?.eq(LOCKED)? {
        st.call_method1("pop", ("error", py.None()))?;
    }
    let f = |k: &str| -> R<f64> { float_of(&st.get_item(k)?) };
    if powered {
        let (e, et) = (f("ext")?, f("ext_target")?);
        if (et - e).abs() > 1e-4 {
            let stp = f("ext_speed")? * dt * ratio;
            st.set_item("ext", e + (et - e).max(-stp).min(stp))?;
            moving = true;
        }
        let (a, at) = (f("angle")?, f("angle_target")?);
        if (at - a).abs() > 1e-3 {
            let stp = f("rot_speed")? * dt * ratio;
            st.set_item("angle", a + (at - a).max(-stp).min(stp))?;
            moving = true;
        }
    }
    if moving {
        wake(c)?;
    }
    let axis = dev.get_item("dir")?.extract::<i64>()?.rem_euclid(6) as usize / 2;
    let s = c.getattr("act_scale")?;
    let mut arr: numpy::PyReadwriteArray2<f64> = s.extract()?;
    let mut a = arr.as_array_mut();
    for k in 0..3 {
        a[[i, k]] = 1.0;
    }
    a[[i, axis]] = 1.0 + f("ext")?;
    st.set_item("attached", is_attached(c, i)?)?;
    Ok(())
}
