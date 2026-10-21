"""カメラモード（自撮り・建造物の撮影用）。画像の保存はしない（OS のスクリーンショットで撮る）。

- 自分の体を三人称で映し、カメラは自分のまわりを回る（ドラッグ・なぞる）、ホイール／ピンチで距離、
  WASD・スティックで注目する所を動かす、Space・Ctrl で上下
- パネル：画面の表示（UI）・ブロックの枠の表示、モーション（MOD のかわいいモーションなど）、体の向き、視野角、傾き、距離、
  パネルを隠す（画面を1回タップ・クリックか H で戻る）、元に戻す、終わる
- 遊びは止めない（ほかの人・機械は動き続ける）。自分は動かない
"""
import math
import time as _time

import numpy as np

from .. import log

L = log.get("camera")


def _rot(axis, ang):
    axis = np.asarray(axis, dtype=np.float64)
    axis = axis / max(np.linalg.norm(axis), 1e-12)
    x, y, z = axis
    K = np.array([[0, -z, y], [z, 0, -x], [-y, x, 0]])
    return np.eye(3) + math.sin(ang) * K + (1 - math.cos(ang)) * (K @ K)


class CameraMode:
    def __init__(self, game):
        self.game = game
        U = game.U
        self.U = U
        w = game.world
        p = w.local_player
        fwd, right, up, look = game.look_vectors()
        self.up0 = up.copy()
        self.yaw = math.pi                  # 0 = 自分の後ろから。π = 正面から（自撮り）
        self.pitch = 0.12
        self.dist = 3.2
        self.height = 0.62                  # 注目点の高さ（身長に対する割合）
        self.pan = np.zeros(3)
        self.roll = 0.0
        self.fov0 = float(U["camera"].fov)
        self.fov = self.fov0
        self.ui_on = False                  # ゲームの画面の表示（HUD・アイテムバーなど）
        self.frame_on = False               # ブロックを置く枠・視線の先の枠
        self.panel_on = True
        self.base_fwd = fwd.copy()          # 体の向き（モードに入ったときの視線の向き）
        self.body_yaw = 0.0                 # 体の向き（0 = モードに入ったときの向き＝カメラの方）
        self.emotes = ["（自動）"] + list(game.emotes)
        self.emote_i = 0
        self.ptr_own = {}
        self.tap = None
        self.prev_third = game.third_person
        p.emote = None
        game.ui_root.enabled = False
        game.set_mouse_capture(False)
        self._build()
        self._apply_face()
        L.info("カメラモード開始")

    # ------------------------------------------------------------------
    def _build(self):
        from ursina import Entity, Text, Button, color, camera, window
        self.root = Entity(parent=camera.ui)
        x0 = window.right.x - 0.2
        self.panel = Entity(parent=self.root)
        Entity(parent=self.panel, model="quad", position=(x0, 0.0, 0.02), scale=(0.37, 0.9),
               color=color.rgba(0.05, 0.06, 0.09, 0.72))
        Text(parent=self.panel, text="カメラモード", position=(x0, 0.42), origin=(0, 0), scale=1.1)
        self.rows = {}
        y = 0.35

        def row(key, label, fn_l, fn_r=None, val=""):
            nonlocal y
            Text(parent=self.panel, text=label, position=(x0 - 0.17, y), origin=(-0.5, 0), scale=0.7)
            t = Text(parent=self.panel, text=val, position=(x0 + 0.07, y), origin=(0, 0), scale=0.7)
            if fn_r is None:
                b = Button(parent=self.panel, text="切替", position=(x0 + 0.14, y), scale=(0.07, 0.04),
                           color=color.rgb(0.22, 0.32, 0.45), text_size=0.6)
                b.on_click = fn_l
            else:
                for dx, fn, txt in ((-0.0, fn_l, "◀"), (0.14, fn_r, "▶")):
                    b = Button(parent=self.panel, text=txt, position=(x0 + dx, y), scale=(0.035, 0.035),
                               color=color.rgb(0.22, 0.32, 0.45), text_size=0.6)
                    b.on_click = fn
            self.rows[key] = t
            y -= 0.058
        row("ui", "画面の表示", self.toggle_ui)
        row("frame", "ブロックの枠", self.toggle_frame)
        row("emote", "モーション", lambda: self.step_emote(-1), lambda: self.step_emote(1))
        row("body", "体の向き", lambda: self.turn_body(-1), lambda: self.turn_body(1))
        row("dist", "距離", lambda: self.zoom(1 / 0.85), lambda: self.zoom(0.85))
        row("height", "高さ", lambda: self.lift(-1), lambda: self.lift(1))
        row("fov", "視野角", lambda: self.set_fov(-5), lambda: self.set_fov(5))
        row("roll", "傾き", lambda: self.set_roll(-1), lambda: self.set_roll(1))
        for text, fn, col in (("パネルを隠す", self.hide_panel, color.rgb(0.25, 0.35, 0.45)),
                              ("カメラを元に戻す", self.reset, color.rgb(0.3, 0.3, 0.33)),
                              ("カメラモードを終わる", self.game.exit_camera, color.rgb(0.5, 0.25, 0.2))):
            b = Button(parent=self.panel, text=text, position=(x0, y - 0.01), scale=(0.3, 0.048), color=col,
                       text_size=0.7)
            b.on_click = fn
            y -= 0.06
        Text(parent=self.panel, position=(x0, y - 0.005), origin=(0, 0.5), scale=0.55, color=color.light_gray,
             text="ドラッグ・なぞる：カメラが回る\nホイール・2本指：距離\nWASD・スティック：注目する所を動かす\n"
                  "Space / Ctrl：上下　H：パネル\n画像は OS のスクリーンショットで")
        self.hint = Text(parent=self.root, text="画面をタップ（クリック）するとパネルが出ます", origin=(0, 0),
                         position=(0, -0.45), scale=0.7, color=color.rgba(1, 1, 1, 0.0), enabled=False)
        self._hint_t = 0.0
        self._refresh()

    def _refresh(self):
        r = self.rows
        r["ui"].text = "表示" if self.ui_on else "隠す"
        r["frame"].text = "表示" if self.frame_on else "隠す"
        r["emote"].text = self.emotes[self.emote_i][:9]
        r["body"].text = f"{math.degrees(self.body_yaw) % 360:.0f}°"
        r["dist"].text = f"{self.dist:.1f} m"
        r["height"].text = f"{self.height * 100:.0f}%"
        r["fov"].text = f"{self.fov:.0f}°"
        r["roll"].text = f"{math.degrees(self.roll):+.0f}°"

    # --- 操作 --------------------------------------------------------------
    def toggle_ui(self):
        self.ui_on = not self.ui_on
        self.game.ui_root.enabled = self.ui_on
        self._refresh()

    def toggle_frame(self):
        self.frame_on = not self.frame_on
        self._refresh()

    def step_emote(self, d):
        self.emote_i = (self.emote_i + d) % len(self.emotes)
        p = self.game.world.local_player
        p.emote = None if self.emote_i == 0 else self.emotes[self.emote_i]
        p.emote_t0 = _time.monotonic()
        self._refresh()

    def turn_body(self, d):
        self.body_yaw = (self.body_yaw + d * math.radians(30)) % (2 * math.pi)
        self._apply_face()
        self._refresh()

    def _apply_face(self):
        p = self.game.world.local_player
        up = p.up / np.linalg.norm(p.up)
        f = self.base_fwd - up * (self.base_fwd @ up)
        f = f / max(np.linalg.norm(f), 1e-9)
        p.face_dir = _rot(up, -self.body_yaw) @ f

    def zoom(self, k):
        self.dist = float(np.clip(self.dist * k, 0.6, 60.0))
        self._refresh()

    def lift(self, d):
        self.height = float(np.clip(self.height + d * 0.1, -0.2, 1.6))
        self._refresh()

    def set_fov(self, d):
        self.fov = float(np.clip(self.fov + d, 15, 120))
        self.U["camera"].fov = self.fov
        self._refresh()

    def set_roll(self, d):
        self.roll = float(np.clip(self.roll + d * math.radians(5), -math.radians(45), math.radians(45)))
        self._refresh()

    def hide_panel(self):
        self.panel_on = False
        self.panel.enabled = False
        self.hint.enabled = True
        self._hint_t = 2.5
        self.hint.color = self.U["color"].rgba(1, 1, 1, 0.85)

    def show_panel(self):
        self.panel_on = True
        self.panel.enabled = True
        self.hint.enabled = False

    def reset(self):
        self.yaw, self.pitch, self.dist, self.height = math.pi, 0.12, 3.2, 0.62
        self.pan[:] = 0
        self.roll = 0.0
        self.fov = self.fov0
        self.U["camera"].fov = self.fov
        self._refresh()

    def on_key(self, key):
        """カメラモード中のキー。使ったら True。"""
        c = self.game.cfg["controls"]
        if key in ("escape", c.get("camera_mode", "c")):
            if not self.panel_on and key == "escape":
                self.show_panel()
            else:
                self.game.exit_camera()
            return True
        if key == "h":
            self.show_panel() if not self.panel_on else self.hide_panel()
            return True
        if key == "scroll up":
            self.zoom(0.9)
            return True
        if key == "scroll down":
            self.zoom(1 / 0.9)
            return True
        return True                         # ほかのキーはゲームの操作に回さない（移動はカメラの方で読む）

    # --- 毎フレーム ------------------------------------------------------------
    def update(self, dt, held, vkeys, stick):
        g = self.game
        c = g.cfg["controls"]
        if self._hint_t > 0:
            self._hint_t -= dt
            if self._hint_t <= 0:
                self.hint.enabled = False
        self._pointers()
        # 注目点を動かす（WASD・スティック）
        p = g.world.local_player
        up = p.up / np.linalg.norm(p.up)
        cf, cr = self._cam_axes(up)
        H = lambda k: 1.0 if (held[k] or vkeys.get(k)) else 0.0  # noqa: E731
        mv = cf * (H(c["forward"]) - H(c["back"]) + stick[1]) + cr * (H(c["right"]) - H(c["left"]) + stick[0])
        mv += up * (H(c["jump"]) - H(c["down"]))
        sp = 1.5 + self.dist * 0.4
        self.pan += mv * sp * dt
        n = np.linalg.norm(self.pan)
        if n > 40:
            self.pan *= 40 / n

    def _cam_axes(self, up):
        """カメラの前（地面に沿って）と右。"""
        f = self.base_fwd - up * (self.base_fwd @ up)
        f = f / max(np.linalg.norm(f), 1e-9)
        f = _rot(up, -self.yaw) @ f
        right = np.cross(up, f)
        return f, right                     # 画面の奥・右（カメラは注目点の -f 側から f の向きに見る）

    def _pointers(self):
        """ドラッグで回る・2本指で距離・タップでパネルを出す（パネルの上から始めた指は使わない）。"""
        ptr = self.game.touch_hud.ptr
        pts = ptr.current()
        try:
            from ursina import mouse
            over_panel = bool(mouse.hovered_entity) and self.panel_on
        except Exception:  # noqa: BLE001
            over_panel = False
        for pid in [k for k in self.ptr_own if k not in pts]:
            o = self.ptr_own.pop(pid)
            if o and not self.panel_on and _time.monotonic() - o["t0"] < 0.35 and o["moved"] < 0.015:
                self.show_panel()           # 画面を1回タップ：パネルを出す
        for pid, (x, y) in pts.items():
            o = self.ptr_own.get(pid, False)
            if o is False:
                self.ptr_own[pid] = None if over_panel else {"last": (x, y), "t0": _time.monotonic(), "moved": 0.0}
                continue
            if o is None:
                continue
            lx, ly = o["last"]
            dx, dy = x - lx, y - ly
            o["last"] = (x, y)
            o["moved"] += abs(dx) + abs(dy)
            live = [v for v in self.ptr_own.values() if v]
            if len(live) >= 2:
                a, b = live[0]["last"], live[1]["last"]
                d = math.hypot(a[0] - b[0], a[1] - b[1])
                pd = o.get("pinch")
                if pd:
                    self.zoom(pd / max(d, 1e-3))
                o["pinch"] = d
            else:
                self.yaw -= dx * 3.0
                self.pitch = float(np.clip(self.pitch - dy * 2.5, -1.4, 1.4))

    def camera(self):
        """(カメラの位置, 視線, 上) をワールドで。"""
        p = self.game.world.local_player
        up = p.up / np.linalg.norm(p.up)
        f = self.base_fwd - up * (self.base_fwd @ up)
        f = f / max(np.linalg.norm(f), 1e-9)
        f = _rot(up, -self.yaw) @ f
        right = np.cross(up, f)
        focus = p.pos + up * (p.height * self.height - p.radius) + self.pan
        d = (-f * math.cos(self.pitch) + up * math.sin(self.pitch)) * self.dist
        pos = focus + d
        look = focus - pos
        look /= np.linalg.norm(look)
        cu = up - look * (up @ look)
        cu /= max(np.linalg.norm(cu), 1e-9)
        cu = _rot(look, self.roll) @ cu
        return pos, look, cu, right

    def destroy(self):
        from .ui_destroy import destroy
        destroy(self.root)
        g = self.game
        g.U["camera"].fov = self.fov0
        g.ui_root.enabled = True
        p = g.world.local_player
        p.emote = None
        p.face_dir = None
        L.info("カメラモード終了")
