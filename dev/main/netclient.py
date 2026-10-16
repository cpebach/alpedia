"""マルチプレイ（LAN）の参加者側。

参加の流れ（Joiner、別スレッド）：
    つなぐ → 名前を送る → MOD のファイルの一覧を受け取り、足りない物だけ受け取る（自分の mods/ に同じ物があれば写す）
    → ワールドの中身を受け取って、自分の PC の saves/.net/world に書く
その後、メインスレッドでそのワールドを普通に読み込み（MOD もワールドを始めるときに一括）、Client をつないでゲームを始める。

遊んでいる間（Client）：
    自分の体（歩く・跳ぶ）だけ自分の PC で動かしてホストへ知らせる。構築物・ほかの人・MOD の状態はホストから受け取る。
    置く・壊す・掘る・E などの操作はホストへ送る（ホストのワールドで行い、結果が返ってくる）。
    F9 の出力は自分の PC（saves/.net/exports）に書く。ほかのワールドの F8 からも読み込める。
"""
import hashlib
import json
import queue
import shutil
import threading
import time

import numpy as np

from . import log, net, netsync, paths

L = log.get("netclient")

ME_HZ = 20.0
EDIT_HZ = 2.0
FILE_HZ = 1.0


def net_dir():
    return paths.SAVES / ".net"


def mods_cache():
    return paths.ROOT / "cache" / "net_mods"


def _sha1(f):
    h = hashlib.sha1()
    with open(f, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


class JoinError(Exception):
    pass


class Joiner:
    """参加の準備（別スレッド）。status に今していること、done / error / result。"""

    def __init__(self, host, port, name, skin=None):
        self.host, self.port, self.name, self.skin = host, int(port), name, skin
        self.status = f"{host}:{port} につないでいます…"
        self.done = False
        self.error = None
        self.result = None
        self.conn = None
        self.cancelled = False
        self.orig_mods = paths.MODS
        threading.Thread(target=self._run, daemon=True, name="net-join").start()

    def cancel(self):
        self.cancelled = True
        if self.conn is not None:
            self.conn.close()

    def _wait(self, *types, timeout=60.0):
        end = time.time() + timeout
        while True:
            if self.cancelled:
                raise JoinError("やめました")
            try:
                _, m = self.conn.inbox.get(timeout=0.2)
            except queue.Empty:
                if time.time() > end:
                    raise JoinError("ホストから返事がありません")
                continue
            t = m.get("t")
            if t == "error":
                raise JoinError(m.get("text", "参加できません"))
            if t in ("_closed", "bye"):
                raise JoinError(m.get("text") or "ホストとのつながりが切れました")
            if t in types:
                return m

    def _run(self):
        try:
            self._join()
            self.done = True
        except Exception as e:  # noqa: BLE001
            if not isinstance(e, JoinError):
                L.exception("参加に失敗")
            self.error = str(e) or type(e).__name__
            if self.conn is not None:
                self.conn.close()

    def _join(self):
        try:
            self.conn = net.connect(self.host, self.port)
        except OSError as e:
            raise JoinError(f"{self.host}:{self.port} につなげません（{e}）。ホストの IP・ポートと、"
                            "ファイアウォールで Alpedia が許可されているかを確かめてください")
        self.conn.send({"t": "hello", "name": self.name, "proto": net.PROTO})
        self.status = "ホストの返事を待っています…"
        w = self._wait("welcome")
        self.welcome = w
        # MOD：足りない物だけ受け取る
        need, total = [], 0
        cache = mods_cache()
        for mid, files in w["manifest"].items():
            dst = cache / mid
            want = {rel for rel, _, _ in files}
            if dst.is_dir():
                for f in sorted(dst.rglob("*"), reverse=True):
                    rel = f.relative_to(dst).as_posix()
                    if f.is_file() and rel not in want:
                        f.unlink()
            for rel, size, sha in files:
                t = dst / rel
                if t.is_file() and t.stat().st_size == size and _sha1(t) == sha:
                    continue
                mine = self.orig_mods / mid / rel
                if mine.is_file() and mine.stat().st_size == size and _sha1(mine) == sha:
                    t.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(mine, t)
                    continue
                need.append([mid, rel])
                total += size
        if need:
            got = 0
            self.status = f"MOD を受け取っています… 0 / {total / 1e6:.1f} MB"
            self.conn.send({"t": "get_files", "files": need})
            parts = {}
            while True:
                m = self._wait("file", "files_done", timeout=120.0)
                if m["t"] == "files_done":
                    break
                key = (m["mod"], m["rel"])
                tmp = cache / m["mod"] / (m["rel"] + ".part")
                tmp.parent.mkdir(parents=True, exist_ok=True)
                if key not in parts:
                    parts[key] = open(tmp, "wb")
                parts[key].write(m["data"])
                got += len(m["data"])
                self.status = f"MOD を受け取っています… {got / 1e6:.1f} / {total / 1e6:.1f} MB"
                if m.get("last"):
                    parts.pop(key).close()
                    final = cache / m["mod"] / m["rel"]
                    if m.get("missing"):
                        tmp.unlink()
                        raise JoinError(f"ホストの MOD のファイル {m['mod']}/{m['rel']} を受け取れません")
                    tmp.replace(final)
        self.status = "ワールドを受け取っています…"
        self.conn.send({"t": "ready", "def": self.skin})
        m = self._wait("world", timeout=120.0)
        wd = net_dir() / "world"
        if wd.exists():
            shutil.rmtree(wd, ignore_errors=True)
        (wd / "scripts").mkdir(parents=True, exist_ok=True)
        for fname, data in m["files"].items():
            (wd / fname).write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
        for name, data in (m.get("scripts") or {}).items():
            if "/" not in name and "\\" not in name and ".." not in name:
                (wd / "scripts" / name).write_bytes(data)
        (net_dir() / "exports").mkdir(parents=True, exist_ok=True)
        self.status = "ワールドを読み込んでいます…"
        self.result = {"conn": self.conn, "pid": w["pid"], "name": w["name"], "world": w["world"], "dir": wd,
                       "host": w.get("host", ""), "mods": w["mods"]}


class Client:
    """遊んでいる間の参加者側。ゲーム画面が毎フレーム frame(game, dt) と、物理の刻みごとに step(...) を呼ぶ。"""

    role = "client"

    def __init__(self, info, world, cfg):
        self.conn = info["conn"]
        self.pid = info["pid"]
        self.name = info["name"]
        self.host_name = info.get("host", "")
        self.w = world
        self.cfg = cfg
        world.net = self
        world.exports_dir = net_dir() / "exports"
        world.local_player.name = self.name
        self.others = {}
        self.files = netsync.FileSync(world.dir / "scripts")
        self.files.mark_all()
        self.files.changes()
        self.closed = None              # 切れた理由（ゲーム画面がメニューへ戻る）
        self.look = np.array([0, 0, 1.0])
        self.edit = None                # 機器の画面で編集中：[構築物ID, 開いたときの状態, 次に送る時刻]
        self.frozen_until = {}
        self._t = dict.fromkeys(("me", "file"), 0.0)
        self._hotbar_key = None
        self._last_s = time.monotonic()
        self._patch_world()
        L.info("参加中: %s（ホスト %s）", world.name, self.host_name)

    # --- ワールドの操作をホストへ回す ----------------------------------------------
    def _patch_world(self):
        w = self.w

        def remove(hit):
            self.act("remove")
            return True

        def dig(hit, radius=0.8):
            self.act("dig")
            return True

        def place(hit, item, dir_index, look_dir):
            self.act("place", item=list(item), dir=int(dir_index), air=hit is None, look=list(map(float, look_dir)))
            return True

        def interact(hit):
            self.act("interact", none=hit is None)

        def key_pressed(key):
            self.act("key", key=key)

        def place_import(data, position=None, look_dir=(0, 0, 1)):
            self.act("import", data=netsync.pack_import(data), look=list(map(float, look_dir)))
            w.message(f"読み込み: {data.get('name', '')}（ホストに送りました）")
            return None
        w.remove, w.dig, w.place, w.interact = remove, dig, place, interact
        w.key_pressed, w.place_import = key_pressed, place_import

    def act(self, a, **kw):
        p = self.w.player
        msg = {"t": "act", "a": a, "eye": p.eye().tolist(), "look": list(map(float, kw.pop("look", self.look)))}
        msg.update(kw)
        self.send(msg)

    def actuator_run(self, c, i):
        """アクチュエーターの画面の「実行」：設定を先に送ってから、ホストで実行する。"""
        self._flush_edit(force=True)
        self.send({"t": "act", "a": "actuator", "id": c.id, "cell": list(c.cells[i])})

    def send(self, msg):
        self.conn.send(msg)

    def can_save(self):
        return False

    def save_extra(self):
        return {}

    def addresses(self):
        return []

    # --- 毎フレーム ------------------------------------------------------------
    def frame(self, game, dt):
        w = self.w
        self._process(game)
        if self.closed:
            return
        netsync.advance(w, dt)
        w.time += dt
        for a in w.addons:
            try:
                a.net_client_tick(w, dt)
            except Exception:  # noqa: BLE001
                L.exception("MOD '%s' の net_client_tick でエラー", a.name)
        now = time.monotonic()
        self._edit_sync(game, now)
        if now - self._t["me"] >= 1.0 / ME_HZ:
            self._t["me"] = now
            self._send_me()
        if now - self._t["file"] >= 1.0 / FILE_HZ:
            self._t["file"] = now
            ch = self.files.changes()
            if ch:
                self.send({"t": "files", "files": ch})

    def step(self, dt, move, jump, thrust):
        """自分の体を動かす（物理の刻みごと）。"""
        w = self.w
        w.player.update(w, dt, move, jump, thrust)
        for c in w.constructs.values():         # 体重は手元では使わない（ホストで伝える）
            if c.force.any():
                c.force[:] = 0

    def _send_me(self):
        p = self.w.player
        msg = {"t": "me", "pos": p.pos.tolist(), "vel": p.vel.tolist(), "up": p.up.tolist(),
               "gv": np.asarray(p.ground_vel, float).tolist(), "og": bool(p.on_ground), "jp": bool(p.jetpack),
               "fl": bool(p.flying), "thrust": np.asarray(p.thrust, float).tolist(), "wt": float(p.in_water),
               "keys": sorted(self.w.input_keys), "fwd": np.asarray(p.look_fwd, float).tolist(),
               "pitch": float(p.pitch), "def": p.defn.get("id"),
               "stand": [p.stand[0], list(p.stand[1])] if p.stand else None}
        hb = netsync.jsonable([list(x) if x else None for x in p.hotbar]) if p.hotbar else None
        key = json.dumps(hb)
        if key != self._hotbar_key:
            self._hotbar_key = key
            msg["hotbar"] = hb
        self.send(msg)

    def _edit_sync(self, game, now):
        """機器・プログラムの画面で変えた分をホストへ送る（開いている間は2回/秒、閉じたときにもう1回）。"""
        ed = getattr(game, "editor", None)
        c = getattr(ed, "c", None) if ed is not None else None
        if c is not None and getattr(c, "devs", None) is not None:
            if self.edit is None or self.edit[0] is not c:
                self._flush_edit(force=True)
                self.edit = [c, netsync.dev_base(c), now + 1.0 / EDIT_HZ]
            elif now >= self.edit[2]:
                self._flush_edit()
                self.edit[2] = now + 1.0 / EDIT_HZ
            self.frozen_until[c.id] = now + 1.5
        elif self.edit is not None:
            self._flush_edit(force=True)
            self.edit = None

    def _flush_edit(self, force=False):
        if self.edit is None:
            return
        c, base = self.edit[0], self.edit[1]
        patches = netsync.dev_patch(c, base)
        if patches:
            self.send({"t": "dev", "id": c.id, "patches": patches})
            self.edit[1] = netsync.dev_base(c)
        self.frozen_until[c.id] = time.monotonic() + 1.5

    # --- 受け取り --------------------------------------------------------------
    def _process(self, game):
        w = self.w
        for _ in range(5000):
            try:
                _, m = self.conn.inbox.get_nowait()
            except queue.Empty:
                break
            t = m.get("t")
            try:
                if t == "s":
                    self._state(m)
                elif t == "m":
                    now = time.monotonic()
                    for e in m["c"]:
                        c = w.constructs.get(e["id"])
                        if c is not None:
                            netsync.apply_meta(c, e, devices=self.frozen_until.get(c.id, 0) < now)
                elif t == "a":
                    for name, st in m["s"].items():
                        a = w.addon(name)
                        if a is not None:
                            a.net_apply(w, st)
                elif t == "you":
                    self._you(m)
                elif t == "files":
                    for name, data in m["files"].items():
                        self.files.put(name, data)
                elif t == "note":
                    w.message(m.get("text", ""))
                elif t in ("bye", "_closed"):
                    self.closed = m.get("text") or m.get("why") or "ホストとのつながりが切れました"
                    return
            except Exception:  # noqa: BLE001
                L.exception("ホストからのメッセージ %s の処理でエラー", t)

    def _state(self, m):
        w = self.w
        w.time = float(m["time"])
        dt = float(m.get("dt", 0.05))
        for e in m["c"]:
            netsync.apply_construct(w, e, dt)
        for cid in m.get("gone", []):
            w.constructs.pop(cid, None)
        for cr in m.get("craters", []):
            if cr["p"] < len(w.planets):
                pl = w.planets[cr["p"]]
                pl.craters = pl.craters[:cr["from"]] + [list(x) for x in cr["list"]]
                pl.version += 1
        seen = set()
        for d in m.get("pl", []):
            pid = d["pid"]
            if pid == self.pid:
                continue
            seen.add(pid)
            p = self.others.get(pid)
            if p is None:
                p = self.others[pid] = w.make_player(d["def"], d["pos"])
            netsync.apply_player_public(w, p, d)
        for pid in [k for k in self.others if k not in seen]:
            del self.others[pid]

    def _you(self, m):
        p = self.w.player
        p.inventory = dict(m.get("inventory") or {})
        s = m.get("seated")
        p.seated = (s[0], tuple(s[1])) if s else None
        p.starter_given = bool(m.get("starter_given"))
        p.credits_given = bool(m.get("credits_given"))
        p.out_of_fuel = bool(m.get("out_of_fuel"))
        for text in m.get("msgs", []):
            self.w.message(text)

    def close(self):
        try:
            self.send({"t": "bye"})
            time.sleep(0.05)
        except Exception:  # noqa: BLE001
            pass
        self.conn.close()
        self.w.net = None
        L.info("参加を終了")
