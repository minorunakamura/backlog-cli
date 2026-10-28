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

## 選択追従プレビュー・Issue Detail・Ctrl+O (#16)

状態: **自動TestBackend/PTY・release製品binaryの直接ローカルHTTPS検証済み。実端末確認と通知修正後のRoot検証は末尾の記録を参照**。
以下は初回worker時点の履歴です。#13/#14のユーザー確認を#16の受入へ流用せず、実ブラウザ・実Backlog接続・書込みは行っていません。

| 項目 | 記録 |
| --- | --- |
| 確認日 / 実施者 | 2026-10-28 UTC / Implementation worker、自動実行 |
| macOS / CPU / toolchain | macOS 26.7.1（25G241）/ arm64 / rustc 1.99.1 |
| 対象build | HEAD `9970424` + #16未コミット差分、`cargo build --locked --release --target aarch64-apple-darwin --bins` |
| release `bl` SHA-256 | `e028a069fbd5e76c59e438f417a6fc381c5070b3684f384fa70fad4740732703` |
| 端末 / TLS / browser | Python標準PTY、100x24→40x12→100x24。localhost自己署名証明書を子processだけでtrust。fake key。PATHの`open` stub、実ブラウザ非起動 |
| 直接実測 | `code:0`、`restored:true`、stderr空、約1.87秒、11 snapshots、9 GETのみ。`open`のargvは引数数1と設定originの`/view/B-3`のみ |

再実施は [CONTRIBUTING.md の #16 手順](../../CONTRIBUTING.md#プレビュー詳細とブラウザ-16) を使用します。
`tests/fixtures/tui_detail.json` / `tui_detail_steps.json` を既存HTTPS server・PTY driverに渡し、test binaryではなくrelease `bl` を直接起動しました。
HTTP記録はspace/projects/count/list、B-3初期GETとproject記法GET、R失敗のB-3 GET、次のR成功のB-3/project GETのみです。
連続選択でB-1/B-2を取得せず、Enterで追加GETせず、末尾30行目へ移動、40x12のGでも末尾へ到達、
R失敗時の末尾保持、次のR成功時のKEY先頭への交換、Backlog記法変換・secret伏字、Ctrl+O失敗後のB-3選択保持を確認しました。
raw出力にfake key原文や外部OSCはありませんでした。

| 検証 | 終了コード / 結果 |
| --- | --- |
| `cargo test --locked --test tui_process preview_debounces_selection_and_enter_reuses_safely_rendered_detail -- --exact`（実装前→実装後） | 101 RED（未実装でプレビュー待ちtimeout）→0 GREEN。最終版はキー送信/HTTP時刻差で約150ms（120ms〜1秒内）、最終対象だけの通信、全項目/安全表示を照合 |
| `cargo test --locked --test tui_process preview_cancelled_before_debounce_has_no_request_or_permanent_loading -- --exact`（取消状態対応前→後） | 101 RED（取消後もLoading表示）→0 GREEN。取消後は対象キーと取消表示、通信なし |
| `cargo test --locked --test tui_process --test tui_shell --lib` | 0 / その後追加した同じfixture試験も含め最終all-targetsで再確認 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0 / 216件成功、実IME専用1件ignored。初回180秒実行枠では途中timeoutし、420秒枠で最初から再実行して完走 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / 警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| release `bl` + HTTPS/PTY（上記再実施手順） | 0 / 上記実測。最初の外部検証assertは40列時の伏字の折返し位置を誤って指定し1、100列へ戻したsnapshotで再照合して0、さらに一式を再実行して0 |

追加の自動試験では、全scrollキー・右側h/l/Tab/Shift+Tab/Esc、Unicode wrap/resize/clamp、Rのissue失敗/project記法失敗での原子保持、
同対象の逆順成功/失敗・選択変更・画面閉鎖/取消後の古い応答で新busyを壊さないこと、別課題応答の非表示、
ブラウザの不正listキー/secret URL拒否・任意API URL非使用・失敗時選択保持を確認しました。
一覧操作の既存試験はプレビューGETを独立に数え、`request_counts`（全通信）と`browse_request_counts`（一覧操作）を区別しています。
`Cargo.lock` はHEADと同じSHA-256 `0b5c7a872566c900fcbbd060bb07ddd1760eb69127359c11dcba5fc43c16c4d6` で不変です。
監査はRootの全体チェックへ委ね、このworkerでは実行していません。

初回worker時点の未実施: 実端末アプリでの#16の目視、OSブラウザ起動/ログイン、実サービスE2E、GitHub Actions。後続の実端末確認は末尾に記録します。
確認する場合は上記一時環境でrelease `bl` を直接TTY起動し、100x24/40x12で全項目/本文への移動・resize・R保持/交換・終了後の通常shell利用を目視してください。
stubを外して実ブラウザを起動する場合も、接続先はこのローカルfixtureのみとし、実Backlogへ無断で接続/書込みしないでください。

既知制限: 固定Ratatui版のunstable行数APIを使用します。スクロール開始位置の上限は65,535折返し行で、
それを超える本文末尾へは到達できない場合があります（原文は不変）。上限でoverflow/panicしないTestBackend試験があります。
PTY driverは簡易VT復元であり、Unicodeの実字幅やIME/目視を保証するものではありません。

### #16 通知表示のユーザーフィードバック対応

ユーザーの実端末フィードバックで、Ctrl+O失敗がProject側に表示されること、Detail R失敗の上部一行を見落とし旧本文を最新と誤認しうることが判明しました。
この節は通知修正worker時点の自動検証記録です。中央popupの見た目と修飾なしEnter/Esc/Spaceのdismiss-onlyはユーザーが実端末で確認済みです。当時未確認だった通常操作による通知消去も、末尾のRoot最終検証で確認しました。前節の修正前binaryのhash/実測は履歴として残します。

再取得/ブラウザの失敗は共通の安全化済みメッセージ1件を中央の枠付きpopupへ表示します。ERROR見出し/閉じるbuttonはなく、赤/太字、NO_COLORでは色なしの枠/太字です。
通常サイズでは修飾キーなしEnter/Esc/Spaceは通知だけを閉じます。次のキーから通常の遷移/取消/scrollを行います。
その他の通常dispatchに進むキー（j/k/h/l/R/Ctrl+O、既存のShift付きキー、未割当の文字）は通知を消してから本来の動作を実行します。Rが再び失敗すれば新しい通知を表示します。
無視するmodifier・Release・時間経過・resizeでは消えず、極小時は安全操作制限を維持して通常サイズへの復帰まで保持します。qは終了、Ctrl+Cは130を優先します。
初期通信エラーの致命的終了、失敗時の本文/scroll/選択保持、世代/busy/cancel、Detailの安全化済みcopy借用は変更していません。

以下は初回の中央popup/dismiss-only修正時の記録です。通常操作時の通知保持という当時の期待値は、末尾の追補で置き換えています。

| 検証 | 終了コード / 結果 |
| --- | --- |
| `cargo test --locked --test tui_process ctrl_o_uses_only_origin_and_selected_key_without_shell_and_failure_keeps_selection -- --exact`（修正前→後） | **101 RED→0 GREEN**。修正前は通知時EnterがDetailへ遷移して次step待ちtimeout。修正後は中央枠/単一表示/ERROR見出しなし、Enterは通知だけ閉じ、次EnterでDetailへ入る |
| `cargo test --locked --lib tui::tests::` | 0 / 10件成功、実IME専用1件ignored。新規2件で40x12/100x24の中央枠/日本語折返し/赤太字/NO_COLOR、Enter/Esc/Space消費、修飾/release/repeat、本文scroll保持・次キーの通常動作・極小復帰を確認 |
| `cargo test --locked --test tui_shell --test tui_process` | 0 / 5 + 30件成功。新規PTY試験で取消済みの遅い応答による通知非破壊、通知中の選択変更、100x24→40x12→39x11→40x12の通知保持とCtrl+C=130/端末復元。Detailのissue/project記法失敗で本文/scroll保持、R通常実行、dismiss後の一覧保持も確認 |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / 警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| `git diff --check` | 0 |

途中の回帰実行では、一覧用fixtureのpreview未定義による意図しない404通知と、popupで隠れる行のassert/旧dismiss手順の不整合を検出しました。
一覧fixtureには正常preview応答を補い、意図した再取得失敗の保持assertはdismiss後も確認するよう整合しました。最後の上記実行で全関連試験が成功しています。
`tests/fixtures/tui_detail_steps.json` は今回の追補でR失敗後のRが通知を消して再取得し、成功後はEnter不要で本文移動する手順へ整合しました。自動sequenceは固定手順用であり、自由な手動操作では応答段階を固定してください。

初回の中央popup修正workerでは修正後の全対象test・audit・release binary直接HTTPS/PTY再実測・実端末目視・実ブラウザ・実Backlog接続・GitHub Actionsは未実施でした。
新規依存/config/永続化/タイマーはありません。長文通知は画面内に収めて折返し、画面を超える部分の独自pagerは設けません。

#### 通常操作時の通知消去契約の追補

初回の説明と実装の不一致を修正しました。製品側の変更は `Shell::key` のCtrl+O分岐内と既存modifier guard通過後で通知を消す2行だけです。
修飾なしEnter/Esc/Spaceのdismiss-only、拒否modifier/Release、Ctrl+C優先、極小制限、世代判定、失敗時の本文/scroll保持は維持しています。

| 今回の検証 | 終了コード / 結果 |
| --- | --- |
| 新規TestBackend通常scroll/focus回帰（追加当初 `error_popup_clears_before_normal_scroll_and_focus_keys_execute`） | **101 RED→0 GREEN**。旧実装ではjの通常scroll後も`Failed:`が残って失敗。2行のclear追加後は成功。その後Ctrl+Oと拒否modifier/Release保持も同じ試験へ追加 |
| `cargo test --locked --lib tui::tests::` | 0 / 11件成功、実IME専用1件ignored。3キーのdismiss-only、j/kのscroll、h/lのfocus移動と通知消去、Ctrl+O、Shift+Enterの通常遷移を確認 |
| `cargo test --locked --test tui_shell --test tui_process` | 0 / 5 + 30件成功、filterなし。Rで旧通知を消した取得中画面と再失敗の新通知、失敗時本文/scroll保持、R成功後はdismiss不要、j選択変更で消去、再度Ctrl+Oの新通知、古い応答非破壊・resize/極小保持・Ctrl+C=130/端末復元を確認 |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / 警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| `git diff --check` / staged filesなしの確認 | 0 / staged filesなし |

今回workerでは全体test・audit・release binaryの直接HTTPS/PTY実行・実端末の再受入は再実施していません。関連試験はdebug製品binaryのHTTPS/PTYを含み、release直接検証と独立reviewはRootに委ねます。
実Backlog/実ブラウザ/Herdr/GitHub操作、commit/push、Plan変更、依存追加は行っていません。

### #16 Rootの最終検証・実端末確認（2026-10-28 UTC）

対象はHEAD `9970424` + 今回の未コミット差分です。最終release `bl` SHA-256は
`b6c131a8c31a51b3b02b97f6a275e7c9a009d1ef69ae5c4362091c2f3e202c42`。
macOS 26.7.1（25G241）/ arm64 / Rust 1.99.1、Herdr 0.8.0 / Ghostty 1.3.1、専用タブ「#16 詳細・プレビュー確認」で確認しました。

| 最終差分に対するRootチェック | 終了コード / 観測 |
| --- | --- |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0 / 221件成功、実IME専用1件ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / 警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| `git diff --check` | 0 |
| `cargo audit --file Cargo.lock`（cargo-audit 0.22.2、通知popup修正後） | 0 / 224依存・1381 advisory、警告・脆弱性報告なし。後の通常入力clear追補は製品2行で、manifest/lockfile不変 |
| release製品 `bl` + ローカルHTTPS/PTY | 0 / 11 snapshots、9 GETのみ、stderr空、端末復元成功。Enter再GETなし、40x12/100x24のscroll/resize、R失敗時保持・次Rで通知消去と成功交換、secret/OSC非表示、安全なbrowser argv・失敗時選択保持をRootが直接assert |

lockfile SHA-256は監査前後・最終確認とも
`0b5c7a872566c900fcbbd060bb07ddd1760eb69127359c11dcba5fc43c16c4d6` でHEADと同じです。
Correctness / Ponytailの独立fresh Reviewを実施し、通知popup修正後も両方のfocused Reviewで追加指摘なし。
採用したPonytail P2の本文copy/再安全化の重複はCow借用とParagraph共有で解消しました。
その後の通常入力clear 2行はRootが呼出し経路・回帰テスト・最終全対象test・直接操作で確認し、全レビューの無条件再実行はしていません。

| 実端末での観測 | 実施者 / 結果 |
| --- | --- |
| 詳細項目・日本語/結合文字/emoji本文 | ユーザーが初回手順1をOK確認 |
| 縮小後の本文 | 初回に本文消失の報告があり、リクエスト回数で進むsequence fixtureが自由操作時にも本文を変更していたため、手動用は各課題の30行本文を固定。約109列×37行でRootがBODYと01〜11行を読み取り、ユーザーも「今は本文が見えている」と確認。初回現象の原因は断定せず、この再確認と区別する |
| R失敗の本文末尾保持・成功時の先頭交換 | Rootが応答を固定500→固定成功へ明示切替し、ユーザーが各OK。Rootも失敗通知/30行目保持、成功後の先頭と伏字本文を確認 |
| ブラウザstub失敗時の選択保持・終了後のshell操作 | ユーザーがOK確認。実OSブラウザは起動せず、`echo shell-ok` は利用者がshellへ入力 |
| 中央popupの見た目・Enter/Esc/Space | popup版release SHA-256 `30d991fc3aa4b041afab83b68bd09f0cbe561ee7841e273f4dae96c93bb0b6a5` でユーザーが1〜3すべてOK。RootもANSIの赤太字・見出しなしを確認 |
| 通常操作時の通知消去（最終2行追補後） | 最終releaseをRootが同じ実Herdrタブで直接操作。200行の固定本文で、jが通知消去と1行scroll、lが通知消去とComment focus移動、Rが旧通知を即消して通信中表示→成功先頭交換、一覧B-3選択保持を確認。ユーザーの再目視確認とは区別する |
| 最終binaryの終了・一時環境のcleanup | Rootの直接操作でexit 0 / `termios unchanged=True`。launcherがローカルserverを停止し、fake key設定/TLS/応答の一時rootを削除。専用Herdrタブは通常shellへ戻した |

実端末で正確な40x12/極小境界の目視は行わず、その境界はTestBackend/PTYで検証しました。新たな実IME受入、実OSブラウザ起動・ログイン、実Backlog接続/書込み、GitHub Actions、配布物作成/公開は未実施です。
実端末の最終通常入力確認にはRoot専用の一時launcherと固定応答の段階切替を用い、製品のdebug flagや永続設定は追加していません。
