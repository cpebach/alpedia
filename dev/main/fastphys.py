"""物理演算の重い部分の入り口：Rust の物理コア（alpedia_core、native.py）を呼ぶ。

- 形合わせ（ボクセルごとの最適な回転）、結合、当たり判定の芯の計算、皮（描画の頂点）・法線、関節つき剛体の解き方。
- Rust は GIL を離して計算するので、別スレッドから呼ぶと複数の CPU コアで同時に動く（world 側で構築物ごとに並列化）。
- 大きな構築物（PARALLEL_MIN_VOXELS 以上）はボクセルごとに複数コアへ分ける。
"""
import numpy as np

from . import native

PARALLEL_MIN_VOXELS = 256     # これ以上のボクセル数ならボクセルごとに複数コアで計算


def _core():
    return native.require()


def shape_match(pos, rest, act_scale, stiff, vox, lastR, quat):
    _core().shape_match(pos, rest, act_scale, stiff, vox, lastR, quat, False)


def shape_match_par(pos, rest, act_scale, stiff, vox, lastR, quat):
    _core().shape_match(pos, rest, act_scale, stiff, vox, lastR, quat, True)


def solve_joints(pos, ja, jb, jr, jk, inv_mass, jcount):
    _core().solve_joints(pos, ja, jb, jr, jk, inv_mass, jcount)


def contact_cores(pos, ctype, cidx):
    return _core().contact_cores(pos, ctype, cidx)


def skin(pos, vidx, w):
    return _core().skin(pos, vidx, w)


def vertex_normals(v, faces):
    return _core().vertex_normals(v, faces)


def art_solve(X, R, invM, IinvB, ra, rb, LA, LB, LC, ang, cap, cg, cl, cn, cd, cp, cprev, cvs, cmu, h, iters, lamN,
              HK=None):
    """HK：関節ごとのヒンジの倒れる側の辺（無ければ全部回転）。"""
    nr = ra.shape[0]
    if HK is None or HK.shape[0] != nr:
        HK = np.full((nr, 2), -1, np.int64)
    _core().art_solve(X, R, invM, IinvB, ra, rb, LA, LB, LC, ang, cap, cg, cl, cn, cd, cp, cprev, cvs, cmu, float(h),
                      int(iters), lamN, np.ascontiguousarray(HK, dtype=np.int64))


def art_contact_vel(V, W, X, R, invM, IinvB, cg, cl, cn, lamN, iters):
    _core().art_contact_vel(V, W, R, invM, IinvB, cg, cl, cn, lamN, int(iters))


def art_write(pos, X, R, gv, vg, vl):
    _core().art_write(pos, X, R, gv, vg, vl)
