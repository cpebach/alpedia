"""軌道都市アマツ（画面側）：景色の箱を描く（非常灯は alert のとき点く）。"""
import time as _time

from main.client.addon_hooks import ClientAddon
from main.client.scenery_view import SceneView


class AmatsuClient(ClientAddon):
    name = "amatsu"

    def __init__(self, game):
        super().__init__(game)
        self.pack = game.world.addon("amatsu")
        self.active = bool(self.pack is not None and self.pack.active)
        self.views = [SceneView(game.scene_root, s) for s in self.pack.scenes] if self.active else []

    def destroy(self):
        for v in self.views:
            v.remove()
        self.views = []

    def aim(self, hit):
        if not self.active or not hit or hit.get("kind") != "amatsu":
            return None
        return ("軌道都市アマツの外壁", True)

    def update(self, origin, far):
        if not self.active:
            return
        now = _time.time()
        for v in self.views:
            v.update(origin, bool(self.pack.alert), now)
