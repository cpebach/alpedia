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
    def test_script_ignites_and_sets_valve(self):
        w = self.world()
        self.script(w, "t.py", "def tick(api):\n"
                               "    api.memory['n'] = api.memory.get('n', 0) + 1\n"
                               "    for v in api.devices('valve'):\n"
                               "        v.open = 0.25\n"
                               "    for t in api.devices('thruster'):\n"
                               "        t.ignite()\n"
                               "    api.display('alt %.1f' % api.altitude)\n")
        c = self.put(w, [((0, 0, 0), "steel", {"def": "battery", "dir": 2}),
                         ((1, 0, 0), "steel", {"def": "computer", "dir": 2, "state": {"script": "t.py", "running": True}}),
                         ((0, 1, 0), "steel", {"def": "tank", "dir": 2, "state": {"contents": {"fuel": 500.0}}}),
                         ((1, 1, 0), "steel", {"def": "valve", "dir": 2}),
                         ((2, 1, 0), "steel", {"def": "thruster", "dir": 3})])
        self.run_for(w, 0.5)
        comp = next(d for d in c.devs if d and d["def"] == "computer")
        thr = next(d for d in c.devs if d and d["def"] == "thruster")
        self.assertIsNone(comp["state"].get("error"))
        self.assertGreater(comp["state"]["memory"]["n"], 3)
        self.assertTrue(thr["state"]["lit"])
        self.assertAlmostEqual(thr["state"]["firing"], 0.25, places=3)
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
        p.device_set(ti, "throttle", 5.0)                  # 前の版との互換：点火の頼みになる
        self.assertTrue(c.devs[ti]["state"].get("ignite"))
        with self.assertRaises(PortError):
            p.device_set(ti, "firing", 1.0)
        with self.assertRaises(PortError):
            p.device_set(ti, "lit", False)               # 消すのはバルブで
        st = p.device_state(ti)
        st["ignite"] = False                     # コピーなので本体は変わらない
        self.assertTrue(c.devs[ti]["state"]["ignite"])

    def test_rotor_and_piston(self):
        """ローターは回すだけ、ピストンは押し出し・引き込みだけ。分離・結合はどちらも。古いアクチュエーターの読み替え。"""
        from main import actuator as act
        w = self.world()
        c = self.put(w, [((0, 0, 0), "steel", {"def": "battery", "dir": 2}),
                         ((1, 0, 0), "steel", {"def": "actuator", "dir": 2, "name": "rot"}),
                         ((1, 1, 0), "steel", None),
                         ((2, 0, 0), "steel", {"def": "piston", "dir": 2, "name": "pis"}),
                         ((2, 1, 0), "steel", None)])
        p = Port(w, c)
        ri, pi = c.index[(1, 0, 0)], c.index[(2, 0, 0)]
        self.assertEqual((act.motion(c.devs[ri], c.reg), act.motion(c.devs[pi], c.reg)), ("rotor", "piston"))
        self.assertEqual(w.registry.device("actuator")["name"], "ローター")
        p.act_rotate(ri, 90)
        with self.assertRaises(PortError):
            p.act_extend(ri, 1.0)
        with self.assertRaises(PortError):
            p.act_rotate(pi, 90)
        p.act_extend(pi, 1.0)
        self.assertEqual(c.devs[pi]["state"]["ext_target"], 1.0)
        self.assertEqual(c.devs[pi]["state"]["preset"]["action"], "extend", "ピストンの既定の動作は押し出し")
        c.devs[ri]["state"]["preset"] = {"action": "extend", "amount": 1.0, "speed": 0.5}   # できない動作の設定
        self.assertIn("回転", act.execute_preset(w, c, ri), "ローターの既定（回転）にする")
        self.assertTrue(p.act_detach(pi))
        self.assertTrue(p.act_detach(ri))
        # 古いセーブ：押し出していたアクチュエーターはピストン、それ以外はローター
        old = Construct.from_voxels(w.registry, [((0, 0, 0), "steel", {"def": "actuator", "dir": 2, "state": {"ext": 0.5}}),
                                                 ((1, 0, 0), "steel", {"def": "actuator", "dir": 2, "state": {
                                                     "preset": {"action": "retract", "amount": 1, "speed": 0.5}}}),
                                                 ((2, 0, 0), "steel", {"def": "actuator", "dir": 2, "state": {"angle": 90}})],
                                    np.zeros(3), size=0.5)
        self.assertEqual([d["def"] for d in old.devs], ["piston", "piston", "actuator"])

    def _three_actuators(self, prog_or_code, full_rigid=False):
        """ローター・ピストン・ヒンジ（どれも出力は上）とその先のブロック。コンピューターで動かして3秒。"""
        from main import blocks as B
        w = self.world(full_rigid=full_rigid)
        st = {"running": True}
        if isinstance(prog_or_code, dict):
            B.save(w.dir / "scripts" / "m.blocks.json", prog_or_code)
            self.script(w, "m.py", B.to_python(prog_or_code, "m.blocks.json"))
            st.update(script="m.py", blocks="m.blocks.json")
        else:
            self.script(w, "m.py", prog_or_code)
            st["script"] = "m.py"
        vox = [((x, 0, z), "steel", None) for x in range(9) for z in range(2)]
        vox += [((0, 1, 1), "steel", {"def": "battery", "dir": 2}),
                ((2, 1, 1), "steel", {"def": "computer", "dir": 4, "state": st}),
                ((0, 1, 0), "steel", {"def": "actuator", "dir": 2, "name": "rot"}),
                ((0, 2, 0), "steel", None), ((1, 2, 0), "aluminum", None),
                ((4, 1, 0), "steel", {"def": "piston", "dir": 2, "name": "pis"}), ((4, 2, 0), "aluminum", None),
                ((7, 1, 0), "steel", {"def": "hinge", "dir": 2, "name": "hin", "state": {"hinge": 0}}),
                ((7, 2, 0), "aluminum", None), ((7, 3, 0), "aluminum", None)]
        c = self.put(w, vox, extra=0.4)
        self.run_for(w, 3.0)
        comp = next(d for d in c.devs if d and d["def"] == "computer")["state"]
        self.assertFalse(comp.get("error"), comp.get("error"))
        F = c.voxel_frames()
        out = {}
        for name, tip in (("rot", (1, 2, 0)), ("pis", (4, 2, 0)), ("hin", (7, 3, 0))):
            i = next(k for k, d in enumerate(c.devs) if d and d["name"] == name)
            v = c.pos[8 * c.index[tip]:8 * c.index[tip] + 8].mean(0) - c.pos[8 * i:8 * i + 8].mean(0)
            out[name] = (c.devs[i]["state"], F[i] @ v / c.size)
        shutil.rmtree(w.dir)
        return out

    def _check_three(self, out, hinge_to):
        st, v = out["rot"]
        self.assertAlmostEqual(st["angle"], 90.0, places=2)
        self.assertLess(abs(v[0]), 0.3, f"ローターの腕は +X から横へ回った {v}")
        self.assertGreater(abs(v[2]), 0.7, f"ローターの腕 {v}")
        st, v = out["pis"]
        self.assertAlmostEqual(st["ext"], 1.0, places=2)
        # ピストンのブロック自身が伸びる（中心は 0.5 上がる）ので、中心から先までは 1 + 1 - 0.5 = 1.5
        self.assertAlmostEqual(v[1], 1.5, delta=0.15, msg=f"ピストンの先は1ブロック押し出された {v}")
        st, v = out["hin"]
        self.assertAlmostEqual(st["angle"], hinge_to, places=2)
        self.assertAlmostEqual(v[0], 2.0 * np.sin(np.radians(hinge_to)), delta=0.25, msg=f"ヒンジ {v}")
        self.assertAlmostEqual(v[1], 2.0 * np.cos(np.radians(hinge_to)), delta=0.25, msg=f"ヒンジ {v}")

    def test_blocks_move_all_actuators(self):
        """ブロックプログラム：「すべて」で回す・押し出すと、できる種類だけが動く（エラーにならない）。名前で選んでも動く。"""
        from main import blocks as B
        prog = B.new_program()
        on = B.new_stmt("on_start")
        r, e, rr = B.new_stmt("act_rotate"), B.new_stmt("act_extend"), B.new_stmt("act_retract")
        r["speed"] = {"type": "num", "v": 180}
        rr["amount"] = {"type": "num", "v": 0}
        on["body"] = [r, e, rr]
        prog["scripts"].append(on)
        for full in (False, True):
            self._check_three(self._three_actuators(prog, full), 90.0)
        prog = B.new_program()
        on = B.new_stmt("on_start")
        a, b, h = B.new_stmt("act_rotate_to"), B.new_stmt("act_extend"), B.new_stmt("act_rotate_to")
        a.update(dev="rot", angle={"type": "num", "v": 90}, speed={"type": "num", "v": 180})
        b.update(dev="pis")
        h.update(dev="hin", angle={"type": "num", "v": 60}, speed={"type": "num", "v": 180})
        on["body"] = [a, b, h]
        prog["scripts"].append(on)
        self._check_three(self._three_actuators(prog), 60.0)

    def test_script_moves_all_actuators(self):
        """スクリプト：ローターは rotate、ピストンは extend、ヒンジは rotate_to。can / motion で種類が分かる。"""
        code = ("def tick(api):\n"
                "    if api.memory.get('sent'):\n        return\n"
                "    api.memory['sent'] = True\n"
                "    for d in api.devices('actuator'):\n"
                "        if d.motion == 'rotor': d.rotate(90, speed=180)\n"
                "        elif d.motion == 'hinge': d.rotate_to(-45, speed=180)\n"
                "        elif d.can('extend'): d.extend(1.0)\n"
                "    assert not api.device('pis').can('rotate') and api.device('rot').can('detach')\n")
        for full in (False, True):
            self._check_three(self._three_actuators(code, full), -45.0)

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

    def _hinge_arm(self, full_rigid, to=90):
        """土台の上のヒンジ（出力は上、+ で -Z 側へ倒れる）と3ブロックの腕。スクリプトで to° 曲げる。"""
        w = self.world(full_rigid=full_rigid)
        self.script(w, "h.py", ("def tick(api):\n"
                               "    a = api.device('arm')\n"
                               "    if not api.memory.get('sent'):\n"
                               "        a.rotate_to(%s, speed=90)\n"
                               "        api.memory['sent'] = True\n") % to)
        vox = [((x, 0, z), "steel", None) for x in range(3) for z in range(3) if (x, z) != (1, 1)]
        vox += [((1, 0, 1), "steel", {"def": "battery", "dir": 2}),
                ((0, 1, 1), "steel", {"def": "computer", "dir": 4, "state": {"script": "h.py", "running": True}}),
                ((1, 1, 1), "steel", {"def": "hinge", "dir": 2, "name": "arm", "state": {"hinge": 5}})]
        vox += [((1, 2 + k, 1), "aluminum", None) for k in range(3)]
        c = self.put(w, vox, extra=0.4)
        self.run_for(w, 3.0)
        i = next(k for k, d in enumerate(c.devs) if d and d["def"] == "hinge")
        st = c.devs[i]["state"]
        tip = c.pos[8 * c.index[(1, 4, 1)]:8 * c.index[(1, 4, 1)] + 8].mean(0) - c.pos[8 * i:8 * i + 8].mean(0)
        return w, c, st, c.voxel_frames()[i] @ tip

    def test_hinge_bends(self):
        """ヒンジ：ブロックの中心を軸に振る（腕が横に倒れ、ヒンジの横に並ぶ）。- の角度は反対側。完全剛体モードでも同じ。"""
        for full, to in ((False, 90), (True, 90), (False, -90)):
            w, c, st, tip = self._hinge_arm(full, to)
            self.assertAlmostEqual(st["angle"], float(to), places=3)
            self.assertLess(abs(tip[1]), 0.3 * c.size, f"腕はヒンジと同じ高さに倒れている（上 {tip[1]:.2f}）{full}")
            self.assertAlmostEqual(tip[2], (-3.0 if to > 0 else 3.0) * c.size, delta=0.3 * c.size,
                                   msg=f"ヒンジの中心から3つ先（{tip[2]:.2f}）{full} {to}")
            if to < 0:
                shutil.rmtree(w.dir)
                continue
            self.assertEqual(len(w.constructs), 1)
            w.save()
            self.assertEqual(json.loads((w.dir / "constructs.json").read_text(encoding="utf-8"))[0]["voxels"][-4][4]
                             ["state"]["hinge"], 5, "曲がる側は保存される")
            shutil.rmtree(w.dir)

    def test_hinge_side_from_placement(self):
        """ヒンジの曲がる側は置き方で決まる：置くときに見ている向きにいちばん近い側。回転のアクチュエーターは回るだけ。"""
        from main import actuator as act
        w = self.world()
        c = self.put(w, [((0, 0, 0), "steel", None)])
        for look, side in (((0, 0, 1.0), 4), ((1.0, 0, 0.2), 0), ((-0.3, 0, -1.0), 5)):
            hit = {"kind": "voxel", "construct": c, "index": 0, "face": 2}
            self.assertTrue(w.place(hit, ("device", "hinge"), 2, np.array(look)))
            j = c.index[(0, 1, 0)]
            self.assertEqual(c.devs[j]["state"]["hinge"], side, look)
            self.assertEqual(act.hinge_of(c.devs[j], c.reg), act.hinge_pair(2, side))
            c.remove_voxels([j])
        self.assertTrue(w.place({"kind": "voxel", "construct": c, "index": 0, "face": 2}, ("device", "actuator"), 2,
                                np.array([0, 0, 1.0])))
        j = c.index[(0, 1, 0)]
        self.assertIsNone(act.hinge_of(c.devs[j], c.reg), "ふつうのアクチュエーターはヒンジにならない")
        c2 = self.put(w, [((0, 0, 0), "steel", {"def": "hinge", "dir": 2, "name": "h"}), ((0, 1, 0), "steel", None)])
        act.cmd_rotate(c2, 0, 500)
        self.assertEqual(c2.devs[0]["state"]["angle_target"], act.HINGE_MAX, "ヒンジは +90° まで")
        act.cmd_rotate(c2, 0, -900)
        self.assertEqual(c2.devs[0]["state"]["angle_target"], -act.HINGE_MAX, "-90° まで")
        c2.devs[0]["state"]["preset"] = {"action": "rotate", "amount": 45.0, "speed": 90.0}
        c2.devs[0]["state"]["angle_target"] = 0.0
        act.execute_preset(w, c2, 0)
        self.assertEqual(c2.devs[0]["state"]["angle_target"], 45.0, "E：設定した角度へ")
        act.execute_preset(w, c2, 0)
        self.assertEqual(c2.devs[0]["state"]["angle_target"], 0.0, "もう一度 E でまっすぐに戻る")

    def test_block_categories_merged(self):
        """機器・アクチュエーター・MOD の機器（迎撃など）のブロックは「アクチュエーター」の1つの種類。"""
        from main import blocks as B
        cats = [c[0] for c in B.CATEGORIES]
        self.assertNotIn("device", cats)
        self.assertEqual(B.STATEMENTS["ignite"]["cat"], "actuator")
        B.register_ext(categories=[("weapon", "迎撃", (1, 0, 0))],
                       statements={"zz_fire": dict(cat="weapon", label="撃つ", code=["pass"])})
        try:
            self.assertNotIn("weapon", [c[0] for c in B.CATEGORIES])
            self.assertEqual(B.STATEMENTS["zz_fire"]["cat"], "actuator")
            self.assertEqual(B.STATEMENTS["zz_fire"]["orig_cat"], "weapon")
        finally:
            B.reset_ext()

    def test_vector_blocks(self):
        """矢印（ベクトル）のブロック：入る欄・Python への変換・実行（重力の矢印・船から見た成分・矢印を見せる）。"""
        from main import blocks as B
        vel = {"type": "vsensor", "name": "velocity"}
        grav = {"type": "vsensor", "name": "gravity"}
        self.assertTrue(B.fits(vel, "vec"))
        self.assertFalse(B.fits(vel, "num"))                   # 数にするには「長さ」などを使う
        self.assertFalse(B.fits({"type": "num", "v": 1}, "vec"))
        self.assertTrue(B.fits({"type": "vlen", "a": vel}, "num"))
        self.assertTrue(B.fits(vel, "text"))
        N = lambda v: {"type": "num", "v": v}  # noqa: E731
        prog = B.new_program()
        tick = B.new_stmt("on_tick")
        tick["body"] = [
            {"type": "set_vvar", "var": "g", "value": grav},
            {"type": "set_var", "var": "重力", "value": {"type": "vlen", "a": {"type": "vvar", "name": "g"}}},
            {"type": "set_var", "var": "下", "value": {"type": "vcomp", "axis": "上", "a": {"type": "vvar", "name": "g"}}},
            {"type": "set_vvar", "var": "斜め", "value": {"type": "vtilt", "a": {"type": "vec", "frame": "船から見た",
                                                                              "x": N(1), "y": N(0), "z": N(0)},
                                                       "b": {"type": "vsensor", "name": "up"}, "n": N(30)}},
            {"type": "set_var", "var": "角度", "value": {"type": "vangle", "a": {"type": "vvar", "name": "斜め"},
                                                       "b": {"type": "vsensor", "name": "up"}}},
            {"type": "set_vvar", "var": "短く", "value": {"type": "vclamp", "a": {"type": "vscale", "op": "×",
                                                                              "a": grav, "b": N(10)}, "b": N(2)}},
            {"type": "show_vec", "value": {"type": "vvar", "name": "g"}, "color": "赤", "scale": N(0.5)},
            {"type": "display", "text": {"type": "join", "a": {"type": "text", "v": "g="}, "b": grav}, "width": N(1)},
        ]
        prog["scripts"].append(tick)
        self.script(w := self.world(), "v.py", B.to_python(prog))
        c = self.put(w, [((0, 0, 0), "steel", {"def": "battery", "dir": 2}),
                         ((1, 0, 0), "steel", {"def": "computer", "dir": 2, "state": {"script": "v.py", "running": True}})],
                     extra=0.6)
        self.run_for(w, 0.3)
        st = next(d for d in c.devs if d and d["def"] == "computer")["state"]
        self.assertIsNone(st.get("error"))
        v = st["memory"]["vars"]
        g = float(np.linalg.norm(w.gravity(c.center_of_mass()[None])[0]))
        self.assertAlmostEqual(v["重力"], g, delta=0.01)
        self.assertAlmostEqual(v["下"], -g, delta=0.05 * g)      # 立っているので、重力は船の下向き
        self.assertAlmostEqual(v["角度"], 30.0, delta=1.0)
        self.assertAlmostEqual(float(np.linalg.norm(v["短く"])), 2.0, places=4)
        arrow = st["_arrows"]["1.000,0.300,0.250"]
        self.assertAlmostEqual(arrow[1], -g * 0.5, delta=0.05 * g)
        self.assertTrue(st["display"]["text"].startswith("g=("))


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



class TestFuelAndFlow(CoreCase):
    """推進機・バルブ・配管と、精錬機・加工機・タンクの物の流れ。"""
    TANK = {"def": "tank", "dir": 2, "state": {"contents": {"fuel": 600.0}}}

    def dev(self, c, def_id, k=0):
        return [d for d in c.devs if d and d["def"] == def_id][k]

    def idx(self, c, def_id, k=0):
        return [i for i, d in enumerate(c.devs) if d and d["def"] == def_id][k]

    def use(self, w, c, i):
        w.interact({"kind": "voxel", "construct": c, "index": i, "face": 2})

    def test_direct_tank_burns_full_and_cannot_stop(self):
        w = self.world(gravity_scale=0.0)
        c = self.put(w, [((0, 0, 0), "steel", {"def": "battery", "dir": 2}), ((1, 0, 0), "steel", self.TANK),
                         ((2, 0, 0), "steel", {"def": "pipe", "dir": 2}), ((3, 0, 0), "steel", {"def": "thruster", "dir": 2})],
                     extra=40.0)
        thr = self.dev(c, "thruster")
        self.run_for(w, 0.3)
        self.assertFalse(thr["state"]["lit"])            # 点火するまでは燃えない
        charge0 = self.dev(c, "battery")["state"]["charge"]
        self.use(w, c, self.idx(c, "thruster"))
        self.run_for(w, 0.5)
        self.assertTrue(thr["state"]["lit"])
        self.assertAlmostEqual(thr["state"]["firing"], 1.0, places=3)   # バルブなし：全開
        self.use(w, c, self.idx(c, "thruster"))         # E でも消せない
        self.run_for(w, 0.2)
        self.assertTrue(thr["state"]["lit"])
        self.assertGreater(self.dev(c, "battery")["state"]["charge"], charge0 - 20000 + 1)  # 点火で使い、燃焼で発電
        self.assertGreater(c.status["production"], 15000)
        fuel = self.dev(c, "tank")["state"]["contents"]["fuel"]
        self.assertLess(fuel, 600.0 - 6.0 * 0.6)

    def test_valve_sets_thrust_and_closing_puts_it_out(self):
        w = self.world(gravity_scale=0.0)
        c = self.put(w, [((0, 0, 0), "steel", {"def": "battery", "dir": 2}), ((1, 0, 0), "steel", self.TANK),
                         ((2, 0, 0), "steel", {"def": "valve", "dir": 2, "state": {"open": 0.4}}),
                         ((3, 0, 0), "steel", {"def": "thruster", "dir": 2})], extra=40.0)
        thr = self.dev(c, "thruster")
        self.use(w, c, self.idx(c, "thruster"))
        self.run_for(w, 0.3)
        self.assertAlmostEqual(thr["state"]["firing"], 0.4, places=3)
        self.dev(c, "valve")["state"]["open"] = 0.8
        self.run_for(w, 0.1)
        self.assertAlmostEqual(thr["state"]["firing"], 0.8, places=3)
        self.use(w, c, self.idx(c, "valve"))            # E：閉じる → 消える
        self.run_for(w, 0.1)
        self.assertFalse(thr["state"]["lit"])
        self.use(w, c, self.idx(c, "valve"))            # 開け直しても、点火し直すまで燃えない
        self.run_for(w, 0.1)
        self.assertFalse(thr["state"]["lit"])
        self.assertAlmostEqual(self.dev(c, "valve")["state"]["open"], 0.8)

    def test_one_valve_is_shared_and_back_to_back_makes_only_power(self):
        w = self.world()
        c = self.put(w, [((x, 0, z), "steel", None) for x in range(-1, 4) for z in range(-1, 2)] + [
            ((-1, 1, 0), "steel", {"def": "battery", "dir": 2}), ((0, 1, 0), "steel", self.TANK),
            ((1, 1, 0), "steel", {"def": "valve", "dir": 2, "state": {"open": 0.5}}),
            ((2, 1, 0), "steel", {"def": "pipe", "dir": 2}),
            ((2, 1, 1), "steel", {"def": "thruster", "dir": 4}), ((2, 1, -1), "steel", {"def": "thruster", "dir": 5})],
            extra=0.3)
        self.run_for(w, 0.5)
        p0 = c.center_of_mass()
        for k in range(2):
            self.use(w, c, self.idx(c, "thruster", k))
        self.run_for(w, 2.0)
        f = [self.dev(c, "thruster", k)["state"]["firing"] for k in range(2)]
        self.assertAlmostEqual(f[0], 0.25, places=3)
        self.assertAlmostEqual(f[1], 0.25, places=3)
        self.assertAlmostEqual(c.status["production"], 10000.0, delta=1.0)
        self.assertLess(float(np.linalg.norm(c.center_of_mass() - p0)), 0.03)

    def test_ignition_needs_power(self):
        w = self.world(gravity_scale=0.0)
        c = self.put(w, [((1, 0, 0), "steel", self.TANK), ((3, 0, 0), "steel", {"def": "thruster", "dir": 2}),
                         ((2, 0, 0), "steel", {"def": "pipe", "dir": 2})], extra=40.0)
        self.use(w, c, self.idx(c, "thruster"))
        self.run_for(w, 0.2)
        thr = self.dev(c, "thruster")
        self.assertFalse(thr["state"]["lit"])
        self.assertIn("電気", thr["state"].get("error", ""))

    def _plant(self, w, open_tank, open_thr):
        """精錬機（水 → 燃料）を真ん中に、バルブ越しにタンクと推進機。電気は太陽光パネルとバッテリー。"""
        return self.put(w, [
            ((0, 0, 0), "steel", {"def": "refiner", "dir": 2, "state": {"store": {"water": 200.0}}}),
            ((-1, 0, 0), "steel", {"def": "valve", "dir": 2, "state": {"open": open_tank}}),
            ((-2, 0, 0), "steel", {"def": "tank", "dir": 2}),
            ((1, 0, 0), "steel", {"def": "valve", "dir": 2, "state": {"open": open_thr}}),
            ((2, 0, 0), "steel", {"def": "thruster", "dir": 3}),
            ((0, 1, 0), "steel", {"def": "battery", "dir": 2, "state": {"charge": 2.0e7}}),
        ], extra=0.6)

    def test_refiner_fuel_goes_through_open_valves(self):
        for open_tank, open_thr in ((1.0, 0.0), (0.0, 1.0), (1.0, 1.0)):
            with self.subTest(tank=open_tank, thruster=open_thr):
                w = self.world()
                c = self._plant(w, open_tank, open_thr)
                self.run_for(w, 5.0)
                before = self.dev(c, "refiner")["state"]["store"].get("fuel", 0.0)
                self.dev(c, "thruster")["state"]["ignite"] = True
                self.run_for(w, 0.5)
                tank = self.dev(c, "tank")["state"]["contents"].get("fuel", 0.0)
                burned = self.dev(c, "thruster")["state"]["lit"] or self.dev(c, "thruster")["state"].get("firing", 0) > 0
                made = self.dev(c, "refiner")["state"]["store"].get("fuel", 0.0)
                if open_tank:
                    self.assertGreater(tank, 0.0)
                    self.assertLess(made, 1e-6)          # できた燃料はタンクへ流れた
                else:
                    self.assertEqual(tank, 0.0)
                if not open_tank and not open_thr:
                    self.assertGreater(made, 0)
                if open_thr and not open_tank:
                    self.assertGreater(before, 0.0)
                    self.assertLess(made, before)        # 推進機が精錬機の燃料を燃やしている
                shutil.rmtree(paths.world_dir("t"), ignore_errors=True)

    def test_iron_moves_to_fabricator_only_when_connected(self):
        cases = {"adjacent": [((1, 0, 0), "steel", {"def": "fabricator", "dir": 2})],
                 "pipe": [((1, 0, 0), "steel", {"def": "pipe", "dir": 2}),
                          ((2, 0, 0), "steel", {"def": "fabricator", "dir": 2})],
                 "valve_closed": [((1, 0, 0), "steel", {"def": "valve", "dir": 2, "state": {"open": 0.0}}),
                                  ((2, 0, 0), "steel", {"def": "fabricator", "dir": 2})],
                 "apart": [((1, 0, 0), "steel", None), ((2, 0, 0), "steel", {"def": "fabricator", "dir": 2})]}
        for name, rest in cases.items():
            with self.subTest(name):
                w = self.world()
                c = self.put(w, [((0, 0, 0), "steel", {"def": "refiner", "dir": 2, "state": {"store": {"ore": 60.0}}}),
                                 ((0, 1, 0), "steel", {"def": "battery", "dir": 2, "state": {"charge": 2.0e7}})] + rest,
                             extra=0.6)
                self.run_for(w, 9.0)
                fab = self.dev(c, "fabricator")["state"]["store"]
                ref = self.dev(c, "refiner")["state"]["store"]
                if name in ("adjacent", "pipe"):
                    self.assertGreater(fab.get("parts", 0.0), 0.0, name)
                else:
                    self.assertEqual(fab.get("parts", 0.0), 0.0, name)
                    self.assertGreater(ref.get("iron", 0.0), 0.0, name)
                shutil.rmtree(paths.world_dir("t"), ignore_errors=True)

    def test_old_world_generator_becomes_pipe(self):
        d = paths.world_dir("t")
        (d / "scripts").mkdir(parents=True, exist_ok=True)
        s = dict(DEFAULT_SETTINGS, name="t", format=2, planets=[{"def": "terra", "position": [0, 0, 0], "craters": []}])
        (d / "world.json").write_text(json.dumps(s), encoding="utf-8")
        (d / "constructs.json").write_text(json.dumps([{"id": "g", "name": "古い", "voxel_size": 0.5, "voxels": [
            [0, 0, 0, "steel", {"def": "generator", "dir": 2, "name": "発電機", "state": {"enabled": True}}],
            [1, 0, 0, "steel", {"def": "valve", "dir": 2, "name": "バルブ", "state": {"open": True}}]]}]), encoding="utf-8")
        w = World("t", CFG).load()
        c = w.constructs["g"]
        self.assertEqual([d["def"] for d in c.devs], ["pipe", "valve"])
        self.assertEqual(c.devs[1]["state"]["open"], 1.0)

if __name__ == "__main__":
    unittest.main()
