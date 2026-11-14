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


def _smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3 - 2 * t)


RIM_GAIN = 1.5      # 縁の光の強さ（下の「陰影」を参照）


class _Model:
    """1つの VRM の形（役者どうしで共有）。テクスチャは Rust の画面に1回だけ渡す。

    描き方：骨の行列で頂点を曲げるのも、陰影（トゥーン調の明るい面・影の面と、縁の光）も、ここ（numpy）で頂点ごとに計算して、
    頂点の色として Rust の画面（alpedia_view）に渡す。ゲームの VRM の体と同じ「陰影は頂点の色に焼き込む」やり方で、
    映画用の表情（8 つ）・役者ごとの照明・立体映像を足すために、曲げる所をこちらで持つ。
    縁の光は頂点の色に足すので、テクスチャの色がかかる（暗い髪では弱くなる）。その分 RIM_GAIN で少し強めてある。
    """
    cache = {}

    @classmethod
    def get(cls, path):
        key = str(path)
        if key not in cls.cache:
            cls.cache[key] = cls(path)
        return cls.cache[key]

    def __init__(self, path):
        from main.client import gfx
        self.v = vrm = load_vrm(path)
        self.sk = _skin()
        self.texs = {}
        for m in vrm.materials:
            ti = m["texture"]
            if ti is not None and ti not in self.texs:
                data, _mime = vrm.image_bytes(ti)
                tid = gfx.VIEW.tex_bytes(data, False)
                if tid < 0:
                    raise RuntimeError(f"{path}: テクスチャ {ti} を読めません")
                self.texs[ti] = gfx.RsTex(tid, f"actor_tex{ti}")
        default = {"texture": None, "color": [1, 1, 1, 1], "shade": [0.7, 0.7, 0.75], "alpha": "OPAQUE",
                   "cutoff": 0.5, "double": False, "uv": ((0.0, 0.0), (1.0, 1.0)), "name": "vrm"}
        self.meshes = []            # [(メッシュ, 骨の番号 (V,4), [部品 …])]
        for mesh in vrm.meshes:
            parts = []
            for idx, mat_i in mesh.parts:
                m = vrm.materials[mat_i] if 0 <= mat_i < len(vrm.materials) else None
                m = m or default
                idx = np.asarray(idx, np.int64).reshape(-1)
                src, inv = np.unique(idx, return_inverse=True)       # この部品が使う頂点だけ
                (ox, oy), (sx, sy) = m.get("uv") or ((0.0, 0.0), (1.0, 1.0))
                uv = np.empty((len(src), 2), np.float32)
                uv[:, 0] = mesh.uv[src, 0] * sx + ox
                uv[:, 1] = 1.0 - (mesh.uv[src, 1] * sy + oy)         # ゲームの VRM の体と同じ向き
                col = np.asarray(m["color"], np.float64)
                parts.append({"src": src, "tris": inv.reshape(-1, 3), "uv": uv, "mat": m, "color": col,
                              "shade": np.asarray(m["shade"], np.float64),
                              "lum": float(col[:3] @ np.array([0.3, 0.59, 0.11]))})
            self.meshes.append((mesh, np.asarray(mesh.joints).astype(np.int64), parts))


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
        from main.client import gfx
        self.stage = stage
        self.name = name
        self.model = _Model.get(vrm_path)
        v = self.v = self.model.v
        sk = self.model.sk
        self.motion = sk.motion.Motion(v)
        self.spring = sk.spring.Spring(v)
        self.scale = scale
        self.root = gfx.RsNode(stage.root, f"actor_{name}")
        # 部品（材質ごと）のメッシュ。頂点は毎コマ書き直す（三角形・UV・テクスチャはそのまま）
        self.parts = []             # [(メッシュの番号, 部品の番号, RsDynamicMesh, 頂点の入れ物 (n,12))]
        for mi, (_mesh, _joints, parts) in enumerate(self.model.meshes):
            for pi, part in enumerate(parts):
                dm = gfx.RsDynamicMesh(self.root, f"actor_{name}")
                buf = np.zeros((len(part["src"]), 12), np.float32)
                buf[:, 10:12] = part["uv"]
                dm.set(buf, part["tris"])
                self.parts.append((mi, pi, dm, buf))
        self._holo_on = False
        self._holo_col = (0.3, 0.9, 1.0)
        self._style(False)
        self.light(key_dir=(0.4, 0.7, -0.6))
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
        self.key_dir = np.asarray(anim.unit(key_dir), float)
        self.key_col = np.asarray(key_col, float)
        self.fill_col = np.asarray(fill_col, float)
        self.rim_col = np.asarray(rim_col, float)

    def holo_mode(self, color=(0.35, 0.85, 1.0)):
        """立体映像（通信のホログラム）として描く：足し算で重ね、奥行きは書かない。濃さは self.holo(t)。"""
        self._holo_col = tuple(float(x) for x in color)
        self._holo_on = True
        self._style(True)

    def _style(self, holo):
        """部品ごとの描き方（半透明・両面・切り抜き／立体映像）を Rust の画面に伝える。"""
        V = self.stage.view
        for mi, pi, dm, _buf in self.parts:
            m = self.model.meshes[mi][2][pi]["mat"]
            if holo:
                V.mesh_props(dm.mid, False, False, bool(m["double"]), 0, 120)
                V.mesh_add(dm.mid, True)
            else:
                blend = m["alpha"] == "BLEND"
                V.mesh_props(dm.mid, blend, not blend, bool(m["double"]), 0, 10 if blend else 0)
                V.mesh_add(dm.mid, False)
            tex = self.model.texs.get(m["texture"])
            V.mesh_tex(dm.mid, 0, tex.id if tex is not None else -1)
            V.mesh_cutoff(dm.mid, float(m["cutoff"]) if m["alpha"] == "MASK" else 0.0)
            V.mesh_holo(dm.mid, bool(holo), 0.0)
        c = self._holo_col if holo else (1.0, 1.0, 1.0)
        for _mi, _pi, dm, _buf in self.parts:           # 色はノードごと（親の色は子に伝わらない）
            dm.np.setColorScale(c[0], c[1], c[2], 1.0)

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
        _t, M, rows, place = self._last
        self._write(t, M, rows, place - origin, self.weights(t), hv if self._holo_on else None)
        self._last_t = t

    def _write(self, t, M, rows, c, ew, holo):
        """骨の行列 M（(J,3,4)）で頂点を曲げ、陰影を頂点の色にして、部品ごとに Rust の画面へ渡す。
        rows：形の座標 → ワールドの向き・大きさ（行ベクトルにかける形）、c：カメラから見た足元の位置、ew：表情の重み、
        holo：立体映像の濃さ（ふつうの描き方なら None）。"""
        V = self.stage.view
        M = np.asarray(M, np.float64)
        scale = float(np.linalg.norm(rows[0])) or 1.0
        skinned = {}
        for mi, pi, dm, buf in self.parts:
            mesh, joints, parts = self.model.meshes[mi]
            if mi not in skinned:
                pos = np.asarray(mesh.pos, np.float64)
                for k in np.nonzero(ew > 1e-4)[0]:               # 表情（まばたき・口・喜怒哀驚）
                    pos = pos + mesh.morphs[k] * float(ew[k])
                Mw = np.einsum("vk,vkij->vij", np.asarray(mesh.weights, np.float64), M[joints])
                P = np.einsum("vij,vj->vi", Mw[:, :, :3], pos) + Mw[:, :, 3]
                N = np.einsum("vij,vj->vi", Mw[:, :, :3], np.asarray(mesh.nrm, np.float64))
                Nw = N @ rows                                      # ワールドでの向き
                Nw /= np.linalg.norm(Nw, axis=1, keepdims=True) + 1e-12
                Pw = P @ rows + c                                  # カメラ（原点）から見た位置
                view = -Pw / (np.linalg.norm(Pw, axis=1, keepdims=True) + 1e-12)
                ndv = np.einsum("ij,ij->i", Nw, view)
                if holo is None:
                    ndl = Nw @ self.key_dir
                    k = _smoothstep(-0.05, 0.18, ndl)             # トゥーン調：明るい面と影の面
                    rim = (1.0 - np.clip(ndv, 0.0, None)) ** 3 * (0.4 + 0.6 * _smoothstep(-0.3, 0.4, -ndl))
                    skinned[mi] = (P, N, k, rim, None)
                else:
                    skinned[mi] = (P, N, None, None, (1.0 - np.abs(ndv)) ** 2)
            P, N, k, rim, fres = skinned[mi]
            part = parts[pi]
            src = part["src"]
            buf[:, 0:3] = P[src]
            buf[:, 3:6] = N[src]
            if holo is None:
                kk = k[src][:, None]
                lit = (part["shade"] * self.fill_col) * (1 - kk) + self.key_col * kk
                lit = lit + self.rim_col * (rim[src][:, None] * RIM_GAIN)
                buf[:, 6:9] = lit * part["color"][:3]
                buf[:, 9] = part["color"][3]
            else:
                # 立体映像：色の欄に (縁の光り方, 高さ m, 明るさの倍率)、不透明度に濃さ（alpedia_view の mesh_holo の決まり）
                buf[:, 6] = fres[src]
                buf[:, 7] = P[src, 1] * scale
                buf[:, 8] = part["lum"]
                buf[:, 9] = float(holo)
                V.mesh_holo(dm.mid, True, float(t))
            V.mesh_set(dm.mid, buf)

    def _pose(self, t, dt, origin, write=True):
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
        # 歩く・走る：足元の動きから速さを求める（ゲームのモーションの歩き方になる）
        p_prev = np.asarray(self.place(t - dt)[0], float)
        vel = (pos - p_prev) / dt
        tang = vel - up * float(vel @ up)
        speed = float(np.linalg.norm(tang))
        if speed > 12:          # 瞬間移動（カットの切り替え）は歩きにしない
            speed, tang = 0.0, np.zeros(3)
        st = {"speed": speed, "vup": 0.0, "tang": tang, "on_ground": True, "flying": False, "water": 0.0,
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
        # 頭の骨の実際の位置（カメラの狙いに使う）
        hj = getattr(self, "_head_joint", None)
        if hj is None:
            hn = self.v.human["head"]
            idx = np.nonzero(self.v.joint_node == hn)[0]
            self._head_joint = hj = (int(idx[0]) if len(idx) else -1)
            self._head_off = np.linalg.inv(self.v.ibm[hj]) if hj >= 0 else None
        if hj >= 0:
            G = np.vstack([M[hj], [0, 0, 0, 1]]) @ self._head_off
            local = G[:3, 3] + G[:3, :3] @ np.array([0.0, 0.07, 0.0])      # 目の高さ（頭の骨の少し上）
            self._head_world = (t, local @ rows + place)
        self._last = (t, M, rows, place)
        if write:
            c = place - origin
            self.root.setMat([*[float(x) for x in rows[0]], 0.0, *[float(x) for x in rows[1]], 0.0,
                              *[float(x) for x in rows[2]], 0.0, *[float(x) for x in c], 1.0])

    def bone_world(self, name, t=None):
        """骨（VRM の名前：rightHand など）の、直前に描いた姿勢でのワールド位置。"""
        last = getattr(self, "_last", None)
        if last is None:
            return None
        t0, M, rows, place = last
        cache = self.__dict__.setdefault("_bone_idx", {})
        if name not in cache:
            hn = self.v.human[name]
            idx = np.nonzero(self.v.joint_node == hn)[0]
            j = int(idx[0])
            cache[name] = (j, np.linalg.inv(self.v.ibm[j]))
        j, off = cache[name]
        G = np.vstack([M[j], [0, 0, 0, 1]]) @ off
        p = G[:3, 3] @ rows + place
        if t is not None:
            p = p + (np.asarray(self.place(t)[0], float) - np.asarray(self.place(t0)[0], float))
        return p

    def face_dir(self, t):
        yaw = self.place(t)[1]
        return np.array([math.sin(math.radians(yaw)), 0.0, math.cos(math.radians(yaw))])

    def mouth_pos(self, t):
        return self.head_pos(t=t) + self.face_dir(t) * 0.11 * self.scale - np.array([0, 0.09 * self.scale, 0])

    def head_pos(self, pos=None, yaw=None, t=None):
        """頭（目の高さ）のワールド位置。直前に描いた姿勢があればその骨の位置、なければめやす。"""
        hw = getattr(self, "_head_world", None)
        if pos is None and hw is not None and t is not None and abs(hw[0] - t) < 0.2:
            p_now = np.asarray(self.place(t)[0], float)
            p_then = np.asarray(self.place(hw[0])[0], float)
            return hw[1] + (p_now - p_then)
        if pos is None:
            pos, yaw = self.place(t)
        v = self.v
        h = float((v.bone_pos("head") - v.bounds()[0]) @ self.motion.U) * self.scale + 0.08 * self.scale
        seat = float(self.motion.p.get("seat", 0.0))
        h -= (self.motion.hips_h * self.scale - 0.08) * seat + 0.12 * seat
        return np.asarray(pos, float) + np.array([0.0, h, 0.0])
