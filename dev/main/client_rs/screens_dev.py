"""Rust の画面：機器の設定（P）。本体は Rust（alpedia_core.DevicePanel / ActuatorPanel）。"""
from .. import native

_c = native.require()
GREY = (0.75, 0.75, 0.75, 1)
BLUE = (0.22, 0.32, 0.45, 1)
DevicePanel = _c.DevicePanel
ActuatorPanel = _c.ActuatorPanel
