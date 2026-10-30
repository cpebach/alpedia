"""ムービーをゲームの中で流す（MOD の movies/<名前>.mp4）。

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
                    snd.setVolume(float(game.cfg.get("audio", {}).get("movie_volume", 0.9)))
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
            if self.sound is not None:
                self.sound.stop()
            if self.tex is not None:
                self.tex.stop()
        except Exception:  # noqa: BLE001
            pass
        from main.client.ui_destroy import destroy
        destroy(self.root)
        self.on_done(self.name)
