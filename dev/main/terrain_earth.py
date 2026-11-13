"""地球型の惑星：山・丘・海・池（水たまり）・洞窟のある地形。本体は Rust（alpedia_core.EarthPlanet）。

- 地表は高さ場（方向 → 半径）。大陸のノイズ＋丘＋尾根状の山脈。
- 海面より低い所は海、ところどころに平らな水面の池がある（水は浮力・抵抗がはたらく）。
- 洞窟は「芋虫」のように地中を進むカプセルの列で、地形の距離場からくり抜く。
- 出現地点（+Y 方向）の周りは平らな陸地にし、近くに池・洞窟の入口・山を必ず置く。
すべて seed から決まるので、保存するのは掘削跡だけでよい。
"""
import numpy as np

from . import native

_c = native.require()

SPAWN_DIR = np.array([0.0, 1.0, 0.0])
_tangent_frame = _c.earth_tangent_frame
_fib_sphere = _c.earth_fib_sphere
_seg_dist = _c.earth_seg_dist
EarthPlanet = _c.EarthPlanet
