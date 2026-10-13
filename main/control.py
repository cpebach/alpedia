"""構築物を外から読み書きする窓口（スクリプト・ブロックプログラム用）。

スクリプトの api（scripting.py）は構築物の中の配列（頂点の位置・機器のリストなど）を直接触らず、
必ずこの Port を通す。Rust などに物理を移すときは、この Port の関数が境目（呼び出し口）になる。

- 読み取りは値のコピー（リスト・辞書・数値）を返す
- 書き込みは決まった項目だけ（WRITABLE）。値は検査してから入れる
- 機器は番号（構築物の中のブロックの番号）で指す。分離などで番号が変わることがあるので、
  機器の番号は毎ティック devices() で取り直す
"""
import numpy as np

from . import actuator, devices, devstate
from .construct import DIR_NAMES

# スクリプトから書き換えてよい機器の状態：{項目: 値を検査して直す関数}
WRITABLE = {
    "throttle": lambda v: float(min(max(float(v), 0.0), 1.0)),
    "open": bool,
    "enabled": bool,
    "running": bool,
}


class PortError(ValueError):
    pass


class Port:
    def __init__(self, world, c):
        self._w = world
        self._c = c
        self._pose = None

    # ------------------------------------------------------------------
    # 構築物全体
    # ------------------------------------------------------------------
    @property
    def time(self):
        return float(self._w.time)

    @property
    def name(self):
        return self._c.name

    def pose(self):
        """(重心 [3], 回転行列 R（列がローカル軸のワールド方向）[3x3], 速度 [3], 角速度 [3])。同じティック中は使い回す。"""
        if self._pose is None:
            c = self._c
            R, _ = c.rigid_fit()
            self._pose = (c.center_of_mass(), R, c.velocity(), c.angular_velocity())
        return self._pose

    def mass(self):
        return float(self._c.total_mass())

    def power(self):
        return dict(self._c.status)

    def storage(self):
        return dict(self._c.storage)

    def cavities(self):
        return [{"cells": n, "pressure_kpa": p} for _, n, p in devices.cavity_report(self._c)]

    def input_keys(self):
        """操縦席に座っている間に押されているキー（座っていなければ空）。"""
        return set(self._w.input_keys) if self._w.seated_in(self._c) else set()

    # ------------------------------------------------------------------
    # 惑星
    # ------------------------------------------------------------------
    def _planet(self):
        return self._w.nearest_planet(self.pose()[0])

    def up(self):
        p = self.pose()[0]
        pl = self._planet()
        return pl.up(p) if pl is not None else np.array([0.0, 1.0, 0.0])

    def altitude(self):
        pl = self._planet()
        return float(pl.sdf(self.pose()[0][None])[0]) if pl is not None else float("inf")

    def planet(self):
        pl = self._planet()
        if pl is None:
            return None
        k = float(self._w.settings.get("gravity_scale", 1.0))
        return {"name": pl.name, "center": pl.center.tolist(), "radius": pl.radius, "mu": pl.gm * k,
                "surface_gravity": pl.gm * k / pl.radius ** 2}

    def orbit(self):
        com, _, v, _ = self.pose()
        return self._w.orbit_info(com, v)

    # ------------------------------------------------------------------
    # 機器
    # ------------------------------------------------------------------
    def _dev(self, i):
        devs = self._c.devs
        if not (0 <= i < len(devs)) or not devs[i]:
            raise PortError(f"機器 {i} がありません")
        return devs[i]

    def devices(self):
        """[(番号, 種類ID, 種類, 名前, 向きの名前)]"""
        out = []
        for i, d in enumerate(self._c.devs):
            if d:
                dd = self._c.reg.device(d["def"])
                out.append((i, d["def"], dd["type"] if dd else "?", d["name"], DIR_NAMES[d["dir"]]))
        return out

    def devices_name(self, i):
        return self._dev(i)["name"]

    def device_state(self, i):
        """機器の状態のコピー（"_" で始まる内部用の項目は除く）。"""
        return {k: v for k, v in self._dev(i)["state"].items() if not k.startswith("_")}

    def device_get(self, i, key, default=None):
        v = self._dev(i)["state"].get(key, default)
        return dict(v) if isinstance(v, dict) else v

    def device_set(self, i, key, value):
        """決まった項目だけ書き換える。"""
        if key not in WRITABLE:
            raise PortError(f"'{key}' はスクリプトから書き換えられません")
        d = self._dev(i)
        dd = self._c.reg.device(d["def"])
        v = WRITABLE[key](value)
        if dd and key not in devstate.fields(dd["type"]):
            return                        # その機器には無い項目（例：推進機の enabled）：何もしない
        if key == "open" and v != d["state"].get("open", True):
            self._c.dirty = True          # 配管のつながりが変わる
        d["state"][key] = v

    def device_cell(self, i):
        self._dev(i)
        return tuple(self._c.cells[i])

    def device_dir(self, i):
        return int(self._dev(i)["dir"])

    def device_offset(self, i):
        """構築物の重心から見た、ローカル座標での機器の位置 [x,y,z] (m)。"""
        self._dev(i)
        com, R, _, _ = self.pose()
        p = self._c.voxel_center(i)
        return (R.T @ (p - com)).tolist()

    def set_display(self, i, payload):
        """表面の文字（payload=None で消す）。payload は scripting.display_payload が作る辞書。"""
        st = self._dev(i)["state"]
        if payload is None:
            st.pop("display", None)
        else:
            st["display"] = payload

    def comp_state(self, i):
        """コンピューター自身の状態（スクリプトの memory・無線の受信位置など）。コンピューターの番号だけ。"""
        d = self._dev(i)
        dd = self._c.reg.device(d["def"])
        if not dd or dd["type"] != "computer":
            raise PortError("コンピューターではありません")
        return d["state"]

    # --- アクチュエーター ---------------------------------------------
    def _act(self, i):
        d = self._dev(i)
        dd = self._c.reg.device(d["def"])
        if not dd or dd["type"] != "actuator":
            raise PortError(f"{d['name']} はアクチュエーターではありません")
        actuator.init_state(d["state"], dd["params"])
        return d

    def act_rotate(self, i, angle, speed=None, absolute=False):
        self._act(i)
        actuator.cmd_rotate(self._c, i, angle, speed, absolute=absolute)

    def act_extend(self, i, amount, speed=None, absolute=False):
        self._act(i)
        actuator.cmd_extend(self._c, i, amount, speed, absolute=absolute)

    def act_retract(self, i, amount, speed=None):
        self._act(i)
        actuator.cmd_retract(self._c, i, amount, speed)

    def act_detach(self, i):
        self._act(i)
        return self._w.actuator_detach(self._c, i)

    def act_attach(self, i):
        self._act(i)
        return actuator.cmd_attach(self._w, self._c, i)

    def act_execute(self, i):
        self._act(i)
        return actuator.execute_preset(self._w, self._c, i)

    def act_attached(self, i):
        self._act(i)
        return actuator.is_attached(self._c, i)

    def act_output_angle(self, i):
        self._act(i)
        return actuator.output_angle(self._c, i, self._w)

    def act_busy(self, i):
        return actuator.busy(self._act(i)["state"])

    # ------------------------------------------------------------------
    # 無線
    # ------------------------------------------------------------------
    def antenna(self):
        """(届く距離, 位置) か None"""
        from .radio import antenna_info
        return antenna_info(self._c)

    def radio_send(self, channel, data):
        return self._w.radio.send(self._c, channel, data)

    def radio_receive(self, comp_i, channel):
        return self._w.radio.receive(self._c, self.comp_state(comp_i), channel)

    def radio_latest(self, channel, default=None, max_age=1.0):
        return self._w.radio.latest(self._c, channel, default, max_age)

    # ------------------------------------------------------------------
    def message(self, text):
        self._w.message(text)
