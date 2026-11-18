"""大都市圏 MOD（メガロポリス）：東京やニューヨークのような大都市・大工業地帯と、それをつなぐ複線の線路・時刻表どおりに走る電車。

レールと車輪は本体の機能（main/rail.py）。この MOD の線路にも、プレイヤーの車輪のある乗り物が乗れる。
"""


def world_addon():
    from .addon import MegaAddon
    return MegaAddon()


def client_addon(game):
    from .client import MegaClient
    return MegaClient(game)
