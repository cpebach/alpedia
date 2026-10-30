"""シナリオモード（mods/story「それでも」）のテスト。

- シナリオモードの MOD を選んでワールドを作ると、依存する MOD（再現ブロック・登場人物・ルナ・ポート・アマツ）も入り、
  舞台の構築物が地面・景色の箱にめり込まずに置かれ、動かしても壊れない
- 序章から終章まで、各章の目標を（画面の操作の代わりに world の操作で）済ませると最後まで進む
  （第9章は航法のブロックプログラム moon.blocks.json で本当に月まで飛んで着陸する。
   第3章は「待つ」ブロックの手順、第11章は迎撃のブロックプログラム、第12章はロボットの操縦で進める）
- 保存して読み込み直しても、進み具合はそのまま（構築物を作り直さない）。前の版のセーブも続きから遊べる
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

from main import blocks as B  # noqa: E402
from main import config, log, modcode, paths  # noqa: E402
from main.construct import DIRS  # noqa: E402
from main.geom import box_sdf  # noqa: E402
from main.tutorial import create_tutorial  # noqa: E402
from main.world import World  # noqa: E402

log.setup("CRITICAL")
CFG = json.loads(json.dumps(config.DEFAULTS))
NAME = "scenario"


class StoryCase(unittest.TestCase):
    def setUp(self):
        self._blockers = [f for f in sys.meta_path if type(f).__name__ == "_Block"]
        for f in self._blockers:
            sys.meta_path.remove(f)
        self.tmp = Path(tempfile.mkdtemp(prefix="alpedia_story_"))
        self._saves = paths.SAVES
        paths.SAVES = self.tmp
        create_tutorial(NAME)
        self.w = World(NAME, CFG).load()
        self.s = self.w.addon("story")
        self.sites = importlib.import_module("alpedia_mods.story.sites")
        self.space = importlib.import_module("alpedia_mods.story.space")
        self.script = importlib.import_module("alpedia_mods.story.script")
        self.programs = importlib.import_module("alpedia_mods.story.programs")

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

    def idx(self, cid):
        return self.script.CHAPTER_IDS.index(cid)

    def tp(self, x, z, lift=1.2):
        p, up = self.s.map.ground(x, z, lift)
        pl = self.w.player
        pl.pos, pl.vel, pl.up, pl.seated = p, np.zeros(3), up, None

    def use(self, role, def_id, name=None):
        c, i = self.s.dev(role, def_id, name)
        self.assertIsNotNone(i, f"{role} の {def_id}")
        self.w.interact({"kind": "voxel", "construct": c, "index": i, "face": 2})

    def sit(self, role):
        c, i = self.s.dev(role, "seat")
        self.w.player.seated = (c.id, c.cells[i])

    def place_on(self, role, cell, item):
        sh = self.s.con(role)
        for f, d in enumerate(DIRS):
            nb = tuple(np.array(cell) - d)
            if nb in sh.index:
                return self.w.place({"kind": "voxel", "construct": sh, "index": sh.index[nb], "face": f}, item, 0,
                                    np.array([0, 0, 1.0]))
        return False

    def save_prog(self, base, prog):
        sd = self.w.dir / "scripts"
        B.save(sd / f"{base}.blocks.json", prog)
        (sd / f"{base}.py").write_text(B.to_python(prog, f"{base}.blocks.json"), encoding="utf-8")
        self.w.runner.invalidate(f"{base}.py")

    def until(self, cid, sec, dt=1 / 60, keys=None):
        """章 cid が終わるまで（最大 sec 秒）進める。"""
        for _ in range(int(sec)):
            if keys is not None:
                self.w.input_keys = set(keys(self))
            self.run_for(1.0, dt)
            if self.ch() != cid:
                return True
        return False

    # --- テスト -------------------------------------------------------------
    def test_world_is_built_without_overlaps(self):
        s, w = self.s, self.w
        self.assertEqual(set(w.mods), {"story", "scenario_blocks", "soredemo_cast", "skinmodel(dummy)", "lunaport",
                                       "amatsu"})
        self.assertTrue(s.active and s.st["built"])
        self.assertEqual(self.ch(), "wake")
        self.assertEqual(s.st["movies"], ["01_prologue"], "序章のムービーを流す")
        self.assertEqual(w.player.defn["id"], "towa", "見た目はトワ")
        self.assertEqual(len(w.constructs), len(s.st["ids"]))
        counts = {}
        space_roles = {"interceptor", "robot", "krobot", "unit"}
        for role, cid in s.st["ids"].items():
            c = w.constructs[cid]
            pl = w.nearest_planet(c.pos.mean(0))
            self.assertGreater(float(pl.sdf(c.pos).min()), 0.0, f"{role} が地面にめり込んでいる")
            P = c.voxel_centers()
            for sc in s.scenes + s.port.scenes + (s.amatsu.scenes if role in space_roles else []):
                if not sc.col.any():
                    continue
                d, _, _ = box_sdf(P, sc.C[sc.col], sc.A[sc.col], sc.E[sc.col])
                self.assertGreater(float(d.min()), c.size * 0.5 - 0.01, f"{role} が景色の箱にめり込んでいる")
            counts[role] = c.n_vox
        self.run_for(2.0)
        for role, cid in s.st["ids"].items():
            self.assertEqual(w.constructs[cid].n_vox, counts[role], f"{role} が壊れた")
        for f in ("moon.py", "moon.blocks.json", "bike.py", "beacon.py", "power.py", "kadai.blocks.json", "otehon.py",
                  "crane.blocks.json", "factory.py", "intercept.blocks.json", "robot.blocks.json", "reaccel.blocks.json"):
            self.assertTrue((w.dir / "scripts" / f).exists(), f)

    def test_playthrough_on_terra(self):
        """序章 → 第8章（ルミナス号の修理）まで。"""
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
        self.assertIn("02_naming", s.st["movies"])
        # 第1章：サクラと話す（トワと名付けられる）・掘る・落石に通り道をあける・段差を越える
        self.assertIn("sakura", s.cast.npcs)
        self.assertIn("トワ", w.addon_call("cast", "talk", "sakura"))
        p, up = s.map.ground(0.0, 12.0)
        for _ in range(3):
            w.dig({"kind": "terrain", "planet": s.terra, "point": p, "normal": up}, 0.8)
        self.run_for(0.4)
        c = s.con("rocks")
        rm = [i for i in range(c.n_vox) if abs(s.map_xz(c.pos[8 * i:8 * i + 8].mean(0))[0]) < 0.7]
        for i in sorted(rm, reverse=True):
            w.remove({"kind": "voxel", "construct": c, "index": i})
        self.run_for(0.6)
        self.assertTrue(s.st["goals"].get("rocks"))
        self.tp(0.0, 42.0)
        self.run_for(0.4)
        self.assertEqual(self.ch(), "power")
        # 第2章：バルブ → 背中合わせの推進機2つに点火（発電）→ 古い端末（ミカガの声）
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
        self.assertTrue(s.st["lights"])
        self.assertEqual(self.ch(), "power", "端末に触れるまでは終わらない")
        w.addon_call("story", "talk", "terminal")
        self.run_for(0.4)
        self.assertEqual(self.ch(), "openloop")
        self.run_for(2.0)
        self.assertLess(float(np.linalg.norm(pc.center_of_mass() - p0)), 0.05, "発電用の推進機で電源が動いた")
        # 第3章：見本（扉の開け閉め）を動かす → クレーンの手順を足して3回運ぶ → 風で荷物が落ちる
        self.use("crane", "computer")
        self.run_for(9.0)
        self.assertTrue(s.st["goals"].get("door"), "待つブロックで扉が2回開け閉めされる")
        self.save_prog("crane", self.programs.crane_solution())
        self.assertTrue(self.until("openloop", 60), "クレーンで3回運ぶ")
        self.assertTrue(any("荷物" in c.name for c in w.constructs.values()), "荷物が落ちる")
        # 第4章：お手本を見る・課題（お手本と同じプログラムにして棒を立てる）
        self.assertEqual(self.ch(), "control")
        self.tp(*s.map_xz(s.con("model").center_of_mass()))
        c, i = s.dev("task", "computer", "課題")
        st = c.devs[i]["state"]
        st.update(script="otehon.py", blocks="otehon.blocks.json", running=True)
        self.assertTrue(self.until("control", 15))
        # 第5章：操縦台に座る・ドローンで輪をくぐる → キンギのドローンのムービー
        self.sit("console")
        self.run_for(0.4)
        d = s.con("drone")
        d.pos += s.points["ring"] - d.center_of_mass()
        self.run_for(0.4)
        self.assertEqual(self.ch(), "factory")
        self.assertIn("03_watch", s.st["movies"])
        w.player.seated = None
        # 第6章：採掘 → 精錬機に入れる → 配管を切られる → 直す → バルブを開けて燃料 → 部品
        for key in ("quarry_ore", "quarry_sulfur"):
            q = s.points[key]
            for _ in range(3):
                w.dig({"kind": "terrain", "planet": s.terra, "point": q, "normal": s.terra.up(q)}, 0.8)
        self.run_for(0.4)
        self.assertTrue(s.st["goals"].get("ore") and s.st["goals"].get("sulfur"))
        self.use("factory", "refiner")
        self.run_for(10.0)
        self.assertTrue(s._flag("pipe_cut"), "キンギのドローンが配管を切る")
        self.assertIsNotNone(s.con("kdrone"))
        self.assertTrue(self.place_on("factory", sites.FACTORY_CUT, ("device", "pipe")))
        self.use("factory", "valve")
        self.assertTrue(self.until("factory", 60))
        self.assertIsNone(s.con("kdrone"), "章が終わるとドローンはいなくなる")
        # 第7章：遮断機・バイクに乗る（ドローンが追ってくる）・船まで行く
        self.assertEqual(self.ch(), "bike")
        self.use("gate", "actuator")
        self.run_for(4.0)
        self.assertTrue(s.st["goals"].get("gate"))
        self.sit("bike")
        self.run_for(3.0)
        self.assertIsNotNone(s.con("kdrone"), "バイクに乗ると黒いドローンが追ってくる")
        self.assertIn("chase", s.st["events"])
        self.tp(0.0, 141.0)
        self.run_for(0.4)
        self.assertEqual(self.ch(), "repair")
        # 第8章：穴をふさぐ・推進機（向きは自動で上向き）・燃料
        for h in sites.SHIP_HOLES:
            self.assertTrue(self.place_on("ship", h, ("material", "steel")), h)
        for t in sites.SHIP_THRUSTERS:
            self.assertTrue(self.place_on("ship", t, ("device", "thruster")), t)
        sh = s.con("ship")
        self.assertEqual([sh.devs[sh.index[t]]["dir"] for t in sites.SHIP_THRUSTERS], [2] * len(sites.SHIP_THRUSTERS))
        for i, dv in enumerate(sh.devs):
            if dv and dv["def"] == "tank" and dv["state"].get("contents", {}).get("fuel", 0) < 790:
                w.interact({"kind": "voxel", "construct": sh, "index": i, "face": 2})
        self.run_for(0.6)
        self.assertEqual(self.ch(), "launch")
        c, i = s.dev("ship", "computer")
        self.assertEqual(c.devs[i]["state"]["script"], "moon.py")

    def test_flight_to_the_moon_and_port(self):
        """第9章（月への自動飛行）→ 第10章（ミカガに会う → ムービー2本）。"""
        s, w, sites = self.s, self.w, self.sites
        s.jump(self.idx("launch"))
        self.run_for(0.5)
        self.use("ship", "computer")
        self.run_for(1.0)
        self.assertLess(float(np.linalg.norm(s.con("ship").velocity())), 0.3, "座るまでは飛ばない")
        self.sit("ship")
        self.run_for(6.0, 1 / 30)
        c, i = s.dev("ship", "computer")
        st = c.devs[i]["state"]
        self.assertIsNone(st.get("error"))
        self.assertEqual(len(st.get("_arrows", {})), 3, "飛んでいる間は矢印（行きたい速度・推力の向き・重力）を見せる")
        self.assertIn("04_launch", s.st["movies"], "発進のムービー")
        self.assertTrue(self.until("launch", 170, 1 / 30))
        self.assertEqual(self.ch(), "port")
        self.assertEqual(s.con("ship").n_vox, len(sites.ship_voxels()))
        said = " ".join(x[2] for x in s.st["log"])
        for k in ("lift", "space", "rutero", "half", "near"):
            self.assertIn(self.script.CHAPTERS[self.idx("launch")]["events"][k][1], said, k)
        w.player.seated = None
        self.assertIn("mikaga", s.cast.npcs)
        w.addon_call("cast", "talk", "mikaga")
        self.run_for(0.4)
        self.assertEqual(self.ch(), "fleet")
        self.assertTrue({"05_declaration", "06_interlude"} <= set(s.st["movies"]))
        self.assertEqual(w.nearest_planet(w.player.pos).defn["id"], "terra", "迎撃艦（アマツの宙域）へ")

    def test_space_chapters(self):
        """第11章（迎撃のプログラム）→ 第12章（ロボット）→ 終章（再加速）→ クリア。"""
        s, w, sp = self.s, self.w, self.space
        s.jump(self.idx("fleet"))
        self.run_for(2.0)
        ship = s.con("interceptor")
        R, _ = ship.rigid_fit()
        h = float((w.player.pos - s._cell_world(ship, (6, 0, 8))) @ R[:, 1])
        self.assertTrue(0.2 < h < 1.0, f"迎撃艦の甲板に立っている（{h:.2f} m）")
        w.addon_call("cast", "talk", "kensu")
        self.run_for(6.0)
        self.assertGreater(len(s.weapons.missiles), 0, "ケンスと話すとミサイルが来る")
        self.save_prog("intercept", self.programs.intercept_starter())
        self.use("interceptor", "computer")
        self.run_for(3.0)
        self.assertFalse(s.st["goals"].get("intercept_prog"), "見本のままでは迎撃にならない")
        self.save_prog("intercept", self.programs.intercept_solution())
        self.assertTrue(self.until("fleet", 60))
        self.assertGreaterEqual(s._count("intercepted"), 0)
        self.assertIn("07_fleet", s.st["movies"])
        # 第12章：ロボットに乗る → 進む → ミサイルをかわす（横に動く）→ 組みつく
        self.assertEqual(self.ch(), "robot")
        self.sit("robot")

        def keys(t):
            g = t.s.st["goals"]
            if not g.get("robot_walk"):
                return {"w"}
            if not g.get("dodge"):
                return {"a"} if int(t.w.time) // 2 % 2 else {"d"}
            return set()
        self.assertTrue(self.until("robot", 90, keys=keys), "ロボットの章が終わる")
        self.assertGreater(s.slow_ticks, 0, "ミサイルが近いとスローモーション")
        self.assertEqual(s.con("robot").n_vox, len(sp.robot_voxels()), "ロボットは壊れない")
        self.assertTrue({"08_duel", "09_dialogue"} <= set(s.st["movies"]))
        w.input_keys = set()
        # 終章：推進モジュール4基 → 点火してバルブをそろえる → 航法に目標の地点 → 上昇
        self.assertEqual(self.ch(), "reaccel")
        w.player.seated = None
        for cell in sp.UNIT_SLOTS:
            self.assertTrue(self.place_on("unit", cell, ("device", "rocket_module")), cell)
        u = s.con("unit")
        self.assertEqual([u.devs[u.index[c]]["dir"] for c in sp.UNIT_SLOTS], [2] * 4)
        for c in sp.UNIT_SLOTS:
            w.interact({"kind": "voxel", "construct": u, "index": u.index[c], "face": 2})
        self.run_for(1.0)
        self.assertFalse(s.st["goals"].get("thrust"), "最初のバルブはばらばら")
        for d in u.devs:
            if d and d["def"] == "valve":
                d["state"]["open"] = 0.8
        self.run_for(0.6)
        self.assertTrue(s.st["goals"].get("thrust"))
        prog = self.programs.reaccel_starter()
        t = s.target_point()
        prog["scripts"][0]["body"][0]["value"] = self.programs.vto([float(x) for x in t])
        self.save_prog("reaccel", prog)
        self.use("unit", "computer", "再加速の航法")
        self.assertTrue(self.until("reaccel", 20))
        self.assertTrue(s.st["done"])
        self.assertEqual(s.cast.npcs["kingi"].emote, "smoke")
        self.assertTrue({"10_reaccel", "11_epilogue"} <= set(s.st["movies"]))
        self.assertEqual(w.nearest_planet(w.player.pos).defn["id"], "luna", "クリアしたらルナ・ポートへ")
        self.assertGreaterEqual(len(s.cast.npcs), 8, "みんながルナ・ポートにいる")
        for name in list(s.st["movies"]):
            w.addon_call("story", "movie_done", name)
        self.assertEqual(s.st["movies"], [])

    def test_vehicles_fly_with_block_programs(self):
        """ドローン（操縦台から無線）・ホバーバイク・ロボットは、ブロックプログラムで飛ぶ。"""
        s, w = self.s, self.w
        for role in ("bike", "drone", "console"):
            c, i = s.dev(role, "computer")
            self.assertTrue(c.devs[i]["state"]["blocks"].endswith(".blocks.json"), role)
            self.assertTrue((w.dir / "scripts" / c.devs[i]["state"]["blocks"]).exists())

        def alt(c):
            p = c.center_of_mass()
            g, up = s.map.ground(*s.map_xz(p))
            return float((p - g) @ up)

        def run(sec, keys=()):
            w.input_keys = set(keys)
            self.run_for(sec)
        s.jump(self.idx("drone"))
        self.sit("console")
        d = s.con("drone")
        z0 = s.map_xz(d.center_of_mass())[1]
        run(1.0)
        run(4.0, ["space"])
        self.assertGreater(alt(d), 7.0, "Space で上がる")
        run(3.0, ["w"])
        self.assertGreater(s.map_xz(d.center_of_mass())[1], z0 + 4.0, "W で前（+Z）へ進む")
        run(1.0, ["x"])
        run(18.0)
        self.assertLess(alt(d), 1.0, "X で着陸する")
        w.player.seated = None
        s.jump(self.idx("bike"))
        self.use("gate", "actuator")                    # 遮断機を上げてから
        run(3.0)
        b = s.con("bike")
        self.sit("bike")
        z0 = s.map_xz(b.center_of_mass())[1]
        run(2.0)
        self.assertGreater(alt(b), 1.5, "座ると浮く")
        run(3.0, ["w"])
        self.assertGreater(s.map_xz(b.center_of_mass())[1], z0 + 8.0, "W で前へ進む")
        w.player.seated = None
        run(9.0)
        self.assertLess(alt(b), 1.0, "降りると止まって、ゆっくり着地する")
        self.assertEqual(b.n_vox, len(self.sites.bike_voxels()))
        for c in (b, d):
            ci = [i for i, dv in enumerate(c.devs) if dv and dv["def"] == "computer"][0]
            self.assertIsNone(c.devs[ci]["state"].get("error"))

    def test_save_and_reload_keeps_progress(self):
        s, w = self.s, self.w
        s.jump(self.idx("factory"))
        self.run_for(0.5)
        n = len(w.constructs)
        w.save()
        modcode.reset()
        w2 = World(NAME, CFG).load()
        s2 = w2.addon("story")
        self.assertEqual(s2.st["ch"], self.idx("factory"))
        self.assertTrue(s2.st["lights"])
        self.assertEqual(len(w2.constructs), n)
        self.assertEqual(len(s2.terra.craters), len(s.terra.craters))
        self.assertTrue(np.allclose(s2.scenes[0].C, s.scenes[0].C))

    def test_old_tutorial_save_continues(self):
        """前の版（「墜落した技術者」・MOD は story だけ）のセーブ：必要な MOD を足し、章を読み替えて続きから。"""
        s, w = self.s, self.w
        w.save()
        d = w.dir
        wj = json.loads((d / "world.json").read_text(encoding="utf-8"))
        st = wj["story"]
        for role in ("crane", "factory", "interceptor", "robot", "krobot", "unit"):
            st["ids"].pop(role, None)
        cons = [c for c in json.loads((d / "constructs.json").read_text(encoding="utf-8"))
                if c["id"] in st["ids"].values()]
        st.pop("v", None)
        st["ch"] = 5                                 # 前の版の「第5章 ホバーバイク」
        (d / "world.json").write_text(json.dumps(wj, ensure_ascii=False), encoding="utf-8")
        (d / "constructs.json").write_text(json.dumps(cons, ensure_ascii=False), encoding="utf-8")
        (d / "mods.json").write_text(json.dumps({"enabled": ["story"]}), encoding="utf-8")
        modcode.reset()
        w2 = World(NAME, CFG).load()
        s2 = w2.addon("story")
        self.assertIn("lunaport", w2.mods)
        self.assertEqual(json.loads((d / "mods.json").read_text())["enabled"][-1], "story")
        self.assertEqual(self.script.CHAPTER_IDS[s2.st["ch"]], "bike")
        for role in ("crane", "factory", "interceptor", "robot", "krobot", "unit"):
            self.assertIsNotNone(s2.con(role), role)

    def test_jump_to_every_chapter(self):
        s, w = self.s, self.w
        where = {"port": "luna"}
        for k, c in enumerate(self.script.CHAPTERS):
            s.jump(k)
            self.run_for(0.3)
            self.assertEqual(s.st["ch"], k, c["id"])
            near = w.nearest_planet(w.player.pos)
            self.assertEqual(near.defn["id"], where.get(c["id"], "terra"), c["id"])
        s.jump(self.idx("launch"))
        self.run_for(0.5)
        self.assertTrue(s.check_hull() and s.check_thrusters() and s.check_fuel_ship())


if __name__ == "__main__":
    unittest.main()
