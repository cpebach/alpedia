"""VRM のモデルを Panda3D で描く（骨での変形はシェーダーで行う＝GPU スキニング）。

ワールドを始めるときに、メッシュ・テクスチャ・シェーダーを全部作っておく（プレイ中にファイルを読まない）。
毎フレームの仕事は、モーションから骨の行列を求めてテクスチャ（バッファ）に書くことと、置く位置を決めることだけ。
"""
import math

import numpy as np
from panda3d.core import (Geom, GeomEnums, GeomNode, GeomTriangles, GeomVertexArrayFormat, GeomVertexData,
                          GeomVertexFormat, InternalName, LMatrix4f, NodePath, OmniBoundingVolume, PNMImage,
                          SamplerState, Shader, StringStream, Texture, TransparencyAttrib, Vec3, Vec4)

from .motion import Motion
from .spring import Spring

VERT = """#version 330
uniform mat4 p3d_ModelViewProjectionMatrix;
uniform mat4 p3d_ModelMatrix;
uniform samplerBuffer bones;
uniform float blink;
in vec4 p3d_Vertex;
in vec3 p3d_Normal;
in vec2 p3d_MultiTexCoord0;
in vec4 joints;
in vec4 weights;
in vec3 morph;
out vec2 v_uv;
out vec3 v_n;

mat4 bone(float j) {
    int i = int(j + 0.5) * 3;
    return transpose(mat4(texelFetch(bones, i), texelFetch(bones, i + 1), texelFetch(bones, i + 2), vec4(0, 0, 0, 1)));
}

void main() {
    mat4 m = bone(joints.x) * weights.x + bone(joints.y) * weights.y
           + bone(joints.z) * weights.z + bone(joints.w) * weights.w;
    vec4 p = m * vec4(p3d_Vertex.xyz + morph * blink, 1.0);
    gl_Position = p3d_ModelViewProjectionMatrix * vec4(p.xyz, 1.0);
    v_n = mat3(p3d_ModelMatrix) * (mat3(m) * p3d_Normal);
    v_uv = p3d_MultiTexCoord0;
}
"""

FRAG = """#version 330
uniform sampler2D p3d_Texture0;
uniform vec3 sun_dir;
uniform vec3 shade;
uniform vec4 base_color;
uniform float cutoff;
in vec2 v_uv;
in vec3 v_n;
out vec4 o_color;

void main() {
    vec4 c = texture(p3d_Texture0, v_uv) * base_color;
    if (c.a < cutoff) discard;
    float l = length(v_n);
    float ndl = l > 1e-6 ? dot(v_n / l, sun_dir) : 1.0;
    float k = smoothstep(-0.1, 0.2, ndl);          // トゥーン調：明るい面と影の面をはっきり分ける
    o_color = vec4(c.rgb * mix(shade, vec3(1.0), k), c.a);
}
"""

_FORMAT = None


def _format():
    global _FORMAT
    if _FORMAT is None:
        a = GeomVertexArrayFormat()
        a.addColumn(InternalName.getVertex(), 3, Geom.NT_float32, Geom.C_point)
        a.addColumn(InternalName.getNormal(), 3, Geom.NT_float32, Geom.C_normal)
        a.addColumn(InternalName.getTexcoord(), 2, Geom.NT_float32, Geom.C_texcoord)
        a.addColumn(InternalName.make("joints"), 4, Geom.NT_float32, Geom.C_other)
        a.addColumn(InternalName.make("weights"), 4, Geom.NT_float32, Geom.C_other)
        a.addColumn(InternalName.make("morph"), 3, Geom.NT_float32, Geom.C_vector)
        f = GeomVertexFormat()
        f.addArray(a)
        _FORMAT = GeomVertexFormat.registerFormat(f)
    return _FORMAT


def _texture(data, mime, name):
    img = PNMImage()
    ext = ".jpg" if "jpeg" in mime or "jpg" in mime else ".png"
    if not img.read(StringStream(data), name + ext):
        raise RuntimeError(f"テクスチャ {name} を読めません")
    t = Texture(name)
    t.load(img)
    t.setMinfilter(SamplerState.FT_linear_mipmap_linear)
    t.setMagfilter(SamplerState.FT_linear)
    t.setAnisotropicDegree(4)
    return t


class VRMModel:
    """1つの VRM の、描くための形（メッシュ・テクスチャ・シェーダー）。ワールドを始めるときに1回だけ作り、
    プレイヤーごとの体はこれを共有する（骨の行列だけ体ごと）。"""

    def __init__(self, defn, vrm, sun):
        self.v = vrm
        self.scale = float(defn.get("model_scale", 1.0))
        self.template = NodePath("vrm_model")
        self.shader = Shader.make(Shader.SL_GLSL, vertex=VERT, fragment=FRAG)
        self.sun = Vec3(*[float(x) for x in sun])
        white = Texture("white")
        white.setup2dTexture(1, 1, Texture.T_unsigned_byte, Texture.F_rgba8)
        white.setRamImage(b"\xff\xff\xff\xff")
        texs = {}
        for m in vrm.materials:
            ti = m["texture"]
            if ti is not None and ti not in texs:
                data, mime = vrm.image_bytes(ti)
                texs[ti] = _texture(data, mime, f"vrm_tex{ti}")
        for mesh in vrm.meshes:
            self._add_mesh(mesh, texs, white)

    def _add_mesh(self, mesh, texs, white):
        n = len(mesh.pos)
        arr = np.zeros((n, 19), np.float32)
        arr[:, 0:3] = mesh.pos
        arr[:, 3:6] = mesh.nrm
        arr[:, 8:12] = mesh.joints
        arr[:, 12:16] = mesh.weights
        arr[:, 16:19] = mesh.morph
        vdatas = {}                     # UV の変換ごとに頂点データを1つ（ふつうは1つだけ）
        for idx, mat_i in mesh.parts:
            m = self.v.materials[mat_i] if 0 <= mat_i < len(self.v.materials) else None
            uvt = m["uv"] if m else ((0.0, 0.0), (1.0, 1.0))
            vd = vdatas.get(uvt)
            if vd is None:
                (ox, oy), (sx, sy) = uvt
                arr[:, 6] = mesh.uv[:, 0] * sx + ox
                arr[:, 7] = 1.0 - (mesh.uv[:, 1] * sy + oy)     # glTF は上が v=0、Panda3D は下が v=0
                vd = vdatas[uvt] = GeomVertexData("vrm", _format(), Geom.UH_static)
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
                np_.setTransparency(TransparencyAttrib.M_alpha)
                np_.setBin("transparent", 10)
                np_.setDepthWrite(False)


class VRMBody:
    """プレイヤー1人の体（VRM）。game が毎フレーム update(origin, shown, dt) を呼ぶ。
    player の pos・up・vel・look_fwd（見ている向き）・pitch などを読んで動く（自分も、ほかの参加者も同じ）。"""

    def __init__(self, game, player, model):
        self.game, self.player, self.model, self.v = game, player, model, model.v
        vrm = model.v
        self.scale = model.scale
        self.motion = Motion(vrm)
        self.spring = Spring(vrm)
        self.shown = False
        self.root = NodePath("vrm_player")
        self.root.reparentTo(game.scene_root)
        self.root.node().setBounds(OmniBoundingVolume())
        self.root.node().setFinal(True)
        model.template.instanceTo(self.root)
        nj = len(vrm.joint_node)
        self.bones = Texture("vrm_bones")
        self.bones.setupBufferTexture(nj * 3, Texture.T_float, Texture.F_rgba32, GeomEnums.UH_dynamic)
        self._write_bones(vrm.pose())
        self.root.setShader(model.shader)
        self.root.setShaderInput("bones", self.bones)
        self.root.setShaderInput("blink", 0.0)
        self.root.setShaderInput("sun_dir", model.sun)
        self.root.setLightOff(1)
        self.face = None                # 体の向き（ワールド、地面に沿った単位ベクトル）
        self.root.hide()

    def _write_bones(self, M):
        self.bones.setRamImage(np.ascontiguousarray(M, np.float32).tobytes())

    # ------------------------------------------------------------------
    def state(self):
        """プレイヤーの状態（モーションに渡す）。"""
        p = self.player
        up = p.up
        rel = p.vel - getattr(p, "ground_vel", 0.0)
        vup = float(rel @ up)
        tang = rel - up * vup
        return {"speed": float(np.linalg.norm(tang)), "vup": vup, "tang": tang, "on_ground": bool(p.on_ground),
                "flying": bool(p.jetpack) and not p.on_ground, "water": float(getattr(p, "in_water", 0.0)),
                "seated": p.seated is not None, "pitch": float(getattr(p, "pitch", 0.0))}

    def _turn(self, up, want, dt):
        """体の向きを want（地面に沿った方向）へなめらかに回す。"""
        f = self.face - up * (self.face @ up)
        if np.linalg.norm(f) < 1e-6:
            f = want
        f /= np.linalg.norm(f)
        r = np.cross(up, f)
        ang = math.atan2(float(want @ r), float(want @ f))
        step = ang * (1.0 - math.exp(-dt * 10.0))
        self.face = f * math.cos(step) + r * math.sin(step)

    def update(self, origin, shown, dt):
        if not shown:
            if self.shown:
                self.root.hide()
                self.shown = False
            return
        if not self.shown:
            self.spring.reset()             # 見えていなかった間の動きで髪が跳ねないように
            self.shown = True
        p = self.player
        up = p.up / np.linalg.norm(p.up)
        cam_f = np.asarray(getattr(p, "look_fwd", self.face if self.face is not None else np.array([0, 0, 1.0])),
                           dtype=np.float64)
        st = self.state()
        if self.face is None:
            self.face = cam_f.copy()
        face_dir = getattr(p, "face_dir", None)
        if face_dir is not None:
            want = np.asarray(face_dir, dtype=np.float64)   # カメラモードで決めた体の向き
        elif st["seated"]:
            want = cam_f
        elif st["speed"] > 0.6:
            want = st["tang"] / st["speed"]
        else:
            want = self.face
        want = want - up * (want @ up)
        if np.linalg.norm(want) > 1e-6:
            self._turn(up, want / np.linalg.norm(want), dt)
        emote = None
        name = getattr(p, "emote", None)
        if name:
            emote = getattr(self.game, "emotes", {}).get(name)
        import time as _t
        rot, hips, blink, seat, face_off = self.motion.update(st, dt, emote, _t.monotonic() - getattr(p, "emote_t0", 0.0))
        # 置き方：モデルの (右, 上, 前) → ワールドの (右, 上, 体の向き)
        f = self.face
        if face_off:
            f = f * math.cos(face_off) - np.cross(up, f) * math.sin(face_off)   # 左回り +
        right = np.cross(up, f)                          # ゲームの「右」（コアの look_vectors と同じ）
        Bm = np.stack([self.motion.R, self.motion.U, self.motion.F])
        W = np.stack([right, up, f])
        rows = (Bm.T @ W) * self.scale                   # 行ベクトルにかける形（Panda3D）
        feet = p.pos - up * p.radius
        drop = (self.motion.hips_h * self.scale - 0.08) * seat
        place = feet - up * drop                         # ワールドでの足元（座ると下げる）
        M = self.v.pose(rot, hips, (self.spring, rows.T, place, dt))
        self._write_bones(M)
        self.root.setShaderInput("blink", float(blink))
        t = place - origin
        self.root.setMat(LMatrix4f(*rows[0], 0, *rows[1], 0, *rows[2], 0, *[float(x) for x in t], 1))
        self.root.show()

    def destroy(self):
        self.root.removeNode()
