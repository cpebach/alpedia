"""エンドロール（エピローグで低い軌道に乗ったら流れる）。

画面の下から上へ、みんなのその後 → 制作 → お世話になったもの（ロゴ）→ 最後にタイトルロゴ。
後ろでは世界が動き続ける（ロケットは軌道を回っている）。Esc・クリックで最後のタイトルまで飛ばせる。
ロゴの絵は mods/story/credits/<名前>.png があればそれを、無ければ名前を文字で出す（公式のロゴの画像は
それぞれの配布元から入手して置いてください）。
"""
import time as _time

from main import log, paths

from . import script

L = log.get("credits")

SPEED = 0.075            # 1 秒に流れる高さ（画面の高さ = 1）
TITLE_HOLD = 6.0         # タイトルロゴを出しておく秒数
LOGO_DIR = paths.MODS / "story" / "credits"


class CreditsRoll:
    def __init__(self, game, on_done):
        from ursina import Entity, Text, Button, camera, color, mouse
        self.game, self.on_done = game, on_done
        self.color = color
        self.done = False
        mouse.locked = False
        self.root = Entity(parent=camera.ui, z=-0.4)
        bg = Button(parent=self.root, model="quad", scale=(4, 3), color=color.rgba(0, 0, 0, 0.62),
                    highlight_color=color.rgba(0, 0, 0, 0.62), pressed_color=color.rgba(0, 0, 0, 0.62), z=0.1)
        bg.on_click = self.skip
        self.bg = bg
        self.roll = Entity(parent=self.root)
        y = 0.0
        gold, cyan, gray = color.rgb(1.0, 0.85, 0.45), color.rgb(0.5, 0.95, 1.0), color.rgb(0.8, 0.82, 0.86)

        def text(t, sc, col=color.white, gap=0.0, x=0.0, origin=(0, 0.5)):
            nonlocal y
            e = Text(parent=self.roll, text=t, position=(x, y), origin=origin, scale=sc, color=col)
            return e
        text("ストーリー vol.1", 1.6, gold)
        y -= 0.12
        text("―― それから ――", 1.0, cyan)
        y -= 0.12
        from .client import wrap
        for who, after in script.CREDITS_AFTER:
            text(who, 1.05, gold)
            y -= 0.05
            body = wrap(after, 30)
            text(body, 0.8, gray)
            y -= 0.04 + 0.034 * (body.count("\n") + 1) + 0.05
        y -= 0.1
        text("―― 制作 ――", 1.0, cyan)
        y -= 0.1
        for role, name in script.CREDITS_STAFF:
            text(role, 0.75, gray)
            y -= 0.045
            text(name, 1.05)
            y -= 0.1
        y -= 0.08
        # 音楽：BGM の MOD（mods/bgm）に入っていて、このシナリオで流した曲
        try:
            from main.client.sound import library
            lib = library()
        except Exception:  # noqa: BLE001
            lib = {}
        used = []
        for k in list(script.BGM["chapters"].values()) + list(script.BGM["movies"].values()) + [script.BGM["credits"]]:
            if k[0] in lib and lib[k[0]][2] not in used:
                used.append(lib[k[0]][2])
        if used:
            text("―― 音楽 ――", 1.0, cyan)
            y -= 0.1
            for title in used:
                text(title, 0.78, gray)
                y -= 0.05
            y -= 0.1
        text("―― お世話になったもの ――", 1.0, cyan)
        y -= 0.11
        items = script.CREDITS_THANKS
        for k in range(0, len(items), 3):
            row = items[k:k + 3]
            for j, (logo, name, note) in enumerate(row):
                x = (j - (len(row) - 1) / 2) * 0.42
                img = LOGO_DIR / f"{logo}.png"
                shown = False
                if img.exists():
                    try:
                        from ursina import load_texture
                        tex = load_texture(str(img))
                        if tex is not None:
                            h = 0.09
                            w = h * tex.width / max(tex.height, 1)
                            Entity(parent=self.roll, model="quad", texture=tex, position=(x, y - h / 2),
                                   scale=(min(w, 0.36), h))
                            shown = True
                    except Exception:  # noqa: BLE001
                        L.warning("ロゴを読めません: %s", img)
                if not shown:
                    Text(parent=self.roll, text=name, position=(x, y - 0.02), origin=(0, 0.5), scale=1.5,
                         color=color.white)
                Text(parent=self.roll, text=note, position=(x, y - 0.105), origin=(0, 0.5), scale=0.62,
                     color=gray)
            y -= 0.2
        y -= 0.1
        text("そして、遊んでくれたあなたへ。", 1.0, gold)
        y -= 0.1
        self.height = -y
        self.roll.y = -0.62                     # 画面の下から出てくる
        self.end_y = self.height + 0.55         # 最後の行が画面の上に消えるまで
        # 最後のタイトル（流れ終わったら出す）
        self.title = Entity(parent=self.root, enabled=False)
        self.title_main = Text(parent=self.title, text="Alpedia", origin=(0, 0), position=(0, 0.06), scale=5.0,
                               color=color.rgba(1, 1, 1, 0))
        self.title_sub = Text(parent=self.title, text="ストーリー vol.1", origin=(0, 0), position=(0, -0.06),
                              scale=1.6, color=color.rgba(1.0, 0.85, 0.45, 0))
        self.title_thanks = Text(parent=self.title, text="Thank you for playing!", origin=(0, 0),
                                 position=(0, -0.15), scale=1.0, color=color.rgba(0.8, 0.9, 1.0, 0))
        self.title_t = None
        self.t0 = _time.monotonic()
        self._last = self.t0
        self.hint = Text(parent=self.root, text="Esc・クリックで飛ばす", origin=(0.5, -0.5), scale=0.6,
                         position=(0.86, -0.48), color=color.rgba(1, 1, 1, 0.35))
        player = self

        class Keys(Entity):
            def input(self_, key):
                if key in ("escape", "space", "enter"):
                    player.skip()

            def update(self_):
                player.update()
        self.keys = Keys(parent=self.root)

    def skip(self):
        if self.title_t is None:
            self._show_title()
        elif _time.monotonic() - self.title_t > 1.0:
            self.finish()

    def _show_title(self):
        self.roll.enabled = False
        self.title.enabled = True
        self.title_t = _time.monotonic()
        self.hint.enabled = False

    def update(self):
        if self.done:
            return
        now = _time.monotonic()
        dt, self._last = min(now - self._last, 0.1), now
        if self.title_t is None:
            self.roll.y += SPEED * dt
            if self.roll.y >= self.end_y:
                self._show_title()
            return
        a = min(1.0, (now - self.title_t) / 2.0)
        c = self.color
        self.title_main.color = c.rgba(1, 1, 1, a)
        self.title_sub.color = c.rgba(1.0, 0.85, 0.45, a)
        self.title_thanks.color = c.rgba(0.8, 0.9, 1.0, max(0.0, min(1.0, (now - self.title_t - 1.2) / 1.5)))
        self.bg.color = c.rgba(0, 0, 0, 0.62 + 0.38 * a)
        if now - self.title_t > TITLE_HOLD:
            self.finish()

    def finish(self):
        if self.done:
            return
        self.done = True
        from main.client.ui_destroy import destroy
        destroy(self.root)
        self.on_done()
