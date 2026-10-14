"""起動時のメニュー画面：ワールドの選択・新規作成・削除・ワールド設定・ゲーム設定。"""
import json
import shutil
import time

from .. import paths, log, config as config_mod

L = log.get("menu")

NAME_CHARS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
RESOLUTIONS = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440), (3840, 2160)]
PAGE_SIZE = 7


# ---------------------------------------------------------------------------
# ワールド操作（UI に依存しない部分）
# ---------------------------------------------------------------------------
def list_worlds():
    out = []
    if not paths.SAVES.exists():
        return out
    for d in paths.SAVES.iterdir():
        wj = d / "world.json"
        if not d.is_dir() or not wj.exists() or d.name.startswith("."):
            continue
        info = {"name": d.name, "constructs": 0, "mods": [], "mtime": 0.0}
        s = {}
        try:
            s = json.loads(wj.read_text(encoding="utf-8"))
            info["time"] = s.get("time", 0.0)
        except Exception:  # noqa: BLE001
            info["broken"] = True
        try:
            info["constructs"] = len(json.loads((d / "constructs.json").read_text(encoding="utf-8")))
        except Exception:  # noqa: BLE001
            pass
        try:
            info["mods"] = json.loads((d / "mods.json").read_text(encoding="utf-8")).get("enabled", [])
        except Exception:  # noqa: BLE001
            pass
        from ..savefmt import upgrade_world
        info["mods"] = upgrade_world(s, info["mods"])[1]      # 古い形式のワールドも MOD で表示
        info["mtime"] = max((f.stat().st_mtime for f in d.glob("*.json")), default=wj.stat().st_mtime)
        out.append(info)
    out.sort(key=lambda x: x["mtime"], reverse=True)
    return out


def list_mods():
    """[(MOD名, 表示名)]。本体に入っている MOD（サバイバル・街）を先に並べる。"""
    out = []
    if not paths.MODS.exists():
        return out
    for d in sorted(paths.MODS.iterdir()):
        if d.is_dir() and not d.name.startswith("."):
            name, builtin = d.name, False
            mj = d / "mod.json"
            if mj.exists():
                try:
                    meta = json.loads(mj.read_text(encoding="utf-8"))
                    name, builtin = meta.get("name", d.name), bool(meta.get("builtin"))
                except Exception:  # noqa: BLE001
                    pass
            out.append((d.name, name, builtin))
    out.sort(key=lambda x: (not x[2], x[0] != "survival", x[0]))
    return [(m, n) for m, n, _ in out]


def read_world_settings(name):
    d = paths.world_dir(name)
    from ..world import DEFAULT_SETTINGS
    from ..savefmt import upgrade_world
    s = json.loads(json.dumps(DEFAULT_SETTINGS))
    raw = {}
    try:
        raw = json.loads((d / "world.json").read_text(encoding="utf-8"))
    except Exception:  # noqa: BLE001
        pass
    try:
        mods = json.loads((d / "mods.json").read_text(encoding="utf-8")).get("enabled", [])
    except Exception:  # noqa: BLE001
        mods = []
    raw, mods, _ = upgrade_world(raw, mods)      # 古い形式（creative / cities）は MOD に読み替える
    s.update(raw)
    s["cleanup"] = {**DEFAULT_SETTINGS["cleanup"], **s.get("cleanup", {})}
    return s, mods


def write_world_settings(name, settings, mods):
    d = paths.world_dir(name)
    (d / "world.json").write_text(json.dumps(settings, ensure_ascii=False, indent=1), encoding="utf-8")
    (d / "mods.json").write_text(json.dumps({"enabled": list(mods)}, ensure_ascii=False, indent=1), encoding="utf-8")
    L.info("ワールド設定を保存: %s", name)


PLANETS = {"terra": "球体（テラ）", "gaia": "地球型（ガイア）"}


def _setup_window_icon():
    """ウィンドウのアイコンを設定し、以後のウィンドウ設定の送り直しにはアイコンを含めないようにする。

    ursina はマウスのロックを切り替えるたびにウィンドウ設定をまるごと送り直す。その中に見つからない
    アイコン（既定の textures/ursina.ico）が入っていると、Windows ではその要求全体が通らず、
    マウスカーソルが視点操作に捕まったまま（非表示・固定）になってしまう。
    そこでアイコンは日本語を含まない一時フォルダにコピーして一度だけ設定し、その後は設定から外す。"""
    import tempfile
    from pathlib import Path
    from ursina import window, application
    try:
        from panda3d.core import Filename, WindowProperties
        src = paths.SAUCE / "textures" / "alpedia.ico"
        if src.exists() and application.base and application.base.win:
            dst = Path(tempfile.gettempdir()) / "alpedia_icon.ico"
            shutil.copyfile(src, dst)
            wp = WindowProperties()
            wp.setIconFilename(Filename.fromOsSpecific(str(dst)).getFullpath())
            application.base.win.requestProperties(wp)
    except Exception as e:  # noqa: BLE001
        L.warning("ウィンドウのアイコンを設定できません: %s", e)
    try:
        window.clearIconFilename()
    except Exception:  # noqa: BLE001
        pass


def create_world(name, template, settings_patch, mods, planet="terra"):
    from ..app import create_empty_world
    if paths.world_dir(name).exists():
        raise FileExistsError(f"ワールド「{name}」は既にあります")
    if template == "tutorial":
        from ..tutorial import create_tutorial
        create_tutorial(name)
    else:
        create_empty_world(name)
        s, _ = read_world_settings(name)
        from ..world import default_bodies
        s["planets"] = default_bodies(planet)
        write_world_settings(name, s, read_world_settings(name)[1])
    s, current_mods = read_world_settings(name)
    if mods is None:
        mods = current_mods
    s.update({k: v for k, v in settings_patch.items() if k != "cleanup"})
    if "cleanup" in settings_patch:
        s["cleanup"].update(settings_patch["cleanup"])
    s["name"] = name
    write_world_settings(name, s, mods)
    L.info("ワールド作成: %s（%s）", name, template)


TRASH = ".trash"   # 消しきれなかったワールドの置き場（一覧には出ない。次回起動時に片付ける）


def _rmtree_retry(d, tries=6):
    """OneDrive などの同期ソフトが一瞬つかんでいて「アクセスが拒否されました」になることがあるので、
    読み取り専用を外して少し待ちながら何度かやり直す。"""
    import os
    import stat
    import sys

    def onexc(func, path, exc):
        if isinstance(exc, FileNotFoundError):
            return
        try:
            os.chmod(path, stat.S_IWRITE)
        except OSError:
            pass
        for k in range(tries):
            time.sleep(0.1 * (k + 1))
            try:
                func(path)
                return
            except FileNotFoundError:
                return
            except OSError:
                continue
        raise exc
    if sys.version_info >= (3, 12):
        shutil.rmtree(d, onexc=onexc)
    else:
        shutil.rmtree(d, onerror=lambda f, p, ei: onexc(f, p, ei[1]))


def delete_world(name):
    """ワールドを削除する。消しきれないものが残ったら、ゴミ箱フォルダへ移して一覧から消す。
    戻り値: True=完全に削除 / False=ゴミ箱に移した（次回起動時に片付ける）"""
    d = paths.world_dir(name)
    if d.resolve().parent != paths.SAVES.resolve():
        raise ValueError("不正なワールド名です")
    try:
        _rmtree_retry(d)
        L.info("ワールド削除: %s", name)
        return True
    except OSError as e:
        if not d.exists():
            return True
        L.warning("ワールド %s を消しきれませんでした（%s）。ゴミ箱フォルダへ移します", name, e)
    trash = paths.SAVES / TRASH
    trash.mkdir(exist_ok=True)
    dst = trash / f"{name}_{int(time.time())}"
    try:
        d.rename(dst)
    except OSError as e:
        raise OSError(f"ファイルが使用中です（OneDrive の同期中など）。少し待ってからもう一度削除してください: {e}") from e
    L.info("ワールド %s をゴミ箱フォルダへ移しました: %s", name, dst)
    return False


def purge_trash():
    """前回消しきれなかったワールドを片付ける（失敗しても気にしない）。"""
    trash = paths.SAVES / TRASH
    if not trash.exists():
        return
    for d in list(trash.iterdir()):
        try:
            _rmtree_retry(d, tries=2)
        except OSError:
            pass
    try:
        trash.rmdir()
    except OSError:
        pass


def save_config(cfg):
    p = paths.CONFIG / "settings.json"
    p.write_text(json.dumps(cfg, indent=2, ensure_ascii=False), encoding="utf-8")
    L.info("ゲーム設定を保存しました")


# ---------------------------------------------------------------------------
# 画面
# ---------------------------------------------------------------------------
class MainMenu:
    def __init__(self, cfg, on_play, on_quit):
        from ursina import Entity, Text, Button, InputField, camera, color, window, mouse
        self.U = dict(Entity=Entity, Text=Text, Button=Button, InputField=InputField, camera=camera,
                      color=color, window=window)
        self.cfg = cfg
        self.on_play = on_play
        self.on_quit = on_quit
        self.selected = None
        self.page_index = 0
        self.status = ""
        window.color = color.rgb(0.07, 0.09, 0.13)
        mouse.locked = False
        self.root = Entity(parent=camera.ui)
        self.page = None
        self.show_main()

    # --- 部品 ----------------------------------------------------------
    def _clear(self):
        from .ui_destroy import destroy
        for c in list(self.root.children):
            destroy(c)

    def _text(self, text, pos, scale=1.0, origin=(0, 0), col=None):
        U = self.U
        return U["Text"](parent=self.root, text=text, position=pos, origin=origin, scale=scale,
                         color=col or U["color"].white)

    def _button(self, text, pos, on_click, size=(0.32, 0.055), col=None, enabled=True):
        U = self.U
        c = col or U["color"].rgb(0.22, 0.32, 0.45)
        b = U["Button"](parent=self.root, text=text, position=pos, scale=size,
                        color=c if enabled else U["color"].rgb(0.2, 0.2, 0.22), text_size=0.8)
        b.text_entity.scale *= 1.0
        if enabled:
            b.on_click = on_click
        else:
            b.text_color = U["color"].gray
        return b

    def _panel(self, pos, size, col=None):
        U = self.U
        return U["Entity"](parent=self.root, model="quad", position=(pos[0], pos[1], 0.01), scale=size,
                           color=col or U["color"].rgba(1, 1, 1, 0.06))

    def _stepper(self, label, y, values, current, fmt, on_change, x=0.0):
        """ラベル < 値 > の行。values: 選択肢のリスト。"""
        if current in values:
            idx = values.index(current)
        elif isinstance(current, (int, float)) and not isinstance(current, bool):
            idx = min(range(len(values)), key=lambda i: abs(values[i] - current))
        else:
            idx = 0
        state = {"i": idx}
        self._text(label, (x - 0.38, y), 0.85, origin=(-0.5, 0))
        val = self._text(fmt(values[idx]), (x + 0.2, y), 0.85)

        def step(d):
            state["i"] = (state["i"] + d) % len(values)
            val.text = fmt(values[state["i"]])
            on_change(values[state["i"]])
        self._button("◀", (x + 0.06, y), lambda: step(-1), size=(0.045, 0.045))
        self._button("▶", (x + 0.34, y), lambda: step(1), size=(0.045, 0.045))

    def _toggle(self, label, y, current, on_change, x=0.0):
        onoff = lambda v: "ON" if v else "OFF"  # noqa: E731
        self._stepper(label, y, [True, False], bool(current), onoff, on_change, x)

    def _status_line(self):
        if self.status:
            self._text(self.status, (0, -0.44), 0.8, col=self.U["color"].orange)

    # --- メイン ---------------------------------------------------------
    def show_main(self):
        self._clear()
        U = self.U
        worlds = list_worlds()
        names = [w["name"] for w in worlds]
        if self.selected not in names:
            self.selected = names[0] if names else None
        self._text("Alpedia", (0, 0.42), 3.0)
        self._text("物理演算ボクセルサンドボックス", (0, 0.35), 0.9, col=U["color"].light_gray)
        # ワールド一覧
        self._panel((-0.2, 0.0), (0.92, 0.58))
        self._text("ワールド", (-0.64, 0.26), 1.0, origin=(-0.5, 0))
        pages = max(1, (len(worlds) + PAGE_SIZE - 1) // PAGE_SIZE)
        self.page_index = min(self.page_index, pages - 1)
        start = self.page_index * PAGE_SIZE
        if not worlds:
            self._text("ワールドがありません。「新規作成」で作ってください", (-0.2, 0.05), 0.85,
                       col=U["color"].light_gray)
        for k, w in enumerate(worlds[start:start + PAGE_SIZE]):
            y = 0.19 - k * 0.07
            sel = w["name"] == self.selected
            col = U["color"].rgb(0.25, 0.45, 0.65) if sel else U["color"].rgb(0.15, 0.17, 0.22)
            when = time.strftime("%Y-%m-%d %H:%M", time.localtime(w["mtime"]))
            mode = "サバイバル" if "survival" in w["mods"] else "クリエイティブ"
            label = f"{w['name']}    {when}  /  {mode}  /  構築物 {w['constructs']}  /  MOD {len(w['mods'])}"
            if w.get("broken"):
                label = f"{w['name']}    （world.json が壊れています）"
            b = self._button(label, (-0.2, y), (lambda n=w["name"]: self._select(n)), size=(0.88, 0.06), col=col)
            b.text_entity.origin = (-0.5, 0)
            b.text_entity.x = -0.47
        if pages > 1:
            self._button("▲", (0.29, 0.26), lambda: self._page(-1), size=(0.045, 0.04))
            self._text(f"{self.page_index + 1}/{pages}", (0.2, 0.26), 0.75)
            self._button("▼", (0.29, -0.27), lambda: self._page(1), size=(0.045, 0.04))
        # 操作ボタン
        has = self.selected is not None
        x = 0.5
        self._button("遊ぶ", (x, 0.2), self._play, size=(0.3, 0.07), col=U["color"].rgb(0.2, 0.5, 0.3), enabled=has)
        self._button("新規作成", (x, 0.11), self.show_new)
        self._button("ワールド設定", (x, 0.04), lambda: self.show_world_settings(self.selected), enabled=has)
        self._button("削除", (x, -0.03), lambda: self.show_delete(self.selected), col=U["color"].rgb(0.5, 0.2, 0.2),
                     enabled=has)
        self._button("ゲーム設定", (x, -0.12), self.show_game_settings)
        self._button("終了", (x, -0.21), self.on_quit, col=U["color"].rgb(0.3, 0.3, 0.33))
        if has:
            self._text(f"選択中: {self.selected}", (-0.2, -0.33), 0.85, col=U["color"].light_gray)
        self._status_line()

    def _select(self, name):
        if name == self.selected:  # 同じワールドをもう一度押したら開始
            self._play()
            return
        self.selected = name
        self.status = ""
        self.show_main()

    def _page(self, d):
        self.page_index = max(0, self.page_index + d)
        self.show_main()

    def _play(self):
        if self.selected:
            self.on_play(self.selected)

    def error(self, msg):
        self.status = msg
        self.show_main()

    def show_load_errors(self, name, errors):
        """MOD のエラーでワールドを始められなかったとき：エラーの一覧と、どうすればよいか。"""
        self._clear()
        U = self.U
        self._text(f"「{name}」を始められません（MOD のエラー）", (0, 0.42), 1.4, col=U["color"].rgb(1, 0.55, 0.45))
        shown = errors[:12]
        lines = [("・" + e if len(e) <= 96 else "・" + e[:94] + "…") for e in shown]
        if len(errors) > len(shown):
            lines.append(f"…ほか {len(errors) - len(shown)} 件（logs/latest.log に全部あります）")
        self._text("\n".join(lines), (-0.8, 0.32), 0.75, origin=(-0.5, 0.5))
        self._text("MOD はワールドを始めるときに一括で読み込みます。エラーがあるとワールドを始めません（セーブは変わっていません）。\n"
                   "・MOD を作っている：エラーの場所を直して、もう一度「遊ぶ」を押してください。\n"
                   "・MOD を外した／入れ替えた：ワールド設定で元の MOD を入れ直すか、変える前のバックアップに戻してください。\n"
                   "新しい MOD を作るとき・ワールドの MOD を変えるときは、先にワールド（saves/ のフォルダ）をコピーしておくことをすすめます。",
                   (-0.8, -0.16), 0.7, origin=(-0.5, 0.5), col=U["color"].light_gray)
        self.status = ""
        self._button("戻る", (0, -0.42), self.show_main, size=(0.25, 0.055), col=U["color"].rgb(0.3, 0.3, 0.33))

    # --- 新規作成 -------------------------------------------------------
    def show_new(self):
        self._clear()
        U = self.U
        names = {w["name"] for w in list_worlds()}
        k = 1
        while f"world{k}" in names:
            k += 1
        opts = {"template": "empty", "planet": "terra", "voxel_size": 0.5, "gravity_scale": 1.0,
                "cleanup": True, "mods": set()}
        self._text("新しいワールド", (0, 0.42), 1.8)
        self._text("名前（半角英数字・- _）", (-0.38, 0.3), 0.85, origin=(-0.5, 0))
        field = U["InputField"](parent=self.root, default_value=f"world{k}", position=(0.2, 0.3),
                                scale=(0.34, 0.05), character_limit=32, active=True)
        field.limit_content_to = NAME_CHARS
        y = 0.21
        self._stepper("テンプレート", y, ["empty", "tutorial"], "empty",
                      lambda v: {"empty": "空のワールド",
                                 "tutorial": "チュートリアル（操作〜姿勢制御〜軌道投入）"}[v],
                      lambda v: opts.__setitem__("template", v))
        self._stepper("惑星", y - 0.07, list(PLANETS), "terra",
                      lambda v: PLANETS[v], lambda v: opts.__setitem__("planet", v))
        self._text("地球型：山・洞窟・海・池がある（チュートリアルは常に球体）", (0.2, y - 0.105), 0.6,
                   col=U["color"].light_gray)
        y -= 0.07
        self._stepper("ボクセルサイズ", y - 0.07, [0.25, 0.5, 1.0, 2.0], 0.5, lambda v: f"{v} m",
                      lambda v: opts.__setitem__("voxel_size", v))
        self._stepper("重力倍率", y - 0.14, [0.0, 0.25, 0.5, 1.0, 1.5, 2.0], 1.0, lambda v: f"×{v}",
                      lambda v: opts.__setitem__("gravity_scale", v))
        self._toggle("構築物の自動削除", y - 0.21, True, lambda v: opts.__setitem__("cleanup", v))
        self._mod_toggles(y - 0.30, opts["mods"])

        def create(play):
            name = field.text_field.text.strip()
            if not name:
                self.status = "名前を入力してください"
                self.show_new()
                return
            try:
                create_world(name, opts["template"],
                             {"voxel_size": opts["voxel_size"],
                              "gravity_scale": opts["gravity_scale"], "cleanup": {"enabled": opts["cleanup"]}},
                             sorted(opts["mods"]) if (opts["template"] == "empty" or opts["mods"]) else None,
                             opts["planet"])
            except Exception as e:  # noqa: BLE001
                L.exception("ワールド作成失敗")
                self.status = f"作成できません: {e}"
                self.show_new()
                return
            self.selected = name
            self.status = f"「{name}」を作成しました"
            if play:
                self.on_play(name)
            else:
                self.show_main()
        self._button("作成して遊ぶ", (-0.22, -0.36), lambda: create(True), col=U["color"].rgb(0.2, 0.5, 0.3))
        self._button("作成", (0.12, -0.36), lambda: create(False))
        self._button("戻る", (0.46, -0.36), self._back, size=(0.2, 0.055), col=U["color"].rgb(0.3, 0.3, 0.33))
        self._status_line()

    def _mod_toggles(self, y, selected: set):
        U = self.U
        mods = list_mods()
        btns = {}
        on_col = U["color"].rgb(0.25, 0.5, 0.35)
        off_col = U["color"].rgb(0.2, 0.2, 0.24)
        self._text("MOD（押して切替）", (-0.38, y), 0.85, origin=(-0.5, 0))
        self._text("サバイバル無し＝クリエイティブ", (-0.38, y - 0.04), 0.58, origin=(-0.5, 0),
                   col=U["color"].light_gray)
        if not mods:
            self._text("（mods フォルダに MOD がありません）", (0.12, y), 0.75, col=U["color"].light_gray)
            return
        for k, (mid, mname) in enumerate(mods[:6]):
            x = -0.02 + (k % 3) * 0.24
            yy = y - (k // 3) * 0.06

            def flip(m=mid):
                if m in selected:
                    selected.discard(m)
                else:
                    selected.add(m)
                btns[m].color = on_col if m in selected else off_col
            btns[mid] = self._button(mname, (x + 0.1, yy), flip, size=(0.22, 0.05),
                                     col=on_col if mid in selected else off_col)

    def _back(self):
        self.status = ""
        self.show_main()

    # --- 削除 -----------------------------------------------------------
    def show_delete(self, name):
        if not name:
            return
        self._clear()
        U = self.U
        self._text("ワールドの削除", (0, 0.25), 1.8)
        self._text(f"「{name}」を削除しますか？\nセーブデータ・スクリプト・出力したOBJもすべて消え、元に戻せません。",
                   (0, 0.1), 1.0)

        def do():
            try:
                done = delete_world(name)
                self.status = f"「{name}」を削除しました" + (
                    "" if done else "（使用中で消せなかったファイルは次回起動時に片付けます）")
                self.selected = None
            except Exception as e:  # noqa: BLE001
                L.exception("削除失敗")
                self.status = f"削除できません: {e}"
            self.show_main()
        self._button("削除する", (-0.18, -0.08), do, col=U["color"].rgb(0.6, 0.2, 0.2))
        self._button("やめる", (0.18, -0.08), self._back)

    # --- ワールド設定 ---------------------------------------------------
    def show_world_settings(self, name):
        if not name:
            return
        self._clear()
        U = self.U
        s, mods = read_world_settings(name)
        sel = set(mods)
        cl = s["cleanup"]
        self._text(f"ワールド設定: {name}", (0, 0.42), 1.6)
        y = 0.31
        self._toggle("完全剛体モード", y, s.get("full_rigid", False), lambda v: s.__setitem__("full_rigid", v))
        self._text("構築物は丸ごと1つの剛体（曲がらない・壊れない）", (0.43, y), 0.62,
                   col=U["color"].light_gray, origin=(-0.5, 0))
        self._stepper("ボクセルサイズ（新規設置）", y - 0.07, [0.25, 0.5, 1.0, 2.0], s.get("voxel_size", 0.5),
                      lambda v: f"{v} m", lambda v: s.__setitem__("voxel_size", v))
        self._stepper("重力倍率", y - 0.14, [0.0, 0.25, 0.5, 1.0, 1.5, 2.0], s.get("gravity_scale", 1.0),
                      lambda v: f"×{v}", lambda v: s.__setitem__("gravity_scale", v))
        self._toggle("構築物の自動削除", y - 0.21, cl.get("enabled", True), lambda v: cl.__setitem__("enabled", v))
        self._stepper("　削除する距離", y - 0.28, [200.0, 500.0, 1000.0, 1500.0, 3000.0, 5000.0, 10000.0],
                      float(cl.get("distance", 1500)), lambda v: f"{v:.0f} m", lambda v: cl.__setitem__("distance", v))
        self._stepper("　離れている時間", y - 0.35, [10.0, 30.0, 60.0, 120.0, 300.0, 600.0],
                      float(cl.get("seconds", 60)), lambda v: f"{v:.0f} 秒", lambda v: cl.__setitem__("seconds", v))
        self._stepper("　対象の最大頂点数", y - 0.42, [80, 200, 400, 800, 1600, 4000],
                      int(cl.get("max_vertices", 400)), lambda v: f"{v}", lambda v: cl.__setitem__("max_vertices", v))
        self._mod_toggles(y - 0.50, sel)
        planets = ", ".join(p.get("def", "?") for p in s.get("planets", []))
        self._text(f"惑星: {planets}（world.json で編集）", (0, y - 0.64), 0.7, col=U["color"].light_gray)
        self._text("MOD を入れる・外す前に、ワールド（saves/ のフォルダ）をコピーしておくことをすすめます。"
                   "使っている MOD を外すと、そのワールドは始められなくなります。", (0, y - 0.70), 0.58,
                   col=U["color"].rgb(1, 0.8, 0.45))

        def save():
            try:
                write_world_settings(name, s, sorted(sel))
                self.status = "ワールド設定を保存しました"
            except Exception as e:  # noqa: BLE001
                self.status = f"保存できません: {e}"
            self.show_main()
        self._button("保存", (0.72, 0.42), save, size=(0.2, 0.055), col=U["color"].rgb(0.2, 0.5, 0.3))
        self._button("戻る", (0.72, 0.35), self._back, size=(0.2, 0.055), col=U["color"].rgb(0.3, 0.3, 0.33))

    # --- ゲーム設定 -----------------------------------------------------
    def show_game_settings(self):
        self._clear()
        U = self.U
        cfg = json.loads(json.dumps(self.cfg))
        w, g, c, ph = cfg["window"], cfg["graphics"], cfg["controls"], cfg["physics"]
        self._text("ゲーム設定", (0, 0.42), 1.8)
        y = 0.31
        res = (w.get("width", 1280), w.get("height", 720))
        res_opts = RESOLUTIONS if res in RESOLUTIONS else [res] + RESOLUTIONS

        def set_res(v):
            w["width"], w["height"] = v
        self._stepper("解像度", y, res_opts, res, lambda v: f"{v[0]}×{v[1]}", set_res)
        self._toggle("フルスクリーン", y - 0.07, w.get("fullscreen", False), lambda v: w.__setitem__("fullscreen", v))
        self._toggle("垂直同期（再起動後）", y - 0.14, w.get("vsync", True), lambda v: w.__setitem__("vsync", v))
        self._stepper("視野角", y - 0.21, list(range(50, 115, 5)), int(g.get("fov", 80)), lambda v: f"{v}°",
                      lambda v: g.__setitem__("fov", v))
        self._stepper("描画距離", y - 0.28, [500, 1000, 2000, 3000, 5000, 10000, 20000],
                      int(g.get("view_distance", 3000)), lambda v: f"{v} m", lambda v: g.__setitem__("view_distance", v))
        self._stepper("地表の細かさ", y - 0.35, [24, 32, 48, 64, 96], int(g.get("terrain_patch_cells", 48)),
                      lambda v: f"{v * 2 + 1}×{v * 2 + 1}", lambda v: g.__setitem__("terrain_patch_cells", v))
        self._stepper("マウス感度", y - 0.42, list(range(10, 210, 10)), int(c.get("mouse_sensitivity", 60)),
                      lambda v: f"{v}", lambda v: c.__setitem__("mouse_sensitivity", float(v)))
        self._stepper("物理サブステップ（多いほど正確・重い）", y - 0.49, [1, 2, 3, 4, 6, 8], int(ph.get("substeps", 3)),
                      lambda v: f"{v}", lambda v: ph.__setitem__("substeps", v))
        self._stepper("物理反復回数（多いほど硬い・重い）", y - 0.56, [1, 2, 3, 4, 6], int(ph.get("iterations", 2)),
                      lambda v: f"{v}", lambda v: ph.__setitem__("iterations", v))
        self._text("キー割り当て・フォントは config/settings.json で変更できます", (0, y - 0.65), 0.7,
                   col=U["color"].light_gray)

        def save():
            self.cfg.clear()
            self.cfg.update(cfg)
            try:
                save_config(self.cfg)
                apply_window_settings(self.cfg)
                self.status = "ゲーム設定を保存しました"
            except Exception as e:  # noqa: BLE001
                self.status = f"保存できません: {e}"
            self.show_main()

        def reset():
            self.cfg.clear()
            self.cfg.update(json.loads(json.dumps(config_mod.DEFAULTS)))
            self.show_game_settings()
        self._button("保存", (0.62, -0.17), save, size=(0.2, 0.055), col=U["color"].rgb(0.2, 0.5, 0.3))
        self._button("初期値に戻す", (0.62, -0.25), reset, size=(0.2, 0.055))
        self._button("戻る", (0.62, -0.33), self._back, size=(0.2, 0.055), col=U["color"].rgb(0.3, 0.3, 0.33))

    def destroy(self):
        from .ui_destroy import destroy
        destroy(self.root)


def apply_window_settings(cfg):
    from ursina import window, Vec2
    w = cfg["window"]
    try:
        if bool(window.fullscreen) != bool(w.get("fullscreen", False)):
            window.fullscreen = bool(w.get("fullscreen", False))
        if not w.get("fullscreen", False):
            window.size = Vec2(w.get("width", 1280), w.get("height", 720))
    except Exception as e:  # noqa: BLE001
        L.warning("ウィンドウ設定の反映に失敗: %s", e)


# ---------------------------------------------------------------------------
# クライアント全体の流れ
# ---------------------------------------------------------------------------
def run_client(cfg, world_name=None, on_ready=None):
    from ursina import Ursina, Text, application, invoke
    from .game import Game, _find_font
    font = _find_font(cfg)
    if font:
        application.fonts_folder = font.parent
        Text.default_font = font.name
    else:
        L.warning("日本語フォントが見つかりません。config/settings.json の graphics.font にパスを指定してください")
    wcfg = cfg["window"]
    app = Ursina(title="Alpedia", borderless=False, fullscreen=wcfg.get("fullscreen", False),
                 size=(wcfg.get("width", 1280), wcfg.get("height", 720)), vsync=wcfg.get("vsync", True),
                 development_mode=False)
    try:
        from ursina import window
        window.exit_button.enabled = False
        window.fps_counter.enabled = True
    except Exception:  # noqa: BLE001
        pass
    _setup_window_icon()
    purge_trash()
    state = {"menu": None, "game": None}

    def quit_app():
        application.quit()

    def open_menu(status=""):
        state["menu"] = MainMenu(cfg, play, quit_app)
        if status:
            state["menu"].error(status)

    def load_failed(name, errors):
        if state["menu"] is None:
            open_menu()
        state["menu"].show_load_errors(name, errors)

    def play(name):
        from ..app import prepare_world
        from ..modcode import ModLoadError
        try:
            World = prepare_world(name)
            world = World(name, cfg).load()
        except ModLoadError as e:
            L.error("ワールド「%s」を始められません（MOD のエラー）:\n%s", name, "\n".join(e.errors))
            load_failed(name, e.errors)
            return
        except Exception as e:  # noqa: BLE001
            L.exception("ワールドの読み込みに失敗: %s", name)
            if state["menu"] is None:
                open_menu()
            state["menu"].error(f"「{name}」を読み込めません: {e}")
            return
        if state["menu"] is not None:
            state["menu"].destroy()
            state["menu"] = None
        game = Game(world, cfg, on_exit=leave)
        state["game"] = game
        try:
            game.start()
        except ModLoadError as e:
            L.error("ワールド「%s」を始められません（MOD のエラー）:\n%s", name, "\n".join(e.errors))
            try:
                game.destroy()
            except Exception:  # noqa: BLE001
                L.exception("画面の後片付けに失敗")
            state["game"] = None
            load_failed(name, e.errors)

    def leave(action):
        def later():
            g = state["game"]
            if g is not None:
                g.destroy()
                state["game"] = None
            if action == "quit":
                quit_app()
            else:
                open_menu()
        invoke(later, delay=0.01)

    if world_name:
        play(world_name)
    else:
        open_menu()
    if on_ready:  # テスト用
        on_ready(state, play, leave)
    app.run()
