"""マルチプレイ（LAN）のホスト側。

ホストの PC がワールド（セーブ・MOD）を持ち、物理・プログラム・MOD の決まりを全部動かす。
参加者の PC は結果を受け取って描き、自分の体（歩く・跳ぶ）だけを自分で動かして知らせてくる。
参加者の操作（置く・壊す・掘る・E・機器の設定・プログラム）はホストに送られ、ホストのワールドで行う。

ゲーム画面（client/game.py）は、毎フレーム物理を進めたあとに tick(dt) を呼ぶ。
"""
import hashlib
import json
import queue
import time

import numpy as np

from . import log, net, netsync, paths

L = log.get("netserver")

STATE_HZ = 20.0         # 構築物・プレイヤーの位置
META_HZ = 8.0           # 機器・資源など
ADDON_HZ = 10.0         # MOD の状態
FILE_HZ = 1.0           # プログラムのファイル
CHUNK = 512 * 1024


def mod_manifest(mod_ids):
    """MOD のファイルの一覧 {MOD名: [[相対パス, 大きさ, sha1], ...]}（参加者が足りない物だけ受け取るため）。"""
    out = {}
    for mid in mod_ids:
        root = paths.MODS / mid
        files = []
        if root.is_dir():
            for f in sorted(root.rglob("*")):
                if not f.is_file() or "__pycache__" in f.parts or f.suffix in (".pyc", ".pyo"):
                    continue
                rel = f.relative_to(root).as_posix()
                files.append([rel, f.stat().st_size, _sha1(f)])
        out[mid] = files
    return out


_SHA = {}


def _sha1(f):
    st = f.stat()
    key = (str(f), st.st_mtime_ns, st.st_size)
    if key not in _SHA:
        h = hashlib.sha1()
        with open(f, "rb") as fh:
            for chunk in iter(lambda: fh.read(1 << 20), b""):
                h.update(chunk)
        _SHA[key] = h.hexdigest()
    return _SHA[key]


class Peer:
    def __init__(self, conn, addr):
        self.conn = conn
        self.addr = addr
        self.name = None
        self.pid = None
        self.player = None
        self.ready = False
        self.you_key = None


class Host:
    role = "host"

    def __init__(self, world, cfg, port=None):
        self.w = world
        self.cfg = cfg
        mp = cfg.get("multiplayer", {})
        self.name = mp.get("name") or _default_name()
        self.inbox = queue.Queue()
        self.listener = net.Listener(port if port is not None else int(mp.get("port", net.PORT)), self.inbox)
        self.port = self.listener.port
        self.announcer = net.Announcer(self._info)
        self.peers = {}             # conn -> Peer
        self.next_pid = 1
        self.cons = netsync.ConsTracker()
        self.meta = netsync.MetaTracker()
        self.craters = {k: len(p.craters) for k, p in enumerate(world.planets)}
        self.addon_sent = {}
        self.files = netsync.FileSync(world.dir / "scripts")
        self.files.mark_all()
        self.files.changes()
        self.known = {}
        try:
            self.known = json.loads((world.dir / "players.json").read_text(encoding="utf-8"))
        except Exception:  # noqa: BLE001
            self.known = {}
        self._manifest = None
        self._t = dict.fromkeys(("state", "meta", "addon", "file"), 0.0)
        self._last_state = world.time
        world.net = self
        world.local_player.name = self.name
        L.info("LAN に公開: %s ポート %d（%s）。参加できないときは、この PC のファイアウォールで TCP %d・UDP %d を許可してください"
               "（LANで公開できるようにする.bat）", world.name, self.port, ", ".join(net.local_addresses()),
               self.port, net.DISCOVERY_PORT)

    # --- 情報 ------------------------------------------------------------
    def _info(self):
        return {"world": self.w.name, "host": self.name, "port": self.port, "players": 1 + len(self.w.remote),
                "mods": list(self.w.mods)}

    def addresses(self):
        return [f"{ip}:{self.port}" for ip in net.local_addresses()]

    def can_save(self):
        return True

    def save_extra(self):
        for p in self.w.remote.values():
            self.known[p.name] = p.to_dict()
        return {"players.json": self.known} if self.known else {}

    def others(self):
        return list(self.w.remote.values())

    def send(self, msg):
        pass

    # --- 毎フレーム --------------------------------------------------------
    def tick(self, dt):
        self._process()
        now = time.monotonic()
        ready = [p for p in self.peers.values() if p.ready and p.conn.alive]
        if not ready:
            self._last_state = self.w.time
            return
        if now - self._t["state"] >= 1.0 / STATE_HZ:
            self._t["state"] = now
            self._send_state(ready, now)
        if now - self._t["meta"] >= 1.0 / META_HZ:
            self._t["meta"] = now
            self._send_meta(ready)
        if now - self._t["addon"] >= 1.0 / ADDON_HZ:
            self._t["addon"] = now
            self._send_addons(ready)
        if now - self._t["file"] >= 1.0 / FILE_HZ:
            self._t["file"] = now
            ch = self.files.changes()
            if ch:
                self._broadcast(ready, {"t": "files", "files": ch})
        for p in ready:
            self._send_you(p)

    def _broadcast(self, peers, msg):
        for p in peers:
            p.conn.send(msg)

    def _send_state(self, ready, now):  # noqa: ARG002
        w = self.w
        entries, gone = self.cons.diff(w)
        players = [netsync.player_public(w.local_player, 0, self.name)]
        players += [netsync.player_public(p.player, p.pid, p.name) for p in self.peers.values() if p.ready]
        craters = []
        for k, pl in enumerate(w.planets):
            n0 = self.craters.get(k, 0)
            if len(pl.craters) != n0:
                start = min(n0, len(pl.craters))
                craters.append({"p": k, "from": start, "list": [list(map(float, c)) for c in pl.craters[start:]]})
                self.craters[k] = len(pl.craters)
        msg = {"t": "s", "time": w.time, "dt": w.time - self._last_state, "c": entries, "gone": gone,
               "pl": players}
        if craters:
            msg["craters"] = craters
        self._last_state = w.time
        self._broadcast(ready, msg)

    def _send_meta(self, ready):
        m = self.meta.diff(self.w)
        if m:
            self._broadcast(ready, {"t": "m", "c": m})

    def _send_addons(self, ready):
        out = {}
        for a in self.w.addons:
            try:
                st = a.net_state(self.w)
            except Exception:  # noqa: BLE001
                L.exception("MOD '%s' の状態を作れません", a.name)
                continue
            if st is None:
                continue
            st = netsync.jsonable(st)
            key = netsync.dumps(st)
            if self.addon_sent.get(a.name) != key:
                self.addon_sent[a.name] = key
                out[a.name] = st
        if out:
            self._broadcast(ready, {"t": "a", "s": out})

    def _send_you(self, peer):
        p = peer.player
        you = netsync.jsonable({"inventory": p.inventory, "seated": [p.seated[0], list(p.seated[1])] if p.seated else None,
                                "starter_given": p.starter_given, "credits_given": p.credits_given,
                                "out_of_fuel": p.out_of_fuel})
        key = netsync.dumps(you)
        msgs = p.msgs[:]
        p.msgs.clear()
        if key != peer.you_key or msgs:
            peer.you_key = key
            peer.conn.send({"t": "you", **you, "msgs": [m for _, m in msgs]})

    # --- 受け取り ----------------------------------------------------------
    def _process(self):
        for _ in range(2000):
            try:
                conn, msg = self.inbox.get_nowait()
            except queue.Empty:
                break
            try:
                self._handle(conn, msg)
            except Exception:  # noqa: BLE001
                L.exception("参加者からのメッセージ %s の処理でエラー", msg.get("t"))

    def _handle(self, conn, m):
        t = m.get("t")
        if t == "_open":
            self.peers[conn] = Peer(conn, m.get("addr"))
            L.info("つながりを受け付けました: %s", m.get("addr"))
            return
        peer = self.peers.get(conn)
        if peer is None:
            return
        if t == "_closed" or t == "bye":
            if peer.player is None:
                L.info("参加の途中で切れました: %s（%s）%s", peer.addr, peer.name or "名前の前", m.get("why") or "")
            self._leave(peer)
            return
        if t == "hello":
            self._hello(peer, m)
        elif t == "get_files":
            self._send_files(peer, m.get("files", []))
        elif t == "ready":
            self._ready(peer, m)
        elif peer.ready:
            self._game_msg(peer, m)

    def _hello(self, peer, m):
        L.info("参加の申し込み: %s（%s）", m.get("name"), peer.addr)
        if m.get("proto") != net.PROTO:
            peer.conn.send({"t": "error", "text": "ゲームの版が違うので参加できません（ホストと同じ版にしてください）"})
            return
        name = (m.get("name") or "player").strip()[:24] or "player"
        used = {self.name} | {p.name for p in self.peers.values() if p.name}
        base, k = name, 2
        while name in used:
            name = f"{base}{k}"
            k += 1
        peer.name = name
        peer.pid = self.next_pid
        self.next_pid += 1
        if self._manifest is None:
            self._manifest = mod_manifest(self.w.registry.loaded_mods)
        peer.conn.send({"t": "welcome", "pid": peer.pid, "name": name, "world": self.w.name,
                        "mods": list(self.w.mods), "loaded": list(self.w.registry.loaded_mods),
                        "manifest": self._manifest, "host": self.name})

    def _send_files(self, peer, files):
        for mid, rel in files:
            root = (paths.MODS / mid).resolve()
            f = (root / rel).resolve()
            if root not in f.parents or not f.is_file():
                peer.conn.send({"t": "file", "mod": mid, "rel": rel, "data": b"", "last": True, "missing": True})
                continue
            with open(f, "rb") as fh:
                while True:
                    data = fh.read(CHUNK)
                    last = fh.tell() >= f.stat().st_size
                    peer.conn.send({"t": "file", "mod": mid, "rel": rel, "data": data, "last": last})
                    if last:
                        break
        peer.conn.send({"t": "files_done"})

    def _ready(self, peer, m):
        w = self.w
        old = self.known.get(peer.name)
        lp = w.local_player
        if old:
            p = w.make_player(old.get("def") or m.get("def"))
            p.load_dict(old)
        else:
            fwd = np.asarray(getattr(lp, "look_fwd", np.array([0, 0, 1.0])), float)
            side = np.cross(lp.up, fwd)
            p = w.make_player(m.get("def"), lp.pos + side * 1.5 + lp.up * 0.2)
            p.up = lp.up.copy()
        p.name, p.pid = peer.name, peer.pid
        p.seated = None
        peer.player = p
        w.remote[peer.pid] = p
        with w.as_player(p):
            for a in w.addons:
                try:
                    a.on_player_join(w, p)
                except Exception:  # noqa: BLE001
                    L.exception("MOD '%s' の参加の処理でエラー", a.name)
        snap = w.snapshot()
        snap["player.json"] = p.to_dict()
        scripts = self.files.all()
        peer.conn.send({"t": "world", "files": netsync.jsonable(snap), "scripts": scripts,
                        "craters_from": {k: len(pl.craters) for k, pl in enumerate(w.planets)}})
        peer.ready = True
        w.message(f"{peer.name} が参加しました")
        L.info("参加: %s（%s）", peer.name, peer.addr)
        self._broadcast([x for x in self.peers.values() if x.ready and x is not peer],
                        {"t": "note", "text": f"{peer.name} が参加しました"})

    def _leave(self, peer):
        self.peers.pop(peer.conn, None)
        peer.conn.close()
        if peer.player is not None:
            self.known[peer.name] = peer.player.to_dict()
            self.w.remote.pop(peer.pid, None)
            self.w.message(f"{peer.name} が退出しました")
            L.info("退出: %s", peer.name)
            self._broadcast([x for x in self.peers.values() if x.ready],
                            {"t": "note", "text": f"{peer.name} が退出しました"})

    # --- 参加者の操作 --------------------------------------------------------
    def _game_msg(self, peer, m):
        t = m["t"]
        w = self.w
        p = peer.player
        if t == "me":
            p.pos = np.asarray(m["pos"], float)
            p.vel = np.asarray(m["vel"], float)
            p.up = np.asarray(m["up"], float)
            p.ground_vel = np.asarray(m.get("gv", [0, 0, 0]), float)
            p.on_ground = bool(m.get("og"))
            p.jetpack = bool(m.get("jp"))
            p.flying = bool(m.get("fl"))
            p.thrust = np.asarray(m.get("thrust", [0, 0, 0]), float)
            p.in_water = float(m.get("wt", 0.0))
            p.input_keys = set(m.get("keys") or [])
            p.look_fwd = np.asarray(m.get("fwd", [0, 0, 1]), float)
            p.pitch = float(m.get("pitch", 0.0))
            st = m.get("stand")
            p.stand = (st[0], tuple(st[1])) if st else None
            if m.get("def") and m["def"] != p.defn.get("id"):
                p.defn = w.player_def(m["def"])
            if "hotbar" in m:
                hb = m["hotbar"]
                p.hotbar = [tuple(x) if x else None for x in hb] if hb else None
            if p.seated and not w.constructs.get(p.seated[0]):
                p.seated = None
        elif t == "act":
            with w.as_player(p):
                self._act(p, m)
        elif t == "dev":
            netsync.apply_dev_patch(w, m["id"], m.get("patches", []))
        elif t == "files":
            for name, data in (m.get("files") or {}).items():
                if self.files.put(name, data):
                    w.runner.invalidate(name)
            # ほかの参加者へも（送った人には送り返さない）
            others = [x for x in self.peers.values() if x.ready and x is not peer]
            self._broadcast(others, {"t": "files", "files": m.get("files") or {}})
        elif t == "call":
            n0, ok = len(p.msgs), True
            with w.as_player(p):
                try:
                    w.run_addon_call(m["addon"], m["path"], m.get("args", []))
                except Exception as e:  # noqa: BLE001
                    L.exception("MOD の操作 %s.%s でエラー", m.get("addon"), m.get("path"))
                    w.message(f"⚠ エラー: {e}")
                    ok = False
            if ok:
                del p.msgs[n0:]     # 結果のメッセージは参加者の PC が先に出している（二重に出さない）
        elif t == "chat":
            w.message(f"{peer.name}: {m.get('text', '')}")

    def _act(self, p, m):
        w = self.w
        a = m["a"]
        eye = np.asarray(m.get("eye", p.eye()), float)
        look = np.asarray(m.get("look", [0, 0, 1]), float)

        def hit():
            return w.raycast(eye, look, float(p.defn.get("reach", 8.0)))
        if a == "remove":
            h = hit()
            if h and h["kind"] == "voxel":
                w.remove(h)
        elif a == "dig":
            h = hit()
            if h and h["kind"] in ("terrain", "water"):
                w.dig(h)
        elif a == "place":
            w.place(hit() if not m.get("air") else None, tuple(m["item"]), int(m.get("dir", 2)), look)
        elif a == "interact":
            tg = m.get("target")
            if tg:
                c = w.constructs.get(tg["id"])
                i = c.index.get(tuple(tg["cell"])) if c else None
                if i is not None:
                    w.interact({"kind": "voxel", "construct": c, "index": i, "t": 0.0, "face": 2,
                                "point": c.pos[8 * i:8 * i + 8].mean(0)})
                return
            w.interact(None if m.get("none") else hit())
        elif a == "key":
            w.key_pressed(m["key"])
        elif a == "import":
            w.place_import(netsync.unpack_import(m["data"]), look_dir=look)
        elif a == "actuator":
            c = w.constructs.get(m["id"])
            i = c.index.get(tuple(m["cell"])) if c else None
            if i is not None:
                from . import actuator
                w.message(actuator.execute_preset(w, c, i))

    # --- 終わり ------------------------------------------------------------
    def close(self):
        for p in list(self.peers.values()):
            p.conn.send({"t": "bye", "text": "ホストがワールドを閉じました"})
        time.sleep(0.05)
        for p in list(self.peers.values()):
            p.conn.close()
        self.peers.clear()
        self.w.remote.clear()
        self.listener.close()
        self.announcer.close()
        self.w.net = None
        L.info("LAN への公開を終了")


def _default_name():
    import getpass
    import socket
    try:
        return getpass.getuser()[:24]
    except Exception:  # noqa: BLE001
        return socket.gethostname()[:24]
