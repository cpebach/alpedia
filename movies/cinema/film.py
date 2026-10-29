"""1本のムービー（カットの並び・字幕・仕上げ）と、それを描いて mp4 にする仕組み。

場面のファイル（scenes/*.py）は Film を継承して、次を書く：
    build(stage)        物・役者を置き、それぞれの「時刻 → 位置」を決める
    shots               [Shot(始め, 終わり, カメラの関数)]  カメラの関数は (t, u) → Cam   u はカット内の 0〜1
    grade(t)            仕上げのつまみ（post.Grade）
    lines               [(始め, 終わり, 話す人, セリフ)]
    captions / titles   [(始め, 終わり, 文字)] / [(始め, 終わり, 大きな文字, 小さな文字)]
"""
import time
from dataclasses import dataclass

import numpy as np

from . import anim
from .post import Composer, Encoder, Grade, ease_alpha


@dataclass
class Cam:
    pos: tuple
    target: tuple
    fov: float = 35.0
    up: tuple = (0.0, 1.0, 0.0)
    roll: float = 0.0
    focus: float = None          # None：見る点までの距離にピント
    aperture: float = 0.0        # ぼけの強さ（遠くのぼけの半径 px）。0 でぼけなし
    max_blur: float = 14.0
    shake: float = 0.0           # 揺れ（m）


@dataclass
class Shot:
    t0: float
    t1: float
    cam: object                  # (t, u) → Cam
    name: str = ""


class Film:
    name = "film"
    duration = 10.0
    fps = 30
    out_size = (1920, 1080)
    aspect = 2.39                # 画の横長さ（シネスコ）。上下は黒帯（字幕を出す）
    near, far = 0.1, 60000.0
    sun = (0.5, 0.5, -0.7)
    ambient = 0.3
    shutter = 0.5                # 動きのぶれ（1コマの何割の時間を重ねるか）
    subframes = 1                # 動きのぶれのために重ねる枚数（1 でぶれなし）
    lines = []
    captions = []
    titles = []
    # audio(path)：音を作って wav に書く（あれば）。build のあとに呼ばれる

    def __init__(self):
        self.shots = []

    def build(self, stage):
        raise NotImplementedError

    def grade(self, t):
        return Grade()

    def shot_at(self, t):
        for s in self.shots:
            if s.t0 <= t < s.t1:
                return s
        return self.shots[-1]

    def cam_at(self, t):
        s = self.shot_at(t)
        u = anim.remap(t, s.t0, s.t1)
        return s.cam(t, u)

    def subtitle(self, t):
        for t0, t1, who, text in self.lines:
            a = ease_alpha(t, t0, t1, 0.18)
            if a > 0:
                return (who, text, a)
        return None

    def caption(self, t):
        for t0, t1, text in self.captions:
            a = ease_alpha(t, t0, t1, 0.4)
            if a > 0:
                return (text, a)
        return None

    def title(self, t):
        for t0, t1, text, sub in self.titles:
            a = ease_alpha(t, t0, t1, 0.5)
            if a > 0:
                return (text, sub, a)
        return None


def render_size(film, scale=1.0):
    ow, oh = int(film.out_size[0] * scale) // 2 * 2, int(film.out_size[1] * scale) // 2 * 2
    h = int(round(ow / film.aspect)) // 2 * 2
    return ow, oh, ow, h


def make_stage(film, scale=1.0):
    from .stage import Stage
    ow, oh, w, h = render_size(film, scale)
    st = Stage(w, h, film.fps, film.near, film.far, 35.0, film.sun, film.ambient)
    st.vertical_fov = getattr(film, "vertical_fov", True)
    return st


def apply_cam(stage, c, t):
    pos = np.asarray(c.pos, float)
    tgt = np.asarray(c.target, float)
    if c.shake:
        off = anim.shake(t, c.shake)
        pos = pos + off
        tgt = tgt + off * 0.6
    stage.camera(pos, tgt, c.up, c.roll, c.fov, c.focus, c.aperture, c.max_blur)


def draw_frame(film, stage, t):
    """時刻 t のコマ（動きのぶれ込み）を描く。"""
    n = max(1, int(film.subframes))
    if n == 1:
        apply_cam(stage, film.cam_at(t), t)
        return stage.draw(t)
    acc = None
    for k in range(n):
        ts = t + ((k + 0.5) / n - 0.5) * film.shutter / film.fps
        # カットの境目はまたがない
        if film.shot_at(ts) is not film.shot_at(t):
            ts = t
        apply_cam(stage, film.cam_at(ts), ts)
        img = stage.draw(ts).astype(np.float32)
        acc = img if acc is None else acc + img
    return (acc / n).astype(np.uint8)


def render(film, out_path, scale=1.0, t_from=0.0, t_to=None, log=print, audio=True):
    """描いて mp4 にする。場面に audio があれば、音を作って重ねる。"""
    from pathlib import Path
    out_path = Path(out_path)
    stage = make_stage(film, scale)
    film.build(stage)
    ow, oh, w, h = render_size(film, scale)
    comp = Composer(w, h, ow, oh)
    want_audio = audio and callable(getattr(film, "audio", None))
    vid = out_path.with_name(out_path.stem + "_video.mp4") if want_audio else out_path
    enc = Encoder(vid, ow, oh, film.fps)
    t_to = film.duration if t_to is None else t_to
    n0, n1 = int(round(t_from * film.fps)), int(round(t_to * film.fps))
    T = time.time()
    for i in range(n0, n1):
        t = i / film.fps
        img = draw_frame(film, stage, t)
        out = comp.finish(img, film.grade(t), i, film.subtitle(t), film.title(t), film.caption(t))
        enc.write(out)
        if i % film.fps == 0:
            el = time.time() - T
            done = (i - n0 + 1) / max(1, n1 - n0)
            log(f"  {t:5.1f} 秒 / {t_to:.1f}  （{el:.0f} 秒経過、残り約 {el / done * (1 - done):.0f} 秒）")
    enc.close()
    if want_audio:
        import subprocess
        from .post import ffmpeg_exe
        wav = out_path.with_suffix(".wav")
        log("音を作っています…")
        film.audio(wav)
        subprocess.run([ffmpeg_exe(), "-y", "-loglevel", "error", "-i", str(vid), "-ss", f"{t_from:.3f}", "-i", str(wav),
                        "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest", "-movflags", "+faststart",
                        str(out_path)], check=True)
        vid.unlink()
    return out_path


def stills(film, times, out_dir, scale=0.5):
    """確認用：指定した時刻のコマを PNG にする（前のコマから順に描くので、揺れものも本番と同じ）。"""
    from PIL import Image
    from pathlib import Path
    stage = make_stage(film, scale)
    film.build(stage)
    ow, oh, w, h = render_size(film, scale)
    comp = Composer(w, h, ow, oh)
    out = []
    times = sorted(times)
    for t in times:
        # 役者の揺れもの・まばたきは前のコマからの続きなので、少し前から描いておく
        for k in range(6, 0, -1):
            draw_frame(film, stage, max(0.0, t - k / film.fps))
        img = draw_frame(film, stage, t)
        fin = comp.finish(img, film.grade(t), int(t * film.fps), film.subtitle(t), film.title(t), film.caption(t))
        p = Path(out_dir) / f"{film.name}_{t:05.2f}.png"
        Image.fromarray(fin).save(p)
        out.append(p)
    return out

