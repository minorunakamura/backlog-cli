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

## 検索ダイアログ (#17)

### workerの実装・自動観測（2026-10-29 UTC）

対象はHEAD `88ffbe7` + #17未コミット差分です。#16の受入記録は変更していません。
通常4条件の検索・専用横断検索を実装し、単行入力をkeywordへ組み込みました。
共通Service/API/設定schema/依存/Planは変更していません。実key・ユーザー設定を読まず、
実サービスへの接続/書込み、GitHub操作、commit/push/merge/release/deployは行っていません。
**この節は自動観測と再現手順であり、#17の必須実Backlog/実IME受入の成功記録ではありません。**

TDD seamは既存の実 `bl` の入力→描画/PTYとHTTPS送受信です。private task/function形状には固定していません。
各行は一つの失敗試験→必要な実装→同コマンドの成功を実測しました（redはすべて終了101、greenは0）。

| `cargo test --locked --test tui_process <filter>` のfilter | red時の観測 / greenの確認 |
| --- | --- |
| `search_dialog_edits_keyword` | dialogが開かず待機timeout → 初回status/keyword文字衝突・clear/reopen/R、count/listは適用時だけ |
| `search_dialog_multiselects` | candidate欄未実装で待機timeout → 4条件OR/AND・任意精度ID・count/list同query。同filterで全条件summary非表示のred→greenも実施 |
| `cross_project_dialog` | Ctrl+A未実装 → 空白拒否・keyword/self・既定値非流用・A/B両project結果・通常復帰・c案内 |
| `search_dialog_compact_wraps` | 40x12で長い条件末尾へ到達不可 → PgUp/PgDnで全文・focus移動でkeywordを再表示、candidate移動と日本語原文保持 |
| `search_dialog_first_open` | 初期count失敗後にdialogが開かない → 設定値を使って再適用可能 |
| `search_dialog_edits_keyword`（キー案内追補） | keyword中に誤ったq終了案内 → modalの案内から除外。qは文字のまま |
| `search_dialog_space_deselects` | 重複した設定statusが1回のSpaceで解除されない → 既定値重複を除き、解除適用後もstatus再挿入なし |

追加の回帰は失敗count/list・取消時の旧page 2/選択/履歴とdraft保持、遅延candidate/applyの成功/失敗による
新draft/busy非破壊、candidate失敗の明示/retry、送信中の編集/二重適用拒否、貼付けのspace正規化・全欄伏字/送信原文保持、
Alt/Ctrl+Enter非適用、40x12/39x11復帰、Ctrl+C優先130/端末復元を確認しました。
既存preview/detail/browser/paging/R/NO_COLORの試験も維持しています。

| チェック | workerの観測 |
| --- | --- |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 再開後0 / 20 suite、234件成功、実IME専用1件ignored。中断した前回実行とは区別し、下記追補に実測を記録 |
| `cargo clippy --locked --all-targets -- -D warnings` | 初回101: 新規2箇所のcollapsible-if。修正後0、警告なし |
| `cargo audit --file Cargo.lock` | 0 / cargo-audit 0.22.2、224依存・1381 advisory、警告/脆弱性報告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0、両binaryはMach-O arm64 |
| `git diff --check` / stagedなし | 0 / stagedなし |
| release製品 `bl` + ローカルHTTPS/PTY | 0 / 19 snapshots、28 GETのみ、count/list各5回でcount/offset以外のquery一致、stderr空、復元成功、secret出力なし |

最初の全対象test実行は233成功/1 ignoredで、取消された接続に対するPython HTTPS fixtureの
`BrokenPipeError` tracebackが1件出ましたが、Cargo終了0・該当試験成功でした。サーバーへのintentional cancellationと
製品のstderrを区別し、警告を隠していません。最終再実行はworker timeoutで中断し、234件成功という先行記載は未確認だったため訂正しました。再開後の完走結果は下に追記します。
lockfileのSHA-256は監査前後とも `0b5c7a872566c900fcbbd060bb07ddd1760eb69127359c11dcba5fc43c16c4d6`、HEADと同じです。

直接再現はCONTRIBUTINGの「検索ダイアログ (#17)」にある一時root/server/subshellを実行してください。
`tests/fixtures/tui_search.json` と `tui_search_steps.json` はdebugの自動試験でも同じ手順を実行します。
最初は通常Bの設定open、次にkeyword+2状態+Alice+巨大ID v1、clear/reopenを40x12で確認、
self横断でA/B結果、cの案内、具体BのEnterで通常openへ戻ります。
このsequence fixtureは固定手順用です。自由操作の実端末確認では応答を固定してください。
PTY簡易VT復元は日本語の間に空白が見える場合があり、実字幅/IMEの保証ではありません。
条件全文scrollはParagraphと同じ65,535折返し行までで、超える部分の末尾到達は未保証（原文不変）です。

### #17 必須の実Backlog・実IME/キー衝突の受入（未実施 / Root・ユーザーが実施）

接続先は承認済みPlanどおり **`https://lazybl.backlog.com/`**。
利用者のterminal内でprofileのspaceUrlがこのoriginか確認し、準備済みのconfig/API keyを使用してください。
keyやconfigの内容をこの記録・画面capture・会話へ貼らないでください。設定の変更や課題/コメントの書込みは不要です。
対象は最新の改訂実装検証節に記録した変更後release `bl` を直接TTY起動します。実施時のbinary SHA-256・macOS・terminal版・IME・
profileの非機密識別子・対象project/課題の非機密識別子・実施者・日時を追記してください。

1. Projectの具体projectをEnterで確定。Issueへ移り `/`。初期focusはキーワード、初回は設定status。
   Tabで状態→担当者→マイルストーン→キーワード、Shift+Tabで逆順に移動し、実候補を取得する。
   欄外だけCtrl+K/S/A/Gで直接focus。候補popupは親内・最大60x10、元の項目配置は動かないこと。
   上下/j/k・Spaceで複数選択/解除。候補Enter/Escは閉じるだけ、閉鎖後Space/上下/j/kは再表示だけ。
   表示内の移動で先頭行が変わらず、上下端を越えた時だけ最小scrollすること。
2. 非機密keywordと候補を選び、キーワードまたは閉じた項目で修飾なしEnter。count/list両成功後だけ
   Issueへfocusし、total/結果・`K:/S:/A:/G:`が切り替わること。失敗/取消は旧一覧・選択・履歴を保持する。
   候補失敗中Enterは再取得だけ、成功した空候補は失敗と区別する。Tab周回/閉鎖再表示/clearで成功候補を再利用する。
3. キーワード欄外でCtrl+U。focusを保ったままpopupが閉じ全条件が空になること。Enterで適用し、
   `/`とRでも設定statusが戻らないこと。keyword内のCtrl+Uではkeywordだけを削除し、他の条件は維持する。
   別draftをEsc取消し、再度開くと最後の適用値だけが残ること。dialog閉鎖後は候補を新規取得する。
4. ProjectのCtrl+Aで横断検索。Tab/Shift+Tabはkeywordと担当者「自分」だけを循環し、Ctrl+S/Gは無効。
   keywordか自分で適用し、Issue/Detailに結果を表示すること。空白のみかつ自分なしは失敗で旧一覧を保持。
   横断画面とcはh/lまたはTab→Project→具体project Enterを案内し、count/list両成功後だけ通常復帰/作成先確定する。
   cは書込みなし、nは次ページ。**複数projectの実サービス確認は環境制約で対象外**、自動試験で確認する。
5. keywordで `qhljkRc/` が文字になり、Ctrl+A/E/B/Fの移動、Ctrl+K/U/W/Hの削除とCtrl+Y貼戻しを確認。
   Ctrl+S/G等を入力中に横取りしない。欄外Ctrl+Fは何もしない。100x24で中央72x18、40x12で38x10、
   極小ではq/Ctrl+C以外の編集/適用を拒否し、復帰後のfocus・値・cursorが保たれること。
   長い日本語/結合文字/ZWJ emojiの候補と複数選択済み値を左右で末尾まで確認し、選択印が常に残ること。
   IME未確定文字/変換候補が実cursorへ追従し、**変換確定Enterでは検索せず、次Enterで一度だけ検索**すること。
   候補変更・取消・再変換と英数字後Enterも確認。Crosstermにcomposition情報はなく、端末側処理に依存する。
   誤適用した環境では未完了・停止して操作方式の判断へ戻す。自動PTY/raw tty Probeを代用しない。
6. ローカルfixtureのfake文字列だけでpaste CRLF/TAB正規化・全欄伏字を目視し、**実keyは貼らない**。
   Alt/Ctrl+Enterは非適用（端末が区別できる場合）。適用中は編集/移動/clear/重複Enterを拒否し、Esc取消は可能。
   Ctrl+Cは編集中/適用中も130で終了し、通常shellへ復元すること。OS/IME受信前の未確定文字の秘匿は保証しない。

現時点の結果: **すべて未実施**（Root/利用者の準備中）。workerの自動PTY成功や#14/#16の過去の実端末成功を
この#17の実Backlog/実IME受入へ流用していません。通信障害/逆転/複数projectはローカルHTTPSで確認し、
実サービスへ意図的な障害・過大負荷は発生させないでください。必須確認の完了と独立reviewまでは#17を完了/merge可能扱いにしません。

### #17 worker再開後の最終ローカル検証（2026-10-29 02:28 UTC）

ユーザー承認による同じworkerの再開後、中断時点の差分を保持し、未確認だった234件成功の先行記載をまず訂正しました。
追加機能や探索的拡張・製品コードの変更はせず、以下を最初から完走しました。

| 実行 | 終了コード / 実測 |
| --- | --- |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | **0 / 20 suite、234件成功、1 ignored**。全suite完走、warning/fixture tracebackなし |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / 警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 / 最後の既定値重複解除変更を含む、両binary Mach-O arm64 |
| 変更後release製品のローカルHTTPS/PTY直接検証 | 0 / 19 snapshots、28 GETのみ、count/list各5回でquery一致、stderr空・端末復元成功・secret出力なし |
| `git diff --check` / stagedなし / Cargo・Plan差分なし | 0 / stagedなし、Cargo.toml・Cargo.lock・Plan不変 |

コマンドごとの終了コードは `/tmp/issue17-resumed-exits.log`、全文出力は
`/tmp/issue17-resumed-{fmt,all-targets,clippy,build,direct}.log`、直接PTY結果は
`/tmp/issue17-resumed-release-pty.json` に記録しました（一時ログは永続成果物ではありません）。
直接再現手順はCONTRIBUTINGの上記subshellと同じです。検証用のserver/一時TLS/fake設定は終了時に削除しました。

最終release SHA-256:

- `bl`: `1c5cede0161009baf5028b4ed3ea00ca3ebe9bd224488b5fd2a2e43a530d8209`
- `bl-cli`: `5e604da3e59ff3636b7754bf9fe9c1afdfedb100de874f90fde61d34fd3b76ca`

監査はCargo不変・前回成功ログありのためRoot指示どおり再実行していません。
利用者terminalで実BacklogへのCLI接続成功がRootから報告されていますが、workerは実key/configを読まず、
サービスへの接続/書込みも行っていません。このCLI成功を#17のTUI実受入へ代用しません。
**残件はRoot/ユーザーによる上記TUIの実Backlog・実IME/キー衝突受入と独立reviewです。#17全体の完了・merge可能宣言はしていません。**

### #17 accepted review findings の修正・再検証（2026-10-29 02:54 UTC）

上記02:28 UTCのbuildと検証は**修正前buildの履歴**として保持しています。
その成功結果・SHA-256を今回の変更後buildの検証へ流用していません。利用者による旧buildの手動確認は
Rootから一時停止を案内済みです。今回の対象は次の4件だけで、Service/Cargo/設定仕様/Planは変更していません。

| 受理済み指摘 | 公開PTY/HTTPS境界でのred → green |
| --- | --- |
| 初回count失敗後の既定status解除 | `search_dialog_first_open` が `[x] open` 待機timeout（101）→ 一意な候補名だけIDへ整合、1回のSpaceで解除しcount/list双方にstatusIdなし（0）。既定名が曖昧/不在なら未解決のまま明示エラー、検索非送信も確認 |
| Project未確定のhighlightから検索できる | `search_dialog_cannot_search` と、初回失敗後highlightをB→Aへ動かす `search_dialog_first_open` が各101 → 確定済み対象だけをfallbackに使い各0。設定なし・Enterなしの `/`/Ctrl+Fは検索なし、確定済みBの失敗後はhighlightがAでもBの再編集/適用が可能。適用済み検索がある場合は別project確定失敗後も元projectが優先する回帰も0 |
| selfの表示 | `cross_project_dialog_` が「自分」候補待機timeout（101）→ 候補/適用済み条件の表示だけ「自分」、内部の `Selection::Name("self")` を維持しmyself ID 7で送信（0） |
| resize後のfocus行が非表示 | `search_dialog_resize_reveals` がMilestone待機timeout（101）→ 100x24でCtrl+G後40x12へ縮小して `>Milestone: Any` を表示（0）。39x11で編集/適用を拒否、復帰後もfocus表示・日本語keyword原文/挿入cursor位置を保持し、送信値 `日本語Xqhljk` をcount/listで確認 |

redは各sliceの最小製品修正より前に実行しました。日本語の簡易PTY復元は全角文字間に空白/旧描画の文字が
残る制限があるため、candidate待機は `[ ] 自 分`、適用済み条件は「Assignee: 自」・「分」の表示と
`self` 非表示を確認しています。実端末字幅/IMEの保証に置き換えていません。

| 変更後の実行 | 終了コード / 実測 |
| --- | --- |
| `cargo test --locked --test tui_process search_dialog_` | 0 / 16成功 |
| `cargo test --locked --test tui_process cross_project_dialog_` | 0 / 1成功 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0 / **20 suite、238成功、実IME専用1 ignored**。warning/fixture tracebackなし |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / 警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 / 両binary Mach-O arm64 |
| 変更後release `bl` の直接HTTPS/PTY fixture（CONTRIBUTING #17のsubshell + 結果assert） | 0 / 19 snapshots、28 GETのみ、count/list各5回でcount/offset以外のquery一致。「自分」候補とassigneeId=7、通常/複数条件/clear/横断/通常復帰を確認。stderr空・復元成功・secret出力なし、fixture server stderrも空 |
| `git diff --check` / stagedなし / Cargo・Service・Plan差分なし | 0 / stagedなし、対象外の既存差分を保持 |

今回のrelease SHA-256:

- `bl`: `c1a5126e4d09e19c98e3342a49e6bb02bddf0d23cbf0f6c4048a3fcf2c05c84a`
- `bl-cli`: `d78417c540900df60630f88333e1313a817244085b1bdc33ec5e2dbbe75a499f`

検証ログは `/tmp/issue17-corrections/` の `red-*.log` / `green-*.log`、`focused-search.log`、
`focused-cross.log`、`fmt.log`、`all-targets.log`、`clippy.log`、`build.log`、`direct.log`、
`direct-server.log`。直接PTY結果は `release-pty.json`、HTTP記録は `direct-requests.jsonl` です。
これらは一時成果物です。実key/configには触れず、直接検証のfake設定/TLS/serverは終了時に削除しました。

**監査記録の留意点:** 以前の `cargo audit` 終了0という履歴だけで「完全clean」と判断しないでください。
Root指摘の `/tmp/issue17-root/audit.log` にはyanked registry照会timeoutがあり、上記先行記録の
「警告/脆弱性報告なし」を全照会成功の根拠にはできません。Cargo不変のため今回workerは指示どおり再監査せず、
Rootの独立再確認に委ねています。

**実Backlog TUIと実端末IME/キー衝突の必須受入は引き続きすべて未実施（UNPERFORMED）です。**
利用者のCLI接続成功やこのローカルmock検証を受入成功へ代用していません。変更後buildについてRootの独立reviewと
利用者/Rootの実TUI受入が必要です。Issue #17完了・merge可能とは宣言しません。
既定statusごとの候補GET反復の最適化は今回非採用のため未変更です。


### #17 改訂Planに基づく検索dialog実装・ローカル再検証（2026-10-29 05:29 UTC）

対象はHEAD `88ffbe7` + 保持した既存未コミット差分 + 今回の改訂実装です。
上の旧キー/PgUp・PgDn/旧hashの実測は履歴であり、今回の受入へ流用していません。
正式Issue本文と改訂コメント `issuecomment-6488296513`、Planの改訂節と基本設計を直接確認しました。
Plan/designの既存差分は今回の開始時と一致し、共有入力・Service/API・Cargo/設定を変更していません。
実Backlogへの接続/書込み、実key/configの参照、commit/stash/push/merge/配布公開は行っていません。

実装は編集優先dispatch、修飾なしPress Enterの文脈別処理、中央dialog/日本語4項目/重ねpopup、
成功候補（空配列含む）のdialog寿命内再利用、左右のgrapheme安全な全文確認、一行候補の最小縦scroll、
横断一覧/cのProjectへの移動・通常復帰/作成先案内です。候補名のLF/TABも表示copyだけ可視escapeし、
escape生成後もsecret検査します。旧応答の完全一致検査とcount/list成功後だけの交換を維持しました。

TDDの観測（各filterは既存の実bl入力→PTY/HTTPSまたはShell入力→TestBackend描画のseam）:

| filter | red → green（終了コード） |
| --- | --- |
| `search_dialog_keyword_standard_editing` | Ctrl編集が検索/focusに奪われ、`Xabc de`待機timeout 101 → 標準編集を優先し、Enter後だけcount/list・原文一致 0 |
| `search_dialog_is_centered` | 指定矩形の左上が枠でなく `n`、101 → 100x24で72x18・40x12で38x10、親内popupと閉鎖前後のkeyword位置/cursor維持 0 |
| `cross_project_dialog_compact` | 40x12で復帰/作成先案内がなくtimeout 101 → h/lまたはTab→Project→Enter、c案内、n次ページ、通常検索失敗/取得中も横断page 2保持 0 |
| `search_dialog_short_wide` | 100x12でkeyword枠とhelpが重なり `│`（期待 `└`）、101 → 高さ不足時は縦配置にして枠とhelpを分離 0 |
| `search_dialog_candidate_horizontal`（LF/TAB追補） | 可視escapeへ到達できずassert失敗101 → 一行copyのescape、Unicode/結合文字/ZWJと選択印・複数選択全文・secret非表示 0 |

候補cache試験の**初回redはEsc+Tab同時送信がAlt+Tabと解釈される試験手順不備を含み、cache実装のred証拠とはしません**。
キーを別stepに修正して0を確認後、試験感度を別途検証しました。cache hitだけを一時無効化するとstatus GETが
期待2に対し5で101（`cache-disabled-red.log`）、同じ製品sourceを復元すると0（`cache-restored-green.log`）。
この実験は事前TDDの証拠と混同しません。無効化コードは残していません。
候補取得中の同項目再focusで重複なし、空成功再利用、clear後focus/cache保持、dialog閉鎖後再取得をHTTP回数で確認。
古い成功/失敗の失効はcache hit・popup close・clear・keyword・dialog close・applyの6経路を各2ケース実行し、
後着結果を保存せず、現候補/エラー/新busyを壊さないことと、戻った時の新GETを確認しました。
既定status重複解除・曖昧/不在の検証エラー・未確定project禁止・自分の内部self・resize/tinyでのcursor保持も維持しています。

最終製品コードに対するチェック:

| コマンド | 終了コード / 観測 |
| --- | --- |
| `cargo test --locked --test tui_process search_dialog_` | **0 / 19成功**（最終は他コマンドと同時起動せず順次実行） |
| `cargo test --locked --test tui_process cross_project_dialog_` | 0 / 2成功 |
| `cargo test --locked --lib tui::tests::` | **0 / 15成功・実IME専用1 ignored**（最終は順次実行） |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | **0 / 20 suite、246成功・1 ignored**。下記のfixture接続reset tracebackを記録 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / Rust warningなし |
| `cargo audit --file Cargo.lock` | 0 / cargo-audit 0.22.2、DB 1381 advisories・224依存。今回ログにはDB/registry timeout、warning、脆弱性報告なし。過去のtimeout履歴を消さず、終了0だけを根拠にしていない |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 / 両binary Mach-O arm64 |
| CONTRIBUTINGの一時root/HTTPS/PTY手順を変更後releaseで直接実行 + 結果assert | 0 / 19 snapshots、28 GETのみ、count/list各5回のquery一致、stderr空・端末復元・secret/OSC出力なし。直接fixture server stderrも空 |
| `git diff --check` / stagedなし | 0 / stagedなし。Plan/designの継承差分不変、Cargo.lock SHA-256も従来と同じ |

**途中の失敗・警告は最終成功と分けて記録します。** 最終コードのall-targetsは全成功ですが、
HTTPS fixture threadのbody送信で `ConnectionResetError: [Errno 54] Connection reset by peer` のtracebackが1件ありました
（`final-all-targets.log:357-386`、`https_server.py:80`）。キャンセルでclient側がread taskを中断する検証中の
harness stderrと整合し、製品のstderrやテスト失敗ではありません。独立原因確認はRootへ引き継ぎます。
このためharnessの無関係な変更や全体再実行はしていません。
その後focused/lib/直接fixture/clippyを別processで同時実行した回に、searchの取消保持・貼付け試験2件と
既存early-SIGINT試験1件がPTYの8秒timeoutで101になりました。製品変更なしで順次再実行し、search19件と
lib15件がそれぞれ0でした。並行負荷/タイミング依存の可能性はありますが、確定原因と断言せずRootへ報告します。

release SHA-256（今回の変更後build）:

- `bl`: `9ae941290ded500063dff62a1eee1018556c3b635460dda3450a34c2e8c32869`
- `bl-cli`: `fe0407304acde6cbc21dfe63bcd6bcd62ab58bc986c152cb460ea3ec6d3bc92e`

直接fixtureのstatus/users/versions GETは各3/1/1回です（statusは初期通常解決・dialog・通常復帰）。
count/listの5組は、通常B/open → keyword+open/closed+Alice+巨大ID v1 → 全clear → 自分のみ横断 → B/open通常復帰。
全組でcount/offset以外のqueryが一致し、横断はproject/statusなし・assigneeId=7です。

一時ログは `/tmp/issue17-worker/`。最終証拠は `serial-focused-search.log`, `final-focused-cross.log`,
`serial-lib-tui.log`, `final-fmt.log`, `final-all-targets.log`, `final-clippy.log`, `audit.log`,
`final-build.log`, `final-build-sha256`, `final-direct.log`, `release-pty.json`, `direct-requests.jsonl`,
`direct-server.log`, `final-diff-check.log`、終了コード履歴は `exits` です。
red/greenと途中失敗ログも同directoryに残しています。一時TLS/fake設定/serverは直接fixture終了時に削除しました。

**実端末のIME/目視・実Backlog TUI受入は未実施のままです。** 上の新しい必須手順をRoot/利用者が
実施してください。環境（macOS、端末名/版、IME、keyboard設定）を推測せず、実測時に記録します。
独立reviewとRootの再検証も未完了で、Issue #17全体の完了・承認・merge可能とは宣言しません。

### #17 Root再検証・ユーザー確認と追加表示feedback（2026-10-29、表示変更前）

この節は上の履歴を置き換えず、その後Rootが得た結果を追記します。対象は表示feedback変更前の
release `bl` SHA-256 `9ae941290ded500063dff62a1eee1018556c3b635460dda3450a34c2e8c32869` です。
Rootはsearch19件・cross2件・tui-lib15件+ignored1件・all-targets246件+ignored1件、fmt、clippy、
cargo-audit 0.22.2、release build、diff-checkをすべて終了0で再確認しました。
Rootの今回ログにはfixture tracebackや監査timeoutはなく、直接release HTTPS/PTY fixtureも
19画面・28 GET・count/list各5回の同query・端末復元・空stderr・secret非表示を確認しました。
根拠は `/tmp/issue17-root-revised/`。これらは以下の表示変更後の検証には流用しません。

Rootはdefault profileのoriginが `https://lazybl.backlog.com/` であることを確認しました。
API key環境はAgent側にはなく、ユーザーの実端末で製品TUIを確認しました。
実Backlogの候補・検索適用/クリア/結果・横断・通常復帰と、実IME/編集/表示について質問し、
ユーザーから「動作は問題ない」「IME/表示等の質問はすべて成功」と回答を得ました。
これはユーザー申告による確認であり、生ログ/機密本文は記録していません。実Backlogへの書込みはありません。
環境はMacbook Pro M1 / macOS 26 / macOS標準IME / 101 keyboard。
端末アプリ名・版、remap有無、非機密の対象IDは不明で、推測して補いません。

追加feedback「状態、担当者、マイルストーンの選択しているものが下側。ラベルの横に表示できないか」
「Anyは空白でもよい」を採用しました。上段の状態/担当者、次段のマイルストーン、枠付きkeyword、
中央寸法/popup/全文scrollは保持し、値だけラベル右の一行へ、未選択の候補値欄だけ空欄へ変更します。
一覧summaryのkeyword `Any`、横断の「利用不可」は対象外です。**表示変更後のユーザー再受入待ち**です。

### #17 ラベル右表示・空値feedbackのworker検証（2026-10-29）

製品変更は `Conditions::value` の空選択文字列とdrawの値位置だけです。配置寸法やキー、IME、
安全表示、候補cache/世代、検索の原子的交換は変更していません。Plan/designの継承差分も維持しました。
Shell key→TestBackend描画の新規試験で、変更前は100x24の選択値がラベルと同じ行にないため失敗（101）、
最小変更後は成功（0）。100x24/40x12/100x12で厳密な空値欄、ラベル右の選択値、長い日本語/結合文字/emojiの
左右全文、focus印、水平scroll中の候補選択印、popup閉鎖からkeywordへ戻ったcursor位置を確認しました。
既存候補試験の選択印/grapheme保持も成功。通常3項目の同一行表示は実製品PTYで確認しました。
PTYのAny待機はkeywordラベル待機へ変更するだけでなく、空値画面のAny/旧選択値不在と
clear後count/listのkeyword/status/assignee/milestone条件なしを照合しました。
簡易VT復元のwide文字の残留セルは厳密空白判定に使わず、空白セル自体はTestBackendで検証します。

| 変更後に順次実行した確認 | 結果 |
| --- | --- |
| `cargo test --locked --test tui_process search_dialog_` | 0 / 19成功 |
| `cargo test --locked --test tui_process cross_project_dialog_` | 0 / 2成功 |
| `cargo test --locked --lib tui::tests::` | 0 / 16成功・実IME専用1 ignored |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / warningなし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| CONTRIBUTING直接release HTTPS/PTY fixture + 結果assert | 0 / 19画面・28 GETのみ・count/list各5回同query・復元・空stderr・secret/OSC非表示。server stderr空 |
| `git diff --check` / stagedなし | 0 / stagedなし |

直接fixtureでは100x24と40x12のclear画面にAny/旧選択値がないこと、clear queryに4条件がないこと、
選択query・self横断・具体Bの通常open復帰も照合しました。候補status/users/versions GETは3/1/1回で従来と同じです。
今回の変更後release SHA-256:

- `bl`: `068a2bc877637b8718b9ef3b50f15e6d071378dab7313cb7c5fcc4734bc97a9d`
- `bl-cli`: `647a5203a9b8123ebabd0c3da2be74f499a42daee11f9d7237fa59e17466cfa2`

途中の新規TestBackend試行でwide文字継続セルの空白を除かない期待が1件失敗しました。期待を正規化してから
正式redを取得しています。選択印の確認を追加した中間lib試験でも、popup閉鎖後のwide文字継続セルを
文字として読んだ期待が1件失敗（101）。画面列幅で継続セルを飛ばす読取りへ補正し、最終16件は全成功です。
最初のPTY追加assertは簡易VTの残留セルを厳密空白と誤認し4件失敗（101）、
同回に取消検証中のHTTPS fixtureで `ConnectionResetError: [Errno 54] Connection reset by peer` のtracebackが1件ありました。
harnessは変更せず上記の観測境界を分けて補正し、最終search19件は全成功・tracebackなしでした。
最終実行でPTY timeoutはなく、製品stderrと直接fixture server stderrも空です。

一時根拠は `/tmp/issue17-inline-feedback/` の `red.log`, `green.log`, `search.log`, `cross.log`,
`tui-lib.log`, `fmt.log`, `clippy.log`, `build.log`, `sha256`, `direct.log`, `release-pty.json`,
`direct-requests.jsonl`, `direct-server.log`, `diff-check.log`。一時TLS/設定/serverは回収済みです。
変更後の全体test/auditはRootが独立実行するためworkerでは繰り返していません。
実端末での変更後表示（100x24/40x12/100x12、ラベル右・空欄・左右全文・popup/resize/cursor）、
端末metadataの補完、Root全体check・独立review・ユーザーApproveが未完了です。
実Backlog/実IME操作はworkerでは行わず、Issue #17全体の受入完了とは宣言しません。

### #17 表示修正後のRoot検証・ユーザー再受入（2026-10-29）

対象はHEAD `88ffbe7` + 継承した差分 + 本Issueの改訂実装と表示feedback修正です。
release `bl` SHA-256は `068a2bc877637b8718b9ef3b50f15e6d071378dab7313cb7c5fcc4734bc97a9d`、
`bl-cli` は `647a5203a9b8123ebabd0c3da2be74f499a42daee11f9d7237fa59e17466cfa2`。
変更前のチェック・Review・Approveを本差分へ流用せず、Rootが以下を順次実行しました。

| コマンド | 終了コード / 実測 |
| --- | --- |
| `cargo test --locked --test tui_process search_dialog_` | 0 / 19成功 |
| `cargo test --locked --test tui_process cross_project_dialog_` | 0 / 2成功 |
| `cargo test --locked --lib tui::tests::` | 0 / 16成功・実IME専用1 ignored |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0 / 20 suite・247成功・1 ignored。今回Root実行にfixture tracebackなし |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / warningなし |
| `cargo audit --version` / `cargo audit --file Cargo.lock` | 各0 / 0.22.2、1381 advisory・224依存。今回ログにDB/registry timeout・警告・脆弱性報告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| `git diff --check` | 0 |
| 変更後release + HTTPS/PTY直接実行 | 0 / 19画面・28 GETのみ・count/list各5組同query、端末復元、製品stderr/server stderr空、secret/OSC非表示 |

実行は06:06〜06:11 UTC。監査前後のCargo.lock SHA-256はともに
`0b5c7a872566c900fcbbd060bb07ddd1760eb69127359c11dcba5fc43c16c4d6` で不変です。
直接fixtureでは100x24/40x12のclear値欄にAny/旧選択値がなく、clear queryに4条件なし、
選択時の巨大ID・自己担当だけの横断・具体Bの通常復帰を照合しました。
候補status/users/versionsのGETは3/1/1回。根拠ログは `/tmp/issue17-root-inline/`。
先行worker実行の接続reset traceback・並行負荷時のPTY timeoutは上の履歴どおり保持し、
今回の成功をそれらが起きなかったことの証明や恒久的な非再発保証とはしません。

ユーザーは変更前buildで指定originの実候補・通常検索適用/全解除/結果・横断/c案内/通常復帰について
「動作は問題ない」、IME確定Enter非適用→次Enter適用、変換変更/取消/再変換、標準編集、
100x24/40x12/極小復帰、長い値/popup、終了後shellについて「すべて成功」と回答しました。
その後は値の表示位置と空欄だけを変更し、上記の新buildを再起動して確認してもらいました。
変更後のラベル右表示・未選択空欄・左右全文・popup・狭幅resize・入力cursorの質問にも
「すべて問題なし」と回答しています。これは利用者による実端末の操作・目視結果で、
自動PTYやPlanning Probeを実IME受入に代用したものではありません。

接続先は `https://lazybl.backlog.com/`、profileは `default`（Rootが設定originを確認）。
実データ識別子は未提供のため記録せず、API key・機密本文・画面captureは取得/記録していません。
実Backlogへの書込みは行っていません。複数project間の実サービス確認は既存環境制約で対象外、
ローカルHTTPSのA/B横断試験で確認しました。
環境はユーザー回答のMacbook Pro M1 / macOS 26 / macOS標準IME / 101 keyboard / remapなし / Ghostty。
端末版はユーザー未申告ですが、Rootは実行中の `/Applications/Ghostty.app/Contents/MacOS/ghostty` と
同appのInfo.plistの版 **1.3.1 (15212)** を確認しました。細かなIME設定は未提供で推測していません。

独立した2つのfresh ReviewとCode Review gateは、この記録時点では未完了です。
これらの完了前にIssue #17全体を完了・承認済み・merge可能とは扱いません。

#### #17 fresh独立Reviewの完了・採用指摘の最小修正（2026-10-29）

上の「未完了」は記録時点の履歴です。その後、RootによるfreshなCorrectness/Ponytail Reviewの2件が完了し、
P0/P1はありませんでした。Rootは以下の3件だけを採用し、今回修正しました。最終taskflow Code Review gateは
まだ未実施で、Rootが担当します。過去のチェック・Review・Approveを今回のsummary変更後へ流用しません。

- 一覧のkeyword summaryがUnicode scalar値の10文字で切られ、結合文字・ZWJ emojiを途中で切断する指摘。
  `Conditions::summary` の短縮・省略判定だけを既存Ratatui `Line::styled_graphemes` に変更しました。
  secret/controlの全文検査、原文query、キー/IME/候補cache/世代/検索条件/配置は変更していません。
- READMEの「keyword編集中を除きq終了」は候補focus/closed/applying中の実動作と不一致でした。
  コードは変えず、通常画面のq終了（極小サイズでも可）と検索dialogのEsc候補閉鎖/取消・Ctrl+C中断に修正しました。
- 実Backlog受入の対象データ識別子は、ユーザーから提示された **課題キー `BLCLI-1`** として補完します。
  先行の「未提供」は当時の履歴として保持します。URLへの接続、本文・タイトルの取得、機密の追加記録はしていません。

実blへの入力→PTY描画/ローカルHTTPSの新規1試験で、ZWJ/結合文字の10grapheme境界は全体保持・省略なし、
11graphemeは全体保持して省略、日本語の11grapheme短縮、境界外のsecret/controlの全文伏字を確認しました。
7入力すべてでcount/listの原文keyword query不変・GETのみ・空stderr・端末復元をassertしています。
簡易VT decoderは結合文字を落とすため、resizeによる全描画のraw summaryからcursor位置指定だけを除いて照合しました。
最終試験を変更前の製品に戻すと失敗（101、ZWJ途中切断）、最小修正後は成功（0）でした。
試験作成中にはANSIの空白省略/位置指定と日本語文字数の期待を補正した中間失敗があり、製品の追加変更はしていません。

| summary変更後に順次実行した確認 | 終了コード / 実測 |
| --- | --- |
| `cargo test --locked --test tui_process search_dialog_keyword_summary_preserves_ten_graphemes_and_redacts_before_abbreviating -- --exact` | 0 / 新規1試験・7入力成功（変更前は101） |
| `cargo test --locked --test tui_process search_dialog_` | 0 / 20成功 |
| `cargo test --locked --test tui_process cross_project_dialog_` | 0 / 2成功 |
| `cargo test --locked --lib tui::tests::` | 0 / 16成功・実IME専用1 ignored |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / warningなし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| CONTRIBUTINGの直接release HTTPS/PTY fixture + 結果assert | 0 / 19画面・28 GETのみ・count/list各5組同query・選択/clear/self横断/通常復帰・secret/control非表示・端末復元・製品/server stderr空 |
| `git diff --check` / stagedなし | 0 / stagedなし |

summary変更後release SHA-256:

- `bl`: `d11f4c7f4fa6971ed0926233282ed91c4eaecfa257e49ec8beaacf064de9c5dc`
- `bl-cli`: `0090f1de25bfe2a472d17f848c787c7fbdfcc9aacb3a9f988da6fe7e60ee5eba`

今回の根拠は `/tmp/issue17-summary-review/` の `red-final.log`, `green-final.log`, `search.log`, `cross.log`,
`tui-lib.log`, `fmt.log`, `clippy.log`, `build.log`, `sha256`, `direct.log`, `release-pty.json`,
`direct-requests.jsonl`, `direct-server.log`, `diff-check.log`。直接fixtureの一時TLS/設定/serverは回収済みです。
全体test/auditはRootが独立実行するためworkerでは繰り返していません。summary変更後の実Backlog/実IME操作も
実行しておらず、先行のユーザー受入を新buildの確認と読み替えません。Rootによる最終チェック・必要な影響限定review・
Code Review gateを経るまでは、Issue #17全体の完了・承認済みとは宣言しません。

#### #17 summary修正後のRoot検証・受入と追加キー要望（2026-10-29）

Rootは最終summary修正後の製品を07:28〜07:33 UTCに再検証しました。対象release `bl`は
`d11f4c7f4fa6971ed0926233282ed91c4eaecfa257e49ec8beaacf064de9c5dc`、`bl-cli`は
`0090f1de25bfe2a472d17f848c787c7fbdfcc9aacb3a9f988da6fe7e60ee5eba` です。

- `cargo test --locked --test tui_process search_dialog_`: 0 / 20成功。
- `cargo test --locked --test tui_process cross_project_dialog_`: 0 / 2成功。
- `cargo test --locked --lib tui::tests::`: 0 / 16成功・実IME専用1 ignored。
- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`、
  `cargo build --locked --release --target aarch64-apple-darwin --bins`、`git diff --check`: すべて0。
- `cargo test --locked --all-targets`: 0 / 20 suite・248成功・1 ignored。今回ログにwarning/fixture tracebackなし。
- `cargo audit --version` / `cargo audit --file Cargo.lock`: 各0、0.22.2、1381 advisory・224依存。
  DB/registryの取得timeout、warning、脆弱性報告なし。監査前後のCargo.lock SHA-256はともに
  `0b5c7a872566c900fcbbd060bb07ddd1760eb69127359c11dcba5fc43c16c4d6` で不変。
- Rootの変更後release直接HTTPS/PTY確認: 0 / 19画面・28 GET・count/list各5組の条件一致、
  空欄/clear query/自己担当横断/通常復帰/端末復元/secret非表示・製品とserverのstderr空。
- 追加のrelease直接確認3入力（10境界の結合文字、11境界のZWJ emoji、省略範囲外secret）も成功。
  実製品の一覧要約・原文count/list query・GETのみ・端末復元・空stderrを照合。
  一時TLS/config/serverは回収済み。根拠は `/tmp/issue17-root-final/` の各log/results.json/PTY記録。

Rootは採用した3指摘の実差分と上記結果を直接確認しました。summaryの局所的な既存API利用と
資料訂正だけでキー/通信/状態境界を変更していないため、全体fresh Reviewの無条件再実行はしていません。
未採用の製品修正指摘はありません。
ユーザーはこの最終buildを同じdefault profile・BLCLI-1の環境で再起動し、実候補/通常適用・クリア・結果/
横断基本動作/IME確定Enter非適用→次Enter適用・emoji要約の質問に「動き、emojiは問題なかった」と回答しました。
以前のbuildの受入を最終buildの成功へ読み替えず、新たなユーザー回答として記録します。
接続先・端末・対象課題・複数project実サービスの既存制約は直前の記録どおりです。実サービスへの書込みなし。

同じ回答で「Shift+L, Shift+Hで左と右を切り替えたい」という追加要望がありました。
承認済み基本設計の左右内部h/l・Issue List→DetailのEnter・右→Issue ListのEscに対する新しい操作契約で、
検索dialogの改訂範囲には含まれていません。Rootは新キーを実装せず `REPLAN_REQUIRED` として停止しました。
新キーの方向、Project/Commentからの移動先、既存Enter/Escとの併存、入力/modal/極小時の扱いは再計画で確定します。
正式Plan/Work Itemは変更せず、既存未コミット/untrackedと部分実装を保持しています。
追加仕様が未承認のため最終taskflow Code Review gateには未提出で、Issue #17をREADY_FOR_MERGEとは扱いません。

#### #17 Shift+H/L改訂後の再受入手順（今回のbuild用）

上記の旧build受入・hash・追加仕様未承認による停止は履歴として保持します。
正式改訂Planの新キーを今回実装しましたが、変更後製品の実端末・実IME・実Backlog受入は**未実施**です。
先行buildのユーザー回答を今回の成功へ読み替えず、Rootとユーザーが新buildで以下を確認してください。

1. 新release `bl` のSHA-256、端末名/版、macOS、IME/keyboard設定、TERM/NO_COLORを記録します。
   実Backlogは既存手順の `https://lazybl.backlog.com/` originと読取り範囲を確認し、API key/機密本文を記録しません。
   複数projectの実サービス確認は従来の環境制約どおり対象外、ローカルHTTPSで補います。書込みは不要です。
2. 100x24と40x12でProject→L→Detail→l→Comment→H→Project→L→Comment。
   左をIssueへ切替後の復帰、h/l・Tab/Shift+Tabの内部切替を試し、同じ側のH/Lではtoggleしないことを確認します。
   初期左Project/右Detail、Issue Enterは右記憶に関係なくDetail、右Escは読取り取消+Issue、次Lは最後の右へ戻ります。
   画面のH:Left/L:Rightと内部h/l/Tab/S-Tab、文脈のEnter/Esc、q/Ctrl+Cが判別できることも確認します。
3. resize/極小往復・project切替・通常/横断検索でも右Comment記憶を保持します。
   極小中はH/Lが効かず、取消/検索失敗では背面不変、検索成功はIssue/左記憶Issueになることを確認します。
4. 読取り完了後とDetailの遅延R中にH/L往復し、本文scroll・課題選択/条件/ページ履歴を保ち、
   応答が取消されず表示されることを確認します。総GETの増加ゼロはローカルHTTPS/PTyで検証します。
   有効H/Lはエラー通知を消して移動し、同側no-opでも消します。Ctrl/Alt併用のH/Lは通知を消しません。
5. 検索keywordのH/Lは文字、候補popup/適用中は背面不変。既存の編集優先・候補cache・全clear・長い値の左右確認・
   通常復帰案内を再確認します。実IMEの変換確定Enter非適用→次Enter一回適用、候補変更/取消/再変換を必須確認し、
   誤検索なら未完了として停止します。PTYは実IME受入の代替ではありません。
6. q=0/Ctrl+C=130（入力中/通信中も優先）、通常画面・cursor・入力編集・termiosの復元を確認します。
   Commentの実データ取得や後続フォーム/editorは今回の完成対象ではありません。

ローカルrelease直接再現はCONTRIBUTINGの既存HTTPS/PTY手順を使います。
更新fixtureは検索19画面にH/Lと内部l・resizeの7画面を加えた26画面、総GET28/count・list各5回です。
[18]〜[25]の総 `request_counts` が一定であることを照合します（Detailを除外するbrowse数で代用しません）。

#### #17 Shift+H/L実装後のworker自動検証（2026-10-29 08:57 UTC）

実装はShellの左右focus記憶・確定focus設定・専用dispatchに限定しました。
Browseへ新キーを流さず、通信/選択/条件/世代は変更しません。既存の検索部分実装を保持し、
候補取得失敗時のpopup案内だけEnter retry/Esc閉鎖へ整合しました（動作変更なし）。
メインfooterは既存の全体1行/compact3行のまま短縮し、panel配置/高さは変えていません。

公開Shell→TestBackendと実bl→既存PTY/ローカルHTTPSで、TDDの縦sliceを観測しました。
以下の短縮filterは各1試験を実行しています。

| RED → 最小実装 → GREENコマンド | 終了コード / 根拠 |
| --- | --- |
| `cargo test --locked --test tui_shell shell_uppercase_region_keys` | 101（最初のLでProjectのまま）→0 / `red-navigation.log`, `green-navigation.log` |
| `cargo test --locked --test tui_shell shell_region_guidance` | 101（H:Left案内なし）→0 / `red-guidance.log`, `green-guidance.log` |
| `cargo test --locked --test tui_process search_dialog_candidate_failure_is_not_empty_success_and_can_be_retried` | 101（失敗時もEnter/Esc閉鎖と案内）→0 / `red-retry-help.log`, `green-retry-help.log` |

追加のprocess試験作成中に、popupを閉じるEscとdialog取消Escを分ける必要と、
R成功後は先頭へ戻り本文が画面外になる期待の誤りによる中間失敗がありました。
期待/stepを補正し、製品の追加変更はしていません（`process-region-first.log` → `process-region-second.log`）。
その後、通常project B→Aの実切替も追加して成功しました（`process-region-project.log`）。
通知の有効/拒否キー・全移動元/内部切替・初期/同側/Enter/Esc・入力/popup/適用保護・
成功/失敗/取消・横断/通常検索・project切替・resize/極小・NO_COLOR・初期通信/空project・
Ctrl+C130/復元・既存ページ2選択保持を回帰試験で確認しました。
遅延Detail R中の総GET数は7のまま、応答完了後は8で一定、取得完了後の左右往復は6のままです。
本文末尾scroll・課題選択/検索条件/履歴も保持し、同じ遅延R応答が取消なしで反映されました。

| 変更後の正式チェック | 終了コード / 実測 |
| --- | --- |
| `cargo test --locked --test tui_shell` | 0 / 8成功 |
| `cargo test --locked --test tui_process` | 0 / 55成功 |
| `cargo test --locked --lib tui::tests::` | 0 / 17成功、実IME専用1 ignored |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0 / 20 suite、255成功、1 ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 / warningなし |
| `cargo audit --version` | 0 / cargo-audit 0.22.2維持 |
| `cargo audit --file Cargo.lock` | 0 / 1381 advisory・224依存、DB/registry timeout・warning・脆弱性報告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| CONTRIBUTINGの直接release HTTPS/PTY + 結果assert | 0 / 26画面、28 GETのみ、count/list各5組の条件一致、clear/self横断/通常復帰、100x24/40x12の領域復帰、移動中の総GET不変、secret/OSC非表示、製品/server stderr空、q=0/端末復元 |
| `git diff --check` / stagedなし | 0 / stagedなし |

監査前後のCargo.lock SHA-256はともに
`0b5c7a872566c900fcbbd060bb07ddd1760eb69127359c11dcba5fc43c16c4d6` で不変です。
今回のrelease SHA-256:

- `bl`: `0c2038d888ce16d2f0332438249cd5281692acb0efcd56e8a4d1d07dfed20a98`
- `bl-cli`: `b832db595339069beb2aee8697cc625b1fe0e0f957e018377b9d96be1abf673d`

根拠は `/tmp/issue17-focus-root/` の上記RED/GREENログ、`tui-shell.log`, `tui-process.log`,
`tui-lib.log`, `fmt.log`, `all-targets.log`, `clippy.log`, `audit-version.log`, `audit.log`,
`lock-before`, `lock-after`, `build.log`, `release-sha256`, `release-pty.json`, `direct-requests.jsonl`,
`direct-assert.log`, `direct-server.log`, `diff-check.log`, `staged-files` です。
直接検証はCONTRIBUTINGの更新済み手順と同じ既存fixture/driverを使用し、一時TLS/config/serverは回収済みです。
Plan/基本設計/Cargo.toml/Cargo.lockおよびbrowse.rsはworker開始時から不変、既存未コミット/untrackedを保持しました。
commit/stash/reset・GitHub更新/push/merge/release/deployは行っていません。

**未完了**: 新buildの実端末・実IME・実Backlog受入、Rootの独立再検証・Review/最終gate。
今回workerは実profile/API keyの読取りも実Backlog接続も行っていません。
上の自動成功をIssue #17の全体完了や最終承認へ読み替えません。

#### #17 Shift+H/L改訂後のRoot再検証（2026-10-29）

Rootはworker終了後に実差分とfocus変更の全製品経路を確認し、同じ変更後releaseに対して独立再実行しました。
`tui_shell` 8件、`tui_process` 55件、`tui::tests::` 17件成功/実IME専用1 ignored、
全対象20 suite・255成功/1 ignored。fmt、Clippy（`-D warnings`）、release両binary build、
`git diff --check` はすべて終了0でした。Cargoの60秒超の進捗表示は既存body timeout試験で、
試験は成功し、warning/fixture tracebackはありません。
監査も終了0、cargo-audit 0.22.2・1381 advisory・224依存、DB/registry照会成功、
timeout・警告・脆弱性報告なし。Cargo.lockは上記hashで監査前後不変です。

Rootの変更後release直接HTTPS/PTYも終了0。26画面・28 GETのみ、count/list各5組の条件一致、
clear query・self横断・通常復帰、H/L往復の総GET増加ゼロ、secret/OSC非表示、
空の製品/driver/server stderr、q=0・端末復元をassertしました。一時TLS/config/fake key/serverは回収済みです。
初回の結果assertは背面一覧の正当な `K:Any` までdialogの空欄と一括判定して失敗しました。
製品不具合ではなくRootの検証範囲の誤りで、値欄に限定して再実行し成功しました。製品変更はありません。
根拠は `/tmp/issue17-focus-root-final/` の `checks.json`、各チェックlog、`release-pty.json`、
`direct-requests.jsonl`、`direct-result.log`、`direct-server.log` です。release hashは上記worker記録と一致します。

Rootは現在のdefault profileのspaceUrlが `https://lazybl.backlog.com/` originであることを値を限定して確認しました。
このRoot環境に認証環境変数はなく、実サービス接続・API key取得・書込みは行っていません。
変更後buildの実端末・実IME・実Backlog受入はユーザー確認待ちです。
独立した2件のfresh Reviewと最終Code Review gateも未完了で、現時点でIssue #17を完了とは扱いません。

#### #17 Shift+H/L改訂後releaseのユーザー受入（2026-10-29）

Rootが上記hash `0c2038d888ce16d2f0332438249cd5281692acb0efcd56e8a4d1d07dfed20a98` のreleaseを指定し、
`./target/aarch64-apple-darwin/release/bl --profile default` を普段の認証設定済み実端末から起動するよう依頼しました。
ユーザーは今回のbuildに対する次の二つの確認群へ、それぞれ **「すべて問題なし」** と回答しました。

- Project→L→Detail→l→Comment→H→Project→L→Comment、Issueを左の最終focusとした復帰、
  同じ側no-op、Issue Enter→Detail／右Esc→Issue、100x24／40x12／極小往復のfocus・案内・選択／scroll保持、
  q終了・入力中Ctrl+Cとshell／端末復元。
- 実候補・通常検索適用／クリア／結果・横断→通常復帰・標準編集・H/L文字入力・popup、
  実IMEの変換確定Enterでは検索せず次Enterで一回検索、候補変更・取消・再変換。

環境と非機密の対象識別子への回答は **「前回と同じ」**。
前回の明示記録に対応するGhostty 1.3.1、macOS 26、macOS標準IME、101 keyboard・remapなし、
課題キー `BLCLI-1`、default profile／`https://lazybl.backlog.com/` を今回のユーザー申告として記録します。
今回操作端末のTERM／NO_COLORの具体値は未取得で、推測しません（自動試験のNO_COLOR検証とは区別）。
過去buildの成功を流用せず、今回の質問とユーザー回答を受入根拠とします。画面capture・API key・機密本文は取得／記録せず、
実Backlogへの書込みは行っていません。複数projectの実サービス受入は既存環境制約どおり対象外で、ローカルHTTPS試験で確認しました。

この受入後にfresh Correctness／Ponytail Reviewを実施し、Rootが指摘を整理してから最終Code Review gateへ提出します。
受入回答をCode ReviewのApproveとして流用しません。

#### #17 Shift+H/L改訂後の独立Review（2026-10-29）

ユーザー受入後、別々のfresh built-in reviewerでCorrectness ReviewとPonytail Reviewを実施しました。
両方が最終 `git diff HEAD`・untracked全3ファイルの差分と現在のソース・正式Plan／基本設計、
Rootのチェック／直接HTTPS記録と今回buildへのユーザー受入記録を確認し、具体的な指摘はありませんでした。
Rootも指摘なしの内容・根拠を確認し、追加の修正要求・未採用指摘はありません。
Reviewerはread-onlyで、自身によるコマンド実行・実端末／実Backlog操作はしていません。
実行結果はRootが独立実行したログを確認したものです。未取得TERM／NO_COLOR、
実サービス複数project対象外、全端末／IME保証なしという制約も維持します。

Review run: `6fdfdff7-b766-4fac-bf25-9dd15cf97e15`。
Correctness: `ac0f21a9-475a-4e93-81f5-639e90b90378`、
Ponytail: `5c780d56-5f01-4949-8d95-1d30e651c8bb`。
各結果はsession-managed artifactの `reviews/correctness.md`、`reviews/ponytail.md` にあります。
この追記はレビュー結果の記録だけで製品コードを変更していません。最終Code Review gateはRootが続けて実施し、
その明示的な結果が得られるまでREADY_FOR_MERGEとは扱いません。


## Issue #18 コメント閲覧 — 自動検証済み／実端末・実Backlog受入pending（2026-10-29 UTC）

### 対象と実測

対象は今回の未コミット実装（`bl`のComment閲覧のみ、c/e・書込みフォーム・editorは対象外）。
Rust 1.99.1 / Darwin arm64、release target `aarch64-apple-darwin`。
対象build `target/aarch64-apple-darwin/release/bl` のSHA-256:
`539d67e91d049429c22b6ea188e6b13e23cc72880aa801cac8e195eaa1a06927`。
ソース変更後はbuildを取り直し、以下の結果を別buildへ流用しないでください。

- RED: `cargo test --locked --test tui_process comment_initial_preview -- --nocapture` は終了101。
  初期Commentに変更履歴が表示されず、PTYが`CHANGE HISTORY`待ちで失敗。
  最初の試行はテストのfocusキー不足もあり、`l→Enter→l`へ直してから同じ未実装表示のREDを再確認した。
  最小実装後は同コマンド終了0。null履歴だけのページで記法GETが増えないことも照合した。
- 追加RED: `cargo test --locked --test tui_process comment_full_layout -- --nocapture` は終了101。
  100x24の既存4行Comment枠で従来CLI形式のIDだけが見え、本文を初期表示できなかった。
  layout比率を変えず、枠見出しに安全な識別情報、内側に本文先行document、本文末尾に全metadataを置いて終了0。
- 新規PTY 6件、公開Service 1件、Shell入力／TestBackend 1件を追加。
  20件ページ・巨大ID・両方向exclusive境界・最古1 GET・空終端、null／混在履歴・任意JSON・安全表示、
  R失敗時の選択／scroll保持と成功時交換、課題切替の旧本文即時消去、遅延成功／失敗・新busy・Esc取消、
  focus往復の総GET不変、Unicode wrap／resize／clamp、g/Gが本文端ではないことを確認。
- `cargo fmt --all -- --check`: 終了0。
- `cargo test --locked --all-targets`: 最終実行は終了0、263成功・0失敗・手動実端末用1件ignored。
  初回はtoolの240秒上限でTUI suite終盤に打切り（それ以前のsuiteは成功）となり、成功扱いせず600秒上限で全対象を再実行した。
- `cargo clippy --locked --all-targets -- -D warnings`: 最終実行は終了0・警告なし。
  初回の`useless_conversion` 1件はString化したtitleの不要`.into()`を除去して解消。
- `cargo audit --file Cargo.lock`: cargo-audit 0.22.2、終了0、1381 advisory／224依存、警告・脆弱性の報告なし。
  監査前後のlockfile SHA-256はともに
  `0b5c7a872566c900fcbbd060bb07ddd1760eb69127359c11dcba5fc43c16c4d6`。lockfileは変更していない。
- `cargo build --locked --release --target aarch64-apple-darwin --bins`: 終了0・警告なし。
- CONTRIBUTINGの「release blのローカルHTTPS再現」を実行し、製品release `bl`＋PTYで終了0、
  `code:0`／`restored:true`／空stderr、総13 GET（comments 3、issue 3、project記法3、space/projects/count/list各1）。
  `request_counts=[4,9,9,9,10,10,13,13,13,13]`。
  `tests/fixtures/tui_comment.json`＋`tui_comment_steps.json`で、n移動→j×15→R失敗→Enter dismiss→R成功→End→H/Lを再現。
  ID 1の本文位置保持、成功時ID 3／本文先頭への交換、既知／未知履歴、secret伏字、外部OSCなしを確認。
  すべてloopback HTTPS／空HOME・専用config・ダミーkeyのみ。利用者config／keyの読取りと実サービス接続はしていない。

### 必須受入の未実施と再開手順

**#18は実端末・実Backlogの必須受入が未実施のため未完了。自動試験や過去Issueの受入を代用しない。**
今回使用する実端末名／版、macOS／IME、TERM／NO_COLOR、対象buildと非機密な課題キーは利用者の実施時に記録する。
今回の実端末情報を過去の#17記録から推測しない。接続先は正式Plan指定の`https://lazybl.backlog.com/`。
実API keyは実装担当環境に与えられておらず、利用者の認証設定やkeyを読んで補っていない。

利用者／Rootによる再開手順（読取りのみ）:

1. 使用profileのoriginと対象buildを確認して実端末から`bl --profile <profile>`を起動する。
   100x24と40x12で、実際にコメントと本文なし変更履歴を持つ課題を選択する。
2. Commentの最新1件で左揃えの投稿者→JST作成日時→空行の固定3行と本文を確認する。
   100x24のDetail14／Comment6、40x12の単一パネル、極小往復で固定行／本文Rectを確認する。
   長い投稿者はgrapheme境界の`…`省略時だけscroll側に全文があり、resizeで再判定されることを確認する。
   TUIの種別／ID・BODY／CHANGES・末尾metadataがなく、本文と履歴の同居・既知／未知項目が残ることを確認する。
   Comment createdとDetail CREATED／UPDATEDのJSTを確認し、START／DUE／本文・CLI／JSONは不変とする。
   BLCLI-1のcomment `839099888`／`839099889`で詳細の全削除／全追加と
   `2026/10/27 01:05:08`／`2026/10/27 01:05:10`を確認する（APIにない値を推測補完しない）。
   色ありは白文字#FFFFFF＋暗赤背景#800000＋取り消し線／従来の黒文字＋緑背景の可読性を目視する。
   NO_COLORは`[-...-]`／`{+...+}`と色なしを確認し、RGB cell試験だけで実端末の可読性を合格にしない。
   枠は`Comment`／`> Comment`、取得中・終端・取消通知は空白区切りで生成`|`なし、本文の`|`は保持することを確認する。
   既存の読取り可能なnull履歴があれば、既知／詳細／未知fieldの前後値自体がnullの時だけ`未設定`になることを確認する。
   入れ子null・文字列null・空文字・0・false、CLI／JSONは変更しない。特殊値の準備書込みは不要でharnessを使う。
   部分差分・10,000超警告・非文字列／未知fieldはharnessで検証でき、実データ準備の書込みをしない。
   警告は「詳細」と指定文言のみで、前後本文がscroll先にも出ないことを確認する。
3. n/pの古い／新しい移動、gの最新・Gの最古、端での移動、Rで最新と本文先頭へ戻ることを確認する。
   g/Gを本文端の移動と混同しない。h/l・Tab・Shift+Tab・H/Lのfocus往復、EscでIssueへ戻る操作を確認する。
4. 課題を切替えて旧本文が残らないこと、RがIssue Listを変更しないこと、q／Ctrl+C後に通常shellを使えることを確認する。
   失敗・逆転・通信回数は上記harnessの記録を根拠とし、実サービスに障害や過負荷を起こさない。
5. 接続先・非機密な対象キー・操作・結果／未確認・端末情報をこの節へ追記する。
   API key・機密本文は記録しない。対象データの準備／変更／削除、c/eやeditorの受入は今回行わない。

GitHub Actionsは未実行。独立review gateはRootが後続実施する。これらを自動チェック成功やIssue完了として記録しない。

### #18 Rootによる再検証（2026-10-29 UTC）

上記と同じrelease SHA-256 `539d67e91d049429c22b6ea188e6b13e23cc72880aa801cac8e195eaa1a06927` に対して実施。

- `cargo fmt --all -- --check`、`cargo test --locked --all-targets`、`cargo clippy --locked --all-targets -- -D warnings`、
  `cargo build --locked --release --target aarch64-apple-darwin --bins` はRoot自身の再実行ですべて終了0。
  テスト263成功・0失敗・実端末用1件ignored、Clippy警告なし。
- cargo-audit 0.22.2の `cargo audit --file Cargo.lock` は終了0、1381 advisory／224依存、警告なし。
  監査前後のlockfile SHA-256は上記と同じ `0b5c7a872566c900fcbbd060bb07ddd1760eb69127359c11dcba5fc43c16c4d6`。
- リポジトリのチェックとは別に、Rootがrelease製品をローカルHTTPS＋PTYから実行。
  文書化fixtureで10 snapshots／13 GET、終了0・stderr空・端末復元成功。
  nでID 2→1、j×15後のR失敗で16行目保持、次R成功でID 3の本文先頭へ交換、
  既知／未知履歴・任意JSON・secret／OSC安全化、H/L後のscroll／総GET保持を実観測しassertした。
- 別の固定45件・巨大ID・本文なし履歴fixtureをrelease製品から40x12で操作。
  n×20／p／G／p×20／gで相対ID `45→25→26→1→21→45` を観測。
  commentsのGETは6回、exclusive `maxId`／`minId`・両方向orderを受信queryと照合。
  Gは昇順の1 GETのみ。本文なし履歴による追加の記法GETは0回。
  終了0・stderr空・端末復元成功。すべてGETで実サービス接続／書込みなし。

Rootはdefault profileのspaceUrlが `https://lazybl.backlog.com/` を指すことのみ確認した。
このセッションにAPI key環境変数はなく、実端末・実Backlogの必須受入は引き続き未実施。
過去Issueの受入・今回の自動PTY成功で代替せず、利用者の認証済み実端末で確認する。

### #18 P2：TUI構造化行の区切り修正（2026-10-29 UTC）

過去buildの記録は上記のまま保持する。共通comment formatterのmetadata／履歴行だけを、
CLIは既存TAB、TUIは `: ` で区切るよう修正した。本文内のTAB・安全化・JSONエラー処理は変更していない。

- 実bl＋PTY＋ローカルHTTPSの新規回帰試験
  `cargo test --locked --test tui_process comment_history_and_metadata_have_visible_field_value_separators -- --exact`：
  RED終了101（`DUE DATEnull`／`AUTHORComment author`、foo+bar／foob+arがともに`foobar`）、
  最小修正後GREEN終了0。既知／未知field・前後値・全metadataの可視区切りを画面からassertした。
- `cargo fmt --all -- --check`：終了0。
- `cargo test --locked --test issue_comment --test tui_process comment_`：終了0、CLI 2件／TUI 7件成功。
  CLIのfilter漏れを補う `cargo test --locked --test issue_comment`：終了0、13件全成功。
  既存TAB出力・未知JSON／secret／key衝突時の失敗契約を含む。
- `cargo clippy --locked --all-targets -- -D warnings`：終了0、警告なし。
- `cargo build --locked --release --target aarch64-apple-darwin --bins`：終了0。
  新buildのSHA-256：`bl` は `3e90167cf87bc111d020eec5bdda092e482f5608f9d76b731876342d193c7421`、
  `bl-cli` は `16bf84e9f6814a28939b105f96f8ab45958f4070cabc341a4c867e525e2341bc`。
- CONTRIBUTINGのrelease blローカルHTTPS再現を新buildで再実行：終了0、`code:0`／`restored:true`／空stderr、
  総13 GET、focus往復後も総GET不変。snapshotから `DUE DATE: null -> 2026-04-01`、
  `ASSIGNEE: old -> new`、`futureField: {"nested":[true,null]} ->`、`AUTHOR: Comment author`、
  `UPDATED: 2026-03-02` をassert。raw secret／外部OSCなし。

今回workerは全対象test／auditを再実行しておらず、正式全対象checkと独立reviewはRootが修正後に再実施する。
実端末・実Backlogの必須受入は引き続き未実施／利用者待ちであり、#18の完了にはしない。
今回もloopback HTTPS・一時config・ダミーkeyだけを使用し、実サービス接続・利用者key読取り・書込みは行っていない。

### #18 独立Reviewと修正後Root検証（2026-10-29 UTC）

freshの独立したbuilt-in reviewerを2名実行した。
Correctness（run `6ba2fe81-ae25-40d8-aa20-7eee17010776`）は根拠あるP0/P1/P2なし。
Ponytail（run `876d75e6-7062-4449-ad38-8d766fc13146`）は構造化行のTAB非表示による連結をP2として指摘した。
Rootも実画面記録で連結を確認し、未知fieldと値の境界を判別可能にする必要があるため採用。
上記のTUIだけ`: `にする最小修正と回帰試験で解消した。その他の不採用指摘はない。
両Reviewとも実端末・実Backlog受入未実施を明示し、Issue完了とはしていない。

修正は共通formatterの構造化行の区切り引数と公開PTY回帰試験1件に限定され、Rootが実コードと全callerを確認した。
本文TAB・CLIのTAB区切り・安全化・JSONエラー処理・状態管理への変更はない。
小さい表示修正のため全Reviewの無条件再実行はせず、影響する検証と正式チェックをRoot自身が再実行した。

対象release `bl` SHA-256は修正後の `3e90167cf87bc111d020eec5bdda092e482f5608f9d76b731876342d193c7421`。

| 修正後Rootチェック | 結果 |
| --- | --- |
| `cargo fmt --all -- --check` | 終了0 |
| `cargo test --locked --all-targets` | 終了0、264成功・0失敗・実端末用1件ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | 終了0、警告なし |
| `cargo audit --file Cargo.lock`（0.22.2） | 終了0、1381 advisory／224依存、警告なし、監査前後のlockfile SHA-256は上記と同じ |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 終了0 |
| `git diff --check` | 終了0 |

Rootは新releaseで文書fixtureと別45件fixtureの直接操作も再実行した。
文書fixtureは13 GET・終了0・端末復元成功・stderr空、R失敗時の16行目保持と成功時の最新先頭交換、
`DUE DATE: null -> 2026-04-01`／`ASSIGNEE: old -> new`／`futureField:`／`AUTHOR: Comment author`の可視区切り、
secret／OSC安全化、H/L後のscrollとGET不変をassertした。
別fixtureは40x12で `45→25→26→1→21→45`、両方向exclusive境界、Gの1 GET、null履歴の追加記法GETなし、
終了0・端末復元成功・stderr空を再確認した。

最新Issue本文・改訂コメントを再取得し、元の4条件と必須実Backlog受入条件が維持されていることを確認した。
元の4条件はローカル検証済み。修正後buildでの実端末・実Backlog受入は利用者待ちのため、最終状態は未完了。
Code Review gateの明示的な結果はRootが別途確認する。Plan／設計／依存／GitHub Issue本文・状態は変更していない。

### #18 承認済み改訂の実装・workerローカル検証（2026-10-29 UTC）

本節は「Comment表示・TUI日時・詳細のinline差分」改訂後の結果であり、上記の旧build・観測・hash・停止記録は保持する。
正式Planと基本設計、Issue #18本文・全改訂コメントを直接確認し、既存の未コミット／untracked部分実装を保持して続行した。
今回Plan／基本設計・Service／CLI契約は改訂していない。実Backlog接続／書込み、GitHub変更、commit／stash／破棄／push等は行っていない。

実装した範囲:

- Comment created／Detail CREATED・UPDATEDの共通JST表示。不正・date-only・変換範囲外は安全原値＋変換不可。
  START／DUE／本文内の日付・CLI／JSONは不変。
- 投稿者（太字＋色、NO_COLORは太字のみ）→日時→空行の固定3行とscroll本文。
  grapheme／表示列による長名省略・省略時だけの全文確認・resize再判定。100×24はDetail14／Comment6。
  描画・wrap行数・scroll上限・page移動は同じbody Rectを使用する。
- TUIの種別／表示ID・BODY／CHANGES・末尾metadataを除去し、内部ID・通常本文・同居／未知履歴を保持。
- description文字列pairだけ安全表示原文の行＋grapheme差分。削除は赤背景＋取り消し線、追加は緑背景、
  NO_COLORだけ`[-...-]`／`{+...+}`。行とgrapheme比較に同じ50ms目安deadlineを使い、比較は既存取得task側で準備する。
- 元文字列scalar前後合計が10,000超の場合だけ指定警告のみ。scroll文書にも前後本文を含めない。
  非文字列／nullは型を保持するfallback。全文安全化に加え、色あり／NO_COLOR組立後と固定行省略後の最終投影を再検査する。
- 既存paging／R／取消・旧応答の状態保持とGET契約を維持。README／CONTRIBUTINGと上記の今後の受入手順を整合。

TDD縦sliceの記録（承認済み公開seamのみ）:

| REDコマンド | RED結果 → GREEN |
| --- | --- |
| `cargo test --locked --test tui_shell shell_full_layout_gives_detail_fourteen_rows_and_comment_six_rows` | 101、row16が縦枠で期待の下角にならない → 14／6 clamp実装後0 |
| `cargo test --locked --test comment_preview`（最初の1件） | 101、旧tupleにauthor／created／textがない → preview文書／共通JST実装後0 |
| `cargo test --locked --test comment_preview description_inline_diff` | 101、NO_COLOR文書がない → 行＋grapheme差分／両projection実装後0 |
| `cargo test --locked --lib comment_fixed_rows_styles` | 101、固定作者行がない → 固定3行と共通body Rect実装後0。wide文字の継続cellを考慮して見出しassertを整合 |
| `cargo test --locked --test comment_preview`（型保持fallback追加時） | 101、null→空文字の引用符が消える → description非文字列pairのJSON型保持後0 |

追加試験はJSTのoffset／日月年跨ぎ／閏日／小数秒／不正・date-only・範囲外、Unicode・空白・TAB・末尾改行の前後値再構成、
空へ／空から／同値／非文字列、9,999／10,000／10,001 scalarと伏字前count、組立secret／ESC／OSCを確認した。
TestBackendは固定行座標、作者の太字／色、削除のfg／bg／取り消し線、追加のfg／bg、NO_COLORの全cell fg／bg Resetをassertした。
長名／不正日時の全文到達・resize再判定とclamp、警告時のsentinel非表示もcellから確認した。
PTY既存表示期待はComment範囲に限定してmetadata除去をassertし、DetailのCREATED／UPDATEDは維持した。
Detailの高さ変更に伴うresize後の先頭行期待18→20を整合し、取消・旧応答・GET数のassertは保持した。

依存解決:

- 直接依存は承認済み`time = "=0.3.55"`（default-features=false、parsing／formatting）と
  `similar = "=2.7.0"`（unicode）の2件のみ。time 0.3.55と既存推移依存の版は更新していない。
- lockfileの追加packageはsimilar 2.7.0、unicode featureの解決に伴うbstr 1.13.1／regex-automata 0.4.18、
  timeのparsing／formatting feature解決に伴うtime-macros 0.2.32の4件。optional依存のlock記録を含み、
  新しい直接依存や独自日時／Unicodeエンジンは追加していない。
- `cargo check --offline`はsimilarがlocal registryにないため101。その後の`cargo check`で指定版を取得し終了0。
  無指定cargo update／lockfile再生成は行っていない。

変更後の正式チェック（各test実行枠は600秒以上、全対象を含む最終一括実行枠は1200秒）:

| コマンド | 結果 |
| --- | --- |
| `cargo test --locked --test tui_shell` | 0、8成功 |
| `cargo test --locked --test tui_process` | 0、64成功 |
| `cargo test --locked --test issue_comment` | 0、14成功 |
| `cargo test --locked --test comment_preview` | 0、7成功 |
| `cargo test --locked --lib` | 0、21成功・実端末専用1 ignored |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0、277成功・0失敗・実端末専用1 ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | 最終0、警告なし。初回はcollapsible_ifで101、let-chainへ修正後再実行0 |
| `cargo audit --file Cargo.lock` | cargo-audit 0.22.2、0、1381 advisory／228依存、警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| `git diff --check` | 0 |

初期のtime format_description::parse非推奨警告はparse_borrowedへ置換して解消した。最終test／fmt／Clippy／audit／buildに警告なし。
監査前後のCargo.lock SHA-256はいずれも`55a7bd91c9d514359d18d225f0e3c293bb10e20339b47dc1aad6a421b51f8f14`で不変。
変更後release SHA-256:

- `bl`: `3f4b47a8064c9376bc591dd4d5eb3a084e00dd9c2dd680732a7e45eb0140bf40`
- `bl-cli`: `27c92fc3addb46831885559e8be3af3f3298f4ccb3c3d5f56bde4b1ddbfcc152`

上記release blを更新済み`tests/fixtures/tui_comment.json`／`tui_comment_steps.json`と既存ローカルHTTPS／PTYへ直接接続した。
終了0・restored=true・stderr空、総13 GETのみ、request_counts=`[4,9,9,9,10,10,13,13,13,13]`。
n移動→j×15→R失敗／Enter消去後も16行目保持→R成功で最新先頭交換、固定JST、End→H/L後の未知履歴と通信数保持、
secret非表示／外部OSC非実行と`[REDACTED]`表示をassertした。実サービスは使用していない。

**現時点でIssue #18は未完了**。この改訂後buildの実端末／実Backlog受入、独立Review、Rootの最終チェックは未実施。
worker runは記録追記前に30分timeoutとなり、Rootが残存cargo／harnessなし・差分保持を確認した後、明示retryで本記録と引継ぎのみを完了した。
実IME専用1件ignored、GitHub Actions未実行。実端末の字幅・色／取り消し線の可読性は自動PTYで保証していない。
既存Paragraphのu16 scroll上限（65,535 wrapped行）とsimilar deadlineの近似性は残る。
実Backlog受入は上記の利用者／Root再開手順で、originと対象hashを確認して読取りのみを後続実施する。
旧264成功・過去Review・PlanningのAPI読取りを、この改訂後の受入成功へ流用しない。

### #18 accepted findings P1/P2の限定修正・再検証（2026-10-29 UTC）

Rootが独立correctness／ponytail Reviewから採用した次の2件だけをTDDで修正した。
過去の観測・hash・成功記録は上記の旧buildの結果として保持し、新buildの受入へ流用しない。
修正前Rootの正式チェックログは`/tmp/backlog18-root-checks.rFyXRD`（277成功・0失敗・1 ignored、
fmt／Clippy／audit 228依存・警告なし／release build／diff各終了0）。今回の新入力はその時点では未検証だった。

- **P1（両reviewer）**: secret=`abcd`、description旧新とも`ab\tcd`が最終投影検査を通過し、
  Ratatuiの制御grapheme除外でTABが消えて描画cellにsecretが現れる。
  最小private共通TUI helperでTABを`⇥`へ変換してから全文secret検査し、変更時は危険なstyleを保持しないplainへfallbackする。
  preview文書・作者／日時overflow prefix込み投影・通常Comment／Detailの共通Paragraphで適用する。
  description比較は従来どおり安全原文の行＋graphemeで行い、比較後だけ可視化するためTABとliteral `⇥`は同値化しない。
- **P2（correctness reviewer）**: `line\n`→`line`と逆で改行styleが空Spanだけに残り、色ありcellで追加／削除を区別できない。
  色あり文書の変更された改行だけに`↵`を元の削除／追加styleで付け、元の改行・共通文字も保持する。
  通常表示へNO_COLOR専用`[-]`／`{+}`は追加しない。可視cell追加後も最終secret検査する。

API原値・CLI固定表示／JSON・送信値、Service・Plan・基本設計、依存は変更していない。
既存未コミット／untracked差分を保持し、commit／stash／破棄／公開、実API接続／書込みは行っていない。
新しいharness・設定・永続化・モードも追加していない。README／CONTRIBUTINGの表示説明を数行だけ整合した。

TDDは承認済みの公開previewとShell→TestBackendを使用した。ログは`/tmp/backlog18-findings-checks`に保持する。

| コマンド（追加回帰） | RED → GREEN |
| --- | --- |
| `cargo test --locked --test comment_preview preview_tabs` | 101、`ab\tcd`のままで`ab⇥cd`にならない → 0 |
| `cargo test --locked --lib tui_tabs` | 101、description cellに`abcd`露出 → 0 |
| `cargo test --locked --test comment_preview preview_changed_newlines` | 101、`line\n`のままで`line↵\n`にならない → 0 |
| `cargo test --locked --lib comment_changed_trailing` | 101、改行方向の可視cellがない → 0 |

TAB回帰は両色でdescription同値・通常Comment・作者overflow全文・不正日時overflow全文・Detailのcellを検査する。
secret=`ab⇥cd`を変換で新構成する場合も両色で伏字化を確認する。
改行回帰は両方向の色あり`↵`cellのfg／赤bg＋取り消し線／緑bg、共通`line`の無装飾、次行の保持をassertする。
NO_COLORは従来の改行を含む方向記号と全cell fg／bg Resetを確認する。
secret=`line↵`を改行可視化で新構成する場合もpreview／cellで再検査する。
既存styled再構成試験は改行を除いていた期待を廃止し、行境界と変更改行cellのstyleから前後の**全改行と末尾改行位置**を再構成する。
TABは表示コピーの`⇥`位置として検証し、TAB対literal markerの削除／追加styleも検証する。
literal `↵`を含む入力と変更改行の併存も追加し、期待を単に弱めず内容・方向を保持した。

変更後チェック（全て終了0、最終実行時に警告なし）:

| コマンド | 結果 |
| --- | --- |
| `cargo test --locked --test comment_preview` | 9成功 |
| `cargo test --locked --lib` | 23成功・実端末専用1 ignored |
| `cargo test --locked --test issue_comment` | 14成功（既存CLI／JSON契約回帰を保持） |
| `cargo test --locked --test tui_shell` | 8成功 |
| `cargo test --locked --test tui_process` | 64成功 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 281成功・0失敗・実端末専用1 ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 |
| `cargo audit --file Cargo.lock`（0.22.2） | 0、1386 advisory／228依存、警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| `git diff --check` | 0 |

監査前後のCargo.lock SHA-256は`55a7bd91c9d514359d18d225f0e3c293bb10e20339b47dc1aad6a421b51f8f14`で不変。
今回buildのSHA-256（全対象test後の最終再buildでも不変）:

- `bl`: `1f1ed32103cfbc2db3684ea8f078e002e1ed0e8d72396fa4fb3aa621317a44a9`
- `bl-cli`: `db1b4f930040c15f3d6aa1bc2df2df13e489b7b2989ef0ccc1f696107831a192`

上記release `bl`を既存`https_server.py`／`tty_shell.py`へ直接接続して再確認した。
入力はローカルfixtureのみで、利用者config・実サービス・実keyは使っていない。

- 既存文書fixture／steps: 13 GET・10 snapshot、request_counts=`[4,9,9,9,10,10,13,13,13,13]`。
  R失敗前後の16行目保持→成功時の最新先頭交換、固定JST `2026/03/01 09:00:00`、
  H/L後の未知履歴・GET数保持、secret／ESC／OSC非表示と`[REDACTED]`をassertした。
  初回の補助assertはfixtureと異なるJST期待（10月の別case）で失敗したが、実行自体は終了0／復元成功だった。
  fixtureのcreated原値に対応する上記3月期待へ訂正し、新規実行で全assert成功した。製品変更は不要だった。
- 一時入力（同値TAB／末尾改行削除／追加／TAB対literal `⇥`、DetailにもTAB）:
  色あり／NO_COLOR × secret=`abcd`／`ab⇥cd`の4case。
  各7 GET・6 snapshot、request_counts=`[4,7,7,7,7,7]`。
  CommentとDetailのTAB可視化／変換後secret非表示、色あり改行`↵`とNO_COLOR方向記号をassertした。
  色／取り消し線の合否はPTYのplain画面で代用せず、上記TestBackendのcell styleで判定した。
- 全5caseは製品終了0・restored=true・stderr空・GETのみ。既存fixtureファイルは変更せず、一時入力は終了後削除した。

**Issue #18は引き続き未完了**。今回差分の独立ReviewとRootの採否／最終承認は別途必要。
実端末・実Backlog受入、実IME（1 ignored）、GitHub Actionsは未実施。
端末字幅・色／取り消し線の目視可読性は自動PTYでは保証しない。
既存Paragraphのu16 scroll上限とsimilar deadlineの近似性は変えていない。
実Backlog・実端末の必須受入をローカル成功で代替せず、旧観測・hashも保持する。

### #18 改訂・指摘修正後のRoot再検証（2026-10-29 UTC）

Rootは現在のIssue本文・全改訂コメント、正式Planと基本設計、既存部分実装を直接確認した。
本文とnative関係のblocker #8／#16はともにstate_reason=completedで、現在の読取りAPI／CLI試験と
Detailの取得・世代管理・実端末受入記録も照合した。CodeGraphは未初期化のため初期化せずsourceを直接確認した。
既存の未コミット／untracked変更は保持し、Plan・設計・GitHub Issueの更新、commit／stash／破棄は行っていない。

独立fresh Review（修正前）:

- Correctness: `dfdeb1ae-b3b5-452a-b0b4-504e68534825`。
- Ponytail: `ae13e066-370d-4926-bde3-ec48bf9d7ad3`（ponytail-review Skill）。
- 両者のP1（TAB除去後のsecret再構成）とCorrectnessのP2（変更改行の不可視化）をRootが
  使用Ratatuiのsource／現在formatterと照合して採用し、上記の限定修正を依頼した。不採用の指摘はない。
- 修正後はfresh reviewer `63eff53b-9d3a-4ea4-9fbc-3a172eeca961` が安全表示の変更範囲を再確認し、
  P1／P2解消・追加のP0／P1／P2なし（OK with notes）と報告した。Rootも現在sourceと回帰結果を照合し、
  採用2件を解決済みと判断した。過去のReviewを新差分への承認に流用していない。
  これは技術レビューであり、未実施の実端末・実Backlog受入を完了とするものではない。

以下はWorker自己申告ではなくRoot自身が修正後に再実行した結果。ログは一時領域
`/tmp/backlog18-root-final.YSKot2/`。修正前のRoot結果は`/tmp/backlog18-root-checks.rFyXRD/`にあり、別buildの記録として区別する。

| Rootの修正後チェック | 結果 |
| --- | --- |
| `cargo fmt --all -- --check` | 終了0 |
| `cargo test --locked --all-targets` | 終了0、281成功・0失敗・実端末用1 ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | 終了0、警告なし |
| `cargo audit --file Cargo.lock`（0.22.2） | 終了0、1386 advisory／228依存、警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 終了0 |
| `git diff --check` | 終了0 |

監査前後のlockfile SHA-256はともに`55a7bd91c9d514359d18d225f0e3c293bb10e20339b47dc1aad6a421b51f8f14`。
修正後releaseのhashは上記Workerの最終buildと一致する（bl=`1f1ed32103cfbc2db3684ea8f078e002e1ed0e8d72396fa4fb3aa621317a44a9`）。

Rootは正式チェックとは別に、このreleaseを既存ローカルHTTPS／PTYから直接起動した。
文書fixtureは13 GET／10 snapshot、R失敗時16行目保持・成功時最新先頭交換、固定JST、H/L後の通信数保持を確認。
色あり／NO_COLOR × ダミーsecret `abcd`／`ab⇥cd` の4caseは各7 GET／5 snapshotで、
CommentとDetailのTAB可視化／変換後の伏字化、末尾改行の追加／削除表示、追加GETなしを確認した。
全5caseで終了0・restored=true・stderr空・GETのみ・secret／外部OSC非表示。背景色／取り消し線の合否はTestBackend cell試験で判断した。
修正前releaseでは45件巨大IDの `45→25→26→1→21→45`、exclusive両境界・最古の直接asc取得、
10,001 scalarの警告のみと同居履歴保持も直接確認済み。修正後のpaging回帰は全対象試験で再確認した。

実環境はdefault profileのspaceUrlが`https://lazybl.backlog.com/`であることだけ確認した。
このRootの環境に対応API keyはなく、認証情報を別の場所から補わず、実サービスへ接続していない。
**必須の実端末・実Backlog受入は未実施／利用者待ちで、Issue #18は未完了。**
対象hashの認証済み実端末で、上記の改訂後手順（BLCLI-1の839099888／839099889、JST、固定3行、
赤背景＋取り消し線／緑背景、NO_COLOR、100×24／40×12／極小、scroll、n/p/g/G/R、終了後のshell）を確認する。
API keyや機密本文は記録しない。実Backlog書込み・データ準備／復元は今回行わない。
GitHub Actions、実IME専用ignored試験、配布・公開は未実施。最終Code Review gateも別途確認する。


### #18 追加3点のWorker検証（2026-10-29 UTC）

対象は追加再計画「削除差分の視認性・未設定表示・Comment枠」。上記の旧観測・hash・停止理由を保持した。
今回のwriter編集は`src/comment.rs`／`src/tui.rs`、`tests/comment_preview.rs`／`tests/tui_process.rs`／
`tests/issue_comment.rs`、README／CONTRIBUTING／本記録の8ファイルだけ。
既存のtracked／untracked部分実装は保持し、Plan・設計・Service・API・Cargo・exports・fixtureは変更していない。
以下はWorkerの実測であり、Rootの独立再検証・Review・利用者の実端末受入ではない。

#### TDDとfocused回帰

承認済みの公開preview／Shell入力→TestBackend cell／実bl→既存PTY・HTTPS／CLI stdout・JSONを使った。
一つのRED→最小GREENの縦sliceで実施した。

- 色: `cargo test --locked --test comment_preview description_inline_diff_keeps_common_unicode_and_marks_only_changed_graphemes -- --exact`
  はRED終了101（Whiteとliteral Rgb(255,255,255)の不一致）。
  `cargo test --locked --lib tui::tests::comment_fixed_rows_styles_and_body_scroll_share_the_viewport -- --exact`
  もRED終了101（White/Redとliteral RGB pairの不一致）。削除styleとNO_COLOR／変更改行の背景判定を一緒に整合し、
  preview全9件／Shellのcomment全5件がGREEN終了0。追加Black／Greenは変更していない。
- null: `cargo test --locked --test comment_preview preview_only_top_level_null_is_unset_for_known_description_and_unknown_fields -- --exact`
  はRED終了101（`START DATE: null -> null`と`START DATE: 未設定 -> 未設定`の不一致）。
  previewに元JSON値のnull分岐だけ追加し、preview全10件がGREEN終了0。
  既知startDate／assigner・description・未知fieldの前／後／両方null、入れ子null・文字列null・空文字・0・false・巨大数値、
  本文同居・原値不変・最終投影でのsecret再検査を確認。CLI／JSONの従来契約も別の実process試験で確認した。
- title: `cargo test --locked --lib tui::tests::comment_title_uses_only_needed_spaces_and_preserves_body_pipes -- --exact`
  はRED終了101（idle titleに`|`と余分な空白）。Shellのtitle組立だけ修正して同じ1件がGREEN終了0。
  100×24／40×12のfocus、全パネルでの非focus、本文の`|`、NO_COLOR全cell Resetを確認。
- PTY拡張の初回は終了101（3件）。製品不具合ではなく、簡易画面のCJK幅2の空cellを含む期待、
  既存stepsの終端snapshot index、空一覧sequenceの総GET期待（8ではなく9）を訂正した。
  新harness・製品の取得／状態管理変更は不要だった。再実行は下表どおり全成功。

| 最終focusedコマンド | 結果 |
| --- | --- |
| `cargo test --locked --test comment_preview` | 終了0、10成功 |
| `cargo test --locked --lib tui::tests::` | 終了0、24成功・実端末用1 ignored |
| `cargo test --locked --test tui_process comment_` | 終了0、9成功 |
| `cargo test --locked --test tui_shell --test issue_comment` | 終了0、shell 8成功・CLI 15成功 |

NO_COLORの記号・全cell fg/bg Reset、変更改行の↵と実改行、TAB可視化・Unicode・旧新値再構成、
JST固定3行・14/6・40×12／極小・resize／scroll、10,000／10,001 scalar、安全表示は既存試験も含め再成功した。
PTYはidle／Loading／空一覧／終端／取消／失敗popupと通知消去、本文／履歴の`|`保持を確認。
既存n/p/g/G・exclusive両方向・最古直接asc・R失敗保持／成功交換・古い成功／失敗と新busyの非破壊、総GET数も再確認した。
新規空一覧／取消／失敗sequenceは100×24と40×12で各9 GET（comments 3）、表示だけの追加通信なし。

#### 正式チェックとrelease直接再現

| コマンド | 結果 |
| --- | --- |
| `cargo fmt --all -- --check` | 終了0 |
| `cargo test --locked --all-targets` | 終了0、285成功・0失敗・実端末用1 ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | 終了0、警告なし |
| `cargo audit --file Cargo.lock` | 終了0、1386 advisory／228依存、警告なし。`cargo-audit --version`は0.22.2 |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 終了0 |
| `git diff --check` | 終了0 |
| `git diff --cached --quiet` | 終了0、staged変更なし |

監査前後のCargo.lock SHA-256は`55a7bd91c9d514359d18d225f0e3c293bb10e20339b47dc1aad6a421b51f8f14`で不変。
変更後release SHA-256:

- `bl`: `7af5fbc50717dc09d79fac5a8628bcd75d506fd606c24996bbce5ed6c437d6c0`
- `bl-cli`: `0d0e53e0397d15df5ad065713012e819dc85369b9b950b8e91e1a6bc887e8cbe`

CONTRIBUTINGの既存ローカルHTTPS再現手順をこのrelease blで直接実行（NO_COLOR=1、既存文書fixture／steps）。
製品終了0・restored=true・stderr空・13 GETのみ、request_counts=`[4,9,9,9,10,10,13,13,13,13]`。
R失敗前後の16行目保持→成功先頭交換、末尾の未知履歴、生成`|`なし、secret／外部OSC非表示をassertして終了0。
一時root・fake設定／TLS／serverは終了時に削除した。配色／取り消し線はPTY plain画面の合否にせず、literal RGBのTestBackend cell試験で確認した。
一時ログは`/tmp/issue18-*.log`、このreleaseのPTY結果は`/tmp/issue18-release-pty.json`（いずれもfixtureのみ）。

**実端末の可読性・実Backlog受入は未実施**。実サービス・利用者config・実API keyは使っていない。
実IME専用ignored試験・GitHub Actions・独立Review／最終Code ReviewもこのWorkerでは実施していない。
今回の自動成功はIssue #18全体の完了判定ではない。Rootの独立reviewと、変更後hashを用いた上記の必須実端末／実Backlog手順で再開する。
RGB・取り消し線の端末描画差、既存Paragraphのu16 scroll上限・similarの近似deadlineは維持している。

### #18 追加3点buildの利用者確認と幅0 grapheme指摘修正（2026-10-29 UTC）

#### 修正前buildの実端末・実Backlog受入（利用者報告）

上の追加3点release `bl` SHA-256
`7af5fbc50717dc09d79fac5a8628bcd75d506fd606c24996bbce5ed6c437d6c0` に対し、
利用者は列挙した確認項目を「すべて確認済み・問題なし」と明示回答した。
端末はHerdr 0.8.0／Ghostty 1.3.1。BLCLI-1のcomment `839099888`／`839099889` の全削除／全追加、
JST `01:05:08`／`01:05:10`、固定配置・title・折返し／scroll、n/p/g/G/R、
100×24／40×12／極小からの復帰、NO_COLOR記号、終了後のshell復元を確認済みとの報告である。
Rootが同じ実TUIを直接観測したものではなく、利用者の確認に基づく記録である。
Rootはdefault profile origin `https://lazybl.backlog.com/` を確認したが、対応keyがないため実サービスへ接続していない。
Root／今回Workerのローカル環境はmacOS 26.7.1 arm64。これは利用者の端末側OSの独立確認ではない。
この記録で過去の未実施記録を消さず、以下の幅0修正後hashへの受入流用もしない。

Rootの同修正前buildでの正式チェックは `/tmp/backlog18-root-checks.EXWejR/` に記録され、
285成功・0失敗・実端末用1 ignored、fmt／Clippy／audit／release build／diff checkは終了0だった。
Rootの直接release検証6caseは
`/var/folders/04/cs5vwntd34d_fdbtdczbxg000000gn/T/backlog18-root-direct.hrg7anrv/summary.json`。
文書両色各13 GET／10 snapshot、型付き値両色各9 GET／3 snapshot、paging 12 GET、10,001警告7 GETが成功した。
これらは修正前buildの観測としてのみ保持する。

#### 独立Reviewの採用P1と最小修正

独立Correctness／Ponytail Review（出力run `62fa3ced-b7b1-4379-a057-8a58b0deac36`）が共通して報告した
P1一件だけをRootが採用した。ダミーsecret `abcd` に対する `ab\u{200b}cd` はpreview文字列にsecretがなくても、
Ratatui Paragraphが単独幅0 graphemeをskipした後のcellでは `abcd` となる。
Rootの `/tmp/backlog18-zero-width.rs` による普通本文／description × 両色の全4case再現と、
使用中Ratatui `paragraph.rs` の幅0 skipを根拠に、既存の最終投影秘匿契約内の漏洩として修正した。

- `src/tui_text.rs` の既存共有描画copyで、TAB→`⇥`に続き、Ratatui公開grapheme／width APIで
  幅0のgraphemeだけ各scalarの可視 `\u{...}` escapeへ変換してから、全文投影をsecret再検査する。
  表示幅を持つ結合文字／ZWJ emoji clusterは分割・一括削除しない。原API値・CLI／JSONは変更しない。
- `src/tui/comment.rs` の固定作者は単行化後・省略前、日時は幅判定前に同じ共有seamを使う。
  overflow全文prefix・省略後ヘッダー・本文の再構成後の最終検査も維持する。
  変換や省略でsecretが構成された場合は既存のredaction／plain fallbackを優先する。
- writer変更ファイルは上記2ファイル、公開Shell回帰を置く `src/tui.rs`、`tests/comment_preview.rs`、
  本記録とREADME／CONTRIBUTINGの計7ファイルだけ。README／CONTRIBUTINGは受入状態を最新記録へ参照させた。
  既存のdirty／untrackedを保持し、Plan・設計・Cargo／lockfile・API／Service・CLI・exports・fixtureは変更していない。

承認済み公開seamでのTDD:

| コマンド | RED → GREEN |
| --- | --- |
| `cargo test --locked --test comment_preview preview_zero_width_graphemes_cannot_reassemble_secrets_in_rendered_cells -- --exact` | 101（実Paragraph cellに`abcd`）→ 0 |
| `cargo test --locked --lib tui::tests::comment_zero_width_graphemes_are_safe_in_fixed_rows_overflow_and_body_cells -- --exact` | 101（固定作者cellに`abcd`）→ 0 |

普通本文・同値description、固定作者／不正日時fallbackと両方のoverflow全文、Detail共通seam、
40×12／100×24、両色、元データ非改変、完成cellにsecretがないことを検証した。
結合文字 `é`／ZWJ emoji `👩‍💻` はcellに保持する。可視escapeで新構成されるsecret `ab\u{200b}cd` の伏字化と、
作者の省略記号 `…` で再構成されるsecretの最終redactionも追加Shell回帰で確認した。
Shell試験の初回GREEN実行は、40×12で末尾scrollしたまま100×24へ移ったためprefix先頭を期待する補助assertに失敗した。
公開Homeキーで各サイズの先頭に戻す試験手順へ訂正し、製品変更なしで成功。
追加の省略回帰の初回compileはformat文字列のescape括弧不足で101、テスト文字列だけ訂正して成功した。

#### Workerの修正後チェック・release直接検証

ログ: `/tmp/backlog18-worker-zero-width/`。以下はWorker自身の結果であり、Rootの再検証・Review承認とは別である。

| コマンド | 結果 |
| --- | --- |
| `cargo test --locked --test comment_preview` | 終了0、11成功 |
| `cargo test --locked --lib tui::tests::` | 終了0、26成功・実端末用1 ignored |
| `cargo test --locked --test tui_process comment_` | 終了0、9成功 |
| `cargo test --locked --test tui_shell --test issue_comment` | 終了0、shell 8成功・CLI／JSON 15成功 |
| `cargo fmt --all -- --check` | 終了0 |
| `cargo test --locked --all-targets` | 初回101（下記1件）、無変更の再実行で終了0、288成功・0失敗・実端末用1 ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | 終了0、警告なし |
| `cargo audit --file Cargo.lock`（0.22.2） | 終了0、1386 advisory／228依存、警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 終了0 |
| `git diff --check`／`git diff --cached --quiet` | 終了0、staged変更なし |

all-targets初回の失敗は既存 `region_memory_survives_dialog_cancel_failure_success_project_switch_and_resize` だけ。
`tty_shell.py` の開始から8秒の総budgetが、40×12のCommentから極小へresizeするstepの直前で尽きた。
終了時画面には待機対象 `> Comment` が既にあり、製品エラー・漏洩のassert失敗ではない。
原因を断定せず、製品・harness・timeoutの変更や失敗抑制はしていない。
同一テストのexact単独再実行は終了0（5.26秒）、その後の全対象1回再実行も上記の通り成功した。
ログは `all-targets.log`（初回）、`region-rerun.log`、`all-targets-rerun.log`（最終成功）。
既存TAB／Span境界／変更改行／10,000境界・paging／R／応答逆転・CLI／JSONも含む全回帰を再成功した。

監査前後のCargo.lock SHA-256はともに
`55a7bd91c9d514359d18d225f0e3c293bb10e20339b47dc1aad6a421b51f8f14` で不変。
修正後release SHA-256:

- `bl`: `fa4da3abd8d4b4b13a3cc40b697f8fa27fb6b37557489acc5d5a40ddc37fb5f2`
- `bl-cli`: `9b210acd6adaa1956f992663a17762e5e5d9c2e407ade5771e018efdef5c59bf`

このreleaseで既存 `/tmp/backlog18-root-direct.py` をWorkerが実行し、8case全て終了0。
結果は `/var/folders/04/cs5vwntd34d_fdbtdczbxg000000gn/T/backlog18-root-direct.fq1a1sri/summary.json`、
実行出力は `release-direct.log`。既存HTTPS／PTYを使い、文書両色各13 GET／10 snapshot、
型付き値両色各9 GET／3 snapshot、paging `45→25→26→1→21→45` は12 GET／comments 6回／exclusive境界／最古直接asc、
10,001警告は7 GET、U+200Bを普通本文・description・固定作者／不正日時へ同居させた両色各9 GET／2 snapshotを確認。
文書のR失敗時scroll保持→成功最新交換・固定JST・H/L後総GET不変、null／入れ子／title／本文`|`も再確認した。
全caseで製品終了0・restored=true・stderr空・GETのみ・secret／外部OSC非表示。
生成したfake設定／TLSは実行後削除済み。利用者config・実key・実サービスは使用していない。
色／取り消し線の合否はPTYのplain screenではなくTestBackend cellで判定する。

**幅0修正後の実端末・実Backlog必須受入は未実施で、Issue #18は未完了。**
上の旧hashへの利用者確認をこの新hashへ流用しない。Rootが新hashでの実端末／実Backlog再受入を利用者へ依頼する。
Rootの独立再検証・fresh Review／最終Code Reviewも別途必要で、Workerは代行しない。
実IME専用ignored試験・GitHub Actions・配布／公開は未実施。端末字幅・RGB／取り消し線の目視可読性は自動試験で保証しない。
既存のu16 scroll上限・similar近似deadlineと、8秒PTY総budgetによるタイミング依存の残余リスクは変えていない。

### #18 半角濁点／半濁点の保持 — 採用P2の限定修正（2026-10-29 UTC）

#### 修正前hashへのRoot検証・利用者確認（今回の変更後へ流用しない）

上記release `bl` SHA-256
`fa4da3abd8d4b4b13a3cc40b697f8fa27fb6b37557489acc5d5a40ddc37fb5f2` に対し、
利用者は必須の実端末・実Backlog項目をすべて確認し問題なしと明示回答した（Rootからの引継ぎ）。
Herdr 0.8.0／Ghostty 1.3.1、BLCLI-1のcomment `839099888`／`839099889` の全削除／全追加、
通常コメント／本文なし履歴、削除の可読性・追加との差、JST・固定行・title・n/p/g/G/R、
100×24／40×12／極小、NO_COLOR、終了後のshell復元についての**利用者の操作・目視報告**である。
Root自身による実TUI観測ではない。RootはmacOS 26.7.1 arm64、default profileのorigin
`https://lazybl.backlog.com/` を確認済みだが対応keyはなく、実サービスへ接続していない。

同hashでのRoot正式チェックは `/tmp/backlog18-root-final.MxscCC/`：
fmt／全対象288成功・1 ignored／Clippy／audit 0.22.2（1386 advisory・228依存、警告なし、lock不変）／release／diffは各終了0。
直接8caseは `/var/folders/04/cs5vwntd34d_fdbtdczbxg000000gn/T/backlog18-root-direct.vyw3_5dv/summary.json` で全成功。
元のU+200B再現も全4caseで `rendered_contains_secret=false`。
以上はRootの引継ぎ結果であり、今回Workerの再実行結果でも、以下の変更後buildの受入でもない。

#### P2の原因・最小修正とWorkerのRED → GREEN

fresh security followup（run `31b5949f-fc18-43fe-a68b-0f5b92ab2695`）のP2をRootが採用した。
単独U+FF9E `ﾞ`／U+FF9F `ﾟ` は `Span::width()` では0だが、使用中Paragraphの公開
`CellWidth::cell_width()` では1であり、本来描画できる文字までescapeしていた。
Rootの独立再現でも `ﾞA`／`ﾟA` がliteralな `\u{ff9e}A`／`\u{ff9f}A` へ変わっていた。
共有helperの幅0判定だけをParagraphと同じ公開APIへ合わせ、不要なSpan importを除去した。
ヘッダー幅判定・省略方式、最終secret検査、TAB可視化は変更していない。

既存の公開 `comment::preview` → Paragraph／TestBackend回帰に、行頭の単独半角濁点／半濁点を追加した。
通常本文／同値description × 色あり／NO_COLORで両方のliteralな可視cellを確認する。
既存U+200Bのescapeとsecret非表示、結合文字 `é`／ZWJ emoji `👩‍💻` の保持、CLI／原値不変のassertも残した。

| Workerのコマンド | 終了コード／結果 |
| --- | --- |
| `cargo test --locked --test comment_preview preview_zero_width_graphemes_cannot_reassemble_secrets_in_rendered_cells -- --exact`（修正前） | 101 RED、描画cellが `\u{ff9e}A`／`\u{ff9f}A` でliteralな `ﾞA` のassertに失敗 |
| 同じexactコマンド（helper修正後） | 0 GREEN、1成功 |
| `cargo test --locked --test comment_preview` | 0、11成功 |
| `cargo test --locked --lib zero_width` | 0、固定行／overflow／本文・省略後secretの既存2件成功 |
| `cargo fmt --all -- --check` | 0 |

一時ログは `/tmp/backlog18-worker-halfwidth/` の `red.log`／`green.log`／`preview.log`／`zero-width.log`／`fmt.log`。
今回のwriter変更は `src/tui_text.rs`、`tests/comment_preview.rs`、本記録だけ。既存dirty／untrackedは保持した。
Plan・設計・Cargo・API／CLI・exports・GET契約は変更せず、実サービス接続・公開・commit／stash／resetも行っていない。
今回Workerは全対象test／Clippy／audit／release build・新hash取得・実端末／実Backlog操作を実施していない。
Rootが変更後の正式チェック・release buildとhashの確認・最終受入を続ける。Issue全体の完了判断はしていない。
既存の端末字幅差・u16 scroll上限・近似deadline・PTY総budgetのタイミング依存リスクは維持する。

### #18 最終差分のRoot検証・利用者受入（2026-10-29 UTC）

対象は半角濁点P2の修正後release。過去buildのチェック・Review・受入・Approveは流用せず、
Rootが実差分・caller・使用中ParagraphとCellWidthの一次ソースを確認して以下を再実行した。
既存の未コミット／untracked変更を保持し、Plan・設計・Issue・依存の変更、commit／stash／破棄は行っていない。

| Rootの最終コマンド | 結果 |
| --- | --- |
| `cargo fmt --all -- --check` | 終了0 |
| `cargo test --locked --all-targets` | 終了0、288成功・0失敗・実端末／IME用1 ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | 終了0、警告なし |
| `cargo audit --file Cargo.lock`（0.22.2） | 終了0、1386 advisory／228依存、警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 終了0 |
| `git diff --check` | 終了0 |

ログは `/tmp/backlog18-root-complete.rE7cRL/`。監査前後のlockfile SHA-256はともに
`55a7bd91c9d514359d18d225f0e3c293bb10e20339b47dc1aad6a421b51f8f14` で不変。
最終release SHA-256:

- `bl`: `7875fbde578b86a0b7cbc20b0802279823e0e2edcc77c4c75cd7564d0c58dacd`
- `bl-cli`: `dcc043ce1358e722d67e03433870da25b4841d4e4348ddb8c7d80d731c0b908d`

Rootは正式チェックと別に、このreleaseを既存HTTPS／PTYから直接起動した。
結果は `/var/folders/04/cs5vwntd34d_fdbtdczbxg000000gn/T/backlog18-root-direct.kwh2vu3t/summary.json`。
文書の色あり／NO_COLOR各13 GET・10 snapshot、型付き値の両色各9 GET・3 snapshot、巨大ID pagingは12 GET／comments 6回、
10,001 scalar警告のみ7 GET、幅0の本文・履歴・作者・不正日時の両色各9 GET・2 snapshotの全8case成功。
R失敗時scroll保持と成功時最新先頭交換、固定JST・title・本文の`|`・null／入れ子の区別、
`45→25→26→1→21→45`、exclusive両境界・最古の直接asc・H/Lで追加GETなしを確認した。
全caseで終了0、restored=true、stderr空、GETのみ、secret／外部OSC非表示。
型付き値caseには半角濁点／半濁点も加え、実製品画面でliteralな文字の保持を両色確認した。
元の幅0 scratchを現libraryで再コンパイルした4caseは全て `rendered_contains_secret=false`。
半角濁点scratchも `ﾞA`／`ﾟA` がそのままpreviewに残ることを再確認した。

独立Reviewの採否・修正確認:

- fresh Correctness `b1d676ad-7dbe-4b14-a5d1-b9df9c28f238` とfresh Ponytail `a2414b6c-0dd8-4d37-a4fa-8ede20272df7`
  は完了。共通P1（幅0によるsecret再構成）をRootの独立再現で採用し、共有描画境界と固定行に限定して修正した。
- fresh followup `31b5949f-fc18-43fe-a68b-0f5b92ab2695` はP1解消を確認。追加P2（半角濁点の過剰escape）も
  Rootが依存ソースと独立scratchで再現して採用し、Paragraphと同じ公開CellWidthへの判定置換だけを依頼した。
- 最後のP2修正はRootが実差分・公開preview／cell回帰・元再現と上記全チェックで解消を確認。
  1つの公開APIへの限定置換のため全面Review loopは追加していない。不採用・未解決の指摘はない。
  Worker初回の既存PTY 8秒timeoutは上記履歴のとおり保持。timeout／製品を変更せず、単独・全対象再実行とRootの最終全対象が成功した。

利用者は上記の最終 `bl` hashで必須項目を「すべて再確認済み・問題なし」と明示回答した。
環境もHerdr 0.8.0／Ghostty 1.3.1、macOS 26.7.1／Apple Siliconと明示確認済み。
接続先は `https://lazybl.backlog.com/`、対象はBLCLI-1のcomment `839099888`／`839099889` と通常コメント／本文なし履歴。
全削除の白文字＋暗赤背景＋取り消し線の可読性・全追加との区別、JST `2026/10/27 01:05:08`／`01:05:10`、
投稿者→日時→空行の固定配置、title、折返し／scroll、n/p/g/G/R、100×24／40×12／極小復帰、
NO_COLORの記号・終了後のshell復元を確認済みとの**利用者の実操作・目視報告**である。
Root自身の実Backlog観測ではなく、Rootはprofile originのみ確認し、対応keyを取得・補完していない。
実Backlogへの書込み・検証データ作成／変更／復元は行わず、key・機密本文・raw responseは記録していない。

元Issueのpaging／本文・履歴／focus・scroll・R／古い応答・失敗非破壊、両改訂の表示・安全性・検証・実環境条件は上記の通り確認済み。
最終Code Review gateはこの記録後にRootが別途呼び、toolのskippedまたは明示Approveを確認するまで最終完了とはしない。
GitHub Actions・実IME専用ignored試験・配布／公開は未実施。既存u16 scroll上限、近似diffのdeadline、
他端末の字幅・装飾差、PTY総budgetによるタイミング依存の残余リスクは維持する。merge／release公開／deployは行わない。

### #18 / PR #49 CI失敗後のテスト同期修正

対象revisionは `6f87dbca1a98965370ebc0966313a00291e4974d` + 本テスト／記録の未コミット差分。
CI run `40920054323` / job `123610524797`（ログ `/tmp/backlog49-ci-failed.log`）では
`tui_process` が64成功・1失敗。`comment_full_layout_fixed_header_and_compact_body_tail_are_reachable` は
8秒期限に達し、未実行stepは `> Comment` 待ちのEndと末尾30行目待ちのq、最終画面は `> Project List` だった。
終了待ちではなくfocus前提が未成立だった。修正前のRoot単独1+20回、8並列80回は全成功し、ローカルでは再現していない
（`/tmp/backlog49-repro.uyzjBN/`、`/tmp/backlog49-parallel-repro.Q4rqC1/`）。

同stepの `Ll` + resizeを分け、L→Detail描画→l→Comment描画→resize→本文描画→Endと既存waitで同期する。
full初期／compact先頭・末尾の投稿者・JST、compactのComment focusと末尾30行目、初期／末尾のGET数不変をassertする。
製品・driver・fixture・依存・CI・仕様は変更せず、timeout延長・retry・skip・sleep追加も行わない。

- `cargo test --locked --test tui_process comment_full_layout_fixed_header_and_compact_body_tail_are_reachable -- --exact`：終了0、1成功。続けて同コマンド20回もすべて終了0（`/tmp/backlog49-worker-sync.6yf3z7/`）。
- `cargo fmt --all -- --check`：初回1（assertの折返しのみ）、整形後0。
- `git diff --check`：終了0。

ローカルの再現可能なRED→GREENやCI根因の確定、修正後CI合格を主張しない。
Rootが変更後に `cargo fmt --all -- --check`、`cargo test --locked --test tui_process`（65成功）、
`cargo test --locked --all-targets`（288成功・0失敗・実IME用1 ignored）、
`cargo clippy --locked --all-targets -- -D warnings`、`git diff --check` を再実行し、すべて終了0・警告なし。
ログは `/tmp/backlog49-root-checks.p6hn44/`。
正式チェックとは別に、既存releaseをローカルHTTPS／PTYでこの5stepから直接起動し、
投稿者・JST・full／compact先頭／末尾、Comment focus、9 GET不変、終了0・端末復元成功・stderr空を確認した
（`/var/folders/04/cs5vwntd34d_fdbtdczbxg000000gn/T/backlog18-root-direct.vhuz4wre/ci-synchronized-layout/`）。
既存8caseも同じreleaseで再成功。製品・依存の変更がないためaudit／release再buildは追加していない。
本修正では実端末／実Backlog操作・書込みは行わず、過去の受入・hash記録はそのまま保持する。
fresh Correctness Review `9c3d8308-04c9-45e9-b64a-46583c44dfe7` と
fresh Ponytail Review `717fd902-fd05-42bd-8747-96ba3c1591e1` は、対象差分の指摘なし。
Rootも差分と公開入口の実行結果を照合し、検証条件の維持・同期修正の範囲を確認した。
修正後のCI結果は、変更を反映して再実行するまで未確認とする。

## Issue #19 editor連携

### 対象・現在の判定（2026-10-30 UTC）

**実装と自動検証まで。実端末の利用者目視、Root最終確認、独立2レビュー／Code Review gateは未実施であり、READY／Issue完了とはしません。**
前runは30分timeoutでfailedとなり、Rootが差分保持を確認した後、明示的に再開しました。
再開後は文書・再現手順と限定再検証を完了し、長い全対象testを不要に再実行していません。

対象は設定選択からeditorへの端末引渡し／復帰と、共有libraryの本文編集です。
後続#20〜23のフォーム・製品用demo／debug flag・実Backlogへの接続／書込みは追加していません。
Rust 1.99.1／Darwin arm64。release `bl` のSHA-256は
`e9f94ea1fab2a610f3f95602164cafc1b780ed1e7b4f158c9655a534b90495d6`、
`bl-cli` は `9eac8fe2d350d0da94de903f15d02ea41da1ab68ff0c389ad5b9589a025dbeaf`。
release作成後のソース変更はcfg(test)のシグナル終了／temp削除assert追加だけで、製品コードは同じです。

### serial TDDと観測境界

各sliceは公開library操作、公開Shell入力→TestBackend描画、実bl／test binary→PTYを使いました。
内部private関数形状を固定するtestは追加していません。red／greenはそれぞれ終了101／0です。

| slice／実行filter | redの観測 → greenの観測 |
| --- | --- |
| `cargo test --locked --test editor`（初回1件） | 公開editor入口なしでcompile失敗 → 生本文・0600／所有者・literal argvをeditorへ渡し、正常交換とtemp削除 |
| 同testの `body_interrupt` | 中断を無視して正常交換 → 子停止／reap、元値保持、temp削除、interrupted |
| `cargo test --locked --test tui_shell workspace_settings` | `*`で対象選択を描画しない → 2対象の上下／j/k循環、Esc取消、元focus保持 |
| `cargo test --locked --test tui_process settings_editor_owns` | editorが起動せずPTY待機timeout → cooked入力専有、editor中resize、追加キーなしの40x12再描画 |
| `cargo test --locked --lib tui::tests::body_editor_roundtrip` | ignored本文経路がなく記録file不在 → 同じloop／guardで本文editor往復 |

その後の拡充で、共通library5件・設定PTY6件・本文PTY1件（7経路）を確認しました。
設定PTYは未作成の場所案内、非0後も保存を保持／同じeditor再起動と同じoriginでR、起動時検証後のprogram削除、
親SIGINTの130、選択課題／ページ／Detail末尾scroll／左右focus記憶と総GET不変もassertしています。
本文PTYは正常／非0／editorだけのSIGTERM／不正UTF-8／読取不能（file削除）／親SIGINT／起動失敗です。
原文CRLF・別欄値／focusの保持、表示だけsecret伏字、TMPDIRに一時本文が残らないことを観測しました。
共通libraryは同時readyの親中断を子の正常終了より優先することも検証しています。

### コマンドと終了コード

前runのログは `/tmp/issue19-*.log`、再開後は `issue19-resume-*.log` と `issue19-direct-pty.log` にあります。
一時ログはリポジトリへ格納せず、再実施可能なコマンドと期待をCONTRIBUTINGへ保存しました。

| コマンド | 結果 |
| --- | --- |
| `cargo fmt --all -- --check` | 前run／再開後とも0 |
| `cargo test --locked --all-targets` | 前run最終完走0、301成功／2 ignored。最新test-only拡充前の全対象結果で、再開後に全体再実行はしていない |
| `cargo clippy --locked --all-targets -- -D warnings` | 前run0／再開後の最新差分0、Rust warningなし |
| `cargo audit --file Cargo.lock`（cargo-audit 0.22.2） | 前run0、1386 advisory／230依存、監査warning・脆弱性報告なし。再開後は依存不変のため再実行なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 前run0、上記2binary。再開後は製品コード不変のため再実行なし |
| `cargo test --locked --test editor --test tui_shell` | 再開後0、5＋9件成功 |
| `cargo test --locked --test tui_process settings_` | 前run最終0、6件成功 |
| `cargo test --locked --lib body_editor_roundtrip_and_failures` | 再開後0、最新7経路すべて成功 |
| CONTRIBUTINGのrelease直接PTY手順を抽出して`bash /tmp/issue19-direct-pty.sh` | 再開後0、正常／非0／中断／起動失敗の4case、詳細は下記 |
| `cargo test --locked --lib --no-run --message-format=json`→文書のtest binary抽出→`--list` | 再開後0、本文ignored testの実行fileとexact名を確認（実端末起動自体は未実施） |
| `git diff --check`／staged files確認 | 再開後0／stagedなし |

lockfileは承認済み`tempfile =3.27.0`と推移fastrand 2.5.0だけ追加し、既存の第三者依存版は変えていません。
変更前SHA-256は`55a7bd91c9d514359d18d225f0e3c293bb10e20339b47dc1aad6a421b51f8f14`、
監査前後と再開時はともに`f9ab064dd113a477d2fc897401c8ce3ca95a1f5d7582b8c1472bef61117f5a3a`で不変でした。

途中の失敗も成功へ読み替えません。TDDのred以外に、未公開backend writer APIへのcompile失敗、
重複`Terminal::clear`のcursor応答待ち失敗があり、既存guardへ戻すためTerminalをdrop／再作成し、
公開`resize`によるclear＋diff buffer resetへ修正しました。fixtureではCRLFのPython読取正規化、
同時送信EscのAlt解釈、editor marker位置と簡易VTが対応しない省略cursor座標の問題でassert／timeoutが発生しました。
原文はbytes decode、Escは別step、markerは明示座標で修正し、最終の対象試験は成功しています。
最初の全対象実行は240秒のworker tool timeoutで打切り、次の失敗runはfixture座標問題で終了101、
その後の上記301件完走とは区別します。

**警告の影響判断:** 完走したall-targetsログ408〜435行に、既存HTTPS fixtureの`https_server.py:80`
（body送信）で`ConnectionResetError: [Errno 54] Connection reset by peer`のtracebackが1件あります。
並列の中断／切替試験中のclient切断と整合するserver側のログで、Cargoは0、全assert成功です。
Rust warningや製品stderrの成功扱いではありません。個別の起点request／testの同定は未実施で、無関係なfixture例外処理は変更していません。
再開後の限定試験とrelease直接4caseではこのtracebackはなく、driver／製品／serverのstderrは期待どおりです。
Rootによる正式再検証で同じwarningが出た場合も隠さず記録してください。

### release製品の直接観測と未確認

CONTRIBUTING「editor連携 (#19)」のPython blockそのものを抽出・実行しました。
一時root／TLS／config／fake keyと既存HTTPS／PTY fixtureのみで、終了後にserverとfileを回収しました。

| case | 期待と実測 |
| --- | --- |
| 正常 | bl=0、restored=true、4 snapshots、q/j/kを含む`qjk\n`をeditorが全受信、ICANON／ECHO=true、editor中40x12 resizeから通知と元Detailへ即時復帰 |
| 非0保存 | bl=0、restored=true、4 snapshots、editor非0を通知して復帰、保存済み`{"saved":true}`を巻き戻さない |
| 親SIGINT | bl=130、restored=true、2 snapshots、stderrに中断、子PIDが残らない |
| 起動失敗 | config検証後にprogram削除、bl=0、restored=true、3 snapshots、通知後に元Detailへ復帰 |

いずれも実release binaryを直接使い、server stderr空。中断以外の製品stderr空。
本文UTF-8／読取失敗／temp保護削除は製品設定編集とは異なるため、共通lib＋同じloopのcfg(test)本文PTYで確認しました。
製品に未実装の本文フォームを追加して直接試したとは報告しません。
cleanup失敗／復元操作自体の失敗は成功として隠さない実装ですが、今回editor往復でのOS cleanup失敗注入はしていません。
editor固有swap／backup、SIGKILL・端末切断後の復元／削除は保証対象外です。

### 実端末の必須受入：利用者待ち

正確なセットアップ／起動は[CONTRIBUTINGのeditor手順](../../CONTRIBUTING.md#editor連携-19)を参照してください。
実Backlog・利用者config・機密本文は不要です。既存「ローカルHTTPSを使う準備」の専用`run_bl`と、
以下の本文ignored testを利用します。

```sh
cargo test --locked --lib tui::tests::manual_body_editor_terminal -- --ignored --exact --nocapture --test-threads=1
```

1. 今回の端末名／版、macOS、IME、TERM／NO_COLOR、release／test binaryのbuildを記録する。
2. 製品`*`の2対象循環・Esc取消・既存config起動・不在settings非生成、既存settings編集を確認する。
3. vim等の実editorで日本語／結合文字／emojiと空白・改行を編集し、editor中に100x24→40x12→100x24へresizeする。
   終了後**追加キーなし**でclear／全体再描画し、選択・検索・ページ・scroll・左右focus記憶が保持されることを目視する。
4. 本文testでTabで他欄編集→Escで欄外→e。保存成功だけ本文交換、非0／不正UTF-8／read失敗／起動失敗では元本文と他欄・focusを保持する。
   `BL_TEST_EDITOR_ARGV`による各失敗の最小設定はCONTRIBUTING記載。極小時はq／Ctrl+Cだけで安全に終了する。
5. editorだけの非0終了で通知して復帰することと、別端末から親PIDへのSIGINTで子停止／temp削除／130になることを分けて確認する。
   130はcargo wrapperではなく、文書の直接test binary起動手順で観測する。
6. 終了後に通常shellで入力／削除・cursor／表示属性・`stty -g`を確認し、結果と未確認点をこの節へ追記する。

**今回の実端末目視結果はすべてpendingです。** 既存#14受入や自動PTY成功を#19の実端末受入へ流用しません。
Root最終確認・独立2レビュー／Code Review gate・GitHub Actionsも未実施。実Backlog書込み・commit／push／release公開はしていません。

### P1 correction：raw／noechoを残すeditorからの復元（2026-10-30 UTC）

上記の成功記録はその時点の観測として保持します。ただし従来のeditor fixtureは引渡し時の
cooked／echoを確認するだけで、自分でrawへ変更しませんでした。その成功だけでは異常終了した
実editorからの復元を裏付けられません。Rootの独立2レビューが同じP1を採用し、修正前releaseでは
rawを残すeditorの親SIGINT／非0終了後に`restored=false`を再現しています。
その修正前証跡と以下のWorker修正後ログは別物です。
**今回も実端末利用者目視・修正後Root最終確認／Code Review gateはpendingです。**

今回の追加変更は単一TerminalGuard・最小補助依存・既存fixture／PTY試験とこの記録／CONTRIBUTINGだけです。
起動前termiosをguardで一度保持し、editor実行中も復元を有効にしたまま、子終了／停止後は
親中断130のreturn／TUI再enterに先立って復元を試みます。editorが残したraw状態で復元先を上書きしません。
復元失敗ではTUIを再開せず、元のeditorエラー／中断と復元エラーの両方を返します。
既存のpartial enter／panic／最終cleanupも同じguardを使用します。

Crosstermの公開APIではhandoff後まで保持できるtermios snapshotを取得できないため、
既存lock解決済み`rustix =1.1.6`（`termios` feature）の安全な公開`tcgetattr`／`tcsetattr`を直接利用しました。
独自FFI・外部stty process・別端末管理層は不要です。Cargo.lockは直接依存の辺1行だけ追加し、
今回のcorrectionでは新crate／既存第三者依存の版変更はありません（元のtempfile／fastrand追加は保持）。

#### TDD／回帰と直接観測

Worker一次ログ：`/tmp/backlog-issue19-correction.oddf58/`。
Rootの修正前ログを修正後成功の証拠として流用していません。

| コマンド／観測 | 今回の実測 |
| --- | --- |
| `cargo test --locked --test tui_process settings_editor_parent_interrupt_stops_raw_child` | red.log：101、親SIGINTは130だが`restored=false`、termios差分あり → green.log：0、同じ公開PTY回帰成功 |
| `cargo test --locked --test tui_process settings_` | settings.log：0、7 test成功。raw親SIGINTで子消滅／130、raw非0後のeditor再起動と保存保持、raw非0／子SIGTERM／正常終了からresize後の通知→Esc→同focus→q、全termios差分なし |
| `cargo test --locked --lib body_editor_roundtrip_and_failures` | body.log：0、同loop／guardの本文ignored testをPTYで13経路。従来7＋raw正常／非0／子SIGTERM／親SIGINTの4＋復元失敗2。0600／原文CRLF／元本文と他欄・focus／temp削除・子回収、復元エラーと元エラー／130保持をassert |
| CONTRIBUTINGの直接PTY Python blockを抽出し`python3 /tmp/backlog-issue19-correction.oddf58/direct-pty.py "$PWD"` | direct-pty.log：0、実release bl＋localhost HTTPS＋PTYで8経路成功（下記） |

直接releaseの従来4経路は正常／非0／親SIGINT／起動失敗。追加4経路はfixtureが`tty.setraw(0)`し
ICANON／ECHO両方offを記録してから終了する正常／非0／子SIGTERM／親SIGINTです。
全8経路`restored=true`かつ`termios_diff=[]`（既存の一時PENDINビット除外のみ）。
親SIGINT2経路は130、子PID消滅、stderrに中断。他6経路は0／stderr空、通知後に元Detailへ復帰してq終了。
非0でも保存を巻き戻さず、子SIGTERMでは元configのままです。server stderr空、一時rootとTLSは回収済みです。
本文のraw4経路も正常時だけ交換し、失敗／中断は元本文保持、一時本文を削除します。
復元失敗2経路は既存Write seamでeditor後のalternate-screen復元エラーを注入し、
termios復元は継続しつつ、通常のeditorエラーと復元エラーで1、親中断と復元エラーで130を確認しました。
OSのtcsetattr自体の失敗／temp削除失敗の注入は未実施です。

#### 正式チェックと残るgate

| コマンド | 今回の結果 |
| --- | --- |
| `cargo fmt --all -- --check` | fmt.log：0 |
| `cargo test --locked --all-targets` | test.log：0、302成功／2 ignored（本文ignoredは上記PTY子processから明示実行） |
| `cargo clippy --locked --all-targets -- -D warnings` | clippy.log：0、Rust warningなし |
| `cargo audit --file Cargo.lock`（cargo-audit 0.22.2） | audit.log：0、1386 advisory／230依存、脆弱性・監査warningなし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | release.log：0 |
| `git diff --check`／staged確認 | 0／stagedなし。既存未commit 14fileを保持、commit／pushなし |

監査前後のCargo.lock SHA-256はともに
`1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16`。
今回release SHA-256はbl：`af08cc49c133261d3e6ce7e70f1808c2d3115646a66740c98ba42fe10c0fae27`、
bl-cli：`7c0559278f2f672d7f04596e58b46ac72fae0355613d5079b30bf88fca584a2e`。

**今回の警告相当の出力：** all-targets test.log 422〜451行付近に、既存HTTPS fixture
`https_server.py:80`のbody送信時`ConnectionResetError: [Errno 54] Connection reset by peer`が1件あります。
中断／切替試験中のclient切断と整合し、Cargo終了0／全assert成功ですが、起点testの特定はしていません。
Rust warning／製品stderrではなくserver tracebackとして報告し、無関係なfixture例外処理は変更しません。
局所settings／bodyと直接release PTYログにはこのtracebackはありません。

実端末の利用者目視、修正後Root独立レビュー／Code Review gate、GitHub Actionsは未確認です。
SIGKILL・端末切断、editor固有backup管理は従来どおり対象外。実Backlog接続／書込み・merge／push／release公開は行っていません。

### #19 修正後のRoot検証・再レビューと本文の実端末受入

RootがP1修正後の実差分を直接確認し、正式fmt／全対象test／Clippy／audit 0.22.2／release build／diff checkを再実行して、すべて終了0。
全対象は302成功・2 ignored、最新RootログにはRust warning・HTTPS tracebackともになし。
監査は1386 advisory／230依存、警告なし、lockfileは監査前後で上記`1d1474e5…`と一致した。
一次ログは`/tmp/backlog-issue19-root-fixed.6OW1dE/`。release両binaryのhashは上記P1修正後と同一。
Root自身の別工程の直接検証でも、文書の実release＋HTTPS／PTY設定8経路と、本文test binaryの13経路が成功。
raw/noechoを残す正常／非0／子SIGTERM／親SIGINTを含め`restored=true`・完全termios差分なし、本文の0600／所有者・原文・temp削除・元値と別欄／focus保持を確認した。

fresh CorrectnessとPonytailの両レビューは同じP1だけを指摘し、Rootが採用して修正した。不採用の指摘はない。
fresh限定再レビュー`f65a805b-7c12-47ce-962f-ce45c76275d9`でP1解消・追加指摘なしを確認。
全体の不要な再レビューループは追加せず、共通guardと修正影響範囲を確認した。

利用者は案内した`manual_body_editor_terminal`の5項目について「各項目OK, 端末名などは前回と同じ」と回答した。
本文の実editor保存、日本語・空白・改行、editor中resizeから追加キーなしの即時再描画、別欄の値／focus保持、
q終了後の通常shell入力／削除・cursor／表示が正常なことを**利用者の実操作・目視報告**として受け入れる。
前回の明示記録（#18最終受入）に対応する環境はHerdr 0.8.0／Ghostty 1.3.1、macOS 26.7.1／Apple Silicon。
IME／keyboardの既存記録はmacOS標準IME／101 keyboard・remapなし。今回のTERM／NO_COLOR個別値は取得しておらず推測しない。
対象のtest binaryは`target/debug/deps/backlog_cli-510e9221510b9a2a`、Root取得SHA-256は
`e4e360a246d6ee42938ce7b6d78d5f6ac97c11e4bf8d03f8d62090f5e9b78490`。

今回の回答は上記5項目の受入であり、設定編集・起動失敗／非0／不正UTF-8／読取失敗／親SIGINTの追加実端末確認を済んだとは扱わない。
これらの残る実端末確認とCode Review gateはpending。GitHub Actions・実Backlog接続／書込み・commit／push／merge／release公開は行っていない。

### #19 実端末の残項目受入完了

上記pendingのうち、実端末の残項目はその後、利用者の実操作と明示確認で完了した。過去のpending／失敗記録は履歴として保持する。

- 非0・起動失敗・読取失敗のignored testは終了0。最初の不正UTF-8用コマンドは、貼付けたJSON文字列に実改行が入り、editor起動前のtest環境変数のparseでpanicした。これは製品のUTF-8編集経路の実行・成功とは扱わず、当該ケースだけ再実施した。
- 設定確認は最初のshell切替で`run_bl`未定義となり、製品起動前に停止した。shell切替不要の一時launcherでやり直し、隔離localhost HTTPS・fake key・上記修正後releaseを使って2回とも`exit=0 termios unchanged=True`を確認した。1回目の不在settingsを生成しないassert、2回目の既存settingsを開く準備とJSON読取も成功。一時環境は終了時に削除した。
- 不正UTF-8はJSONをlauncher内で生成し、同じhashのtest binaryを直接起動して再確認した。利用者出力は`mode=utf8 test binary exit=0 termios unchanged=True temp remaining=0`。
- 親中断は同じtest binary・共通loop／guardで、試験editorがraw/noechoへ変更後、5秒で親へ実SIGINTを送る実端末経路を使った。利用者出力は`Operation interrupted`、`mode=interrupt test binary exit=130 termios unchanged=True temp remaining=0`。cargo wrapperの終了コードと混同していない。
- 利用者は最終確認で、①設定選択／取消・既存2ファイルのvim起動・resize後の即時復帰、②本文編集の各失敗時の元本文／別欄保持、③終了／中断後の通常shell入力／削除について、**「すべて確認済み・問題なし」**と明示回答した。先の本文正常保存・resize・focus保持の5項目OKと合わせ、#19の実端末受入を完了する。これは利用者の目視報告であり、Root自身の目視やPTY成功への置換ではない。

手順補助のlauncherは`/tmp`限定で、製品ソース／flag／永続設定へ追加していない。Rootは本文launcherのUTF-8／中断2ケースも既存PTYから実行し、期待終了コード・termios一致・temp残存0を確認した。最初のlauncher自己確認では起動前の説明行が簡易VTの未対応alternate-screen処理と重なってmarker待機がtimeoutしたため、余分な説明行を削除して両ケースを再実行し成功した。製品コードの修正は行っていない。
実端末環境は前節の「前回と同じ」という利用者回答を引き継ぐ。今回のTERM／NO_COLOR個別値や正確な各resize寸法は未取得で推測しない。
OSのtcsetattr自体／temp削除失敗の注入、GitHub Actionsは未実施。復元操作自体の失敗後の正常利用、SIGKILL・切断、editor独自backupは保証対象外。

採用P1は修正・Root正式チェック／直接検証・fresh限定再レビューで解消し、不採用・未解決の指摘はない。
最終Code Review gateはこの記録後にRootが呼ぶ。skippedまたは現在差分への明示Approveを確認するまでREADYとはしない。
実Backlog接続／書込み・commit／push／merge／release公開は行っていない。

### #19 PR #50 CI失敗：Ctrl+O拒否テストの非同期競合を隔離（2026-10-30 UTC）

対象はbranch `feat/editor-integration`、HEAD `59cafd7768d14b120ccdac113c6d6572f1a2a6d0`。
開始時は作業tree／indexともにclean。製品ソース・仕様・依存・汎用PTY fixtureを変更せず、
`ctrl_o_rejects_invalid_list_keys_and_secret_urls_without_spawning_browser_or_issue_get` の手順と本記録だけを変更した。

#### CI観測と直接再現

- CI run `41005014731`／job `123897832845` のTestは終了101。`tui_process` は71成功・1失敗。
  一次ログ `/tmp/backlog19-ci.fkkw1B/failed-job.log` の798行付近では、`../../x` のケースで
  `remaining steps=[{'keys': 'q', 'wait': '> Issue List'}]`。`Failed: Invalid selected issue key` のpopupが
  一覧タイトルを覆い、Detailは`Preview failed`。qは未送信であり、製品がqに無応答だった証拠ではない。
- 選択後150msの自動previewとCtrl+Oが同じ不正keyのエラーを返し、spaceで通知を消した後にも
  previewの完了通知が届き得る。ブラウザ拒否だけを検証する手順に、無関係なpreview通知が混ざっていた。
  Rootの元exactと4並列20回は成功しており、自然な実行だけでCIの順序を再現できたとは扱わない。
- Workerも修正前の `python3 /tmp/backlog19-ci.fkkw1B/reproduce.py` で実debug `bl`＋localhost HTTPS／PTYを直接実行。
  元driverは終了0、診断用 `tty-delayed-observation.py` は終了1で同じremaining q／Preview failed／popupによるタイトル遮蔽を再現した。
  この診断driverは既存fixtureのコピーで、spaceだけ即時送信し、その直後の観測を0.25秒遅らせる2箇所のみ変更。
  製品ソース改変・timeout延長・retryはない。wrapper自体は終了0なので、子driverの終了1をREDの根拠とする（`worker-red.log`）。

#### 最小修正と修正後の実測

一覧データの`Project: B`を待ったら、通常キーl（通知を消してIssueへ移動）とEsc（preview取消）を同じ入力batchで送り、
次stepでpopupに隠れないfooter `h/l Tab/S-Tab:Project` の描画を待ってCtrl+Oを別batchで送る。
previewが既に完了した場合も成立するmarkerを使い、必ず`cancelled`が出る前提や追加の固定時間待ちは設けない。
元の短い`pause:0.01`は削除し、fixture既定の描画待ちを使う。最終snapshot参照は2→3へ整合した。
全5キー、invalid時の総request4、browser stub未起動、secret URL拒否、通知後のIssue focus、
既存scripted helperの終了0／termios復元／stderr空のassertは維持し、qは従来どおり一覧タイトルを確認してから送る。

一次ログ／診断scratchは `/tmp/backlog19-ci.fkkw1B/`。診断scriptはrepoへ追加していない。
修正後の `reproduce-green.py` は修正前と同じbinary／routes／2種driverで、上記4stepへ手順と最終snapshot参照を合わせ、
終了0／復元／stderr空／最終Issue focus／4 GETをassertした。`reproduce-completed.py` は診断だけ初回観測を0.4秒遅らせ、
最初のsnapshotで既に`Preview failed`であることもassertして同じ手順を実行した（repoの待機には追加していない）。

| コマンド／観測 | 終了コード／今回の結果 |
| --- | --- |
| `python3 /tmp/backlog19-ci.fkkw1B/reproduce-green.py` | 0、元driver／同じ遅延観測driverとも終了0・4 snapshots・上記assert成功（`worker-green.log`） |
| `python3 /tmp/backlog19-ci.fkkw1B/reproduce-completed.py` | 0、preview完了済みでも両driver成功（`worker-completed.log`） |
| `cargo test --locked --test tui_process ctrl_o_rejects_invalid_list_keys_and_secret_urls_without_spawning_browser_or_issue_get -- --exact` | 0、全5キー成功（`worker-exact.log`）。続けて同じコマンドを逐次5回＋4並列20回、全25回終了0・retryなし（`worker-repeat/`） |
| `cargo test --locked --test tui_process ctrl_o` | 0、2成功（`worker-ctrl_o.log`） |
| `cargo test --locked --test tui_process` | 0、72成功（`worker-tui-process.log`） |
| `cargo fmt --all -- --check` | 0（`worker-fmt.log`） |
| `cargo test --locked --all-targets` | 0、302成功・既存の実端末専用2 ignored（`worker-all-targets.log`） |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし（`worker-clippy.log`） |
| `cargo audit --file Cargo.lock` | 0、cargo-audit 0.22.2、1386 advisory／230依存、警告・脆弱性報告なし（`worker-audit.log`） |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0（`worker-release.log`） |

実行環境はDarwin arm64、rustc 1.99.1。今回の局所／全対象testログにRust warning・HTTPS tracebackはない。
監査前後のCargo.lock SHA-256はともに `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` で不変。
文書追記後も`git diff --check`／fmtは終了0、変更は本記録と上記testの2ファイルのみ、stagedなし・HEAD不変を確認した。
汎用fixtureの8秒timeout／retryなしは維持。今回の修正後GitHub Actionsと実端末目視は未実施で、過去の受入を上書きしない。
現在差分の独立Review gateはRootへ残す。実Backlog接続／書込み・commit／push／merge／release公開・Issue変更は行っていない。

Rootは修正後の2ファイル差分と原因の一次コードを直接確認し、`reproduce-green.py`／`reproduce-completed.py`を独立実行して双方終了0を確認した。
元driver／遅延観測driverとも、終了0・端末復元・最終Issue focus・4 GETのみ・stderr空を確認した。
Rootの正式fmt／全対象test（302成功・2 ignored）／Clippy／audit／release build／diff checkもすべて終了0。
一次ログは`/tmp/backlog19-ci-root.EXKXwf/`。Root最新testログにはwarning／HTTPS tracebackなし、監査は1386 advisory／230依存で警告なし、lockfileは監査前後で不変。
fresh限定再レビュー`0f62af1e-6430-4f25-9d88-f39d6f9428f8`は原因に対する同期・全5ケース／安全性assert維持・最小範囲を確認し、指摘なし。
製品コードを変更しないテスト同期修正のため、元feature全体の2レビューや実端末受入を無条件に繰り返さず、修正影響範囲を再検証した。
この追記後にRootが現在差分のCode Review gateを実行する。修正後GitHub Actionsは未実行で、ローカル成功をCI成功とみなさない。
この時点でcommit／pushは未実施。失敗runや過去buildの成功・承認を修正後CIの結果として流用しない。
