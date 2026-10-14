"""MOD のプログラム（街・サバイバル）が、画面なしで読み込めて動くことを確かめる（別のプロセスで実行）。"""
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

CODE = r'''
import importlib.abc, json, shutil, sys, tempfile
from pathlib import Path
sys.path.insert(0, ROOT)
class B(importlib.abc.MetaPathFinder):
    def find_spec(self, name, path=None, target=None):
        if name.split(".")[0] in ("ursina", "panda3d", "direct") or name.startswith("main.client"):
            raise ImportError(name)
sys.meta_path.insert(0, B())
import numpy as np
from main import config, log, paths
from main.world import World, DEFAULT_SETTINGS, default_bodies
log.setup("ERROR")
tmp = Path(tempfile.mkdtemp()); paths.SAVES = tmp
d = paths.world_dir("c"); (d / "scripts").mkdir(parents=True)
s = dict(DEFAULT_SETTINGS, name="c", format=2, planets=default_bodies())
(d / "world.json").write_text(json.dumps(s), encoding="utf-8")
(d / "mods.json").write_text(json.dumps({"enabled": ["survival", "city"]}), encoding="utf-8")
cfg = json.loads(json.dumps(config.DEFAULTS))
w = World("c", cfg).load()
assert [a.name for a in w.addons] == ["survival", "city"] and not w.creative
pack = w.addon("city")
p = w.player; iron0 = p.inventory.get("iron", 0)
pl = w.planets[0]; up = pl.up(p.pos); side = np.cross(up, [0, 0, 1.0]); side /= np.linalg.norm(side)
gp = pl.center + up * float(pl.surface_radius(up[None])[0]) + side * 3
placed = w.place({"kind": "terrain", "t": 3, "planet": pl, "point": gp, "normal": up}, ("material", "steel"), 2, side)
iron1 = p.inventory.get("iron", 0)
for k in range(30):
    w.step(1 / 60)
c = [c for c in pack.city_list() if c.homes][0]
w.player.inventory["credits"] = 1000
msg = pack.economy.rent_home(c, c.homes[0])
pack.set_home(c.homes[0]["id"])
w.save()
w2 = World("c", cfg).load()
print(json.dumps({"cities": len(pack.city_list()), "home": w2.addon("city").home_id() == c.homes[0]["id"],
                  "starter": iron0, "paid": iron0 - iron1, "placed": placed, "creative2": w2.creative,
                  "client": [m for m in sys.modules if m.startswith("main.client")]}))
shutil.rmtree(tmp, ignore_errors=True)
'''


class TestCityPack(unittest.TestCase):
    def test_city_pack_without_client(self):
        r = subprocess.run([sys.executable, "-c", CODE.replace("ROOT", repr(str(ROOT)))], capture_output=True,
                           text=True, timeout=600)
        self.assertEqual(r.returncode, 0, r.stderr[-2000:])
        import json
        out = json.loads(r.stdout.strip().splitlines()[-1])
        self.assertGreater(out["cities"], 5)
        self.assertTrue(out["home"])
        self.assertEqual(out["starter"], 250.0)       # サバイバルの最初の資材
        self.assertTrue(out["placed"])
        self.assertGreater(out["paid"], 0)            # 置くと資源が減る
        self.assertFalse(out["creative2"])
        self.assertEqual(out["client"], [])


if __name__ == "__main__":
    unittest.main()
