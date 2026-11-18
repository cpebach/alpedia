"""A-01〜A-03 のテスト：燃料の重さ・切り離し・ジェット・気候・集落（ブロックの建物の出し入れ）・前のセーブの読み替え。

    python -m unittest discover -s tests -p test_scale.py -v      （dev フォルダで）
"""
import json
import unittest

import numpy as np

from test_core import CoreCase, DT
from main import paths
from main.construct import Construct
from main.world import World


class TestFuelMass(CoreCase):
    def test_fuel_has_weight_and_burning_makes_it_lighter(self):
        w = self.world(gravity_scale=0.0)
        tank = {"def": "tank", "dir": 2, "state": {"contents": {"fuel": 800.0}}}
        c = self.put(w, [((0, 0, 0), "steel", {"def": "battery", "dir": 2}), ((1, 0, 0), "steel", tank),
                         ((2, 0, 0), "steel", {"def": "valve", "dir": 2}), ((3, 0, 0), "steel", {"def": "thruster", "dir": 2})],
                     extra=40.0)
        m0 = float(c.mass.sum())
        # タンクは空なら 160 kg（アルミの箱）＋燃料 800 kg
        self.assertGreater(m0, 975 * 3 + 160 + 790)
        i = next(k for k, d in enumerate(c.devs) if d and d["def"] == "thruster")
        w.interact({"kind": "voxel", "construct": c, "index": i, "face": 2})       # E：点火
        for _ in range(int(1.5 / DT)):
            w.step(DT)
        m1 = float(c.mass.sum())
        fuel = c.devs[1]["state"]["contents"].get("fuel", 0.0)
        self.assertLess(fuel, 800.0)
        self.assertAlmostEqual(m0 - m1, 800.0 - fuel, delta=0.03 * m0)


class TestDecoupler(CoreCase):
    def test_script_fire_splits_the_construct(self):
        w = self.world(gravity_scale=0.0)
        V = [((0, 0, 0), "aluminum", {"def": "seat", "dir": 4}), ((0, -1, 0), "aluminum", None),
             ((0, -2, 0), "aluminum", {"def": "decoupler", "dir": 3}), ((0, -3, 0), "aluminum", None),
             ((0, -4, 0), "aluminum", None)]
        c = self.put(w, V, extra=40.0)
        n0 = len(w.constructs)
        i = next(k for k, d in enumerate(c.devs) if d and d["def"] == "decoupler")
        c.devs[i].setdefault("state", {})["fire"] = True
        w.step(DT)
        self.assertEqual(len(w.constructs), n0 + 1)
        self.assertTrue(c.devs[i]["state"]["fired"])
        other = [x for x in w.constructs.values() if x is not c][-1]
        w.step(DT)
        # 離れていく
        d0 = float(np.linalg.norm(other.center_of_mass() - c.center_of_mass()))
        for _ in range(30):
            w.step(DT)
        self.assertGreater(float(np.linalg.norm(other.center_of_mass() - c.center_of_mass())), d0 + 0.2)


class TestJet(CoreCase):
    def jet(self, planet):
        w = self.world(gravity_scale=0.0, planets=[{"def": planet, "position": [0, 0, 0], "craters": []}])
        tank = {"def": "tank", "dir": 2, "state": {"contents": {"fuel": 800.0}}}
        c = self.put(w, [((0, 0, 0), "steel", {"def": "battery", "dir": 2}), ((1, 0, 0), "steel", tank),
                         ((2, 0, 0), "steel", {"def": "valve", "dir": 2}), ((3, 0, 0), "steel", {"def": "jet", "dir": 2})],
                     extra=5.0)
        i = next(k for k, d in enumerate(c.devs) if d and d["def"] == "jet")
        w.interact({"kind": "voxel", "construct": c, "index": i, "face": 2})       # E：点火
        for _ in range(10):
            w.step(DT)
        return c.devs[i]["state"]

    def test_jet_needs_air(self):
        self.assertGreater(self.jet("terra").get("firing", 0.0), 0.5)
        st = self.jet("luna")
        self.assertEqual(st.get("firing", 0.0), 0.0)
        self.assertIn("空気", st.get("error", ""))


class TestClimate(CoreCase):
    def test_spawn_is_temperate_and_there_are_deserts_and_snow(self):
        w = self.world(planets=[{"def": "gaia", "position": [0, 0, 0], "craters": []}])
        pl = w.planets[0]
        rng = np.random.default_rng(1)
        d = rng.normal(size=(4000, 3))
        d /= np.linalg.norm(d, axis=1)[:, None]
        h = np.asarray(pl.height0(d))
        cl = np.asarray(pl._nat().climate(d, h))
        t, m = cl[:, 0], cl[:, 1]
        land = h > 0
        self.assertGreater(land.mean(), 0.3)
        self.assertTrue(((t > 0.5) & (m < 0.25) & land).any(), "砂漠")
        self.assertTrue(((t < 0.0) & land).any(), "雪")
        s = np.asarray(pl._nat().climate(np.array([[0.0, 1.0, 0.0]]), np.array([3.0])))[0]
        self.assertTrue(0.3 < s[0] < 0.75 and s[1] > 0.5, f"出現地点 {s}")


class TestSettlements(CoreCase):
    def test_settlements_of_every_kind_and_block_buildings_near_players(self):
        import sys
        sys.path.insert(0, str(paths.MODS))
        from city.code.cities import CityManager
        from city.code.blockify import Streamer
        from main.world import default_bodies
        w = self.world(planets=default_bodies("gaia", True, False))
        m = CityManager(w)
        kinds = {getattr(c, "stype", None) for c in m.cities if c.host is w.planets[0]}
        for k in ("village", "farm", "port", "mine", "residential"):
            self.assertIn(k, kinds)
        v = next(c for c in m.cities if getattr(c, "stype", "") == "village")
        st = Streamer(w, m)
        w.player.pos[:] = v.x + (v.x - w.planets[0].center) / np.linalg.norm(v.x - w.planets[0].center) * 3
        for _ in range(10):                       # 1 回に少しずつ置く
            st.update(2.0)
        mine = [cid for cid in w.constructs if cid.startswith(f"blk-{v.id}-")]
        self.assertGreater(len(mine), 3)
        self.assertTrue(all(w.constructs[cid].anchored for cid in mine))
        st.update(2.0)
        # 遠くへ行くと片付く。手を加えた物は残る
        kept = w.constructs[mine[0]]
        kept.version += 1
        st.update(2.0)
        w.player.pos[:] = w.player.pos + 5000.0 * np.array([1.0, 0, 0])
        st.update(2.0)
        left = [cid for cid in w.constructs if cid.startswith(f"blk-{v.id}-")]
        self.assertEqual(left, [mine[0]])


class TestLegacySave(CoreCase):
    def test_old_planet_positions_are_spread(self):
        d = paths.world_dir("old")
        (d / "scripts").mkdir(parents=True, exist_ok=True)
        from main.world import DEFAULT_SETTINGS
        s = dict(DEFAULT_SETTINGS, name="old", format=2, planets=[{"def": "terra", "position": [0, 0, 0], "craters": []},
                                                                  {"def": "luna", "position": [0, 0, 4000], "craters": []}])
        (d / "world.json").write_text(json.dumps(s), encoding="utf-8")
        w = World("old", json.loads(json.dumps(__import__("main.config", fromlist=["x"]).DEFAULTS))).load()
        self.assertAlmostEqual(float(np.linalg.norm(w.planets[1].center0)), 60000.0, delta=1.0)


if __name__ == "__main__":
    unittest.main()
