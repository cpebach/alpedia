"""起動処理。本体は Rust（alpedia_core の app_*）。"""
from . import native

_c = native.require()

create_empty_world = _c.app_create_empty_world
prepare_world = _c.app_prepare_world
_check_core = _c.app_check_core          # 物理コア（lib/alpedia_core）があるか。無ければ作り方を出して終わる
_rust_view_ok = _c.app_rust_view_ok      # 画面（lib/alpedia_view）が使えるか
main = _c.app_main
run_headless = _c.app_run_headless
