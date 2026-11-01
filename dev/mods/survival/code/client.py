"""サバイバルの画面：左上に目標（上から順に）。"""
from main.client.addon_hooks import ClientAddon


class SurvivalClient(ClientAddon):
    name = "survival"

    def __init__(self, game):
        super().__init__(game)
        self.pack = game.world.addon("survival")
        U = game.U
        Entity, Text, color, window = U["Entity"], U["Text"], U["color"], U["window"]
        self.root = Entity(parent=game.ui_root, position=window.top_left + (0.012, -0.1))
        self.bg = Entity(parent=self.root, model="quad", origin=(-0.5, 0.5), scale=(0.6, 0.1),
                         color=color.rgba(0.02, 0.05, 0.1, 0.62), z=0.02)
        self.title = Text(parent=self.root, text="サバイバルの目標", position=(0.012, -0.012), origin=(-0.5, 0.5),
                          scale=0.75, color=color.rgb(0.6, 1.0, 0.6))
        self.text = Text(parent=self.root, text="", position=(0.012, -0.043), origin=(-0.5, 0.5), scale=0.66,
                         line_height=1.15)
        self._t = 0.0

    def update(self, origin, far):
        import time
        now = time.time()
        if self.pack is None or now - self._t < 0.5:
            return
        self._t = now
        goals = self.pack.goals_state(self.world)
        todo = [(g, t) for g, t, d in goals if not d]
        done_n = len(goals) - len(todo)
        if not todo:
            lines = ["全部達成！ ここからは自由に作ろう"]
        else:
            lines = [f"□ {t}" for _, t in todo[:2]]          # 次の2つだけ（画面をふさがない）
        lines.append(f"（{done_n}/{len(goals)} 達成）")
        txt = "\n".join(lines)
        if self.text.text != txt:
            self.text.text = txt
        self.bg.scale = (0.6, 0.06 + len(lines) * 0.024)

    def destroy(self):
        from main.client.ui_destroy import destroy
        destroy(self.root)
