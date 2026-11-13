# Rust（alpedia_core・alpedia_view）

ゲームの本体を Rust で書いたものです。2つのモジュールがあります。

| モジュール | 場所 | 中身 |
|---|---|---|
| `alpedia_core` | `alpedia_core/src/` | 物理コア（下の「物理コア」の表）と、ゲーム本体（`src/game/`。下の「ゲーム本体」） |
| `alpedia_view` | `alpedia_view/src/` | 窓・描画（wgpu・winit）・音（rodio）。下の「画面」 |

ビルドしたファイルを `lib/` に置くと、ゲームが使います（どちらも無いとゲームは起動しません。前の版の numba・Panda3D・ursina はもう使いません）。

## 物理コア（Python の値を使わない計算）

| 中身 | Rust（`alpedia_core/src/`） | 元の Python（今は消して Rust だけ） |
|---|---|---|
| 1ティックの物理まるごと（動いている構築物すべてのサブステップの繰り返し・MOD の箱との接触・構築物どうしの押し出し・固定の戻し・結合直後のおさえ。Python の処理が要ること（眠っていた物が起きた・ちぎれた）が起きたらそこで返す） | `tick.rs`, `lib.rs`（`step_tick`） | `main/world.py` の `step` のサブステップの繰り返し（`_native_tick`） |
| 機器の1ティック（電力・推進機・配管の流れとバルブ・精錬機・加工機・タンク・通気口・アンテナ・ライト・MOD の機器。状態は Python の dict のまま読み書き。アクチュエーターだけ Python） | `devices.rs`, `lib.rs`（`DevPlan`） | `main/devices.py` の `update` |
| 遠景の箱（LOD）の頂点・機器の記号の四角 | `lib.rs`（`lod_vertices`・`decal_quads`） | `main/client/render.py` の `lod_vertices`・`DecalView._write` |
| 構築物1つの1サブステップまるごと（重力・浮力・減衰・積分・形合わせ・結合・回転ジョイント・地面との接触・塑性変形・ちぎれの判定、剛体・完全剛体モードの接触も） | `body.rs`, `lib.rs`（`Body`） | `main/construct.py` の `substep_begin` / `substep_end` |
| 描画用の頂点（ブロックごとに動いている構築物の形・法線・陰影・噴射の色をまとめて） | `mesh.rs`, `lib.rs`（`mesh_pack`） | `main/client/render.py` の `_update_blob` |
| 構築物どうし（と、結合していないボクセルどうし）の押し出し | `contacts.rs`, `pairs.rs`, `lib.rs`（`Proxy`・`contacts`） | `main/world.py` の `_voxel_collisions` |
| 完全剛体モードの関節つき剛体の1サブステップ（押し出し・関節・地面と MOD の箱の接触） | `artstep.rs`, `lib.rs`（`ArtTopo`） | `main/construct.py` の `_art_begin` / `_art_end` |
| ボクセルごとの形合わせ・結合 | `kernels.rs` | `main/fastphys.py`（前の版の numba） |
| 当たり判定の芯・描画用の頂点と法線 | `kernels.rs` | `main/fastphys.py` |
| LAN マルチプレイの通信（メッセージ ⇔ バイト列・TCP の送受信スレッド・受け付け・LAN のワールドの知らせ） | `net.rs`（`Conn`・`Inbox`・`Listener`・`Announcer`・`Browser`） | `main/net.py` |
| 向き付きの箱との距離・レイと箱の交差 | `kernels.rs`, `geomq.rs` | `main/geom.py` |
| 形への最短距離（プレイヤーとブロックの当たり）・視線とブロック／地面／水面の交差・角の暗さ（AO）・OBJ の読み込みとボクセル化 | `geomq.rs`, `lib.rs`（`closest_on_shape`・`voxel_raycast`・`PlanetShape.raycast`・`water_raycast`・`block_ao`・`obj_parse`・`voxelize`） | `main/shape.py`, `main/world.py` の `raycast`, `main/planet.py`, `main/client/render.py`, `main/objio.py` |
| 完全剛体モードの関節つき剛体 | `art.rs` | `main/fastphys.py` |
| 惑星の地形（距離場・タイル・池・洞窟・掘削跡）と地面との接触 | `planet.rs`, `lib.rs` | `main/planet.py`, `main/terrain_earth.py`, `main/construct.py` |
| 重力・構築物どうしの近い点の組 | `lib.rs` | `main/world.py`（scipy の cKDTree） |
| 構築物の形の芯（2×2×2 の窓ごとの点・線・斜面の三角形）・頂点ごとの当たり判定の芯・形の距離場（描画のメッシュの元） | `topo.rs`, `lib.rs`（`topology`・`build_contacts`・`shape_sdf`） | `main/shape.py` の `topology`・`build_mesh`、`main/construct.py` の `_build_contacts` |
| 値ノイズ・fbm（地形・色・池・洞窟の生成） | `planet.rs`, `lib.rs`（`noise`） | `main/planet.py` の `value_noise`・`fbm` |
| 形の等値面（Marching Cubes）・空洞の塗り分け・いちばん近い点・小さな凸包・負にならない最小二乗 | `geomx.rs`, `mc_tables.rs`, `body.rs`（`nnls`） | scikit-image・scipy（`main/shape.py`, `main/airtight.py`, `main/construct.py`） |

Python 側の入り口は `main/fastphys.py`（芯・皮・法線）、`main/planet.py`（`_nat()`）、`main/native.py`（読み込み）です。
Rust と同じ計算をする Python の予備（numpy 版）はもうありません。地形・物理・機器の計算は Rust だけで行います。

物理の1サブステップは、構築物ごとの計算（ブロックごと・剛体・完全剛体・関節つき剛体）も、構築物どうしの押し出しも Rust で行います。
構築物の形が変わったとき（ブロックの追加・削除・ちぎれ）は、`Body`・`Proxy`・`ArtTopo` を作り直します。

## ゲーム本体（`alpedia_core/src/game/`）

前は `main/` の Python にあったゲームの決まりごと・画面を、同じ名前のまま Rust に移したものです（`main/world.py` → `game/world.rs` など）。
`main/` の Python は、既定値の表と、ここを呼ぶだけの薄い入口です。どのファイルに何があるかは `dev/README.md` の「コード構成」の表にあります。

- **値は Python のまま持ちます。** 位置・速度（numpy）、機器の状態・設定（dict）などは Python の値として持ち、Rust から読み書きします。
  MOD とゲーム内のプログラムが、前と同じように Python から同じ値を触れるようにするためです。
  その分、ここの Rust は Python の属性の読み書き・メソッドの呼び出しが多く、Python のインタープリタを止めたまま動きます
  （複数コアで同時に進むのは、上の「物理コア」の計算だけ）。
- **Python のままの物**：MOD のプログラム（`mods/*/code/`）、ゲーム内のプログラム（`scripts/*.py`。ブロックプログラムもここへ変換される）、
  チュートリアルの舞台の一部（`main/tutorial_drone.py`・`tutorial_rod.py`）、スクリプトの説明の文章（`main/content.py`）。
  池・洞窟の配置を決める乱数も numpy のまま（今までのワールドと同じ地形にするため）。
- MOD の画面のための ursina・panda3d.core のまねは `game/ursina.rs`・`game/panda.rs`。

## Windows でビルドする

最初に1回だけ、Rust を入れます。

1. https://rustup.rs を開き、`rustup-init.exe` をダウンロードして実行します。
   - 「Visual Studio のビルドツールが無い」と聞かれたら、そのまま進めてかまいません（このビルドでは使いません）。
   - 選択肢は既定（1）のままで大丈夫です。
2. 入れ終わったら、開いているコマンドのウィンドウは閉じます。

あとは **`native\build_windows.bat` をダブルクリック**するだけです。`alpedia_core` と `alpedia_view` を、それぞれ2つの形で作ります。

| できるもの | 使う Python |
|---|---|
| `lib\alpedia_core.pyd`・`lib\alpedia_view.pyd` | この PC の Python（3.10 以上ならどれでも。`python3.dll` を使う形＝abi3） |
| `lib\alpedia_core.cp312-win_amd64.pyd`・`lib\alpedia_view.cp312-win_amd64.pyd` と `..\packages\windows\Alpedia\game\lib\`（alpedia フォルダの packages） の同じもの | パッケージ版に同梱の Python 3.12（`python3.dll` が無いので 3.12 専用に作る） |

- abi3 の形では、`#[pyclass]` で Python の例外などの組み込みの型を継げません（3.12 専用の形では通ってしまうので、両方のビルドで確かめる）。
- 初回は、Windows 用の GNU ツールチェーンの準備とビルドに数分かかります。
- ビルドの途中のファイルは `%LOCALAPPDATA%\alpedia_build` に置きます（OneDrive で同期されないように）。
- 記録は `native\build_log.txt` に残ります。失敗したときはこのファイルを見ます。
- Python のインストールが無くても、パッケージ版用は作れます（Python の情報は設定ファイルで渡し、
  リンクは `python312.dll` に直接します）。
- Rust のコードを変えたら、もう一度ダブルクリックします。

## Linux / macOS でビルドする

```
cd native/alpedia_core
PYO3_NO_PYTHON=1 cargo build --release --features abi3
cp target/release/libalpedia_core.so ../../lib/alpedia_core.abi3.so      # macOS は .dylib を .so の名前で
```

## 確かめる

```
python -m unittest discover -s tests -v
```

`tests/test_native.py` が、物理コアの計算の性質（動かない物は動かさない・式どおり）と、テストの中に numpy で書いた同じ計算との一致を確かめます
（前にあった「Python 版の本体」はもう無いので、比べる相手はテストの中の式だけです）。ゲーム本体（`game/`）は、ほかのテスト
（`test_core.py`・`test_mods.py`・`test_net.py`・`test_story.py` など）が Python から動かして確かめます。
Rust の中の単体テスト（`cargo test`）はまだありません。

## 画面（alpedia_view）

`native/alpedia_view/` は画面を描く Rust（wgpu・winit）のモジュールです。Python が「ノード（親子の行列）」「メッシュ（頂点 (n,12) float32）」
「テクスチャ」「カメラ」を作って動かし、`pump()` で入力を受け取り、`render()` で描きます。頂点は前の画面（Panda3D）と同じ形のまま使っていて、
`main/client/render.py`・`scenery_view.py` の描画は `main/client/gfx.py`（NodePath のまね）を通して描かれます。
入口は `main/client_rs/client.py`（メニュー）・`viewer.py`（遊ぶ画面）。`build_windows.bat` が `lib/alpedia_view.pyd` も作ります
（ゲームの画面なので、これが無いと起動できません）。

- 骨で曲げる形（VRM）：`skin_geom`（位置・法線・骨の番号・重み・まばたき）を1回、体ごとに `skin_part`（材質ごとに使う頂点だけ取り出した
  メッシュ）、毎フレーム `skin_pose`（骨の行列 (J,3,4)・まばたき・光の向き）で CPU で曲げ、トゥーン調の陰影を頂点の色に焼き込む。
  `mesh_cutoff` は不透明さの切り捨て（MASK）。
- テクスチャ：`tex_bytes`（PNG・JPEG のファイルの中身）は初めて描くときに読み解き、GPU に送ったら画素は捨てる。
  描くメッシュが使うテクスチャだけ GPU に送る。線形のテクスチャ（文字・アイコン・VRM）は縮小した段（ミップマップ）も作る。
  `tex_update` は画素の差し替え（ムービーの1コマ）。
- 窓：`set_size`・`set_fullscreen`（設定の解像度・フルスクリーン）。
- 音：`alpedia_view.Audio`（rodio）。`load`（wav・mp3 など）・`load_pcm`（読み解いた音。ムービーの音）・`play(番号, 始める秒)`・
  `stop`・`playing`・`set_volume`・`set_loop`。音の出口が無い PC では何も鳴らさないだけ（`ok()` が False）。
  Linux で作るときは ALSA の開発用ファイル（`libasound2-dev`）が要ります。
