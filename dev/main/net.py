"""LAN マルチプレイの通信（送り方・受け方・LAN の中のワールドを見つける）。中身は Rust（alpedia_core の net.rs）。

- 1つのメッセージ = dict（JSON で書けるもの）。numpy の配列・bytes はそのまま入れてよい（JSON の外に生のまま載せる）
- TCP で、[長さ 4 バイト][フラグ 1 バイト][JSON の長さ 4 バイト][頭の長さ 4 バイト][JSON][頭][生のデータ] の形で送る
  （大きいものは zlib で圧縮）
- 送る・受けるはそれぞれ Rust のスレッドで行い、ゲーム側（メインスレッド）は毎フレーム inbox から取り出すだけ
- LAN の中のワールドは UDP のブロードキャストで知らせる（ホストが1秒ごと）

悪意のある相手は想定しない（LAN の中の知り合いどうしで遊ぶ前提。MOD のプログラムもホストから受け取って動かす）。
"""
from . import native

PROTO = 1
PORT = 47620            # TCP（ゲーム）
DISCOVERY_PORT = 47621  # UDP（LAN のワールドの知らせ）
MAGIC = "alpedia-lan"


def _core():
    return native.require()


def encode(msg):
    """メッセージ → 送るバイト列（先頭の長さ・フラグ込み）"""
    return _core().net_encode(msg)


def decode(flag, body):
    """フラグと本体 → メッセージ"""
    return _core().net_decode(int(flag), bytes(body))


def Inbox():
    """受けたもの (つながり, メッセージ) の列（queue.Queue と同じ get / get_nowait。空なら queue.Empty）。"""
    return _core().Inbox()


def connect(host, port=PORT, timeout=5.0, inbox=None):
    """host:port につなぐ（つながらないときは ConnectionRefusedError など）。"""
    return _core().net_connect(str(host), int(port), float(timeout), inbox)


def Listener(port=PORT, inbox=None):
    """ホスト：つながりを受け付ける。新しいつながりは inbox に (conn, {"t": "_open", "addr": ...}) で入る。"""
    return _core().Listener(int(port), inbox if inbox is not None else Inbox())


def local_addresses():
    """この PC の LAN 側の IP アドレス（表示用）。"""
    return list(_core().net_local_addresses())


def Announcer(info_fn, port=DISCOVERY_PORT):
    """ホスト：1秒ごとに LAN へ「ここでワールドを公開中」と知らせる。"""
    return _core().Announcer(info_fn, int(port))


def Browser(port=DISCOVERY_PORT):
    """参加する側：LAN の知らせを聞いて、公開中のワールドの一覧を作る（.worlds()、.error）。"""
    return _core().Browser(int(port))
