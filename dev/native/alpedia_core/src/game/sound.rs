//! 音（main/client/sound.py）：BGM（mods/bgm の曲）と効果音（sauce/sounds/*.wav）。
//!
//! 音量はゲーム設定の audio.bgm・audio.sfx（0〜100 %）。BGM は音楽の系統、効果音とムービーの音は
//! 効果音の系統で鳴らすので（Rust の alpedia_view.Audio）、それぞれの音量がそのまま効く。
//!
//! BGM の曲は mods/bgm/bgm.json の "tracks"：{曲の ID: {"file": ファイル名, "gain_db": 音量の補正, "title": 題名}}。
//! どの場面でどの曲を流すかは、画面側の拡張の bgm()（シナリオなら章ごと）とムービー（set_override）が決める。
//! 曲のファイルが無ければ鳴らさない（BGM の MOD は入れなくても遊べる）。

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyDict, PyList, PySet, PyTuple};

use super::imp;
use super::ursina::f64of;

type R<T> = PyResult<T>;
type B<'py> = Bound<'py, PyAny>;

fn tb<T: IntoPy<PyObject>>(py: Python<'_>, x: T) -> B<'_> {
    x.into_py(py).into_bound(py)
}
fn lg(py: Python<'_>) -> R<B<'_>> {
    imp(py, "main.log")?.call_method1("get", ("sound",))
}
fn is_exc(py: Python<'_>, e: &PyErr) -> bool {
    e.is_instance_of::<PyException>(py)
}
fn smod(py: Python<'_>) -> R<B<'_>> {
    Ok(imp(py, "main.client.sound")?.into_any())
}

static SYS: GILOnceCell<std::sync::Mutex<Option<PyObject>>> = GILOnceCell::new();

/// ゲーム全体で1つの音の係（Rust の alpedia_view.Audio で鳴らす。使えなければ鳴らさない係）
#[pyfunction]
#[pyo3(signature = (cfg=None))]
pub fn sound_get(py: Python<'_>, cfg: Option<&B<'_>>) -> R<PyObject> {
    let cell = SYS.get_or_init(py, || std::sync::Mutex::new(None));
    let cur = cell.lock().unwrap().as_ref().map(|x| x.clone_ref(py));
    let sys = match cur {
        Some(s) => s,
        None => {
            let s = match py.get_type_bound::<RsBase>().call0().and_then(|b| py.get_type_bound::<SoundSystem>().call1((b,))) {
                Ok(s) => s.unbind(),
                Err(e) if is_exc(py, &e) => {
                    let k = PyDict::new_bound(py);
                    k.set_item("exc_info", e.value_bound(py))?;
                    lg(py)?.call_method("exception", ("音を鳴らせません",), Some(&k))?;
                    Py::new(py, Silent {})?.into_any()
                }
                Err(e) => return Err(e),
            };
            *cell.lock().unwrap() = Some(s.clone_ref(py));
            s
        }
    };
    if let Some(c) = cfg.filter(|c| !c.is_none()) {
        sys.bind(py).call_method1("apply", (c,))?;
    }
    Ok(sys)
}

#[pyfunction]
pub fn sound_volume_of(py: Python<'_>, cfg: &B<'_>, key: &B<'_>, default: f64) -> R<f64> {
    let r = (|| -> R<f64> {
        let c = if cfg.is_truthy()? { cfg.clone() } else { PyDict::new_bound(py).into_any() };
        let v = c.call_method1("get", ("audio", PyDict::new_bound(py)))?.call_method1("get", (key, default))?;
        let f: f64 = py.import_bound("builtins")?.getattr("float")?.call1((v,))?.extract()?;
        Ok(f)
    })();
    match r {
        Ok(f) => Ok(f.min(100.0).max(0.0) / 100.0),
        Err(e) if e.is_instance_of::<pyo3::exceptions::PyTypeError>(py) || e.is_instance_of::<pyo3::exceptions::PyValueError>(py) => Ok(default / 100.0),
        Err(e) => Err(e),
    }
}

/// {曲の ID: (ファイルのパス, 音量の倍率, 題名)}。mods/bgm/bgm.json が無い・読めなければ空
#[pyfunction]
pub fn sound_library(py: Python<'_>) -> R<Bound<'_, PyDict>> {
    let out = PyDict::new_bound(py);
    let d = imp(py, "main.paths")?.getattr("MODS")?.call_method1("__truediv__", ("bgm",))?;
    let meta = match (|| -> R<B<'_>> {
        let k = PyDict::new_bound(py);
        k.set_item("encoding", "utf-8")?;
        let t = d.call_method1("__truediv__", ("bgm.json",))?.call_method("read_text", (), Some(&k))?;
        imp(py, "json")?.call_method1("loads", (t,))
    })() {
        Ok(m) => m,
        Err(e) if is_exc(py, &e) => return Ok(out),
        Err(e) => return Err(e),
    };
    let tracks = meta.call_method1("get", ("tracks",))?;
    let tracks = if tracks.is_truthy()? { tracks } else { PyDict::new_bound(py).into_any() };
    for kv in tracks.call_method0("items")?.iter()? {
        let kv = kv?;
        let (tid, t) = (kv.get_item(0)?, kv.get_item(1)?);
        let r = (|| -> R<()> {
            let f = d.call_method1("__truediv__", (t.get_item("file")?,))?;
            if f.call_method0("exists")?.is_truthy()? {
                let g: f64 = py.import_bound("builtins")?.getattr("float")?.call1((t.call_method1("get", ("gain_db", 0.0))?,))?.extract()?;
                out.set_item(&tid, (f, 10f64.powf(g / 20.0), t.call_method1("get", ("title", &tid))?))?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => {}
            Err(e) if is_exc(py, &e) => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

/// 音を鳴らさない係（呼び出し口だけ同じ）
#[pyclass(module = "main.client.sound", name = "_Silent", subclass, dict)]
pub struct Silent {}
#[pymethods]
impl Silent {
    #[new]
    fn new() -> Self {
        Silent {}
    }
    fn apply(&self, cfg: &B<'_>) {
        let _ = cfg;
    }
    #[pyo3(signature = (name, volume=1.0))]
    fn sfx(&self, name: &B<'_>, volume: f64) {
        let _ = (name, volume);
    }
    #[pyo3(name = "loop")]
    fn loop_(&self, name: &B<'_>, volume: &B<'_>) {
        let _ = (name, volume);
    }
    fn stop_loops(&self) {}
    fn set_bgm(&self, want: &B<'_>) {
        let _ = want;
    }
    fn set_override(&self, want: &B<'_>) {
        let _ = want;
    }
    fn update(&self, dt: &B<'_>) {
        let _ = dt;
    }
}

// ---------------------------------------------------------------- Rust の音を、SoundSystem が使う形に包む
#[pyclass(module = "main.client.sound", name = "_RsSound", subclass, dict)]
pub struct RsSound {
    #[pyo3(get, set)]
    base: PyObject,
    #[pyo3(get, set)]
    id: i64,
    #[pyo3(get, set)]
    music: bool,
    #[pyo3(get, set)]
    vol: f64,
    #[pyo3(get, set)]
    start: f64,
}

#[pymethods]
impl RsSound {
    #[classattr]
    #[allow(non_snake_case)]
    fn PLAYING() -> i64 {
        2
    }
    #[classattr]
    #[allow(non_snake_case)]
    fn READY() -> i64 {
        1
    }
    #[new]
    fn new(base: PyObject, sid: i64, music: bool) -> Self {
        RsSound { base, id: sid, music, vol: 1.0, start: 0.0 }
    }
    fn _apply(&self, py: Python<'_>) -> R<()> {
        let b = self.base.bind(py);
        let k = f64of(&b.getattr(if self.music { "music_vol" } else { "sfx_vol" })?)?;
        b.getattr("au")?.call_method1("set_volume", (self.id, self.vol * k))?;
        Ok(())
    }
    #[pyo3(name = "setVolume")]
    fn set_volume(&mut self, py: Python<'_>, v: f64) -> R<()> {
        self.vol = v;
        self._apply(py)
    }
    #[pyo3(name = "setLoop", signature = (on=None))]
    fn set_loop(&self, py: Python<'_>, on: Option<&B<'_>>) -> R<()> {
        let on = match on { Some(o) => o.is_truthy()?, None => true };
        self.base.bind(py).getattr("au")?.call_method1("set_loop", (self.id, on))?;
        Ok(())
    }
    #[pyo3(name = "setTime")]
    fn set_time(&mut self, t: f64) {
        self.start = t;
    }
    fn play(&mut self, py: Python<'_>) -> R<()> {
        self._apply(py)?;
        self.base.bind(py).getattr("au")?.call_method1("play", (self.id, self.start))?;
        self.start = 0.0;
        Ok(())
    }
    fn stop(&self, py: Python<'_>) -> R<()> {
        let au = self.base.bind(py).getattr("au")?;
        au.call_method1("stop", (self.id,))?;
        if self.music {
            au.call_method1("unload", (self.id,))?; // 曲は止めたら捨てる（大きいので）
        }
        Ok(())
    }
    fn status(&self, py: Python<'_>) -> R<i64> {
        Ok(if self.base.bind(py).getattr("au")?.call_method1("playing", (self.id,))?.is_truthy()? { 2 } else { 1 })
    }
    fn length<'py>(&self, py: Python<'py>) -> R<B<'py>> {
        self.base.bind(py).getattr("au")?.call_method1("length", (self.id,))
    }
}

#[pyclass(module = "main.client.sound", name = "_RsVolume", subclass, dict)]
pub struct RsVolume {
    #[pyo3(get, set)]
    base: PyObject,
    #[pyo3(get, set)]
    key: String,
}
#[pymethods]
impl RsVolume {
    #[new]
    fn new(base: PyObject, key: String) -> Self {
        RsVolume { base, key }
    }
    #[pyo3(name = "setVolume")]
    fn set_volume(&self, py: Python<'_>, v: f64) -> R<()> {
        let b = self.base.bind(py);
        b.setattr(self.key.as_str(), v)?;
        let music = self.key == "music_vol";
        let made: Vec<B<'_>> = b.getattr("made")?.iter()?.collect::<R<_>>()?;
        for s in made {
            if s.getattr("music")?.is_truthy()? == music {
                s.call_method0("_apply")?;
            }
        }
        Ok(())
    }
}

#[pyclass(module = "main.client.sound", name = "_RsLoader", subclass, dict)]
pub struct RsLoader {
    #[pyo3(get, set)]
    base: PyObject,
}
#[pymethods]
impl RsLoader {
    #[new]
    fn new(base: PyObject) -> Self {
        RsLoader { base }
    }
    fn _load(&self, py: Python<'_>, f: &B<'_>, music: bool) -> R<PyObject> {
        let path = if f.hasattr("toOsSpecific")? { f.call_method0("toOsSpecific")? } else { f.str()?.into_any() };
        let b = self.base.bind(py);
        let sid: i64 = b.getattr("au")?.call_method1("load", (path,))?.extract()?;
        if sid < 0 {
            return Ok(py.None());
        }
        let snd = Py::new(py, RsSound::new(self.base.clone_ref(py), sid, music))?.into_bound(py).into_any();
        b.getattr("made")?.call_method1("append", (&snd,))?;
        if music {
            let au = b.getattr("au")?;
            let keep = PyList::empty_bound(py);
            for x in b.getattr("made")?.iter()? {
                let x = x?;
                if !x.getattr("music")?.is_truthy()? || x.is(&snd) || au.call_method1("playing", (x.getattr("id")?,))?.is_truthy()? {
                    keep.append(x)?;
                }
            }
            b.setattr("made", keep)?;
        }
        Ok(snd.unbind())
    }
    #[pyo3(name = "loadSfx")]
    fn load_sfx(&self, py: Python<'_>, f: &B<'_>) -> R<PyObject> {
        self._load(py, f, false)
    }
    #[pyo3(name = "loadMusic")]
    fn load_music(&self, py: Python<'_>, f: &B<'_>) -> R<PyObject> {
        self._load(py, f, true)
    }
    /// 読み解いた音（float32、チャンネルを交互に）から効果音として（動画の音など）
    #[pyo3(name = "loadPcm")]
    fn load_pcm(&self, py: Python<'_>, samples: &B<'_>, channels: &B<'_>, rate: &B<'_>) -> R<PyObject> {
        let np = imp(py, "numpy")?;
        let int = py.import_bound("builtins")?.getattr("int")?;
        let b = self.base.bind(py);
        let arr = np.call_method1("ascontiguousarray", (samples, np.getattr("float32")?))?.call_method1("reshape", (-1,))?;
        let sid: i64 = b.getattr("au")?.call_method1("load_pcm", (arr, int.call1((channels,))?, int.call1((rate,))?))?.extract()?;
        if sid < 0 {
            return Ok(py.None());
        }
        let snd = Py::new(py, RsSound::new(self.base.clone_ref(py), sid, false))?;
        b.getattr("made")?.call_method1("append", (&snd,))?;
        Ok(snd.into_any())
    }
}

/// application.base のうち、SoundSystem が使う所のまね
#[pyclass(module = "main.client.sound", name = "_RsBase", subclass, dict)]
pub struct RsBase {}
#[pymethods]
impl RsBase {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        RsBase {}
    }
    fn __init__(slf: &Bound<'_, Self>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let au = imp(py, "alpedia_view")?.getattr("Audio")?.call0()?;
        if !au.call_method0("ok")?.is_truthy()? {
            lg(py)?.call_method1("warning", ("音の出口が見つかりません（音は鳴りません）",))?;
        }
        s.setattr("au", au)?;
        s.setattr("music_vol", 1.0)?;
        s.setattr("sfx_vol", 1.0)?;
        s.setattr("made", PyList::empty_bound(py))?;
        s.setattr("loader", Py::new(py, RsLoader::new(s.clone().unbind()))?)?;
        s.setattr("musicManager", Py::new(py, RsVolume::new(s.clone().unbind(), "music_vol".into()))?)?;
        s.setattr("sfxManagerList", vec![Py::new(py, RsVolume::new(s.clone().unbind(), "sfx_vol".into()))?])?;
        Ok(())
    }
}

/// BGM・効果音（音そのものは base が Rust で鳴らす。update(dt) を毎フレーム呼ぶ）
#[pyclass(module = "main.client.sound", subclass, dict)]
pub struct SoundSystem {}

fn g<'py>(s: &B<'py>, k: &str) -> R<B<'py>> {
    s.getattr(k)
}

#[pymethods]
impl SoundSystem {
    #[new]
    #[pyo3(signature = (*_a, **_k))]
    fn new(_a: &Bound<'_, PyTuple>, _k: Option<&Bound<'_, PyDict>>) -> Self {
        SoundSystem {}
    }
    fn __init__(slf: &Bound<'_, Self>, base: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        s.setattr("base", base)?;
        s.setattr("bgm_vol", 0.6)?;
        s.setattr("sfx_vol", 0.8)?;
        s.setattr("lib", py.None())?;
        s.setattr("cache", PyDict::new_bound(py))?; // 効果音の名前 → [AudioSound]（重ねて鳴らせるように数個）
        s.setattr("loops", PyDict::new_bound(py))?; // 繰り返す効果音の名前 → AudioSound
        s.setattr("music", py.None())?; // 今の曲 {"key", "snd", "gain", "vol"}
        s.setattr("fading", PyList::empty_bound(py))?; // 消えていく曲
        s.setattr("want", py.None())?; // 場面が流したい曲 (曲の ID, 始める秒)
        s.setattr("override", py.None())?; // ムービーの曲（場面の曲より優先）
        Ok(())
    }
    fn _fname(&self, p: &B<'_>) -> R<String> {
        Ok(p.str()?.to_string())
    }
    // --- 音量 ---
    fn apply(slf: &Bound<'_, Self>, cfg: &B<'_>) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let bv = sound_volume_of(py, cfg, &tb(py, "bgm"), 60.0)?;
        let sv = sound_volume_of(py, cfg, &tb(py, "sfx"), 80.0)?;
        s.setattr("bgm_vol", bv)?;
        s.setattr("sfx_vol", sv)?;
        let r = (|| -> R<()> {
            let b = g(s, "base")?;
            b.getattr("musicManager")?.call_method1("setVolume", (bv,))?;
            for m in b.getattr("sfxManagerList")?.iter()? {
                m?.call_method1("setVolume", (sv,))?;
            }
            Ok(())
        })();
        match r {
            Err(e) if !is_exc(py, &e) => Err(e),
            _ => Ok(()),
        }
    }
    // --- 効果音 ---
    #[pyo3(signature = (name, music=false))]
    fn _load<'py>(slf: &Bound<'py, Self>, name: &B<'py>, music: bool) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let p = imp(py, "main.paths")?.getattr("SAUCE")?.call_method1("__truediv__", ("sounds",))?.call_method1("__truediv__", (format!("{}.wav", name.str()?),))?;
        if !p.call_method0("exists")?.is_truthy()? {
            return Ok(py.None().into_bound(py));
        }
        let r = (|| -> R<B<'py>> {
            let ld = g(s, "base")?.getattr("loader")?;
            let f = s.call_method1("_fname", (&p,))?;
            ld.call_method1(if music { "loadMusic" } else { "loadSfx" }, (f,))
        })();
        match r {
            Ok(v) => Ok(v),
            Err(e) if is_exc(py, &e) => Ok(py.None().into_bound(py)),
            Err(e) => Err(e),
        }
    }
    /// 効果音を1回鳴らす（同じ音は 3 つまで重ねて鳴る）
    #[pyo3(signature = (name, volume=1.0))]
    fn sfx(slf: &Bound<'_, Self>, name: &B<'_>, volume: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        if f64of(&g(s, "sfx_vol")?)? <= 0.0 || volume <= 0.01 {
            return Ok(());
        }
        let cache = g(s, "cache")?;
        let mut pool = cache.call_method1("get", (name,))?;
        if pool.is_none() {
            let l = PyList::empty_bound(py);
            for _ in 0..3 {
                let x = s.call_method1("_load", (name,))?;
                if !x.is_none() {
                    l.append(x)?;
                }
            }
            cache.set_item(name, &l)?;
            pool = l.into_any();
        }
        if !pool.is_truthy()? {
            return Ok(());
        }
        let mut pick = pool.get_item(0)?;
        for x in pool.iter()? {
            let x = x?;
            if !x.call_method0("status")?.eq(x.getattr("PLAYING")?)? {
                pick = x;
                break;
            }
        }
        pick.call_method1("setVolume", (volume.clamp(0.0, 1.0),))?;
        pick.call_method0("play")?;
        Ok(())
    }
    /// 繰り返す効果音の大きさ（0 で止める）。毎フレーム呼ぶ
    #[pyo3(name = "loop")]
    fn loop_(slf: &Bound<'_, Self>, name: &B<'_>, volume: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let loops = g(s, "loops")?;
        let mut snd = loops.call_method1("get", (name,))?;
        if volume <= 0.01 {
            if !snd.is_none() && snd.call_method0("status")?.eq(snd.getattr("PLAYING")?)? {
                snd.call_method0("stop")?;
            }
            return Ok(());
        }
        if snd.is_none() {
            snd = s.call_method1("_load", (name,))?;
            if snd.is_none() {
                loops.set_item(name, py.None())?;
                return Ok(());
            }
            snd.call_method1("setLoop", (true,))?;
            loops.set_item(name, &snd)?;
        }
        snd.call_method1("setVolume", (volume.clamp(0.0, 1.0),))?;
        if !snd.call_method0("status")?.eq(snd.getattr("PLAYING")?)? {
            snd.call_method0("play")?;
        }
        Ok(())
    }
    fn stop_loops(slf: &Bound<'_, Self>) -> R<()> {
        for snd in g(slf.as_any(), "loops")?.call_method0("values")?.iter()? {
            let snd = snd?;
            if !snd.is_none() {
                snd.call_method0("stop")?;
            }
        }
        Ok(())
    }
    // --- BGM ---
    /// 場面の曲：(曲の ID, 始める秒) か None（止める）。毎フレーム呼んでよい（変わったときだけ替える）
    fn set_bgm(slf: &Bound<'_, Self>, want: &B<'_>) -> R<()> {
        slf.as_any().setattr("want", want)
    }
    /// ムービーの曲（None で場面の曲に戻す）
    fn set_override(slf: &Bound<'_, Self>, want: &B<'_>) -> R<()> {
        slf.as_any().setattr("override", want)
    }
    fn _start<'py>(slf: &Bound<'py, Self>, key: &B<'py>) -> R<B<'py>> {
        let py = slf.py();
        let s = slf.as_any();
        let none = py.None().into_bound(py);
        if g(s, "lib")?.is_none() {
            s.setattr("lib", smod(py)?.call_method0("library")?)?;
        }
        let (tid, start) = (key.get_item(0)?, key.get_item(1)?);
        let t = g(s, "lib")?.call_method1("get", (&tid,))?;
        if t.is_none() {
            return Ok(none);
        }
        let snd = match s.call_method1("_fname", (t.get_item(0)?,)).and_then(|f| g(s, "base")?.getattr("loader")?.call_method1("loadMusic", (f,))) {
            Ok(x) => x,
            Err(e) if is_exc(py, &e) => {
                lg(py)?.call_method1("warning", ("BGM を読めません: %s", t.get_item(0)?))?;
                return Ok(none);
            }
            Err(e) => return Err(e),
        };
        if snd.is_none() {
            return Ok(none);
        }
        snd.call_method1("setLoop", (true,))?;
        snd.call_method1("setVolume", (0.0,))?;
        if start.is_truthy()? {
            match py.import_bound("builtins")?.getattr("float")?.call1((&start,)).and_then(|f| snd.call_method1("setTime", (f,))) {
                Err(e) if !is_exc(py, &e) => return Err(e),
                _ => {}
            }
        }
        snd.call_method0("play")?;
        lg(py)?.call_method1("info", ("BGM: %s（%s 秒から）", t.get_item(2)?, &start))?;
        let d = PyDict::new_bound(py);
        d.set_item("key", key)?;
        d.set_item("snd", snd)?;
        d.set_item("gain", f64of(&t.get_item(1)?)?.min(1.0))?;
        d.set_item("vol", 0.0)?;
        Ok(d.into_any())
    }
    fn update(slf: &Bound<'_, Self>, dt: f64) -> R<()> {
        let py = slf.py();
        let s = slf.as_any();
        let ov = g(s, "override")?;
        let k0 = if ov.is_truthy()? { ov } else { g(s, "want")? };
        let key = if k0.is_truthy()? { py.import_bound("builtins")?.getattr("tuple")?.call1((k0,))? } else { py.None().into_bound(py) };
        let cur = g(s, "music")?;
        let ck = if cur.is_truthy()? { cur.get_item("key")? } else { py.None().into_bound(py) };
        if !ck.eq(&key)? {
            if !cur.is_none() {
                g(s, "fading")?.call_method1("append", (&cur,))?;
            }
            let m = if key.is_truthy()? { Self::_start(slf, &key)? } else { py.None().into_bound(py) };
            s.setattr("music", m)?;
        }
        let fade = f64of(&smod(py)?.getattr("FADE")?)?;
        let step = dt / fade;
        let m = g(s, "music")?;
        if !m.is_none() {
            let v = (f64of(&m.get_item("vol")?)? + step).min(1.0);
            m.set_item("vol", v)?;
            m.get_item("snd")?.call_method1("setVolume", (v * f64of(&m.get_item("gain")?)?,))?;
        }
        let fading = g(s, "fading")?;
        let fl: Vec<B<'_>> = fading.iter()?.collect::<R<_>>()?;
        for f in fl {
            let v = f64of(&f.get_item("vol")?)? - step;
            f.set_item("vol", v)?;
            if v <= 0.0 {
                f.get_item("snd")?.call_method0("stop")?;
                fading.call_method1("remove", (&f,))?;
            } else {
                f.get_item("snd")?.call_method1("setVolume", (v * f64of(&f.get_item("gain")?)?,))?;
            }
        }
        Ok(())
    }
}

/// 近くで燃えている推進機・動いているアクチュエーター・ジェットパックの音の大きさ（距離で小さく）。
/// 点火した推進機は sfx("ignite") で「ボッ」。戻り値 ({ループの名前: 大きさ}, 燃えている推進機の組)
#[pyfunction]
pub fn sound_sources<'py>(py: Python<'py>, w: &B<'py>, prev_lit: &B<'py>, sfx: &B<'py>) -> R<Bound<'py, PyTuple>> {
    let act = imp(py, "main.actuator")?;
    let npl = imp(py, "numpy")?.getattr("linalg")?;
    let eye = w.getattr("player")?.call_method0("eye")?;
    let (mut thr, mut mot, mut jet) = (0.0f64, 0.0f64, 0.0f64);
    let lit = PySet::empty_bound(py)?;
    for c in w.call_method1("constructs_near", (&eye, 150.0))?.iter()? {
        let c = c?;
        if !c.getattr("n_vox")?.is_truthy()? || !c.getattr("has_devices")?.is_truthy()? {
            continue;
        }
        let cid = c.getattr("id")?;
        for (i, d) in c.getattr("devs")?.iter()?.enumerate() {
            let d = d?;
            if !d.is_truthy()? {
                continue;
            }
            let st = d.get_item("state")?;
            if st.contains("firing")? || st.contains("lit")? {
                let fv = st.call_method1("get", ("firing", 0.0))?;
                let f = if fv.is_truthy()? { f64of(&fv)? } else { 0.0 };
                if f > 0.01 || st.call_method1("get", ("lit",))?.is_truthy()? {
                    let dist = f64of(&npl.call_method1("norm", (c.call_method1("voxel_center", (i,))?.call_method1("__sub__", (&eye,))?,))?)?;
                    let att = 1.0 / (1.0 + (dist / 12.0).powi(2));
                    thr += f.max(0.15) * att * 0.5;
                    let key = PyTuple::new_bound(py, [cid.clone(), tb(py, i)]);
                    lit.add(&key)?;
                    if !prev_lit.contains(&key)? {
                        sfx.call1(("ignite", (att * 1.5).min(1.0)))?;
                    }
                }
            } else if st.contains("angle_target")? && act.call_method1("busy", (&st,))?.is_truthy()? {
                let dist = f64of(&npl.call_method1("norm", (c.call_method1("voxel_center", (i,))?.call_method1("__sub__", (&eye,))?,))?)?;
                mot += 0.7 / (1.0 + (dist / 8.0).powi(2));
            }
        }
    }
    let p = w.getattr("player")?;
    if p.getattr("flying").map(|x| x.is_truthy().unwrap_or(false)).unwrap_or(false) {
        let t = p.getattr("thrust").unwrap_or_else(|_| imp(py, "numpy").unwrap().call_method1("zeros", (3,)).unwrap());
        if f64of(&npl.call_method1("norm", (t,))?)? > 0.05 {
            jet = 0.6;
        }
    }
    let out = PyDict::new_bound(py);
    out.set_item("thruster_loop", thr.min(1.0))?;
    out.set_item("motor_loop", mot.min(1.0))?;
    out.set_item("jetpack_loop", jet.min(1.0))?;
    Ok(PyTuple::new_bound(py, [out.into_any(), lit.into_any()]))
}

pub fn register(m: &Bound<'_, PyModule>) -> R<()> {
    for f in [wrap_pyfunction_bound!(sound_get, m)?, wrap_pyfunction_bound!(sound_volume_of, m)?, wrap_pyfunction_bound!(sound_library, m)?,
              wrap_pyfunction_bound!(sound_sources, m)?] {
        m.add_function(f)?;
    }
    m.add_class::<Silent>()?;
    m.add_class::<RsSound>()?;
    m.add_class::<RsVolume>()?;
    m.add_class::<RsLoader>()?;
    m.add_class::<RsBase>()?;
    m.add_class::<SoundSystem>()?;
    Ok(())
}
