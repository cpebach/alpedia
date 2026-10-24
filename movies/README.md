# ムービー（プリレンダリング）

ストーリーの章の区切りに入れるムービー（ドラマ・艦隊戦など）を、**ゲームと同じ描画**（ブロックの構築物・惑星・VRM のキャラクター）で
1 コマずつ描いて mp4 にする道具です。カメラのアングル・パン・ピント（被写界深度）・ピント送り・揺れ・動きのぶれ・
光のにじみ・色の調整・黒帯（シネスコ）・字幕・効果音と音楽まで、すべてプログラムで作ります。

## 使い方

```
cd movies
python render.py pilot_drama              # → output/pilot_drama.mp4（1920×1080、30 コマ/秒、音つき）
python render.py pilot_fleet
python render.py pilot_fleet --scale 0.5  # 半分の大きさで早く確かめる
python render.py pilot_drama --still 3,8.5   # その時刻のコマを output/stills/ に PNG で
python render.py pilot_fleet --from 9 --to 13  # その間だけ
python render.py pilot_drama --no-audio
```

画面は出ません（オフスクリーンで描く）。1 本 2〜10 分ほど（GPU があればもっと速い）。

### 必要なもの

ゲーム（dev/）と同じ Python 環境に、`pip install -r movies/requirements.txt`（ffmpeg の入った imageio-ffmpeg）。
ゲームの requirements（ursina・numpy・scipy・pillow）も使います。

## できているもの（パイロット版）

| ファイル | 中身 |
|---|---|
| `output/pilot_drama.mp4` | ドラマ「通信」約 17 秒：旗艦の外観 → 艦橋のミオ → 警報 → ピント送り → 肩越しに黒鉄艦隊の総督ヴァルガの立体映像 → 決意の寄り |
| `output/pilot_fleet.mp4` | 艦隊戦「黒鉄の艦隊」約 16 秒：防衛線 → 敵艦の一斉発射 → ミサイルを追うカメラと迎撃 → 命中 → 主砲 → 敵艦が折れて沈む → 題名 |

役者はどちらも今ある VRM（`dev/mods/skinmodel(dummy)/alpedia_player1.vrm`、作者 omicron / ©Nukers、再配布・改変可・クレジット表記が必要）を
代役に使っています。ヴァルガは赤い立体映像として描いています。本番の VRM をもらったら、`scenes/sets.py` の `vrm()` に名前を渡すだけで差し替わります。

## 仕組み

| ファイル | 中身 |
|---|---|
| `render.py` | 描いて mp4 にする入口 |
| `cinema/stage.py` | 撮影所：ゲームの描画を画面なしで使う。ブロックの構築物を完全な剛体として、決めた位置・向きに置く（形は 1 回だけ作る） |
| `cinema/actor.py` | VRM の役者：表情（まばたき・口・喜怒哀驚）、セリフの間の口の動き、ポーズのキーフレーム、見る先、照明、立体映像の描き方。動き・揺れものは VRM の見た目 MOD のものを使う |
| `cinema/fx.py` | 光の玉・煙・ビーム・爆発・破片・遠景の惑星（海・雲・大気の縁）・星空。どれも「時刻 → 見え方」だけで計算（何度描いても同じ） |
| `cinema/shapes.py` | ブロックで作る船（ルナ艦隊の巡洋艦・旗艦アルテミス・黒鉄艦隊の巡洋艦と旗艦・ミサイル・遠くの張りぼて） |
| `cinema/post.py` | ピントのシェーダー（奥行き 32 ビット）、仕上げ（ブルーム・色・周辺減光・粒・閃光・色ずれ・フェード・黒帯・字幕）、ffmpeg への書き出し |
| `cinema/film.py` | 1 本のムービー（カットの並び・字幕・仕上げ）と、描く手順（動きのぶれは 1 コマに数枚重ねる） |
| `cinema/sound.py` | 効果音と音楽を数式で作る（爆発・光線・警報・発射・和音・太鼓など。録音素材は使わない） |
| `scenes/*.py` | 場面。`build`（物と役者を置く）、`shots`（カットとカメラ）、`lines`（字幕）、`grade`（色）、`audio`（音） |

### 場面の書き方（例）

```python
class FILM(Film):
    name, duration = "my_scene", 8.0
    lines = [(1.0, 3.0, "ミオ", "こちらルナ・ポート")]

    def __init__(self):
        super().__init__()
        self.shots = [Shot(0, 4, self.cam_a), Shot(4, 8, self.cam_b)]

    def build(self, st):
        ship = st.ship(shapes.luna_cruiser().voxels(), size=1.0)
        ship.motion = lambda t: (np.array([0, 0, 10.0 * t]), np.eye(3))     # 時刻 → (位置, 回転)
        mio = Actor(st, sets.vrm(), "mio")
        mio.place = lambda t: ((0, 0, 0), 180)                              # 足元と向き
        mio.talk = [(1.0, 3.0)]

    def cam_a(self, t, u):     # u はカットの中の 0〜1
        return Cam(pos=(0, 1.5, -3), target=(0, 1.4, 0), fov=30, aperture=10)   # aperture でぼかす
```
