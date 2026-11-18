"""地球の名所（ワールド側）：プレイヤーが名所の範囲に入ったら名前を知らせる（1 秒ごとに調べる）。"""
import numpy as np

from main import log
from main.hooks import Addon

L = log.get("earth")


class EarthAddon(Addon):
    name = "earth"

    def __init__(self):
        self.places = []          # [(名前, 惑星, 中心の向き, 半径)]
        self.inside = {}          # プレイヤー → 今いる名所の名前
        self.t = 0.0

    def on_load(self, world):
        self.world = world
        for pl in world.planets:
            for f in pl.defn.get("features") or []:
                x, z = (list(f["at"]) + [0.0])[:2]
                dist = float(np.hypot(x, z))
                a = dist / pl.radius
                d = np.array([0.0, 1.0, 0.0]) if dist < 1e-9 else np.array([np.sin(a) * x / dist, np.cos(a), np.sin(a) * z / dist])
                self.places.append((f.get("name") or f["kind"], pl, d / np.linalg.norm(d), float(f["radius"])))
        if self.places:
            L.info("地形の名所 %d", len(self.places))

    def before_physics(self, world, dt):
        self.t += dt
        if self.t < 1.0 or not self.places:
            return set()
        self.t = 0.0
        for p in [world.player] + list(getattr(world, "remote", {}).values()):
            if p is None:
                continue
            here = None
            for name, pl, d, r in self.places:
                q = np.asarray(p.pos, float) - pl.center
                n = float(np.linalg.norm(q))
                if n > 1e-6 and np.arccos(np.clip(q @ d / n, -1, 1)) * pl.radius < r * 0.8:
                    here = name
                    break
            if here != self.inside.get(id(p)):
                self.inside[id(p)] = here
                if here and p is world.player:
                    world.message(f"{here}に入りました")
        return set()
