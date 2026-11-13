"""構築物（プレイヤーの建築物・乗り物・デブリ）。本体は Rust（alpedia_core.Construct）。

各ボクセルは自分専用の8頂点を持つ「ポリゴンのボクセル」。
- ボクセル内部: 形状マッチングで元の形に戻ろうとする（素材の stiffness）
- 隣接ボクセル間: 同じ位置の頂点同士を結合（素材の joint_stiffness）。
  頂点間の距離が break_distance を超えると結合が離れ、構築物がちぎれる。
- 塑性: 変形が yield_strain を超えると plasticity に応じて元の形自体が変わる。
- 再結合: rebond な素材（液体・粘土）は近づいた頂点同士が新たに結合する。
ここは MOD・ほかのモジュールが読む定数と、Rust の入り口だけ。
"""
from itertools import combinations

import numpy as np

from . import native

_c = native.require()

CORNER_BITS = np.array([[c & 1, (c >> 1) & 1, (c >> 2) & 1] for c in range(8)], dtype=np.float64)
REST_UNIT = CORNER_BITS - 0.5
DIRS = np.array([[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]], dtype=np.int64)
DIR_TUPLES = [tuple(int(x) for x in d) for d in DIRS]
DIR_NAMES = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"]


def _face_quad(d):
    axis, positive = d // 2, d % 2 == 0
    u, v = (axis + 1) % 3, (axis + 2) % 3
    quad = []
    for bu, bv in ((0, 0), (1, 0), (1, 1), (0, 1)):
        bits = [0, 0, 0]
        bits[axis] = 1 if positive else 0
        bits[u], bits[v] = bu, bv
        quad.append(bits[0] + 2 * bits[1] + 4 * bits[2])
    return quad if positive else quad[::-1]


FACE_QUADS = np.array([_face_quad(d) for d in range(6)], dtype=np.int64)
# リンク a→b（b = a + DIRS[d]）で結合する頂点ペア
JOINT_PAIRS = np.array([[(c, c ^ (1 << (d // 2))) for c in FACE_QUADS[d]] for d in range(6)], dtype=np.int64)  # (6,4,2)
EDGE_PAIRS = np.array(list(combinations(range(8), 2)))
CONTACT_SOFT = 1.0

dir_index = _c.construct_dir_index
_clean_dev = _c.construct_clean_dev
link_key = _c.construct_link_key
_native_planets = _c.construct_native_planets
Construct = _c.Construct


def native_core():
    return native.core
