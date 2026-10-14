# Rust の物理コア（alpedia_core）

物理の重い部分を Rust で書いたものです。Python からは `alpedia_core` というモジュールとして使います。
ビルドしたファイルを `lib/` に置くと、ゲームが自動で使います（画面左上に「物理:Rust」と出ます）。
無いときは、今までどおり Python（numba）で計算します。

## Rust にしたもの

| 中身 | Rust（`alpedia_core/src/`） | 元の Python |
|---|---|---|
| 構築物1つの1サブステップまるごと（重力・浮力・減衰・積分・形合わせ・結合・回転ジョイント・地面との接触・塑性変形・ちぎれの判定、剛体・完全剛体モードの接触も） | `body.rs`, `lib.rs`（`Body`） | `main/construct.py` の `substep_begin` / `substep_end` |
| 描画用の頂点（ブロックごとに動いている構築物の形・法線・陰影・噴射の色をまとめて） | `mesh.rs`, `lib.rs`（`mesh_pack`） | `main/client/render.py` の `_update_blob` |
| 構築物どうし（と、結合していないボクセルどうし）の押し出し | `contacts.rs`, `pairs.rs`, `lib.rs`（`Proxy`・`contacts`） | `main/world.py` の `_voxel_collisions` |
| 完全剛体モードの関節つき剛体の1サブステップ（押し出し・関節・地面と MOD の箱の接触） | `artstep.rs`, `lib.rs`（`ArtTopo`） | `main/construct.py` の `_art_begin` / `_art_end` |
| ボクセルごとの形合わせ・結合 | `kernels.rs` | `main/fastphys.py` |
| 当たり判定の芯・描画用の頂点と法線 | `kernels.rs` | `main/fastphys.py` |
| 向き付きの箱との距離 | `kernels.rs` | `main/geom.py` |
| 完全剛体モードの関節つき剛体 | `art.rs` | `main/fastphys.py` |
| 惑星の地形（距離場・タイル・池・洞窟・掘削跡）と地面との接触 | `planet.rs`, `lib.rs` | `main/planet.py`, `main/terrain_earth.py`, `main/construct.py` |
| 重力・構築物どうしの近い点の組 | `lib.rs` | `main/world.py`（scipy の cKDTree） |

Python 側の入り口は `main/fastphys.py`（関数ごとに Rust と numba を選ぶ）、`main/planet.py`（`_nat()`）、
`main/native.py`（読み込み）です。`config/settings.json` の `physics.backend` で選べます。

- `"auto"`：既定。Rust があれば使う。
- `"rust"`：Rust が無ければエラーにする。
- `"python"`：Rust を使わない（比べるとき用）。

環境変数 `ALPEDIA_BACKEND=python` でも切り替えられます。

物理の1サブステップは、構築物ごとの計算（ブロックごと・剛体・完全剛体・関節つき剛体）も、構築物どうしの押し出しも Rust で行います。
Python に残っているのは、次のものだけです。

- 街などの MOD が出す当たり判定の箱との接触（MOD のプログラムを呼ぶため）。完全剛体・関節つき剛体では、
  Python が箱との接触点を集めて Rust に渡し、地面の接触と一緒に解きます
- ブロックごと ⇔ 剛体 ⇔ レールの切り替えの判断、ちぎれたあとの作り直し、機器・スクリプト（ゲームの決まりごと）
- 環境変数 `ALPEDIA_NATIVE_STEP=0` のとき（比べるとき用。Rust 化の前の計算に戻す）

構築物の形が変わったとき（ブロックの追加・削除・ちぎれ）は、`Body`・`Proxy`・`ArtTopo` を作り直します。

## Windows でビルドする

最初に1回だけ、Rust を入れます。

1. https://rustup.rs を開き、`rustup-init.exe` をダウンロードして実行します。
   - 「Visual Studio のビルドツールが無い」と聞かれたら、そのまま進めてかまいません（このビルドでは使いません）。
   - 選択肢は既定（1）のままで大丈夫です。
2. 入れ終わったら、開いているコマンドのウィンドウは閉じます。

あとは **`native\build_windows.bat` をダブルクリック**するだけです。2つ作ります。

| できるもの | 使う Python |
|---|---|
| `lib\alpedia_core.pyd` | この PC の Python（3.10 以上ならどれでも。`python3.dll` を使う形） |
| `lib\alpedia_core.cp312-win_amd64.pyd` と `package for windows\Alpedia\game\lib\` の同じもの | パッケージ版に同梱の Python 3.12（`python3.dll` が無いので 3.12 専用に作る） |

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

`tests/test_native.py` が、Rust 版と Python 版に同じ入力を与えて結果を比べます（違いが 1e-7 以内か）。
Rust が無ければ、このテストは飛ばされます。
