"""効果音と音楽を数式で作る（録音した素材は使わない）。48kHz ステレオの wav に書き出す。

場面の audio(path) が Mixer に音を置いていく：
    mx = Mixer(秒数)
    mx.add(boom(2.0), 3.5, gain=0.8, pan=-0.3)
    mx.save(path)
"""
import wave

import numpy as np
from scipy import signal

SR = 48000


def _t(dur):
    return np.arange(int(dur * SR)) / SR


def env(dur, a=0.01, d=0.1, s=0.7, r=0.2):
    """ADSR の形（dur は全体の長さ）。"""
    n = int(dur * SR)
    e = np.full(n, s, np.float64)
    na, nd, nr = int(a * SR), int(d * SR), int(r * SR)
    if na:
        e[:na] = np.linspace(0, 1, na)
    if nd:
        e[na:na + nd] = np.linspace(1, s, len(e[na:na + nd]))
    if nr:
        e[-nr:] *= np.linspace(1, 0, nr)
    return e


def decay(dur, k):
    return np.exp(-_t(dur) * k)


def lowpass(x, f, order=2):
    b, a = signal.butter(order, min(f / (SR / 2), 0.99), "low")
    return signal.lfilter(b, a, x)


def highpass(x, f, order=2):
    b, a = signal.butter(order, min(f / (SR / 2), 0.99), "high")
    return signal.lfilter(b, a, x)


def bandpass(x, f0, f1, order=2):
    b, a = signal.butter(order, [f0 / (SR / 2), min(f1 / (SR / 2), 0.99)], "band")
    return signal.lfilter(b, a, x)


def noise(dur, seed=0):
    return np.random.default_rng(seed).standard_normal(int(dur * SR))


def sweep_lowpass(x, f0, f1, blocks=64):
    """だんだん（または急に）変わる低域通過（ブロックごとに切り替え）。"""
    out = np.zeros_like(x)
    n = len(x)
    edges = np.linspace(0, n, blocks + 1).astype(int)
    zi = None
    for k in range(blocks):
        f = f0 * (f1 / f0) ** (k / max(blocks - 1, 1))
        b, a = signal.butter(2, min(f / (SR / 2), 0.99), "low")
        if zi is None:
            zi = signal.lfilter_zi(b, a) * 0
        seg, zi = signal.lfilter(b, a, x[edges[k]:edges[k + 1]], zi=zi)
        out[edges[k]:edges[k + 1]] = seg
    return out


def saw(freq, dur, detune=0.0):
    t = _t(dur)
    out = 0
    for d in (-detune, 0.0, detune) if detune else (0.0,):
        out = out + signal.sawtooth(2 * np.pi * freq * (1 + d) * t)
    return out / (3 if detune else 1)


def sine(freq, dur, phase=0.0):
    t = _t(dur)
    f = np.asarray(freq) if np.ndim(freq) else freq
    if np.ndim(f):
        ph = 2 * np.pi * np.cumsum(f) / SR
        return np.sin(ph + phase)
    return np.sin(2 * np.pi * f * t + phase)


# --- 部品 ----------------------------------------------------------------------
def boom(dur=2.5, size=1.0, seed=0):
    """爆発：最初の破裂（明るい雑音）＋低いうなり（落ちていく音）＋ごろごろした残り。"""
    n = noise(dur, seed)
    crack = highpass(n, 900) * decay(dur, 18) * 0.6
    body = sweep_lowpass(n, 2400 * size ** 0.3, 120, 48) * decay(dur, 2.2 / size) * 1.6
    sub = sine(np.linspace(70, 28, int(dur * SR)), dur) * decay(dur, 2.5 / size) * 1.2
    rumble = lowpass(noise(dur, seed + 1), 90) * env(dur, 0.05, 0.3, 0.6, dur * 0.7) * 3.0
    x = crack + body + sub + rumble
    return x / (np.abs(x).max() + 1e-9)


def zap(dur=0.35, f0=2400, f1=300, seed=0):
    """光線の発射：高い音が急に下がる。"""
    f = f0 * (f1 / f0) ** (_t(dur) / dur)
    x = sine(f, dur) * 0.7 + signal.square(2 * np.pi * np.cumsum(f * 0.5) / SR) * 0.25
    return x * decay(dur, 9)


def whoosh(dur=1.2, f0=300, f1=4000, seed=0):
    """風を切る音（帯域の動く雑音。前半で強く）。"""
    x = sweep_lowpass(noise(dur, seed), f0, f1, 40)
    e = np.sin(np.pi * np.clip(_t(dur) / dur, 0, 1)) ** 1.5
    return x * e * 2.5


def launch(dur=1.4, seed=0):
    """ミサイルの発射：ドン＋シューッ。"""
    th = sine(np.linspace(140, 45, int(0.4 * SR)), 0.4) * decay(0.4, 9)
    x = np.zeros(int(dur * SR))
    x[:len(th)] += th * 0.9
    hiss = bandpass(noise(dur, seed), 1500, 9000) * env(dur, 0.02, 0.2, 0.5, dur * 0.6) * 0.45
    return x + hiss


def alarm(dur, f=(880, 660), rate=1.1):
    """警報：2つの高さを行き来する。"""
    t = _t(dur)
    sel = (np.floor(t * rate * 2) % 2).astype(bool)
    fr = np.where(sel, f[0], f[1])
    x = np.sign(np.sin(2 * np.pi * np.cumsum(fr) / SR)) * 0.3 + np.sin(2 * np.pi * np.cumsum(fr) / SR) * 0.5
    return lowpass(x, 3500) * (0.65 + 0.35 * np.sin(2 * np.pi * rate * t) ** 2)


def pad(freqs, dur, a=1.5, r=1.5, bright=1200.0, detune=0.004):
    """和音のパッド（ゆっくり立ち上がる、のこぎり波をやわらかくしたもの）。"""
    x = 0
    for f in freqs:
        x = x + saw(f, dur, detune)
    x = lowpass(x / len(freqs), bright)
    return x * env(dur, a, 0.0, 1.0, r)


def drone(f, dur, seed=0):
    """低いうなり（船の機関・艦橋の空調）。"""
    t = _t(dur)
    x = sine(f, dur) * 0.6 + sine(f * 2.01, dur) * 0.25 + sine(f * 3.02, dur) * 0.1
    x *= 0.85 + 0.15 * np.sin(2 * np.pi * 0.21 * t)
    x += lowpass(noise(dur, seed), 180) * 0.8
    return x


def hit(dur=1.6, f=55):
    """太鼓のような低い一打（映画の『ドン』）。"""
    x = sine(np.linspace(f * 2.2, f, int(dur * SR)), dur) * decay(dur, 4.0)
    x += lowpass(noise(dur, 7), 400) * decay(dur, 18) * 0.8
    return x


def riser(dur=1.5, f0=200, f1=1600):
    x = sweep_lowpass(noise(dur, 11), f0, f1, 40) * (_t(dur) / dur) ** 2 * 2.5
    x += sine(np.linspace(f0, f1, int(dur * SR)), dur) * (_t(dur) / dur) ** 3 * 0.3
    return x


def shimmer(dur=1.2, seed=0):
    """立体映像が現れる音（細かくきらめく高い音）。"""
    rng = np.random.default_rng(seed)
    x = np.zeros(int(dur * SR))
    for _ in range(40):
        f = rng.uniform(1800, 6500)
        s0 = int(rng.uniform(0, dur * 0.8) * SR)
        ln = int(0.12 * SR)
        seg = np.sin(2 * np.pi * f * np.arange(ln) / SR) * np.exp(-np.arange(ln) / SR * 30)
        x[s0:s0 + ln] += seg[:len(x[s0:s0 + ln])] * 0.15
    return x + whoosh(dur, 600, 5000, seed) * 0.3


class Mixer:
    def __init__(self, dur):
        self.buf = np.zeros((int(dur * SR) + SR, 2))
        self.dur = dur

    def add(self, x, t0, gain=1.0, pan=0.0):
        """pan：-1 左 〜 +1 右。"""
        x = np.asarray(x, float)
        s0 = int(t0 * SR)
        if s0 < 0:
            x = x[-s0:]
            s0 = 0
        n = min(len(x), len(self.buf) - s0)
        if n <= 0:
            return
        l, r = np.cos((pan + 1) * np.pi / 4), np.sin((pan + 1) * np.pi / 4)
        self.buf[s0:s0 + n, 0] += x[:n] * gain * l * 1.41
        self.buf[s0:s0 + n, 1] += x[:n] * gain * r * 1.41

    def save(self, path, fade_out=0.6):
        b = self.buf[:int(self.dur * SR)].copy()
        nf = int(fade_out * SR)
        b[-nf:] *= np.linspace(1, 0, nf)[:, None]
        # やわらかく音割れを防ぐ
        peak = np.abs(b).max() + 1e-9
        b = np.tanh(b / peak * 1.4) / np.tanh(1.4) * 0.92
        data = (b * 32767).astype(np.int16)
        with wave.open(str(path), "wb") as w:
            w.setnchannels(2)
            w.setsampwidth(2)
            w.setframerate(SR)
            w.writeframes(data.tobytes())
        return path


NOTE = {n: 440.0 * 2 ** ((k - 9) / 12) for k, n in enumerate(["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"])}


def chord(names, octave=3):
    """'A C E' → 周波数（octave 4 が A=440 の高さ）。"""
    out = []
    for n in names.split():
        o = octave
        if n[-1].isdigit():
            n, o = n[:-1], int(n[-1])
        out.append(NOTE[n] * 2 ** (o - 4))
    return out
