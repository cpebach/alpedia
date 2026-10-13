"""Ursina によるゲーム画面・操作。"""
import time as _time
from pathlib import Path

import numpy as np

from .. import log
from ..construct import DIR_NAMES
from . import render

L = log.get("client")

FONT_CANDIDATES = [
    "C:/Windows/Fonts/meiryo.ttc", "C:/Windows/Fonts/YuGothM.ttc", "C:/Windows/Fonts/BIZ-UDGothicR.ttc",
    "C:/Windows/Fonts/msgothic.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
]

HELP = """[操作]  （F1 で表示切替）
WASD 移動 / Space ジャンプ・上昇 / Ctrl 下降 / F ジェットパック
マウス 視点 / 左クリック 削除・掘る / 右クリック 設置
I インベントリ / ホイール・数字キー アイテムバーの選択 / R 向き変更 / N 空中に新しい構築物
E 機器の操作・操縦席に座る/降りる・街に固定・NPC と話す/取引 / V 三人称 / M 目印（街・自宅） / K マップ
P 機器の設定（名前）／プログラム編集／アクチュエーターの動作設定
F5 保存 / F9 見ている構築物を OBJ 出力 / F8 読み込みメニュー
Esc 一時停止メニュー（保存・メニューへ戻る・終了）
[操縦席] Space などの割り当てキーで推進機、X でアクチュエータ
"""

SEAT_KEYS = ["w", "a", "s", "d", "q", "e", "space", "left control", "left shift", "x", "z", "c",
             "up arrow", "down arrow", "left arrow", "right arrow", "1", "2", "3", "4"]


def _find_font(cfg):
    custom = cfg.get("graphics", {}).get("font")
    for f in ([custom] if custom else []) + FONT_CANDIDATES:
        if f and Path(f).exists():
            return Path(f)
    return None


class Game:
    def __init__(self, world, cfg, on_exit=None):
        self.world = world
        self.cfg = cfg
        self.on_exit = on_exit or (lambda action: None)

    def start(self):
        from ursina import Entity, Text, Button, camera, window, color, application, held_keys, mouse, load_model
        from ursina import time as utime
        self.U = dict(Entity=Entity, Text=Text, camera=camera, window=window, color=color, held_keys=held_keys,
                      mouse=mouse, utime=utime, application=application, load_model=load_model)
        window.color = color.rgb(0.45, 0.65, 0.9)
        camera.fov = self.cfg["graphics"].get("fov", 80)
        camera.clip_plane_near = 0.05
        camera.clip_plane_far = float(self.cfg["graphics"].get("view_distance", 3000)) * 4
        camera.position = (0, 0, 0)
        self.scene_root = Entity()  # 世界の描画ルート
        self.ui_root = Entity(parent=camera.ui)
        loader = application.base.loader
        self.loader = loader
        self.views = {}
        self.displays = {}
        self.planet_views = [render.PlanetView(self.scene_root, loader, p) for p in self.world.planets]
        g = self.cfg["graphics"]
        self.patch = render.TerrainPatch(self.scene_root, loader, int(g.get("terrain_patch_cells", 48)),
                                         float(g.get("terrain_patch_step", 1.0)))
        self.carve = render.CarveView(self.scene_root, loader)
        # 水中にいる時の青いもや
        self.underwater = Entity(parent=camera.ui, model="quad", scale=(3, 2), color=color.rgba(0.1, 0.3, 0.55, 0.45),
                                 z=1, enabled=False)
        self.highlight = Entity(model="wireframe_cube", color=color.yellow, parent=self.scene_root)
        self.ghost = Entity(model="wireframe_cube", color=color.lime, parent=self.scene_root)
        self.body = self._make_body(Entity, load_model, color)
        # UI
        ui = self.ui_root
        Entity(parent=ui, model="quad", scale=0.006, color=color.white)
        self.hud = Text(parent=ui, position=window.top_left + (0.01, -0.01), origin=(-0.5, 0.5),
                        scale=0.75, text="", background=False)
        self.sel_text = Text(parent=ui, position=(0, -0.385), origin=(0, 0), scale=0.85, text="")
        self.msg_text = Text(parent=ui, position=window.bottom_left + (0.01, 0.16), origin=(-0.5, -0.5),
                             scale=0.7, text="")
        self.aim_text = Text(parent=ui, position=(0.02, -0.03), origin=(-0.5, 0.5), scale=0.7, text="")
        self.help_text = Text(parent=ui, position=window.top_right + (-0.01, -0.01), origin=(0.5, 0.5),
                              scale=0.65, text=HELP)
        # 最寄りの街の目印（画面の外なら端に矢印）
        self.city_views = {}
        self.marker_on = True
        self.marker_mode = "city"
        self.npc_views = {}
        self.marker = Entity(parent=ui, enabled=False)
        Entity(parent=self.marker, model="quad", scale=0.022, rotation_z=45, color=color.rgb(1.0, 0.25, 0.85))
        Entity(parent=self.marker, model="quad", scale=0.012, rotation_z=45, color=color.rgb(0.1, 0.95, 1.0), z=-0.01)
        self.marker_text = Text(parent=self.marker, position=(0, -0.02), origin=(0, 0.5), scale=0.7, text="",
                                color=color.rgb(0.6, 1.0, 1.0))
        self._marker_t = 0.0
        self._marker_city = None
        # 一時停止メニュー
        self.pause_root = Entity(parent=ui, enabled=False)
        Entity(parent=self.pause_root, model="quad", scale=(3, 2), color=color.rgba(0, 0, 0, 0.55), z=0.1)
        Text(parent=self.pause_root, text="一時停止中", position=(0, 0.25), origin=(0, 0), scale=2)

        def pbtn(text, y, fn, col=None):
            b = Button(parent=self.pause_root, text=text, position=(0, y), scale=(0.36, 0.06),
                       color=col or color.rgb(0.22, 0.32, 0.45), text_size=0.8)
            b.on_click = fn
            return b
        pbtn("再開", 0.12, self.toggle_pause, color.rgb(0.2, 0.5, 0.3))
        pbtn("保存", 0.04, self.world.save)
        pbtn("保存してメニューへ", -0.04, lambda: self._leave("menu"))
        pbtn("保存せずにメニューへ", -0.12, lambda: self._leave("menu", save=False), color.rgb(0.4, 0.3, 0.2))
        pbtn("保存して終了", -0.20, lambda: self._leave("quit"), color.rgb(0.5, 0.2, 0.2))
        # 状態
        from . import inventory
        inventory.ensure_hotbar(self.world.player, self.world.registry)
        self.sel = 0
        self.hotbar_view = inventory.HotbarView(ui, self)
        self.dir_index = 2
        self.yaw_fwd = None
        self.pitch = 0.0
        self.third_person = False
        self.paused = False
        self.leaving = False
        self.editor = None
        self._esc_block = 0.0
        self.acc = 0.0
        self.import_idx = 0
        self.hit = None
        self.origin = self.world.player.eye()
        self._captured = None
        self._skip_look = 2
        self.set_mouse_capture(True)
        game = self

        class Driver(Entity):
            def update(self_):
                if not game.leaving:
                    game.update()

            def input(self_, key):
                if not game.leaving:
                    try:
                        game.input(key)
                    except Exception as e:  # noqa: BLE001
                        # キー操作のエラーはログに残してゲームは続ける
                        L.exception("キー入力 %r の処理でエラー", key)
                        game.world.message(f"⚠ エラー（{key}）: {e}（logs/latest.log を見てください）")
        self.driver = Driver()
        L.info("ゲーム開始: %s", self.world.name)

    # --- スクリプトエディタ ---------------------------------------------
    def _computers(self, c):
        out = []
        for i, d in enumerate(c.devs):
            if d:
                dd = c.reg.device(d["def"])
                if dd and dd["type"] == "computer":
                    out.append(i)
        return out

    def open_editor(self):
        w = self.world
        target = None
        if w.player.seated:
            c = w.constructs.get(w.player.seated[0])
            if c and self._computers(c):
                target = (c, self._computers(c)[0])
        elif self.hit and self.hit["kind"] == "voxel":
            c, i = self.hit["construct"], self.hit["index"]
            from .. import actuator as act
            if act.is_actuator(c, i):
                from .actuator_ui import ActuatorPanel
                self.set_mouse_capture(False)
                self.ui_root.enabled = False
                self.editor = ActuatorPanel(self, c, i)
                return
            comps = self._computers(c)
            if i in comps:
                target = (c, i)
            elif c.devs[i]:
                # コンピューター・アクチュエーター以外の機器：名前などの設定メニュー
                from .device_ui import DevicePanel
                self.set_mouse_capture(False)
                self.ui_root.enabled = False
                self.editor = DevicePanel(self, c, i)
                return
            elif comps:
                target = (c, comps[0])
        if target is None:
            w.message("コンピューターを見ながら（または操縦席で）P を押すとプログラムを編集できます")
            return
        self.set_mouse_capture(False)
        self.ui_root.enabled = False
        c, i = target
        if c.devs[i]["state"].get("blocks"):
            from .block_editor import BlockEditor
            self.editor = BlockEditor(self, c, i)
        else:
            from .editor import ScriptEditor
            self.editor = ScriptEditor(self, c, i)

    def current_item(self):
        it = self.world.player.hotbar[self.sel]
        if it is None:
            self.world.message("アイテムバーのこの枠は空です（I でインベントリを開いて入れます）")
        return it

    def open_inventory(self):
        from .inventory import InventoryPanel
        L.info("インベントリを開きます")
        self.set_mouse_capture(False)
        self.ui_root.enabled = False
        try:
            self.editor = InventoryPanel(self)
        except Exception as e:  # noqa: BLE001
            L.exception("インベントリを開けませんでした")
            root = getattr(getattr(self, "editor", None), "root", None)
            self.editor = None
            if root is not None:
                from .ui_destroy import destroy
                destroy(root)
            self.editor_closed()
            self.world.message(f"⚠ インベントリを開けませんでした: {e}（logs/latest.log を見てください）")

    def editor_closed(self):
        self.editor = None
        self.ui_root.enabled = True
        self.pause_root.enabled = self.paused
        self._esc_block = _time.time() + 0.2
        if not self.paused:
            self.set_mouse_capture(True)

    def set_mouse_capture(self, capture):
        """視点操作のためにマウスカーソルを隠して捕まえる / 離して普通に使えるようにする。

        ursina の mouse.locked はウィンドウ設定をまるごと送り直すため、環境によっては切り替えが効かず
        カーソルが捕まったままになる。ここでは必要な項目（カーソル表示・マウスモード）だけを送り、
        視点の回転はカーソルを画面中央に戻しながら自前で計算する。"""
        from panda3d.core import WindowProperties
        capture = bool(capture)
        try:
            self.U["mouse"]._locked = False     # ursina 側の中央固定は使わない
        except Exception:  # noqa: BLE001
            pass
        base = self.U["application"].base
        if base is None or base.win is None:
            return
        wp = WindowProperties()
        wp.setCursorHidden(capture)
        wp.setMouseMode(WindowProperties.M_confined if capture else WindowProperties.M_absolute)
        base.win.requestProperties(wp)
        if capture and self._captured is not True:
            self._skip_look = 2     # 捕まえた直後の移動量は捨てる（視点が飛ばないように）
            self._center_pointer()
        self._captured = capture

    def _center_pointer(self):
        win = self.U["application"].base.win
        try:
            win.movePointer(0, win.getXSize() // 2, win.getYSize() // 2)
        except Exception:  # noqa: BLE001
            pass

    def _look_delta(self):
        """前のフレームからのマウスの移動量（画面の高さ = 1 の単位）。カーソルは中央に戻す。"""
        if not self._captured:
            return 0.0, 0.0
        win = self.U["application"].base.win
        try:
            if not win.getProperties().getForeground():
                self._skip_look = 2
                return 0.0, 0.0
            md = win.getPointer(0)
            w, h = win.getXSize(), win.getYSize()
            if not md.getInWindow() or h <= 0:
                self._center_pointer()
                return 0.0, 0.0
            dx = (md.getX() - w // 2) / h
            dy = -(md.getY() - h // 2) / h
        except Exception:  # noqa: BLE001
            return 0.0, 0.0
        self._center_pointer()
        if self._skip_look > 0:
            self._skip_look -= 1
            return 0.0, 0.0
        return dx, dy

    def toggle_pause(self):
        self.paused = not self.paused
        self.set_mouse_capture(not self.paused)
        self.pause_root.enabled = self.paused

    def _leave(self, action, save=True):
        if self.leaving:
            return
        if save:
            try:
                self.world.save()
            except Exception:  # noqa: BLE001
                L.exception("保存に失敗")
        self.leaving = True
        self.on_exit(action)

    def destroy(self):
        from .ui_destroy import destroy
        if self.editor is not None:
            try:
                destroy(self.editor.root)
                destroy(self.editor.field)
            except Exception:  # noqa: BLE001
                pass
            self.editor = None
        for v in self.views.values():
            v.remove()
        self.views = {}
        for v in getattr(self, "city_views", {}).values():
            v.remove()
        self.city_views = {}
        for v in getattr(self, "npc_views", {}).values():
            v.remove()
        self.npc_views = {}
        for e in (self.driver, self.scene_root, self.ui_root, self.underwater):
            destroy(e)
        self.set_mouse_capture(False)
        L.info("ゲーム終了: %s", self.world.name)

    # ------------------------------------------------------------------
    def _make_body(self, Entity, load_model, color):
        d = self.world.player.defn
        col = d.get("color", [1, 1, 1])
        model = None
        if d.get("model"):
            try:
                model = load_model(str(d["model"]))
            except Exception:  # noqa: BLE001
                model = None
        e = Entity(model=model or "cube", color=color.rgb(*col) if max(col) <= 1 else color.rgb32(*col),
                   parent=self.scene_root)
        e.hide()
        return e

    def look_vectors(self):
        p = self.world.player
        up = p.up
        if self.yaw_fwd is None:
            ref = np.array([0, 0, 1.0]) if abs(up[2]) < 0.9 else np.array([1.0, 0, 0])
            self.yaw_fwd = ref - up * np.dot(ref, up)
        f = self.yaw_fwd - up * np.dot(self.yaw_fwd, up)
        if np.linalg.norm(f) < 1e-3:
            # 上向きが急に変わって（別の天体に近づいたなど）前方向が上と重なったときの保険
            ref = np.array([0, 0, 1.0]) if abs(up[2]) < 0.9 else np.array([1.0, 0, 0])
            f = ref - up * np.dot(ref, up)
        f /= max(np.linalg.norm(f), 1e-9)
        self.yaw_fwd = f
        look = f * np.cos(self.pitch) + up * np.sin(self.pitch)
        right = np.cross(up, f)
        return f, right, up, look

    def _rotate_yaw(self, ang):
        self.look_vectors()
        up = self.world.player.up
        f = self.yaw_fwd
        r = np.cross(up, f)
        self.yaw_fwd = f * np.cos(ang) + r * np.sin(ang)

    # ------------------------------------------------------------------
    def input(self, key):
        w = self.world
        c = self.cfg["controls"]
        if self.editor is not None or _time.time() < self._esc_block:
            return  # エディタ表示中のキーはエディタが処理する
        if key == "escape":
            self.toggle_pause()
            return
        if self.paused:
            return
        if key == c.get("edit_script", "p"):
            self.open_editor()
            return
        if w.player.seated:
            if key in SEAT_KEYS or key in ("x",):
                w.key_pressed(key)
            if key == c["interact"]:
                w.interact(None)
            elif key == c["save"]:
                w.save()
            elif key == c["export"]:
                self._export()
            return
        if key == "left mouse down":
            if self.hit and self.hit["kind"] == "voxel":
                w.remove(self.hit)
            elif self.hit and self.hit["kind"] in ("terrain", "water"):
                w.dig(self.hit)
        elif key == "right mouse down":
            item = self.current_item()
            if item:
                _, _, _, look = self.look_vectors()
                w.place(self.hit, item, self.dir_index, look)
        elif key == c["new_construct"]:
            item = self.current_item()
            if item:
                _, _, _, look = self.look_vectors()
                w.place(None, item, self.dir_index, look)
        elif key == "scroll up":
            self.sel = (self.sel - 1) % 9
        elif key == "scroll down":
            self.sel = (self.sel + 1) % 9
        elif key in "123456789" and len(key) == 1:
            self.sel = int(key) - 1
        elif key == c.get("inventory", "i"):
            self.open_inventory()
        elif key == c["rotate"]:
            self.dir_index = (self.dir_index + 1) % 6
        elif key == c["interact"]:
            if not self._city_interact(self.hit):
                w.interact(self.hit)
        elif key == c["jetpack"]:
            w.player.jetpack = not w.player.jetpack
            w.message("ジェットパック " + ("ON" if w.player.jetpack else "OFF"))
        elif key == c["save"]:
            w.save()
        elif key == c["export"]:
            self._export()
        elif key == c["import"]:
            self._import()
        elif key == c["toggle_help"]:
            self.help_text.enabled = not self.help_text.enabled
        elif key == "v":
            self.third_person = not self.third_person
        elif key == c.get("city_map", "k"):
            if not w.cities.enabled:
                w.message("このワールドには街がありません（ワールド設定で ON にできます）")
            else:
                from . import city_ui
                self._open_panel(lambda: city_ui.MapPanel(self))
        elif key == c.get("city_marker", "m"):
            # 目印：最寄りの街 → 自宅（あれば）→ なし
            modes = ["city"] + (["home"] if w.economy.home_id() else []) + ["off"]
            cur = self.marker_mode if self.marker_mode in modes else "off"
            self.marker_mode = modes[(modes.index(cur) + 1) % len(modes)]
            self.marker_on = self.marker_mode != "off"
            self._marker_city = None
            w.message("目印: " + {"city": "最寄りの街", "home": "自宅", "off": "なし"}[self.marker_mode])

    def _export(self):
        w = self.world
        target = None
        if w.player.seated:
            target = w.constructs.get(w.player.seated[0])
        elif self.hit and self.hit["kind"] == "voxel":
            target = self.hit["construct"]
        if target is None:
            w.message("出力する構築物を見てください")
            return
        try:
            w.export_obj(target)
        except Exception as e:  # noqa: BLE001
            L.exception("OBJ 出力失敗")
            w.message(f"OBJ 出力失敗: {e}")

    def _import(self):
        """F8：読み込みメニューを開く（置くもの・大きさ・必要な資源を確かめて OK で置く）。"""
        from .import_ui import ImportPanel
        L.info("読み込みメニューを開きます")
        self.set_mouse_capture(False)
        self.ui_root.enabled = False
        try:
            self.editor = ImportPanel(self)
        except Exception as e:  # noqa: BLE001
            L.exception("読み込みメニューを開けませんでした")
            root = getattr(getattr(self, "editor", None), "root", None)
            self.editor = None
            if root is not None:
                from .ui_destroy import destroy
                destroy(root)
            self.editor_closed()
            self.world.message(f"⚠ 読み込みメニューを開けませんでした: {e}")

    # ------------------------------------------------------------------
    def _test_hook(self):
        """テスト用：環境変数 ALPEDIA_SHOT=path:frames で画面を保存して終了。"""
        import os
        spec = os.environ.get("ALPEDIA_SHOT")
        if not spec:
            return
        path, frames = spec.rsplit(":", 1)
        self._frames = getattr(self, "_frames", 0) + 1
        if self._frames == 1 and os.environ.get("ALPEDIA_PITCH"):
            self.pitch = float(os.environ["ALPEDIA_PITCH"])
        if self._frames == 1 and os.environ.get("ALPEDIA_TP"):
            self.third_person = True
        if self._frames == int(frames):
            from panda3d.core import Filename
            self.U["application"].base.win.saveScreenshot(Filename.fromOsSpecific(path))
            L.info("screenshot %s player %s look %s", path, self.world.player.pos.round(1), self.look_vectors()[3].round(2))
            for c in self.world.constructs.values():
                L.info("  %s com %s", c.name, c.center_of_mass().round(1))
            self.U["application"].quit()

    def update(self):
        U = self.U
        w = self.world
        self._test_hook()
        dt = min(U["utime"].dt, 0.1)
        held = U["held_keys"]
        c = self.cfg["controls"]
        if self.paused:
            return
        editing = self.editor is not None
        # 視点
        sens = float(c.get("mouse_sensitivity", 60.0))
        if self._captured and not editing:
            dx, dy = self._look_delta()
            self._rotate_yaw(np.radians(dx * sens))
            self.pitch = float(np.clip(self.pitch + np.radians(dy * sens), -1.55, 1.55))
        fwd, right, up, look = self.look_vectors()
        # 入力
        move = np.zeros(3)
        if not w.player.seated and not editing:
            move += fwd * (held[c["forward"]] - held[c["back"]])
            move += right * (held[c["right"]] - held[c["left"]])
            nm = np.linalg.norm(move)
            if nm > 0:
                move /= nm
        jump = bool(held[c["jump"]]) and not w.player.seated and not editing
        thrust = np.zeros(3)
        if not w.player.seated and not editing:
            # ジェットパックの噴射方向（水中では泳ぐ方向にも使う）
            thrust = move * 0.6 + up * (held[c["jump"]] - held[c["down"]])
        w.input_keys = {k for k in SEAT_KEYS if held[k]} if (w.player.seated and not editing) else set()
        # 固定ステップでシミュレーション
        tick = 1.0 / float(self.cfg["physics"].get("tick_rate", 60))
        self.acc += dt
        n = 0
        while self.acc >= tick and n < 3:
            try:
                w.step(tick, move, jump and n == 0, thrust)
            except Exception:  # noqa: BLE001
                L.exception("シミュレーションエラー")
                raise
            self.acc -= tick
            n += 1
        if n == 3:
            self.acc = 0.0
        # カメラ
        fwd, right, up, look = self.look_vectors()
        origin = w.player.eye()
        cam = U["camera"]
        if self.third_person:
            origin = origin - look * 5.0 + up * 0.8
        cam.position = (0, 0, 0)
        from panda3d.core import LPoint3f, LVector3f
        cam.lookAt(LPoint3f(*look.astype(np.float32)), LVector3f(*up.astype(np.float32)))
        # 描画
        self._render(origin)
        if not editing:
            self._update_aim(look)
        self._update_hud(dt)

    def _render(self, origin):
        w = self.world
        for cid in list(self.views):
            if cid not in w.constructs:
                self.views.pop(cid).remove()
        far = float(self.cfg["graphics"].get("view_distance", 3000))
        decal_dist = float(self.cfg["graphics"].get("decal_distance", 30.0))
        for cid, c in w.constructs.items():
            v = self.views.get(cid)
            if v is None:
                v = self.views[cid] = render.ConstructView(self.scene_root, self.loader, c)
            if np.linalg.norm(c.pos[0] - origin) > far:
                for g in v.groups:
                    g["mesh"].np.hide()
                v.hide_decals()
                continue
            for g in v.groups:
                g["mesh"].np.show()
            v.update(origin)
            v.update_decals(origin, origin, decal_dist)
        self._render_displays(origin, far)
        self._render_cities(origin, far)
        for pv in self.planet_views:
            pv.update(origin, w.player.pos)
        npl = w.nearest_planet(w.player.pos)
        self.carve.update(npl, w.player.pos, origin)
        self.patch.update(npl, w.player.pos, origin, self.carve)
        self.underwater.enabled = bool(npl is not None and npl.has_water
                                       and npl.water_depth(origin[None])[0] > 0.02)
        # プレイヤーの体（三人称）
        if self.third_person:
            p = w.player
            fwd, right, up, _ = self.look_vectors()
            center = p.pos + up * (p.height / 2 - p.radius) - origin
            self._set_box(self.body, center, np.stack([right, up, fwd]),
                          np.array([p.radius * 2, p.height, p.radius * 1.4]))
            self.body.show()
        else:
            self.body.hide()
        self.origin = origin

    def _city_interact(self, h):
        """街の NPC・家具（カウンター・ベッド・収納箱）を E で使う。使ったら True。"""
        w = self.world
        if not h or h["kind"] not in ("npc", "city"):
            return False
        from .. import citylife
        from . import city_ui
        city = h["city"]
        survival = not w.settings.get("creative", True)
        npc = h.get("npc")
        if h["kind"] == "city":
            tag = city.parts[h["part"]].get("tag")
            if not tag:
                return False
            if tag[0] == "counter":
                npc = next((n for n in city.npcs if n["id"] == tag[1]), None)
                if npc is None or not citylife.npc_alive(city, npc):
                    return True
            else:
                home = next((x for x in city.homes if x["id"] == tag[1]), None)
                if home is None:
                    return True
                if city.state[home["bld"]] >= 2:
                    w.message("建物が壊れていて使えません")
                    return True
                if not w.economy.can_use(home["id"]):
                    w.message(f"{home['name']} は使えません（不動産屋で買うか借りると使えます）")
                    return True
                if tag[0] == "bed":
                    w.economy.set_home(home["id"])
                    w.save()
                    self.marker_mode = "home" if self.marker_mode == "home" else self.marker_mode
                    w.message(f"{home['name']} で休みました（保存しました）。ここが自宅です（M で自宅の目印）")
                    return True
                self._open_panel(lambda: city_ui.StoragePanel(self, city, home))
                return True
        if npc is None:
            return False
        if getattr(city, "fallen", False):
            w.message(f"{city.name} は滅びてしまいました")
            return True
        if npc["role"] == "trader" and survival:
            self._open_panel(lambda: city_ui.TradePanel(self, city, npc))
        elif npc["role"] == "estate" and survival:
            self._open_panel(lambda: city_ui.EstatePanel(self, city, npc))
        else:
            w.message(f"{npc['name']}「{citylife.talk_line(city, npc)}」")
        return True

    def _open_panel(self, make):
        self.set_mouse_capture(False)
        self.ui_root.enabled = False
        try:
            self.editor = make()
        except Exception as e:  # noqa: BLE001
            L.exception("画面を開けませんでした")
            root = getattr(getattr(self, "editor", None), "root", None)
            self.editor = None
            if root is not None:
                from .ui_destroy import destroy
                destroy(root)
            self.editor_closed()
            self.world.message(f"⚠ 画面を開けませんでした: {e}")

    def _render_cities(self, origin, far):
        cm = self.world.cities
        if not cm.enabled:
            return
        g = self.cfg["graphics"]
        detail = float(g.get("city_detail_distance", 100.0))
        far_c = min(far, float(g.get("city_view_distance", 2500.0)))
        for c in cm.cities:
            v = self.city_views.get(c.id)
            if v is None:
                v = self.city_views[c.id] = render.CityView(self.scene_root, c)
            v.update(origin, origin, far_c, detail)
            if c.npcs:
                nv = self.npc_views.get(c.id)
                if nv is None:
                    nv = self.npc_views[c.id] = render.NpcView(self.scene_root, c)
                nv.update(origin, self.world.time)
        self._update_marker(origin)

    def _update_marker(self, origin):
        """最寄りの街の方向に目印。画面の外にあるときは画面の端に寄せ、矢印で向きを示す。"""
        cm = self.world.cities
        if not self.marker_on or not cm.cities:
            self.marker.enabled = False
            return
        now = _time.time()
        if now - self._marker_t > 0.5 or self._marker_city is None:
            self._marker_t = now
            self._marker_city, _, self._marker_surface = cm.marker_target(origin)
        c = self._marker_city
        home = None
        if self.marker_mode == "home":
            hid = self.world.economy.home_id()
            for cc in cm.cities:
                for h in cc.homes:
                    if h["id"] == hid:
                        c, home = cc, h
        if c is None:
            self.marker.enabled = False
            return
        U = self.U
        # 街の中心より少し上（地表の街ならビルの高さの辺り）を指す
        up = c.R[:, 1] if c.kind != "surface" else (c.x - c.host.center) / np.linalg.norm(c.x - c.host.center)
        target = c.x + up * 15.0
        if home is not None:
            target = c.x + c.R @ home["door"] + up * 1.5
        rel = target - origin
        dist = float(np.linalg.norm(rel))
        # カメラの向き（cam.lookAt(look, up) と同じ）で、画面のどこに映るかを計算する
        # ursina は左手系（y-up-left）なので、画面の右は up × look（look_vectors の right と同じ向き）
        _, _, cup, look = self.look_vectors()
        right = np.cross(cup, look)
        right /= max(np.linalg.norm(right), 1e-9)
        upo = np.cross(look, right)
        xc, yc, zc = float(rel @ right), float(rel @ upo), float(rel @ look)
        fov = U["application"].base.cam.node().getLens().getFov()
        tx, ty = np.tan(np.radians(fov[0]) / 2), np.tan(np.radians(fov[1]) / 2)
        aspect = U["window"].aspect_ratio
        inside = False
        if zc > 0.1:
            fx, fy = xc / zc / tx, yc / zc / ty
            inside = abs(fx) <= 0.95 and abs(fy) <= 0.9
        dtxt = f"{dist / 1000:.1f} km" if dist >= 1000 else f"{dist:.0f} m"
        if inside:
            x, y = fx * aspect / 2, fy * 0.5
            arrow = ""
        else:
            dx, dy = xc, yc
            if abs(dx) < 1e-6 and abs(dy) < 1e-6:
                dx = 1.0
            L_ = max((dx * dx + dy * dy) ** 0.5, 1e-9)
            dx, dy = dx / L_, dy / L_
            hx, hy = aspect / 2 - 0.12, 0.5 - 0.1
            k = min(hx / max(abs(dx), 1e-9), hy / max(abs(dy), 1e-9))
            x, y = dx * k, max(dy * k, -0.33)     # 下はアイテムバーに重ならないように
            arrow = ("◀ " if dx < 0 else "▶ ") if abs(dx) >= abs(dy) else ("▲ " if dy > 0 else "▼ ")
        self.marker.position = (x, y, -0.05)
        self.marker_text.text = f"{arrow}{home['name'] + '（自宅）' if home else c.name}  {dtxt}"
        self.marker.enabled = True

    def _render_displays(self, origin, far):
        """コンピューターの表示（看板）を描画。"""
        want = {}
        for cid, c in self.world.constructs.items():
            frames = None
            for i, d in enumerate(c.devs):
                if not d:
                    continue
                disp = d["state"].get("display")
                if not disp or not disp.get("text"):
                    continue
                ctr = c.pos[8 * i]
                if np.linalg.norm(ctr - origin) > min(far, 400.0):
                    continue
                if frames is None:
                    frames = c.voxel_frames()
                for f in render.display_faces(c, i, disp):
                    want[(cid, c.cells[i], f)] = (c, i, f, disp, frames)
        for k in list(self.displays):
            if k not in want:
                self.displays.pop(k).remove()
        for k, (c, i, f, disp, frames) in want.items():
            v = self.displays.get(k)
            if v is None:
                v = self.displays[k] = render.DisplayView(self.scene_root)
            v.set(disp, c.size)
            center, right, up, fwd = render.face_frame(c, i, f, frames)
            v.place(center - origin, right, up, fwd)

    def _set_box(self, ent, center, axes, scale):
        from panda3d.core import LMatrix4f
        m = axes * np.asarray(scale)[:, None]
        ent.setMat(LMatrix4f(m[0, 0], m[0, 1], m[0, 2], 0, m[1, 0], m[1, 1], m[1, 2], 0,
                             m[2, 0], m[2, 1], m[2, 2], 0, center[0], center[1], center[2], 1))

    def _update_aim(self, look):
        w = self.world
        p = w.player
        if p.seated:
            self.hit = None
            self.highlight.hide()
            self.ghost.hide()
            self.aim_text.text = ""
            return
        self.hit = w.raycast(p.eye(), look, float(p.defn.get("reach", 8.0)))
        h = self.hit
        if h and h["kind"] == "voxel":
            c, i = h["construct"], h["index"]
            F = c.voxel_frames()[i]
            ctr = c.pos[8 * i:8 * i + 8].mean(0)
            self._set_box(self.highlight, ctr - self.origin, F, np.full(3, c.size * 1.02))
            self.highlight.show()
            axis, sign = h["face"] // 2, (1 if h["face"] % 2 == 0 else -1)
            g = ctr + F[axis] * sign * c.size
            self._set_box(self.ghost, g - self.origin, F, np.full(3, c.size * 0.98))
            self.ghost.show()
            d = c.devs[i]
            m = c.reg.material(c.mats[i])
            info = f"{c.name}  [{m['name']}]"
            if getattr(c, "anchor", None):
                info += "  ［街に固定中：機器以外のブロックで E を押すと切り離し］"
            if d:
                info += f"\n{d['name']} ({d['def']}) 向き {DIR_NAMES[d['dir']]}"
                same = sum(1 for x in c.devs if x and x["name"] == d["name"])
                if same > 1:
                    info += f"  （同名 {same} 個：まとめて動く）"
                if d["def"] and (c.reg.device(d["def"]) or {}).get("type") == "computer":
                    info += "  ［E 起動/停止・P プログラム編集］"
                elif (c.reg.device(d["def"]) or {}).get("type") != "actuator":
                    info += "  ［P 名前の設定］"
                t_ = (c.reg.device(d["def"]) or {}).get("type")
                surv = not w.settings.get("creative", True)
                if t_ == "tank":
                    info += "  ［E 手持ちの燃料を入れる］" if surv else "  ［E 中身の切り替え］"
                elif t_ == "processor" and surv:
                    info += "  ［E 原料を入れる・できた物を受け取る］"
                if (c.reg.device(d["def"]) or {}).get("type") == "actuator":
                    from .. import actuator as act
                    st = d["state"]
                    info += (f"\n角度 {st.get('angle', 0):.0f}°  伸び {st.get('ext', 0):.2f}  "
                             f"結合 {'あり' if act.is_attached(c, i) else 'なし'}"
                             f"\n設定: {act.preset_text(st)}  ［E 実行・P 動作設定］")
                is_act = (c.reg.device(d["def"]) or {}).get("type") == "actuator"
                st = {} if is_act else {k: v for k, v in d["state"].items()
                                        if not k.startswith("_") and k not in ("memory", "display")}
                if st:
                    info += "\n" + ", ".join(f"{k}={_fmt(v)}" for k, v in st.items())
            s = c.status
            if s:
                info += f"\n電力 {s.get('stored', 0)/1e6:.2f}/{s.get('capacity', 0)/1e6:.1f} MJ  発電 {s.get('production', 0)/1e3:.1f} kW  消費 {s.get('demand', 0)/1e3:.1f} kW"
            from ..devices import cavity_report
            cav = cavity_report(c)
            if cav:
                info += "\n気密空洞: " + ", ".join(f"{n}セル {p_:.1f}kPa" for _, n, p_ in cav)
            if c.storage:
                info += "\n資源: " + c.reg.res_text(c.storage, ", ")
            self.aim_text.text = info
        elif h and h["kind"] == "terrain":
            self.highlight.hide()
            size = float(w.settings.get("voxel_size", 0.5))
            g = h["point"] + h["normal"] * size * 0.55
            fwd, right, up, _ = self.look_vectors()
            self._set_box(self.ghost, g - self.origin, np.stack([right, h["normal"], fwd]), np.full(3, size))
            self.ghost.show()
            if w.settings.get("creative", True):
                self.aim_text.text = f"{h['planet'].name} の地面（左クリックで掘る）"
            else:
                res = w.registry.res_name(h["planet"].resource_at(h["point"]))
                self.aim_text.text = f"{h['planet'].name} の地面：{res}（左クリックで掘る）"
        elif h and h["kind"] == "npc":
            self.highlight.hide()
            self.ghost.hide()
            n = h["npc"]
            from .. import citylife
            act = {"trader": "取引する", "estate": "部屋を見る", "citizen": "話す"}[n["role"]]
            if w.settings.get("creative", True) and n["role"] != "citizen":
                act = "話す（取引はサバイバルだけ）"
            self.aim_text.text = f"{n['name']}（{citylife.ROLE_NAMES[n['role']]}）［E {act}］"
        elif h and h["kind"] == "city" and h["city"].parts[h["part"]].get("tag"):
            self.highlight.hide()
            self.ghost.hide()
            tag = h["city"].parts[h["part"]]["tag"]
            eco = w.economy
            if tag[0] == "counter":
                self.aim_text.text = "カウンター［E 取引する］"
            else:
                home = next((x for x in h["city"].homes if x["id"] == tag[1]), None)
                name = home["name"] if home else "部屋"
                if not eco.can_use(tag[1]):
                    self.aim_text.text = f"{name} の{'ベッド' if tag[0] == 'bed' else '収納箱'}（不動産屋で買うか借りると使えます）"
                elif tag[0] == "bed":
                    self.aim_text.text = f"{name} のベッド［E 休む：保存して、ここを自宅に］"
                else:
                    self.aim_text.text = f"{name} の収納箱［E 開く］"
        elif h and h["kind"] == "city":
            self.highlight.hide()
            size = float(w.settings.get("voxel_size", 0.5))
            g = h["point"] + h["normal"] * size * 0.55
            fwd, right, up, _ = self.look_vectors()
            self._set_box(self.ghost, g - self.origin, np.stack([right, h["normal"], fwd]), np.full(3, size))
            self.ghost.show()
            city = h["city"]
            st = int(city.state[city.bld[h["part"]]]) if city.bld[h["part"]] >= 0 else 0
            kind_ = city.parts[h["part"]]["kind"]
            what = {"ground": "地面", "stair": "階段"}.get(kind_, "建物")
            txt = f"{city.name} の{what}" + ("（壊れている）" if st == 1 else "")
            if city.kind == "orbit":
                txt += "\n右クリックで置いたブロックは街に固定（推進機で街ごと動かせる）"
            self.aim_text.text = txt
        elif h and h["kind"] == "water":
            self.highlight.hide()
            size = float(w.settings.get("voxel_size", 0.5))
            g = h["point"] + h["normal"] * size * 0.55
            fwd, right, up, _ = self.look_vectors()
            self._set_box(self.ghost, g - self.origin, np.stack([right, h["normal"], fwd]), np.full(3, size))
            self.ghost.show()
            self.aim_text.text = "水面" + ("" if w.settings.get("creative", True) else "（左クリックで水をくむ：いくらくんでも減りません）")
        else:
            self.highlight.hide()
            self.ghost.hide()
            self.aim_text.text = ""

    def _update_hud(self, dt):
        w = self.world
        p = w.player
        pl = w.nearest_planet(p.pos)
        alt = float(pl.sdf(p.pos[None])[0]) if pl else 0
        spd = float(np.linalg.norm(p.vel))
        lines = [f"Alpedia  ワールド: {w.name}  時刻 {w.time:.0f}s",
                 f"{pl.name if pl else '宇宙'}  高度 {alt:.1f} m  速度 {spd:.1f} m/s",
                 f"構築物 {len(w.constructs)}  頂点 {sum(c.n_vert for c in w.constructs.values())}",
                 f"ジェットパック {'ON' if p.jetpack else 'OFF'}{'' if w.settings.get('creative', True) or not p.jetpack else ('（燃料 %.0f）' % p.inventory.get('fuel', 0.0))}  {'クリエイティブ' if w.settings.get('creative', True) else 'サバイバル'}{'  完全剛体' if w.settings.get('full_rigid') else ''}"]
        if p.seated:
            c = w.constructs.get(p.seated[0])
            if c:
                vv = c.velocity()
                lines.append(f"操縦中: {c.name}  速度 {np.linalg.norm(vv):.1f} m/s  質量 {c.total_mass()/1000:.1f} t")
                o = w.orbit_info(c.center_of_mass(), vv)
                if o:
                    ap = "∞" if o["apoapsis"] == float("inf") else f"{o['apoapsis']:.0f}"
                    pe = f"{o['periapsis']:.0f}" if o["periapsis"] > -o["radius"] * 0.99 else "—"
                    lines.append(f"軌道: 遠点 {ap} m / 近点 {pe} m  水平速度 {o['horizontal_speed']:.1f} / 円軌道 {o['orbital_speed']:.1f} m/s")
        if not w.settings.get("creative", True) and p.inventory:
            lines.append("所持: " + w.registry.res_text(p.inventory, ", "))
        self.hud.text = "\n".join(lines)
        self.hotbar_view.refresh()
        item = p.hotbar[self.sel]
        if item is None:
            self.sel_text.text = f"[{self.sel + 1}] （空）  I でインベントリ"
        else:
            kind, id_ = item
            if kind == "material":
                name = w.registry.material(id_)["name"]
                extra = ""
            else:
                dd = w.registry.device(id_)
                name = dd["name"]
                extra = f"  向き {DIR_NAMES[self.dir_index]}" if dd.get("directional") else ""
            cost = w.item_cost((kind, id_))
            cs = "" if w.settings.get("creative", True) or not cost else "  費用 " + w.registry.res_text(cost, ", ")
            self.sel_text.text = f"{'素材' if kind == 'material' else '機器'}: {name}{extra}{cs}"
        now = _time.time()
        self.msg_text.text = "\n".join(m for t, m in w.msgs if now - t < 8)


def _fmt(v):
    if isinstance(v, float):
        return f"{v:.2f}"
    if isinstance(v, dict):
        return "{" + ", ".join(f"{k}:{_fmt(x)}" for k, x in v.items()) + "}"
    return str(v)
