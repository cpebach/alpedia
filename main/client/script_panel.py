"""コンピューター（Python スクリプト）の画面：ファイルの選択・実行・停止・エラーと出力の表示。

スクリプトの編集はゲームの外のエディタ（メモ帳・VS Code など）で行う。
「エディタで開く」でファイルを開き、保存するとゲームが自動で読み込み直す（実行中でも次の tick から新しいコード）。
"""
import os
import subprocess
import sys
import time

from .. import log, scripting

L = log.get("script_panel")

NEW_TEMPLATE = '''"""新しいスクリプト

tick(api) が一定間隔で呼ばれます。使える機能は scripts/README.txt を参照。
保存するとゲームが自動で読み込み直します（実行中でも）。
"""


def tick(api):
    # 例: 操縦席で Space を押している間だけ推進機を全開にする
    on = "space" in api.input
    for t in api.devices("thruster"):
        t.throttle = 1.0 if on else 0.0
'''


def open_external(path):
    """OS の既定のアプリでファイル・フォルダを開く。"""
    path = str(path)
    if sys.platform.startswith("win"):
        os.startfile(path)  # noqa: S606
    elif sys.platform == "darwin":
        subprocess.Popen(["open", path])  # noqa: S603, S607
    else:
        subprocess.Popen(["xdg-open", path])  # noqa: S603, S607


def _wrap(s, n):
    s = str(s)
    return "\n  ".join(s[i:i + n] for i in range(0, len(s), n)) if len(s) > n else s


class ScriptPanel:
    def __init__(self, game, construct, index):
        from ursina import Entity, Text, Button, camera, color, mouse
        self.game = game
        self.world = game.world
        self.c = construct
        self.cell = construct.cells[index]
        self.modal = None
        self.U = dict(Entity=Entity, Text=Text, Button=Button, color=color)
        mouse.locked = False
        self.root = Entity(parent=camera.ui)
        Entity(parent=self.root, model="quad", scale=(3, 2), color=color.rgba(0.02, 0.03, 0.05, 0.92), z=0.2)
        self.title = Text(parent=self.root, text="", position=(-0.86, 0.475), origin=(-0.5, 0.5), scale=1.1)
        Text(parent=self.root, position=(-0.86, -0.455), origin=(-0.5, 0.5), scale=0.65, color=color.light_gray,
             text="スクリプトはゲームの外のエディタで編集します。保存するとゲームが自動で読み込み直します / Esc 閉じる")
        self.info = Text(parent=self.root, text="", position=(-0.84, 0.40), origin=(-0.5, 0.5), scale=0.75)
        Text(parent=self.root, text="出力（print）", position=(-0.84, 0.05), origin=(-0.5, 0.5), scale=0.7)
        self.log_text = Text(parent=self.root, text="", position=(-0.84, 0.01), origin=(-0.5, 0.5), scale=0.62,
                             color=color.light_gray)
        x = 0.55

        def btn(text, pos, fn, w=0.2, col=None):
            b = Button(parent=self.root, text=text, position=pos, scale=(w, 0.045),
                       color=col or color.rgb(0.22, 0.32, 0.45), text_size=0.75)
            b.on_click = fn
            return b
        btn("このコンピューターの名前を変更", (x, 0.40), self.open_rename, w=0.42)
        btn("ブロックで編集（ノードプログラミング）", (x, 0.33), self.open_blocks, w=0.42,
            col=color.rgb(0.75, 0.5, 0.15))
        self.py_btn = btn("Python モードにする", (x, 0.26), self.to_python, w=0.42, col=color.rgb(0.45, 0.35, 0.6))
        btn("◀ 前のファイル", (x - 0.11, 0.17), lambda: self.cycle_file(-1))
        btn("次のファイル ▶", (x + 0.11, 0.17), lambda: self.cycle_file(1))
        btn("エディタで開く", (x - 0.11, 0.11), self.open_in_editor, col=color.rgb(0.2, 0.45, 0.3))
        btn("新規ファイル", (x + 0.11, 0.11), self.new_file)
        btn("実行", (x - 0.11, 0.05), self.run, col=color.rgb(0.2, 0.5, 0.3))
        btn("停止", (x + 0.11, 0.05), self.stop, col=color.rgb(0.5, 0.25, 0.2))
        btn("scripts フォルダを開く", (x - 0.11, -0.01), lambda: self._open(self.scripts_dir()))
        btn("API 一覧（README）", (x + 0.11, -0.01), lambda: self._open(self.readme()))
        btn("閉じる", (x, -0.08), self.close, w=0.42, col=color.rgb(0.3, 0.3, 0.33))
        self.status = Text(parent=self.root, text="", position=(x - 0.21, -0.14), origin=(-0.5, 0.5), scale=0.7,
                           color=color.orange)

        class Keys(Entity):
            def input(self_, key):
                if self.modal is None and key == "escape":
                    self.close()

            def update(self_):
                self.update()
        self.keys = Keys(parent=self.root)

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

    def readme(self):
        p = self.scripts_dir() / "README.txt"
        if not p.exists():
            from ..content import SCRIPT_README
            p.write_text(SCRIPT_README, encoding="utf-8")
        return p

    def set_status(self, msg):
        self.status.text = msg
        self._status_time = time.time()

    def _open(self, path):
        try:
            open_external(path)
            self.set_status(f"{path.name} を開きました")
        except Exception as e:  # noqa: BLE001
            L.warning("開けませんでした: %s (%s)", path, e)
            self.set_status(f"開けませんでした: {path}")

    def _set_script(self, name):
        d = self.device()
        if d is None:
            return
        st = d["state"]
        st["script"] = name
        for k in ("_env", "error", "error_line"):
            st.pop(k, None)
        self.world.runner.invalidate(name)

    # ------------------------------------------------------------------
    def cycle_file(self, step):
        files = sorted(p.name for p in self.scripts_dir().glob("*.py"))
        if not files:
            self.set_status("scripts フォルダに .py がありません（新規ファイルで作れます）")
            return
        cur = (self.device() or {}).get("state", {}).get("script", "")
        i = files.index(cur) if cur in files else -1
        name = files[(i + step) % len(files)]
        self._set_script(name)
        self.set_status(f"{name} をこのコンピューターに設定しました")

    def new_file(self):
        k = 1
        while (self.scripts_dir() / f"script{k}.py").exists():
            k += 1
        name = f"script{k}.py"
        (self.scripts_dir() / name).write_text(NEW_TEMPLATE, encoding="utf-8")
        self._set_script(name)
        self.set_status(f"{name} を作って設定しました（エディタで開いて編集）")

    def open_in_editor(self):
        d = self.device()
        name = (d or {}).get("state", {}).get("script", "")
        if not name:
            self.new_file()
            name = self.device()["state"]["script"]
        try:
            p = scripting.script_path(self.world.dir, name)
        except ValueError as e:
            self.set_status(str(e))
            return
        if not p.exists():
            p.write_text(NEW_TEMPLATE, encoding="utf-8")
        self._open(p)

    def run(self):
        d = self.device()
        if d is None:
            return
        st = d["state"]
        name = st.get("script", "")
        try:
            p = scripting.script_path(self.world.dir, name)
            compile(p.read_text(encoding="utf-8"), name, "exec")
        except SyntaxError as e:
            st["error"], st["error_line"] = f"SyntaxError: {e.msg}（{e.lineno} 行目）", e.lineno
            self.set_status(f"構文エラー {e.lineno} 行目: {e.msg}")
            return
        except (OSError, ValueError) as e:
            self.set_status(f"スクリプトを読めません: {e}")
            return
        for k in ("_env", "error", "error_line"):
            st.pop(k, None)
        st["running"] = True
        self.set_status(f"{name} を実行しました")

    def stop(self):
        d = self.device()
        if d:
            d["state"]["running"] = False
            self.set_status("停止しました")

    def to_python(self):
        d = self.device()
        if d and d["state"].pop("blocks", None):
            self.set_status("Python モードにしました（ブロックから作ったコードを、そのまま Python として使います）")

    def open_blocks(self):
        game, c, i = self.game, self.c, self.index
        self.close()
        if i is None:
            return
        from .block_editor import BlockEditor
        game.U["mouse"].locked = False
        game.ui_root.enabled = False
        game.editor = BlockEditor(game, c, i)

    def open_rename(self):
        if self.index is None:
            return
        from .device_ui import DevicePanel
        DevicePanel(self.game, self.c, self.index, owner=self)

    def close(self):
        from .ui_destroy import destroy
        destroy(self.root)
        self.game.editor_closed()

    # ------------------------------------------------------------------
    def update(self):
        d = self.device()
        c = self.c
        if d is None:
            self.title.text = "コンピューターが失われました"
            return
        st = d["state"]
        running = "実行中" if st.get("running") else "停止中"
        mode = "ブロック" if st.get("blocks") else "Python"
        self.py_btn.enabled = bool(st.get("blocks"))
        self.title.text = f"{d['name']}  —  {c.name}"
        s = c.status or {}
        info = [f"スクリプト: {st.get('script') or '未設定'}（{mode}）",
                f"状態: {running}",
                f"電力: {s.get('stored', 0)/1e6:.2f} / {s.get('capacity', 0)/1e6:.1f} MJ  (供給 {s.get('ratio', 1)*100:.0f}%)",
                f"場所: scripts/{st.get('script') or ''}"]
        if st.get("error"):
            info.append("エラー:")
            info.append("  " + _wrap(st["error"], 56))
        self.info.text = "\n".join(info).replace("<", "＜").replace(">", "＞")
        logs = st.get("_log", [])
        self.log_text.text = "\n".join(_wrap(t, 64) for t in logs[-16:]).replace("<", "＜").replace(">", "＞")
        if self.status.text and time.time() - getattr(self, "_status_time", 0) > 8:
            self.status.text = ""
