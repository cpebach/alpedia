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

from . import native
from .construct import CORNER_BITS


def marching_cubes(vol, level=0.0, spacing=(1.0, 1.0, 1.0)):
    """等値面（Rust の Marching Cubes）：(頂点 (n,3)、三角形 (m,3)、None、None)。頂点は配列の番号 × spacing、
    値が level 以下の所を中とし、三角形は外向き（前の scikit-image と同じ形の戻り値）。"""
    v, f = native.require().marching_cubes(np.ascontiguousarray(vol, dtype=np.float32), float(level),
                                           tuple(float(x) for x in spacing))
    return v, f, None, None


class Csr:
    """疎な行列（行ごとに、列の番号と値）。前の scipy.sparse.csr_matrix の代わりに、当たり判定の点の重みに使う。"""

    def __init__(self, rows, cols, vals, shape):
        rows, cols, vals = np.asarray(rows, np.int64), np.asarray(cols, np.int64), np.asarray(vals, np.float64)
        order = np.lexsort((cols, rows))
        self.indices, self.data, r = cols[order], vals[order], rows[order]
        self.indptr = np.zeros(shape[0] + 1, np.int64)
        np.add.at(self.indptr, r + 1, 1)
        self.indptr = np.cumsum(self.indptr)
        self.rows = r
        self.shape = shape

    def tocsr(self):
        return self

    def sort_indices(self):
        pass

    def __matmul__(self, x):
        x = np.asarray(x, dtype=np.float64)
        prod = self.data.reshape((-1,) + (1,) * (x.ndim - 1)) * x[self.indices]
        out = np.zeros((self.shape[0],) + x.shape[1:], np.float64)
        np.add.at(out, self.rows, prod)
        return out

OFFS8 = [tuple(int(x) for x in o) for o in itertools.product((0, 1), repeat=3)]
_pattern_cache = {}
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
            tris = [tuple(int(x) for x in s) for s in native.require().convex_hull(np.ascontiguousarray(P))]
    res = [tuple(key[a] for a in t) for t in tris]
    _pattern_cache[key] = res
    return res


def block_kinds(c):
    """ブロックの種類の番号（素材ごと。機器は機器の種類ごと）。階段状の隙間を埋めるのは同じ種類どうしだけ。"""
    names = {}
    out = np.zeros(c.n_vox, np.int64)
    for i, (m, d) in enumerate(zip(c.mats, c.devs)):
        key = ("dev", d["def"]) if d else ("mat", m)
        out[i] = names.setdefault(key, len(names))
    return out


_HULL_TABLE = []


def _hull_table():
    """2×2×2 の窓の中の結合成分（OFFS8 の番号のビット）→ 三角形（OFFS8 の番号）の表（Rust の topology に渡す）。"""
    if not _HULL_TABLE:
        start, tris = [0], []
        for bits in range(256):
            offs = [OFFS8[k] for k in range(8) if bits >> k & 1]
            if len(offs) >= 3:
                for t in _hull_tris(offs):
                    tris.append([OFFS8.index(o) for o in t])
            start.append(len(tris))
        _HULL_TABLE.append((np.array(start, np.int64), np.array(tris, np.int64).reshape(-1, 3)))
    return _HULL_TABLE[0]


def topology(c):
    """形を作る図形の一覧を返す: dict(points, segs, tris)（ボクセル番号の配列）。Rust（alpedia_core.topology）で作る。
    階段状・L 字の内側を埋める斜面（三角形）は、同じ種類のブロックどうしだけで作る
    （違う種類どうし（内装と外装など）は階段状のまま）。"""
    idx = c.index

    def pairs(keys):
        out = [(idx.get(a), idx.get(b)) for a, b in keys]
        return np.array([p for p in out if p[0] is not None and p[1] is not None], np.int64).reshape(-1, 2)
    hs, ht = _hull_table()
    pts, segs, tris = native.require().topology(np.asarray(c.cells, np.int64).reshape(-1, 3), pairs(c.links),
                                                pairs(getattr(c, "extras", {})), block_kinds(c), hs, ht)
    return {"points": pts, "segs": segs, "tris": tris}


# ---------------------------------------------------------------------------
# かたい素材の角ばった見た目（多面体）
# ---------------------------------------------------------------------------
# 素材の "facet": true（鋼鉄・岩など）は、芯からの距離を球ではなく多面体（面・辺・角を落とした立方体＝
# 26 面の菱形立方八面体に近い形）で測る。芯（点・線・三角形）は同じなので、直線や階段状に並べたときの
# つなぎ目・斜面の埋まり方は今までどおり。当たり判定は今までどおり丸い形のまま（坂で転がる）。
FACET_SCALE = 0.97          # 面の位置（丸い形の半径に対して）。角が少しだけ外へ出て、面は少し内側（Rust の topo.rs と同じ）


def facet_voxels(c):
    """ボクセルごとの「角ばった見た目にするか」（素材の facet）。"""
    reg = c.reg
    cache = {}
    out = np.zeros(c.n_vox, bool)
    for i, m in enumerate(c.mats):
        if m not in cache:
            cache[m] = bool((reg.material(m) or {}).get("facet", False))
        out[i] = cache[m]
    return out


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
    m = 2 * k + 3
    hard = facet_voxels(c)

    def shape_of(points, segs, tris, extra_segs=None):
        """芯（点・線・三角形）からの距離 − r の格子（Rust の shape_sdf）。"""
        sp = centers[segs] if len(segs) else np.zeros((0, 2, 3))
        sf = hard[segs].all(1) if len(segs) else np.zeros(0, bool)
        if extra_segs is not None and len(extra_segs):
            sp = np.concatenate([sp, extra_segs])
            sf = np.concatenate([sf, np.zeros(len(extra_segs), bool)])
        f64 = lambda a: np.ascontiguousarray(a, dtype=np.float64)  # noqa: E731
        b8 = lambda a: np.ascontiguousarray(a, dtype=bool)  # noqa: E731
        return native.require().shape_sdf(
            tuple(int(x) for x in dims), tuple(float(x) for x in lo), float(k), float(r), int(m),
            f64(centers[points].reshape(-1, 3)), b8(hard[points] if len(points) else np.zeros(0, bool)),
            f64(sp.reshape(-1, 2, 3)), b8(sf),
            f64(centers[tris].reshape(-1, 3, 3) if len(tris) else np.zeros((0, 3, 3))),
            b8(hard[tris].all(1) if len(tris) else np.zeros(0, bool)))

    points, segs, tris = topo["points"], topo["segs"], topo["tris"]
    gid = c._art_groups() if getattr(c, "rotors", None) and hasattr(c, "_art_groups") else None
    if gid is None or gid.max() == 0:
        vol = shape_of(points, segs, tris).reshape(dims)
        if vol.min() >= 0:
            return None
        parts = [(vol, None)]
    else:
        # 回転ジョイント（アクチュエーター）で別に動く部分ごとに、別の形・別のメッシュにする。
        # 1つの形のままだと、出力側が回ったときに境目が引き伸ばされてねじれて見えるため。
        # 部分ごとの形は、その部分のボクセルだけの芯から作る（部分どうしを埋める形は作らない）。
        # アクチュエーターと出力をつなぐ線分は真ん中（2つのセルの境の面）で半分ずつに分ける。
        rotor_pairs = {}
        for r_ in c.rotors:
            rotor_pairs[tuple(sorted((r_["a"], r_["b"])))] = True
        G = int(gid.max()) + 1
        vols = []
        for g_ in range(G):
            pts = points[gid[points] == g_] if len(points) else points
            sg = segs[(gid[segs[:, 0]] == g_) & (gid[segs[:, 1]] == g_)] if len(segs) else segs
            tr = tris[(gid[tris[:, 0]] == g_) & (gid[tris[:, 1]] == g_) & (gid[tris[:, 2]] == g_)] if len(tris) else tris
            half = []
            for a_, b_ in rotor_pairs:
                for x, y in ((a_, b_), (b_, a_)):
                    if gid[x] == g_ and gid[y] != g_:
                        half.append([centers[x], (centers[x] + centers[y]) * 0.5])
            vols.append(shape_of(pts, sg, tr, np.asarray(half) if half else None))
        V = np.stack(vols)                                     # (G, N)
        union = V.min(0)
        if union.min() >= 0:
            return None
        lab = V.argmin(0)
        parts = []
        for g_ in range(G):
            # ほかの部分の中は外側として扱う（境の切り口が両側で同じ位置になるように符号だけ反転）
            # （ちょうど 0 の点は marching_cubes が中とみなすので、ほんの少し外へ）
            v = np.where(lab == g_, union, np.maximum(np.abs(union), 1e-4))
            parts.append((v.reshape(dims), gid == g_))
    P, F, OW = [], [], []
    off = 0
    for vol_g, allowed in parts:
        if vol_g.min() >= 0:
            continue
        verts, faces, _, _ = marching_cubes(vol_g, level=0.0)
        if not len(faces):
            continue
        p = lo + verts / k                                     # セル単位
        # 面の向きをそろえる（外向き）：閉じた面の符号付き体積が正になる向き
        # （以前は marching_cubes の法線に合わせていたが、それは内向きで、上の面が暗く描かれていた）
        a, b, c3 = p[faces[:, 0]], p[faces[:, 1]], p[faces[:, 2]]
        if np.einsum("ij,ij->", a - p.mean(0), np.cross(b - a, c3 - a)) < 0:
            faces = faces[:, ::-1]
        P.append(p)
        F.append(faces + off)
        OW.append(_owner(c, p, centers, allowed))
        off += len(p)
    if not P:
        return None
    p = np.concatenate(P)
    faces = np.concatenate(F)
    # 各頂点をボクセルに割り当て、そのボクセルの8頂点で補間（スキニング）
    owner = np.concatenate(OW)
    u = p - cells[owner]                                       # ボクセル内の位置 0〜1（外へはみ出すこともある）
    W = np.ones((len(p), 8))
    for k8 in range(8):
        bits = CORNER_BITS[k8]
        for ax in range(3):
            W[:, k8] *= np.where(bits[ax] > 0, u[:, ax], 1 - u[:, ax])
    return {"rest": p, "faces": faces.astype(np.int64), "owner": owner, "w": W,
            "vidx": (8 * owner[:, None] + np.arange(8)[None]).astype(np.int64), "version": c.version}


# ---------------------------------------------------------------------------
# 裏での読み込み：大きい構築物の形のメッシュは別のスレッドで作る（作り終わるまでは遠景の箱で見せる）
# ---------------------------------------------------------------------------
_pool = None
_jobs = {}


def cached_mesh(c):
    """作ってあるメッシュ（形が変わっていなければ）。無ければ None。"""
    m = getattr(c, "_mesh_cache", None)
    return m[1] if m is not None and m[0] == (c.version, c.n_vox) else None


def mesh_ready(c):
    return getattr(c, "_mesh_cache", (None,))[0] == (c.version, c.n_vox)


def request_mesh(c):
    """メッシュを裏で作り始める（もう作ってある・作っている途中なら何もしない）。できていれば True。"""
    global _pool
    key = (c.version, c.n_vox)
    if mesh_ready(c):
        return True
    job = _jobs.get(c.id)
    if job is not None and job[0] == key:
        if job[1].done():
            _jobs.pop(c.id, None)
            try:
                c._mesh_cache = (key, job[1].result())
            except Exception:  # noqa: BLE001
                return False                       # 作っている間に形が変わったなど：次に頼まれたら作り直す
            return True
        return False
    if _pool is None:
        from concurrent.futures import ThreadPoolExecutor
        _pool = ThreadPoolExecutor(max_workers=2, thread_name_prefix="mesh")
    _jobs[c.id] = (key, _pool.submit(build_mesh, c))
    return False


def mesh_for(c):
    """描画用のメッシュ：作ってあればそれを、無ければ今作る。"""
    if mesh_ready(c):
        return cached_mesh(c)
    m = build_mesh(c)
    c._mesh_cache = ((c.version, c.n_vox), m)
    return m


def _owner(c, p, centers, allowed=None):
    """点（セル単位）が属するボクセル：そのセルのボクセル、無ければいちばん近いボクセル。
    allowed（ボクセルごとの bool）があれば、その中から選ぶ。"""
    cell = np.floor(p).astype(np.int64)
    vc = np.asarray(c.cells, dtype=np.int64)
    lo = np.minimum(vc.min(0), cell.min(0)) if len(cell) else vc.min(0)
    span = (np.maximum(vc.max(0), cell.max(0) if len(cell) else vc.max(0)) - lo + 1)
    enc = lambda a: ((a[:, 0] - lo[0]) * span[1] + (a[:, 1] - lo[1])) * span[2] + (a[:, 2] - lo[2])  # noqa: E731
    keys = enc(vc)
    order = np.argsort(keys)
    sk = keys[order]
    q = enc(cell)
    pos = np.clip(np.searchsorted(sk, q), 0, len(sk) - 1)
    hit = sk[pos] == q
    owner = np.where(hit, order[pos], -1).astype(np.int64)
    if allowed is not None:
        owner[(owner >= 0) & ~allowed[np.maximum(owner, 0)]] = -1
    miss = owner < 0
    if miss.any():
        cand = np.arange(len(centers)) if allowed is None else np.nonzero(allowed)[0]
        nn = native.require().nearest(np.ascontiguousarray(centers[cand], dtype=np.float64),
                                      np.ascontiguousarray(p[miss], dtype=np.float64))
        owner[miss] = cand[nn]
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
    """点 p から形（表面）への最短：(距離, 最近点, 近いボクセル番号) または None（Rust）。
    距離は「芯（点・線・三角形）からの距離 − r」。"""
    if not c.n_vox:
        return None
    cen = c.centers_cached() if hasattr(c, "centers_cached") else c.voxel_centers()
    topo = c.topo
    res = native.require().closest_on_shape(
        np.ascontiguousarray(cen, dtype=np.float64), np.asarray(topo["segs"], np.int64).reshape(-1, 2),
        np.asarray(topo["tris"], np.int64).reshape(-1, 3), [float(x) for x in p], float(max_dist), float(c.size))
    if res is None:
        return None
    d, q, vi = res
    return d, np.asarray(q), int(vi)


def collision_proxies(c):
    """地面などとの当たり判定に使う球（半径 r）: 各ボクセル中心・結合の中点・三角形の重心。
    戻り値: (P,) 個の重み行列（P×ボクセル数）"""
    n = c.n_vox
    S = np.asarray(c.topo["segs"], np.int64).reshape(-1, 2)
    T = np.asarray(c.topo["tris"], np.int64).reshape(-1, 3)
    ns, nt = len(S), len(T)
    rows = np.concatenate([np.arange(n), np.repeat(n + np.arange(ns), 2), np.repeat(n + ns + np.arange(nt), 3)])
    cols = np.concatenate([np.arange(n), S.ravel(), T.ravel()])
    vals = np.concatenate([np.ones(n), np.full(2 * ns, 0.5), np.full(3 * nt, 1 / 3)])
    return Csr(rows, cols, vals, (n + ns + nt, n))
