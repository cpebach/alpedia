"""軌道都市アマツ（ワールド側）：景色の箱・当たり判定・視線。

ほかの MOD は world.addon("amatsu") の deck（甲板の座標：point(x, z, y)・frame(yaw)）と points を使える。
alert を True にすると非常灯（赤）が点く（シナリオで都市が落ちていく間）。
"""
from main import log
from main.hooks import Addon
from main.scenery import SceneryHost

from . import city

L = log.get("amatsu")


class AmatsuAddon(SceneryHost, Addon):
    name = "amatsu"
    scene_kind = "amatsu"

    def __init__(self):
        self.scenes = []
        self.points = {}
        self.deck = None
        self.active = False
        self.alert = False

    def on_load(self, world):
        self.world = world
        main = next((p for p in world.planets if p.defn["id"] == "terra"), None) or \
            (world.planets[0] if world.planets else None)
        if main is None:
            return
        s = city.build(main.center)
        self.scenes = [s]
        self.points = dict(s.points)
        self.deck = s.deck
        self.active = True

    def net_state(self, world):
        return {"alert": bool(self.alert)} if self.active else None

    def net_apply(self, world, state):
        if isinstance(state, dict):
            self.alert = bool(state.get("alert"))
