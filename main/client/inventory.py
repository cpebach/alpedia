"""アイテムバー（画面下の9枠）とインベントリ画面（I キー）。

インベントリにはすべての素材・機器（MOD で追加されたものも）がアイコン付きで並ぶ。
アイテムをクリックで持ち上げ、下のアイテムバーの枠をクリックして入れる。
アイテムバーの枠をクリックするとそのアイテムを持ち上げて、別の枠へ移動・入れ替えできる。
アイテムにマウスを合わせて 1〜9 キーを押すと、その番号の枠に直接入れられる。Esc で閉じる。
"""
from .. import log
from . import icons

L = log.get("inventory")

SLOTS = 9
DEFAULT_HOTBAR = [("material", "steel"), ("material", "aluminum"), ("material", "wood"), ("material", "glass"),
                  ("material", "rock"), ("device", "thruster"), ("device", "tank"), ("device", "pipe"),
                  ("device", "computer")]
TYPE_NAMES = {
    "thruster": "推進機", "tank": "タンク", "pipe": "配管", "valve": "バルブ", "battery": "バッテリー",
    "generator": "発電機", "solar": "太陽光パネル", "crank": "手回し発電機", "computer": "コンピューター", "actuator": "アクチュエーター",
    "processor": "精錬機・加工機", "seat": "操縦席", "air_vent": "通気口", "antenna": "アンテナ",
}


def valid_item(reg, item):
    if not item:
        return False
    kind, id_ = item
    return bool(reg.material(id_) if kind == "material" else reg.device(id_))


def ensure_hotbar(player, reg):
    """アイテムバーを9枠にそろえ、無くなったアイテム（MOD を外した等）を空にする。"""
    hb = list(player.hotbar) if player.hotbar else None
    if hb is None:
        pal = reg.palette()
        hb = [it for it in DEFAULT_HOTBAR if valid_item(reg, it)]
        for it in pal:
            if len(hb) >= SLOTS:
                break
            if it not in hb:
                hb.append(it)
    hb = [tuple(x) if x and valid_item(reg, tuple(x)) else None for x in hb][:SLOTS]
    hb += [None] * (SLOTS - len(hb))
    player.hotbar = hb
    return hb


def item_details(reg, item):
    kind, id_ = item
    if kind == "material":
        m = reg.material(id_)
        lines = [f"素材: {m['name']}",
                 f"密度 {m['density']:.0f} kg/m³　硬さ {m['stiffness']:.2f}　ちぎれにくさ {m['break_distance']:.2f}"]
        extra = []
        if m.get("plasticity", 0) >= 0.5:
            extra.append("曲がったまま戻りにくい")
        if m.get("rebond"):
            extra.append("離れても近づくと再びくっつく")
        if m.get("density", 0) < 1000:
            extra.append("水に浮く")
        if extra:
            lines.append("・" + "・".join(extra))
    else:
        d = reg.device(id_)
        lines = [f"機器: {d['name']}（{TYPE_NAMES.get(d.get('mark') or d['type'], d['type'])}）"]
        pw = {"none": "電力なし", "manual": "手動", "electric": f"電力 {d.get('power_use', 0):.0f} W"}.get(
            d.get("power"), "")
        mat = (reg.material(d.get("material")) or {}).get("name", d.get("material"))
        info = [pw, f"素材 {mat}"]
        if d.get("directional"):
            info.append("向きあり（R で変更）")
        if d.get("pipe_port"):
            info.append("配管につながる")
        lines.append("　".join(x for x in info if x))
        p = d.get("params", {})
        if d["type"] == "generator":
            fr = float(p.get("fuel_rate", 0))
            lines.append(f"発電 {float(p.get('output', 0)) / 1000:.1f} kW" +
                         (f"（{reg.res_name(p.get('fuel', 'fuel'))} {fr:g} kg/秒）" if fr > 0 else "（燃料いらず）"))
        if d["type"] == "processor":
            for r in p.get("recipes", []):
                pw = float(r.get("power", d.get("power_use", 0))) / 1000
                lines.append(f"・{reg.res_text(r.get('in', {}))} → {reg.res_text(r.get('out', {}))}"
                             f"（{float(r.get('time', 1)):g}秒・{pw:g} kW）")
        if d.get("_source") and d.get("_source") != "base":
            lines.append(f"MOD: {d['_source']}")
    return lines


class HotbarView:
    """画面下のアイテムバー（表示のみ）。"""
    SW = 0.07
    GAP = 0.008

    def __init__(self, parent, game, y=-0.448):
        from ursina import Entity, Text, color
        self.game = game
        self.color = color
        self.root = Entity(parent=parent)
        self.slots = []
        for k in range(SLOTS):
            x = (k - (SLOTS - 1) / 2) * (self.SW + self.GAP)
            bg = Entity(parent=self.root, model="quad", scale=self.SW, position=(x, y, 0.01),
                        color=color.rgba(0, 0, 0, 0.5))
            icon = Entity(parent=self.root, model="quad", scale=self.SW * 0.88, position=(x, y, 0), enabled=False)
            Text(parent=self.root, text=str(k + 1), position=(x - self.SW / 2 + 0.005, y + self.SW / 2 - 0.003),
                 origin=(-0.5, 0.5), scale=0.55, color=color.rgba(1, 1, 1, 0.8))
            self.slots.append((bg, icon))
        self._state = None

    def refresh(self, force=False):
        g = self.game
        hb = g.world.player.hotbar
        state = (tuple(hb), g.sel)
        if state == self._state and not force:
            return
        self._state = state
        reg = g.world.registry
        for k, (bg, icon) in enumerate(self.slots):
            it = hb[k]
            if it:
                icon.texture = icons.item_texture(reg, it)
                icon.enabled = True
            else:
                icon.enabled = False
            bg.color = self.color.rgba(1.0, 0.82, 0.25, 0.9) if k == g.sel else self.color.rgba(0, 0, 0, 0.5)
            bg.scale = self.SW * (1.12 if k == g.sel else 1.0)


class InventoryPanel:
    COLS = 9
    ROWS = 4
    CW = 0.118
    CH = 0.118

    def __init__(self, game):
        from ursina import Entity, Text, Button, camera, color, mouse
        self.game = game
        self.world = game.world
        self.reg = game.world.registry
        self.player = game.world.player
        self.U = dict(Entity=Entity, Text=Text, Button=Button, color=color, mouse=mouse)
        mouse.locked = False
        self.tab = "all"
        self.page = 0
        self.held = None          # (item, 元の枠 or None)
        self.root = Entity(parent=camera.ui, z=-0.3)
        bg = Button(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0, 0, 0, 0.55),
                    highlight_color=color.rgba(0, 0, 0, 0.55), pressed_color=color.rgba(0, 0, 0, 0.55), z=0.1)
        bg.on_click = self.drop_held
        Entity(parent=self.root, model="quad", scale=(1.32, 0.92), position=(0, 0.0, 0.05),
               color=color.rgb(0.07, 0.08, 0.11))
        Text(parent=self.root, text="インベントリ", position=(-0.63, 0.425), origin=(-0.5, 0.5), scale=1.3)
        self.body = Entity(parent=self.root)
        self.info = Text(parent=self.root, text="", position=(-0.62, -0.165), origin=(-0.5, 0.5), scale=0.7,
                         color=color.light_gray)
        self.held_icon = Entity(parent=self.root, model="quad", scale=0.075, z=-0.2, enabled=False)
        game_ = self

        class Keys(Entity):
            def input(self_, key):
                game_.input(key)

            def update(self_):
                game_.update()
        self.keys = Keys(parent=self.root)
        try:
            self.build()
        except Exception:
            from .ui_destroy import destroy
            destroy(self.root)
            raise

    # ------------------------------------------------------------------
    def items(self):
        pal = self.reg.palette()
        if self.tab == "material":
            pal = [p for p in pal if p[0] == "material"]
        elif self.tab == "device":
            pal = [p for p in pal if p[0] == "device"]
        return pal

    def build(self):
        from .ui_destroy import destroy
        for e in list(self.body.children):
            destroy(e)
        U = self.U
        col = U["color"]
        B, T, E = U["Button"], U["Text"], U["Entity"]
        # タブ
        for k, (tid, name) in enumerate((("all", "すべて"), ("material", "素材"), ("device", "機器"))):
            b = B(parent=self.body, text=name, position=(-0.2 + k * 0.13, 0.405), scale=(0.12, 0.045),
                  color=col.rgb(0.3, 0.45, 0.65) if tid == self.tab else col.rgb(0.18, 0.2, 0.25), text_size=0.75)
            b.on_click = (lambda t=tid: self.set_tab(t))
        T(parent=self.body, text="クリックで持ち上げ → 下のアイテムバーの枠をクリックで入れる / マウスを合わせて 1〜9 キーでも入れられます",
          position=(-0.63, 0.355), origin=(-0.5, 0.5), scale=0.62, color=col.light_gray)
        # アイテムの格子
        items = self.items()
        per = self.COLS * self.ROWS
        pages = max(1, (len(items) + per - 1) // per)
        self.page = min(self.page, pages - 1)
        shown = items[self.page * per:(self.page + 1) * per]
        x0 = -(self.COLS - 1) / 2 * self.CW
        y0 = 0.26
        self.cells = []
        for k, it in enumerate(shown):
            x = x0 + (k % self.COLS) * self.CW
            y = y0 - (k // self.COLS) * self.CH
            b = B(parent=self.body, model="quad", position=(x, y), scale=(self.CW - 0.01, self.CH - 0.01),
                  color=col.rgb(0.13, 0.15, 0.2), highlight_color=col.rgb(0.25, 0.3, 0.4),
                  pressed_color=col.rgb(0.35, 0.4, 0.5))
            b.on_click = (lambda i=it: self.pick(i))
            E(parent=self.body, model="quad", texture=icons.item_texture(self.reg, it), position=(x, y + 0.012, -0.01),
              scale=0.075)
            name = icons.item_name(self.reg, it)
            T(parent=self.body, text=name if len(name) <= 7 else name[:6] + "…", position=(x, y - 0.045, -0.01),
              origin=(0, 0), scale=0.55)
            self.cells.append((b, it))
        if pages > 1:
            B(parent=self.body, text="◀", position=(0.42, 0.405), scale=(0.05, 0.045),
              color=col.rgb(0.22, 0.32, 0.45)).on_click = lambda: self.set_page(-1)
            T(parent=self.body, text=f"{self.page + 1}/{pages}", position=(0.49, 0.405), origin=(0, 0), scale=0.75)
            B(parent=self.body, text="▶", position=(0.56, 0.405), scale=(0.05, 0.045),
              color=col.rgb(0.22, 0.32, 0.45)).on_click = lambda: self.set_page(1)
        # アイテムバー
        T(parent=self.body, text="アイテムバー（クリックで持ち上げ・入れ替え / 右クリックで外す）",
          position=(-0.63, -0.278), origin=(-0.5, 0.5), scale=0.62, color=col.light_gray)
        self.slot_buttons = []
        sw = 0.085
        hb = self.player.hotbar
        for k in range(SLOTS):
            x = (k - (SLOTS - 1) / 2) * (sw + 0.01)
            y = -0.36
            sel = k == self.game.sel
            b = B(parent=self.body, model="quad", position=(x, y), scale=sw,
                  color=col.rgb(0.55, 0.45, 0.15) if sel else col.rgb(0.16, 0.18, 0.23),
                  highlight_color=col.rgb(0.3, 0.38, 0.5))
            b.on_click = (lambda kk=k: self.click_slot(kk))
            if hb[k]:
                E(parent=self.body, model="quad", texture=icons.item_texture(self.reg, hb[k]), position=(x, y, -0.01),
                  scale=sw * 0.85)
            T(parent=self.body, text=str(k + 1), position=(x - sw / 2 + 0.006, y + sw / 2 - 0.004, -0.02),
              origin=(-0.5, 0.5), scale=0.55)
            self.slot_buttons.append(b)
        # ボタン
        for k, (text, fn) in enumerate((("初期に戻す", self.reset), ("全部外す", self.clear_all),
                                        ("閉じる（Esc）", self.close))):
            b = B(parent=self.body, text=text, position=(-0.45 + k * 0.45, -0.43),
                  scale=(0.2 if k < 2 else 0.24, 0.042), text_size=0.7,
                  color=col.rgb(0.3, 0.3, 0.33) if k < 2 else col.rgb(0.2, 0.45, 0.3))
            b.on_click = fn

    # ------------------------------------------------------------------
    def set_tab(self, t):
        self.tab = t
        self.page = 0
        self.build()

    def set_page(self, d):
        self.page += d
        if self.page < 0:
            self.page = 0
        self.build()

    def pick(self, item):
        self.return_held()
        self.held = (item, None)

    def click_slot(self, k):
        hb = self.player.hotbar
        if self.held is None:
            if hb[k]:
                self.held = (hb[k], k)
                hb[k] = None
                self.build()
            else:
                self.game.sel = k
                self.build()
            return
        item, origin = self.held
        prev = hb[k]
        hb[k] = item
        if origin is not None and prev is not None:
            hb[origin] = prev      # 入れ替え
        self.held = None
        self.game.sel = k
        self.build()

    def set_slot(self, k, item):
        hb = self.player.hotbar
        # 同じアイテムが他の枠にあれば入れ替える
        if item in hb:
            j = hb.index(item)
            hb[j] = hb[k]
        hb[k] = item
        self.game.sel = k
        self.build()

    def return_held(self):
        if self.held and self.held[1] is not None:
            hb = self.player.hotbar
            if hb[self.held[1]] is None:
                hb[self.held[1]] = self.held[0]
        self.held = None

    def drop_held(self):
        if self.held:
            self.return_held()
            self.build()

    def reset(self):
        self.held = None
        self.player.hotbar = None
        ensure_hotbar(self.player, self.reg)
        self.build()

    def clear_all(self):
        self.held = None
        self.player.hotbar = [None] * SLOTS
        self.build()

    def hovered_item(self):
        for b, it in getattr(self, "cells", []):
            if b.hovered:
                return it
        return None

    def hovered_slot(self):
        for k, b in enumerate(getattr(self, "slot_buttons", [])):
            if b.hovered:
                return k
        return None

    # ------------------------------------------------------------------
    def input(self, key):
        if key == "escape":       # 閉じるのは Esc（機器の設定画面と同じ）
            self.close()
        elif len(key) == 1 and key in "123456789":
            it = self.hovered_item()
            if it is None and self.held:
                it = self.held[0]
                self.held = None
            if it is not None:
                self.set_slot(int(key) - 1, it)
        elif key == "right mouse down":
            k = self.hovered_slot()
            if k is not None:
                self.player.hotbar[k] = None
                self.build()
            elif self.held:
                self.drop_held()

    def update(self):
        m = self.U["mouse"]
        if self.held:
            self.held_icon.texture = icons.item_texture(self.reg, self.held[0])
            self.held_icon.enabled = True
            self.held_icon.position = (m.x, m.y, -0.2)
        else:
            self.held_icon.enabled = False
        it = self.hovered_item()
        if it is None:
            k = self.hovered_slot()
            if k is not None:
                it = self.player.hotbar[k]
        if it is None and self.held:
            it = self.held[0]
        lines = item_details(self.reg, it) if it else ["アイテムにマウスを合わせると説明が出ます"]
        if it and not self.world.settings.get("creative", True):
            cost = self.world.item_cost(it)
            if cost:
                lines.append("費用: " + self.reg.res_text(cost))
        if not self.world.settings.get("creative", True):
            inv = self.player.inventory
            lines.append("所持資源: " + (self.reg.res_text(inv) or "なし"))
        self.info.text = "\n".join(lines).replace("<", "＜").replace(">", "＞")

    def close(self):
        from .ui_destroy import destroy
        L.info("インベントリを閉じます")
        self.return_held()
        destroy(self.root)
        self.game.editor_closed()
        self.game.hotbar_view.refresh(force=True)
