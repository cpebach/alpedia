"""登場人物の MOD：シナリオ「それでも」の13人の見た目（players/cast.json）と、人物（NPC）を立たせる仕組み。

ほかの MOD は world.addon("cast") の place / walk_to / remove / clear で人物を立たせ、
話しかけられたら（E）各 MOD の on_npc_talk(world, key) が呼ばれる。描くのは VRM の見た目 MOD（skinmodel）の体。
"""


def world_addon():
    from .addon import CastAddon
    return CastAddon()


def client_addon(game):
    from .client import CastClient
    return CastClient(game)
