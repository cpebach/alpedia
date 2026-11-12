"""ムービーをゲームの中で流す（MOD の movies/<名前>.mp4）。

Rust の画面（--view rust）では RsMoviePlayer：映像と音は PyAV（pip の av）で読み解き、1コマずつ Rust のテクスチャへ送り、
音は Rust（alpedia_view.Audio）で鳴らす（av が無ければ題名だけ出して次へ）。

Panda3D の MovieTexture（ffmpeg）で映像を、同じファイルの音を loadSfx で鳴らして、映像を音に合わせる。
画面いっぱいに黒い背景と映像。Esc・Space・Enter・クリックで飛ばせる。流している間、世界の時間は止める
（マルチプレイ中は止めない）。読めないときは、名前だけ出してすぐ終わる。
"""
import time as _time

from main import log, paths

from . import script

L = log.get("movie")


def movie_path(name):
    return paths.MODS / "story" / "movies" / f"{name}.mp4"


class MoviePlayer:
    def __new__(cls, game, name, on_done):
        from main.client import gfx
        if cls is MoviePlayer and gfx.active():         # Rust の画面
            return object.__new__(RsMoviePlayer)
        return object.__new__(cls)

    def __init__(self, game, name, on_done):
        from panda3d.core import Filename, MovieTexture, TextureStage
        from ursina import Entity, Text, Button, camera, color, mouse, window
        self.game, self.name, self.on_done = game, name, on_done
        self.done = False
        mouse.locked = False
        self.root = Entity(parent=camera.ui, z=-0.5)
        bg = Button(parent=self.root, model="quad", scale=(4, 3), color=color.black, highlight_color=color.black,
                    pressed_color=color.black, z=0.1)
        bg.on_click = self.finish
        self.tex = None
        self.sound = None
        self.t0 = _time.monotonic()
        self.length = 0.0
        try:                           # ムービーの曲（BGM の MOD の曲。無ければ鳴らない）
            from main.client import sound
            sound.get().set_override(script.BGM["movies"].get(name))
        except Exception:  # noqa: BLE001
            L.exception("ムービーの曲を流せません")
        path = movie_path(name)
        title = script.MOVIES.get(name, name)
        try:
            if not path.exists():
                raise FileNotFoundError(str(path))
            tex = MovieTexture(name)
            if not tex.read(Filename.fromOsSpecific(str(path))):
                raise RuntimeError("動画を読めません")
            self.tex = tex
            self.length = float(tex.getVideoLength())
            aspect = tex.getVideoWidth() / max(tex.getVideoHeight(), 1)
            w = min(window.aspect_ratio, aspect)
            screen = Entity(parent=self.root, model="quad", scale=(w, w / aspect), z=0.05)
            screen.setTexture(tex, 1)
            screen.setTexScale(TextureStage.getDefault(), tex.getTexScale())
            self.screen = screen
            try:
                snd = game.U["application"].base.loader.loadSfx(Filename.fromOsSpecific(str(path)))
                if snd is not None and snd.length() > 0:
                    snd.setVolume(1.0)            # 大きさは効果音の音量（設定）。曲は BGM として別に流れる
                    tex.synchronizeTo(snd)
                    snd.play()
                    self.sound = snd
                else:
                    tex.play()
            except Exception as e:  # noqa: BLE001
                L.warning("ムービーの音を鳴らせません: %s", e)
                tex.play()
        except Exception as e:  # noqa: BLE001
            L.warning("ムービー %s を流せません: %s", name, e)
            Text(parent=self.root, text=f"ムービー「{title}」\n（再生できません：{e}）", origin=(0, 0), scale=1.2,
                 color=color.light_gray)
            self.length = 2.5
        self.hint = Text(parent=self.root, text=f"{title}　　Esc・クリックで飛ばす", position=window.bottom_right + (-0.02, 0.02),
                         origin=(0.5, -0.5), scale=0.7, color=color.rgba(1, 1, 1, 0.5))
        self._dim = color.rgba(1, 1, 1, 0.22)
        self._dimmed = False
        player = self

        class Keys(Entity):
            def input(self_, key):
                if key in ("escape", "space", "enter", "left mouse down"):
                    player.finish()

            def update(self_):
                player.update()
        self.keys = Keys(parent=self.root)

    def update(self):
        if self.done:
            return
        t = _time.monotonic() - self.t0
        if self.sound is not None:
            from panda3d.core import AudioSound
            if t > 1.0 and self.sound.status() != AudioSound.PLAYING:
                self.finish()
                return
        if t > self.length + 0.3:
            self.finish()
        if t > 3.0 and not self._dimmed:
            self._dimmed = True
            self.hint.color = self._dim

    def finish(self):
        if self.done:
            return
        self.done = True
        try:
            from main.client import sound
            sound.get().set_override(None)
        except Exception:  # noqa: BLE001
            pass
        try:
            if self.sound is not None:
                self.sound.stop()
            if self.tex is not None:
                self.tex.stop()
        except Exception:  # noqa: BLE001
            pass
        from main.client.ui_destroy import destroy
        destroy(self.root)
        self.on_done(self.name)


class RsMoviePlayer(MoviePlayer):
    """Rust の画面のムービー：PyAV で読み解いた1コマを、時間に合わせて Rust のテクスチャへ。"""
    MAX_W = 1280                       # 送る大きさ（これより大きい動画は縮める）

    def __init__(self, game, name, on_done):
        from ursina import Entity, Text, Button, camera, color, window
        from main.client import gfx, sound
        self.game, self.name, self.on_done = game, name, on_done
        self.done = False
        self.root = Entity(parent=camera.ui, z=-0.5)
        bg = Button(parent=self.root, model="quad", scale=(4, 3), color=color.black, highlight_color=color.black,
                    pressed_color=color.black, z=0.1)
        bg.on_click = self.finish
        self.tex = None
        self.sound = None
        self.cont = None
        self.length = 0.0
        snd_sys = sound.get()
        try:
            snd_sys.set_override(script.BGM["movies"].get(name))
        except Exception:  # noqa: BLE001
            L.exception("ムービーの曲を流せません")
        path = movie_path(name)
        title = script.MOVIES.get(name, name)
        try:
            if not path.exists():
                raise FileNotFoundError(str(path))
            import av
            self.cont = av.open(str(path))
            vs = self.cont.streams.video[0]
            vs.thread_type = "AUTO"
            W, H = vs.codec_context.width, vs.codec_context.height
            tw = min(W, self.MAX_W)
            th = max(2, int(round(H * tw / W / 2)) * 2)
            self.size = (tw, th)
            self.length = float(vs.duration * vs.time_base) if vs.duration else float(self.cont.duration or 0) / 1e6
            self.frames = self.cont.decode(vs)
            self.tex = gfx.tex_rgba(tw, th, bytes(tw * th * 4), f"movie_{name}")
            aspect = W / max(H, 1)
            w = min(window.aspect_ratio, aspect)
            self.screen = Entity(parent=self.root, model="quad", texture=self.tex, scale=(w, w / aspect), z=0.05)
            self.pending = None                 # 読み解いたが、まだ見せる時間でない1コマ
            try:
                self.sound = self._audio(path, snd_sys)
            except Exception as e:  # noqa: BLE001
                L.warning("ムービーの音を鳴らせません: %s", e)
            self.t0 = _time.monotonic()
            if self.sound is not None:
                self.sound.play()
            self._show(0.0)
        except Exception as e:  # noqa: BLE001
            L.warning("ムービー %s を流せません: %s", name, e)
            Text(parent=self.root, text=f"ムービー「{title}」\n（再生できません：{e}）", origin=(0, 0), scale=1.2,
                 color=color.light_gray)
            self.length = 2.5
            self.cont = None
            self.t0 = _time.monotonic()
        self.hint = Text(parent=self.root, text=f"{title}　　Esc・クリックで飛ばす", position=window.bottom_right + (-0.02, 0.02),
                         origin=(0.5, -0.5), scale=0.7, color=color.rgba(1, 1, 1, 0.5))
        self._dim = color.rgba(1, 1, 1, 0.22)
        self._dimmed = False
        player = self

        class Keys(Entity):
            def input(self_, key):
                if key in ("escape", "space", "enter", "left mouse down"):
                    player.finish()

            def update(self_):
                player.update()
        self.keys = Keys(parent=self.root)

    def _audio(self, path, snd_sys):
        """動画の音を全部読み解いて（48kHz・ステレオ・float32）、効果音として鳴らす準備。"""
        import av
        import numpy as np
        loader = getattr(getattr(snd_sys, "base", None), "loader", None)
        if loader is None or not hasattr(loader, "loadPcm"):
            return None
        with av.open(str(path)) as c:
            if not c.streams.audio:
                return None
            rs = av.AudioResampler(format="flt", layout="stereo", rate=48000)
            chunks = []
            for fr in c.decode(c.streams.audio[0]):
                for o in rs.resample(fr):
                    chunks.append(o.to_ndarray().reshape(-1))
            for o in rs.resample(None):
                chunks.append(o.to_ndarray().reshape(-1))
        if not chunks:
            return None
        snd = loader.loadPcm(np.concatenate(chunks), 2, 48000)
        if snd is not None:
            snd.setVolume(1.0)
        return snd

    def _show(self, t):
        """t 秒までの1コマを読み解いて、最後の1コマを送る。"""
        from main.client import gfx
        last = None
        while True:
            fr = self.pending
            if fr is None:
                try:
                    fr = next(self.frames)
                except StopIteration:
                    self.length = min(self.length, t) if self.length else t
                    break
            if fr.time is not None and fr.time > t:
                self.pending = fr
                break
            self.pending = None
            last = fr
        if last is not None:
            a = last.to_ndarray(format="rgba", width=self.size[0], height=self.size[1])
            gfx.VIEW.tex_update(self.tex.id, self.size[0], self.size[1], a.tobytes())

    def update(self):
        if self.done:
            return
        t = _time.monotonic() - self.t0
        if self.cont is not None:
            try:
                self._show(t)
            except Exception:  # noqa: BLE001
                L.exception("ムービーの1コマを読めません")
                self.cont = None
        if t > self.length + 0.3:
            self.finish()
        if t > 3.0 and not self._dimmed:
            self._dimmed = True
            self.hint.color = self._dim

    def finish(self):
        if self.done:
            return
        self.done = True
        try:
            from main.client import sound
            sound.get().set_override(None)
        except Exception:  # noqa: BLE001
            pass
        try:
            if self.sound is not None:
                self.sound.stop()
                self.sound.base.au.unload(self.sound.id)      # 読み解いた音は大きいので捨てる
            if self.cont is not None:
                self.cont.close()
        except Exception:  # noqa: BLE001
            pass
        from main.client.ui_destroy import destroy
        destroy(self.root)
        self.on_done(self.name)
