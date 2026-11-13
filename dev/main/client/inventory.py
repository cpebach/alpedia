"""アイテムバー（画面下の9枠）とインベントリの中身（アイテムの並べ方・説明）。
画面は main/client_rs/screens.py の Inventory。処理は Rust（alpedia_core.inv_*）。
"""
from .. import log, native

L = log.get("inventory")

SLOTS = 9
DEFAULT_HOTBAR = [("material", "steel"), ("material", "aluminum"), ("material", "wood"), ("material", "glass"),
                  ("material", "rock"), ("device", "thruster"), ("device", "tank"), ("device", "pipe"),
                  ("device", "computer")]
TYPE_NAMES = {
    "thruster": "推進機", "tank": "タンク", "pipe": "配管", "valve": "バルブ", "battery": "バッテリー",
    "generator": "発電機", "solar": "太陽光パネル", "crank": "手回し発電機", "computer": "コンピューター", "actuator": "アクチュエーター",
    "processor": "精錬機・加工機", "seat": "操縦席", "air_vent": "通気口", "antenna": "アンテナ",
    "anchor": "アンカー（構築物を惑星に固定）", "light": "ライト（電力で光る）",
}


_c = native.require()
valid_item = _c.inv_valid_item
ensure_hotbar = _c.inv_ensure_hotbar          # アイテムバーを9枠にそろえ、無くなったアイテム（MOD を外した等）を空にする
item_details = _c.inv_item_details
