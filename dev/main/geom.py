"""形の計算（コア）：向き付きの箱の符号付き距離とレイの交差（Rust）。

拡張（街など）が当たり判定用の箱を出すとき、コアの物理はこの関数で箱との接触を解く。
箱は 中心 C (B,3)・向き A (B,3,3)（行が箱の軸）・半分の大きさ E (B,3) で表す。
"""
import numpy as np

from . import native


def _f(a, shape):
    return np.ascontiguousarray(a, dtype=np.float64).reshape(shape)


def box_sdf(P, C, A, E):
    """P (N,3) と箱 (B) → 一番近い箱までの距離 d (N,)・外向きの向き n (N,3)・箱の番号 k (N,)"""
    return native.require().box_sdf(_f(P, (-1, 3)), _f(C, (-1, 3)), _f(A, (-1, 3, 3)), _f(E, (-1, 3)))


def ray_boxes(o, dvec, C, A, E, max_t):
    """レイと箱の交差：(t, 箱の番号, 法線) または None"""
    if not len(C):
        return None
    r = native.require().ray_boxes([float(x) for x in o], [float(x) for x in dvec], _f(C, (-1, 3)), _f(A, (-1, 3, 3)),
                                   _f(E, (-1, 3)), float(max_t))
    return None if r is None else (r[0], int(r[1]), np.asarray(r[2]))
