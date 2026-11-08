"""ゲーム同梱の文章（スクリプトの説明 README）。チュートリアルなどが使う。"""
import numpy as np

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
api.gravity           重心の場所の重力加速度 [x,y,z] m/s²（近くのすべての天体の分の合計）
api.up_local          上方向（構築物ローカル座標。直立なら [0,1,0]）
api.angular_velocity_local  角速度（ローカル座標）
api.velocity_local    速度（ローカル座標）
api.mass              質量 kg
api.power             {"production", "demand", "stored", "capacity", "ratio"}
api.storage           構築物の精錬機・加工機の中にある物の合計
api.input             操縦席に座っている時に押されているキーの集合
api.seated            この構築物の操縦席に誰かが座っているか（True/False）
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
api.show_arrow(vec, color=(1,0.85,0.15), scale=1)
                      矢印 vec [x,y,z]（ワールド座標）を、構築物の重心から伸びる3Dの矢印で見せる（約0.3秒で消える。
                      毎回呼ぶ。色ごとに1本）
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
推進機:         .ignite()  点火（電気を少し使う。燃えていれば何もしない）
                .lit       燃えているか　.throttle  いまの出力 0〜1（読み取り）
                燃えている間は燃料を使って推力と電気を出す。タンクと直接つながっていると
                燃料が尽きるまで全開で燃え続ける（止められない）。止める・出力を変えるには、
                タンクとのあいだにバルブを入れて、バルブで（閉じると火が消える。開けたら点火し直す）
バルブ:         .open = 0.0〜1.0  開き具合（True/False も可）。最大流量 × 開き具合だけ燃料・物資を通す
太陽光パネル/精錬機/加工機/通気口: .enabled = True/False
精錬機・加工機: 原料は配管でつながった所（タンク・ほかの機械の作った物）から自動で受け取り、
                作った物は自分の中にためる。つながったタンクは、作った燃料などを受け取る
タンク:         .contents（読み取り）
バッテリー:     .charge（読み取り）
コンピューター: .display(text, ...) / .clear_display()（他のコンピューターに表示）
アクチュエーター（出力面の先の部分を動かす）:
  .rotate(角度, speed=度/秒)        相対回転      .rotate_to(角度, speed)   絶対角度へ
  .extend(量, speed=ブロック/秒)    押し出し（最大ブロック1つ分）
  .retract(量, speed)               引き込み      .extend_to(位置 0〜1, speed)
  .detach()                         分離          .attach()  出力面の先のブロックと結合
  .execute()                        P メニューで設定した動作を実行
  ヒンジ（機器「ヒンジ」）も同じ名前で使う：.rotate / .rotate_to で -90〜+90° 振る（ブロックの中心が軸。+ で倒れる側は置き方で決まる）
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
