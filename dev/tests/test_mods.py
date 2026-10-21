"""MOD の読み込みの決まりのテスト。

- MOD はワールドを始めるときに一括で読み込む（code/ の中の .py は全部 import する）
- エラーがあればそのときにまとめて出し（ModLoadError）、ワールドは始めない
- 使っている MOD を外したワールドは始められない（足りない定義を一覧にする）
"""
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from main import config, log, modcode, paths  # noqa: E402
from main.world import World, DEFAULT_SETTINGS  # noqa: E402

log.setup("CRITICAL")
CFG = json.loads(json.dumps(config.DEFAULTS))


class ModCase(unittest.TestCase):
    def setUp(self):
        # tests/test_core.py の「コアが MOD を読み込まない」ための仕掛けは、このテストの間だけ外す
        self._blockers = [f for f in sys.meta_path if type(f).__name__ == "_Block"]
        for f in self._blockers:
            sys.meta_path.remove(f)
        self.tmp = Path(tempfile.mkdtemp(prefix="alpedia_mods_"))
        self._saves, self._mods = paths.SAVES, paths.MODS
        paths.SAVES = self.tmp / "saves"
        paths.MODS = self.tmp / "mods"
        paths.MODS.mkdir(parents=True)

    def tearDown(self):
        paths.SAVES, paths.MODS = self._saves, self._mods
        modcode.reset()
        shutil.rmtree(self.tmp, ignore_errors=True)
        for f in self._blockers:
            sys.meta_path.insert(0, f)

    def mod(self, mid, files):
        d = paths.MODS / mid
        (d / "code").mkdir(parents=True, exist_ok=True)
        (d / "mod.json").write_text(json.dumps({"id": mid, "name": mid}), "utf-8")
        for rel, text in files.items():
            p = d / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text, "utf-8")

    def world(self, mods, constructs=None):
        d = paths.world_dir("w")
        (d / "scripts").mkdir(parents=True, exist_ok=True)
        s = dict(DEFAULT_SETTINGS, name="w", format=2, planets=[{"def": "terra", "position": [0, 0, 0], "craters": []}])
        (d / "world.json").write_text(json.dumps(s), "utf-8")
        (d / "mods.json").write_text(json.dumps({"enabled": mods}), "utf-8")
        if constructs is not None:
            (d / "constructs.json").write_text(json.dumps(constructs), "utf-8")
        return World("w", CFG)


GOOD_INIT = """
def world_addon():
    from main.hooks import Addon
    from .rules import Rules
    return Rules()
"""
GOOD_RULES = """
from main.hooks import Addon
class Rules(Addon):
    name = "rules"
"""


class TestModLoading(ModCase):
    def test_all_modules_are_loaded_at_start(self):
        self.mod("m1", {"code/__init__.py": GOOD_INIT, "code/rules.py": GOOD_RULES,
                        "code/later.py": "VALUE = 3\n", "code/sub/__init__.py": "", "code/sub/deep.py": "X = 1\n"})
        w = self.world(["m1"]).load()
        self.assertEqual([a.name for a in w.addons], ["rules"])
        for name in ("alpedia_mods.m1", "alpedia_mods.m1.rules", "alpedia_mods.m1.later", "alpedia_mods.m1.sub.deep"):
            self.assertIn(name, sys.modules)        # 使われていないモジュールも始めるときに読み込み済み

    def test_error_in_any_module_stops_the_world(self):
        self.mod("m1", {"code/__init__.py": GOOD_INIT, "code/rules.py": GOOD_RULES,
                        "code/later.py": "def f(:\n    pass\n"})
        with self.assertRaises(modcode.ModLoadError) as cm:
            self.world(["m1"]).load()
        msg = "\n".join(cm.exception.errors)
        self.assertIn("later.py", msg)
        self.assertIn("line 1", msg)                                 # 場所（ファイルと行）つき

    def test_runtime_error_in_module_is_reported_with_line(self):
        self.mod("m1", {"code/__init__.py": GOOD_INIT, "code/rules.py": GOOD_RULES,
                        "code/bad.py": "A = 1\nB = 1 / 0\n"})
        with self.assertRaises(modcode.ModLoadError) as cm:
            self.world(["m1"]).load()
        self.assertTrue(any("bad.py" in e and "ZeroDivisionError" in e and "2 行目" in e for e in cm.exception.errors))

    def test_missing_mod_stops_the_world(self):
        with self.assertRaises(modcode.ModLoadError) as cm:
            self.world(["gone"]).load()
        self.assertTrue(any("gone" in e for e in cm.exception.errors))

    def test_removed_mod_contents_are_listed(self):
        """MOD の素材・機器を使った構築物があるのに MOD を外した"""
        cons = [{"id": "c1", "name": "船", "voxel_size": 0.5,
                 "voxels": [[0, 0, 0, "unobtainium", None], [1, 0, 0, "steel", {"def": "warp_drive", "dir": 2, "state": {}}]]}]
        with self.assertRaises(modcode.ModLoadError) as cm:
            self.world([], cons).load()
        msg = "\n".join(cm.exception.errors)
        self.assertIn("unobtainium", msg)
        self.assertIn("warp_drive", msg)
        self.assertIn("船", msg)

    def test_bad_definition_and_missing_texture(self):
        self.mod("m2", {"materials/a.json": "{not json", "materials/b.json": json.dumps(
            {"id": "shiny", "extends": "steel", "texture": "textures/none.png"})})
        with self.assertRaises(modcode.ModLoadError) as cm:
            self.world(["m2"]).load()
        msg = "\n".join(cm.exception.errors)
        self.assertIn("a.json", msg)
        self.assertIn("none.png", msg)

    def test_client_modules_are_left_for_the_screen(self):
        """画面用のモジュール（ursina などを使う）は、画面の無い所では後回し（エラーにしない）"""
        self.mod("m1", {"code/__init__.py": GOOD_INIT, "code/rules.py": GOOD_RULES,
                        "code/ui.py": "import ursina_not_here_but_named_ursina\n"})
        if "ursina" in sys.modules:
            self.skipTest("画面のライブラリが読み込まれている")
        w = self.world(["m1"]).load()
        self.assertEqual(len(w.addons), 1)
        self.assertEqual([x[1] for x in modcode._deferred], ["alpedia_mods.m1.ui"])

    def test_builtin_mods_load_cleanly(self):
        """本体に入っている MOD（サバイバル・街）がエラーなく読み込める"""
        paths.MODS = self._mods
        w = self.world(["survival", "city"]).load()
        self.assertEqual(sorted(a.name for a in w.addons), ["city", "survival"])


SKIN = "skinmodel(dummy)"


class TestVRMMod(ModCase):
    """VRM の見た目 MOD（モデルのファイルがあるときだけ）"""

    def setUp(self):
        super().setUp()
        paths.MODS = self._mods
        d = paths.MODS / SKIN
        self.files = sorted(d.glob("*.vrm")) if d.is_dir() else []
        if not self.files:
            self.skipTest("VRM のファイルが無い")
        modcode.load([SKIN], [])
        import importlib
        self.vrm = importlib.import_module(f"alpedia_mods.{SKIN}.vrm")
        self.motion = importlib.import_module(f"alpedia_mods.{SKIN}.motion")
        self.spring = importlib.import_module(f"alpedia_mods.{SKIN}.spring")

    def test_world_with_vrm_player_loads(self):
        d = paths.world_dir("w")
        w = self.world([SKIN])
        s = json.loads((d / "world.json").read_text("utf-8"))
        s["player"] = "player1"
        (d / "world.json").write_text(json.dumps(s), "utf-8")
        w = World("w", CFG).load()
        self.assertTrue(w.player.defn["model"].endswith(".vrm"))
        self.assertEqual([a.name for a in w.addons], ["skinmodel"])

    def test_rest_pose_and_motion(self):
        import numpy as np
        v = self.vrm.VRM(self.files[0])
        M0 = v.pose()
        m = v.meshes[0]
        P = np.c_[m.pos, np.ones(len(m.pos))]
        sk = sum(m.weights[:, k, None] * np.einsum("nij,nj->ni", M0[m.joints[:, k]], P) for k in range(4))
        self.assertLess(np.abs(sk - m.pos).max(), 1e-4)              # 初期姿勢では形が変わらない
        mo = self.motion.Motion(v)
        R_, U, F = v.basis()
        foot = v.human["leftFoot"]
        # 左脚を前に出す → 左足が前（F の向き）に動く
        mo.p.update(leg_l=0.6, knee_l=0.0)
        G = v.globals(*self._apply(v, mo.rotations()))
        self.assertGreater((G[foot, :3, 3] - v.rest_global[foot, :3, 3]) @ F, 0.2)
        for st in ({"speed": 0}, {"speed": 5, "on_ground": True}, {"on_ground": False, "vup": -5},
                   {"flying": True}, {"water": 1.0}, {"seated": True}):
            for _ in range(30):
                rot, hips, blink, seat, _ = mo.update(dict(st, on_ground=st.get("on_ground", True)), 1 / 60)
            self.assertTrue(np.isfinite(v.pose(rot, hips)).all())
        # かわいいモーション MOD の動き：どれも形が崩れず、腕などが動く
        import importlib.util
        spec = importlib.util.spec_from_file_location("cute_motions_test", paths.MODS / "cute_motions" / "code" / "motions.py")
        cm = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cm)
        hand = v.human["rightHand"]
        st = {"speed": 0, "on_ground": True}
        for _ in range(60):
            rot0, hips0, *_ = mo.update(st, 1 / 60)
        G0 = v.globals(*self._apply(v, rot0))
        for name, fn in cm.MOTIONS.items():
            for k in range(90):
                rot, hips, blink, seat, face = mo.update(st, 1 / 60, fn, k / 60)
            M = v.pose(rot, hips)
            self.assertTrue(np.isfinite(M).all(), name)
            G = v.globals(*self._apply(v, rot))
            moved = np.linalg.norm(G[hand, :3, 3] - G0[hand, :3, 3])
            self.assertGreater(moved, 0.05, name)
        if True:
            # 「ばんざい」では右手が頭より上
            for k in range(90):
                rot, *_ = mo.update(st, 1 / 60, cm.banzai, 0.0)
            G = v.globals(*self._apply(v, rot))
            R_, U, F = v.basis()
            self.assertGreater((G[hand, :3, 3] - G[v.human["head"], :3, 3]) @ U, 0.1)

    def _apply(self, v, rot):
        from importlib import import_module
        q = import_module(f"alpedia_mods.{SKIN}.vrm")
        T, R = v.T.copy(), v.R.copy()
        for n, r in rot.items():
            W = v.rest_rot[n]
            R[n] = q.qmul(R[n], q.qmul(q.qinv(W), q.qmul(r, W)))
        return T, R, v.S

    def test_spring_follows_motion(self):
        import numpy as np
        v = self.vrm.VRM(self.files[0])
        sp = self.spring.Spring(v)
        if not sp.n:
            self.skipTest("揺れものが無い")
        A = np.eye(3)
        for i in range(60):
            M = v.pose(None, None, (sp, A, np.array([0.0, 0.0, 0.08 * i]), 1 / 60))   # 前へ走る
        self.assertTrue(np.isfinite(M).all())
        self.assertGreater(np.abs(M - v.pose()).max(), 1e-3)                     # 揺れている


if __name__ == "__main__":
    unittest.main()
