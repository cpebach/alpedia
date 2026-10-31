"""ルナ・ポート（ワールド側）：景色の箱を作り、当たり判定・視線・受付の会話を受け持つ。

ほかの MOD（シナリオなど）は world.addon("lunaport") の map（月の地図）・points（発着場 pad・その上向き pad_up・
受付 desk・広場 plaza）を使える。受付を E で触ると、各 MOD の on_scenery_talk("lunaport", "desk") が呼ばれる。
"""
from main import log
from main.hooks import Addon
from main.scenery import SceneryHost

from . import port

L = log.get("lunaport")


class LunaPortAddon(SceneryHost, Addon):
    name = "lunaport"
    scene_kind = "lunaport"

    def __init__(self):
        self.scenes = []
        self.points = {}
        self.map = None
        self.active = False

    def on_load(self, world):
        self.world = world
        luna = next((p for p in world.planets if p.defn["id"] == "luna"), None)
        main = next((p for p in world.planets if p.defn["id"] in ("gaia", "terra")), None) or \
            next((p for p in world.planets if p is not luna), None)
        if luna is None or main is None:
            L.info("月ルナと主な惑星が無いので、ルナ・ポートは作りません")
            return
        self.luna, self.terra = luna, main
        self.map = port.luna_map(luna, main.center)
        s = port.build(luna, main)
        self.scenes = [s]
        self.points = dict(s.points)
        self.active = True

    def talk(self, tag):
        """景色の触れる物（受付）を E。"""
        if not self.active:
            return None
        for a in self.world.addons:
            if a is self:
                continue
            try:
                msg = a.on_scenery_talk(self.world, "lunaport", tag)
            except Exception:  # noqa: BLE001
                L.exception("会話の処理に失敗")
                msg = None
            if msg:
                return msg
        if tag == "desk":
            return "管制塔の受付「ようこそ、自由港ルナ・ポートへ。帝国の人も連盟の人も、ここでは同じお客さんです」"
        return None
