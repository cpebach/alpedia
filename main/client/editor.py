"""ゲーム内スクリプトエディタ：コンピューター（制御ブロック）のプログラムを表示・編集する。"""
import time
from pathlib import Path

from .. import log, scripting

L = log.get("editor")

CODE_FONT_CANDIDATES = [
    "C:/Windows/Fonts/BIZ-UDGothicR.ttc", "C:/Windows/Fonts/msgothic.ttc",
    "/System/Library/Fonts/Osaka.ttf",
]

NEW_TEMPLATE = '''"""新しいスクリプト

tick(api) が一定間隔で呼ばれます。使える機能は 「API」 ボタンか scripts/README.txt を参照。
"""


def tick(api):
    # 例: 操縦席で Space を押している間だけ推進機を全開にする
    on = "space" in api.input
    for t in api.devices("thruster"):
        t.throttle = 1.0 if on else 0.0
'''

KEYWORDS = ["def ", "return", "if ", "elif ", "else:", "for ", "while ", " in ", "import ", "from ",
            "and ", "or ", "not ", "True", "False", "None", "break", "continue", "pass", "lambda ", "try:",
            "except", "finally:", "class ", "global "]


def _find_code_font(cfg):
    custom = cfg.get("graphics", {}).get("code_font")
    for f in ([custom] if custom else []) + CODE_FONT_CANDIDATES:
        if f and Path(f).exists():
            return Path(f)
    return None


def _make_field_class():
    from ursina import Text
    from ursina.prefabs.text_field import TextField
    from ursina.string_utilities import multireplace

    class CodeField(TextField):
        """行番号を1始まりにし、スクロール時のコメント色の不具合を直した TextField。"""

        def render(self):
            lines = self.text.split("\n")
            text = "\n" * self.scroll + "\n".join(lines[self.scroll:self.max_lines + self.scroll])
            if not hasattr(self.text_entity, "raw_text") or self._prev_text != text or self.scroll != self._prev_scroll:
                _lines = text.split("\n")
                for i in range(len(_lines)):
                    s = _lines[i]
                    if s.lstrip().startswith("#"):
                        _lines[i] = f"☾gray☽{s}☾default☽"
                    elif self.replacements:
                        _lines[i] = multireplace(s, self.replacements)
                self.text_entity.text = "\n".join(_lines)
                shown = min(len(lines) - self.scroll, self.max_lines)
                self.line_numbers.text = ("     \n" * self.scroll) + "\n".join(
                    str(e + self.scroll + 1).rjust(3, " ") for e in range(max(shown, 0)))
                self._prev_text = text
                self._prev_scroll = self.scroll
                self.scroll_parent.y = (self.scroll * Text.size * self.line_height)
                self.cursor.visible = self.cursor.y >= self.scroll and self.cursor.y < self.scroll + self.max_lines
                self.draw_selection()
            if self.on_value_changed:
                self.on_value_changed()
    return CodeField


class ScriptEditor:
    MAX_LINES = 26

    def __init__(self, game, construct, index):
        from ursina import Entity, Text, Button, InputField, camera, color, application, mouse
        self.game = game
        self.world = game.world
        self.c = construct
        self.cell = construct.cells[index]
        self.dirty = False
        self.discard_armed = False
        self.show_api = False
        self.U = dict(Entity=Entity, Text=Text, Button=Button, camera=camera, color=color)
        mouse.locked = False
        self.root = Entity(parent=camera.ui)
        Entity(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0.02, 0.03, 0.05, 0.92), z=0.2)
        self.title = Text(parent=self.root, text="", position=(-0.86, 0.475), origin=(-0.5, 0.5), scale=1.1)
        self.hint = Text(parent=self.root, position=(-0.86, -0.455), origin=(-0.5, 0.5), scale=0.65,
                         color=color.light_gray,
                         text="Ctrl+S 保存 / Ctrl+R 保存して実行 / Ctrl+Z 元に戻す / Tab 字下げ / ホイール スクロール / Esc 閉じる")
        # コード欄（等幅の日本語フォントがあれば使う）
        CodeField = _make_field_class()
        font = _find_code_font(game.cfg)
        old_mono, old_folder = Text.default_monospace_font, application.fonts_folder
        if font:
            application.fonts_folder = font.parent
            Text.default_monospace_font = font.name
        try:
            self.field = CodeField(max_lines=self.MAX_LINES, register_mouse_input=True)
        finally:
            Text.default_monospace_font, application.fonts_folder = old_mono, old_folder
        self.field.parent = self.root
        self.field.position = (-0.8, 0.42)
        self.field.scale = 0.95
        self.field.bg.color = color.rgb(0.1, 0.11, 0.14)
        self.field.bg.scale = (1.2 / 0.95, self.field.bg.scale_y)
        self.field.bg.x = -0.05
        self.field.line_numbers.enabled = True
        self.field.replacements = {k: f"☾azure☽{k}☾default☽" for k in KEYWORDS}
        self.field.replacements["api."] = "☾orange☽api☾default☽."
        self.field.on_value_changed = self._changed
        # 右側パネル
        x = 0.62
        self.info = Text(parent=self.root, text="", position=(x - 0.2, 0.40), origin=(-0.5, 0.5), scale=0.7)
        Text(parent=self.root, text="ファイル名", position=(x - 0.2, 0.22), origin=(-0.5, 0.5), scale=0.7)
        self.name_field = InputField(parent=self.root, default_value="", position=(x, 0.17), scale=(0.4, 0.045),
                                     character_limit=40)
        self.name_field.limit_content_to = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_."
        self.name_field.active = False

        def btn(text, pos, fn, w=0.19, col=None):
            b = Button(parent=self.root, text=text, position=pos, scale=(w, 0.045),
                       color=col or color.rgb(0.22, 0.32, 0.45), text_size=0.75)
            b.on_click = fn
            return b
        btn("◀ 前のファイル", (x - 0.1, 0.11), lambda: self._cycle_file(-1))
        btn("次のファイル ▶", (x + 0.1, 0.11), lambda: self._cycle_file(1))
        btn("保存", (x - 0.1, 0.05), self.save, col=color.rgb(0.2, 0.45, 0.3))
        btn("保存して実行", (x + 0.1, 0.05), self.save_and_run, col=color.rgb(0.2, 0.5, 0.3))
        btn("停止", (x - 0.1, -0.01), self.stop, col=color.rgb(0.5, 0.25, 0.2))
        btn("新規ファイル", (x + 0.1, -0.01), self.new_file)
        btn("API 一覧", (x - 0.1, -0.07), self.toggle_api)
        btn("閉じる", (x + 0.1, -0.07), self.close, col=color.rgb(0.3, 0.3, 0.33))
        btn("ブロックで編集（ノードプログラミング）", (x, 0.28), self.open_blocks, w=0.4,
            col=color.rgb(0.75, 0.5, 0.15))
        btn("このコンピューターの名前を変更", (x, 0.465), self.open_rename, w=0.4)
        self.status = Text(parent=self.root, text="", position=(x - 0.2, -0.12), origin=(-0.5, 0.5), scale=0.7,
                           color=color.orange)
        Text(parent=self.root, text="出力（print）", position=(x - 0.2, -0.22), origin=(-0.5, 0.5), scale=0.7)
        self.log_text = Text(parent=self.root, text="", position=(x - 0.2, -0.255), origin=(-0.5, 0.5), scale=0.6,
                             color=color.light_gray)
        self.api_panel = Entity(parent=self.root, enabled=False)
        Entity(parent=self.api_panel, model="quad", position=(-0.2, 0), scale=(1.3, 0.92),
               color=color.rgba(0.05, 0.06, 0.09, 0.97), z=-0.3)
        Text(parent=self.api_panel, text=self._api_text(), position=(-0.83, 0.44), origin=(-0.5, 0.5), scale=0.62,
             z=-0.31)

        class Keys(Entity):
            def input(self_, key):
                self.input(key)

            def update(self_):
                self.update()
        self.keys = Keys(parent=self.root)
        self._load_from_device()

    # ------------------------------------------------------------------
    @property
    def index(self):
        return self.c.index.get(self.cell)

    def device(self):
        i = self.index
        return None if i is None else self.c.devs[i]

    def _api_text(self):
        try:
            from ..demo import SCRIPT_README
            text = SCRIPT_README
        except Exception:  # noqa: BLE001
            text = ""
        return text.replace("<", "＜").replace(">", "＞")

    def scripts_dir(self):
        d = self.world.dir / "scripts"
        d.mkdir(exist_ok=True)
        return d

    def _load_from_device(self):
        d = self.device()
        name = d["state"].get("script", "") if d else ""
        self._open_file(name)

    def _open_file(self, name):
        self.name_field.text_field.text = name
        self.name_field.text_field.render()
        text = ""
        if name:
            try:
                p = scripting.script_path(self.world.dir, name)
                text = p.read_text(encoding="utf-8") if p.exists() else ""
                if not p.exists():
                    self._set_status(f"{name} はまだありません（保存すると作成されます）")
            except ValueError as e:
                self._set_status(str(e))
        else:
            text = NEW_TEMPLATE
            self.name_field.text_field.text = self._free_name()
            self.name_field.text_field.render()
            self._set_status("スクリプト未設定：テンプレートを表示中（保存すると設定されます）")
        self.field.text = text.replace("\r\n", "\n").replace("\t", "    ")
        self.field.cursor.position = (0, 0)
        self.field.set_scroll(0, render=False)
        self.field.render()
        self.field.on_undo = []
        self.field.active = True
        self.dirty = not bool(name)
        self.loaded_name = name

    def _free_name(self):
        k = 1
        while (self.scripts_dir() / f"script{k}.py").exists():
            k += 1
        return f"script{k}.py"

    def _changed(self):
        if getattr(self, "loaded_text", None) != self.field.text:
            self.dirty = True
            self.discard_armed = False
        self.loaded_text = self.field.text

    def _set_status(self, msg):
        self.status.text = msg
        self._status_time = time.time()

    def file_name(self):
        n = self.name_field.text_field.text.strip()
        if n and not n.endswith(".py"):
            n += ".py"
        return n

    # ------------------------------------------------------------------
    def save(self):
        d = self.device()
        if d is None:
            self._set_status("コンピューターが見つかりません（壊れた？）")
            return False
        name = self.file_name()
        try:
            p = scripting.script_path(self.world.dir, name)
        except ValueError as e:
            self._set_status(str(e))
            return False
        code = self.field.text
        p.write_text(code, encoding="utf-8")
        self.world.runner.invalidate(name)
        d["state"]["script"] = name
        was_blocks = bool(d["state"].pop("blocks", None))
        d["state"].pop("_env", None)
        d["state"].pop("error", None)
        d["state"].pop("error_line", None)
        self.dirty = False
        self.loaded_name = name
        try:
            compile(code, name, "exec")
            self._set_status(f"{name} を保存しました" + ("（Python モードに切り替わりました）" if was_blocks else ""))
        except SyntaxError as e:
            self._set_status(f"保存しました（構文エラー {e.lineno} 行目: {e.msg}）")
            d["state"]["error"] = f"SyntaxError: {e.msg}"
            d["state"]["error_line"] = e.lineno
            self._goto_line(e.lineno)
        L.info("スクリプト保存: %s", p)
        return True

    def save_and_run(self):
        if not self.save():
            return
        d = self.device()
        if d["state"].get("error_line") and d["state"].get("error", "").startswith("SyntaxError"):
            return
        d["state"]["running"] = True
        d["state"].pop("error", None)
        self._set_status(f"{d['state']['script']} を保存して実行しました")

    def stop(self):
        d = self.device()
        if d:
            d["state"]["running"] = False
            self._set_status("停止しました")

    def new_file(self):
        self.name_field.text_field.text = self._free_name()
        self.name_field.text_field.render()
        self.field.text = NEW_TEMPLATE
        self.field.cursor.position = (0, 0)
        self.field.set_scroll(0, render=False)
        self.field.render()
        self.dirty = True
        self._set_status("新しいファイル（保存するとこのコンピューターに設定されます）")

    def _cycle_file(self, step):
        files = sorted(p.name for p in self.scripts_dir().glob("*.py"))
        if not files:
            self._set_status("scripts フォルダに .py がありません")
            return
        if self.dirty and not self.discard_armed:
            self.discard_armed = True
            self._set_status("未保存の変更があります。もう一度押すと破棄して切り替えます")
            return
        cur = self.file_name()
        i = files.index(cur) if cur in files else -1
        self._open_file(files[(i + step) % len(files)])
        self._set_status(f"{self.file_name()} を開きました（保存するとこのコンピューターに設定されます）")
        self.dirty = self.file_name() != (self.device() or {}).get("state", {}).get("script")

    def _goto_line(self, line):
        if not line:
            return
        y = max(0, int(line) - 1)
        self.field.cursor.position = (0, y)
        self.field.set_scroll(max(0, y - self.MAX_LINES // 2))

    def toggle_api(self):
        self.show_api = not self.show_api
        self.api_panel.enabled = self.show_api
        self.field.enabled = not self.show_api
        self.field.active = not self.show_api

    def open_blocks(self):
        """ブロック編集画面に切り替える（Python からブロックへの変換はしない）。"""
        if self.dirty and not self.discard_armed:
            self.discard_armed = True
            self._set_status("未保存の変更があります。もう一度押すと破棄してブロック画面に切り替えます")
            return
        game, c, i = self.game, self.c, self.index
        self.close(force=True)
        if i is None:
            return
        from .block_editor import BlockEditor
        game.U["mouse"].locked = False
        game.ui_root.enabled = False
        game.editor = BlockEditor(game, c, i)

    def close(self, force=False):
        if self.dirty and not force and not self.discard_armed:
            self.discard_armed = True
            self._set_status("未保存の変更があります。Ctrl+S で保存、もう一度 Esc で破棄して閉じます")
            return
        from .ui_destroy import destroy
        destroy(self.root)
        try:
            destroy(self.field)
        except Exception:  # noqa: BLE001
            pass
        self.game.editor_closed()

    # ------------------------------------------------------------------
    def open_rename(self):
        if self.index is None:
            return
        from .device_ui import DevicePanel
        DevicePanel(self.game, self.c, self.index, owner=self)

    def after_modal(self):
        self.field.active = not self.show_api

    def input(self, key):
        from ursina import input_handler
        if getattr(self, "modal", None):
            return
        combo = input_handler.get_combined_key(key)
        if key == "escape":
            if self.show_api:
                self.toggle_api()
            else:
                self.close()
        elif combo == "control+s":
            self.save()
        elif combo == "control+r":
            self.save_and_run()
        elif key == "left mouse down":
            # ファイル名欄とコード欄のどちらに入力するか
            self.field.active = not self.name_field.hovered and not self.show_api

    def update(self):
        d = self.device()
        c = self.c
        if d is None:
            self.title.text = "コンピューターが失われました"
            return
        st = d["state"]
        running = "実行中" if st.get("running") else "停止中"
        mark = " ●未保存" if self.dirty else ""
        self.title.text = f"{d['name']}  —  {c.name}  [{st.get('script') or 'スクリプト未設定'}]  {running}{mark}"
        s = c.status or {}
        info = [f"状態: {running}",
                f"電力: {s.get('stored', 0)/1e6:.2f} / {s.get('capacity', 0)/1e6:.1f} MJ  (供給 {s.get('ratio', 1)*100:.0f}%)"]
        if st.get("error"):
            info.append("エラー:")
            info.append("  " + _wrap(st["error"], 40))
        self.info.text = "\n".join(info)
        logs = st.get("_log", [])
        self.log_text.text = "\n".join(_wrap(t, 46) for t in logs[-12:])
        if self.status.text and time.time() - getattr(self, "_status_time", 0) > 8:
            self.status.text = ""


def _wrap(s, n):
    s = str(s)
    return "\n  ".join(s[i:i + n] for i in range(0, len(s), n)) if len(s) > n else s
