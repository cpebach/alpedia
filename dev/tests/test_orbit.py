"""惑星の公転（L-08）のテスト：月が回る・月の上の物が月と一緒に動く・保存して読み直しても同じ位置・公転を止められる。

    python -m unittest tests.test_orbit -v      （dev フォルダで）
"""
import unittest

import numpy as np

from test_core import CoreCase, DT
from main.construct import Construct

TWO = [{"def": "terra", "position": [0, 0, 0], "craters": []},
       {"def": "luna", "position": [0, 0, 60000], "craters": []}]


class TestOrbit(CoreCase):
    def two(self, **kw):
        return self.world(planets=[dict(p) for p in TWO], **kw)

    def on_luna(self, w, voxels, extra=2.0):
        lu = w.planets[1]
        up = np.array([0.0, 1.0, 0.0])
        r = float(lu.surface_radius(up[None])[0])
        c = Construct.from_voxels(w.registry, voxels, lu.center + up * (r + extra), np.eye(3), size=0.5, name="on_luna")
        w.add_construct(c)
        return c

    def test_moon_orbits_the_main_planet(self):
        w = self.two()
        te, lu = w.planets
        self.assertFalse(te.orbiting)
        self.assertTrue(lu.orbiting)
        c0 = np.array(lu.center, float)
        self.run_for(w, 30.0)
        c1 = np.array(lu.center, float)
        self.assertAlmostEqual(float(np.linalg.norm(c1)), 60000.0, delta=1e-6)     # 円軌道
        moved = float(np.linalg.norm(c1 - c0))
        # 速さ √(GM/r)（2つの惑星の GM の和）× 30 秒くらい動く
        v = np.sqrt((te.gm + lu.gm) / 60000.0 ** 3) * 60000.0
        self.assertAlmostEqual(moved, v * 30.0, delta=v * 30.0 * 0.02)
        self.assertGreater(lu.soi, lu.radius)
        self.assertAlmostEqual(float(np.linalg.norm(lu.orbit_vel)), v, delta=v * 1e-6)

    def test_things_on_the_moon_move_with_it(self):
        w = self.two()
        lu = w.planets[1]
        c = self.on_luna(w, [((x, 0, z), "steel", None) for x in range(3) for z in range(3)])
        # プレイヤーも月へ（遠い物は物理を止めるので）
        up = np.array([0.0, 1.0, 0.0])
        w.player.pos[:] = lu.center + up * (float(lu.surface_radius(up[None])[0]) + 1.0) + np.array([8.0, 0.0, 0.0])
        self.run_for(w, 3.0)
        d0 = c.center_of_mass() - lu.center
        self.run_for(w, 20.0)
        d1 = c.center_of_mass() - lu.center
        # 月から見て止まっている（月は 20 秒で数百 m 動く）
        self.assertLess(float(np.linalg.norm(d1 - d0)), 0.3)
        self.assertLess(float(lu.sdf(c.center_of_mass()[None])[0]), 1.0)

    def test_player_on_the_moon_moves_with_it(self):
        w = self.two()
        lu = w.planets[1]
        up = np.array([0.0, 1.0, 0.0])
        r = float(lu.surface_radius(up[None])[0])
        w.player.pos[:] = lu.center + up * (r + 1.0)
        w.player.vel[:] = 0
        self.run_for(w, 3.0)
        d0 = w.player.pos - lu.center
        self.run_for(w, 10.0)
        d1 = w.player.pos - lu.center
        self.assertLess(float(np.linalg.norm(d1 - d0)), 0.3)

    def test_leaving_the_moon_keeps_world_velocity(self):
        w = self.two()
        lu = w.planets[1]
        # 月の影響圏のすぐ内側に、月に対して止まっている物を置き、影響圏の外へ押し出す
        p = lu.center + np.array([0.0, lu.soi - 1.0, 0.0])
        c = Construct.from_voxels(w.registry, [((0, 0, 0), "steel", None)], p, np.eye(3), size=0.5, name="probe")
        w.add_construct(c)
        w.step(DT)
        self.assertEqual(c.frame, 1)
        v_world = c.vel.mean(0) + lu.orbit_vel
        c.pos[:] += np.array([0.0, 3.0, 0.0])
        w.step(DT)
        self.assertEqual(c.frame, -1)
        # 世界から見た速度は続いている（重力 1 ティック分の違いだけ）
        self.assertLess(float(np.linalg.norm(c.vel.mean(0) - v_world)), 0.5)

    def test_reload_keeps_the_orbit(self):
        w = self.two()
        self.run_for(w, 5.0)
        c = np.array(w.planets[1].center, float)
        w2 = self.reload(w)
        self.assertTrue(w2.planets[1].orbiting)
        self.assertLess(float(np.linalg.norm(np.array(w2.planets[1].center, float) - c)), 1e-6)
        w2.step(DT)
        self.assertGreater(float(np.linalg.norm(np.array(w2.planets[1].center, float) - c)), 0.0)

    def test_orbit_can_be_turned_off(self):
        pl = [dict(p) for p in TWO]
        pl[1]["orbit"] = False
        w = self.world(planets=pl)
        c0 = np.array(w.planets[1].center, float)
        self.run_for(w, 2.0)
        self.assertFalse(w.planets[1].orbiting)
        self.assertEqual(float(np.linalg.norm(np.array(w.planets[1].center, float) - c0)), 0.0)


if __name__ == "__main__":
    unittest.main()
