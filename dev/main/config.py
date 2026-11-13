"""config/settings.json の読み書き。無ければ既定値で作成する。"""
from . import native

DEFAULTS = {
    "window": {"width": 1280, "height": 720, "fullscreen": False, "vsync": True},
    "graphics": {"fov": 80, "terrain_patch_cells": 48, "terrain_patch_step": 1.0, "view_distance": 3000,
                 # 描画の読み込み距離（m）：これより遠い構築物は、それっぽい色の箱で描く（ブロックを読み込まない）。0 = 全部ブロック
                 "load_distance": 400.0,
                 "decal_distance": 30.0,
                 # ブロックの見た目：輪郭の線（セルの境目）・角の暗さ（環境光の遮り）
                 "block_edges": True, "block_ao": True,
                 # 街：この距離（m、街の外周から）より近い街だけ窓・ネオンなどを詳しく描く／これより遠い街は描かない
                 "city_detail_distance": 100.0, "city_view_distance": 2500.0,
                 # 開発用の表示（処理の重さ・頂点数・物理コアなど）。F3 で切り替え
                 "debug_hud": False},
    "controls": {
        "mouse_sensitivity": 60.0,
        "forward": "w", "back": "s", "left": "a", "right": "d",
        "jump": "space", "down": "left control", "jetpack": "f",
        "interact": "e", "rotate": "r", "edit_script": "p", "new_construct": "n",
        "save": "f5", "export": "f9", "import": "f8", "toggle_help": "f1", "inventory": "i", "city_marker": "m", "city_map": "k",
        "third_person": "v", "camera_mode": "c", "story_menu": "j", "debug_hud": "f3",
        # タッチ操作：画面左下の仮想ジョイスティックで移動・画面をなぞって視点（マウスで視点を回さない）
        "touch": False,
    },
    "physics": {"tick_rate": 60, "substeps": 3, "iterations": 2,
                # 軽量化：剛体化・遠方のレール
                "rigid_enabled": True, "rails_enabled": True, "rails_distance": 400.0,
                # 機能処理の距離（m）：どのプレイヤーからもこれより遠い構築物は、機器・プログラム・物理を止める。0 = 全部処理する
                "process_distance": 1500.0,
                # 結合した直後（merge_settle_time 秒）は、めり込みの押し出しを速度にしない（速さの変化を
                # merge_settle_acc m/s² までにする。給油車などをつないだ瞬間に吹き飛ばないように）
                "merge_settle_time": 1.0, "merge_settle_acc": 30.0,
                # 構築物ごとの計算に使うスレッド数（0 = 自動）
                "threads": 0},
    "game": {"default_world": "scenario", "language": "ja",
             # スクリプトを開くエディタのコマンド（空なら自動：VS Code があればそれ、無ければメモ帳）。例 "notepad++.exe"
             "editor": ""},
    # 音量（0〜100 %）：BGM（mods/bgm の曲）と効果音（ブロック・機器・ムービーの効果音）
    "audio": {"bgm": 60, "sfx": 80},
    # マルチプレイ（LAN）：自分の名前（空ならパソコンのユーザー名）、公開するポート
    "multiplayer": {"name": "", "port": 47620},
    "log_level": "INFO",
}


def _merge(base, over):
    return native.require().config_merge(base, over)


def load() -> dict:
    """config/settings.json を読む（無ければ既定値で作る。読み書きは Rust：game/modmeta.rs）。"""
    return native.require().config_load(DEFAULTS)
