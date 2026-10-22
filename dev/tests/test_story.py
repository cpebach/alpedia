"""ストーリーのチュートリアル（mods/story）のテスト。

- ワールドを作ると、舞台の構築物が地面・景色の箱にめり込まずに置かれ、動かしても壊れない
- 序章から終章まで、各章の目標を（画面の操作の代わりに world の操作で）済ませると最後まで進む
  （第7章は航法プログラム moon.py で本当に月まで飛んで着陸する）
- 保存して読み込み直しても、進み具合はそのまま（構築物を作り直さない）
- 章の一覧の「ここから」（jump）で、どの章からでも始められる
移植・更新のときは、このテストが通ることと、docs/ストーリー台本.md の表を見比べる。
"""
import importlib
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from main import config, log, modcode, paths  # noqa: E402
from main.construct import DIRS  # noqa: E402
from main.geom import box_sdf  # noqa: E402
from main.tutorial import create_tutorial  # noqa: E402
from main.world import World  # noqa: E402

log.setup("CRITICAL")
CFG = json.loads(json.dumps(config.DEFAULTS))


class StoryCase(unittest.TestCase):
    def setUp(self):
        self._blockers = [f for f in sys.meta_path if type(f).__name__ == "_Block"]
        for f in self._blockers:
            sys.meta_path.remove(f)
        self.tmp = Path(tempfile.mkdtemp(prefix="alpedia_story_"))
        self._saves = paths.SAVES
        paths.SAVES = self.tmp
        create_tutorial("tutorial")
        self.w = World("tutorial", CFG).load()
        self.s = self.w.addon("story")
        self.sites = importlib.import_module("alpedia_mods.story.sites")
        self.script = importlib.import_module("alpedia_mods.story.script")

    def tearDown(self):
        paths.SAVES = self._saves
        modcode.reset()
        shutil.rmtree(self.tmp, ignore_errors=True)
        for f in self._blockers:
            sys.meta_path.insert(0, f)

    # --- 道具 ---------------------------------------------------------------
    def run_for(self, sec, dt=1 / 60):
        for _ in range(int(round(sec / dt))):
            self.w.step(dt)

    def ch(self):
        return self.script.CHAPTER_IDS[self.s.st["ch"]] if self.s.st["ch"] < len(self.script.CHAPTERS) else "end"

    def tp(self, x, z, lift=1.2):
        p, up = self.s.map.ground(x, z, lift)
        pl = self.w.player
        pl.pos, pl.vel, pl.up, pl.seated = p, np.zeros(3), up, None

    def use(self, role, def_id, name=None):
        c, i = self.s.dev(role, def_id, name)
        self.assertIsNotNone(i, f"{role} の {def_id}")
        self.w.interact({"kind": "voxel", "construct": c, "index": i, "face": 2})

    def place_on_ship(self, cell, item):
        sh = self.s.con("ship")
        for f, d in enumerate(DIRS):
            nb = tuple(np.array(cell) - d)
            if nb in sh.index:
                return self.w.place({"kind": "voxel", "construct": sh, "index": sh.index[nb], "face": f}, item, 0,
                                    np.array([0, 0, 1.0]))
        return False

    # --- テスト -------------------------------------------------------------
    def test_world_is_built_without_overlaps(self):
        s, w = self.s, self.w
        self.assertTrue(s.active and s.st["built"])
        self.assertEqual(self.ch(), "wake")
        self.assertEqual(len(w.constructs), len(s.st["ids"]))
        counts = {}
        for role, cid in s.st["ids"].items():
            c = w.constructs[cid]
            pl = w.nearest_planet(c.pos.mean(0))
            self.assertGreater(float(pl.sdf(c.pos).min()), 0.0, f"{role} が地面にめり込んでいる")
            P = c.voxel_centers()
            for sc in s.scenes:
                d, _, _ = box_sdf(P, sc.C[sc.col], sc.A[sc.col], sc.E[sc.col])
                self.assertGreater(float(d.min()), c.size * 0.5, f"{role} が景色の箱にめり込んでいる")
            counts[role] = c.n_vox
        self.run_for(2.0)
        for role, cid in s.st["ids"].items():
            self.assertEqual(w.constructs[cid].n_vox, counts[role], f"{role} が壊れた")
        for f in ("moon.py", "bike.py", "beacon.py", "power.py", "kadai.blocks.json", "otehon.py"):
            self.assertTrue((w.dir / "scripts" / f).exists(), f)

    def test_playthrough_to_the_moon(self):
        s, w, sites = self.s, self.w, self.sites
        # 序章：見回す・ビーコンまで歩く・起動
        for k in range(20):
            a = k * 0.15
            w.player.look_fwd = np.array([np.sin(a), 0.0, np.cos(a)])
            self.run_for(0.05)
        self.tp(*s.map_xz(s.con("beacon").center_of_mass()))
        self.run_for(0.4)
        self.use("beacon", "computer")
        self.run_for(0.4)
        self.assertEqual(self.ch(), "valley")
        # 第1章：掘る・落石に通り道をあける・段差を越える
        p, up = s.map.ground(0.0, 12.0)
        for _ in range(3):
            w.dig({"kind": "terrain", "planet": s.terra, "point": p, "normal": up}, 0.8)
        self.run_for(0.4)
        self.assertFalse(s.st["goals"].get("rocks"))
        c = s.con("rocks")
        rm = [i for i in range(c.n_vox) if abs(s.map_xz(c.pos[8 * i:8 * i + 8].mean(0))[0]) < 0.7]
        for i in sorted(rm, reverse=True):
            w.remove({"kind": "voxel", "construct": c, "index": i})
        self.run_for(0.6)
        self.assertTrue(s.st["goals"].get("rocks"))
        self.tp(0.0, 42.0)
        self.run_for(0.4)
        self.assertEqual(self.ch(), "power")
        # 第2章：バルブを開ける → 背中合わせの推進機2つに点火（発電）。押す力は打ち消し合って動かない
        pc = s.con("power")
        thr = [i for i, d in enumerate(pc.devs) if d and d["def"] == "thruster"]
        w.interact({"kind": "voxel", "construct": pc, "index": thr[0], "face": 2})
        self.run_for(0.3)
        self.assertFalse(pc.devs[thr[0]]["state"]["lit"], "バルブが閉じていると点火できない")
        self.use("power", "valve")
        for i in thr:
            w.interact({"kind": "voxel", "construct": pc, "index": i, "face": 2})
        p0 = pc.center_of_mass()
        self.run_for(0.6)
        self.assertEqual(self.ch(), "control")
        self.assertTrue(s.st["lights"])
        self.assertGreater(pc.status["production"], 0)
        self.run_for(3.0)
        self.assertLess(float(np.linalg.norm(pc.center_of_mass() - p0)), 0.05, "発電用の推進機で電源が動いた")
        self.assertEqual(pc.n_vox, len(self.sites.power_voxels()), "電源が壊れた")
        # 第3章：お手本を見る・課題（お手本と同じプログラムにして棒を立てる）
        self.tp(*s.map_xz(s.con("model").center_of_mass()))
        c, i = s.dev("task", "computer", "課題")
        st = c.devs[i]["state"]
        self.run_for(3.0)
        self.assertLess(s.st["rod_t"], 1.0, "プログラムを直す前は棒が揺れる")
        st.update(script="otehon.py", blocks="otehon.blocks.json", running=False)
        self.run_for(0.2)
        st["running"] = True
        self.run_for(8.0)
        self.assertEqual(self.ch(), "drone")
        # 第4章：操縦台に座る・ドローンで輪をくぐる
        cc, ci = s.dev("console", "seat")
        w.player.seated = (cc.id, cc.cells[ci])
        self.run_for(0.4)
        d = s.con("drone")
        d.pos += s.points["ring"] - d.center_of_mass()
        self.run_for(0.4)
        self.assertEqual(self.ch(), "bike")
        w.player.seated = None
        # 第5章：遮断機を上げる・バイクに乗る・船まで行く
        self.use("gate", "actuator")
        self.run_for(4.0)
        self.assertTrue(s.st["goals"].get("gate"))
        bc, bi = s.dev("bike", "seat")
        w.player.seated = (bc.id, bc.cells[bi])
        self.run_for(0.4)
        self.tp(0.0, 141.0)
        self.run_for(0.4)
        self.assertEqual(self.ch(), "repair")
        # 第6章：穴をふさぐ・推進機（向きは自動で上向き）・燃料
        for h in sites.SHIP_HOLES:
            self.assertTrue(self.place_on_ship(h, ("material", "steel")), h)
        for t in sites.SHIP_THRUSTERS:
            self.assertTrue(self.place_on_ship(t, ("device", "thruster")), t)
        sh = s.con("ship")
        self.assertEqual([sh.devs[sh.index[t]]["dir"] for t in sites.SHIP_THRUSTERS], [2] * len(sites.SHIP_THRUSTERS))
        for i, dv in enumerate(sh.devs):
            if dv and dv["def"] == "tank" and dv["state"].get("contents", {}).get("fuel", 0) < 790:
                w.interact({"kind": "voxel", "construct": sh, "index": i, "face": 2})
        self.run_for(0.6)
        self.assertEqual(self.ch(), "launch")
        c, i = s.dev("ship", "computer")
        self.assertEqual(c.devs[i]["state"]["script"], "moon.py")
        # 第7章：コンピューターを起動して座る → 月まで自動で飛んで着陸
        self.use("ship", "computer")
        self.run_for(1.0)
        self.assertLess(float(np.linalg.norm(s.con("ship").velocity())), 0.3, "座るまでは飛ばない")
        self.use("ship", "seat")
        for _ in range(16):
            self.run_for(10.0, 1 / 30)
            if self.ch() != "launch":
                break
        self.assertEqual(self.ch(), "meet")
        self.assertEqual(s.con("ship").n_vox, len(sites.ship_voxels()))
        self.assertEqual(set(s.st["events"]), {"lift", "space", "half", "near"})
        # 終章：ミオに会う
        w.player.seated = None
        s.talk("mio")
        self.run_for(0.4)
        self.assertTrue(s.st["done"])

    def test_save_and_reload_keeps_progress(self):
        s, w = self.s, self.w
        s.jump(3)
        self.run_for(0.5)
        n = len(w.constructs)
        w.save()
        modcode.reset()
        w2 = World("tutorial", CFG).load()
        s2 = w2.addon("story")
        self.assertEqual(s2.st["ch"], 3)
        self.assertTrue(s2.st["lights"])
        self.assertEqual(len(w2.constructs), n)
        self.assertEqual(len(s2.terra.craters), len(s.terra.craters))
        self.assertTrue(np.allclose(s2.scenes[0].C, s.scenes[0].C))

    def test_jump_to_every_chapter(self):
        s, w = self.s, self.w
        for k, c in enumerate(self.script.CHAPTERS):
            s.jump(k)
            self.run_for(0.3)
            self.assertEqual(s.st["ch"], k, c["id"])
            near = w.nearest_planet(w.player.pos)
            self.assertEqual(near.defn["id"], "luna" if c["id"] == "meet" else "terra", c["id"])
        s.jump(self.script.CHAPTER_IDS.index("launch"))
        self.run_for(0.5)
        self.assertTrue(s.check_hull() and s.check_thrusters() and s.check_fuel())


if __name__ == "__main__":
    unittest.main()
