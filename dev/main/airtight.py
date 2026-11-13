"""気密判定：ボクセルで完全に囲まれた空洞を探す（Rust：alpedia_core の game/airtight.rs）。"""
from . import native

AIR_DENSITY_1ATM = 1.2   # kg/m^3
ATM_KPA = 101.3


def find_cavities(cells):
    """cells: [(i,j,k)] → [set(空洞セル)]（外部と繋がっていない空のセル群）"""
    return native.require().airtight_find_cavities([(int(c[0]), int(c[1]), int(c[2])) for c in cells])


def pressure_kpa(air_kg, volume_m3):
    return native.require().airtight_pressure_kpa(float(air_kg), float(volume_m3))


def air_for_pressure(kpa, volume_m3):
    return native.require().airtight_air_for_pressure(float(kpa), float(volume_m3))
