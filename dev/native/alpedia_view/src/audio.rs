//! 音（rodio）：効果音（wav）と BGM（mp3 など）。Python からは alpedia_view.Audio。
//! 音の出口が無い PC でも例外にせず、何も鳴らさないだけにする。

use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use pyo3::prelude::*;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

struct Snd {
    data: Arc<[u8]>,
    pcm: Option<(Arc<Vec<f32>>, u16, u32)>,     // 読み解いた音（標本・チャンネル数・毎秒の標本数）
    sink: Option<Sink>,
    looping: bool,
    volume: f32,
    length: f64,
}

#[pyclass(unsendable)]
pub struct Audio {
    out: Option<(OutputStream, OutputStreamHandle)>,
    sounds: Vec<Snd>,
}

impl Drop for Audio {
    fn drop(&mut self) {
        for s in &mut self.sounds {
            if let Some(k) = s.sink.take() {
                k.stop();
            }
        }
        // 音の出口は閉じずに残す（終わるときに閉じると、環境によっては止まることがある）
        if let Some(o) = self.out.take() {
            std::mem::forget(o);
        }
    }
}

#[pymethods]
impl Audio {
    #[new]
    fn new() -> Self {
        Audio { out: OutputStream::try_default().ok(), sounds: vec![] }
    }

    /// 音の出口があるか
    fn ok(&self) -> bool {
        self.out.is_some()
    }

    /// 音のファイルを読む。戻り値は音の番号（読めなければ -1）。
    fn load(&mut self, path: &str) -> i64 {
        let data: Arc<[u8]> = match std::fs::read(path) {
            Ok(d) => d.into(),
            Err(_) => return -1,
        };
        let length = match Decoder::new(Cursor::new(data.clone())) {
            Ok(d) => d.total_duration().map(|t| t.as_secs_f64()).unwrap_or(0.0),
            Err(_) => return -1,
        };
        self.sounds.push(Snd { data, pcm: None, sink: None, looping: false, volume: 1.0, length });
        self.sounds.len() as i64 - 1
    }

    /// 読み解いた音（f32、チャンネルを交互に並べた標本）から。動画の音など。戻り値は音の番号。
    fn load_pcm(&mut self, samples: numpy::PyReadonlyArrayDyn<'_, f32>, channels: u16, rate: u32) -> i64 {
        use numpy::PyUntypedArrayMethods;
        if channels == 0 || rate == 0 {
            return -1;
        }
        let samples: Vec<f32> = match samples.as_slice() {
            Ok(x) => x.to_vec(),
            Err(_) => return -1,
        };
        let length = samples.len() as f64 / channels as f64 / rate as f64;
        self.sounds.push(Snd { data: Arc::from(Vec::new()), pcm: Some((Arc::new(samples), channels, rate)), sink: None,
                               looping: false, volume: 1.0, length });
        self.sounds.len() as i64 - 1
    }

    /// 鳴らす（鳴っていれば最初から）。start：始める秒
    #[pyo3(signature = (id, start=0.0))]
    fn play(&mut self, id: usize, start: f64) {
        let handle = match &self.out {
            Some((_, h)) => h,
            None => return,
        };
        let Some(s) = self.sounds.get_mut(id) else { return };
        if let Some(old) = s.sink.take() {
            old.stop();
        }
        let Ok(sink) = Sink::try_new(handle) else { return };
        sink.set_volume(s.volume);
        let skip0 = Duration::from_secs_f64(start.max(0.0));
        if let Some((pcm, ch, rate)) = &s.pcm {
            let src = rodio::buffer::SamplesBuffer::new(*ch, *rate, pcm.as_ref().clone()).skip_duration(skip0);
            if s.looping {
                sink.append(src.repeat_infinite());
            } else {
                sink.append(src);
            }
            s.sink = Some(sink);
            return;
        }
        let cur = Cursor::new(s.data.clone());
        // 始める秒は読み飛ばす（mp3 の seek は環境によって止まるので使わない）
        let skip = Duration::from_secs_f64(start.max(0.0));
        if s.looping {
            match Decoder::new_looped(cur) {
                Ok(d) => sink.append(d.skip_duration(skip)),
                Err(_) => return,
            }
        } else {
            match Decoder::new(cur) {
                Ok(d) => sink.append(d.skip_duration(skip)),
                Err(_) => return,
            }
        }
        s.sink = Some(sink);
    }

    fn stop(&mut self, id: usize) {
        if let Some(s) = self.sounds.get_mut(id) {
            if let Some(k) = s.sink.take() {
                k.stop();
            }
        }
    }

    fn playing(&self, id: usize) -> bool {
        self.sounds.get(id).and_then(|s| s.sink.as_ref()).map(|k| !k.empty()).unwrap_or(false)
    }

    fn set_volume(&mut self, id: usize, v: f32) {
        if let Some(s) = self.sounds.get_mut(id) {
            s.volume = v.clamp(0.0, 4.0);
            if let Some(k) = &s.sink {
                k.set_volume(s.volume);
            }
        }
    }

    fn set_loop(&mut self, id: usize, on: bool) {
        if let Some(s) = self.sounds.get_mut(id) {
            s.looping = on;
        }
    }

    /// 長さ（秒、わからなければ 0）
    fn length(&self, id: usize) -> f64 {
        self.sounds.get(id).map(|s| s.length).unwrap_or(0.0)
    }

    /// 音を捨てる（BGM を替えたとき）
    fn unload(&mut self, id: usize) {
        if let Some(s) = self.sounds.get_mut(id) {
            if let Some(k) = s.sink.take() {
                k.stop();
            }
            s.data = Arc::from(Vec::new());
            s.pcm = None;
        }
    }
}
