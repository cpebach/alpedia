"""形の計算（コア）：向き付きの箱の符号付き距離とレイの交差。

拡張（街など）が当たり判定用の箱を出すとき、コアの物理はこの関数で箱との接触を解く。
箱は 中心 C (B,3)・向き A (B,3,3)（行が箱の軸）・半分の大きさ E (B,3) で表す。
"""
import numpy as np


def box_sdf(P, C, A, E):
    """P (N,3) と箱 (B) → 一番近い箱までの距離 d (N,)・外向きの向き n (N,3)・箱の番号 k (N,)"""
    q = np.einsum("bij,nbj->nbi", A, P[:, None, :] - C[None])
    aq = np.abs(q) - E[None]
    out = np.linalg.norm(np.maximum(aq, 0.0), axis=-1)
    d_all = out + np.minimum(aq.max(-1), 0.0)
    k = np.argmin(d_all, axis=1)
    rows = np.arange(len(P))
    d = d_all[rows, k]
    qk, aqk = q[rows, k], aq[rows, k]
    g = np.maximum(aqk, 0.0) * np.sign(qk)
    inside = d <= 0
    if inside.any():
        ax = np.argmax(aqk[inside], axis=1)
        gi = np.zeros((int(inside.sum()), 3))
        sg = np.sign(qk[inside][np.arange(len(ax)), ax])
        gi[np.arange(len(ax)), ax] = np.where(sg == 0, 1.0, sg)     # ちょうど真ん中なら + 側へ
        g[inside] = gi
    g /= np.maximum(np.linalg.norm(g, axis=1, keepdims=True), 1e-12)
    n = np.einsum("ni,nij->nj", g, A[k])
    return d, n, k


def ray_boxes(o, dvec, C, A, E, max_t):
    """レイと箱の交差：(t, 箱の番号, 法線) または None"""
    lo = np.einsum("bij,bj->bi", A, o[None] - C)
    ld = np.einsum("bij,j->bi", A, dvec)
    with np.errstate(divide="ignore", invalid="ignore"):
        inv = 1.0 / np.where(np.abs(ld) < 1e-12, 1e-12, ld)
        t1 = (-E - lo) * inv
        t2 = (E - lo) * inv
    tmin, tmax = np.minimum(t1, t2), np.maximum(t1, t2)
    tn, tf = tmin.max(1), tmax.min(1)
    ok = (tn <= tf) & (tf > 0) & (tn < max_t)
    if not ok.any():
        return None
    idx = np.nonzero(ok)[0]
    k = idx[np.argmin(np.maximum(tn[idx], 0))]
    t = max(float(tn[k]), 0.0)
    ax = int(np.argmax(tmin[k]))
    nl = np.zeros(3)
    nl[ax] = -np.sign(ld[k, ax])
    return t, int(k), nl @ A[k]
