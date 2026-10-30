"""場所の MOD「軌道都市アマツ」：テラの上空の円筒の街（景色の箱）。シナリオモードでは第11章〜終章の舞台。"""


def world_addon():
    from .addon import AmatsuAddon
    return AmatsuAddon()


def client_addon(game):
    from .client import AmatsuClient
    return AmatsuClient(game)
