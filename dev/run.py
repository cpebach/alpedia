"""Alpedia 起動スクリプト。

  python run.py                     メニュー画面から起動
  python run.py --world 名前         メニューを飛ばして指定ワールドで起動（無ければ新規作成）
  python run.py --headless --steps 600   描画なしでシミュレーションだけ実行（テスト用）
"""
import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from main import app  # noqa: E402

if __name__ == "__main__":
    ap = argparse.ArgumentParser(description="Alpedia")
    ap.add_argument("--world", default=None)
    ap.add_argument("--headless", action="store_true")
    ap.add_argument("--steps", type=int, default=600)
    ap.add_argument("--new", action="store_true", help="空のワールドを新規作成")
    args = ap.parse_args()
    app.main(args)
