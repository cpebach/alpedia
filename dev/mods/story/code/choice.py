"""物語の中で選ぶ欄（第12章のあと、ネムリの前で）。選ぶまで閉じない（Esc でも閉じない）。"""
from . import script


class ChoicePanel:
    def __init__(self, game, on_choose):
        from ursina import Entity, Text, Button, camera, color
        self.game, self.on_choose = game, on_choose
        self.root = Entity(parent=camera.ui, z=-0.4)
        bg = Button(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0, 0, 0, 0.7),
                    highlight_color=color.rgba(0, 0, 0, 0.7), pressed_color=color.rgba(0, 0, 0, 0.7), z=0.1)
        bg.on_click = lambda: None
        Entity(parent=self.root, model="quad", scale=(1.1, 0.42), color=color.rgb(0.06, 0.07, 0.1), z=0.05)
        Text(parent=self.root, text=script.CHOICE["title"], position=(0, 0.13), origin=(0, 0), scale=1.05)
        Text(parent=self.root, text="どうする？", position=(0, 0.07), origin=(0, 0), scale=0.75, color=color.light_gray)
        for k, (key, label, _) in enumerate(script.CHOICE["options"]):
            b = Button(parent=self.root, text=label, position=(0, -0.02 - k * 0.085), scale=(0.62, 0.065),
                       color=color.rgb(0.45, 0.22, 0.22) if key == "shoot" else color.rgb(0.22, 0.38, 0.55),
                       text_size=0.85)
            b.on_click = (lambda kk=key: self.choose(kk))

    def choose(self, key):
        from main.client.ui_destroy import destroy
        destroy(self.root)
        self.on_choose(key)

    def close(self):              # 選ぶまで閉じない
        pass
