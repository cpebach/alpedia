"""ムービーを描いて mp4 にする。

    python render.py pilot_drama                 本番（1920×1080、30 コマ/秒）→ output/pilot_drama.mp4
    python render.py pilot_drama --scale 0.5     半分の大きさで早く確かめる
    python render.py pilot_drama --still 3,8.5   その時刻のコマを PNG に（output/stills/）
    python render.py pilot_drama --from 5 --to 9 その間だけ

場面は scenes/<名前>.py の FILM。
"""
import argparse
import importlib
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("scene")
    ap.add_argument("--scale", type=float, default=1.0)
    ap.add_argument("--still", default="")
    ap.add_argument("--from", dest="t_from", type=float, default=0.0)
    ap.add_argument("--to", dest="t_to", type=float, default=None)
    ap.add_argument("--no-audio", action="store_true")
    ap.add_argument("--out", default="")
    a = ap.parse_args()
    from cinema import film as F
    mod = importlib.import_module(f"scenes.{a.scene}")
    film = mod.FILM()
    out_dir = HERE / "output"
    out_dir.mkdir(exist_ok=True)
    if a.still:
        sd = out_dir / "stills"
        sd.mkdir(exist_ok=True)
        times = [float(x) for x in a.still.split(",") if x.strip()]
        for p in F.stills(film, times, sd, scale=a.scale if a.scale != 1.0 else 0.5):
            print("書き出し:", p)
        return
    out = Path(a.out) if a.out else out_dir / f"{film.name}.mp4"
    print(f"「{film.name}」を描いています（{film.duration:.1f} 秒）→ {out}")
    T = time.time()
    F.render(film, out, a.scale, a.t_from, a.t_to, audio=not a.no_audio)
    print(f"できました（{time.time() - T:.0f} 秒）: {out}")


if __name__ == "__main__":
    main()
