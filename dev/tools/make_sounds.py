"""効果音（sauce/sounds/*.wav）を数式で作る（録音素材は使わない）。

    python tools/make_sounds.py

どれも短いモノラル 22.05kHz。_loop の付く物はつなぎ目なく繰り返せる（推進機・ジェットパック・アクチュエーターの音）。
"""
import sys
import wave
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from main import paths  # noqa: E402

SR = 22050
RNG = np.random.default_rng(7)


def t(dur):
    return np.arange(int(dur * SR)) / SR


def noise(dur):
    return RNG.uniform(-1, 1, int(dur * SR))


def lowpass(x, f):
    """1 次のローパス（2 回かけて少し急に）。"""
    a = np.exp(-2 * np.pi * f / SR)
    for _ in range(2):
        y = np.empty_like(x)
        acc = 0.0
        for k, v in enumerate(x):
            acc = (1 - a) * v + a * acc
            y[k] = acc
        x = y
    return x


def highpass(x, f):
    return x - lowpass(x, f)


def decay(dur, k):
    return np.exp(-t(dur) * k)


def attack(x, ms=3):
    n = max(1, int(SR * ms / 1000))
    x = x.copy()
    x[:n] *= np.linspace(0, 1, n)
    return x


def norm(x, peak=0.8):
    return x / (np.abs(x).max() + 1e-9) * peak


def loopable(x, fade=0.08):
    """終わりを始めに重ねて、繰り返してもつなぎ目が鳴らないようにする。"""
    n = int(fade * SR)
    head, body, tail = x[:n], x[n:-n], x[-n:]
    w = np.linspace(0, 1, n)
    return np.concatenate([tail * (1 - w) + head * w, body])


def place():
    """ブロックを置く：コトッ（低い短い音＋こすれ）。"""
    d = 0.16
    x = np.sin(2 * np.pi * 140 * t(d) * (1 - 0.3 * t(d) / d)) * decay(d, 38)
    x += lowpass(noise(d), 1800) * decay(d, 60) * 0.6
    return norm(attack(x), 0.75)


def device():
    """機器を置く：カチャッ（金属）。"""
    d = 0.22
    x = (np.sin(2 * np.pi * 1320 * t(d)) * 0.5 + np.sin(2 * np.pi * 2210 * t(d)) * 0.3) * decay(d, 30)
    x += np.sin(2 * np.pi * 120 * t(d)) * decay(d, 40) * 0.8
    x += highpass(noise(d), 2500) * decay(d, 90) * 0.4
    return norm(attack(x), 0.7)


def brk():
    """ブロックを壊す：ガシャッ（砕ける）。"""
    d = 0.3
    x = lowpass(noise(d), 2600) * decay(d, 16)
    for k in range(4):                                        # 小さな破片の音
        s = int((0.03 + 0.05 * k) * SR)
        g = lowpass(noise(0.06), 3500) * decay(0.06, 70) * 0.5
        x[s:s + len(g)] += g[:len(x) - s]
    x += np.sin(2 * np.pi * 90 * t(d)) * decay(d, 25) * 0.6
    return norm(attack(x), 0.8)


def dig():
    """地面を掘る：ザクッ。"""
    d = 0.24
    x = lowpass(noise(d), 1200) * decay(d, 20) + highpass(noise(d), 3000) * decay(d, 45) * 0.3
    return norm(attack(x, 6), 0.7)


def click():
    """機器を使う・画面のボタン：カチッ。"""
    d = 0.06
    x = np.sin(2 * np.pi * 1800 * t(d)) * decay(d, 120) + highpass(noise(d), 4000) * decay(d, 200) * 0.3
    return norm(attack(x, 1), 0.55)


def seat():
    """操縦席に座る・降りる：ガチャン（ベルト）。"""
    d = 0.3
    x = np.sin(2 * np.pi * 95 * t(d)) * decay(d, 18) * 0.8
    x += (np.sin(2 * np.pi * 980 * t(d)) + 0.6 * np.sin(2 * np.pi * 1530 * t(d))) * decay(d, 35) * 0.35
    return norm(attack(x), 0.7)


def ignite():
    """点火：ボッ（立ち上がる炎）。"""
    d = 0.7
    env = np.minimum(t(d) / 0.05, 1.0) * decay(d, 4.5)
    x = lowpass(noise(d), 700) * env + np.sin(2 * np.pi * 55 * t(d)) * env * 0.5
    return norm(x, 0.85)


def thruster_loop():
    """推進機が燃えている音（ゴーッ）：低いノイズとうなり。"""
    d = 1.6
    x = lowpass(noise(d + 0.08), 420) + lowpass(noise(d + 0.08), 1500) * 0.25
    x *= 1 + 0.12 * np.sin(2 * np.pi * 7.5 * t(d + 0.08))
    return norm(loopable(x), 0.8)


def jetpack_loop():
    """ジェットパック（シューッ）：高めのノイズ。"""
    d = 1.2
    x = highpass(lowpass(noise(d + 0.08), 3500), 600)
    return norm(loopable(x), 0.6)


def motor_loop():
    """アクチュエーターが動いている音（ウィーン）：モーターのうなりとギアの細かい音。"""
    d = 1.25                       # 80 Hz の倍数の周期になる長さ（ループでうなりがずれない）
    tt = t(d + 0.08)
    x = (np.sin(2 * np.pi * 240 * tt) * 0.5 + np.sin(2 * np.pi * 480 * tt) * 0.25
         + np.sign(np.sin(2 * np.pi * 80 * tt)) * 0.12)
    x += highpass(noise(d + 0.08), 2500) * 0.08
    x *= 1 + 0.1 * np.sin(2 * np.pi * 12 * tt)
    return norm(loopable(lowpass(x, 3000)), 0.55)


def error():
    """できない：ブッブー（低い2回）。"""
    d = 0.32
    x = np.sign(np.sin(2 * np.pi * 180 * t(d))) * 0.4
    x = lowpass(x, 1200)
    x[int(0.13 * SR):int(0.18 * SR)] = 0
    x *= decay(d, 4)
    return norm(attack(x), 0.45)


SOUNDS = {"place": place, "device": device, "break": brk, "dig": dig, "click": click, "seat": seat,
          "ignite": ignite, "thruster_loop": thruster_loop, "jetpack_loop": jetpack_loop, "motor_loop": motor_loop,
          "error": error}


def write(path, x):
    data = (np.clip(x, -1, 1) * 32767).astype(np.int16)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(data.tobytes())


def main():
    out = paths.SAUCE / "sounds"
    out.mkdir(parents=True, exist_ok=True)
    for name, fn in SOUNDS.items():
        x = fn()
        write(out / f"{name}.wav", x)
        print(f"{name}.wav  {len(x) / SR:.2f} 秒")


if __name__ == "__main__":
    main()
