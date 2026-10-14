"""コアだけの自動テスト（画面と MOD のプログラムを読み込まずに動くことも確かめる）。

    python -m unittest discover -s tests -v      （alpedia フォルダで）

最初に「画面（ursina / panda3d / main.client）と MOD のプログラム（alpedia_mods）を import したら失敗する」仕掛けを入れてから
コアを読み込むので、コアがそれらに頼っていればここで分かる。セーブは一時フォルダに作る（saves/ には触らない）。
"""
import importlib.abc
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

BLOCKED = ("ursina", "panda3d", "direct", "main.client", "alpedia_mods")


class _Block(importlib.abc.MetaPathFinder):
    def find_spec(self, name, path=None, target=None):
        if any(name == b or name.startswith(b + ".") for b in BLOCKED):
            raise ImportError(f"コアから {name} を読み込もうとしました")
        return None


sys.meta_path.insert(0, _Block())

import numpy as np  # noqa: E402

from main import config, devstate, hooks, log, paths, savefmt  # noqa: E402
from main.construct import Construct  # noqa: E402
from main.control import Port, PortError  # noqa: E402
from main.world import World, DEFAULT_SETTINGS  # noqa: E402

log.setup("ERROR")
CFG = json.loads(json.dumps(config.DEFAULTS))
DT = 1 / 60


class CoreCase(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="alpedia_test_"))
        self._saves = paths.SAVES
        paths.SAVES = self.tmp

    def tearDown(self):
        paths.SAVES = self._saves
        shutil.rmtree(self.tmp, ignore_errors=True)

    def world(self, addons=(), **settings):
        d = paths.world_dir("t")
        (d / "scripts").mkdir(parents=True, exist_ok=True)
        s = dict(DEFAULT_SETTINGS, name="t", format=2, planets=[{"def": "terra", "position": [0, 0, 0], "craters": []}])
        s.update(settings)
        (d / "world.json").write_text(json.dumps(s), encoding="utf-8")
        return World("t", CFG, addons=list(addons)).load()

    def reload(self, w, addons=()):
        w.save()
        return World("t", CFG, addons=list(addons)).load()

    @staticmethod
    def ground(w, extra):
        pl = w.planets[0]
        up = np.array([0.0, 1.0, 0.0])
        r = float(pl.surface_radius(up[None])[0])
        return pl.center + up * (r + extra), up

    def put(self, w, voxels, extra=3.0, name="test"):
        p, _ = self.ground(w, extra)
        c = Construct.from_voxels(w.registry, voxels, p, np.eye(3), size=0.5, name=name)
        w.add_construct(c)
        return c

    def run_for(self, w, seconds):
        for _ in range(int(seconds / DT)):
            w.step(DT)

    def script(self, w, name, code):
        (w.dir / "scripts" / name).write_text(code, encoding="utf-8")


# ---------------------------------------------------------------------------
class TestImports(CoreCase):
    def test_core_does_not_load_client_or_addons(self):
        loaded = [m for m in sys.modules if any(m == b or m.startswith(b + ".") for b in BLOCKED)]
        self.assertEqual(loaded, [])


class TestPhysics(CoreCase):
    def test_block_falls_and_rests(self):
        w = self.world()
        c = self.put(w, [((x, 0, z), "steel", None) for x in range(3) for z in range(3)], extra=4.0)
        self.run_for(w, 4.0)
        alt = float(w.planets[0].sdf(c.center_of_mass()[None])[0])
        self.assertLess(alt, 0.6)
        self.assertGreater(alt, 0.0)
        self.assertLess(float(np.linalg.norm(c.velocity())), 0.3)
        self.assertEqual(len(w.constructs), 1)

    def test_full_rigid_keeps_shape(self):
        w = self.world(full_rigid=True)
        c = self.put(w, [((x, y, 0), "steel", None) for x in range(4) for y in range(2)], extra=6.0)
        P0 = c.pos.copy()
        self.run_for(w, 3.0)
        dist = lambda P: np.linalg.norm(P[:, None] - P[None], axis=2)  # noqa: E731
        err = np.abs(dist(c.pos) - dist(P0)).max()                      # 頂点どうしの距離が変わらない
        self.assertLess(err, 0.02)
        self.assertEqual(len(w.constructs), 1)


class TestScripts(CoreCase):
    def test_script_sets_throttle_and_reports(self):
        w = self.world()
        self.script(w, "t.py", "def tick(api):\n"
                               "    api.memory['n'] = api.memory.get('n', 0) + 1\n"
                               "    for t in api.devices('thruster'):\n"
                               "        t.throttle = 0.25\n"
                               "    api.display('alt %.1f' % api.altitude)\n")
        c = self.put(w, [((0, 0, 0), "steel", {"def": "battery", "dir": 2}),
                         ((1, 0, 0), "steel", {"def": "computer", "dir": 2, "state": {"script": "t.py", "running": True}}),
                         ((2, 0, 0), "steel", {"def": "thruster", "dir": 3})])
        self.run_for(w, 0.5)
        comp = next(d for d in c.devs if d and d["def"] == "computer")
        thr = next(d for d in c.devs if d and d["def"] == "thruster")
        self.assertIsNone(comp["state"].get("error"))
        self.assertGreater(comp["state"]["memory"]["n"], 3)
        self.assertEqual(thr["state"]["throttle"], 0.25)
        self.assertTrue(comp["state"]["display"]["text"].startswith("alt"))

    def test_script_error_stops_computer(self):
        w = self.world()
        self.script(w, "bad.py", "def tick(api):\n    1 / 0\n")
        c = self.put(w, [((0, 0, 0), "steel", {"def": "battery", "dir": 2}),
                         ((1, 0, 0), "steel", {"def": "computer", "dir": 2, "state": {"script": "bad.py", "running": True}})])
        self.run_for(w, 0.2)
        st = next(d for d in c.devs if d and d["def"] == "computer")["state"]
        self.assertFalse(st["running"])
        self.assertIn("ZeroDivisionError", st["error"])
        self.assertEqual(st["error_line"], 2)

    def test_port_only_writes_allowed_fields(self):
        w = self.world()
        c = self.put(w, [((0, 0, 0), "steel", {"def": "thruster", "dir": 3}), ((1, 0, 0), "steel", {"def": "tank", "dir": 2})])
        p = Port(w, c)
        ti = next(i for i, *_ in p.devices() if c.devs[i]["def"] == "thruster")
        p.device_set(ti, "throttle", 5.0)
        self.assertEqual(c.devs[ti]["state"]["throttle"], 1.0)
        with self.assertRaises(PortError):
            p.device_set(ti, "firing", 1.0)
        st = p.device_state(ti)
        st["throttle"] = 0.0                     # コピーなので本体は変わらない
        self.assertEqual(c.devs[ti]["state"]["throttle"], 1.0)

    def test_actuator_rotates(self):
        w = self.world()
        self.script(w, "a.py", "def tick(api):\n"
                               "    a = api.device('arm')\n"
                               "    if not api.memory.get('sent'):\n"
                               "        a.rotate_to(90, speed=180)\n"
                               "        api.memory['sent'] = True\n")
        c = self.put(w, [((0, 0, 0), "steel", {"def": "battery", "dir": 2}),
                         ((1, 0, 0), "steel", {"def": "computer", "dir": 2, "state": {"script": "a.py", "running": True}}),
                         ((2, 0, 0), "steel", {"def": "actuator", "dir": 2, "name": "arm"}),
                         ((2, 1, 0), "steel", None), ((3, 1, 0), "steel", None)])
        self.run_for(w, 2.0)
        st = next(d for d in c.devs if d and d["def"] == "actuator")["state"]
        self.assertAlmostEqual(st["angle"], 90.0, places=3)
        self.assertEqual(len(w.constructs), 1)


class TestRadio(CoreCase):
    def _station(self, w, offset, script, antenna=True):
        vox = [((0, 0, 0), "steel", {"def": "battery", "dir": 2}),
               ((1, 0, 0), "steel", {"def": "computer", "dir": 2, "state": {"script": script, "running": True}})]
        if antenna:
            vox.append(((2, 0, 0), "aluminum", {"def": "antenna", "dir": 2}))
        c = self.put(w, vox)
        c.pos += np.array(offset, float)
        return c

    def test_send_receive_and_range(self):
        w = self.world()
        self.script(w, "tx.py", "def tick(api):\n    api.send('ch', {'t': api.time})\n")
        self.script(w, "rx.py", "def tick(api):\n"
                                "    got = api.receive('ch')\n"
                                "    if got:\n"
                                "        api.memory['last'] = got[-1]['data']['t']\n"
                                "        api.memory['dist'] = got[-1]['distance']\n")
        self._station(w, (0, 0, 0), "tx.py")
        rx = self._station(w, (0, 0, 8), "rx.py")
        self.run_for(w, 0.5)
        mem = next(d for d in rx.devs if d and d["def"] == "computer")["state"]["memory"]
        self.assertGreater(mem.get("last", 0), 0.2)
        self.assertLess(abs(mem["dist"] - 8.0), 1.0)

    def test_no_antenna_is_an_error(self):
        w = self.world()
        self.script(w, "tx.py", "def tick(api):\n    api.send('ch', 1)\n")
        c = self._station(w, (0, 0, 0), "tx.py", antenna=False)
        self.run_for(w, 0.2)
        st = next(d for d in c.devs if d and d["def"] == "computer")["state"]
        self.assertFalse(st["running"])
        self.assertIn("アンテナ", st["error"])


class TestFormats(CoreCase):
    def test_devstate_normalize(self):
        st = devstate.normalize("actuator", {"speed": 0.3}, {"ext": 1, "bogus": 3, "_env": {}})
        self.assertNotIn("bogus", st)
        self.assertIn("_env", st)
        self.assertEqual(st["ext_target"], 1.0)
        self.assertEqual(st["ext_speed"], 0.3)
        self.assertEqual(devstate.check("actuator", st), [])
        st["attached"] = True
        self.assertNotIn("attached", devstate.to_saved("actuator", st))

    def test_old_world_settings_become_mods(self):
        s, mods, up = savefmt.upgrade_world({"creative": False, "voxel_size": 0.5}, [])
        self.assertEqual((mods, up, "creative" in s, s["format"]), (["survival", "city"], True, False, 2))
        s, mods, _ = savefmt.upgrade_world({"creative": True, "cities": False}, ["example_mod"])
        self.assertEqual(mods, ["example_mod"])
        self.assertEqual(savefmt.upgrade_world({"format": 2}, ["city"])[1:], (["city"], False))

    def test_creative_without_rule_mods(self):
        w = self.world()
        self.assertTrue(w.creative)
        c = self.put(w, [((0, 0, 0), "steel", {"def": "tank", "dir": 2})])
        w.interact({"kind": "voxel", "construct": c, "index": 0, "face": 2})
        self.assertGreater(c.devs[0]["state"]["contents"].get("fuel", 0), 0)   # E で燃料満タン

    def test_save_roundtrip(self):
        w = self.world()
        self.put(w, [((0, 0, 0), "steel", {"def": "tank", "dir": 2, "state": {"contents": {"fuel": 100.0}}}),
                     ((1, 0, 0), "steel", {"def": "thruster", "dir": 3}), ((2, 0, 0), "steel", {"def": "battery", "dir": 2})])
        self.put(w, [((0, 0, 0), "rock", None)], extra=8.0, name="rock")
        self.run_for(w, 1.0)
        before = {c.id: (c.center_of_mass(), c.velocity()) for c in w.constructs.values()}
        w2 = self.reload(w)
        for c in w2.constructs.values():
            self.assertLess(np.abs(before[c.id][0] - c.center_of_mass()).max(), 1e-3)
            self.assertLess(np.abs(before[c.id][1] - c.velocity()).max(), 1e-3)
        d = w2.dir
        self.assertEqual(savefmt.check_world(json.loads((d / "world.json").read_text("utf-8"))), [])
        for cd in json.loads((d / "constructs.json").read_text("utf-8")):
            self.assertEqual(savefmt.check_construct(cd), [])
            for v in cd["voxels"]:
                if v[4]:
                    dd = w2.registry.device(v[4]["def"])
                    self.assertEqual(devstate.check(dd["type"], devstate.normalize(dd["type"], dd["params"], v[4]["state"])), [])
        self.assertEqual(savefmt.check_spawn(json.loads((d / "spawn.json").read_text("utf-8"))), [])
        self.assertEqual(savefmt.check_player(json.loads((d / "player.json").read_text("utf-8"))), [])


class _Floor:
    """テスト用の拡張の物体（止まっている箱）。"""
    def is_moving(self):
        return False

    def point_velocity(self, p):
        return np.zeros((len(np.atleast_2d(p)), 3))


class FloorAddon(hooks.Addon):
    """地面から少し上に浮いた大きな板（当たり判定の箱）を出す拡張。"""
    name = "floor"

    def __init__(self, center, up):
        self.owner = _Floor()
        self.C = np.array([center])
        self.A = np.array([np.stack([np.cross(up, [0, 0, 1.0]), up, [0, 0, 1.0]])])
        self.E = np.array([[6.0, 0.5, 6.0]])
        self.events = 0
        self.loaded = False

    def on_load(self, world):
        self.loaded = True

    def colliders(self, world, lo, hi, margin):
        return [(self.owner, self.C, self.A, self.E, np.array([0]))]

    def after_physics(self, world, cons):
        self.events += sum(len(c.ext_events) for c in cons)

    def push_sphere(self, world, center, r):
        from main.geom import box_sdf
        d, n, _ = box_sdf(np.asarray(center)[None], self.C, self.A, self.E)
        return [(n[0] * (r - d[0]), n[0], self.owner)] if d[0] < r else []

    def raycast(self, world, o, dvec, max_t):
        from main.geom import ray_boxes
        r = ray_boxes(o, dvec, self.C, self.A, self.E, max_t)
        return {"kind": "floor", "t": r[0], "point": o + dvec * r[0], "normal": r[2], "placeable": True} if r else None


class TestHooks(CoreCase):
    def test_addon_collider_raycast_and_player(self):
        w0 = self.world()
        top, up = self.ground(w0, 10.0)
        a = FloorAddon(top, up)
        w = World("t", CFG, addons=[a]).load()
        self.assertTrue(a.loaded)
        c = self.put(w, [((x, 0, z), "steel", None) for x in range(2) for z in range(2)], extra=13.0)
        c.vel[:] = -up * 12.0
        self.run_for(w, 2.0)
        h = float((c.center_of_mass() - top) @ up)
        self.assertGreater(h, 0.5)              # 板の上に乗っている（地面まで落ちていない）
        self.assertLess(h, 1.2)
        self.assertGreater(a.events, 0)         # 強く当たった記録が拡張に届いた
        hit = w.raycast(top + up * 5 + np.array([3.0, 0, 3.0]), -up, 20.0)
        self.assertEqual(hit["kind"], "floor")
        spot = top + up * 0.1 + np.array([4.0, 0, -4.0])             # 構築物の無い所・板の中
        w.player.pos = spot.copy()
        w.player.vel[:] = 0
        self.run_for(w, 0.1)
        self.assertGreater(float((w.player.pos - spot) @ up), 0.4)  # 板の中から押し出された


if __name__ == "__main__":
    unittest.main()
