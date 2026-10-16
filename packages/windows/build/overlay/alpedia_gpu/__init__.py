"""Alpedia パッケージ版の GPU 高速化。

ゲーム本体（game/main）には手を入れず、起動時（boot.py）に描画クラスの一部を差し替える。

- 柔らかい構築物（毎フレーム形が変わるもの）のスキニング・法線・陰影を GPU で計算する。
  CPU は節点の位置（ボクセル数 × 8 点）を送るだけになり、頂点ごとの計算と頂点データの書き換えがなくなる。
    - GL 4.3 以上：コンピュートシェーダー（shaders/skin.comp）でスキニング → 頂点シェーダーで法線
    - GL 3.3     ：頂点シェーダーだけでスキニングと法線
    - それ未満・失敗時：元の CPU（numba）計算のまま
- 設定：config/settings.json の graphics.gpu = "auto"（既定）/ "compute" / "vertex" / "off"
"""
from .hook import install, status  # noqa: F401
