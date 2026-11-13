//! マルチプレイで、ホストと参加者が同じ形で使う「状態の送り方・受け方」（main/netsync.py）。
//! 構築物：形が変わったら丸ごと、剛体の間は (R, x, v, w)、ブロックごとに動く間は頂点、止まったら最後の位置を1回だけ。
//! 機器・資源・電力などの中身（メタ）は変わったときだけ。プログラムのファイルは変わったファイルだけ。

use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyBytes, PyDict, PyList, PyTuple};

use super::construct::{a1, add, cross, fv, mat, norm, v3, v3s, R};
use super::imp;

const PRIVATE: [&str; 1] = ["memory"];            // 機器の状態のうち送らないもの（"_" で始まるものも送らない）
const TEXT_SUFFIXES: [&str; 4] = [".py", ".json", ".txt", ".md"];
pub const EXTRAPOLATE: f64 = 0.08;                // 更新が来ない間、速度のまま進めてよい時間

fn np(py: Python<'_>) -> R<Bound<'_, PyModule>> {
    imp(py, "numpy")
}
fn f32a<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    o.call_method1("astype", (np(o.py())?.getattr("float32")?,))
}
fn f64a<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    np(o.py())?.call_method1("asarray", (o, "float64"))
}
fn opt<'py>(o: &Bound<'py, PyAny>, k: &str) -> R<Option<Bound<'py, PyAny>>> {
    super::construct::opt(o, k)
}
fn private(k: &str) -> bool {
    k.starts_with('_') || PRIVATE.contains(&k)
}

// ---------------------------------------------------------------- 構築物

/// ホスト：前に送った構築物の状態を覚えておき、変わった分だけの送り物を作る
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct ConsTracker {
    #[pyo3(get, set)]
    sent: PyObject,
}

#[pymethods]
impl ConsTracker {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        ConsTracker { sent: PyDict::new_bound(py).into_any().unbind() }
    }
    fn diff<'py>(&self, py: Python<'py>, world: &Bound<'py, PyAny>) -> R<(Bound<'py, PyList>, Bound<'py, PyList>)> {
        let out = PyList::empty_bound(py);
        let sent = self.sent.bind(py).downcast::<PyDict>()?.clone();
        let cons = world.getattr("constructs")?;
        for c in PyList::new_bound(py, cons.call_method0("values")?.iter()?.collect::<R<Vec<_>>>()?).iter() {
            if c.getattr("n_vox")?.extract::<usize>()? == 0 {
                continue;
            }
            let id = c.getattr("id")?;
            let ver = c.getattr("version")?;
            let asleep = c.getattr("asleep")?.is_truthy()?;
            let mut s = sent.call_method1("get", (&id,))?;
            if s.is_none() || !s.get_item("ver")?.eq(&ver)? {
                let e = PyDict::new_bound(py);
                e.set_item("id", &id)?;
                e.set_item("full", c.call_method0("to_dict")?)?;
                e.set_item("spawn", c.call_method0("spawn_dict")?)?;
                let ns = PyDict::new_bound(py);
                ns.set_item("ver", &ver)?;
                ns.set_item("sleep", py.None())?;
                ns.set_item("rb", py.None())?;
                sent.set_item(&id, &ns)?;
                s = ns.into_any();
                if asleep {
                    e.set_item("sleep", true)?;
                    s.set_item("sleep", c.getattr("sleep_epoch")?)?;
                }
                out.append(e)?;
                if asleep {
                    continue;
                }
            }
            if asleep {
                let ep = c.getattr("sleep_epoch")?;
                if !s.get_item("sleep")?.eq(&ep)? {
                    let e = PyDict::new_bound(py);
                    e.set_item("id", &id)?;
                    e.set_item("sleep", f32a(&c.getattr("pos")?)?)?;
                    out.append(e)?;
                    s.set_item("sleep", ep)?;
                    s.set_item("rb", py.None())?;
                }
                continue;
            }
            s.set_item("sleep", py.None())?;
            let mode: String = c.getattr("mode")?.extract()?;
            let rb = c.getattr("rb")?;
            if (mode == "rigid" || mode == "rails") && !rb.is_none() {
                let e = PyDict::new_bound(py);
                e.set_item("id", &id)?;
                for k in ["R", "x", "v", "w"] {
                    e.set_item(k, f64a(&rb.get_item(k)?)?)?;
                }
                let ep = c.getattr("rb_epoch")?;
                if !s.get_item("rb")?.eq(&ep)? {
                    e.set_item("local", np(py)?.call_method1("asarray", (rb.get_item("local")?, np(py)?.getattr("float32")?))?)?;
                    s.set_item("rb", ep)?;
                }
                out.append(e)?;
            } else {
                s.set_item("rb", py.None())?;
                let e = PyDict::new_bound(py);
                e.set_item("id", &id)?;
                e.set_item("pos", f32a(&c.getattr("pos")?)?)?;
                out.append(e)?;
            }
        }
        let gone = PyList::empty_bound(py);
        for cid in sent.keys() {
            let c = cons.call_method1("get", (&cid,))?;
            if c.is_none() || c.getattr("n_vox")?.extract::<usize>()? == 0 {
                gone.append(cid)?;
            }
        }
        for cid in gone.iter() {
            sent.del_item(cid)?;
        }
        Ok((out, gone))
    }
}

fn sleep_c(c: &Bound<'_, PyAny>) -> R<()> {
    c.setattr("vel", np(c.py())?.call_method1("zeros_like", (c.getattr("pos")?,))?)?;
    c.setattr("asleep", true)?;
    let ep: i64 = c.getattr("sleep_epoch")?.extract()?;
    c.setattr("sleep_epoch", ep + 1)
}
#[pyfunction]
#[pyo3(name = "netsync_sleep")]
pub fn netsync_sleep(c: &Bound<'_, PyAny>) -> R<()> {
    sleep_c(c)
}

/// 参加者：構築物の更新を1つ当てる。作り直したら新しい構築物を返す
#[pyfunction]
pub fn netsync_apply_construct<'py>(py: Python<'py>, world: &Bound<'py, PyAny>, e: &Bound<'py, PyAny>, dt_since: f64) -> R<Bound<'py, PyAny>> {
    let none = py.None().into_bound(py);
    let cid = e.get_item("id")?;
    let cons = world.getattr("constructs")?;
    let c0 = cons.call_method1("get", (&cid,))?;
    if !c0.is_none() {
        c0.setattr("_net_age", 0.0)?;
    }
    if e.contains("full")? {
        let sp = e.call_method1("get", ("spawn",))?;
        let sp = if sp.is_truthy()? { sp } else { PyDict::new_bound(py).into_any() };
        let c = imp(py, "main.construct")?.getattr("Construct")?.call_method1("from_dict", (world.getattr("registry")?, e.get_item("full")?, sp))?;
        if !c0.is_none() && c.getattr("anchor")?.is_none() {
            c.setattr("anchor", c0.getattr("anchor")?)?;
        }
        cons.set_item(&cid, &c)?;
        c.setattr("_net_age", 0.0)?;
        if e.call_method1("get", ("sleep",))?.is_truthy()? {
            sleep_c(&c)?;
        }
        return Ok(c);
    }
    let c = c0;
    if c.is_none() {
        return Ok(none);
    }
    if e.contains("sleep")? {
        let p = f64a(&e.get_item("sleep")?)?;
        if p.getattr("shape")?.eq(c.getattr("pos")?.getattr("shape")?)? {
            c.setattr("pos", p)?;
        }
        c.setattr("mode", "soft")?;
        c.setattr("rb", py.None())?;
        sleep_c(&c)?;
    } else if e.contains("R")? {
        if e.contains("local")? || c.getattr("rb")?.is_none() || !c.getattr("mode")?.eq("rails")? {
            let loc = match e.call_method1("get", ("local",))? {
                l if !l.is_none() => f64a(&l)?,
                _ => {
                    // 届く前に作り直したとき（次の版で揃う）
                    let x = v3(&e.get_item("x")?)?;
                    let rm = super::construct::m3(&e.get_item("R")?)?;
                    let l: Vec<[f64; 3]> = v3s(&c.getattr("pos")?)?.iter().map(|p| super::construct::mul_r(super::construct::sub(*p, x), &rm)).collect();
                    super::construct::a3(py, &l)?
                }
            };
            let rb = PyDict::new_bound(py);
            rb.set_item("local", loc)?;
            rb.set_item("R", mat(py, &super::construct::eye())?)?;
            for k in ["x", "v", "w"] {
                rb.set_item(k, a1(py, [0.0; 3]))?;
            }
            rb.set_item("Ib", mat(py, &super::construct::eye())?)?;
            rb.set_item("ext", 1.0)?;
            c.setattr("rb", rb)?;
            let ep: i64 = c.getattr("rb_epoch")?.extract()?;
            c.setattr("rb_epoch", ep + 1)?;
        }
        c.setattr("mode", "rails")?;
        let rb = c.getattr("rb")?;
        for k in ["R", "x", "v", "w"] {
            rb.set_item(k, f64a(&e.get_item(k)?)?)?;
        }
        c.call_method0("_rails_sync")?;
        c.setattr("asleep", false)?;
    } else if e.contains("pos")? {
        let p = f64a(&e.get_item("pos")?)?;
        let pos = c.getattr("pos")?;
        if !p.getattr("shape")?.eq(pos.getattr("shape")?)? {
            return Ok(none);
        }
        if c.getattr("mode")?.eq("rails")? {
            c.setattr("mode", "soft")?;
            c.setattr("rb", py.None())?;
            c.setattr("vel", np(py)?.call_method1("zeros_like", (&p,))?)?;
        } else if dt_since > 1e-3 && !c.getattr("asleep")?.is_truthy()? {
            let (a, b) = (fv(&p)?, fv(&pos)?);
            let v: Vec<f64> = a.iter().zip(&b).map(|(x, y)| (x - y) / dt_since).collect();
            let n = v.len() / 3;
            c.setattr("vel", super::construct::arr(py, v, &[n, 3])?)?;
        } else {
            c.setattr("vel", np(py)?.call_method1("zeros_like", (&p,))?)?;
        }
        c.setattr("pos", p)?;
        c.setattr("asleep", false)?;
    }
    Ok(none)
}

fn rot(axis: [f64; 3], ang: f64) -> [[f64; 3]; 3] {
    let [x, y, z] = axis;
    let k = [[0.0, -z, y], [z, 0.0, -x], [-y, x, 0.0]];
    let kk = super::construct::matmul(&k, &k);
    let mut o = super::construct::eye();
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = o[i][j] + ang.sin() * k[i][j] + (1.0 - ang.cos()) * kk[i][j];
        }
    }
    o
}
#[pyfunction]
pub fn netsync_rot<'py>(py: Python<'py>, axis: &Bound<'py, PyAny>, ang: f64) -> R<Bound<'py, PyAny>> {
    mat(py, &rot(v3(axis)?, ang))
}

/// 参加者：次の更新が届くまでの間、構築物を速度のまま進める（動きをなめらかにする）
#[pyfunction]
pub fn netsync_advance(py: Python<'_>, world: &Bound<'_, PyAny>, dt: f64) -> R<()> {
    for c in world.getattr("constructs")?.call_method0("values")?.iter()? {
        let c = c?;
        if c.getattr("asleep")?.is_truthy()? || c.getattr("n_vox")?.extract::<usize>()? == 0 {
            continue;
        }
        let age: f64 = match opt(&c, "_net_age")? { Some(a) => a.extract()?, None => 0.0 };
        c.setattr("_net_age", age + dt)?;
        if age >= EXTRAPOLATE {
            continue;
        }
        let rb = c.getattr("rb")?;
        if c.getattr("mode")?.eq("rails")? && !rb.is_none() {
            let x = v3(&rb.get_item("x")?)?;
            let v = v3(&rb.get_item("v")?)?;
            rb.set_item("x", a1(py, add(x, [v[0] * dt, v[1] * dt, v[2] * dt])))?;
            let w = v3(&rb.get_item("w")?)?;
            let wn = norm(w);
            if wn > 1e-9 {
                let r0 = super::construct::m3(&rb.get_item("R")?)?;
                rb.set_item("R", mat(py, &super::construct::matmul(&rot([w[0] / wn, w[1] / wn, w[2] / wn], wn * dt), &r0))?)?;
            }
            c.call_method0("_rails_sync")?;
        } else {
            let (p, v) = (fv(&c.getattr("pos")?)?, fv(&c.getattr("vel")?)?);
            let n = p.len() / 3;
            let q: Vec<f64> = p.iter().zip(&v).map(|(a, b)| a + b * dt).collect();
            c.setattr("pos", super::construct::arr(py, q, &[n, 3])?)?;
        }
    }
    let _ = cross;
    Ok(())
}

// ---------------------------------------------------------------- 機器・資源など（メタ）

/// 機器の、相手に見せる部分（名前と状態。"_" で始まるものと memory は除く）
#[pyfunction]
pub fn netsync_dev_public<'py>(py: Python<'py>, d: &Bound<'py, PyAny>) -> R<Bound<'py, PyTuple>> {
    let st = PyDict::new_bound(py);
    for (k, v) in d.get_item("state")?.downcast::<PyDict>()?.iter() {
        let ks: String = k.str()?.to_string();
        if !private(&ks) {
            st.set_item(k, v)?;
        }
    }
    Ok(PyTuple::new_bound(py, [d.get_item("name")?, st.into_any()]))
}

#[pyfunction]
pub fn netsync_meta_of<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let devs = PyList::empty_bound(py);
    for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
        let d = d?;
        if d.is_truthy()? {
            let p = netsync_dev_public(py, &d)?;
            devs.append(PyList::new_bound(py, [i.into_py(py), p.get_item(0)?.unbind(), p.get_item(1)?.unbind()]))?;
        }
    }
    let m = PyDict::new_bound(py);
    m.set_item("name", c.getattr("name")?)?;
    m.set_item("devs", devs)?;
    m.set_item("status", c.getattr("status")?)?;
    m.set_item("storage", c.getattr("storage")?)?;
    m.set_item("powered", c.getattr("powered")?.is_truthy()?)?;
    m.set_item("anchor", c.getattr("anchor")?)?;
    let cav = PyList::empty_bound(py);
    for (k, v) in c.getattr("cavity_air")?.downcast::<PyDict>()?.iter() {
        cav.append(PyList::new_bound(py, [PyList::new_bound(py, k.iter()?.collect::<R<Vec<_>>>()?).into_any(), v]))?;
    }
    m.set_item("cav", cav)?;
    Ok(m)
}

/// json.dumps の default：numpy の配列・数、集合・タプルを JSON にできる形に
#[pyfunction]
pub fn netsync_default(py: Python<'_>, x: &Bound<'_, PyAny>) -> R<PyObject> {
    let np = np(py)?;
    if x.is_instance(&np.getattr("ndarray")?)? {
        return Ok(x.call_method0("tolist")?.unbind());
    }
    if x.is_instance(&np.getattr("integer")?)? {
        return Ok(x.call_method0("__int__")?.unbind());
    }
    if x.is_instance(&np.getattr("floating")?)? {
        return Ok(x.call_method0("__float__")?.unbind());
    }
    if x.is_instance_of::<pyo3::types::PySet>() || x.is_instance_of::<PyTuple>() {
        return Ok(PyList::new_bound(py, x.iter()?.collect::<R<Vec<_>>>()?).into_any().unbind());
    }
    Ok(x.str()?.into_any().unbind())
}

#[pyfunction]
pub fn netsync_dumps(py: Python<'_>, x: &Bound<'_, PyAny>) -> R<String> {
    let kw = PyDict::new_bound(py);
    kw.set_item("sort_keys", true)?;
    kw.set_item("ensure_ascii", false)?;
    kw.set_item("default", wrap_pyfunction_bound!(netsync_default, py)?)?;
    imp(py, "json")?.call_method("dumps", (x,), Some(&kw))?.extract()
}
#[pyfunction]
pub fn netsync_jsonable<'py>(py: Python<'py>, x: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    imp(py, "json")?.call_method1("loads", (netsync_dumps(py, x)?,))
}

#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct MetaTracker {
    #[pyo3(get, set)]
    sent: PyObject,
}

#[pymethods]
impl MetaTracker {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(py: Python<'_>, _a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        MetaTracker { sent: PyDict::new_bound(py).into_any().unbind() }
    }
    fn diff<'py>(&self, py: Python<'py>, world: &Bound<'py, PyAny>) -> R<Bound<'py, PyList>> {
        let out = PyList::empty_bound(py);
        let sent = self.sent.bind(py).downcast::<PyDict>()?.clone();
        let cons = world.getattr("constructs")?;
        for c in PyList::new_bound(py, cons.call_method0("values")?.iter()?.collect::<R<Vec<_>>>()?).iter() {
            let m = netsync_jsonable(py, netsync_meta_of(py, &c)?.as_any())?;
            let key = netsync_dumps(py, &m)?;
            let id = c.getattr("id")?;
            if !sent.call_method1("get", (&id,))?.eq(&key)? {
                sent.set_item(&id, &key)?;
                let e = PyDict::new_bound(py);
                e.set_item("id", &id)?;
                e.update(m.downcast::<PyDict>()?.as_mapping())?;
                out.append(e)?;
            }
        }
        let gone: Vec<Bound<'_, PyAny>> = sent.keys().iter().filter(|k| !cons.contains(k).unwrap_or(true)).collect();
        for k in gone {
            sent.del_item(k)?;
        }
        Ok(out)
    }
}

#[pyfunction]
#[pyo3(signature = (c, m, devices=true))]
pub fn netsync_apply_meta(py: Python<'_>, c: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>, devices: bool) -> R<()> {
    let g = |k: &str| m.call_method1("get", (k,));
    c.setattr("name", m.call_method1("get", ("name", c.getattr("name")?))?)?;
    let or_dict = |v: Bound<'_, PyAny>| -> R<PyObject> { Ok(if v.is_truthy()? { v.unbind() } else { PyDict::new_bound(py).into_any().unbind() }) };
    c.setattr("status", or_dict(g("status")?)?)?;
    c.setattr("storage", or_dict(g("storage")?)?)?;
    c.setattr("powered", g("powered")?.is_truthy()?)?;
    c.setattr("anchor", g("anchor")?)?;
    let cav = PyDict::new_bound(py);
    for kv in m.call_method1("get", ("cav", PyList::empty_bound(py)))?.iter()? {
        let kv = kv?;
        cav.set_item(PyTuple::new_bound(py, kv.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?), kv.get_item(1)?)?;
    }
    c.setattr("cavity_air", cav)?;
    if !devices {
        return Ok(());
    }
    let devs = c.getattr("devs")?;
    let n = devs.len()?;
    for e in m.call_method1("get", ("devs", PyList::empty_bound(py)))?.iter()? {
        let e = e?;
        let i: usize = e.get_item(0)?.extract()?;
        if i >= n {
            continue;
        }
        let d = devs.get_item(i)?;
        if !d.is_truthy()? {
            continue;
        }
        d.set_item("name", e.get_item(1)?)?;
        let st = PyDict::new_bound(py);
        st.update(e.get_item(2)?.downcast::<PyDict>()?.as_mapping())?;
        for (k, v) in d.get_item("state")?.downcast::<PyDict>()?.iter() {
            if private(&k.str()?.to_string()) {
                st.set_item(k, v)?;
            }
        }
        d.set_item("state", st)?;
    }
    Ok(())
}

/// 参加者：機器の画面で変えた分（base は開いたときの dev_public の一覧）。[(セル, 名前, 変えた値, 消したキー)]
#[pyfunction]
pub fn netsync_dev_patch<'py>(py: Python<'py>, c: &Bound<'py, PyAny>, base: &Bound<'py, PyAny>) -> R<Bound<'py, PyList>> {
    let out = PyList::empty_bound(py);
    let cells = c.getattr("cells")?;
    for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
        let d = d?;
        if !d.is_truthy()? || !base.contains(i)? {
            continue;
        }
        let p = netsync_jsonable(py, netsync_dev_public(py, &d)?.as_any())?;
        let (name, st) = (p.get_item(0)?, p.get_item(1)?);
        let b = base.get_item(i)?;
        let (bname, bst) = (b.get_item(0)?, b.get_item(1)?);
        let chg = PyDict::new_bound(py);
        for (k, v) in st.downcast::<PyDict>()?.iter() {
            let same = match bst.downcast::<PyDict>()?.get_item(&k)? { Some(bv) => bv.eq(&v)?, None => false };
            if !same {
                chg.set_item(k, v)?;
            }
        }
        let rm = PyList::empty_bound(py);
        for k in bst.downcast::<PyDict>()?.keys() {
            if !st.contains(&k)? {
                rm.append(k)?;
            }
        }
        if !name.eq(&bname)? || !chg.is_empty() || !rm.is_empty() {
            let e = PyDict::new_bound(py);
            e.set_item("cell", PyList::new_bound(py, cells.get_item(i)?.iter()?.collect::<R<Vec<_>>>()?))?;
            e.set_item("name", name)?;
            e.set_item("set", chg)?;
            e.set_item("del", rm)?;
            out.append(e)?;
        }
    }
    Ok(out)
}

#[pyfunction]
pub fn netsync_dev_base<'py>(py: Python<'py>, c: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let out = PyDict::new_bound(py);
    for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
        let d = d?;
        if d.is_truthy()? {
            let j = netsync_jsonable(py, netsync_dev_public(py, &d)?.as_any())?;
            out.set_item(i, PyTuple::new_bound(py, j.iter()?.collect::<R<Vec<_>>>()?))?;
        }
    }
    Ok(out)
}

/// ホスト：参加者が機器の画面で変えた分を当てる
#[pyfunction]
pub fn netsync_apply_dev_patch(py: Python<'_>, world: &Bound<'_, PyAny>, cid: &Bound<'_, PyAny>, patches: &Bound<'_, PyAny>) -> R<()> {
    let c = world.getattr("constructs")?.call_method1("get", (cid,))?;
    if c.is_none() {
        return Ok(());
    }
    let devs = c.getattr("devs")?;
    for p in patches.iter()? {
        let p = p?;
        let i = c.getattr("index")?.call_method1("get", (PyTuple::new_bound(py, p.get_item("cell")?.iter()?.collect::<R<Vec<_>>>()?),))?;
        if i.is_none() {
            continue;
        }
        let d = devs.get_item(&i)?;
        if !d.is_truthy()? {
            continue;
        }
        let nm = p.call_method1("get", ("name",))?;
        if nm.is_truthy()? {
            d.set_item("name", nm)?;
        }
        let st = d.get_item("state")?;
        let set = p.call_method1("get", ("set",))?;
        if set.is_truthy()? {
            st.call_method1("update", (&set,))?;
        }
        let del = p.call_method1("get", ("del",))?;
        if del.is_truthy()? {
            for k in del.iter()? {
                st.call_method1("pop", (k?, py.None()))?;
            }
        }
        if set.is_truthy()? && ["running", "script", "blocks"].iter().any(|k| set.contains(*k).unwrap_or(false)) {
            st.call_method1("pop", ("_env", py.None()))?;          // プログラムを変えた・実行し直した：最初から
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- プログラムのファイル

fn text_name(n: &str) -> bool {
    TEXT_SUFFIXES.iter().any(|s| n.ends_with(s))
}
fn sha1(py: Python<'_>, data: &Bound<'_, PyAny>) -> R<String> {
    imp(py, "hashlib")?.call_method1("sha1", (data,))?.call_method0("hexdigest")?.extract()
}
#[pyfunction]
pub fn netsync_hash(py: Python<'_>, data: &Bound<'_, PyAny>) -> R<String> {
    sha1(py, data)
}

/// フォルダの中のファイル（プログラム）の変化を見張る。相手から受け取ったものは送り返さない
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct FileSync {
    #[pyo3(get, set)]
    dir: PyObject,
    #[pyo3(get, set)]
    known: PyObject,
    #[pyo3(get, set)]
    stat: PyObject,
}

#[pymethods]
impl FileSync {
    #[new]
    fn new(py: Python<'_>, folder: PyObject) -> Self {
        FileSync { dir: folder, known: PyDict::new_bound(py).into_any().unbind(), stat: PyDict::new_bound(py).into_any().unbind() }
    }
    fn all<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyDict>> {
        let out = PyDict::new_bound(py);
        let d = self.dir.bind(py);
        if d.call_method0("is_dir")?.is_truthy()? {
            let files = imp(py, "builtins")?.call_method1("sorted", (d.call_method0("iterdir")?,))?;
            for f in files.iter()? {
                let f = f?;
                let name: String = f.getattr("name")?.extract()?;
                if f.call_method0("is_file")?.is_truthy()? && text_name(&name)
                    && f.call_method0("stat")?.getattr("st_size")?.extract::<i64>()? < 4_000_000 {
                    out.set_item(&name, f.call_method0("read_bytes")?)?;
                }
            }
        }
        Ok(out)
    }
    fn mark_all(&self, py: Python<'_>) -> R<()> {
        let known = self.known.bind(py);
        for (k, v) in self.all(py)?.iter() {
            known.set_item(k, sha1(py, &v)?)?;
        }
        Ok(())
    }
    /// 前に見たときから変わったファイル {名前: 中身}
    fn changes<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyDict>> {
        let out = PyDict::new_bound(py);
        let d = self.dir.bind(py);
        if !d.call_method0("is_dir")?.is_truthy()? {
            return Ok(out);
        }
        let (stat, known) = (self.stat.bind(py), self.known.bind(py));
        for f in d.call_method0("iterdir")?.iter()? {
            let f = f?;
            let name: String = f.getattr("name")?.extract()?;
            if !(f.call_method0("is_file")?.is_truthy()? && text_name(&name)) {
                continue;
            }
            let st = match f.call_method0("stat") {
                Ok(s) => s,
                Err(e) if e.is_instance_of::<pyo3::exceptions::PyOSError>(py) => continue,
                Err(e) => return Err(e),
            };
            let key = PyTuple::new_bound(py, [st.getattr("st_mtime_ns")?, st.getattr("st_size")?]);
            if stat.call_method1("get", (&name,))?.eq(&key)? {
                continue;
            }
            stat.set_item(&name, key)?;
            let data = match f.call_method0("read_bytes") {
                Ok(b) => b,
                Err(e) if e.is_instance_of::<pyo3::exceptions::PyOSError>(py) => continue,
                Err(e) => return Err(e),
            };
            let h = sha1(py, &data)?;
            if !known.call_method1("get", (&name,))?.eq(&h)? {
                known.set_item(&name, h)?;
                out.set_item(&name, data)?;
            }
        }
        Ok(out)
    }
    /// 相手から受け取ったファイルを書く（名前にフォルダは含めない）
    fn put(&self, py: Python<'_>, name: &str, data: &Bound<'_, PyAny>) -> R<bool> {
        if name.contains('/') || name.contains('\\') || name.contains("..") || !text_name(name) {
            return Ok(false);
        }
        let d = self.dir.bind(py);
        d.call_method("mkdir", (), Some(&[("parents", true), ("exist_ok", true)].into_py_dict_bound(py)))?;
        let p = d.call_method1("__truediv__", (name,))?;
        p.call_method1("write_bytes", (data,))?;
        self.known.bind(py).set_item(name, sha1(py, data)?)?;
        let st = p.call_method0("stat")?;
        self.stat.bind(py).set_item(name, PyTuple::new_bound(py, [st.getattr("st_mtime_ns")?, st.getattr("st_size")?]))?;
        Ok(true)
    }
}

// ---------------------------------------------------------------- OBJ 読み込みの結果（参加者 → ホスト）

#[pyfunction]
pub fn netsync_pack_import<'py>(py: Python<'py>, data: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    let out = PyDict::new_bound(py);
    out.update(data.downcast::<PyDict>()?.as_mapping())?;
    let vox = PyList::empty_bound(py);
    for v in data.get_item("voxels")?.iter()? {
        let v = v?;
        vox.append(PyList::new_bound(py, [PyList::new_bound(py, v.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?).into_any(), v.get_item(1)?, v.get_item(2)?]))?;
    }
    out.set_item("voxels", vox)?;
    let l = data.call_method1("get", ("links",))?;
    if !l.is_none() {
        let ll = PyList::empty_bound(py);
        for k in l.iter()? {
            let k = k?;
            ll.append([PyList::new_bound(py, k.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?),
                       PyList::new_bound(py, k.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?)])?;
        }
        out.set_item("links", ll)?;
    }
    netsync_jsonable(py, out.as_any())
}

#[pyfunction]
pub fn netsync_unpack_import<'py>(py: Python<'py>, d: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let out = PyDict::new_bound(py);
    out.update(d.downcast::<PyDict>()?.as_mapping())?;
    let vox = PyList::empty_bound(py);
    for v in d.get_item("voxels")?.iter()? {
        let v = v?;
        vox.append((PyTuple::new_bound(py, v.get_item(0)?.iter()?.collect::<R<Vec<_>>>()?), v.get_item(1)?, v.get_item(2)?))?;
    }
    out.set_item("voxels", vox)?;
    let l = d.call_method1("get", ("links",))?;
    if !l.is_none() {
        let s = pyo3::types::PySet::empty_bound(py)?;
        for k in l.iter()? {
            let k = k?;
            s.add(super::construct::construct_link_key(py, &k.get_item(0)?, &k.get_item(1)?)?)?;
        }
        out.set_item("links", s)?;
    }
    Ok(out)
}

// ---------------------------------------------------------------- プレイヤー

fn tolist<'py>(o: &Bound<'py, PyAny>) -> R<Bound<'py, PyAny>> {
    f64a(o)?.call_method0("tolist")
}

/// ほかの人に見せるプレイヤーの状態（描画用）
#[pyfunction]
pub fn netsync_player_public<'py>(py: Python<'py>, p: &Bound<'py, PyAny>, pid: &Bound<'py, PyAny>, name: &Bound<'py, PyAny>) -> R<Bound<'py, PyDict>> {
    let d = PyDict::new_bound(py);
    d.set_item("pid", pid)?;
    d.set_item("name", name)?;
    d.set_item("def", p.getattr("defn")?.get_item("id")?)?;
    d.set_item("pos", p.getattr("pos")?.call_method0("tolist")?)?;
    d.set_item("up", p.getattr("up")?.call_method0("tolist")?)?;
    d.set_item("vel", p.getattr("vel")?.call_method0("tolist")?)?;
    d.set_item("gv", tolist(&p.getattr("ground_vel")?)?)?;
    d.set_item("og", p.getattr("on_ground")?.is_truthy()?)?;
    d.set_item("jp", p.getattr("jetpack")?.is_truthy()?)?;
    d.set_item("fl", match opt(p, "flying")? { Some(f) => f.is_truthy()?, None => false })?;
    d.set_item("wt", p.getattr("in_water")?.call_method0("__float__")?)?;
    let s = p.getattr("seated")?;
    d.set_item("seat", if s.is_truthy()? {
        PyList::new_bound(py, [s.get_item(0)?, PyList::new_bound(py, s.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any()]).into_any()
    } else { py.None().into_bound(py) })?;
    d.set_item("fwd", match opt(p, "look_fwd")? { Some(f) => tolist(&f)?, None => PyList::new_bound(py, [0.0, 0.0, 1.0]).into_any() })?;
    d.set_item("pitch", match opt(p, "pitch")? { Some(f) => f.extract::<f64>()?, None => 0.0 })?;
    d.set_item("emote", match p.getattr("emote") { Ok(e) => e, Err(_) => py.None().into_bound(py) })?;
    d.set_item("face", match opt(p, "face_dir")? { Some(f) => tolist(&f)?, None => py.None().into_bound(py) })?;
    Ok(d)
}

#[pyfunction]
pub fn netsync_apply_player_public(py: Python<'_>, world: &Bound<'_, PyAny>, p: &Bound<'_, PyAny>, d: &Bound<'_, PyAny>) -> R<()> {
    if !p.getattr("defn")?.call_method1("get", ("id",))?.eq(d.get_item("def")?)? {
        p.setattr("defn", world.call_method1("player_def", (d.get_item("def")?,))?)?;
    }
    for (k, key) in [("pos", "pos"), ("up", "up"), ("vel", "vel"), ("ground_vel", "gv"), ("look_fwd", "fwd")] {
        p.setattr(k, f64a(&d.get_item(key)?)?)?;
    }
    for (k, key) in [("on_ground", "og"), ("jetpack", "jp"), ("flying", "fl"), ("in_water", "wt"), ("pitch", "pitch"), ("name", "name"), ("pid", "pid")] {
        p.setattr(k, d.get_item(key)?)?;
    }
    let s = d.call_method1("get", ("seat",))?;
    p.setattr("seated", if s.is_truthy()? {
        PyTuple::new_bound(py, [s.get_item(0)?, PyTuple::new_bound(py, s.get_item(1)?.iter()?.collect::<R<Vec<_>>>()?).into_any()]).into_any()
    } else { py.None().into_bound(py) })?;
    let em = d.call_method1("get", ("emote",))?;
    let cur = match p.getattr("emote") { Ok(e) => e, Err(_) => py.None().into_bound(py) };
    if !em.eq(&cur)? {
        p.setattr("emote", &em)?;
        p.setattr("emote_t0", imp(py, "time")?.call_method0("monotonic")?)?;
    }
    let f = d.call_method1("get", ("face",))?;
    p.setattr("face_dir", if f.is_none() { f } else { f64a(&f)? })?;
    let _ = PyBytes::new_bound(py, b"");
    Ok(())
}
