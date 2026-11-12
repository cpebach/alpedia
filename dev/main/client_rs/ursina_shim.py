"""ursina のまね（Rust の画面で、今までの ursina の画面の部品をそのまま動かす）。

今までの画面（ブロックプログラム・プログラム・メニューなど）と MOD の画面側は ursina の Entity・Text・Button・
InputField・mouse・camera.ui・color などで作られている。Rust の画面のときはこのモジュールを sys.modules["ursina"] に
入れておくと、そのコードがそのまま Rust の画面に描かれる。今まねているのは画面の上（camera.ui）の 2D の部品。

座標は ursina と同じ（画面の縦の高さが 1、真ん中が原点、x 右・y 上。z が小さいほど手前）。
"""
import math
import time as _t

import numpy as np

from ..client import gfx

BIN0 = 100000             # 画面の上の部品は、ゲームの HUD より後（手前）に描く
TEXT_SIZE = 0.025          # ursina の Text.size（scale 1 の文字の大きさ）
FONT_K = 1.1               # 文字の高さ → フォントの大きさ（ursina の見た目に合わせる）


# ---------------------------------------------------------------------------
# 色
# ---------------------------------------------------------------------------
class Color(tuple):
    def __new__(cls, r, g, b, a=1.0):
        return super().__new__(cls, (float(r), float(g), float(b), float(a)))

    r = property(lambda s: s[0])
    g = property(lambda s: s[1])
    b = property(lambda s: s[2])
    a = property(lambda s: s[3])


class _ColorMod:
    def rgb(self, r, g, b):
        if max(r, g, b) > 1.0:
            r, g, b = r / 255, g / 255, b / 255
        return Color(r, g, b, 1)

    def rgba(self, r, g, b, a=1.0):
        if max(r, g, b) > 1.0:
            r, g, b, a = r / 255, g / 255, b / 255, (a / 255 if a > 1 else a)
        return Color(r, g, b, a)

    def hsv(self, h, s, v, a=1.0):
        import colorsys
        return Color(*colorsys.hsv_to_rgb((h % 360) / 360, s, v), a)

    white = Color(1, 1, 1)
    black = Color(0, 0, 0)
    clear = Color(0, 0, 0, 0)
    gray = Color(0.5, 0.5, 0.5)
    grey = gray
    light_gray = Color(0.75, 0.75, 0.75)
    lighter_gray = Color(0.85, 0.85, 0.85)
    dark_gray = Color(0.25, 0.25, 0.25)
    smoke = Color(1, 1, 1, 0.5)
    red = Color(1, 0.1, 0.1)
    green = Color(0.1, 0.8, 0.2)
    lime = Color(0.5, 1, 0.2)
    blue = Color(0.1, 0.3, 1)
    azure = Color(0.2, 0.55, 1)
    cyan = Color(0, 1, 1)
    yellow = Color(1, 1, 0.1)
    orange = Color(1, 0.55, 0.1)
    magenta = Color(1, 0.1, 1)
    pink = Color(1, 0.5, 0.75)
    violet = Color(0.55, 0.2, 1)
    brown = Color(0.55, 0.35, 0.15)
    olive = Color(0.5, 0.5, 0.1)
    gold = Color(1, 0.8, 0.2)

    def __getattr__(self, name):
        return Color(0.6, 0.6, 0.6)


color = _ColorMod()


class Vec2(tuple):
    def __new__(cls, x=0.0, y=0.0):
        return super().__new__(cls, (float(x), float(y)))

    x = property(lambda s: s[0])
    y = property(lambda s: s[1])

    def __add__(self, o):
        return Vec2(self[0] + o[0], self[1] + o[1])

    def __sub__(self, o):
        return Vec2(self[0] - o[0], self[1] - o[1])

    def __mul__(self, k):
        if isinstance(k, (tuple, list)):
            return Vec2(self[0] * k[0], self[1] * k[1])
        return Vec2(self[0] * k, self[1] * k)


class Vec3(tuple):
    def __new__(cls, x=0.0, y=0.0, z=0.0):
        if isinstance(x, (tuple, list)):
            x, y, z = (list(x) + [0, 0, 0])[:3]
        return super().__new__(cls, (float(x), float(y), float(z)))

    x = property(lambda s: s[0])
    y = property(lambda s: s[1])
    z = property(lambda s: s[2])

    def __add__(self, o):
        o = Vec3(o)
        return Vec3(self[0] + o[0], self[1] + o[1], self[2] + o[2])

    def __sub__(self, o):
        o = Vec3(o)
        return Vec3(self[0] - o[0], self[1] - o[1], self[2] - o[2])

    def __mul__(self, k):
        return Vec3(self[0] * k, self[1] * k, self[2] * k)


# ---------------------------------------------------------------------------
# 形（Quad・Mesh）とテクスチャ
# ---------------------------------------------------------------------------
class Mesh:
    def __init__(self, vertices=(), triangles=None, colors=None, uvs=None, mode="triangle", **kw):
        self.vertices = [tuple(v) for v in vertices]
        self.triangles = triangles
        self.colors = colors
        self.uvs = uvs
        self.mode = mode

    def arrays(self):
        """頂点 (n,3)・UV (n,2)・三角形 (m,3)"""
        V = np.asarray(self.vertices, float).reshape(-1, 3)
        if self.triangles:
            T = np.asarray([t for t in self.triangles], np.int64).reshape(-1, 3)
        else:
            T = np.arange(len(V) - len(V) % 3, dtype=np.int64).reshape(-1, 3)
        UV = np.asarray(self.uvs, float).reshape(-1, 2) if self.uvs else V[:, :2] + 0.5
        return V, UV, T


class Quad(Mesh):
    """角の丸い四角（幅 1・高さ 1、真ん中が原点。aspect＝幅÷高さで丸みを合わせる）。"""

    def __init__(self, radius=0.1, aspect=1.0, segments=3, **kw):
        r = max(0.0, min(float(radius), 0.5))
        a = max(float(aspect), 1e-3)
        pts = []
        if r <= 1e-4:
            pts = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]
        else:
            rx, ry = r / a, r          # 縦の高さ 1 に対する丸み → 幅に直す
            n = max(2, int(segments) * 2)
            for cx, cy, a0 in ((0.5 - rx, 0.5 - ry, 0), (-0.5 + rx, 0.5 - ry, 90), (-0.5 + rx, -0.5 + ry, 180),
                               (0.5 - rx, -0.5 + ry, 270)):
                for k in range(n + 1):
                    t = math.radians(a0 + 90 * k / n)
                    pts.append((cx + rx * math.cos(t), cy + ry * math.sin(t)))
        verts = [(0.0, 0.0, 0.0)] + [(x, y, 0.0) for x, y in pts]
        tris = [(0, 1 + k, 1 + (k + 1) % len(pts)) for k in range(len(pts))]
        super().__init__(vertices=verts, triangles=tris, uvs=[(x + 0.5, y + 0.5) for x, y, _ in verts])


_QUAD = Mesh(vertices=[(-0.5, -0.5, 0), (0.5, -0.5, 0), (0.5, 0.5, 0), (-0.5, 0.5, 0)], triangles=[(0, 1, 2), (0, 2, 3)],
             uvs=[(0, 0), (1, 0), (1, 1), (0, 1)])


def _circle(n=24):
    pts = [(0.5 * math.cos(2 * math.pi * k / n), 0.5 * math.sin(2 * math.pi * k / n)) for k in range(n)]
    verts = [(0.0, 0.0, 0.0)] + [(x, y, 0.0) for x, y in pts]
    return Mesh(vertices=verts, triangles=[(0, 1 + k, 1 + (k + 1) % n) for k in range(n)],
                uvs=[(x + 0.5, y + 0.5) for x, y, _ in verts])


_MODELS = {"quad": _QUAD, "circle": _circle(), "cube": _QUAD}


class Texture:
    def __init__(self, src=None, *a, **k):
        self.rs = None
        if isinstance(src, gfx.RsTex):
            self.rs = src
        elif src is not None and hasattr(src, "convert"):            # PIL の画像
            im = src.convert("RGBA")
            self.rs = gfx.tex_rgba(im.width, im.height, im.tobytes(), "pil")
        elif isinstance(src, str):
            self.rs = gfx.tex(src)
        self.filtering = None

    width = property(lambda s: 1)
    height = property(lambda s: 1)


def load_texture(path, *a, **k):
    t = Texture(str(path))
    return t if t.rs is not None else None


def load_model(*a, **k):
    return None


def _tex_of(t):
    if t is None:
        return None
    if isinstance(t, Texture):
        return t.rs
    if isinstance(t, gfx.RsTex):
        return t
    if isinstance(t, str):
        from .. import paths
        for p in (t, paths.SAUCE / "textures" / t, paths.SAUCE / "textures" / f"{t}.png"):
            r = gfx.tex(str(p))
            if r is not None:
                return r
        return None
    if hasattr(t, "convert"):
        return Texture(t).rs
    return None


# ---------------------------------------------------------------------------
# 部品
# ---------------------------------------------------------------------------
_seq = [0]
_entities = []            # 生きている Entity（入力・update を配る順）
_dirty = set()


class Entity:
    """ursina の Entity（画面の上の 2D の部品）。"""
    default_shader = None

    def __init__(self, parent=None, model=None, texture=None, color=None, position=(0, 0, 0), scale=None,
                 origin=(0, 0), rotation=None, enabled=True, z=None, x=None, y=None, eternal=False, **kw):
        self._parent = None
        self.children = []
        self._seq = _seq[0] = _seq[0] + 1
        self._node = None
        self._mesh = None
        self._model = None
        self._texture = None
        self._color = Color(1, 1, 1)
        self._pos = list(Vec3(position))
        self._scale = [1.0, 1.0, 1.0]
        self._origin = (0.0, 0.0)
        self._rot = 0.0
        self._enabled = True
        self._alpha = 1.0
        self.name = kw.pop("name", type(self).__name__)
        self.ignore_input = False
        self.collider = kw.pop("collider", None)
        self._alive = True
        if parent is None:
            parent = scene
        self.parent = parent
        if scale is not None:
            self.scale = scale
        if origin is not None:
            self.origin = origin
        if rotation is not None:
            self.rotation = rotation
        if x is not None:
            self.x = x
        if y is not None:
            self.y = y
        if z is not None:
            self.z = z
        if color is not None:
            self.color = color
        if texture is not None:
            self.texture = texture
        self.model = model
        self.enabled = enabled
        for k, v in kw.items():
            try:
                setattr(self, k, v)
            except Exception:  # noqa: BLE001
                pass
        _entities.append(self)

    # ---- 親子 ----
    @property
    def parent(self):
        return self._parent

    @parent.setter
    def parent(self, p):
        if self._parent is not None and self in self._parent.children:
            self._parent.children.remove(self)
        self._parent = p
        if p is not None:
            p.children.append(self)
        if self._node is not None and p is not None:
            self._node.parent = p.node()
        self._touch()

    def node(self):
        if self._node is None:
            pn = self._parent.node() if self._parent is not None else None
            self._node = gfx.RsNode(pn, self.name)
            self._apply()
        return self._node

    @property
    def is_ui(self):
        p = self
        while p is not None:
            if p is camera.ui:
                return True
            p = p._parent
        return False

    # ---- 位置・大きさ ----
    def _touch(self):
        if self._alive:
            _dirty.add(self)

    def _apply(self):
        """ノードの行列（位置・回転・大きさ）と、形の z の順番。"""
        if self._node is None or not self._alive:
            return
        sx, sy, sz = self._scale
        c, s = math.cos(math.radians(-self._rot)), math.sin(math.radians(-self._rot))
        x, y, z = self._pos
        self._node.setMat([sx * c, sx * s, 0, 0, -sy * s, sy * c, 0, 0, 0, 0, sz, 0, x, y, 0, 1])
        on = self._enabled and self.visible_self
        (self._node.show if on else self._node.hide)()

    visible_self = True

    def _world_z(self):
        z, p = 0.0, self
        while p is not None and p is not camera.ui:
            z += p._pos[2]
            p = p._parent
        return z

    def _order(self):
        """画面の上の描く順（z が小さいほど後＝手前、同じなら作った順）。"""
        return int(max(-2000, min(2000, -self._world_z() * 2000))) * 100000 + self._seq % 100000

    position = property(lambda s: Vec3(s._pos), lambda s, v: s._set_pos(Vec3(v)))
    world_position = property(lambda s: Vec3(s._world_xy() + (s._world_z(),)))

    def _set_pos(self, v):
        self._pos = list(v) if len(v) == 3 else [v[0], v[1], self._pos[2]]
        self._touch()

    def _seti(self, i, v):
        self._pos[i] = float(v)
        self._touch()

    x = property(lambda s: s._pos[0], lambda s, v: s._seti(0, v))
    y = property(lambda s: s._pos[1], lambda s, v: s._seti(1, v))
    z = property(lambda s: s._pos[2], lambda s, v: s._seti(2, v))

    @property
    def scale(self):
        return Vec3(self._scale)

    @scale.setter
    def scale(self, v):
        if isinstance(v, (int, float)):
            self._scale = [float(v)] * 3
        else:
            v = list(v)
            self._scale = [float(v[0]), float(v[1]), float(v[2]) if len(v) > 2 else 1.0]
        self._touch()

    def _sets(self, i, v):
        self._scale[i] = float(v)
        self._touch()

    scale_x = property(lambda s: s._scale[0], lambda s, v: s._sets(0, v))
    scale_y = property(lambda s: s._scale[1], lambda s, v: s._sets(1, v))
    scale_z = property(lambda s: s._scale[2], lambda s, v: s._sets(2, v))

    @property
    def world_scale(self):
        sx, sy, p = 1.0, 1.0, self
        while p is not None and p is not camera.ui:
            sx *= p._scale[0]
            sy *= p._scale[1]
            p = p._parent
        return Vec3(sx, sy, 1)

    @world_scale.setter
    def world_scale(self, v):
        v = [v] * 3 if isinstance(v, (int, float)) else list(v)
        ps = self._parent.world_scale if self._parent is not None else Vec3(1, 1, 1)
        self.scale = (v[0] / max(ps[0], 1e-9), v[1] / max(ps[1], 1e-9), 1)

    world_scale_x = property(lambda s: s.world_scale[0])
    world_scale_y = property(lambda s: s.world_scale[1])

    @property
    def rotation(self):
        return Vec3(0, 0, self._rot)

    @rotation.setter
    def rotation(self, v):
        self._rot = float(v[2] if isinstance(v, (tuple, list)) else v)
        self._touch()

    rotation_z = property(lambda s: s._rot, lambda s, v: setattr(s, "rotation", v))

    @property
    def origin(self):
        return Vec2(*self._origin)

    @origin.setter
    def origin(self, v):
        self._origin = (float(v[0]), float(v[1]) if len(v) > 1 else 0.0)
        self._model_dirty = True
        self._touch()

    origin_x = property(lambda s: s._origin[0], lambda s, v: setattr(s, "origin", (v, s._origin[1])))
    origin_y = property(lambda s: s._origin[1], lambda s, v: setattr(s, "origin", (s._origin[0], v)))

    def _world_xy(self):
        """画面の上の (x, y)（親の位置・大きさ・回転を含める。回転は z 軸だけ）。"""
        p = self
        pts = []
        while p is not None and p is not camera.ui:
            pts.append(p)
            p = p._parent
        # 点 (0,0) を自分の座標から親へ順に
        px, py = 0.0, 0.0
        for e in pts:
            c, s = math.cos(math.radians(-e._rot)), math.sin(math.radians(-e._rot))
            qx, qy = px * e._scale[0], py * e._scale[1]
            px, py = qx * c - qy * s + e._pos[0], qx * s + qy * c + e._pos[1]
        return (px, py)

    def _world_box(self):
        """形（quad など）の画面の上の外枠 (x0, y0, x1, y1)。回転は無視。"""
        ws = self.world_scale
        cx, cy = self._world_xy()
        ox, oy = self._origin
        x0 = cx + (-0.5 - ox) * ws[0]
        x1 = cx + (0.5 - ox) * ws[0]
        y0 = cy + (-0.5 - oy) * ws[1]
        y1 = cy + (0.5 - oy) * ws[1]
        return min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1)

    # ---- 見た目 ----
    @property
    def model(self):
        return self._model

    @model.setter
    def model(self, m):
        if isinstance(m, str):
            m = _MODELS.get(m, _QUAD if m else None)
        self._model = m
        self._model_dirty = True
        self._build_mesh()

    @property
    def texture(self):
        return self._texture

    @texture.setter
    def texture(self, t):
        self._texture = t
        if self._mesh is not None:
            r = _tex_of(t)
            gfx.VIEW.mesh_tex(self._mesh.mid, 0, r.id if r is not None else -1)

    @property
    def color(self):
        return self._color

    @color.setter
    def color(self, c):
        c = tuple(c)
        self._color = Color(*(list(c) + [1.0])[:4])
        if self._mesh is not None:
            self._paint()

    alpha = property(lambda s: s._color[3], lambda s, v: setattr(s, "color", (s._color[0], s._color[1], s._color[2], v)))

    def _build_mesh(self):
        if not self._alive:
            return
        if self._model is None or not self.is_ui:          # 3D の部品はまだ描かない（画面の上だけ）
            if self._mesh is not None:
                self._mesh.remove()
                self._mesh = None
            return
        if self._mesh is None:
            self._mesh = gfx.RsDynamicMesh(self.node(), "e")
            gfx.VIEW.mesh_overlay(self._mesh.mid, self.is_ui)
        self._paint(new=True)
        self.texture = self._texture
        self._touch()

    def _paint(self, new=False):
        V, UV, T = self._model.arrays()
        ox, oy = self._origin
        v = np.zeros((len(V), 12), np.float32)
        v[:, 0] = V[:, 0] - ox
        v[:, 1] = V[:, 1] - oy
        v[:, 2] = 0
        v[:, 5] = -1
        cols = getattr(self._model, "colors", None)
        base = np.array(self._color, float)
        if cols is not None and len(cols) == len(V):
            v[:, 6:10] = np.asarray([tuple(c)[:4] if len(c) >= 4 else tuple(c) + (1,) for c in cols], float) * base
        else:
            v[:, 6:10] = base
        v[:, 10:12] = UV
        self._mesh.set(v, T if (new or self._mesh.n_rows != len(v)) else None)
        self._mesh.n_rows = len(v)

    def _sort(self):
        if self._mesh is not None and self._alive:
            gfx.VIEW.mesh_props(self._mesh.mid, True, False, True, 0, BIN0 + self._order() // 100000 * 1000 + self._seq % 1000)
            if getattr(self, "_model_dirty", False):
                self._model_dirty = False
                self._paint()

    # ---- 表示・入力 ----
    @property
    def enabled(self):
        return self._enabled

    @enabled.setter
    def enabled(self, on):
        self._enabled = bool(on)
        self._touch()
        if not on and _mouse.hovered_entity is not None:
            _mouse.hovered_entity = None

    visible = enabled

    def active_chain(self):
        p = self
        while p is not None:
            if not p._enabled or not p._alive:
                return False
            p = p._parent
        return True

    def show(self):
        self.enabled = True

    def hide(self):
        self.enabled = False

    @property
    def hovered(self):
        return _mouse.hovered_entity is self

    def disable(self):
        self.enabled = False

    def enable(self):
        self.enabled = True

    # Panda3D の NodePath の物はまねるだけ
    def setTwoSided(self, *a):
        pass

    setDepthOffset = setBin = setLightOff = setShaderOff = setDepthTest = setDepthWrite = setTransparency = setTwoSided
    set_two_sided = setTwoSided

    def setColorScale(self, r, g, b, a=1.0):
        self.node()
        gfx.VIEW.node_tint(self._node.id, float(r), float(g), float(b), float(a))

    def animate_scale(self, v, *a, **k):
        self.scale = v

    def animate_color(self, v, *a, **k):
        self.color = v

    def fade_out(self, *a, **k):
        pass

    def _destroy(self):
        if not self._alive:
            return
        self._alive = False
        for ch in list(self.children):
            ch._destroy()
        if self._node is not None:
            self._node.removeNode()
            self._node = None
        self._mesh = None
        if self._parent is not None and self in self._parent.children:
            self._parent.children.remove(self)
        if self in _entities:
            _entities.remove(self)
        _dirty.discard(self)
        if _mouse.hovered_entity is self:
            _mouse.hovered_entity = None
        if _focus[0] is self:
            _focus[0] = None


def destroy(e, delay=0):
    if e is None:
        return
    if delay:
        invoke(destroy, e, delay=delay)
        return
    if isinstance(e, Entity):
        e._destroy()


# ---------------------------------------------------------------------------
# 文字
# ---------------------------------------------------------------------------
class Text(Entity):
    size = TEXT_SIZE
    default_font = None
    default_monospace_font = None
    default_resolution = 1080

    def __init__(self, text="", parent=None, origin=(-0.5, 0.5), wordwrap=0, **kw):
        self._text = ""
        self._txt = None
        self._wordwrap = int(wordwrap or 0)
        self._wh = (0.0, 0.0)
        kw.pop("start_tag", None)
        kw.pop("end_tag", None)
        kw.pop("font", None)
        kw.pop("resolution", None)
        kw.pop("background", None)
        kw.pop("line_height", None)
        col = kw.pop("color", color.white)
        super().__init__(parent=parent if parent is not None else camera.ui, origin=origin, **kw)
        self._color = Color(*(list(col) + [1.0])[:4])
        self.text = text

    def _wrap(self, s):
        if not self._wordwrap:
            return s
        out = []
        for line in s.split("\n"):
            while len(line) > self._wordwrap:
                out.append(line[:self._wordwrap])
                line = line[self._wordwrap:]
            out.append(line)
        return "\n".join(out)

    @property
    def text(self):
        return self._text

    @text.setter
    def text(self, s):
        s = "" if s is None else str(s)
        self._text = s
        self._render()

    @property
    def wordwrap(self):
        return self._wordwrap

    @wordwrap.setter
    def wordwrap(self, n):
        self._wordwrap = int(n or 0)
        self._render()

    def _render(self):
        if not self._alive:
            return
        if not self.is_ui:
            return
        if self._txt is None:
            self._txt = gfx.RsText(self.node(), overlay=True)
        s = self._wrap(self._text)
        ox = self._origin[0]
        al = 0 if ox <= -0.25 else (2 if ox >= 0.25 else 1)     # ursina と同じく、行ごとに origin の x でそろえる
        w, h = self._txt.set(s, TEXT_SIZE * FONT_K, tuple(self._color), 0.0, al) if s else (0.0, 0.0)
        if not s:
            self._txt.set("", TEXT_SIZE * FONT_K)
        self._wh = (w, h)
        self._place()
        self._touch()

    def _place(self):
        if self._txt is None:
            return
        w, h = self._wh
        ox, oy = self._origin
        self._txt.node.setPos(-(ox + 0.5) * w, -(oy - 0.5) * h, 0.0)

    @Entity.color.setter
    def color(self, c):
        self._color = Color(*(list(tuple(c)) + [1.0])[:4])
        if getattr(self, "_txt", None) is not None:
            self._txt.key = None
            self._render()

    @Entity.origin.setter
    def origin(self, v):
        self._origin = (float(v[0]), float(v[1]) if len(v) > 1 else 0.0)
        self._render() if getattr(self, "_txt", None) is not None else None
        self._touch()

    width = property(lambda s: s._wh[0])
    height = property(lambda s: s._wh[1])

    def _sort(self):
        if self._txt is not None and self._alive:
            b = BIN0 + self._order() // 100000 * 1000 + self._seq % 1000 + 1
            gfx.VIEW.mesh_props(self._txt.text.mid, True, False, True, 0, b)

    def _world_box(self):
        ws = self.world_scale
        cx, cy = self._world_xy()
        w, h = self._wh[0] * ws[0], self._wh[1] * ws[1]
        ox, oy = self._origin
        x0 = cx - (ox + 0.5) * w
        y1 = cy - (oy - 0.5) * h
        return x0, y1 - h, x0 + w, y1


# ---------------------------------------------------------------------------
# ボタン・入力欄
# ---------------------------------------------------------------------------
class Button(Entity):
    def __init__(self, text="", parent=None, model="quad", radius=0.1, color=None, highlight_color=None, pressed_color=None,
                 text_size=1, text_color=None, text_origin=(0, 0), scale=None, **kw):
        kw.pop("highlight_scale", None)
        kw.pop("pressed_scale", None)
        kw.pop("highlight_text_color", None)
        kw.pop("icon", None)
        kw.pop("disabled", None)
        on_click = kw.pop("on_click", None)
        if model == "quad" and radius:
            model = Quad(radius=radius, aspect=1.0)
        self._base = None
        super().__init__(parent=parent if parent is not None else camera.ui, model=model,
                         scale=scale if scale is not None else (0.25, 0.075), **kw)
        if isinstance(self._model, Quad):
            sx, sy = self._scale[0], self._scale[1]
            self.model = Quad(radius=radius, aspect=sx / max(sy, 1e-6))
        base = color if color is not None else Color(0.18, 0.18, 0.2, 1)
        self._base = Color(*(list(tuple(base)) + [1.0])[:4])
        self.highlight_color = highlight_color if highlight_color is not None else \
            Color(*[min(1, v * 1.25 + 0.05) for v in self._base[:3]], self._base[3])
        self.pressed_color = pressed_color if pressed_color is not None else self.highlight_color
        Entity.color.fset(self, self._base)
        self.text_entity = None
        self._text_size = float(text_size)
        self.on_click = on_click
        self._text_color = text_color
        if text:
            self.text = text

    @property
    def color(self):
        return self._base if self._base is not None else self._color

    @color.setter
    def color(self, c):
        if getattr(self, "_base", None) is None:
            Entity.color.fset(self, c)
            return
        self._base = Color(*(list(tuple(c)) + [1.0])[:4])
        self.highlight_color = Color(*[min(1, v * 1.25 + 0.05) for v in self._base[:3]], self._base[3])
        Entity.color.fset(self, self._base)

    @property
    def text(self):
        return self.text_entity.text if self.text_entity is not None else ""

    @text.setter
    def text(self, s):
        if self.text_entity is None:
            self.text_entity = Text(parent=self, text="", origin=(0, 0), z=-0.001,
                                    color=self._text_color if self._text_color is not None else color.white)
            self._fit_text()
        self.text_entity.text = s

    @property
    def text_size(self):
        return self._text_size

    @text_size.setter
    def text_size(self, v):
        self._text_size = float(v)
        self._fit_text()

    def _fit_text(self):
        if self.text_entity is not None:
            ws = self.world_scale
            self.text_entity.scale = (self._text_size / max(ws[0], 1e-6), self._text_size / max(ws[1], 1e-6), 1)

    @property
    def text_color(self):
        return self.text_entity.color if self.text_entity is not None else color.white

    @text_color.setter
    def text_color(self, c):
        self._text_color = c
        if self.text_entity is not None:
            self.text_entity.color = c

    def _hover(self, on):
        Entity.color.fset(self, self.highlight_color if on else self._base)


class _TextField:
    """InputField.text_field のまね（text・render・cursor）。"""

    class _Cursor:
        x = 0
        y = 0

    def __init__(self, owner):
        self.owner = owner
        self.text = ""
        self.cursor = self._Cursor()

    def render(self):
        self.owner._show()


class InputField(Button):
    def __init__(self, default_value="", label="", max_lines=1, character_limit=24, text="", active=False, **kw):
        kw.setdefault("scale", (0.5, TEXT_SIZE * 2 * max_lines))
        kw.setdefault("color", Color(0.1, 0.1, 0.12, 1))
        super().__init__(**kw)
        self.text_field = _TextField(self)
        self.text_field.text = str(text or default_value or "")
        self.character_limit = character_limit
        self.limit_content_to = None
        self.submit_on = []
        self.on_submit = None
        self.on_value_changed = None
        self._shown = Text(parent=self, text="", origin=(-0.5, 0), x=-0.48, z=-0.001)
        ws = self.world_scale
        self._shown.scale = (1 / max(ws[0], 1e-6), 1 / max(ws[1], 1e-6), 1)
        self.on_click = self._focus
        self._active = False
        self.active = active

    @property
    def active(self):
        return self._active

    @active.setter
    def active(self, on):
        self._active = bool(on)
        if on:
            _focus[0] = self
        elif _focus[0] is self:
            _focus[0] = None
        self._show()

    def _focus(self):
        self.active = True

    @property
    def value(self):
        return self.text_field.text

    def _show(self):
        if getattr(self, "_shown", None) is not None:
            self._shown.text = self.text_field.text + ("｜" if self._active else "")

    def type(self, t):
        if self.limit_content_to:
            t = "".join(ch for ch in t if ch in self.limit_content_to)
        s = self.text_field.text + t
        if self.character_limit:
            s = s[:self.character_limit]
        self.text_field.text = s
        self._show()
        if self.on_value_changed:
            self.on_value_changed()

    def key(self, k):
        if k == "backspace":
            self.text_field.text = self.text_field.text[:-1]
            self._show()
            return True
        if k in ("enter", "return") and self.on_submit:
            self.on_submit()
            return True
        return k == "space" or len(k) == 1


_focus = [None]


# ---------------------------------------------------------------------------
# 画面・マウス・時間
# ---------------------------------------------------------------------------
class _Mouse:
    def __init__(self):
        self.x = 0.0
        self.y = 0.0
        self.locked = False
        self.visible = True
        self.hovered_entity = None
        self.left = False
        self.right = False
        self.velocity = Vec3(0, 0, 0)

    position = property(lambda s: Vec3(s.x, s.y, 0))


_mouse = _Mouse()
mouse = _mouse


class _Window:
    def __init__(self):
        self._size = (1280, 720)
        self.color = Color(0.45, 0.65, 0.9)
        self._fullscreen = False
        self.borderless = False
        self.title = "Alpedia"
        self.vsync = True

    @property
    def size(self):
        return Vec2(*self._size)

    @size.setter
    def size(self, v):
        """窓の大きさを変える（設定の解像度）。"""
        v = (int(v[0]), int(v[1]))
        if gfx.VIEW is not None and v != tuple(self._size):
            gfx.VIEW.set_size(*v)
        self._size = v

    @property
    def fullscreen(self):
        return gfx.VIEW.fullscreen() if gfx.VIEW is not None else self._fullscreen

    @fullscreen.setter
    def fullscreen(self, on):
        self._fullscreen = bool(on)
        if gfx.VIEW is not None:
            gfx.VIEW.set_fullscreen(bool(on))

    aspect_ratio = property(lambda s: s._size[0] / max(s._size[1], 1))
    top = property(lambda s: Vec2(0, 0.5))
    bottom = property(lambda s: Vec2(0, -0.5))
    left = property(lambda s: Vec2(-s.aspect_ratio / 2, 0))
    right = property(lambda s: Vec2(s.aspect_ratio / 2, 0))
    top_left = property(lambda s: Vec2(-s.aspect_ratio / 2, 0.5))
    top_right = property(lambda s: Vec2(s.aspect_ratio / 2, 0.5))
    bottom_left = property(lambda s: Vec2(-s.aspect_ratio / 2, -0.5))
    bottom_right = property(lambda s: Vec2(s.aspect_ratio / 2, -0.5))


window = _Window()
held_keys = {}


class _Time:
    dt = 1 / 60


time_ = _Time()


class _App:
    paused = False

    @staticmethod
    def quit():
        pass

    fonts_folder = None
    base = None
    asset_folder = None


class _Lens:
    def getFov(self):
        return camera.fov_pair


class _CamNode:
    def getLens(self):
        return _Lens()


class _Cam:
    def node(self):
        return _CamNode()


class _Base:
    """application.base のまね（MOD が画面の視野角を読む：base.cam.node().getLens().getFov()）。"""
    cam = _Cam()


application = _App()
application.base = _Base()


class _Scene(Entity):
    def __init__(self):
        self._alive = True
        self._parent = None
        self.children = []
        self._seq = 0
        self._node = None
        self._mesh = None
        self._model = None
        self._texture = None
        self._color = Color(1, 1, 1)
        self._pos = [0.0, 0.0, 0.0]
        self._scale = [1.0, 1.0, 1.0]
        self._origin = (0.0, 0.0)
        self._rot = 0.0
        self._enabled = True
        self.name = "scene"


scene = _Scene()


class _Camera:
    def __init__(self):
        self.ui = _Scene()
        self.ui.name = "ui"
        self.fov = 80
        self.fov_pair = (80.0, 50.0)        # (横, 縦) の視野角（Rust の画面が毎フレーム入れる）
        self.position = Vec3()
        self.clip_plane_near = 0.05
        self.clip_plane_far = 10000
        self.orthographic = False


camera = _Camera()

_invokes = []


def invoke(fn, *args, delay=0.0, **kw):
    _invokes.append((_t.time() + float(delay), fn, args, kw))


class Sequence:
    def __init__(self, *a, **k):
        pass

    def start(self):
        pass

    def kill(self):
        pass


class Audio:
    def __init__(self, *a, **k):
        pass

    def play(self):
        pass

    def stop(self, *a, **k):
        pass


class _InputHandler:
    """ursina の input_handler（Ctrl などと組み合わせたキーの名前）。"""

    @staticmethod
    def get_combined_key(key):
        mods = [m for m, ks in (("control", ("left control", "right control")), ("shift", ("left shift", "right shift")),
                                ("alt", ("left alt", "right alt"))) if any(held_keys.get(k) for k in ks)]
        base = key.split(" ")[-1] if key.startswith(("left ", "right ")) else key
        mods = [m for m in mods if m != base]
        return "+".join(mods + [key]) if mods else key


input_handler = _InputHandler()


class Ursina:
    def __init__(self, *a, **k):
        pass


# ---------------------------------------------------------------------------
# Rust の画面からまわす
# ---------------------------------------------------------------------------
def _overrides(e, name):
    return getattr(type(e), name, None) is not getattr(Entity, name, None) and hasattr(e, name)


def frame(view, dt):
    """毎フレーム：invoke・update() を呼び、位置・描く順を Rust の画面へ。"""
    time_.dt = dt
    w, h = view.size()
    window._size = (w, h)
    now = _t.time()
    due = [x for x in _invokes if x[0] <= now]
    for x in due:
        _invokes.remove(x)
        try:
            x[1](*x[2], **x[3])
        except Exception:  # noqa: BLE001
            from .. import log
            log.get("ursina_shim").exception("invoke に失敗しました")
    for e in list(_entities):
        if e._alive and _overrides(e, "update") and e.active_chain():
            e.update()
    for e in list(_dirty):
        if e._alive:
            e._apply()
    if _dirty:
        for e in list(_entities):
            if e._alive and e.is_ui:
                e._sort()
    _dirty.clear()


def _hit(x, y):
    """マウスの下のいちばん手前のボタン。"""
    best = None
    for e in _entities:
        if not isinstance(e, Button) or not e._alive or e.ignore_input or not e.active_chain() or not e.is_ui:
            continue
        x0, y0, x1, y1 = e._world_box()
        if x0 <= x <= x1 and y0 <= y <= y1:
            k = (e._world_z(), -e._seq)
            if best is None or k < best[0]:
                best = (k, e)
    return best[1] if best else None


def event(view, kind, a, b, n):
    """入力を ursina の形（"left mouse down" など）にして配る。使った（画面が開いている）なら True。"""
    if kind == "cursor":
        w, h = view.size()
        _mouse.x, _mouse.y = (a - w / 2) / h, (h / 2 - b) / h
        hb = _hit(_mouse.x, _mouse.y)
        if hb is not _mouse.hovered_entity:
            old = _mouse.hovered_entity
            if isinstance(old, Button) and old._alive:
                old._hover(False)
            if isinstance(hb, Button):
                hb._hover(True)
            _mouse.hovered_entity = hb
        return True
    if kind == "button":
        name = ("left", "right", "middle", "x")[min(n, 3)] + " mouse " + ("down" if a else "up")
        if n == 0:
            _mouse.left = bool(a)
        if a and n == 0:
            hb = _hit(_mouse.x, _mouse.y)
            if hb is not None and not isinstance(hb, InputField) and _focus[0] is not None:
                pass
            if hb is not None and hb.on_click is not None:
                fn = hb.on_click
                fn() if callable(fn) else None
        _send(name)
        return True
    if kind == "wheel":
        _send("scroll up" if a > 0 else "scroll down")
        return True
    if kind.startswith("key:"):
        k = {"control": "left control", "shift": "left shift", "alt": "left alt"}.get(kind[4:], kind[4:])
        held_keys[k] = 1 if a else 0
        f = _focus[0]
        if a and f is not None and f._alive and f.key(k):
            return True
        _send(k if a else f"{k} up")
        return True
    if kind.startswith("text:"):
        f = _focus[0]
        if f is not None and f._alive:
            f.type(kind[5:])
        return True
    return kind == "mouse"


def _send(key):
    for e in list(_entities):
        if e._alive and _overrides(e, "input") and e.active_chain():
            try:
                e.input(key)
            except Exception:  # noqa: BLE001
                from .. import log
                log.get("ursina_shim").exception("input に失敗しました")


def any_ui():
    """画面の上に ursina の部品が出ているか。"""
    return any(e._alive and e.active_chain() for e in camera.ui.children)


def install():
    """sys.modules["ursina"] をこのまねにする（Rust の画面のときだけ）。"""
    import sys
    import types
    from . import panda_compat
    panda_compat.install()
    m = sys.modules[__name__]
    sys.modules["ursina"] = m
    sys.modules["ursina.color"] = types.SimpleNamespace(**{k: getattr(color, k) for k in dir(type(color)) if not k.startswith("_")})
    return m


time = time_            # from ursina import time（time.dt）
