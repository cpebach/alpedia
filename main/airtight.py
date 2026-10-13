"""気密判定：ボクセルで完全に囲まれた空洞を探す。"""
import numpy as np
from scipy import ndimage

AIR_DENSITY_1ATM = 1.2   # kg/m^3
ATM_KPA = 101.3


def find_cavities(cells):
    """cells: [(i,j,k)] → [set(空洞セル)]（外部と繋がっていない空のセル群）"""
    if len(cells) < 6:
        return []
    arr = np.asarray(cells)
    lo = arr.min(0) - 1
    hi = arr.max(0) + 1
    shape = tuple(hi - lo + 1)
    if np.prod(shape) > 4_000_000:
        return []  # 大きすぎる場合は省略
    solid = np.zeros(shape, bool)
    solid[tuple((arr - lo).T)] = True
    lab, n = ndimage.label(~solid)  # 6近傍
    if n <= 1:
        return []
    border = set(np.unique(np.concatenate([
        lab[0].ravel(), lab[-1].ravel(), lab[:, 0].ravel(), lab[:, -1].ravel(),
        lab[:, :, 0].ravel(), lab[:, :, -1].ravel()])))
    out = []
    for k in range(1, n + 1):
        if k in border:
            continue
        idx = np.argwhere(lab == k) + lo
        out.append({tuple(int(x) for x in p) for p in idx})
    return out


def pressure_kpa(air_kg, volume_m3):
    if volume_m3 <= 0:
        return 0.0
    return air_kg / volume_m3 / AIR_DENSITY_1ATM * ATM_KPA


def air_for_pressure(kpa, volume_m3):
    return kpa / ATM_KPA * AIR_DENSITY_1ATM * volume_m3
