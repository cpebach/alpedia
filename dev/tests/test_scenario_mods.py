"""MOD の依存関係・ワールドの形を決める MOD・待つブロック・MOD の機器（迎撃砲塔）・設計図のテスト。"""
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from main import blocks as B  # noqa: E402
from main import config, log, modcode, modmeta, objio, paths  # noqa: E402
from main.construct import Construct  # noqa: E402
from main.tutorial import create_world  # noqa: E402
from main.world import World  # noqa: E402

log.setup("CRITICAL")
CFG = json.loads(json.dumps(config.DEFAULTS))


class Case(unittest.TestCase):
    def setUp(self):
        self._blockers = [f for f in sys.meta_path if type(f).__name__ == "_Block"]   # コアの検査用の止め具を外す
        for f in self._blockers:
            sys.meta_path.remove(f)
        self.tmp = Path(tempfile.mkdtemp(prefix="alpedia_sc_"))
        self._saves = paths.SAVES
        paths.SAVES = self.tmp

    def tearDown(self):
        paths.SAVES = self._saves
        modcode.reset()
        shutil.rmtree(self.tmp, ignore_errors=True)
        for f in self._blockers:
            sys.meta_path.insert(0, f)

    def test_requires_are_resolved_in_order(self):
        order, added, missing = modmeta.resolve(["story"])
        self.assertEqual(order[-1], "story")
        self.assertLess(order.index("skinmodel(dummy)"), order.index("soredemo_cast"))
        self.assertEqual(set(added), {"scenario_blocks", "soredemo_cast", "skinmodel(dummy)", "lunaport", "amatsu"})
        self.assertEqual(missing, [])
        self.assertIn("story", modmeta.dependents("lunaport", order))
        self.assertEqual(modmeta.world_preset(order)[0], "story")

    def test_plain_world_with_place_mods(self):
        """場所の MOD はシナリオでなくても使える（ルナ・ポートとアマツが出てくる）。"""
        create_world("w", ["lunaport", "amatsu", "soredemo_cast"])
        w = World("w", CFG).load()
        self.assertIn("skinmodel(dummy)", w.mods)
        self.assertTrue(w.addon("lunaport").active and w.addon("amatsu").active)
        self.assertIsNone(w.addon("story"))
        cast = w.addon("cast")
        p = w.addon("lunaport").points["plaza"]
        cast.place("sakura", p, w.planets[1].up(p), [0, 0, 1.0])
        self.assertIn("サクラ", cast.talk("sakura"))
        for _ in range(30):
            w.step(1 / 60)

    def test_wait_blocks_run_in_order(self):
        prog = B.new_program()
        tick = B.new_stmt("on_tick")
        T = lambda v: {"type": "text", "v": v}  # noqa: E731
        tick["body"] = [{"type": "print", "text": T("A")}, {"type": "wait", "n": {"type": "num", "v": 1}},
                        {"type": "repeat", "n": {"type": "num", "v": 2},
                         "body": [{"type": "print", "text": T("B")}, {"type": "wait", "n": {"type": "num", "v": 0.5}}]},
                        {"type": "print", "text": T("C")}]
        prog["scripts"].append(tick)
        env = {}
        exec(compile(B.to_python(prog), "x", "exec"), env)  # noqa: S102
        out = []

        class Api:
            memory, input, time = {}, set(), 0.0
        api = Api()
        env["print"] = lambda x: out.append((round(api.time, 2), x))
        for k in range(50):
            api.time = k * 0.05
            env["tick"](api)
        self.assertEqual([x for _, x in out][:5], ["A", "B", "B", "C", "A"])
        self.assertEqual(out[1][0], 1.0)

    def test_turret_shoots_missile_with_block_program(self):
        """シナリオ再現ブロック：レーダーと迎撃砲塔とブロックプログラムで、ミサイルを撃ち落とす。"""
        create_world("w", ["scenario_blocks"])
        w = World("w", CFG).load()
        vox = [((x, 0, z), "steel", None) for x in range(4) for z in range(4)]
        vox[0] = ((0, 0, 0), "steel", {"def": "battery", "dir": 2, "name": "バッテリー"})
        vox += [((1, 1, 1), "steel", {"def": "turret", "dir": 2, "name": "砲塔"}),
                ((2, 1, 1), "steel", {"def": "radar", "dir": 2, "name": "レーダー"}),
                ((0, 1, 0), "steel", {"def": "computer", "dir": 4, "name": "制御",
                                      "state": {"script": "t.py", "running": True}})]
        pos = w.player.pos + np.array([0.0, 30.0, 0.0])
        c = Construct.from_voxels(w.registry, vox, pos, size=0.5, name="砲台")
        w.add_construct(c)
        prog = B.new_program()
        tick = B.new_stmt("on_tick")
        tick["body"] = [{"type": "turret_aim", "dev": "*", "vec": {"type": "vsensor", "name": "nearest_missile"}},
                        {"type": "if", "cond": {"type": "cmp", "op": ">", "a": {"type": "sensor", "name": "missile_count"},
                                                "b": {"type": "num", "v": 0}},
                         "body": [{"type": "turret_fire", "dev": "*"}]}]
        prog["scripts"].append(tick)
        (w.dir / "scripts" / "t.py").write_text(B.to_python(prog), encoding="utf-8")
        weapons = w.addon("weapons")
        got = []
        weapons.listeners.append(lambda k, info: got.append(k))
        weapons.spawn_missile(pos + np.array([150.0, 40.0, 0.0]), np.array([-30.0, 0, 0]), target=c.id, speed=30.0)
        for _ in range(240):
            w.step(1 / 60)
            c.pos += pos - c.center_of_mass()          # 浮かせておく
            c.vel[:] = 0
        ci = [i for i, d in enumerate(c.devs) if d and d["def"] == "computer"][0]
        self.assertIsNone(c.devs[ci]["state"].get("error"))
        self.assertIn("intercept", got)
        self.assertEqual(weapons.missiles, [])

    def test_pump_sends_fuel_through_attached_actuator(self):
        """燃料ポンプ：後ろのタンクから前の区画のタンクへ送る。アクチュエーターも配管と同じく通し、分離すると止まる。"""
        create_world("w", ["scenario_blocks"])
        w = World("w", CFG).load()
        full = {"def": "tank", "dir": 2, "name": "満タン", "state": {"contents": {"fuel": 800.0}}}
        empty = {"def": "tank", "dir": 2, "name": "空", "state": {}}
        vox = [((x, 0, 0), "steel", None) for x in range(6)]
        vox += [((0, 1, 0), "steel", full), ((1, 1, 0), "steel", {"def": "fuel_pump", "dir": 0, "name": "ポンプ"}),
                ((2, 1, 0), "steel", {"def": "pipe", "dir": 2}),
                ((3, 1, 0), "steel", {"def": "actuator", "dir": 0, "name": "口"}),
                ((4, 1, 0), "steel", {"def": "pipe", "dir": 2}), ((5, 1, 0), "steel", empty)]
        pos = w.player.pos + np.array([0.0, 20.0, 0.0])
        c = Construct.from_voxels(w.registry, vox, pos, size=0.5, name="給油")
        w.add_construct(c)

        def fuel(name):
            return [d for d in c.devs if d and d["name"] == name][0]["state"].get("contents", {}).get("fuel", 0.0)
        for _ in range(60):
            w.step(1 / 60)
            c.pos += pos - c.center_of_mass()
            c.vel[:] = 0
        self.assertGreater(fuel("空"), 40.0, "アクチュエーターを通って届く")
        self.assertAlmostEqual(fuel("空") + fuel("満タン"), 800.0, delta=1e-6)
        ai = c.index[(3, 1, 0)]
        self.assertTrue(w.actuator_detach(c, ai))
        before = fuel("満タン")
        for _ in range(30):
            w.step(1 / 60)
        self.assertAlmostEqual(fuel("満タン"), before, delta=1e-6, msg="分離すると通らない")

    def test_block_hint_leads_to_solution(self):
        """プログラム編集のヒント：見本との違いを1つずつ直していくと、見本と同じになる。"""
        sol = B.new_program()
        tick = B.new_stmt("on_tick")
        tick["body"] = [{"type": "wait", "n": {"type": "num", "v": 1.5}},
                        {"type": "if", "cond": {"type": "cmp", "op": ">", "a": {"type": "sensor", "name": "altitude"},
                                                "b": {"type": "num", "v": 3}},
                         "body": [{"type": "print", "text": {"type": "text", "v": "高い"}}]}]
        sol["scripts"].append(tick)
        cur = B.new_program()
        for _ in range(20):
            h = B.find_hint(cur, sol)
            if h is None:
                break
            self.assertTrue(B.hint_text(h).startswith("ヒント："))
            if h["kind"] == "insert":
                h["list"].insert(h["index"], B.new_stmt(h["type"]))
            elif h["kind"] == "expr":
                h["owner"][h["key"]] = json.loads(json.dumps(h["node"]))
            else:
                v = h["value"]
                h["owner"][h["key"]] = {"type": h["type"], "v": v} if h.get("type") else v
        self.assertIsNone(B.find_hint(cur, sol))
        self.assertEqual(B.to_python(cur), B.to_python(sol))

    def test_blueprints_are_listed_with_requirements(self):
        create_world("w", [])
        w = World("w", CFG).load()
        src = objio.list_sources(w.dir, paths.SAVES, w.mods)
        names = {p.name for g, p in src if g == "設計図"}
        self.assertIn("luminous.alp.json", names)
        self.assertIn("robot_towa.alp.json", names)
        info = objio.inspect(w.registry, paths.SAUCE / "blueprints" / "robot_towa.alp.json")
        self.assertEqual(info["requires"], ["scenario_blocks"])


if __name__ == "__main__":
    unittest.main()
