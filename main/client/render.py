"""必要最低限の描画：Panda3D の頂点データを毎フレーム numpy から書き換える。

浮動原点：全ての描画位置は「カメラ（プレイヤーの目）からの相対座標」にしてから
float32 に変換するので、世界がどれだけ広くても精度が落ちない。
"""
import numpy as np
from panda3d.core import (Geom, GeomEnums, GeomNode, GeomTriangles, GeomVertexArrayFormat, GeomVertexData,
                          GeomVertexFormat, InternalName, OmniBoundingVolume, SamplerState, Texture)

SUN = np.array([0.35, 0.85, 0.4])
SUN /= np.linalg.norm(SUN)
AMBIENT = 0.38

_arr = GeomVertexArrayFormat()
_arr.addColumn(InternalName.getVertex(), 3, Geom.NT_float32, Geom.C_point)
_arr.addColumn(InternalName.getNormal(), 3, Geom.NT_float32, Geom.C_normal)
_arr.addColumn(InternalName.getColor(), 4, Geom.NT_float32, Geom.C_color)
_arr.addColumn(InternalName.getTexcoord(), 2, Geom.NT_float32, Geom.C_texcoord)
FORMAT = GeomVertexFormat.registerFormat(GeomVertexFormat(_arr))

_tex_cache = {}


def load_texture(loader, path):
    if not path:
        return None
    if path in _tex_cache:
        return _tex_cache[path]
    from panda3d.core import Filename
    try:
        tex = loader.loadTexture(Filename.fromOsSpecific(path))
        tex.setMagfilter(SamplerState.FT_nearest)
        tex.setMinfilter(SamplerState.FT_linear_mipmap_linear)
        tex.setWrapU(Texture.WM_repeat)
        tex.setWrapV(Texture.WM_repeat)
    except Exception:  # noqa: BLE001
        tex = None
    _tex_cache[path] = tex
    return tex


class DynamicMesh:
    """頂点（位置・法線・色・UV）と三角形を numpy 配列から設定できるメッシュ。"""

    def __init__(self, parent, name="mesh"):
        self.node = GeomNode(name)
        # 頂点を直接書き換えるので境界ボリュームは無限大にしてカリングさせない
        self.node.setBounds(OmniBoundingVolume())
        self.node.setFinal(True)
        self.np = parent.attachNewNode(self.node)
        self.np.setTwoSided(True)
        self.np.setLightOff()
        self.vdata = None
        self.n_rows = -1
        self.n_tris = -1

    def set(self, verts, tris=None):
        """verts: (N,12) float32 [x,y,z,nx,ny,nz,r,g,b,a,u,v]、tris: (M,3) 三角形（変わった時だけ）"""
        verts = np.ascontiguousarray(verts, dtype=np.float32)
        n = len(verts)
        if n == 0:
            self.node.removeAllGeoms()
            self.vdata = None
            self.n_rows = 0
            return
        if self.vdata is None or n != self.n_rows or tris is not None:
            self.vdata = GeomVertexData("v", FORMAT, Geom.UH_dynamic)
            self.vdata.uncleanSetNumRows(n)
            self.n_rows = n
            if tris is None:
                raise ValueError("頂点数が変わる時は tris が必要です")
            tris = np.ascontiguousarray(tris, dtype=np.uint32).ravel()
            prim = GeomTriangles(Geom.UH_static)
            prim.setIndexType(GeomEnums.NT_uint32)
            parr = prim.modifyVertices()
            parr.uncleanSetNumRows(len(tris))
            memoryview(parr).cast("B")[:] = tris.tobytes()
            geom = Geom(self.vdata)
            geom.addPrimitive(prim)
            self.node.removeAllGeoms()
            self.node.addGeom(geom)
            self.geom = geom
            self.n_tris = len(tris) // 3
        arr = self.geom.modifyVertexData().modifyArray(0)
        memoryview(arr).cast("B")[:] = verts.tobytes()

    def remove(self):
        self.np.removeNode()


def quad_tris(nq):
    base = (np.arange(nq) * 4)[:, None]
    return np.concatenate([base + [0, 1, 2], base + [0, 2, 3]], axis=1).reshape(-1, 3)


def shade(normals, colors):
    lam = np.clip(normals @ SUN, 0, 1)
    k = AMBIENT + (1 - AMBIENT) * lam
    return colors * k[:, None]


def _centers8(P8):
    return P8.mean(axis=1)


def _frames8(P8):
    """(n,8,3) の頂点 → 各ボクセルのローカル軸 (n,3,3)。construct.voxel_frames と同じ。"""
    ex = (P8[:, [1, 3, 5, 7]] - P8[:, [0, 2, 4, 6]]).mean(1)
    ey = (P8[:, [2, 3, 6, 7]] - P8[:, [0, 1, 4, 5]]).mean(1)
    ez = (P8[:, [4, 5, 6, 7]] - P8[:, [0, 1, 2, 3]]).mean(1)
    F = np.stack([ex, ey, ez], axis=1)
    return F / np.maximum(np.linalg.norm(F, axis=2, keepdims=True), 1e-9)


def _native():
    from .. import native
    core = native.core
    return core if core is not None and hasattr(core, "mesh_pack") else None


class ConstructView:
    """1つの構築物の描画（テクスチャごとにメッシュを分ける）と、その機器の記号・看板。

    全部を1つの親ノード（root）の下に置く。剛体・静止中の構築物は、形・記号・看板を構築物の向きで一度だけ作り、
    毎フレームは親ノードの位置と向き（行列1つ）だけを動かす。ブロックごとに動いている構築物は毎フレーム作り直す
    （Rust の物理コアがあれば、頂点・法線・陰影をまとめて Rust で作る）。"""

    def __init__(self, parent, loader, construct):
        from ursina import Entity
        self.c = construct
        self.root = Entity(parent=parent)
        self.parent = self.root
        self.loader = loader
        self.version = -1
        self.groups = []   # [(mesh, face_rows, colors(F,3), tex)]
        self.displays = {}  # (ボクセル, 面) → DisplayView
        self._devs_ver = None
        self._mat_key = None
        self.visible = True

    # -- 共通 --------------------------------------------------------------
    def set_visible(self, on):
        if on != self.visible:
            self.visible = on
            (self.root.show if on else self.root.hide)()

    def _dev_ids(self):
        """機器のあるボクセルの番号（形が変わったときだけ数え直す）。"""
        c = self.c
        if self._devs_ver != c.version:
            self._devs_ver = c.version
            self._devs = [i for i, d in enumerate(c.devs) if d]
        return self._devs

    def _firing(self):
        c = self.c
        ids = self._dev_ids()
        f = np.zeros(c.n_vox)
        for i in ids:
            d = c.devs[i]
            if d:
                f[i] = d["state"].get("firing", 0.0)
        return f

    def _set_root_mat(self, M):
        """M：4x4（行ベクトル形式）か None（単位行列）。"""
        from panda3d.core import LMatrix4f
        if M is None:
            if self._mat_key is None:
                return
            self._mat_key = None
            self.root.setMat(LMatrix4f.identMat())
            return
        self._mat_key = 1
        self.root.setMat(LMatrix4f(*[float(x) for x in M.ravel()]))

    def _rebuild(self):
        """形のメッシュが作れない（ブロックが無い）とき：何も描かない。"""
        for g in self.groups:
            g["mesh"].remove()
        self.groups = []
        self.version = self.c.version

    def _rebuild_blob(self):
        from .. import shape
        for g in self.groups:
            g["mesh"].remove()
        self.groups = []
        c = self.c
        m = shape.build_mesh(c)
        self.blob = m
        self.version = c.version
        if m is None:
            return False
        reg = c.reg
        col_v, tex_v = [], []
        for i in range(c.n_vox):
            d = c.devs[i]
            if d:
                dd = reg.device(d["def"]) or {}
                col_v.append(dd.get("color", [1, 0, 1]))
                tex_v.append(dd.get("texture"))
            else:
                mt = reg.material(c.mats[i])
                col_v.append(mt["color"])
                tex_v.append(mt.get("texture"))
        col_v = np.asarray(col_v, dtype=np.float64)
        owner = m["owner"]
        faces = m["faces"]
        m["faces64"] = np.ascontiguousarray(faces, dtype=np.int64)
        m["vidx64"] = np.ascontiguousarray(m["vidx"], dtype=np.int64)
        m["w64"] = np.ascontiguousarray(m["w"], dtype=np.float64)
        # 静止時の法線で、テクスチャ座標を面の向きに合わせて付ける（ブロック1つでテクスチャ1枚）
        rest = m["rest"]
        rn = shape.vertex_normals(rest, faces)
        ax = np.argmax(np.abs(rn), axis=1)
        uv = np.where(ax[:, None] == 0, rest[:, [1, 2]], np.where(ax[:, None] == 1, rest[:, [0, 2]], rest[:, [0, 1]]))
        face_tex = [tex_v[o] for o in owner[faces[:, 0]]]
        groups = {}
        for f, t in enumerate(face_tex):
            groups.setdefault(t, []).append(f)
        for tex, fl in groups.items():
            fl = np.asarray(fl, dtype=np.int64)
            sub = faces[fl]
            verts, inv = np.unique(sub.ravel(), return_inverse=True)
            mesh = DynamicMesh(self.root, f"c{c.id}")
            t = load_texture(self.loader, tex)
            if t:
                mesh.np.setTexture(t, 1)
            self.groups.append({"mesh": mesh, "verts": np.ascontiguousarray(verts, dtype=np.int64),
                                "tris": inv.reshape(-1, 3),
                                "cols": np.ascontiguousarray(col_v[owner[verts]]),
                                "uv": np.ascontiguousarray(uv[verts], dtype=np.float32),
                                "owner": np.ascontiguousarray(owner[verts], dtype=np.int64), "new": True})
        return True

    def _set_mesh(self, g, v):
        if g["new"]:
            g["mesh"].set(v, g["tris"])
            g["new"] = False
        else:
            g["mesh"].set(v)

    def _update_rigid(self, origin, tf, highlight_flames):
        """剛体・静止中の構築物：形は作り直さず（メッシュの使い回し）、位置と向きだけ動かす。
        陰影（色）は向きが少し変わったときや噴射の状態が変わったときだけ付け直す。"""
        from .. import shape
        c = self.c
        R, x, epoch = tf
        firing = None
        if highlight_flames:
            firing = tuple(round(float(c.devs[i]["state"].get("firing", 0.0)), 2) for i in self._dev_ids())
        b = getattr(self, "_baked", None)
        if b is None or b["epoch"] != epoch:
            V = shape.skin(c, self.blob)
            local = (V - x) @ R          # 構築物の向きでの形
            nl = shape.vertex_normals(local, self.blob["faces"])
            b = self._baked = {"epoch": epoch, "local": local, "nl": nl, "R": None, "firing": None}
            for g in self.groups:
                g["new"] = True
        need_color = (b["R"] is None or b["firing"] != firing
                      or float(np.trace(b["R"].T @ R)) < 1 + 2 * np.cos(np.radians(4.0)))
        if need_color or any(g["new"] for g in self.groups):
            b["R"], b["firing"] = R.copy(), firing
            fl_all = None
            if firing is not None and any(firing):
                fl_all = np.zeros(c.n_vox)
                fl_all[self._dev_ids()] = firing
            for g in self.groups:
                vs = g["verts"]
                nw = b["nl"][vs] @ R.T
                col = shade(nw, g["cols"])
                if fl_all is not None:
                    fl = fl_all[g["owner"]][:, None]
                    col = col * (1 - fl * 0.6) + np.array([1.0, 0.75, 0.25]) * fl
                v = np.empty((len(vs), 12), dtype=np.float32)
                v[:, 0:3] = b["local"][vs]
                v[:, 3:6] = b["nl"][vs]
                v[:, 6:9] = col
                v[:, 9] = 1.0
                v[:, 10:12] = g["uv"]
                self._set_mesh(g, v)

    def _update_blob(self, origin, highlight_flames):
        from .. import shape
        c = self.c
        m = self.blob
        firing = self._firing() if highlight_flames else None
        core = _native()
        if core is not None:
            pos = c.pos if c.pos.flags.c_contiguous and c.pos.dtype == np.float64 else np.ascontiguousarray(c.pos, np.float64)
            outs = core.mesh_pack(pos, m["vidx64"], m["w64"], m["faces64"], [float(x) for x in origin],
                                  [(g["verts"], g["cols"], g["uv"], g["owner"]) for g in self.groups],
                                  [float(x) for x in SUN], float(AMBIENT), firing)
            for g, v in zip(self.groups, outs):
                self._set_mesh(g, v)
            return
        V = shape.skin(c, m) - origin
        N = shape.vertex_normals(V, m["faces"])
        for g in self.groups:
            vs = g["verts"]
            n = N[vs]
            col = shade(n, g["cols"])
            if firing is not None and firing.any():
                fl = firing[g["owner"]][:, None]
                col = col * (1 - fl * 0.6) + np.array([1.0, 0.75, 0.25]) * fl
            v = np.empty((len(vs), 12), dtype=np.float32)
            v[:, 0:3] = V[vs]
            v[:, 3:6] = n
            v[:, 6:9] = col
            v[:, 9] = 1.0
            v[:, 10:12] = g["uv"]
            self._set_mesh(g, v)

    # -- 毎フレーム --------------------------------------------------------
    def update(self, origin, highlight_flames=True):
        """形を描く。戻り値：剛体・静止中なら (R, x, 版)、ブロックごとなら None（記号・看板の位置合わせに使う）。"""
        c = self.c
        self._tf = None
        if self.version != c.version:
            if not self._rebuild_blob():
                self.blob = None
                self._rebuild()
            self._baked = None
        tf = c.render_transform() if hasattr(c, "render_transform") else None
        if tf is not None:
            R, x, _ = tf
            # 行ベクトル形式（Panda3D）：p' = p R^T + (x - origin)
            M = np.eye(4)
            M[:3, :3] = R.T
            M[3, :3] = x - origin
            self._set_root_mat(M)
        else:
            self._set_root_mat(None)
        self._tf = tf
        if getattr(self, "blob", None) is not None:
            if tf is not None:
                self._update_rigid(origin, tf, highlight_flames)
            else:
                self._baked = None
                self._update_blob(origin, highlight_flames)
        return tf

    def _local_pos(self):
        """記号・看板用の頂点位置：剛体・静止中は構築物の向きでの位置（版ごとに1回だけ計算）、それ以外は None。"""
        tf = self._tf
        if tf is None:
            return None
        R, x, epoch = tf
        lp = getattr(self, "_lp", None)
        if lp is None or lp[0] != epoch:
            lp = self._lp = (epoch, ((self.c.pos - x) @ R).reshape(-1, 8, 3))
        return lp[1]

    def update_decals(self, origin, cam_pos, max_dist):
        if not hasattr(self, "decals"):
            self.decals = DecalView(self.root, self.c, max_dist)
        self.decals.max_dist = max_dist
        self.decals.update(origin, cam_pos, self)

    def hide_decals(self):
        if hasattr(self, "decals") and self.decals.visible:
            self.decals.mesh.np.hide()
            self.decals.visible = False

    def update_displays(self, origin, far):
        """コンピューターの表示（看板）。"""
        c = self.c
        want = {}
        lim2 = min(far, 400.0) ** 2
        for i in self._dev_ids():
            d = c.devs[i]
            if not d:
                continue
            disp = d["state"].get("display")
            if not disp or not disp.get("text"):
                continue
            q = c.pos[8 * i] - origin
            if float(q @ q) > lim2:
                continue
            for f in display_faces(c, i, disp):
                want[(i, f)] = disp
        for k in list(self.displays):
            if k not in want:
                self.displays.pop(k).remove()
        if not want:
            return
        L = self._local_pos()
        key = None if self._tf is None else self._tf[2]
        for (i, f), disp in want.items():
            v = self.displays.get((i, f))
            if v is None:
                v = self.displays[(i, f)] = DisplayView(self.root)
            v.set(disp, c.size)
            if L is not None:
                if v.placed == key:
                    continue
                P8 = L[i:i + 1]
                center, right, up, fwd = face_frame_p(P8[0], _frames8(P8)[0], f, c.size)
                v.place(center, right, up, fwd)
                v.placed = key
            else:
                P8 = c.pos[8 * i:8 * i + 8][None]
                center, right, up, fwd = face_frame_p(P8[0], _frames8(P8)[0], f, c.size)
                v.place(center - origin, right, up, fwd)
                v.placed = None

    def remove(self):
        from .ui_destroy import destroy
        for g in self.groups:
            g["mesh"].remove()
        self.groups = []
        if hasattr(self, "decals"):
            self.decals.remove()
        for v in self.displays.values():
            v.remove()
        self.displays = {}
        destroy(self.root)


class DecalView:
    """機器ブロックの表面に種類の記号を貼る。向きのある機器は、横の面の ▲ が機器の向き、
    向いている面は黄色い輪、反対側の面は薄い記号になる。遠くの構築物では表示しない（負荷対策）。"""
    _texture = None
    _uv = None

    def __init__(self, parent, construct, max_dist=30.0):
        from panda3d.core import TransparencyAttrib
        self.c = construct
        self.max_dist = max_dist
        self.mesh = DynamicMesh(parent, f"decal{construct.id}")
        self.mesh.np.setTransparency(TransparencyAttrib.MAlpha)
        self.mesh.np.setDepthOffset(2)
        self.mesh.np.setDepthWrite(False)
        self.mesh.np.setBin("transparent", 5)
        if DecalView._texture is None:
            from . import icons
            img, DecalView._uv = icons.decal_atlas()
            DecalView._texture = icons.panda_texture(img, "decals")
        self.mesh.np.setTexture(DecalView._texture, 1)
        self.version = -1
        self.items = None
        self.visible = False
        self._placed = None

    def _rebuild(self):
        from ..construct import DIRS
        c = self.c
        reg = c.reg
        vox, nrm, upv, uvs = [], [], [], []
        for i, d in enumerate(c.devs):
            if not d:
                continue
            dd = reg.device(d["def"]) or {}
            kind = dd.get("mark") or dd.get("type")
            kind = kind if kind in self._kinds() else "other"
            directional = bool(dd.get("directional"))
            dirv = DIRS[int(d.get("dir", 2)) % 6]
            disp = d["state"].get("display")
            disp_faces = set(display_faces(c, i, disp)) if disp and disp.get("text") else set()
            cell = c.cells[i]
            for f in range(6):
                n = DIRS[f]
                if (cell[0] + n[0], cell[1] + n[1], cell[2] + n[2]) in c.index or f in disp_faces:
                    continue
                default_up = np.array([0, 0, 1]) if n[1] != 0 else np.array([0, 1, 0])
                if directional:
                    dot = int(np.dot(dirv, n))
                    if dot == 1:
                        var, up = "front", default_up
                    elif dot == -1:
                        var, up = "back", default_up
                    else:
                        var, up = "side", dirv
                else:
                    var, up = "plain", default_up
                vox.append(i)
                nrm.append(n)
                upv.append(up)
                uvs.append(self._uv[(kind, var)])
        self.version = c.version
        self._placed = None
        if not vox:
            self.items = None
            self.mesh.set(np.zeros((0, 12), np.float32))
            return
        self.items = (np.array(vox), np.array(nrm, float), np.array(upv, float), np.array(uvs, np.float32))
        self._new = True

    @staticmethod
    def _kinds():
        from .icons import DECAL_TYPES
        return DECAL_TYPES

    def _display_sig(self):
        return tuple(i for i, d in enumerate(self.c.devs) if d and (d["state"].get("display") or {}).get("text"))

    def update(self, origin, cam_pos, view=None):
        c = self.c
        if self.items is not None or self.version != c.version:
            sig = self._display_sig()
            if self.version != c.version or sig != getattr(self, "_sig", None):
                self._sig = sig
                self._rebuild()
        if self.items is None:
            return
        # 遠くの構築物では記号を出さない
        rel = c.pos[::8] - cam_pos
        near = np.einsum("ij,ij->i", rel, rel).min() < self.max_dist ** 2
        if not near:
            if self.visible:
                self.mesh.np.hide()
                self.visible = False
            return
        if not self.visible:
            self.mesh.np.show()
            self.visible = True
        L = view._local_pos() if view is not None else None
        if L is not None:
            # 剛体・静止中：構築物の向きで一度だけ作る（位置と向きは親ノードの行列で動く）
            key = view._tf[2]
            if self._placed == key:
                return
            self._placed = key
            self._write(L, np.zeros(3))
        else:
            self._placed = None
            self._write(c.pos.reshape(-1, 8, 3), origin)

    def _write(self, P8, origin):
        c = self.c
        vox, nl, al, uv = self.items
        sel = P8[vox]
        cen = _centers8(sel)
        F = _frames8(sel)                             # 行がローカル x,y,z 軸
        n = np.einsum("dk,dkj->dj", nl, F)
        a = np.einsum("dk,dkj->dj", al, F)
        r = np.cross(a, n)
        h = c.size * 0.31
        p = cen + n * (c.size * 0.5 * 1.012) - origin
        corners = np.stack([p - r * h - a * h, p + r * h - a * h, p + r * h + a * h, p - r * h + a * h], axis=1)
        D = len(vox)
        v = np.empty((D, 4, 12), dtype=np.float32)
        v[:, :, 0:3] = corners
        v[:, :, 3:6] = n[:, None, :]
        v[:, :, 6:9] = shade(n, np.ones((D, 3)))[:, None, :]
        v[:, :, 9] = 1.0
        v[:, 0, 10:12] = uv[:, [0, 1]]
        v[:, 1, 10:12] = uv[:, [2, 1]]
        v[:, 2, 10:12] = uv[:, [2, 3]]
        v[:, 3, 10:12] = uv[:, [0, 3]]
        if getattr(self, "_new", False):
            self.mesh.set(v.reshape(-1, 12), quad_tris(D))
            self._new = False
        else:
            self.mesh.set(v.reshape(-1, 12))

    def remove(self):
        self.mesh.remove()


class PlanetView:
    """遠景用の粗い球（キューブスフィア）。"""

    def __init__(self, parent, loader, planet, n=48):
        if planet.has_water:
            n = 96   # 山や海岸線が見えるように細かく
        self.p = planet
        self.root = parent.attachNewNode(f"planet_{planet.id}")
        self.mesh = DynamicMesh(self.root, "far")
        g = np.linspace(-1, 1, n + 1)
        gx, gy = np.meshgrid(g, g, indexing="ij")
        verts, tris = [], []
        for face in range(6):
            major, neg = face // 2, face % 2
            d = np.zeros(gx.shape + (3,))
            d[..., major] = -1.0 if neg else 1.0
            d[..., (major + 1) % 3] = gx
            d[..., (major + 2) % 3] = gy
            d = d.reshape(-1, 3)
            d /= np.linalg.norm(d, axis=1, keepdims=True)
            off = len(verts) and sum(len(v) for v in verts)
            verts.append(d)
            i = np.arange(n)[:, None] * (n + 1) + np.arange(n)[None, :]
            i = i.ravel()
            t = np.stack([i, i + n + 1, i + n + 2, i, i + n + 2, i + 1], axis=1).reshape(-1, 3) + off
            tris.append(t)
        dirs = np.concatenate(verts)
        r, col = planet.far_samples(dirs)
        r = r - 0.6
        pos = dirs * r[:, None]
        from ..shape import vertex_normals
        normals = vertex_normals(pos, np.concatenate(tris))
        normals *= np.sign(np.einsum("ij,ij->i", normals, dirs) + 1e-9)[:, None]
        normals = np.where(np.einsum("ij,ij->i", normals, dirs)[:, None] > 0.2, normals, dirs)
        self.base = np.empty((len(dirs), 12), dtype=np.float32)
        self.base[:, 0:3] = pos
        self.base[:, 3:6] = normals
        self.base[:, 6:9] = shade(normals, col)
        self.base[:, 9] = 1
        self.base[:, 10:12] = 0
        self.tris = np.concatenate(tris)
        self.dirs = dirs
        self.cut = None
        e = np.linalg.norm(pos[self.tris[:, 0]] - pos[self.tris[:, 2]], axis=1).max()
        # プレイヤーのまわりは詳細な地表があるので、遠景の球をくり抜く（洞窟や穴を覆わないように）
        self.CUT = max(0.0, 46.0 - e)
        self.mesh.set(self.base, self.tris)

    def update(self, origin, player_pos=None):
        c = self.p.center - origin
        self.root.setPos(float(c[0]), float(c[1]), float(c[2]))
        cut = None
        if player_pos is not None:
            rel = np.asarray(player_pos) - self.p.center
            rn = np.linalg.norm(rel)
            if rn - self.p.radius < 250:
                cut = rel / max(rn, 1e-9)
        if cut is None and self.cut is None:
            return
        if cut is not None and self.cut is not None and np.linalg.norm(cut - self.cut) * self.p.radius < 4.0:
            return
        self.cut = cut
        tris = self.tris
        if cut is not None:
            near = np.linalg.norm(self.dirs - cut, axis=1) * self.p.radius < self.CUT
            tris = tris[~near[tris].any(axis=1)]
        self.mesh.set(self.base, tris)


class TerrainPatch:
    """プレイヤー周辺の詳細な地表（掘削跡も反映）と水面。"""

    def __init__(self, parent, loader, cells=48, step=1.0):
        from panda3d.core import TransparencyAttrib
        self.root = parent.attachNewNode("terrain_patch")
        self.mesh = DynamicMesh(self.root, "patch")
        self.water = DynamicMesh(self.root, "water")
        self.water.np.setTransparency(TransparencyAttrib.MAlpha)
        self.water.np.setDepthWrite(False)
        self.water.np.setBin("transparent", 10)
        self.loader = loader
        self.cells = cells
        self.step = step
        self.anchor = None
        self.planet = None
        self.version = -1
        self.carve_version = -1
        self.center = np.zeros(3)

    def update(self, planet, player_pos, origin, carve=None):
        if planet is None or np.linalg.norm(player_pos - planet.center) - planet.radius > 400:
            self.root.hide()
            return
        self.root.show()
        cv = carve.version if carve is not None else -1
        need = (self.planet is not planet or self.anchor is None or self.version != planet.version
                or cv != self.carve_version
                or np.linalg.norm(player_pos - self.anchor) > self.step * self.cells * 0.25)
        if need:
            self._build(planet, player_pos, carve)
        c = self.center - origin
        self.root.setPos(float(c[0]), float(c[1]), float(c[2]))

    def _build(self, pl, player_pos, carve=None):
        self.planet = pl
        self.version = pl.version
        self.carve_version = carve.version if carve is not None else -1
        use_carve = carve is not None and carve.enabled
        up = pl.up(player_pos)
        R0 = float(pl.surface_radius(up[None])[0])
        # 格子をスナップしてちらつきを防ぐ
        ref = np.array([0, 1.0, 0]) if abs(up[1]) < 0.9 else np.array([1.0, 0, 0])
        t1 = np.cross(up, ref)
        t1 /= np.linalg.norm(t1)
        t2 = np.cross(up, t1)
        n = self.cells
        g = (np.arange(-n, n + 1)) * self.step
        gx, gy = np.meshgrid(g, g, indexing="ij")
        d = up[None] * R0 + t1[None] * gx.reshape(-1, 1) + t2[None] * gy.reshape(-1, 1)
        d /= np.linalg.norm(d, axis=1, keepdims=True)
        sm = pl._sample(d)
        r, wr = sm[:, 0].copy(), sm[:, 1]
        r_nat = r.copy()
        if pl.craters and not use_carve:
            pts = pl.center + d * r[:, None]
            s = pl.sdf(pts)
            idx = np.nonzero(s > 1e-4)[0]
            if len(idx):
                maxr = max(cr[3] for cr in pl.craters)
                lo = r[idx] - 2.2 * maxr
                hi = r[idx].copy()
                dd = d[idx]
                for _ in range(16):
                    mid = (lo + hi) * 0.5
                    sm2 = pl.sdf(pl.center + dd * mid[:, None])
                    out = sm2 > 0
                    hi = np.where(out, mid, hi)
                    lo = np.where(out, lo, mid)
                r[idx] = lo
        inside = np.zeros(len(d), bool)
        if use_carve and carve.active:
            inside = carve.contains(d * r[:, None])
            r = r - inside * 0.08   # 掘削・洞窟の詳細メッシュを優先して見せる
        pos = pl.center + d * r[:, None]
        self.anchor = player_pos.copy()
        self.center = pl.center + up * R0
        rel = pos - self.center
        m = 2 * n + 1
        grid = rel.reshape(m, m, 3)
        ex = np.gradient(grid, axis=0)
        ey = np.gradient(grid, axis=1)
        nrm = np.cross(ex, ey).reshape(-1, 3)
        nrm /= np.maximum(np.linalg.norm(nrm, axis=1, keepdims=True), 1e-9)
        nrm *= np.sign((nrm * d).sum(1))[:, None]
        col = pl.surface_color(d, r_nat, nrm)
        # 掘った所は土の色に
        if pl.craters and not use_carve:
            depth = np.clip((r_nat - r) / 1.0, 0, 1)[:, None]
            col = col * (1 - depth) + np.array([0.45, 0.33, 0.22]) * depth
        v = np.empty((len(rel), 12), dtype=np.float32)
        v[:, 0:3] = rel
        v[:, 3:6] = nrm
        v[:, 6:9] = shade(nrm, col)
        v[:, 9] = 1
        v[:, 10] = (gx.ravel() / 4.0)
        v[:, 11] = (gy.ravel() / 4.0)
        i = (np.arange(m - 1)[:, None] * m + np.arange(m - 1)[None, :]).ravel()
        quads = np.stack([i, i + m, i + m + 1, i + 1], axis=1)
        if inside.any():
            quads = quads[~inside[quads].all(axis=1)]
        tris = quads[:, [0, 1, 2, 0, 2, 3]].reshape(-1, 3)
        self.mesh.set(v, tris)
        tex = load_texture(self.loader, pl.defn.get("texture"))
        if tex:
            self.mesh.np.setTexture(tex, 1)
        # 水面
        if pl.has_water:
            wet = wr > r_nat
            wq = quads[wet[quads].any(axis=1)] if len(quads) else quads
            if len(wq):
                wp = pl.center + d * wr[:, None] - self.center
                wv = np.empty((len(wp), 12), dtype=np.float32)
                wv[:, 0:3] = wp
                wv[:, 3:6] = d
                wv[:, 6:9] = shade(d, np.tile([0.2, 0.42, 0.75], (len(d), 1)))
                wv[:, 9] = 0.72
                wv[:, 10:12] = 0
                self.water.set(wv, wq[:, [0, 1, 2, 0, 2, 3]].reshape(-1, 3))
                self.water.np.show()
            else:
                self.water.np.hide()
        else:
            self.water.np.hide()


class CarveView:
    """洞窟や掘削跡のまわりを、距離場から作った詳細なメッシュで描く（8 m の立方体チャンクごと）。
    高さ場の地表（TerrainPatch）では表せない横穴・洞窟の中を見せる。scikit-image が必要。"""
    CH = 8.0
    RES = 0.5

    def __init__(self, parent, loader, view_range=56.0):
        self.enabled = True
        self.root = parent.attachNewNode("carve")
        self.mesh = DynamicMesh(self.root, "carve")
        self.loader = loader
        self.range = view_range
        self.cache = {}           # (planet id, chunk key) → (verts, tris) or None
        self.active = set()       # 描画中のチャンク（惑星ローカルの整数座標）
        self.version = 0
        self.planet = None
        self.anchor = None
        self.pver = -1
        self.n_craters = 0
        self.pending = []
        self.dirty = False

    def contains(self, local_pts):
        """惑星ローカル座標の点が描画中のチャンクに入っているか。"""
        if not self.active:
            return np.zeros(len(local_pts), bool)
        k = np.floor(np.asarray(local_pts) / self.CH).astype(np.int64)
        keys = {tuple(x) for x in self.active}
        return np.array([tuple(x) in keys for x in k], bool)

    def _wanted(self, pl, player_pos):
        local = player_pos - pl.center
        rng = self.range
        boxes = []
        if pl.has_caves:
            for k in pl.caves_near(local, rng):
                a, b = pl.cave_a[k], pl.cave_b[k]
                rr = max(pl.cave_ra[k], pl.cave_rb[k]) + 1.0
                boxes.append((np.minimum(a, b) - rr, np.maximum(a, b) + rr))
        for c in pl.craters:
            cc = np.asarray(c[:3])
            if np.linalg.norm(cc - local) < rng + c[3]:
                boxes.append((cc - c[3] - 1.0, cc + c[3] + 1.0))
        want = set()
        lo_v = np.floor((local - rng) / self.CH).astype(int)
        hi_v = np.floor((local + rng) / self.CH).astype(int)
        for lo, hi in boxes:
            i0 = np.maximum(np.floor(lo / self.CH).astype(int), lo_v)
            i1 = np.minimum(np.floor(hi / self.CH).astype(int), hi_v)
            if (i1 < i0).any():
                continue
            for x in range(i0[0], i1[0] + 1):
                for y in range(i0[1], i1[1] + 1):
                    for z in range(i0[2], i1[2] + 1):
                        want.add((x, y, z))
        if len(want) > 900:   # 多すぎるときは近い順に
            want = set(sorted(want, key=lambda k: np.linalg.norm((np.array(k) + 0.5) * self.CH - local))[:900])
        return want

    def _mesh_chunk(self, pl, key):
        from .. import shape
        n = int(round(self.CH / self.RES)) + 1
        o = np.array(key, dtype=np.float64) * self.CH
        g = np.arange(n) * self.RES
        pts = np.stack(np.meshgrid(g, g, g, indexing="ij"), -1).reshape(-1, 3) + o
        h_nat, d, c = pl.sdf_parts(pts + pl.center)
        s = d if c is None else np.maximum(d, -c)
        vol = s.reshape(n, n, n)
        if vol.min() > 0 or vol.max() < 0:
            return None
        try:
            verts, faces, _, _ = shape.marching_cubes(vol.astype(np.float32), 0.0, spacing=(self.RES,) * 3)
        except Exception:  # noqa: BLE001
            return None
        if not len(faces):
            return None
        verts = verts.astype(np.float64) + o
        wp = verts + pl.center
        _, nrm = pl.sdf_grad(wp)
        h_v, _, _ = pl.sdf_parts(wp)
        depth = np.clip(-h_v, 0, None)
        dirs = verts / np.linalg.norm(verts, axis=1, keepdims=True)
        top = pl.surface_color(dirs, np.linalg.norm(verts, axis=1) + depth, nrm)
        dirt = np.array([0.42, 0.34, 0.26])
        stone = np.array([0.44, 0.42, 0.4])
        under = dirt * (1 - np.clip((depth - 1.5) / 4, 0, 1))[:, None] + stone * np.clip((depth - 1.5) / 4, 0, 1)[:, None]
        k = np.clip(depth / 0.6, 0, 1)[:, None]
        col = top * (1 - k) + under * k
        col = pl.underground_tint(verts, col)
        # 地中は日が当たらないので、向きによる陰影を弱めて少し暗くする（真っ暗にはしない）
        deep = np.clip((depth - 1.0) / 6.0, 0, 1)[:, None]
        lit = shade(nrm, col) * (1 - deep) + col * (0.62 + 0.18 * np.clip(nrm @ SUN, 0, 1))[:, None] * deep
        v = np.empty((len(verts), 12), dtype=np.float32)
        v[:, 3:6] = nrm
        v[:, 6:9] = lit
        v[:, 9] = 1
        ax = np.abs(nrm).argmax(1)
        u = np.where(ax == 0, verts[:, 1], verts[:, 0])
        w = np.where(ax == 2, verts[:, 1], verts[:, 2])
        v[:, 10] = u / 4.0
        v[:, 11] = w / 4.0
        return verts, v, faces.astype(np.int64)

    def update(self, pl, player_pos, origin):
        if not self.enabled or pl is None or np.linalg.norm(player_pos - pl.center) - pl.radius > 300:
            if self.active:
                self.active = set()
                self.version += 1
            self.root.hide()
            return
        self.root.show()
        if pl is not self.planet:
            self.planet, self.anchor, self.cache, self.n_craters = pl, None, {}, 0
            self.active = set()
        # 新しい掘削跡が触れるチャンクは作り直す
        if len(pl.craters) != self.n_craters:
            for c in pl.craters[self.n_craters:]:
                cc = np.asarray(c[:3])
                for key in list(self.cache):
                    lo = np.array(key) * self.CH - c[3] - 1.0
                    hi = lo + self.CH + 2 * c[3] + 2.0
                    if ((cc >= lo) & (cc <= hi)).all():
                        self.cache.pop(key)
            self.n_craters = len(pl.craters)
            self.anchor = None
        if self.anchor is None or np.linalg.norm(player_pos - self.anchor) > 6.0:
            self.anchor = player_pos.copy()
            want = self._wanted(pl, player_pos)
            self.pending = sorted((k for k in want if k not in self.cache),
                                  key=lambda k: np.linalg.norm((np.array(k) + 0.5) * self.CH
                                                               - (player_pos - pl.center)))
            self.want = want
            self.dirty = True
        # 1フレームに作るチャンクは時間で制限（カクつき防止）
        import time
        t0 = time.perf_counter()
        while self.pending and time.perf_counter() - t0 < 0.012:
            k = self.pending.pop(0)
            if k not in self.cache:
                self.cache[k] = self._mesh_chunk(pl, k)
            self.dirty = True
        if self.dirty and (not self.pending or time.perf_counter() - getattr(self, "_last_asm", 0) > 0.4):
            self._last_asm = time.perf_counter()
            self._assemble()
        c = pl.center - origin
        self.root.setPos(float(c[0]), float(c[1]), float(c[2]))

    def _assemble(self):
        self.dirty = False
        vs, ts, n = [], [], 0
        active = set()
        for k in self.want:
            m = self.cache.get(k)
            if m is None:
                continue
            active.add(k)
            verts, v, f = m
            vv = v.copy()
            vv[:, 0:3] = verts
            vs.append(vv)
            ts.append(f + n)
            n += len(vv)
        if vs:
            self.mesh.set(np.concatenate(vs), np.concatenate(ts))
            tex = load_texture(self.loader, self.planet.defn.get("texture"))
            if tex:
                self.mesh.np.setTexture(tex, 1)
            self.mesh.np.show()
        else:
            self.mesh.np.hide()
        # 画面外のキャッシュを捨てる
        if len(self.cache) > 2500:
            for k in list(self.cache):
                if k not in self.want:
                    self.cache.pop(k)
        if active != self.active:
            self.active = active
            self.version += 1


class DisplayView:
    """コンピューターの表面に表示する文字（看板）。"""

    def __init__(self, parent):
        from ursina import Entity, Text, color
        self.root = Entity(parent=parent)
        self.bg = Entity(parent=self.root, model="quad", color=color.black, z=0.0)
        self.bg.setTwoSided(True)
        self.text = Text(parent=self.root, text="", origin=(-0.5, 0), start_tag="\x01", end_tag="\x02", z=-0.004)
        self.text.setDepthOffset(2)
        self.key = None
        self.placed = None
        self.U_color = color

    def set(self, disp, voxel_size):
        key = (disp["text"], disp.get("width"), tuple(disp.get("color", ())), tuple(disp.get("background", ())),
               voxel_size)
        if key == self.key:
            return
        self.key = key
        col = disp.get("color", [1, 1, 1])
        bgc = disp.get("background", [0.04, 0.05, 0.08, 0.92])
        self.text.text = disp["text"]
        self.text.color = self.U_color.rgba(col[0], col[1], col[2], 1)
        self.text.scale = 1
        w_target = float(disp.get("width") or voxel_size * 0.96)
        tw, th = max(self.text.width, 1e-6), max(self.text.height, 1e-6)
        k = w_target * 0.9 / tw
        h = th * k / 0.88
        if disp.get("width") is None:  # ブロックの大きさに収める
            if h > voxel_size * 0.96:
                k *= voxel_size * 0.96 / h
                h = voxel_size * 0.96
            h = max(h, voxel_size * 0.96)
        self.text.scale = k
        self.text.y = 0
        self.text.x = -tw * k / 2  # 左揃え・全体は中央
        self.bg.scale = (w_target, h)
        # 背景は不透明にして、奥の看板の文字が透けないようにする
        self.bg.color = self.U_color.rgb(*bgc[:3])

    def place(self, center, right, up, fwd):
        from panda3d.core import LMatrix4f
        self.root.setMat(LMatrix4f(right[0], right[1], right[2], 0, up[0], up[1], up[2], 0,
                                   fwd[0], fwd[1], fwd[2], 0, center[0], center[1], center[2], 1))

    def remove(self):
        from .ui_destroy import destroy
        destroy(self.root)


def display_faces(c, i, disp):
    """表示する面のリスト（他のボクセルで塞がれた面は除く）。"""
    from ..construct import DIR_TUPLES, link_key
    f = disp.get("face", 2)
    faces = range(6) if f == "all" else [int(f)]
    cell = c.cells[i]
    out = []
    for d in faces:
        nb = (cell[0] + DIR_TUPLES[d][0], cell[1] + DIR_TUPLES[d][1], cell[2] + DIR_TUPLES[d][2])
        if link_key(cell, nb) in c.links:
            continue
        out.append(d)
    return out


def face_frame_p(P8, F, d, size):
    """ボクセルの8頂点 P8 とローカル軸 F から、面 d の (中心, 右, 上, 手前向き)。"""
    axis, sign = d // 2, (1.0 if d % 2 == 0 else -1.0)
    n = F[axis] * sign
    up = F[2] * (-sign) if axis == 1 else F[1]
    up = up - n * np.dot(up, n)
    up /= max(np.linalg.norm(up), 1e-9)
    fwd = -n
    right = np.cross(up, fwd)
    center = P8.mean(0) + n * (size * 0.5 + 0.004)
    return center, right, up, fwd


def face_frame(c, i, d, frames):
    F = frames[i]
    axis, sign = d // 2, (1.0 if d % 2 == 0 else -1.0)
    n = F[axis] * sign
    up = F[2] * (-sign) if axis == 1 else F[1]
    up = up - n * np.dot(up, n)
    up /= max(np.linalg.norm(up), 1e-9)
    fwd = -n
    right = np.cross(up, fwd)
    center = c.pos[8 * i:8 * i + 8].mean(0) + n * (c.size * 0.5 + 0.004)
    return center, right, up, fwd
