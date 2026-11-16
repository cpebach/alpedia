//! 遊ぶ画面（Rust の alpedia_view で描く）：ワールドの描画・操作・HUD・MOD の画面側（main/client_rs/viewer.py）。
//!
//!   python run.py --world 名前
//!
//! 描く物：構築物（ブロック・遠景の箱・ライト・機器の記号・看板）、地表と掘った跡、惑星、景色の箱、空の色、
//! 人物（VRM・色の箱）。画面の上の部品（プログラムの画面・MOD の画面・メニューなど）は ursina のまね（ursina.rs）で動き、
//! 一時停止メニュー・インベントリ・機器の設定は ui.rs・screens*.rs。
//!
//! Viewer・GameAdapter の値は Python の属性（MOD やほかの部品が読み書きする）。処理は Rust。

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyString, PyTuple};

use super::imp;
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

pub const SKY_DAY: [f64; 3] = [0.45, 0.65, 0.9];
pub const SKY_SPACE: [f64; 3] = [0.012, 0.016, 0.04];

// ---------------------------------------------------------------- 小さな道具
fn ga<'py>(o: &B<'py>, k: &str) -> R<B<'py>> {
    o.getattr(k)
}
fn gd<'py>(o: &B<'py>, k: &str, d: B<'py>) -> B<'py> {
    o.getattr(k).unwrap_or(d)
}
fn none(py: Python<'_>) -> B<'_> {
    py.None().into_bound(py)
}
fn tb<T: IntoPy<PyObject>>(py: Python<'_>, x: T) -> B<'_> {
    x.into_py(py).into_bound(py)
}
fn get<'py>(d: &B<'py>, k: &str, dflt: impl IntoPy<PyObject>) -> R<B<'py>> {
    let py = d.py();
    d.call_method1("get", (k, dflt.into_py(py)))
}
fn lg(py: Python<'_>) -> R<B<'_>> {
    imp(py, "main.log")?.call_method1("get", ("client_rs",))
}
fn is_exc(py: Python<'_>, e: &PyErr) -> bool {
    e.is_instance_of::<PyException>(py)
}
fn log_exc(py: Python<'_>, e: &PyErr, msg: &str, args: Vec<PyObject>) -> R<()> {
    let mut v: Vec<PyObject> = vec![msg.into_py(py)];
    v.extend(args);
    let k = PyDict::new_bound(py);
    k.set_item("exc_info", e.value_bound(py))?;
    lg(py)?.call_method("exception", PyTuple::new_bound(py, v), Some(&k))?;
    Ok(())
}
/// Python の例外（Exception）なら記録して続ける。それ以外（KeyboardInterrupt など）はそのまま
fn swallow<T>(py: Python<'_>, r: R<T>, f: impl FnOnce(&PyErr) -> R<()>) -> R<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if is_exc(py, &e) => {
            f(&e)?;
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
fn pf(py: Python<'_>, v: f64, spec: &str) -> R<String> {
    py.import_bound("builtins")?.getattr("format")?.call1((v, spec))?.extract()
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
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn gfx(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.gfx")?.into_any())
}
fn gview(py: Python<'_>) -> R<B<'_>> {
    gfx(py)?.getattr("VIEW")
}
fn render_mod(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.render")?.into_any())
}
fn rs_text<'py>(py: Python<'py>, parent: B<'py>, overlay: bool, bg: Option<(f64, f64, f64, f64)>, bin: Option<i64>) -> R<B<'py>> {
    let k = PyDict::new_bound(py);
    k.set_item("overlay", overlay)?;
    if let Some(b) = bg {
        k.set_item("bg", b)?;
    }
    if let Some(b) = bin {
        k.set_item("bin", b)?;
    }
    gfx(py)?.getattr("RsText")?.call((parent,), Some(&k))
}
fn shim(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client_rs.ursina_shim")?.into_any())
}
const BG: (f64, f64, f64, f64) = (0.02, 0.03, 0.05, 0.55);

fn keymap(k: &str) -> &str {
    match k {
        "control" => "left control",
        "shift" => "left shift",
        "alt" => "left alt",
        x => x,
    }
}

/// MOD の口を呼ぶ関数（Rust の中の lambda の代わり）
#[pyclass(module = "alpedia_core")]
pub struct AddonCall {
    name: String,
    args: Py<PyTuple>,
}
// 場面の用意（_render）の内訳（開発用）：前置き・構築物の見た目・遠くの箱・明かり・地形・街などの飾り
const SCENE_PARTS: [&str; 6] = ["s_pre", "s_views", "s_lod", "s_light", "s_terr", "s_deco"];
static SCENE_PROF: std::sync::Mutex<[f64; 6]> = std::sync::Mutex::new([0.0; 6]);
fn scene_prof(i: usize, t: &mut std::time::Instant) {
    let now = std::time::Instant::now();
    if let Ok(mut p) = SCENE_PROF.lock() {
        p[i] += (now - *t).as_secs_f64();
    }
    *t = now;
}

#[pymethods]
impl AddonCall {
    fn __call__(&self, py: Python<'_>, a: &B<'_>) -> R<PyObject> {
        Ok(a.call_method1(self.name.as_str(), self.args.bind(py))?.unbind())
    }
    #[getter]
    fn __name__(&self) -> &str {
        "<lambda>"
    }
}
fn acall<'py>(py: Python<'py>, name: &str, args: Vec<PyObject>) -> R<B<'py>> {
    Ok(Py::new(py, AddonCall { name: name.into(), args: PyTuple::new_bound(py, args).unbind() })?.into_bound(py).into_any())
}

// ---------------------------------------------------------------- Viewer
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct Viewer {}

#[pymethods]
impl Viewer {
    #[classattr]
    #[allow(non_snake_case)]
    fn KEYMAP(py: Python<'_>) -> R<Py<PyDict>> {
        let d = PyDict::new_bound(py);
        d.set_item("control", "left control")?;
        d.set_item("shift", "left shift")?;
        d.set_item("alt", "left alt")?;
        Ok(d.unbind())
    }

    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        Viewer {}
    }
    fn __init__(slf: &Bound<'_, Self>, world: &B<'_>, cfg: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("w", world)?;
        s.setattr("cfg", cfg)?;
        let g = cfg.get_item("graphics")?;
        let win = get(cfg, "window", PyDict::new_bound(py))?;
        shim(py)?.call_method0("install")?; // 今までの画面の部品（ursina）を、この画面で動かす
        let gx = gfx(py)?;
        let view = if gx.call_method0("active")?.is_truthy()? {
            gx.getattr("VIEW")?
        } else {
            gx.call_method1("start", (get(&win, "width", 1280)?, get(&win, "height", 720)?, "Alpedia", get(&win, "vsync", true)?))?
        };
        s.setattr("view", &view)?;
        let paths = imp(py, "main.paths")?;
        let rm = render_mod(py)?;
        let style = PyDict::new_bound(py);
        style.set_item("edges", get(&g, "block_edges", true)?.is_truthy()?)?;
        style.set_item("ao", get(&g, "block_ao", true)?.is_truthy()?)?;
        rm.setattr("STYLE", style)?;
        let tex_dir = paths.getattr("ROOT")?.call_method1("__truediv__", ("sauce",))?.call_method1("__truediv__", ("textures",))?;
        rm.setattr("EDGE_TEXTURE", tex_dir.call_method1("__truediv__", ("px_edge.png",))?.str()?)?;
        rm.setattr("DEVICE_TEXTURE", tex_dir.call_method1("__truediv__", ("px_device.png",))?.str()?)?;
        let node = gx.getattr("RsNode")?;
        let root = node.call1((py.None(), "scene"))?;
        s.setattr("root", &root)?;
        s.setattr("views", PyDict::new_bound(py))?;
        let pvs = PyList::empty_bound(py);
        for p in world.getattr("planets")?.iter()? {
            pvs.append(rm.getattr("PlanetView")?.call1((&root, py.None(), p?))?)?;
        }
        s.setattr("planet_views", pvs)?;
        let int = py.import_bound("builtins")?.getattr("int")?;
        s.setattr("patch", rm.getattr("TerrainPatch")?.call1((&root, py.None(), int.call1((get(&g, "terrain_patch_cells", 48)?,))?,
            f64of(&get(&g, "terrain_patch_step", 1.0)?)?))?)?;
        s.setattr("carve", rm.getattr("CarveView")?.call1((&root, py.None()))?)?;
        s.setattr("lod", rm.getattr("LodLayer")?.call1((&root,))?)?;
        s.setattr("lights", rm.getattr("LightLayer")?.call1((&root,))?)?;
        s.setattr("static_root", node.call1((&root, "static"))?)?;
        s.setattr("anchor", py.None())?;
        s.setattr("anchor_ver", 0)?;
        s.setattr("loaded", PySet::empty_bound(py)?)?;
        // 画面の上：左上の状態・左下の知らせ・照準
        gx.call_method1("font", (get(&g, "font", py.None())?,))?;
        s.setattr("hud", rs_text(py, none(py), true, Some(BG), Some(10))?)?;
        s.setattr("msg", rs_text(py, none(py), true, Some(BG), Some(10))?)?;
        let cross = gx.getattr("RsDynamicMesh")?.call1((node.call1((py.None(), "cross"))?, "cross"))?;
        view.call_method1("mesh_overlay", (cross.getattr("mid")?, true))?;
        gx.call_method1("quad", (&cross, -0.003, 0.003, 0.003, -0.003, (1, 1, 1, 0.9)))?;
        s.setattr("cross", cross)?;
        // 手の操作：狙っている物の枠・置く場所の枠・説明・アイテムバー
        s.setattr("highlight", gx.call_method1("wire_box", (&root, (1.0, 0.92, 0.2, 1.0), 0.025))?)?;
        s.setattr("ghost", gx.call_method1("wire_box", (&root, (0.4, 1.0, 0.4, 1.0), 0.02))?)?;
        s.setattr("aim_text", rs_text(py, none(py), true, Some(BG), Some(10))?)?;
        s.setattr("item_text", rs_text(py, none(py), true, Some(BG), Some(10))?)?;
        s.setattr("hit", py.None())?;
        s.setattr("sel", 0)?;
        s.setattr("dir_index", 0)?;
        s.setattr("bodies", PyDict::new_bound(py))?;
        s.setattr("labels", PyDict::new_bound(py))?;
        s.setattr("third_person", false)?;
        s.setattr("_was_seated", false)?;
        s.setattr("debug_hud", get(&g, "debug_hud", false)?.is_truthy()?)?;
        s.setattr("ui", imp(py, "main.client_rs.ui")?.getattr("UI")?.call1((&view,))?)?;
        s.setattr("shim", shim(py)?)?;
        s.setattr("editor", py.None())?; // 今までの画面の部品（ursina のまね）で開いている画面（プログラムなど）
        let game = py.get_type_bound::<GameAdapter>().call1((s,))?;
        s.setattr("game", &game)?;
        s.setattr("snd", imp(py, "main.client.sound")?.call_method1("get", (cfg,))?)?;
        s.setattr("_snd_t", 0.0)?;
        s.setattr("_snd_lit", PySet::empty_bound(py)?)?;
        s.setattr("_snd_loops", PyDict::new_bound(py))?;
        imp(py, "main.client.inventory")?.call_method1("ensure_hotbar", (world.getattr("player")?, world.getattr("registry")?))?;
        let addons = s.call_method0("_load_addons")?;
        s.setattr("addons", &addons)?;
        let r = (|| -> R<()> {
            let th = imp(py, "main.client.touch_ui")?.getattr("TouchHUD")?.call1((&game,))?;
            game.setattr("touch_hud", th) // 画面のボタン・仮想ジョイスティック（今までの画面と同じ部品）
        })();
        swallow(py, r, |e| log_exc(py, e, "画面のボタンを作れません", vec![]))?;
        // 画面側の MOD が描かない拡張の景色の箱（画面側の無い MOD）はここで描く
        let own: Vec<B<'_>> = addons.iter()?.map(|a| a.map(|a| gd(&a, "pack", none(py)))).collect::<R<_>>()?;
        let scenes = PyList::empty_bound(py);
        let sv = imp(py, "main.client.scenery_view")?.getattr("SceneView")?;
        for a in world.getattr("addons")?.iter()? {
            let a = a?;
            if own.iter().any(|o| o.is(&a)) {
                continue;
            }
            let sc = gd(&a, "scenes", none(py));
            if !sc.is_truthy()? {
                continue;
            }
            for x in sc.iter()? {
                let x = x?;
                let r = sv.call1((&root, &x));
                if let Some(v) = swallow(py, r, |e| log_exc(py, e, "景色を描けません: %s", vec![gd(&a, "name", a.clone()).unbind()]))? {
                    scenes.append((&a, v))?;
                }
            }
        }
        s.setattr("scenes", scenes)?;
        s.setattr("_addon_err", PySet::empty_bound(py)?)?;
        s.setattr("hotbar", py.None())?;
        s.setattr("held", PySet::empty_bound(py)?)?;
        s.setattr("yaw_fwd", py.None())?;
        s.setattr("pitch", 0.0)?;
        s.setattr("grabbed", false)?;
        s.setattr("acc", 0.0)?;
        s.setattr("quit", false)?;
        let perf = PyDict::new_bound(py);
        perf.set_item("frames", 0)?;
        perf.set_item("t", imp(py, "time")?.call_method0("perf_counter")?)?;
        perf.set_item("phys", 0.0)?;
        perf.set_item("rend", 0.0)?;
        perf.set_item("ticks", 0)?;
        s.setattr("perf", perf)?;
        Ok(())
    }

    // ---- MOD の画面側 ----
    /// MOD の画面側（ClientAddon）を作る（今までの画面と同じ modcode.client_addons）
    fn _load_addons<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let mc = imp(py, "main.modcode")?;
        let errors = PyList::empty_bound(py);
        mc.call_method1("load_client", (&errors,))?;
        let game = ga(s, "game")?;
        let out = mc.call_method1("client_addons", (&game, &errors))?;
        let w = ga(s, "w")?;
        let sv = w.call_method1("addon", ("survival",))?;
        if !sv.is_none() && gd(&sv, "mod_id", none(py)).is_none() {
            let r = (|| -> R<()> {
                let sc = imp(py, "main.client.survival_ui")?.getattr("SurvivalClient")?.call1((&game,))?;
                out.call_method1("insert", (0, sc))?;
                Ok(())
            })();
            swallow(py, r, |e| log_exc(py, e, "サバイバルの目標を出せません", vec![]))?;
        }
        let em = ga(&game, "emotes")?;
        for a in out.iter()? {
            let a = a?;
            let r = (|| -> R<()> {
                let e = a.call_method0("emotes")?;
                let e = if e.is_truthy()? { e } else { PyDict::new_bound(py).into_any() };
                em.call_method1("update", (e,))?;
                Ok(())
            })();
            swallow(py, r, |e| {
                errors.append(format!("MOD '{}' のモーションを読めません: {}", gd(&a, "name", tb(py, "?")).str()?, e.value_bound(py).str()?))
            })?;
        }
        for e in errors.iter() {
            let first: String = e.call_method0("splitlines")?.get_item(0)?.extract()?;
            w.call_method1("message", (format!("⚠ {}", first),))?;
        }
        s.setattr("load_errors", errors)?;
        Ok(out)
    }
    /// MOD の口を順に呼ぶ（エラーは1回だけ記録して続ける）。first：最初に真を返した物の値
    #[pyo3(signature = (f, first=false))]
    fn _each_addon<'py>(slf: &Bound<'py, Self>, f: &B<'py>, first: bool) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let addons: Vec<B<'_>> = ga(s, "addons")?.iter()?.collect::<R<_>>()?;
        for a in addons {
            let r = match f.call1((&a,)) {
                Ok(v) => v,
                Err(e) if is_exc(py, &e) => {
                    let key = (gd(&a, "name", tb(py, "?")), gd(f, "__name__", tb(py, "")));
                    let key = PyTuple::new_bound(py, [key.0, key.1]);
                    let errs = ga(s, "_addon_err")?;
                    if !errs.contains(&key)? {
                        errs.call_method1("add", (&key,))?;
                        log_exc(py, &e, "MOD '%s' の画面側でエラー", vec![key.get_item(0)?.unbind()])?;
                        ga(s, "w")?.call_method1("message", (format!("⚠ MOD '{}' の画面でエラー（logs/latest.log）", key.get_item(0)?.str()?),))?;
                    }
                    none(py)
                }
                Err(e) => return Err(e),
            };
            if first && r.is_truthy()? {
                return Ok(r);
            }
        }
        Ok(none(py))
    }
    fn _usable(slf: &Bound<'_, Self>, hit: &B<'_>) -> R<bool> {
        let py = slf.py();
        if !hit.is_truthy()? {
            return Ok(false);
        }
        let aim = acall(py, "aim", vec![hit.clone().unbind()])?;
        if hit.get_item("kind")?.eq("voxel")? {
            let d = hit.get_item("construct")?.getattr("devs")?.get_item(hit.get_item("index")?)?;
            if d.is_truthy()? {
                return Ok(true);
            }
        }
        Self::_each_addon(slf, &aim, true)?.is_truthy()
    }
    /// 近くの触れる物（機器・MOD の受付・お店・人物など）：使う順（今までの画面と同じ play.nearby_targets）
    #[pyo3(signature = (radius=2.6))]
    fn nearby_targets<'py>(slf: &Bound<'py, Self>, radius: f64) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        imp(py, "main.client.play")?.call_method1("nearby_targets", (ga(s, "w")?, ga(s, "addons")?, radius))
    }
    /// E：MOD が使わなければワールドの interact（機器・席）
    fn do_interact(slf: &Bound<'_, Self>, hit: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let f = acall(py, "interact", vec![hit.clone().unbind()])?;
        if !Self::_each_addon(slf, &f, true)?.is_truthy()? {
            let w = ga(s, "w")?;
            let mut kind = none(py);
            if hit.is_truthy()? && get(hit, "kind", py.None())?.eq("voxel")? {
                let d = hit.get_item("construct")?.getattr("devs")?.get_item(hit.get_item("index")?)?;
                if d.is_truthy()? {
                    let dd = w.getattr("registry")?.call_method1("device", (d.get_item("def")?,))?;
                    let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
                    kind = get(&dd, "type", py.None())?;
                }
            }
            w.call_method1("interact", (hit,))?;
            s.call_method1("sfx", (if kind.eq("seat")? { "seat" } else { "click" },))?;
        } else {
            s.call_method1("sfx", ("click",))?;
        }
        Ok(())
    }
    // ---- 音 ----
    #[pyo3(signature = (name, volume=1.0))]
    fn sfx(slf: &Bound<'_, Self>, name: &B<'_>, volume: f64) -> R<()> {
        let snd = ga(slf.as_any(), "snd")?;
        if !snd.is_none() {
            snd.call_method1("sfx", (name, volume))?;
        }
        Ok(())
    }
    /// BGM（MOD が決める）と、近くの推進機・アクチュエーター・ジェットパックの音（今までの画面と同じ）
    fn _update_sound(slf: &Bound<'_, Self>, dt: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let snd = ga(s, "snd")?;
        if snd.is_none() {
            return Ok(());
        }
        let f = acall(py, "bgm", vec![])?;
        snd.call_method1("set_bgm", (Self::_each_addon(slf, &f, true)?,))?;
        let w = ga(s, "w")?;
        if ga(&ga(s, "ui")?, "stack")?.is_truthy()? && gd(&w, "net", none(py)).is_none() {
            snd.call_method0("stop_loops")?;
        } else {
            let t = f64of(&ga(s, "_snd_t")?)? - dt;
            s.setattr("_snd_t", t)?;
            if t <= 0.0 {
                // 機器の様子を数えるのは 0.1 秒ごと
                s.setattr("_snd_t", 0.1)?;
                let r = imp(py, "main.client.sound")?.call_method1("sources", (&w, ga(s, "_snd_lit")?, ga(s, "sfx")?))?;
                s.setattr("_snd_loops", r.get_item(0)?)?;
                s.setattr("_snd_lit", r.get_item(1)?)?;
            }
            let loops = ga(s, "_snd_loops")?;
            for name in ["thruster_loop", "motor_loop", "jetpack_loop"] {
                snd.call_method1("loop", (name, get(&loops, name, 0.0)?))?;
            }
        }
        snd.call_method1("update", (dt,))?;
        Ok(())
    }
}

/// Rust の画面でワールドを開く。frames・shot：試験用（その数だけ描いて、最後の画面を保存して終わる）
#[pyfunction]
#[pyo3(signature = (cfg, world_name, frames=None, shot=None))]
pub fn viewer_run<'py>(py: Python<'py>, cfg: &B<'py>, world_name: &B<'py>, frames: Option<i64>, shot: Option<&B<'py>>) -> R<B<'py>> {
    let paths = imp(py, "main.paths")?;
    let name = if world_name.is_truthy()? { world_name.clone() } else { get(&cfg.get_item("game")?, "default_world", "scenario")? };
    let world_cls = imp(py, "main.app")?.call_method1("prepare_world", (&name,))?;
    if !paths.call_method1("world_dir", (&name,))?.call_method1("__truediv__", ("world.json",))?.call_method0("exists")?.is_truthy()? {
        let se = py.import_bound("builtins")?.getattr("SystemExit")?.call1((format!("ワールド {} がありません", name.str()?),))?;
        return Err(PyErr::from_value_bound(se));
    }
    let w = world_cls.call1((&name, cfg))?.call_method0("load")?;
    let v = imp(py, "main.client_rs.viewer")?.getattr("Viewer")?.call1((w, cfg))?;
    let Some(frames) = frames else {
        v.call_method0("run")?;
        return Ok(v);
    };
    let t = imp(py, "time")?;
    let mut last: f64 = t.call_method0("perf_counter")?.extract()?;
    for k in 0..frames {
        let now: f64 = t.call_method0("perf_counter")?.extract()?;
        let dt = (now - last).min(0.1);
        last = now;
        if let Some(sh) = shot.filter(|x| x.is_truthy().unwrap_or(false)) {
            if k == frames - 1 {
                ga(&v, "view")?.call_method1("screenshot", (sh.str()?,))?;
            }
        }
        v.call_method1("frame", (dt,))?;
    }
    Ok(v)
}

// 中身は分けて書いている（C-02）
mod input;
mod frame;
mod hud;
mod render;
mod adapter;
pub(crate) use self::adapter::*;
