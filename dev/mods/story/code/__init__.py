"""ストーリー MOD：「墜落した技術者」のチュートリアル（台本は script.py、舞台は sites.py）。"""


def world_addon():
    from .addon import StoryAddon
    return StoryAddon()


def client_addon(game):
    from .client import StoryClient
    return StoryClient(game)
