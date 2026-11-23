"""A-14 のテスト：扉ブロック（E・スクリプトで開け閉め）・気密と浮力・閉じた構築物の中身を描かない。

    python -m unittest discover -s tests -p test_door.py -v      （dev フォルダで）
"""
import unittest

import numpy as np

from test_core import CoreCase, DT
from main import devices, shape
from main.construct import Construct

DOOR = {"def": "door", "dir": 4}


def hollow_box(n, mat="steel", door_cells=()):
    """n×n×n の中が空いた箱。door_cells は扉にするセル"""
    V = []
    for x in range(n):
        for y in range(n):
            for z in range(n):
                if 0 < x < n - 1 and 0 < y < n - 1 and 0 < z < n - 1:
                    continue
                d = dict(DOOR) if (x, y, z) in door_cells else None
                V.append(((x, y, z), "steel" if d else mat, d))
    return V


class TestDoor(CoreCase):
    def make(self, w, n=6, mat="steel", extra=3.0):
        doors = [(n // 2, 1, n - 1), (n // 2, 2, n - 1)]
        c = self.put(w, hollow_box(n, mat, doors), extra=extra)
        return c, [c.index[d] for d in doors]

    def test_e_opens_connected_doors_and_breaks_the_seal(self):
        w = self.world()
        c, ds = self.make(w)
        self.run_for(w, 0.1)
        self.assertEqual(len(devices.rebuild_networks(c).cavities), 1, "閉じた箱の中は気密")
        ver = c.version
        w.interact({"kind": "voxel", "construct": c, "index": ds[0], "face": 4})
        self.assertTrue(all(c.devs[i]["state"]["open"] == 1.0 for i in ds), "となりの扉も一緒に開く")
        self.assertEqual(c.version, ver, "扉の開け閉めでは version は変わらない（手を加えたことにしない）")
        pts = set(map(int, c.topo["points"]))
        self.assertFalse(any(i in pts for i in ds), "開いた扉は形から外れる（通れる・見えない）")
        self.run_for(w, 0.1)
        self.assertEqual(len(devices.rebuild_networks(c).cavities), 0, "扉が開くと気密でなくなる")
        # 開いた扉は E で閉められる（視線はボクセルの箱で当たる）
        p = c.voxel_center(ds[0])
        hit = w.raycast(p + np.array([0, 0, 3.0]), np.array([0, 0, -1.0]), 6.0)
        self.assertEqual(hit and hit.get("index"), ds[0])
        w.interact(hit)
        self.run_for(w, 0.1)
        self.assertEqual(len(devices.rebuild_networks(c).cavities), 1)

    def test_script_opens_door(self):
        w = self.world()
        self.script(w, "d.py", "def tick(api):\n"
                               "    for d in api.devices('door'):\n"
                               "        d.open = 1\n")
        n = 6
        V = hollow_box(n, "steel", [(3, 1, n - 1), (3, 2, n - 1)])
        V += [((0, -1, 0), "steel", {"def": "battery", "dir": 2}),
              ((1, -1, 0), "steel", {"def": "computer", "dir": 2, "state": {"script": "d.py", "running": True}})]
        c = self.put(w, V, extra=3.5)
        self.run_for(w, 0.3)
        doors = [d for d in c.devs if d and d["def"] == "door"]
        self.assertTrue(doors and all(d["state"]["open"] == 1.0 for d in doors))
        self.assertEqual(len(devices.rebuild_networks(c).cavities), 0)

    def test_inner_faces_are_marked(self):
        """閉じた空洞の方を向いた面は inner（遠くからは描かない）。扉を開けると無くなる"""
        w = self.world()
        c, ds = self.make(w, n=6)
        m = shape.build_mesh(c)
        inner = np.asarray(m["inner"], bool)
        self.assertTrue(inner.any() and not inner.all())
        # 外向きの面は inner でない：箱の外の点の方を向いた面がある
        c.devs[ds[0]]["state"]["open"] = 1.0
        c.refresh_shape()
        m2 = shape.build_mesh(c)
        self.assertFalse(np.asarray(m2["inner"], bool).any(), "開いた箱は全部描く")

    def test_flat_mesh(self):
        """A-15・A-16：遠くの構築物の平らなメッシュ（面をまとめる。ガラスと壁は同じ面でつながる・三角形がずっと少ない）"""
        w = self.world()
        n = 10
        V = []
        for x, y, z in [(x, y, z) for x in range(n) for y in range(n) for z in range(n)]:
            if 0 < x < n - 1 and 0 < y < n - 1 and 0 < z < n - 1:
                continue
            glass = y % 2 == 1 and 0 < x < n - 1 and z in (0, n - 1)        # 窓の帯
            V.append(((x, y, z), "glass" if glass else "concrete", None))
        c = self.put(w, V, extra=6.0)
        smooth = shape.build_mesh(c)
        flat = shape.build_mesh(c, None, False)
        self.assertTrue(flat.get("flat"))
        nf, ns = len(flat["faces"]), len(smooth["faces"])
        self.assertLess(nf, ns, f"三角形が少ない（{nf} / {ns}）")
        # ガラスの面はみな箱の表面（0 か n の平面の上）にある：ガラスと壁の境・ガラスどうしの間に面を作らない
        P = np.asarray(flat["rest"])
        F = np.asarray(flat["faces"])
        own = np.asarray(flat["owner"])
        inner = np.asarray(flat["inner"], bool)
        lo, hi = P.min(0), P.max(0)
        glass = np.array([c.mats[int(own[t[0]])] == "glass" for t in F])
        on_surface = np.array([any(np.all(np.isclose(P[t, a], v)) for a in range(3) for v in (lo[a], hi[a])) for t in F])
        self.assertTrue(glass.any())
        self.assertTrue(on_surface[glass & ~inner].all())
        self.assertTrue(inner.any(), "中の空洞の面は inner")
        # なめらかと平らは別々に使い回す
        self.assertIsNotNone(shape.mesh_for(c, False))
        self.assertTrue(shape.mesh_ready(c, False))
        self.assertFalse(shape.mesh_ready(c, True))

    def test_submarine_ballast(self):
        """水の中で：扉を閉じた空洞は浮き、扉を開けると水が入って沈む"""
        from main.world import default_bodies
        w = self.world(planets=default_bodies("gaia", False, False), gravity_scale=1.0)
        pl = w.planets[0]
        rng = np.random.default_rng(3)
        D = rng.normal(size=(4000, 3))
        D /= np.linalg.norm(D, axis=1)[:, None]
        v = np.asarray(pl._tile_values(D))
        deep = np.nonzero(v[:, 1] - v[:, 0] > 60.0)[0]
        self.assertTrue(len(deep))
        d = D[deep[0]]
        sea = float(v[deep[0], 1])

        def drop(open_):
            n = 12
            V = hollow_box(n, "rubber", [(6, 3, n - 1), (6, 4, n - 1)])
            up = d
            t1 = np.cross(up, [1.0, 0, 0]); t1 /= np.linalg.norm(t1)
            R = np.stack([t1, up, np.cross(t1, up)], axis=1)
            c = Construct.from_voxels(w.registry, V, pl.center + d * (sea - 8.0), R, size=0.5, name="浮き")
            w.add_construct(c)
            w.player.pos = pl.center + d * (sea + 20.0)       # 近くに（遠い物は物理が止まる）
            w.player.vel = np.zeros(3)
            w.player.flying = True
            if open_:
                i = next(k for k, x in enumerate(c.devs) if x and x["def"] == "door")
                w.interact({"kind": "voxel", "construct": c, "index": i, "face": 4})
            h0 = float(np.linalg.norm(c.center_of_mass() - pl.center))
            for _ in range(int(3.0 / DT)):
                w.step(DT)
            h1 = float(np.linalg.norm(c.center_of_mass() - pl.center))
            del w.constructs[c.id]
            return h1 - h0

        rise_closed = drop(False)
        rise_open = drop(True)
        self.assertGreater(rise_closed, 0.5, "閉じた空洞は浮く")
        self.assertLess(rise_open, -0.5, "扉を開けると沈む")


if __name__ == "__main__":
    unittest.main()
