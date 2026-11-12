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
        self.view = gfx.start(win.get("width", 1280), win.get("height", 720), "Alpedia（Rust の画面）", win.get("vsync", True))
        gfx.font(cfg.get("graphics", {}).get("font"))
        self.menu = None
        self.viewer = None
        self.running = True

    # ---- メニュー ----
    def open_menu(self, status=""):
        from ..client.menu import MainMenu, purge_trash
        purge_trash()
        self.view.grab(False)
        self.menu = MainMenu(self.cfg, self.play, self.quit, None)
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

    def leave(self):
        v = self.viewer
        self.viewer = None
        try:
            v.w.save()
        except Exception:  # noqa: BLE001
            L.exception("保存に失敗しました")
        v.destroy()
        if getattr(v, "close_app", False):
            self.running = False
        else:
            self.open_menu()

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
