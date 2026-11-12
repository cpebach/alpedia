"""Rust の画面の画面（一時停止メニュー・インベントリ）。並べ方・動きは今までの画面（ursina）と同じ。"""
from ..client import icons
from ..client.inventory import SLOTS, ensure_hotbar, item_details
from . import ui as U

GREY = (0.75, 0.75, 0.75, 1)


class Pause(U.Screen):
    """Esc：一時停止メニュー。"""

    def __init__(self, ui, viewer):
        super().__init__(ui)
        self.v = viewer

    def build(self):
        self.clear()
        self.button("", 0, 0, 3.0, 2.0, None, color=(0, 0, 0, 0.45))     # 後ろを暗く（押しても何もしない）
        net = self.v.w.net
        client = net is not None and net.role == "client"
        g = self.v.game
        if client:          # マルチプレイに参加中（保存はホストがする）
            items = (("続ける（Esc）", self.close), ("ワールドから抜けてメニューへ", lambda: g._leave("menu")),
                     ("ワールドから抜けて終了", lambda: g._leave("quit")))
        else:
            items = (("続ける（Esc）", self.close), ("保存する（F5）", self.save), ("保存してメニューへ", lambda: g._leave("menu")),
                     ("保存せずにメニューへ", lambda: g._leave("menu", save=False)), ("保存して終了", lambda: g._leave("quit")))
            if net is None:
                items += (("LAN に公開（マルチプレイ）", self.host),)
        top = 0.08 + len(items) * 0.045
        self.panel(-0.27, top + 0.1, 0.27, top - len(items) * 0.09 - 0.02 - (0.06 if net is not None and not client else 0))
        self.text("一時停止", 0, top + 0.06, 0.034, anchor=(0.5, 0))
        for k, (label, fn) in enumerate(items):
            self.button(label, 0, top - 0.04 - k * 0.09, 0.44, 0.065, fn,
                        color=(0.2, 0.45, 0.3, 1) if k == 0 else (0.22, 0.25, 0.32, 1), size=0.024)
        if net is not None and not client:
            self.text("LAN に公開中: " + "  ".join(net.addresses()[:2]), 0, top - 0.04 - len(items) * 0.09 + 0.01,
                      0.02, anchor=(0.5, 0))

    def host(self):
        self.v.start_hosting()
        self.build()

    def save(self):
        self.v.w.save()
        self.v.w.message("保存しました")

    def quit(self):
        self.v.game._leave("menu")

    def on_key(self, key):
        if key == "escape":
            self.close()
            return True
        return False

    def close(self):
        super().close()
        self.v.screen_closed()


class Inventory(U.Screen):
    """I：インベントリ（アイテムバーに入れる物を選ぶ）。今までの画面の InventoryPanel と同じ並べ方。"""
    COLS = 9
    ROWS = 4
    CW = 0.118
    CH = 0.118

    def __init__(self, ui, viewer):
        super().__init__(ui)
        self.v = viewer
        self.w = viewer.w
        self.reg = self.w.registry
        self.player = self.w.player
        self.tab = "all"
        self.page = 0
        self.held = None                  # (アイテム, 元の枠 or None)
        self.cells = []
        self.slots = []
        self.info = None
        self.held_img = None

    def items(self):
        pal = [it for it in self.reg.palette() if self.w.item_allowed(it)]
        if self.tab == "material":
            pal = [p for p in pal if p[0] == "material"]
        elif self.tab == "device":
            pal = [p for p in pal if p[0] == "device"]
        return pal

    def build(self):
        self.clear()
        self.button("", 0, 0, 3.0, 2.0, None, color=(0, 0, 0, 0.55), on_right=self.drop_held)
        self.widgets[-1]["click"] = self.drop_held
        self.panel(-0.66, 0.46, 0.66, -0.46)
        self.text("インベントリ", -0.63, 0.44, 0.032)
        for k, (tid, name) in enumerate((("all", "すべて"), ("material", "素材"), ("device", "機器"))):
            self.button(name, -0.2 + k * 0.13, 0.405, 0.12, 0.045, (lambda t=tid: self.set_tab(t)),
                        color=(0.3, 0.45, 0.65, 1) if tid == self.tab else (0.18, 0.2, 0.25, 1), size=0.019)
        self.text("クリックで持ち上げ → 下のアイテムバーの枠をクリックで入れる / マウスを合わせて 1〜9 キーでも入れられます",
                  -0.63, 0.37, 0.0155, GREY)
        items = self.items()
        per = self.COLS * self.ROWS
        pages = max(1, (len(items) + per - 1) // per)
        self.page = min(self.page, pages - 1)
        shown = items[self.page * per:(self.page + 1) * per]
        x0 = -(self.COLS - 1) / 2 * self.CW
        y0 = 0.26
        self.cells = []
        surv = not self.w.creative
        inv = self.player.inventory
        for k, it in enumerate(shown):
            x = x0 + (k % self.COLS) * self.CW
            y = y0 - (k // self.COLS) * self.CH
            self.button("", x, y, self.CW - 0.01, self.CH - 0.01, (lambda i=it: self.pick(i)), color=(0.13, 0.15, 0.2, 1),
                        info=it)
            cost = self.w.item_cost(it) if surv else {}
            ok = all(inv.get(r, 0.0) >= a for r, a in cost.items())
            self.image(U.item_tex(self.reg, it), x, y + (0.02 if surv else 0.012), 0.064 if surv else 0.075,
                       1.0 if ok else 0.35)
            name = icons.item_name(self.reg, it)
            self.text(name if len(name) <= 7 else name[:6] + "…", x, y - (0.026 if surv else 0.045), 0.0138, anchor=(0.5, 0.5))
            if surv and cost:
                txt = " ".join(f"{self.reg.res_name(r)}{a:g}" for r, a in cost.items())
                self.text(txt if len(txt) <= 12 else txt[:11] + "…", x, y - 0.046, 0.0113,
                          (0.75, 0.85, 0.75, 1) if ok else (1.0, 0.45, 0.4, 1), anchor=(0.5, 0.5))
            self.cells.append((self.widgets[-1], it))
        if pages > 1:
            self.button("◀", 0.42, 0.405, 0.05, 0.045, lambda: self.set_page(-1), color=(0.22, 0.32, 0.45, 1))
            self.text(f"{self.page + 1}/{pages}", 0.49, 0.405, 0.019, anchor=(0.5, 0.5))
            self.button("▶", 0.56, 0.405, 0.05, 0.045, lambda: self.set_page(1), color=(0.22, 0.32, 0.45, 1))
        self.text("アイテムバー（クリックで持ち上げ・入れ替え / 右クリックで外す）", -0.63, -0.278, 0.0155, GREY)
        sw = 0.085
        hb = self.player.hotbar
        self.slots = []
        for k in range(SLOTS):
            x = (k - (SLOTS - 1) / 2) * (sw + 0.01)
            y = -0.36
            sel = k == self.v.sel
            self.button("", x, y, sw, sw, (lambda kk=k: self.click_slot(kk)),
                        color=(0.55, 0.45, 0.15, 1) if sel else (0.16, 0.18, 0.23, 1), on_right=(lambda kk=k: self.unset(kk)))
            self.slots.append(self.widgets[-1])
            if hb[k]:
                self.image(U.item_tex(self.reg, hb[k]), x, y, sw * 0.85)
            self.text(str(k + 1), x - sw / 2 + 0.006, y + sw / 2 - 0.004, 0.0138)
        for k, (label, fn) in enumerate((("初期に戻す", self.reset), ("全部外す", self.clear_all), ("閉じる（Esc）", self.close))):
            self.button(label, -0.45 + k * 0.45, -0.43, 0.2 if k < 2 else 0.24, 0.042, fn,
                        color=(0.3, 0.3, 0.33, 1) if k < 2 else (0.2, 0.45, 0.3, 1), size=0.0175)
        self.info, _ = self.text("", -0.62, -0.165, 0.0175, GREY)
        self.held_img = None
        self._info_key = None

    # ---- 操作（InventoryPanel と同じ） ----
    def set_tab(self, t):
        self.tab, self.page = t, 0
        self.build()

    def set_page(self, d):
        self.page = max(0, self.page + d)
        self.build()

    def pick(self, item):
        self.return_held()
        self.held = (item, None)
        self._show_held()

    def click_slot(self, k):
        hb = self.player.hotbar
        if self.held is None:
            if hb[k]:
                self.held = (hb[k], k)
                hb[k] = None
            else:
                self.v.sel = k
            self.build()
            self._show_held()
            return
        item, origin = self.held
        prev = hb[k]
        hb[k] = item
        if origin is not None and prev is not None:
            hb[origin] = prev
        self.held = None
        self.v.sel = k
        self.build()

    def unset(self, k):
        self.player.hotbar[k] = None
        self.build()

    def set_slot(self, k, item):
        hb = self.player.hotbar
        if item in hb:
            j = hb.index(item)
            hb[j] = hb[k]
        hb[k] = item
        self.v.sel = k
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
        h = self.hover
        if h is None:
            return None
        if h.get("info") is not None:
            return h["info"]
        if h in self.slots:
            return self.player.hotbar[self.slots.index(h)]
        return None

    def on_key(self, key):
        if key == "escape" or key == "i":
            self.close()
        elif len(key) == 1 and key in "123456789":
            it = self.hovered_item() if (self.hover is not None and self.hover.get("info") is not None) else None
            if it is None and self.held:
                it = self.held[0]
                self.held = None
            if it is not None:
                self.set_slot(int(key) - 1, it)
        return True

    def _show_held(self):
        if self.held_img is not None:
            self.held_img.remove()
            self.held_img = None
        if self.held:
            self.held_img = self.image(U.item_tex(self.reg, self.held[0]), 0, 0, 0.075)
            from ..client import gfx
            gfx.VIEW.mesh_props(self.held_img.mid, True, False, True, 0, self._bin(8))

    def update(self, dt):
        if self.held_img is not None:
            mx, my = self.ui.mouse
            self.held_img.np.setPos(mx, my, 0)
        it = self.hovered_item()
        if it is None and self.held:
            it = self.held[0]
        key = (it, tuple(sorted(self.player.inventory.items())) if not self.w.creative else None)
        if key == self._info_key:
            return
        self._info_key = key
        lines = item_details(self.reg, it) if it else ["アイテムにマウスを合わせると説明が出ます"]
        if it and not self.w.creative:
            cost = self.w.item_cost(it)
            if cost:
                lines.append("費用: " + self.reg.res_text(cost))
        if not self.w.creative:
            lines.append("所持資源: " + (self.reg.res_text(self.player.inventory) or "なし"))
        w, h = self.info.set("\n".join(lines), 0.0175, GREY)

    def close(self):
        self.return_held()
        super().close()
        self.v.screen_closed()
