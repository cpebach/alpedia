"""マルチプレイ（LAN）の参加者側。本体は Rust（alpedia_core の Joiner・Client）。

参加の流れ（Joiner、別スレッド）：
    つなぐ → 名前を送る → MOD のファイルの一覧を受け取り、足りない物だけ受け取る（自分の mods/ に同じ物があれば写す）
    → ワールドの中身を受け取って、自分の PC の saves/.net/world に書く
その後、メインスレッドでそのワールドを普通に読み込み（MOD もワールドを始めるときに一括）、Client をつないでゲームを始める。

遊んでいる間（Client）：
    自分の体（歩く・跳ぶ）だけ自分の PC で動かしてホストへ知らせる。構築物・ほかの人・MOD の状態はホストから受け取る。
    置く・壊す・掘る・E などの操作はホストへ送る（ホストのワールドで行い、結果が返ってくる）。
    F9 の出力は自分の PC（saves/.net/exports）に書く。ほかのワールドの F8 からも読み込める。
"""
from . import log, native

_c = native.require()
L = log.get("netclient")

ME_HZ = 20.0
EDIT_HZ = 2.0
FILE_HZ = 1.0

net_dir = _c.nc_net_dir
mods_cache = _c.nc_mods_cache
_sha1 = _c.nc_sha1


class JoinError(Exception):
    pass


class Joiner(_c.Joiner):
    """参加の準備（別スレッド）。status に今していること、done / error / result（中身は Rust）。"""

    def __init__(self, host, port, name, skin=None):
        self._begin()


class Client(_c.Client):
    """遊んでいる間の参加者側。ゲーム画面が毎フレーム frame(game, dt) と、物理の刻みごとに step(...) を呼ぶ（中身は Rust）。"""

    def __init__(self, info, world, cfg):
        self._begin()
