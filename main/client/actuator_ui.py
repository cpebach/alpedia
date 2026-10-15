"""アクチュエーターの動作設定メニュー（P キー）。設定した動作は E キーで実行する。"""
from .. import actuator as act

ROT_AMOUNTS = [-360.0, -180.0, -90.0, -45.0, -30.0, -15.0, -5.0, 5.0, 15.0, 30.0, 45.0, 90.0, 180.0, 360.0]
ROT_SPEEDS = [5.0, 15.0, 30.0, 45.0, 90.0, 180.0, 360.0]
EXT_AMOUNTS = [0.1, 0.25, 0.5, 0.75, 1.0]
EXT_SPEEDS = [0.05, 0.1, 0.25, 0.5, 1.0, 2.0]


def _nearest(values, v, default):
    try:
        v = float(v)
    except (TypeError, ValueError):
        return default
    return min(values, key=lambda x: abs(x - v))


class ActuatorPanel:
    def __init__(self, game, construct, index):
        from ursina import Entity, Text, Button, camera, color, mouse
        self.game = game
        self.world = game.world
        self.c = construct
        self.cell = construct.cells[index]
        self.U = dict(Entity=Entity, Text=Text, Button=Button, color=color)
        mouse.locked = False
        st = construct.devs[index]["state"]
        dd = construct.reg.device(construct.devs[index]["def"]) or {"params": {}}
        act.init_state(st, dd["params"])
        p = dict(st.get("preset") or act.DEFAULT_PRESET)
        self.preset = {"action": p.get("action", "rotate"), "amount": p.get("amount", 90.0),
                       "speed": p.get("speed", 90.0)}
        self._fix_values()
        self.root = Entity(parent=camera.ui)
        Entity(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0, 0, 0, 0.5), z=0.3)
        Entity(parent=self.root, model="quad", scale=(1.0, 0.78), color=color.rgb(0.06, 0.07, 0.1), z=0.2)
        self.body = Entity(parent=self.root)
        self.status = Text(parent=self.root, text="", position=(-0.46, 0.26), origin=(-0.5, 0.5), scale=0.75,
                           color=color.light_gray)

        class Keys(Entity):
            def input(self_, key):
                self.input(key)

            def update(self_):
                self.update()
        self.keys = Keys(parent=self.root)
        self.build()

    # ------------------------------------------------------------------
    @property
    def index(self):
        return self.c.index.get(self.cell)

    def _fix_values(self):
        a = self.preset["action"]
        if a == "rotate":
            self.preset["amount"] = _nearest(ROT_AMOUNTS, self.preset["amount"], 90.0)
            self.preset["speed"] = _nearest(ROT_SPEEDS, self.preset["speed"], 90.0)
        elif a in ("extend", "retract"):
            self.preset["amount"] = _nearest(EXT_AMOUNTS, abs(float(self.preset["amount"] or 1.0)), 1.0)
            self.preset["speed"] = _nearest(EXT_SPEEDS, self.preset["speed"], 0.5)

    def build(self):
        from .ui_destroy import destroy
        for e in list(self.body.children):
            destroy(e)
        U = self.U
        col = U["color"]
        d = self.c.devs[self.index] if self.index is not None else None
        name = d["name"] if d else "アクチュエーター"
        U["Text"](parent=self.body, text=f"{name} の動作設定", position=(-0.1, 0.35), origin=(0, 0), scale=1.3)
        self._button("名前を変更", (0.36, 0.35), self.open_rename, col.rgb(0.6, 0.45, 0.15))
        y = 0.12
        a = self.preset["action"]
        self._row("動作", y, act.ACTIONS, a, lambda v: act.ACTION_NAMES[v], self._set_action)
        if a == "rotate":
            self._row("回転量", y - 0.08, ROT_AMOUNTS, self.preset["amount"], lambda v: f"{v:+.0f}°",
                      lambda v: self.preset.__setitem__("amount", v))
            self._row("回転速度", y - 0.16, ROT_SPEEDS, self.preset["speed"], lambda v: f"{v:.0f}°/秒",
                      lambda v: self.preset.__setitem__("speed", v))
        elif a in ("extend", "retract"):
            self._row("移動量", y - 0.08, EXT_AMOUNTS, self.preset["amount"], lambda v: f"{v:.2f} ブロック",
                      lambda v: self.preset.__setitem__("amount", v))
            self._row("移動速度", y - 0.16, EXT_SPEEDS, self.preset["speed"], lambda v: f"{v:.2f} ブロック/秒",
                      lambda v: self.preset.__setitem__("speed", v))
        else:
            msg = ("出力面の先の部分を切り離します" if a == "detach"
                   else "出力面の先にあるブロック（別の構築物でも可）とつなぎます")
            U["Text"](parent=self.body, text=msg, position=(0, y - 0.11), origin=(0, 0), scale=0.8,
                      color=col.light_gray)
        U["Text"](parent=self.body, position=(0, -0.13), origin=(0, 0), scale=0.7, color=col.light_gray,
                  text="閉じた後、このアクチュエーターを見て E で実行します\n"
                       "操縦席では X キーで実行／コンピューターからは rotate() extend() など")
        self._button("設定して閉じる", (-0.3, -0.27), self.save_close, col.rgb(0.2, 0.45, 0.3))
        self._button("今すぐ実行", (0.0, -0.27), self.run_now, col.rgb(0.2, 0.5, 0.3))
        self._button("キャンセル", (0.3, -0.27), self.cancel, col.rgb(0.3, 0.3, 0.33))

    def _button(self, text, pos, fn, c):
        b = self.U["Button"](parent=self.body, text=text, position=pos, scale=(0.26, 0.055), color=c,
                             text_size=0.8)
        b.on_click = fn
        return b

    def _row(self, label, y, values, current, fmt, on_change):
        U = self.U
        idx = values.index(current) if current in values else 0
        state = {"i": idx}
        U["Text"](parent=self.body, text=label, position=(-0.4, y), origin=(-0.5, 0), scale=0.9)
        val = U["Text"](parent=self.body, text=fmt(values[idx]), position=(0.17, y), origin=(0, 0), scale=0.9)

        def step(dlt):
            state["i"] = (state["i"] + dlt) % len(values)
            val.text = fmt(values[state["i"]])
            on_change(values[state["i"]])
        b1 = U["Button"](parent=self.body, text="◀", position=(-0.02, y), scale=(0.05, 0.05),
                         color=U["color"].rgb(0.22, 0.32, 0.45))
        b1.on_click = lambda: step(-1)
        b2 = U["Button"](parent=self.body, text="▶", position=(0.36, y), scale=(0.05, 0.05),
                         color=U["color"].rgb(0.22, 0.32, 0.45))
        b2.on_click = lambda: step(1)

    def _set_action(self, v):
        self.preset["action"] = v
        if v == "rotate":
            self.preset["amount"], self.preset["speed"] = 90.0, 90.0
        elif v in ("extend", "retract"):
            self.preset["amount"], self.preset["speed"] = 1.0, 0.5
        self.build()

    # ------------------------------------------------------------------
    def _store(self):
        i = self.index
        if i is None:
            return False
        self.c.devs[i]["state"]["preset"] = dict(self.preset)
        return True

    def save_close(self):
        if self._store():
            self.world.message(f"動作を設定: {act.preset_text(self.c.devs[self.index]['state'])}（E で実行）")
        self.close()

    def run_now(self):
        if self._store():
            net = getattr(self.world, "net", None)
            if net is not None and net.role == "client":
                net.actuator_run(self.c, self.index)        # マルチプレイの参加者：ホストで実行する
            else:
                self.world.message(act.execute_preset(self.world, self.c, self.index))
        self.close()

    def cancel(self):
        self.close()

    def close(self):
        from .ui_destroy import destroy
        destroy(self.root)
        self.game.editor_closed()

    def open_rename(self):
        if self.index is None:
            return
        from .device_ui import DevicePanel
        DevicePanel(self.game, self.c, self.index, owner=self)

    def after_modal(self):
        self.build()

    def input(self, key):
        if getattr(self, "modal", None):
            return
        if key == "escape":
            self.save_close()
        elif key == "enter":
            self.run_now()

    def update(self):
        i = self.index
        if i is None:
            self.status.text = "アクチュエーターが失われました"
            return
        st = self.c.devs[i]["state"]
        s = self.c.status or {}
        att = "あり" if act.is_attached(self.c, i) else "なし"
        moving = "動作中" if act.busy(st) else "停止"
        self.status.text = (f"角度 {st.get('angle', 0):.0f}°   伸び {st.get('ext', 0):.2f} ブロック   "
                            f"出力面の結合 {att}   {moving}\n"
                            f"電力 {s.get('stored', 0) / 1e6:.2f} MJ（動かすには電力が必要）")
