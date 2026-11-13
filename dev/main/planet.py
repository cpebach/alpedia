"""惑星：距離場（SDF）で表した地形。掘削（クレーター）で形が変わる。本体は Rust（alpedia_core.Planet）。"""
from . import native

_c = native.require()

fcross = _c.planet_fcross          # np.cross より速い外積（(N,3) と (N,3) / (3,) の組み合わせ）
value_noise = _c.planet_value_noise
fbm = _c.planet_fbm
smoothstep = _c.planet_smoothstep
make_planet = _c.planet_make
Planet = _c.Planet
