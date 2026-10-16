"""街パックの描画：街の建物（CityView）と NPC（NpcView）。"""
import numpy as np
from panda3d.core import SamplerState, Texture

from main.client.render import DynamicMesh, quad_tris, shade
from . import citylife


_window_tex = None


def window_texture():
    """ビルの壁の窓の模様（暗い壁にところどころ明かりの付いた窓）。4m × 4m で1枚。"""
    global _window_tex
    if _window_tex is not None:
        return _window_tex
    from PIL import Image
    rng = np.random.default_rng(7)
    S = 64
    img = np.zeros((S, S, 3), np.float32)
    img[:] = (0.13, 0.14, 0.18)
    cols = np.array([[1.0, 0.85, 0.5], [0.5, 0.95, 1.0], [1.0, 0.45, 0.9], [0.9, 0.9, 0.95]])
    for gy in range(4):
        for gx in range(4):
            if rng.random() < 0.45:
                c = cols[rng.integers(len(cols))] * rng.uniform(0.6, 1.0)
            else:
                c = np.array([0.05, 0.06, 0.09])
            x0, y0 = gx * 16 + 3, gy * 16 + 5
            img[y0:y0 + 8, x0:x0 + 10] = c

    from panda3d.core import PNMImage
    pnm = PNMImage(S, S)
    for y in range(S):
        for x in range(S):
            r, g, b = img[y, x]
            pnm.setXel(x, y, float(r), float(g), float(b))
    t = Texture("windows")
    t.load(pnm)
    t.setMagfilter(SamplerState.FT_nearest)
    t.setMinfilter(SamplerState.FT_linear_mipmap_linear)
    t.setWrapU(Texture.WM_repeat)
    t.setWrapV(Texture.WM_repeat)
    _window_tex = t
    return t


FRONT_CW = True   # 表面の回り順（描画系の座標の向きに合わせる）
WALL_UV = (0.23, 0.023)   # 窓の模様の画像の、壁の色の所（窓のない所）
_BOX_FACES = [  # (軸, 符号)
    (0, 1), (0, -1), (1, 1), (1, -1), (2, 1), (2, -1)]


def _box_quads(C, A, E):
    """箱 (B) → 各面の4頂点 (B,6,4,3)・法線 (B,6,3)・面の幅と高さ (B,6,2)"""
    B = len(C)
    V = np.zeros((B, 6, 4, 3))
    N = np.zeros((B, 6, 3))
    WH = np.zeros((B, 6, 2))
    for f, (ax, sg) in enumerate(_BOX_FACES):
        a1, a2 = (ax + 1) % 3, (ax + 2) % 3
        if ax != 1:
            a1, a2 = (2 if ax == 0 else 0), 1          # 側面は「横・縦」
        n = A[:, ax] * sg
        u = A[:, a1] * E[:, a1:a1 + 1]
        v = A[:, a2] * E[:, a2:a2 + 1]
        c = C + n * E[:, ax:ax + 1]
        V[:, f, 0] = c - u - v
        V[:, f, 1] = c + u - v
        V[:, f, 2] = c + u + v
        V[:, f, 3] = c - u + v
        # 外から見て同じ回り順にそろえる（片面描画で表側だけが描かれるように）
        cr = np.cross(V[:, f, 1] - V[:, f, 0], V[:, f, 2] - V[:, f, 0])
        flip = np.einsum("ij,ij->i", cr, n) < 0
        if FRONT_CW:
            flip = ~flip
        V[flip, f] = V[flip, f, ::-1]
        N[:, f] = n
        WH[:, f, 0] = 2 * E[:, a1]
        WH[:, f, 1] = 2 * E[:, a2]
    return V, N, WH


class CityView:
    """街1つの描画。2段階の細かさ（LOD）で描く。
    - 簡略（遠く）：建物の本体だけを1つの箱で。窓の模様・ネオン・看板・連絡通路なし。色に建物のネオン色を少し混ぜる。
    - 詳細（近く：city_detail_distance 以内）：窓の模様つきのビル＋ネオン・看板・アンテナ・連絡通路。
    どちらもメッシュは街のローカル座標で作って使い回し、毎フレームは位置と向き（行列）だけを変える。
    境界ボリュームを付けてあるので、画面の外や惑星の裏側の街は描かれない（視錐台カリング）。"""

    def __init__(self, parent, city):
        self.city = city
        self.parent = parent
        self.simple = None          # {"mesh", "item"}
        self.detail = None          # {"body", "neon", "items"}
        self.version = -1
        self.shade_R = None
        self.level = None           # None / "simple" / "detail"

    @staticmethod
    def _mesh(parent, name):
        m = DynamicMesh(parent, name)
        m.node.clearBounds()        # 頂点から境界を計算させる（カリングを効かせる）
        m.node.setFinal(False)
        m.np.setTwoSided(False)     # 箱の裏側（内側）は描かない（描く量がおよそ半分になる）
        return m

    # --- 作る ---------------------------------------------------------------
    def _item(self, mesh, V, N, col, uv, lit):
        return {"v": V.reshape(-1, 3), "n": np.repeat(N.reshape(-1, 3), 4, axis=0), "col": col, "uv": uv,
                "mesh": mesh, "lit": lit, "tris": quad_tris(len(V.reshape(-1, 3)) // 4), "new": True}

    def _build_simple(self):
        c = self.city
        C, E, live, st = c._geom()
        parts = np.nonzero(live & np.array([p["kind"] in ("body", "ground", "wall") for p in c.parts]))[0]
        if self.simple is None:
            self.simple = {"mesh": self._mesh(self.parent, f"citys{c.id}")}
        mesh = self.simple["mesh"]
        if not len(parts):
            mesh.set(np.zeros((0, 12), np.float32))
            self.simple["item"] = None
            return
        V, N, _ = _box_quads(C[parts], c.A0[parts], E[parts])
        # 下の面は見えないので省く（三角形を減らす）
        keep = [0, 1, 2, 4, 5]
        V, N = V[:, keep], N[:, keep]
        body = np.array([c.parts[k]["color"] for k in parts])
        # 建物ごとのネオン色（ネオンの部品から）を本体に少し混ぜて、遠くからでも街らしく見せる
        neon_of = {}
        for k, p_ in enumerate(c.parts):
            if p_["neon"] is not None and p_["bld"] >= 0 and p_["bld"] not in neon_of:
                neon_of[p_["bld"]] = p_["neon"]
        tint = np.array([neon_of.get(int(c.bld[k]), np.array([0.3, 0.3, 0.4])) for k in parts])
        col_b = body * 2.2 + tint * 0.18
        col_b[st[parts] == 1] = (0.3, 0.22, 0.2)
        gnd = np.array([c.parts[k]["kind"] in ("ground", "wall") for k in parts])
        col_b[gnd] = body[gnd] * 1.3
        tint[gnd] = body[gnd] * 1.4
        nf = len(keep)
        col = np.repeat(np.repeat(col_b, nf, axis=0), 4, axis=0)
        # 屋上はネオン色を強めに（上から見たときの目印）
        top = np.tile(np.repeat(np.arange(nf) == 2, 4), len(parts))
        col[top] = np.repeat(col_b * 0.4 + tint * 0.6, 4, axis=0)
        self.simple["item"] = self._item(mesh, V, N, col, np.zeros((len(V) * nf * 4, 2)), True)

    def _build_detail(self):
        c = self.city
        if self.detail is None:
            body = self._mesh(self.parent, f"city{c.id}")
            body.np.setTexture(window_texture(), 1)
            self.detail = {"body": body, "neon": self._mesh(self.parent, f"neon{c.id}"),
                           "plain": self._mesh(self.parent, f"cityp{c.id}")}    # 舗装・階段（模様なし）
        C, E, live, st = c._geom()
        parts = np.nonzero(live)[0]
        kinds = np.array([c.parts[k]["kind"] for k in parts])
        plain = np.isin(kinds, ["ground", "stair", "furn", "wall"])   # 模様なし（舗装・階段・家具・内装のある建物の壁）
        isbody = np.isin(kinds, ["body", "walk"])
        dmg = st[parts] == 1
        A = c.A0[parts]
        V, N, WH = _box_quads(C[parts], A, E[parts])
        items = []
        for mesh, m in ((self.detail["body"], isbody), (self.detail["plain"], plain), (self.detail["neon"], ~isbody & ~plain)):
            if not m.any():
                mesh.set(np.zeros((0, 12), np.float32))
                continue
            idx = np.nonzero(m)[0]
            cols = np.array([c.parts[parts[k]]["color"] for k in idx])
            if mesh is self.detail["plain"]:
                col = np.repeat(np.repeat(cols, 6, axis=0), 4, axis=0)
                items.append(self._item(mesh, V[idx], N[idx], col, np.zeros((len(idx) * 24, 2)), True))
            elif mesh is self.detail["body"]:
                tint = np.clip(cols / max(float(cols.max()), 1e-6), 0.4, 1.0) * 0.55 + 0.45
                tint[dmg[idx]] = (0.45, 0.32, 0.3)
                col = np.repeat(np.repeat(tint, 6, axis=0), 4, axis=0)
                wh = WH[idx].reshape(-1, 2) / 4.0
                quv = np.array([[0, 0], [1, 0], [1, 1], [0, 1]], float)
                uv = np.repeat(wh, 4, axis=0) * np.tile(quv, (len(wh), 1))
                # 屋根・床と、壊れた建物は窓なし（壁の色）、舗装・階段は模様なし（白×自分の色）
                flat = np.tile(np.array([0, 0, 1, 1, 0, 0], bool), len(idx))
                nouv = np.repeat(flat | np.repeat(dmg[idx], 6), 4)
                uv[nouv] = WALL_UV
                items.append(self._item(mesh, V[idx], N[idx], col, uv, True))
            else:
                col = np.repeat(np.repeat(cols, 6, axis=0), 4, axis=0)
                items.append(self._item(mesh, V[idx], N[idx], col, np.zeros((len(idx) * 24, 2)), False))
        self.detail["items"] = items

    def _meshes(self, level):
        if level == "simple":
            return [self.simple["mesh"]] if self.simple else []
        return [self.detail["body"], self.detail["plain"], self.detail["neon"]] if self.detail else []

    def _items(self, level):
        if level == "simple":
            it = self.simple.get("item") if self.simple else None
            return [it] if it else []
        return self.detail.get("items", []) if self.detail else []

    # --- 毎フレーム ---------------------------------------------------------
    def update(self, origin, cam_pos, far, detail_dist=100.0):
        c = self.city
        dist = float(np.linalg.norm(c.x - cam_pos)) - c.radius
        if dist >= far or c.alive() <= 0:
            want = None
        else:
            # 行ったり来たりでちらつかないように、詳細表示から戻るときは少し余裕を持たせる
            lim = detail_dist * (1.2 if self.level == "detail" else 1.0)
            want = "detail" if dist < lim else "simple"
        if self.version != c.version:
            # 壊れた・消えた：作ってある方を作り直す（もう片方は使うときに作る）
            self.version = c.version
            self._stale = {"simple": True, "detail": True}
            self.shade_R = None
        stale = getattr(self, "_stale", {"simple": True, "detail": True})
        if want is not None and stale.get(want, True):
            (self._build_simple if want == "simple" else self._build_detail)()
            stale[want] = False
            self._stale = stale
            for it in self._items(want):
                it["new"] = True
        if want != self.level:
            for lv in ("simple", "detail"):
                for m in self._meshes(lv):
                    (m.np.show if lv == want else m.np.hide)()
            self.level = want
            self.shade_R = None
        if want is None:
            return
        R = c.R
        reshade = self.shade_R is None or float(np.trace(self.shade_R.T @ R)) < 1 + 2 * np.cos(np.radians(4.0))
        for it in self._items(want):
            if it["new"] or (reshade and it["lit"]):
                col = shade(it["n"] @ R.T, it["col"]) * 1.25 if it["lit"] else it["col"]
                v = np.empty((len(it["v"]), 12), np.float32)
                v[:, 0:3] = it["v"]
                v[:, 3:6] = it["n"]
                v[:, 6:9] = col
                v[:, 9] = 1.0
                v[:, 10:12] = it["uv"]
                if it["new"]:
                    it["mesh"].set(v, it["tris"])
                    it["new"] = False
                else:
                    it["mesh"].set(v)
        if reshade:
            self.shade_R = R.copy()
        from panda3d.core import LMatrix4f
        M = np.eye(4)
        M[:3, :3] = R.T
        M[3, :3] = c.x - origin
        mat = LMatrix4f(*[float(x) for x in M.ravel()])
        for m in self._meshes(want):
            m.np.setMat(mat)

    def remove(self):
        for lv in ("simple", "detail"):
            for m in self._meshes(lv):
                m.remove()


class NpcView:
    """街の NPC（プレイヤーと同じような簡素な姿：体・頭・光るバイザー）。近くの街だけ毎フレーム作る。"""
    PARTS = [  # (中心の高さ, 半分の大きさ(横,縦,前後), 色の種類)
        (0.45, (0.16, 0.45, 0.14), "leg"),
        (1.22, (0.28, 0.36, 0.17), "body"),
        (1.72, (0.17, 0.17, 0.17), "head"),
        (1.74, (0.14, 0.05, 0.02), "visor"),
    ]

    def __init__(self, parent, city):
        self.city = city
        self.mesh = DynamicMesh(parent, f"npc{city.id}")
        self.mesh.np.setTwoSided(False)
        self.n = -1
        self.visible = True

    def update(self, origin, t, max_dist=70.0):
        c = self.city
        if float(np.linalg.norm(c.x - origin)) - c.radius > max_dist or not c.npcs:
            if self.visible:
                self.mesh.np.hide()
                self.visible = False
            return
        live = [n for n in c.npcs if citylife.npc_alive(c, n)]
        boxes_c, boxes_a, boxes_e, cols = [], [], [], []
        for n in live:
            p, f, up = citylife.npc_pose(c, n, t)
            side = np.cross(up, f)
            A = np.stack([side, up, f])
            for y, e, kind in self.PARTS:
                ctr = p + up * y
                if kind == "visor":
                    ctr = ctr + f * 0.17
                elif kind == "leg":
                    for sx in (-0.13, 0.13):
                        boxes_c.append(ctr + side * sx - origin)
                        boxes_a.append(A)
                        boxes_e.append((0.1, 0.45, 0.12))
                        cols.append(n["color"] * 0.7)
                    continue
                boxes_c.append(ctr - origin)
                boxes_a.append(A)
                boxes_e.append(e)
                cols.append(n["visor"] if kind == "visor" else (n["color"] * (1.25 if kind == "head" else 1.0)))
        if not boxes_c:
            self.mesh.set(np.zeros((0, 12), np.float32))
            return
        V, N, _ = _box_quads(np.array(boxes_c), np.array(boxes_a), np.array(boxes_e, float))
        nb = len(boxes_c)
        cols = np.array(cols)
        col = np.repeat(np.repeat(cols, 6, axis=0), 4, axis=0)
        nn = np.repeat(N.reshape(-1, 3), 4, axis=0)
        v = np.empty((nb * 24, 12), np.float32)
        v[:, 0:3] = V.reshape(-1, 3)
        v[:, 3:6] = nn
        v[:, 6:9] = shade(nn, col) * 1.2
        v[:, 9] = 1.0
        v[:, 10:12] = 0
        if nb != self.n:
            self.mesh.set(v, quad_tris(nb * 6))
            self.n = nb
        else:
            self.mesh.set(v)
        if not self.visible:
            self.mesh.np.show()
            self.visible = True

    def remove(self):
        self.mesh.remove()
