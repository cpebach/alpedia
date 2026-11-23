//! 遊ぶ画面（Rust の alpedia_view で描く）：ワールドの描画・操作・HUD・MOD の画面側（main/client_rs/viewer.py）。
//!
//!   python run.py --world 名前
//!
//! 描く物：構築物（ブロック・遠景の箱・ライト・機器の記号・看板）、地表と掘った跡、惑星、景色の箱、空の色、
//! 人物（VRM・色の箱）。画面の上の部品（プログラムの画面・MOD の画面・メニューなど）は ursina のまね（ursina.rs）で動き、
//! 一時停止メニュー・インベントリ・機器の設定は ui.rs・screens*.rs。
//!
//! Viewer・GameAdapter の値は Python の属性（MOD やほかの部品が読み書きする）。処理は Rust。

use crate::game::PyPathOps;
use crate::game::PyDictOps;
use crate::game::world::{wld, World};
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyString, PyTuple};

use super::imp;
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

pub const SKY_DAY: [f64; 3] = [0.45, 0.65, 0.9];
/// 地平線のかすみの色と、空気遠近で見える距離の目安（m。海面で 1/e になる距離）
pub const HAZE: [f64; 3] = [0.78, 0.85, 0.93];
pub const FOG_VIS: f64 = 14000.0;
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
    d.dget2(k, dflt.into_py(py))
}
fn lg(py: Python<'_>) -> R<B<'_>> {
    imp(py, "main.log")?.dget1("client_rs")
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
/// MOD ごとの毎フレームの update の時間（秒の合計。ログに 1 フレームあたりで出す。A-22）
pub(crate) static MOD_PROF: std::sync::Mutex<Vec<(String, f64)>> = std::sync::Mutex::new(Vec::new());
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
pub struct Viewer {
    /// _addon_err（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_addon_err: Option<PyObject>,
    /// _aim_txt（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_aim_txt: Option<PyObject>,
    /// _alt_free（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_alt_free: Option<PyObject>,
    /// _cam（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_cam: Option<PyObject>,
    /// _hud_shown（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_hud_shown: Option<PyObject>,
    /// _perf_log（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_perf_log: Option<PyObject>,
    /// _snd_lit（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_snd_lit: Option<PyObject>,
    /// _snd_loops（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_snd_loops: Option<PyObject>,
    /// _snd_t（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_snd_t: Option<PyObject>,
    /// _third_before_seat（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_third_before_seat: Option<PyObject>,
    /// _was_seated（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_was_seated: Option<PyObject>,
    /// acc（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_acc: Option<PyObject>,
    /// addons（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_addons: Option<PyObject>,
    /// aim_text（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_aim_text: Option<PyObject>,
    /// anchor（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_anchor: Option<PyObject>,
    /// anchor_ver（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_anchor_ver: Option<PyObject>,
    /// bodies（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_bodies: Option<PyObject>,
    /// carve（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_carve: Option<PyObject>,
    /// cfg（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_cfg: Option<PyObject>,
    /// close_app（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_close_app: Option<PyObject>,
    /// cross（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_cross: Option<PyObject>,
    /// debug_hud（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_debug_hud: Option<PyObject>,
    /// dir_index（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_dir_index: Option<PyObject>,
    /// editor（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_editor: Option<PyObject>,
    /// fps_detail（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_fps_detail: Option<PyObject>,
    /// fps_text（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_fps_text: Option<PyObject>,
    /// frame_no（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_frame_no: Option<PyObject>,
    /// game（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_game: Option<PyObject>,
    /// ghost（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_ghost: Option<PyObject>,
    /// grabbed（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_grabbed: Option<PyObject>,
    /// held（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_held: Option<PyObject>,
    /// highlight（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_highlight: Option<PyObject>,
    /// hit（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_hit: Option<PyObject>,
    /// hotbar（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_hotbar: Option<PyObject>,
    /// hud（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_hud: Option<PyObject>,
    /// item_text（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_item_text: Option<PyObject>,
    /// labels（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_labels: Option<PyObject>,
    /// lights（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_lights: Option<PyObject>,
    /// load_errors（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_load_errors: Option<PyObject>,
    /// loaded（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_loaded: Option<PyObject>,
    /// lod（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_lod: Option<PyObject>,
    /// msg（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_msg: Option<PyObject>,
    /// patch（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_patch: Option<PyObject>,
    /// perf（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_perf: Option<PyObject>,
    /// pitch（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_pitch: Option<PyObject>,
    /// planet_views（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_planet_views: Option<PyObject>,
    /// quit（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_quit: Option<PyObject>,
    /// root（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_root: Option<PyObject>,
    /// scenes（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_scenes: Option<PyObject>,
    /// sel（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_sel: Option<PyObject>,
    /// shim（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_shim: Option<PyObject>,
    /// show_help（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_show_help: Option<PyObject>,
    /// snd（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_snd: Option<PyObject>,
    /// static_root（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_static_root: Option<PyObject>,
    /// third_person（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_third_person: Option<PyObject>,
    /// ui（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_ui: Option<PyObject>,
    /// view（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_view: Option<PyObject>,
    /// views（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_views: Option<PyObject>,
    /// w（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_w: Option<PyObject>,
    /// yaw_fwd（入れていなければ None。そのとき Python からは属性が無いのと同じ）
    pub a_yaw_fwd: Option<PyObject>,
}

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
        Viewer { a_addon_err: None, a_aim_txt: None, a_alt_free: None, a_cam: None, a_hud_shown: None, a_perf_log: None, a_snd_lit: None, a_snd_loops: None, a_snd_t: None, a_third_before_seat: None, a_was_seated: None, a_acc: None, a_addons: None, a_aim_text: None, a_anchor: None, a_anchor_ver: None, a_bodies: None, a_carve: None, a_cfg: None, a_close_app: None, a_cross: None, a_debug_hud: None, a_dir_index: None, a_editor: None, a_fps_detail: None, a_fps_text: None, a_frame_no: None, a_game: None, a_ghost: None, a_grabbed: None, a_held: None, a_highlight: None, a_hit: None, a_hotbar: None, a_hud: None, a_item_text: None, a_labels: None, a_lights: None, a_load_errors: None, a_loaded: None, a_lod: None, a_msg: None, a_patch: None, a_perf: None, a_pitch: None, a_planet_views: None, a_quit: None, a_root: None, a_scenes: None, a_sel: None, a_shim: None, a_show_help: None, a_snd: None, a_static_root: None, a_third_person: None, a_ui: None, a_view: None, a_views: None, a_w: None, a_yaw_fwd: None }
    }
    fn __init__(slf: &Bound<'_, Self>, world: &B<'_>, cfg: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        Self::sa_w(slf, world)?;
        Self::sa_cfg(slf, cfg)?;
        let g = cfg.get_item("graphics")?;
        let win = get(cfg, "window", PyDict::new_bound(py))?;
        shim(py)?.call_method0("install")?; // 今までの画面の部品（ursina）を、この画面で動かす
        let gx = gfx(py)?;
        let view = if gx.call_method0("active")?.is_truthy()? {
            gx.getattr("VIEW")?
        } else {
            gx.call_method1("start", (get(&win, "width", 1280)?, get(&win, "height", 720)?, "Alpedia", get(&win, "vsync", true)?))?
        };
        Self::sa_view(slf, &view)?;
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
        Self::sa_root(slf, &root)?;
        Self::sa_views(slf, PyDict::new_bound(py))?;
        let pvs = PyList::empty_bound(py);
        for p in World::g_planets(wld(&world)?)?.iter()? {
            pvs.append(rm.getattr("PlanetView")?.call1((&root, py.None(), p?))?)?;
        }
        Self::sa_planet_views(slf, pvs)?;
        let int = py.import_bound("builtins")?.getattr("int")?;
        Self::sa_patch(slf, rm.getattr("TerrainPatch")?.call1((&root, py.None(), int.call1((get(&g, "terrain_patch_cells", 48)?,))?,
            f64of(&get(&g, "terrain_patch_step", 1.0)?)?))?)?;
        Self::sa_carve(slf, rm.getattr("CarveView")?.call1((&root, py.None()))?)?;
        Self::sa_lod(slf, rm.getattr("LodLayer")?.call1((&root,))?)?;
        Self::sa_lights(slf, rm.getattr("LightLayer")?.call1((&root,))?)?;
        Self::sa_static_root(slf, node.call1((&root, "static"))?)?;
        Self::sa_anchor(slf, py.None())?;
        Self::sa_anchor_ver(slf, 0)?;
        Self::sa_loaded(slf, PySet::empty_bound(py)?)?;
        // 画面の上：左上の状態・左下の知らせ・照準
        gx.call_method1("font", (get(&g, "font", py.None())?,))?;
        Self::sa_hud(slf, rs_text(py, none(py), true, Some(BG), Some(10))?)?;
        Self::sa_msg(slf, rs_text(py, none(py), true, Some(BG), Some(10))?)?;
        let cross = gx.getattr("RsDynamicMesh")?.call1((node.call1((py.None(), "cross"))?, "cross"))?;
        view.call_method1("mesh_overlay", (cross.getattr("mid")?, true))?;
        gx.call_method1("quad", (&cross, -0.003, 0.003, 0.003, -0.003, (1, 1, 1, 0.9)))?;
        Self::sa_cross(slf, cross)?;
        // 手の操作：狙っている物の枠・置く場所の枠・説明・アイテムバー
        Self::sa_highlight(slf, gx.call_method1("wire_box", (&root, (1.0, 0.92, 0.2, 1.0), 0.025))?)?;
        Self::sa_ghost(slf, gx.call_method1("wire_box", (&root, (0.4, 1.0, 0.4, 1.0), 0.02))?)?;
        Self::sa_aim_text(slf, rs_text(py, none(py), true, Some(BG), Some(10))?)?;
        Self::sa_item_text(slf, rs_text(py, none(py), true, Some(BG), Some(10))?)?;
        Self::sa_hit(slf, py.None())?;
        Self::sa_sel(slf, 0)?;
        Self::sa_dir_index(slf, 0)?;
        Self::sa_bodies(slf, PyDict::new_bound(py))?;
        Self::sa_labels(slf, PyDict::new_bound(py))?;
        Self::sa_third_person(slf, false)?;
        Self::sa_was_seated(slf, false)?;
        Self::sa_debug_hud(slf, get(&g, "debug_hud", false)?.is_truthy()?)?;
        Self::sa_ui(slf, imp(py, "main.client_rs.ui")?.getattr("UI")?.call1((&view,))?)?;
        Self::sa_shim(slf, shim(py)?)?;
        Self::sa_editor(slf, py.None())?; // 今までの画面の部品（ursina のまね）で開いている画面（プログラムなど）
        let game = py.get_type_bound::<GameAdapter>().call1((s,))?;
        Self::sa_game(slf, &game)?;
        Self::sa_snd(slf, imp(py, "main.client.sound")?.dget1(cfg)?)?;
        Self::sa_snd_t(slf, 0.0)?;
        Self::sa_snd_lit(slf, PySet::empty_bound(py)?)?;
        Self::sa_snd_loops(slf, PyDict::new_bound(py))?;
        imp(py, "main.client.inventory")?.call_method1("ensure_hotbar", (World::g_player(wld(&world)?)?, World::g_registry(wld(&world)?)?))?;
        let addons = Self::_load_addons(slf).map(|v| IntoPy::<PyObject>::into_py(v, slf.py()).into_bound(slf.py()))?;
        Self::sa_addons(slf, &addons)?;
        let r = (|| -> R<()> {
            let th = imp(py, "main.client.touch_ui")?.getattr("TouchHUD")?.call1((&game,))?;
            game.setattr("touch_hud", th) // 画面のボタン・仮想ジョイスティック（今までの画面と同じ部品）
        })();
        swallow(py, r, |e| log_exc(py, e, "画面のボタンを作れません", vec![]))?;
        // 画面側の MOD が描かない拡張の景色の箱（画面側の無い MOD）はここで描く
        let own: Vec<B<'_>> = addons.iter()?.map(|a| a.map(|a| gd(&a, "pack", none(py)))).collect::<R<_>>()?;
        let scenes = PyList::empty_bound(py);
        let sv = imp(py, "main.client.scenery_view")?.getattr("SceneView")?;
        for a in World::g_addons(wld(&world)?)?.iter()? {
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
        Self::sa_scenes(slf, scenes)?;
        Self::sa_addon_err(slf, PySet::empty_bound(py)?)?;
        Self::sa_hotbar(slf, py.None())?;
        Self::sa_held(slf, PySet::empty_bound(py)?)?;
        Self::sa_yaw_fwd(slf, py.None())?;
        Self::sa_pitch(slf, 0.0)?;
        Self::sa_grabbed(slf, false)?;
        Self::sa_acc(slf, 0.0)?;
        Self::sa_quit(slf, false)?;
        let perf = PyDict::new_bound(py);
        perf.set_item("frames", 0)?;
        perf.set_item("t", imp(py, "time")?.call_method0("perf_counter")?)?;
        perf.set_item("phys", 0.0)?;
        perf.set_item("rend", 0.0)?;
        perf.set_item("ticks", 0)?;
        Self::sa_perf(slf, perf)?;
        Ok(())
    }

    // ---- MOD の画面側 ----
    /// MOD の画面側（ClientAddon）を作る（今までの画面と同じ modcode.client_addons）
    pub(crate) fn _load_addons<'py>(slf: &Bound<'py, Self>) -> R<B<'py>> {
        let py = slf.py();
        let mc = imp(py, "main.modcode")?;
        let errors = PyList::empty_bound(py);
        mc.call_method1("load_client", (&errors,))?;
        let game = Self::ga_game(slf)?;
        let out = mc.call_method1("client_addons", (&game, &errors))?;
        let w = Self::ga_w(slf)?;
        let sv = World::addon(wld(&w)?, "survival").map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?;
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
            World::message(wld(&w)?, &format!("⚠ {}", first)).map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?;
        }
        Self::sa_load_errors(slf, errors)?;
        Ok(out)
    }
    /// MOD の口を順に呼ぶ（エラーは1回だけ記録して続ける）。first：最初に真を返した物の値
    #[pyo3(signature = (f, first=false))]
    pub(crate) fn _each_addon<'py>(slf: &Bound<'py, Self>, f: &B<'py>, first: bool) -> R<B<'py>> {
        let py = slf.py();
        let addons: Vec<B<'_>> = Self::ga_addons(slf)?.iter()?.collect::<R<_>>()?;
        let timed = f.downcast::<AddonCall>().map(|x| x.borrow().name == "update").unwrap_or(false);
        for a in addons {
            let t = std::time::Instant::now();
            let r = f.call1((&a,));
            if timed {
                let dt = t.elapsed().as_secs_f64();
                let name: String = gd(&a, "name", tb(py, "?")).str()?.to_string();
                if let Ok(mut p) = MOD_PROF.lock() {
                    match p.iter_mut().find(|x| x.0 == name) {
                        Some(x) => x.1 += dt,
                        None => p.push((name, dt)),
                    }
                }
            }
            let r = match r {
                Ok(v) => v,
                Err(e) if is_exc(py, &e) => {
                    let key = (gd(&a, "name", tb(py, "?")), gd(f, "__name__", tb(py, "")));
                    let key = PyTuple::new_bound(py, [key.0, key.1]);
                    let errs = Self::ga_addon_err(slf)?;
                    if !errs.contains(&key)? {
                        errs.call_method1("add", (&key,))?;
                        log_exc(py, &e, "MOD '%s' の画面側でエラー", vec![key.get_item(0)?.unbind()])?;
                        Self::ga_w(slf)?.call_method1("message", (format!("⚠ MOD '{}' の画面でエラー（logs/latest.log）", key.get_item(0)?.str()?),))?;
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
    pub(crate) fn _usable(slf: &Bound<'_, Self>, hit: &B<'_>) -> R<bool> {
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
    pub(crate) fn nearby_targets<'py>(slf: &Bound<'py, Self>, radius: f64) -> R<B<'py>> {
        let py = slf.py();
        imp(py, "main.client.play")?.call_method1("nearby_targets", (Self::ga_w(slf)?, Self::ga_addons(slf)?, radius))
    }
    /// E：MOD が使わなければワールドの interact（機器・席）
    pub(crate) fn do_interact(slf: &Bound<'_, Self>, hit: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let f = acall(py, "interact", vec![hit.clone().unbind()])?;
        if !Self::_each_addon(slf, &f, true)?.is_truthy()? {
            let w = Self::ga_w(slf)?;
            let mut kind = none(py);
            if hit.is_truthy()? && get(hit, "kind", py.None())?.eq("voxel")? {
                let d = hit.get_item("construct")?.getattr("devs")?.get_item(hit.get_item("index")?)?;
                if d.is_truthy()? {
                    let dd = World::g_registry(wld(&w)?)?.call_method1("device", (d.get_item("def")?,))?;
                    let dd = if dd.is_truthy()? { dd } else { PyDict::new_bound(py).into_any() };
                    kind = get(&dd, "type", py.None())?;
                }
            }
            World::interact(wld(&w)?, &IntoPy::<PyObject>::into_py(hit, w.py()).into_bound(w.py())).map(|v| IntoPy::<PyObject>::into_py(v, w.py()).into_bound(w.py()))?;
            s.call_method1("sfx", (if kind.eq("seat")? { "seat" } else { "click" },))?;
        } else {
            s.call_method1("sfx", ("click",))?;
        }
        Ok(())
    }
    // ---- 音 ----
    #[pyo3(signature = (name, volume=1.0))]
    pub(crate) fn sfx(slf: &Bound<'_, Self>, name: &B<'_>, volume: f64) -> R<()> {
        let snd = ga(slf.as_any(), "snd")?;
        if !snd.is_none() {
            snd.call_method1("sfx", (name, volume))?;
        }
        Ok(())
    }
    /// BGM（MOD が決める）と、近くの推進機・アクチュエーター・ジェットパックの音（今までの画面と同じ）
    pub(crate) fn _update_sound(slf: &Bound<'_, Self>, dt: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let snd = Self::ga_snd(slf)?;
        if snd.is_none() {
            return Ok(());
        }
        let f = acall(py, "bgm", vec![])?;
        snd.call_method1("set_bgm", (Self::_each_addon(slf, &f, true)?,))?;
        let w = Self::ga_w(slf)?;
        if ga(&Self::ga_ui(slf)?, "stack")?.is_truthy()? && gd(&w, "net", none(py)).is_none() {
            snd.call_method0("stop_loops")?;
        } else {
            let t = f64of(&Self::ga_snd_t(slf)?)? - dt;
            Self::sa_snd_t(slf, t)?;
            if t <= 0.0 {
                // 機器の様子を数えるのは 0.1 秒ごと
                Self::sa_snd_t(slf, 0.1)?;
                let r = imp(py, "main.client.sound")?.call_method1("sources", (&w, Self::ga_snd_lit(slf)?, ga(s, "sfx")?))?;
                Self::sa_snd_loops(slf, r.get_item(0)?)?;
                Self::sa_snd_lit(slf, r.get_item(1)?)?;
            }
            let loops = Self::ga_snd_loops(slf)?;
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
    if !paths.call_method1("world_dir", (&name,))?.call_method1("__truediv__", ("world.json",))?.pexists()?.is_truthy()? {
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

// ---------------------------------------------------------------- Viewer の欄の読み書き（C-01。前は __dict__）
// 入れていない欄を読むと、前と同じく AttributeError（getattr(x, 名前, 既定値) も前と同じに動く）
macro_rules! viewer_attr_fields {
    ($($g:ident $s:ident $pg:ident $ps:ident $f:ident $name:literal),* $(,)?) => {
        #[allow(dead_code, non_snake_case)]
        impl Viewer { $(
            #[inline]
            pub fn $g<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
                match &slf.try_borrow()?.$f {
                    Some(v) => Ok(v.bind(slf.py()).clone()),
                    None => Err(pyo3::exceptions::PyAttributeError::new_err($name)),
                }
            }
            #[inline]
            pub fn $s(slf: &Bound<'_, Self>, v: impl IntoPy<PyObject>) -> PyResult<()> {
                let v = v.into_py(slf.py());
                slf.try_borrow_mut()?.$f = Some(v);
                Ok(())
            }
        )* }
        #[pymethods]
        #[allow(non_snake_case)]
        impl Viewer { $(
            #[getter($name)]
            fn $pg(slf: &Bound<'_, Self>) -> PyResult<PyObject> {
                Ok(Self::$g(slf)?.unbind())
            }
            #[setter($name)]
            fn $ps(&mut self, v: PyObject) {
                self.$f = Some(v);
            }
        )* }
    };
}
viewer_attr_fields!(
    ga_addon_err sa_addon_err pga_addon_err psa_addon_err a_addon_err "_addon_err", ga_aim_txt sa_aim_txt pga_aim_txt psa_aim_txt a_aim_txt "_aim_txt", ga_alt_free sa_alt_free pga_alt_free psa_alt_free a_alt_free "_alt_free", ga_cam sa_cam pga_cam psa_cam a_cam "_cam", ga_hud_shown sa_hud_shown pga_hud_shown psa_hud_shown a_hud_shown "_hud_shown", ga_perf_log sa_perf_log pga_perf_log psa_perf_log a_perf_log "_perf_log", ga_snd_lit sa_snd_lit pga_snd_lit psa_snd_lit a_snd_lit "_snd_lit", ga_snd_loops sa_snd_loops pga_snd_loops psa_snd_loops a_snd_loops "_snd_loops", ga_snd_t sa_snd_t pga_snd_t psa_snd_t a_snd_t "_snd_t", ga_third_before_seat sa_third_before_seat pga_third_before_seat psa_third_before_seat a_third_before_seat "_third_before_seat", ga_was_seated sa_was_seated pga_was_seated psa_was_seated a_was_seated "_was_seated", ga_acc sa_acc pga_acc psa_acc a_acc "acc", ga_addons sa_addons pga_addons psa_addons a_addons "addons", ga_aim_text sa_aim_text pga_aim_text psa_aim_text a_aim_text "aim_text", ga_anchor sa_anchor pga_anchor psa_anchor a_anchor "anchor", ga_anchor_ver sa_anchor_ver pga_anchor_ver psa_anchor_ver a_anchor_ver "anchor_ver", ga_bodies sa_bodies pga_bodies psa_bodies a_bodies "bodies", ga_carve sa_carve pga_carve psa_carve a_carve "carve", ga_cfg sa_cfg pga_cfg psa_cfg a_cfg "cfg", ga_close_app sa_close_app pga_close_app psa_close_app a_close_app "close_app", ga_cross sa_cross pga_cross psa_cross a_cross "cross", ga_debug_hud sa_debug_hud pga_debug_hud psa_debug_hud a_debug_hud "debug_hud", ga_dir_index sa_dir_index pga_dir_index psa_dir_index a_dir_index "dir_index", ga_editor sa_editor pga_editor psa_editor a_editor "editor", ga_fps_detail sa_fps_detail pga_fps_detail psa_fps_detail a_fps_detail "fps_detail", ga_fps_text sa_fps_text pga_fps_text psa_fps_text a_fps_text "fps_text", ga_frame_no sa_frame_no pga_frame_no psa_frame_no a_frame_no "frame_no", ga_game sa_game pga_game psa_game a_game "game", ga_ghost sa_ghost pga_ghost psa_ghost a_ghost "ghost", ga_grabbed sa_grabbed pga_grabbed psa_grabbed a_grabbed "grabbed", ga_held sa_held pga_held psa_held a_held "held", ga_highlight sa_highlight pga_highlight psa_highlight a_highlight "highlight", ga_hit sa_hit pga_hit psa_hit a_hit "hit", ga_hotbar sa_hotbar pga_hotbar psa_hotbar a_hotbar "hotbar", ga_hud sa_hud pga_hud psa_hud a_hud "hud", ga_item_text sa_item_text pga_item_text psa_item_text a_item_text "item_text", ga_labels sa_labels pga_labels psa_labels a_labels "labels", ga_lights sa_lights pga_lights psa_lights a_lights "lights", ga_load_errors sa_load_errors pga_load_errors psa_load_errors a_load_errors "load_errors", ga_loaded sa_loaded pga_loaded psa_loaded a_loaded "loaded", ga_lod sa_lod pga_lod psa_lod a_lod "lod", ga_msg sa_msg pga_msg psa_msg a_msg "msg", ga_patch sa_patch pga_patch psa_patch a_patch "patch", ga_perf sa_perf pga_perf psa_perf a_perf "perf", ga_pitch sa_pitch pga_pitch psa_pitch a_pitch "pitch", ga_planet_views sa_planet_views pga_planet_views psa_planet_views a_planet_views "planet_views", ga_quit sa_quit pga_quit psa_quit a_quit "quit", ga_root sa_root pga_root psa_root a_root "root", ga_scenes sa_scenes pga_scenes psa_scenes a_scenes "scenes", ga_sel sa_sel pga_sel psa_sel a_sel "sel", ga_shim sa_shim pga_shim psa_shim a_shim "shim", ga_show_help sa_show_help pga_show_help psa_show_help a_show_help "show_help", ga_snd sa_snd pga_snd psa_snd a_snd "snd", ga_static_root sa_static_root pga_static_root psa_static_root a_static_root "static_root", ga_third_person sa_third_person pga_third_person psa_third_person a_third_person "third_person", ga_ui sa_ui pga_ui psa_ui a_ui "ui", ga_view sa_view pga_view psa_view a_view "view", ga_views sa_views pga_views psa_views a_views "views", ga_w sa_w pga_w psa_w a_w "w", ga_yaw_fwd sa_yaw_fwd pga_yaw_fwd psa_yaw_fwd a_yaw_fwd "yaw_fwd"
);
