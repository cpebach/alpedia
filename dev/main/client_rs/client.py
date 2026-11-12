"""Rust の画面のクライアント全体の流れ：タイトルのメニュー（今までの menu.MainMenu を ursina のまねで）→ ワールド → メニュー。

  python run.py --view rust            メニューから
  python run.py --view rust --world 名前   メニューを飛ばして
"""
import time

from .. import log
from ..client import gfx

L = log.get("client_rs")


class Client:
    def __init__(self, cfg):
        from . import ursina_shim
        self.U = ursina_shim.install()
        self.cfg = cfg
        win = cfg.get("window", {})
        self.view = gfx.start(win.get("width", 1280), win.get("height", 720), "Alpedia", win.get("vsync", True))
        gfx.font(cfg.get("graphics", {}).get("font"))
        from .. import paths
        self.view.set_icon(str(paths.SAUCE / "textures" / "alpedia.ico"))
        from ..client import sound
        sound.get(cfg)
        if win.get("fullscreen", False):
            self.view.set_fullscreen(True)
        self.menu = None
        self.viewer = None
        self.joining = None
        self.running = True

    # ---- メニュー ----
    def open_menu(self, status=""):
        from ..client.menu import MainMenu, purge_trash
        purge_trash()
        self.view.grab(False)
        self.menu = MainMenu(self.cfg, self.play, self.quit, self.join)
        if status:
            self.menu.error(status)

    def quit(self):
        self.running = False

    def play(self, name):
        from ..app import prepare_world
        from ..modcode import ModLoadError
        from .viewer import Viewer
        try:
            World = prepare_world(name)
            world = World(name, self.cfg).load()
        except ModLoadError as e:
            L.error("ワールド「%s」を始められません（MOD のエラー）:\n%s", name, "\n".join(e.errors))
            if self.menu is None:
                self.open_menu()
            self.menu.show_load_errors(name, e.errors)
            return
        except Exception as e:  # noqa: BLE001
            L.exception("ワールドの読み込みに失敗: %s", name)
            if self.menu is None:
                self.open_menu()
            self.menu.error(f"「{name}」を読み込めません: {e}")
            return
        if self.menu is not None:
            self.menu.destroy()
            self.menu = None
        self.viewer = Viewer(world, self.cfg)

    # ---- マルチプレイに参加 ----
    def join(self, host, port, name):
        """MOD とワールドを受け取ってから（別スレッド）、そのワールドで遊ぶ（今までの画面の menu.run_menu と同じ流れ）。"""
        from ..netclient import Joiner
        menu = self.menu
        j = Joiner(host, port, name)

        def cancel():
            j.cancel()
            menu.show_multiplayer()
        menu.show_joining(j, cancel)
        self.joining = j

    def _check_join(self):
        j = self.joining
        if j is None:
            return
        if j.error:
            self.joining = None
            if self.menu is not None and not j.cancelled:
                self.menu.status = f"参加できません：{j.error}"
                self.menu.show_multiplayer()
        elif j.done:
            self.joining = None
            self.start_joined(j.result)

    def start_joined(self, info):
        from .. import paths
        from ..modcode import ModLoadError
        from ..netclient import Client as NetClient, mods_cache
        from ..world import World
        from .viewer import Viewer
        orig = paths.MODS
        paths.MODS = mods_cache()           # ホストから受け取った MOD で読み込む
        try:
            try:
                world = World(info["world"], self.cfg, directory=info["dir"]).load()
            except ModLoadError as e:
                info["conn"].close()
                self.menu.show_load_errors(info["world"], e.errors)
                return
            except Exception as e:  # noqa: BLE001
                L.exception("受け取ったワールドを読み込めません")
                info["conn"].close()
                self.menu.error(f"受け取ったワールドを読み込めません: {e}")
                return
            NetClient(info, world, self.cfg)
            if self.menu is not None:
                self.menu.destroy()
                self.menu = None
            self.viewer = Viewer(world, self.cfg)
        finally:
            paths.MODS = orig

    def leave(self):
        """ワールドを出る。MOD が決めた行き先（leave_action）：("menu"・"play:名前", 保存するか, 知らせ)。"""
        v = self.viewer
        self.viewer = None
        action, save, status = getattr(v, "leave_action", None) or ("menu", True, "")
        net = getattr(v.w, "net", None)
        if save and not (net is not None and net.role == "client"):
            try:
                v.w.save()
            except Exception:  # noqa: BLE001
                L.exception("保存に失敗しました")
        if net is not None:
            try:
                net.close()
            except Exception:  # noqa: BLE001
                L.exception("マルチプレイの終了でエラー")
        v.destroy()
        if getattr(v, "close_app", False) or action == "quit":
            self.running = False
        elif action.startswith("play:"):
            self.play(action[5:])
            if self.viewer is None and self.menu is None:
                self.open_menu(status)
        else:
            self.open_menu(status)

    # ---- まわす ----
    def frame(self, dt):
        if self.viewer is not None:
            self.viewer.frame(dt)
            if self.viewer.quit:
                self.leave()
            return
        for kind, a, b, n in self.view.pump():
            if kind == "close":
                self.running = False
            self.U.event(self.view, kind, a, b, n)
        self.U.frame(self.view, dt)
        self._check_join()
        from ..client import sound
        sound.get().update(dt)
        c = self.U.window.color
        self.view.set_clear(c[0], c[1], c[2])
        self.view.render()

    def run(self, world_name=None):
        if world_name:
            self.play(world_name)
        if self.viewer is None:
            self.open_menu()
        last = time.perf_counter()
        while self.running:
            now = time.perf_counter()
            dt = min(now - last, 0.1)
            last = now
            self.frame(dt)


def run_client(cfg, world_name=None):
    Client(cfg).run(world_name)
