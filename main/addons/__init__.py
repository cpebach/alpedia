"""ゲームに入っている拡張（アドオン）。コア（main/ 直下の物理・ワールド）はここを読み込まない。

install() で World.default_addons に登録する（起動処理 app.py が呼ぶ）。
各拡張は hooks.Addon（ワールド側）と、必要なら client_hooks.ClientAddon（画面側）を持つ。
"""

# (名前, ワールド側を作る関数を返す, 画面側のクラスを返す)
def _city_world():
    from .city.addon import CityAddon
    return CityAddon


def _city_client():
    from .city.client import CityClient
    return CityClient


PACKS = [("city", _city_world, _city_client)]


def install():
    """ワールドを作るときに使う拡張を登録する（何度呼んでもよい）。"""
    from ..world import World
    names = {getattr(f, "addon_name", None) for f in World.default_addons}
    for name, world_side, _ in PACKS:
        if name in names:
            continue
        cls = world_side()
        cls.addon_name = name
        World.default_addons.append(cls)


def client_addons(game):
    """画面側の拡張を、ワールドに入っている拡張に合わせて作る。"""
    out = []
    for name, _, client_side in PACKS:
        if game.world.addon(name) is not None:
            out.append(client_side()(game))
    return out
