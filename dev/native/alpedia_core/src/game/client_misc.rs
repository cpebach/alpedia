//! 画面側の小さな部品：MOD の画面側の口（addon_hooks.ClientAddon）、部品を中身ごと消す（ui_destroy）、
//! サバイバルの目標（survival_ui）、アイテムバーとインベントリの中身（inventory）。

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::imp;
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

fn tb<T: IntoPy<PyObject>>(py: Python<'_>, x: T) -> B<'_> {
    x.into_py(py).into_bound(py)
}
fn get<'py>(d: &B<'py>, k: &str, dflt: impl IntoPy<PyObject>) -> R<B<'py>> {
    let py = d.py();
    d.call_method1("get", (k, dflt.into_py(py)))
}
fn pf(v: &B<'_>, spec: &str) -> R<String> {
    v.py().import_bound("builtins")?.getattr("format")?.call1((v, spec))?.extract()
}
fn pff(py: Python<'_>, v: f64, spec: &str) -> R<String> {
    pf(&tb(py, v), spec)
}

// ---------------------------------------------------------------- addon_hooks
/// 画面側の拡張の口。ワールド側の拡張（hooks.Addon）と組で使う。
///
/// ゲーム画面は拡張の中身を知らず、次の口だけを呼ぶ。
/// 拡張はワールドの中身を直接触らず、自分のワールド側（world.addon(名前)）の問い合わせ関数を使う。
#[pyclass(module = "main.client.addon_hooks", subclass, dict)]
pub struct ClientAddon {}

#[pymethods]
impl ClientAddon {
    #[classattr]
    fn name() -> &'static str {
        "addon"
    }
    /// 操作説明に足す行
    #[classattr]
    fn help_text() -> &'static str {
        ""
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        ClientAddon {}
    }
    fn __init__(slf: &Bound<'_, Self>, game: &B<'_>) -> R<()> {
        slf.as_any().setattr("game", game)?;
        slf.as_any().setattr("world", game.getattr("world")?)?;
        Ok(())
    }
    /// キーが押された。使ったら True
    fn on_key(&self, key: &B<'_>) -> bool {
        let _ = key;
        false
    }
    /// E で hit（視線の先）を使う。使ったら True（コアの操作はしない）
    fn interact(&self, hit: &B<'_>) -> bool {
        let _ = hit;
        false
    }
    /// 視線の先の説明。(文, ブロックを置ける面なら True) か None（この拡張のものでない）
    fn aim(&self, py: Python<'_>, hit: &B<'_>) -> PyObject {
        let _ = hit;
        py.None()
    }
    /// カメラモードで選べるモーション：{表示名: pose(t)}（pose は秒 t を受け取り、骨の向きの辞書を返す関数）。
    /// 辞書の書き方は mods/README.md の「モーション」。体の見た目の MOD（VRM の見た目など）が使う。
    fn emotes<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        PyDict::new_bound(py)
    }
    /// 今流したい BGM：(曲の ID, 始める秒) か None（決めない）。曲の ID は mods/bgm/bgm.json の tracks。
    /// いちばん先に None 以外を返した拡張の曲が流れる。毎フレーム呼ばれる。
    fn bgm(&self, py: Python<'_>) -> PyObject {
        py.None()
    }
    /// カメラモードで写真を保存した（path：保存した画像）
    fn on_photo(&self, path: &B<'_>) {
        let _ = path;
    }
    /// カメラモードに入った（on=True）・終わった（False）
    fn on_camera(&self, on: &B<'_>) {
        let _ = on;
    }
    /// プレイヤーの近く（pos から radius m 以内）にある、この拡張の触れるもの（お店・NPC など）。
    /// 画面右にボタンが出て、押す（または E）と interact(hit) が呼ばれる。
    /// [{"label": 表示, "hit": interact に渡すもの, "kind": "e", "dist": 距離, "icon": "interact"}]
    fn nearby<'py>(&self, py: Python<'py>, pos: &B<'py>, radius: &B<'py>) -> Bound<'py, PyList> {
        let _ = (pos, radius);
        PyList::empty_bound(py)
    }
    /// プレイヤー（自分・マルチプレイのほかの人）の体の見た目を引き受けるなら、そのオブジェクトを返す。
    /// オブジェクトは update(origin, shown, dt) を毎フレーム受け取り（shown: 見せるか）、destroy() で消える。
    /// player の pos・up・vel・look_fwd（見ている向き）・pitch・seated などを読んで動かす。
    /// 見た目を変えたら作り直す。プレイ中にファイルを読まないように、モデルは拡張を作るときに読んでおく。
    fn player_body(&self, py: Python<'_>, player: &B<'_>) -> PyObject {
        let _ = player;
        py.None()
    }
    /// 近くのボタンの並び順の補正（小さいほど上）。今の目標の機器を上にするときなどに負の数を返す
    fn nearby_priority(&self, target: &B<'_>) -> f64 {
        let _ = target;
        0.0
    }
    /// 座っている構築物（操縦台など）が遠隔操作の台なら、カメラで追う構築物を返す（ドローンなど）。違えば None
    fn seat_camera(&self, py: Python<'_>, construct: &B<'_>) -> PyObject {
        let _ = construct;
        py.None()
    }
    /// 毎フレームの描画。origin はカメラの位置（浮動原点）
    fn update(&self, origin: &B<'_>, far: &B<'_>) {
        let _ = (origin, far);
    }
    /// ゲーム画面を閉じる
    fn destroy(&self) {}
}

// ---------------------------------------------------------------- ui_destroy
/// 画面部品（ursina の Entity）を中身ごと消す。
///
/// ursina の destroy() は渡した部品だけを消し、子の部品（ボタン・文字など）は消さない。
/// 消されなかった子は画面からは見えなくなるが部品一覧やマウスの当たり判定の一覧に残り続け、
/// 毎フレームの処理が少しずつ重くなっていく。そこで子から順にまとめて消す。
#[pyfunction]
#[pyo3(signature = (entity, *args, **kwargs))]
pub fn ui_destroy(py: Python<'_>, entity: &B<'_>, args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> R<()> {
    let d = imp(py, "ursina")?.getattr("destroy")?;
    if entity.is_none() {
        return Ok(());
    }
    let ch = entity.getattr("children").ok().filter(|c| c.is_truthy().unwrap_or(false));
    if let Some(ch) = ch {
        let l: Vec<B<'_>> = ch.iter()?.collect::<R<_>>()?;
        for c in l {
            ui_destroy(py, &c, &PyTuple::empty_bound(py), None)?;
        }
    }
    let mut v: Vec<PyObject> = vec![entity.clone().unbind()];
    v.extend(args.iter().map(|x| x.unbind()));
    d.call(PyTuple::new_bound(py, v), kwargs)?;
    Ok(())
}

// ---------------------------------------------------------------- survival_ui
/// サバイバルの画面：左上に目標（上から順に）
#[pyclass(module = "main.client.survival_ui", extends = ClientAddon, subclass)]
pub struct SurvivalClient {}

#[pymethods]
impl SurvivalClient {
    #[classattr]
    fn name() -> &'static str {
        "survival"
    }
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> (Self, ClientAddon) {
        (SurvivalClient {}, ClientAddon {})
    }
    fn __init__(slf: &Bound<'_, Self>, game: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("game", game)?;
        s.setattr("world", game.getattr("world")?)?;
        s.setattr("pack", game.getattr("world")?.call_method1("addon", ("survival",))?)?;
        let u = game.getattr("U")?;
        let (entity, text, color, window) = (u.get_item("Entity")?, u.get_item("Text")?, u.get_item("color")?, u.get_item("window")?);
        let kw = |pairs: Vec<(&str, PyObject)>| -> R<Bound<'_, PyDict>> {
            let d = PyDict::new_bound(py);
            for (k, v) in pairs {
                d.set_item(k, v)?;
            }
            Ok(d)
        };
        let pos = window.getattr("top_left")?.call_method1("__add__", ((0.012, -0.1),))?;
        let root = entity.call((), Some(&kw(vec![("parent", game.getattr("ui_root")?.unbind()), ("position", pos.unbind())])?))?;
        s.setattr("root", &root)?;
        s.setattr("bg", entity.call((), Some(&kw(vec![("parent", root.clone().unbind()), ("model", "quad".into_py(py)), ("origin", (-0.5, 0.5).into_py(py)),
            ("scale", (0.6, 0.1).into_py(py)), ("color", color.call_method1("rgba", (0.02, 0.05, 0.1, 0.62))?.unbind()), ("z", 0.02.into_py(py))])?))?)?;
        s.setattr("title", text.call((), Some(&kw(vec![("parent", root.clone().unbind()), ("text", "サバイバルの目標".into_py(py)), ("position", (0.012, -0.012).into_py(py)),
            ("origin", (-0.5, 0.5).into_py(py)), ("scale", 0.75.into_py(py)), ("color", color.call_method1("rgb", (0.6, 1.0, 0.6))?.unbind())])?))?)?;
        s.setattr("text", text.call((), Some(&kw(vec![("parent", root.clone().unbind()), ("text", "".into_py(py)), ("position", (0.012, -0.043).into_py(py)),
            ("origin", (-0.5, 0.5).into_py(py)), ("scale", 0.66.into_py(py)), ("line_height", 1.15.into_py(py))])?))?)?;
        s.setattr("_t", 0.0)?;
        Ok(())
    }
    fn update(slf: &Bound<'_, Self>, origin: &B<'_>, far: &B<'_>) -> R<()> {
        let _ = (origin, far);
        let py = slf.py();
        let s = slf.as_any();
        let now: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
        let pack = s.getattr("pack")?;
        if pack.is_none() || now - f64of(&s.getattr("_t")?)? < 0.5 {
            return Ok(());
        }
        s.setattr("_t", now)?;
        let goals: Vec<B<'_>> = pack.call_method1("goals_state", (s.getattr("world")?,))?.iter()?.collect::<R<_>>()?;
        let mut todo: Vec<B<'_>> = vec![];
        for g in &goals {
            if !g.get_item(2)?.is_truthy()? {
                todo.push(g.get_item(1)?);
            }
        }
        let done_n = goals.len() - todo.len();
        let mut lines: Vec<String> = if todo.is_empty() {
            vec!["全部達成！ ここからは自由に作ろう".into()]
        } else {
            todo.iter().take(2).map(|t| Ok(format!("□ {}", t.str()?))).collect::<R<_>>()? // 次の2つだけ（画面をふさがない）
        };
        lines.push(format!("（{}/{} 達成）", done_n, goals.len()));
        let txt = lines.join("\n");
        let t = s.getattr("text")?;
        if !t.getattr("text")?.eq(&txt)? {
            t.setattr("text", &txt)?;
        }
        s.getattr("bg")?.setattr("scale", (0.6, 0.06 + lines.len() as f64 * 0.024))?;
        Ok(())
    }
    fn destroy(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        ui_destroy(py, &slf.as_any().getattr("root")?, &PyTuple::empty_bound(py), None)
    }
}

// ---------------------------------------------------------------- inventory
fn inv(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.inventory")?.into_any())
}

#[pyfunction]
pub fn inv_valid_item(reg: &B<'_>, item: &B<'_>) -> R<bool> {
    if !item.is_truthy()? {
        return Ok(false);
    }
    let (kind, id_) = (item.get_item(0)?, item.get_item(1)?);
    if item.len()? != 2 {
        return Err(pyo3::exceptions::PyValueError::new_err("too many values to unpack (expected 2)"));
    }
    Ok(if kind.eq("material")? { reg.call_method1("material", (id_,))? } else { reg.call_method1("device", (id_,))? }.is_truthy()?)
}

/// アイテムバーを9枠にそろえ、無くなったアイテム（MOD を外した等）を空にする
#[pyfunction]
pub fn inv_ensure_hotbar<'py>(py: Python<'py>, player: &B<'py>, reg: &B<'py>) -> R<Bound<'py, PyList>> {
    let m = inv(py)?;
    let slots: usize = m.getattr("SLOTS")?.extract()?;
    let tup = py.import_bound("builtins")?.getattr("tuple")?;
    let cur = player.getattr("hotbar")?;
    let hb = if cur.is_truthy()? {
        PyList::new_bound(py, cur.iter()?.collect::<R<Vec<_>>>()?)
    } else {
        let pal = reg.call_method0("palette")?;
        let hb = PyList::empty_bound(py);
        for it in m.getattr("DEFAULT_HOTBAR")?.iter()? {
            let it = it?;
            if inv_valid_item(reg, &it)? {
                hb.append(it)?;
            }
        }
        for it in pal.iter()? {
            if hb.len() >= slots {
                break;
            }
            let it = it?;
            if !hb.contains(&it)? {
                hb.append(it)?;
            }
        }
        hb
    };
    let out = PyList::empty_bound(py);
    for x in hb.iter() {
        if x.is_truthy()? {
            let t = tup.call1((&x,))?;
            if inv_valid_item(reg, &t)? {
                out.append(t)?;
                continue;
            }
        }
        out.append(py.None())?;
    }
    let out = PyList::new_bound(py, out.iter().take(slots).collect::<Vec<_>>());
    while out.len() < slots {
        out.append(py.None())?;
    }
    player.setattr("hotbar", &out)?;
    Ok(out)
}

#[pyfunction]
pub fn inv_item_details<'py>(py: Python<'py>, reg: &B<'py>, item: &B<'py>) -> R<Bound<'py, PyList>> {
    let (kind, id_) = (item.get_item(0)?, item.get_item(1)?);
    let lines = PyList::empty_bound(py);
    if kind.eq("material")? {
        let m = reg.call_method1("material", (&id_,))?;
        lines.append(format!("素材: {}", m.get_item("name")?.str()?))?;
        lines.append(format!("密度 {} kg/m³　硬さ {}　ちぎれにくさ {}", pf(&m.get_item("density")?, ".0f")?, pf(&m.get_item("stiffness")?, ".2f")?,
            pf(&m.get_item("break_distance")?, ".2f")?))?;
        let mut extra: Vec<&str> = vec![];
        if get(&m, "plasticity", 0)?.ge(0.5)? {
            extra.push("曲がったまま戻りにくい");
        }
        if get(&m, "rebond", py.None())?.is_truthy()? {
            extra.push("離れても近づくと再びくっつく");
        }
        if get(&m, "density", 0)?.lt(1000)? {
            extra.push("水に浮く");
        }
        if !extra.is_empty() {
            lines.append(format!("・{}", extra.join("・")))?;
        }
    } else {
        let d = reg.call_method1("device", (&id_,))?;
        let mark = get(&d, "mark", py.None())?;
        let tkey = if mark.is_truthy()? { mark } else { d.get_item("type")? };
        let tn = inv(py)?.getattr("TYPE_NAMES")?.call_method1("get", (tkey, d.get_item("type")?))?;
        lines.append(format!("機器: {}（{}）", d.get_item("name")?.str()?, tn.str()?))?;
        let power = get(&d, "power", py.None())?;
        let pw = if power.eq("none")? {
            "電力なし".to_string()
        } else if power.eq("manual")? {
            "手動".to_string()
        } else if power.eq("electric")? {
            format!("電力 {} W", pf(&get(&d, "power_use", 0)?, ".0f")?)
        } else {
            String::new()
        };
        let mat_id = get(&d, "material", py.None())?;
        let mm = reg.call_method1("material", (&mat_id,))?;
        let mm = if mm.is_truthy()? { mm } else { PyDict::new_bound(py).into_any() };
        let mat = get(&mm, "name", mat_id)?;
        let mut info = vec![pw, format!("素材 {}", mat.str()?)];
        if get(&d, "directional", py.None())?.is_truthy()? {
            info.push("向きあり（R で変更）".into());
        }
        if get(&d, "pipe_port", py.None())?.is_truthy()? {
            info.push("配管につながる".into());
        }
        lines.append(info.into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("　"))?;
        let p = get(&d, "params", PyDict::new_bound(py))?;
        let ty = d.get_item("type")?;
        if ty.eq("generator")? {
            let fr = f64of(&get(&p, "fuel_rate", 0)?)?;
            let tail = if fr > 0.0 {
                format!("（{} {} kg/秒）", reg.call_method1("res_name", (get(&p, "fuel", "fuel")?,))?.str()?, pff(py, fr, "g")?)
            } else {
                "（燃料いらず）".into()
            };
            lines.append(format!("発電 {} kW{}", pff(py, f64of(&get(&p, "output", 0)?)? / 1000.0, ".1f")?, tail))?;
        }
        if ty.eq("processor")? {
            for r in get(&p, "recipes", PyList::empty_bound(py))?.iter()? {
                let r = r?;
                let pw = f64of(&get(&r, "power", get(&d, "power_use", 0)?)?)? / 1000.0;
                lines.append(format!("・{} → {}（{}秒・{} kW）", reg.call_method1("res_text", (get(&r, "in", PyDict::new_bound(py))?,))?.str()?,
                    reg.call_method1("res_text", (get(&r, "out", PyDict::new_bound(py))?,))?.str()?, pff(py, f64of(&get(&r, "time", 1)?)?, "g")?,
                    pff(py, pw, "g")?))?;
            }
        }
        let src = get(&d, "_source", py.None())?;
        if src.is_truthy()? && !src.eq("base")? {
            lines.append(format!("MOD: {}", d.get_item("_source")?.str()?))?;
        }
    }
    Ok(lines)
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<ClientAddon>()?;
    m.add_class::<SurvivalClient>()?;
    for f in [wrap_pyfunction_bound!(ui_destroy, m)?, wrap_pyfunction_bound!(inv_valid_item, m)?, wrap_pyfunction_bound!(inv_ensure_hotbar, m)?,
              wrap_pyfunction_bound!(inv_item_details, m)?] {
        m.add_function(f)?;
    }
    Ok(())
}
