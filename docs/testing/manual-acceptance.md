# TUI基本shell (#13) 手動受入

状態: **#13 shellの手動受入OK（ユーザー確認）**。自動PTY/TestBackendと実端末のユーザー確認を区別します。
修正前release（SHA-256 `4311dbe563df350f21695228f7b9578b9001188d1103c961ee27096fa3fa9ea1`）について、
下記準備・観測手順1〜7を案内した後、ユーザーから「手動確認はOK」と報告されました。
Comment領域を低くする要望を受け、SIGINT登録・blocking処理終了待ちの修正と合わせて調整しました。
最終の右列80:20（Comment 20%）のbuildを再起動し、表示の高さ・q/Ctrl+C・通常shell復帰・
headers/body待機中の速やかな終了/中断について、ユーザーから「すべてOK」と確認を得ました。
各サイズ・各操作の生ログやスクリーンショットは収集しておらず、実端末の評価はユーザー申告に基づきます。
この記録はshellだけが対象です。候補選択・確定、課題/コメント取得・検索・入力・editor・IMEは未実装/対象外です。

## 確認環境・結果

| 項目 | 記録 |
| --- | --- |
| 確認日 / 実施者 | 2026-10-28 UTC / ユーザー操作・確認、Root記録 |
| macOS / CPU | macOS 26.7.1（25G241）/ arm64（Root取得） |
| 端末アプリ名 / 版 | Herdr 0.8.0、Ghostty 1.3.1 / 新規専用ペイン |
| build / revision / ローカル差分 | HEAD `65af555d11e7bff41e32f120acaf109e4509265c` + #13未コミット差分、release `bl` SHA-256 `a0e23fc968fcd5bb435468d97f03db920c0733e23c0934dd09f9acad2c7e2a01` |
| TERM / NO_COLOR / 接続 | xterm-256color / 1 / ローカルHTTPS・fake keyのみ。LANGは個別記録なし |
| 観測 / 不具合 | 最終Comment 20%の高さ、q=0/Ctrl+C=130、通常shell復帰、遅延通信中の終了/中断はユーザー確認OK。申告された不具合なし |

Rootの自動検証では、SIGINT/runtime終了待ち修正後の全対象182テスト、fmt、Clippy、audit、release buildが終了0。
最後の比率70→80の変更後は、影響する15テスト（process 7・TestBackend 5・復元/中断 3）、fmt、Clippy、
release build、diff checkを再実行し終了0。release binaryの直接PTY実行でも100x24でDetail16行/Comment4行、
実HTTPSデータ表示・q=0・端末復元を確認しました。実端末のユーザー確認はこれらの代替ではありません。

## ローカルHTTPSを使う準備

実Backlogや実API keyは不要です。リポジトリ直下の実端末で、Python 3/OpenSSLを用意して実行します。
以下はshellを開いたままにする手順で、最後に `exit` するとfixtureを回収します。

```sh
cargo build --locked --bins
bash
bl_bin="$PWD/target/debug/bl"
fixture="$PWD/tests/fixtures/https_server.py"
root=$(mktemp -d)
server=
trap 'if [ -n "$server" ]; then kill "$server" 2>/dev/null || :; wait "$server" 2>/dev/null || :; fi; rm -rf "$root"' EXIT
printf '%s\n' '{"routes":{"/api/v2/space":{"body":"{\"name\":\"Local shell test\"}"},"/api/v2/projects":{"body":"[{\"id\":1,\"projectKey\":\"LOCAL\",\"name\":\"日本語 é 👩‍💻\"}]"}}}' > "$root/response.json"
python3 "$fixture" "$root" > "$root/port" &
server=$!
while [ ! -s "$root/port" ]; do kill -0 "$server" 2>/dev/null || break; sleep 0.1; done
mkdir "$root/backlog"
printf '{"profiles":{"default":{"spaceUrl":"https://localhost:%s","apiKeyEnv":"TEST_KEY"}}}\n' "$(cat "$root/port")" > "$root/backlog/config.json"
run_bl() {
  (cd "$root" && env -i HOME="$root" XDG_CONFIG_HOME="$root" TEST_KEY=test-secret NO_COLOR=1 \
    SSL_CERT_FILE="$root/tls/localhost-cert.pem" "$bl_bin" "$@")
}
before=$(stty -g)
run_bl
code=$?; printf 'exit=%s\n' "$code"
[ "$(stty -g)" = "$before" ] && echo 'termios unchanged'
# 完了時: exit
```

## 再実施用の観測手順

1. 100x24以上で5パネルと実fixture名が見えること、二重枠/太字/>でfocusが識別できること。
   Projectは表示のみ。Enter・j/kで確定や課題取得を行わないこと。
2. h/l・Tab・Shift+TabでProject/Issueを切替。IssueでEnter→空のDetail、h/lでComment、Esc→Issue。
   未実装操作を利用可能として案内しないこと。日本語などの見え方を記録（IME受入ではない）。
3. 99x24、100x23、40x12へresizeしてfocusパネルだけになること。
   39x12、40x11では案内とq/Ctrl+Cのみ。小さい間にTab/Enterを押し、戻してfocus不変を確認。
4. qで0、再起動してCtrl+Cで130。終了ごとにcursorと通常画面が戻り、shellで文字入力/削除できること。
   上記termios比較と終了コードも記録。`NO_COLOR`なしでも別途実行し、復元後の表示属性を目視確認。
5. fixture応答を以下へ変更し、通信中にfocus移動とq/Ctrl+Cを試す。
   別の実端末から `kill -INT <blのPID>` も試し、130と復元を確認。
   `header_delay` を `delay` に替えるとheaders後body待ちを試せます。
   `printf '%s\n' '{"header_delay":20,"body":"{\"name\":\"Delayed\"}"}' > "$root/response.json"`
6. `printf '%s\n' '{"status":401}' > "$root/response.json"` 後に起動。
   初期取得エラーと終了1を明示し、架空space/空projects成功にせず、端末を復元すること。
7. `run_bl </dev/null`、`run_bl >"$root/output"` はTTYエラー1。
   `run_bl --help` / `--version` は設定fileを一時的に別名にしても0（確認後file名を戻す）。

panic/復元失敗注入は自動試験 `cargo test --locked --lib shell_restores -- --nocapture` を使用し、
製品にデバッグ起動flagを設けません。SIGKILL/端末切断後の復元は保証対象外です。

# 単行入力部品 (#14) 早期実端末確認

状態: **#14の早期実端末・実IME受入OK（ユーザー確認）。tui-input 0.15.5を共有単行入力に採用**。
2026-10-28 UTC、Herdrの新規タブで修正後の実部品を起動し、Rootが操作を順に案内し、ユーザーが各結果を確認しました。
上記#13のshell OK記録は保持し、この入力部品の受入やIME成功へ流用していません。
下表は実際に確認した操作と、自動試験のみの境界を区別します。全端末・全IMEの一致を保証しません。
本格的な検索/フォーム組込みは今回の範囲外です。

## 起動（実サービス・設定不要）

Apple Silicon Macの実端末で、リポジトリ直下から実行してください。
手動testは `cfg(test)` 限定で、通常の製品 `bl` に入力確認command/flagや設定を追加しません。
実部品 `SingleLineInput` と製品の端末loop・16ms tick・poll/32 event上限・TerminalGuard・SIGINT復元を共有し、
初期API取得のみ省きます。実keyや実データを入力せず、fake secret **`test-secret`** だけを使ってください。
原文のdebug出力・保存・送信はありません。

```sh
before=$(stty -g)
cargo test --locked --lib tui::tests::manual_single_line_input_terminal -- --ignored --exact --nocapture --test-threads=1
code=$?; printf 'cargo exit=%s\n' "$code"
[ "$(stty -g)" = "$before" ] && echo 'termios unchanged'
```

stdin/stdout両方がTTY必須です。pipe、リダイレクト、通常の自動test経由ではIME受入できません。
入力は最初からfocusされています。Tab/Shift+Tabで欄と編集外を切替、Escで編集外へ移動し、
編集外のqで正常終了します。Ctrl+Cはfocusにかかわらず中断します。
手動test自身は中断130で終了しますが、Cargo経由の終了コードはCargoが非0へ変換する場合があります。
通常 `bl` のq=0/Ctrl+C=130契約は別途既存PTY回帰で確認します。

## 再実施用の観測手順

以下は詳細な再実施手順です。今回のユーザー確認では操作を分けて案内し、実施した範囲を下表に記録しました。
本節の全操作を手動実施済みとは扱いません。

1. 100x24以上で起動し、日本語IMEで「日本語入力確認」を**未確定のまま**入力する。
   未確定文字・変換候補が実cursorの入力位置に追従し、確定後に原文を編集できることを目視する。
   OS/IMEがアプリ受信前に描く未確定文字の秘匿は保証しない。
2. `日本 é 👩‍💻`（e + combining acute、ZWJ emoji）を入力/貼付けし、左右/Home/End・Backspace/Deleteで
   結合文字やemojiがgrapheme単位で移動/削除されることを確認する。
   q/h/l/j/kが普通の文字として入力され、Ctrl+H（削除）等の編集キーがglobal操作にならないことも確認する。
3. 長い日本語/結合文字/emojiの行を貼付けて末尾・中間・先頭へ移動する。
   水平scrollで実cursorが可視欄内にあり、40x12への縮小・拡大後も値と編集位置が維持されること。
   Tabで編集外へ移りcursorが消え、Shift+Tabで戻ると続きから編集できること。
   39x12や40x11では案内とq/Ctrl+Cのみ。文字/貼付け/削除/Tabを試し、40x12へ戻して値・focus不変を確認する。
4. 別shellで `printf 'first\r\nsecond\rthird\nfourth\tend' | pbcopy` を実行して貼付ける。
   `first second third fourth end` の単行になること（CRLFはspace一つ、CR/LF/TABもspace、trimしない）。
5. 通常の長文の末尾へfake secret `test-secret` を加える。
   入力全体が `[REDACTED]` の固定表示となり、cursorは欄左端に固定されること。
   Home/Endでsecretを横scroll外に移しても、一部の通常文字が表示されないこと。
   End後Backspaceを11回でsecretだけを除くと元の通常長文が戻ること（表示変換で原文を壊していない）。
   Tabでfocusを外しても伏字を維持すること。
6. 別shellで `printf '保持\033]52;c;fake\007後続' | pbcopy` を実行して貼付ける（fake値のみ）。
   全欄伏字・cursor固定になり、外部OSC命令を実行しないこと。
   Ctrl+Uで入力を消すと通常入力へ戻れること。原文をログへdumpしない。
7. Esc→qで終了、再起動して入力中Ctrl+C、別shellからSIGINTも試す。
   それぞれraw/alternate screen/bracketed paste/cursorが復元され、通常shellで入力/削除/貼付けできること。
   termios比較・終了コードを記録する。NO_COLOR=1でも再確認する。

## 記録

| 項目 | #14の記録 |
| --- | --- |
| 実施日 / 実施者 | 2026-10-28 UTC / ユーザー操作・確認、Root案内・記録 |
| macOS / CPU | macOS 26.7.1（25G241）/ arm64（Root取得） |
| 端末名 / 版、TERM / LANG / NO_COLOR | Herdr 0.8.0 / Ghostty 1.3.1（Root環境で版を確認）、専用タブ `#14 IME手動確認`。RootのTERMはxterm-256color。対象ペインのLANG/NO_COLORは個別取得できず、NO_COLOR=1での追加手動実行は未実施 |
| IME名 / 版・入力モード | macOS標準の日本語入力（ユーザー回答）、OS版は上記。独立したIME版・ローマ字/かな等の詳細モードは未取得 |
| revision / 差分 / 対象test binaryのSHA-256 | HEAD `d57e2e2c182f946462a2b32e130e51f945aa6718` + #14未コミット差分（grapheme補正後）。`target/debug/deps/backlog_cli-2a9da4e820b7bd33` SHA-256 `f6b43e0ba07fba64d1cf1da14477ccd0df89eb80df3401f91630f64ab8b1bdf9`、実施後も一致 |
| 日本語未確定文字 / 候補のcursor追従 | **ユーザー確認OK**。「1は確認した」と報告。Herdr新規タブの修正後test binary（SHA-256 `f6b43e0ba07fba64d1cf1da14477ccd0df89eb80df3401f91630f64ab8b1bdf9`）で実施。環境と他の手順の結果は本表の各行を参照 |
| 日本語 / 結合文字 / ZWJ emojiの見え方 | **手順2はユーザー確認OK**。`日本é👩‍💻` を貼り付け、Backspace 1回で `日本é`、さらに1回で `日本` となることを確認。Forward Delete・Ctrl+H等の個別手動操作は未実施で、自動入力試験と区別 |
| 長文scroll / resize / focus / 伏字の目視 | **手順3の長文scroll・resize・focusと手順5の伏字はユーザー確認OK**。欄を超える反復貼付け、Ctrl+A/Eの先頭/末尾移動、Tab/Shift+Tab往復、ウィンドウ縮小/復帰で入力保持を確認。`普通の文字test-secret後ろの文字` が全欄 `[REDACTED]`・左端固定cursorとなり、左右/Ctrl+A/E・focus外でも原文非表示、Ctrl+U後は通常入力へ戻ることを確認。正確な40x12/極小境界の手動操作は未実施で、TestBackendによる境界試験と区別 |
| 複数行貼付け | **ユーザー確認OK**。3行の `first` / `second` / `third` を貼り付け、`first second third` の1行表示を確認。実端末でのCRLF/CR/TAB個別操作は未実施で、自動試験の結果と区別 |
| 危険制御文字の貼付け | **手順6はユーザー確認OK**。明示許可を得て `保持\033]52;c;fake\007後続` を `pbcopy` で用意し、ユーザーが実TUIへ貼付け。全欄 `[REDACTED]`・左端固定cursor、Ctrl+U後の通常入力復帰を確認 |
| 終了 / 中断後の通常shell操作 | **Esc→qと入力中Ctrl+Cの両方でユーザー確認OK**。shell復帰後の `echo shell-ok` と入力/削除を確認。RootもHerdrのforegroundがzshであること、通常終了は `cargo exit=0`、中断は子process 130 / `cargo exit=130`、いずれも `termios unchanged` の実出力を確認。外部SIGINT・NO_COLORの追加手動実行は未実施で、自動PTY/回帰試験と区別 |

自動検証では `tests/tui_input.rs` のTestBackend 5件と、入力shellのキー衝突/PTY 2件が成功。
既存shell/TestBackend・実blプロセス・復元失敗/初回SIGINT/blocking処理の回帰も成功。
PTYで実Crossterm bracketed pasteの受信→全欄伏字→Esc/q=0・Ctrl+C=130→termios/端末復元を確認した。
これらはOSの実IME/端末の目視結果ではなく、ユーザー確認と混同しません。未実施と記載した個別操作の代替にはしません。

### #14 自動検証実測（2026-10-28 UTC）

| チェック | 終了コード / 結果 |
| --- | --- |
| 関連 `tui_input` / `tui_shell` / `tui_process` | 0 / 5 + 5 + 7件成功 |
| `cargo test --locked --lib tui::tests:: -- --nocapture` | 0 / 6件成功、実IME用1件ignored（別途自動PTY子processとしての起動は実施） |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0 / 189件成功、実IME用1件ignored。最初の実行はrunnerの180秒制限で中断し、420秒制限で再実行して完走 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / 警告なし |
| `cargo audit --file Cargo.lock`（cargo-audit 0.22.2） | 0 / 224依存・1381 advisory、警告/脆弱性報告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |

監査前後のCargo.lock SHA-256はともに
`0b5c7a872566c900fcbbd060bb07ddd1760eb69127359c11dcba5fc43c16c4d6` で不変。
依存差分はtui-input 0.15.5の追加とその依存辺のみで、既存crateの版/featureは変更していない。
この結果は当該時点の自動検証であり、この時点では実IME/実端末目視も未実施でした。後の手動結果は上表に記録しています。GitHub Actions・実サービス接続/書込みは行っていません。

### #14 隣接grapheme結合の修正後実測

paste/通常文字挿入と削除で隣接文字が結合した後、完成原文のgrapheme末尾へcursorを揃える。
既存境界での移動・全体削除、combining/ZWJ/regional indicator、幅1描画、resize/focusでの原文保持、
control前後の編集と連続cut/yankを公開 `SingleLineInput` → 状態/TestBackendで確認した。

| チェック | 終了コード / 結果 |
| --- | --- |
| 修正前 `cargo test --locked --test tui_input single_line_input_aligns_joined_graphemes_after_paste_and_character_insertion` | **101 / RED**、cursor表示列の期待値との差を検出 |
| 同じtestの最小修正後再実行 | **0 / GREEN** |
| `cargo test --locked --test tui_input` | 0 / 8件成功（上記5件に回帰3件を追加） |
| `cargo test --locked --lib tui::tests:: -- --nocapture` | 0 / 6件成功、実IME用1件ignored |
| `cargo test --locked --test tui_shell --test tui_process` | 0 / 5 + 7件成功 |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / 警告なし |

境界補正は変更応答のあるeventだけ既存Inputをcloneして判定するため、入力長に比例するcopyがある。
Workerの修正時点では全対象test・audit・release buildと実IMEは未確認でした。その後Rootが修正後のコードに対して以下を再実行しました（すべて終了0）。

- `cargo fmt --all -- --check`
- `cargo test --locked --all-targets`：192件成功、通常の自動実行では実IME用1件ignored。
- `cargo clippy --locked --all-targets -- -D warnings`：警告なし。
- `cargo audit --file Cargo.lock`：cargo-audit 0.22.2、224依存・1381 advisory、警告/脆弱性報告なし。監査前後lockfileハッシュは上記と同じで不変。
- `cargo build --locked --release --target aarch64-apple-darwin --bins`
- `git diff --check`

Rootの直接probeでもcombining/ZWJ/国旗の隣接結合後の表示列・全体削除・幅1非panicを確認。
修正後の実test binaryをPTYで直接起動し、paste受信/全欄伏字/q=0・Ctrl+C=130/復元も再確認しました。
Correctness ReviewのP1を採用して修正し、fresh再レビューで解消と追加指摘なしを確認。独立Ponytail Reviewも追加指摘なし。
実IME/実端末目視は、その後上表のとおりユーザー確認を完了しました。確認範囲で入力部品を採用し、検索/フォーム等の後続Issueを実装済みとは扱いません。
