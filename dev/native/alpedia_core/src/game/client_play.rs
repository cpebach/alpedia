//! 遊ぶ画面で共通に使う物（main/client/game.py）：フォントの候補探し・ほかの人の体（色の箱）・機器の状態の言葉。
//! 手の操作（main/client/play.py）：狙っている物の説明・置く場所の枠・三人称のカメラ・近くの触れる物。

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};

use super::imp;
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

fn tb<T: IntoPy<PyObject>>(py: Python<'_>, x: T) -> B<'_> {
    x.into_py(py).into_bound(py)
}
fn none(py: Python<'_>) -> B<'_> {
    py.None().into_bound(py)
}
fn get<'py>(d: &B<'py>, k: &str, dflt: impl IntoPy<PyObject>) -> R<B<'py>> {
    let py = d.py();
    d.call_method1("get", (k, dflt.into_py(py)))
}
fn or_dict<'py>(o: B<'py>) -> R<B<'py>> {
    let py = o.py();
    Ok(if o.is_truthy()? { o } else { PyDict::new_bound(py).into_any() })
}
fn pf(v: &B<'_>, spec: &str) -> R<String> {
    v.py().import_bound("builtins")?.getattr("format")?.call1((v, spec))?.extract()
}
fn pff(py: Python<'_>, v: f64, spec: &str) -> R<String> {
    pf(&tb(py, v), spec)
}
fn np(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "numpy")?.into_any())
}
fn v3(o: &B<'_>) -> R<[f64; 3]> {
    Ok([f64of(&o.get_item(0)?)?, f64of(&o.get_item(1)?)?, f64of(&o.get_item(2)?)?])
}
fn np3(py: Python<'_>, v: [f64; 3]) -> B<'_> {
    numpy::PyArray1::from_vec_bound(py, v.to_vec()).into_any()
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn is_exc(py: Python<'_>, e: &PyErr) -> bool {
    e.is_instance_of::<PyException>(py)
}
/// c.pos[8i:8i+8].mean(0)（numpy と同じ計算）
fn vox_center<'py>(c: &B<'py>, i: i64) -> R<B<'py>> {
    let py = c.py();
    c.getattr("pos")?.get_item(pyo3::types::PySlice::new_bound(py, (8 * i) as isize, (8 * i + 8) as isize, 1))?.call_method1("mean", (0,))
}

// ---------------------------------------------------------------- game.py
/// 日本語の出るフォント（設定の graphics.font、無ければいつもの候補から）
#[pyfunction]
pub fn game_find_font<'py>(py: Python<'py>, cfg: &B<'py>) -> R<B<'py>> {
    let custom = get(&get(cfg, "graphics", PyDict::new_bound(py))?, "font", py.None())?;
    let pathc = imp(py, "pathlib")?.getattr("Path")?;
    let mut cands: Vec<B<'_>> = vec![];
    if custom.is_truthy()? {
        cands.push(custom);
    }
    for f in imp(py, "main.client.game")?.getattr("FONT_CANDIDATES")?.iter()? {
        cands.push(f?);
    }
    for f in cands {
        if f.is_truthy()? {
            let p = pathc.call1((&f,))?;
            if p.call_method0("exists")?.is_truthy()? {
                return Ok(p);
            }
        }
    }
    Ok(none(py))
}

/// 体の見た目（MOD が引き受けないとき）：定義の色の箱。update(origin, shown, dt)・destroy()
#[pyclass(module = "main.client.game", subclass, dict)]
pub struct BoxBody {
    #[pyo3(get, set)]
    game: PyObject,
    #[pyo3(get, set)]
    player: PyObject,
    #[pyo3(get, set)]
    e: PyObject,
}

#[pymethods]
impl BoxBody {
    #[new]
    #[pyo3(signature = (game, player, model=None))]
    fn new(py: Python<'_>, game: &B<'_>, player: &B<'_>, model: Option<&B<'_>>) -> R<Self> {
        let _ = model;
        let gfx = imp(py, "main.client.gfx")?;
        let col: Vec<f64> = get(&player.getattr("defn")?, "color", vec![1, 1, 1])?.iter()?.map(|x| f64of(&x?)).collect::<R<_>>()?;
        let mx = col.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let c: Vec<f64> = if mx > 1.0 { col.iter().map(|x| x / 255.0).collect() } else { col };
        let e = gfx.getattr("RsNode")?.call1((game.getattr("scene_root")?, "body"))?;
        let vt = gfx.call_method1("box_vertices", ((c[0], c[1], c[2], 1.0),))?;
        gfx.getattr("RsDynamicMesh")?.call1((&e, "body"))?.call_method1("set", (vt.get_item(0)?, vt.get_item(1)?))?;
        e.call_method0("hide")?;
        Ok(BoxBody { game: game.clone().unbind(), player: player.clone().unbind(), e: e.unbind() })
    }
    fn update(&self, py: Python<'_>, origin: &B<'_>, shown: &B<'_>, dt: &B<'_>) -> R<()> {
        let _ = dt;
        let e = self.e.bind(py);
        if !shown.is_truthy()? {
            e.call_method0("hide")?;
            return Ok(());
        }
        let p = self.player.bind(py);
        let up0 = v3(&p.getattr("up")?)?;
        let n = norm(up0);
        let up = [up0[0] / n, up0[1] / n, up0[2] / n];
        let f0 = v3(&p.getattr("look_fwd")?)?;
        let d = dot(f0, up);
        let f1 = [f0[0] - up[0] * d, f0[1] - up[1] * d, f0[2] - up[2] * d];
        let nf = norm(f1);
        let f = if nf > 1e-6 { [f1[0] / nf, f1[1] / nf, f1[2] / nf] } else { cross(up, [1.0, 0.0, 0.0]) };
        let right = cross(up, f);
        let (h, r) = (f64of(&p.getattr("height")?)?, f64of(&p.getattr("radius")?)?);
        let pos = v3(&p.getattr("pos")?)?;
        let o = v3(origin)?;
        let center: [f64; 3] = std::array::from_fn(|k| pos[k] + up[k] * (h / 2.0 - r) - o[k]);
        let npm = np(py)?;
        let axes = npm.call_method1("stack", (vec![np3(py, right), np3(py, up), np3(py, f)],))?;
        imp(py, "main.client.gfx")?.call_method1("set_box", (e, np3(py, center), axes, np3(py, [r * 2.0, h, r * 1.4])))?;
        e.call_method0("show")?;
        Ok(())
    }
    fn destroy(&self, py: Python<'_>) -> R<()> {
        self.e.bind(py).call_method0("removeNode")?;
        Ok(())
    }
}

/// 機器の今の状態を、遊ぶ人が読める言葉で（内部の値の名前は出さない）
#[pyfunction]
pub fn game_device_status_text(py: Python<'_>, c: &B<'_>, i: &B<'_>, d: &B<'_>, dd: &B<'_>) -> R<String> {
    let st = get(d, "state", PyDict::new_bound(py))?;
    let t_ = get(dd, "type", py.None())?;
    let reg = c.getattr("reg")?;
    let fl = |o: B<'_>| -> R<f64> { f64of(&o) };
    let mut out = String::new();
    let is = |s: &str| t_.eq(s);
    let cap_of = || -> R<f64> { fl(get(&or_dict(get(dd, "params", py.None())?)?, "capacity", 0.0)?) };
    if is("thruster")? {
        if get(&st, "lit", py.None())?.is_truthy()? {
            let v = get(&st, "throttle", get(&st, "firing", 0.0)?)?;
            let v = if v.is_truthy()? { fl(v)? } else { 0.0 };
            out = format!("燃えている　出力 {}%", pff(py, v * 100.0, ".0f")?);
        } else {
            out = "止まっている".into();
        }
    } else if is("valve")? {
        let o = fl(get(&st, "open", 0.0)?)?;
        out = format!("開き具合 {}%{}", pff(py, o * 100.0, ".0f")?, if o <= 0.001 { "（閉）" } else { "" });
    } else if is("tank")? {
        let cap = cap_of()?;
        let cont = PyDict::new_bound(py);
        for kv in or_dict(get(&st, "contents", py.None())?)?.call_method0("items")?.iter()? {
            let kv = kv?;
            if kv.get_item(1)?.ge(0.05)? {
                cont.set_item(kv.get_item(0)?, kv.get_item(1)?)?;
            }
        }
        if cont.is_empty() {
            out = format!("空{}", if cap != 0.0 { format!("（{} まで入る）", pff(py, cap, ".0f")?) } else { String::new() });
        } else {
            out = format!("中身: {}{}", reg.call_method1("res_text", (cont, ", "))?.str()?, if cap != 0.0 { format!(" / {}", pff(py, cap, ".0f")?) } else { String::new() });
        }
    } else if is("battery")? {
        let cap = cap_of()?;
        let ch = fl(get(&st, "charge", 0.0)?)?;
        let tail = if cap != 0.0 { format!(" / {} MJ（{}%）", pff(py, cap / 1e6, ".1f")?, pff(py, ch / cap * 100.0, ".0f")?) } else { " MJ".into() };
        out = format!("電気 {}{}", pff(py, ch / 1e6, ".2f")?, tail);
    } else if is("computer")? {
        out = if get(&st, "running", py.None())?.is_truthy()? { "動いている".into() } else { "止まっている".into() };
        let b = get(&st, "blocks", py.None())?;
        let sc = get(&st, "script", py.None())?;
        if b.is_truthy()? || sc.is_truthy()? {
            let nm = if b.is_truthy()? { b } else { sc };
            out += &format!("　プログラム「{}」", nm.str()?).replace(".blocks.json", "").replace(".py", "");
        }
    } else if is("actuator")? {
        let act = imp(py, "main.actuator")?;
        let m = act.call_method1("motion", (d, &reg))?;
        let g = |k: &str, spec: &str| -> R<String> { pf(&get(&st, k, 0)?, spec) };
        let val = if m.eq("piston")? {
            format!("伸び {}", g("ext", ".2f")?)
        } else if !m.eq("both")? {
            format!("角度 {}°", g("angle", ".0f")?)
        } else {
            format!("角度 {}°  伸び {}", g("angle", ".0f")?, g("ext", ".2f")?)
        };
        let mode = if m.eq("hinge")? { act.call_method1("mode_name", (&st, d))? } else { act.getattr("MOTION_NAMES")?.get_item(&m)? };
        let att = if act.call_method1("is_attached", (c, i))?.is_truthy()? { "あり" } else { "なし" };
        let pt = act.call_method1("preset_text", (&st, act.call_method1("is_hinge_dev", (d, &reg))?))?;
        out = format!("{}  {}  結合 {}\n設定: {}", mode.str()?, val, att, pt.str()?);
    } else if is("processor")? {
        let store = PyDict::new_bound(py);
        for kv in or_dict(get(&st, "store", py.None())?)?.call_method0("items")?.iter()? {
            let kv = kv?;
            if kv.get_item(1)?.ge(0.5)? {
                store.set_item(kv.get_item(0)?, kv.get_item(1)?)?;
            }
        }
        out = if get(&st, "working", py.None())?.is_truthy()? {
            "動いている".into()
        } else if !get(&st, "enabled", true)?.is_truthy()? {
            "止めている".into()
        } else {
            "待機中".into()
        };
        if !store.is_empty() {
            out += &format!("　中: {}", reg.call_method1("res_text", (store, ", "))?.str()?);
        }
    } else if is("generator")? || is("antenna")? || is("air_vent")? || is("custom")? {
        out = if get(&st, "enabled", true)?.is_truthy()? { "ON".into() } else { "OFF".into() };
        if is("air_vent")? {
            let mode = get(&st, "mode", py.None())?;
            let txt = if mode.eq("fill")? { "与圧".to_string() } else if mode.eq("drain")? { "減圧".to_string() } else { get(&st, "mode", "")?.str()?.to_string() };
            out += &format!("　{}", txt);
        }
    }
    let err = get(&st, "error", py.None())?;
    if err.is_truthy()? {
        out = format!("{}{}⚠ {}", out, if out.is_empty() { "" } else { "\n" }, st.get_item("error")?.str()?);
    }
    Ok(out)
}

// ---------------------------------------------------------------- play.py
/// 狙っている物 h（world.raycast の結果）の、枠と説明。
/// 戻り値 {"highlight": (中心, 軸 (3,3), 大きさ (3,)) か None, "ghost": 置く場所の枠 か None, "text": 説明}。座標はワールド
#[pyfunction]
pub fn play_aim<'py>(py: Python<'py>, w: &B<'py>, h: &B<'py>, look_vectors: &B<'py>) -> R<Bound<'py, PyDict>> {
    let out = PyDict::new_bound(py);
    out.set_item("highlight", py.None())?;
    out.set_item("ghost", py.None())?;
    out.set_item("text", "")?;
    if !h.is_truthy()? {
        return Ok(out);
    }
    let npm = np(py)?;
    let kind = h.get_item("kind")?;
    if kind.eq("voxel")? {
        let (c, i) = (h.get_item("construct")?, h.get_item("index")?);
        let ii: i64 = i.extract()?;
        let f = c.call_method0("voxel_frames")?.get_item(&i)?;
        let ctr = vox_center(&c, ii)?;
        let size = c.getattr("size")?;
        let sz = f64of(&size)?;
        out.set_item("highlight", (&ctr, &f, npm.call_method1("full", (3, sz * 1.02))?))?;
        let face: i64 = h.get_item("face")?.extract()?;
        let (axis, sign) = (face.div_euclid(2), if face.rem_euclid(2) == 0 { 1.0 } else { -1.0 });
        let g = ctr.call_method1("__add__", (f.get_item(axis)?.call_method1("__mul__", (sign,))?.call_method1("__mul__", (&size,))?,))?;
        out.set_item("ghost", (g, &f, npm.call_method1("full", (3, sz * 0.98))?))?;
        let d = c.getattr("devs")?.get_item(&i)?;
        let reg = c.getattr("reg")?;
        let m = reg.call_method1("material", (c.getattr("mats")?.get_item(&i)?,))?;
        let mut info = format!("{}  [{}]", c.getattr("name")?.str()?, m.get_item("name")?.str()?);
        if c.getattr("anchor")?.is_truthy()? {
            info += "  ［固定中：機器以外のブロックで E を押すと切り離し］";
        }
        if d.is_truthy()? {
            let dd = or_dict(reg.call_method1("device", (d.get_item("def")?,))?)?;
            let t_ = get(&dd, "type", py.None())?;
            info += &format!("\n{}", d.get_item("name")?.str()?);
            if get(&dd, "directional", py.None())?.is_truthy()? {
                let dn = imp(py, "main.construct")?.getattr("DIR_NAMES")?.get_item(d.get_item("dir")?)?;
                info += &format!("  向き {}", dn.str()?);
            }
            let nm = d.get_item("name")?;
            let mut same = 0;
            for x in c.getattr("devs")?.iter()? {
                let x = x?;
                if x.is_truthy()? && x.get_item("name")?.eq(&nm)? {
                    same += 1;
                }
            }
            if same > 1 {
                info += &format!("  （同じ名前が {} 個：まとめて動く）", same);
            }
            let surv = !w.getattr("creative")?.is_truthy()?;
            let hint = if t_.eq("computer")? {
                "E 起動/停止・P プログラム"
            } else if t_.eq("thruster")? {
                "E 点火・止めるのはバルブで"
            } else if t_.eq("valve")? {
                "E 開閉・P 開き具合"
            } else if t_.eq("actuator")? {
                "E 実行・P 動作の設定"
            } else if t_.eq("tank")? {
                if surv { "E 手持ちの燃料を入れる" } else { "E 中身の切り替え" }
            } else if t_.eq("processor")? {
                if surv { "E 原料を入れる・できた物を受け取る" } else { "P 設定" }
            } else if t_.eq("seat")? {
                "E 座る"
            } else {
                "P 名前の設定"
            };
            info += &format!("\n［{}］", hint);
            let txt = game_device_status_text(py, &c, &i, &d, &dd)?;
            if !txt.is_empty() {
                info += &format!("\n{}", txt);
            }
        }
        let s = c.getattr("status")?;
        if s.is_truthy()? {
            let g = |k: &str| -> R<f64> { f64of(&get(&s, k, 0)?) };
            info += &format!("\n電力 {}/{} MJ  発電 {} kW  消費 {} kW", pff(py, g("stored")? / 1e6, ".2f")?, pff(py, g("capacity")? / 1e6, ".1f")?,
                pff(py, g("production")? / 1e3, ".1f")?, pff(py, g("demand")? / 1e3, ".1f")?);
        }
        let dv = imp(py, "main.devices")?;
        let cav = dv.call_method1("cavity_report", (&c,))?;
        if cav.is_truthy()? {
            let mut parts = vec![];
            for x in cav.iter()? {
                let x = x?;
                parts.push(format!("{}セル {}kPa", x.get_item(1)?.str()?, pf(&x.get_item(2)?, ".1f")?));
            }
            info += &format!("\n気密空洞: {}", parts.join(", "));
        }
        let tot = dv.call_method1("storage_total", (&c,))?;
        if tot.is_truthy()? {
            info += &format!("\n機械の中の物: {}", reg.call_method1("res_text", (tot, ", "))?.str()?);
        }
        out.set_item("text", info)?;
    } else if kind.eq("terrain")? || kind.eq("water")? {
        let size = f64of(&get(&w.getattr("settings")?, "voxel_size", 0.5)?)?;
        let g = h.get_item("point")?.call_method1("__add__", (h.get_item("normal")?.call_method1("__mul__", (size * 0.55,))?,))?;
        let (fwd, right) = (look_vectors.get_item(0)?, look_vectors.get_item(1)?);
        out.set_item("ghost", (g, npm.call_method1("stack", (vec![right, h.get_item("normal")?, fwd],))?, npm.call_method1("full", (3, size))?))?;
        let creative = w.getattr("creative")?.is_truthy()?;
        let pl = h.get_item("planet")?;
        let text = if kind.eq("water")? {
            format!("水面{}", if creative { "" } else { "（左クリックで水をくむ：いくらくんでも減りません）" })
        } else if creative {
            format!("{} の地面（左クリックで掘る）", pl.getattr("name")?.str()?)
        } else {
            let res = w.getattr("registry")?.call_method1("res_name", (pl.call_method1("resource_at", (h.get_item("point")?,))?,))?;
            format!("{} の地面：{}（左クリックで掘る）", pl.getattr("name")?.str()?, res.str()?)
        };
        out.set_item("text", text)?;
    }
    Ok(out)
}

/// アイテムバーの選んでいる枠の説明
#[pyfunction]
pub fn play_item_text(py: Python<'_>, w: &B<'_>, item: &B<'_>, sel: i64, dir_index: &B<'_>) -> R<String> {
    if item.is_none() {
        return Ok(format!("[{}] （空）", sel + 1));
    }
    let (kind, id_) = (item.get_item(0)?, item.get_item(1)?);
    let reg = w.getattr("registry")?;
    if kind.eq("material")? {
        let m = or_dict(reg.call_method1("material", (&id_,))?)?;
        return Ok(format!("[{}] 素材: {}", sel + 1, get(&m, "name", &id_)?.str()?));
    }
    let dd = or_dict(reg.call_method1("device", (&id_,))?)?;
    let extra = if get(&dd, "directional", py.None())?.is_truthy()? {
        format!("  向き {}", imp(py, "main.construct")?.getattr("DIR_NAMES")?.get_item(dir_index)?.str()?)
    } else {
        String::new()
    };
    Ok(format!("[{}] 機器: {}{}", sel + 1, get(&dd, "name", &id_)?.str()?, extra))
}

/// 三人称のカメラが壁・地面・建物の向こうに入らないように、手前で止める（乗っている乗り物は通り抜ける）
#[pyfunction]
pub fn play_camera_clip<'py>(py: Python<'py>, w: &B<'py>, eye: &B<'py>, offset: &B<'py>) -> R<B<'py>> {
    let off = v3(offset)?;
    let n = norm(off);
    if n < 1e-6 {
        return Ok(eye.clone());
    }
    let d = [off[0] / n, off[1] / n, off[2] / n];
    let e = v3(eye)?;
    let p = w.getattr("player")?;
    let seated = p.getattr("seated")?;
    let own = if seated.is_truthy()? { seated.get_item(0)? } else { none(py) };
    let full = || -> R<B<'py>> { eye.call_method1("__add__", (offset,)) };
    let (mut start, mut left) = (e, n);
    for _ in 0..4 {
        let h = match w.call_method1("raycast", (np3(py, start), np3(py, d), left)) {
            Ok(h) => h,
            Err(err) if is_exc(py, &err) => return full(),
            Err(err) => return Err(err),
        };
        if h.is_none() {
            return full();
        }
        if get(&h, "kind", py.None())?.eq("voxel")? && !own.is_none() && h.get_item("construct")?.getattr("id")?.eq(&own)? {
            let step = f64of(&h.get_item("t")?)? + 0.05; // 自分の乗り物：その先から調べ直す
            start = [start[0] + d[0] * step, start[1] + d[1] * step, start[2] + d[2] * step];
            left -= step;
            if left <= 0.0 {
                return full();
            }
            continue;
        }
        let t = f64of(&h.get_item("t")?)?;
        let q = [start[0] + d[0] * t - e[0], start[1] + d[1] * t - e[1], start[2] + d[2] * t - e[2]];
        let k = (norm(q) - 0.4).max(0.3);
        return Ok(np3(py, [e[0] + d[0] * k, e[1] + d[1] * k, e[2] + d[2] * k]));
    }
    full()
}

/// 座っている席が遠隔操作なら、操っている構築物を後ろから映す：(重心, 距離) か None。MOD の seat_camera() が決める
#[pyfunction]
pub fn play_seat_follow<'py>(py: Python<'py>, w: &B<'py>, addons: &B<'py>) -> R<B<'py>> {
    let p = w.getattr("player")?;
    let seated = p.getattr("seated")?;
    if !seated.is_truthy()? {
        return Ok(none(py));
    }
    let c = w.getattr("constructs")?.call_method1("get", (seated.get_item(0)?,))?;
    if c.is_none() {
        return Ok(none(py));
    }
    for a in addons.iter()? {
        let a = a?;
        let f = a.getattr("seat_camera").unwrap_or_else(|_| none(py));
        let t = if f.is_truthy()? { f.call1((&c,))? } else { none(py) };
        if !t.is_none() && t.getattr("n_vox")?.is_truthy()? {
            let pos = t.getattr("pos")?;
            let ext = f64of(&np(py)?.getattr("linalg")?.call_method1("norm", (pos.call_method1("max", (0,))?.call_method1("__sub__", (pos.call_method1("min", (0,))?,))?,))?)?;
            return Ok(PyTuple::new_bound(py, [t.call_method0("center_of_mass")?, tb(py, (ext * 2.0).clamp(4.0, 30.0))]).into_any());
        }
    }
    Ok(none(py))
}

/// 近くにある、触れるもの（使う順）：[{"label", "hit", "kind": "e", "settings": 設定を開けるか, "icon", "dist"}]
/// 機器 1 つにボタン 1 つ（設定はボタンの歯車・P）。配管・バッテリーは出さない。MOD の nearby_priority で並べ替える
#[pyfunction]
#[pyo3(signature = (w, addons, radius=2.6))]
pub fn play_nearby_targets<'py>(py: Python<'py>, w: &B<'py>, addons: &B<'py>, radius: f64) -> R<Bound<'py, PyList>> {
    let p = w.getattr("player")?;
    if p.getattr("seated")?.is_truthy()? {
        return Ok(PyList::empty_bound(py));
    }
    let eye_o = p.call_method0("eye")?;
    let nplinalg = np(py)?.getattr("linalg")?;
    let mut out: Vec<B<'_>> = vec![];
    let surv = !w.getattr("creative")?.is_truthy()?;
    let quiet = imp(py, "main.client.play")?.getattr("QUIET_DEVICES")?;
    let name_of = |t: &B<'_>| -> R<&'static str> {
        Ok(if t.eq("seat")? {
            "座る"
        } else if t.eq("computer")? {
            "起動・停止"
        } else if t.eq("tank")? {
            if surv { "燃料を入れる" } else { "中身を切り替える" }
        } else if t.eq("actuator")? {
            "動かす"
        } else if t.eq("processor")? {
            if surv { "原料を入れる・受け取る" } else { "ON/OFF" }
        } else if t.eq("thruster")? {
            "点火"
        } else if t.eq("valve")? {
            "開閉"
        } else if t.eq("crank")? {
            "回す"
        } else {
            "操作"
        })
    };
    for c in w.call_method1("constructs_near", (&eye_o, radius))?.iter()? {
        let c = c?;
        if !c.getattr("n_vox")?.is_truthy()? || !c.getattr("has_devices")?.is_truthy()? {
            continue;
        }
        let reg = c.getattr("reg")?;
        for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
            let d = d?;
            if !d.is_truthy()? {
                continue;
            }
            let dd = or_dict(reg.call_method1("device", (d.get_item("def")?,))?)?;
            let t_ = get(&dd, "type", py.None())?;
            if quiet.contains(&t_)? {
                continue;
            }
            let ctr = vox_center(&c, i as i64)?;
            let dist = f64of(&nplinalg.call_method1("norm", (ctr.call_method1("__sub__", (&eye_o,))?,))?)?; // numpy と同じ値に
            if dist > radius {
                continue;
            }
            let hit = PyDict::new_bound(py);
            hit.set_item("kind", "voxel")?;
            hit.set_item("t", dist)?;
            hit.set_item("construct", &c)?;
            hit.set_item("index", i)?;
            hit.set_item("face", 2)?;
            hit.set_item("point", &ctr)?;
            let t = PyDict::new_bound(py);
            t.set_item("label", format!("{}（{}）", name_of(&t_)?, d.get_item("name")?.str()?))?;
            t.set_item("hit", hit)?;
            t.set_item("kind", "e")?;
            t.set_item("dist", dist)?;
            t.set_item("icon", "interact")?;
            t.set_item("settings", !t_.eq("seat")?)?;
            out.push(t.into_any());
        }
    }
    let ads: Vec<B<'_>> = addons.iter()?.collect::<R<_>>()?;
    for a in &ads {
        match a.call_method1("nearby", (p.getattr("pos")?, radius)) {
            Ok(r) => {
                if r.is_truthy()? {
                    for x in r.iter()? {
                        out.push(x?);
                    }
                }
            }
            Err(e) if is_exc(py, &e) => {}
            Err(e) => return Err(e),
        }
    }
    let mut keyed: Vec<((f64, f64), B<'_>)> = vec![];
    for t in out {
        let mut pr = 0.0;
        for a in &ads {
            let f = a.getattr("nearby_priority").unwrap_or_else(|_| none(py));
            if !f.is_none() {
                match f.call1((&t,)).and_then(|v| if v.is_truthy()? { f64of(&v) } else { Ok(0.0) }) {
                    Ok(v) => pr += v,
                    Err(e) if is_exc(py, &e) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        let dist = f64of(&t.get_item("dist")?)?;
        keyed.push(((pr, dist), t));
    }
    keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(PyList::new_bound(py, keyed.into_iter().take(4).map(|x| x.1)))
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<BoxBody>()?;
    for f in [wrap_pyfunction_bound!(game_find_font, m)?, wrap_pyfunction_bound!(game_device_status_text, m)?, wrap_pyfunction_bound!(play_aim, m)?,
              wrap_pyfunction_bound!(play_item_text, m)?, wrap_pyfunction_bound!(play_camera_clip, m)?, wrap_pyfunction_bound!(play_seat_follow, m)?,
              wrap_pyfunction_bound!(play_nearby_targets, m)?] {
        m.add_function(f)?;
    }
    let _ = PyString::new_bound;
    Ok(())
}
