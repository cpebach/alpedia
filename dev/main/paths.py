"""ゲーム本体のディレクトリ構成。

<ゲームフォルダ>/
    main/    本体コード
    saves/   セーブデータ（ワールドごとのディレクトリ）
    mods/    MOD
    sauce/   本体のテクスチャ・基本定義
    logs/    ログ
    config/  設定
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAIN = ROOT / "main"
SAVES = ROOT / "saves"
MODS = ROOT / "mods"
SAUCE = ROOT / "sauce"
LOGS = ROOT / "logs"
CONFIG = ROOT / "config"

BASE_DEFS = SAUCE / "base"
TEXTURES = SAUCE / "textures"


def ensure_dirs():
    for d in (SAVES, MODS, SAUCE, LOGS, CONFIG):
        d.mkdir(parents=True, exist_ok=True)


def world_dir(name: str) -> Path:
    return SAVES / name
