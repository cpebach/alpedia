"""素材・機器・惑星・プレイヤー・資源の定義を読み込むレジストリ（Rust：alpedia_core の game/registry.rs）。

本体の基本定義は sauce/base/<カテゴリ>/*.json、
MOD は mods/<MOD名>/<カテゴリ>/*.json に同じ形式で置く。
定義に "extends": "<既存ID>" を書くと、その定義を引き継いで差分だけ上書きできる。
"""
from . import native

CATEGORIES = ("resources", "materials", "devices", "planets", "players")
REQUIRED_TYPES = {
    "thruster", "tank", "pipe", "valve", "battery", "generator", "crank", "computer",
    "actuator", "processor", "seat", "air_vent", "antenna", "anchor", "light", "custom",
}

Registry = native.require().Registry
DEFAULTS = native.require().registry_defaults()      # カテゴリごとの既定値（足りない項目はここから補う）


def _deep_merge(base, over):
    return native.require().registry_deep_merge(base, over)
