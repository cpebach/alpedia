"""起動時のメニュー画面：ワールドの選択・新規作成・削除・ワールド設定・ゲーム設定。

中身は Rust（native/alpedia_core/src/game/menu.rs）。ここは名前と値を残すだけ。
"""
from .. import native

_c = native.require()

NAME_CHARS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
RESOLUTIONS = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440), (3840, 2160)]
# 構築物の読み込み距離・機能処理の距離（m）。いちばん右の 0 は「全部」（これまで通り、距離に関係なく全部）
LOAD_STEPS = [50.0, 100.0, 200.0, 300.0, 400.0, 600.0, 1000.0, 1500.0, 2500.0, 0.0]
PROCESS_STEPS = [50.0, 100.0, 200.0, 400.0, 800.0, 1500.0, 3000.0, 6000.0, 0.0]
PAGE_SIZE = 6
PLANETS = {"terra": "球体（テラ）", "gaia": "地球型（ガイア）"}
MODES = {"creative": "クリエイティブ", "survival": "サバイバル"}
MODE_NOTES = {"creative": "何でも置ける（資源なし）", "survival": "置くのに資源が要る・目標がある"}
TRASH = ".trash"   # 消しきれなかったワールドの置き場（一覧には出ない。次回起動時に片付ける）

_dist_text = _c.menu_dist_text                      # 距離の表示（0 は「全部」）
# ワールド操作（UI に依存しない部分）
list_worlds = _c.menu_list_worlds                   # saves/ のワールドの一覧（新しい順）
world_mode_text = _c.menu_world_mode_text           # 一覧に出す遊び方
list_mods = _c.menu_list_mods                       # [(MOD名, 表示名)]（要素を足す MOD）
read_world_settings = _c.menu_read_world_settings   # (settings, mods)
write_world_settings = _c.menu_write_world_settings
create_world = _c.menu_create_world                 # create_world(name, settings_patch, mods, planet="terra")
_rmtree_retry = _c.menu_rmtree_retry                # 同期ソフトがつかんでいても、何度かやり直して消す
delete_world = _c.menu_delete_world                 # True=完全に削除 / False=ゴミ箱に移した
purge_trash = _c.menu_purge_trash                   # 前回消しきれなかったワールドを片付ける
save_config = _c.menu_save_config                   # config/settings.json に保存
_default_name = _c.menu_default_name
apply_window_settings = _c.menu_apply_window_settings   # 設定のウィンドウ（解像度・全画面）を反映する
MainMenu = _c.MainMenu                              # 画面：MainMenu(cfg, on_play, on_quit, on_join=None)
