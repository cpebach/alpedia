"""ストーリーの章の一覧（J）：章ごとの状態、その章の始めからやり直す、無線のセリフの記録。"""
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
        Entity(parent=self.root, model="quad", scale=(1.5, 0.92), position=(0, 0, 0.05), color=color.rgb(0.07, 0.08, 0.11))
        Text(parent=self.root, text="ストーリー：墜落した技術者", position=(-0.72, 0.43), origin=(-0.5, 0.5), scale=1.25)
        Text(parent=self.root, text="「ここから」で、その章の始めからやり直せます（前の章で済ませることは済んだ状態になります）",
             position=(-0.72, 0.37), origin=(-0.5, 0.5), scale=0.62, color=color.light_gray)
        st = self.pack.st
        cur = len(script.CHAPTERS) if st.get("done") else st.get("ch", 0)
        for k, c in enumerate(script.CHAPTERS):
            y = 0.30 - k * 0.062
            mark = "■" if k < cur else ("▶" if k == cur else "・")
            col = color.rgb(0.5, 0.9, 0.6) if k < cur else (color.rgb(1.0, 0.9, 0.4) if k == cur else color.light_gray)
            Text(parent=self.root, text=f"{mark} {c['title']}", position=(-0.72, y), origin=(-0.5, 0), scale=0.78,
                 color=col)
            Text(parent=self.root, text=c["place"], position=(-0.72, y - 0.024), origin=(-0.5, 0), scale=0.55,
                 color=color.gray)
            b = Button(parent=self.root, text="ここから", position=(-0.17, y - 0.008), scale=(0.11, 0.038),
                       color=color.rgb(0.22, 0.32, 0.45), text_size=0.62)
            b.on_click = (lambda i=k: self.jump(i))
        Text(parent=self.root, text="無線の記録", position=(0.0, 0.30), origin=(-0.5, 0), scale=0.8,
             color=color.rgb(0.45, 0.95, 1.0))
        t = self.world.time
        lines = [x for x in st.get("log", []) if x[3] <= t][-9:]
        body = "\n".join(wrap(f"{x[1]}：{x[2]}", 30) for x in lines) or "（まだありません）"
        Text(parent=self.root, text=body, position=(0.0, 0.27), origin=(-0.5, 0.5), scale=0.6, line_height=1.1)
        b = Button(parent=self.root, text="閉じる", position=(0.62, -0.41), scale=(0.16, 0.045),
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

    def close(self):
        from main.client.ui_destroy import destroy
        destroy(self.root)
        self.game.editor_closed()
