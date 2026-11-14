"""マルチプレイ（LAN）のテスト：同じ PC の中でホストと参加者をつなぎ、同じ世界が見えることを確かめる（画面なし）。"""
import json
import shutil
import sys
import tempfile
import time
import types
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import numpy as np  # noqa: E402

from main import config, log, modcode, paths, netclient, netserver, netsync  # noqa: E402
from main.world import World, DEFAULT_SETTINGS  # noqa: E402

log.setup("CRITICAL")
CFG = json.loads(json.dumps(config.DEFAULTS))
DT = 1 / 60


class NetCase(unittest.TestCase):
    def setUp(self):
        self._blockers = [f for f in sys.meta_path if type(f).__name__ == "_Block"]
        for f in self._blockers:
            sys.meta_path.remove(f)
        self.tmp = Path(tempfile.mkdtemp(prefix="alpedia_net_"))
        self._saves, self._mods_cache = paths.SAVES, netclient.mods_cache
        paths.SAVES = self.tmp / "saves"
        netclient.mods_cache = lambda: self.tmp / "cache"
        self.host = self.client = None

    def tearDown(self):
        if self.client is not None:
            self.client.close()
        if self.host is not None:
            self.host.close()
        paths.SAVES = self._saves
        netclient.mods_cache = self._mods_cache
        modcode.reset()
        shutil.rmtree(self.tmp, ignore_errors=True)
        for f in self._blockers:
            sys.meta_path.insert(0, f)

    def make_host(self, mods):
        d = paths.world_dir("h")
        (d / "scripts").mkdir(parents=True, exist_ok=True)
        s = dict(DEFAULT_SETTINGS, name="h", format=2, planets=[{"def": "terra", "position": [0, 0, 0], "craters": []}])
        (d / "world.json").write_text(json.dumps(s), "utf-8")
        (d / "mods.json").write_text(json.dumps({"enabled": mods}), "utf-8")
        (d / "scripts" / "a.py").write_text("def tick(api):\n    pass\n", "utf-8")
        w = World("h", CFG).load()
        p = w.player
        for _ in range(60):
            w.step(DT)
        look = np.array([0.0, 0.0, 1.0])
        w.place(None, ("material", "steel"), 2, look)
        w.place(None, ("device", "computer"), 2, look + np.array([0.6, 0, 0]))
        self.hw = w
        self.host = netserver.Host(w, CFG, port=0)
        return w, p

    def join(self, name="bob"):
        j = netclient.Joiner("127.0.0.1", self.host.port, name)
        end = time.time() + 30
        while not (j.done or j.error) and time.time() < end:
            self.hw.step(DT)
            self.host.tick(DT)
            time.sleep(0.002)
        self.assertIsNone(j.error)
        self.assertTrue(j.done, j.status)
        info = j.result
        orig = paths.MODS
        paths.MODS = netclient.mods_cache()
        try:
            cw = World(info["world"], CFG, directory=info["dir"]).load()
        finally:
            paths.MODS = orig
        self.cw = cw
        self.client = netclient.Client(info, cw, CFG)
        self.game = types.SimpleNamespace(editor=None)
        return cw

    def run_frames(self, n, move=np.zeros(3), seconds=None):
        end = time.time() + (seconds or 0)
        k = 0
        while k < n or time.time() < end:
            k += 1
            self.hw.step(DT)
            self.host.tick(DT)
            self.client.frame(self.game, DT)
            self.client.step(DT, move, False, np.zeros(3))
            time.sleep(0.003)

    def test_join_and_see_same_world(self):
        w, _ = self.make_host(["survival", "example_mod"])           # 前の版のサバイバルの MOD → 遊び方
        cw = self.join()
        self.run_frames(200)                                          # 落ちて止まるまで
        self.assertEqual(set(cw.constructs), set(w.constructs))
        self.assertTrue((self.tmp / "cache" / "example_mod" / "mod.json").exists())     # MOD を受け取った
        self.assertFalse(w.creative or cw.creative, "参加者もサバイバル")
        self.assertTrue((cw.dir / "scripts" / "a.py").exists())
        for cid, c in w.constructs.items():
            self.assertLess(np.abs(cw.constructs[cid].pos - c.pos).max(), 0.02)
        self.assertIn(self.client.pid, w.remote)
        self.assertEqual(len(self.client.others), 1)                 # ホストが見える
        # 参加者の体の位置がホストに届く
        self.assertLess(np.linalg.norm(w.remote[self.client.pid].pos - cw.player.pos), 0.5)

    def test_client_actions_run_on_host(self):
        w, _ = self.make_host(["survival"])
        cw = self.join()
        self.run_frames(10)
        rp = w.remote[self.client.pid]
        steel = dict(rp.inventory)
        n0 = len(w.constructs)
        self.client.look = np.array([0.0, 0.3, 1.0]) / np.linalg.norm([0, 0.3, 1])
        cw.place(None, ("material", "steel"), 2, self.client.look)
        self.run_frames(20)
        self.assertEqual(len(w.constructs), n0 + 1)                   # ホストのワールドに置かれた
        self.assertEqual(len(cw.constructs), n0 + 1)                  # 参加者にも届いた
        self.assertNotEqual(rp.inventory, steel)                      # 参加者の持ち物から払った
        self.assertEqual(cw.player.inventory, rp.inventory)           # 持ち物が参加者に届く

    def test_device_edit_and_script_file(self):
        w, _ = self.make_host([])
        cw = self.join()
        self.run_frames(10)
        cid, i = next((c.id, i) for c in cw.constructs.values() for i, d in enumerate(c.devs) if d)
        c = cw.constructs[cid]
        self.game.editor = types.SimpleNamespace(c=c)
        self.run_frames(2)
        c.devs[i]["name"] = "メイン"
        c.devs[i]["state"]["script"] = "a.py"
        (cw.dir / "scripts" / "b.py").write_text("X = 1\n", "utf-8")
        self.game.editor = None
        self.run_frames(10, seconds=1.5)
        hd = w.constructs[cid].devs[i]
        self.assertEqual(hd["name"], "メイン")
        self.assertEqual(hd["state"]["script"], "a.py")
        self.assertEqual((w.dir / "scripts" / "b.py").read_text("utf-8"), "X = 1\n")
        # ホストで変えたファイルも参加者に届く
        (w.dir / "scripts" / "c.py").write_text("Y = 2\n", "utf-8")
        self.run_frames(10, seconds=1.5)
        self.assertTrue((cw.dir / "scripts" / "c.py").exists())

    def test_leave_and_rejoin_keeps_state(self):
        w, _ = self.make_host(["survival"])
        self.join("carol")
        self.run_frames(10)
        inv = dict(w.remote[self.client.pid].inventory)
        self.client.close()
        self.client = None
        for _ in range(30):
            w.step(DT)
            self.host.tick(DT)
            time.sleep(0.002)
        self.assertFalse(w.remote)
        self.assertIn("carol", self.host.known)
        w.save()
        self.assertIn("carol", json.loads((w.dir / "players.json").read_text("utf-8")))
        cw = self.join("carol")
        self.run_frames(5)
        self.assertEqual(cw.player.inventory, inv)                    # 戻ったら続きから（もう一度は配られない）

    def test_city_world(self):
        """街パックのワールド：参加者にも同じ街ができ、街での取引はホストで行われる"""
        w, _ = self.make_host(["survival", "city"])
        cw = self.join()
        self.run_frames(30)
        hc, cc = w.addon("city"), cw.addon("city")
        self.assertEqual([c.id for c in hc.cities.cities], [c.id for c in cc.cities.cities])
        orbit = [c for c in hc.cities.cities if c.kind == "orbit"]
        if orbit:
            o = orbit[0]
            # 動くステーションも同じ所（通信の 1〜2 フレームの遅れの分だけはずれてよい：軌道の速さ 60 m/s ほど × 1/60 秒）
            self.assertLess(np.linalg.norm(cc.cities.by_id[o.id].x - o.x), 2.5)
        rp = w.remote[self.client.pid]
        city = cc.cities.cities[0]
        before = rp.inventory.get("credits", 0.0)
        self.assertGreater(before, 0)                                 # 参加者にも最初のクレジット
        res = next(iter(__import__("alpedia_mods.city.citylife", fromlist=["prices"]).prices(city)))
        cw.addon_call("city", "economy.buy", city, res, 1)
        self.run_frames(10)
        self.assertLess(rp.inventory.get("credits", 0.0), before)     # ホストで払った

    def test_messages_encode(self):
        from main import net
        m = {"a": np.arange(6, dtype=np.float32).reshape(2, 3), "b": b"xyz" * 1000, "c": [1, "あ", None]}
        import struct
        data = net.encode(m)
        ln, flag = struct.unpack("<IB", data[:5])
        out = net.decode(flag, data[5:])
        self.assertTrue(np.array_equal(out["a"], m["a"]))
        self.assertEqual(out["b"], m["b"])
        self.assertEqual(out["c"], m["c"])


class TestNetSafety(NetCase):
    """S-02：合言葉・ホストの MOD のプログラムの確認・ホストのファイルを読ませない・大きすぎるメッセージ。"""

    def _try_join(self, password="", answer=None, empty_mods=False):
        orig = paths.MODS
        if empty_mods:
            paths.MODS = self.tmp / "my_mods"            # 自分の mods/ が空：ホストの MOD のプログラムは全部「違う」
            paths.MODS.mkdir(exist_ok=True)
        try:
            j = netclient.Joiner("127.0.0.1", self.host.port, "bob", password=password)
        finally:
            paths.MODS = orig
        end = time.time() + 30
        asked = None
        while not (j.done or j.error) and time.time() < end:
            if getattr(j, "consent", None) and answer is not None and asked is None:
                asked = list(j.consent)
                j.consent_answer = answer
            self.hw.step(DT)
            self.host.tick(DT)
            time.sleep(0.002)
        return j, asked

    def test_password(self):
        cfg = json.loads(json.dumps(CFG))
        cfg["multiplayer"]["password"] = "あいことば"
        w, _ = self.make_host(["example_mod"])
        self.host.close()
        self.host = netserver.Host(w, cfg, port=0)
        self.assertTrue(self.host._info()["locked"])
        j, _ = self._try_join("")
        self.assertIn("合言葉が要ります", j.error or "")
        j, _ = self._try_join("ちがう")
        self.assertIn("合言葉が違います", j.error or "")
        j, _ = self._try_join("あいことば")
        self.assertIsNone(j.error)
        self.assertTrue(j.done)

    def test_foreign_mod_code_needs_consent(self):
        self.make_host(["example_mod", "cute_motions"])     # cute_motions にはプログラム（code/*.py）がある
        j, asked = self._try_join(answer=False, empty_mods=True)
        self.assertTrue(asked and any("cute_motions" in a for a in asked), asked)
        self.assertFalse(any("example_mod" in a for a in asked), "プログラムの無い MOD は聞かない")
        self.assertIn("動かさない", j.error or "")
        self.assertFalse((self.tmp / "cache" / "cute_motions" / "code").exists(), "断ったのにプログラムを受け取った")
        j, asked = self._try_join(answer=True, empty_mods=True)
        self.assertTrue(asked)
        self.assertTrue(j.done, j.error)
        # 自分の mods/ と同じ物なら聞かない
        j, asked = self._try_join(answer=False)
        self.assertIsNone(asked)
        self.assertTrue(j.done, j.error)

    def test_host_only_sends_listed_mod_files(self):
        from main import net
        self.make_host(["example_mod"])
        conn = net.connect("127.0.0.1", self.host.port, 5.0)

        def pump(secs=0.5):
            out, end = [], time.time() + secs
            while time.time() < end:
                self.host.tick(DT)
                try:
                    out.append(conn.inbox.get(timeout=0.02)[1])
                except Exception:  # noqa: BLE001
                    pass
            return out
        bad = [["..", "main/config.py"], ["example_mod", "../../main/config.py"], [str(paths.ROOT), "run.py"]]
        conn.send({"t": "get_files", "files": bad})
        self.assertEqual([m for m in pump() if m.get("t") == "file"], [], "受け付ける前にファイルを送った")
        conn.send({"t": "hello", "name": "eve", "proto": net.PROTO})
        pump()
        conn.send({"t": "get_files", "files": bad + [["example_mod", "mod.json"]]})
        files = [m for m in pump(1.0) if m.get("t") == "file"]
        got = {(m["mod"], m["rel"]): m for m in files}
        for mid, rel in bad:
            self.assertTrue(got[(mid, rel)].get("missing"), f"{mid}/{rel} を送った")
            self.assertEqual(got[(mid, rel)]["data"], b"")
        self.assertFalse(got[("example_mod", "mod.json")].get("missing"))
        conn.close()

    def test_oversized_message_is_refused(self):
        from main import net
        import struct
        import zlib
        # 展開すると大きすぎる物（圧縮爆弾）は、展開しきる前に断る
        z = zlib.compressobj(9)
        chunk = b"\0" * (1 << 20)
        bomb = b"".join(z.compress(chunk) for _ in range(600)) + z.flush()
        with self.assertRaises(Exception):
            net.decode(1, bomb)
        # 嘘の長さ（4 GB 近く）のメッセージ：そのつながりだけ切れて、ホストは動き続ける
        self.make_host([])
        import socket
        s = socket.create_connection(("127.0.0.1", self.host.port), timeout=5)
        s.sendall(struct.pack("<IB", 0xFFFFFFF0, 0))
        time.sleep(0.3)
        self.host.tick(DT)
        s.close()
        j, _ = self._try_join()                          # ほかの人はふつうに参加できる
        self.assertTrue(j.done, j.error)


if __name__ == "__main__":
    unittest.main()
