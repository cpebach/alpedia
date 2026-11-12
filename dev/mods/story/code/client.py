"""シナリオモード（画面側）：景色の箱を描く、目標の欄、セリフ（無線）、目標の目印、置く場所の光る枠、章の一覧（J）、
ムービーを流す。"""
import time as _time

import numpy as np

from main.client.addon_hooks import ClientAddon
from main.client.render import DynamicMesh, quad_tris
from main.client.scenery_view import SceneView, box_quads

from . import script, sites

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


class StoryClient(ClientAddon):
    name = "story"
    help_text = "J シナリオ（章の一覧・ムービー・やり直し）"

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
        self.ghost.np.setTransparency(True)
        self.ghost.np.setDepthWrite(False)
        self.ghost.np.setBin("transparent", 10)
        self._ghost_n = -1
        self._goal_key = None
        self._goal_t = 0.0
        self._movie = None
        self._credits = None

    def destroy(self):
        for v in self.views:
            v.remove()
        self.views = []
        if self.active:
            self.ghost.remove()

    # ------------------------------------------------------------------
    def bgm(self):
        """章ごとの BGM（ムービーの間はムービーの曲、エンドロールは歓喜の歌）。"""
        if not self.active:
            return None
        st = self.pack.st             # ムービーの間の曲は MoviePlayer が決める（set_override）
        if st.get("credits"):
            return script.BGM["credits"]
        c = self._chapter(st)
        return script.BGM["chapters"].get(c["id"]) if c else None

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

    TAGS = {"terminal": ("古い端末（E で触れる）", "古い端末に触れる"), "photo": ("古い記録写真（E で見る）", "記録写真を見る"),
            "record": ("脱出ポッドの記録（E で見る）", "ポッドの記録を見る"),
            "rocket": ("発射台の乗り口（E でロケットに乗る）", "ロケットに乗る")}

    def aim(self, hit):
        if not self.active or not hit or hit.get("kind") != "story":
            return None
        t = self.TAGS.get(hit.get("tag"))
        if t:
            return (t[0], False)
        return ("", True)

    # 物語で「使う」物：構築物のどこを見て E を押しても、その主な機器を使う（ビーコンの電池を見ていても起動する など）
    PRIMARY = {"beacon": "computer", "console": "seat", "bike": "seat", "robot": "seat", "gate": "actuator"}

    def nearby_priority(self, target):
        """今の目標の機器（目印の所にある物）を、近くのボタンのいちばん上に。"""
        if not self.active:
            return 0.0
        goal, _ = self._target(self.pack.st)
        h = target.get("hit") or {}
        pt = h.get("point")
        if goal is None or pt is None:
            return 0.0
        return -10.0 if float(np.linalg.norm(np.asarray(pt) - np.asarray(goal))) < 0.8 else 0.0

    def seat_camera(self, construct):
        """ドローンの操縦台に座っている間は、ドローンを後ろから映す。"""
        if not self.active or construct.id != self.pack.st.get("ids", {}).get("console"):
            return None
        return self.pack.con("drone")

    def _primary_hit(self, hit):
        if not hit or hit.get("kind") != "voxel":
            return None
        c = hit["construct"]
        ids = self.pack.st.get("ids", {})
        role = next((r for r in self.PRIMARY if ids.get(r) == c.id), None)
        if role is None:
            return None
        want = self.PRIMARY[role]
        d = c.devs[hit["index"]]
        if d and d["def"] == want:
            return None                      # もうその機器を見ている
        for i, x in enumerate(c.devs):
            if x and x["def"] == want:
                ctr = c.pos[8 * i:8 * i + 8].mean(0)
                return {"kind": "voxel", "t": hit.get("t", 1.0), "construct": c, "index": i, "face": 2, "point": ctr}
        return None

    def interact(self, hit):
        if not self.active or not hit:
            return False
        ph = self._primary_hit(hit)
        if ph is not None:
            self.world.interact(ph)
            return True
        if hit.get("kind") != "story" or not hit.get("tag"):
            return False
        msg = self.world.addon_call("story", "talk", hit["tag"])
        if msg:
            self.world.message(msg)
        return True

    def nearby(self, pos, radius):
        if not self.active:
            return []
        out = []
        for tag, (_, label) in self.TAGS.items():
            p = self.pack.points.get(tag)
            if p is None:
                continue
            d = float(np.linalg.norm(np.asarray(pos) - p))
            if d <= max(radius, 3.0):
                out.append({"label": label, "hit": {"kind": "story", "tag": tag}, "kind": "e", "dist": d,
                            "icon": "interact"})
        return out

    # ------------------------------------------------------------------
    def update(self, origin, far):
        if not self.active:
            return
        st = self.pack.st
        t = self.world.time
        for v in self.views:
            v.update(origin, bool(st.get("lights")), _time.time())
        self._update_movies(st)
        self._update_choice(st)
        self._update_credits(st)
        self._update_goals(st)
        self._update_say(st, t)
        self._update_marker(st, origin)
        self._update_ghost(st, origin)

    def _update_movies(self, st):
        """まだ流していないムービーがあれば流す（ほかの画面を開いていない時）。"""
        g = self.game
        if self._movie is not None or not st.get("movies") or g.editor is not None or g.paused \
                or g.cam_mode is not None:
            return
        name = st["movies"][0]
        from .movie import MoviePlayer

        def done(n):
            self._movie = None
            g.hold = False
            g.editor_closed()
            self.world.addon_call("story", "movie_done", n)
        g.hold = True
        self._movie = name
        g._open_panel(lambda: MoviePlayer(g, name, done))
        if g.editor is None:              # 開けなかった
            done(name)

    def _update_choice(self, st):
        """第12章のあと：ネムリの前で選ぶ欄（ムービーのあと・ほかの画面を開いていない時）。"""
        g = self.game
        c = self._chapter(st)
        if (c is None or c["id"] != "robot" or not st.get("goals", {}).get("clash") or st.get("flags", {}).get("choice")
                or st.get("movies") or self._movie is not None or g.editor is not None or g.paused
                or g.cam_mode is not None):
            return
        from .choice import ChoicePanel

        def done(key):
            g.editor_closed()
            self.world.addon_call("story", "choose", key)
        g._open_panel(lambda: ChoicePanel(g, done))

    def _update_credits(self, st):
        """軌道に乗ったら（st["credits"]）エンドロールを流し、終わったらワールドの選択（メニュー）へ戻る。"""
        g = self.game
        if self._credits is not None or not st.get("credits") or st.get("movies") or self._movie is not None:
            return
        if g.cam_mode is not None:
            g.exit_camera()
        if g.editor is not None or g.paused:
            return
        from .credits import CreditsRoll

        def done():
            self._credits = None
            g.editor_closed()
            self.world.addon_call("story", "credits_done")
            if self.world.net is None:
                g._leave("menu", status="ストーリー vol.1 クリア！ 遊んでくれてありがとう")
        self._credits = True
        g._open_panel(lambda: CreditsRoll(g, done))
        if g.editor is None:
            self._credits = None

    def on_photo(self, path):
        if self.active:
            self.world.addon_call("story", "photo_taken")

    def on_camera(self, on):
        if self.active:
            self.world.addon_call("story", "posing", bool(on))

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
            self.goal_title.text = "シナリオ クリア！"
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
        self.say_name.text = "【無線】" + who if who in script.RADIO else who
        self.say_text.text = wrap(text[:n], 34)

    def _target(self, st):
        c = self._chapter(st)
        if c is None:
            return None, ""
        for gid, text, key in c["goals"]:
            if not st.get("goals", {}).get(gid):
                p = None
                dev = self.MARK_DEVICE.get(gid)
                if dev is not None:                  # 目印は「触る機器」の上に（構築物の重心は何もない所のことがある）
                    try:
                        p = self.pack._devpos(*dev)
                    except Exception:  # noqa: BLE001
                        p = None
                if p is None:
                    p = self.pack.where(key)
                return p, text.split("（")[0]
        return None, ""

    # 目標 → 目印を付ける機器 (役, 機器の種類, 名前)
    MARK_DEVICE = {"beacon": ("beacon", "computer", None), "door": ("crane", "computer", None),
                   "trips": ("crane", "computer", None), "rod": ("task", "computer", None),
                   "console": ("console", "seat", None), "ride": ("bike", "seat", None),
                   "seat": ("ship", "seat", None)}

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

    def _ghost_cells(self, st):
        """光る枠を出す (構築物, [(セル, 色)])：修理の場所・切られた配管・推進モジュールの置き場。"""
        c = self._chapter(st)
        if c is None:
            return []
        cid = c["id"]
        if cid in ("bike", "repair"):
            ship = self.pack.con("ship")
            if ship is None:
                return []
            cells = [(h, (0.3, 0.95, 1.0)) for h in sites.SHIP_HOLES if tuple(h) not in ship.index]
            for h in sites.SHIP_THRUSTERS:
                i = ship.index.get(tuple(h))
                if i is None or not ship.devs[i] or ship.devs[i]["def"] != "thruster":
                    cells.append((h, (1.0, 0.6, 0.15)))
            groups = [(ship, cells)]
            cart = self.pack.con("cart")
            if cid == "repair" and cart is not None:
                hose = [(h, (0.95, 0.9, 0.3)) for h in sites.HOSE if tuple(h) not in cart.index]
                groups.append((cart, hose))
            return groups
        if cid == "factory" and st.get("flags", {}).get("pipe_cut"):
            fac = self.pack.con("factory")
            if fac is not None and tuple(sites.FACTORY_CUT) not in fac.index:
                return [(fac, [(sites.FACTORY_CUT, (1.0, 0.75, 0.25))])]
        if cid == "reaccel":
            from . import space
            unit = self.pack.con("unit")
            if unit is not None:
                cells = []
                for h in space.UNIT_SLOTS:
                    i = unit.index.get(tuple(h))
                    if i is None or not unit.devs[i] or unit.devs[i]["def"] != "rocket_module":
                        cells.append((h, (0.4, 1.0, 0.7)))
                return [(unit, cells)]
        return []

    def _update_ghost(self, st, origin):
        """光る枠：なくなった外壁（水色）・推進機（橙）・切られた配管（黄）・推進モジュール（緑）の場所。"""
        groups = [(con, cells) for con, cells in self._ghost_cells(st) if con is not None and cells]
        cells = [x for _, cl in groups for x in cl]
        if not cells:
            if self._ghost_n != 0:
                self.ghost.set(np.zeros((0, 12), np.float32))
                self._ghost_n = 0
            return
        C, A = [], []
        for con, cl in groups:
            R, t0 = con.rigid_fit()
            size = con.size
            C += [R @ ((np.asarray(h, float) + 0.5) * size) + t0 - origin for h, _ in cl]
            A += [R.T] * len(cl)
        C, A = np.array(C), np.array(A)
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
