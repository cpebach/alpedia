"""起動時のメニュー画面：ワールドの選択・新規作成・削除・ワールド設定・ゲーム設定。"""
import json
import shutil
import time

from .. import paths, log, config as config_mod

L = log.get("menu")

NAME_CHARS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
RESOLUTIONS = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440), (3840, 2160)]
PAGE_SIZE = 6


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
            if isinstance(s.get("story"), dict):
                info["story"] = s["story"]
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
        s2, info["mods"], _ = upgrade_world(s, info["mods"])      # 古い形式のワールドも今の形で表示
        info["mode"] = s2.get("mode", "creative")
        info["mtime"] = max((f.stat().st_mtime for f in d.glob("*.json")), default=wj.stat().st_mtime)
        out.append(info)
    out.sort(key=lambda x: x["mtime"], reverse=True)
    return out


def world_mode_text(w):
    """一覧に出す遊び方：ストーリーなら今の章、ほかはサバイバル／クリエイティブと構築物の数。"""
    st = w.get("story")
    if isinstance(st, dict):
        if st.get("done"):
            return "ストーリー（クリア）"
        if not st.get("built"):
            return "ストーリー（はじめから）"
        try:
            ch = int(st.get("ch", 0))
        except (TypeError, ValueError):
            return "ストーリー"
        names = ["序章"] + [f"第{k}章" for k in range(1, 13)] + ["終章", "エピローグ"]
        return "ストーリー：" + (names[ch] if 0 <= ch < len(names) else f"{ch}")
    mode = "サバイバル" if w.get("mode") == "survival" else "クリエイティブ"
    return f"{mode}  /  構築物 {w['constructs']}"


def list_mods():
    """[(MOD名, 表示名)]。要素を足す MOD（シナリオは除く）。本体に入っている MOD を先に並べる。"""
    from .. import modmeta
    return [(m["id"], m.get("name", m["id"])) for m in modmeta.list_all(kind="addon")]


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
    raw, mods, _ = upgrade_world(raw, mods)      # 古い形式（creative / cities・サバイバルの MOD）を読み替える
    s.update(raw)
    s["cleanup"] = {**DEFAULT_SETTINGS["cleanup"], **s.get("cleanup", {})}
    return s, mods


def write_world_settings(name, settings, mods):
    d = paths.world_dir(name)
    (d / "world.json").write_text(json.dumps(settings, ensure_ascii=False, indent=1), encoding="utf-8")
    (d / "mods.json").write_text(json.dumps({"enabled": list(mods)}, ensure_ascii=False, indent=1), encoding="utf-8")
    L.info("ワールド設定を保存: %s", name)


PLANETS = {"terra": "球体（テラ）", "gaia": "地球型（ガイア）"}
MODES = {"creative": "クリエイティブ", "survival": "サバイバル"}
MODE_NOTES = {"creative": "何でも置ける（資源なし）", "survival": "置くのに資源が要る・目標がある"}


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


def create_world(name, settings_patch, mods, planet="terra"):
    """ワールドを作る（選んだ MOD に依存する MOD も入れる。シナリオなどワールドの形を決める MOD があればその形で）。"""
    from ..tutorial import create_world as _create
    _create(name, mods, settings_patch, planet)


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
    def __init__(self, cfg, on_play, on_quit, on_join=None):
        from ursina import Entity, Text, Button, InputField, camera, color, window, mouse
        self.U = dict(Entity=Entity, Text=Text, Button=Button, InputField=InputField, camera=camera,
                      color=color, window=window)
        self.cfg = cfg
        self.on_play = on_play
        self.on_quit = on_quit
        self.on_join = on_join
        self.browser = None
        self.selected = None
        self.page_index = 0
        self.status = ""
        self._new = None            # 新規作成の入力（MOD の画面へ行って戻っても残す）
        self._ws = None             # ワールド設定の入力（同上）
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
        self._text("Alpedia", (0, 0.44), 2.6)
        self._text("物理演算ボクセルサンドボックス", (0, 0.385), 0.8, col=U["color"].light_gray)
        # 遊び方を選ぶ（新しく始める）
        modes = [("シナリオ", "物語を選んで始める", self.show_scenarios, U["color"].rgb(0.45, 0.3, 0.6)),
                 ("サバイバル", "資源を集めて作る・月をめざす", lambda: self._quick_new("survival"), U["color"].rgb(0.25, 0.45, 0.3)),
                 ("クリエイティブ", "何でも置ける・自由に作る", lambda: self._quick_new("creative"), U["color"].rgb(0.25, 0.38, 0.55))]
        for k, (title, sub, fn, col) in enumerate(modes):
            x = -0.52 + k * 0.35
            self._button(title, (x, 0.29), fn, size=(0.32, 0.075), col=col)
            self._text(sub, (x, 0.235), 0.6, col=U["color"].light_gray)
        # 続きから（ワールド一覧）
        self._panel((-0.17, -0.06), (0.98, 0.44))
        self._text("続きから", (-0.64, 0.135), 0.95, origin=(-0.5, 0))
        pages = max(1, (len(worlds) + PAGE_SIZE - 1) // PAGE_SIZE)
        self.page_index = min(self.page_index, pages - 1)
        start = self.page_index * PAGE_SIZE
        if not worlds:
            self._text("まだワールドがありません。上のボタンで始めてください", (-0.17, -0.05), 0.85,
                       col=U["color"].light_gray)
        for k, w in enumerate(worlds[start:start + PAGE_SIZE]):
            y = 0.075 - k * 0.062
            sel = w["name"] == self.selected
            col = U["color"].rgb(0.25, 0.45, 0.65) if sel else U["color"].rgb(0.15, 0.17, 0.22)
            when = time.strftime("%Y-%m-%d %H:%M", time.localtime(w["mtime"]))
            label = f"{w['name']}    {when}  /  {world_mode_text(w)}"
            if w.get("broken"):
                label = f"{w['name']}    （world.json が壊れています）"
            b = self._button(label, (-0.17, y), (lambda n=w["name"]: self._select(n)), size=(0.94, 0.054), col=col)
            b.text_entity.origin = (-0.5, 0)
            b.text_entity.x = -0.47
        if pages > 1:
            self._button("▲", (0.27, 0.135), lambda: self._page(-1), size=(0.045, 0.04))
            self._text(f"{self.page_index + 1}/{pages}", (0.18, 0.135), 0.75)
            self._button("▼", (0.27, -0.255), lambda: self._page(1), size=(0.045, 0.04))
        # 操作ボタン
        has = self.selected is not None
        x = 0.55
        self._button("遊ぶ", (x, 0.1), self._play, size=(0.3, 0.07), col=U["color"].rgb(0.2, 0.5, 0.3), enabled=has)
        self._button("ワールド設定", (x, 0.02), lambda: self.show_world_settings(self.selected), enabled=has)
        self._button("削除", (x, -0.05), lambda: self.show_delete(self.selected), col=U["color"].rgb(0.5, 0.2, 0.2),
                     enabled=has)
        self._button("詳しく設定して作る…", (x, -0.12), self.show_new)
        self._button("マルチプレイ（LAN）", (x, -0.19), self.show_multiplayer, col=U["color"].rgb(0.25, 0.35, 0.55))
        self._button("ゲーム設定", (x, -0.26), self.show_game_settings)
        self._button("終了", (x, -0.33), self.on_quit, col=U["color"].rgb(0.3, 0.3, 0.33))
        if has:
            self._text(f"選択中: {self.selected}", (-0.17, -0.31), 0.8, col=U["color"].light_gray)
        self._status_line()

    def _free_name(self, base):
        names = {w["name"] for w in list_worlds()}
        k = 1
        while f"{base}{k}" in names or paths.world_dir(f"{base}{k}").exists():
            k += 1
        return f"{base}{k}"

    def _quick_new(self, mode):
        """サバイバル・クリエイティブを、おすすめの設定（惑星ガイア）ですぐ始める。"""
        name = self._free_name(mode)
        # MOD は入れない（街などは「詳しく設定して作る」の MOD の選択で）。完全剛体モード（軽い）
        mods = []
        patch = {"mode": mode, "full_rigid": True}
        if mode == "creative":
            patch["starter_sample"] = True          # クリエイティブ：見本のロケットを置く
        try:
            create_world(name, patch, mods, planet="gaia")
        except Exception as e:  # noqa: BLE001
            self.status = f"作れません: {e}"
            self.show_main()
            return
        self.selected = name
        self.on_play(name)

    # --- シナリオを選ぶ ---------------------------------------------------
    def show_scenarios(self, page=0):
        """シナリオの MOD（mod.json の "type": "scenario"）の一覧。選ぶと新しいワールドで最初から始める。"""
        self._clear()
        U = self.U
        from .. import modmeta
        sc = modmeta.list_all(kind="scenario")
        self._text("シナリオを選ぶ", (0, 0.42), 1.8)
        self._text("選んだシナリオを最初から始めます（途中のシナリオは「続きから」に残ります）", (0, 0.36), 0.7,
                   col=U["color"].light_gray)
        rows = 4
        pages = max(1, (len(sc) + rows - 1) // rows)
        page = max(0, min(page, pages - 1))
        if not sc:
            self._text("シナリオがありません（mods/ に \"type\": \"scenario\" の MOD を入れると出ます）", (0, 0.1), 0.8,
                       col=U["color"].light_gray)
        for k, m in enumerate(sc[page * rows:(page + 1) * rows]):
            y = 0.24 - k * 0.15
            self._panel((0.0, y - 0.02), (1.5, 0.13), col=U["color"].rgba(1, 1, 1, 0.06))
            self._text(m.get("name", m["id"]), (-0.7, y + 0.02), 1.0, origin=(-0.5, 0))
            desc = (m.get("description") or "").replace("\n", " ")
            lines = [desc[i:i + 52] for i in range(0, min(len(desc), 104), 52)]
            if len(desc) > 104:
                lines[-1] = lines[-1][:-1] + "…"
            self._text("\n".join(lines), (-0.7, y - 0.035), 0.6, origin=(-0.5, 0), col=U["color"].light_gray)
            ok = m["exists"] and not m.get("error")
            self._button("始める", (0.62, y - 0.01), (lambda mid=m["id"]: self._start_scenario(mid)),
                         size=(0.16, 0.06), col=U["color"].rgb(0.45, 0.3, 0.6), enabled=ok)
        if pages > 1:
            self._button("◀ 前", (-0.2, -0.36), lambda: self.show_scenarios(page - 1), size=(0.14, 0.05))
            self._text(f"{page + 1}/{pages}", (0, -0.36), 0.8)
            self._button("次 ▶", (0.2, -0.36), lambda: self.show_scenarios(page + 1), size=(0.14, 0.05))
        self._button("戻る", (0.62, -0.42), self._back, size=(0.2, 0.055), col=U["color"].rgb(0.3, 0.3, 0.33))
        self._status_line()

    def _start_scenario(self, mod_id):
        """シナリオを最初から（新しいワールド）。"""
        name = self._free_name("story" if mod_id == "story" else mod_id)
        try:
            create_world(name, {}, [mod_id], planet="gaia")
        except Exception as e:  # noqa: BLE001
            self.status = f"作れません: {e}"
            self.show_main()
            return
        self.selected = name
        self.on_play(name)

    # --- マルチプレイ（LAN） -------------------------------------------------
    def show_multiplayer(self):
        """LAN の中で公開中のワールドの一覧と、IP アドレスを入れて参加する画面。"""
        from .. import net
        self._clear()
        U = self.U
        mp = self.cfg.setdefault("multiplayer", {})
        if self.browser is None:
            self.browser = net.Browser()
        self._text("マルチプレイ（LAN）", (0, 0.42), 1.8)
        self._text("名前", (-0.62, 0.32), 0.9, origin=(-0.5, 0))
        name = U["InputField"](parent=self.root, default_value=mp.get("name") or _default_name(), position=(-0.2, 0.32),
                               scale=(0.5, 0.05), character_limit=24)
        self._panel((-0.2, 0.03), (0.92, 0.46))
        self._text("LAN で公開中のワールド", (-0.64, 0.23), 0.9, origin=(-0.5, 0))
        rows = self.U["Entity"](parent=self.root)
        state = {"key": None, "t": 0.0}

        def player_name():
            n = name.text_field.text.strip() if hasattr(name, "text_field") else name.text.strip()
            return n or _default_name()

        def join(host, port):
            mp["name"] = player_name()
            try:
                save_config(self.cfg)
            except Exception:  # noqa: BLE001
                pass
            self._close_browser()
            if self.on_join:
                self.on_join(host, port, mp["name"])

        def redraw():
            from .ui_destroy import destroy
            found = self.browser.worlds() if self.browser else []
            key = tuple((ip, port, info.get("world"), info.get("players")) for ip, port, info in found)
            if key == state["key"]:
                return
            state["key"] = key
            for ch in list(rows.children):
                destroy(ch)
            if not found:
                msg = "探しています…（同じ LAN でホストが「LAN に公開」すると、ここに出ます）"
                if self.browser is not None and self.browser.error:
                    msg = f"LAN の知らせを受け取れません（{self.browser.error}）。下で IP アドレスを入れて参加してください"
                U["Text"](parent=rows, text=msg, position=(-0.2, 0.1), origin=(0, 0), scale=0.75,
                          color=U["color"].light_gray)
            for k, (ip, port, info) in enumerate(found[:5]):
                y = 0.16 - k * 0.07
                label = (f"{info.get('world', '?')}   ホスト {info.get('host', '?')}  /  {info.get('players', 1)} 人"
                         f"  /  {ip}:{port}")
                b = U["Button"](parent=rows, text=label, position=(-0.2, y), scale=(0.88, 0.06),
                                color=U["color"].rgb(0.15, 0.25, 0.35), text_size=0.8)
                b.text_entity.origin = (-0.5, 0)
                b.text_entity.x = -0.47
                b.on_click = (lambda ip=ip, port=port: join(ip, port))

        class Ticker(U["Entity"]):
            def update(self_):
                import time as _t
                if _t.time() - state["t"] > 0.5:
                    state["t"] = _t.time()
                    redraw()
        Ticker(parent=self.root)
        redraw()
        self._text("IP アドレスで参加", (-0.64, -0.26), 0.85, origin=(-0.5, 0))
        addr = U["InputField"](parent=self.root, default_value=mp.get("last_host", ""), position=(-0.12, -0.26),
                               scale=(0.36, 0.05), character_limit=40)

        def join_addr():
            txt = (addr.text_field.text if hasattr(addr, "text_field") else addr.text).strip()
            if not txt:
                return
            host, _, port = txt.partition(":")
            mp["last_host"] = txt
            join(host.strip(), int(port) if port.strip().isdigit() else int(mp.get("port", net.PORT)))
        self._button("参加", (0.18, -0.26), join_addr, size=(0.16, 0.05), col=U["color"].rgb(0.2, 0.5, 0.3))
        self._text("ホストになるには：ワールドで遊んで Esc →「LAN に公開」。ワールド・セーブ・MOD はホストの PC のものを使い、\n"
                   "参加する人には MOD が自動で届きます。保存はホストが行います（参加者は F9 で構築物を自分の PC に出力できます）。",
                   (0, -0.36), 0.62, col=U["color"].light_gray)

        def back():
            self._close_browser()
            self._back()
        self._button("戻る", (0.5, 0.42), back, size=(0.2, 0.055), col=U["color"].rgb(0.3, 0.3, 0.33))
        self._status_line()

    def _close_browser(self):
        if self.browser is not None:
            self.browser.close()
            self.browser = None

    def show_joining(self, joiner, on_cancel):
        """参加の準備中（MOD・ワールドを受け取っている）。"""
        self._clear()
        U = self.U
        self._text("マルチプレイに参加", (0, 0.2), 1.6)
        msg = self._text(joiner.status, (0, 0.05), 0.95)

        class Ticker(U["Entity"]):
            def update(self_):
                if msg.text != joiner.status:
                    msg.text = joiner.status
        Ticker(parent=self.root)
        self._button("やめる", (0, -0.15), on_cancel, size=(0.25, 0.055), col=U["color"].rgb(0.4, 0.3, 0.2))

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
    def show_new(self, keep=False):
        self._clear()
        U = self.U
        if not keep or getattr(self, "_new", None) is None:
            names = {w["name"] for w in list_worlds()}
            k = 1
            while f"world{k}" in names:
                k += 1
            self._new = {"name": f"world{k}", "planet": "gaia", "voxel_size": 0.5, "gravity_scale": 1.0,
                         "cleanup": True, "full_rigid": True, "mode": "creative", "mods": set()}
        opts = self._new
        self._text("新しいワールド", (0, 0.42), 1.8)
        self._text("名前（半角英数字・- _）", (-0.38, 0.3), 0.85, origin=(-0.5, 0))
        field = U["InputField"](parent=self.root, default_value=opts["name"], position=(0.2, 0.3),
                                scale=(0.34, 0.05), character_limit=32, active=True)
        field.limit_content_to = NAME_CHARS

        def name_now():
            return (field.text_field.text if hasattr(field, "text_field") else field.text).strip()
        from .. import modmeta
        pre_id, preset = modmeta.world_preset(sorted(opts["mods"]))
        y = 0.21

        def redraw():
            opts["name"] = name_now() or opts["name"]
            self.show_new(keep=True)
        self._stepper("遊び方", y, ["creative", "survival"], opts.get("mode", "creative"), lambda v: MODES[v],
                      lambda v: (opts.__setitem__("mode", v), redraw()))
        self._text(MODE_NOTES[opts.get("mode", "creative")], (0.2, y - 0.035), 0.6, col=U["color"].light_gray)
        y -= 0.075
        if preset:
            self._text(f"ワールドの形：{modmeta.name_of(pre_id)}", (-0.38, y), 0.85, origin=(-0.5, 0),
                       col=U["color"].rgb(1.0, 0.85, 0.45))
            self._text(preset.get("note", "惑星・大きさは MOD が決めます"), (-0.38, y - 0.045), 0.62, origin=(-0.5, 0),
                       col=U["color"].light_gray)
            y -= 0.07
            self._toggle("完全剛体モード", y - 0.02, opts["full_rigid"],
                         lambda v: (opts.__setitem__("full_rigid", v), redraw()))
            self._rigid_note((0.2, y - 0.055), opts["full_rigid"], centered=True)
            y -= 0.07
        else:
            self._stepper("惑星", y, list(PLANETS), opts["planet"],
                          lambda v: PLANETS[v], lambda v: opts.__setitem__("planet", v))
            self._text("地球型：山・洞窟・海・池がある", (0.2, y - 0.035), 0.6, col=U["color"].light_gray)
            self._stepper("ボクセルサイズ", y - 0.07, [0.25, 0.5, 1.0, 2.0], opts["voxel_size"], lambda v: f"{v} m",
                          lambda v: opts.__setitem__("voxel_size", v))
            self._stepper("重力倍率", y - 0.14, [0.0, 0.25, 0.5, 1.0, 1.5, 2.0], opts["gravity_scale"],
                          lambda v: f"×{v}", lambda v: opts.__setitem__("gravity_scale", v))
            self._toggle("構築物の自動削除", y - 0.21, opts["cleanup"], lambda v: opts.__setitem__("cleanup", v))
            self._toggle("完全剛体モード", y - 0.28, opts["full_rigid"],
                         lambda v: (opts.__setitem__("full_rigid", v), redraw()))
            self._rigid_note((0.2, y - 0.315), opts["full_rigid"], centered=True)
            y -= 0.30
        y -= 0.09

        def open_mods():
            opts["name"] = name_now() or opts["name"]
            self.show_mods(opts["mods"], lambda: self.show_new(keep=True), "新しいワールドの MOD")
        self._mod_summary(y, opts["mods"], open_mods)

        def create(play):
            name = name_now()
            if not name:
                self.status = "名前を入力してください"
                opts["name"] = ""
                self.show_new(keep=True)
                return
            try:
                create_world(name, {"mode": opts.get("mode", "creative"),
                                    "voxel_size": opts["voxel_size"], "gravity_scale": opts["gravity_scale"],
                                    "cleanup": {"enabled": opts["cleanup"]}, "full_rigid": bool(opts.get("full_rigid", True))},
                             sorted(opts["mods"]), opts["planet"])
            except Exception as e:  # noqa: BLE001
                L.exception("ワールド作成失敗")
                self.status = f"作成できません: {e}"
                opts["name"] = name
                self.show_new(keep=True)
                return
            self._new = None
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

    def _rigid_note(self, pos, on, centered=False, short=False):
        """完全剛体モードの説明。OFF（やわらかいブロック）は重くなることがあるので、そのことを目立たせる。"""
        U = self.U
        if on:
            txt = "1つの剛体で軽い（おすすめ）" if short else "ON：構築物は丸ごと1つの剛体（曲がらない・壊れない。軽い。おすすめ）"
            col = U["color"].light_gray
        else:
            txt = ("やわらかい。負荷が増え重くなることも" if short
                   else "OFF：やわらかいブロック（曲がる・ちぎれる）。負荷が増え、重くなることがあります")
            col = U["color"].orange
        self._text(txt, pos, 0.58, col=col, origin=(0, 0) if centered else (-0.5, 0))

    def _mod_summary(self, y, selected, open_mods):
        """MOD の行：選んでいる MOD の名前と「MOD を選ぶ…」ボタン（MOD の画面へ）。"""
        U = self.U
        from .. import modmeta
        self._text("MOD", (-0.38, y), 0.85, origin=(-0.5, 0))
        order, _, _ = modmeta.resolve(sorted(selected))
        names = [modmeta.name_of(m) for m in order] or ["なし"]
        txt = "、".join(names)
        if len(txt) > 40:
            txt = txt[:38] + "…"
        self._text(txt, (-0.25, y), 0.7, origin=(-0.5, 0), col=U["color"].light_gray)
        self._text(f"{len(order)} 個", (-0.25, y - 0.035), 0.58, origin=(-0.5, 0),
                   col=U["color"].gray)
        self._button("MOD を選ぶ…", (0.55, y - 0.012), open_mods, size=(0.22, 0.055), col=U["color"].rgb(0.3, 0.38, 0.55))

    # --- MOD の選択（新規作成・ワールド設定から） --------------------------------
    MOD_ROWS = 6

    def show_mods(self, selected: set, done, title="MOD", page=0, note=""):
        """MOD の一覧（説明・必要な MOD）。押すと入れる／外す。入れると必要な MOD も一緒に入る。"""
        self._clear()
        U = self.U
        from .. import modmeta
        mods = modmeta.list_all(kind="addon")          # シナリオはメニューの「シナリオ」から選ぶ
        on_col = U["color"].rgb(0.25, 0.5, 0.35)
        off_col = U["color"].rgb(0.2, 0.2, 0.24)
        self._text(title, (0, 0.44), 1.6)
        self._text("押すと入れる・外す。ほかの MOD が必要な MOD を入れると、必要な MOD も一緒に入ります（★＝必要として入った）",
                   (0, 0.385), 0.62, col=U["color"].light_gray)
        order, _, _ = modmeta.resolve(sorted(selected))
        auto = [m for m in order if m not in selected]
        pages = max(1, (len(mods) + self.MOD_ROWS - 1) // self.MOD_ROWS)
        page = max(0, min(page, pages - 1))
        names = {m["id"]: m.get("name", m["id"]) for m in mods}

        def flip(mid):
            msg = ""
            if mid in order:
                need = [m for m in modmeta.dependents(mid, order) if m in order]
                selected.discard(mid)
                if need:
                    for m in need:
                        selected.discard(m)
                    msg = f"「{names.get(mid, mid)}」を使う " + "、".join(names.get(m, m) for m in need) + " も外しました"
            else:
                selected.add(mid)
                o2, _, miss = modmeta.resolve(sorted(selected))
                add = [m for m in o2 if m not in order and m != mid]
                if add:
                    msg = f"「{names.get(mid, mid)}」に必要な " + "、".join(names.get(m, m) for m in add) + " も入れました"
                if miss:
                    msg += "（見つからない MOD：" + "、".join(m for m, _ in miss) + "）"
            self.show_mods(selected, done, title, page, msg)
        for k, m in enumerate(mods[page * self.MOD_ROWS:(page + 1) * self.MOD_ROWS]):
            y = 0.30 - k * 0.11
            mid = m["id"]
            on = mid in order
            mark = ("★ " if mid in auto else "") + ("ON" if on else "OFF")
            self._panel((0.0, y - 0.018), (1.62, 0.1), col=U["color"].rgba(1, 1, 1, 0.05 if not on else 0.09))
            self._button(mark, (-0.72, y - 0.012), (lambda mid=mid: flip(mid)), size=(0.12, 0.05),
                         col=on_col if on else off_col)
            cat = f"［{m['category']}］ " if m.get("category") else ""
            self._text(f"{cat}{m.get('name', mid)}", (-0.64, y + 0.012), 0.85, origin=(-0.5, 0))
            desc = (m.get("description") or "").replace("\n", " ")
            if len(desc) > 92:
                desc = desc[:90] + "…"
            self._text(desc, (-0.64, y - 0.022), 0.58, origin=(-0.5, 0), col=U["color"].light_gray)
            extra = []
            if m["requires"]:
                extra.append("必要な MOD：" + "、".join(names.get(r, r + "（見つかりません）") for r in m["requires"]))
            if isinstance(m.get("world"), dict):
                extra.append("新しいワールドを作るときに入れると、このワールドの形になります")
            if m.get("error"):
                extra.append(m["error"])
            if extra:
                self._text("　".join(extra), (-0.64, y - 0.05), 0.56, origin=(-0.5, 0),
                           col=U["color"].rgb(1.0, 0.82, 0.45))
        if pages > 1:
            self._button("◀ 前", (-0.2, -0.36), lambda: self.show_mods(selected, done, title, page - 1), size=(0.14, 0.05))
            self._text(f"{page + 1}/{pages}", (0, -0.36), 0.8)
            self._button("次 ▶", (0.2, -0.36), lambda: self.show_mods(selected, done, title, page + 1), size=(0.14, 0.05))
        if note:
            self._text(note, (0, -0.30), 0.7, col=U["color"].orange)
        self._text(f"入る MOD：{len(order)} 個", (-0.72, -0.42), 0.7, origin=(-0.5, 0), col=U["color"].light_gray)
        self._button("決定", (0.62, -0.42), done, size=(0.2, 0.055), col=U["color"].rgb(0.2, 0.5, 0.3))

    def _back(self):
        self.status = ""
        self._new = None
        self._ws = None
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
    def show_world_settings(self, name, keep=False):
        if not name:
            return
        self._clear()
        U = self.U
        if keep and getattr(self, "_ws", None) and self._ws[0] == name:
            _, s, sel = self._ws
        else:
            s, mods = read_world_settings(name)
            sel = set(mods)
        cl = s["cleanup"]
        self._text(f"ワールド設定: {name}", (0, 0.42), 1.6)
        y = 0.31

        def redraw():
            self._ws = (name, s, sel)
            self.show_world_settings(name, keep=True)
        self._stepper("遊び方", y, ["creative", "survival"], s.get("mode", "creative"), lambda v: MODES[v],
                      lambda v: (s.__setitem__("mode", v), redraw()))
        self._text(MODE_NOTES[s.get("mode", "creative")], (0.43, y), 0.62, col=U["color"].light_gray, origin=(-0.5, 0))
        self._toggle("完全剛体モード", y - 0.065, s.get("full_rigid", True),
                     lambda v: (s.__setitem__("full_rigid", v), redraw()))
        self._rigid_note((0.43, y - 0.065), s.get("full_rigid", True), short=True)
        y -= 0.065
        self._stepper("ボクセルサイズ（新規設置）", y - 0.065, [0.25, 0.5, 1.0, 2.0], s.get("voxel_size", 0.5),
                      lambda v: f"{v} m", lambda v: s.__setitem__("voxel_size", v))
        self._stepper("重力倍率", y - 0.13, [0.0, 0.25, 0.5, 1.0, 1.5, 2.0], s.get("gravity_scale", 1.0),
                      lambda v: f"×{v}", lambda v: s.__setitem__("gravity_scale", v))
        self._toggle("構築物の自動削除", y - 0.195, cl.get("enabled", True), lambda v: cl.__setitem__("enabled", v))
        self._stepper("　削除する距離", y - 0.26, [200.0, 500.0, 1000.0, 1500.0, 3000.0, 5000.0, 10000.0],
                      float(cl.get("distance", 1500)), lambda v: f"{v:.0f} m", lambda v: cl.__setitem__("distance", v))
        self._stepper("　離れている時間", y - 0.325, [10.0, 30.0, 60.0, 120.0, 300.0, 600.0],
                      float(cl.get("seconds", 60)), lambda v: f"{v:.0f} 秒", lambda v: cl.__setitem__("seconds", v))
        self._stepper("　対象の最大頂点数", y - 0.39, [80, 200, 400, 800, 1600, 4000],
                      int(cl.get("max_vertices", 400)), lambda v: f"{v}", lambda v: cl.__setitem__("max_vertices", v))
        def open_mods():
            self._ws = (name, s, sel)
            self.show_mods(sel, lambda: self.show_world_settings(name, keep=True), f"ワールド「{name}」の MOD")
        self._mod_summary(y - 0.465, sel, open_mods)
        planets = ", ".join(p.get("def", "?") for p in s.get("planets", []))
        self._text(f"惑星: {planets}（world.json で編集）", (0, y - 0.6), 0.7, col=U["color"].light_gray)
        self._text("MOD を入れる・外す前に、ワールド（saves/ のフォルダ）をコピーしておくことをすすめます。"
                   "使っている MOD を外すと、そのワールドは始められなくなります。", (0, y - 0.66), 0.58,
                   col=U["color"].rgb(1, 0.8, 0.45))

        def save():
            try:
                from .. import modmeta
                write_world_settings(name, s, modmeta.resolve(sorted(sel))[0])
                self._ws = None
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
        self._toggle("仮想ジョイスティック（タッチ操作）", y - 0.63, c.get("touch", False), lambda v: c.__setitem__("touch", v))

        def keys():
            from .keys_panel import KeysPanel, ACTIONS

            def changed():
                for aid, _ in ACTIONS:
                    c[aid] = self.cfg["controls"].get(aid, c.get(aid))
            KeysPanel(self.U, self.cfg, self.root, None, on_change=changed)
        self._button("操作とキー設定", (0.62, -0.09), keys, size=(0.2, 0.055), col=U["color"].rgb(0.25, 0.35, 0.45))
        self._text("フォントは config/settings.json で変更できます", (0, y - 0.7), 0.6, col=U["color"].light_gray)

        def save():
            prev = dict(self.cfg.get("window", {}))
            self.cfg.clear()
            self.cfg.update(cfg)
            try:
                save_config(self.cfg)
                apply_window_settings(self.cfg, prev)
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
        self._close_browser()
        destroy(self.root)


def _default_name():
    import getpass
    try:
        return getpass.getuser()[:24] or "player"
    except Exception:  # noqa: BLE001
        return "player"


def patch_mouse_properties():
    """ursina の mouse.locked / mouse.visible は、ウィンドウの設定をまるごと（作ったときの大きさ・全画面のまま）送り直す。
    そのため、ウィンドウの大きさを変えたり最大化したりしたあとにメニュー・画面を開くと、元の大きさに戻ってしまう。
    カーソルの表示とマウスのモードだけを送るように差し替える（大きさ・全画面はそのまま）。"""
    from panda3d.core import WindowProperties
    from ursina import application
    from ursina.mouse import Mouse

    def send(hidden=None, mode=None):
        base = application.base
        if base is None or getattr(base, "win", None) is None:
            return
        wp = WindowProperties()
        if hidden is not None:
            wp.setCursorHidden(bool(hidden))
        if mode is not None:
            wp.setMouseMode(mode)
        base.win.requestProperties(wp)

    def set_locked(self, value):
        self._locked = value
        send(hidden=value, mode=WindowProperties.M_confined if value else WindowProperties.M_absolute)
        self._locked_mouse_last_frame = True

    def set_visible(self, value):
        self._visible = value
        send(hidden=not value)

    try:
        Mouse.locked = property(Mouse.locked.fget, set_locked)
        Mouse.visible = property(Mouse.visible.fget, set_visible)
    except Exception as e:  # noqa: BLE001
        L.warning("マウスの設定の差し替えに失敗: %s", e)


def apply_window_settings(cfg, prev=None):
    """設定のウィンドウ（解像度・全画面）を反映する。prev（前の設定）と同じなら何もしない
    （手で大きさを変えた・最大化したウィンドウを、設定の保存で元に戻さないため）。"""
    from ursina import window, Vec2
    w = cfg["window"]
    if prev is not None and all(prev.get(k) == w.get(k) for k in ("width", "height", "fullscreen")):
        return
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
    patch_mouse_properties()
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
        state["menu"] = MainMenu(cfg, play, quit_app, join)
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

    def leave(action, status=""):
        def later():
            g = state["game"]
            if g is not None:
                g.destroy()
                state["game"] = None
            if action == "quit":
                quit_app()
            elif action.startswith("play:"):         # 読み込み直す（シナリオの章のオートセーブに戻すなど）
                open_menu(status)
                play(action[5:])
            else:
                open_menu(status)
        invoke(later, delay=0.01)

    def join(host, port, name):
        """マルチプレイに参加：MOD とワールドを受け取ってから（別スレッド）、そのワールドで遊ぶ。"""
        from ..netclient import Joiner
        from ursina import Entity
        menu = state["menu"]
        j = Joiner(host, port, name)

        def cancel():
            j.cancel()
            menu.show_multiplayer()
        menu.show_joining(j, cancel)

        class Wait(Entity):
            def update(self_):
                if j.error:
                    from .ui_destroy import destroy
                    destroy(self_)
                    if state["menu"] is not None and not j.cancelled:
                        state["menu"].status = f"参加できません：{j.error}"
                        state["menu"].show_multiplayer()
                elif j.done:
                    from .ui_destroy import destroy
                    destroy(self_)
                    start_joined(j.result)
        Wait()

    def start_joined(info):
        from ..netclient import Client, mods_cache
        from ..world import World
        from ..modcode import ModLoadError
        orig = paths.MODS
        paths.MODS = mods_cache()           # ホストから受け取った MOD で読み込む
        try:
            try:
                world = World(info["world"], cfg, directory=info["dir"]).load()
            except ModLoadError as e:
                info["conn"].close()
                load_failed(info["world"], e.errors)
                return
            except Exception as e:  # noqa: BLE001
                L.exception("受け取ったワールドを読み込めません")
                info["conn"].close()
                state["menu"].error(f"受け取ったワールドを読み込めません: {e}")
                return
            Client(info, world, cfg)
            if state["menu"] is not None:
                state["menu"].destroy()
                state["menu"] = None
            game = Game(world, cfg, on_exit=leave)
            state["game"] = game
            try:
                game.start()
            except ModLoadError as e:
                try:
                    game.destroy()
                except Exception:  # noqa: BLE001
                    L.exception("画面の後片付けに失敗")
                state["game"] = None
                load_failed(info["world"], e.errors)
        finally:
            paths.MODS = orig

    if world_name:
        play(world_name)
    else:
        open_menu()
    if on_ready:  # テスト用
        on_ready(state, play, leave)
    app.run()
