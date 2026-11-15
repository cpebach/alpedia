# エンドロールのロゴ

エンドロールの「お世話になったもの」に出すロゴの絵（`<名前>.png`）の置き場です。
並びと表示名・ひとことは `mods/story/code/script.py` の `CREDITS_THANKS`。絵が無いものは名前を文字で出します。

ロゴの絵は各社・各団体のものなので、使い方（リポジトリに入れる・ゲームと一緒に配る）はそれぞれの意向を確かめてから決めます。
確かめるまでは絵をリポジトリに入れません（`.gitignore` で `*.png` を外しています）。手元で置いた絵は、パッケージを作るとゲームに入ります。

| 名前 | 表示名 | 使っているところ | 扱い |
|---|---|---|---|
| anthropic | Anthropic | Claude（いっしょに作った） | 未確認 |
| python | Python | ゲームのほとんど | 未確認 |
| rust | Rust | 物理演算のコア・画面・音 | 未確認 |
| github | GitHub | ソースの置き場 | 未確認 |
| numpy | NumPy | 計算 | 未確認 |
| ffmpeg | FFmpeg | ムービー | 未確認 |
| vroid | VRoid Studio | 登場人物 | 未確認 |

意向を確かめたら「扱い」を書き換え、入れてよいものは `.gitignore` に `!dev/mods/story/credits/<名前>.png` を足してリポジトリに入れます。
使わなくなったもの（Panda3D・Ursina Engine。今の画面は Rust）は絵も一覧も消しました。
