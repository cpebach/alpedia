"""形の計算（コア）：向き付きの箱の符号付き距離とレイの交差（Rust）。

拡張（街など）が当たり判定用の箱を出すとき、コアの物理はこの関数で箱との接触を解く。
箱は 中心 C (B,3)・向き A (B,3,3)（行が箱の軸）・半分の大きさ E (B,3) で表す。
"""
from . import native

_c = native.require()
box_sdf = _c.geom_box_sdf        # P (N,3) と箱 (B) → 一番近い箱までの距離 d (N,)・外向きの向き n (N,3)・箱の番号 k (N,)
ray_boxes = _c.geom_ray_boxes    # レイと箱の交差：(t, 箱の番号, 法線) または None
