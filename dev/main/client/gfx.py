"""描画の下まわり：Panda3D（今までの画面）か、Rust の画面（native/alpedia_view）か。

render.py・scenery_view.py の描画（ノード・メッシュ・テクスチャ）は、Rust の画面のときはここの RsNode・RsDynamicMesh を使う。
RsNode は Panda3D の NodePath のうち、render.py が使う分だけをまねる（親子・行列・表示・テクスチャ・半透明など）。
"""
VIEW = None            # alpedia_view.View（Rust の画面を使っているとき）
EDGES = "edges"        # 2枚目のテクスチャ（輪郭の線）の段の目印


def active():
    return VIEW is not None


def start(width=1280, height=720, title="Alpedia", vsync=True):
    """Rust の画面を開く。使えなければ例外。"""
    global VIEW
    import alpedia_view
    VIEW = alpedia_view.View(int(width), int(height), str(title), bool(vsync))
    return VIEW


class RsTex:
    def __init__(self, tid, path):
        self.id = tid
        self.path = path

    # Panda3D のテクスチャの設定はまねるだけ
    def setMagfilter(self, *a):
        pass

    setMinfilter = setWrapU = setWrapV = setMagfilter


_tex = {}


def tex(path, nearest=True):
    if not path:
        return None
    key = (str(path), bool(nearest))
    if key not in _tex:
        tid = VIEW.tex_load(str(path), bool(nearest))
        _tex[key] = RsTex(tid, path) if tid >= 0 else None
    return _tex[key]


def tex_rgba(w, h, data, name="tex", nearest=False):
    tid = VIEW.tex_rgba(int(w), int(h), bytes(data), bool(nearest))
    return RsTex(tid, name)


class RsNode:
    """Rust の画面のノード（NodePath のまね）。"""

    def __init__(self, parent=None, name=""):
        self._parent = parent
        pid = parent.id if isinstance(parent, RsNode) else 0
        self.id = VIEW.node_new(pid)
        self.children = []
        self.meshes = []                  # このノードに付いているメッシュの番号
        self.props = {}
        self.tex = [None, None]
        self.alive = True
        self.enabled = True
        if isinstance(parent, RsNode):
            parent.children.append(self)

    # ---- 親子 ----
    def attachNewNode(self, name=""):
        return RsNode(self, name if isinstance(name, str) else "")

    @property
    def parent(self):
        return self._parent

    @parent.setter
    def parent(self, p):
        if p is self._parent:
            return
        if isinstance(self._parent, RsNode) and self in self._parent.children:
            self._parent.children.remove(self)
        self._parent = p
        if isinstance(p, RsNode):
            p.children.append(self)
            VIEW.node_parent(self.id, p.id)
        else:
            VIEW.node_parent(self.id, 0)

    def reparentTo(self, p):
        self.parent = p

    def removeNode(self):
        if not self.alive:
            return
        VIEW.node_remove(self.id)
        self._mark_dead()
        if isinstance(self._parent, RsNode) and self in self._parent.children:
            self._parent.children.remove(self)

    def _mark_dead(self):
        self.alive = False
        for c in self.children:
            c._mark_dead()

    # ---- 位置 ----
    def setMat(self, m):
        VIEW.node_mat(self.id, [float(x) for x in m])

    def setPos(self, x, y=None, z=None):
        if y is None:
            x, y, z = x
        VIEW.node_pos(self.id, float(x), float(y), float(z))

    def show(self):
        self.enabled = True
        VIEW.node_show(self.id, True)

    def hide(self):
        self.enabled = False
        VIEW.node_show(self.id, False)

    def setColorScale(self, r, g, b, a=1.0):
        VIEW.node_tint(self.id, float(r), float(g), float(b), float(a))

    # ---- 見え方（このノードとその下のメッシュ） ----
    def _apply(self):
        p = self._effective()
        for m in self.meshes:
            VIEW.mesh_props(m, p.get("blend", False), p.get("depth_write", True), p.get("two_sided", True),
                            p.get("depth_offset", 0), p.get("bin", 0))
            for slot in (0, 1):
                t = self._tex_eff(slot)
                VIEW.mesh_tex(m, slot, t.id if t is not None else -1)
        for c in self.children:
            c._apply()

    def _effective(self):
        out = dict(self._parent._effective()) if isinstance(self._parent, RsNode) else {}
        out.update(self.props)
        return out

    def _tex_eff(self, slot):
        if self.tex[slot] is not None:
            return self.tex[slot]
        return self._parent._tex_eff(slot) if isinstance(self._parent, RsNode) else None

    def _set(self, **kw):
        self.props.update(kw)
        self._apply()

    def setTransparency(self, on=True, *a):
        self._set(blend=bool(on))

    def setDepthWrite(self, on, *a):
        self._set(depth_write=bool(on))

    def setTwoSided(self, on, *a):
        self._set(two_sided=bool(on))

    def setDepthOffset(self, n, *a):
        self._set(depth_offset=int(n))

    def setBin(self, name, n=0, *a):
        self._set(bin=(100 if name == "fixed" else 0) + int(n))

    def setTexture(self, *args):
        if len(args) >= 2 and args[0] is EDGES:
            self.tex[1] = args[1]
        elif args and isinstance(args[0], RsTex):
            self.tex[0] = args[0]
        self._apply()

    def clearTexture(self, *a):
        self.tex = [None, None]
        self._apply()

    # Panda3D の設定でまねるだけの物
    def setLightOff(self, *a):
        pass

    setShaderOff = setFinal = setBounds = clearBounds = setColor = setAttrib = setLightOff


class RsDynamicMesh:
    """render.DynamicMesh の Rust 版：頂点 (N,12) と三角形を numpy 配列から設定する。"""

    def __init__(self, parent, name="mesh"):
        self.np = parent.attachNewNode(name) if isinstance(parent, RsNode) else RsNode(None, name)
        self.node = self.np                  # Panda3D の GeomNode のまね（境界の設定などはしない）
        self.mid = VIEW.mesh_new(self.np.id)
        self.np.meshes.append(self.mid)
        self.np._apply()
        self.n_rows = -1

    def set(self, verts, tris=None):
        import numpy as np
        verts = np.ascontiguousarray(verts, dtype=np.float32)
        if len(verts) == 0:
            VIEW.mesh_set(self.mid, np.zeros((0, 12), np.float32), np.zeros((0, 3), np.int64))
            self.n_rows = 0
            return
        if tris is None and len(verts) != self.n_rows:
            raise ValueError("頂点数が変わる時は tris が必要です")
        VIEW.mesh_set(self.mid, verts.reshape(-1, 12),
                      None if tris is None else np.ascontiguousarray(tris, dtype=np.int64).reshape(-1, 3))
        self.n_rows = len(verts)

    def remove(self):
        self.np.removeNode()


# ---------------------------------------------------------------------------
# 文字・板（画面の上の 2D と、ワールドの中の看板）
# ---------------------------------------------------------------------------
_font_ok = None


def font(path=None):
    """フォントを読む（1回だけ）。path が無ければ、いつもの候補から探す。"""
    global _font_ok
    if _font_ok is not None:
        return _font_ok
    from pathlib import Path
    from .game import FONT_CANDIDATES
    for f in ([path] if path else []) + FONT_CANDIDATES:
        if f and Path(f).exists():
            try:
                VIEW.font_load(str(f), 0, 40.0)
                _font_ok = True
                return True
            except Exception:  # noqa: BLE001
                continue
    _font_ok = False
    return False


def quad(mesh, x0, y0, x1, y1, rgba, z=0.0):
    """メッシュに色の四角を1つ（文字の下地など）。"""
    import numpy as np
    v = np.zeros((4, 12), np.float32)
    v[:, 0:3] = [[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]]
    v[:, 5] = -1.0
    v[:, 6:10] = rgba
    mesh.set(v, np.array([[0, 1, 2], [0, 2, 3]], np.int64))
    VIEW.mesh_props(mesh.mid, rgba[3] < 0.999, True, True, 0, 0)


class RsText:
    """文字（と下地）。overlay なら画面の上（縦の高さ 1、真ん中が原点）、そうでなければ親ノードの座標で。"""

    def __init__(self, parent=None, overlay=True, bg=None, bin=0):
        self.node = RsNode(parent) if parent is not None else RsNode(None, "text")
        self.bg = RsDynamicMesh(self.node, "bg") if bg is not None else None
        self.text = RsDynamicMesh(self.node, "text")
        self.overlay = overlay
        self.bgc = bg
        self.bin = bin
        for m in (self.bg, self.text):
            if m is not None:
                VIEW.mesh_overlay(m.mid, overlay)
        self.key = None
        self.size = (0.0, 0.0)

    def set(self, text, size=0.025, color=(1, 1, 1, 1), max_width=0.0, align=0, pad=0.008):
        """文字を変える（同じなら何もしない）。戻り値 (幅, 高さ)。"""
        key = (text, size, tuple(color), max_width, align)
        if key == self.key:
            return self.size
        self.key = key
        if not font() or not text:
            VIEW.mesh_set(self.text.mid, __import__("numpy").zeros((0, 12), "float32"), __import__("numpy").zeros((0, 3), "int64"))
            if self.bg is not None:
                self.bg.np.hide()
            self.size = (0.0, 0.0)
            return self.size
        w, h = VIEW.text_mesh(self.text.mid, str(text), float(size), [float(x) for x in color], float(max_width), int(align))
        VIEW.mesh_props(self.text.mid, True, False, True, 2 if not self.overlay else 0, self.bin + 1)
        if self.bg is not None:
            self.bg.np.show()
            quad(self.bg, -pad, pad, w + pad, -h - pad, self.bgc)
            VIEW.mesh_props(self.bg.mid, self.bgc[3] < 0.999, not self.overlay, True, 0, self.bin)
        self.size = (w, h)
        return self.size

    def place(self, x, y):
        self.node.setPos(float(x), float(y), 0.0)

    def show(self, on=True):
        (self.node.show if on else self.node.hide)()

    def remove(self):
        self.node.removeNode()


class RsDisplayView:
    """コンピューターの表示（看板）の Rust の画面版（render.DisplayView と同じ大きさ・置き方）。"""

    def __init__(self, parent):
        self.root = RsNode(parent)
        self.inner = RsNode(self.root)
        self.bg = RsDynamicMesh(self.root, "display_bg")
        self.txt = RsText(self.inner, overlay=False)
        self.key = None
        self.placed = None

    def set(self, disp, voxel_size):
        key = (disp["text"], disp.get("width"), tuple(disp.get("color", ())), tuple(disp.get("background", ())), voxel_size)
        if key == self.key:
            return
        self.key = key
        col = disp.get("color", [1, 1, 1])
        bgc = disp.get("background", [0.04, 0.05, 0.08, 0.92])
        tw, th = self.txt.set(disp["text"], 1.0, (col[0], col[1], col[2], 1.0))
        tw, th = max(tw, 1e-6), max(th, 1e-6)
        w_target = float(disp.get("width") or voxel_size * 0.96)
        k = w_target * 0.9 / tw
        h = th * k / 0.88
        if disp.get("width") is None:
            if h > voxel_size * 0.96:
                k *= voxel_size * 0.96 / h
                h = voxel_size * 0.96
            h = max(h, voxel_size * 0.96)
        # 文字：左揃え・全体は中央（文字の座標は 1行目の上が 0、下へ負）。看板の表は -Z の向き（Panda3D 版と同じ）
        M = [k, 0, 0, 0, 0, k, 0, 0, 0, 0, k, 0, -tw * k / 2, th * k / 2, -0.004, 1]
        self.inner.setMat(M)
        quad(self.bg, -w_target / 2, -h / 2, w_target / 2, h / 2, (bgc[0], bgc[1], bgc[2], 1.0))

    def place(self, center, right, up, fwd):
        self.root.setMat([right[0], right[1], right[2], 0, up[0], up[1], up[2], 0, fwd[0], fwd[1], fwd[2], 0,
                          center[0], center[1], center[2], 1])

    def remove(self):
        self.root.removeNode()
