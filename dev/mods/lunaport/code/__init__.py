"""場所の MOD「ルナ・ポート」：月の自由港（景色の箱）。シナリオモードでは第10章・エピローグの舞台。"""


def world_addon():
    from .addon import LunaPortAddon
    return LunaPortAddon()


def client_addon(game):
    from .client import LunaPortClient
    return LunaPortClient(game)
