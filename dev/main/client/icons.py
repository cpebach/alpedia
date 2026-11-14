"""アイテム（素材・機器）のアイコン画像を作る。本体は Rust（alpedia_core.icons_*）。

ブロックは球の形なので、アイコンも陰影付きの球にする。
素材はその色とテクスチャ、機器は機器の色に種類ごとのマーク（炎・タンクの帯・画面など）を重ねる。
画像は起動中だけメモリに置く（ファイルは作らない）。
"""
import functools

import numpy as np

from .. import native

LIGHT = np.array([-0.45, 0.6, 0.66])
LIGHT /= np.linalg.norm(LIGHT)
_img_cache = {}
_tex_cache = {}
_src_cache = {}
_ui_cache = {}

# 機器ブロックの表面に貼る記号（デカール）
DECAL_TYPES = ["thruster", "tank", "pipe", "valve", "battery", "generator", "solar", "crank", "computer", "actuator",
               "processor", "seat", "air_vent", "antenna", "turret", "radar", "launcher", "pump", "hinge", "bend", "piston",
               "anchor", "light", "other",
               # 面ごとの印（推進機・アクチュエーターの、向きの面と反対の面）
               "nozzle", "thrust_face", "shaft", "rod", "hinge_face", "mount"]
DECAL_VARIANTS = ["plain", "side", "front", "back"]
DECAL_CELL = 128
_atlas = None

_c = native.require()
_texture_array = _c.icons_texture_array       # 画像ファイル → (高さ, 幅, 3) の 0〜1（キャッシュ）
sphere = _c.icons_sphere                      # 陰影付きの球（RGBA の PIL 画像）
_mark = _c.icons_mark                         # 機器の種類ごとのマーク
item_image = _c.icons_item_image              # アイテムのアイコン（PIL 画像）
item_texture = _c.icons_item_texture          # アイテムのアイコンのテクスチャ（キャッシュ）
item_name = _c.icons_item_name                # アイテムの名前
panda_texture = _c.icons_panda_texture        # PIL 画像 → 画面（Rust）のテクスチャ（名前は昔の名残り）
_decal_image = _c.icons_decal_image           # 記号1マス
decal_atlas = _c.icons_decal_atlas            # (PIL 画像, {(種類, 変化): (u0, v0, u1, v1)})
ui_image = _c.icons_ui_image                  # 画面のボタン（タッチ操作）のアイコン：丸い下地に白い絵
ui_texture = _c.icons_ui_texture
_to_texture = functools.partial(panda_texture, name="icon")
