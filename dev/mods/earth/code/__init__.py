"""場所の MOD「地球の名所（Earth）」：主な惑星アースの地形の名所に近づくと、その名前を知らせる。
地形そのものは惑星の定義（planets/earth.json の "features"）で決まり、Rust の地形（features.rs）が作る。"""


def world_addon():
    from .addon import EarthAddon
    return EarthAddon()
