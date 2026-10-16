"""ゲーム同梱の文章とサンプル（スクリプトの説明 README・ホバー制御のサンプル）。チュートリアルなどが使う。"""
import numpy as np

HOVER_SCRIPT = '''"""ホバー制御スクリプト（サンプル）

コンピューターを E で起動すると、高度 target m で浮くように推進機を調整します。
四隅の推進機の出力差で姿勢（傾き）も保ちます。
操縦席に座っている間は W/S で目標高度、矢印キーで傾けて移動できます。
api の使い方は scripts/README.txt を参照。
"""


def tick(api):
    target = api.memory.get("target", 12.0)
    keys = api.input
    if "w" in keys:
        target += 0.2
    if "s" in keys:
        target = max(2.0, target - 0.2)
    api.memory["target"] = target

    # 高度：重力を打ち消す基本スロットル + PD 制御
    err = target - api.altitude
    vs = api.vertical_speed
    base = 0.5 + err * 0.03 - vs * 0.2

    # 姿勢：ローカルY軸を惑星の上方向へ向ける
    ux, uy, uz = api.up_local
    wx, wy, wz = api.angular_velocity_local
    # 矢印キーで少し傾ける（その方向へ移動）
    lean_x = (0.15 if "right arrow" in keys else 0) - (0.15 if "left arrow" in keys else 0)
    lean_z = (0.15 if "up arrow" in keys else 0) - (0.15 if "down arrow" in keys else 0)
    # キーを押していない時は横方向の速度を打ち消すように傾ける
    vx, vy, vz = api.velocity_local
    if lean_x == 0:
        lean_x = max(-0.1, min(0.1, -vx * 0.04))
    if lean_z == 0:
        lean_z = max(-0.1, min(0.1, -vz * 0.04))
    ex, ez = ux + lean_x, uz + lean_z
    # 重心の偏りを打ち消す積分項
    ix = api.memory.get("ix", 0.0) + ex * 0.05
    iz = api.memory.get("iz", 0.0) + ez * 0.05
    ix, iz = max(-1, min(1, ix)), max(-1, min(1, iz))
    api.memory["ix"], api.memory["iz"] = ix, iz
    tx = (ez * 2.0 + iz * 1.0) - wx * 1.2
    tz = -(ex * 2.0 + ix * 1.0) - wz * 1.2

    thrusters = api.devices("thruster")
    for t in thrusters:
        ox, oy, oz = t.offset
        adj = -oz * tx + ox * tz
        t.throttle = min(max(base + adj * 0.4, 0.0), 1.0)

    n = api.memory.get("n", 0) + 1
    api.memory["n"] = n
    if n % 60 == 0:
        print(f"高度 {api.altitude:.1f} m / 目標 {target:.1f} m / 傾き {(1 - uy) * 100:.1f}%")
'''

SCRIPT_README = '''ゲーム内 Python スクリプト
========================

ここ（saves/<ワールド>/scripts/）に .py を置き、コンピューター機器の state.script に
ファイル名を設定すると、その構築物を制御できます（E キーで起動/停止）。
ゲーム内ではコンピューターを見て P キーを押すと、ファイルの選択・実行・停止と「エディタで開く」ができます。
編集はメモ帳や VS Code などのエディタで行い、保存するとゲームが自動で読み込み直します（実行中でも）。
コンピューターは電力を使います。電力が無いと停止します。

スクリプトには tick(api) 関数を書きます。一定間隔（既定 0.05 秒）で呼ばれます。
import できるのは math / random / statistics のみです。print() はゲーム画面とログに出ます。

api で使えるもの
----------------
api.time              ワールド時刻（秒）
api.position          重心位置 [x,y,z]
api.velocity          速度 [x,y,z]
api.speed             速さ
api.altitude          最寄り惑星の地表からの高度
api.vertical_speed    上下方向の速度
api.up                上方向ベクトル
api.up_local          上方向（構築物ローカル座標。直立なら [0,1,0]）
api.angular_velocity_local  角速度（ローカル座標）
api.velocity_local    速度（ローカル座標）
api.mass              質量 kg
api.power             {"production", "demand", "stored", "capacity", "ratio"}
api.storage           構築物の固形資源
api.input             操縦席に座っている時に押されているキーの集合
api.memory            スクリプト用の保存領域（dict、セーブされる）
api.orientation()     回転行列
api.cavities()        気密空洞の一覧と気圧
api.devices(type=None, name=None)   機器の一覧（type 例: "thruster"）
api.device(name)      名前で機器を取得
api.log(...)          メッセージ表示
api.display(text, face=None, width=None, color=None, background=None)
                      このコンピューターの表面に文字を表示（停止しても残るので看板になる）
                      face: "+X" "-X" "+Y" "-Y" "+Z" "-Z" "all"（省略時はブロックの向き）
                      width: 表示板の幅 m（省略時はブロックの大きさ）
                      color / background: (r,g,b) 0〜1 または 0〜255
api.clear_display()   表示を消す
api.stop()            このコンピューターを停止
api.planet()          最寄り惑星 {"name","center","radius","mu","surface_gravity"}
api.orbit()           軌道 {"altitude","apoapsis","periapsis","eccentricity","period",
                      "horizontal_speed","vertical_speed","orbital_speed","time_to_apoapsis"}

無線通信（構築物にアンテナが必要。アンテナが無い・電力が無い・OFF のときは使うとエラー）
------------------------------------------------------------------------------------
api.send(channel, data)     チャンネルに情報を送る。data は数値・文字・True/False/None と、
                            それらのリスト・辞書（4096 文字まで）。送ったものは 2 秒間届く
api.receive(channel)        前回呼んだ後に届いた情報のリスト（古い順）。各要素は
                            {"data": 情報, "from": 送った構築物の名前, "distance": 距離m, "age": 秒}
api.latest(channel, default=None, max_age=1.0)
                            届いている一番新しい情報（max_age 秒より古ければ default）。消えない
api.antenna                 アンテナが使えるか（True/False）
api.antenna_range           届く距離 m（送る側と受ける側の短いほうまで届く。基本のアンテナは 3000 m）
自分の構築物が送ったものは自分では受け取りません。遠隔操作の例は drone.py / drone_remote.py。

機器（devices() の要素）
------------------------
.name .type .kind .direction .state .cell .offset（重心からのローカル位置）
推進機:         .throttle = 0.0〜1.0
バルブ:         .open = True/False
発電機/精錬機/加工機/通気口: .enabled = True/False
タンク:         .contents（読み取り）
バッテリー:     .charge（読み取り）
コンピューター: .display(text, ...) / .clear_display()（他のコンピューターに表示）
アクチュエーター（出力面の先の部分を動かす）:
  .rotate(角度, speed=度/秒)        相対回転      .rotate_to(角度, speed)   絶対角度へ
  .extend(量, speed=ブロック/秒)    押し出し（最大ブロック1つ分）
  .retract(量, speed)               引き込み      .extend_to(位置 0〜1, speed)
  .detach()                         分離          .attach()  出力面の先のブロックと結合
  .execute()                        P メニューで設定した動作を実行
  .angle .extension .attached .busy（読み取り）
  .output_angle                     出力側ブロックの世界から見た向き（度・横軸なら真上が 0）
'''


def rot_for(up):
    """上方向 up に立てた回転行列（列が ローカル x, y, z のワールド方向）。"""
    up = up / np.linalg.norm(up)
    fwd = np.cross([1.0, 0, 0], up)
    fwd /= np.linalg.norm(fwd)
    right = np.cross(up, fwd)
    return np.stack([right, up, fwd], axis=1)
