"""大都市圏（画面側）：遠くの建物（まだブロックで置いていない物）を窓の模様の箱で、線路を細い帯で描く。

ブロックで置いた建物は描かない（置いた物の組が変わったら作り直す）。メッシュは惑星ローカルで作り、毎フレームは位置だけ変える。
"""
import numpy as np

from main.client import gfx
from main.client.addon_hooks import ClientAddon
from main.client.render import AMBIENT, SUN, DynamicMesh

COLORS = {"tower": (0.42, 0.52, 0.62), "office": (0.66, 0.66, 0.66), "apartment": (0.78, 0.74, 0.66),
          "factory": (0.58, 0.58, 0.6), "warehouse": (0.42, 0.45, 0.52), "tank": (0.88, 0.88, 0.88), "chimney": (0.72, 0.7, 0.68)}
WINDOWED = {"tower", "office", "apartment"}
_tex = None


def window_texture():
    """壁の窓の模様（4 m × 4 m で 1 枚）。白い壁に濃い窓（色は頂点の色を掛ける）"""
    global _tex
    if _tex is not None:
        return _tex
    S = 32
    img = np.full((S, S, 4), 255, np.uint8)
    for gy in range(2):
        for gx in range(2):
            y0, x0 = gy * 16 + 4, gx * 16 + 3
            img[y0:y0 + 9, x0:x0 + 10, :3] = (70, 85, 105)
    _tex = gfx.tex_rgba(S, S, img.tobytes(), "mega_windows", nearest=False)
    return _tex


def _boxes(C, X, Y, Z, E, col, win):
    """箱（中心 C・軸 X Y Z・半分の大きさ E (n,3)）→ 頂点 (m,12)・三角形。下の面は作らない。
    側面は窓の模様（4 m で 1 枚。win が False の物は壁の色だけ）"""
    n = len(C)
    e0, e1, e2 = E[:, 0:1], E[:, 1:2], E[:, 2:3]
    # (法線, 法線の向きの半分の大きさ, 縦の軸, 縦の半分, 横の軸, 横の半分, 側面か)
    faces = [(X, e0, Y, e1, Z, e2, True), (-X, e0, Y, e1, Z, e2, True), (Z, e2, Y, e1, X, e0, True),
             (-Z, e2, Y, e1, X, e0, True), (Y, e1, X, e0, Z, e2, False)]
    vs, ts = [], []
    base = 0
    for N, eN, A, ea, B, eb, side in faces:
        c = C + N * eN
        P = [c - A * ea - B * eb, c - A * ea + B * eb, c + A * ea + B * eb, c + A * ea - B * eb]
        lam = np.clip(N @ SUN, 0, 1)
        k = (AMBIENT + (1 - AMBIENT) * lam)[:, None] * col
        V = np.zeros((n, 4, 12), np.float32)
        hh, ww = (2 * ea[:, 0] / 4.0), (2 * eb[:, 0] / 4.0)
        uvq = [(0, 0), (1, 0), (1, 1), (0, 1)]
        for q in range(4):
            V[:, q, 0:3] = P[q]
            V[:, q, 3:6] = N
            V[:, q, 6:9] = k * (1.0 if side else 0.85)
            V[:, q, 9] = 1.0
            if side:
                V[:, q, 10] = np.where(win, ww * uvq[q][0], 0.02)
                V[:, q, 11] = np.where(win, hh * uvq[q][1], 0.02)
            else:
                V[:, q, 10:12] = 0.02
        vs.append(V.reshape(-1, 12))
        idx = base + np.arange(n)[:, None] * 4
        ts.append((idx + np.array([[0, 1, 2, 0, 2, 3]])).reshape(-1, 3))
        base += n * 4
    return np.concatenate(vs), np.concatenate(ts).astype(np.int64)


class MegaClient(ClientAddon):
    name = "megalopolis"

    def __init__(self, game):
        super().__init__(game)
        self.pack = game.world.addon("megalopolis")
        self.root = None
        self.meshes = {}
        self.version = -1
        self.track_mesh = None

    def destroy(self):
        for m in list(self.meshes.values()) + ([self.track_mesh] if self.track_mesh else []):
            try:
                m.remove()
            except Exception:  # noqa: BLE001
                pass
        self.meshes = {}
        if self.root is not None:
            self.root.removeNode()
            self.root = None

    def _build(self):
        p = self.pack
        pl = p.planet
        made = p.made.keys() | p.kept
        for s in p.plan["sites"]:
            bs = [b for b in p.plan["buildings"] if b["site"] == s["id"] and b["id"] not in made and b["id"] not in p.gone]
            m = self.meshes.get(s["id"])
            if m is None:
                m = self.meshes[s["id"]] = DynamicMesh(self.root, f"mega_{s['id']}")
                m.np.setTwoSided(True)
                m.np.setTexture(window_texture(), 1)
            if not bs:
                m.set(np.zeros((0, 12), np.float32))
                continue
            up = np.array([b["dir"] for b in bs])
            Zr = s["F"][2]
            Z = up * 0
            Z[:] = Zr
            Z = Z - up * np.einsum("ij,ij->i", Z, up)[:, None]
            Z /= np.linalg.norm(Z, axis=1)[:, None]
            X = np.cross(up, Z)
            h = np.array([b["h"] for b in bs])
            g = np.array([b["g"] for b in bs])
            C = up * (g + h / 2)[:, None]
            E = np.stack([[b["w"] / 2 for b in bs], h / 2, [b["d"] / 2 for b in bs]], 1)
            col = np.array([COLORS.get(b["style"], (0.6, 0.6, 0.6)) for b in bs])
            win = np.array([b["style"] in WINDOWED for b in bs])
            V, T = _boxes(C, X, up, Z, E, col, win)
            m.set(V, T)
        if self.track_mesh is None:
            self._build_tracks()

    def _build_tracks(self):
        p = self.pack
        vs, ts = [], []
        base = 0
        for ln in p.plan["lines"]:
            P, U = ln["P"], ln["U"]
            T = np.gradient(P, axis=0)
            T /= np.linalg.norm(T, axis=1)[:, None]
            Rt = np.cross(U, T)
            Rt /= np.linalg.norm(Rt, axis=1)[:, None]
            c = P - U * 0.6                        # 区間（ブロック）の砂利の中に隠れる高さ
            L_ = c - Rt * 4.5
            Rr = c + Rt * 4.5
            n = len(P)
            V = np.zeros((2 * n, 12), np.float32)
            V[0::2, 0:3] = L_
            V[1::2, 0:3] = Rr
            V[:, 3:6] = np.repeat(U, 2, axis=0)
            V[:, 6:9] = (0.33, 0.31, 0.3)
            V[:, 9] = 1.0
            V[:, 10:12] = 0.02
            vs.append(V)
            i = base + np.arange(n - 1)[:, None] * 2
            ts.append((i + np.array([[0, 1, 3, 0, 3, 2]])).reshape(-1, 3))
            base += 2 * n
        self.track_mesh = DynamicMesh(self.root, "mega_tracks")
        self.track_mesh.np.setTwoSided(True)
        if vs:
            self.track_mesh.set(np.concatenate(vs), np.concatenate(ts).astype(np.int64))

    def update(self, origin, far):
        p = self.pack
        if p is None or p.plan is None:
            return
        if self.root is None:
            self.root = self.game.scene_root.attachNewNode("megalopolis")
        c = np.asarray(p.planet.center, float) - np.asarray(origin, float)
        self.root.setPos(float(c[0]), float(c[1]), float(c[2]))
        if self.version != p.version:
            self.version = p.version
            self._build()
