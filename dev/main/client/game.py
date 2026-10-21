"""Ursina によるゲーム画面・操作。"""
import time as _time
from pathlib import Path

import numpy as np

from .. import log, native
from ..construct import DIR_NAMES
from . import render

L = log.get("client")

FONT_CANDIDATES = [
    "C:/Windows/Fonts/meiryo.ttc", "C:/Windows/Fonts/YuGothM.ttc", "C:/Windows/Fonts/BIZ-UDGothicR.ttc",
    "C:/Windows/Fonts/msgothic.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
]

HELP = """[操作]  （Esc →「操作とキー設定」・F1 でいつでも見られます）
WASD 移動 / Space ジャンプ・上昇 / Ctrl 下降 / F ジェットパック
マウス 視点 / 左クリック 削除・掘る / 右クリック 設置
I インベントリ / ホイール・数字キー アイテムバーの選択 / R 向き変更 / N 空中に新しい構築物
E 機器の操作・操縦席に座る/降りる・街に固定・NPC と話す/取引 / V 三人称
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


class BoxBody:
    """コアの体の見た目：定義の色の箱（または定義の .obj モデル）。"""

    def __init__(self, game, player, model=None):
        U = game.U
        col = player.defn.get("color", [1, 1, 1])
        self.game, self.player = game, player
        self.e = U["Entity"](model=model or "cube", parent=game.scene_root,
                             color=U["color"].rgb(*col) if max(col) <= 1 else U["color"].rgb32(*col))
        self.e.hide()

    def update(self, origin, shown, dt):
        if not shown:
            self.e.hide()
            return
        p = self.player
        up = p.up / np.linalg.norm(p.up)
        f = np.asarray(p.look_fwd, float)
        f = f - up * (f @ up)
        f = f / np.linalg.norm(f) if np.linalg.norm(f) > 1e-6 else np.cross(up, [1.0, 0, 0])
        right = np.cross(up, f)
        center = p.pos + up * (p.height / 2 - p.radius) - origin
        self.game._set_box(self.e, center, np.stack([right, up, f]), np.array([p.radius * 2, p.height, p.radius * 1.4]))
        self.e.show()

    def destroy(self):
        from .ui_destroy import destroy
        destroy(self.e)


class Game:
    def __init__(self, world, cfg, on_exit=None):
        self.world = world
        self.cfg = cfg
        self.on_exit = on_exit or (lambda action, status="": None)

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
        self.load_errors = []
        self.views = {}
        self.planet_views = [render.PlanetView(self.scene_root, loader, p) for p in self.world.planets]
        g = self.cfg["graphics"]
        from .. import paths as _paths
        render.STYLE = {"edges": bool(g.get("block_edges", True)), "ao": bool(g.get("block_ao", True))}
        tex_dir = _paths.ROOT / "sauce" / "textures"
        render.EDGE_TEXTURE = str(tex_dir / "px_edge.png")
        render.DEVICE_TEXTURE = str(tex_dir / "px_device.png")
        # MOD を含むテクスチャをここで全部読み込む（プレイ中にファイルを読みに行かない）
        self.load_errors += render.preload(loader, self.world.registry)
        self.patch = render.TerrainPatch(self.scene_root, loader, int(g.get("terrain_patch_cells", 48)),
                                         float(g.get("terrain_patch_step", 1.0)))
        self.carve = render.CarveView(self.scene_root, loader)
        # 水中にいる時の青いもや
        self.underwater = Entity(parent=camera.ui, model="quad", scale=(3, 2), color=color.rgba(0.1, 0.3, 0.55, 0.45),
                                 z=1, enabled=False)
        self.highlight = Entity(model="wireframe_cube", color=color.yellow, parent=self.scene_root)
        self.ghost = Entity(model="wireframe_cube", color=color.lime, parent=self.scene_root)
        # UI
        ui = self.ui_root
        self.crosshair = Entity(parent=ui, model="quad", scale=0.006, color=color.white)
        self.hud = Text(parent=ui, position=window.top_left + (0.01, -0.01), origin=(-0.5, 0.5),
                        scale=0.75, text="", background=False)
        self.sel_text = Text(parent=ui, position=(0, -0.385), origin=(0, 0), scale=0.85, text="")
        self.msg_text = Text(parent=ui, position=window.left + (0.01, 0.06), origin=(-0.5, -0.5),
                             scale=0.7, text="")
        self.aim_text = Text(parent=ui, position=(0.02, -0.03), origin=(-0.5, 0.5), scale=0.7, text="")
        # 拡張（街パックなど）の画面側
        from ..modcode import client_addons, load_client
        load_client(self.load_errors)
        self.addons = client_addons(self, self.load_errors)
        # プレイヤーの体：MOD（VRM の見た目など）が引き受けなければ、コアが定義の色の箱（またはモデル）で描く。
        # 見た目のモデルはここで全部読み込む（ゲーム中に見た目を変える・ほかの参加者が来ても、ファイルを読まない）
        self.bodies = {}            # キー → (見た目の ID, Player, 体)
        self.labels = {}
        self._obj_models = self._load_player_models(load_model)
        # 画面のボタン（タッチ・マウス）と仮想ジョイスティック
        self.vkeys = {}
        self.cam_mode = None
        # MOD のモーション（カメラモードで選ぶ）：{名前: モーション}
        self.emotes = {}
        for a in self.addons:
            try:
                for name, spec in (a.emotes() or {}).items():
                    self.emotes[name] = spec
            except Exception as e:  # noqa: BLE001
                self.load_errors.append(f"MOD '{getattr(a, 'name', '?')}' のモーションを読めません: {e}")
        from .touch_ui import TouchHUD
        self.touch_hud = TouchHUD(self)
        # 一時停止メニュー
        self.pause_root = Entity(parent=ui, enabled=False)
        Entity(parent=self.pause_root, model="quad", scale=(3, 2), color=color.rgba(0, 0, 0, 0.55), z=0.1)
        Text(parent=self.pause_root, text="一時停止中", position=(0, 0.25), origin=(0, 0), scale=2)

        def pbtn(text, y, fn, col=None):
            b = Button(parent=self.pause_root, text=text, position=(0, y), scale=(0.36, 0.06),
                       color=col or color.rgb(0.22, 0.32, 0.45), text_size=0.8)
            b.on_click = fn
            return b
        self._pbtn = pbtn
        self._build_pause()
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
        self._perf = dict.fromkeys(("frames", "wall", "phys", "ticks", "render", "update", "lost"), 0)
        self._perf_text = ""
        self._perf_log_t = _time.monotonic()
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
        if self.load_errors:
            from ..modcode import ModLoadError
            raise ModLoadError(self.load_errors)
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

    def open_editor(self, hit=None):
        w = self.world
        target = None
        if hit is None:
            hit = self.hit
            if not (hit and hit["kind"] == "voxel" and hit["construct"].devs[hit["index"]]):
                near = next((t for t in self.nearby_targets() if t["kind"] == "p"), None)
                if near is not None:
                    hit = near["hit"]           # 見ていなくても、近くの機器の設定を開く
        if w.player.seated:
            c = w.constructs.get(w.player.seated[0])
            if c and self._computers(c):
                target = (c, self._computers(c)[0])
        elif hit and hit["kind"] == "voxel":
            c, i = hit["construct"], hit["index"]
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
            from .script_panel import ScriptPanel
            self.editor = ScriptPanel(self, c, i)

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
        self.set_mouse_capture(not self.paused and self._want_capture())
        self.pause_root.enabled = self.paused
        if self.paused:
            self._build_pause()

    # --- カメラモード ------------------------------------------------------------
    def enter_camera(self):
        if self.cam_mode is not None or self.editor is not None:
            return
        from .camera_mode import CameraMode
        self.touch_hud._release_all()
        self.cam_mode = CameraMode(self)

    def exit_camera(self):
        if self.cam_mode is None:
            return
        self.cam_mode.destroy()
        self.cam_mode = None
        self._esc_block = _time.time() + 0.2
        self.set_mouse_capture(self._want_capture())

    def _want_capture(self):
        """マウスで視点を回す（カーソルを隠して捕まえる）か。タッチ操作の ON・Alt を押している間は捕まえない。"""
        if self.cfg["controls"].get("touch", False) or self.cam_mode is not None:
            return False
        try:
            hk = self.U["held_keys"]
            return not (hk["left alt"] or hk["alt"] or hk["right alt"])
        except Exception:  # noqa: BLE001
            return True

    def set_touch(self, on):
        """仮想ジョイスティック（タッチ操作）の ON/OFF（設定に保存）。"""
        self.cfg["controls"]["touch"] = bool(on)
        try:
            from .menu import save_config
            save_config(self.cfg)
        except Exception:  # noqa: BLE001
            L.exception("設定を保存できません")
        self.world.message("タッチ操作（仮想ジョイスティック） " + ("ON" if on else "OFF"))
        self._build_pause()

    def open_keys(self):
        from .keys_panel import KeysPanel
        extra = [a.help_text for a in self.addons if a.help_text]
        self._open_panel(lambda: KeysPanel(self.U, self.cfg, self.ui_root.parent, self.editor_closed, extra,
                                           on_change=self._keys_changed))

    def _keys_changed(self):
        self._build_pause()

    # --- 操作（キー・画面のボタン・近くのもの） ---------------------------------
    def do_remove(self):
        w = self.world
        if self.hit and self.hit["kind"] == "voxel":
            w.remove(self.hit)
        elif self.hit and self.hit["kind"] in ("terrain", "water"):
            w.dig(self.hit)

    def do_place(self):
        item = self.current_item()
        if item:
            _, _, _, look = self.look_vectors()
            self.world.place(self.hit, item, self.dir_index, look)

    def do_interact(self, hit):
        if not any(a.interact(hit) for a in self.addons):
            self.world.interact(hit)

    def do_target(self, t):
        """近くのもの（画面右のボタン）を使う。"""
        if t["kind"] == "p":
            self.open_editor(t["hit"])
        else:
            self.do_interact(t["hit"])

    def _usable(self, hit):
        """視線の先が E で使える物か（機器・お店・NPC など）。"""
        if not hit:
            return False
        if hit["kind"] == "voxel":
            return bool(hit["construct"].devs[hit["index"]]) or any(a.aim(hit) for a in self.addons)
        return any(a.aim(hit) for a in self.addons)

    def nearby_targets(self, radius=2.6):
        """近くにある、触れるもの（近い順）：[{"label", "hit", "kind": "e"(E)/"p"(設定), "icon", "dist"}]"""
        w = self.world
        p = w.player
        if p.seated:
            return []
        eye = p.eye()
        out = []
        surv = not w.creative
        names = {"seat": "座る", "computer": "起動・停止", "tank": "燃料を入れる" if surv else "中身を切り替える",
                 "actuator": "動かす", "processor": "原料を入れる・受け取る" if surv else None}
        for c in w.constructs.values():
            if not c.n_vox or not any(c.devs):
                continue
            lo, hi = c.pos.min(0) - radius, c.pos.max(0) + radius
            if np.any(eye < lo) or np.any(eye > hi):
                continue
            for i, d in enumerate(c.devs):
                if not d:
                    continue
                ctr = c.pos[8 * i:8 * i + 8].mean(0)
                dist = float(np.linalg.norm(ctr - eye))
                if dist > radius:
                    continue
                dd = c.reg.device(d["def"]) or {}
                t_ = dd.get("type")
                hit = {"kind": "voxel", "t": dist, "construct": c, "index": i, "face": 2, "point": ctr}
                lab = names.get(t_, "操作") if t_ in names else "操作"
                if lab:
                    out.append({"label": f"{lab}（{d['name']}）", "hit": hit, "kind": "e", "dist": dist,
                                "icon": "interact"})
                if t_ in ("computer", "actuator") or t_ not in ("seat",):
                    sl = {"computer": "プログラム", "actuator": "動作の設定"}.get(t_, "名前の設定")
                    out.append({"label": f"{sl}（{d['name']}）", "hit": hit, "kind": "p", "dist": dist + 0.01,
                                "icon": "settings"})
        for a in self.addons:
            try:
                out += a.nearby(p.pos, radius) or []
            except Exception:  # noqa: BLE001
                L.exception("近くのものを探せません（%s）", a.name)
        out.sort(key=lambda t: t["dist"])
        # 同じ物の E と設定は並べて、最大 4 つ
        return out[:4]


    def _leave(self, action, save=True, status=""):
        if self.leaving:
            return
        net = self.world.net
        if save and not (net is not None and net.role == "client"):
            try:
                self.world.save()
            except Exception:  # noqa: BLE001
                L.exception("保存に失敗")
        if net is not None:
            try:
                net.close()
            except Exception:  # noqa: BLE001
                L.exception("マルチプレイの終了でエラー")
        self.leaving = True
        self.on_exit(action, status)

    def destroy(self):
        from .ui_destroy import destroy
        if getattr(self, "cam_mode", None) is not None:
            try:
                self.cam_mode.destroy()
            except Exception:  # noqa: BLE001
                pass
            self.cam_mode = None
        if self.editor is not None:
            try:
                destroy(self.editor.root)
            except Exception:  # noqa: BLE001
                pass
            self.editor = None
        for v in self.views.values():
            v.remove()
        self.views = {}
        for ent in getattr(self, "bodies", {}).values():
            try:
                ent[2].destroy()
            except Exception:  # noqa: BLE001
                pass
        self.bodies = {}
        for t in getattr(self, "labels", {}).values():
            destroy(t)
        self.labels = {}
        if self.world.net is not None:
            try:
                self.world.net.close()
            except Exception:  # noqa: BLE001
                L.exception("マルチプレイの終了でエラー")
        for a in getattr(self, "addons", []):
            a.destroy()
        self.addons = []
        for e in (self.driver, self.scene_root, self.ui_root, self.underwater):
            destroy(e)
        self.set_mouse_capture(False)
        L.info("ゲーム終了: %s", self.world.name)

    # ------------------------------------------------------------------
    def _load_player_models(self, load_model):
        """見た目の定義の .obj モデル（VRM は MOD が読む）。"""
        out = {}
        for d in self.world.registry.defs["players"].values():
            m = d.get("model")
            if not m or str(m).lower().endswith(".vrm") or m in out:
                continue
            try:
                out[m] = load_model(str(m))
            except Exception as e:  # noqa: BLE001
                out[m] = None
                self.load_errors.append(f"プレイヤーのモデル '{m}' を読み込めません: {e}")
            if out[m] is None and not any(str(m) in x for x in self.load_errors):
                self.load_errors.append(f"プレイヤーのモデル '{m}' を読み込めません")
        return out

    def _new_body(self, player):
        for a in self.addons:
            try:
                b = a.player_body(player)
            except Exception:  # noqa: BLE001
                L.exception("体の見た目を作れません（%s）", getattr(a, "name", "?"))
                b = None
            if b is not None:
                return b
        return BoxBody(self, player, self._obj_models.get(player.defn.get("model")))

    def _render_players(self, origin, dt):
        """自分と、マルチプレイのほかの人の体・名前。見た目を変えたら作り直す。"""
        w = self.world
        net = w.net
        want = [("me", w.local_player)]
        if net is not None:
            others = net.others() if callable(getattr(net, "others", None)) else list(net.others.values())
            want += [(f"p{p.pid}", p) for p in others]
        keys = set()
        for key, p in want:
            keys.add(key)
            ent = self.bodies.get(key)
            if ent is None or ent[0] != p.defn.get("id") or ent[1] is not p:
                if ent is not None:
                    ent[2].destroy()
                ent = self.bodies[key] = (p.defn.get("id"), p, self._new_body(p))
            if key == "me":
                shown = self.third_person or self.cam_mode is not None
            else:
                q = p.pos - origin
                shown = float(q @ q) < 500.0 ** 2
            ent[2].update(origin, shown, dt)
            if key != "me":
                self._label(key, p, origin, shown)
        for key in [k for k in self.bodies if k not in keys]:
            self.bodies.pop(key)[2].destroy()
        for key in [k for k in self.labels if k not in keys]:
            from .ui_destroy import destroy
            destroy(self.labels.pop(key))

    def _label(self, key, p, origin, shown):
        t = self.labels.get(key)
        if t is None:
            t = self.labels[key] = self.U["Text"](text=p.name, parent=self.scene_root, billboard=True, origin=(0, 0),
                                                  scale=2, color=self.U["color"].white)
        if t.text != p.name:
            t.text = p.name
        t.enabled = bool(shown)
        if shown:
            up = p.up / np.linalg.norm(p.up)
            q = p.pos + up * (p.height - p.radius + 0.3) - origin
            t.setPos(float(q[0]), float(q[1]), float(q[2]))
            t.setScale(float(np.clip(np.linalg.norm(q) * 0.12, 1.2, 40.0)))   # 遠くても読める大きさ

    # --- 一時停止メニュー（保存・見た目・LAN） ---------------------------------
    def _build_pause(self):
        from .ui_destroy import destroy
        U = self.U
        color, Text = U["color"], U["Text"]
        for e in list(getattr(self, "_pause_items", [])):
            destroy(e)
        items = self._pause_items = []
        pbtn = self._pbtn
        net = self.world.net
        client = net is not None and net.role == "client"
        items.append(pbtn("再開", 0.12, self.toggle_pause, color.rgb(0.2, 0.5, 0.3)))
        if client:
            items.append(pbtn("ワールドから抜けてメニューへ", 0.04, lambda: self._leave("menu"), color.rgb(0.4, 0.3, 0.2)))
            items.append(pbtn("ワールドから抜けて終了", -0.04, lambda: self._leave("quit"), color.rgb(0.5, 0.2, 0.2)))
        else:
            items.append(pbtn("保存", 0.04, self.world.save))
            items.append(pbtn("保存してメニューへ", -0.04, lambda: self._leave("menu")))
            items.append(pbtn("保存せずにメニューへ", -0.12, lambda: self._leave("menu", save=False), color.rgb(0.4, 0.3, 0.2)))
            items.append(pbtn("保存して終了", -0.20, lambda: self._leave("quit"), color.rgb(0.5, 0.2, 0.2)))
            if net is None:
                items.append(pbtn("LAN に公開（マルチプレイ）", -0.30, self.start_hosting, color.rgb(0.25, 0.35, 0.55)))
            else:
                items.append(Text(parent=self.pause_root, origin=(0, 0), position=(0, -0.30), scale=0.8,
                                  text="LAN に公開中: " + "  ".join(net.addresses()[:2])))
        # 右の列：操作の説明・キー設定、タッチ操作
        b = pbtn("操作とキー設定", 0.12, self.open_keys, color.rgb(0.25, 0.35, 0.45))
        b.x = 0.45
        items.append(b)
        touch = bool(self.cfg["controls"].get("touch", False))
        b = pbtn(f"仮想ジョイスティック {'ON' if touch else 'OFF'}", 0.04, lambda: self.set_touch(not touch),
                 color.rgb(0.2, 0.45, 0.35) if touch else color.rgb(0.3, 0.3, 0.33))
        b.x = 0.45
        items.append(b)
        items.append(Text(parent=self.pause_root, origin=(0, 0.5), position=(0.45, -0.01), scale=0.6,
                          color=color.light_gray,
                          text="ON：画面左下のスティックで移動、ほかの所をなぞって視点\n"
                               "（タッチ画面向け。OFF でもボタンは Alt を押すと押せます）"))
        # 見た目（自分だけ。ほかの人に見える）
        defs = list(self.world.registry.defs["players"].values())
        cur = self.world.local_player.defn
        items.append(Text(parent=self.pause_root, origin=(0, 0), position=(0, 0.19), scale=0.9,
                          text=f"見た目: {cur.get('name', cur.get('id'))}"))
        if len(defs) > 1:
            from ursina import Button
            for x, d, txt in ((-0.25, -1, "◀"), (0.25, 1, "▶")):
                b = Button(parent=self.pause_root, text=txt, position=(x, 0.19), scale=(0.045, 0.045),
                           color=color.rgb(0.22, 0.32, 0.45), text_size=0.8)
                b.on_click = (lambda d=d: self.cycle_skin(d))
                items.append(b)

    def cycle_skin(self, step):
        """見た目を変える（宇宙飛行士・VRM など。ワールドに入っている MOD の見た目から選ぶ）。"""
        w = self.world
        ids = list(w.registry.defs["players"])
        cur = w.local_player.defn.get("id")
        i = ids.index(cur) if cur in ids else 0
        w.local_player.defn = w.player_def(ids[(i + step) % len(ids)])
        w.message(f"見た目: {w.local_player.defn.get('name')}")
        self._build_pause()

    def start_hosting(self):
        from ..netserver import Host
        try:
            Host(self.world, self.cfg)
        except OSError as e:
            L.exception("LAN に公開できません")
            self.world.message(f"LAN に公開できません（ポート {self.cfg.get('multiplayer', {}).get('port')} が使えない？）: {e}")
            return
        self.world.message("LAN に公開しました: " + "  ".join(self.world.net.addresses()[:2])
                           + "（参加する人はメニューの「マルチプレイ」から）")
        self._build_pause()

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
        if self.cam_mode is not None:
            self.cam_mode.on_key(key)
            return
        if key == "escape":
            self.toggle_pause()
            return
        if self.paused:
            return
        if key in ("left mouse down", "right mouse down") and (
                self.cfg["controls"].get("touch", False) or self.touch_hud.mouse_over_ui()):
            return  # 画面のボタンを押した（ボタンの方で処理する）・タッチ操作中は画面を触っても掘らない
        if key == c.get("edit_script", "p"):
            self.open_editor()
            return
        if key == c.get("toggle_help", "f1"):
            self.open_keys()
            return
        if key == c.get("camera_mode", "c") and not w.player.seated:
            self.enter_camera()
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
            self.do_remove()
        elif key == "right mouse down":
            self.do_place()
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
            hit = self.hit
            if not self._usable(hit):          # 見ている物が使えなければ、近くにある物（画面右のボタンの一番上）
                near = next((t for t in self.nearby_targets() if t["kind"] == "e"), None)
                if near is not None:
                    hit = near["hit"]
            self.do_interact(hit)
        elif key == c["jetpack"]:
            w.player.jetpack = not w.player.jetpack
            w.message("ジェットパック " + ("ON" if w.player.jetpack else "OFF"))
        elif key == c["save"]:
            w.save()
        elif key == c["export"]:
            self._export()
        elif key == c["import"]:
            self._import()
        elif key == c.get("third_person", "v"):
            self.third_person = not self.third_person
        else:
            for a in self.addons:
                if a.on_key(key):
                    break

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
    def update(self):
        U = self.U
        w = self.world
        dt = min(U["utime"].dt, 0.1)
        held = U["held_keys"]
        c = self.cfg["controls"]
        net = w.net
        hud = self.touch_hud
        cam_mode = self.cam_mode
        if cam_mode is None:
            hud.update(dt)                 # 画面のボタン・ジョイスティック（押されたキー → self.vkeys）
        vk = self.vkeys

        def H(k):
            return 1 if (held[k] or vk.get(k)) else 0
        online = net is not None        # LAN に公開中・参加中は、一時停止しても世界と通信を止めない（参加の受け付けも）
        if self.paused and not online:
            return                  # マルチプレイ中は一時停止しても世界は止めない（ほかの人がいるので）
        if cam_mode is not None:
            cam_mode.update(dt, held, vk, np.zeros(2))     # カメラを動かす（自分は動かない）
        editing = self.editor is not None or self.paused or cam_mode is not None
        if not editing or cam_mode is not None:
            want = self._want_capture()
            if want != bool(self._captured):
                self.set_mouse_capture(want)    # Alt を押している間・タッチ操作中はカーソルを出す
        # 視点
        sens = float(c.get("mouse_sensitivity", 60.0))
        dx = dy = 0.0
        if self._captured and not editing:
            dx, dy = self._look_delta()
        if not editing:
            tdx, tdy = hud.take_look()          # 画面をなぞった量（タッチ）
            dx, dy = dx + tdx * 1.6, dy + tdy * 1.6
        if dx or dy:
            self._rotate_yaw(np.radians(dx * sens))
            self.pitch = float(np.clip(self.pitch + np.radians(dy * sens), -1.55, 1.55))
        fwd, right, up, look = self.look_vectors()
        w.local_player.look_fwd, w.local_player.pitch = fwd, self.pitch
        # 入力（キーボード ＋ 画面のボタン・ジョイスティック）
        st = hud.stick
        move = np.zeros(3)
        if not w.player.seated and not editing:
            move += fwd * (H(c["forward"]) - H(c["back"]) + st[1])
            move += right * (H(c["right"]) - H(c["left"]) + st[0])
            nm = np.linalg.norm(move)
            if nm > 1:
                move /= nm
        jump = bool(H(c["jump"])) and not w.player.seated and not editing
        thrust = np.zeros(3)
        if not w.player.seated and not editing:
            # ジェットパックの噴射方向（水中では泳ぐ方向にも使う）
            thrust = move * 0.6 + up * (H(c["jump"]) - H(c["down"]))
        if w.player.seated and not editing:
            keys = {k for k in SEAT_KEYS if H(k)}
            # 操縦席ではジョイスティックを W/A/S/D として使う
            for k, on in ((c["forward"], st[1] > 0.4), (c["back"], st[1] < -0.4), (c["right"], st[0] > 0.4),
                          (c["left"], st[0] < -0.4)):
                if on:
                    keys.add(k)
            w.input_keys = keys
        else:
            w.input_keys = set()
        # 固定ステップでシミュレーション
        tick = 1.0 / float(self.cfg["physics"].get("tick_rate", 60))
        self.acc += dt
        n = 0
        pf = self._perf
        t_phys = _time.perf_counter()
        client = net is not None and net.role == "client"
        if client:
            net.look = look
            net.frame(self, dt)            # ホストから届いた世界を当てる
            if net.closed:
                self._leave("menu", status=f"マルチプレイから抜けました：{net.closed}")
                return
        while self.acc >= tick and n < 3:
            try:
                if client:
                    net.step(tick, move, jump and n == 0, thrust)      # 自分の体だけ動かす
                else:
                    w.step(tick, move, jump and n == 0, thrust)
            except Exception:  # noqa: BLE001
                L.exception("シミュレーションエラー")
                raise
            self.acc -= tick
            n += 1
        if net is not None and not client:
            net.tick(dt)                   # 参加者へ送る・参加者の操作を行う
        t_rend = _time.perf_counter()
        pf["phys"] += t_rend - t_phys
        pf["ticks"] += n
        if n == 3:
            pf["lost"] += self.acc      # 間に合わずに捨てた時間（ゲーム内の時間が遅れる）
            self.acc = 0.0
        # カメラ
        fwd, right, up, look = self.look_vectors()
        origin = w.player.eye()
        cam = U["camera"]
        cam_up = up
        cam_look = look
        if self.cam_mode is not None:
            origin, cam_look, cam_up, _ = self.cam_mode.camera()
        elif self.third_person:
            origin = origin - look * 5.0 + up * 0.8
        cam.position = (0, 0, 0)
        from panda3d.core import LPoint3f, LVector3f
        cam.lookAt(LPoint3f(*cam_look.astype(np.float32)), LVector3f(*cam_up.astype(np.float32)))
        # 描画
        t_r = _time.perf_counter()
        self._render(origin)
        pf["render"] += _time.perf_counter() - t_r
        if self.cam_mode is not None and not self.cam_mode.frame_on:
            self.highlight.hide()
            self.ghost.hide()
        elif not editing or self.cam_mode is not None:
            self._update_aim(look)
        self._update_hud(dt)
        pf["update"] += _time.perf_counter() - t_phys
        self._perf_frame(U["utime"].dt)

    def _perf_frame(self, real_dt):
        """処理の重さの内訳を数秒ごとにまとめる（画面左上と、10秒ごとにログ）。"""
        pf = self._perf
        pf["frames"] += 1
        pf["wall"] += real_dt
        if pf["wall"] < 2.0:
            return
        f = max(pf["frames"], 1)
        fps = f / pf["wall"]
        ms = lambda x: x / f * 1000  # noqa: E731
        phys_tick = pf["phys"] / max(pf["ticks"], 1) * 1000
        other = max(pf["wall"] - pf["update"], 0.0) / f * 1000
        slow = pf["lost"] / pf["wall"] * 100
        self._perf_text = (f"fps {fps:.0f}  物理 {phys_tick:.1f} ms/回×{pf['ticks'] / f:.1f}  "
                           f"描画準備 {ms(pf['render']):.1f} ms  描画・待ち {other:.1f} ms"
                           + (f"  遅れ {slow:.0f}%" if slow >= 1 else ""))
        now = _time.monotonic()
        if now - self._perf_log_t >= 10.0:
            self._perf_log_t = now
            L.info("重さ: %s（構築物 %d, 物理:%s）", self._perf_text, len(self.world.constructs), native.name())
        for k in pf:
            pf[k] = 0

    def _render(self, origin):
        w = self.world
        for cid in list(self.views):
            if cid not in w.constructs:
                self.views.pop(cid).remove()
        far = float(self.cfg["graphics"].get("view_distance", 3000))
        decal_dist = float(self.cfg["graphics"].get("decal_distance", 30.0))
        for cid, c in w.constructs.items():
            v = self.views.get(cid)
            if v is not None and v.c is not c:       # マルチプレイ：形が変わって作り直された
                v.remove()
                v = None
            if v is None:
                v = self.views[cid] = render.ConstructView(self.scene_root, self.loader, c)
            q = c.pos[0] - origin
            if float(q @ q) > far * far:
                v.set_visible(False)
                continue
            v.set_visible(True)
            v.update(origin)
            v.update_decals(origin, origin, decal_dist)
            v.update_displays(origin, far)
        for a in self.addons:
            a.update(origin, far)
        npl = w.nearest_planet(w.player.pos)
        self.carve.update(npl, w.player.pos, origin)
        self.patch.update(npl, w.player.pos, origin, self.carve)
        for pv in self.planet_views:
            pv.update(origin, w.player.pos, self.patch.frame_for(pv.p))
        self.underwater.enabled = bool(npl is not None and npl.has_water
                                       and npl.water_depth(origin[None])[0] > 0.02)
        # プレイヤーの体（自分は三人称のとき、ほかの参加者はいつも）
        self._render_players(origin, self.U["utime"].dt)
        self.origin = origin

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
            if c.anchor:
                info += "  ［固定中：機器以外のブロックで E を押すと切り離し］"
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
                surv = not w.creative
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
            if w.creative:
                self.aim_text.text = f"{h['planet'].name} の地面（左クリックで掘る）"
            else:
                res = w.registry.res_name(h["planet"].resource_at(h["point"]))
                self.aim_text.text = f"{h['planet'].name} の地面：{res}（左クリックで掘る）"
        elif h and any(a.aim(h) for a in self.addons):
            self._addon_aim(h)
        elif h and h["kind"] == "water":
            self.highlight.hide()
            size = float(w.settings.get("voxel_size", 0.5))
            g = h["point"] + h["normal"] * size * 0.55
            fwd, right, up, _ = self.look_vectors()
            self._set_box(self.ghost, g - self.origin, np.stack([right, h["normal"], fwd]), np.full(3, size))
            self.ghost.show()
            self.aim_text.text = "水面" + ("" if w.creative else "（左クリックで水をくむ：いくらくんでも減りません）")
        else:
            self.highlight.hide()
            self.ghost.hide()
            self.aim_text.text = ""

    def _addon_aim(self, h):
        """拡張のもの（街の建物・NPC など）を見ている：説明と、置ける面ならブロックの置き場所。"""
        w = self.world
        txt, placeable = next(r for r in (a.aim(h) for a in self.addons) if r)
        self.highlight.hide()
        if placeable:
            size = float(w.settings.get("voxel_size", 0.5))
            g = h["point"] + h["normal"] * size * 0.55
            fwd, right, up, _ = self.look_vectors()
            self._set_box(self.ghost, g - self.origin, np.stack([right, h["normal"], fwd]), np.full(3, size))
            self.ghost.show()
        else:
            self.ghost.hide()
        self.aim_text.text = txt

    def _update_hud(self, dt):
        w = self.world
        p = w.player
        pl = w.nearest_planet(p.pos)
        alt = float(pl.sdf(p.pos[None])[0]) if pl else 0
        spd = float(np.linalg.norm(p.vel))
        lines = [f"Alpedia  ワールド: {w.name}  時刻 {w.time:.0f}s",
                 f"{pl.name if pl else '宇宙'}  高度 {alt:.1f} m  速度 {spd:.1f} m/s",
                 f"構築物 {len(w.constructs)}  頂点 {sum(c.n_vert for c in w.constructs.values())}",
                 f"ジェットパック {'ON' if p.jetpack else 'OFF'}{'' if w.creative or not p.jetpack else ('（燃料 %.0f）' % p.inventory.get('fuel', 0.0))}  {'クリエイティブ' if w.creative else 'サバイバル'}{'  完全剛体' if w.settings.get('full_rigid') else ''}  物理:{native.name()}"]
        if self._perf_text:
            lines.append(self._perf_text)
        net = w.net
        if net is not None and net.role == "client":
            lines.append(f"マルチプレイ参加中（ホスト {net.host_name}）  {1 + len(net.others)} 人")
        elif net is not None:
            lines.append(f"LAN に公開中 {'  '.join(net.addresses()[:1])}  {1 + len(w.remote)} 人")
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
        if not w.creative and p.inventory:
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
            cs = "" if w.creative or not cost else "  費用 " + w.registry.res_text(cost, ", ")
            self.sel_text.text = f"{'素材' if kind == 'material' else '機器'}: {name}{extra}{cs}"
        now = _time.time()
        self.msg_text.text = "\n".join(m for t, m in w.msgs if now - t < 8)


def _fmt(v):
    if isinstance(v, float):
        return f"{v:.2f}"
    if isinstance(v, dict):
        return "{" + ", ".join(f"{k}:{_fmt(x)}" for k, x in v.items()) + "}"
    return str(v)
