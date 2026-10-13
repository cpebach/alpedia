"""読み込みメニュー（F8）：OBJ・構築物データを選んで、大きさや必要な資源を確かめてから置く。

左：読み込めるファイルの一覧（このワールドの imports・exports、ほかのワールドの exports）
右：選んだものの情報。外部の OBJ は大きさ（いちばん長い辺）・細かさ（ボクセルの大きさ）・素材を指定できる。
    サバイバルでは必要な資源と手持ちを並べて表示する。
OK で目の前に置き、閉じるボタン（または Esc）で何もせず閉じる。
"""
import os

import numpy as np

from .. import log, objio, paths

L = log.get("import_ui")

ROWS = 13
VOXEL_SIZES = (0.25, 0.5, 1.0)
WARN_VOXELS = 20000


def _fmt_len(x):
    return f"{x:.2f}" if x < 10 else f"{x:.1f}"


class ImportPanel:
    def __init__(self, game):
        from ursina import Entity, Text, Button, camera, color, mouse
        self.game = game
        self.world = game.world
        self.reg = game.world.registry
        self.U = dict(Entity=Entity, Text=Text, Button=Button, color=color)
        mouse.locked = False
        self.page = 0
        self.sel = None
        self.info = None
        self.data = None
        self.error = ""
        self.status = ""
        self.voxel_size = float(self.world.settings.get("voxel_size", 0.5))
        self.longest = None
        self.material = "steel"
        self._data_key = None

        self.root = Entity(parent=camera.ui, z=-0.3)
        bg = Button(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0, 0, 0, 0.55),
                    highlight_color=color.rgba(0, 0, 0, 0.55), pressed_color=color.rgba(0, 0, 0, 0.55), z=0.1)
        bg.on_click = lambda: None
        Entity(parent=self.root, model="quad", scale=(1.56, 0.92), position=(0, 0, 0.05),
               color=color.rgb(0.07, 0.08, 0.11))
        Text(parent=self.root, text="読み込み（OBJ・構築物）", position=(-0.75, 0.425), origin=(-0.5, 0.5), scale=1.3)
        self.list_root = Entity(parent=self.root)
        self.ctrl_root = Entity(parent=self.root)
        self.detail = Text(parent=self.root, text="", position=(-0.17, 0.36), origin=(-0.5, 0.5), scale=0.72,
                           color=color.light_gray)
        self.cost_text = Text(parent=self.root, text="", position=(-0.17, -0.2), origin=(-0.5, 0.5), scale=0.72)
        self.status_text = Text(parent=self.root, text="", position=(-0.75, -0.395), origin=(-0.5, 0.5),
                                scale=0.7, color=color.yellow)
        self.ok_btn = self._btn("OK（目の前に置く）", (0.42, -0.4), self.ok, w=0.26, col=color.rgb(0.2, 0.5, 0.3))
        self._btn("閉じる", (0.66, -0.4), self.close, w=0.16, col=color.rgb(0.3, 0.3, 0.33))
        self._btn("imports を開く", (-0.62, -0.335), self.open_folder, w=0.2, col=color.rgb(0.25, 0.28, 0.35))
        self._btn("一覧を更新", (-0.4, -0.335), self.rescan, w=0.17, col=color.rgb(0.25, 0.28, 0.35))
        panel = self

        class Keys(Entity):
            def input(self_, key):
                if key == "escape":
                    panel.close()
                elif key == "scroll up":
                    panel.set_page(panel.page - 1)
                elif key == "scroll down":
                    panel.set_page(panel.page + 1)
        self.keys = Keys(parent=self.root)
        self.rescan()

    # ------------------------------------------------------------------
    def _btn(self, text, pos, fn, w=0.1, h=0.042, col=None, parent=None, ts=0.7):
        B, color = self.U["Button"], self.U["color"]
        b = B(parent=parent or self.root, text=text, position=pos, scale=(w, h),
              color=col or color.rgb(0.22, 0.32, 0.45), text_size=ts)
        b.on_click = fn
        return b

    def _text(self, text, pos, parent, scale=0.7, col=None, origin=(-0.5, 0)):
        T, color = self.U["Text"], self.U["color"]
        return T(parent=parent, text=text, position=pos, origin=origin, scale=scale, color=col or color.light_gray)

    def _clear(self, ent):
        from .ui_destroy import destroy
        for ch in list(ent.children):
            destroy(ch)

    # --- 一覧 -----------------------------------------------------------
    def rescan(self):
        self.sources = objio.list_sources(self.world.dir, paths.SAVES)
        cur = self.sources[self.sel][1] if self.sel is not None and self.sel < len(self.sources) else None
        self.sel = None
        if cur is not None:
            for k, (_, p) in enumerate(self.sources):
                if p == cur:
                    self.sel = k
        if self.sel is None and self.sources:
            self.select(0)
        else:
            self.build_list()
            self.refresh()

    def set_page(self, page):
        n_pages = max(1, (len(self.sources) + ROWS - 1) // ROWS)
        page = max(0, min(n_pages - 1, page))
        if page != self.page:
            self.page = page
            self.build_list()

    def build_list(self):
        color = self.U["color"]
        self._clear(self.list_root)
        x0, y0 = -0.5, 0.35
        if not self.sources:
            self._text("読み込めるファイルがありません。\n\n"
                       "saves/<ワールド>/imports に .obj（と .alp.json）を\n置いて「一覧を更新」を押してください。\n"
                       "F9 で出力した構築物は exports に入り、\nここに自動で出てきます。",
                       (-0.74, 0.3), self.list_root, scale=0.68, origin=(-0.5, 0.5))
            return
        start = self.page * ROWS
        last_group = None
        for row, k in enumerate(range(start, min(start + ROWS, len(self.sources)))):
            group, p = self.sources[k]
            y = y0 - row * 0.05
            label = p.name if len(p.name) <= 30 else p.name[:28] + "…"
            if group != last_group:
                label = f"[{group}] " + label
                last_group = group
            sel = k == self.sel
            b = self._btn(label, (x0 - 0.0, y), (lambda k=k: self.select(k)), w=0.5, h=0.044,
                          col=color.rgb(0.3, 0.45, 0.65) if sel else color.rgb(0.16, 0.18, 0.23),
                          parent=self.list_root, ts=0.62)
            b.text_entity.origin = (-0.5, 0)
            b.text_entity.x = -0.47
        n_pages = (len(self.sources) + ROWS - 1) // ROWS
        if n_pages > 1:
            yb = y0 - ROWS * 0.05
            self._btn("▲", (-0.66, yb), lambda: self.set_page(self.page - 1), w=0.06, parent=self.list_root)
            self._text(f"{self.page + 1}/{n_pages}", (-0.6, yb), self.list_root)
            self._btn("▼", (-0.34, yb), lambda: self.set_page(self.page + 1), w=0.06, parent=self.list_root)

    def select(self, k):
        self.sel = k
        self.page = k // ROWS
        self.info, self.data, self.error, self._data_key = None, None, "", None
        self.status = ""
        group, p = self.sources[k]
        try:
            self.info = objio.inspect(self.reg, p)
        except Exception as e:  # noqa: BLE001
            L.exception("読み込みの下調べに失敗: %s", p)
            self.error = f"このファイルは読み込めません: {e}"
        if self.info and self.info["kind"] == "obj":
            inf = self.info
            orig = float(inf["extent"].max()) or 1.0
            self.voxel_size = float(inf["voxel_size"] or self.world.settings.get("voxel_size", 0.5))
            if inf["meta"].get("scale") is not None:
                self.longest = orig * inf["scale"]
            elif 0.5 <= orig <= 40:
                self.longest = orig
            else:
                self.longest = 8.0      # mm 単位で作られた OBJ などはとりあえず 8 m に
            self.material = inf["default_material"] if self.reg.get("materials", inf["default_material"] or "") \
                else "steel"
        self.build_list()
        self.build_controls()
        self.refresh()

    # --- 大きさ・素材の指定（外部 OBJ のみ） -------------------------------
    def build_controls(self):
        color = self.U["color"]
        self._clear(self.ctrl_root)
        if not self.info or self.info["kind"] != "obj":
            return
        y = -0.03
        R = self.ctrl_root
        self._text("大きさ（いちばん長い辺）", (-0.17, y + 0.045), R, col=color.white)
        self.len_text = self._text("", (-0.17, y), R, scale=0.8, col=color.white)
        for k, (lab, fn) in enumerate((("×½", lambda: self.resize(0.5)), ("−10%", lambda: self.resize(1 / 1.1)),
                                       ("+10%", lambda: self.resize(1.1)), ("×2", lambda: self.resize(2.0)),
                                       ("元の大きさ", self.reset_size))):
            self._btn(lab, (0.0 + k * 0.085 + (0.02 if k == 4 else 0), y), fn, w=0.08 if k < 4 else 0.1, parent=R)
        y -= 0.065
        self._text("細かさ（ボクセル）", (-0.17, y), R, col=color.white)
        for k, v in enumerate(VOXEL_SIZES):
            on = abs(v - self.voxel_size) < 1e-9
            self._btn(f"{v:g} m", (0.085 + k * 0.085, y), (lambda v=v: self.set_voxel(v)), w=0.08, parent=R,
                      col=color.rgb(0.3, 0.45, 0.65) if on else color.rgb(0.2, 0.22, 0.28))
        y -= 0.06
        self._text("素材", (-0.17, y), R, col=color.white)
        self._btn("◀", (0.0, y), lambda: self.cycle_material(-1), w=0.05, parent=R)
        self.mat_text = self._text("", (0.035, y), R, col=color.white)
        self._btn("▶", (0.27, y), lambda: self.cycle_material(1), w=0.05, parent=R)

    def resize(self, f):
        self.longest = float(np.clip(self.longest * f, 0.2, 500.0))
        self._changed()

    def reset_size(self):
        self.longest = float(self.info["extent"].max())
        self._changed()

    def set_voxel(self, v):
        self.voxel_size = v
        self.build_controls()
        self._changed()

    def cycle_material(self, d):
        ids = [m for m in self.reg.defs["materials"]]
        k = ids.index(self.material) if self.material in ids else 0
        self.material = ids[(k + d) % len(ids)]
        self._changed()

    def _changed(self):
        self.data, self._data_key = None, None
        self.status = ""
        self.refresh()

    # --- 中身の計算 -------------------------------------------------------
    def _prepare(self):
        """選んだもの・指定した大きさでボクセルを作る（結果は覚えておく）。"""
        if not self.info or self.error:
            return None
        inf = self.info
        if inf["kind"] == "voxels":
            key = ("v", str(self.sources[self.sel][1]))
            opts = {}
        else:
            scale = self.longest / max(float(inf["extent"].max()), 1e-9)
            key = ("o", str(self.sources[self.sel][1]), round(scale, 6), self.voxel_size, self.material)
            opts = {"voxel_size": self.voxel_size, "scale": scale, "default_material": self.material}
        if key != self._data_key:
            self._data_key = key
            self.data = None
            try:
                self.data = objio.import_construct(self.reg, self.sources[self.sel][1], **opts)
                self._data_err = ""
            except Exception as e:  # noqa: BLE001
                L.warning("ボクセル化できません: %s", e)
                self._data_err = str(e)
        return self.data

    def refresh(self):
        color = self.U["color"]
        lines, cost_lines = [], []
        self.can_place = False
        if self.sel is None or not self.sources:
            self.detail.text = "左の一覧から置くものを選んでください"
        elif self.error:
            self.detail.text = self.error
        else:
            group, p = self.sources[self.sel]
            inf = self.info
            data = self._prepare()
            lines.append(f"名前: {inf['name']}")
            lines.append(f"場所: {group}/{p.name}")
            if inf["kind"] == "voxels":
                lines.append("種類: ゲーム内で作った構築物（ボクセル・機器をそのまま再現）")
                n_sc = len((data or {}).get("scripts") or {})
                if n_sc:
                    lines.append(f"プログラム {n_sc} 個も一緒に入ります（scripts フォルダへ）")
            else:
                ex = inf["extent"]
                lines.append(f"種類: 外部の OBJ（三角形 {inf['n_tris']} 個をボクセルに変換）")
                lines.append(f"元の大きさ: {_fmt_len(ex[0])} × {_fmt_len(ex[1])} × {_fmt_len(ex[2])}（OBJ の単位）")
                if inf["mapping"]:
                    lines.append("素材の対応: " + "、".join(f"{k}→{self.reg.material(v)['name']}"
                                                       for k, v in list(inf["mapping"].items())[:4]))
                self.len_text.text = f"{_fmt_len(self.longest)} m"
                self.mat_text.text = self.reg.material(self.material)["name"] + \
                    ("（対応表に無い面）" if inf["mapping"] else "")
            if data is None:
                lines.append(f"※ ボクセルにできません: {self._data_err}")
            else:
                vox = data["voxels"]
                if not vox:
                    lines.append("※ ボクセルが 0 個です（小さすぎるか、閉じていないメッシュです）")
                else:
                    cells = np.array([v[0] for v in vox])
                    ext = (cells.max(0) - cells.min(0) + 1) * data["size"]
                    n_dev = sum(1 for v in vox if v[2])
                    lines.append(f"置いたときの大きさ: {_fmt_len(ext[0])} × {_fmt_len(ext[1])} × {_fmt_len(ext[2])} m"
                                 f"（ボクセル {data['size']:g} m）")
                    lines.append(f"ボクセル {len(vox)} 個" + (f"・機器 {n_dev} 個" if n_dev else ""))
                    mats = {}
                    for _, m, d in vox:
                        if not d:
                            mats[m] = mats.get(m, 0) + 1
                    if mats:
                        top = sorted(mats.items(), key=lambda kv: -kv[1])[:5]
                        lines.append("素材: " + "、".join(f"{self.reg.material(m)['name']} {n}" for m, n in top))
                    if len(vox) > WARN_VOXELS:
                        lines.append(f"※ ボクセルが多いので動きが重くなります（{WARN_VOXELS} 個までがおすすめ）")
                    self.can_place = True
                    if not self.world.settings.get("creative", True):
                        cost = self.world.voxels_cost(vox)
                        inv = self.world.player.inventory
                        cost_lines.append("必要な資源（サバイバル）")
                        if not cost:
                            cost_lines.append("　なし")
                        for k, v in cost.items():
                            have = inv.get(k, 0.0)
                            ok = have >= v
                            cost_lines.append(f"・{self.reg.res_name(k)} {v:.0f}（手持ち {have:.0f}）"
                                              + ("　足りています" if ok else f"　{v - have:.0f} 足りません"))
                            if not ok:
                                self.can_place = False
                    else:
                        cost_lines.append("クリエイティブ：資源は要りません")
            self.detail.text = "\n".join(lines).replace("<", "＜").replace(">", "＞")
        self.cost_text.text = "\n".join(cost_lines)
        self.cost_text.color = color.white
        self.ok_btn.color = color.rgb(0.2, 0.5, 0.3) if self.can_place else color.rgb(0.25, 0.3, 0.27)
        self.status_text.text = self.status

    # --- ボタン -----------------------------------------------------------
    def ok(self):
        if not self.can_place or self.data is None:
            self.status = "置けません（上の ※ や「足りません」を確認してください）"
            self.refresh()
            return
        g = self.game
        _, _, _, look = g.look_vectors()
        c = self.world.place_import(self.data, look_dir=look)
        if c is None:
            self.status = self.world.msgs[-1][1] if self.world.msgs else "置けませんでした"
            self.refresh()
            return
        self.close()

    def open_folder(self):
        d = self.world.dir / "imports"
        d.mkdir(exist_ok=True)
        try:
            if hasattr(os, "startfile"):
                os.startfile(str(d))  # noqa: S606（Windows のエクスプローラーで開く）
                self.status = "imports フォルダを開きました。ファイルを置いたら「一覧を更新」を押してください"
            else:
                self.status = f"フォルダ: {d}"
        except Exception as e:  # noqa: BLE001
            self.status = f"フォルダを開けません: {e}"
        self.refresh()

    def close(self):
        from .ui_destroy import destroy
        destroy(self.root)
        self.game.editor_closed()
