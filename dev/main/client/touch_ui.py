"""画面のボタン（タッチ・マウスで押せる操作）と、仮想ジョイスティック。

- 右上：メニュー・持ち物・マップなどの小さなボタン（いつも出ている）
- 右下：ジャンプ・掘る・置くなどの操作ボタン（操縦席では「降りる」と、機器に割り当てたキー）
- 右の中ほど：近くにある、触れるもの（操縦席・コンピューター・お店・NPC など）のボタン（近づくと出る）
- 左下：仮想ジョイスティック（ゲーム設定・一時停止メニューで ON にしたとき）。画面のほかの所をなぞると視点が回る

指（タッチ）は、Windows の複数タッチが使えればそれを、使えなければマウスの左ボタン（1本の指はマウスとして届く）を使う。
キーボード・マウスで遊ぶときは、Alt を押している間カーソルが出るので、ボタンをクリックできる。
"""
import time as _time

import numpy as np

from .. import log
from . import icons

L = log.get("touch")

SMALL = 0.034           # 右上のボタンの半径（画面の高さ = 1）
BIG = 0.058
MID = 0.046
STICK_R = 0.11          # ジョイスティックの半径
KEY_NAMES = {"space": "Space", "left control": "Ctrl", "left shift": "Shift", "escape": "Esc", "left alt": "Alt",
             "up arrow": "↑", "down arrow": "↓", "left arrow": "←", "right arrow": "→", "tab": "Tab",
             "enter": "Enter", "backspace": "BS"}


def key_name(k):
    if not k:
        return ""
    return KEY_NAMES.get(k, k.upper() if len(k) <= 3 else k)


class Pointers:
    """いま画面に触れている指（またはマウスの左ボタン）：{番号: (x, y)}（画面の UI 座標）。"""

    def __init__(self, game):
        self.game = game
        self.touches = {}
        self.touch_t = 0.0
        self.multi = False
        win = game.U["application"].base.win
        add = getattr(win, "add_python_event_handler", None) or getattr(win, "addPythonEventHandler", None)
        if add is not None:
            try:
                add(self._on_event, "alpedia_touch")
                self.multi = True
            except Exception as e:  # noqa: BLE001
                L.info("複数タッチは使えません（%s）。1本の指（マウス）で操作します", e)
        L.info("タッチ：%s", "複数タッチ（Windows）とマウス" if self.multi else "マウス（1本の指）")

    def _on_event(self, data):
        try:
            if not data.is_touch_event():
                return
            seen = set()
            for i in range(data.get_num_touches()):
                t = data.get_touch_info(i)
                tid = t.get_id()
                f = t.get_flags()
                if f & 4:                                   # 指を離した（TIF_up）
                    self.touches.pop(tid, None)
                else:
                    self.touches[tid] = (t.get_x(), t.get_y())
                    seen.add(tid)
            self.touch_t = _time.monotonic()
        except Exception:  # noqa: BLE001
            L.exception("タッチの処理でエラー")

    def _to_ui(self, px, py):
        win = self.game.U["application"].base.win
        w, h = max(win.getXSize(), 1), max(win.getYSize(), 1)
        return (px - w / 2) / h, -(py - h / 2) / h

    def current(self):
        if self.touches and _time.monotonic() - self.touch_t < 5.0:
            return {("t", k): self._to_ui(*v) for k, v in self.touches.items()}
        self.touches.clear()
        base = self.game.U["application"].base
        try:
            from panda3d.core import MouseButton
            if base.mouseWatcherNode.is_button_down(MouseButton.one()):
                return {"m": self.mouse()}
        except Exception:  # noqa: BLE001
            pass
        return {}

    def mouse(self):
        win = self.game.U["application"].base.win
        md = win.getPointer(0)
        return self._to_ui(md.getX(), md.getY())


class Btn:
    def __init__(self, hud, bid, icon, label, r, kind="tap", key=None, on_tap=None, press_key=False):
        U = hud.U
        self.id, self.r, self.kind, self.key, self.on_tap, self.press_key = bid, r, kind, key, on_tap, press_key
        self.color = U["color"]
        self.label = label
        self.x = self.y = 0.0
        self.visible = False
        self.pressed = False
        self.root = U["Entity"](parent=hud.root, enabled=False)
        self.img = U["Entity"](parent=self.root, model="quad", texture=icons.ui_texture(icon), scale=2 * r)
        self.text = U["Text"](parent=self.root, text=label, origin=(0, 0.5), position=(0, -r - 0.004),
                              scale=0.55 if r < MID else 0.62, color=U["color"].rgba(1, 1, 1, 0.92))

    def place(self, x, y, visible=True, label=None):
        self.x, self.y = x, y
        self.root.position = (x, y, 0)
        if label is not None and label != self.text.text:
            self.text.text = label
        if visible != self.visible:
            self.visible = visible
            self.root.enabled = visible
            if not visible:
                self.pressed = False

    def hit(self, x, y):
        return self.visible and (x - self.x) ** 2 + (y - self.y) ** 2 <= (self.r * 1.15) ** 2

    def show_pressed(self, on):
        self.img.scale = 2 * self.r * (0.9 if on else 1.0)
        self.img.color = self.color.rgb(0.7, 0.88, 1.0) if on else self.color.white


class Pill:
    """近くのもの（お店・操縦席など）のボタン：横長の札。"""

    def __init__(self, hud):
        U = hud.U
        self.w, self.h = 0.34, 0.052
        self.root = U["Entity"](parent=hud.root, enabled=False)
        self.bg = U["Entity"](parent=self.root, model="quad", scale=(self.w, self.h), color=U["color"].rgba(0.08, 0.1, 0.14, 0.78))
        self.icon = U["Entity"](parent=self.root, model="quad", scale=self.h * 0.85, x=-self.w / 2 + self.h * 0.55,
                                texture=icons.ui_texture("interact"))
        self.text = U["Text"](parent=self.root, text="", origin=(-0.5, 0), x=-self.w / 2 + self.h * 1.15, scale=0.68)
        self.target = None
        self.x = self.y = 0.0

    def set(self, target, x, y, key_hint):
        self.target = target
        self.x, self.y = x, y
        self.root.position = (x, y, 0)
        self.icon.texture = icons.ui_texture(target.get("icon", "interact"))
        txt = target["label"] + (f"  [{key_hint}]" if key_hint else "")
        if self.text.text != txt:
            self.text.text = txt
        self.root.enabled = True

    def hide(self):
        self.root.enabled = False
        self.target = None

    def hit(self, x, y):
        return self.root.enabled and abs(x - self.x) <= self.w / 2 and abs(y - self.y) <= self.h / 2


class TouchHUD:
    def __init__(self, game):
        self.game = game
        self.U = U = game.U
        self.root = U["Entity"](parent=game.ui_root)
        self.ptr = Pointers(game)
        c = game.cfg["controls"]
        g = game

        def key(k):
            return lambda: g.input(c.get(k, k))
        B = Btn
        self.top = [
            B(self, "menu", "menu", "メニュー", SMALL, on_tap=lambda: g.input("escape")),
            B(self, "inventory", "inventory", "持ち物", SMALL, on_tap=key("inventory")),
            B(self, "map", "map", "マップ", SMALL, on_tap=key("city_map")),
            B(self, "marker", "marker", "目印", SMALL, on_tap=key("city_marker")),
            B(self, "view", "view", "視点", SMALL, on_tap=key("third_person")),
            B(self, "camera", "camera", "カメラ", SMALL, on_tap=g.enter_camera),
            B(self, "import", "import", "読み込み", SMALL, on_tap=key("import")),
            B(self, "export", "export", "出力", SMALL, on_tap=key("export")),
            B(self, "save", "save", "保存", SMALL, on_tap=key("save")),
        ]
        self.act = {
            "jump": B(self, "jump", "jump", "ジャンプ", BIG, kind="hold", key="jump"),
            "down": B(self, "down", "down", "下へ", MID, kind="hold", key="down"),
            "jetpack": B(self, "jetpack", "jetpack", "ジェット", MID, on_tap=key("jetpack")),
            "remove": B(self, "remove", "remove", "掘る・壊す", MID, on_tap=g.do_remove),
            "place": B(self, "place", "place", "置く", MID, on_tap=g.do_place),
            "rotate": B(self, "rotate", "rotate", "向き", SMALL, on_tap=key("rotate")),
            "new": B(self, "new", "new", "空中に置く", SMALL, on_tap=key("new_construct")),
            "exit": B(self, "exit", "exit", "降りる", MID, on_tap=lambda: g.world.interact(None)),
        }
        self.seat_btns = {}                 # 操縦席：機器に割り当てたキー → ボタン
        self.pills = [Pill(self) for _ in range(4)]
        self.targets = []
        self._t_scan = 0.0
        # ジョイスティック
        self.stick_bg = U["Entity"](parent=self.root, model="circle", scale=STICK_R * 2, color=U["color"].rgba(0, 0, 0, 0.28),
                                    enabled=False)
        self.stick_knob = U["Entity"](parent=self.root, model="circle", scale=STICK_R * 0.8,
                                      color=U["color"].rgba(1, 1, 1, 0.45), enabled=False)
        self.stick_home = (0.0, 0.0)
        self.stick = np.zeros(2)            # -1..1（右, 前）
        self.look = np.zeros(2)             # なぞった量（画面の高さ = 1）
        self.vkeys = {}                     # ボタンで押しているキー（押している間 1）
        self.own = {}                       # 指 → 何に使っているか
        self.visible = True

    # ------------------------------------------------------------------
    @property
    def touch_mode(self):
        return bool(self.game.cfg["controls"].get("touch", False))

    def key_hint(self, action):
        if self.touch_mode:
            return ""
        return key_name(self.game.cfg["controls"].get(action, ""))

    def over_ui(self, x, y):
        if any(b.hit(x, y) for b in self._all_btns()) or any(p.hit(x, y) for p in self.pills):
            return True
        return self.touch_mode and self._in_stick_zone(x, y)

    def mouse_over_ui(self):
        try:
            return self.over_ui(*self.ptr.mouse())
        except Exception:  # noqa: BLE001
            return False

    def _all_btns(self):
        return self.top + list(self.act.values()) + list(self.seat_btns.values())

    def _in_stick_zone(self, x, y):
        win = self.U["window"]
        return x < win.left.x + 0.62 and y < 0.05

    # ------------------------------------------------------------------
    def layout(self):
        g = self.game
        w = g.world
        win = self.U["window"]
        right, top = win.right.x, win.top.y
        seated = w.player.seated is not None
        city = w.addon("city")
        has_city = bool(city is not None and getattr(city, "enabled", False))
        net = w.net
        can_save = net is None or net.can_save()
        show = {"menu": True, "inventory": not seated, "map": has_city, "marker": has_city, "view": True, "camera": True,
                "import": not seated, "export": True, "save": can_save}
        hints = {"menu": "Esc", "inventory": "inventory", "map": "city_map", "marker": "city_marker",
                 "view": "third_person", "camera": "camera_mode", "import": "import", "export": "export", "save": "save"}
        x = right - 0.045
        for b in self.top:
            if not show[b.id]:
                b.place(0, 0, False)
                continue
            h = "Esc" if b.id == "menu" and not self.touch_mode else self.key_hint(hints[b.id])
            b.place(x, top - 0.045, True, b.label + (f" {h}" if h and b.id != "menu" else (" Esc" if h else "")))
            x -= SMALL * 2 + 0.03
        # 右下の操作
        bx, by = right - 0.1, -0.36
        A = self.act
        flying = bool(w.player.jetpack) or w.player.in_water > 0.3
        lay = {
            "jump": (bx, by, not seated), "down": (bx, by + 0.15, not seated and flying),
            "jetpack": (bx - 0.02, by + 0.27 if flying else by + 0.15, not seated),
            "remove": (bx - 0.13, by + 0.07, not seated), "place": (bx - 0.15, by - 0.06, not seated),
            "rotate": (bx - 0.25, by + 0.02, not seated), "new": (bx - 0.25, by - 0.08, not seated),
            "exit": (bx, by, seated),
        }
        keys = {"jump": "jump", "down": "down", "jetpack": "jetpack", "rotate": "rotate", "new": "new_construct",
                "exit": "interact"}
        mouse_hint = {"remove": "左クリック", "place": "右クリック"}
        for k, (xx, yy, vis) in lay.items():
            h = self.key_hint(keys[k]) if k in keys else ("" if self.touch_mode else mouse_hint.get(k, ""))
            A[k].place(xx, yy, vis, A[k].label + (f" {h}" if h else ""))
        if A["jetpack"].visible:
            on = "ON" if w.player.jetpack else "OFF"
            hk = self.key_hint("jetpack")
            A["jetpack"].text.text = f"ジェット {on}" + (f" {hk}" if hk else "")
        self._layout_seat(seated, bx, by)

    def _layout_seat(self, seated, bx, by):
        """操縦席：機器に割り当てたキーのボタン（押している間 そのキーを押したことになる）。"""
        want = {}
        if seated:
            w = self.game.world
            c = w.constructs.get(w.player.seated[0])
            if c is not None:
                for d in c.devs:
                    if not d:
                        continue
                    dd = c.reg.device(d["def"]) or {}
                    k = (dd.get("params") or {}).get("key")
                    if k:
                        want.setdefault(k, []).append(d["name"])
            for k in ("space", "left control", "left shift"):
                want.setdefault(k, [])
        for k in [k for k in self.seat_btns if k not in want]:
            b = self.seat_btns.pop(k)
            from .ui_destroy import destroy
            destroy(b.root)
        for n, (k, names) in enumerate(sorted(want.items(), key=lambda kv: (not kv[1], kv[0]))):
            b = self.seat_btns.get(k)
            label = (names[0] if names else key_name(k)) + (f" 他{len(names) - 1}" if len(names) > 1 else "")
            label = label[:8] + (f" {key_name(k)}" if names else "")
            if b is None:
                b = self.seat_btns[k] = Btn(self, f"seat:{k}", "key", label, MID, kind="hold", key=k, press_key=True)
            col, row = n % 3, n // 3
            b.place(bx - 0.13 * (col + 1) + 0.02 * row, by + 0.12 * row + 0.04 * col, True, label)

    # ------------------------------------------------------------------
    def update(self, dt):
        g = self.game
        active = g.editor is None and not g.paused
        if active != self.visible:
            self.visible = active
            self.root.enabled = active
            if not active:
                self._release_all()
        if not active:
            return
        self.layout()
        now = _time.monotonic()
        if now - self._t_scan > 0.2:
            self._t_scan = now
            self.targets = g.nearby_targets()
            self._layout_pills()
        self._pointers()
        self._draw_stick()

    def _layout_pills(self):
        win = self.U["window"]
        x = win.right.x - 0.26
        key = self.key_hint("interact")
        pkey = self.key_hint("edit_script")
        for k, p in enumerate(self.pills):
            if k < len(self.targets):
                t = self.targets[k]
                p.set(t, x, 0.12 - k * 0.065, (key if t.get("kind", "e") == "e" else pkey) if k == 0 or t.get("kind") == "p"
                      else "")
            else:
                p.hide()

    def _pointers(self):
        g = self.game
        pts = self.ptr.current()
        touch = self.touch_mode
        # 離した指
        for pid in [p for p in self.own if p not in pts]:
            self._release(pid, self.own.pop(pid), pts)
        for pid, (x, y) in pts.items():
            o = self.own.get(pid)
            if o is None:
                o = self._grab(pid, x, y, touch)
                self.own[pid] = o
                continue
            kind = o[0]
            if kind == "btn":
                b = o[1]
                b.show_pressed(b.hit(x, y))
            elif kind == "stick":
                hx, hy = self.stick_home
                v = np.array([x - hx, y - hy]) / STICK_R
                n = float(np.linalg.norm(v))
                self.stick = v / n if n > 1 else v
            elif kind == "look":
                lx, ly = o[1]
                self.look += (x - lx, y - ly)
                o[1] = (x, y)
        if not any(o[0] == "stick" for o in self.own.values()):
            self.stick[:] = 0
        g.vkeys = self.vkeys

    def _grab(self, pid, x, y, touch):
        for b in self._all_btns():
            if b.hit(x, y):
                b.pressed = True
                b.show_pressed(True)
                if b.kind == "hold":
                    k = self.game.cfg["controls"].get(b.key, b.key)
                    self.vkeys[k] = 1
                    if b.press_key:
                        self.game.input(k)
                return ["btn", b]
        for p in self.pills:
            if p.hit(x, y):
                return ["pill", p, p.target]
        if touch and self._in_stick_zone(x, y):
            win = self.U["window"]
            hx = min(max(x, win.left.x + STICK_R + 0.03), win.left.x + 0.62 - STICK_R)
            hy = min(max(y, win.bottom.y + STICK_R + 0.03), 0.05 - STICK_R)
            self.stick_home = (hx, hy)
            return ["stick"]
        if touch:
            return ["look", (x, y)]
        return ["none"]

    def _release(self, pid, o, pts):
        if o[0] == "btn":
            b = o[1]
            b.show_pressed(False)
            b.pressed = False
            if b.kind == "hold":
                k = self.game.cfg["controls"].get(b.key, b.key)
                if not any(x[0] == "btn" and x[1] is not b and x[1].key == b.key for x in self.own.values()):
                    self.vkeys.pop(k, None)
            elif b.on_tap is not None:
                try:
                    b.on_tap()
                except Exception as e:  # noqa: BLE001
                    L.exception("ボタン %s の操作でエラー", b.id)
                    self.game.world.message(f"⚠ エラー: {e}")
        elif o[0] == "pill" and o[2] is not None:
            try:
                self.game.do_target(o[2])
            except Exception as e:  # noqa: BLE001
                L.exception("近くのものの操作でエラー")
                self.game.world.message(f"⚠ エラー: {e}")

    def _release_all(self):
        for o in self.own.values():
            if o[0] == "btn":
                o[1].show_pressed(False)
                o[1].pressed = False
        self.own.clear()
        self.vkeys.clear()
        self.stick[:] = 0
        self.look[:] = 0
        self.game.vkeys = self.vkeys

    def take_look(self):
        d = self.look.copy()
        self.look[:] = 0
        return d

    def _draw_stick(self):
        on = self.touch_mode
        self.stick_bg.enabled = self.stick_knob.enabled = on
        if not on:
            return
        win = self.U["window"]
        if not any(o[0] == "stick" for o in self.own.values()):
            self.stick_home = (win.left.x + 0.2, win.bottom.y + 0.2)
        hx, hy = self.stick_home
        self.stick_bg.position = (hx, hy, 0.01)
        self.stick_knob.position = (hx + self.stick[0] * STICK_R * 0.7, hy + self.stick[1] * STICK_R * 0.7, 0)

    def destroy(self):
        from .ui_destroy import destroy
        destroy(self.root)
