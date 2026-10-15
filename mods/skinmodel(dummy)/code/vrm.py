"""VRM（.vrm = glTF バイナリ + VRM の拡張）を読む。画面のライブラリは使わない（numpy だけ）。

読むもの:
    骨（ノードの親子と初期姿勢）、人型の骨の対応（hips・leftUpperArm など）、
    メッシュ（位置・法線・UV・骨の番号と重み・三角形）、素材（色・テクスチャ・透明）、画像（PNG/JPEG のバイト列）、
    まばたきの形（ブレンドシェイプ）、作者・利用条件（meta）
VRM 0.x（extensions.VRM）と VRM 1.0（extensions.VRMC_vrm）の両方に対応する。
"""
import json
import struct

import numpy as np

_CTYPE = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
_NCOMP = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}
_NORM = {5120: 127.0, 5121: 255.0, 5122: 32767.0, 5123: 65535.0}


class VRMError(Exception):
    pass


# --- 四元数（glTF と同じ x, y, z, w の順） --------------------------------------
def qmul(a, b):
    a = np.asarray(a, dtype=np.float64)
    b = np.asarray(b, dtype=np.float64)
    ax, ay, az, aw = a[..., 0], a[..., 1], a[..., 2], a[..., 3]
    bx, by, bz, bw = b[..., 0], b[..., 1], b[..., 2], b[..., 3]
    out = np.empty(np.broadcast(ax, bx).shape + (4,))
    out[..., 0] = aw * bx + ax * bw + ay * bz - az * by
    out[..., 1] = aw * by - ax * bz + ay * bw + az * bx
    out[..., 2] = aw * bz + ax * by - ay * bx + az * bw
    out[..., 3] = aw * bw - ax * bx - ay * by - az * bz
    return out


def qinv(q):
    q = np.asarray(q, dtype=np.float64)
    return q * np.array([-1.0, -1.0, -1.0, 1.0])


def qaxis(axis, ang):
    a = np.asarray(axis, dtype=np.float64)
    n = np.linalg.norm(a)
    if n < 1e-12 or abs(ang) < 1e-12:
        return np.array([0.0, 0.0, 0.0, 1.0])
    s = np.sin(ang / 2) / n
    return np.array([a[0] * s, a[1] * s, a[2] * s, np.cos(ang / 2)])


def qrot(q, v):
    """四元数 q で v を回す。"""
    q = np.asarray(q, dtype=np.float64)
    u, w = q[..., :3], q[..., 3:4]
    v = np.asarray(v, dtype=np.float64)
    t = 2 * np.cross(u, v)
    return v + w * t + np.cross(u, t)


def qmat(q):
    """(..., 4) → (..., 3, 3) 回転行列（列ベクトルにかける形）。"""
    q = np.asarray(q, dtype=np.float64)
    x, y, z, w = q[..., 0], q[..., 1], q[..., 2], q[..., 3]
    m = np.empty(q.shape[:-1] + (3, 3))
    m[..., 0, 0] = 1 - 2 * (y * y + z * z)
    m[..., 0, 1] = 2 * (x * y - z * w)
    m[..., 0, 2] = 2 * (x * z + y * w)
    m[..., 1, 0] = 2 * (x * y + z * w)
    m[..., 1, 1] = 1 - 2 * (x * x + z * z)
    m[..., 1, 2] = 2 * (y * z - x * w)
    m[..., 2, 0] = 2 * (x * z - y * w)
    m[..., 2, 1] = 2 * (y * z + x * w)
    m[..., 2, 2] = 1 - 2 * (x * x + y * y)
    return m


# --- 読み込み ----------------------------------------------------------------
class Mesh:
    """1つのメッシュ（頂点は共有、三角形は素材ごと）。"""

    def __init__(self):
        self.pos = self.nrm = self.uv = self.joints = self.weights = self.morph = None
        self.parts = []         # [(三角形の頂点番号 (M,), 素材番号)]


class VRM:
    def __init__(self, path):
        self.path = str(path)
        with open(path, "rb") as fh:
            data = fh.read()
        if len(data) < 20 or data[:4] != b"glTF":
            raise VRMError("VRM（glTF バイナリ）ではありません")
        ver, length = struct.unpack("<II", data[4:12])
        if ver != 2:
            raise VRMError(f"glTF のバージョン {ver} には対応していません")
        off, js, bin_ = 12, None, b""
        while off + 8 <= min(length, len(data)):
            clen, ctype = struct.unpack("<II", data[off:off + 8])
            chunk = data[off + 8:off + 8 + clen]
            if ctype == 0x4E4F534A:
                js = json.loads(chunk.decode("utf-8"))
            elif ctype == 0x004E4942:
                bin_ = chunk
            off += 8 + clen
        if js is None:
            raise VRMError("glTF の JSON がありません")
        self.js, self.bin = js, bin_
        ext = js.get("extensions", {})
        if "VRMC_vrm" in ext:
            self.version = 1
            self.ext = ext["VRMC_vrm"]
        elif "VRM" in ext:
            self.version = 0
            self.ext = ext["VRM"]
        else:
            raise VRMError("VRM の情報（extensions.VRM / VRMC_vrm）がありません。普通の glTF は読めません")
        self.meta = self.ext.get("meta", {}) or {}
        self._nodes()
        self._humanoid()
        self._materials()
        self._skins()
        self._meshes()
        self._blink()

    # --- 中身の取り出し ------------------------------------------------------
    def accessor(self, i):
        a = self.js["accessors"][i]
        ct, n, nc = a["componentType"], a["count"], _NCOMP[a["type"]]
        dt = np.dtype(_CTYPE[ct])
        if "bufferView" in a:
            out = self._view(a["bufferView"], a.get("byteOffset", 0), dt, n, nc)
        else:
            out = np.zeros((n, nc), dt)
        sp = a.get("sparse")
        if sp:
            out = out.copy()
            idx = self._view(sp["indices"]["bufferView"], sp["indices"].get("byteOffset", 0),
                             np.dtype(_CTYPE[sp["indices"]["componentType"]]), sp["count"], 1)[:, 0]
            val = self._view(sp["values"]["bufferView"], sp["values"].get("byteOffset", 0), dt, sp["count"], nc)
            out[idx.astype(np.int64)] = val
        if a.get("normalized") and ct in _NORM:
            out = out.astype(np.float32) / _NORM[ct]
        return out

    def _view(self, vi, boff, dt, n, nc):
        bv = self.js["bufferViews"][vi]
        if bv.get("buffer", 0) != 0:
            raise VRMError("外部ファイルのバッファには対応していません")
        start = bv.get("byteOffset", 0) + boff
        stride = bv.get("byteStride") or dt.itemsize * nc
        if n == 0:
            return np.zeros((0, nc), dt)
        need = stride * (n - 1) + dt.itemsize * nc
        raw = np.frombuffer(self.bin, np.uint8, need, start)
        if stride == dt.itemsize * nc:
            return raw.view(dt).reshape(n, nc)
        rows = np.lib.stride_tricks.as_strided(raw, (n, dt.itemsize * nc), (stride, 1))
        return np.ascontiguousarray(rows).view(dt).reshape(n, nc)

    def image_bytes(self, i):
        im = self.js["images"][i]
        if "bufferView" not in im:
            raise VRMError("外部ファイルの画像には対応していません")
        bv = self.js["bufferViews"][im["bufferView"]]
        s = bv.get("byteOffset", 0)
        return bytes(self.bin[s:s + bv["byteLength"]]), im.get("mimeType", "image/png")

    def _nodes(self):
        nodes = self.js.get("nodes", [])
        n = len(nodes)
        self.names = [nd.get("name", f"node{i}") for i, nd in enumerate(nodes)]
        self.parent = np.full(n, -1, np.int64)
        self.T = np.zeros((n, 3))
        self.R = np.tile([0.0, 0.0, 0.0, 1.0], (n, 1))
        self.S = np.ones((n, 3))
        for i, nd in enumerate(nodes):
            for c in nd.get("children", []):
                self.parent[c] = i
            if "matrix" in nd:
                m = np.array(nd["matrix"], dtype=np.float64).reshape(4, 4).T
                self.T[i] = m[:3, 3]
                sc = np.linalg.norm(m[:3, :3], axis=0)
                self.S[i] = sc
                self.R[i] = _mat_to_quat(m[:3, :3] / sc)
            else:
                self.T[i] = nd.get("translation", [0, 0, 0])
                self.R[i] = nd.get("rotation", [0, 0, 0, 1])
                self.S[i] = nd.get("scale", [1, 1, 1])
        # 親から順に計算するための段（深さ）
        depth = np.zeros(n, np.int64)
        for i in range(n):
            d, p = 0, self.parent[i]
            while p >= 0:
                d, p = d + 1, self.parent[p]
                if d > n:
                    raise VRMError("ノードの親子が循環しています")
            depth[i] = d
        self.levels = [np.nonzero(depth == d)[0] for d in range(int(depth.max()) + 1 if n else 0)]
        self.rest_global = self.globals(self.T, self.R, self.S)

    def globals(self, T, R, S):
        """各ノードのワールド（モデル）行列 (n, 4, 4)。"""
        n = len(T)
        L = np.zeros((n, 4, 4))
        L[:, :3, :3] = qmat(R) * S[:, None, :]
        L[:, :3, 3] = T
        L[:, 3, 3] = 1
        G = L.copy()
        for lv in self.levels[1:]:
            G[lv] = self.parent_mats(G, lv) @ L[lv]
        return G

    def parent_mats(self, G, lv):
        return G[self.parent[lv]]

    def _humanoid(self):
        hb = self.ext.get("humanoid", {}).get("humanBones", {})
        self.human = {}
        if isinstance(hb, list):            # VRM 0.x
            for h in hb:
                if "bone" in h and "node" in h and h["node"] >= 0:
                    self.human[h["bone"]] = int(h["node"])
        else:                               # VRM 1.0
            for k, v in hb.items():
                if isinstance(v, dict) and v.get("node", -1) >= 0:
                    self.human[k] = int(v["node"])
        need = ("hips", "spine", "head", "leftUpperLeg", "rightUpperLeg", "leftLowerLeg", "rightLowerLeg",
                "leftUpperArm", "rightUpperArm", "leftLowerArm", "rightLowerArm")
        miss = [b for b in need if b not in self.human]
        if miss:
            raise VRMError("人型の骨が足りません: " + ", ".join(miss))

    def _materials(self):
        props = {}
        if self.version == 0:
            for mp in self.ext.get("materialProperties", []):
                props[mp.get("name")] = mp
        self.materials = []
        for m in self.js.get("materials", []):
            pbr = m.get("pbrMetallicRoughness", {})
            bct = pbr.get("baseColorTexture")
            tex = None
            uvt = ((0.0, 0.0), (1.0, 1.0))
            if bct is not None:
                tex = self.js["textures"][bct["index"]].get("source")
                tt = bct.get("extensions", {}).get("KHR_texture_transform")
                if tt:
                    uvt = (tuple(tt.get("offset", (0, 0))), tuple(tt.get("scale", (1, 1))))
            alpha = m.get("alphaMode", "OPAQUE")
            cutoff = float(m.get("alphaCutoff", 0.5))
            shade = None
            mp = props.get(m.get("name"))
            if mp:                          # VRM 0.x の MToon の値
                fp, vp = mp.get("floatProperties", {}), mp.get("vectorProperties", {})
                bm = int(fp.get("_BlendMode", 0))
                alpha = {0: "OPAQUE", 1: "MASK", 2: "BLEND", 3: "BLEND"}.get(bm, alpha)
                cutoff = float(fp.get("_Cutoff", cutoff))
                if "_ShadeColor" in vp:
                    shade = [float(x) for x in vp["_ShadeColor"][:3]]
            mt = m.get("extensions", {}).get("VRMC_materials_mtoon")
            if mt and "shadeColorFactor" in mt:
                shade = [float(x) for x in mt["shadeColorFactor"][:3]]
            self.materials.append({
                "name": m.get("name", ""), "texture": tex, "uv": uvt,
                "color": [float(x) for x in pbr.get("baseColorFactor", [1, 1, 1, 1])],
                "alpha": alpha, "cutoff": cutoff, "double": bool(m.get("doubleSided", False)),
                "shade": shade or [0.72, 0.72, 0.78],
            })

    def _skins(self):
        """全部のスキンの骨を1つの表にまとめる（同じノードでもスキンごとに別の行）。"""
        self.joint_node, ibms, self.skin_base = [], [], []
        for sk in self.js.get("skins", []):
            self.skin_base.append(len(self.joint_node))
            j = sk["joints"]
            if "inverseBindMatrices" in sk:
                ibm = self.accessor(sk["inverseBindMatrices"]).astype(np.float64).reshape(-1, 4, 4).transpose(0, 2, 1)
            else:
                ibm = np.tile(np.eye(4), (len(j), 1, 1))
            self.joint_node += list(j)
            ibms.append(ibm)
        self.joint_node = np.array(self.joint_node, np.int64)
        self.ibm = np.concatenate(ibms) if ibms else np.zeros((0, 4, 4))

    def _meshes(self):
        """シーンに置かれたメッシュ（スキンつきは骨で、スキン無しは置かれたノードの骨 1 本で動かす）。"""
        self.meshes = []
        self.mesh_of = {}           # glTF のメッシュ番号 → [self.meshes の番号]
        extra_nodes = []
        for ni, nd in enumerate(self.js.get("nodes", [])):
            if "mesh" not in nd:
                continue
            gm = self.js["meshes"][nd["mesh"]]
            if "skin" in nd:
                base = self.skin_base[nd["skin"]]
                rigid = None
            else:
                base, rigid = None, ni
                extra_nodes.append(ni)
            shared = {}             # 同じ頂点を使う（素材だけ違う）三角形はまとめる
            for pr in gm.get("primitives", []):
                if pr.get("mode", 4) != 4:
                    continue        # 三角形以外（線・点）は描かない
                at = pr["attributes"]
                key = (tuple(sorted(at.items())), tuple(tuple(sorted(t.items())) for t in pr.get("targets", [])))
                if key in shared:
                    m = shared[key]
                    idx = self.accessor(pr["indices"])[:, 0].astype(np.uint32) if "indices" in pr else \
                        np.arange(len(m.pos), dtype=np.uint32)
                    m.parts.append((idx, pr.get("material", -1)))
                    continue
                m = Mesh()
                shared[key] = m
                m.pos = self.accessor(at["POSITION"]).astype(np.float32)
                nv = len(m.pos)
                m.nrm = self.accessor(at["NORMAL"]).astype(np.float32) if "NORMAL" in at else np.zeros((nv, 3), np.float32)
                m.uv = self.accessor(at["TEXCOORD_0"]).astype(np.float32) if "TEXCOORD_0" in at else np.zeros((nv, 2), np.float32)
                if rigid is None and "JOINTS_0" in at:
                    m.joints = self.accessor(at["JOINTS_0"]).astype(np.int64) + base
                    w = self.accessor(at["WEIGHTS_0"]).astype(np.float32)
                    s = w.sum(1, keepdims=True)
                    m.weights = np.where(s > 0, w / np.maximum(s, 1e-9), np.array([1, 0, 0, 0], np.float32))
                else:
                    m.joints = None         # 後で「ノードそのもの」の骨を足す
                    m.rigid_node = ni
                    m.weights = np.tile(np.array([1, 0, 0, 0], np.float32), (nv, 1))
                idx = self.accessor(pr["indices"])[:, 0].astype(np.uint32) if "indices" in pr else \
                    np.arange(nv, dtype=np.uint32)
                m.parts = [(idx, pr.get("material", -1))]
                m.targets = [self.accessor(t["POSITION"]).astype(np.float32) if "POSITION" in t else None
                             for t in pr.get("targets", [])]
                m.morph = np.zeros((nv, 3), np.float32)
                self.mesh_of.setdefault(nd["mesh"], []).append(len(self.meshes))
                self.meshes.append(m)
        # スキンの無いメッシュは、置かれたノードを骨として足す（逆バインド行列は単位行列）
        for m in self.meshes:
            if m.joints is None:
                j = len(self.joint_node)
                self.joint_node = np.append(self.joint_node, m.rigid_node)
                self.ibm = np.concatenate([self.ibm, np.eye(4)[None]])
                m.joints = np.full((len(m.pos), 4), j, np.int64)
        if not self.meshes:
            raise VRMError("メッシュがありません")

    def _blink(self):
        """まばたきの形（両目）。メッシュの頂点に「閉じたときのずれ」を足しておく。"""
        binds = []
        if self.version == 0:
            for g in self.ext.get("blendShapeMaster", {}).get("blendShapeGroups", []):
                if (g.get("presetName") or "").lower() == "blink" or g.get("name", "").lower() == "blink":
                    binds = [(b["mesh"], b["index"], float(b.get("weight", 100)) / 100) for b in g.get("binds", [])]
                    break
        else:
            ex = self.ext.get("expressions", {}).get("preset", {}).get("blink", {})
            for b in ex.get("morphTargetBinds", []):
                nd = self.js["nodes"][b["node"]]
                if "mesh" in nd:
                    binds.append((nd["mesh"], b["index"], float(b.get("weight", 1.0))))
        self.has_blink = False
        for mesh_i, ti, w in binds:
            for k in self.mesh_of.get(mesh_i, []):
                m = self.meshes[k]
                if ti < len(m.targets) and m.targets[ti] is not None:
                    m.morph += m.targets[ti] * w
                    self.has_blink = True
        for m in self.meshes:
            m.targets = None        # 使い終わった形は捨てる（メモリ）

    # --- 姿勢 ----------------------------------------------------------------
    def bone_pos(self, bone):
        return self.rest_global[self.human[bone], :3, 3].copy()

    def basis(self):
        """モデルの (右, 上, 前) の向き（モデル座標）。腕と頭の位置から求める（VRM 0 は -Z、1.0 は +Z が前）。"""
        up = self.bone_pos("head") - self.bone_pos("hips")
        up /= np.linalg.norm(up)
        right = self.bone_pos("rightUpperArm") - self.bone_pos("leftUpperArm")
        right -= up * (right @ up)
        right /= np.linalg.norm(right)
        fwd = np.cross(up, right)            # 右手系（glTF）では 上 × 右 = 前
        return right, up, fwd

    def bounds(self):
        lo = np.min([m.pos.min(0) for m in self.meshes], 0)
        hi = np.max([m.pos.max(0) for m in self.meshes], 0)
        return lo, hi

    def pose(self, rot=None, hips_offset=None, spring=None):
        """骨の行列 (J, 3, 4)（シェーダーに送る）。
        rot: {ノード番号: 四元数}  初期姿勢からの回転（モデル座標の向きで表す。骨の位置を中心に回す）
        hips_offset: hips の位置のずれ（モデル座標）
        spring: (Spring, A, t, dt)  揺れもの（A, t はモデル → ワールドの変換）"""
        T, R = self.T, self.R
        if rot or hips_offset is not None:
            T, R = T.copy(), R.copy()
            if rot:
                n = np.fromiter(rot.keys(), np.int64, len(rot))
                q = np.array(list(rot.values()))
                W = self.rest_rot[n]
                R[n] = qmul(R[n], qmul(qinv(W), qmul(q, W)))
            if hips_offset is not None:
                h = self.human["hips"]
                p = self.parent[h]
                off = np.asarray(hips_offset, dtype=np.float64)
                if p >= 0:          # 親の向きに直す
                    off = np.linalg.solve(self.rest_global[p, :3, :3], off)
                T[h] = T[h] + off
        G = self.globals(T, R, self.S)
        if spring is not None and spring[0].n:
            sp, A, t, dt = spring
            G, R2 = sp.step(G, T, R, A, t, dt)
            if not sp.complete:
                G = self.globals(T, R2, self.S)
        M = G[self.joint_node] @ self.ibm
        return M[:, :3, :]

    @property
    def rest_rot(self):
        if not hasattr(self, "_rest_rot"):
            m = self.rest_global[:, :3, :3]
            m = m / np.linalg.norm(m, axis=1, keepdims=True)
            self._rest_rot = _mat_to_quat(m)
        return self._rest_rot


def _mat_to_quat(m):
    """回転行列 (..., 3, 3) → 四元数 (..., 4)。"""
    m = np.asarray(m, dtype=np.float64)
    single = m.ndim == 2
    m = m.reshape(-1, 3, 3)
    m00, m11, m22 = m[:, 0, 0], m[:, 1, 1], m[:, 2, 2]
    q = np.empty((len(m), 4))
    q[:, 3] = np.sqrt(np.maximum(0, 1 + m00 + m11 + m22)) / 2
    q[:, 0] = np.sqrt(np.maximum(0, 1 + m00 - m11 - m22)) / 2
    q[:, 1] = np.sqrt(np.maximum(0, 1 - m00 + m11 - m22)) / 2
    q[:, 2] = np.sqrt(np.maximum(0, 1 - m00 - m11 + m22)) / 2
    q[:, 0] = np.copysign(q[:, 0], m[:, 2, 1] - m[:, 1, 2])
    q[:, 1] = np.copysign(q[:, 1], m[:, 0, 2] - m[:, 2, 0])
    q[:, 2] = np.copysign(q[:, 2], m[:, 1, 0] - m[:, 0, 1])
    q /= np.linalg.norm(q, axis=1, keepdims=True)
    return q[0] if single else q
