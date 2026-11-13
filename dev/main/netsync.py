"""マルチプレイで、ホストと参加者が同じ形で使う「状態の送り方・受け方」。本体は Rust（alpedia_core の netsync_*）。

構築物：
    形（ボクセル・結合）が変わったら丸ごと（to_dict）、剛体として動いている間は (R, x, v, w)、
    ブロックごとに動いている間は頂点の位置、止まったら最後の位置を1回だけ送る（止まっている間は何も送らない）
機器・資源・電力などの中身（メタ）：変わったときだけ
プログラムのファイル（scripts/）：変わったファイルだけ（どちらの PC で書き換えても相手に届く）
"""
import numpy as np

from . import native

_c = native.require()
F32 = np.float32
PRIVATE = ("memory",)           # 機器の状態のうち送らないもの（"_" で始まるものも送らない）
EXTRAPOLATE = 0.08              # 更新が来ない間、速度のまま進めてよい時間（これより長く来なければ止めて待つ）
TEXT_SUFFIXES = (".py", ".json", ".txt", ".md")

# 構築物
ConsTracker = _c.ConsTracker                # ホスト：前に送った状態を覚えておき、変わった分だけの送り物を作る
apply_construct = _c.netsync_apply_construct  # 参加者：構築物の更新を1つ当てる（作り直したら新しい構築物を返す）
_sleep = _c.netsync_sleep
advance = _c.netsync_advance                # 参加者：次の更新が届くまで、構築物を速度のまま進める
_rot = _c.netsync_rot

# 機器・資源など（メタ）
dev_public = _c.netsync_dev_public          # 機器の、相手に見せる部分（名前と状態）
meta_of = _c.netsync_meta_of
dumps = _c.netsync_dumps
_default = _c.netsync_default
jsonable = _c.netsync_jsonable
MetaTracker = _c.MetaTracker
apply_meta = _c.netsync_apply_meta
dev_patch = _c.netsync_dev_patch            # 参加者：機器の画面で変えた分
dev_base = _c.netsync_dev_base
apply_dev_patch = _c.netsync_apply_dev_patch  # ホスト：参加者が機器の画面で変えた分を当てる

# プログラムのファイル
FileSync = _c.FileSync                      # フォルダの中のファイルの変化を見張る（相手から受け取ったものは送り返さない）
_hash = _c.netsync_hash

# OBJ 読み込みの結果（参加者 → ホスト）
pack_import = _c.netsync_pack_import
unpack_import = _c.netsync_unpack_import

# プレイヤー
player_public = _c.netsync_player_public    # ほかの人に見せるプレイヤーの状態（描画用）
apply_player_public = _c.netsync_apply_player_public
