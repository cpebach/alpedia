"""コンピューターのブロック（ノード）プログラミング画面。スクラッチのようにブロックを並べて作る。

- 左のパレットでブロックを選ぶと、選択中の行の下（帽子ブロックや「ここに追加」を選んでいればその中）に入る
- 白い入力欄をクリックすると、数値・センサー・計算・比較・変数などを選べる
- 保存すると scripts/<名前>.blocks.json と、変換した <名前>.py ができ、コンピューターは .py を実行する
- 「Python をコピー」で変換したコードをクリップボードへ
"""
import copy
import time

from .. import blocks as B, log, scripting

L = log.get("blocks")

ROW_H = 0.037
VISIBLE = 22
AREA_X0, AREA_X1 = -0.50, 0.44
AREA_TOP = 0.42
NAME_CHARS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
TEXT_CHARS = NAME_CHARS + " .,:;!?+-*/=()[]%#@&'\"<>"


def _dim(c, k=0.6):
    return tuple(x * k for x in c)


class Row:
    def __init__(self, kind, depth, parent=None, index=None, node=None, text=""):
        self.kind, self.depth, self.parent, self.index, self.node, self.text = kind, depth, parent, index, node, text


class BlockEditor:
    def __init__(self, game, construct, index):
        from ursina import Entity, Text, Button, InputField, camera, color, mouse
        self.game = game
        self.world = game.world
        self.c = construct
        self.cell = construct.cells[index]
        def SafeText(**kw):
            kw["text"] = _safe(kw.get("text", ""))
            return Text(**kw)
        self.U = dict(Entity=Entity, Text=SafeText, Button=Button, InputField=InputField, color=color)
        mouse.locked = False
        self.cat = "event"
        self.scroll = 0
        self.sel = None          # (parent list, index, kind)
        self.dirty = False
        self.discard_armed = False
        self.dialogs = []
        self.root = Entity(parent=camera.ui)
        Entity(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0.03, 0.035, 0.05, 0.95), z=0.5)
        Entity(parent=self.root, model="quad", position=((AREA_X0 + AREA_X1) / 2, 0.02, 0.4),
               scale=(AREA_X1 - AREA_X0 + 0.02, 0.86), color=color.rgb(0.08, 0.09, 0.12))
        self.title = Text(parent=self.root, text="", position=(-0.87, 0.485), origin=(-0.5, 0.5), scale=1.0)
        self.palette_root = Entity(parent=self.root)
        self.rows_root = Entity(parent=self.root)
        self.dialog_root = Entity(parent=self.root, z=-0.2)
        self._build_right_panel()
        self._load()

        class Keys(Entity):
            def input(self_, key):
                self.input(key)

            def update(self_):
                self.update()
        self.keys = Keys(parent=self.root)
        self.build_palette()
        self.render()

    # ------------------------------------------------------------------
    # 読み込み・保存
    # ------------------------------------------------------------------
    @property
    def index(self):
        return self.c.index.get(self.cell)

    def device(self):
        i = self.index
        return None if i is None else self.c.devs[i]

    def scripts_dir(self):
        d = self.world.dir / "scripts"
        d.mkdir(exist_ok=True)
        return d

    def _load(self):
        d = self.device()
        name = (d or {}).get("state", {}).get("blocks", "")
        self.prog = None
        if name:
            try:
                self.prog = B.load(self.scripts_dir() / name)
                self.base = name[:-len(".blocks.json")]
            except Exception as e:  # noqa: BLE001
                self._status(f"{name} を読めません: {e}")
        if self.prog is None:
            self.prog = B.new_program()
            self.prog["scripts"].append(B.new_stmt("on_start"))
            self.prog["scripts"].append(B.new_stmt("on_tick"))
            k = 1
            while (self.scripts_dir() / f"blocks{k}.blocks.json").exists():
                k += 1
            self.base = f"blocks{k}"
            self.dirty = True
            if d and d["state"].get("script"):
                self._status(f"新しいブロックプログラムです（今の {d['state']['script']} は変換されません）")
        self.name_field.text_field.text = self.base
        self.name_field.text_field.render()

    def open_next_file(self):
        """scripts/ にあるブロックプログラムを順に開く（保存するとこのコンピューターに設定される）。"""
        files = sorted(p.name for p in self.scripts_dir().glob("*.blocks.json"))
        if not files:
            self._status("scripts/ にブロックプログラムがありません")
            return
        cur = f"{self.base}.blocks.json"
        nxt = files[(files.index(cur) + 1) % len(files)] if cur in files else files[0]
        try:
            self.prog = B.load(self.scripts_dir() / nxt)
        except Exception as e:  # noqa: BLE001
            self._status(f"{nxt} を読めません: {e}")
            return
        self.base = nxt[:-len(".blocks.json")]
        self.name_field.text_field.text = self.base
        self.name_field.text_field.render()
        self.sel = None
        self.scroll = 0
        d = self.device()
        self.dirty = not (d and d["state"].get("blocks") == nxt)
        self._status(f"{nxt} を開きました" + ("（保存するとこのコンピューターに設定されます）" if self.dirty else ""))
        self.render()

    def file_base(self):
        n = "".join(ch for ch in self.name_field.text_field.text.strip() if ch in NAME_CHARS)
        return n or "blocks"

    def code(self):
        return B.to_python(self.prog, f"{self.file_base()}.blocks.json")

    def save(self):
        d = self.device()
        if d is None:
            self._status("コンピューターが見つかりません")
            return False
        base = self.file_base()
        try:
            py = scripting.script_path(self.world.dir, base + ".py")
        except ValueError as e:
            self._status(str(e))
            return False
        code = self.code()
        try:
            compile(code, py.name, "exec")
        except SyntaxError as e:  # 変換の不具合（通常は起きない）
            self._status(f"変換エラー: {e}")
            return False
        B.save(self.scripts_dir() / f"{base}.blocks.json", self.prog)
        py.write_text(code, encoding="utf-8")
        self.world.runner.invalidate(py.name)
        st = d["state"]
        st["script"] = py.name
        st["blocks"] = f"{base}.blocks.json"
        for k in ("_env", "error", "error_line"):
            st.pop(k, None)
        self.base = base
        self.dirty = False
        self._status(f"{base}.blocks.json と {py.name} を保存しました")
        L.info("ブロックプログラム保存: %s", base)
        return True

    def save_and_run(self):
        if self.save():
            self.device()["state"]["running"] = True
            self._status("保存して実行しました")

    def stop(self):
        d = self.device()
        if d:
            d["state"]["running"] = False
            self._status("停止しました")

    def copy_python(self):
        code = self.code()
        try:
            import pyperclip
            pyperclip.copy(code)
            self._status(f"Python コード（{len(code.splitlines())} 行）をクリップボードにコピーしました")
        except Exception as e:  # noqa: BLE001
            L.warning("クリップボードにコピーできません: %s", e)
            self._status("クリップボードにコピーできませんでした（保存すると scripts/ に .py ができます）")

    def open_python(self):
        if self.dirty and not self.save():
            return
        game, c, i = self.game, self.c, self.index
        self.close(force=True)
        from .editor import ScriptEditor
        game.U["mouse"].locked = False
        game.ui_root.enabled = False
        game.editor = ScriptEditor(game, c, i)
        game.editor._set_status("ブロックから変換したコードです。ここで保存すると Python モードになります")

    def close(self, force=False):
        if self.dirty and not force and not self.discard_armed:
            self.discard_armed = True
            self._status("未保存の変更があります。保存するか、もう一度 Esc で破棄して閉じます")
            return
        from .ui_destroy import destroy
        destroy(self.root)
        self.game.editor_closed()

    # ------------------------------------------------------------------
    # 右パネル
    # ------------------------------------------------------------------
    def _btn(self, parent, text, pos, fn, w=0.19, h=0.04, col=None, ts=0.7):
        U = self.U
        b = U["Button"](parent=parent, text=_safe(text), position=pos, scale=(w, h),
                        color=col or U["color"].rgb(0.22, 0.32, 0.45), text_size=ts)
        b.on_click = fn
        return b

    def _build_right_panel(self):
        U = self.U
        col = U["color"]
        r = self.root
        x = 0.665
        self._btn(r, "このコンピューターの名前を変更", (x, 0.475), self.open_rename, w=0.4, h=0.035)
        U["Text"](parent=r, text="ファイル名", position=(x - 0.2, 0.43), origin=(-0.5, 0.5), scale=0.7)
        self._btn(r, "別のファイルを開く ▶", (x + 0.1, 0.422), self.open_next_file, w=0.2, h=0.032, ts=0.6)
        self.name_field = U["InputField"](parent=r, default_value="", position=(x, 0.38), scale=(0.4, 0.04),
                                          character_limit=30)
        self.name_field.limit_content_to = NAME_CHARS
        self.name_field.active = False
        self._btn(r, "保存", (x - 0.1, 0.32), self.save, col=col.rgb(0.2, 0.45, 0.3))
        self._btn(r, "保存して実行", (x + 0.1, 0.32), self.save_and_run, col=col.rgb(0.2, 0.5, 0.3))
        self._btn(r, "停止", (x - 0.1, 0.275), self.stop, col=col.rgb(0.5, 0.25, 0.2))
        self._btn(r, "Python をコピー", (x + 0.1, 0.275), self.copy_python)
        self._btn(r, "Python で開く", (x - 0.1, 0.23), self.open_python)
        self._btn(r, "閉じる", (x + 0.1, 0.23), self.close, col=col.rgb(0.3, 0.3, 0.33))
        U["Text"](parent=r, text="選んだブロック", position=(x - 0.2, 0.18), origin=(-0.5, 0.5), scale=0.7)
        self._btn(r, "▲ 上へ", (x - 0.1, 0.135), lambda: self.move(-1))
        self._btn(r, "▼ 下へ", (x + 0.1, 0.135), lambda: self.move(1))
        self._btn(r, "複製", (x - 0.1, 0.09), self.duplicate)
        self._btn(r, "削除", (x + 0.1, 0.09), self.delete, col=col.rgb(0.5, 0.2, 0.2))
        self.status = U["Text"](parent=r, text="", position=(x - 0.2, 0.045), origin=(-0.5, 0.5), scale=0.62,
                                color=col.orange)
        self.info = U["Text"](parent=r, text="", position=(x - 0.2, -0.04), origin=(-0.5, 0.5), scale=0.62)
        self.log_text = U["Text"](parent=r, text="", position=(x - 0.2, -0.17), origin=(-0.5, 0.5), scale=0.58,
                                  color=col.light_gray)
        U["Text"](parent=r, position=(-0.87, -0.455), origin=(-0.5, 0.5), scale=0.6, color=col.light_gray,
                  text="左のブロックを押すと、選んだ行の下（「＋ここに追加」を選べばその中）に入ります。"
                       "白い枠をクリックすると値やセンサー・計算を選べます。 Ctrl+S 保存 / Ctrl+R 保存して実行 / Esc 閉じる")

    # ------------------------------------------------------------------
    # パレット
    # ------------------------------------------------------------------
    def build_palette(self):
        from .ui_destroy import destroy
        for e in list(self.palette_root.children):
            destroy(e)
        U = self.U
        col = U["color"]
        cats = [c for c in B.CATEGORIES if c[0] not in ("op", "sensor")]
        for k, (cid, name, rgb) in enumerate(cats):
            x = -0.79 + (k % 2) * 0.17
            y = 0.42 - (k // 2) * 0.042
            c = col.rgb(*rgb) if cid == self.cat else col.rgb(*_dim(rgb, 0.45))
            self._btn(self.palette_root, name, (x, y), (lambda cc=cid: self._set_cat(cc)), w=0.165, h=0.036,
                      col=c, ts=0.65)
        y = 0.42 - ((len(cats) + 1) // 2) * 0.042 - 0.024
        rgb = B.CAT_COLOR[self.cat]
        for kind, spec in B.STATEMENTS.items():
            if spec["cat"] != self.cat:
                continue
            label = "".join(("□" if t == "slot" else s) for t, s in B.label_parts(spec["label"]))
            if spec.get("hat"):
                label = "▶ " + label
            b = self._btn(self.palette_root, label, (-0.705, y), (lambda kk=kind: self.insert(kk)), w=0.34, h=0.036,
                          col=col.rgb(*rgb), ts=0.6)
            b.text_entity.origin = (-0.5, 0)
            b.text_entity.x = -0.47
            y -= 0.041
        U["Text"](parent=self.palette_root, position=(-0.87, y - 0.01), origin=(-0.5, 0.5), scale=0.58,
                  color=col.light_gray,
                  text="数値・センサー・計算・比較は\nブロックの白い枠をクリック")

    def _set_cat(self, cid):
        self.cat = cid
        self.build_palette()

    # ------------------------------------------------------------------
    # プログラムの行
    # ------------------------------------------------------------------
    def flatten(self):
        rows = []

        def body(lst, depth, owner):
            for i, n in enumerate(lst):
                rows.append(Row("block", depth, lst, i, n))
                spec = B.STATEMENTS.get(n.get("type"), {})
                if spec.get("body"):
                    body(n["body"], depth + 1, n)
                    if spec.get("orelse"):
                        rows.append(Row("label", depth, node=n, text="でなければ"))
                        body(n["else"], depth + 1, n)
            rows.append(Row("end", depth, lst, len(lst), owner))

        for i, s in enumerate(self.prog["scripts"]):
            rows.append(Row("block", 0, self.prog["scripts"], i, s))
            body(s["body"], 1, s)
            rows.append(Row("gap", 0))
        rows.append(Row("end", 0, self.prog["scripts"], len(self.prog["scripts"]), None,
                        text="＋ 新しいスクリプト（イベントのブロックを押す）"))
        return rows

    def render(self):
        from .ui_destroy import destroy
        for e in list(self.rows_root.children):
            destroy(e)
        U = self.U
        col = U["color"]
        rows = self.flatten()
        self.rows = rows
        self.scroll = max(0, min(self.scroll, max(0, len(rows) - VISIBLE)))
        y = AREA_TOP
        for r in rows[self.scroll:self.scroll + VISIBLE]:
            x0 = AREA_X0 + r.depth * 0.035
            w = AREA_X1 - x0
            selected = self.sel is not None and r.parent is self.sel[0] and r.index == self.sel[1] \
                and r.kind == self.sel[2]
            if r.kind == "gap":
                y -= ROW_H * 0.6
                continue
            if r.kind == "end":
                txt = r.text or "＋ ここに追加"
                c = col.rgb(0.35, 0.4, 0.5) if selected else col.rgb(0.12, 0.13, 0.17)
                b = U["Button"](parent=self.rows_root, position=(x0 + w / 2, y), scale=(w, ROW_H * 0.8), color=c,
                                text=_safe(txt), text_size=0.55)
                b.text_entity.origin = (-0.5, 0)
                b.text_entity.x = -0.48
                b.text_entity.color = col.light_gray
                b.on_click = (lambda rr=r: self.select(rr))
                y -= ROW_H
                continue
            if r.kind == "label":
                rgb = B.CAT_COLOR[B.STATEMENTS[r.node["type"]]["cat"]]
                U["Entity"](parent=self.rows_root, model="quad", position=(x0 + w / 2, y), scale=(w, ROW_H * 0.85),
                            color=col.rgb(*_dim(rgb, 0.8)))
                U["Text"](parent=self.rows_root, text=r.text, position=(x0 + 0.01, y), origin=(-0.5, 0), scale=0.75,
                          z=-0.01)
                y -= ROW_H
                continue
            n = r.node
            spec = B.STATEMENTS.get(n.get("type"), {"label": n.get("type", "?"), "cat": "control"})
            rgb = B.CAT_COLOR[spec["cat"]]
            c = col.rgb(*[min(1, v * 1.25) for v in rgb]) if selected else col.rgb(*_dim(rgb, 0.8))
            bg = U["Button"](parent=self.rows_root, position=(x0 + w / 2, y), scale=(w, ROW_H * 0.88), color=c)
            bg.on_click = (lambda rr=r: self.select(rr))
            if selected:
                U["Entity"](parent=self.rows_root, model="quad", position=(x0 - 0.006, y), scale=(0.006, ROW_H * 0.88),
                            color=col.white)
            x = x0 + 0.01
            parts = B.label_parts(spec["label"])
            if spec.get("hat"):
                parts = [("text", "▶ ")] + parts
            for kind, s in parts:
                if kind == "text":
                    t = U["Text"](parent=self.rows_root, text=s, position=(x, y), origin=(-0.5, 0), scale=0.75, z=-0.01)
                    x += t.width * 0.75
                else:
                    x += self._slot(n, s, B.slot_kind(n["type"], s), x, y) + 0.006
            y -= ROW_H
        # スクロール位置
        if len(rows) > VISIBLE:
            U["Text"](parent=self.rows_root, text=f"{self.scroll + 1}〜{min(len(rows), self.scroll + VISIBLE)} / {len(rows)} 行"
                      "（ホイールでスクロール）", position=(AREA_X1, -0.43), origin=(0.5, 0), scale=0.55,
                      color=col.light_gray)
        self._update_title()

    def _slot(self, node, key, kind, x, y):
        U = self.U
        col = U["color"]
        txt = B.summary(node.get(key))
        if len(txt) > 34:
            txt = txt[:33] + "…"
        t = U["Text"](parent=self.rows_root, text=txt, position=(x + 0.008, y), origin=(-0.5, 0), scale=0.68, z=-0.02,
                      color=col.black)
        w = t.width * 0.68 + 0.016
        bg_col = col.rgb(0.95, 0.95, 0.95) if not kind.startswith(("dev", "key", "choice", "var")) \
            else col.rgb(0.85, 0.9, 1.0)
        b = U["Button"](parent=self.rows_root, position=(x + w / 2, y), scale=(w, ROW_H * 0.7), color=bg_col, z=-0.01)
        b.on_click = (lambda: self.open_dialog(node, key, kind))
        return w

    def select(self, row):
        self.sel = (row.parent, row.index, row.kind)
        self.render()

    def _update_title(self):
        d = self.device()
        if d is None:
            self.title.text = "コンピューターが失われました"
            return
        st = d["state"]
        run = "実行中" if st.get("running") else "停止中"
        mark = " ●未保存" if self.dirty else ""
        self.title.text = _safe(f"ブロックプログラム — {d['name']}（{self.c.name}） [{self.file_base()}]  {run}{mark}")

    # ------------------------------------------------------------------
    # 編集
    # ------------------------------------------------------------------
    def changed(self):
        self.dirty = True
        self.discard_armed = False
        self.render()

    def insert(self, kind):
        spec = B.STATEMENTS[kind]
        node = B.new_stmt(kind)
        dev_kind = None
        for s, k in spec.get("slots", {}).items():
            if k.startswith("dev:"):
                dev_kind = k[4:]
                names = self.device_names(dev_kind)
                if len(names) == 1:
                    node[s] = names[0]
        scripts = self.prog["scripts"]
        if spec.get("hat"):
            scripts.append(node)
            self.sel = (scripts, len(scripts) - 1, "block")
        else:
            target, idx = None, None
            if self.sel:
                lst, i, k = self.sel
                if k == "end" and lst is not scripts:
                    target, idx = lst, i
                elif k == "block" and lst is scripts and i < len(scripts):
                    target, idx = scripts[i]["body"], 0
                elif k == "block" and i < len(lst):
                    target, idx = lst, i + 1
            if target is None:
                if not scripts:
                    scripts.append(B.new_stmt("on_tick"))
                target, idx = scripts[-1]["body"], len(scripts[-1]["body"])
            target.insert(idx, node)
            self.sel = (target, idx, "block")
        self._ensure_visible()
        self.changed()

    def _ensure_visible(self):
        rows = self.flatten()
        for k, r in enumerate(rows):
            if self.sel and r.parent is self.sel[0] and r.index == self.sel[1] and r.kind == self.sel[2]:
                if k < self.scroll:
                    self.scroll = k
                elif k >= self.scroll + VISIBLE:
                    self.scroll = k - VISIBLE + 2
                return

    def _selected_block(self):
        if not self.sel or self.sel[2] != "block":
            self._status("ブロックを選んでください")
            return None
        lst, i, _ = self.sel
        return (lst, i) if i < len(lst) else None

    def move(self, d):
        sb = self._selected_block()
        if not sb:
            return
        lst, i = sb
        j = i + d
        if 0 <= j < len(lst):
            lst[i], lst[j] = lst[j], lst[i]
            self.sel = (lst, j, "block")
            self.changed()

    def duplicate(self):
        sb = self._selected_block()
        if not sb:
            return
        lst, i = sb
        lst.insert(i + 1, copy.deepcopy(lst[i]))
        self.sel = (lst, i + 1, "block")
        self.changed()

    def delete(self):
        sb = self._selected_block()
        if not sb:
            return
        lst, i = sb
        del lst[i]
        self.sel = (lst, min(i, len(lst) - 1), "block") if lst else None
        self.changed()

    # ------------------------------------------------------------------
    # 値の選択ダイアログ
    # ------------------------------------------------------------------
    def device_names(self, kind):
        names = []
        for d in self.c.devs:
            if not d:
                continue
            dd = self.c.reg.device(d["def"]) or {}
            if kind in ("*", None) or dd.get("type") == kind:
                if d["name"] not in names:
                    names.append(d["name"])
        return names

    def var_names(self):
        found = []

        def walk(x):
            if isinstance(x, dict):
                if x.get("type") in ("set_var", "change_var"):
                    found.append(x.get("var"))
                if x.get("type") == "var":
                    found.append(x.get("name"))
                for v in x.values():
                    walk(v)
            elif isinstance(x, list):
                for v in x:
                    walk(v)
        walk(self.prog)
        out = []
        for n in found + B.PRESET_VARS:
            if n and n not in out:
                out.append(n)
        return out

    def open_dialog(self, node, key, kind):
        self.dialogs.append((node, key, kind))
        self.render_dialog()

    def close_dialog(self, all_=False):
        if all_:
            self.dialogs = []
        elif self.dialogs:
            self.dialogs.pop()
        self.render_dialog()
        self.render()

    def set_value(self, node, key, value, stay=False):
        node[key] = value
        self.dirty = True
        self.discard_armed = False
        if not stay:
            self.close_dialog()
        else:
            self.render_dialog()
            self.render()

    def render_dialog(self):
        from .ui_destroy import destroy
        for e in list(self.dialog_root.children):
            destroy(e)
        if not self.dialogs:
            return
        U = self.U
        col = U["color"]
        node, key, kind = self.dialogs[-1]
        cur = node.get(key)
        r = self.dialog_root
        blocker = U["Button"](parent=r, model="quad", scale=(3, 2), color=col.rgba(0, 0, 0, 0.55), z=0.05,
                              highlight_color=col.rgba(0, 0, 0, 0.55), pressed_color=col.rgba(0, 0, 0, 0.55))
        blocker.on_click = lambda: None
        U["Entity"](parent=r, model="quad", position=(0, 0.0, 0.02), scale=(1.25, 0.9), color=col.rgb(0.1, 0.11, 0.15))
        kind_name = {"num": "数値", "bool": "条件", "text": "文字", "key": "キー", "var": "変数"}.get(
            kind.split(":")[0], "機器" if kind.startswith("dev") else "選択")
        U["Text"](parent=r, text=f"入力欄（{kind_name}）を設定", position=(-0.6, 0.42), origin=(-0.5, 0.5), scale=1.0)
        U["Text"](parent=r, text=f"今の値: {B.summary(cur)}", position=(-0.6, 0.37), origin=(-0.5, 0.5), scale=0.8,
                  color=col.light_gray)
        y = 0.30
        # 組み合わせの式なら、中身を編集できるようにする
        children = B.expr_children(cur) if isinstance(cur, dict) else []
        if children or (isinstance(cur, dict) and cur.get("type") in ("op", "cmp", "logic", "func")):
            x = -0.6
            U["Text"](parent=r, text="中身:", position=(x, y), origin=(-0.5, 0), scale=0.75)
            x += 0.07
            opt = self._op_cycle(cur)
            if opt:
                self._btn(r, f"種類: {opt[0]}  ⟳", (x + 0.1, y), opt[1], w=0.2, h=0.04)
                x += 0.215
            for ck, ckind in children:
                label = f"{ck}: {B.summary(cur.get(ck))}"
                w = min(0.5, max(0.14, len(label) * 0.013))
                self._btn(r, label, (x + w / 2, y), (lambda c_=cur, k_=ck, kk=ckind: self.open_dialog(c_, k_, kk)),
                          w=w, h=0.04, col=col.rgb(0.85, 0.85, 0.85)).text_entity.color = col.black
                x += w + 0.01
            y -= 0.06
        U["Text"](parent=r, text="置き換える:", position=(-0.6, y), origin=(-0.5, 0), scale=0.75, color=col.light_gray)
        y -= 0.045
        y = self._options(r, node, key, kind, cur, y)
        if len(self.dialogs) > 1:
            self._btn(r, "◀ 戻る", (0.36, -0.40), self.close_dialog, w=0.15)
        self._btn(r, "閉じる", (0.53, -0.40), lambda: self.close_dialog(all_=True), w=0.15,
                  col=col.rgb(0.3, 0.3, 0.33))

    def _op_cycle(self, cur):
        t = cur.get("type")
        if t == "op":
            ops = list(B.OPS)
        elif t == "cmp":
            ops = list(B.CMPS)
        elif t == "logic":
            ops = ["and", "or"]
        elif t == "func":
            ops = list(B.FUNCS)
            key = "fn"
        elif t == "devval":
            ops, key = list(B.DEVPROPS), "prop"
        elif t == "devbool":
            ops, key = list(B.DEVBOOLS), "prop"
        else:
            return None
        if t in ("op", "cmp", "logic"):
            key = "op"
        names = {"and": "かつ", "or": "または"}

        def nxt():
            i = ops.index(cur.get(key)) if cur.get(key) in ops else -1
            cur[key] = ops[(i + 1) % len(ops)]
            self.dirty = True
            self.render_dialog()
            self.render()
        return names.get(cur.get(key), cur.get(key)), nxt

    def _grid(self, r, items, y, cols=4, w=0.27):
        """items: [(表示, 関数, 色)] を並べる。次の y を返す。"""
        for k, (label, fn, c) in enumerate(items):
            x = -0.45 + (k % cols) * (w + 0.02)
            yy = y - (k // cols) * 0.045
            self._btn(r, label, (x, yy), fn, w=w, h=0.038, col=c, ts=0.62)
        rows = (len(items) + cols - 1) // cols
        return y - rows * 0.045 - 0.01

    def _options(self, r, node, key, kind, cur, y):
        U = self.U
        col = U["color"]
        sv = self.set_value
        sens_c = col.rgb(*_dim(B.CAT_COLOR["sensor"], 0.9))
        op_c = col.rgb(*_dim(B.CAT_COLOR["op"], 0.9))
        var_c = col.rgb(*_dim(B.CAT_COLOR["var"], 0.9))
        base = kind.split(":")[0]
        if base == "key":
            items = [(k, (lambda kk=k: sv(node, key, kk)), col.rgb(0.3, 0.3, 0.4)) for k in B.KEYS]
            return self._grid(r, items, y, cols=5, w=0.2)
        if base == "choice":
            opts = kind.split(":", 1)[1].split("|")
            items = [(o, (lambda oo=o: sv(node, key, oo)), col.rgb(0.3, 0.3, 0.4)) for o in opts]
            return self._grid(r, items, y)
        if base == "dev":
            dk = kind.split(":", 1)[1]
            names = self.device_names(dk)
            items = [("すべて", (lambda: sv(node, key, "*")), col.rgb(0.3, 0.3, 0.4))]
            items += [(n, (lambda nn=n: sv(node, key, nn)), col.rgb(0.25, 0.4, 0.6)) for n in names]
            y = self._grid(r, items, y)
            U["Text"](parent=r, text="同じ名前の機器はまとめて動きます", position=(-0.6, y), origin=(-0.5, 0.5),
                      scale=0.65, color=col.light_gray)
            return y - 0.04
        if base == "var":
            items = [(n, (lambda nn=n: sv(node, key, nn)), var_c) for n in self.var_names()]
            y = self._grid(r, items, y)
            return self._input_row(r, y, "新しい変数名（半角英数字）", "", NAME_CHARS,
                                   lambda s: sv(node, key, s) if s else None)
        # 値（数値・条件・文字）
        if base in ("num", "text"):
            if base == "num":
                v0 = B._fmt_num(cur.get("v")) if isinstance(cur, dict) and cur.get("type") == "num" else ""
                y = self._input_row(r, y, "数値を入力", v0, "0123456789.-",
                                    lambda s: sv(node, key, {"type": "num", "v": _to_float(s)}))
            else:
                v0 = cur.get("v", "") if isinstance(cur, dict) and cur.get("type") == "text" else ""
                y = self._input_row(r, y, "文字を入力（半角）", v0, TEXT_CHARS,
                                    lambda s: sv(node, key, {"type": "text", "v": s}))
                y = self._grid(r, [("□ と □ をつなぐ", (lambda: sv(node, key, {
                    "type": "join", "a": cur if isinstance(cur, dict) else {"type": "text", "v": ""},
                    "b": {"type": "sensor", "name": "altitude"}}, stay=True)), op_c)], y)
            items = [(name, (lambda kk=k: sv(node, key, {"type": "sensor", "name": kk})), sens_c)
                     for k, (name, _) in B.SENSORS.items()]
            items.append(("機器の値…", (lambda: sv(node, key, {"type": "devval", "dev": "*", "prop": "角度"}, stay=True)),
                          sens_c))
            items.append(("通信の受信値…", (lambda: sv(node, key, {"type": "radio_val", "ch": {"type": "text", "v": "ch1"}},
                                                          stay=True)), col.rgb(*_dim(B.CAT_COLOR["radio"], 0.9))))
            y = self._grid(r, items, y, cols=5, w=0.2)
            ops = []
            for _cat, label, n in B.EXPR_PALETTE:
                if n["type"] in ("op", "func", "random"):
                    nn = copy.deepcopy(n)
                    if isinstance(cur, dict) and cur.get("type") in ("num", "sensor", "var", "op", "func"):
                        nn["a"] = copy.deepcopy(cur)
                    ops.append((label, (lambda x_=nn: sv(node, key, copy.deepcopy(x_), stay=True)), op_c))
            y = self._grid(r, ops, y, cols=4, w=0.24)
            vitems = [(n, (lambda nn=n: sv(node, key, {"type": "var", "name": nn})), var_c) for n in self.var_names()]
            return self._grid(r, vitems, y, cols=6, w=0.16)
        if base == "bool":
            items = []
            for _cat, label, n in B.EXPR_PALETTE:
                if n["type"] in ("cmp", "logic", "not", "key", "devbool", "radio_has", "radio_ok"):
                    nn = copy.deepcopy(n)
                    if n["type"] == "cmp":
                        nn["a"] = {"type": "sensor", "name": "altitude"}
                        nn["b"] = {"type": "num", "v": 10}
                    if n["type"] in ("logic", "not") and isinstance(cur, dict):
                        nn["a"] = copy.deepcopy(cur)
                    items.append((label, (lambda x_=nn: sv(node, key, copy.deepcopy(x_), stay=True)), op_c))
            items.append(("はい（常に成り立つ）", (lambda: sv(node, key, {"type": "bool", "v": True})), op_c))
            items.append(("いいえ", (lambda: sv(node, key, {"type": "bool", "v": False})), op_c))
            return self._grid(r, items, y, cols=3, w=0.33)
        return y

    def _input_row(self, r, y, label, value, chars, on_ok):
        U = self.U
        U["Text"](parent=r, text=label, position=(-0.6, y), origin=(-0.5, 0), scale=0.7)
        f = U["InputField"](parent=r, default_value=str(value), position=(0.02, y), scale=(0.34, 0.04),
                            character_limit=40, active=True)
        f.limit_content_to = chars
        f.submit_on = ["enter"]
        f.on_submit = lambda: on_ok(f.text_field.text.strip())
        self._btn(r, "決定", (0.26, y), lambda: on_ok(f.text_field.text.strip()), w=0.1, h=0.04,
                  col=self.U["color"].rgb(0.2, 0.45, 0.3))
        return y - 0.055

    # ------------------------------------------------------------------
    def _status(self, msg):
        if hasattr(self, "status"):
            self.status.text = _safe(_wrap(msg, 24))
            self._status_t = time.time()

    def open_rename(self):
        if self.index is None:
            return
        from .device_ui import DevicePanel
        self._names_before = {k: d["name"] for k, d in enumerate(self.c.devs) if d}
        DevicePanel(self.game, self.c, self.index, owner=self)

    def after_modal(self):
        """名前を変えたら、プログラム中の古い名前も新しい名前に置き換える（ほかに同名がなければ）。"""
        before = getattr(self, "_names_before", {})
        now = {k: d["name"] for k, d in enumerate(self.c.devs) if d}
        names_now = set(now.values())
        changes = {old: now[k] for k, old in before.items() if k in now and now[k] != old and old not in names_now}
        hit = []
        if changes:
            def walk(x):
                if isinstance(x, dict):
                    if isinstance(x.get("dev"), str) and x["dev"] in changes:
                        x["dev"] = changes[x["dev"]]
                        self.dirty = True
                        hit.append(1)
                    for v in x.values():
                        walk(v)
                elif isinstance(x, list):
                    for v in x:
                        walk(v)
            walk(self.prog)
            if hit:
                self._status("名前の変更をプログラムにも反映しました（保存してください）")
        self.render()

    def input(self, key):
        from ursina import input_handler
        if getattr(self, "modal", None):
            return
        combo = input_handler.get_combined_key(key)
        if key == "escape":
            if self.dialogs:
                self.close_dialog(all_=True)
            else:
                self.close()
            return
        if self.dialogs:
            return
        if combo == "control+s":
            self.save()
        elif combo == "control+r":
            self.save_and_run()
        elif key == "scroll up":
            self.scroll = max(0, self.scroll - 2)
            self.render()
        elif key == "scroll down":
            self.scroll += 2
            self.render()
        elif key == "delete":
            self.delete()

    def update(self):
        d = self.device()
        if d is None:
            return
        st = d["state"]
        s = self.c.status or {}
        info = [f"状態: {'実行中' if st.get('running') else '停止中'}",
                f"電力: {s.get('stored', 0) / 1e6:.2f} MJ"]
        if st.get("error"):
            info.append("エラー: " + _safe(_wrap(st["error"], 30)))
        self.info.text = "\n".join(info)
        self.log_text.text = _safe("出力:\n" + "\n".join(_wrap(t, 38) for t in st.get("_log", [])[-10:]))
        if self.status.text and time.time() - getattr(self, "_status_t", 0) > 8:
            self.status.text = ""
        if int(time.time() * 2) != getattr(self, "_tt", 0):
            self._tt = int(time.time() * 2)
            self._update_title()


def _safe(s):
    """Ursina の文字装飾タグ（< >）として解釈されないように全角にする。"""
    return str(s).replace("<", "＜").replace(">", "＞")


def _to_float(s):
    try:
        return float(s)
    except ValueError:
        return 0.0


def _wrap(s, n):
    s = str(s)
    return "\n".join(s[i:i + n] for i in range(0, len(s), n)) if len(s) > n else s
