"""シナリオの乗り物・艦・ロボットの設計図（sauce/blueprints/*.alp.json）を作り直す。

    python tools/make_blueprints.py

シナリオモードのワールドを一時フォルダに作り、舞台の構築物をそのまま設計図にする（プログラムも一緒に入れる）。
設計図は F8 の読み込みメニューの「設計図」に最初から並び、どのワールドでも出せる（必要な MOD があれば）。
"""
import json
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from main import config, log, objio, paths  # noqa: E402
from main.construct import _clean_dev  # noqa: E402
from main.tutorial import create_tutorial  # noqa: E402
from main.world import World  # noqa: E402

# (役, ファイル名, 名前, 説明)
ITEMS = [
    ("ship", "luminous", "ルミナス号", "シナリオの宇宙船（直した姿）。航法プログラムは月の発着場へ向かう。座ると発進"),
    ("bike", "hover_bike", "ホバーバイク", "浮いて走るバイク。座って W/A/S/D、Space で高く"),
    ("drone", "drone", "ドローン", "操縦台から無線で飛ばすドローン（操縦台と一緒に使う）"),
    ("console", "drone_console", "ドローンの操縦台", "座ると押したキーを無線でドローンへ送る"),
    ("crane", "cargo_crane", "搬入クレーン", "扉と旋回クレーン。「待つ」ブロックで手順を動かす練習に"),
    ("factory", "factory", "工場", "精錬機・加工機・燃料のバルブとタンク（電気は太陽光とバッテリー）"),
    ("interceptor", "interceptor", "連盟の迎撃艦", "迎撃砲塔4つとレーダー。迎撃のブロックプログラムつき"),
    ("robot", "robot_towa", "トワのロボット", "ホバーバイクと同じ浮き方の人型ロボット。腕はアクチュエーター"),
    ("krobot", "robot_kingi", "キンギのロボット", "黒と赤のロボット。肩にミサイル発射機"),
    ("unit", "reaccel_unit", "再加速の作業台", "推進モジュール4基の架台（モジュール入り）。バルブで推力をそろえる"),
    ("cart", "fuel_cart", "給油車", "燃料タンク4つと燃料ポンプ。配管でつないで E でポンプを動かすと、つながったタンクへ燃料を送る"),
    ("booth", "glass_booth", "ガラスの管制室", "ガラス張りの小部屋（宇宙で人が入る所）"),
    ("rocket", "depart_rocket", "旅立ちのロケット", "カプセルつきのロケット。左右のブースターはアクチュエーターで分離できる"),
]
NEEDS = {"scenario_blocks"}


def main():
    log.setup("WARNING")
    tmp = Path(tempfile.mkdtemp(prefix="alpedia_bp_"))
    saves = paths.SAVES
    paths.SAVES = tmp
    try:
        create_tutorial("bp")
        w = World("bp", json.loads(json.dumps(config.DEFAULTS))).load()
        s = w.addon("story")
        s._rebuild_ship()
        s._place_cart()
        s._place_rocket()
        import importlib
        space = importlib.import_module("alpedia_mods.story.space")
        # トワのロボット（人型）：2026-11-09 からシナリオでは第12章で変形ロボット ツバメ（make_starter_blueprints.py）を
        # 出すようになり、始めのワールドには置かれない。設計図としては残すので、前と同じ形・置き方でここで作る
        if s.con("robot") is None and s.deck is not None:
            rx, rz = space.ROBOT_START
            rv = space.robot_voxels()
            s._put_space("robot", "トワのロボット", rv, s.deck.point(rx, rz, 0.3), s.deck.frame(0.0), (1.5, 0.0, 2.5),
                         links=space.robot_links(rv))
        out = paths.SAUCE / "blueprints"
        out.mkdir(parents=True, exist_ok=True)
        sd = w.dir / "scripts"
        for role, fname, name, desc in ITEMS:
            c = s.con(role)
            if c is None:
                raise SystemExit(f"シナリオのワールドに「{name}」（役 {role}）がありません。mods/story の舞台の作り方が変わったら、"
                                 "この ITEMS も合わせて直してください")
            vox =[[cl[0], cl[1], cl[2], m, _clean_dev(d, c.reg)] for cl, m, d in zip(c.cells, c.mats, c.devs)]
            if role == "unit":
                for cell in space.UNIT_SLOTS:
                    vox.append([cell[0], cell[1], cell[2], "steel", {"def": "rocket_module", "dir": 2,
                                                                       "name": "推進モジュール", "state": {}}])
            if role == "interceptor":       # 迎撃のプログラムは完成した形で
                prog = importlib.import_module("alpedia_mods.story.programs").intercept_solution()
                from main import blocks as B
                B.save(sd / "intercept.blocks.json", prog)
                (sd / "intercept.py").write_text(B.to_python(prog, "intercept.blocks.json"), encoding="utf-8")
            scripts = {}
            for f in objio._script_files([v[4] for v in vox]):
                if (sd / f).exists():
                    scripts[f] = (sd / f).read_text(encoding="utf-8")
                    if f.endswith(".blocks.json") and (sd / (f[:-12] + ".py")).exists():
                        scripts[f[:-12] + ".py"] = (sd / (f[:-12] + ".py")).read_text(encoding="utf-8")
            mats = {v[3] for v in vox}
            devs = {v[4]["def"] for v in vox if v[4]}
            req = sorted(NEEDS) if (mats | devs) & _mod_ids(w) else []
            meta = {"format": "alpedia-construct", "version": 1, "name": name, "description": desc,
                    "requires": req, "voxel_size": c.size, "scale": 1.0, "default_material": "steel",
                    "voxels": vox, "links": None, "storage": {}, "scripts": scripts}
            if role == "robot":             # コックピットの扉は胴とつながず、アクチュエーターでだけつなぐ
                meta["links"] = [[list(a), list(b)] for a, b in sorted(c.links)]
                for v in vox:
                    if v[4] and v[4]["def"] == "computer":
                        v[4].setdefault("state", {})["running"] = True
            if meta["links"] is None:
                del meta["links"]
            (out / f"{fname}.alp.json").write_text(json.dumps(meta, ensure_ascii=False), encoding="utf-8")
            print(f"{fname}.alp.json  {name}  ボクセル {len(vox)}  必要な MOD {req}")
    finally:
        paths.SAVES = saves
        shutil.rmtree(tmp, ignore_errors=True)


def _mod_ids(w):
    """シナリオ再現ブロックの MOD の素材・機器の ID。"""
    reg = w.registry
    return {k for cat in ("materials", "devices") for k, v in reg.defs[cat].items() if v.get("_source") == "scenario_blocks"}


if __name__ == "__main__":
    main()
