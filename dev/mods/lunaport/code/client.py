"""ルナ・ポート（画面側）：景色の箱を描く。受付の案内とボタン。"""
import time as _time

import numpy as np

from main.client.addon_hooks import ClientAddon
from main.client.scenery_view import SceneView


class LunaPortClient(ClientAddon):
    name = "lunaport"

    def __init__(self, game):
        super().__init__(game)
        self.pack = game.world.addon("lunaport")
        self.active = bool(self.pack is not None and self.pack.active)
        self.views = [SceneView(game.scene_root, s) for s in self.pack.scenes] if self.active else []

    def destroy(self):
        for v in self.views:
            v.remove()
        self.views = []

    def aim(self, hit):
        if not self.active or not hit or hit.get("kind") != "lunaport":
            return None
        if hit.get("tag") == "desk":
            return ("管制塔の受付（E で話す）", False)
        return ("", True)

    def interact(self, hit):
        if not self.active or not hit or hit.get("kind") != "lunaport" or not hit.get("tag"):
            return False
        msg = self.world.addon_call("lunaport", "talk", hit["tag"])
        if msg:
            self.world.message(msg)
        return True

    def nearby(self, pos, radius):
        if not self.active:
            return []
        p = self.pack.points.get("desk")
        if p is None:
            return []
        d = float(np.linalg.norm(np.asarray(pos) - p))
        if d > max(radius, 4.0):
            return []
        return [{"label": "受付で話す", "hit": {"kind": "lunaport", "tag": "desk"}, "kind": "e", "dist": d,
                 "icon": "interact"}]

    def update(self, origin, far):
        if not self.active:
            return
        now = _time.time()
        for v in self.views:
            v.update(origin, True, now)
