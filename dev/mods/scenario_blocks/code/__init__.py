"""シナリオ再現ブロック：迎撃砲塔・レーダー・ミサイル発射機（MOD の機器 "type": "custom"）と、そのブロックプログラム。

    devices/scenario.json   機器の定義（推進モジュールはふつうの推進機の大きいもの）
    blocks.py               スクリプト・ブロックプログラムの口（読み込むときに登録）
    addon.py                ミサイル・砲塔のビーム・爆発の動き（ワールド側）
    client.py               ミサイル・ビーム・爆発を描く（画面側）
ほかの MOD は world.addon("weapons") の spawn_missile / protected を使える。
"""
from . import blocks  # noqa: F401（読み込むときに口を登録する）


def world_addon():
    from .addon import WeaponsAddon
    return WeaponsAddon()


def client_addon(game):
    from .client import WeaponsClient
    return WeaponsClient(game)
