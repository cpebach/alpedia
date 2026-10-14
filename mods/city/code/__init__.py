"""サイバーパンクの街 MOD：地表と軌道上の街・暮らしと経済（citylife）・政治（politics）と、その画面（ui, views, client）。

取引・部屋の売買や賃貸は、サバイバル（資源）MOD も入っているときだけ使える。
"""


def world_addon():
    from .addon import CityAddon
    return CityAddon()


def client_addon(game):
    from .client import CityClient
    return CityClient(game)
