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

画面は出ません（オフスクリーンで描く）。本編は 1 本 15〜40 分ほど（CPU 2 つのとき。GPU があればもっと速い）。

### 必要なもの

ゲーム（dev/）と同じ Python 環境に、`pip install -r movies/requirements.txt`（ffmpeg の入った imageio-ffmpeg）。
ゲームの requirements（ursina・numpy・scipy・pillow）も使います。

## 本編のムービー（章の区切りに入れるもの）

`docs/キャラクター・勢力設定 ブラッシュアップ版.pdf` の章立てどおり、全 11 本。ゲームで流すものは
**`dev/mods/story/movies/`** に置いてあります（シナリオモードが章の区切りで再生。描き直したら `output/` から移し、`ffmpeg -i x.mp4 -c:v libx264 -crf 25 -preset slow -vf hqdn3d=1.5:1.5:4:4 -c:a copy` で小さくしてから置き換える）。役者は `dev/mods/skinmodel(dummy)/` の登場人物の VRM
（towa・mikaga・nemuri・sakura・siyama・sirotsu・kensu・kingi・donachi・nekosa・dabai・rutero・omiku）です。

| ファイル | 章 | 中身 |
|---|---|---|
| `01_prologue.mp4` | 序章 | 脱出ポッドが荒れ地へ落ちる。ミカガの無線、ポッドの中で目を開けるトワ（紫の目）、題名 |
| `02_naming.mp4` | 1章 | 墜落現場に来たサクラが、識別名「三号」の少女に「トワ」と名付ける |
| `03_watch.mp4` | 5章 | キンギのドローンが基地を見張る。月の総統府でネムリが「目を離すな」と命じる |
| `04_launch.mp4` | 8章 | 直したルミナス号で発進。サクラが手を振って見送り、ミカガの無線 |
| `05_declaration.mp4` | 10章の始め | ルナ・ポートでミカガと出会う（紫の目）。ネムリの宣戦の立体映像に、トワの頭を記憶のかけらがよぎる（すっかり戻るのはゲームの中でポッドの記録を見たとき） |
| `06_interlude.mp4` | 10章の終わり | シヤマの誘い。旧資料室でサクラがアマツの落下の証拠をつかむ |
| `07_fleet.mp4` | 11章 | 艦隊戦。ケンスとシロツ、キンギのミサイルの雨をルミナス号がすり抜ける |
| `08_duel.mp4` | 12章 | アマツの外壁でロボットどうしの決闘。キンギのミサイルをかわして組み合う |
| `09_dialogue.mp4` | 終章A | 落ちていくアマツの中心でネムリと対話。撃たない・落とさない、三つ目を探す |
| `10_reaccel.mp4` | 終章B | 撃ち合いが止まり、両艦隊とモジュール 24 基でアマツを押し上げる。キンギが倒れドナチが引き継ぐ |
| `11_epilogue.mp4` | エピローグ | 法廷のネムリ、N財団の残党、みんなでトワのロケットを見送る。ALPEDIA ／ それでも |

**音楽**：ゲームに入れるムービーの音は効果音と環境音だけです（`python remix_audio.py` で、描き直さずに音だけ作り直せる）。
曲はゲームが BGM の MOD（`dev/mods/bgm`）から重ねて流します（どの曲を何秒から流すかは `dev/mods/story/code/script.py` の `BGM`）。
BGM と効果音の音量は、ゲーム設定でそれぞれ 0〜100 % に変えられます。`render.py` でそのまま作ると、今までどおり数式の和音の音楽も入ります。

場面のファイルは `scenes/m01_prologue.py` 〜 `scenes/m11_epilogue.py`。共通の配役・ポーズ・部屋・船・ロボット・ロケットは
`scenes/lib.py`、セリフ（字幕と口の動き）は `scenes/base.py` の `Movie.say()` です。

## パイロット版

| ファイル | 中身 |
|---|---|
| `output/pilot_drama.mp4` | ドラマ「通信」約 17 秒（ためしの VRM で） |
| `output/pilot_fleet.mp4` | 艦隊戦「黒鉄の艦隊」約 16 秒 |

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
