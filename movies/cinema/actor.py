"""VRM のキャラクター（役者）。

ゲームの VRM の見た目 MOD（mods/skinmodel(dummy)）の読み込み・動き・揺れものを使い、映画用に足したもの：
  ・表情（まばたき・口の あ い う・喜・怒・哀・驚）を重ねられる。セリフの間は口が動く
  ・ポーズをキーフレームで（腕・胴・顔の向き。書き方はかわいいモーション MOD と同じ）
  ・見る先（頭と目がそちらを向く）
  ・照明（主光の向きと色・影の色・縁の光）と、ホログラム（通信の立体映像）の描き方
"""
import importlib.util
import math
import sys

import numpy as np

from . import anim
from .stage import DEV

SKIN_DIR = DEV / "mods" / "skinmodel(dummy)" / "code"


def _skin():
    """見た目 MOD のプログラムを、パッケージ vrmskin として読み込む（フォルダ名にかっこがあるため）。"""
    if "vrmskin" not in sys.modules:
        spec = importlib.util.spec_from_file_location("vrmskin", SKIN_DIR / "__init__.py",
                                                      submodule_search_locations=[str(SKIN_DIR)])
        m = importlib.util.module_from_spec(spec)
        sys.modules["vrmskin"] = m
        spec.loader.exec_module(m)
    import vrmskin.motion  # noqa: F401
    import vrmskin.spring  # noqa: F401
    import vrmskin.vrm  # noqa: F401,F811
    return sys.modules["vrmskin"]


EXPRESSIONS = ("blink", "aa", "ih", "ou", "happy", "angry", "sad", "surprised")


def load_vrm(path):
    sk = _skin()
    base = sk.vrm.VRM

    class MovieVRM(base):
        def _blink(self):
            """表情ごとの形（頂点のずれ）を、メッシュごとに取っておく。"""
            ex = {}
            if self.version == 0:
                for g in self.ext.get("blendShapeMaster", {}).get("blendShapeGroups", []):
                    name = (g.get("presetName") or g.get("name") or "").lower()
                    name = {"a": "aa", "i": "ih", "u": "ou", "joy": "happy", "sorrow": "sad", "fun": "relaxed"}.get(name, name)
                    ex[name] = [(b["mesh"], b["index"], float(b.get("weight", 100)) / 100) for b in g.get("binds", [])]
            else:
                for name, e in self.ext.get("expressions", {}).get("preset", {}).items():
                    lst = []
                    for b in e.get("morphTargetBinds", []):
                        nd = self.js["nodes"][b["node"]]
                        if "mesh" in nd:
                            lst.append((nd["mesh"], b["index"], float(b.get("weight", 1.0))))
                    ex[name] = lst
            for m in self.meshes:
                m.morphs = np.zeros((len(EXPRESSIONS), len(m.pos), 3), np.float32)
            self.has_expr = set()
            for k, name in enumerate(EXPRESSIONS):
                for mesh_i, ti, w in ex.get(name, []):
                    for j in self.mesh_of.get(mesh_i, []):
                        m = self.meshes[j]
                        if ti < len(m.targets) and m.targets[ti] is not None:
                            m.morphs[k] += m.targets[ti] * w
                            self.has_expr.add(name)
            for m in self.meshes:
                m.morph = m.morphs[0]
                m.targets = None
    return MovieVRM(path)


VERT = """#version 330
uniform mat4 p3d_ModelViewProjectionMatrix;
uniform mat4 p3d_ModelMatrix;
uniform samplerBuffer bones;
uniform float ew[8];
in vec4 p3d_Vertex;
in vec3 p3d_Normal;
in vec2 p3d_MultiTexCoord0;
in vec4 joints;
in vec4 weights;
in vec3 e0; in vec3 e1; in vec3 e2; in vec3 e3; in vec3 e4; in vec3 e5; in vec3 e6; in vec3 e7;
out vec2 v_uv;
out vec3 v_n;
out vec3 v_p;
out float v_h;

mat4 bone(float j) {
    int i = int(j + 0.5) * 3;
    return transpose(mat4(texelFetch(bones, i), texelFetch(bones, i + 1), texelFetch(bones, i + 2), vec4(0, 0, 0, 1)));
}

void main() {
    mat4 m = bone(joints.x) * weights.x + bone(joints.y) * weights.y
           + bone(joints.z) * weights.z + bone(joints.w) * weights.w;
    vec3 d = e0 * ew[0] + e1 * ew[1] + e2 * ew[2] + e3 * ew[3] + e4 * ew[4] + e5 * ew[5] + e6 * ew[6] + e7 * ew[7];
    vec4 p = m * vec4(p3d_Vertex.xyz + d, 1.0);
    gl_Position = p3d_ModelViewProjectionMatrix * vec4(p.xyz, 1.0);
    v_n = mat3(p3d_ModelMatrix) * (mat3(m) * p3d_Normal);
    v_p = (p3d_ModelMatrix * vec4(p.xyz, 1.0)).xyz;
    v_h = p.y;
    v_uv = p3d_MultiTexCoord0;
}
"""

FRAG = """#version 330
uniform sampler2D p3d_Texture0;
uniform vec3 key_dir;
uniform vec3 key_col;
uniform vec3 shade;
uniform vec3 fill_col;
uniform vec3 rim_col;
uniform vec4 base_color;
uniform float cutoff;
uniform float holo;
uniform vec3 holo_col;
uniform float time;
in vec2 v_uv;
in vec3 v_n;
in vec3 v_p;
in float v_h;
out vec4 o_color;

void main() {
    vec4 c = texture(p3d_Texture0, v_uv) * base_color;
    if (c.a < cutoff) discard;
    float l = length(v_n);
    vec3 n = l > 1e-6 ? v_n / l : vec3(0, 1, 0);
    vec3 v = normalize(-v_p);
    float ndl = dot(n, key_dir);
    float k = smoothstep(-0.05, 0.18, ndl);              // トゥーン調：明るい面と影の面
    vec3 lit = c.rgb * mix(shade * fill_col, key_col, k);
    float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0);
    lit += rim_col * rim * (0.4 + 0.6 * smoothstep(-0.3, 0.4, dot(n, -key_dir)));
    if (holo > 0.0) {
        // ホログラム：単色・走査線・ちらつき・縁が光る
        float lum = dot(c.rgb, vec3(0.3, 0.59, 0.11));
        float scan = 0.6 + 0.4 * sin(v_h * 260.0 - time * 12.0);
        float band = smoothstep(0.0, 0.04, fract(v_h * 1.7 - time * 0.6)) * 0.25 + 0.75;
        float fres = pow(1.0 - abs(dot(n, v)), 2.0);
        vec3 hc = holo_col * (0.1 + 0.4 * lum + 0.8 * fres) * scan * band;
        o_color = vec4(hc * holo, holo * (0.3 + 0.4 * fres + 0.1 * scan));
        return;
    }
    o_color = vec4(lit, c.a);
}
"""


class _Model:
    """1つの VRM の形（役者どうしで共有）。"""
    cache = {}

    @classmethod
    def get(cls, path):
        key = str(path)
        if key not in cls.cache:
            cls.cache[key] = cls(path)
        return cls.cache[key]

    def __init__(self, path):
        from panda3d.core import (Geom, GeomNode, GeomTriangles, GeomVertexArrayFormat, GeomVertexData,
                                  GeomVertexFormat, InternalName, NodePath, Shader, Texture, Vec3, Vec4)
        self.v = vrm = load_vrm(path)
        sk = _skin()
        self.template = NodePath("vrm_actor")
        self.shader = Shader.make(Shader.SL_GLSL, vertex=VERT, fragment=FRAG)
        a = GeomVertexArrayFormat()
        a.addColumn(InternalName.getVertex(), 3, Geom.NT_float32, Geom.C_point)
        a.addColumn(InternalName.getNormal(), 3, Geom.NT_float32, Geom.C_normal)
        a.addColumn(InternalName.getTexcoord(), 2, Geom.NT_float32, Geom.C_texcoord)
        a.addColumn(InternalName.make("joints"), 4, Geom.NT_float32, Geom.C_other)
        a.addColumn(InternalName.make("weights"), 4, Geom.NT_float32, Geom.C_other)
        for k in range(8):
            a.addColumn(InternalName.make(f"e{k}"), 3, Geom.NT_float32, Geom.C_vector)
        f = GeomVertexFormat()
        f.addArray(a)
        fmt = GeomVertexFormat.registerFormat(f)
        from vrmskin.view import _texture
        white = Texture("white")
        white.setup2dTexture(1, 1, Texture.T_unsigned_byte, Texture.F_rgba8)
        white.setRamImage(b"\xff\xff\xff\xff")
        texs = {}
        for m in vrm.materials:
            ti = m["texture"]
            if ti is not None and ti not in texs:
                data, mime = vrm.image_bytes(ti)
                texs[ti] = _texture(data, mime, f"actor_tex{ti}")
        for mesh in vrm.meshes:
            n = len(mesh.pos)
            arr = np.zeros((n, 16 + 24), np.float32)
            arr[:, 0:3] = mesh.pos
            arr[:, 3:6] = mesh.nrm
            arr[:, 8:12] = mesh.joints
            arr[:, 12:16] = mesh.weights
            for k in range(8):
                arr[:, 16 + 3 * k:19 + 3 * k] = mesh.morphs[k]
            vdatas = {}
            for idx, mat_i in mesh.parts:
                m = vrm.materials[mat_i] if 0 <= mat_i < len(vrm.materials) else None
                uvt = m["uv"] if m else ((0.0, 0.0), (1.0, 1.0))
                vd = vdatas.get(uvt)
                if vd is None:
                    (ox, oy), (sx, sy) = uvt
                    arr[:, 6] = mesh.uv[:, 0] * sx + ox
                    arr[:, 7] = 1.0 - (mesh.uv[:, 1] * sy + oy)
                    vd = vdatas[uvt] = GeomVertexData("actor", fmt, Geom.UH_static)
                    vd.uncleanSetNumRows(n)
                    memoryview(vd.modifyArray(0)).cast("B")[:] = arr.tobytes()
                tris = GeomTriangles(Geom.UH_static)
                tris.setIndexType(Geom.NT_uint32)
                h = tris.modifyVertices()
                h.uncleanSetNumRows(len(idx))
                memoryview(h).cast("B")[:] = np.ascontiguousarray(idx, np.uint32).tobytes()
                g = Geom(vd)
                g.addPrimitive(tris)
                node = GeomNode(m["name"] if m else "vrm")
                node.addGeom(g)
                np_ = self.template.attachNewNode(node)
                m = m or {"texture": None, "color": [1, 1, 1, 1], "shade": [0.7, 0.7, 0.75], "alpha": "OPAQUE",
                          "cutoff": 0.5, "double": False}
                np_.setTexture(texs.get(m["texture"], white), 1)
                np_.setShaderInput("base_color", Vec4(*m["color"]))
                np_.setShaderInput("shade", Vec3(*m["shade"]))
                np_.setShaderInput("cutoff", m["cutoff"] if m["alpha"] == "MASK" else -1.0)
                if m["double"]:
                    np_.setTwoSided(True)
                if m["alpha"] == "BLEND":
                    from panda3d.core import TransparencyAttrib
                    np_.setTransparency(TransparencyAttrib.M_alpha)
                    np_.setBin("transparent", 10)
                    np_.setDepthWrite(False)
        self.sk = sk


# ---------------------------------------------------------------------------
STAND = {"arm_l": (0.25, -1.0, 0.05), "arm_r": (0.25, -1.0, 0.05), "fore_l": (0.15, -1.0, 0.15),
         "fore_r": (0.15, -1.0, 0.15), "head": (0.0, 0.0, 1.0), "chest": (0.0, 1.0, 0.0), "spine": (0.0, 1.0, 0.0),
         "tilt": 0.0, "twist": 0.0, "bob": 0.0}


def blend_pose(a, b, k):
    out = {}
    for key in set(a) | set(b):
        va = a.get(key, STAND.get(key))
        vb = b.get(key, STAND.get(key))
        if va is None:
            out[key] = vb
            continue
        if vb is None:
            out[key] = va
            continue
        if isinstance(va, (tuple, list, np.ndarray)):
            va, vb = anim.unit(va), anim.unit(vb)
            out[key] = tuple(va + (vb - va) * k)
        else:
            out[key] = va + (vb - va) * k
    return out


class Actor:
    """役者1人。pos（足元）・向き（yaw 度、0 で +Z を向く）・ポーズ・表情・見る先を時刻の関数やキーで決める。"""

    def __init__(self, stage, vrm_path, name="actor", scale=1.0):
        from panda3d.core import GeomEnums, NodePath, OmniBoundingVolume, Texture, Vec3
        self.stage = stage
        self.name = name
        self.model = _Model.get(vrm_path)
        v = self.v = self.model.v
        sk = self.model.sk
        self.motion = sk.motion.Motion(v)
        self.spring = sk.spring.Spring(v)
        self.scale = scale
        self.root = NodePath(f"actor_{name}")
        self.root.reparentTo(stage.root)
        self.root.node().setBounds(OmniBoundingVolume())
        self.root.node().setFinal(True)
        self.model.template.instanceTo(self.root)
        nj = len(v.joint_node)
        self.bones = Texture(f"bones_{name}")
        self.bones.setupBufferTexture(nj * 3, Texture.T_float, Texture.F_rgba32, GeomEnums.UH_dynamic)
        self.root.setShader(self.model.shader)
        self.root.setShaderInput("bones", self.bones)
        self.root.setShaderInput("ew", [0.0] * 8)
        self.root.setLightOff(1)
        self.light(key_dir=(0.4, 0.7, -0.6))
        self.root.setShaderInput("holo", 0.0)
        self.root.setShaderInput("holo_col", Vec3(0.3, 0.9, 1.0))
        self.root.setShaderInput("time", 0.0)
        # 時刻の関数（場面が決める）
        self.place = lambda t: ((0.0, 0.0, 0.0), 0.0)          # → (足元の位置, 向き yaw 度)
        self.poses = [(0.0, {})]                                # [(時刻, ポーズ)] つなぎはなめらかに
        self.look = None                                        # t → 見る点（ワールド）か None
        self.expr = {}                                          # 表情名 → Track（0〜1）
        self.talk = []                                          # [(始め, 終わり)] 口が動く
        self.blinks = []                                        # まばたきの時刻
        self.holo = None                                        # t → 0〜1（ホログラムの濃さ）か None
        self.visible = None                                     # t → bool
        self._last_t = None
        stage.add(self)

    def light(self, key_dir=(0.4, 0.7, -0.6), key_col=(1.0, 0.97, 0.92), fill_col=(0.85, 0.88, 1.0),
              rim_col=(0.25, 0.35, 0.5)):
        from panda3d.core import Vec3
        self.root.setShaderInput("key_dir", Vec3(*anim.unit(key_dir)))
        self.root.setShaderInput("key_col", Vec3(*key_col))
        self.root.setShaderInput("fill_col", Vec3(*fill_col))
        self.root.setShaderInput("rim_col", Vec3(*rim_col))

    def holo_mode(self, color=(0.35, 0.85, 1.0)):
        from panda3d.core import ColorBlendAttrib, Vec3
        self.root.setShaderInput("holo_col", Vec3(*color))
        self.root.setAttrib(ColorBlendAttrib.make(ColorBlendAttrib.M_add, ColorBlendAttrib.O_incoming_alpha,
                                                  ColorBlendAttrib.O_one))
        self.root.setDepthWrite(False)
        self.root.setBin("fixed", 20)

    # --- 時刻 → 姿勢 -----------------------------------------------------
    def pose_at(self, t):
        ps = self.poses
        if t <= ps[0][0]:
            return dict(ps[0][1])
        for (t0, a), (t1, b) in zip(ps, ps[1:]):
            if t < t1:
                return blend_pose(a, b, anim.smoother(anim.remap(t, t0, t1)))
        return dict(ps[-1][1])

    def weights(self, t):
        w = np.zeros(8)
        for k, name in enumerate(EXPRESSIONS):
            tr = self.expr.get(name)
            if tr is not None:
                w[k] = float(tr(t))
        # まばたき（0.14 秒）
        for tb in self.blinks:
            x = t - tb
            if 0 <= x < 0.16:
                w[0] = max(w[0], 1 - abs(x - 0.07) / 0.08)
        # 口：話している間は「あ・い・う」を音節のように切り替える
        for t0, t1 in self.talk:
            if t0 <= t <= t1:
                env = min(1.0, (t - t0) / 0.08, (t1 - t) / 0.12)
                ph = (t - t0) * 9.0
                syl = int(ph)
                f = ph - syl
                open_ = math.sin(math.pi * f) ** 0.8
                kind = (syl * 7 + 3) % 5
                amt = env * open_ * (0.55 + 0.35 * ((syl * 13) % 3) / 2)
                if kind in (0, 3):
                    w[1] = max(w[1], amt)
                elif kind == 1:
                    w[2] = max(w[2], amt * 0.8)
                else:
                    w[3] = max(w[3], amt * 0.8)
        # 驚き・怒りの形は目も閉じるものがあるので、まばたきとの重なりを抑える
        return np.clip(w, 0, 1)

    def update(self, t, origin):
        if self.visible is not None and not self.visible(t):
            self.root.hide()
            return
        hv = self.holo(t) if self.holo is not None else None
        if hv is not None and hv <= 0.003:
            self.root.hide()
            return
        self.root.show()
        dt = 1.0 / self.stage.fps
        if self._last_t is None:
            # 最初のコマ：ふだんの姿勢になじませておく（揺れものも落ち着かせる）
            for k in range(30):
                self._pose(t - (30 - k) * dt, dt, origin, write=False)
            self._last_t = t - dt
        self._pose(t, dt, origin, write=True)
        self.root.setShaderInput("ew", [float(x) for x in self.weights(t)])
        self.root.setShaderInput("time", float(t))
        if hv is not None:
            self.root.setShaderInput("holo", float(hv))
        self._last_t = t

    def _pose(self, t, dt, origin, write=True):
        from panda3d.core import LMatrix4f
        mo = self.motion
        pos, yaw = self.place(t)
        pos = np.asarray(pos, float)
        up = np.array([0.0, 1.0, 0.0])
        face = np.array([math.sin(math.radians(yaw)), 0.0, math.cos(math.radians(yaw))])
        right = np.cross(up, face)
        pose = self.pose_at(t)
        if self.look is not None:
            tgt = self.look(t)
            if tgt is not None:
                head = self.head_pos(pos, yaw)
                d = anim.unit(np.asarray(tgt, float) - head)
                lx, ly, lz = float(d @ right), float(d @ up), float(d @ face)
                # 体の向きから見た向き（顔は 70° まで、残りは目で）
                yaw_l = math.atan2(lx, lz)
                pit = math.atan2(ly, math.hypot(lx, lz))
                hy = max(-1.2, min(1.2, yaw_l))
                hp = max(-0.6, min(0.6, pit))
                hd = (math.sin(hy) * math.cos(hp), math.sin(hp), math.cos(hy) * math.cos(hp))
                base = pose.get("head")
                if base is not None:              # ポーズの顔の向きに少し足す
                    hd = tuple(anim.unit(np.asarray(hd) + (np.asarray(anim.unit(base)) - np.array([0, 0, 1.0])) * 0.6))
                pose["head"] = hd
        st = {"speed": 0.0, "vup": 0.0, "tang": np.zeros(3), "on_ground": True, "flying": False, "water": 0.0,
              "seated": bool(pose.pop("seated", False)), "pitch": 0.0}
        p = dict(pose)
        rot, hips, _blink, seat, face_off = mo.update(st, dt, (lambda _t, pp=p: pp), 10.0)
        Bm = np.stack([mo.R, mo.U, mo.F])
        if face_off:
            face = face * math.cos(face_off) - np.cross(up, face) * math.sin(face_off)
            right = np.cross(up, face)
        W = np.stack([right, up, face])
        rows = (Bm.T @ W) * self.scale
        place = pos - up * ((mo.hips_h * self.scale - 0.08) * seat)
        M = self.v.pose(rot, hips, (self.spring, rows.T, place, dt))
        if write:
            self.bones.setRamImage(np.ascontiguousarray(M, np.float32).tobytes())
            c = place - origin
            self.root.setMat(LMatrix4f(*rows[0], 0, *rows[1], 0, *rows[2], 0, *[float(x) for x in c], 1))

    def head_pos(self, pos=None, yaw=None, t=None):
        """頭（目の高さ）のワールド位置のめやす。"""
        if pos is None:
            pos, yaw = self.place(t)
        v = self.v
        h = float((v.bone_pos("head") - v.bounds()[0]) @ self.motion.U) * self.scale + 0.08 * self.scale
        return np.asarray(pos, float) + np.array([0.0, h, 0.0])
