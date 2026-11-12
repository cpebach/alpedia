"""Rust の画面の UI の部品：板・文字・ボタン・画像。座標は画面の上（縦の高さが 1、真ん中が原点、x 右・y 上）。

画面（Screen）は部品を持ち、マウスの位置・クリック・キーを受け取る。部品は作り直す（build）のが基本
（今までの画面の ursina の作り方と同じ考え方）。
"""
import numpy as np

from ..client import gfx

_icon_tex = {}


def item_tex(reg, item):
    """アイテムのアイコン（今までの画面と同じ絵）のテクスチャ。"""
    key = tuple(item)
    if key not in _icon_tex:
        from ..client import icons
        img = icons.item_image(reg, item).convert("RGBA")
        _icon_tex[key] = gfx.tex_rgba(img.width, img.height, img.tobytes(), f"icon_{key}")
    return _icon_tex[key]


def rect_mesh(node, x0, y0, x1, y1, rgba, tex=None, z=0.0):
    """色の四角（tex があれば画像）。"""
    m = gfx.RsDynamicMesh(node, "rect")
    gfx.VIEW.mesh_overlay(m.mid, True)
    v = np.zeros((4, 12), np.float32)
    v[:, 0:3] = [[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]]
    v[:, 5] = -1.0
    v[:, 6:10] = rgba
    v[:, 10:12] = [[0, 0], [1, 0], [1, 1], [0, 1]]
    m.set(v, np.array([[0, 1, 2], [0, 2, 3]], np.int64))
    gfx.VIEW.mesh_props(m.mid, True, False, True, 0, 0)
    if tex is not None:
        gfx.VIEW.mesh_tex(m.mid, 0, tex.id)
    return m


class Screen:
    """画面ひとつ（メニュー・インベントリなど）。build() で部品を作り、閉じるときは close()。"""

    def __init__(self, ui):
        self.ui = ui
        self.root = gfx.RsNode(None, "screen")
        self.layer = 0
        self.widgets = []          # (x0, y0, x1, y1, on_click, on_right, 部品の情報)
        self.hover = None

    # ---- 部品 ----
    def clear(self):
        self.root.removeNode()
        self.root = gfx.RsNode(None, "screen")
        self.widgets = []
        self.hover = None

    def _bin(self, k):
        return 20 + self.layer * 10 + k

    def panel(self, x0, y0, x1, y1, rgba=(0.07, 0.08, 0.11, 0.96)):
        m = rect_mesh(self.root, x0, y0, x1, y1, rgba)
        gfx.VIEW.mesh_props(m.mid, True, False, True, 0, self._bin(0))
        return m

    def text(self, s, x, y, size=0.024, color=(1, 1, 1, 1), align=0, max_width=0.0, anchor=(0, 0)):
        """文字。anchor：(0,0) 左上・(0.5,0.5) 真ん中 など（文字の大きさに対する置き場所）。"""
        t = gfx.RsText(self.root, overlay=True, bin=self._bin(3) - 1)    # 文字は板・ボタンより後に描く
        w, h = t.set(s, size, color, max_width, align)
        t.place(x - w * anchor[0], y + h * anchor[1])
        return t, (w, h)

    def image(self, tex, x, y, size, alpha=1.0):
        m = rect_mesh(self.root, x - size / 2, y - size / 2, x + size / 2, y + size / 2, (1, 1, 1, alpha), tex)
        gfx.VIEW.mesh_props(m.mid, True, False, True, 0, self._bin(2))
        return m

    def button(self, label, x, y, w, h, on_click, color=(0.18, 0.2, 0.25, 1.0), size=0.022, on_right=None, info=None):
        """ボタン（真ん中が (x, y)）。"""
        m = rect_mesh(self.root, x - w / 2, y - h / 2, x + w / 2, y + h / 2, color)
        gfx.VIEW.mesh_props(m.mid, True, False, True, 0, self._bin(1))
        if label:
            self.text(label, x, y, size, anchor=(0.5, 0.5))
        self.widgets.append({"box": (x - w / 2, y - h / 2, x + w / 2, y + h / 2), "click": on_click, "right": on_right,
                             "mesh": m, "color": color, "info": info})
        return m

    # ---- 入力 ----
    def hit(self, x, y):
        for wd in reversed(self.widgets):
            x0, y0, x1, y1 = wd["box"]
            if x0 <= x <= x1 and y0 <= y <= y1:
                return wd
        return None

    def on_move(self, x, y):
        h = self.hit(x, y)
        if h is not self.hover:
            if self.hover is not None and self.hover["mesh"].np.alive:
                gfx.VIEW.node_tint(self.hover["mesh"].np.id, 1.0, 1.0, 1.0, 1.0)
            if h is not None:
                gfx.VIEW.node_tint(h["mesh"].np.id, 1.45, 1.45, 1.45, 1.0)        # マウスが乗ったら明るく
            self.hover = h

    def on_click(self, x, y, button):
        h = self.hit(x, y)
        if h is None:
            return self.on_empty_click(button)
        fn = h["click"] if button == 0 else h["right"]
        if fn is not None:
            fn()
        return True

    def on_empty_click(self, button):
        return False

    def on_key(self, key):
        """キー。True なら使った。"""
        return False

    def on_text(self, t):
        return False

    def update(self, dt):
        pass

    def close(self):
        self.root.removeNode()
        self.ui.closed(self)


class UI:
    """画面の重なり（いちばん上が入力を受ける）とマウスの位置。"""

    def __init__(self, view):
        self.view = view
        self.stack = []
        self.mouse = (0.0, 0.0)

    @property
    def top(self):
        return self.stack[-1] if self.stack else None

    def open(self, scr_cls, *a, **k):
        s = scr_cls(self, *a, **k)
        s.layer = len(self.stack)
        self.stack.append(s)
        s.build()
        return s

    def closed(self, s):
        if s in self.stack:
            self.stack.remove(s)

    def to_ui(self, px, py):
        w, h = self.view.size()
        return (px - w / 2) / h, (h / 2 - py) / h

    def event(self, kind, a, b, n):
        """入力を渡す。画面が開いていて使ったら True。"""
        s = self.top
        if kind == "cursor":
            self.mouse = self.to_ui(a, b)
            if s is not None:
                s.on_move(*self.mouse)
            return s is not None
        if s is None:
            return False
        if kind == "button":
            if a:
                s.on_click(self.mouse[0], self.mouse[1], n)
            return True
        if kind.startswith("key:"):
            if a:
                s.on_key(kind[4:])
            return True
        if kind.startswith("text:"):
            s.on_text(kind[5:])
            return True
        return kind in ("wheel", "mouse")
