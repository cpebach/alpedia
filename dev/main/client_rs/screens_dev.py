"""Rust の画面：機器の設定（P）。今までの画面の device_ui.DevicePanel・actuator_ui.ActuatorPanel と同じ中身。"""
from .. import actuator as act
from ..client.actuator_ui import EXT_AMOUNTS, EXT_SPEEDS, ROT_AMOUNTS, ROT_SPEEDS, _nearest
from ..client.device_ui import NAME_LIMIT, TYPE_NAMES, number_same_named, same_named, update_programs
from . import ui as U

GREY = (0.75, 0.75, 0.75, 1)
BLUE = (0.22, 0.32, 0.45, 1)


class DevicePanel(U.Screen):
    """機器の名前などの設定。on_close：閉じたあとに呼ぶ（アクチュエーターの設定から開いたとき）。"""

    def __init__(self, ui, viewer, c, index, on_close=None):
        super().__init__(ui)
        self.v, self.w, self.c = viewer, viewer.w, c
        self.cell = c.cells[index]
        self.on_close = on_close
        d = c.devs[index]
        dd = c.reg.device(d["def"]) or {}
        self.dtype = dd.get("type")
        self.default_name = dd.get("name", d["def"])
        self.type_name = TYPE_NAMES.get(dd.get("type"), dd.get("type", "機器"))
        self.start_name = d["name"]

    @property
    def index(self):
        return self.c.index.get(self.cell)

    def build(self):
        self.clear()
        self.button("", 0, 0, 3.0, 2.0, lambda: None, color=(0, 0, 0, 0.55))
        self.panel(-0.55, 0.4, 0.55, -0.4)
        self.text(f"{self.type_name} の設定", 0, 0.37, 0.035, anchor=(0.5, 0))
        self.text("名前", -0.5, 0.265, 0.0225)
        self.name = self.field(-0.26, 0.25, 0.62, 0.05, self.start_name, NAME_LIMIT, self.ok)
        b = lambda t, x, y, fn, w=0.08, col=BLUE, size=0.0175: self.button(t, x, y, w, 0.042, fn, col, size)  # noqa: E731
        b("既定の名前", -0.27, 0.16, lambda: self.name.set(self.default_name), 0.17)
        b("1文字消す", -0.09, 0.16, lambda: self.name.set(self.name.value[:-1]), 0.17)
        b("全部消す", 0.09, 0.16, lambda: self.name.set(""), 0.17)
        b("同名に番号を振る", 0.29, 0.16, self.number_all, 0.2, (0.6, 0.45, 0.15, 1))
        self.info, _ = self.text("", -0.5, 0.09, 0.018, GREY)
        self.text("同じ名前の機器は、コンピューターからまとめて動きます。\n"
                  "名前を分けると、ブロックの機器選択や api.device(\"名前\") で個別に制御できます。\n"
                  "（名前はキーボードで入力します。日本語の入力もできます）", -0.5, -0.09, 0.0155, GREY)
        self.valve_text = None
        if self.dtype == "valve":
            self.valve_text, _ = self.text("", -0.5, -0.23, 0.02)
            for k, v in enumerate((0.0, 0.1, 0.25, 0.5, 0.75, 1.0)):
                b(f"{v:.0%}", -0.18 + k * 0.1, -0.245, (lambda v=v: self.set_open(v)), 0.085)
        elif self.dtype == "thruster":
            self.text("E で点火（電気を少し使う）。燃えている間は発電もする。\n"
                      "止める・出力を変えるには、タンクとのあいだにバルブを入れて、バルブで。", -0.5, -0.22, 0.0155,
                      (1, 0.85, 0.5, 1))
        b("OK", 0.2, -0.33, self.ok, 0.16, (0.2, 0.5, 0.3, 1), 0.02)
        b("キャンセル", 0.38, -0.33, self.close, 0.16, (0.3, 0.3, 0.33, 1), 0.02)
        self._key = None

    def number_all(self):
        i = self.index
        if i is None:
            return
        name = self.name.value.strip() or self.c.devs[i]["name"]
        self.c.devs[i]["name"] = name
        n = number_same_named(self.c, name)
        self.name.set(self.c.devs[i]["name"])
        self.w.message(f"「{name}」{n} 個に番号を付けました" if n else f"「{name}」という名前の機器は1つだけです")

    def set_open(self, v):
        i = self.index
        if i is None:
            return
        st = self.c.devs[i]["state"]
        st["open"] = float(v)
        if v > 0:
            st["set"] = float(v)

    def update(self, dt):
        i = self.index
        if i is None:
            self.info.set("機器が見つかりません", 0.018, GREY)
            return
        if self.valve_text is not None:
            o = float(self.c.devs[i]["state"].get("open", 1.0))
            self.valve_text.set(f"開き具合 {o:.0%}" + ("（閉）" if o <= 0 else ""), 0.02)
        name = self.name.value.strip()
        dup = [k for k in same_named(self.c, name) if k != i]
        lines = [f"構築物: {self.c.name}　種類: {self.type_name}（{self.c.devs[i]['def']}）"]
        if not name:
            lines.append("名前が空です")
        elif dup:
            lines.append(f"⚠ 同じ名前の機器がほかに {len(dup)} 個あります（まとめて動きます）")
        else:
            lines.append("この構築物の中で一つだけの名前です（個別に制御できます）")
        lines.append(f"コンピューターからは  api.device(\"{name}\")")
        self.info.set("\n".join(lines), 0.018, GREY)

    def ok(self):
        i = self.index
        name = self.name.value.strip()
        if i is not None and name:
            old = self.c.devs[i]["name"]
            self.c.devs[i]["name"] = name
            if old != name:
                n = 0
                if not same_named(self.c, old):
                    n = update_programs(self.w, self.c, old, name)
                self.w.message(f"名前を変更: {old} → {name}" + (f"（ブロックプログラム {n} 個も更新）" if n else ""))
        self.close()

    def on_key(self, key):
        if key == "escape":
            self.close()
        return True

    def close(self):
        super().close()
        if self.on_close is not None:
            self.on_close()
        else:
            self.v.screen_closed()


class ActuatorPanel(U.Screen):
    """アクチュエーター・ピストン・ヒンジの動作設定。"""

    def __init__(self, ui, viewer, c, index):
        super().__init__(ui)
        self.v, self.w, self.c = viewer, viewer.w, c
        self.cell = c.cells[index]
        d = c.devs[index]
        st = d["state"]
        dd = c.reg.device(d["def"]) or {"params": {}}
        act.init_state(st, dd["params"])
        p = dict(st.get("preset") or act.default_preset(d, c.reg))
        self.actions = act.actions_for(d, c.reg)
        if p.get("action") not in self.actions:
            p = act.default_preset(d, c.reg)
        self.hinge = act.is_hinge_dev(d, c.reg)
        self.preset = {"action": p.get("action", "rotate"), "amount": p.get("amount", 90.0), "speed": p.get("speed", 90.0)}
        self._fix_values()

    @property
    def index(self):
        return self.c.index.get(self.cell)

    def _fix_values(self):
        a = self.preset["action"]
        if a == "rotate" and self.hinge:
            self.preset["amount"] = _nearest(act.HINGE_AMOUNTS, self.preset["amount"], 90.0)
            self.preset["speed"] = _nearest(ROT_SPEEDS, self.preset["speed"], 90.0)
        elif a == "rotate":
            self.preset["amount"] = _nearest(ROT_AMOUNTS, self.preset["amount"], 90.0)
            self.preset["speed"] = _nearest(ROT_SPEEDS, self.preset["speed"], 90.0)
        elif a in ("extend", "retract"):
            self.preset["amount"] = _nearest(EXT_AMOUNTS, abs(float(self.preset["amount"] or 1.0)), 1.0)
            self.preset["speed"] = _nearest(EXT_SPEEDS, self.preset["speed"], 0.5)

    def build(self):
        self.clear()
        self.button("", 0, 0, 3.0, 2.0, lambda: None, color=(0, 0, 0, 0.5))
        self.panel(-0.5, 0.39, 0.5, -0.39, (0.06, 0.07, 0.1, 0.97))
        d = self.c.devs[self.index] if self.index is not None else None
        name = d["name"] if d else "アクチュエーター"
        self.text(f"{name} の動作設定", -0.1, 0.35, 0.032, anchor=(0.5, 0.5))
        self.button("名前を変更", 0.36, 0.35, 0.26, 0.055, self.open_rename, (0.6, 0.45, 0.15, 1), 0.02)
        self.status, _ = self.text("", -0.46, 0.27, 0.0188, GREY)
        y = 0.12
        a = self.preset["action"]
        names = dict(act.ACTION_NAMES, rotate="曲げる") if self.hinge else act.ACTION_NAMES
        self._row("動作", y, self.actions, a, lambda v: names[v], self._set_action)
        if a == "rotate":
            self._row("曲げる角度" if self.hinge else "回転量", y - 0.08, act.HINGE_AMOUNTS if self.hinge else ROT_AMOUNTS,
                      self.preset["amount"], lambda v: f"{v:+.0f}°", lambda v: self.preset.__setitem__("amount", v))
            self._row("速さ", y - 0.16, ROT_SPEEDS, self.preset["speed"], lambda v: f"{v:.0f}°/秒",
                      lambda v: self.preset.__setitem__("speed", v))
            if self.hinge:
                dev = self.c.devs[self.index]
                self.text(f"{act.mode_name(dev['state'], dev)}：ブロックの中心を軸に -90〜+90° 振ります。"
                          "E を押すたびにこの角度と 0° を行き来します。\n倒れる側は置き方で決まります（置くときに見ていた側が +）",
                          0, y - 0.23, 0.015, GREY, align=1, anchor=(0.5, 0.5))
        elif a in ("extend", "retract"):
            self._row("移動量", y - 0.08, EXT_AMOUNTS, self.preset["amount"], lambda v: f"{v:.2f} ブロック",
                      lambda v: self.preset.__setitem__("amount", v))
            self._row("移動速度", y - 0.16, EXT_SPEEDS, self.preset["speed"], lambda v: f"{v:.2f} ブロック/秒",
                      lambda v: self.preset.__setitem__("speed", v))
        else:
            msg = {"detach": "出力面の先の部分を切り離します",
                   "none": "E・操縦席のキー（X）では動かしません（コンピューターのプログラムで動かす物に）"}.get(
                a, "出力面の先にあるブロック（別の構築物でも可）とつなぎます")
            self.text(msg, 0, y - 0.11, 0.02, GREY, anchor=(0.5, 0.5))
        self.text("閉じた後、このアクチュエーターを見て E で実行します\n操縦席では X キーで実行／コンピューターからは rotate() extend() など",
                  0, -0.205, 0.0155, GREY, align=1, anchor=(0.5, 0.5))
        self.button("設定して閉じる", -0.3, -0.3, 0.26, 0.055, self.save_close, (0.2, 0.45, 0.3, 1), 0.02)
        self.button("今すぐ実行", 0.0, -0.3, 0.26, 0.055, self.run_now, (0.2, 0.5, 0.3, 1), 0.02)
        self.button("キャンセル", 0.3, -0.3, 0.26, 0.055, self.close, (0.3, 0.3, 0.33, 1), 0.02)

    def _row(self, label, y, values, current, fmt, on_change):
        idx = values.index(current) if current in values else 0
        st = {"i": idx}
        self.text(label, -0.4, y, 0.0225, anchor=(0, 0.5))
        val, _ = self.text(fmt(values[idx]), 0.17, y, 0.0225, anchor=(0.5, 0.5))

        def step(d):
            st["i"] = (st["i"] + d) % len(values)
            w, h = val.set(fmt(values[st["i"]]), 0.0225)
            val.place(0.17 - w / 2, y + h / 2)
            on_change(values[st["i"]])
        self.button("◀", -0.02, y, 0.05, 0.05, lambda: step(-1), BLUE)
        self.button("▶", 0.36, y, 0.05, 0.05, lambda: step(1), BLUE)

    def _set_action(self, v):
        self.preset["action"] = v
        if v == "rotate":
            self.preset["amount"], self.preset["speed"] = 90.0, 90.0
        elif v in ("extend", "retract"):
            self.preset["amount"], self.preset["speed"] = 1.0, 0.5
        self.build()

    def _store(self):
        i = self.index
        if i is None:
            return False
        self.c.devs[i]["state"]["preset"] = dict(self.preset)
        return True

    def save_close(self):
        if self._store():
            self.w.message(f"動作を設定: {act.preset_text(self.c.devs[self.index]['state'], self.hinge)}（E で実行）")
        self.close()

    def run_now(self):
        if self._store():
            net = getattr(self.w, "net", None)
            if net is not None and net.role == "client":
                net.actuator_run(self.c, self.index)
            else:
                self.w.message(act.execute_preset(self.w, self.c, self.index))
        self.close()

    def open_rename(self):
        if self.index is not None:
            self.ui.open(DevicePanel, self.v, self.c, self.index, on_close=self.build)

    def on_key(self, key):
        if key == "escape":
            self.save_close()
        elif key == "enter":
            self.run_now()
        return True

    def update(self, dt):
        i = self.index
        if i is None:
            self.status.set("アクチュエーターが失われました", 0.0188, GREY)
            return
        st = self.c.devs[i]["state"]
        s = self.c.status or {}
        att = "あり" if act.is_attached(self.c, i) else "なし"
        moving = "動作中" if act.busy(st) else "停止"
        m = act.motion(self.c.devs[i], self.c.reg)
        val = {"piston": f"伸び {st.get('ext', 0):.2f} ブロック   ",
               "both": f"角度 {st.get('angle', 0):.0f}°   伸び {st.get('ext', 0):.2f} ブロック   "}.get(
            m, f"角度 {st.get('angle', 0):.0f}°   ")
        self.status.set(val + f"出力面の結合 {att}   {moving}\n電力 {s.get('stored', 0) / 1e6:.2f} MJ（動かすには電力が必要）",
                        0.0188, GREY)

    def close(self):
        super().close()
        self.v.screen_closed()
