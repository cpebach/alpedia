"""ブロックで建物・船・飾りを組み立てる道具（シナリオ・場所の MOD が使う）。

Kit にセル（整数の (x, y, z)。+Y が上、+Z が前）ごとに素材・機器を置いていき、voxels() で構築物のボクセルの
リスト [(セル, 素材, 機器)] にする。あとから置いた物が前の物を上書きする。clear で消す。

- 形：fill（詰まった箱）・shell（壁・床・天井だけの箱）・dome（半球）・ellipsoid（だ円体）・cylinder（円柱）
- 開口：door（壁に穴）・windows（壁の一部をガラスに）
- 家具：desk・chair・table・sofa・bed・shelf・console（表示つきのコンピューター）・planter・crates・lamp_post など
- anchored(V)：足もとの下にアンカーを足す（動かない物。sites.anchored と同じ規則）
- spawn(world, V, ...)：地図の場所・ワールドの姿勢に置く。ensure(world, key, items)：無ければ作る（MOD 用）
"""
import numpy as np

from .construct import Construct

ANCHOR = {"def": "anchor", "dir": 2, "name": "アンカー"}
_N6 = ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))
# 機器の向き（dir）：+X 0, -X 1, +Y 2, -Y 3, +Z 4, -Z 5
FACING = {"+x": 0, "-x": 1, "+y": 2, "-y": 3, "+z": 4, "-z": 5}


class Kit:
    def __init__(self):
        self.v = {}

    # --- 基本 ------------------------------------------------------------
    def put(self, x, y, z, mat, dev=None):
        self.v[(int(x), int(y), int(z))] = (mat, dict(dev) if dev else None)
        return self

    def get(self, x, y, z):
        return self.v.get((int(x), int(y), int(z)))

    def clear(self, x0, y0, z0, x1, y1, z1):
        for c in [c for c in self.v if _inside(c, x0, y0, z0, x1, y1, z1)]:
            del self.v[c]
        return self

    def fill(self, x0, y0, z0, x1, y1, z1, mat):
        """両端を含む箱を詰める。"""
        for x in _rng(x0, x1):
            for y in _rng(y0, y1):
                for z in _rng(z0, z1):
                    self.v[(x, y, z)] = (mat, None)
        return self

    def shell(self, x0, y0, z0, x1, y1, z1, wall, floor=None, roof=None):
        """壁・床・天井だけの箱（中は空き）。floor・roof が None なら壁の素材、False なら作らない。"""
        for x in _rng(x0, x1):
            for y in _rng(y0, y1):
                for z in _rng(z0, z1):
                    side = x in (x0, x1) or z in (z0, z1)
                    if y == y0 and floor is not False:
                        self.v[(x, y, z)] = (floor or wall, None)
                    elif y == y1 and roof is not False:
                        self.v[(x, y, z)] = (roof or wall, None)
                    elif side and y0 <= y <= y1:
                        self.v[(x, y, z)] = (wall, None)
        return self

    def paint(self, mat, x0, y0, z0, x1, y1, z1, only=None):
        """範囲の中の今あるブロックの素材を替える（only：その素材のものだけ）。"""
        for c, (m, d) in list(self.v.items()):
            if _inside(c, x0, y0, z0, x1, y1, z1) and not d and (only is None or m == only):
                self.v[c] = (mat, None)
        return self

    def door(self, x0, y0, z0, x1, y1, z1):
        """壁に出入り口（範囲を空ける）。"""
        return self.clear(x0, y0, z0, x1, y1, z1)

    def windows(self, x0, y0, z0, x1, y1, z1, mat="glass"):
        """範囲の壁をガラスに。"""
        return self.paint(mat, x0, y0, z0, x1, y1, z1)

    def dome(self, cx, cy, cz, r, mat, thick=1.0, cut_floor=True):
        """半球の殻（中心 (cx, cy, cz)、半径 r セル）。"""
        R = int(np.ceil(r))
        for x in range(-R, R + 1):
            for y in range(0, R + 1):
                for z in range(-R, R + 1):
                    d = np.sqrt(x * x + y * y + z * z)
                    if r - thick <= d <= r:
                        self.v[(cx + x, cy + y, cz + z)] = (mat, None)
        return self

    def ellipsoid(self, cx, cy, cz, a, b, c, mat, thick=1.2, solid=False):
        """だ円体（半径 a・b・c セル。X・Y・Z の向き）。殻だけ（solid で詰める）。"""
        A, Bn, C = int(np.ceil(a)), int(np.ceil(b)), int(np.ceil(c))
        for x in range(-A, A + 1):
            for y in range(-Bn, Bn + 1):
                for z in range(-C, C + 1):
                    q = (x / a) ** 2 + (y / b) ** 2 + (z / c) ** 2
                    if q > 1.0:
                        continue
                    if not solid:
                        s = min(a, b, c)
                        inner = (x / max(a - thick, 0.5)) ** 2 + (y / max(b - thick, 0.5)) ** 2 + \
                            (z / max(c - thick, 0.5)) ** 2
                        if inner <= 1.0 and s > thick:
                            continue
                    self.v[(cx + x, cy + y, cz + z)] = (mat, None)
        return self

    def cylinder(self, cx, y0, cz, r, h, mat, hollow=False):
        R = int(np.ceil(r))
        for x in range(-R, R + 1):
            for z in range(-R, R + 1):
                d = np.hypot(x, z)
                if d > r or (hollow and d < r - 1.0):
                    continue
                for y in range(y0, y0 + h):
                    self.v[(cx + x, y, cz + z)] = (mat, None)
        return self

    def merge(self, other, dx=0, dy=0, dz=0):
        for (x, y, z), md in other.v.items():
            self.v[(x + dx, y + dy, z + dz)] = md
        return self

    def voxels(self):
        return [(c, m, dict(d) if d else None) for c, (m, d) in sorted(self.v.items())]

    def __len__(self):
        return len(self.v)

    # --- 家具（向き：f は前の向き "+z"・"-z"・"+x"・"-x"） ---------------------
    def desk(self, x, y, z, f="+z", mat="wood", w=3):
        """机：天板（横 w）と両端の脚。"""
        ax, _ = _axes(f)
        for k in range(w):
            p = _add((x, y, z), ax, k)
            self.put(p[0], p[1] + 1, p[2], mat)
        for k in (0, w - 1):
            p = _add((x, y, z), ax, k)
            self.put(*p, mat)
        return self

    def chair(self, x, y, z, f="+z", mat="hull_dark"):
        """椅子：座面と背もたれ（f の向きに座る）。"""
        _, fw = _axes(f)
        self.put(x, y, z, mat)
        b = _add((x, y, z), fw, -1)
        self.put(b[0], y + 1, b[2], mat)
        return self

    def table(self, x, y, z, w=2, d=2, mat="wood"):
        for i in range(w):
            for j in range(d):
                self.put(x + i, y + 1, z + j, mat)
        self.put(x, y, z, mat)
        self.put(x + w - 1, y, z + d - 1, mat)
        return self

    def sofa(self, x, y, z, f="+z", w=3, mat="carpet_red"):
        ax, fw = _axes(f)
        for k in range(w):
            p = _add((x, y, z), ax, k)
            self.put(*p, mat)
            b = _add(p, fw, -1)
            self.put(b[0], y, b[2], mat)
            self.put(b[0], y + 1, b[2], mat)
        return self

    def bed(self, x, y, z, f="+z", mat="cloth_white", frame="wood"):
        """ベッド：1×3 の敷き布団と枕元の板。"""
        _, fw = _axes(f)
        for k in range(3):
            p = _add((x, y, z), fw, k)
            self.put(*p, mat)
        h = _add((x, y, z), fw, -1)
        self.put(*h, frame)
        self.put(h[0], y + 1, h[2], frame)
        return self

    def shelf(self, x, y, z, f="+z", w=3, h=4, mat="wood", goods=("hull_red", "hull_blue", "hull_yellow", "cloth_white")):
        """棚：背板と、段ごとに品物。"""
        ax, _ = _axes(f)
        for k in range(w):
            p = _add((x, y, z), ax, k)
            for j in range(h):
                self.put(p[0], y + j, p[2], mat if j % 2 == 0 else goods[(k + j) % len(goods)])
        return self

    def console(self, x, y, z, f="+z", text="", mat="hull_dark", width=1.2, color=(0.6, 1.0, 0.8)):
        """操作盤：台と、表示つきのコンピューター（f の向きに表示）。"""
        self.put(x, y, z, mat)
        st = {"display": {"text": text, "face": FACING[f], "width": width, "color": list(color),
                          "background": [0.02, 0.05, 0.06, 0.92]}} if text else {}
        self.put(x, y + 1, z, "steel", {"def": "computer", "dir": FACING[f], "name": "操作盤", "state": st})
        return self

    def planter(self, x, y, z, w=2, d=1, h=2):
        """植木鉢・畑：土の箱と、その上の植物。"""
        for i in range(w):
            for j in range(d):
                self.put(x + i, y, z + j, "soil")
                for k in range(1, h):
                    self.put(x + i, y + k, z + j, "plant")
        return self

    def crates(self, x, y, z, n=3, mat="wood"):
        spots = ((0, 0, 0), (1, 0, 0), (0, 0, 1), (0, 1, 0), (1, 0, 1))[:n]
        for dx, dy, dz in spots:
            self.put(x + dx, y + dy, z + dz, mat)
        return self

    def light(self, x, y, z, color=None, group=None, blink=None, on=True, name="明かり"):
        """ライト（電力で光るブロック）。color：光の色、group：まとめて点ける名前、blink：点滅の周期（秒）。
        同じ構築物にバッテリーが要る（battery で置く）。"""
        st = {"enabled": bool(on)}
        if color is not None:
            st["color"] = [float(v) for v in color]
        if group:
            st["group"] = group
        if blink:
            st["blink"] = float(blink)
        return self.put(x, y, z, "glass", {"def": "light", "dir": 2, "name": name, "state": st})

    def battery(self, x, y, z, charge=2.0e7):
        """明かりなどの電源（満タンのバッテリー）。"""
        return self.put(x, y, z, "steel", {"def": "battery", "dir": 2, "name": "バッテリー",
                                           "state": {"charge": float(charge)}})

    def rug(self, x0, y, z0, x1, z1, mat="carpet_red"):
        return self.fill(x0, y, z0, x1, y, z1, mat)


def _rng(a, b):
    return range(int(min(a, b)), int(max(a, b)) + 1)


def _inside(c, x0, y0, z0, x1, y1, z1):
    return min(x0, x1) <= c[0] <= max(x0, x1) and min(y0, y1) <= c[1] <= max(y0, y1) and \
        min(z0, z1) <= c[2] <= max(z0, z1)


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
def root_cells(V):
    """アクチュエーターに動かされない部分（どのアクチュエーターの出力の先でもない部分）のセルの集合。"""
    cells = {tuple(c) for c, _, _ in V}
    outs = {}
    for c, _, d in V:
        if d and d.get("def") in ("actuator", "piston", "hinge"):
            n = _N6[int(d.get("dir", 2))]
            outs[tuple(c)] = (c[0] + n[0], c[1] + n[1], c[2] + n[2])
    cut = {frozenset((a, b)) for a, b in outs.items()}
    parent = {c: c for c in cells}

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x
    for c in cells:
        for n in _N6:
            nb = (c[0] + n[0], c[1] + n[1], c[2] + n[2])
            if nb in cells and frozenset((c, nb)) not in cut:
                parent[find(c)] = find(nb)
    driven = {find(b) for b in outs.values() if b in cells}
    return {c for c in cells if find(c) not in driven}


def anchor_cells(V, step=2):
    """V の足もと（いちばん下の段で、アクチュエーターに動かされない部分）の下に置くアンカーのセル。
    step ごと（と四隅にいちばん近い所）に1つ。"""
    root = root_cells(V)
    if not root:
        return []
    y0 = min(c[1] for c in root)
    foot = sorted(c for c in root if c[1] == y0)
    xs, zs = [c[0] for c in foot], [c[2] for c in foot]
    pick = {c for c in foot if (c[0] - min(xs)) % step == 0 and (c[2] - min(zs)) % step == 0}
    for cx in (min(xs), max(xs)):
        for cz in (min(zs), max(zs)):
            pick.add(min(foot, key=lambda c: (c[0] - cx) ** 2 + (c[2] - cz) ** 2))
    return [(x, y - 1, z) for x, y, z in sorted(pick)]


def anchored(V, step=2):
    """V にアンカーを足したもの（足もとの下に埋める）。"""
    return list(V) + [(c, "steel", dict(ANCHOR)) for c in anchor_cells(V, step)]


def anchored_inside(V, cells):
    """V の中のセル cells をアンカーに置き換えたもの（宇宙の構築物など、下に地面がないとき）。"""
    cells = {tuple(c) for c in cells}
    return [((c, "steel", dict(ANCHOR)) if tuple(c) in cells and not d else (c, m, d)) for c, m, d in V]


def anchor_center(V):
    """真ん中あたりの、機器の無いブロック1つをアンカーに（宇宙に浮かぶ船など）。"""
    cells = [c for c, _, d in V if not d]
    if not cells:
        return list(V)
    P = np.asarray(cells, float)
    k = int(np.argmin(np.linalg.norm(P - P.mean(0), axis=1)))
    return anchored_inside(V, [cells[k]])


# ---------------------------------------------------------------------------
# 置く
# ---------------------------------------------------------------------------
def grounded(V, R, t, size, sdf, mat=None, step=2):
    """足もとと地面のすき間を土台のブロックで埋め、その下にアンカーを置き直した V（坂に置いても浮かない）。
    R, t：格子の座標（セル × 大きさ）→ ワールド。sdf(P)：点の地面からの高さ（中は負）。
    mat：土台の素材（None なら真上のブロックと同じ）。"""
    base = [(c, m, d) for c, m, d in V if not (d and d.get("def") == "anchor")]
    root = root_cells(base)
    if not root:
        return list(V)
    y0 = min(c[1] for c in root)
    mats = {tuple(c): m for c, m, d in base}
    foot = sorted(c for c in root if c[1] == y0)
    P = np.array([R @ ((np.array([x + 0.5, y0, z + 0.5])) * size) + t for x, _, z in foot])
    h = np.asarray(sdf(P), float)
    out = list(base)
    deep = {}
    for (x, _, z), hh in zip(foot, h):
        n = int(np.ceil(max(hh, 0.0) / size - 0.15))
        for k in range(1, n + 1):
            out.append(((x, y0 - k, z), mat or mats.get((x, y0, z), "concrete"), None))
        deep[(x, z)] = y0 - n
    xs, zs = [c[0] for c in foot], [c[2] for c in foot]
    pick = {(x, z) for x, _, z in foot if (x - min(xs)) % step == 0 and (z - min(zs)) % step == 0}
    for cx in (min(xs), max(xs)):
        for cz in (min(zs), max(zs)):
            pick.add(min(((x, z) for x, _, z in foot), key=lambda q: (q[0] - cx) ** 2 + (q[1] - cz) ** 2))
    for x, z in sorted(pick):
        out.append(((x, deep[(x, z)] - 1, z), "steel", dict(ANCHOR)))
    return out


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
def light_devices(c, group=None):
    """構築物 c のライト（group が名前ならそのまとまりだけ）の機器のリスト。"""
    cache = getattr(c, "_lgrp_cache", None)
    if cache is None or cache[0] != (c.version, c.n_vox):
        ids = [i for i, d in enumerate(c.devs) if d and (c.reg.device(d["def"]) or {}).get("type") == "light"]
        cache = c._lgrp_cache = ((c.version, c.n_vox), ids)
    out = [c.devs[i] for i in cache[1]]
    return [d for d in out if group is None or d["state"].get("group") == group]


def set_light_group(world, group, on):
    """まとまり group のライトを全部点ける・消す（物語の「基地の明かり」・アマツの非常灯など）。"""
    n = 0
    for c in world.constructs.values():
        for d in light_devices(c, group):
            if bool(d["state"].get("enabled", True)) != bool(on):
                d["state"]["enabled"] = bool(on)
                n += 1
    return n
