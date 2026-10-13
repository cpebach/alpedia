"""物理演算の重い部分を numba で機械語に変換して高速化する（numba が無ければ使わない）。

- 形合わせ（ボクセル・チャンクごとの最適な回転）、結合、当たり判定の芯の計算、構築物どうしの押し出しを
  ループのまま書き、numba でコンパイルする。
- nogil=True なので、別スレッドから呼ぶと GIL を離して複数の CPU コアで同時に動く（world 側で構築物ごとに並列化）。
- 大きな構築物は parallel=True 版でボクセルごとに複数コアへ分ける。
- 初回はコンパイルに数十秒かかることがある（cache=True で __pycache__ に保存し、2回目からは速い）。
"""
import numpy as np

import os

try:
    if os.environ.get("ALPEDIA_NO_NUMBA"):
        raise ImportError("disabled")
    from numba import njit, prange
    AVAILABLE = True
except Exception:  # noqa: BLE001
    AVAILABLE = False

    def njit(*a, **k):
        def deco(f):
            return f
        return deco
    prange = range

_HAS_NUMBA = AVAILABLE
PARALLEL_MIN_VOXELS = 256     # これ以上のボクセル数ならボクセルごとに複数コアで計算


# ---------------------------------------------------------------------------
# 3x3 の最適な回転（極分解）
# ---------------------------------------------------------------------------
@njit(cache=True, nogil=True, fastmath=True)
def _polar(A):
    U, S, Vt = np.linalg.svd(A)
    R = U @ Vt
    if np.linalg.det(R) < 0:
        for r in range(3):
            U[r, 2] = -U[r, 2]
        R = U @ Vt
    return R


@njit(cache=True, nogil=True, fastmath=True)
def _rotation(A, q):
    """A に最も合う回転（Müller らの反復法。前回の回転 q から始めるので数回で収束）。q を更新して行列を返す。"""
    R = np.empty((3, 3))
    for it in range(40):
        w0, x, y, z = q[0], q[1], q[2], q[3]
        R[0, 0] = 1 - 2 * (y * y + z * z); R[0, 1] = 2 * (x * y - w0 * z); R[0, 2] = 2 * (x * z + w0 * y)
        R[1, 0] = 2 * (x * y + w0 * z); R[1, 1] = 1 - 2 * (x * x + z * z); R[1, 2] = 2 * (y * z - w0 * x)
        R[2, 0] = 2 * (x * z - w0 * y); R[2, 1] = 2 * (y * z + w0 * x); R[2, 2] = 1 - 2 * (x * x + y * y)
        ox = oy = oz = 0.0
        dd = 0.0
        for c in range(3):
            rx, ry, rz = R[0, c], R[1, c], R[2, c]
            ax, ay, az = A[0, c], A[1, c], A[2, c]
            ox += ry * az - rz * ay
            oy += rz * ax - rx * az
            oz += rx * ay - ry * ax
            dd += rx * ax + ry * ay + rz * az
        inv = 1.0 / (abs(dd) + 1e-9)
        ox *= inv; oy *= inv; oz *= inv
        wn = np.sqrt(ox * ox + oy * oy + oz * oz)
        if wn < 1e-9:
            break
        h = 0.5 * wn
        sh = np.sin(h) / wn
        dw, dx, dy, dz = np.cos(h), ox * sh, oy * sh, oz * sh
        nw = dw * w0 - dx * x - dy * y - dz * z
        nx = dw * x + dx * w0 + dy * z - dz * y
        ny = dw * y - dx * z + dy * w0 + dz * x
        nz = dw * z + dx * y - dy * x + dz * w0
        nq = np.sqrt(nw * nw + nx * nx + ny * ny + nz * nz)
        q[0], q[1], q[2], q[3] = nw / nq, nx / nq, ny / nq, nz / nq
        if wn < 1e-7:
            break
    w0, x, y, z = q[0], q[1], q[2], q[3]
    R[0, 0] = 1 - 2 * (y * y + z * z); R[0, 1] = 2 * (x * y - w0 * z); R[0, 2] = 2 * (x * z + w0 * y)
    R[1, 0] = 2 * (x * y + w0 * z); R[1, 1] = 1 - 2 * (x * x + z * z); R[1, 2] = 2 * (y * z - w0 * x)
    R[2, 0] = 2 * (x * z - w0 * y); R[2, 1] = 2 * (y * z + w0 * x); R[2, 2] = 1 - 2 * (x * x + y * y)
    return R


@njit(cache=True, nogil=True, fastmath=True)
def _match_one(pos, rest, act_scale, stiff, i, lastR, quat):
    c = np.zeros(3)
    for k in range(8):
        for a in range(3):
            c[a] += pos[8 * i + k, a]
    c /= 8.0
    Q = np.empty((8, 3))
    qm = np.zeros(3)
    for k in range(8):
        for a in range(3):
            Q[k, a] = rest[i, k, a] * act_scale[i, a]
            qm[a] += Q[k, a]
    qm /= 8.0
    A = np.zeros((3, 3))
    for k in range(8):
        for a in range(3):
            Q[k, a] -= qm[a]
        for a in range(3):
            x = pos[8 * i + k, a] - c[a]
            for b in range(3):
                A[a, b] += x * Q[k, b]
    R = _rotation(A, quat[i])
    s = stiff[i]
    for k in range(8):
        for a in range(3):
            g = c[a] + R[a, 0] * Q[k, 0] + R[a, 1] * Q[k, 1] + R[a, 2] * Q[k, 2]
            pos[8 * i + k, a] += s * (g - pos[8 * i + k, a])
    for a in range(3):
        for b in range(3):
            lastR[i, a, b] = R[a, b]


@njit(cache=True, nogil=True, fastmath=True)
def shape_match(pos, rest, act_scale, stiff, vox, lastR, quat):
    for t in range(vox.shape[0]):
        _match_one(pos, rest, act_scale, stiff, vox[t], lastR, quat)


@njit(cache=True, nogil=True, fastmath=True, parallel=True)
def shape_match_par(pos, rest, act_scale, stiff, vox, lastR, quat):
    for t in prange(vox.shape[0]):
        _match_one(pos, rest, act_scale, stiff, vox[t], lastR, quat)


# ---------------------------------------------------------------------------
# チャンク（かたまり）ごとの形合わせ
# ---------------------------------------------------------------------------
@njit(cache=True, nogil=True, fastmath=True)
def shape_match_chunks(pos, cv, cc, Q, n_chunks):
    cen = np.zeros((n_chunks, 3))
    cnt = np.zeros(n_chunks)
    for t in range(cv.shape[0]):
        k = cc[t]
        cnt[k] += 1
        for a in range(3):
            cen[k, a] += pos[cv[t], a]
    for k in range(n_chunks):
        for a in range(3):
            cen[k, a] /= max(cnt[k], 1.0)
    A = np.zeros((n_chunks, 3, 3))
    for t in range(cv.shape[0]):
        k = cc[t]
        for a in range(3):
            x = pos[cv[t], a] - cen[k, a]
            for b in range(3):
                A[k, a, b] += x * Q[t, b]
    R = np.empty((n_chunks, 3, 3))
    for k in range(n_chunks):
        R[k] = _polar(A[k])
    for t in range(cv.shape[0]):
        k = cc[t]
        for a in range(3):
            pos[cv[t], a] = cen[k, a] + R[k, a, 0] * Q[t, 0] + R[k, a, 1] * Q[t, 1] + R[k, a, 2] * Q[t, 2]


# ---------------------------------------------------------------------------
# 結合（頂点どうしの距離）
# ---------------------------------------------------------------------------
@njit(cache=True, nogil=True, fastmath=True)
def solve_joints(pos, ja, jb, jr, jk, inv_mass, jcount):
    nv = pos.shape[0]
    delta = np.zeros((nv, 3))
    for j in range(ja.shape[0]):
        a = ja[j]
        b = jb[j]
        dx = pos[b, 0] - pos[a, 0]
        dy = pos[b, 1] - pos[a, 1]
        dz = pos[b, 2] - pos[a, 2]
        L = np.sqrt(dx * dx + dy * dy + dz * dz)
        C = L - jr[j]
        wa = inv_mass[a]
        wb = inv_mass[b]
        s = C * jk[j] / (wa + wb) / max(L, 1e-9)
        delta[a, 0] += s * dx * wa
        delta[a, 1] += s * dy * wa
        delta[a, 2] += s * dz * wa
        delta[b, 0] -= s * dx * wb
        delta[b, 1] -= s * dy * wb
        delta[b, 2] -= s * dz * wb
    for i in range(nv):
        f = 1.0 / jcount[i]
        for a in range(3):
            pos[i, a] += delta[i, a] * f


# ---------------------------------------------------------------------------
# 当たり判定の芯（点・線分・三角形の最近点）
# ---------------------------------------------------------------------------
@njit(cache=True, nogil=True, fastmath=True)
def _closest_seg(p, a, b, out):
    ab0, ab1, ab2 = b[0] - a[0], b[1] - a[1], b[2] - a[2]
    den = ab0 * ab0 + ab1 * ab1 + ab2 * ab2
    t = ((p[0] - a[0]) * ab0 + (p[1] - a[1]) * ab1 + (p[2] - a[2]) * ab2) / max(den, 1e-12)
    t = min(max(t, 0.0), 1.0)
    out[0] = a[0] + ab0 * t
    out[1] = a[1] + ab1 * t
    out[2] = a[2] + ab2 * t
    d0, d1, d2 = p[0] - out[0], p[1] - out[1], p[2] - out[2]
    return np.sqrt(d0 * d0 + d1 * d1 + d2 * d2)


@njit(cache=True, nogil=True, fastmath=True)
def _closest_tri(p, a, b, c, out, q1, q2, q3):
    ab0, ab1, ab2 = b[0] - a[0], b[1] - a[1], b[2] - a[2]
    ac0, ac1, ac2 = c[0] - a[0], c[1] - a[1], c[2] - a[2]
    n0 = ab1 * ac2 - ab2 * ac1
    n1 = ab2 * ac0 - ab0 * ac2
    n2 = ab0 * ac1 - ab1 * ac0
    nn = max(np.sqrt(n0 * n0 + n1 * n1 + n2 * n2), 1e-12)
    n0 /= nn; n1 /= nn; n2 /= nn
    dn = (p[0] - a[0]) * n0 + (p[1] - a[1]) * n1 + (p[2] - a[2]) * n2
    pr0, pr1, pr2 = p[0] - n0 * dn, p[1] - n1 * dn, p[2] - n2 * dn
    v20, v21, v22 = pr0 - a[0], pr1 - a[1], pr2 - a[2]
    d00 = ab0 * ab0 + ab1 * ab1 + ab2 * ab2
    d01 = ab0 * ac0 + ab1 * ac1 + ab2 * ac2
    d11 = ac0 * ac0 + ac1 * ac1 + ac2 * ac2
    d20 = v20 * ab0 + v21 * ab1 + v22 * ab2
    d21 = v20 * ac0 + v21 * ac1 + v22 * ac2
    den = d00 * d11 - d01 * d01
    if abs(den) < 1e-12:
        den = 1e-12
    v = (d11 * d20 - d01 * d21) / den
    w = (d00 * d21 - d01 * d20) / den
    u = 1 - v - w
    if u >= 0 and v >= 0 and w >= 0:
        out[0], out[1], out[2] = pr0, pr1, pr2
        return abs(dn)
    e1 = _closest_seg(p, a, b, q1)
    e2 = _closest_seg(p, b, c, q2)
    e3 = _closest_seg(p, c, a, q3)
    if e1 <= e2 and e1 <= e3:
        out[0], out[1], out[2] = q1[0], q1[1], q1[2]
        return e1
    if e2 <= e3:
        out[0], out[1], out[2] = q2[0], q2[1], q2[2]
        return e2
    out[0], out[1], out[2] = q3[0], q3[1], q3[2]
    return e3


@njit(cache=True, nogil=True, fastmath=True)
def contact_cores(pos, ctype, cidx):
    """各頂点の芯の最近点（内部の頂点は NaN）。"""
    nv = pos.shape[0]
    n = nv // 8
    cen = np.zeros((n, 3))
    for i in range(n):
        for k in range(8):
            for a in range(3):
                cen[i, a] += pos[8 * i + k, a]
        for a in range(3):
            cen[i, a] /= 8.0
    K = np.empty((nv, 3))
    tmp = np.empty(3)
    t1 = np.empty(3)
    t2 = np.empty(3)
    t3 = np.empty(3)
    for v in range(nv):
        t = ctype[v]
        if t < 0:
            K[v, 0] = np.nan
            K[v, 1] = np.nan
            K[v, 2] = np.nan
        elif t == 0:
            K[v, 0] = cen[cidx[v, 0], 0]
            K[v, 1] = cen[cidx[v, 0], 1]
            K[v, 2] = cen[cidx[v, 0], 2]
        elif t == 1:
            _closest_seg(pos[v], cen[cidx[v, 0]], cen[cidx[v, 1]], tmp)
            K[v, 0], K[v, 1], K[v, 2] = tmp[0], tmp[1], tmp[2]
        else:
            _closest_tri(pos[v], cen[cidx[v, 0]], cen[cidx[v, 1]], cen[cidx[v, 2]], tmp, t1, t2, t3)
            K[v, 0], K[v, 1], K[v, 2] = tmp[0], tmp[1], tmp[2]
    return K


# ---------------------------------------------------------------------------
# 描画用：変形後の表面の頂点と法線
# ---------------------------------------------------------------------------
@njit(cache=True, nogil=True, fastmath=True)
def skin(pos, vidx, w):
    M = vidx.shape[0]
    out = np.zeros((M, 3))
    for m in range(M):
        for k in range(8):
            wk = w[m, k]
            p = vidx[m, k]
            out[m, 0] += wk * pos[p, 0]
            out[m, 1] += wk * pos[p, 1]
            out[m, 2] += wk * pos[p, 2]
    return out


@njit(cache=True, nogil=True, fastmath=True)
def vertex_normals(v, faces):
    vn = np.zeros_like(v)
    for f in range(faces.shape[0]):
        a, b, c = faces[f, 0], faces[f, 1], faces[f, 2]
        e10, e11, e12 = v[b, 0] - v[a, 0], v[b, 1] - v[a, 1], v[b, 2] - v[a, 2]
        e20, e21, e22 = v[c, 0] - v[a, 0], v[c, 1] - v[a, 1], v[c, 2] - v[a, 2]
        n0 = e11 * e22 - e12 * e21
        n1 = e12 * e20 - e10 * e22
        n2 = e10 * e21 - e11 * e20
        for i in (a, b, c):
            vn[i, 0] += n0
            vn[i, 1] += n1
            vn[i, 2] += n2
    for i in range(v.shape[0]):
        L = np.sqrt(vn[i, 0] ** 2 + vn[i, 1] ** 2 + vn[i, 2] ** 2)
        L = max(L, 1e-12)
        vn[i, 0] /= L
        vn[i, 1] /= L
        vn[i, 2] /= L
    return vn


# ---------------------------------------------------------------------------
# 完全剛体モードの関節つき剛体（アクチュエーターで分かれた部分ごとの剛体）
# 位置ベースの剛体ソルバー（XPBD の剛体版、コンプライアンス 0）：
#   アクチュエーターの出力面の4隅を、回転角だけ回した位置に出力側の部分の4隅を合わせる点拘束と、
#   地面・建物との接触（めり込み＋静止摩擦）を、部分ごとの並進・回転として交互に解く。
# ---------------------------------------------------------------------------
@njit(cache=True, nogil=True, fastmath=True)
def _rot_left(R, px, py, pz):
    """R ← exp([p]×) R（ロドリゲスの回転公式）。R は 3x3（その場で書き換える）"""
    th = np.sqrt(px * px + py * py + pz * pz)
    if th < 1e-14:
        return
    kx, ky, kz = px / th, py / th, pz / th
    s, c = np.sin(th), np.cos(th)
    t = 1.0 - c
    m00 = c + kx * kx * t
    m01 = kx * ky * t - kz * s
    m02 = kx * kz * t + ky * s
    m10 = ky * kx * t + kz * s
    m11 = c + ky * ky * t
    m12 = ky * kz * t - kx * s
    m20 = kz * kx * t - ky * s
    m21 = kz * ky * t + kx * s
    m22 = c + kz * kz * t
    for j in range(3):
        r0, r1, r2 = R[0, j], R[1, j], R[2, j]
        R[0, j] = m00 * r0 + m01 * r1 + m02 * r2
        R[1, j] = m10 * r0 + m11 * r1 + m12 * r2
        R[2, j] = m20 * r0 + m21 * r1 + m22 * r2


@njit(cache=True, nogil=True, fastmath=True)
def _iw(R, IinvB, g, vx, vy, vz):
    """ワールドの逆慣性 × v（= R Iinv_b R^T v）"""
    Rg = R[g]
    I = IinvB[g]
    ax = Rg[0, 0] * vx + Rg[1, 0] * vy + Rg[2, 0] * vz
    ay = Rg[0, 1] * vx + Rg[1, 1] * vy + Rg[2, 1] * vz
    az = Rg[0, 2] * vx + Rg[1, 2] * vy + Rg[2, 2] * vz
    bx = I[0, 0] * ax + I[0, 1] * ay + I[0, 2] * az
    by = I[1, 0] * ax + I[1, 1] * ay + I[1, 2] * az
    bz = I[2, 0] * ax + I[2, 1] * ay + I[2, 2] * az
    return (Rg[0, 0] * bx + Rg[0, 1] * by + Rg[0, 2] * bz,
            Rg[1, 0] * bx + Rg[1, 1] * by + Rg[1, 2] * bz,
            Rg[2, 0] * bx + Rg[2, 1] * by + Rg[2, 2] * bz)


@njit(cache=True, nogil=True, fastmath=True)
def _apply_pt(X, R, invM, IinvB, g, rx, ry, rz, Px, Py, Pz, sgn):
    """部分 g の点（重心からの位置 r）に、位置の補正 P（インパルス×h に相当）を加える。"""
    X[g, 0] += sgn * Px * invM[g]
    X[g, 1] += sgn * Py * invM[g]
    X[g, 2] += sgn * Pz * invM[g]
    tx = ry * Pz - rz * Py
    ty = rz * Px - rx * Pz
    tz = rx * Py - ry * Px
    phx, phy, phz = _iw(R, IinvB, g, tx, ty, tz)
    _rot_left(R[g], sgn * phx, sgn * phy, sgn * phz)


@njit(cache=True, nogil=True, fastmath=True)
def _gen_w(R, invM, IinvB, g, rx, ry, rz, nx, ny, nz):
    """部分 g の点 r での方向 n の一般化逆質量。"""
    cx = ry * nz - rz * ny
    cy = rz * nx - rx * nz
    cz = rx * ny - ry * nx
    ix, iy, iz = _iw(R, IinvB, g, cx, cy, cz)
    return invM[g] + cx * ix + cy * iy + cz * iz


@njit(cache=True, nogil=True, fastmath=True)
def _ang_w(R, IinvB, g, ux, uy, uz):
    ix, iy, iz = _iw(R, IinvB, g, ux, uy, uz)
    return ux * ix + uy * iy + uz * iz


@njit(cache=True, nogil=True, fastmath=True)
def art_solve(X, R, invM, IinvB, ra, rb, LA, LB, LC, ang, cap, cg, cl, cn, cd, cp, cprev, cvs, cmu, h, iters, lamN):
    nr = ra.shape[0]
    nc = cg.shape[0]
    lamT = np.zeros(nc)
    pa = np.empty((4, 3))
    tg = np.empty((4, 3))
    pb = np.empty((4, 3))
    H = np.zeros((3, 3))
    used = np.zeros(nr)          # このサブステップでモーターが回した角度（上限 cap まで：モーターの力には限りがある）
    for it in range(iters):
        # --- 関節（アクチュエーター） ---
        for q in range(nr):
            a, b = ra[q], rb[q]
            for k in range(4):
                for i in range(3):
                    pa[k, i] = X[a, i] + R[a, i, 0] * LA[q, k, 0] + R[a, i, 1] * LA[q, k, 1] + R[a, i, 2] * LA[q, k, 2]
            cx = (pa[0, 0] + pa[1, 0] + pa[2, 0] + pa[3, 0]) * 0.25
            cy = (pa[0, 1] + pa[1, 1] + pa[2, 1] + pa[3, 1]) * 0.25
            cz = (pa[0, 2] + pa[1, 2] + pa[2, 2] + pa[3, 2]) * 0.25
            ax = X[a, 0] + R[a, 0, 0] * LC[q, 0] + R[a, 0, 1] * LC[q, 1] + R[a, 0, 2] * LC[q, 2]
            ay = X[a, 1] + R[a, 1, 0] * LC[q, 0] + R[a, 1, 1] * LC[q, 1] + R[a, 1, 2] * LC[q, 2]
            az = X[a, 2] + R[a, 2, 0] * LC[q, 0] + R[a, 2, 1] * LC[q, 1] + R[a, 2, 2] * LC[q, 2]
            nx, ny, nz = cx - ax, cy - ay, cz - az
            L = np.sqrt(nx * nx + ny * ny + nz * nz)
            if L < 1e-12:
                continue
            nx, ny, nz = nx / L, ny / L, nz / L
            cs, sn = np.cos(ang[q]), np.sin(ang[q])
            for k in range(4):
                vx, vy, vz = pa[k, 0] - cx, pa[k, 1] - cy, pa[k, 2] - cz
                dot = vx * nx + vy * ny + vz * nz
                crx = ny * vz - nz * vy
                cry = nz * vx - nx * vz
                crz = nx * vy - ny * vx
                tg[k, 0] = cx + vx * cs + crx * sn + nx * dot * (1 - cs)
                tg[k, 1] = cy + vy * cs + cry * sn + ny * dot * (1 - cs)
                tg[k, 2] = cz + vz * cs + crz * sn + nz * dot * (1 - cs)
            # 出力側の今の4隅と、その中心
            bcx = 0.0
            bcy = 0.0
            bcz = 0.0
            for k in range(4):
                for i in range(3):
                    pb[k, i] = X[b, i] + R[b, i, 0] * LB[q, k, 0] + R[b, i, 1] * LB[q, k, 1] + R[b, i, 2] * LB[q, k, 2]
                bcx += pb[k, 0] * 0.25
                bcy += pb[k, 1] * 0.25
                bcz += pb[k, 2] * 0.25
            # (1) 向き：モーターは出力側だけを回す（反トルクは土台に伝えない：今までのブロックごとの計算と同じ）
            #     出力面の4隅（中心から見た向き）を目標の4隅に重ねる回転を、出力面の中心のまわりにかける
            H[:, :] = 0.0
            for k in range(4):
                ux, uy, uz = pb[k, 0] - bcx, pb[k, 1] - bcy, pb[k, 2] - bcz
                vx, vy, vz = tg[k, 0] - cx, tg[k, 1] - cy, tg[k, 2] - cz
                H[0, 0] += ux * vx
                H[0, 1] += ux * vy
                H[0, 2] += ux * vz
                H[1, 0] += uy * vx
                H[1, 1] += uy * vy
                H[1, 2] += uy * vz
                H[2, 0] += uz * vx
                H[2, 1] += uz * vy
                H[2, 2] += uz * vz
            Q = _polar(H.T)
            # 回転ベクトル φ（軸×角度）に直し、モーターの軸まわり（ねじれ）と、それ以外（軸の傾き）に分ける
            cth = (Q[0, 0] + Q[1, 1] + Q[2, 2] - 1.0) * 0.5
            if cth > 1.0:
                cth = 1.0
            if cth < -1.0:
                cth = -1.0
            th = np.arccos(cth)
            fx, fy, fz = Q[2, 1] - Q[1, 2], Q[0, 2] - Q[2, 0], Q[1, 0] - Q[0, 1]
            fl = np.sqrt(fx * fx + fy * fy + fz * fz)
            if fl > 1e-12 and th > 1e-12:
                fx, fy, fz = fx / fl * th, fy / fl * th, fz / fl * th
            else:
                fx, fy, fz = 0.0, 0.0, 0.0
            tw = fx * nx + fy * ny + fz * nz
            bx, by, bz = fx - nx * tw, fy - ny * tw, fz - nz * tw
            # (1a) 軸の傾き：軸受けとして硬く保つ（両側の回りにくさで分け合う）
            bl = np.sqrt(bx * bx + by * by + bz * bz)
            if bl > 1e-12:
                ux, uy, uz = bx / bl, by / bl, bz / bl
                lam = bl / max(_ang_w(R, IinvB, a, ux, uy, uz) + _ang_w(R, IinvB, b, ux, uy, uz), 1e-30)
                pbx, pby, pbz = _iw(R, IinvB, b, ux * lam, uy * lam, uz * lam)
                pax, pay, paz = _iw(R, IinvB, a, ux * lam, uy * lam, uz * lam)
                _rot_left(R[b], pbx, pby, pbz)
                _rot_left(R[a], -pax, -pay, -paz)
            # (1b) ねじれ：モーター。軸まわりの角度を目標に合わせる（反作用は土台側にも：両側の回りにくさで分け合う）。
            #      1サブステップで直せる角度には上限 cap がある（ぶつかって回れないときに無限の力で押し続けないように）
            room = cap[q] - used[q]
            if room < 0:
                room = 0.0
            if tw > room:
                tw = room
            if tw < -room:
                tw = -room
            used[q] += abs(tw)
            if abs(tw) > 1e-14:
                lam = tw / max(_ang_w(R, IinvB, a, nx, ny, nz) + _ang_w(R, IinvB, b, nx, ny, nz), 1e-30)
                pbx, pby, pbz = _iw(R, IinvB, b, nx * lam, ny * lam, nz * lam)
                pax, pay, paz = _iw(R, IinvB, a, nx * lam, ny * lam, nz * lam)
                _rot_left(R[b], pbx, pby, pbz)
                _rot_left(R[a], -pax, -pay, -paz)
            # 出力面の中心（つなぎ目の補正用）
            bcx = 0.0
            bcy = 0.0
            bcz = 0.0
            for k in range(4):
                for i in range(3):
                    pb[k, i] = X[b, i] + R[b, i, 0] * LB[q, k, 0] + R[b, i, 1] * LB[q, k, 1] + R[b, i, 2] * LB[q, k, 2]
                bcx += pb[k, 0] * 0.25
                bcy += pb[k, 1] * 0.25
                bcz += pb[k, 2] * 0.25
            # (2) つなぎ目：出力面の中心どうしを合わせる（両側の重さ・回りにくさで分け合う）
            dx, dy, dz = cx - bcx, cy - bcy, cz - bcz
            d = np.sqrt(dx * dx + dy * dy + dz * dz)
            if d < 1e-10:
                continue
            ux, uy, uz = dx / d, dy / d, dz / d
            rax, ray, raz = bcx - X[a, 0], bcy - X[a, 1], bcz - X[a, 2]
            rbx, rby, rbz = bcx - X[b, 0], bcy - X[b, 1], bcz - X[b, 2]
            w = _gen_w(R, invM, IinvB, a, rax, ray, raz, ux, uy, uz) + \
                _gen_w(R, invM, IinvB, b, rbx, rby, rbz, ux, uy, uz)
            lam = d / w
            _apply_pt(X, R, invM, IinvB, b, rbx, rby, rbz, ux * lam, uy * lam, uz * lam, 1.0)
            _apply_pt(X, R, invM, IinvB, a, rax, ray, raz, ux * lam, uy * lam, uz * lam, -1.0)
        # --- 地面・建物との接触 ---
        for k in range(nc):
            g = cg[k]
            px = X[g, 0] + R[g, 0, 0] * cl[k, 0] + R[g, 0, 1] * cl[k, 1] + R[g, 0, 2] * cl[k, 2]
            py = X[g, 1] + R[g, 1, 0] * cl[k, 0] + R[g, 1, 1] * cl[k, 1] + R[g, 1, 2] * cl[k, 2]
            pz = X[g, 2] + R[g, 2, 0] * cl[k, 0] + R[g, 2, 1] * cl[k, 1] + R[g, 2, 2] * cl[k, 2]
            nx, ny, nz = cn[k, 0], cn[k, 1], cn[k, 2]
            pen = cd[k] - ((px - cp[k, 0]) * nx + (py - cp[k, 1]) * ny + (pz - cp[k, 2]) * nz)
            if pen > 0:
                rx, ry, rz = px - X[g, 0], py - X[g, 1], pz - X[g, 2]
                w = _gen_w(R, invM, IinvB, g, rx, ry, rz, nx, ny, nz)
                lam = pen / w
                lamN[k] += lam
                _apply_pt(X, R, invM, IinvB, g, rx, ry, rz, nx * lam, ny * lam, nz * lam, 1.0)
                px += nx * pen
                py += ny * pen
                pz += nz * pen
            if lamN[k] <= 0:
                continue
            # 静止摩擦：このサブステップでの接点の横すべりを、押し戻した量×摩擦係数の範囲で止める
            dx = px - cprev[k, 0] - cvs[k, 0] * h
            dy = py - cprev[k, 1] - cvs[k, 1] * h
            dz = pz - cprev[k, 2] - cvs[k, 2] * h
            dn = dx * nx + dy * ny + dz * nz
            tx, ty, tz = dx - nx * dn, dy - ny * dn, dz - nz * dn
            tl = np.sqrt(tx * tx + ty * ty + tz * tz)
            if tl < 1e-10:
                continue
            ux, uy, uz = -tx / tl, -ty / tl, -tz / tl
            rx, ry, rz = px - X[g, 0], py - X[g, 1], pz - X[g, 2]
            w = _gen_w(R, invM, IinvB, g, rx, ry, rz, ux, uy, uz)
            lam = tl * cmu[k] / w
            lim = 1.5 * lamN[k] - lamT[k]
            if lim <= 0:
                continue
            if lam > lim:
                lam = lim
            lamT[k] += lam
            _apply_pt(X, R, invM, IinvB, g, rx, ry, rz, ux * lam, uy * lam, uz * lam, 1.0)


@njit(cache=True, nogil=True, fastmath=True)
def art_contact_vel(V, W, X, R, invM, IinvB, cg, cl, cn, lamN, iters):
    """接触点の速度の後始末（XPBD の速度パス、はね返り 0）：めり込みを押し戻したことで生まれた
    「地面から離れる向きの速度」を消す（押し戻し → 跳ねる → 落ちる、の小刻みな揺れを防ぐ）。"""
    nc = cg.shape[0]
    for it in range(iters):
        for k in range(nc):
            if lamN[k] <= 0:
                continue
            g = cg[k]
            Rg = R[g]
            rx = Rg[0, 0] * cl[k, 0] + Rg[0, 1] * cl[k, 1] + Rg[0, 2] * cl[k, 2]
            ry = Rg[1, 0] * cl[k, 0] + Rg[1, 1] * cl[k, 1] + Rg[1, 2] * cl[k, 2]
            rz = Rg[2, 0] * cl[k, 0] + Rg[2, 1] * cl[k, 1] + Rg[2, 2] * cl[k, 2]
            vx = V[g, 0] + W[g, 1] * rz - W[g, 2] * ry
            vy = V[g, 1] + W[g, 2] * rx - W[g, 0] * rz
            vz = V[g, 2] + W[g, 0] * ry - W[g, 1] * rx
            nx, ny, nz = cn[k, 0], cn[k, 1], cn[k, 2]
            vn = vx * nx + vy * ny + vz * nz
            if vn <= 0:
                continue
            w = _gen_w(R, invM, IinvB, g, rx, ry, rz, nx, ny, nz)
            j = -vn / w
            V[g, 0] += nx * j * invM[g]
            V[g, 1] += ny * j * invM[g]
            V[g, 2] += nz * j * invM[g]
            tx = ry * nz - rz * ny
            ty = rz * nx - rx * nz
            tz = rx * ny - ry * nx
            ix, iy, iz = _iw(R, IinvB, g, tx * j, ty * j, tz * j)
            W[g, 0] += ix
            W[g, 1] += iy
            W[g, 2] += iz


@njit(cache=True, nogil=True, fastmath=True)
def art_write(pos, X, R, gv, vg, vl):
    """部分ごとの姿勢から頂点の位置を作る。"""
    for k in range(gv.shape[0]):
        v = gv[k]
        g = vg[k]
        for i in range(3):
            pos[v, i] = X[g, i] + R[g, i, 0] * vl[k, 0] + R[g, i, 1] * vl[k, 1] + R[g, i, 2] * vl[k, 2]


def set_enabled(on):
    """設定で高速化を切り替える（numba が無ければ常に OFF）。"""
    global AVAILABLE
    from . import construct
    AVAILABLE = bool(on) and _HAS_NUMBA
    construct.FAST = AVAILABLE


def warmup():
    """起動時に一度だけ小さなデータで呼んでコンパイルしておく。"""
    if not AVAILABLE:
        return
    pos = np.random.rand(16, 3)
    rest = np.random.rand(2, 8, 3)
    sc = np.ones((2, 3))
    st = np.ones(2)
    lr = np.zeros((2, 3, 3))
    vox = np.arange(2, dtype=np.int64)
    qq = np.zeros((2, 4))
    qq[:, 0] = 1
    shape_match(pos, rest, sc, st, vox, lr, qq)
    shape_match_par(pos, rest, sc, st, vox, lr, qq)
    shape_match_chunks(pos, np.arange(16, dtype=np.int64), np.zeros(16, np.int64), np.random.rand(16, 3), 1)
    ja = np.array([0, 1], np.int64)
    jb = np.array([8, 9], np.int64)
    solve_joints(pos, ja, jb, np.zeros(2), np.ones(2), np.ones(16), np.ones(16))
    ct = np.array([0, 1, 2, -1] * 4, np.int64)
    ci = np.zeros((16, 3), np.int64)
    ci[:, 1] = 1
    contact_cores(pos, ct, ci)
    vi = np.zeros((3, 8), np.int64)
    sk = skin(pos, vi, np.full((3, 8), 0.125))
    vertex_normals(sk, np.array([[0, 1, 2]], np.int64))
    X = np.zeros((2, 3)); R2 = np.zeros((2, 3, 3)); R2[:] = np.eye(3)
    IB = np.zeros((2, 3, 3)); IB[:] = np.eye(3)
    art_solve(X, R2, np.ones(2), IB, np.array([0], np.int64), np.array([1], np.int64), np.random.rand(1, 4, 3),
              np.random.rand(1, 4, 3), np.zeros((1, 3)), np.zeros(1), np.ones(1), np.array([0], np.int64), np.zeros((1, 3)),
              np.array([[0, 1.0, 0]]), np.ones(1) * 0.01, np.zeros((1, 3)), np.zeros((1, 3)), np.zeros((1, 3)),
              np.ones(1), 0.01, 2, np.zeros(1))
    art_contact_vel(np.zeros((2, 3)), np.zeros((2, 3)), X, R2, np.ones(2), IB, np.array([0], np.int64), np.zeros((1, 3)),
                    np.array([[0, 1.0, 0]]), np.ones(1), 2)
    art_write(np.zeros((4, 3)), X, R2, np.arange(4, dtype=np.int64), np.zeros(4, np.int64), np.zeros((4, 3)))
