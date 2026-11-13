"""マルチプレイ（LAN）のホスト側。本体は Rust（alpedia_core.Host）。

ホストの PC がワールド（セーブ・MOD）を持ち、物理・プログラム・MOD の決まりを全部動かす。
参加者の PC は結果を受け取って描き、自分の体（歩く・跳ぶ）だけを自分で動かして知らせてくる。
参加者の操作（置く・壊す・掘る・E・機器の設定・プログラム）はホストに送られ、ホストのワールドで行う。

ゲーム画面は、毎フレーム物理を進めたあとに tick(dt) を呼ぶ。
"""
from . import log, native

_c = native.require()
L = log.get("netserver")

STATE_HZ = 20.0         # 構築物・プレイヤーの位置
META_HZ = 8.0           # 機器・資源など
ADDON_HZ = 10.0         # MOD の状態
FILE_HZ = 1.0           # プログラムのファイル
CHUNK = 512 * 1024

mod_manifest = _c.ns_mod_manifest     # MOD のファイルの一覧 {MOD名: [[相対パス, 大きさ, sha1], ...]}
_sha1 = _c.ns_sha1_cached
_default_name = _c.ns_default_name
Peer = _c.Peer


class Host(_c.Host):
    """作るときに LAN への知らせを始めてワールドにつなぐ（中身は Rust。ここは作ったあとの準備を呼ぶだけ）。"""

    def __init__(self, world, cfg, port=None):
        self._start()
