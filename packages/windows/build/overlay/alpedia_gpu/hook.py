"""render.ConstructView に GPU 描画を差し込む。"""
import builtins
import logging
from pathlib import Path

import numpy as np

SHADERS = Path(__file__).resolve().parent / "shaders"
L = logging.getLogger("alpedia.gpu")

_state = {"mode": "auto", "resolved": None, "reason": "", "shaders": {}}


def status():
    """('compute' | 'vertex' | 'off', 理由)"""
    return _state["resolved"] or "未確定", _state["reason"]


# ---------------------------------------------------------------------------
# GPU の能力を調べてシェーダーを作る（最初の構築物を描く時に1回だけ）
# ---------------------------------------------------------------------------
def _src(name, header):
    return header + (SHADERS / name).read_text(encoding="utf-8")


def _resolve():
    if _state["resolved"] is not None:
        return _state["resolved"]
    mode = _state["mode"]
    res, why = "off", ""
    try:
        from panda3d.core import Shader
        gsg = builtins.base.win.getGsg()
        mi = gsg.getDriverShaderVersionMinor()
        glsl = gsg.getDriverShaderVersionMajor() * 100 + (mi if mi >= 10 else mi * 10)   # 4.50 → 450
        info = f"{gsg.getDriverVendor()} / {gsg.getDriverRenderer()} / GL {gsg.getDriverVersion()} / GLSL {glsl}"
        L.info("GPU: %s", info)
        if mode == "off":
            why = "設定で無効"
        else:
            want = []
            if mode in ("auto", "compute") and gsg.getSupportsComputeShaders() and glsl >= 430:
                want.append("compute")
            if mode in ("auto", "vertex", "compute") and glsl >= 330:
                want.append("vertex")
            for m in want:
                head = "#version 430\n#define USE_COMPUTE\n" if m == "compute" else "#version 330\n"
                sh = {"draw": Shader.make(Shader.SL_GLSL, vertex=_src("blob.vert", head),
                                          fragment=_src("lit.frag", head))}
                if m == "compute":
                    sh["skin"] = Shader.makeCompute(Shader.SL_GLSL, _src("skin.comp", ""))
                if all(s is not None for s in sh.values()):
                    _state["shaders"] = sh
                    res = m
                    break
                L.warning("GPU: %s 用シェーダーを作れませんでした", m)
            why = info if res != "off" else f"GLSL {glsl} では使えません（{info}）"
    except Exception as e:  # noqa: BLE001
        res, why = "off", f"確認に失敗: {e}"
    _state["resolved"], _state["reason"] = res, why
    L.info("GPU 描画: %s（%s）", res, why)
    return res


# ---------------------------------------------------------------------------
# 形ごとの GPU データ
# ---------------------------------------------------------------------------
def _buffer(n, ttype, fmt, data=None, dynamic=False):
    from panda3d.core import GeomEnums, Texture
    t = Texture("buf")
    t.setupBufferTexture(max(int(n), 1), ttype, fmt, GeomEnums.UH_dynamic if dynamic else GeomEnums.UH_static)
    if data is not None:
        t.setRamImage(np.ascontiguousarray(data).tobytes())
    return t


def _adjacency(faces, n):
    """頂点ごとの隣接三角形 (j, k)：外積 (Pj-Pi)×(Pk-Pi) が三角形の法線（面積×2）になる順。"""
    a, b, c = faces[:, 0], faces[:, 1], faces[:, 2]
    owner = np.concatenate([a, b, c])
    pair = np.concatenate([np.stack([b, c], 1), np.stack([c, a], 1), np.stack([a, b], 1)])
    order = np.argsort(owner, kind="stable")
    owner, pair = owner[order], pair[order]
    count = np.bincount(owner, minlength=n)
    start = np.concatenate([[0], np.cumsum(count)[:-1]])
    return pair.astype(np.int32), start, count


class GpuBlob:
    def __init__(self, view, mode):
        from panda3d.core import Texture
        c = view.c
        m = view.blob
        self.view = view
        self.mode = mode
        M = len(m["rest"])
        self.n_nodes = len(c.pos)
        pair, start, count = _adjacency(m["faces"], M)
        self.tex = {
            "node_pos": _buffer(self.n_nodes, Texture.T_float, Texture.F_rgba32, dynamic=True),
            "v_owner": _buffer(M, Texture.T_int, Texture.F_r32i, m["owner"].astype(np.int32)),
            "v_w": _buffer(2 * M, Texture.T_float, Texture.F_rgba32, m["w"].astype(np.float32).reshape(-1, 4)),
            "adj": _buffer(2 * len(pair), Texture.T_int, Texture.F_r32i, pair.ravel()),
            "firing": _buffer(max(c.n_vox, 1), Texture.T_float, Texture.F_r32, dynamic=True),
        }
        self._firing = None
        self._set_firing(np.zeros(max(c.n_vox, 1), dtype=np.float32))
        if mode == "compute":
            sk = _buffer(M, Texture.T_float, Texture.F_rgba32, dynamic=True)
            sk.setClearColor((0, 0, 0, 0))
            sk.clearImage()
            self.tex["skinned"] = sk
        self.info = np.stack([np.arange(M), start, count, m["owner"]], 1).astype(np.float32)
        self.cn = None
        self.active = False

    # 描画メッシュを GPU 用の頂点形式に作り直す
    def activate(self):
        from panda3d.core import (ComputeNode, Geom, GeomEnums, GeomTriangles, GeomVertexArrayFormat,
                                  GeomVertexData, GeomVertexFormat, InternalName, OmniBoundingVolume)
        from main.client.render import SUN, AMBIENT
        sh = _state["shaders"]
        arr = GeomVertexArrayFormat()
        arr.addColumn(InternalName.getVertex(), 3, Geom.NT_float32, Geom.C_point)
        arr.addColumn(InternalName.getColor(), 4, Geom.NT_float32, Geom.C_color)
        arr.addColumn(InternalName.getTexcoord(), 2, Geom.NT_float32, Geom.C_texcoord)
        arr.addColumn(InternalName.make("skin_info"), 4, Geom.NT_float32, Geom.C_other)
        fmt = GeomVertexFormat.registerFormat(GeomVertexFormat(arr))
        rest = self.view.blob["rest"].astype(np.float32)
        for g in self.view.groups:
            vs = g["verts"]
            v = np.empty((len(vs), 13), dtype=np.float32)
            v[:, 0:3] = rest[vs]
            v[:, 3:6] = g["cols"]
            v[:, 6] = 1.0
            v[:, 7:9] = g["uv"]
            v[:, 9:13] = self.info[vs]
            vd = GeomVertexData("gpu", fmt, Geom.UH_static)
            vd.uncleanSetNumRows(len(vs))
            memoryview(vd.modifyArray(0)).cast("B")[:] = v.tobytes()
            tris = np.ascontiguousarray(g["tris"], dtype=np.uint32).ravel()
            prim = GeomTriangles(Geom.UH_static)
            prim.setIndexType(GeomEnums.NT_uint32)
            pa = prim.modifyVertices()
            pa.uncleanSetNumRows(len(tris))
            memoryview(pa).cast("B")[:] = tris.tobytes()
            geom = Geom(vd)
            geom.addPrimitive(prim)
            mesh = g["mesh"]
            mesh.node.removeAllGeoms()
            mesh.node.addGeom(geom)
            mesh.vdata = None                     # CPU 側に戻る時は作り直させる
            mesh.n_rows = -1
            mesh.np.setShader(sh["draw"])
            mesh.np.setShaderInput("sun_dir", tuple(float(x) for x in SUN))
            mesh.np.setShaderInput("ambient", float(AMBIENT))
            for k in ("adj", "firing") + (("skinned",) if self.mode == "compute" else ("node_pos", "v_owner", "v_w")):
                mesh.np.setShaderInput(k, self.tex[k])
            mesh.np.setMat(_ident())
            g["new"] = False
        if self.mode == "compute" and self.cn is None:
            cn = ComputeNode("alpedia-skin")
            cn.addDispatch((len(self.info) + 63) // 64, 1, 1)
            cn.setBounds(OmniBoundingVolume())
            cn.setFinal(True)
            self.cn = self.view.parent.attachNewNode(cn)
            self.cn.setShader(sh["skin"])
            for k in ("node_pos", "v_owner", "v_w", "skinned"):
                self.cn.setShaderInput(k, self.tex[k])
            self.cn.setBin("background", 0)      # 描画より先に計算させる
        if self.cn is not None:
            self.cn.show()
        self.active = True

    def deactivate(self):
        """剛体になった時など：CPU の描画に戻す。"""
        for g in self.view.groups:
            g["mesh"].np.clearShader()
            g["new"] = True
        if self.cn is not None:
            self.cn.hide()
        self.active = False

    def _set_firing(self, f):
        f = np.asarray(f, dtype=np.float32)
        if self._firing is not None and len(f) == len(self._firing) and np.array_equal(f, self._firing):
            return
        self._firing = f.copy()
        self.tex["firing"].setRamImage(f.tobytes())

    def update(self, origin, highlight_flames):
        if not self.active:
            self.activate()
        c = self.view.c
        p = np.empty((self.n_nodes, 4), dtype=np.float32)
        p[:, :3] = c.pos - origin
        p[:, 3] = 1.0
        self.tex["node_pos"].setRamImage(p.tobytes())
        if highlight_flames and c.n_vox:
            f = np.fromiter(((d["state"].get("firing", 0.0) if d else 0.0) for d in c.devs), np.float32, c.n_vox)
        else:
            f = np.zeros(max(c.n_vox, 1), dtype=np.float32)
        self._set_firing(f)
        if self.cn is not None:
            self.cn.show()

    def hide(self):
        if self.cn is not None:
            self.cn.hide()

    def remove(self):
        if self.cn is not None:
            self.cn.removeNode()
            self.cn = None


def _ident():
    from panda3d.core import LMatrix4f
    return LMatrix4f.identMat()


# ---------------------------------------------------------------------------
# 差し替え
# ---------------------------------------------------------------------------
def install(mode="auto"):
    from main.client import render
    _state["mode"] = str(mode or "auto").lower()
    CV = render.ConstructView
    if getattr(CV, "_alpedia_gpu", False):
        return
    orig_rebuild_blob = CV._rebuild_blob
    orig_rebuild = CV._rebuild
    orig_update_blob = CV._update_blob
    orig_update_rigid = CV._update_rigid
    orig_hide = CV.hide_decals
    orig_remove = CV.remove

    def _drop(self):
        gb = getattr(self, "_gpu", None)
        if gb is not None:
            gb.remove()
            self._gpu = None

    def _rebuild_blob(self):
        _drop(self)
        ok = orig_rebuild_blob(self)
        if ok and _resolve() != "off":
            try:
                self._gpu = GpuBlob(self, _state["resolved"])
            except Exception as e:  # noqa: BLE001
                L.warning("GPU データを作れませんでした（CPU で描きます）: %s", e)
                self._gpu = None
        return ok

    def _rebuild(self):
        _drop(self)
        return orig_rebuild(self)

    def _update_blob(self, origin, highlight_flames):
        gb = getattr(self, "_gpu", None)
        if gb is None:
            return orig_update_blob(self, origin, highlight_flames)
        try:
            gb.update(origin, highlight_flames)
        except Exception as e:  # noqa: BLE001
            L.warning("GPU 描画に失敗したので CPU に戻します: %s", e)
            gb.deactivate()
            gb.remove()
            self._gpu = None
            orig_update_blob(self, origin, highlight_flames)

    def _update_rigid(self, origin, tf, highlight_flames):
        gb = getattr(self, "_gpu", None)
        if gb is not None and gb.active:
            gb.deactivate()
            self._baked = None
        return orig_update_rigid(self, origin, tf, highlight_flames)

    def hide_decals(self):
        gb = getattr(self, "_gpu", None)
        if gb is not None:
            gb.hide()
        return orig_hide(self)

    def remove(self):
        _drop(self)
        return orig_remove(self)

    CV._rebuild_blob = _rebuild_blob
    CV._rebuild = _rebuild
    CV._update_blob = _update_blob
    CV._update_rigid = _update_rigid
    CV.hide_decals = hide_decals
    CV.remove = remove
    CV._alpedia_gpu = True
    L.info("GPU 描画の差し込み完了（設定: %s）", _state["mode"])
