# Alpedia パッケージ版（Windows 64bit）

Python やライブラリのインストールなしで遊べる版です。

## 遊ぶ

`Alpedia` フォルダの **`Alpedia.exe` をダブルクリック**します。

- `Alpedia-console.exe` は、ログを画面に出しながら起動する調査用です。
- 初めての起動では、物理計算（numba）の機械語への変換に数十秒かかることがあります。2 回目からは `Alpedia/cache` に保存されたものを使うので速くなります。
- ほかの PC に配るときは、`Alpedia` フォルダを丸ごと渡すか、`dist/Alpedia-win64.zip` を渡します（展開してから `Alpedia.exe` を起動）。

### セーブと設定

パッケージ版の保存先は `Alpedia/game/saves`、設定は `Alpedia/game/config/settings.json` です。開発版（`alpedia/dev`）の `saves` とは別なので、開発版のセーブには触れません。開発版のワールドで遊びたいときは、`alpedia/dev/saves/<ワールド名>` フォルダを `Alpedia/game/saves/` にコピーします。

## GPU による高速化

柔らかい構築物は毎フレーム形が変わります。その頂点の位置（スキニング）、法線、陰影の計算を GPU のシェーダーで行います。CPU は節点の位置（ボクセル数 × 8 点）を送るだけで済みます。

| GPU の対応 | 使う方法 |
|---|---|
| OpenGL 4.3 以上（最近の GPU のほとんど） | コンピュートシェーダーで頂点位置を計算し、頂点シェーダーで法線を計算 |
| OpenGL 3.3 | 頂点シェーダーだけで両方を計算 |
| それ未満、またはエラーが出たとき | 元の CPU（numba）計算に自動で戻る |

`Alpedia/game/config/settings.json` の `graphics.gpu` で切り替えます。

- `"auto"`：既定。上の表のとおりに自動で選ぶ。
- `"compute"`
- `"vertex"`
- `"off"`

どれが使われたかは `game/logs/latest.log` の `alpedia.gpu` の行に出ます。

- 剛体になって止まっている構築物は形の計算が要らないので、これまでどおり CPU 側で使い回します。
- 物理計算そのもの（PBD の形合わせ）は CPU（numba、複数スレッド）のままです。構築物ごとの計算が小さく、GPU に送って結果を受け取る往復のほうが遅くなるためです。

シェーダーは `Alpedia/alpedia_gpu/shaders/`（元は `build/overlay/alpedia_gpu/shaders/`）にあります。

- `skin.comp`：コンピュートシェーダー。頂点位置を計算する。
- `blob.vert`：頂点シェーダー。法線を計算する（GL 3.3 版ではスキニングも行う）。
- `lit.frag`：陰影。CPU 版の `render.shade` と同じ式。

ゲーム本体（`main/`）のコードは変えていません。起動時に `boot.py` が描画クラスの一部を差し替えます。

## Rust の物理コア

`game/lib/alpedia_core.pyd` があると、物理の重い部分を Rust で計算します（画面左上に「物理:Rust」）。
開発版の `alpedia\dev\native\build_windows.bat` をダブルクリックすると、作ってここにも入れます（最初に Rust を入れる必要があります。`dev/native/README.md`）。
無いときは numba で計算します。

## パッケージを作り直す（ゲームを更新したとき）

開発版（`alpedia/dev`）でゲームを変えたら、このフォルダの **`パッケージを作る.bat` をダブルクリック**します（`dev` の中身から作り直します）。PC に Python 3.8 以上が必要です。

- `build/build_package.py` を実行するので、Windows 以外の OS でも作れます。
- `python build_package.py --skip-runtime` は、ゲーム部分だけを更新します（速い）。
- `python build_package.py --zip` は、配布用の zip も作ります。
- `python build_package.py --test` は、作ったあとに描画なしで起動テストをします（Windows のみ）。

作り直しても、パッケージ版の次のものは消しません。

- `game/saves`
- `game/logs`
- `game/config/settings.json`
- `game/mods` に自分で足したもの（本体に入っている MOD のサバイバル・街は入れ替えます）

### 使っているもの

| 部品 | 中身 |
|---|---|
| Python 3.12 | 次の2つのどちらか。どちらを使ったかは `Alpedia/runtime/RUNTIME.txt` に書かれます。<br>・python.org の「埋め込み用 Python」<br>・取れないネット環境では、Panda3D の Windows 用 wheel に入っている `python312.dll` と、`build/runtime_fallback/python312.zip`（標準ライブラリ。`make_stdlib.py` で作成） |
| ライブラリ | `build/requirements-windows.txt` の版（動作確認した版に固定）。PyPI から Windows 用の wheel を取って入れます。 |
| ランチャー | `build/launcher/launcher.c`（C 言語）。`runtime/python312.dll` を読み込んで `boot.py` を実行します。<br>`pip install ziglang` が入っていれば、作り直すたびに zig の C コンパイラで exe も作り直します（なければ同梱の exe を使います）。 |

`build/cache` はダウンロードしたものの置き場です。消してもかまいません（次に作るときにもう一度ダウンロードします）。

## 配る（GitHub のリリース）

パッケージ本体（`Alpedia/`、約 800MB）は git には入れません（GitHub は 1 ファイル 100MB まで・リポジトリも重くなるため）。
配るときは zip にして、GitHub の「Releases」に添付します（1 ファイル 2GB まで）。

1. **`配布用zipを作る.bat` をダブルクリック** → `dist\Alpedia-win64.zip` ができる
   （ゲーム部分を `dev` から入れ直してから zip にします。セーブ・ログ・cache は入りません）
2. GitHub のリポジトリのページ → 右側の「Releases」→「Draft a new release」
3. 「Choose a tag」に `v1.1` などの版を入れて「Create new tag」、タイトルと説明を書く
4. `Alpedia-win64.zip` をページ下の枠にドラッグして、「Publish release」

もらった人は zip を展開して `Alpedia.exe` を起動します。
