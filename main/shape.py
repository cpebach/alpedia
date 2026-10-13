"""構築物の「形」：ボクセルを球として扱い、結合したボクセルの間を埋める。

形 = 次の図形を半径 r（ボクセルの半分）でなぞったものの和集合
  - 各ボクセルの中心（点）              → 球
  - 結合した2つのボクセルの中心を結ぶ線 → 両端が球の円柱（カプセル）
  - 2×2×2 の範囲で結合しているボクセル群の中心の凸包の面（三角形） → 斜面・平面
したがって、2つ並べればカプセル、L字や階段状に置けば内側の角が斜面で埋まる。

この形は描画（マーチングキューブでメッシュ化し、ボクセルの8頂点で変形させる）と
当たり判定（地面・プレイヤー・構築物どうし）の両方で使う。
"""
import itertools

import numpy as np

from .construct import DIR_TUPLES, CORNER_BITS, link_key

from skimage.measure import marching_cubes

OFFS8 = [tuple(int(x) for x in o) for o in itertools.product((0, 1), repeat=3)]
_pattern_cache = {}
CUBE_FACES = []  # 2×2×2 窓の6面（各面4頂点のオフセット）
for _ax in range(3):
    for _v in (0, 1):
        CUBE_FACES.append([o for o in OFFS8 if o[_ax] == _v])


def _hull_tris(offsets):
    """{0,1}^3 の点の凸包の表面三角形（点の番号の組）。"""
    key = tuple(sorted(offsets))
    if key in _pattern_cache:
        return _pattern_cache[key]
    P = np.asarray(key, dtype=float)
    tris = []
    if len(P) >= 3:
        Q = P - P.mean(0)
        rank = np.linalg.matrix_rank(Q, tol=1e-6)
        if rank == 2:
            _, _, vt = np.linalg.svd(Q)
            uv = Q @ vt[:2].T
            ang = np.arctan2(uv[:, 1], uv[:, 0])
            order = list(np.argsort(ang))
            for k in range(1, len(order) - 1):
                tris.append((order[0], order[k], order[k + 1]))
        elif rank == 3:
            from scipy.spatial import ConvexHull
            h = ConvexHull(P)
            tris = [tuple(int(x) for x in s) for s in h.simplices]
    res = [tuple(key[a] for a in t) for t in tris]
    _pattern_cache[key] = res
    return res


def topology(c):
    """形を作る図形の一覧を返す: dict(points, segs, tris)（ボクセル番号の配列）。"""
    idx = c.index
    links = c.links
    n = c.n_vox
    linked_count = np.zeros(n, int)
    seg = set()
    for a, b in links:
        ia, ib = idx.get(a), idx.get(b)
        if ia is not None and ib is not None:
            seg.add((min(ia, ib), max(ia, ib)))
            linked_count[ia] += 1
            linked_count[ib] += 1
    for a, b in getattr(c, "extras", {}):
        ia, ib = idx.get(a), idx.get(b)
        if ia is not None and ib is not None:
            seg.add((min(ia, ib), max(ia, ib)))
    interior = linked_count >= 6
    # 2×2×2 の窓
    anchors = set()
    for cell in c.cells:
        for o in OFFS8:
            anchors.add((cell[0] - o[0], cell[1] - o[1], cell[2] - o[2]))
    full = set()
    window_members = {}
    for a in anchors:
        mem = []
        for o in OFFS8:
            j = idx.get((a[0] + o[0], a[1] + o[1], a[2] + o[2]))
            if j is not None:
                mem.append((o, j))
        if len(mem) < 2:
            continue
        window_members[a] = mem
        if len(mem) == 8:
            cells_ = [(a[0] + o[0], a[1] + o[1], a[2] + o[2]) for o, _ in mem]
            ok = all(link_key(x, (x[0] + d[0], x[1] + d[1], x[2] + d[2])) in links
                     for x in cells_ for d in DIR_TUPLES
                     if (x[0] + d[0] - a[0]) in (0, 1) and (x[1] + d[1] - a[1]) in (0, 1)
                     and (x[2] + d[2] - a[2]) in (0, 1))
            if ok:
                full.add(a)
    tris = set()
    for a, mem in window_members.items():
        if a in full:
            # 内部の面は省略し、隣の窓が埋まっていない面だけ作る
            for f, face in enumerate(CUBE_FACES):
                ax, v = f // 2, f % 2
                nb = list(a)
                nb[ax] += 1 if v == 1 else -1
                if tuple(nb) in full:
                    continue
                m = {o: j for o, j in mem}
                q = [m[o] for o in face]  # face の順: 0,1,2,3 は格子順
                # 4点 (00,01,10,11) → 2三角形
                tris.add(tuple(sorted((q[0], q[1], q[3]))))
                tris.add(tuple(sorted((q[0], q[3], q[2]))))
            continue
        # 窓の中の結合成分
        parent = {o: o for o, _ in mem}

        def find(x):
            while parent[x] != x:
                parent[x] = parent[parent[x]]
                x = parent[x]
            return x
        m = {o: j for o, j in mem}
        for o, _ in mem:
            for d in DIR_TUPLES:
                o2 = (o[0] + d[0], o[1] + d[1], o[2] + d[2])
                if o2 in m:
                    ca = (a[0] + o[0], a[1] + o[1], a[2] + o[2])
                    cb = (a[0] + o2[0], a[1] + o2[1], a[2] + o2[2])
                    if link_key(ca, cb) in links:
                        parent[find(o)] = find(o2)
        comps = {}
        for o, _ in mem:
            comps.setdefault(find(o), []).append(o)
        for comp in comps.values():
            if len(comp) < 3:
                continue
            for t in _hull_tris(comp):
                tri = tuple(sorted(m[o] for o in t))
                if not all(interior[list(tri)]):
                    tris.add(tri)
    points = np.nonzero(~interior)[0]
    segs = np.array([s for s in seg if not (interior[s[0]] and interior[s[1]])], dtype=np.int64).reshape(-1, 2)
    return {"points": points.astype(np.int64), "segs": segs,
            "tris": np.array(sorted(tris), dtype=np.int64).reshape(-1, 3)}


# ---------------------------------------------------------------------------
# 距離（ベクトル化）
# ---------------------------------------------------------------------------
def dist_segments(p, a, b):
    ab = b - a
    t = np.einsum("...i,...i->...", p - a, ab) / np.maximum(np.einsum("...i,...i->...", ab, ab), 1e-12)
    t = np.clip(t, 0, 1)
    q = a + ab * t[..., None]
    return np.linalg.norm(p - q, axis=-1), q


def dist_triangles(p, a, b, c):
    """点 p と三角形 abc の距離と最近点（形はブロードキャスト可能）。"""
    ab, ac = b - a, c - a
    n = np.cross(ab, ac)
    nn = np.linalg.norm(n, axis=-1, keepdims=True)
    n = n / np.maximum(nn, 1e-12)
    ap = p - a
    dn = np.einsum("...i,...i->...", ap, n)
    proj = p - n * dn[..., None]
    # 重心座標
    v0, v1, v2 = ab, ac, proj - a
    d00 = np.einsum("...i,...i->...", v0, v0)
    d01 = np.einsum("...i,...i->...", v0, v1)
    d11 = np.einsum("...i,...i->...", v1, v1)
    d20 = np.einsum("...i,...i->...", v2, v0)
    d21 = np.einsum("...i,...i->...", v2, v1)
    den = d00 * d11 - d01 * d01
    den = np.where(np.abs(den) < 1e-12, 1e-12, den)
    v = (d11 * d20 - d01 * d21) / den
    w = (d00 * d21 - d01 * d20) / den
    u = 1 - v - w
    inside = (u >= 0) & (v >= 0) & (w >= 0)
    d1, q1 = dist_segments(p, a, b)
    d2, q2 = dist_segments(p, b, c)
    d3, q3 = dist_segments(p, c, a)
    de = np.minimum(np.minimum(d1, d2), d3)
    qe = np.where((d1 <= d2)[..., None] & (d1 <= d3)[..., None], q1, np.where((d2 <= d3)[..., None], q2, q3))
    d = np.where(inside, np.abs(dn), de)
    q = np.where(inside[..., None], proj, qe)
    return d, q


# ---------------------------------------------------------------------------
# メッシュ（描画・OBJ 出力）
# ---------------------------------------------------------------------------
def build_mesh(c, k=None):
    """形の表面メッシュを作る（格子座標＝セル単位）。ブロックが無ければ None。"""
    if not c.n_vox:
        return None
    topo = c.topo
    nprim = len(topo["points"]) + len(topo["segs"]) + len(topo["tris"])
    if k is None:
        k = 8 if nprim < 60 else 6 if nprim < 400 else 4 if nprim < 1500 else 3 if nprim < 6000 else 2
    r = 0.5
    cells = np.asarray(c.cells, dtype=np.float64)
    centers = cells + 0.5
    lo = cells.min(0) - 1
    hi = cells.max(0) + 2
    dims = np.ceil((hi - lo) * k).astype(int) + 1
    if np.prod(dims) > 30_000_000:
        k = 2
        dims = np.ceil((hi - lo) * k).astype(int) + 1
    sdf = np.full(int(np.prod(dims)), 1.0, dtype=np.float32)
    m = 2 * k + 3
    O = np.stack(np.meshgrid(*(np.arange(m),) * 3, indexing="ij"), -1).reshape(-1, 3)

    def apply(kind, prims):
        if not len(prims):
            return
        for s in range(0, len(prims), max(1, 4_000_000 // len(O))):
            P = prims[s:s + max(1, 4_000_000 // len(O))]
            pts = centers[P].reshape(len(P), -1, 3)            # (C, q, 3)
            pmin = pts.min(1) - r
            i0 = np.floor((pmin - lo) * k).astype(int)          # (C,3)
            idx = i0[:, None, :] + O[None]                      # (C, S, 3)
            idx = np.clip(idx, 0, dims - 1)
            g = lo + idx / k
            if kind == "p":
                d = np.linalg.norm(g - pts[:, 0][:, None], axis=-1)
            elif kind == "s":
                d, _ = dist_segments(g, pts[:, 0][:, None], pts[:, 1][:, None])
            else:
                d, _ = dist_triangles(g, pts[:, 0][:, None], pts[:, 1][:, None], pts[:, 2][:, None])
            lin = (idx[..., 0] * dims[1] + idx[..., 1]) * dims[2] + idx[..., 2]
            np.minimum.at(sdf, lin.ravel(), (d - r).ravel().astype(np.float32))
    apply("p", topo["points"].reshape(-1, 1))
    apply("s", topo["segs"])
    apply("t", topo["tris"])
    vol = sdf.reshape(dims)
    if vol.min() >= 0:
        return None
    verts, faces, normals, _ = marching_cubes(vol, level=0.0)
    p = lo + verts / k                                         # セル単位
    # 面の向きをそろえる（外向き）
    fn = np.cross(p[faces[:, 1]] - p[faces[:, 0]], p[faces[:, 2]] - p[faces[:, 0]])
    vn = normals[faces].mean(1)
    if np.einsum("ij,ij->", fn, vn) < 0:
        faces = faces[:, ::-1]
    # 各頂点をボクセルに割り当て、そのボクセルの8頂点で補間（スキニング）
    owner = _owner(c, p, centers)
    u = p - cells[owner]                                       # ボクセル内の位置 0〜1（外へはみ出すこともある）
    W = np.ones((len(p), 8))
    for k8 in range(8):
        bits = CORNER_BITS[k8]
        for ax in range(3):
            W[:, k8] *= np.where(bits[ax] > 0, u[:, ax], 1 - u[:, ax])
    return {"rest": p, "faces": faces.astype(np.int64), "owner": owner, "w": W,
            "vidx": (8 * owner[:, None] + np.arange(8)[None]).astype(np.int64), "version": c.version}


def _owner(c, p, centers):
    cell = np.floor(p).astype(np.int64)
    owner = np.full(len(p), -1, dtype=np.int64)
    for k, cl in enumerate(map(tuple, cell)):
        j = c.index.get(cl)
        if j is not None:
            owner[k] = j
    miss = owner < 0
    if miss.any():
        from scipy.spatial import cKDTree
        _, nn = cKDTree(centers).query(p[miss])
        owner[miss] = nn
    return owner


def skin(c, mesh):
    """現在の頂点位置（変形後）。"""
    from . import fastphys
    return fastphys.skin(c.pos, mesh["vidx"], mesh["w"])


def vertex_normals(v, faces):
    from . import fastphys
    return fastphys.vertex_normals(np.ascontiguousarray(v, dtype=np.float64),
                                   np.ascontiguousarray(faces, dtype=np.int64))


# ---------------------------------------------------------------------------
# 当たり判定
# ---------------------------------------------------------------------------
def closest_on_shape(c, p, max_dist):
    """点 p から形（表面）への最短：(距離, 最近点, 近いボクセル番号) または None。
    距離は「芯（点・線・三角形）からの距離 − r」。"""
    if not c.n_vox:
        return None
    r = c.size * 0.5
    cen = c.voxel_centers()
    near = np.linalg.norm(cen - p, axis=1) < max_dist + c.size * 1.8
    if not near.any():
        return None
    topo = c.topo
    best = (np.inf, None, -1)
    pts = np.nonzero(near)[0]
    d = np.linalg.norm(cen[pts] - p, axis=1)
    k = int(np.argmin(d))
    best = (d[k], cen[pts[k]], int(pts[k]))
    segs = topo["segs"]
    if len(segs):
        sel = near[segs[:, 0]] | near[segs[:, 1]]
        if sel.any():
            S = segs[sel]
            d, q = dist_segments(p[None], cen[S[:, 0]], cen[S[:, 1]])
            k = int(np.argmin(d))
            if d[k] < best[0]:
                best = (d[k], q[k], int(S[k, 0]))
    tris = topo["tris"]
    if len(tris):
        sel = near[tris[:, 0]] | near[tris[:, 1]] | near[tris[:, 2]]
        if sel.any():
            T = tris[sel]
            d, q = dist_triangles(p[None], cen[T[:, 0]], cen[T[:, 1]], cen[T[:, 2]])
            k = int(np.argmin(d))
            if d[k] < best[0]:
                best = (d[k], q[k], int(T[k, 0]))
    return best[0] - r, best[1], best[2]


def collision_proxies(c):
    """地面などとの当たり判定に使う球（半径 r）: 各ボクセル中心・結合の中点・三角形の重心。
    戻り値: (P,) 個の重み行列（P×ボクセル数）"""
    from scipy.sparse import csr_matrix
    n = c.n_vox
    rows, cols, vals = [], [], []
    pid = 0
    for i in range(n):
        rows.append(pid); cols.append(i); vals.append(1.0)
        pid += 1
    for a, b in c.topo["segs"]:
        rows += [pid, pid]; cols += [int(a), int(b)]; vals += [0.5, 0.5]
        pid += 1
    for t in c.topo["tris"]:
        rows += [pid] * 3; cols += [int(x) for x in t]; vals += [1 / 3] * 3
        pid += 1
    return csr_matrix((vals, (rows, cols)), shape=(pid, n))
