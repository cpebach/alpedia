"""シナリオの章の一覧（J）：章ごとの状態、その章の始めからやり直す、見たムービーをもう一度見る、セリフの記録。"""
from . import script
from .client import wrap


class StoryPanel:
    def __init__(self, game):
        from ursina import Entity, Text, Button, camera, color, mouse
        self.game = game
        self.world = game.world
        self.pack = game.world.addon("story")
        mouse.locked = False
        self.color = color
        self.root = Entity(parent=camera.ui, z=-0.3)
        bg = Button(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0, 0, 0, 0.55),
                    highlight_color=color.rgba(0, 0, 0, 0.55), pressed_color=color.rgba(0, 0, 0, 0.55), z=0.1)
        bg.on_click = lambda: None
        Entity(parent=self.root, model="quad", scale=(1.6, 0.94), position=(0, 0, 0.05), color=color.rgb(0.07, 0.08, 0.11))
        Text(parent=self.root, text="シナリオ「それでも」", position=(-0.77, 0.44), origin=(-0.5, 0.5), scale=1.25)
        Text(parent=self.root, text="「ここから」で、その章の始めからやり直せます（前の章で済ませることは済んだ状態になります）",
             position=(-0.77, 0.385), origin=(-0.5, 0.5), scale=0.6, color=color.light_gray)
        st = self.pack.st
        cur = len(script.CHAPTERS) if st.get("done") else st.get("ch", 0)
        for k, c in enumerate(script.CHAPTERS):
            y = 0.33 - k * 0.047
            mark = "■" if k < cur else ("▶" if k == cur else "・")
            col = color.rgb(0.5, 0.9, 0.6) if k < cur else (color.rgb(1.0, 0.9, 0.4) if k == cur else color.light_gray)
            Text(parent=self.root, text=f"{mark} {c['title']}", position=(-0.77, y + 0.006), origin=(-0.5, 0), scale=0.7,
                 color=col)
            Text(parent=self.root, text=c["place"], position=(-0.77, y - 0.015), origin=(-0.5, 0), scale=0.5,
                 color=color.gray)
            b = Button(parent=self.root, text="ここから", position=(-0.25, y - 0.004), scale=(0.1, 0.034),
                       color=color.rgb(0.22, 0.32, 0.45), text_size=0.58)
            b.on_click = (lambda i=k: self.jump(i))
        # ムービー（見たものだけ、もう一度見られる）
        Text(parent=self.root, text="ムービー（見たもの）", position=(-0.13, 0.33), origin=(-0.5, 0), scale=0.75,
             color=color.rgb(1.0, 0.85, 0.5))
        seen = [n for n in script.MOVIES if n in st.get("seen", [])]
        if not seen:
            Text(parent=self.root, text="（まだありません）", position=(-0.13, 0.29), origin=(-0.5, 0), scale=0.6,
                 color=color.gray)
        for k, n in enumerate(seen):
            x = -0.13 + (k % 2) * 0.22
            y = 0.29 - (k // 2) * 0.042
            b = Button(parent=self.root, text=script.MOVIES[n], position=(x + 0.105, y), scale=(0.21, 0.036),
                       color=color.rgb(0.3, 0.26, 0.18), text_size=0.55)
            b.on_click = (lambda nn=n: self.replay(nn))
        # セリフの記録
        Text(parent=self.root, text="セリフの記録", position=(0.33, 0.33), origin=(-0.5, 0), scale=0.75,
             color=color.rgb(0.45, 0.95, 1.0))
        t = self.world.time
        lines = [x for x in st.get("log", []) if x[3] <= t][-12:]
        body = "\n".join(wrap(f"{x[1]}：{x[2]}", 24) for x in lines) or "（まだありません）"
        Text(parent=self.root, text=body, position=(0.33, 0.30), origin=(-0.5, 0.5), scale=0.55, line_height=1.08)
        b = Button(parent=self.root, text="閉じる", position=(0.66, -0.42), scale=(0.16, 0.045),
                   color=color.rgb(0.3, 0.3, 0.33), text_size=0.7)
        b.on_click = self.close
        panel = self

        class Keys(Entity):
            def input(self_, key):
                if key in ("escape", panel.game.cfg["controls"].get("story_menu", "j")):
                    panel.close()
        self.keys = Keys(parent=self.root)

    def jump(self, k):
        self.world.addon_call("story", "jump", k)
        self.close()

    def replay(self, name):
        self.world.addon_call("story", "replay", name)
        self.close()

    def close(self):
        from main.client.ui_destroy import destroy
        destroy(self.root)
        self.game.editor_closed()
