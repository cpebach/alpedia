"""config/settings.json の読み書き。無ければ既定値で作成する。"""
import copy
import json

from . import paths, log

DEFAULTS = {
    "window": {"width": 1280, "height": 720, "fullscreen": False, "vsync": True},
    "graphics": {"fov": 80, "terrain_patch_cells": 48, "terrain_patch_step": 1.0, "view_distance": 3000,
                 "decal_distance": 30.0,
                 # ブロックの見た目：輪郭の線（セルの境目）・角の暗さ（環境光の遮り）
                 "block_edges": True, "block_ao": True,
                 # 街：この距離（m、街の外周から）より近い街だけ窓・ネオンなどを詳しく描く／これより遠い街は描かない
                 "city_detail_distance": 100.0, "city_view_distance": 2500.0},
    "controls": {
        "mouse_sensitivity": 60.0,
        "forward": "w", "back": "s", "left": "a", "right": "d",
        "jump": "space", "down": "left control", "jetpack": "f",
        "interact": "e", "rotate": "r", "edit_script": "p", "new_construct": "n",
        "save": "f5", "export": "f9", "import": "f8", "toggle_help": "f1", "inventory": "i", "city_marker": "m", "city_map": "k",
        "third_person": "v", "camera_mode": "c",
        # タッチ操作：画面左下の仮想ジョイスティックで移動・画面をなぞって視点（マウスで視点を回さない）
        "touch": False,
    },
    "physics": {"tick_rate": 60, "substeps": 3, "iterations": 2,
                # 軽量化：剛体化・遠方のレール
                "rigid_enabled": True, "rails_enabled": True, "rails_distance": 400.0,
                # 構築物ごとの計算に使うスレッド数（0 = 自動）
                "threads": 0,
                # 物理コア："auto"（Rust の alpedia_core があれば使う）/ "rust"（必須）/ "python"（numba）
                "backend": "auto"},
    "game": {"default_world": "tutorial", "language": "ja",
             # スクリプトを開くエディタのコマンド（空なら自動：VS Code があればそれ、無ければメモ帳）。例 "notepad++.exe"
             "editor": ""},
    # マルチプレイ（LAN）：自分の名前（空ならパソコンのユーザー名）、公開するポート
    "multiplayer": {"name": "", "port": 47620},
    "log_level": "INFO",
}


def _merge(base, over):
    out = copy.deepcopy(base)
    for k, v in over.items():
        if isinstance(v, dict) and isinstance(out.get(k), dict):
            out[k] = _merge(out[k], v)
        else:
            out[k] = v
    return out


def load() -> dict:
    p = paths.CONFIG / "settings.json"
    paths.CONFIG.mkdir(parents=True, exist_ok=True)
    data = {}
    if p.exists():
        try:
            data = json.loads(p.read_text(encoding="utf-8"))
        except Exception as e:  # noqa: BLE001
            log.get("config").error("settings.json の読み込みに失敗: %s（既定値を使用）", e)
    cfg = _merge(DEFAULTS, data)
    if not p.exists():
        p.write_text(json.dumps(cfg, indent=2, ensure_ascii=False), encoding="utf-8")
    return cfg
