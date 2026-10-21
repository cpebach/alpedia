"""ストーリー MOD（画面側）：景色の箱を描く、目標の欄、無線のセリフ、目標の目印、修理の場所の光る枠、章の一覧（J）。"""
import time as _time

import numpy as np

from main.client.addon_hooks import ClientAddon
from main.client.render import DynamicMesh, quad_tris, shade

from . import script, sites

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


def wrap(text, n=34):
    """日本語の文を n 文字くらいで折り返す（句読点の後ろで切りやすく）。"""
    out = []
    for para in text.split("\n"):
        line = ""
        for ch in para:
            line += ch
            if len(line) >= n and ch in "、。！？」）" or len(line) >= n + 6:
                out.append(line)
                line = ""
        if line or not out:
            out.append(line)
    return "\n".join(out)


TEXTURES = {"rock": ("rock.png", 3.0), "metal": ("metal.png", 2.0), "ground": ("ground.png", 2.5)}
_tex_cache = {}


def _texture(kind):
    if kind not in _tex_cache:
        from panda3d.core import Filename, SamplerState
        from ursina import application
        from main import paths
        t = application.base.loader.loadTexture(Filename.fromOsSpecific(str(paths.SAUCE / "textures" / TEXTURES[kind][0])))
        t.setMinfilter(SamplerState.FT_linear_mipmap_linear)
        _tex_cache[kind] = t
    return _tex_cache[kind]


def tex_kind(p):
    """景色の箱の見た目の種類：岩・地面（道）・金属（建物など）・なし（光る物）。"""
    if p["glow"] is not None:
        return "plain"
    c = tuple(np.round(p["color"], 2))
    if c in {tuple(np.round(x, 2)) for x in (sites.ROCK, sites.ROCK2, sites.ROCK3, sites.MOON_G)}:
        return "rock"
    if c in {tuple(np.round(x, 2)) for x in (sites.CONC, (0.40, 0.42, 0.45))}:
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
            m = DynamicMesh(parent, f"story_{scene.id}_{name}")
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


class StoryClient(ClientAddon):
    name = "story"
    help_text = "J ストーリー（章の一覧・やり直し）"

    def __init__(self, game):
        super().__init__(game)
        self.pack = game.world.addon("story")
        self.views = []
        self.active = bool(self.pack is not None and self.pack.active)
        if not self.active:
            return
        U = game.U
        Entity, Text, color, window = U["Entity"], U["Text"], U["color"], U["window"]
        self.color = color
        root = self.game.scene_root
        self.views = [SceneView(root, s) for s in self.pack.scenes]
        ui = game.ui_root
        # 目標の欄（左上、状態の表示の下）
        self.goal_root = Entity(parent=ui, position=window.top_left + (0.012, -0.155))
        bg = color.rgba(0.02, 0.05, 0.1, 0.62)
        from ursina import Button
        self.goal_bg = Button(parent=self.goal_root, model="quad", origin=(-0.5, 0.5), scale=(0.62, 0.2), color=bg,
                              highlight_color=color.rgba(0.05, 0.12, 0.2, 0.72), pressed_color=bg, z=0.02)
        self.goal_bg.on_click = self._goal_click          # タッチ・カーソルを出したクリックでも章の一覧を開ける
        self.goal_title = Text(parent=self.goal_root, text="", position=(0.012, -0.012), origin=(-0.5, 0.5),
                               scale=0.78, color=color.rgb(0.5, 0.95, 1.0))
        self.goal_text = Text(parent=self.goal_root, text="", position=(0.012, -0.045), origin=(-0.5, 0.5),
                              scale=0.68, line_height=1.15)
        # 無線のセリフ（画面の下の真ん中）
        self.say_root = Entity(parent=ui, position=(0, -0.235), enabled=False)
        Entity(parent=self.say_root, model="quad", scale=(1.0, 0.115), color=color.rgba(0.02, 0.04, 0.08, 0.78), z=0.02)
        Entity(parent=self.say_root, model="quad", scale=(0.006, 0.1), position=(-0.497, 0),
               color=color.rgb(0.3, 0.9, 1.0), z=0.01)
        self.say_name = Text(parent=self.say_root, text="", position=(-0.48, 0.045), origin=(-0.5, 0.5), scale=0.72,
                             color=color.rgb(0.45, 0.95, 1.0))
        self.say_text = Text(parent=self.say_root, text="", position=(-0.48, 0.012), origin=(-0.5, 0.5), scale=0.78,
                             line_height=1.12)
        self._line = None             # いま出しているセリフ [番号, 話す人, 文, 出す時刻]
        self._line_t = 0.0
        self._seen = 0
        # 目標の目印
        self.marker = Entity(parent=ui, enabled=False)
        Entity(parent=self.marker, model="quad", scale=0.024, rotation_z=45, color=color.rgb(1.0, 0.8, 0.15))
        Entity(parent=self.marker, model="quad", scale=0.012, rotation_z=45, color=color.rgb(0.1, 0.1, 0.1), z=-0.01)
        self.marker_text = Text(parent=self.marker, position=(0, -0.02), origin=(0, 0.5), scale=0.68, text="",
                                color=color.rgb(1.0, 0.9, 0.5))
        # 修理の場所の光る枠
        self.ghost = DynamicMesh(root, "story_ghost")
        from panda3d.core import TransparencyAttrib
        self.ghost.np.setTransparency(TransparencyAttrib.M_alpha)
        self.ghost.np.setDepthWrite(False)
        self.ghost.np.setBin("transparent", 10)
        self._ghost_n = -1
        self._goal_key = None
        self._goal_t = 0.0

    def destroy(self):
        for v in self.views:
            v.remove()
        self.views = []
        if self.active:
            self.ghost.remove()

    # ------------------------------------------------------------------
    def on_key(self, key):
        if not self.active:
            return False
        if key == self.game.cfg["controls"].get("story_menu", "j"):
            self.open_panel()
            return True
        return False

    def _goal_click(self):
        want = getattr(self.game, "_want_capture", None)
        if want is not None and want():
            return                          # 視点操作中（カーソルなし）の左クリックは、掘る・壊すのほう
        self.open_panel()

    def open_panel(self):
        from .panel import StoryPanel
        self.game._open_panel(lambda: StoryPanel(self.game))

    def aim(self, hit):
        if not self.active or not hit or hit.get("kind") != "story":
            return None
        if hit.get("tag") == "mio":
            return ("管制塔の受付（E でミオと話す）", False)
        return ("", True)

    def interact(self, hit):
        if not self.active or not hit or hit.get("kind") != "story" or not hit.get("tag"):
            return False
        msg = self.world.addon_call("story", "talk", hit["tag"])
        if msg:
            self.world.message(msg)
        return True

    def nearby(self, pos, radius):
        if not self.active:
            return []
        p = self.pack.points.get("mio")
        if p is None:
            return []
        d = float(np.linalg.norm(np.asarray(pos) - p))
        if d > max(radius, 4.0):
            return []
        return [{"label": "ミオと話す", "hit": {"kind": "story", "tag": "mio"}, "kind": "e", "dist": d,
                 "icon": "interact"}]

    # ------------------------------------------------------------------
    def update(self, origin, far):
        if not self.active:
            return
        st = self.pack.st
        t = self.world.time
        for v in self.views:
            v.update(origin, bool(st.get("lights")), _time.time())
        self._update_goals(st)
        self._update_say(st, t)
        self._update_marker(st, origin)
        self._update_ghost(st, origin)

    def _chapter(self, st):
        ch = st.get("ch", 0)
        if st.get("done") or ch >= len(script.CHAPTERS):
            return None
        return script.CHAPTERS[ch]

    def _update_goals(self, st):
        now = _time.time()
        if now - self._goal_t < 0.25:
            return
        self._goal_t = now
        c = self._chapter(st)
        if c is None:
            self.goal_title.text = "ストーリー クリア！"
            lines = script.FREE_PLAY[1:]
        else:
            self.goal_title.text = c["title"]
            lines = []
            for gid, text, _ in c["goals"]:
                done = st.get("goals", {}).get(gid)
                pr = "" if done else self.pack.progress(gid)
                lines.append(("■ " if done else "□ ") + wrap(text, 26).replace("\n", "\n　 ") + (f"  {pr}" if pr else ""))
        lines.append("J（またはここを押す）：章の一覧")
        txt = "\n".join(lines)
        if self.goal_text.text != txt:
            self.goal_text.text = txt
        n = txt.count("\n") + 1
        self.goal_bg.scale = (0.62, 0.058 + n * 0.0215)

    def _update_say(self, st, t):
        log_ = [x for x in st.get("log", []) if x[3] <= t]
        if not log_:
            self.say_root.enabled = False
            return
        last = log_[-1]
        if self._line is None or last[0] != self._line[0]:
            self._line = last
            self._line_t = _time.time()
        who, text = self._line[1], self._line[2]
        age = _time.time() - self._line_t
        show_for = 3.5 + len(text) * 0.09
        if age > show_for:
            self.say_root.enabled = False
            return
        self.say_root.enabled = True
        n = int(age * 40) + 1                              # 1文字ずつ出す
        self.say_name.text = "【無線】" + who if who == script.MIO else who
        self.say_text.text = wrap(text[:n], 34)

    def _target(self, st):
        c = self._chapter(st)
        if c is None:
            return None, ""
        for gid, text, key in c["goals"]:
            if not st.get("goals", {}).get(gid):
                return self.pack.where(key), text.split("（")[0]
        return None, ""

    def _update_marker(self, st, origin):
        target, label = self._target(st)
        if target is None:
            self.marker.enabled = False
            return
        U = self.game.U
        rel = np.asarray(target) - origin
        dist = float(np.linalg.norm(rel))
        if dist < 2.5:
            self.marker.enabled = False
            return
        self.marker.enabled = True
        _, _, cup, look = self.game.look_vectors()
        right = np.cross(cup, look)
        right /= max(np.linalg.norm(right), 1e-9)
        upo = np.cross(look, right)
        xc, yc, zc = float(rel @ right), float(rel @ upo), float(rel @ look)
        fov = U["application"].base.cam.node().getLens().getFov()
        tx, ty = np.tan(np.radians(fov[0]) / 2), np.tan(np.radians(fov[1]) / 2)
        aspect = U["window"].aspect_ratio
        inside = False
        if zc > 0.1:
            fx, fy = xc / zc / tx, yc / zc / ty
            inside = abs(fx) <= 0.95 and abs(fy) <= 0.9
        dtxt = f"{dist / 1000:.1f} km" if dist >= 1000 else f"{dist:.0f} m"
        if inside:
            x, y, arrow = fx * aspect / 2, fy * 0.5, ""
        else:
            dx, dy = (xc, yc) if abs(xc) + abs(yc) > 1e-6 else (1.0, 0.0)
            L_ = max((dx * dx + dy * dy) ** 0.5, 1e-9)
            dx, dy = dx / L_, dy / L_
            hx, hy = aspect / 2 - 0.12, 0.5 - 0.1
            k = min(hx / max(abs(dx), 1e-9), hy / max(abs(dy), 1e-9))
            x, y = dx * k, max(dy * k, -0.33)
            arrow = ("◀ " if dx < 0 else "▶ ") if abs(dx) >= abs(dy) else ("▲ " if dy > 0 else "▼ ")
        self.marker.position = (x, y, -0.05)
        self.marker_text.text = f"{arrow}{label}\n{dtxt}"

    def _update_ghost(self, st, origin):
        """修理の章：なくなった外壁（水色）・推進機（橙）の場所に、光る枠。"""
        c = self._chapter(st)
        ship = self.pack.con("ship") if c is not None and c["id"] in ("reach", "repair", "bike") else None
        if ship is None:
            if self._ghost_n != 0:
                self.ghost.set(np.zeros((0, 12), np.float32))
                self._ghost_n = 0
            return
        R, t0 = ship.rigid_fit()
        size = ship.size
        cells = [(h, (0.3, 0.95, 1.0)) for h in sites.SHIP_HOLES if tuple(h) not in ship.index]
        for h in sites.SHIP_THRUSTERS:
            i = ship.index.get(tuple(h))
            if i is None or not ship.devs[i] or ship.devs[i]["def"] != "thruster":
                cells.append((h, (1.0, 0.6, 0.15)))
        if not cells:
            if self._ghost_n != 0:
                self.ghost.set(np.zeros((0, 12), np.float32))
                self._ghost_n = 0
            return
        C = np.array([R @ ((np.asarray(h, float) + 0.5) * size) + t0 for h, _ in cells]) - origin
        A = np.repeat(R.T[None], len(cells), axis=0)
        E = np.full((len(cells), 3), size * 0.5)
        V, N = box_quads(C, A, E)
        a = 0.28 + 0.18 * np.sin(_time.time() * 4.0)
        v = np.empty((len(cells) * 24, 12), np.float32)
        v[:, 0:3] = V.reshape(-1, 3)
        v[:, 3:6] = np.repeat(N.reshape(-1, 3), 4, axis=0)
        v[:, 6:9] = np.repeat(np.array([col for _, col in cells]), 24, axis=0)
        v[:, 9] = a
        v[:, 10:12] = 0
        if len(cells) != self._ghost_n:
            self.ghost.set(v, quad_tris(len(cells) * 6))
            self._ghost_n = len(cells)
        else:
            self.ghost.set(v)
