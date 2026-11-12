"""VRM のモデルを描く：骨の行列を Rust の画面（alpedia_view）に渡し、Rust が頂点を曲げる（陰影はトゥーン調）。

ワールドを始めるときに、形（頂点・骨の重み）とテクスチャを全部渡しておく（プレイ中にファイルを読まない。
テクスチャは画像ファイルの中身のまま渡し、初めて描くときに読み解く）。
毎フレームの仕事は、モーションから骨の行列を求めて渡すことと、置く位置を決めることだけ。
"""
import math

import numpy as np

from .motion import Motion
from .spring import Spring


class VRMModel:
    """1つの VRM の、描くための形（頂点・骨の重み）とテクスチャ。ワールドを始めるときに1回だけ Rust に渡し、
    プレイヤーごとの体（VRMBody）が材質ごとの部品を作る（骨の行列だけ体ごと）。"""

    def __init__(self, defn, vrm, sun):
        from main.client import gfx
        self.v = vrm
        self.scale = float(defn.get("model_scale", 1.0))
        self.sun = np.asarray(sun, np.float64)
        self.texs = {}           # 画像はファイルの中身のまま渡す（読み解くのは初めて描くとき）
        for m in vrm.materials:
            ti = m["texture"]
            if ti is not None and ti not in self.texs:
                data, _ = vrm.image_bytes(ti)
                tid = gfx.VIEW.tex_bytes(data, False)
                if tid < 0:
                    raise RuntimeError(f"テクスチャ {ti} を読めません")
                self.texs[ti] = gfx.RsTex(tid, f"vrm_tex{ti}")
        self.geoms = []          # [(形の番号, メッシュ)]
        for mesh in vrm.meshes:
            gid = gfx.VIEW.skin_geom(np.ascontiguousarray(mesh.pos, np.float32), np.ascontiguousarray(mesh.nrm, np.float32),
                                     np.ascontiguousarray(mesh.joints, np.float32), np.ascontiguousarray(mesh.weights, np.float32),
                                     np.ascontiguousarray(mesh.morph, np.float32))
            self.geoms.append((gid, mesh))


class VRMBody:
    """プレイヤー1人の体（VRM）。game が毎フレーム update(origin, shown, dt) を呼ぶ。
    player の pos・up・vel・look_fwd（見ている向き）・pitch などを読んで動く（自分も、ほかの参加者も同じ）。"""

    def __init__(self, game, player, model):
        from main.client import gfx
        self.game, self.player, self.model, self.v = game, player, model, model.v
        self.scale = model.scale
        self.motion = Motion(model.v)
        self.spring = Spring(model.v)
        self.shown = False
        self.face = None
        self.root = gfx.RsNode(game.scene_root, "vrm_player")
        self.parts = []
        self.meshes = []
        for gid, mesh in model.geoms:
            uvs = {}
            for idx, mat_i in mesh.parts:
                mt = self.v.materials[mat_i] if 0 <= mat_i < len(self.v.materials) else None
                mt = mt or {"texture": None, "color": [1, 1, 1, 1], "shade": [0.7, 0.7, 0.75], "alpha": "OPAQUE",
                            "cutoff": 0.5, "double": False, "uv": ((0.0, 0.0), (1.0, 1.0))}
                uvt = mt.get("uv") or ((0.0, 0.0), (1.0, 1.0))
                uv = uvs.get(uvt)
                if uv is None:
                    (ox, oy), (sx, sy) = uvt
                    uv = np.empty((len(mesh.pos), 2), np.float32)
                    uv[:, 0] = mesh.uv[:, 0] * sx + ox
                    uv[:, 1] = 1.0 - (mesh.uv[:, 1] * sy + oy)     # 今までの画面と同じ向き（テクスチャは上下を返して渡す）
                    uvs[uvt] = uv
                m = gfx.RsDynamicMesh(self.root, "vrm")
                pid = gfx.VIEW.skin_part(gid, m.mid, np.ascontiguousarray(idx, np.int64), uv,
                                         [float(x) for x in mt["color"]], [float(x) for x in mt["shade"]])
                blend = mt["alpha"] == "BLEND"
                gfx.VIEW.mesh_props(m.mid, blend, not blend, bool(mt["double"]), 0, 10 if blend else 0)
                tex = model.texs.get(mt["texture"])
                if tex is not None:
                    gfx.VIEW.mesh_tex(m.mid, 0, tex.id)
                if mt["alpha"] == "MASK":
                    gfx.VIEW.mesh_cutoff(m.mid, float(mt["cutoff"]))
                self.parts.append(pid)
                self.meshes.append(m)
        self.root.hide()

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
        rows = (Bm.T @ W) * self.scale                   # 行ベクトルにかける形
        feet = p.pos - up * p.radius
        drop = (self.motion.hips_h * self.scale - 0.08) * seat
        place = feet - up * drop                         # ワールドでの足元（座ると下げる）
        M = self.v.pose(rot, hips, (self.spring, rows.T, place, dt))
        self._draw(M, float(blink), rows, place - origin)

    def _draw(self, M, blink, rows, t):
        from main.client import gfx
        sun = rows @ self.model.sun                       # 光の向きを形の座標へ（行ベクトルにかける形の逆）
        gfx.VIEW.skin_pose(self.parts, np.ascontiguousarray(M, np.float32).reshape(-1), blink, [float(x) for x in sun])
        self.root.setMat([*rows[0], 0, *rows[1], 0, *rows[2], 0, *[float(x) for x in t], 1])
        self.root.show()

    def destroy(self):
        from main.client import gfx
        for pid in self.parts:
            gfx.VIEW.skin_part_remove(pid)
        self.parts = []
        self.root.removeNode()


RsVRMModel, RsVRMBody = VRMModel, VRMBody          # 前の版の名前
