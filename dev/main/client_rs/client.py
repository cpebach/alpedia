"""Rust の画面のクライアント全体の流れ：タイトルのメニュー（今までの menu.MainMenu を ursina のまねで）→ ワールド → メニュー。
本体は Rust（alpedia_core.RsClient）。

  python run.py --view rust            メニューから
  python run.py --view rust --world 名前   メニューを飛ばして
"""
from .. import native

_c = native.require()
Client = _c.RsClient
run_client = _c.rs_run_client
