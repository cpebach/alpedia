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


class TestVolcano(CoreCase):
    """A-13：火山（円すいの山と火口）。火口のまわりに硫黄・上の方は黒い岩・火口の底は溶岩の色。外側の球も広がる"""

    def check(self, pdef, n):
        w = self.world(planets=[{"def": pdef, "position": [0, 0, 0], "craters": []}])
        pl = w.planets[0]
        vs = pl._nat().volcanoes()
        self.assertEqual(len(vs), n)
        d, r, h, cr = vs[0]
        d = np.asarray(d, float)
        up = np.array([0.0, 1.0, 0.0])
        self.assertGreater(np.arccos(np.clip(d @ up, -1, 1)) * pl.radius, r + 3000, "出現地点のそばには無い")
        t1 = np.cross(d, [1.0, 0, 0]); t1 /= np.linalg.norm(t1)

        def at(m):
            q = d * pl.radius + t1 * m
            return q / np.linalg.norm(q)
        hs = np.asarray(pl._tile_values(np.array([at(0), at(cr), at(r * 0.5), at(r * 1.3)])))[:, 0] - pl.radius
        self.assertGreater(hs[1], hs[0] + h * 0.1, "火口はへこんでいる")
        self.assertGreater(hs[1], hs[2] + h * 0.2, "火口のふちがいちばん高い")
        self.assertGreater(hs[2], hs[3] + h * 0.1, "裾より中腹が高い")
        self.assertGreaterEqual(pl.bound - pl.radius, h)
        # 火口のまわりに硫黄（まだら）・火口の底は溶岩の色
        rng = np.random.default_rng(0)
        pts = np.array([at(m) for m in rng.uniform(0, cr * 1.5, 400)])
        hh = np.asarray(pl._tile_values(pts))[:, 0] - pl.radius
        sul = next(np.asarray(wv) for dep, wv in pl._deposit_weights(pts, hh) if dep["resource"] == "sulfur")
        self.assertTrue(0.05 < float((sul > 0.5).mean()) < 0.8)
        self.assertEqual(pl.resource_at(pl.center + pts[int(np.argmax(sul))] * (pl.radius + hh[int(np.argmax(sul))] - 0.3)), "sulfur")
        vp = np.asarray(pl._nat().volcano_paint(np.array([at(0), at(r * 1.2)])))
        self.assertGreater(vp[0, 2], 0.9)
        self.assertEqual(float(vp[1].max()), 0.0)
        # 同じ惑星なら毎回同じ
        w2 = self.world(planets=[{"def": pdef, "position": [0, 0, 0], "craters": []}])
        self.assertEqual(w2.planets[0]._nat().volcanoes(), vs)

    def test_terra(self):
        self.check("terra", 6)

    def test_gaia(self):
        self.check("gaia", 8)


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


class TestEarthMod(CoreCase):
    """地球の名所（Earth MOD）：10 の地形が読み込まれ、それぞれの形になっている"""

    def test_landmarks(self):
        from main.world import default_bodies
        d = paths.world_dir("t")
        (d / "scripts").mkdir(parents=True, exist_ok=True)
        (d / "mods.json").write_text(json.dumps({"enabled": ["earth"]}), encoding="utf-8")
        w = self.world(planets=default_bodies("earth", False, False))
        pl = w.planets[0]
        fs = {f["kind"]: f for f in pl.defn["features"]}
        self.assertEqual(len(fs), 10)

        def at(kind, dx=0.0, dz=0.0, n=0, spread=0.0):
            f = fs[kind]
            x, z = f["at"][0] + dx, f["at"][1] + dz
            if n:
                rng = np.random.default_rng(1)
                pts = np.array([[x, z]]) + rng.uniform(-spread, spread, size=(n, 2))
            else:
                pts = np.array([[x, z]])
            dist = np.hypot(pts[:, 0], pts[:, 1])
            a = dist / pl.radius
            dirs = np.stack([np.sin(a) * pts[:, 0] / dist, np.cos(a), np.sin(a) * pts[:, 1] / dist], 1)
            v = np.asarray(pl._tile_values(dirs))
            return v[:, 0] - pl.radius, v[:, 1] - pl.radius, dirs

        h, wv, _ = at("canyon", n=400, spread=1500)
        self.assertGreater(h.max() - h.min(), 300, "峡谷は深い")
        h, wv, _ = at("dune_lagoons", n=400, spread=1200)
        self.assertGreater((wv > h).mean(), 0.1, "砂丘の間に湖")
        h, wv, _ = at("pillars", n=600, spread=500)
        self.assertGreater(h.max(), 120, "高い石柱")
        h, wv, dirs = at("high_mountains", n=300, spread=2000)
        self.assertGreater(h.max(), 1000, "高い山脈")
        top = dirs[int(np.argmax(h))][None]
        t_, _ = np.asarray(pl._nat().climate(top, np.array([h.max()])))[0]
        self.assertLess(t_, 0.0, "高い所は雪")
        h, wv, _ = at("travertine", n=300, spread=300)
        self.assertGreater((wv > h).mean(), 0.05, "石灰棚の水たまり")
        h, wv, _ = at("cascade_lakes", n=300, spread=400)
        self.assertGreater((wv > h).mean(), 0.05, "段々の湖")

    def test_waterfalls_at_cliffs_and_lake_steps(self):
        from main.world import default_bodies
        d = paths.world_dir("t")
        (d / "scripts").mkdir(parents=True, exist_ok=True)
        (d / "mods.json").write_text(json.dumps({"enabled": ["earth"]}), encoding="utf-8")
        w = self.world(planets=default_bodies("earth", False, False))
        pl = w.planets[0]
        f = np.asarray(pl._nat().waterfalls())
        self.assertGreaterEqual(len(f), 4, "テプイの崖と段々の湖に滝")
        top, bot = f[:, :3], f[:, 3:6]
        drop = np.linalg.norm(top, axis=1) - np.linalg.norm(bot, axis=1)
        self.assertTrue((drop > 5).all(), f"落差 {drop}")
        self.assertGreater(drop.max(), 150, "テプイの滝は高い")


class TestRivers(CoreCase):
    """川（A-07）：陸に、海へ向かう細い水の筋。地面は川の所だけ削れている"""

    def test_rivers_on_land(self):
        w = self.world(planets=[{"def": "gaia", "position": [0, 0, 0], "craters": []}])
        pl = w.planets[0]
        self.assertGreater(pl.defn.get("rivers", 0), 0)
        rng = np.random.default_rng(3)
        d = rng.normal(size=(60000, 3))
        d /= np.linalg.norm(d, axis=1)[:, None]
        v = np.asarray(pl._tile_values(d))
        h, wv = v[:, 0] - pl.radius, v[:, 1] - pl.radius
        land = h > 2.0
        river = land & (wv > h)
        self.assertGreater(river.sum(), 20, "陸に川")
        self.assertLess(river.sum() / land.sum(), 0.15, "川は細い")
        # 川は谷：まわり（少し離れた所）より低い
        i = np.flatnonzero(river)[:20]
        side = d[i] + np.cross(d[i], [0.0, 1.0, 0.0]) * (150.0 / pl.radius)
        side /= np.linalg.norm(side, axis=1)[:, None]
        hs = np.asarray(pl._tile_values(side))[:, 0] - pl.radius
        self.assertGreater(float(np.median(hs - h[i])), 1.0)


if __name__ == "__main__":
    unittest.main()
