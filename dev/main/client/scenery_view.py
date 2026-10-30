"""景色の箱（main/scenery.py の Scenery）を描く。シナリオ・場所の MOD の画面側が使う。"""
import numpy as np

from .. import scenery as S
from .render import DynamicMesh, quad_tris, shade

_FACES = [(0, 1), (0, -1), (1, 1), (1, -1), (2, 1), (2, -1)]


def box_quads(C, A, E):
    """箱 (B) → 各面の4頂点 (B,6,4,3)・法線 (B,6,3)。外から見て表になる回り順。"""
    B = len(C)
    V = np.zeros((B, 6, 4, 3))
    N = np.zeros((B, 6, 3))
    for f, (ax, sg) in enumerate(_FACES):
        a1, a2 = (ax + 1) % 3, (ax + 2) % 3
        n = A[:, ax] * sg
        u = A[:, a1] * E[:, a1:a1 + 1]
        v = A[:, a2] * E[:, a2:a2 + 1]
        c = C + n * E[:, ax:ax + 1]
        V[:, f, 0] = c - u - v
        V[:, f, 1] = c + u - v
        V[:, f, 2] = c + u + v
        V[:, f, 3] = c - u + v
        cr = np.cross(V[:, f, 1] - V[:, f, 0], V[:, f, 2] - V[:, f, 0])
        flip = ~(np.einsum("ij,ij->i", cr, n) < 0)
        V[flip, f] = V[flip, f, ::-1]
        N[:, f] = n
    return V, N


TEXTURES = {"rock": ("rock.png", 3.0), "metal": ("metal.png", 2.0), "ground": ("ground.png", 2.5)}
_tex_cache = {}


def _texture(kind):
    if kind not in _tex_cache:
        from panda3d.core import Filename, SamplerState
        from ursina import application
        from .. import paths
        t = application.base.loader.loadTexture(Filename.fromOsSpecific(str(paths.SAUCE / "textures" / TEXTURES[kind][0])))
        t.setMinfilter(SamplerState.FT_linear_mipmap_linear)
        _tex_cache[kind] = t
    return _tex_cache[kind]


def tex_kind(p):
    """景色の箱の見た目の種類：岩・地面（道）・金属（建物など）・なし（光る物）。"""
    if p["glow"] is not None:
        return "plain"
    c = tuple(np.round(p["color"], 2))
    if c in {tuple(np.round(x, 2)) for x in (S.ROCK, S.ROCK2, S.ROCK3, S.MOON_G)}:
        return "rock"
    if c in {tuple(np.round(x, 2)) for x in (S.CONC, (0.40, 0.42, 0.45))}:
        return "ground"
    return "metal"


class SceneView:
    """景色の箱1か所の描画。ふつうの箱（影つき・模様つき）・明かり（点く／消える）・点滅のメッシュ。"""

    def __init__(self, parent, scene):
        self.s = scene
        self.meshes = {}
        P = scene.parts
        groups = {}
        for k, p in enumerate(P):
            if p["blink"] is not None:
                key = "blink"
            elif p["group"] is not None:
                key = "lights"
            else:
                key = "static:" + tex_kind(p)
            groups.setdefault(key, []).append(k)
        self.state = {}
        for name, idx in groups.items():
            m = DynamicMesh(parent, f"scenery_{scene.id}_{name}")
            m.node.clearBounds()
            m.node.setFinal(False)
            m.np.setTwoSided(False)
            kind = name.split(":")[1] if ":" in name else "plain"
            tile = 1.0
            if kind in TEXTURES:
                m.np.setTexture(_texture(kind), 1)
                tile = TEXTURES[kind][1]
            idx = np.array(idx)
            C, A, E = scene.C[idx] - scene.center, scene.A[idx], scene.E[idx]
            V, N = box_quads(C, A, E)
            keep = [0, 1, 2, 4, 5]                          # 下の面は見えない
            V, N = V[:, keep], N[:, keep]
            nb, nf = len(idx), len(keep)
            # 模様の座標：面の中の2つの軸に沿った長さ（m）÷ 1枚の大きさ
            UV = np.zeros((nb, nf, 4, 2))
            for j, f in enumerate(keep):
                ax = _FACES[f][0]
                a1, a2 = (ax + 1) % 3, (ax + 2) % 3
                if ax != 1:
                    a1, a2 = (2 if ax == 0 else 0), 1
                rel = V[:, j] - C[:, None, :]
                UV[:, j, :, 0] = np.einsum("bvk,bk->bv", rel, A[:, a1]) / tile
                UV[:, j, :, 1] = np.einsum("bvk,bk->bv", rel, A[:, a2]) / tile
            v = np.empty((nb * nf * 4, 12), np.float32)
            v[:, 0:3] = V.reshape(-1, 3)
            v[:, 3:6] = np.repeat(N.reshape(-1, 3), 4, axis=0)
            v[:, 9] = 1.0
            v[:, 10:12] = UV.reshape(-1, 2)
            self.meshes[name] = {"mesh": m, "idx": idx, "v": v, "nf": nf, "tris": quad_tris(nb * nf), "new": True,
                                 "bright": 1.35 if kind in TEXTURES else 1.15}
        for name in self.meshes:
            if name.startswith("static"):
                self._colors(name, None)

    def _colors(self, name, on):
        """色を入れる。光る部品は（static なら いつも、lights なら on のとき）光の色で影なし、ほかは影つき。"""
        it = self.meshes.get(name)
        if it is None:
            return
        P = self.s.parts
        per = it["nf"] * 4
        static = name.startswith("static")
        glowing = np.array([P[k]["glow"] is not None and (static or bool(on)) for k in it["idx"]])
        base = np.array([P[k]["glow"] if g else P[k]["color"] for k, g in zip(it["idx"], glowing)], float)
        col = np.repeat(base, per, axis=0)
        lit = np.repeat(~glowing, per)
        col[lit] = shade(it["v"][lit, 3:6].astype(np.float64), col[lit]) * it["bright"]
        it["v"][:, 6:9] = np.clip(col, 0, 1)
        if it["new"]:
            it["mesh"].set(it["v"], it["tris"])
            it["new"] = False
        else:
            it["mesh"].set(it["v"])

    def update(self, origin, lights_on, t):
        pos = self.s.center - origin
        for it in self.meshes.values():
            it["mesh"].np.setPos(float(pos[0]), float(pos[1]), float(pos[2]))
        if self.state.get("lights") != lights_on:
            self.state["lights"] = lights_on
            self._colors("lights", lights_on)
        it = self.meshes.get("blink")
        if it is not None and t - self.state.get("blink_t", -1) > 0.08:
            self.state["blink_t"] = t
            P = self.s.parts
            nf = it["nf"]
            k_on = np.array([0.5 + 0.5 * np.sin(t * 2 * np.pi / P[k]["blink"] + k * 0.7) for k in it["idx"]])
            cols = np.concatenate([np.tile(P[k]["color"] + (np.asarray(P[k]["glow"]) - P[k]["color"]) * (0.25 + 0.75 * a),
                                           (nf * 4, 1)) for k, a in zip(it["idx"], k_on)])
            it["v"][:, 6:9] = np.clip(cols, 0, 1)
            it["mesh"].set(it["v"], it["tris"] if it["new"] else None)
            it["new"] = False

    def remove(self):
        for it in self.meshes.values():
            it["mesh"].remove()
