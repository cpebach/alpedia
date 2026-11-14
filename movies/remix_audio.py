"""ゲームに入れるムービー（dev/mods/story/movies/*.mp4）の音を、音楽ぬき（効果音と環境音だけ）に作り直す。

    python remix_audio.py              全部
    python remix_audio.py 05_declaration

映像はそのまま（作り直さない）。ゲームではムービーの間、BGM の MOD（mods/bgm）の曲を重ねて流す
（どの曲を流すかは dev/mods/story/code/script.py の BGM）。BGM の音量と効果音の音量は、ゲーム設定でそれぞれ変えられる。
"""
import importlib
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
GAME = HERE.parent / "dev" / "mods" / "story" / "movies"


def remix(name):
    from cinema import sound as S
    from cinema.post import ffmpeg_exe
    S.MUSIC = False
    scene = next(p.stem for p in (HERE / "scenes").glob("m*.py") if p.stem[1:] == name)
    film = importlib.import_module(f"scenes.{scene}").FILM()
    mp4 = GAME / f"{name}.mp4"
    with tempfile.TemporaryDirectory() as td:
        wav = Path(td) / "a.wav"
        try:
            film.audio(wav)
        except Exception:                       # 物を置いてからでないと音を作れない場面
            from cinema.film import make_stage
            film.build(make_stage(film, 0.25))
            film.audio(wav)
        out = Path(td) / "out.mp4"
        subprocess.run([ffmpeg_exe(), "-y", "-loglevel", "error", "-i", str(mp4), "-i", str(wav), "-map", "0:v:0",
                        "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac", "-strict", "-2", "-b:a", "160k", "-shortest",
                        "-movflags", "+faststart", str(out)], check=True)
        mp4.write_bytes(out.read_bytes())
    print("音を作り直しました:", mp4.name)


def main():
    names = sys.argv[1:] or sorted(p.stem for p in GAME.glob("*.mp4"))
    if len(names) == 1:
        remix(names[0])
        return
    for n in names:                  # 物を置く場面は撮影所を作るので、1本ずつ別のプロセスで
        subprocess.run([sys.executable, str(Path(__file__).resolve()), n], check=True)


if __name__ == "__main__":
    main()
