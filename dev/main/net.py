"""LAN マルチプレイの通信（送り方・受け方・LAN の中のワールドを見つける）。

- 1つのメッセージ = dict（JSON で書けるもの）。numpy の配列・bytes はそのまま入れてよい（JSON の外に生のまま載せる）
- TCP で、[長さ 4 バイト][フラグ 1 バイト][JSON の長さ 4 バイト][JSON][生のデータ] の形で送る（大きいものは zlib で圧縮）
- 送る・受けるはそれぞれ専用のスレッドで行い、ゲーム側（メインスレッド）は毎フレーム inbox から取り出すだけ
- LAN の中のワールドは UDP のブロードキャストで知らせる（ホストが1秒ごと）

悪意のある相手は想定しない（LAN の中の知り合いどうしで遊ぶ前提。MOD のプログラムもホストから受け取って動かす）。
"""
import json
import queue
import socket
import struct
import threading
import time
import zlib

import numpy as np

from . import log

L = log.get("net")

PROTO = 1
PORT = 47620            # TCP（ゲーム）
DISCOVERY_PORT = 47621  # UDP（LAN のワールドの知らせ）
MAGIC = "alpedia-lan"


# ---------------------------------------------------------------------------
# メッセージ ⇔ バイト列
# ---------------------------------------------------------------------------
def encode(msg):
    blobs = []

    def conv(x):
        if isinstance(x, np.ndarray):
            a = np.ascontiguousarray(x)
            blobs.append(a.tobytes())
            return {"__nd__": [len(blobs) - 1, a.dtype.str, list(a.shape)]}
        if isinstance(x, (bytes, bytearray, memoryview)):
            blobs.append(bytes(x))
            return {"__b__": len(blobs) - 1}
        if isinstance(x, dict):
            return {str(k): conv(v) for k, v in x.items()}
        if isinstance(x, (list, tuple)):
            return [conv(v) for v in x]
        if isinstance(x, (np.integer,)):
            return int(x)
        if isinstance(x, (np.floating,)):
            return float(x)
        if isinstance(x, np.bool_):
            return bool(x)
        return x
    js = json.dumps(conv(msg), ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    sizes = [len(b) for b in blobs]
    head = json.dumps(sizes).encode("ascii")
    body = struct.pack("<II", len(js), len(head)) + js + head + b"".join(blobs)
    flag = 0
    if len(body) > 2048:
        z = zlib.compress(body, 1)
        if len(z) < len(body) * 0.9:
            body, flag = z, 1
    return struct.pack("<IB", len(body) + 1, flag) + body


def decode(flag, body):
    if flag & 1:
        body = zlib.decompress(body)
    jl, hl = struct.unpack("<II", body[:8])
    js = body[8:8 + jl]
    sizes = json.loads(body[8 + jl:8 + jl + hl].decode("ascii"))
    off = 8 + jl + hl
    blobs = []
    for s in sizes:
        blobs.append(body[off:off + s])
        off += s

    def conv(x):
        if isinstance(x, dict):
            if "__nd__" in x and len(x) == 1:
                i, dt, shape = x["__nd__"]
                return np.frombuffer(blobs[i], dtype=np.dtype(dt)).reshape(shape).copy()
            if "__b__" in x and len(x) == 1:
                return blobs[x["__b__"]]
            return {k: conv(v) for k, v in x.items()}
        if isinstance(x, list):
            return [conv(v) for v in x]
        return x
    return conv(json.loads(js.decode("utf-8")))


# ---------------------------------------------------------------------------
# 1本のつながり
# ---------------------------------------------------------------------------
class Conn:
    """TCP のつながり1本。send は誰からでも呼べる（送るスレッドが順に送る）。受けたものは inbox へ。"""

    def __init__(self, sock, inbox=None, tag=None, start=True):
        self.sock = sock
        self.tag = tag
        self.inbox = inbox if inbox is not None else queue.Queue()
        self.outbox = queue.Queue()
        self.alive = True
        self.error = None
        self.sent_bytes = 0
        self.recv_bytes = 0
        try:
            sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_KEEPALIVE, 1)
        except OSError:
            pass
        self._rt = threading.Thread(target=self._reader, daemon=True, name="net-recv")
        self._wt = threading.Thread(target=self._writer, daemon=True, name="net-send")
        if start:
            self.start()

    def start(self):
        self._rt.start()
        self._wt.start()

    def send(self, msg):
        if self.alive:
            self.outbox.put(msg)

    def pending(self):
        return self.outbox.qsize()

    def close(self, why=None):
        if not self.alive:
            return
        self.alive = False
        self.error = self.error or why
        self.outbox.put(None)
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        try:
            self.sock.close()
        except OSError:
            pass

    def _recv_exact(self, n):
        buf = bytearray()
        while len(buf) < n:
            chunk = self.sock.recv(min(n - len(buf), 1 << 20))
            if not chunk:
                raise ConnectionError("相手が切断しました")
            buf += chunk
        return bytes(buf)

    def _reader(self):
        try:
            while self.alive:
                ln, flag = struct.unpack("<IB", self._recv_exact(5))
                body = self._recv_exact(ln - 1)
                self.recv_bytes += ln + 4
                msg = decode(flag, body)
                self.inbox.put((self, msg))
        except Exception as e:  # noqa: BLE001
            if self.alive:
                L.info("受信の終了（%s）: %s", self.tag, e)
                self.error = self.error or str(e)
        finally:
            was = self.alive
            self.close()
            if was:
                self.inbox.put((self, {"t": "_closed", "why": self.error}))

    def _writer(self):
        try:
            while True:
                msg = self.outbox.get()
                if msg is None:
                    break
                data = encode(msg)
                self.sock.sendall(data)
                self.sent_bytes += len(data)
        except Exception as e:  # noqa: BLE001
            if self.alive:
                L.info("送信の終了（%s）: %s", self.tag, e)
                self.error = self.error or str(e)
            self.close()


def connect(host, port=PORT, timeout=5.0):
    s = socket.create_connection((host, int(port)), timeout=timeout)
    s.settimeout(None)
    return Conn(s, tag=f"{host}:{port}")


class Listener:
    """ホスト：つながりを受け付ける。新しいつながりは inbox に ("_join", conn) で入る。"""

    def __init__(self, port=PORT, inbox=None):
        self.inbox = inbox if inbox is not None else queue.Queue()
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind(("", int(port)))
        self.sock.listen(8)
        self.port = self.sock.getsockname()[1]
        self.alive = True
        threading.Thread(target=self._accept, daemon=True, name="net-accept").start()

    def _accept(self):
        while self.alive:
            try:
                s, addr = self.sock.accept()
            except OSError:
                break
            c = Conn(s, self.inbox, tag=f"{addr[0]}:{addr[1]}", start=False)
            self.inbox.put((c, {"t": "_open", "addr": addr[0]}))
            c.start()           # 受信は「つながった」の後から（先に届いた挨拶を取りこぼさないように）

    def close(self):
        self.alive = False
        try:
            self.sock.close()
        except OSError:
            pass


# ---------------------------------------------------------------------------
# LAN の中のワールドを見つける
# ---------------------------------------------------------------------------
def local_addresses():
    """この PC の LAN 側の IP アドレス（表示用）。"""
    out = []
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("10.255.255.255", 1))        # 実際には送らない（経路から自分のアドレスを知る）
        out.append(s.getsockname()[0])
        s.close()
    except OSError:
        pass
    try:
        for ip in socket.gethostbyname_ex(socket.gethostname())[2]:
            if ip not in out and not ip.startswith("127."):
                out.append(ip)
    except OSError:
        pass
    return out or ["127.0.0.1"]


class Announcer:
    """ホスト：1秒ごとに LAN へ「ここでワールドを公開中」と知らせる。"""

    def __init__(self, info_fn, port=DISCOVERY_PORT):
        self.info_fn = info_fn
        self.port = port
        self.alive = True
        threading.Thread(target=self._run, daemon=True, name="net-announce").start()

    def _run(self):
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        while self.alive:
            try:
                data = json.dumps({"magic": MAGIC, "proto": PROTO, **self.info_fn()}, ensure_ascii=False).encode()
                for target in ("255.255.255.255", "127.0.0.1"):
                    try:
                        s.sendto(data, (target, self.port))
                    except OSError:
                        pass
            except Exception:  # noqa: BLE001
                L.exception("LAN への知らせに失敗")
            time.sleep(1.0)
        s.close()

    def close(self):
        self.alive = False


class Browser:
    """参加する側：LAN の知らせを聞いて、公開中のワールドの一覧を作る。"""

    def __init__(self, port=DISCOVERY_PORT):
        self.found = {}         # (ip, port) -> (受けた時刻, 情報)
        self.alive = True
        self.error = None
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
        except (AttributeError, OSError):
            pass
        try:
            self.sock.bind(("", port))
        except OSError as e:
            self.error = str(e)
            self.alive = False
            return
        self.sock.settimeout(0.5)
        threading.Thread(target=self._run, daemon=True, name="net-browse").start()

    def _run(self):
        while self.alive:
            try:
                data, addr = self.sock.recvfrom(65536)
            except socket.timeout:
                continue
            except OSError:
                break
            try:
                info = json.loads(data.decode("utf-8"))
            except Exception:  # noqa: BLE001
                continue
            if info.get("magic") != MAGIC:
                continue
            ip = addr[0]
            if ip.startswith("127.") and any(k[0] != ip and k[1] == info.get("port") for k in self.found):
                continue
            self.found[(ip, int(info.get("port", PORT)))] = (time.time(), info)

    def worlds(self, max_age=3.5):
        now = time.time()
        return [(ip, port, info) for (ip, port), (t, info) in sorted(self.found.items()) if now - t < max_age]

    def close(self):
        self.alive = False
        try:
            self.sock.close()
        except OSError:
            pass
