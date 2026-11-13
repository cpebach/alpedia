"""物理演算の重い部分の入り口：Rust の物理コア（alpedia_core、native.py）を呼ぶ。

- 当たり判定の芯の計算、皮（描画の頂点）・法線。1サブステップ・1ティックの物理は construct / world から Rust の
  Body・ArtTopo・step_tick を直接呼ぶ。
- Rust は GIL を離して計算するので、別スレッドから呼ぶと複数の CPU コアで同時に動く（world 側で構築物ごとに並列化）。
- 大きな構築物（PARALLEL_MIN_VOXELS 以上）はボクセルごとに複数コアへ分ける。
"""
from . import native

PARALLEL_MIN_VOXELS = 256     # これ以上のボクセル数ならボクセルごとに複数コアで計算


def _core():
    return native.require()


def contact_cores(pos, ctype, cidx):
    return _core().contact_cores(pos, ctype, cidx)


def skin(pos, vidx, w):
    return _core().skin(pos, vidx, w)


def vertex_normals(v, faces):
    return _core().vertex_normals(v, faces)
