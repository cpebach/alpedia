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
    // ---- 視点 ----
    fn look_vectors<'py>(slf: &Bound<'py, Self>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let s = slf.as_any();
        let up_o = ga(&ga(&ga(s, "w")?, "player")?, "up")?;
        let up = v3(&up_o)?;
        let refv = || if up[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let yo = ga(s, "yaw_fwd")?;
        let yaw = if yo.is_none() {
            let r = refv();
            sub(r, mul(up, dot(r, up)))
        } else {
            v3(&yo)?
        };
        let mut f = sub(yaw, mul(up, dot(yaw, up)));
        if norm(f) < 1e-3 {
            let r = refv();
            f = sub(r, mul(up, dot(r, up)));
        }
        let n = norm(f).max(1e-9);
        f = [f[0] / n, f[1] / n, f[2] / n];
        let fo = np3(py, f);
        s.setattr("yaw_fwd", &fo)?;
        let pitch = f64of(&ga(s, "pitch")?)?;
        let look = add(mul(f, pitch.cos()), mul(up, pitch.sin()));
        Ok(PyTuple::new_bound(py, [fo, np3(py, cross(up, f)), up_o, np3(py, look)]))
    }
    fn _input(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let c = ga(s, "cfg")?.get_item("controls")?;
        let (mut dx, mut dy) = (0.0f64, 0.0f64);
        let view = ga(s, "view")?;
        let game = ga(s, "game")?;
        let sh = ga(s, "shim")?;
        let ui = ga(s, "ui")?;
        let held = ga(s, "held")?;
        let evs = view.call_method0("pump")?;
        for ev in evs.iter()? {
            let ev = ev?;
            let (kind_o, a, b, n) = (ev.get_item(0)?, ev.get_item(1)?, ev.get_item(2)?, ev.get_item(3)?);
            let kind: String = kind_o.extract()?;
            let cm = ga(&game, "cam_mode")?;
            let editor_none = ga(s, "editor")?.is_none();
            let stack = ga(&ui, "stack")?.is_truthy()?;
            if !cm.is_none() && editor_none && !stack {
                // カメラモード：パネルのボタン・ドラッグは ursina のまねへ、キーとホイールはカメラモードへ
                sh.call_method1("event", (&view, &kind_o, &a, &b, &n))?;
                if let Some(k) = kind.strip_prefix("key:") {
                    if a.is_truthy()? {
                        held.call_method1("add", (k,))?;
                        if !n.is_truthy()? {
                            cm.call_method1("on_key", (keymap(k),))?;
                        }
                    } else {
                        held.call_method1("discard", (k,))?;
                    }
                } else if kind == "wheel" && a.is_truthy()? {
                    cm.call_method1("on_key", (if f64of(&a)? > 0.0 { "scroll up" } else { "scroll down" },))?;
                } else if kind == "close" {
                    s.setattr("quit", true)?;
                    s.setattr("close_app", true)?;
                }
                continue;
            }
            if !editor_none && sh.call_method1("event", (&view, &kind_o, &a, &b, &n))?.is_truthy()? {
                continue;
            }
            if kind.starts_with("touch:") {
                // 指：画面のボタン・ジョイスティック・なぞって視点
                let (ww, hh): (f64, f64) = (f64of(&view.call_method0("size")?.get_item(0)?)?, f64of(&view.call_method0("size")?.get_item(1)?)?);
                ga(&ga(&game, "touch_hud")?, "ptr")?.call_method1("feed", (&kind_o, (f64of(&a)? - ww / 2.0) / hh, (hh / 2.0 - f64of(&b)?) / hh, &n))?;
                continue;
            }
            let grabbed = ga(s, "grabbed")?.is_truthy()?;
            if editor_none && !grabbed && !stack {
                // 画面の上のボタン（目標の欄・画面のボタン）
                if kind == "cursor" {
                    sh.call_method1("event", (&view, &kind_o, &a, &b, &n))?;
                } else if kind == "button" && n.eq(0)? {
                    let m = ga(&sh, "mouse")?;
                    let (mx, my) = (ga(&m, "x")?, ga(&m, "y")?);
                    let pass = !a.is_truthy()? || sh.call_method1("_hit", (&mx, &my))?.is_truthy()?
                        || ga(&game, "touch_hud")?.call_method1("over_ui", (&mx, &my))?.is_truthy()? || s.call_method0("_touch_mode")?.is_truthy()?;
                    if pass {
                        sh.call_method1("event", (&view, &kind_o, &a, &b, &n))?; // ボタンは指と同じ（ursina のまねの mouse.left）
                        continue;
                    }
                }
            }
            if ui.call_method1("event", (&kind_o, &a, &b, &n))?.is_truthy()? {
                continue;
            }
            if kind == "close" {
                s.setattr("quit", true)?;
                s.setattr("close_app", true)?;
            } else if let Some(k) = kind.strip_prefix("key:") {
                if k == "alt" {
                    // Alt を押している間はカーソルを出す（画面のボタンを押せる）
                    if a.is_truthy()? && ga(s, "grabbed")?.is_truthy()? {
                        s.setattr("_alt_free", true)?;
                        s.call_method1("set_grab", (false,))?;
                    } else if !a.is_truthy()? && gd(s, "_alt_free", tb(py, false)).is_truthy()? {
                        s.setattr("_alt_free", false)?;
                        if !ga(&ui, "stack")?.is_truthy()? && ga(s, "editor")?.is_none() && ga(&game, "cam_mode")?.is_none() {
                            s.call_method1("set_grab", (true,))?;
                        }
                    }
                }
                if a.is_truthy()? {
                    held.call_method1("add", (k,))?;
                    if !n.is_truthy()? {
                        s.call_method1("_key", (k, &c))?;
                    }
                } else {
                    held.call_method1("discard", (k,))?;
                }
            } else if kind == "mouse" {
                dx += f64of(&a)?;
                dy += f64of(&b)?;
            } else if kind == "button" && a.is_truthy()? && !ga(s, "grabbed")?.is_truthy()? {
                if !s.call_method0("_touch_mode")?.is_truthy()? {
                    s.call_method1("set_grab", (true,))?;
                }
            } else if kind == "button" && a.is_truthy()? {
                s.call_method1("_click", (&n,))?;
            } else if kind == "wheel" && a.is_truthy()? {
                let sel: i64 = ga(s, "sel")?.extract()?;
                s.setattr("sel", (sel + if f64of(&a)? > 0.0 { -1 } else { 1 }).rem_euclid(9))?;
            } else if kind == "focus" && !a.is_truthy()? {
                held.call_method0("clear")?;
            }
        }
        if dx != 0.0 || dy != 0.0 {
            let sens = f64of(&get(&c, "mouse_sensitivity", 60.0)?)? / 60.0 * 0.0025;
            Self::look_vectors(slf)?;
            let up = v3(&ga(&ga(&ga(s, "w")?, "player")?, "up")?)?;
            let f = v3(&ga(s, "yaw_fwd")?)?;
            let ang = dx * sens;
            s.setattr("yaw_fwd", np3(py, add(mul(f, ang.cos()), mul(cross(up, f), ang.sin()))))?;
            let p = f64of(&ga(s, "pitch")?)?;
            s.setattr("pitch", (p - dy * sens).clamp(-1.55, 1.55))?;
        }
        Ok(())
    }
    /// 左：壊す・掘る、右：置く
    fn _click(slf: &Bound<'_, Self>, button: i64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let seated = ga(&ga(&w, "player")?, "seated")?.is_truthy()?;
        let hit = ga(s, "hit")?;
        if seated || hit.is_none() {
            if button == 1 && !seated {
                s.call_method1("_place", (py.None(),))?;
            }
            return Ok(());
        }
        let kind = hit.get_item("kind")?;
        if button == 0 {
            if kind.eq("voxel")? {
                if !w.call_method1("remove", (&hit,))?.is(&tb(py, false)) {
                    s.call_method1("sfx", ("break",))?;
                }
            } else if (kind.eq("terrain")? || kind.eq("water")?) && !w.call_method1("dig", (&hit,))?.is(&tb(py, false)) {
                s.call_method1("sfx", ("dig",))?;
            }
        } else if button == 1 {
            s.call_method1("_place", (&hit,))?;
        }
        Ok(())
    }
    fn _place(slf: &Bound<'_, Self>, hit: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let item = ga(&ga(&w, "player")?, "hotbar")?.get_item(ga(s, "sel")?)?;
        if item.is_none() {
            w.call_method1("message", ("アイテムバーのこの枠は空です",))?;
            return Ok(());
        }
        let look = Self::look_vectors(slf)?.get_item(3)?;
        let ok = w.call_method1("place", (hit, &item, ga(s, "dir_index")?, look))?;
        let name = if !ok.is(&tb(py, false)) { if item.get_item(0)?.eq("device")? { "device" } else { "place" } } else { "error" };
        s.call_method1("sfx", (name,))?;
        Ok(())
    }
    /// 一時停止メニュー・インベントリを開く（マウスを出して、歩くのを止める）
    fn open_screen(slf: &Bound<'_, Self>, name: &str) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let sc = imp(py, "main.client_rs.screens")?;
        ga(s, "held")?.call_method0("clear")?;
        s.call_method1("set_grab", (false,))?;
        ga(&ga(s, "game")?, "ui_root")?.setattr("enabled", false)?; // MOD の画面の部品（目標の欄など）は隠す
        let cls = match name {
            "pause" => sc.getattr("Pause")?,
            "inventory" => sc.getattr("Inventory")?,
            _ => return Err(pyo3::exceptions::PyKeyError::new_err(name.to_string())),
        };
        ga(s, "ui")?.call_method1("open", (cls, s))?;
        Ok(())
    }
    /// P：見ている機器の設定（アクチュエーター・ほかの機器）。コンピューターはプログラムの画面
    fn open_editor(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let h = ga(s, "hit")?;
        let ok = h.is_truthy()? && h.get_item("kind")?.eq("voxel")? && h.get_item("construct")?.getattr("devs")?.get_item(h.get_item("index")?)?.is_truthy()?;
        if !ok {
            w.call_method1("message", ("機器を見ながら P を押すと設定を開けます",))?;
            return Ok(());
        }
        let (c, i) = (h.get_item("construct")?, h.get_item("index")?);
        let d = c.getattr("devs")?.get_item(&i)?;
        let dd = c.getattr("reg")?.call_method1("device", (d.get_item("def")?,))?;
        let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
        ga(s, "held")?.call_method0("clear")?;
        if get(&dd, "type", py.None())?.eq("computer")? {
            s.call_method1("open_program", (&c, &i))?;
            return Ok(());
        }
        s.call_method1("set_grab", (false,))?;
        ga(&ga(s, "game")?, "ui_root")?.setattr("enabled", false)?;
        let sd = imp(py, "main.client_rs.screens_dev")?;
        let cls = if imp(py, "main.actuator")?.call_method1("is_actuator", (&c, &i))?.is_truthy()? { sd.getattr("ActuatorPanel")? } else { sd.getattr("DevicePanel")? };
        ga(s, "ui")?.call_method1("open", (cls, s, &c, &i))?;
        Ok(())
    }
    /// コンピューターのプログラムの画面（ScriptPanel・BlockEditor を ursina のまねで動かす）
    fn open_program(slf: &Bound<'_, Self>, c: &B<'_>, i: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.call_method1("set_grab", (false,))?;
        let game = ga(s, "game")?;
        ga(&game, "ui_root")?.setattr("enabled", false)?;
        let blocks = get(&c.getattr("devs")?.get_item(i)?.get_item("state")?, "blocks", py.None())?;
        let ed = if blocks.is_truthy()? {
            imp(py, "main.client.block_editor")?.getattr("BlockEditor")?.call1((&game, c, i))?
        } else {
            imp(py, "main.client.script_panel")?.getattr("ScriptPanel")?.call1((&game, c, i))?
        };
        s.setattr("editor", ed)?;
        Ok(())
    }
    fn _touch_mode(slf: &Bound<'_, Self>) -> R<bool> {
        get(&ga(slf.as_any(), "cfg")?.get_item("controls")?, "touch", false)?.is_truthy()
    }
    /// マウスで視点を回す（カーソルを隠して閉じ込める）か。タッチ操作の ON のときは閉じ込めない
    fn set_grab(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let s = slf.as_any();
        let on = on.is_truthy()? && !Self::_touch_mode(slf)?;
        s.setattr("grabbed", on)?;
        ga(s, "game")?.setattr("mouse_captured", on)?;
        ga(s, "view")?.call_method1("grab", (on,))?;
        Ok(())
    }
    /// 画面の UI 座標 (x, y) を通る視線：(始点, 向き)。タッチで触った所を狙うのに使う
    fn screen_ray<'py>(slf: &Bound<'py, Self>, x: f64, y: f64) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let s = slf.as_any();
        let cam = gd(s, "_cam", none(py));
        if cam.is_none() {
            let look = Self::look_vectors(slf)?.get_item(3)?;
            return Ok(PyTuple::new_bound(py, [ga(&ga(s, "w")?, "player")?.call_method0("eye")?, look]));
        }
        let origin = cam.get_item(0)?;
        let look = v3(&cam.get_item(1)?)?;
        let up = v3(&cam.get_item(2)?)?;
        let vfov = f64of(&cam.get_item(3)?)?;
        let mut right = cross(look, up); // 画面の右（カメラの向きから）
        let n = norm(right).max(1e-9);
        right = [right[0] / n, right[1] / n, right[2] / n];
        let cu = cross(right, look);
        let t = (vfov.to_radians() / 2.0).tan();
        let d = add(add(look, mul(right, x * 2.0 * t)), mul(cu, y * 2.0 * t));
        let dn = norm(d);
        let np = imp(py, "numpy")?;
        let o = np.call_method1("asarray", (origin, py.import_bound("builtins")?.getattr("float")?))?;
        Ok(PyTuple::new_bound(py, [o, np3(py, [d[0] / dn, d[1] / dn, d[2] / dn])]))
    }
    /// タッチ：画面の (x, y) を触った。kind = "tap"（使う／置く）・"hold"（長押し：壊す・掘る）
    fn touch_action(slf: &Bound<'_, Self>, kind: &str, x: f64, y: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let p = ga(&w, "player")?;
        if ga(&p, "seated")?.is_truthy()? || !ga(s, "editor")?.is_none() || ga(&ga(s, "ui")?, "stack")?.is_truthy()? {
            return Ok(());
        }
        let r = Self::screen_ray(slf, x, y)?;
        let (o, d) = (r.get_item(0)?, r.get_item(1)?);
        let eye = v3(&p.call_method0("eye")?)?;
        let reach = f64of(&get(&ga(&p, "defn")?, "reach", 8.0)?)? + norm(sub(v3(&o)?, eye));
        let hit = w.call_method1("raycast", (&o, &d, reach))?;
        let f = tb(py, false);
        if kind == "hold" {
            if hit.is_truthy()? && hit.get_item("kind")?.eq("voxel")? {
                if !w.call_method1("remove", (&hit,))?.is(&f) {
                    s.call_method1("sfx", ("break",))?;
                }
            } else if hit.is_truthy()? && (hit.get_item("kind")?.eq("terrain")? || hit.get_item("kind")?.eq("water")?) && !w.call_method1("dig", (&hit,))?.is(&f) {
                s.call_method1("sfx", ("dig",))?;
            }
            return Ok(());
        }
        if hit.is_truthy()? && Self::_usable(slf, &hit)? {
            s.call_method1("do_interact", (&hit,))?;
            return Ok(());
        }
        if hit.is_truthy()? {
            let k = hit.get_item("kind")?;
            if k.eq("voxel")? || k.eq("terrain")? || k.eq("water")? || get(&hit, "placeable", py.None())?.is_truthy()? {
                s.call_method1("_place", (&hit,))?;
            }
        }
        Ok(())
    }
    /// LAN に公開する（一時停止メニューから）
    fn start_hosting(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let cfg = ga(s, "cfg")?;
        if let Err(e) = imp(py, "main.netserver")?.getattr("Host")?.call1((&w, &cfg)) {
            if e.is_instance_of::<pyo3::exceptions::PyOSError>(py) {
                log_exc(py, &e, "LAN に公開できません", vec![])?;
                let port = get(&get(&cfg, "multiplayer", PyDict::new_bound(py))?, "port", py.None())?;
                w.call_method1("message", (format!("LAN に公開できません（ポート {} が使えない？）: {}", port.str()?, e.value_bound(py).str()?),))?;
                return Ok(());
            }
            return Err(e);
        }
        let a = w.getattr("net")?.call_method0("addresses")?.get_item(pyo3::types::PySlice::new_bound(py, 0, 2, 1))?;
        let j = PyString::new_bound(py, "  ").call_method1("join", (a,))?;
        w.call_method1("message", (format!("LAN に公開しました: {}（参加する人はメニューの「マルチプレイ」から）", j.str()?),))?;
        Ok(())
    }
    fn screen_closed(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let stack = ga(&ga(s, "ui")?, "stack")?.is_truthy()?;
        let game = ga(s, "game")?;
        if !stack && ga(s, "editor")?.is_none() && ga(&game, "cam_mode")?.is_none() {
            ga(&game, "ui_root")?.setattr("enabled", true)?;
        }
        if !stack {
            s.call_method1("set_grab", (true,))?;
        }
        Ok(())
    }
    fn _key(slf: &Bound<'_, Self>, k: &str, c: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let seat_keys = imp(py, "main.client.game")?.getattr("SEAT_KEYS")?;
        let p = ga(&w, "player")?;
        let ck = |name: &str, d: &str| -> R<bool> { Ok(get(c, name, d)?.eq(k)?) };
        let ci = |name: &str| -> R<bool> { c.get_item(name)?.eq(k) };
        if ga(&p, "seated")?.is_truthy()? && k != "escape" {
            let kk = keymap(k);
            let in_seat = seat_keys.contains(kk)?;
            if in_seat || kk == "x" {
                w.call_method1("key_pressed", (kk,))?;
            }
            if ci("interact")? {
                w.call_method1("interact", (py.None(),))?;
                s.call_method1("sfx", ("seat",))?;
            } else if ck("third_person", "v")? {
                let t = ga(s, "third_person")?.is_truthy()?;
                s.setattr("third_person", !t)?;
            } else if ck("save", "f5")? {
                w.call_method0("save")?;
            } else if ck("export", "f9")? {
                s.call_method0("export_obj")?;
            } else if !in_seat {
                let f = acall(py, "on_key", vec![kk.into_py(py)])?;
                Self::_each_addon(slf, &f, true)?; // MOD のキー（座っている間も使えるもの）
            }
            return Ok(());
        }
        if k.chars().count() == 1 && "123456789".contains(k) {
            s.setattr("sel", k.parse::<i64>().unwrap() - 1)?;
        } else if ci("rotate")? {
            let d: i64 = ga(s, "dir_index")?.extract()?;
            s.setattr("dir_index", (d + 1).rem_euclid(6))?;
        } else if ci("interact")? {
            let mut hit = ga(s, "hit")?;
            if !Self::_usable(slf, &hit)? {
                // 見ている物が使えなければ、近くの MOD の物（受付・お店など）
                for t in Self::nearby_targets(slf, 2.6)?.iter()? {
                    let t = t?;
                    if get(&t, "kind", py.None())?.eq("e")? {
                        hit = t.get_item("hit")?;
                        break;
                    }
                }
            }
            if !hit.is_none() {
                s.call_method1("do_interact", (&hit,))?;
            }
        } else if ci("new_construct")? {
            s.call_method1("_place", (py.None(),))?;
        }
        if k == "escape" {
            s.call_method1("open_screen", ("pause",))?;
        } else if ck("inventory", "i")? {
            s.call_method1("open_screen", ("inventory",))?;
        } else if ck("edit_script", "p")? {
            s.call_method0("open_editor")?;
        } else if ck("jetpack", "f")? {
            let j = ga(&p, "jetpack")?.is_truthy()?;
            p.setattr("jetpack", !j)?;
        } else if ck("save", "f5")? {
            w.call_method0("save")?;
        } else if ck("third_person", "v")? {
            let t = ga(s, "third_person")?.is_truthy()?;
            s.setattr("third_person", !t)?;
        } else if ck("toggle_help", "f1")? {
            let t = gd(s, "show_help", tb(py, false)).is_truthy()?;
            s.setattr("show_help", !t)?;
        } else if ck("debug_hud", "f3")? {
            let t = gd(s, "debug_hud", tb(py, false)).is_truthy()?;
            s.setattr("debug_hud", !t)?;
        } else if k == "f11" {
            let v = ga(s, "view")?;
            let fs = v.call_method0("fullscreen")?.is_truthy()?;
            v.call_method1("set_fullscreen", (!fs,))?;
        } else if ck("camera_mode", "c")? {
            ga(s, "game")?.call_method0("enter_camera")?;
        } else if ck("export", "f9")? {
            s.call_method0("export_obj")?;
        } else if ck("import", "f8")? {
            ga(s, "game")?.call_method0("open_import")?;
        } else if !(ci("forward")? || ci("back")? || ci("left")? || ci("right")? || ci("jump")?) {
            let f = acall(py, "on_key", vec![k.into_py(py)])?;
            Self::_each_addon(slf, &f, true)?;
        }
        Ok(())
    }

    // ---- 1フレーム ----
    fn frame(slf: &Bound<'_, Self>, dt: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let cfg = ga(s, "cfg")?;
        let c = cfg.get_item("controls")?;
        s.call_method0("_input")?;
        let game = ga(s, "game")?;
        let cam_mode = ga(&game, "cam_mode")?;
        let hud = ga(&game, "touch_hud")?;
        if cam_mode.is_none() {
            hud.call_method1("update", (dt,))?; // 画面のボタン・ジョイスティック・近くの物（押されたキー → vkeys）
        }
        let vk = ga(&game, "vkeys")?;
        let tl = hud.call_method0("take_look")?; // 画面をなぞった量（タッチ）
        let (tdx, tdy) = (f64of(&tl.get_item(0)?)?, f64of(&tl.get_item(1)?)?);
        let player = ga(&w, "player")?;
        if tdx != 0.0 || tdy != 0.0 {
            let sens = (f64of(&get(&c, "mouse_sensitivity", 60.0)?)? * 1.6).to_radians();
            Self::look_vectors(slf)?;
            let upv = v3(&ga(&player, "up")?)?;
            let ang = tdx * sens;
            let yf = v3(&ga(s, "yaw_fwd")?)?;
            s.setattr("yaw_fwd", np3(py, add(mul(yf, ang.cos()), mul(cross(upv, yf), ang.sin()))))?;
            let p = f64of(&ga(s, "pitch")?)?;
            s.setattr("pitch", (p + tdy * sens).clamp(-1.55, 1.55))?;
        }
        let lv = Self::look_vectors(slf)?;
        let (fwd_o, right_o, up_o, look_o) = (lv.get_item(0)?, lv.get_item(1)?, lv.get_item(2)?, lv.get_item(3)?);
        let (fwd, right, up) = (v3(&fwd_o)?, v3(&right_o)?, v3(&up_o)?);
        let lp = ga(&w, "local_player")?;
        lp.setattr("look_fwd", &fwd_o)?;
        lp.setattr("pitch", ga(s, "pitch")?)?;
        let held = PySet::empty_bound(py)?;
        for k in ga(s, "held")?.iter()? {
            let k = k?;
            let ks: String = k.extract()?;
            held.add(keymap(&ks))?;
        }
        for kv in vk.call_method0("items")?.iter()? {
            let kv = kv?;
            if kv.get_item(1)?.is_truthy()? {
                held.add(kv.get_item(0)?)?;
            }
        }
        let hk = |k: &B<'_>| -> R<f64> {
            Ok(if held.contains(k)? || (k.eq("control")? && held.contains("left control")?) { 1.0 } else { 0.0 })
        };
        let st = ga(&hud, "stick")?;
        let (st0, st1) = (f64of(&st.get_item(0)?)?, f64of(&st.get_item(1)?)?);
        let mut mv = [0.0f64; 3];
        let ui = ga(s, "ui")?;
        let busy = ga(&ui, "stack")?.is_truthy()? || !ga(s, "editor")?.is_none() || ga(&game, "hold")?.is_truthy()? || !cam_mode.is_none();
        let seated = ga(&player, "seated")?.is_truthy()?;
        let ci = |n: &str| c.get_item(n);
        if !seated && !busy {
            let a = hk(&ci("forward")?)? - hk(&ci("back")?)? + st1;
            let b = hk(&ci("right")?)? - hk(&ci("left")?)? + st0;
            mv = add(add(mv, mul(fwd, a)), mul(right, b));
            let nm = norm(mv);
            if nm > 1.0 {
                mv = [mv[0] / nm, mv[1] / nm, mv[2] / nm];
            }
        }
        let jump = hk(&ci("jump")?)? != 0.0 && !seated && !busy;
        let thrust = if !busy {
            add(mul(mv, 0.6), mul(up, hk(&ci("jump")?)? - hk(&get(&c, "down", "left control")?)?))
        } else {
            [0.0; 3]
        };
        let seat_keys = imp(py, "main.client.game")?.getattr("SEAT_KEYS")?;
        if seated && !busy {
            let keys = PySet::empty_bound(py)?;
            for k in held.iter() {
                if seat_keys.contains(&k)? {
                    keys.add(k)?;
                }
            }
            for (k, on) in [("forward", st1 > 0.4), ("back", st1 < -0.4), ("right", st0 > 0.4), ("left", st0 < -0.4)] {
                // 操縦席ではスティックを W/A/S/D として使う
                if on {
                    keys.add(ci(k)?)?;
                }
            }
            w.setattr("input_keys", keys)?;
        } else {
            w.setattr("input_keys", PySet::empty_bound(py)?)?;
        }
        let tick = 1.0 / f64of(&get(&cfg.get_item("physics")?, "tick_rate", 60)?)?;
        let mut acc = f64of(&ga(s, "acc")?)? + dt;
        let mut n = 0;
        let time = imp(py, "time")?;
        let pc = || -> R<f64> { time.call_method0("perf_counter")?.extract() };
        let t0 = pc()?;
        let net = gd(&w, "net", none(py));
        let client = !net.is_none() && ga(&net, "role")?.eq("client")?;
        if ga(&game, "hold")?.is_truthy()? && net.is_none() {
            acc = 0.0; // MOD が世界の時間を止めている（ムービーなど）
        }
        if client {
            // マルチプレイに参加中：ホストから届いた世界を当てる
            net.setattr("look", &look_o)?;
            let top = ga(&ui, "top")?;
            let ed = ga(s, "editor")?;
            let edv = if ed.is_truthy()? { ed } else if top.hasattr("c")? { top } else { none(py) };
            let k = PyDict::new_bound(py);
            k.set_item("editor", edv)?;
            net.call_method1("frame", (imp(py, "types")?.getattr("SimpleNamespace")?.call((), Some(&k))?, dt))?;
            let closed = ga(&net, "closed")?;
            if closed.is_truthy()? {
                s.setattr("acc", acc)?;
                let k = PyDict::new_bound(py);
                k.set_item("status", format!("マルチプレイから抜けました：{}", closed.str()?))?;
                game.call_method("_leave", ("menu",), Some(&k))?;
                return Ok(());
            }
        }
        let mvo = np3(py, mv);
        let tho = np3(py, thrust);
        while acc >= tick && n < 3 {
            if client {
                net.call_method1("step", (tick, &mvo, jump && n == 0, &tho))?; // 自分の体だけ動かす
            } else {
                w.call_method1("step", (tick, &mvo, jump && n == 0, &tho))?;
            }
            acc -= tick;
            n += 1;
        }
        if n == 3 {
            acc = 0.0;
        }
        s.setattr("acc", acc)?;
        if !net.is_none() && !client {
            net.call_method1("tick", (dt,))?; // 参加者へ送る・参加者の操作を行う
        }
        let t1 = pc()?;
        let cam = s.call_method1("_camera", (dt, &look_o, &up_o))?;
        let (origin, cam_look, cam_up) = (cam.get_item(0)?, cam.get_item(1)?, cam.get_item(2)?);
        s.call_method1("_render", (&origin,))?;
        s.call_method1("_bodies", (&origin,))?;
        let g = cfg.get_item("graphics")?;
        let view = ga(s, "view")?;
        let sz = view.call_method0("size")?;
        let (ww, hh) = (sz.get_item(0)?, sz.get_item(1)?);
        let sh = ga(s, "shim")?;
        let fov = if !cam_mode.is_none() { f64of(&ga(&ga(&sh, "camera")?, "fov")?)? } else { f64of(&get(&g, "fov", 80)?)? };
        let hfov = fov.to_radians(); // Panda3D と同じく横の視野角
        let wwf = f64of(&ww)?;
        let hhf = f64of(&hh)?;
        let vfov = (2.0 * ((hfov / 2.0).tan() * hhf / wwf.max(1.0)).atan()).to_degrees();
        let cl = v3(&cam_look)?;
        let cu = v3(&cam_up)?;
        view.call_method1("camera", (vec![0.0, 0.0, 0.0], cl.to_vec(), cu.to_vec(), vfov, 0.05))?;
        s.setattr("_cam", PyTuple::new_bound(py, [origin.clone(), np3(py, cl), np3(py, cu), tb(py, vfov)]))?;
        ga(&sh, "camera")?.setattr("fov_pair", (hfov.to_degrees(), vfov))?;
        let far = f64of(&get(&g, "view_distance", 3000)?)?;
        let f = acall(py, "update", vec![origin.clone().unbind(), far.into_py(py)])?;
        Self::_each_addon(slf, &f, false)?;
        if !cam_mode.is_none() && !ga(&cam_mode, "frame_on")?.is_truthy()? {
            s.setattr("hit", py.None())?;
            ga(s, "highlight")?.call_method0("hide")?;
            ga(s, "ghost")?.call_method0("hide")?;
            s.setattr("_aim_txt", "")?;
        } else {
            s.call_method1("_aim", (&origin, &look_o))?;
        }
        s.call_method1("_overlay", (&ww, &hh))?;
        sh.call_method1("frame", (&view, dt))?; // 今までの画面の部品（プログラムの画面など）
        s.call_method1("_update_sound", (dt,))?;
        view.call_method0("render")?;
        let t2 = pc()?;
        let perf = ga(s, "perf")?;
        let pi = |k: &str| -> R<f64> { f64of(&perf.get_item(k)?) };
        let frames: i64 = perf.get_item("frames")?.extract::<i64>()? + 1;
        perf.set_item("frames", frames)?;
        perf.set_item("phys", pi("phys")? + (t1 - t0))?;
        perf.set_item("rend", pi("rend")? + (t2 - t1))?;
        let ticks: i64 = perf.get_item("ticks")?.extract::<i64>()? + n;
        perf.set_item("ticks", ticks)?;
        let el = t2 - pi("t")?;
        if el >= 1.0 {
            let fr = frames as f64;
            let txt = format!("fps {}  物理 {} ms/回  描画 {} ms", pf(py, fr / el, ".0f")?, pf(py, pi("phys")? / (ticks.max(1) as f64) * 1000.0, ".1f")?,
                pf(py, pi("rend")? / fr * 1000.0, ".1f")?);
            s.setattr("fps_text", &txt)?;
            let pl: i64 = gd(s, "_perf_log", tb(py, 0)).extract::<i64>()? + 1;
            s.setattr("_perf_log", pl)?;
            if pl % 10 == 1 {
                // ログは 10 秒ごと
                lg(py)?.call_method1("info", ("重さ: %s（構築物 %d）", &txt, ga(&w, "constructs")?.len()?))?;
            }
            let np = PyDict::new_bound(py);
            np.set_item("frames", 0)?;
            np.set_item("t", t2)?;
            np.set_item("phys", 0.0)?;
            np.set_item("rend", 0.0)?;
            np.set_item("ticks", 0)?;
            s.setattr("perf", np)?;
        }
        Ok(())
    }
    /// カメラの位置・視線・上（今までの画面と同じ：カメラモード・遠隔操作・三人称・一人称）
    fn _camera<'py>(slf: &Bound<'py, Self>, dt: f64, look: &B<'py>, up: &B<'py>) -> R<Bound<'py, PyTuple>> {
        let py = slf.py();
        let s = slf.as_any();
        let play = imp(py, "main.client.play")?;
        let w = ga(s, "w")?;
        let player = ga(&w, "player")?;
        let mut origin = player.call_method0("eye")?;
        let seated = !ga(&player, "seated")?.is_none(); // 座ったら三人称（降りたら元に）。V でいつでも切り替え
        let was = ga(s, "_was_seated")?.is_truthy()?;
        if seated && !was {
            s.setattr("_third_before_seat", ga(s, "third_person")?)?;
            s.setattr("third_person", true)?;
        } else if was && !seated {
            s.setattr("third_person", gd(s, "_third_before_seat", tb(py, false)))?;
        }
        s.setattr("_was_seated", seated)?;
        let cm = ga(&ga(s, "game")?, "cam_mode")?;
        if !cm.is_none() {
            let held = Py::new(py, Held::make(s)?)?;
            cm.call_method1("update", (dt, held, PyDict::new_bound(py), imp(py, "numpy")?.call_method1("zeros", (2,))?))?;
            let r = cm.call_method0("camera")?;
            return Ok(PyTuple::new_bound(py, [r.get_item(0)?, r.get_item(1)?, r.get_item(2)?]));
        }
        let tp = ga(s, "third_person")?.is_truthy()?;
        let follow = if tp { play.call_method1("seat_follow", (&w, ga(s, "addons")?))? } else { none(py) };
        let lk = v3(look)?;
        let u = v3(up)?;
        if !follow.is_none() {
            // 遠隔操作：操っている物（ドローンなど）の後ろから
            let com = v3(&follow.get_item(0)?)?;
            let dist = f64of(&follow.get_item(1)?)?;
            return Ok(PyTuple::new_bound(py, [np3(py, add(sub(com, mul(lk, dist)), mul(u, dist * 0.25))), look.clone(), up.clone()]));
        }
        if tp {
            let mut dist = 5.0;
            if seated {
                // 乗り物の大きさに合わせて離れる
                let c = ga(&w, "constructs")?.call_method1("get", (ga(&player, "seated")?.get_item(0)?,))?;
                if !c.is_none() && ga(&c, "n_vox")?.is_truthy()? {
                    let pos = ga(&c, "pos")?;
                    let ext = f64of(&imp(py, "numpy")?.getattr("linalg")?.call_method1("norm", (pos.call_method1("max", (0,))?.call_method1("__sub__", (pos.call_method1("min", (0,))?,))?,))?)?;
                    dist = (ext * 1.3).clamp(5.0, 40.0);
                }
            }
            origin = play.call_method1("camera_clip", (&w, &origin, np3(py, add(mul(lk, -dist), mul(u, 0.8 + dist * 0.1)))))?;
        }
        Ok(PyTuple::new_bound(py, [origin, look.clone(), up.clone()]))
    }
    /// F9：見ている（座っている）構築物を OBJ で出力
    fn export_obj(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let p = ga(&w, "player")?;
        let mut target = none(py);
        let hit = ga(s, "hit")?;
        if ga(&p, "seated")?.is_truthy()? {
            target = ga(&w, "constructs")?.call_method1("get", (ga(&p, "seated")?.get_item(0)?,))?;
        } else if hit.is_truthy()? && hit.get_item("kind")?.eq("voxel")? {
            target = hit.get_item("construct")?;
        }
        if target.is_none() {
            w.call_method1("message", ("出力する構築物を見てください",))?;
            return Ok(());
        }
        let r = w.call_method1("export_obj", (&target,));
        swallow(py, r, |e| {
            log_exc(py, e, "OBJ 出力失敗", vec![])?;
            w.call_method1("message", (format!("OBJ 出力失敗: {}", e.value_bound(py).str()?),))?;
            Ok(())
        })?;
        Ok(())
    }
    /// 狙っている物：枠と説明（今までの画面と同じ play.aim）
    fn _aim(slf: &Bound<'_, Self>, origin: &B<'_>, look: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let play = imp(py, "main.client.play")?;
        let w = ga(s, "w")?;
        let p = ga(&w, "player")?;
        let hit = if ga(&p, "seated")?.is_truthy()? {
            none(py)
        } else {
            w.call_method1("raycast", (p.call_method0("eye")?, look, f64of(&get(&ga(&p, "defn")?, "reach", 8.0)?)?))?
        };
        s.setattr("hit", &hit)?;
        let gx = gfx(py)?;
        let (hl, gh) = (ga(s, "highlight")?, ga(s, "ghost")?);
        if hit.is_truthy()? {
            let k = hit.get_item("kind")?;
            if !(k.eq("voxel")? || k.eq("terrain")? || k.eq("water")?) {
                let f = acall(py, "aim", vec![hit.clone().unbind()])?;
                let r = Self::_each_addon(slf, &f, true)?; // MOD の物（街の建物・NPC など）
                if r.is_truthy()? {
                    let (txt, placeable) = (r.get_item(0)?, r.get_item(1)?.is_truthy()?);
                    hl.call_method0("hide")?;
                    if placeable {
                        let size = f64of(&get(&ga(&w, "settings")?, "voxel_size", 0.5)?)?;
                        let lv = Self::look_vectors(slf)?;
                        let (fwd, right) = (v3(&lv.get_item(0)?)?, v3(&lv.get_item(1)?)?);
                        let nrm = v3(&hit.get_item("normal")?)?;
                        let c = sub(add(v3(&hit.get_item("point")?)?, mul(nrm, size * 0.55)), v3(origin)?);
                        let np = imp(py, "numpy")?;
                        let ax = np.call_method1("stack", (vec![np3(py, right), np3(py, nrm), np3(py, fwd)],))?;
                        gx.call_method1("set_box", (&gh, np3(py, c), ax, np.call_method1("full", (3, size))?))?;
                        gh.call_method0("show")?;
                    } else {
                        gh.call_method0("hide")?;
                    }
                    s.setattr("_aim_txt", txt)?;
                    if placeable {
                        hit.set_item("placeable", true)?;
                    }
                    return Ok(());
                }
            }
        }
        let a = play.call_method1("aim", (&w, &hit, Self::look_vectors(slf)?))?;
        for (node, key) in [(&hl, "highlight"), (&gh, "ghost")] {
            let bx = a.get_item(key)?;
            if bx.is_none() {
                node.call_method0("hide")?;
            } else {
                gx.call_method1("set_box", (node, bx.get_item(0)?.call_method1("__sub__", (origin,))?, bx.get_item(1)?, bx.get_item(2)?))?;
                node.call_method0("show")?;
            }
        }
        s.setattr("_aim_txt", a.get_item("text")?)?;
        Ok(())
    }
    /// ほかの人（マルチプレイ）の体：MOD（VRM の見た目など）か、定義の色の箱
    fn _bodies(slf: &Bound<'_, Self>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let seen = PySet::empty_bound(py)?;
        let net = gd(&w, "net", none(py));
        let mut others: Vec<B<'_>> = vec![];
        if !net.is_none() {
            let o = gd(&net, "others", none(py));
            others = if o.is_callable() { o.call0()?.iter()?.collect::<R<_>>()? } else { o.call_method0("values")?.iter()?.collect::<R<_>>()? };
        }
        let mut list: Vec<(String, B<'_>)> = vec![("me".into(), ga(&w, "local_player")?)];
        for p in others {
            list.push((format!("p{}", ga(&p, "pid")?.str()?), p));
        }
        let bodies = ga(s, "bodies")?;
        let game = ga(s, "game")?;
        let errs = ga(s, "_addon_err")?;
        let ov = v3(origin)?;
        for (key, p) in list {
            seen.add(&key)?;
            let mut b = bodies.call_method1("get", (&key,))?;
            let defid = get(&ga(&p, "defn")?, "id", py.None())?;
            if !b.is_none() {
                let bd = if b.hasattr("_defid")? { ga(&b, "_defid")? } else { none(py) };
                if !(bd.is_none() || bd.eq(&defid)?) {
                    b.call_method0("destroy")?; // 見た目を変えた：作り直す
                    bodies.set_item(&key, py.None())?;
                    b = none(py);
                }
            }
            if b.is_none() {
                let f = acall(py, "player_body", vec![p.clone().unbind()])?;
                let mb = Self::_each_addon(slf, &f, true)?; // MOD が体の見た目を引き受ける
                if !mb.is_none() {
                    bodies.set_item(&key, &mb)?;
                    b = mb;
                }
            }
            if b.is_none() {
                b = imp(py, "main.client.game")?.getattr("BoxBody")?.call1((&game, &p))?; // MOD が引き受けなければ定義の色の箱
                bodies.set_item(&key, &b)?;
            }
            if !b.hasattr("_defid")? {
                if let Err(e) = b.setattr("_defid", &defid) {
                    if !e.is_instance_of::<pyo3::exceptions::PyAttributeError>(py) {
                        return Err(e);
                    }
                }
            }
            let shown = if key == "me" {
                // 自分：三人称・カメラモードのときだけ
                ga(s, "third_person")?.is_truthy()? || !ga(&game, "cam_mode")?.is_none()
            } else {
                let q = sub(v3(&ga(&p, "pos")?)?, ov);
                let sh = dot(q, q) < 500.0f64.powi(2);
                s.call_method1("_label", (&key, &p, origin, sh))?;
                sh
            };
            if let Err(e) = b.call_method1("update", (origin, shown, 1.0 / 60.0)) {
                if !is_exc(py, &e) {
                    return Err(e);
                }
                let k = PyTuple::new_bound(py, [tb(py, "body"), b.get_type().getattr("__name__")?]);
                if !errs.contains(&k)? {
                    errs.call_method1("add", (&k,))?;
                    log_exc(py, &e, "体の見た目でエラー", vec![])?;
                }
            }
        }
        let keys: Vec<B<'_>> = bodies.iter()?.collect::<R<_>>()?;
        for k in keys {
            if !seen.contains(&k)? {
                bodies.call_method1("pop", (&k,))?.call_method0("destroy")?;
            }
        }
        let labels = ga(s, "labels")?;
        let keys: Vec<B<'_>> = labels.iter()?.collect::<R<_>>()?;
        for k in keys {
            if !seen.contains(&k)? || k.eq("me")? {
                labels.call_method1("pop", (&k,))?.call_method0("remove")?;
            }
        }
        Ok(())
    }
    /// ほかの人の頭の上の名前（いつもカメラの方を向く）
    fn _label(slf: &Bound<'_, Self>, key: &B<'_>, p: &B<'_>, origin: &B<'_>, shown: bool) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let labels = ga(s, "labels")?;
        let mut t = labels.call_method1("get", (key,))?;
        if t.is_none() {
            t = rs_text(py, ga(s, "root")?, false, Some(BG), None)?;
            labels.set_item(key, &t)?;
        }
        let nm = ga(p, "name")?;
        let nm = if nm.is_truthy()? { nm } else { tb(py, "?") };
        let wh = t.call_method1("set", (nm, 0.22))?;
        let (w_, h_) = (f64of(&wh.get_item(0)?)?, f64of(&wh.get_item(1)?)?);
        t.call_method1("show", (shown,))?;
        if !shown {
            return Ok(());
        }
        let lv = Self::look_vectors(slf)?;
        let (right, look) = (v3(&lv.get_item(1)?)?, v3(&lv.get_item(3)?)?);
        let cu = cross(look, right); // カメラの上
        let c = sub(add(v3(&ga(p, "pos")?)?, mul(v3(&ga(p, "up")?)?, f64of(&ga(p, "height")?)? - f64of(&ga(p, "radius")?)? + 0.35)), v3(origin)?);
        let o = add(sub(c, mul(right, w_ / 2.0)), mul(cu, h_ / 2.0));
        let m = vec![right[0], right[1], right[2], 0.0, cu[0], cu[1], cu[2], 0.0, -look[0], -look[1], -look[2], 0.0, o[0], o[1], o[2], 1.0];
        ga(&t, "node")?.call_method1("setMat", (m,))?;
        Ok(())
    }
    /// 画面下のアイテムバー（アイコン・選んでいる枠は黄色。今までの画面と同じ並べ方）
    fn _hotbar(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let slots: i64 = imp(py, "main.client.inventory")?.getattr("SLOTS")?.extract()?;
        let u = imp(py, "main.client_rs.ui")?;
        let w = ga(s, "w")?;
        let hb = ga(&ga(&w, "player")?, "hotbar")?;
        let items: Vec<B<'_>> = hb.iter()?.collect::<R<_>>()?;
        let mut allowed = vec![];
        for it in &items {
            allowed.push(it.is_truthy()? && w.call_method1("item_allowed", (it,))?.is_truthy()?);
        }
        let sel = ga(s, "sel")?;
        let state = PyTuple::new_bound(py, [PyTuple::new_bound(py, &items).into_any(), sel.clone(), PyTuple::new_bound(py, &allowed).into_any()]);
        let cur = ga(s, "hotbar")?;
        if !cur.is_none() && cur.get_item(0)?.eq(&state)? {
            return Ok(());
        }
        if !cur.is_none() {
            cur.get_item(1)?.call_method0("removeNode")?;
        }
        let gx = gfx(py)?;
        let root = gx.getattr("RsNode")?.call1((py.None(), "hotbar"))?;
        let (sw, gap, y) = (0.07, 0.008, -0.448);
        let selk: i64 = sel.extract()?;
        let view = gview(py)?;
        for k in 0..slots {
            let x = (k as f64 - (slots - 1) as f64 / 2.0) * (sw + gap);
            let sz = sw * if k == selk { 1.12 } else { 1.0 };
            let col = if k == selk { (1.0, 0.82, 0.25, 0.9) } else { (0.0, 0.0, 0.0, 0.5) };
            u.call_method1("rect_mesh", (&root, x - sz / 2.0, y - sz / 2.0, x + sz / 2.0, y + sz / 2.0, col))?;
            let it = &items[k as usize];
            if it.is_truthy()? {
                let tex = u.call_method1("item_tex", (ga(&w, "registry")?, it))?;
                let m = u.call_method1("rect_mesh", (&root, x - sw * 0.44, y - sw * 0.44, x + sw * 0.44, y + sw * 0.44,
                    (1.0, 1.0, 1.0, if allowed[k as usize] { 1.0 } else { 0.18 }), tex))?;
                view.call_method1("mesh_props", (m.getattr("mid")?, true, false, true, 0, 11))?;
            }
            let t = rs_text(py, root.clone(), true, None, Some(11))?;
            t.call_method1("set", ((k + 1).to_string(), 0.0138, (1.0, 1.0, 1.0, 0.8)))?;
            t.call_method1("place", (x - sw / 2.0 + 0.005, y + sw / 2.0 - 0.003))?;
        }
        if !gd(s, "_hud_shown", tb(py, true)).is_truthy()? {
            root.call_method0("hide")?;
        }
        s.setattr("hotbar", PyTuple::new_bound(py, [state.into_any(), root]))?;
        Ok(())
    }
    /// 画面の上の文字（左上：場所・高度・速度・重さ、左下：知らせ）。プログラムなどの画面を開いている間は隠す
    fn _overlay(slf: &Bound<'_, Self>, ww: f64, hh: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let cm = ga(&ga(s, "game")?, "cam_mode")?;
        let shown = ga(s, "editor")?.is_none() && (cm.is_none() || ga(&cm, "ui_on")?.is_truthy()?);
        let texts = [ga(s, "hud")?, ga(s, "msg")?, ga(s, "aim_text")?, ga(s, "item_text")?];
        if shown != gd(s, "_hud_shown", tb(py, true)).is_truthy()? {
            s.setattr("_hud_shown", shown)?;
            for t in &texts {
                t.call_method1("show", (shown,))?;
            }
            ga(&ga(s, "cross")?, "np")?.call_method0(if shown { "show" } else { "hide" })?;
            let hb = ga(s, "hotbar")?;
            if !hb.is_none() {
                hb.get_item(1)?.call_method0(if shown { "show" } else { "hide" })?;
            }
        }
        if !shown {
            return Ok(());
        }
        let p = ga(&w, "player")?;
        let pl = w.call_method1("nearest_planet", (ga(&p, "pos")?,))?;
        let pos = ga(&p, "pos")?;
        let alt = if pl.is_truthy()? { f64of(&pl.call_method1("sdf", (pos.get_item(py.None())?,))?.get_item(0)?)? } else { 0.0 };
        let pname = if pl.is_truthy()? { ga(&pl, "name")?.str()?.to_string() } else { "宇宙".into() };
        let vel = norm(v3(&ga(&p, "vel")?)?);
        let mut lines = vec![format!("{}  高度 {} m  速度 {} m/s", pname, pf(py, alt, ".1f")?, pf(py, vel, ".1f")?)];
        if ga(&p, "jetpack")?.is_truthy()? {
            lines.push("ジェットパック ON".into());
        }
        let seated = ga(&p, "seated")?;
        let cs = ga(&w, "constructs")?;
        if seated.is_truthy()? && cs.contains(seated.get_item(0)?)? {
            let c = cs.get_item(seated.get_item(0)?)?;
            let cv = norm(v3(&c.call_method0("velocity")?)?);
            lines.push(format!("操縦中: {}  速度 {} m/s", ga(&c, "name")?.str()?, pf(py, cv, ".1f")?));
        }
        let net = gd(&w, "net", none(py));
        if !net.is_none() && ga(&net, "role")?.eq("client")? {
            lines.push(format!("マルチプレイ参加中（ホスト {}）  {} 人", ga(&net, "host_name")?.str()?, 1 + ga(&net, "others")?.len()?));
        } else if !net.is_none() {
            let a = net.call_method0("addresses")?.get_item(pyo3::types::PySlice::new_bound(py, 0, 1, 1))?;
            let j = PyString::new_bound(py, "  ").call_method1("join", (a,))?;
            lines.push(format!("LAN に公開中 {}  {} 人", j.str()?, 1 + ga(&w, "remote")?.len()?));
        }
        let ft = gd(s, "fps_text", tb(py, ""));
        if ft.is_truthy()? && gd(s, "debug_hud", tb(py, false)).is_truthy()? {
            // F3：処理の重さ
            lines.push(ft.str()?.to_string());
        }
        if gd(s, "show_help", tb(py, false)).is_truthy()? {
            // F1：操作の説明
            let help: String = imp(py, "main.client.game")?.getattr("HELP")?.extract()?;
            lines.push(help.trim_end().to_string());
            for a in ga(s, "addons")?.iter()? {
                let ht = gd(&a?, "help_text", tb(py, ""));
                if ht.is_truthy()? {
                    lines.push(ht.str()?.to_string());
                }
            }
        }
        if Self::_touch_mode(slf)? {
            // タッチ操作：画面のボタンとジョイスティック
        } else if !ga(s, "grabbed")?.is_truthy()? && !ga(&ga(s, "ui")?, "stack")?.is_truthy()? {
            lines.push("クリックで操作（マウスで見回す）・Esc でメニュー・I でインベントリ".into());
        }
        let aspect = ww / hh.max(1.0);
        let (hud, msg, aim_text, item_text) = (&texts[0], &texts[1], &texts[2], &texts[3]);
        hud.call_method1("set", (lines.join("\n"), 0.026))?;
        hud.call_method1("place", (-aspect / 2.0 + 0.015, 0.5 - 0.015))?;
        let now: f64 = imp(py, "time")?.call_method0("time")?.extract()?;
        let all: Vec<B<'_>> = ga(&w, "msgs")?.iter()?.collect::<R<_>>()?;
        let mut msgs: Vec<String> = vec![];
        for m in all.iter().skip(all.len().saturating_sub(5)) {
            if now - f64of(&m.get_item(0)?)? < 8.0 {
                msgs.push(m.get_item(1)?.str()?.to_string());
            }
        }
        let k = PyDict::new_bound(py);
        k.set_item("max_width", aspect * 0.6)?;
        let h = f64of(&msg.call_method("set", (msgs.join("\n"), 0.024), Some(&k))?.get_item(1)?)?;
        msg.call_method1("show", (!msgs.is_empty(),))?;
        msg.call_method1("place", (-aspect / 2.0 + 0.015, -0.5 + 0.115 + h))?; // アイテムバーより上
        let at = gd(s, "_aim_txt", tb(py, ""));
        let k = PyDict::new_bound(py);
        k.set_item("max_width", aspect * 0.5)?;
        let tw = f64of(&aim_text.call_method("set", (&at, 0.022), Some(&k))?.get_item(0)?)?;
        aim_text.call_method1("show", (at.is_truthy()?,))?;
        aim_text.call_method1("place", (-tw / 2.0, -0.03))?;
        let it = imp(py, "main.client.play")?.call_method1("item_text", (&w, ga(&p, "hotbar")?.get_item(ga(s, "sel")?)?, ga(s, "sel")?, ga(s, "dir_index")?))?;
        let r = item_text.call_method1("set", (it, 0.022))?;
        let (tw, th) = (f64of(&r.get_item(0)?)?, f64of(&r.get_item(1)?)?);
        item_text.call_method1("place", (-tw / 2.0, -0.5 + 0.13 + th))?;
        Self::_hotbar(slf)?;
        let st: Vec<B<'_>> = ga(&ga(s, "ui")?, "stack")?.iter()?.collect::<R<_>>()?;
        for x in st {
            x.call_method1("update", (0.0,))?;
        }
        Ok(())
    }
    fn _render(slf: &Bound<'_, Self>, origin: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let w = ga(s, "w")?;
        let views = ga(s, "views")?;
        let cons = ga(&w, "constructs")?;
        let vk: Vec<B<'_>> = views.iter()?.collect::<R<_>>()?;
        for cid in vk {
            if !cons.contains(&cid)? {
                views.call_method1("pop", (&cid,))?.call_method0("remove")?;
            }
        }
        let loaded = ga(s, "loaded")?;
        loaded.call_method1("intersection_update", (py.import_bound("builtins")?.getattr("set")?.call1((&cons,))?,))?;
        let g = ga(s, "cfg")?.get_item("graphics")?;
        let far = f64of(&get(&g, "view_distance", 3000)?)?;
        let decal_dist = f64of(&get(&g, "decal_distance", 30.0)?)?;
        let ld = get(&g, "load_distance", 0)?;
        let load = if ld.is_truthy()? { f64of(&ld)? } else { 0.0 };
        let o = v3(origin)?;
        let anchor = ga(s, "anchor")?;
        if anchor.is_none() || norm(sub(o, v3(&anchor)?)) > 256.0 {
            s.setattr("anchor", np3(py, o))?;
            let av: i64 = ga(s, "anchor_ver")?.extract()?;
            s.setattr("anchor_ver", av + 1)?;
        }
        let an = ga(s, "anchor")?;
        let a3 = v3(&an)?;
        let d0 = sub(a3, o);
        ga(s, "static_root")?.call_method1("setPos", (d0[0], d0[1], d0[2]))?;
        let ba = w.call_method0("_bounds_arrays")?;
        let cs: Vec<B<'_>> = ba.get_item(0)?.iter()?.collect::<R<_>>()?;
        let cc = ba.get_item(1)?;
        let rr = ba.get_item(3)?;
        let mut dists = vec![];
        if !cs.is_empty() {
            let np = imp(py, "numpy")?;
            let cr: numpy::PyReadonlyArray2<f64> = np.call_method1("ascontiguousarray", (&cc, np.getattr("float64")?))?.extract()?;
            let rrr: numpy::PyReadonlyArray1<f64> = np.call_method1("ascontiguousarray", (&rr, np.getattr("float64")?))?.extract()?;
            let ca = cr.as_array();
            let ra = rrr.as_array();
            for i in 0..cs.len() {
                let d = [ca[[i, 0]] - o[0], ca[[i, 1]] - o[1], ca[[i, 2]] - o[2]];
                dists.push((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - ra[i]);
            }
        }
        let player = ga(&w, "player")?;
        let seatv = ga(&player, "seated")?;
        let seat = if seatv.is_truthy()? { seatv.get_item(0)? } else { none(py) };
        let lod_list = PyList::empty_bound(py);
        let fno: i64 = gd(s, "frame_no", tb(py, 0)).extract::<i64>()? + 1;
        s.setattr("frame_no", fno)?;
        let time = imp(py, "time")?;
        let budget = f64of(&time.call_method0("perf_counter")?)? + 0.008;
        let shape = imp(py, "main.shape")?;
        let rm = render_mod(py)?;
        let root = ga(s, "root")?;
        let static_root = ga(s, "static_root")?;
        let anchor_ver = ga(s, "anchor_ver")?;
        let hash = py.import_bound("builtins")?.getattr("hash")?;
        for (c, dist) in cs.iter().zip(dists.iter()) {
            let cid = ga(c, "id")?;
            if !cons.call_method1("get", (&cid,))?.is(c) {
                continue;
            }
            let mut v = views.call_method1("get", (&cid,))?;
            if *dist > far {
                if !v.is_none() {
                    v.call_method1("set_visible", (false,))?;
                }
                continue;
            }
            let seated = seat.eq(&cid)?;
            let near = load <= 0.0 || seated || *dist < load || (loaded.contains(&cid)? && *dist < load * 1.1);
            if !near {
                if !v.is_none() {
                    views.call_method1("pop", (&cid,))?.call_method0("remove")?;
                    c.setattr("_mesh_cache", py.None())?;
                }
                loaded.call_method1("discard", (&cid,))?;
                lod_list.append(c)?;
                continue;
            }
            if v.is_none() {
                if !seated && ga(c, "n_vox")?.extract::<i64>()? > 150 && !shape.call_method1("request_mesh", (c,))?.is_truthy()? {
                    lod_list.append(c)?;
                    continue;
                }
                if f64of(&time.call_method0("perf_counter")?)? > budget && !loaded.contains(&cid)? && !seated {
                    lod_list.append(c)?;
                    continue;
                }
                v = rm.getattr("ConstructView")?.call1((&root, py.None(), c))?;
                views.set_item(&cid, &v)?;
            }
            loaded.call_method1("add", (&cid,))?;
            v.call_method1("set_visible", (true,))?;
            let tf = c.call_method0("render_transform")?;
            let static_ = !tf.is_none() && !seated && (ga(c, "is_static")?.is_truthy()? || ga(c, "asleep")?.is_truthy()?)
                && !c.call_method1("dev_ids", ("thruster",))?.is_truthy()?;
            let key = if static_ { PyTuple::new_bound(py, [ga(c, "version")?, tf.get_item(2)?, anchor_ver.clone()]).into_any() } else { none(py) };
            let vd = ga(&v, "__dict__")?;
            if key.is_none() || !get(&vd, "_static_key", py.None())?.eq(&key)? {
                v.setattr("_static_key", &key)?;
                if static_ {
                    let k = PyDict::new_bound(py);
                    k.set_item("static_root", &static_root)?;
                    k.set_item("anchor", &an)?;
                    v.call_method("update", (origin,), Some(&k))?;
                } else {
                    v.call_method1("update", (origin,))?;
                }
                v.call_method1("update_decals", (origin, origin, decal_dist))?;
            }
            let hv: i64 = hash.call1((&cid,))?.extract()?;
            if key.is_none() || (fno as i128 + hv as i128).rem_euclid(6) == 0 {
                // 看板（止まっている物は6フレームに1回）
                v.call_method1("update_displays", (origin, far))?;
            }
        }
        ga(s, "lod")?.call_method1("update", (lod_list, origin))?;
        let lc = PyList::empty_bound(py);
        for k in loaded.iter()? {
            let k = k?;
            if cons.contains(&k)? {
                lc.append(cons.get_item(&k)?)?;
            }
        }
        ga(s, "lights")?.call_method1("update", (lc, origin, f64of(&ga(&w, "time")?)?))?;
        let ppos = ga(&player, "pos")?;
        let npl = w.call_method1("nearest_planet", (&ppos,))?;
        let carve = ga(s, "carve")?;
        carve.call_method1("update", (&npl, &ppos, origin))?;
        let patch = ga(s, "patch")?;
        patch.call_method1("update", (&npl, &ppos, origin, &carve))?;
        for pv in ga(s, "planet_views")?.iter()? {
            let pv = pv?;
            pv.call_method1("update", (origin, &ppos, patch.call_method1("frame_for", (ga(&pv, "p")?,))?))?;
        }
        let now = time.call_method0("time")?;
        for x in ga(s, "scenes")?.iter()? {
            let x = x?;
            let (a, sv) = (x.get_item(0)?, x.get_item(1)?);
            let on = gd(&a, "alert", tb(py, false)).is_truthy()? || gd(&a, "lights", tb(py, false)).is_truthy()?;
            sv.call_method1("update", (origin, on, &now))?;
        }
        // 空の色
        let mut air = 0.0;
        if !npl.is_none() {
            let p = (f64of(&get(&ga(&npl, "defn")?, "atmosphere_pressure", 101.3)?)? / 101.3).clamp(0.0, 1.0);
            let rad = f64of(&ga(&npl, "radius")?)?;
            let alt = (norm(sub(o, v3(&ga(&npl, "center")?)?)) - rad).max(0.0);
            air = p * (-alt / (rad * 0.25).max(30.0)).exp();
        }
        let c: Vec<f64> = (0..3).map(|i| SKY_SPACE[i] + (SKY_DAY[i] - SKY_SPACE[i]) * air).collect();
        ga(s, "view")?.call_method1("set_clear", (c[0], c[1], c[2]))?;
        Ok(())
    }
    /// 画面から消す（メニューに戻るとき）
    fn destroy(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let snd = ga(s, "snd")?;
        if !snd.is_none() {
            snd.call_method1("set_bgm", (py.None(),))?;
            snd.call_method1("set_override", (py.None(),))?;
            snd.call_method0("stop_loops")?;
            snd.call_method1("update", (10.0,))?; // 曲を止める
        }
        let f = acall(py, "destroy", vec![])?;
        Self::_each_addon(slf, &f, false)?;
        s.setattr("addons", PyList::empty_bound(py))?;
        let r = ga(&ga(s, "game")?, "touch_hud").and_then(|h| h.call_method0("destroy"));
        swallow(py, r, |_| Ok(()))?;
        for b in ga(s, "bodies")?.call_method0("values")?.iter()? {
            let b = b?;
            swallow(py, b.call_method0("destroy"), |_| Ok(()))?;
        }
        s.setattr("bodies", PyDict::new_bound(py))?;
        for t in ga(s, "labels")?.call_method0("values")?.iter()? {
            t?.call_method0("remove")?;
        }
        s.setattr("labels", PyDict::new_bound(py))?;
        for v in ga(s, "views")?.call_method0("values")?.iter()? {
            v?.call_method0("remove")?;
        }
        s.setattr("views", PyDict::new_bound(py))?;
        for n in [ga(s, "root")?, ga(&ga(s, "hud")?, "node")?, ga(&ga(s, "msg")?, "node")?, ga(&ga(s, "aim_text")?, "node")?,
                  ga(&ga(s, "item_text")?, "node")?, ga(&ga(s, "cross")?, "np")?] {
            n.call_method0("removeNode")?;
        }
        let hb = ga(s, "hotbar")?;
        if !hb.is_none() {
            hb.get_item(1)?.call_method0("removeNode")?;
        }
        let st: Vec<B<'_>> = ga(&ga(s, "ui")?, "stack")?.iter()?.collect::<R<_>>()?;
        for x in st {
            x.call_method0("close")?;
        }
        let ed = ga(s, "editor")?;
        if !ed.is_none() && ed.hasattr("close")? {
            swallow(py, ed.call_method0("close"), |_| Ok(()))?;
        }
        ga(s, "shim")?.call_method1("destroy", (ga(&ga(s, "game")?, "ui_root")?,))?;
        ga(s, "view")?.call_method1("grab", (false,))?;
        Ok(())
    }
    fn run(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let t = imp(py, "time")?;
        let mut last: f64 = t.call_method0("perf_counter")?.extract()?;
        while !ga(s, "quit")?.is_truthy()? {
            let now: f64 = t.call_method0("perf_counter")?.extract()?;
            let dt = (now - last).min(0.1);
            last = now;
            s.call_method1("frame", (dt,))?;
        }
        ga(s, "w")?.call_method0("save")?;
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

/// カメラモードに渡す「押しているキー」（ursina の held_keys の形）
#[pyclass(module = "alpedia_core", name = "_Held", dict)]
pub struct Held {
    #[pyo3(get, set)]
    keys: Py<PySet>,
}
impl Held {
    fn make(v: &B<'_>) -> R<Self> {
        let py = v.py();
        let keys = PySet::empty_bound(py)?;
        for k in ga(v, "held")?.iter()? {
            let k: String = k?.extract()?;
            keys.add(keymap(&k))?;
        }
        Ok(Held { keys: keys.unbind() })
    }
}
#[pymethods]
impl Held {
    #[new]
    fn new(v: &B<'_>) -> R<Self> {
        Held::make(v)
    }
    fn __getitem__(&self, py: Python<'_>, k: &B<'_>) -> R<i64> {
        Ok(if self.keys.bind(py).contains(k)? { 1 } else { 0 })
    }
}

/// カメラモードのドラッグ：マウスの左ボタンを押している間、指1本として位置を返す
#[pyclass(module = "alpedia_core", name = "_Pointers", dict)]
pub struct Pointers {}
#[pymethods]
impl Pointers {
    #[new]
    fn new() -> Self {
        Pointers {}
    }
    fn current<'py>(&self, py: Python<'py>) -> R<Bound<'py, PyDict>> {
        let m = shim(py)?.getattr("mouse")?;
        let d = PyDict::new_bound(py);
        if ga(&m, "left")?.is_truthy()? {
            d.set_item(0, (ga(&m, "x")?, ga(&m, "y")?))?;
        }
        Ok(d)
    }
}

/// 画面のボタンを作れなかったときの代わり（何も出さない）
#[pyclass(module = "alpedia_core", name = "_TouchStub", dict)]
pub struct TouchStub {
    #[pyo3(get, set)]
    ptr: PyObject,
    #[pyo3(get, set)]
    stick: PyObject,
}
#[pymethods]
impl TouchStub {
    #[new]
    fn new(py: Python<'_>) -> R<Self> {
        Ok(TouchStub { ptr: Py::new(py, Pointers {})?.into_any(), stick: imp(py, "numpy")?.call_method1("zeros", (2,))?.unbind() })
    }
    fn _release_all(&self) {}
    fn update(&self, _dt: &B<'_>) {}
    fn take_look<'py>(&self, py: Python<'py>) -> R<B<'py>> {
        imp(py, "numpy")?.call_method1("zeros", (2,))
    }
    fn over_ui(&self, _x: &B<'_>, _y: &B<'_>) -> bool {
        false
    }
    fn mouse_over_ui(&self) -> bool {
        false
    }
    fn destroy(&self) {}
}

// ---------------------------------------------------------------- GameAdapter
/// 今までの画面の部品（ScriptPanel・BlockEditor・DevicePanel など）が使う game のまね
#[pyclass(module = "alpedia_core", subclass, dict)]
pub struct GameAdapter {
    #[pyo3(get, set)]
    v: PyObject,
}

#[pymethods]
impl GameAdapter {
    #[new]
    fn new(py: Python<'_>, viewer: &B<'_>) -> R<Self> {
        let _ = py;
        Ok(GameAdapter { v: viewer.clone().unbind() })
    }
    fn __init__(slf: &Bound<'_, Self>, viewer: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let u = shim(py)?;
        slf.borrow_mut().v = viewer.clone().unbind();
        s.setattr("world", ga(viewer, "w")?)?;
        s.setattr("cfg", ga(viewer, "cfg")?)?;
        let d = PyDict::new_bound(py);
        for (k, n) in [("Entity", "Entity"), ("Text", "Text"), ("Button", "Button"), ("camera", "camera"), ("window", "window"), ("color", "color"),
                       ("held_keys", "held_keys"), ("mouse", "mouse"), ("utime", "time"), ("application", "application")] {
            d.set_item(k, u.getattr(n)?)?;
        }
        s.setattr("U", d)?;
        let k = PyDict::new_bound(py);
        k.set_item("parent", u.getattr("camera")?.getattr("ui")?)?;
        s.setattr("ui_root", u.getattr("Entity")?.call((), Some(&k))?)?;
        s.setattr("scene_root", ga(viewer, "root")?)?; // MOD の 3D の物（DynamicMesh・SceneView）を付ける所
        s.setattr("hold", false)?; // MOD が世界の時間を止める（ムービーなど）
        s.setattr("cam_mode", py.None())?;
        s.setattr("emotes", PyDict::new_bound(py))?;
        s.setattr("leaving", false)?;
        s.setattr("load_errors", PyList::empty_bound(py))?;
        s.setattr("touch_hud", Py::new(py, TouchStub::new(py)?)?)?;
        s.setattr("vkeys", PyDict::new_bound(py))?;
        s.setattr("mouse_captured", false)?;
        Ok(())
    }
    #[getter]
    fn addons<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        Ok(gd(&v, "addons", PyList::empty_bound(py).into_any()))
    }
    #[getter]
    fn third_person<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        ga(slf.borrow().v.bind(py), "third_person")
    }
    #[setter]
    fn set_third_person(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let py = slf.py();
        slf.borrow().v.bind(py).setattr("third_person", on.is_truthy()?)
    }
    fn _camera_hook(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        let f = acall(py, "on_camera", vec![on.clone().unbind()])?;
        v.call_method1("_each_addon", (f,))?;
        Ok(())
    }
    /// C：カメラモード（今までの画面の CameraMode を ursina のまねで）
    fn enter_camera(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        if !ga(s, "cam_mode")?.is_none() || !ga(s, "editor")?.is_none() || ga(&ga(&v, "ui")?, "stack")?.is_truthy()?
            || ga(&ga(&ga(s, "world")?, "player")?, "seated")?.is_truthy()? {
            return Ok(());
        }
        ga(&v, "held")?.call_method0("clear")?;
        s.setattr("cam_mode", imp(py, "main.client.camera_mode")?.getattr("CameraMode")?.call1((s,))?)?;
        s.call_method1("_camera_hook", (true,))?;
        Ok(())
    }
    /// F8：読み込み（設計図・OBJ）
    fn open_import(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let cls = imp(py, "main.client.import_ui")?.getattr("ImportPanel")?;
        let f = imp(py, "functools")?.getattr("partial")?.call1((cls, slf))?;
        slf.call_method1("_open_panel", (f,))?;
        Ok(())
    }
    /// 操作とキー設定
    fn open_keys(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let extra = PyList::empty_bound(py);
        for a in Self::addons(slf)?.iter()? {
            let ht = ga(&a?, "help_text")?;
            if ht.is_truthy()? {
                extra.append(ht)?;
            }
        }
        let cls = imp(py, "main.client.keys_panel")?.getattr("KeysPanel")?;
        let f = imp(py, "functools")?.getattr("partial")?.call1((cls, ga(s, "U")?, ga(s, "cfg")?, ga(&ga(s, "ui_root")?, "parent")?, ga(s, "editor_closed")?, extra))?;
        s.call_method1("_open_panel", (f,))?;
        Ok(())
    }
    #[getter]
    fn paused(slf: &Bound<'_, Self>) -> R<bool> {
        let py = slf.py();
        ga(&ga(slf.borrow().v.bind(py), "ui")?, "stack")?.is_truthy()
    }
    fn look_vectors<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        slf.borrow().v.bind(py).call_method0("look_vectors")
    }
    fn exit_camera(slf: &Bound<'_, Self>) -> R<()> {
        let s = slf.as_any();
        let cm = ga(s, "cam_mode")?;
        if cm.is_none() {
            return Ok(());
        }
        cm.call_method0("destroy")?;
        s.setattr("cam_mode", s.py().None())?;
        s.call_method1("_camera_hook", (false,))?;
        ga(s, "ui_root")?.setattr("enabled", true)?;
        s.call_method1("set_mouse_capture", (true,))?;
        Ok(())
    }
    /// MOD の画面（今までの ursina の部品）を開く
    fn _open_panel(slf: &Bound<'_, Self>, make: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.call_method1("set_mouse_capture", (false,))?;
        ga(slf.borrow().v.bind(py), "held")?.call_method0("clear")?;
        ga(s, "ui_root")?.setattr("enabled", false)?;
        match make.call0() {
            Ok(e) => s.setattr("editor", e)?,
            Err(e) if is_exc(py, &e) => {
                log_exc(py, &e, "画面を開けませんでした", vec![])?;
                let ed = gd(s, "editor", none(py));
                let root = gd(&ed, "root", none(py));
                s.setattr("editor", py.None())?;
                if !root.is_none() {
                    shim(py)?.call_method1("destroy", (root,))?;
                }
                s.call_method0("editor_closed")?;
                ga(s, "world")?.call_method1("message", (format!("⚠ 画面を開けませんでした: {}", e.value_bound(py).str()?),))?;
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }
    /// ワールドを出る（action："menu"・"play:名前"）
    #[pyo3(signature = (action, save=true, status=""))]
    fn _leave(slf: &Bound<'_, Self>, action: &B<'_>, save: bool, status: &str) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if ga(s, "leaving")?.is_truthy()? {
            return Ok(());
        }
        s.setattr("leaving", true)?;
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        v.setattr("leave_action", (action, save, status))?;
        v.setattr("quit", true)?;
        Ok(())
    }
    #[getter]
    fn sel<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        ga(slf.borrow().v.bind(slf.py()), "sel")
    }
    #[setter]
    fn set_sel(slf: &Bound<'_, Self>, k: &B<'_>) -> R<()> {
        slf.borrow().v.bind(slf.py()).setattr("sel", k)
    }
    #[getter]
    fn editor<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        ga(slf.borrow().v.bind(slf.py()), "editor")
    }
    #[setter]
    fn set_editor(slf: &Bound<'_, Self>, e: &B<'_>) -> R<()> {
        slf.borrow().v.bind(slf.py()).setattr("editor", e)
    }
    fn editor_closed(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        v.setattr("editor", py.None())?;
        ga(slf.as_any(), "ui_root")?.setattr("enabled", true)?;
        v.call_method0("screen_closed")?;
        Ok(())
    }
    fn set_mouse_capture(slf: &Bound<'_, Self>, on: &B<'_>) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method1("set_grab", (on,))?;
        Ok(())
    }
    // ---- 画面のボタン（touch_ui.TouchHUD）が使う口 ----
    #[getter]
    fn hit<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        ga(slf.borrow().v.bind(slf.py()), "hit")
    }
    fn _usable<'py>(slf: &Bound<'py, Self>, hit: &B<'py>) -> R<B<'py>> {
        slf.borrow().v.bind(slf.py()).call_method1("_usable", (hit,))
    }
    /// 画面のボタンがキーを押した（ursina のキーの名前）
    fn input(slf: &Bound<'_, Self>, key: &B<'_>) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        v.call_method1("_key", (key, ga(slf.as_any(), "cfg")?.get_item("controls")?))?;
        Ok(())
    }
    fn do_remove(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method1("_click", (0,))?;
        Ok(())
    }
    fn do_place(slf: &Bound<'_, Self>) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method1("_click", (1,))?;
        Ok(())
    }
    #[pyo3(signature = (hit=None))]
    fn open_editor(slf: &Bound<'_, Self>, hit: Option<&B<'_>>) -> R<()> {
        let py = slf.py();
        let v = slf.borrow().v.clone_ref(py).into_bound(py);
        if let Some(h) = hit.filter(|h| !h.is_none()) {
            v.setattr("hit", h)?;
        }
        v.call_method0("open_editor")?;
        Ok(())
    }
    #[pyo3(signature = (radius=2.6))]
    fn nearby_targets<'py>(slf: &Bound<'py, Self>, radius: f64) -> R<B<'py>> {
        slf.borrow().v.bind(slf.py()).call_method1("nearby_targets", (radius,))
    }
    #[pyo3(signature = (t, settings=false))]
    fn do_target(slf: &Bound<'_, Self>, t: &B<'_>, settings: bool) -> R<()> {
        let py = slf.py();
        if settings || get(t, "kind", py.None())?.eq("p")? {
            slf.call_method1("open_editor", (t.get_item("hit")?,))?;
        } else {
            slf.borrow().v.bind(py).call_method1("do_interact", (t.get_item("hit")?,))?;
        }
        Ok(())
    }
    fn touch_action(slf: &Bound<'_, Self>, kind: &B<'_>, x: &B<'_>, y: &B<'_>) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method1("touch_action", (kind, x, y))?;
        Ok(())
    }
    #[pyo3(signature = (name, volume=1.0))]
    fn sfx(slf: &Bound<'_, Self>, name: &B<'_>, volume: f64) -> R<()> {
        slf.borrow().v.bind(slf.py()).call_method1("sfx", (name, volume))?;
        Ok(())
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    m.add_class::<AddonCall>()?;
    m.add_class::<Viewer>()?;
    m.add_class::<Held>()?;
    m.add_class::<Pointers>()?;
    m.add_class::<TouchStub>()?;
    m.add_class::<GameAdapter>()?;
    m.add_function(wrap_pyfunction_bound!(viewer_run, m)?)?;
    Ok(())
}
