"""ミサイル・砲塔のビーム・爆発を描く（画面側）。毎フレーム頂点を作り直す（数は多くても数十）。"""
import numpy as np

from main.client.addon_hooks import ClientAddon
from main.client.render import DynamicMesh, quad_tris

_FACES = [(0, 1), (0, -1), (1, 1), (1, -1), (2, 1), (2, -1)]


def _boxes(C, A, E, col, alpha):
    """箱 (B) → 頂点 (B*24, 12)。A は (B,3,3) で行が軸。"""
    B = len(C)
    V = np.zeros((B, 6, 4, 3))
    N = np.zeros((B, 6, 3))
    for f, (ax, sg) in enumerate(_FACES):
        a1, a2 = (ax + 1) % 3, (ax + 2) % 3
        n = A[:, ax] * sg
        u = A[:, a1] * E[:, a1:a1 + 1]
        v = A[:, a2] * E[:, a2:a2 + 1]
        c = C + n * E[:, ax:ax + 1]
        V[:, f] = np.stack([c - u - v, c + u - v, c + u + v, c - u + v], axis=1)
        N[:, f] = n
    out = np.zeros((B * 24, 12), np.float32)
    out[:, 0:3] = V.reshape(-1, 3)
    out[:, 3:6] = np.repeat(N.reshape(-1, 3), 4, axis=0)
    out[:, 6:9] = np.repeat(np.asarray(col, float).reshape(-1, 3), 24, axis=0) if np.ndim(col) == 2 else col
    out[:, 9] = np.repeat(np.asarray(alpha, float).reshape(-1), 24) if np.ndim(alpha) else alpha
    return out


def _frame(d):
    d = np.asarray(d, float)
    n = np.linalg.norm(d)
    f = d / n if n > 1e-9 else np.array([0.0, 0.0, 1.0])
    ref = np.array([0.0, 1.0, 0.0]) if abs(f[1]) < 0.9 else np.array([1.0, 0.0, 0.0])
    r = np.cross(ref, f)
    r /= np.linalg.norm(r)
    u = np.cross(f, r)
    return np.stack([r, u, f])


class WeaponsClient(ClientAddon):
    name = "weapons"

    def __init__(self, game):
        super().__init__(game)
        self.pack = game.world.addon("weapons")
        root = game.scene_root
        self.solid = DynamicMesh(root, "weapons_solid")
        self.glow = DynamicMesh(root, "weapons_glow")
        self.glow.np.setTransparency(True)
        self.glow.np.setBlendAdd(True)                  # 光：足し算で重ねる
        self.glow.np.setDepthWrite(False)
        self.glow.np.setBin("fixed", 20)
        self._n = (-1, -1)

    def destroy(self):
        self.solid.np.removeNode()
        self.glow.np.removeNode()

    def update(self, origin, far):
        if self.pack is None:
            return
        t = self.world.time
        ms = self.pack.missiles
        solid = []
        glow = []
        if ms:
            C = np.array([m["p"] for m in ms]) - origin
            A = np.array([_frame(m["v"]) for m in ms])
            E = np.tile([0.22, 0.22, 0.9], (len(ms), 1))
            solid.append(_boxes(C, A, E, (0.78, 0.78, 0.82), 1.0))
            # 噴射の光（後ろ）
            back = C - A[:, 2] * 1.3
            glow.append(_boxes(back, A, np.tile([0.35, 0.35, 0.6], (len(ms), 1)), (1.0, 0.55, 0.15), 0.9))
            glow.append(_boxes(back - A[:, 2] * 1.2, A, np.tile([0.6, 0.6, 1.2], (len(ms), 1)), (1.0, 0.35, 0.1), 0.35))
        for kind, p, q, t0 in self.pack.effects:
            age = t - t0
            if age < 0:
                continue
            if kind == "beam" and q is not None:
                a = max(0.0, 1.0 - age / 0.25)
                if a <= 0:
                    continue
                mid = (p + q) / 2 - origin
                F = _frame(q - p)
                L_ = float(np.linalg.norm(q - p)) / 2
                glow.append(_boxes(mid[None], F[None], np.array([[0.12, 0.12, L_]]), (0.4, 0.95, 1.0), a))
                glow.append(_boxes(mid[None], F[None], np.array([[0.35, 0.35, L_]]), (0.2, 0.6, 1.0), a * 0.35))
            elif kind in ("boom", "spark", "launch"):
                life = {"boom": 1.2, "spark": 0.6, "launch": 0.5}[kind]
                if age > life:
                    continue
                k = age / life
                size = {"boom": 1.0 + 7.0 * k ** 0.5, "spark": 0.5 + 2.0 * k, "launch": 0.6 + 2.5 * k}[kind]
                col = (1.0, 0.55 * (1 - k) + 0.25, 0.15) if kind != "launch" else (0.9, 0.9, 0.95)
                c = (p - origin)[None]
                for rot in (np.eye(3), _frame([1.0, 1.0, 0.3])):
                    glow.append(_boxes(c, rot[None], np.array([[size, size, size]]) * 0.5, col, (1 - k) * 0.55))
        self._set(self.solid, solid, 0)
        self._set(self.glow, glow, 1)

    def _set(self, mesh, parts, slot):
        if not parts:
            if self._n[slot] != 0:
                mesh.set(np.zeros((0, 12), np.float32))
                n = list(self._n)
                n[slot] = 0
                self._n = tuple(n)
            return
        v = np.concatenate(parts)
        nq = len(v) // 4
        if nq != self._n[slot]:
            mesh.set(v, quad_tris(nq))
            n = list(self._n)
            n[slot] = nq
            self._n = tuple(n)
        else:
            mesh.set(v)
