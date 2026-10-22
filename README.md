# darask-IME

Mozc の辞書・接続コストを土台にした、候補の出し方を改善した日本語 IME(fcitx5 アドオン)。

## 候補の工夫
| 入力 | 先頭付近の候補 |
|---|---|
| `ekusutensyon` | エクステンション / **extension** / えくすてんしょん |
| `fairu` | ファイル / **file** |
| `konpyuta` (長音なし) | コンピュータ / コンピューター / **computer** |
| `kifewonomu` | **キフェを**飲む(意味のない外来語音は自動でカタカナ化、ほかは通常変換) |
| `dockerwoinsutorusuru` | dockerをインストールする(英字と日本語の混在を文節分割) |
| `Githubdepush…` | Github で … (大文字始まりは英字を優先) |

- 英語候補: CMUdict の発音 → カタカナ読みを生成して逆引き(長音の有無は無視)。`data/src/extra_en.tsv` で手動追加可。
- 技術用語 (`crates/core/tech_words.txt`) は英字のまま文節として認識。
- 学習: 選んだ候補は `~/.local/share/darask-ime/history.tsv` に保存され、辞書にない語も次回から候補になる。
- キー: Space/↓ 次候補、↑ 前候補、←/→ 文節移動、**Shift+←/→ 文節の伸縮**、1-9 候補選択、
  Tab/↓(入力中) サジェスト選択、F6-F10 ひらがな/カタカナ/半角カナ/全角英数/半角英数、Esc 戻る、Enter 確定。

## ビルド・導入 (NixOS)
```
nix build .#addon
```
アドオンは fcitx5 本体と同じバージョンでビルドするのが安全です。overlay を使うと、システムの nixpkgs の fcitx5 に合わせてビルドされます:
```nix
nixpkgs.overlays = [ darask-ime.overlays.default ];
i18n.inputMethod = {
  enable = true;
  type = "fcitx5";
  fcitx5.addons = [ pkgs.fcitx5-darask ];
};
```
(手早く試すだけなら `darask-ime.packages.x86_64-linux.default` も使えますが、flake の nixpkgs の fcitx5 でビルドされます。)
再構築後、fcitx5 の設定で入力メソッド「Darask (日本語)」を追加。

## 検証
`nix flake check` で、本物の fcitx5 にアドオンをロードしてキー入力→確定まで通す E2E テスト(addon/test/e2e.cpp)が走ります。

## 開発
```
nix develop
cargo test --release            # 要 data/build (下記)
cargo run --release -p darask-cli -- ekusutensyon
```
開発用データ生成(JMdict があれば外来語の原綴りも使う):
```
cd data/src && curl -LO https://raw.githubusercontent.com/cmusphinx/cmudict/master/cmudict.dict \
  && curl -LO https://norvig.com/ngrams/count_1w.txt && cd ../..
cargo run --release -p darask-build -- $MOZC_SRC data/src data/build
```

## 構成
- `crates/build` 辞書データ生成 / `crates/core` 変換エンジン(ラティス+Viterbi/A*, 候補生成, 学習, 入力状態機械)
- `crates/ffi` C ABI / `addon/` fcitx5 シム(C++)

## 既知の制限
- 辞書は Mozc OSS 版(Google IME の Web 由来語彙は無し)。
- 英単語の端がローマ字の途中に来る場合(`pushisita`)は、英語らしい語だけ区切りを試す。誤判定する場合は候補から選び直せる。
- 自分の NixOS の fcitx5 (システム側) への導入と、実画面での候補ウィンドウの見た目は未確認(E2E は fcitx5 のテストフロントエンドで実施)。
