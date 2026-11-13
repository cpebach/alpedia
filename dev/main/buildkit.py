"""ブロックで建物・船・飾りを組み立てる道具（シナリオ・場所の MOD が使う）。Kit・アンカー・土台・明かりは Rust（game/buildkit.rs）。

Kit にセル（整数の (x, y, z)。+Y が上、+Z が前）ごとに素材・機器を置いていき、voxels() で構築物のボクセルの
リスト [(セル, 素材, 機器)] にする。あとから置いた物が前の物を上書きする。clear で消す。

- 形：fill（詰まった箱）・shell（壁・床・天井だけの箱）・dome（半球）・ellipsoid（だ円体）・cylinder（円柱）
- 開口：door（壁に穴）・windows（壁の一部をガラスに）
- 家具：desk・chair・table・sofa・bed・shelf・console（表示つきのコンピューター）・planter・crates・lamp_post など
- anchored(V)：足もとの下にアンカーを足す（動かない物。sites.anchored と同じ規則）
- spawn(world, V, ...)：地図の場所・ワールドの姿勢に置く。ensure(world, key, items)：無ければ作る（MOD 用）
"""
import numpy as np

from . import native
from .construct import Construct

ANCHOR = {"def": "anchor", "dir": 2, "name": "アンカー"}
_N6 = ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))
# 機器の向き（dir）：+X 0, -X 1, +Y 2, -Y 3, +Z 4, -Z 5
FACING = {"+x": 0, "-x": 1, "+y": 2, "-y": 3, "+z": 4, "-z": 5}


_C = native.require()
Kit = _C.Kit                             # セルごとに素材・機器を置いていく（v：セル → (素材, 機器)）


def _rng(a, b):
    return range(int(min(a, b)), int(max(a, b)) + 1)


def _axes(f):
    """前の向き f → (横の向き, 前の向き)（セルの差）。"""
    fw = {"+z": (0, 0, 1), "-z": (0, 0, -1), "+x": (1, 0, 0), "-x": (-1, 0, 0)}[f]
    ax = (fw[2], 0, -fw[0]) if fw[0] else (1 if fw[2] > 0 else -1, 0, 0)
    return ax, fw


def _add(p, d, k):
    return (p[0] + d[0] * k, p[1] + d[1] * k, p[2] + d[2] * k)


# ---------------------------------------------------------------------------
# アンカー
# ---------------------------------------------------------------------------
root_cells = _C.bk_root_cells            # アクチュエーターに動かされない部分のセルの集合
anchor_cells = _C.bk_anchor_cells        # 足もとの下に置くアンカーのセル
anchored = _C.bk_anchored                # V にアンカーを足したもの（足もとの下に埋める）
anchored_inside = _C.bk_anchored_inside  # V の中のセル cells をアンカーに置き換えたもの
anchor_center = _C.bk_anchor_center      # 真ん中あたりの、機器の無いブロック1つをアンカーに


# ---------------------------------------------------------------------------
# 置く
# ---------------------------------------------------------------------------
def grounded(V, R, t, size, sdf, mat=None, step=2):
    """足もとと地面のすき間を土台のブロックで埋め、その下にアンカーを置き直した V（坂に置いても浮かない）。
    R, t：格子の座標（セル × 大きさ）→ ワールド。sdf(P)：点の地面からの高さ（中は負）。
    mat：土台の素材（None なら真上のブロックと同じ）。"""
    return _C.bk_grounded(V, np.ascontiguousarray(R, dtype=np.float64), np.ascontiguousarray(t, dtype=np.float64).ravel(),
                          float(size), sdf, mat, int(step))


def spawn(world, vox, p, R, size=0.5, name="建物", center=False, lift_sdf=None, id_=None):
    """ボクセルを置く。p：足もとの基準点（center なら足もとの真ん中）、R：向き（列が 右・上・前）。
    lift_sdf(P) があれば、アンカー以外のブロックがすべて外（> 0.03）に出るまで持ち上げる。"""
    if center:
        body = [cl for cl, _, d in vox if not (d and d.get("def") == "anchor")] or [cl for cl, _, _ in vox]
        y0 = min(cl[1] for cl in body)
        foot = np.array([cl for cl in body if cl[1] == y0], float)
        p = p - R @ (np.array([foot[:, 0].mean() + 0.5, y0, foot[:, 2].mean() + 0.5]) * size)
    c = Construct.from_voxels(world.registry, vox, p, R, size=size, name=name, id_=id_)
    if lift_sdf is not None:
        anc = c.anchor_index()
        rows = np.setdiff1d(np.arange(c.n_vert), (8 * anc[:, None] + np.arange(8)[None]).ravel()) if len(anc) \
            else np.arange(c.n_vert)
        for _ in range(8):
            dmin = float(lift_sdf(c.pos[rows]))
            if dmin >= 0.03:
                break
            c.pos += R[:, 1] * (0.03 - dmin + 0.01)
    world.add_construct(c)
    return c


def ensure(world, key, items):
    """MOD の建物：まだ無ければ作る（ワールドに1回だけ。壊されて無くなった物は作り直さない）。
    items：[(役, 作る関数 make() → 構築物)]。作った物の ID は world.settings["blocks"][key] に残す。"""
    if world.net is not None and getattr(world.net, "role", "") == "client":
        return {}
    reg = world.settings.setdefault("blocks", {}).setdefault(key, {})
    made = {}
    for role, make in items:
        if role in reg:
            continue
        try:
            c = make()
        except Exception:  # noqa: BLE001
            from . import log
            log.get("buildkit").exception("建物を作れません: %s/%s", key, role)
            continue
        if c is not None:
            reg[role] = c.id
            made[role] = c
    return made


def ground_fn(m, scenes=()):
    """地図 m の惑星の地面と、景色の箱（床・台）を合わせた「地面からの高さ」の関数。"""
    from .geom import box_sdf

    def h(P):
        P = np.asarray(P, float)
        d = m.pl.sdf(P)
        for sc in scenes:
            if sc.map is not None and sc.col.any() and np.linalg.norm(P[0] - sc.center) < sc.radius + 10:
                dd, _, _ = box_sdf(P, sc.C[sc.col], sc.A[sc.col], sc.E[sc.col])
                d = np.minimum(d, dd)
        return d
    return h


def spawn_on_map(world, vox, m, place, name, scenes=(), size=None, center=False, sink=False, id_=None,
                 foundation="auto"):
    """地図 m の場所 place = (x, z, 向き（度）, 高さ) に置く。地面と景色の箱にめり込まないように持ち上げ
    （アンカーは埋まっていてよい）、center なら足もとの真ん中を場所に合わせ、坂で浮いた所は土台で埋めて
    アンカーをその下に置き直す。sink：持ち上げない（地面に刺す物）。"""
    x, z, yaw, lift = place
    p, _ = m.ground(x, z, lift)
    R = m.frame(x, z, yaw)
    size = float(size or world.settings.get("voxel_size", 0.5))
    if center:
        body = [cl for cl, _, d in vox if not (d and d.get("def") == "anchor")] or [cl for cl, _, _ in vox]
        y0 = min(cl[1] for cl in body)
        foot = np.array([cl for cl in body if cl[1] == y0], float)
        p = p - R @ (np.array([foot[:, 0].mean() + 0.5, y0, foot[:, 2].mean() + 0.5]) * size)
    c = Construct.from_voxels(world.registry, vox, p, R, size=size, name=name, id_=id_)
    gh = ground_fn(m, scenes)
    anc = c.anchor_index()
    rows = np.setdiff1d(np.arange(c.n_vert), (8 * anc[:, None] + np.arange(8)[None]).ravel()) if len(anc) \
        else np.arange(c.n_vert)
    for _ in range(0 if sink else 8):
        dmin = float(gh(c.pos[rows]).min())
        if dmin >= 0.03:
            break
        c.pos += R[:, 1] * (0.03 - dmin + 0.01)
    if center and not sink and len(anc):
        Rl, tl = c.rigid_fit()
        body = [cl for cl, _, d in vox if not (d and d.get("def") == "anchor")]
        y0 = min(b[1] for b in body)
        wide = len({(cl[0], cl[2]) for cl in body if cl[1] == y0}) > 6
        fmat = ("concrete" if wide else None) if foundation == "auto" else foundation
        vox2 = grounded(vox, Rl, tl, size, gh, fmat, step=2)
        c = Construct.from_voxels(world.registry, vox2, tl, Rl, size=size, name=name, id_=id_)
    world.add_construct(c)
    return c


# ---------------------------------------------------------------------------
# 明かりのまとまり
# ---------------------------------------------------------------------------
light_devices = _C.bk_light_devices      # 構築物 c のライト（group が名前ならそのまとまりだけ）の機器のリスト
set_light_group = _C.bk_set_light_group  # まとまり group のライトを全部点ける・消す
