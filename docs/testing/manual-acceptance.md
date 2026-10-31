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

# Issue #20: TUI課題作成

状態: **ローカル実装／自動試験段階。実端末・実Backlogの必須受入はpending、Issue #20全体の受入完了ではない。**
過去Issueの端末／実API確認を今回の成功として流用しない。
最初のworker runは1800000msでtimeoutし、部分差分を残した。その後、同じ差分を保持してローカル検証と記録を再開した。
実サービス接続・書込み、実credentialの参照、Issue編集、commit/pushは行っていない。

## 今回のローカル対象buildと観測

| 項目 | 記録 |
| --- | --- |
| 記録日 / 実施者 | 2026-10-30 UTC / implementation worker（自動実行のみ） |
| 環境 | macOS 26.7.1 (25G241)、arm64。PTYはPython標準library、HTTPSは既存localhost fixture |
| revision | HEAD `bba5c248b91a8dca35167cfefcf03e7f1c2706e1` + 未コミットの#20差分 |
| release `bl` SHA-256 | `2c04bf537f9c0f6e2172d503808696df41f1fbb9bf7ff9810aec7e33d19c3482` |
| release `bl-cli` SHA-256 | `7b79bd8ef4c58a64d37d1dc9ead790991cec17f67f2c45be2ff9e104ee596487` |
| release target | `cargo build --locked --release --target aarch64-apple-darwin --bins` 終了0。両binaryとも`file`はMach-O arm64 |
| 監査 | cargo-audit 0.22.2、`cargo audit --file Cargo.lock` 終了0。1386 advisories / 230 dependencies、警告・DB失敗なし |
| lockfile SHA-256（監査前＝後） | `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` |
| その他 | fmt / Clippy (`-D warnings`) / `git diff --check` 終了0。依存版・設定schema変更なし |

CONTRIBUTINGの「TUI課題作成 (#20): releaseの直接ローカル検証」にあるPython blockをそのまま抽出し、
実release `bl --profile local-check` + localhost HTTPS + PTYで実行した（cargo test経由ではない）。終了0、出力:

```text
PASS: release bl; confirm Enter=0 POST; explicit send=1 POST; raw form/default clear/milestones/editor; refresh=2 count+2 list; exit=0; termios restored
```

この実行はfake keyだけの一時HOME/XDG/cwdを使い、profile identity、確認Enterの非送信、sで単一POST、
送信中の追加入力禁止、既定担当者解除・複数milestone・実在日付・タイトル/本文の前後空白と改行のform原文、
editor往復、count/list再取得、終了0とtermios復元をassertで照合した。
ログは `/tmp/issue20-release-repro.log`、抽出した同一コードは `/tmp/issue20-release-repro.py`。
一時fixtureの設定・鍵・HTTP記録は回収済み。実端末の目視成功や実Backlog登録成功を意味しない。

公開PTY/HTTPS境界の縦sliceのred/greenログは `/tmp/issue20-slice{1..6}-{red,green}.log`。
対象はmodal文字入力と変更時だけの破棄、全項目/default/clear/独立送信、3段階の中断確度、
全欄伏字とeditor/resize/長い確認本文、取消した候補の世代、新しいフォームへの旧created確度非継承。
追加の回帰確認は `bl_create_` 9件成功、新フォーム中断1件成功、公開Service abort/join試験成功。
`/tmp/issue20-create-focused.log`、`/tmp/issue20-slice6-green.log`、`/tmp/issue20-service-abort.log`を参照。
これらに加え、現在差分での正式チェックの最終結果を下に記録する。
最初の全対象コマンドは240秒のtool timeoutで打ち切られ、後の初回全対象終了0も追加回帰試験前の結果なので最終結果にはしない。

## 操作と再開する必須受入

製品のキーはREADME「TUI課題作成 (#20、実環境受入pending)」、直接ローカル再現はCONTRIBUTINGを参照。
フォームのTab順はタイトル→種別→優先度→担当者→開始日→期限日→マイルストーン→本文編集→確認へ→取消。
Ctrl+T/Y/P/A/Bで直接focusし、Ctrl+Bはfocusのみ。候補Enter/Space、milestoneはSpace複数選択、
本文編集操作のEnter/Spaceでeditor、候補/本文のDelete/Backspaceでクリア。
確認ではj/k・PgUp/PgDn・Space・g/G・Home/Endで全文を読み、Enterは非送信、独立したsだけで送る。
送信中の編集/再送/破棄/qは無効、Ctrl+Cは確度を保持して中断する。

**未実施／再開条件（成功扱いしない）:**

1. Apple Silicon Macの実端末で、対象hash・端末名/版・macOS・IME・TERM・NO_COLORを記録し、
   100x24 / 40x12 / それ未満でfocus/help、全確認本文、長い日本語・結合文字・emoji、IME候補/cursor、
   通常文字キー、editor前後のresize/失敗/中断、値保持、秘匿、終了後の通常shell操作を目視確認する。
   Rootが別途許可を求めて実施し、PTY成功を実端末受入の代替にしない。
2. **別の明示的な書込み許可が必要**。接続先 `https://lazybl.backlog.com/`、使用profile、許可projectと
   作成内容を利用者と確認した後だけ、TUIの入力→全文確認→独立送信で検証課題を作成する。
   Backlog上の全登録値とTUIのキー/URL/結果表示を照合し、対象課題識別子と結果をこの節へ追記する。
   許可前の接続・書込み、既存課題の無断変更・cleanupはしない。実環境の複数project確認は既存制約により対象外。
3. Rootによる独立レビューと受入判定。実端末/実Backlog未実施のまま、Issue #20を完了・受入済みと宣言しない。

## 現在差分の正式チェック結果

- `cargo fmt --all -- --check`: 終了0、`/tmp/issue20-final-fmt.log`。
- `cargo test --locked --all-targets`: 終了0、最終ログ `/tmp/issue20-final-tests.log`、exit記録 `/tmp/issue20-final-tests.exit`。
  公開PTYの`tests/tui_process.rs`は82件成功。実端末用ignored testは自動実行していない。
- `cargo clippy --locked --all-targets -- -D warnings`: 終了0、`/tmp/issue20-final-clippy.log`。
- `cargo audit --file Cargo.lock`: 終了0、`/tmp/issue20-final-audit.log`。上表のlock hash前後一致。
- `cargo build --locked --release --target aarch64-apple-darwin --bins`: 終了0、`/tmp/issue20-final-release.log`。hashは上表。
- `git diff --check`: 終了0。staged fileなし。ローカル自動検証の結果であり、上記の未実施gateは残る。

## #20 Rootの独立検証・送信前の実端末受入と書込み許可

Rootが実差分（untracked `src/tui/create.rs`を含む）と接続コードを直接確認し、現在差分で正式チェックを独立実行した。
ログは `/tmp/backlog20-root.Z1MF62/`。fmt／全対象test（313成功・0失敗・実端末用2 ignored）／Clippy／
cargo-audit 0.22.2／release build／diff checkはすべて終了0。Rust warning・HTTPS traceback・監査警告／DB失敗なし。
監査は1386 advisory／230依存、Cargo.lock SHA-256は監査前後とも上記`1d1474e5…`で不変。
release両binaryのhashも上表と一致した。

Rootは別工程としてCONTRIBUTINGの#20 Python blockをそのまま抽出し、
`python3 /tmp/backlog20-root.Z1MF62/direct-release.py`でrelease製品をlocalhost HTTPS／PTYから直接実行した。
終了0、`direct-release.log`に上記と同じPASS。確認Enterの非送信・独立sの単一POST・原文form／担当者既定値解除・
複数milestone・editor往復・2 count＋2 list・終了0／termios復元をassertで確認した。実サービスや実設定は使用していない。

利用者はこのhashのrelease `bl --profile default`を実端末で操作し、送信前手順について
**「指定値で確認画面まで進め、各確認は問題なし」**と回答した。これは利用者の実操作／目視報告であり、Root自身の目視ではない。
確認対象は日本語／emojiの入力位置・IME候補、focus移動、100x24／40x12／極小からの復帰、
Ctrl+Bだけではeditorを開かず次のEnterで開くこと、editor中resize後の入力保持、全確認内容の末尾までの閲覧、
確認Enterで送信せずEscで戻っても値を保持すること。実端末のeditor異常終了／中断や終了後shell復元は、
この送信前回答だけで新たに確認済みとはしない（ローカル自動試験と#19の過去受入は別の根拠として保持）。
環境は選択肢の「以前の記録と同じ」という利用者回答を引き継ぎ、Herdr 0.8.0／Ghostty 1.3.1、
macOS 26.7.1／Apple Silicon、macOS標準IME／101 keyboard・remapなし。今回のTERM／NO_COLOR個別値は未取得で推測しない。

Rootが共通設定のprofile名／spaceUrlだけを安全に読み、`default`が`https://lazybl.backlog.com/`を指すことを確認した。
このAgentの環境に指定keyの値はなく、他の保存先から補わず利用者の認証済み端末で実受入する。
key本体は表示・記録・チャットで要求していない。

利用者は別の質問に **「上記の1件だけ作成を許可する」**と明示回答した。許可範囲は次の検証課題1件だけで、
既存課題の変更・削除・再送・cleanupは含まない。送信前確認の回答をこの許可の代用にしていない。

- profile `default`、origin `https://lazybl.backlog.com/`、project `BLCLI`
- title `backlog-cli Issue #20 TUI作成検証 日本語👩‍💻`
- 種別 `タスク`、優先度 `中`、担当者未設定
- 開始日 `2026-10-30`、期限日 `2026-10-31`、マイルストーン未選択
- 本文（2行）: `Issue #20 のTUI作成受入テストです。`／`入力・確認・独立送信を検証します。`
- 状態／完了理由はAPIの初期値。まだsを押さず独立レビュー後の案内を待つよう依頼した。

独立Correctness／Ponytail reviewerは双方が1800000msでtimeoutした。これは指摘なしではなく結果未取得であり、
Rootが終了記録・差分不変・コード変更なしを確認し、それぞれ同じreviewerを公開resume APIで再開した。
この時点では実Backlog作成、最終結果／登録内容、終了後shell復元、独立レビューの結論とCode Review gateはpending。
commit／push／Issue変更は行っていない。

## #20 安全表示P1の限定修正・現在buildの再検証（2026-10-30 UTC）

Rootが受理したP1は、候補名 `test-\u{200b}secret`（実データの中央はU+200B）に対し、
key `test-secret` の完全一致検査後にParagraphが幅0のgraphemeを落とし、画面でkeyを再構成する問題。
同じ原因を共通単行入力でも再現した。上記の過去build／利用者の送信前目視結果は保持するが、
**修正後buildの実端末再確認・独立再レビュー／Code Review gate・実Backlogの1件作成と登録値照合はpending。
実書込みは引き続きON HOLD**。このworkerは実サービス・実credentialにアクセスしていない。

- `create.rs`内の既存safe closureに既存`tui_text::display`を適用し、
  project／loading・error／選択済み値／notice／候補popupを同じ順序で安全化。popupは可視化・最終secret検査後に水平sliceする。
- 共通`SingleLineInput::draw`は同じ既存helperで原文全体を検査し、表示変換が必要なら欄全体を固定伏字化する。
  原文・編集・paste処理は変えず、伏字時cursorは欄左端へ固定する。変換した表示へ未変換のcursorを置く方式は採らない。
- 公開seamの回帰2件を追加。候補fixtureの初期化だけはRoot承認により`create.rs`のtest moduleに置き、
  操作／観測は`Shell::key/draw`とTestBackend bufferを使用（private method／内部状態assertなし）。
  40x12／100x24でpopup、左右scroll、選択後フォームにkeyが出ず、可視escapeが出ることを確認する。
  `tests/tui_input.rs`では公開new/value/event/drawで幅0・control・offscreenの固定伏字、狭幅・focus、
  固定cursorと原文編集保持を確認する。既存の日本語・結合文字・ZWJ emoji・長文scroll／cursor試験も維持する。

一次ログ／抽出script／終了コードは `/tmp/issue20-security/`。実装変更前の各REDはともに終了101で、
TestBackend bufferに`test-secret`が出ることを確認した（`create-red.log`／`input-red.log`）。
候補の最小修正後に1件GREEN、次に単行入力のRED→最小修正→全9件GREENの順で実行した。

| コマンド／観測 | 終了コード／結果（同directory内のlog） |
| --- | --- |
| `cargo test --locked --lib create_candidate_popup_and_selected_form_do_not_reconstruct_secrets` | 0、1成功（`focused-create.log`） |
| `cargo test --locked --test tui_input` | 0、9成功（`focused-input.log`） |
| `cargo fmt --all -- --check` | 0（`fmt.log`） |
| `cargo test --locked --all-targets` | 0、315成功・0失敗・既存実端末専用2 ignored（`all-targets.log`）、warning／HTTPS tracebackなし |
| `cargo clippy --locked --all-targets -- -D warnings` | 0（`clippy.log`） |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0（`release.log`） |
| `cargo audit --file Cargo.lock` | 0、1386 advisory／230依存、警告なし（`audit.log`） |
| `python3 /tmp/issue20-security/direct-release.py` | 0、CONTRIBUTINGの#20 Python blockをそのまま抽出し、現在releaseのlocalhost HTTPS／PTY直接検証が同じPASS（`direct-release.log`） |
| `python3 /tmp/issue20-security/direct-zero-width.py` | 0、同じlocalhost手順の候補／既定種別名とtitleだけにU+200Bを入れた追加self-check（`direct-zero-width.log`） |

追加self-checkも確認Enter=0 POST、独立送信=1 POST、原文title（U+200Bと前後spaceを含む）・種別ID等のform完全一致、
既定担当者解除・複数milestone・editor往復・2 count＋2 list・終了0／termios復元・stdoutにkeyがないことをassertした。
scriptは`/tmp`のみで製品／fixture／依存／Planに追加していない。実サービスのPOSTではない。

現在release SHA-256（古いhashを上書きしない）:

- `bl`: `4fe1f3fa98b93afa071cdc494d82e3383ed394b27ef83b52c1440b0de4e633cf`
- `bl-cli`: `0896c75947cfb2dc9ec0140544628d76051d73e08c06420a03c4302f17e356ed`

Cargo.lock SHA-256は監査前後とも上記`1d1474e5…`で不変。既存#20差分とRoot文書を保持し、
今回の変更は`src/tui/create.rs`／`src/tui/input.rs`／`tests/tui_input.rs`／本追記のみ。
commit／stash／push／Issue変更なし、staged fileなし。現在buildの必須受入をローカル自動成功で代用しない。

## #20 修正後のRoot検証・独立レビュー・実Backlog受入完了

RootはP1修正後の実差分と既存`tui_text::display`／入力部品／Ratatuiの幅0文字処理を直接確認した。
正式チェックを現在差分で再実行し、fmt／全対象test（315成功・0失敗・既存実端末専用2 ignored）／Clippy／
cargo-audit 0.22.2／release build／diff checkはすべて終了0。Rootログは `/tmp/backlog20-root-fixed.NeeIxj/`。
Rust warning・HTTPS traceback・監査警告／DB取得失敗なし。1386 advisory／230依存を監査し、lockfileは前後不変。
release hashは直前の修正後hash（bl `4fe1f3fa…e633cf`、bl-cli `0896c759…356ed`）と一致した。

Rootは別工程として、CONTRIBUTINGからそのまま抽出した `direct-release.py` と、内容を直接確認した
`/tmp/issue20-security/direct-zero-width.py` を実行し、両方終了0／PASSを確認した。
通常経路とU+200B入り候補／titleについて、実releaseからの確認Enter=0 POST・独立送信=1 POST・原文form保持・
editor／一覧再取得・終了0／termios復元・fake key非表示を確認した。これはlocalhostのみの検証である。

独立したfresh Correctness Reviewは具体的な指摘なし。
fresh Ponytail ReviewのP1（候補／選択値のゼロ幅文字除去後のsecret再構成）をRootが採用し、
同じ原因の新しいタイトル入力経路も含めて最小修正した。不採用の指摘はない。
修正後、別のfresh限定再レビュー `391e61ef-de94-4610-a068-5733760fcfb9` がP1解消・影響範囲の追加指摘なしを確認した。
全体の2レビューを無条件に繰り返さず、表示／入力／原文／cursorと回帰テストの修正影響範囲を再確認した。
元の2レビューはtimeout後の同一reviewer再開で実結果を取得し、失敗を指摘なしと扱っていない。
Correctness実結果 `2fd8c307-6021-4d03-8010-303a2a7f7c4e`、Ponytail実結果 `3551b87a-a7ae-4bd7-bcf0-8a31a698d065`。
Reviewerはread-onlyで、自身による試験再実行／実端末操作／実サービスアクセスではなくコードとRootログを確認した。

Rootは旧buildから送信しないよう依頼し、修正後hashの製品を起動し直す自己完結の手順を提示した。
利用者は許可済みの同じ入力について、送信前にIME／cursor、focus、100x24／40x12／極小復帰、editor中resize後の
値保持、変更時の破棄確認と編集復帰、全入力の確認、確認Enter非送信／Esc後の値保持を再確認してから、
独立したsを一度だけ押す手順を実施した。既存課題の変更・削除・再送・cleanupは依頼していない。

利用者の今回の明示回答:

- 課題URL: **https://lazybl.backlog.com/view/BLCLI-2**
- TUIの作成・一覧更新結果: **OK**
- Backlogの登録値: **全項目一致**
- 修正後buildの送信前確認: **問題なし**
- 終了後のshell: **正常**
- 端末環境: **前回と同じ**

この回答は修正後buildの実操作・Web登録内容の照合についての利用者報告として受け入れ、Rootが実APIを独立読取りしたとはしない。
対象はprofile `default`／origin `https://lazybl.backlog.com/`／project `BLCLI`、課題キー`BLCLI-2`。
登録内容は上記の許可済みtitle／本文2行、種別タスク・優先度中・担当者なし、開始日2026-10-30・期限日2026-10-31・
マイルストーンなし（状態／完了理由はAPI初期値）との全項目一致である。
実端末環境は利用者の「前回と同じ」という回答に基づきHerdr 0.8.0／Ghostty 1.3.1、macOS 26.7.1／Apple Silicon、
macOS標準IME／101 keyboard・remapなしを引き継ぐ。今回のTERM／NO_COLOR個別値は未取得で推測しない。
新しいbuildの受入を過去buildの結果で代用せず、過去のpending／timeout／P1履歴も保持した。

Issue #20の実端末・実Backlog受入は上記回答で完了。障害／中断／応答逆転等はローカルHTTPS／PTY試験による根拠と区別する。
実サービスへの意図的な障害・過大負荷・複数project検証は行っていない。GitHub Actionsは未実行。
実端末でのeditor異常終了／中断の追加注入は今回の正常作成手順には含めず、共通editorの#19受入と今回の自動試験を維持する。
SIGKILL／端末切断後の復元やeditor独自backupは従来どおり保証対象外。

この記録とREADME／CONTRIBUTINGの未実施表示を整合した後、Rootが最終Code Review gateを呼ぶ。
その結果がskippedまたは現在差分への明示ApproveとなるまでREADY_FOR_MERGEとはしない。
commit／push／merge／release公開／deploy／GitHub Issue変更は行っていない。

## Issue #21: TUI課題更新

状態: **Workerのローカル実装・自動検証。実端末／実Backlog必須受入は未実施（pending）**。
過去の#20以前の確認はこの更新フォームの受入へ流用しません。開始HEAD `3d9523c` + #21未コミット差分。
実施日: 2026-10-30 UTC。Workerは実サービス・利用者config・secretを参照していません。
Rootによる独立Review／Code Review gateも別工程であり、この記録で完了扱いにしません。

### 実装と自動検証の境界

- Detailのeで最新課題→最新projectIdの候補・記法を取得。createの既定値を流用せず全更新値を初期表示します。
- createフォームの入力／phase／候補／確認s／editor／安全表示を共用します。Serviceの既存差分・検証を
  crate内のopaqueなprepare/sendへ分離し、確認内容と単一PATCHを一致させます。CLI/commentの部分指定契約は維持します。
- 正常成功objectだけをDetailへ交換。古いDetail取得を失効させます。一覧の旧タイトル／条件／ページ履歴／選択／scrollと
  Commentの選択／scrollは保持し、成功確認や一覧／Comment更新のGETは追加しません。
- API共有確度は対象operationを一致させて更新にも用い、既存CreateProgress公開契約を維持します。
  HTTP futureを直接所有し、TUI終了はabort+join→snapshot→通知／端末復元です。

TDDは承認済みの実bl+PTY入力→描画／HTTPS境界を使用しました。private関数の形状固定試験は追加していません。
最初の試験で画面外本文をwaitしてしまった失敗は機能REDの根拠にしません。修正した試験を開始HEADの隔離buildへ当て、
e後に最新フォームへ進まないRED（exit101）を再確認しました（`/tmp/issue21-baseline-red.log`）。
同じ現行製品試験はGREEN（`/tmp/issue21-green1.log`）。必要候補clearと同ID候補名違いのRED→GREENも実測しました
（`/tmp/issue21-red2.log`→`green2.log`、`red3.log`→`green3.log`）。

更新PTY試験は全項目／解除／ID0／集合同値／無変更／確認だけ非送信／一回送信／一覧・Comment保持／世代取消／
入力・editor失敗・resize・極小・キー衝突／secret・制御・zero-width／中断3段階と200後失敗を含みます。
個別実測ログ: `/tmp/issue21-allfields.log`, `clears.log`, `interrupt.log`, `failures.log`, `generation.log`,
`validation.log`, `safety.log`, `editor.log`, `preservation.log`, `fixture.log`（すべて同じ`issue21-`接頭辞）。
既存create試験で検出したloading文言回帰は修正し、`/tmp/issue21-create-regression.log`で再確認しました。

正式全対象testの初回はtoolの240秒timeoutでtui_process開始後に途切れ、完走の成功として扱っていません。
正式チェックの完走・release直接検証結果は下の追記へ記録します。

### Root／利用者による必須の実端末受入（未実施）

対象buildのSHA-256、端末名／版、macOS、IME、TERM／NO_COLORを記録してください。最新releaseを使い、
CONTRIBUTING「TUI課題更新 (#21): releaseの直接ローカル検証」の一時localhost設定／fixtureでまず目視します。
PTY成功は実IME・字幅・終了後の通常shell確認の代替ではありません。

1. Detail e→最新値（タイトル・本文・種別・優先度・担当者・状態・完了理由・開始／期限日・複数milestone）。
   create既定値と異なる値も現値のままであることを確認します。
2. 100x24／40x12／それ未満で全欄へのfocus・候補popup・左右全文・日本語／結合文字／emoji・IME候補と確定、
   q/h/l/j/k/H/L等の文字入力、秘密値／制御／zero-widthの安全表示を確認します。極小では編集・確認・送信しません。
3. タイトル／本文／各候補／日付の変更と明示解除、完了理由ID0、milestone複数・同じ集合・未変更を確認します。
   変更と解除だけの確認でEnter非送信→独立s一回、確認Esc→編集、変更ありだけの破棄確認を確認します。
4. editor正常・非0／起動失敗／不正UTF-8／読取失敗、editor中resizeと追加キーなしの再描画、本文原文／他欄／focus保持を確認します。
5. 送信中の編集・破棄・再送禁止。結果表示とCtrl+C／SIGINT、not_updated／unknown／updatedと再送誘導なし、
   終了後の入力・削除・cursor・termios／属性復元を目視します。
6. 正常成功Detail反映後の「更新済み・コメント表示更新中」→完了を待ち、Rなしで最新Comment／本文なし変更履歴と本文先頭へ移ることを確認します。
   待機中のEnter／Esc・s／e／H／L・極小qで閉鎖／再送／背面focus変更せず、結果scroll・resize・Ctrl+C／SIGINT中断130と端末復元が可能なことを確認します。
   表示更新失敗時は更新済みDetailと旧Commentページ・選択／scroll（空／未取得含む）保持、updated／旧表示保持・再送不要・Comment R案内を確認します。
   一覧旧タイトル・全行・条件・total・ページ履歴・選択／scrollは維持し、一覧R後の新表示と区別します。

### Rootによる実Backlog段階的受入（未実施）

接続先はPlanの `https://lazybl.backlog.com/`。使用profileのoriginを確認し、**別途の明示許可**がある検証用課題・
変更／解除内容だけを使います。対象／内容／復元が未許可なら書き込みません。Workerは実接続していません。

- 実施前に対象課題の識別子と変更する項目／解除、保持する項目を記録し、TUI Detail eから開始します。
- 現値初期表示→変更だけの確認→独立s一回。Backlog上の変更／解除と、未指定項目の保持、TUI結果とDetailを照合します。
- RなしでComment最新履歴が表示されることをWeb等と照合します。今回の更新履歴IDを探索する動作ではありません。
- 一覧タイトル・全行・検索条件／total／ページ履歴／選択／scrollは保持され、一覧Rで初めて取得されることを確認します。
- 記録にAPI key・機密本文・raw responseを入れず、対象build・手順・結果・未確認事項を追記します。
  許可／認証／権限／対象不足や失敗はpending／未完了として報告し、#24へ先送りして成功扱いにしません。

### Workerローカル完走記録（2026-10-30 UTC）

| コマンド／確認 | 実終了コード | 実測／ログ |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | `/tmp/issue21-fmt.log`、警告なし |
| `cargo test --locked --all-targets`（最初から再実行） | 0 | 328 passed／0 failed／2 ignored（実端末用）。`/tmp/issue21-all-tests.log` と `.exit` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 | `/tmp/issue21-clippy.log` と `.exit`、警告なし |
| `cargo audit --file Cargo.lock` | 0 | cargo-audit 0.22.2、1386 advisories／230依存、警告／脆弱性出力なし。DB取得・registry更新失敗なし。`/tmp/issue21-audit.log` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 | `/tmp/issue21-build.log` と `.exit` |
| CONTRIBUTINGの#21 Python blockを抽出し、`python3 /tmp/issue21-direct.py`（release製品の直接起動） | 0 | `/tmp/issue21-direct.log`、raw結果 `/tmp/issue21-direct-result.json` |
| `git diff --check` | 0 | `/tmp/issue21-diff-check.log` |

監査前後の`Cargo.lock` SHA-256はいずれも
`1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16`。manifest／lock／設定schema／Plan／design差分なし。
Clippyの途中のcollapsible-if指摘は修正後に正式コマンドを再実行し、上記は最終差分の結果です。

直接release検証は10 snapshots、最新値、確認Enter時HTTP増加なし、独立sでPATCH一回、変更title／本文・担当者解除／
resolution0／milestone2件の正確なform、**17 GET + 1 PATCH**、追加の一覧／Comment／成功確認GETなし、
更新Detailと旧一覧title・Comment併存、q=0／空stderr／termios全項目復元（既存のmacOS PENDIN除外のみ）をassertしました。
これはcargo testと別に製品を直接起動したローカルHTTPS/PTY検証で、実Backlogや実端末／IMEの成功ではありません。
RootはCONTRIBUTINGの自己完結手順を同じrelease buildで再実行できます。

対象release SHA-256:
- `bl`: `957da0e959990bbf591bcf4ba006134f1c3441ac49f34557726a2ba2e141010e`
- `bl-cli`: `479eea31b5c34676c2d01008746fc88b51dce681cef3c513779e9a6f2eb5d515`

**未確認**: 実端末／実IMEと実Backlogの必須受入、Rootの2独立レビュー／Code Review gate。ローカル成功をこれらの完了扱いにしません。

### #21 accepted review findings 3件の修正・再検証（2026-10-30 UTC）

上記Worker完走記録とRootの `/tmp/backlog21-root.MvmnKr/` は**修正前の履歴**です。
今回の開始HEADも `3d9523c4e01325fb30398d0923f29bb4fe92dada` で、継承した未コミット差分をすべて保持しました。
修正は承認された3原因だけです。Plan／design／Cargo／CLIの部分object成功契約／設定schemaは変更せず、
実サービス・利用者config・secret参照、依存追加、commit／stage／stash／push等は行っていません。

| accepted finding | 最小修正と公開PTY/HTTPSでのRED → GREEN |
| --- | --- |
| 更新前の一覧・Comment要求が背面の対象を変更 | 更新対象取得時に既存busy失効と `cancel_previews` を使用し、一覧／Detail／Commentの未完了要求を無効化。成功交換は固定対象と現在Detail key一致時だけ。`bl_update_invalidates_pending_` は遅延pageの後着でB-1選択待機timeout（101）→ 遅延page／一覧R／Comment G・Rの4ケースが0。1PATCH後もB-1・旧一覧・Comment保持、pで追加通信なし、Detail RもB-1だけを取得 |
| 部分成功objectのDetail描画panic | `issue_view` の既存必要field検証をcrate内で最小共通化し、TUIの表示反映前にも使用。共通send／CLIのid/keyだけの成功契約は維持。`bl_update_incomplete_or_mistyped_` はid/keyだけの200で製品exit1（101）→ id/keyのみ、13 fieldそれぞれの欠落／型不正、milestone.name不正の計28ケースが0。旧Detail保持、updated応答処理失敗・Do not resend、1PATCH・追加GETなし・q正常終了 |
| 最新GET／成功PATCHが別keyでも採用 | GET直後・候補取得前に固定要求key一致を確認。PATCHも表示検証時に固定key一致を確認し、不一致はupdatedの応答処理失敗。`bl_update_latest_get_for_another_key_` と `bl_update_patch_success_for_another_key_` は完全なB-2 objectを返し各待機timeout（101）→ 各0。GET不一致は候補／フォーム初期化／PATCHなし、PATCH不一致は旧Detail・updated保持、再送／追加GETなし |

新規4テストは承認済みの実bl入力／PTY→描画＋HTTPS送受信のみを観測し、private形状固定試験は追加していません。
REDログは `/tmp/backlog21-fixes/red1.log`, `red2.log`, `red3-get.log`, `red3-patch.log`、各`.exit`は101。
対応GREENは同じ接頭辞の `green1`, `green2`, `green3-get`, `green3-patch`（各`.exit`は0）です。
途中の試験側修正も区別します。最初のgreen1試行はComment記法用GETを数えず期待3／実4で101、期待を既存通信どおり4へ訂正。
最初のgreen3-get試行はLoadFailedのEnterで既存の明示再試行を発生させ、GET期待2／実3で101でした。
送信しないsだけにして余分な再試行を除きました。これらを製品不具合や最終成功と混同していません。

| 変更後に順次実行したコマンド | 終了コード／観測（ログ `/tmp/backlog21-fixes/`） |
| --- | --- |
| `cargo test --locked --test tui_process bl_update_` | 0／17成功、`focused-update.log` |
| `cargo test --locked --test tui_process create_` | 0／10成功（前回作成確度の非継承を含む）、`focused-create.log` |
| `cargo test --locked --test service issue_update_` | 0／2成功、`service-update.log`。部分object成功を維持 |
| `cargo test --locked --test issue_update --test comment_add` | 0／各14成功、`cli-update-comment.log` |
| `cargo fmt --all -- --check` | 0／差分なし、`fmt.log` |
| `cargo test --locked --all-targets` | 0／**332成功・0失敗・実端末用2 ignored**、22 suite。`all-targets.log` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0／警告なし、`clippy.log` |
| `cargo audit --version` | 0／cargo-audit 0.22.2、`audit-version.log` |
| `shasum -a 256 Cargo.lock`（監査前／後） | 各0／同一hash、`lock-before.log`／`lock-after.log` |
| `cargo audit --file Cargo.lock` | 0／1386 advisories・230依存、通常DB取得・index更新成功、警告／脆弱性／照会失敗なし、`audit.log` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0／両binary成功、`build.log` |
| CONTRIBUTING #21の既存Python blockをそのまま抽出し `python3 /tmp/backlog21-fixes/direct-release.py` | 0／**変更後release製品を直接起動**、`direct-release.log`／`direct-result.json` |
| `git diff --check`／stagedなし | 0／空出力。既存未コミット差分・untracked fixture保持 |

各コマンドの終了コードは `exits.log` と各`.exit`。focused／CLI／全対象test・clippy・buildのRust warning、
fixture traceback、実行失敗はありません。監査前後のlock hashは
`1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` で不変です。

直接release検証はcargo testとは別工程で、10 snapshots、最新値、確認Enterで通信増加なし、独立sの1PATCH、
正確な変更／解除・resolution0・milestone form、17 GET + 1 PATCH、追加の一覧／Comment／成功確認GETなし、
更新Detail＋旧一覧title＋Comment保持、exit0・空stderr・termios復元をassertしました。fixture server stderrも空です。
今回の変更後release SHA-256（`release-hashes.log`）:

- `bl`: `6281c47ec4788054ddfc6a4ed8f2b37c842c42287ea61b85c70dd948a59121c3`
- `bl-cli`: `7ba53d9ef675b1bb93f7d601c6e585dc5fac8ad7ea16714eed6c93662ba1c89a`

一時ログは永続成果物ではありません。新規失敗条件は上の公開PTYテスト、通常release手順はCONTRIBUTINGで再実行可能です。
**実端末／実IME・実Backlog必須受入は引き続き未実施（pending）。Rootの独立review／Code Review gateも別工程です。**
ローカル成功をIssue完了・READY_FOR_MERGE・merge承認として扱いません。実サービス接続／書込みやGit変更は行っていません。

### #21 Root検証で判明した既存create試験の非同期カウント修正

Rootの `/tmp/backlog21-root-fixed.Xe6rA2/test.log` は `tui_process` が98成功／1失敗。
`bl_create_refresh_swaps_history_only_after_both_reads_and_never_injects_created_issue` のlist失敗ケースで、
`request_counts[8]=12`／`[9]=14` により「history navigation must be local」が失敗しました。
同じassertは既存HEADにもあり、全要求カウントにページ再訪時の非同期preview＋project GET 2件が混入します。
一覧履歴のローカル性を検証するため、この比較だけを既存fixtureの `browse_request_counts`（Detail／Comment／個別project取得を除外）へ変更。
count／listのHTTP回数・同一path、POST一回、作成課題非注入、選択／page／scroll保持のassertは変更していません。
製品コード・依存・Plan・他テスト、上記の過去成功履歴は変更していません。

| 限定再検証 | 終了コード／結果（ログ `/tmp/backlog21-count-fix.TiRmkF/`） |
| --- | --- |
| `cargo test --locked --test tui_process bl_create_refresh_swaps_history_only_after_both_reads_and_never_injects_created_issue -- --exact` ×3 | 各0／各1成功、`exact-1.log`〜`exact-3.log` |
| `cargo test --locked --test tui_process`（timeout 600秒） | 0／99成功・0失敗、既存create系と`bl_update_`全件を含む。44.59秒、`tui-process.log` |
| `cargo fmt --all -- --check`／`git diff --check` | 各0／出力なし、`fmt.log`／`diff-check.log` |

各終了値は同名の`.exit`に記録。testの警告・fixture tracebackなし。ローカルHTTPS／fake keyのみで実サービス・secret参照・Git書込みなし。
全対象test／Clippy／audit／release build／実端末・実IMEは今回再実行せず、Rootの全体検証と独立review gateは別工程です。

### #21 修正後のRoot独立検証・限定再レビュー（2026-10-30 UTC）

RootはP1修正と既存create試験のカウント比較1行を直接確認しました。採用したP1は、更新開始前の未完了一覧／Commentによる対象ずれ、不完全な成功objectの描画panic、最新GET／PATCHの別key応答採用の3件です。いずれも既存の通信失効・表示用検証と対象key照合の最小修正で対応し、不採用の指摘はありません。CLIの部分object成功契約は維持しました。

初回の独立Correctness／Ponytailレビューが同じ3原因を指摘し、fresh限定再レビュー `5677285a-cb47-4d8a-9652-29b792f3fac5` で3件解消・影響範囲の追加指摘なしを確認しました。全体2レビューの無条件再実行はしていません。同Reviewerはカウント比較修正も確認し、Rootの全対象test再実行を残条件としていました。

| Rootが実行した検証 | 終了コード／結果 |
| --- | --- |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets`（カウント比較修正後） | 0／332成功、0失敗、既存manual専用2件ignored。警告／fixture tracebackなし |
| `cargo clippy --locked --all-targets -- -D warnings` | 0／警告なし |
| `cargo audit --file Cargo.lock`（P1修正後） | 0／cargo-audit 0.22.2、230依存・1386 advisory、警告／DB・registry取得失敗なし。lockfile SHA-256は前後とも上記 `1d1474e5…` と一致 |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0／P1修正後。後続変更はテストassert1行と受入記録のみ |
| CONTRIBUTING #21のPython blockを抽出したrelease製品の直接HTTPS／PTY検証 | 0／17 GET＋1 PATCH、確認Enter非送信、変更・解除form一致、旧一覧／Comment保持、exit 0・空stderr・端末復元 |
| `git diff --check` | 0 |

最終Rootログは `/tmp/backlog21-root-fixed.Xe6rA2/` の `final-exits.log`／`final-{fmt,test,clippy,diff}.log`、P1修正後のaudit／build／directログです。同ディレクトリの旧 `test.log` の101は失敗履歴として保持し、最終成功へ書き換えていません。最後のshell wrapperは異常語句検索の無一致によって終了1でしたが、各正式チェックの終了コードは直後に個別取得して上表の0を確認しています。

対象releaseはP1修正後の `bl` SHA-256 `6281c47ec4788054ddfc6a4ed8f2b37c842c42287ea61b85c70dd948a59121c3`、`bl-cli` は `7ba53d9ef675b1bb93f7d601c6e585dc5fac8ad7ea16714eed6c93662ba1c89a` です。

実Backlog受入について利用者は、origin `https://lazybl.backlog.com/` のBLCLI-2でタイトル末尾へ ` [#21更新検証]` を追加し期限日を解除した後、この2項目だけ検証開始時の値へ復元することを明示許可しました。Rootはdefault profileのorigin一致を確認しましたが、Agent環境にAPI keyがないため、別の認証情報源を探索せず利用者の認証済み端末での操作を依頼します。現時点では実端末／実IME・実Backlog受入は未実施です。Code Review gateも未実施で、Issue #21完了・READY_FOR_MERGEとは判定していません。

### #21 実端末フィードバック：単行入力の編集キー横取り修正（2026-10-30 UTC）

#### 利用者からの修正前buildの報告（今回の修正後buildへ流用しない）

- 利用者が確認した修正前 `bl` SHA-256 は `6281c47ec4788054ddfc6a4ed8f2b37c842c42287ea61b85c70dd948a59121c3`（hash一致）。
  タイトル内Ctrl+Aが行頭移動でなくAssigneeへ移る不具合を報告した。
- 許可済みBLCLI-2のタイトル末尾 ` [#21更新検証]` 追加／期限日解除、およびその2項目だけの復元を完了し、
  Webでも元値へ戻ったことを確認したという回答を得た。Workerによる実Backlog接続／書込みではない。
- CommentがRまで変わらないのは正式仕様どおり。Issue Listのタイトル末尾は見えず、手動での旧タイトル保持確認は未完了
  （自動試験による保持確認とは区別）。ほかIME／resize／editor／終了の個別成功は未報告であり、成功と推測しない。

#### 限定修正・公開境界でのRED→GREEN

正式Planの「通常文字や編集キーをグローバル操作に横取りしない」へ合わせるbug fixのみ。
`src/tui/create.rs` の作成／更新共用eventで、Formかつpopupなしのタイトル／開始日／期限日に限り、
インストール済み `tui-input 0.15.5` の公開 `backend::crossterm::to_input_request` が認識する編集を先に既存入力部品へ渡す。
公開APIの実ソースも確認した。値が変わらない空欄／端でも認識したeventを消費し、focus shortcutへ漏らさない。
キー一覧を複製せず、`src/tui/input.rs`／依存／Plan／service/APIは変更していない。
入力欄外のCtrl+Y/A/B、編集キーでないCtrl+T/P、Tab/Shift+Tab、Enter/Esc、popup、phase、Ctrl+C、極小保護は維持する。

新規 `bl_create_and_update_single_line_editing_precedes_form_shortcuts_even_when_edit_is_noop` は、
実blのPTY入力→描画＋localhost HTTPSを公開seamとして作成・更新の両方を通す。
空欄のCtrl+A/B/Y、行頭のCtrl+A/Bのno-op後も入力欄に留まり、先頭挿入／1文字戻り／Ctrl+K削除→Ctrl+Y貼戻しを確認。
日本語・結合文字・ZWJ emojiの貼付けとq/h/l/j/k/H/Lの通常文字、日付2欄の同じ編集、欄外の既存shortcut、
確認Enter非送信→独立sの1回送信を描画と原文formで照合する（実IME未確定文字の試験ではない）。
既存の安全表示原文保持・busy保護・中断・対象固定・成功field検証・別key拒否も全対象試験で維持した。
旧キー横取りに依存した既存試験／fixture steps／CONTRIBUTING直接手順は、先にTab等で欄外へ出る操作へ整合。
開始日からは期限日も入力欄なので、shortcut前にTabを2回使う。

| 検証 | exit／結果 | ログ |
|---|---|---|
| `cargo test --locked --test tui_process bl_create_and_update_single_line_editing -- --nocapture` 修正前 | **101 RED**、空欄Ctrl+A/B/Yの後にIssue typeへ移り、タイトル編集結果を描画できずtimeout | `/tmp/issue21-keys/red.log` |
| 同試験、最小guard追加後 | **0 GREEN**、作成／更新両経路成功 | `/tmp/issue21-keys/green.log` |
| 既存create／updateの部分試験（手順移行途中） | 各101、開始日からTab1回では期限日に留まりCtrl+Bが編集になる旧shortcut手順が2件残存。Tab2回へ修正し、各個別試験0、下記全対象0 | `create.log`／`update.log`、`create-repaired.log`／`update-repaired.log`（同一tmp directory） |
| `cargo fmt --all -- --check` | 0 | `/tmp/issue21-keys/fmt.log` |
| `cargo test --locked --all-targets`（timeout 900秒） | 0、**333 passed／0 failed／2 ignored**。ignoredは実端末／IMEと本文editorの既存手動専用試験。tui_processは100件成功 | `/tmp/issue21-keys/test-all.log` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし | `/tmp/issue21-keys/clippy.log` |
| `cargo audit --file Cargo.lock`（cargo-audit 0.22.2） | 0、DB取得成功・1395 advisories／230 dependencies、脆弱性／warning報告なし。DB失敗による未検証ではない | `/tmp/issue21-keys/audit.log` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 | `/tmp/issue21-keys/build.log` |
| `git diff --check` | 0 | `/tmp/issue21-keys/diff-check.log` |
| CONTRIBUTING #20の自己完結Python blockを抽出し `python3 /tmp/issue21-keys/direct-20.py` | 0、**変更後release blを直接起動**。確認Enter=0 POST／独立送信=1 POST、原文／既定値解除／milestones／editor一致、refresh=2 count＋2 list、exit0／空stderr／端末復元 | `/tmp/issue21-keys/direct-20.log` |
| CONTRIBUTING #21の自己完結Python blockを抽出し `python3 /tmp/issue21-keys/direct-21.py` | 0、**変更後release blを直接起動**。最新値／変更・解除form一致、確認Enter非送信／独立s=1 PATCH、17 GET＋1 PATCH、旧一覧／Comment保持、exit0／空stderr／termios差分なし。server警告なし | `/tmp/issue21-keys/direct-21.log`、raw結果 `/tmp/issue21-direct-result.json` |

全検証はfake key／一時config／localhost HTTPSのみ。利用者config／secret探索、実サービス通信、Git書込みは行っていない。
HEAD `3d9523c4e01325fb30398d0923f29bb4fe92dada`＋保持した全未コミット実装＋今回の限定修正。
既存create履歴の `browse_request_counts` 修正を維持し、今回無関係の既存実装ファイルも開始時とbyte一致を確認した。
Cargo.lock SHA-256は監査前後とも `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` で不変。

変更後release SHA-256:

- `bl`: `a1fc596156e4223b93c5605382a6f7fc484c00c131fcc51f03a3af7ab0f5fe7b`
- `bl-cli`: `1b9bb92930d38b792073edc29c65d3bb9400337c3e29d46e2a1aad339f5e49f1`

**残件**: このhashのbuildについてRootの独立レビューと利用者による実端末再受入が必要。
タイトル／開始日／期限日でCtrl+A/B/Yの編集・no-op時focus保持と、Tabで欄外へ出た後のshortcutを再確認する。
修正後buildでの実IME／resize／editor／終了の個別確認と、Issue List旧タイトル保持の手動確認は未実施。
修正前buildの実Backlog更新／復元成功や過去レビューを、この修正後buildの成功へ流用せず、Issue #21の完了とは宣言しない。
Workerは実端末・実Backlogを実施していない。

### #21 入力優先修正後のRoot検証・限定再レビュー（2026-10-30 UTC）

Rootは共用フォームの追加6行と入力部品の公開キー判定を直接確認し、CONTRIBUTINGから#20／#21のPython blockを改めて抽出して変更後releaseを直接起動しました。両方の直接HTTPS／PTY検証、fmt／Clippy／audit／release build／diff checkは終了0。監査はcargo-audit 0.22.2、230依存・1386 advisory、警告・DB／registry取得失敗なし、lockfile前後不変です。ログは `/tmp/backlog21-root-keys.xjeAdU/`。

最初の全対象testは既存 `search_dialog_multiselects_all_fields_and_applies_identical_count_list_filters` がsummary待機で失敗して終了101でした。失敗結果のsnapshot11は、まだApplying中のdialog背面にある `> Issue List` を待機条件が拾った画面で、snapshot12では正しい `S:2 A:2 G:2` が反映されていました。今回の製品修正は作成／更新フォーム内だけで検索dispatchは未変更です。Rootは検索試験・fixture・期待値を変更せず、単独再実行の終了0を確認してから全対象を最初から再実行し、終了0／22 suite・333成功・0失敗・既存manual2 ignoredを確認しました。初回101は `test.log` に保持し、`search-exact.log` と `test-rerun.log`／`test-rerun.exit` を区別しています。既存検索PTY試験の待機raceによる不安定性は残り、再実行成功で隠していません。

fresh限定再レビュー `65700284-a7aa-42f6-9fe3-9be79d1a287d` は、この6行・関連試験・fixture・手順の範囲で追加指摘なし。編集no-opのfocus非漏出、欄外shortcut維持、phase／popup／Ctrl+C／極小保護、原文送信とcaller整合を確認しました。以前の全体2レビュー／3P1再レビューを無条件に繰り返してはいません。

対象hashは直前のWorker記録の `a1fc5961…`／`1b9bb929…` と一致しています。利用者へこの新buildでの実端末／IME・resize・editor・終了と、許可済みと同じ2項目の実Backlog変更／解除／復元の再確認を依頼します。現時点では新buildの手動受入と最終Code Review gateは未完了です。

### #21 入力優先修正後buildの利用者受入とComment更新への要望

利用者は新build `bl` SHA-256 `a1fc596156e4223b93c5605382a6f7fc484c00c131fcc51f03a3af7ab0f5fe7b` の一致を回答しました。以下は利用者の実端末／実Backlog操作・Web照合の報告であり、Rootが独立して実APIを読み取った結果ではありません。

- Ctrl+A/B/Yと日本語IME：OK。
- resize・editor往復・破棄確認：OK。
- BLCLI-2の変更／期限日解除・他項目保持・Detail反映：OK。
- タイトル・期限日の復元：OK。
- 終了コード0・通常shell利用：OK。
- 一覧タイトルについては、利用者が先頭を変更して更新確認済みと補足しました。末尾が不可視だったという先行報告とは区別します。この補足だけから、一覧Rの前後のどちらで新旧タイトルを確認したかは推測しません。自動試験の旧一覧保持の根拠は別に維持します。
- 端末環境は利用者の「前回と同じ」との回答に基づき、Herdr 0.8.0／Ghostty 1.3.1、macOS 26.7.1／Apple Silicon、macOS標準IME／101 keyboard・remapなしを引き継ぎます。今回のTERM／NO_COLOR個別値は未取得です。

同時に「CommentがRしないと更新されないのが納得いかない」との指摘がありました。現Approved Plan／基本設計とIssue #21は、課題更新後のComment自動更新を明示的に行わない契約です。Rootは現動作の理由と、自動更新への変更には正式な再計画が必要なことを説明し、方針確認中です。この要望をPlan変更の承認とは扱いません。最終Code Review gateは未実施で、READY_FOR_MERGEとは判定していません。

### #21 Comment自動更新への仕様変更要求による停止

後続Issue #22／#23の最新本文・コメントを取得し、#22はコメント追加後の最新ページ自動取得、#23は既存の同じコメントの成功応答反映であり、どちらも#21のDetail eによる課題更新後のComment非更新契約を変更しないことを確認しました。説明後、利用者は「では今回のIssueで修正する」と回答しました。RootはIssue #21で課題更新成功後のComment自動更新を求める変更要求として受け付けましたが、Plan改訂の正式承認とは扱いません。

現Approved Plan／基本設計／Issue #21はCommentを自動更新せずRへ任せる契約のため、状態を **REPLAN_REQUIRED** としてImplementationを停止します。自動取得後の表示対象・scroll／選択、取得失敗・中断時の保持と更新済み確度、旧応答失効、一覧非更新の維持と検証範囲は再計画で決定する未決事項です。このセッションでは自動更新の製品実装、Plan／基本設計／Issueの改訂、Planning／Workerの追加起動は行いません。

すべての未コミット差分・untracked fixtureを保持し、commit／stash／破棄／push／merge／release／deployは行いません。直前buildの実端末／実Backlog受入成功と復元完了、333件成功等の検証・独立レビューは旧仕様に対する履歴として残し、今後変更する差分の検証・承認には流用しません。最終Code Review gateは未実施です。正式な改訂Planの明示Approve・保存と、整合するWork Itemの確認・保存・公開を新しいPlanning Rootで行い、その後別のImplementationセッションで再開する必要があります。

### #21 再計画後：Comment自動更新の実装・ローカル検証（2026-10-30 UTC）

正式入力はIssue #21本文と全2改訂コメント（最新 `issuecomment-6525860173`）、Approved Planの「Issue #21 再計画：課題更新成功後のComment自動再取得」、基本設計「更新」。上記の停止記録・旧hash・旧受入・旧17 GET＋1 PATCHは履歴として保持し、新差分へ流用しません。今後の手順だけを改訂契約へ整合しました。開始HEAD `3d9523c4e01325fb30398d0923f29bb4fe92dada` と既存の全未コミット差分／untrackedを保持しています。正式Plan／基本設計、API／Service／lib、Cargo設定／lockfile／依存は今回編集していません。

#### 今回の差分と検証境界

- `src/tui.rs`：対象一致・表示準備・実Detail反映が成功した後だけ、固定keyで既存Comments Latestを起動。200後の不正body／JSON／必須field／別key応答では起動しません。結果待機の極小qを閉じ、送信中／取得中の極小画面にもq終了を案内しません。
- `src/tui/browse.rs`：既存20／desc／境界なし、本文時だけのissue／project記法GET、全preview準備、世代／key検査・runtime abort+joinを再利用。更新後Commentの完了を結果画面へ通知するメモリ内状態だけを追加しました。再select・count／list再取得・一覧の直接書換えはありません。
- `src/tui/create.rs`：作成の一覧更新とは分離した更新済み・コメント表示更新中／完了／失敗。既存refreshingの閉鎖ガード・scrollを利用し、Enter／Esc／再編集／再送／背面focusを待機中は受け付けません。失敗はupdated／旧表示保持・再送不要・Comment R案内を安全表示します。
- 実bl＋既存PTY入力／画面／exit／termiosとlocalhost HTTPSだけを公開seamとして追加5試験・既存更新経路を整合。private型／内部task数の固定や新harnessはありません。旧ページの非先頭選択・非ゼロscrollからLatest／本文先頭へ交換、通常本文・他者の最新投稿＋記法GET、本文なし履歴、正常空ページを区別しました。
- Commentの403／通信切断／不正JSON／必須field不正／記法GET拒否／後続項目の安全表示不正（伏字後object key衝突）の6ケースで、更新済みDetail・旧選択ID／scrollを保持。手動Rの503失敗でも保持し、その後R成功で交換しました。元から空／未取得の失敗保持→Rの正常空成功も別に確認しました。
- 遅延取得中にEnter／Esc・s／e／H／L・通常移動を送っても結果画面を維持。100×24／40×12／極小を往復し、長い固定対象keyの結果で40×12のEnd→Homeによる実scrollを確認しました。極小qでも終了しません。Ctrl+C／SIGINTそれぞれ通常／極小で130・updated／再送不要・termios全項目復元・4秒未満の終了を確認しました。
- 古い一覧page／一覧R／Comment G／Comment Rの成功と403失敗を遅延させ、新しい自動取得の完了・通知・内容を壊さないことを確認しました。旧Detail／取消loadの成功・失敗、未送信／無変更／取消、PATCH拒否／unknown／200後応答処理失敗の自動取得ゼロ、一覧条件／total／訪問済みページ／選択／scroll保持は既存試験を回帰しました。

#### RED→GREENと途中失敗（成功へ丸めない）

ログと実exitは `/tmp/issue21-comments/` の各 `.log`／`.exit` です。

| 実行 | exit／実測 | ログ |
|---|---|---|
| `cargo test --locked --test tui_process bl_update_local_acceptance_fixture_ -- --nocapture` 実装前 | **101 RED**。旧実装はUpdated結果からComment完了にならずPTY待機timeout | `red.log`／`red.exit` |
| 同コマンド、最小実装後の最初 | 101。Comment更新そのものは成功したが、試験の履歴labelをSUMMARYと誤記（既存表示はTITLE）。labelだけを訂正 | 初期`green.log`は後続成功で置換。toolが保存した当時のtail出力を`green-initial-tail.log`へ回収 |
| 同コマンド、訂正後 | **0 GREEN**。旧一覧＋新しいTITLE変更履歴、1 PATCH＋最新1 GET | `green.log`／`green.exit` |
| 既存更新試験の移行途中 | 101、16成功／1失敗。本文Commentの既存記法用issue GETが1増える期待の整合漏れ。成功確認GETではない | `update-first.log`／`.exit` |
| 追加Comment試験の最初 | 101、2成功／2失敗。全画面modalの背面Comment可視assertと、fixtureのraw body順序をdescと思い込んだ回復待機を修正。製品変更なし | `comments-first.log`／`.exit` |
| 同追加試験再実行 | 0、4成功 | `comments-second.log`／`.exit` |
| 更新試験の中間実行 | 101、21成功／1失敗。未取得Commentの1行本文領域に2行目statusが見えるとしたassertを、対象key保持／正常空表示ではないことへ修正 | `update.log`／`.exit` |
| 更新focusedの完走 | 0、22成功 | `update-final.log`／`.exit` |
| 長い固定keyによる結果scroll強化後の個別実行 | 0 | `scroll.log`／`.exit` |

途中の101は上記のとおり分離し、機能REDと試験期待ミスを混同していません。汎用refresh・retry・PATCH再送・永続化・競合merge・#22／#23は追加していません。

#### 正式チェック・releaseの直接実行

| コマンド | 実exit／結果 | `/tmp/issue21-comments/` のログ |
|---|---|---|
| `cargo test --locked --test tui_process bl_create_and_update_single_line_editing_` | 0、1成功（両フォーム） | `shared-input.log`／`.exit` |
| `cargo test --locked --test tui_process bl_create_` | 0、10成功 | `create.log`／`.exit` |
| `cargo test --locked --test service issue_update_` | 0、2成功 | `service-update.log`／`.exit` |
| `cargo test --locked --test issue_update --test comment_add --test issue_comment` | 0、issue_update 14／comment_add 14／issue_comment 15成功 | `cli-comments.log`／`.exit` |
| `cargo test --locked --test tui_shell` | 0、9成功 | `tui-shell.log`／`.exit` |
| `cargo fmt --all -- --check` | 0 | `fmt.log`／`.exit` |
| `cargo test --locked --all-targets` 初回 | 0、**338成功／0失敗／既存manual2 ignored**、22 suite。tui_process105成功。今回は既存検索PTY待機raceの失敗なし | `test-all.log`／`.exit` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし | `clippy.log`／`.exit` |
| `cargo audit --file Cargo.lock` | 0、cargo-audit 0.22.2。DB取得・crates.io index更新成功、1395 advisories／230依存、脆弱性・warningなし | `audit.log`／`.exit`、`audit-version.log` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 | `build.log`／`.exit` |
| `git diff --check` | 0 | `diff-check.log`／`.exit` |
| 更新済みCONTRIBUTING #21の自己完結Pythonを抽出し `python3 /tmp/issue21-comments/direct-21.py` | 0。**変更後release製品を直接起動**、確認Enter=0 PATCH、明示s=1 PATCH、解除・ID0・milestone form一致、18 GET＋1 PATCH。PATCH後は最新Comment GETだけ。旧一覧＋新しい本文なし履歴、fake-secret非表示、exit0／空stderr／termios差分なし。server stderr空 | `direct-21.log`／`.exit`、`direct-21-result.json`（fakeデータだけ） |
| 同CONTRIBUTING #20を抽出し `python3 /tmp/issue21-comments/direct-20.py` | 0。変更後releaseで作成確認非POST・独立1 POST、原文／解除／milestone／editor、2 count＋2 list、exit0／空stderr／termios復元 | `direct-20.log`／`.exit` |

本文なし履歴のこのfixtureは、初期space／projects／count／list各1、初期Detailのissue／project各1、初期CommentのComments／記法issue／project各1、編集loadのissue／project＋候補6、最後にPATCH＋Comment Latest各1で、**18 GET＋1 PATCH**です。本文ありの最新応答なら必要な記法issue＋project GETが別に2増えます。全fixture経路を一律件数へ固定していません。

監査前後のCargo.lock SHA-256はともに `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` で不変（`lock-before.sha256`／`lock-after.sha256`）。開始時に保存したAPI／Service／lib／正式Plan／基本設計／Cargo.lockのhash照合も全件OKです。

今回release SHA-256（`release.sha256`）:

- `bl`: `707bd5d1bf1ee0ed3af11932d4db5111788f53d6d2c2d1ec9f94f4b6ff13b4be`
- `bl-cli`: `87c1a7ac9fb3af62cb7592a7d55d80aa0ab7a55c783600843bff3e8902bb7fc3`

**残件／再開条件**: この変更後buildの実端末／実IME／実BacklogはWorker未実施、許可も導出していません。Rootが利用者に対象build・端末名／版・macOS・IME／keyboard・TERM／NO_COLOR、100×24／40×12／極小、入力優先・editor・破棄・独立送信・終了復元を確認し、profile origin `https://lazybl.backlog.com/`・対象候補BLCLI-2・変更／解除／復元の別の明示許可を得て必須受入を行う必要があります。RなしのComment最新表示とWeb照合、一覧R前後の新旧表示、未指定項目保持・復元を新hashで再確認します。旧buildの利用者成功を流用しません。2件の独立レビューと最終Code Review gateも未実施であり、このローカル結果だけではIssue完了／READY_FOR_MERGEではありません。Git stage／commit／stash／破棄、実サービスアクセス、Issue変更、push／merge／公開release／deployは行っていません。既存検索PTY待機race、SIGKILL／端末切断後の復元非保証、実API競合非検出は残ります。

結果scroll強化と旧失敗通知の非上書きassert追加後も `cargo fmt --all -- --check`、`cargo test --locked --all-targets`、`cargo clippy --locked --all-targets -- -D warnings` を最初から再実行し、すべて0でした（`fmt-final.log`／`test-all-final.log`／`clippy-final.log` と各 `.exit`）。全対象は再度338成功／0失敗／既存manual2 ignored、tui_process105成功で、検索PTYの再試行は今回も不要でした。製品ソースはrelease build時から不変、変更は試験／説明のみです。最終 `git diff --check` も0（`diff-check-final.log`／`.exit`）。stagedファイルなしを確認しました。

#### 最終追記：自動取得失敗のHTTP status表示（同日）

最終ソース照合でPlan要件7のHTTP status表示が既存Comment notice流用では欠けることを発見しました。公開PTYの失敗試験へ、Comment API拒否／記法GET拒否の両方で `HTTP 403` を結果画面に表示するassertを追加し、`cargo test --locked --test tui_process bl_update_comment_refresh_failures_ -- --nocapture` が101 RED（`http-status-red.log`／`.exit`）になることを確認しました。自動取得の失敗通知だけ、既存Errorの任意statusとmessageを合わせて既存safetyで安全化する最小修正後、同じ試験は0 GREEN（`http-status-green.log`／`.exit`）です。通常Comment R／API／Serviceの契約は変えていません。

この製品修正後に再実行した**最終結果**（すべて `/tmp/issue21-comments/`、各 `.exit` も保存）:

| コマンド／直接検証 | exit／結果 | ログ |
|---|---|---|
| `cargo test --locked --test tui_process bl_update_` | 0、22成功 | `update-status-final.log` |
| `cargo fmt --all -- --check` | 0 | `fmt-status-final.log` |
| `cargo test --locked --all-targets` | 0、338成功／0失敗／既存manual2 ignored、tui_process105成功。検索PTY待機raceの失敗・再試行なし | `test-all-status-final.log` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし | `clippy-status-final.log` |
| `cargo audit --file Cargo.lock` | 0、cargo-audit 0.22.2、DB取得・index更新成功、1395 advisories／230依存、脆弱性・warningなし | `audit-status-final.log` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 | `build-status-final.log` |
| `python3 /tmp/issue21-comments/direct-21.py`（更新済みCONTRIBUTINGから抽出） | 0。新releaseで18 GET＋1 PATCH、確認非送信／独立1 PATCH・form一致・旧一覧／Rなし最新履歴・fake-secret非表示・exit0／空stderr／termios差分なし・server stderr空 | `direct-21-status-final.log`、`direct-21-status-final-result.json` |
| `python3 /tmp/issue21-comments/direct-20.py` | 0。新releaseの共用作成フォーム・原文／解除／milestone／editor・非送信確認／独立1 POST・2 count＋2 list・exit0／空stderr／termios復元 | `direct-20-status-final.log` |
| `git diff --check` | 0 | `diff-check-status-final.log` |

監査前後lockfile hashは再度 `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` のまま（`lock-status-before.sha256`／`lock-status-after.sha256`）。保護対象6ファイルの開始時hash照合はすべてOK、stagedファイルなしです。

**Rootの再検証・今後の必須受入で使う最終release SHA-256**（`release-status-final.sha256`）:

- `bl`: `8e5312fa548567724777eb70940055d5ee2ae21380b08c04b10ad4fb9db2f91d`
- `bl-cli`: `d7e37ee164234fcf796939617cd9fc6bd9917b11a89747d09c0a7a094666f4f1`

直前の`707bd5d1…`／`87c1a7ac…`はHTTP status表示修正前のローカル履歴です。最終hashへの実端末／実Backlog受入、2独立レビュー／Code Review gateは引き続き未実施で、Issue完了とはしません。

### #21 改訂実装後のRoot独立検証（2026-10-30 UTC）

Rootは正式Issue本文・全2改訂コメントと改訂Plan／基本設計を再取得・確認し、blocker #10／#20のcompleted理由と実装／完了記録を照合しました。開始時の全差分・untrackedを保持し、今回追加分を一次コードとテストで再確認しました。ログは `/tmp/issue21-root.7YQMCL/` です。

| Root実行コマンド | exit／結果 |
|---|---|
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0、22 suite・338成功／0失敗／既存manual2 ignored。検索PTY待機raceの失敗・再試行なし |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし |
| `cargo-audit --version` | 0、cargo-audit 0.22.2 |
| `cargo audit --file Cargo.lock` | 0、DB取得・index更新成功、1395 advisory／230依存。脆弱性・警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| `git diff --check` | 0 |
| 更新済みCONTRIBUTINGの#21 Pythonを新たに抽出し `python3 /tmp/issue21-root.7YQMCL/direct-21.py` | 0。release blを直接起動、18 GET＋1 PATCH、確認非送信・指定form／解除／ID0／milestone一致、PATCH後はLatest Comments一回だけ。旧一覧保持・Rなし最新履歴・fake-secret非表示・exit0／空stderr／termios差分なし |
| 同#20を抽出し `python3 /tmp/issue21-root.7YQMCL/direct-20.py` | 0。共有フォームの非送信確認／独立1 POST・原文／解除／editor・2 count＋2 list・exit0／空stderr／termios復元 |

監査前後のlockfile SHA-256は `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` で一致。Root再build後も上記最終release hash `8e5312fa…`／`d7e37ee1…` と一致しました。実ログにRust warning・監査警告・HTTPS tracebackはありません。チェックと直接release検証は別工程として実行し、Workerの自己申告だけを証拠にしていません。

利用者はこの再開セッションで、origin `https://lazybl.backlog.com/`・profile `default`・対象 `BLCLI-2`、検証開始時タイトル末尾へ ` [#21更新検証]` を追加し期限日を解除、その後この2項目だけ検証開始時の値へ復元する範囲を明示許可しました。他項目の変更・コメント投稿・削除は許可範囲外です。Rootはdefault profileのorigin一致を確認しましたが、指定API key環境変数がAgent環境では空のため、別の認証情報源を探索していません。変更後buildの必須実端末／実IME・実Backlog受入は利用者の認証済み端末で行う必要があり、現時点で未実施です。独立2レビューと最終Code Review gateも別工程で、Issue完了／READY_FOR_MERGEとは判定していません。

### #21 新差分の独立2レビュー（2026-10-30 UTC）

別々のfresh built-in reviewerによるCorrectness Review（`27c79bec-6e14-454b-962f-4f553258df72`）と、ponytail-review Skill付きPonytail Review（`04f51bd0-ef3a-4f72-b0d0-757c5b416cf3`）を完了しました。両者は現在の全未コミット差分、untracked fixture、正式Issue本文・全改訂コメント、改訂Plan／基本設計、Rootの実チェック・直接release検証ログを読取り専用で確認し、具体的な修正要求なしでした。Rootも根拠と要求解釈を照合し、採用／不採用に分ける指摘はありません。旧333成功・旧受入・旧Reviewは根拠にしていません。

Rootがread-onlyで取得保存した `/tmp/issue21-root.7YQMCL/full-current.diff`（全3417行、SHA-256 `72221d32557b966a86bfc634c7c51cce86762f104fda6cdce4ad9306c57bb832`）と `issue21.json` を、shell権限のないReviewerにも提供しました。Reviewerによるコマンド再実行・実端末・実サービス検証は行っておらず、実行結果はRootログで確認した技術レビューです。必須の新hash実端末／実IME・実Backlog受入と、その記録後の最終Code Review gateは未完了です。

### #21 Comment自動取得buildへの利用者報告（2026-10-30 UTC）

Rootが最終release `bl` SHA-256 `8e5312fa548567724777eb70940055d5ee2ae21380b08c04b10ad4fb9db2f91d` の起動・検証手順を提示した後、利用者から以下の報告を受けました。これは利用者の認証済み実端末／実Backlog操作の報告であり、Rootの実API独立読取りではありません。

- 端末名／版・macOS、IME／keyboard・remap、TERM／NO_COLOR：前回／以前と同じ。
- 送信前の入力・resize・editor・破棄確認：OK。
- 許可済みBLCLI-2の変更／他項目保持／Detail反映：OK。
- RなしのComment最新履歴：OK。
- タイトル・期限日2項目の復元：OK。
- 終了後のshell：OK。
- 一覧R前後については「一覧はRをしなくても更新されていたのが、今回更新されなくなった。なぜ？」との質問。今回の自動試験・直接release観測は改訂Plan要件10どおり一覧非更新。Rootは現在の一次コードと今回の追加差分、旧受入記録を照合し、今回削除した自動更新処理はなく、開始時の部分実装も一覧非更新だったこと、作成後の一覧自動再取得とは別契約であることを説明しました。以前のRなし更新の具体経路・buildは不明で、利用者の観測を否定／別操作と断定していません。一覧仕様の受入判断は確認待ちです。

環境の既存明示値はHerdr 0.8.0／Ghostty 1.3.1、macOS 26.7.1／Apple Silicon、macOS標準IME／101 keyboard・remapなし。TERM／NO_COLORの過去の個別値は記録がなく、「以前と同じ」から値を推測しません。利用者側hash実照合結果とTERM／NO_COLORの具体値も未取得です。最終Code Review gateは未実施で、READY_FOR_MERGEとは判定していません。

### #21 新build受入の残確認・一覧仕様の合意（2026-10-30 UTC）

上記報告の後、利用者は「今回はRで更新する仕様でよい」と明示回答しました。これは改訂Plan要件10どおりの受入で、新しい一覧自動更新の要求／Plan変更ではありません。以前のRなし更新についての原因は未特定のままで、今回の全差分・初期部分実装に削除された一覧自動更新はありません。今回の一覧非更新・履歴保持と一覧Rの契約の根拠は、現在buildの利用者受入、Rootの直接release観測と自動回帰を区別して維持します。

利用者は起動対象hashとして **`8e5312fa548567724777eb70940055d5ee2ae21380b08c04b10ad4fb9db2f91d`** を回答しました。これはRootが再build・直接検証した最終blと一致します。TERM／NO_COLOR取得の最初のprintfは全角スペースによる `command not found` で失敗し、この失敗を環境値の取得成功とは扱いません。その後提示したPythonコマンドに対する利用者の実出力は次のとおりです。

```text
TERM: 'xterm-256color'
NO_COLOR: None
```

したがって、今回の実端末はTERM `xterm-256color`、NO_COLORは未設定です。端末名／版・macOS・IME／keyboardは利用者の「前回／以前と同じ」に基づき上記の明示値を引き継ぎ、推測した個別値はありません。

この新hashに対する送信前のIME／単行編集・100×24／40×12／極小往復・editor／resize・破棄確認、許可範囲のBLCLI-2変更／期限日解除・未指定項目保持・Detail反映・RなしComment最新表示、タイトル／期限日の復元、終了後の通常shellについて、提示済み手順への利用者のOK報告を受入結果として記録します。障害注入・中断・旧応答・空結果・失敗時保持はlocalhost HTTPS／PTYの観測であり、実サービスへ障害や過大負荷を与えていません。Root／Reviewer自身による実API読取りや実IME操作の成功とはしません。

Issue #21の改訂後実端末／実Backlog必須受入は上記の利用者報告で確認しました。最終Code Review gateはこの記録と最終diff確認の後にRootが呼びます。skippedまたは現在差分に対する明示Approveを確認するまでREADY_FOR_MERGEとはしません。GitHub Actionsは未実行、stage／commit／stash／破棄／push／merge／release公開／deploy／GitHub Issue変更は行っていません。

### #21 / PR #52 CI検索PTY待機raceの限定修正（2026-10-30 UTC）

対象はbranch `feat/tui-issue-update`、CI対象SHA `684d62664593233484d9c240c30ef6f7ae1e3b6b`、Actions run `41092304496` / job `124189046171` のQuality Test失敗です。CIログは `tui_process` 104成功／1失敗・exit101で、`search_dialog_multiselects_all_fields_and_applies_identical_count_list_filters` の「summary must show all applied fields」が失敗しました。snapshot[11]は検索のApplying中で、背面の `> Issue List` と旧条件 `K:Any S:0 A:0` を拾い、[12]では適用済み `K:fix qhljk S:2 A:2 G:2` になっています。背景focusは適用完了の証拠になりません。単独再実行成功でこのCI失敗を取り消していません。

変更は `tests/tui_process.rs` の適用Enter直後のwait **1行だけ**を `> Issue List` から `K:fix qhljk S:2 A:2 G:2` に変更したものです。既存のsummary assert、選択値の行、巨大ID、count/list同条件、同project再確定の余計検索なし・4 queryの検証を維持しています。timeout増加・retry・固定sleep追加・期待値緩和、製品／共有driver／fixture／Plan／基本設計／依存の変更はありません。

以下のログと個別 `.exit` は `/tmp/issue21-ci.oUz8G0/` に保存しました。公開seamは既存の実bl＋PTYの入力・画面・exit／termiosと、localhost HTTPSのHTTP記録です。

| 実行 | exit／実測 | ログ（同名 `.exit`） |
|---|---|---|
| 修正前exact（完全な上記test名＋`-- --exact`） | 0、1成功。Rootの別の修正前exactも0で、CIのREDとは区別 | `worker-before-exact.log`（Root分 `before-exact.log`） |
| 同じtestの適用count/list応答だけ各`header_delay=0.25`を一時注入、旧wait | **101 RED**。h/Enterが適用前に進み、次のProject focus待機がtimeout。CIのsummary assert失敗とは現れ方が異なる | `worker-delayed-red.log`、注入差分 `worker-delay-only.patch` |
| 同じ遅延・同じ手順、新wait | **0 GREEN**、1成功。遅延以外の期待値は不変 | `worker-delayed-green.log`、`worker-delay-and-wait.patch` |
| 遅延注入を除去後のexact | 0、1成功。最終repoには遅延を残していない | `worker-after-exact.log` |
| `cargo test --locked --test tui_process search_dialog_` | 0、20成功、初回成功・再試行なし | `worker-search-dialog.log` |
| `cargo fmt --all -- --check` | 0 | `worker-fmt.log` |
| `cargo test --locked --all-targets` | 0、22 suite、338成功／0失敗／既存manual2 ignored、tui_process105成功。全対象初回成功・再試行なし | `worker-all-targets.log` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし | `worker-clippy.log` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0。bl SHA-256 `8e5312fa548567724777eb70940055d5ee2ae21380b08c04b10ad4fb9db2f91d`（製品不変） | `worker-build-release.log`、`worker-release.sha256` |
| `bash /tmp/issue21-ci.oUz8G0/worker-direct-multiselect.sh` | 0。CONTRIBUTING #17の直接release手順を利用し、/tmpの応答・stepsだけを対象testと同等の15 stepへ変更。15 GETのみ、count/list各2、全条件・巨大ID一致、新waitのsnapshot[11]は適用済みsummaryでApplying modalなし。選択値行・同project余計検索なし、fake-secret非表示、exit0／空stderr／termios復元 | `worker-direct-multiselect.log`、`worker-direct-applied-screen.txt`、`worker-direct-check.log` |
| 無変更のCONTRIBUTING #17直接release手順 | 起動0、26 snapshots、count/list各5のquery一致、fake-secret非表示、空stderr／termios復元、focus移動[18..25]でGET増加なし。ただし**実総数33 GETで文書記載28とは不一致**。28固定の初回照合assertは1、この不一致を成功へ丸めていない | `worker-direct-17.log`、`worker-direct-check-attempt1.log`（1）、不一致明記の残りの照合 `worker-direct-check.log`（0） |
| `git diff --check` | 0 | `worker-diff-check.log`（追記後は `worker-diff-check-final.log`） |

直接multiselect手順の/tmp準備は最初にsteps抽出対象を誤りIndexError（準備exit1）、未生成scriptの起動127でした。製品テストの失敗ではなく、抽出箇所を修正後の直接起動0と区別しています（`worker-direct-setup-attempt1.log`／`worker-direct-multiselect-attempt1.log`と各 `.exit`）。既存の汎用driverやfixtureへ変更を広げていません。上記#17の総GET数不一致も今回の対象外として記録し、CONTRIBUTINGの旧記述は変更していません。

今回のcorrectionは**test-only＋実測追記**です。新たな実端末／実IME／実Backlog操作は行わず、以前の製品受入を新たに実行したとはしません。依存不変のため監査は再実行していません。リモート再CIは未実行・未確認、変更後の独立レビューとCode Review gateはRootの別工程です。stage／commit／push／merge／PR変更・実Backlog接続／書込みは行っていません。過去の履歴・受入結果は保持しています。

Rootも修正後のexact（1成功）、`search_dialog_`（20成功）、fmt、全対象test（338成功／0失敗／既存manual2 ignored）、Clippy、release build、diff-checkを独立実行し、すべてexit0でした（同directoryの `root-*.log`／`.exit`）。全対象の初回失敗・再試行、Rust warningはありません。release bl hashは再度 `8e5312fa548567724777eb70940055d5ee2ae21380b08c04b10ad4fb9db2f91d` と一致しました。

別工程のRoot直接検証 `python3 /tmp/issue21-ci.oUz8G0/root-direct.py` もexit0です。同じrelease bl・同じ15 step・適用count/listだけ各250ms遅延で、旧waitは先行操作によるdriver exit1／timeoutを再現、新waitは適用済みsummary・Applying modalなし、4件の同条件count/list query・巨大ID・余計検索なし、15 GETのみ・fake-secret非表示・終了0／termios差分なしを確認しました。再現用の変更は/tmp内の応答とstepsだけで、repoに遅延・retry・timeout変更を残していません。CI元ログのassert失敗と、制御遅延時の後続focus待機timeoutは、同じ早期進行が異なるタイミングで現れたものとして区別します。

この限定差分についてfresh Correctness Review（`b5a398ae-61ee-409d-9c88-3b49f61844c2`）とfresh Ponytail Review（`bba9afc5-e061-4459-a781-6c6319406a6a`）を別々に実行し、両方とも修正要求なしでした。Rootは、旧focus markerとApplying背面の関係、新summaryが適用済み条件からのみ描画されること、期待query／既存assertの維持、Rootの実行ログと直接HTTP記録への根拠を照合して受け入れました。採用／不採用に分ける具体的指摘はありません。Reviewer自身のテスト再実行やリモートCI成功とはしていません。最終Code Review gateはこの記録後にRootが呼び、現在差分への明示Approveまたはskippedを確認するまで完了とはしません。

## Issue #22: TUIコメント追加

### Worker実装・ローカル検証（2026-10-30 UTC）

正式入力はIssue #22の最新本文・受入改訂コメント、Approved Plan `docs/plans/backlog-cli-initial.md`、基本設計 `docs/designs/backlog-cli.md`。開始HEADは `a89c90c`、開始時clean。対象はCommentの追加だけで、既存コメント編集（#23）・依存・設定・CI・Plan／設計・Issue状態は変更していません。

Commentの修飾なしPress `c`を既存modalへ接続し、本文／状態／担当者／完了理由と確認／取消だけを表示します。最新課題の実値を初期値とし、本文は空、create既定値は不使用です。共通Serviceの既存変更判定からopaque prepared commentを作り、同一method／formを確認・送信します。本文のみは1 POST、課題項目の実変更は任意の非空本文付き1 PATCHで、descriptionは送りません。state／resolution連動・retry・fallback・確認の追加GETはありません。既存のLatest comments取得と表示準備の原子的交換を再利用し、正常な課題応答はDetailに反映、Issue Listには読取りも直接変更も行いません。comment.addのPOST／PATCH確度を共通progressへ記録し、終了時は既存abort＋join後に観測値を通知します。

公開seamは事前承認済みの実bl＋PTY（入力／render／exit／termios）、localhost HTTPS（method／path／form／回数）、共通Service／既存CLIです。private関数形状の新規テストや新規driverは追加していません。9件の `bl_comment_add_` テストは本文のみ／同値候補、本文＋項目、本文なし項目、空／no-op、最新値・既定値非流用・候補外現値、独立status／resolution・ID0・明示解除、確認非送信と独立1送信、editor失敗／resize／極小／破棄保持、正常／失敗応答、表示準備失敗と旧Comment選択／scroll保持、空ページと本文ありの条件付き記法取得、取消loadと旧Detail／Comment応答、POST／PATCHの送信開始後／成功headers後／表示更新中のCtrl+CとSIGINTを確認します。送信前PATCH中断も別テストで確認します。既存作成／更新と全CLIの回帰も実行しました。

#### RED／GREEN・途中失敗の履歴

以下のログはすべて `/tmp/` に保存し、失敗やtimeoutを最終成功に読み替えていません。

| 実行コマンド／段階 | exit／実測 | ログ |
|---|---|---|
| `cargo test --locked --test tui_process bl_comment_add_body_only -- --nocapture`、実装前 | **101 RED**、Comment c未接続でAdd comment待機timeout | `22-red-body.log` |
| 同コマンド、最小modal→prepare→POST→refresh実装後 | **0 GREEN**、1成功 | `22-green-body.log` |
| `cargo test --locked --test tui_process bl_comment_add_patch_confirmation_interrupt -- --nocapture`、確認時progress修正前 | **101 RED**、送信前PATCHの確度がnot_updatedでなかった | `22-red-certainty.log` |
| 同コマンド、prepared methodの確度記録後 | **0 GREEN**、1成功 | `22-green-certainty.log` |
| `cargo test --locked --test tui_process bl_comment_add_ -- --nocapture`、項目slice追加 | 0、4成功 | `22-fields.log` |
| 同filter、失敗／中断coverage追加 | 101、4成功／2失敗。Unknownの通知にcomment.addがなく、別assertは差分描画のraw stdoutに連続文字列を期待していた | `22-failures-interrupt.log` |
| 上記修正の最初の再実行 | 101、compile E0369（PartialEqのないWriteOutcomeへ`==`を使用）。依存やenumを広げず`matches!`へ修正 | `22-green-failures-interrupt.log` |
| 同filter、通知・assert修正後 | 0、6成功。送信中もpreparedの確認内容を保持 | `22-green-failures-interrupt-2.log` |
| `cargo test --locked --test tui_process bl_comment_add_refresh_is_atomic -- --nocapture` | 初回101（fixture配列が最新順でなくOTHER ENTRY待機失敗）、2回目101（記法失敗sequenceをフォーム開始時に消費）。fixture期待順を修正し3回目0、1成功 | `22-refresh.log`／`22-refresh-2.log`／`22-refresh-3.log` |
| `cargo test --locked --test tui_process bl_comment_add_editor_failures -- --nocapture` | 初回101（tiny resizeと同時入力が保護で無視されるのにTab進行を期待）、2回目101（既存editorエラー文の期待違い）。操作順／既存文言を修正し3回目0、1成功。製品の保護・エラーを弱めていない | `22-editor.log`／`22-editor-2.log`／`22-editor-3.log` |
| `cargo test --locked --test tui_process bl_comment_add_cancelled_load -- --nocapture` | 0、1成功（4ケース） | `22-reversal.log` |
| `cargo test --locked --test tui_process bl_comment_add_ -- --nocapture` | 0、9成功 | `22-focused.log` |
| `cargo test --locked --test service --test comment_add --test issue_comment --test issue_update --test tui_shell` | 0、5 suite／72成功 | `22-service-cli.log` |
| `cargo test --locked --test tui_process bl_create_` | 0、10成功（共用単行入力testも含む） | `22-create.log` |
| `cargo test --locked --test tui_process bl_update_` | 0、22成功 | `22-update.log` |
| `cargo test --locked --all-targets`、最初の実行 | **コマンドtimeout**（240秒）、exit未取得・全対象成功ではない | `22-all-tests.log` |
| 同、2回目 | **101**、40×12のComment helpにc:addを足したためEsc:Issueが欠けた既存testが1失敗。n/p:pageへ短縮し操作案内を収めた | `22-all-tests-2.log` |
| `cargo test --locked --test tui_shell`、help修正後 | 0、9成功。40×12にもc:addがあるassertを追加 | `22-help-green.log` |
| `cargo test --locked --all-targets`、3回目 | **Worker runの1800000ms上限でabort**、exit未取得・全対象成功ではない。利用者許可で同じWorkerを再開 | `22-all-tests-3.log` |
| 再開後の `cargo test --locked --all-targets`（command timeout 1200秒） | 0、22 suite／347成功／0失敗／既存manual2 ignored | `22-all-tests-final.log`、`.exit` |
| 再開後の `cargo fmt --all -- --check` | **1**、短縮helpのmatch armを1行へwrapする差分のみ。`cargo fmt --all`（0）後に下記全チェックを再実行 | `22-fmt-final.log`、`.exit` |

再開前のfmt／Clippy／audit／build／直接release検証のexitは0でした（`22-fmt.log`、`22-clippy.log`、`22-audit.log`、`22-build.log`、`22-direct.log`）。auditは0.22.2、1395 advisory／230依存、警告なし、lock hashは下記と同じでした。help変更前buildの直接検証を最終sourceの証拠に流用せず、再開後のbuild／直接検証0（`22-build-final.log`／`22-direct-final.log`）に加え、最後のrustfmt後にも下表をすべて実行しました。

#### 最終sourceの正式チェック・release直接検証

実行環境はDarwin arm64、`rustc 1.99.1 (58cf8f9c3 2026-10-13)`／`cargo 1.99.1 (5f94df478 2026-08-27)`。下表ログと同名 `.exit` に個別終了コードを保存しました。

| コマンド | exit／実測 | `/tmp/`ログ |
|---|---|---|
| `cargo fmt --all -- --check` | 0、差分なし | `22-fmt-final-2.log` |
| `cargo test --locked --all-targets` | 0、22 suite／347成功／0失敗／既存manual2 ignored。tui_process114成功、新規9件を含む。warningなし | `22-all-tests-final-2.log` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし | `22-clippy-final-2.log` |
| `cargo audit --version` | 0、`cargo-audit-audit 0.22.2` | `22-audit-version-final.log` |
| `cargo audit --file Cargo.lock` | 0、通常DB取得／index更新成功、1395 advisory／230依存、脆弱性・警告なし（2026-10-30 UTC） | `22-audit-final.log` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0、両binary成功、警告なし | `22-build-final-2.log` |
| `python3 /tmp/issue22-direct-final.py`（CONTRIBUTING #22のPythonを抽出） | 0、下記2ケースとも期待どおりPASS | `22-direct-final-2.log` |
| `git diff --check`（本記録追記後） | 0 | `22-diff-check-final.log` |

監査前後のlockfile SHA-256はともに **`1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16`**（`22-lock-before-final.sha256`／`22-lock-after-final.sha256`、cmp exit0）。Cargo.toml／Cargo.lockの変更はありません。

最終releaseのSHA-256（`22-build-final-2.sha256`）:

- `bl`: **`4f10174bb04f846c9af266c8e0f4df84252714441be00c03804328a27b564162`**
- `bl-cli`: **`900777b7f5b8afed70c73d8055ac3335cc4a0b61d60b597fd1c221fc61737eaf`**

直接release検証はcargo testとは別に、この `bl` をlocalhost HTTPS／PTYで起動しました。CONTRIBUTINGの自己完結コマンドと `tui_comment_add.json`／`tui_comment_add_{post,patch}_steps.json` が再現入力です。**期待・実測とも各17 GET＋1書込み**。本文のみはPOST `/api/v2/issues/B-1/comments` にcontentだけ、項目付きはPATCH `/api/v2/issues/B-1` にstatusId=2／assigneeId空／resolutionId=0／非空commentだけを送信しました。確認EnterでHTTP増加なし、独立sの1回だけ、原文の空白・改行・日本語保持、fake secretと外部OSCの非表示、editorへ渡す元本文が空・一時file0600／削除、最新Comment表示、PATCH時Detail反映／POST時旧Detail、旧List保持をassertしました。書込み後はLatest comments 1 GETと既存のissue＋project記法GETだけで、list／count再取得なし。両ケースともexit0／空stderr／termios差分なし／server stderr空、PASSでした。

raw PTY結果とfake-key HTTP記録は `/tmp/issue22-direct-post-result.json`、`/tmp/issue22-direct-post-requests.json`、`/tmp/issue22-direct-patch-result.json`、`/tmp/issue22-direct-patch-requests.json`。途中実行の同名結果は最後の実行で置換されています。実key／機密本文は使用していません。

#### 必須受入・レビューの未実施事項

**Issue #22は未完了です。READY_FOR_MERGE／承認済みとは判定していません。** 上記はWorkerのローカル検証で、Rootの独立実行・fresh 2レビュー・最終Code Review gateはこの記録時点では未実施／別工程です。GitHub Actionsも未実行です。

実端末のIME・字幅／候補・100×24／40×12／極小往復・editor復帰直後の描画・終了後shellと、実Backlog API／権限／実データとの結合は必須ですが、Workerは実施していません。過去の#18／#20／#21受入やローカルharnessを#22製品の実受入へ流用しません。Workerは実Backlog・credentialsへアクセスせず、認証情報の探索・実書込み・復元を行っていません。

Root／利用者向けの許可済み受入範囲はorigin `https://lazybl.backlog.com/`、profile `default`、対象 `BLCLI-2`。本文のみ `#22 本文のみ検証` と、本文 `#22 状態変更付き検証`＋状態「処理中」の追加、その後**初期状態だけ復元**し、コメントは残します。これはWorkerへの実API実行権限ではありません。Rootの事前確認ではprofile originは一致し、Agent環境のBACKLOG_API_KEYは不在です。利用者の認証済み端末で最終hashを照合して実施し、登録内容・Rなし最新Comment・Detailと旧List・復元・環境情報と結果を追記する必要があります。実サービスへ障害注入・過大負荷・削除・自動cleanupは行いません。

stage／commit／stash／破棄／push／merge／release公開／deploy／Issue変更は行っていません。最終確認でstagedファイルなし。scratchログ／直接検証script／結果は `/tmp/` のみ、repoへの追加物は上記のfixture3点と既存文書への追記だけです。

### Root独立実行の失敗と限定test同期修正（2026-10-30 UTC）

上記Worker成功とは別に、Rootの独立実行 `/tmp/issue22-root.x71JMP/test.log` は **exit101** でした。
全対象の途中まで337成功／1失敗／既存manual2 ignored、`tui_process` は113成功／1失敗で、後続の `tui_shell` は未実行です。
同directoryのfmt／Clippy／audit／build／diffはexit0。test.log 467〜481行の失敗は
`bl_comment_add_refresh_is_atomic_preserves_old_selection_scroll_and_certainty_on_failure` が
`Comment R: refresh display` を待つ一方、実画面は `Created: B-1`／`Comment display refreshed` となったdriver timeoutです。
これを成功・単なる時間不足とは扱わず、過去のWorker結果も書き換えていません。

**原因はtestの記法失敗注入sequenceと開始同期の不足**でした。旧testは `OTHER ENTRY` を見た時点で `Lln`、
次の `OLD LINE 01` で `jjjjjc` を送り、独立したDetailの完了を待ちませんでした。
初期Commentはcontent:nullなので記法GET不要（`src/tui/browse.rs:164–169`）です。
Detailはissue GET後にproject記法GETを行います（同193〜208行）が、Commentのcは
`update_target`→`cancel_previews` で未完了Detailを取消します（`src/tui.rs:267–280`、`src/tui/browse.rs:583–611`、
取消taskのabort＋joinは `src/tui.rs:735–738`）。初期Detailがissue GET待ちのうちにcへ進むと、
`/api/v2/projects/2` の `[markdown, markdown, {}]` はフォーム開始で1番目、成功後Comment表示準備で2番目しか消費されません。
本来の3番目 `{}` へ到達せず、製品が正常に更新成功を示してtest期待だけが外れます。

#### 制御遅延による再現と修正の証拠

`/tmp/issue22-correction.4Fa546/repro.py` は既存fixtureと実 `target/debug/bl`／既存HTTPS・PTY driverだけを使用し、
初期Detailのissue GETに700msのheader遅延を注入しました。POST／PATCHをそれぞれ旧stepsと新stepsで1回ずつ実行し、
HTTPの原記録を各subdirectoryの `requests`、画面／失敗を `driver.out`／`driver.err` に保持しています。
制御script自体は期待どおりのRED／GREENとHTTP回数をassertしてexit0（`repro.log`／`repro.exit`）。

| 条件 | driver exit | 書込み前／後のproject記法GET | 結果 |
|---|---|---|---|
| POST、旧steps | **1** | **1／1** | 同じrefresh成功画面でfailure通知待機timeoutを再現 |
| PATCH、旧steps | **1** | **1／1** | 同じ誤注入・timeoutを再現 |
| POST、新steps | **0** | **2／1** | 3番目 `{}` で表示準備失敗、Created／旧表示保持／再送禁止／OLD LINE 06保持 |
| PATCH、新steps | **0** | **2／1** | 3番目 `{}` で表示準備失敗、Updated／旧表示保持／再送禁止／OLD LINE 06保持 |

旧HTTP順序は初期issue GET（取消）、フォームissue→project(markdown)、1書込み、最新comments→issue→project(markdown)です。
新順序は初期issue→project(markdown)の**描画完了後**、フォームissue→project(markdown)、1書込み、最新comments→issue→project({})です。
初期commentsと初期issueの相互到着順は実際にも前後しており、そこには依存していません。

repoの変更は **`tests/tui_process.rs` のこの1テストと本追記のみ**です。
`TITLE: Detail B-1` の描画待ちを先頭に加え、result snapshotを6→7へ更新しました。
formattingケースには上記700ms遅延を残して競合条件を常に試し、書込み前2／後1の記法HTTP数をassertします
（同5838〜5852、5862、5880〜5888行）。これはsleepによる成功待ちやtimeout延長ではなく、公開画面で完了を同期するtestです。
既存の失敗通知・旧選択／scroll保持・Created／Updated確度・再送禁止・Latest 1 GET・List非更新assertは維持しています。
製品・公開契約・Plan／設計・依存・共通harness／timeout・既存fixture・他テストは変更していません。

#### 修正後の実行結果

下記ログと同名 `.exit` はすべて `/tmp/issue22-correction.4Fa546/` に保存しています。
修正後の各testコマンドは初回成功で、timeout増加や成功するまでの再試行はしていません。

| コマンド | exit／実測 | ログ |
|---|---|---|
| `cargo test --locked --test tui_process bl_comment_add_refresh_is_atomic_preserves_old_selection_scroll_and_certainty_on_failure -- --exact --nocapture` | 0、1成功（POST／PATCH×5ケース、formattingは制御遅延あり） | `focused.log` |
| `cargo test --locked --test tui_process bl_comment_add_ -- --nocapture` | 0、9成功 | `comment-add.log` |
| `cargo test --locked --all-targets`（command timeout1200秒） | 0、22 suite／347成功／0失敗／既存manual2 ignored。tui_process114成功、tui_shell9成功、warningなし | `all-targets.log` |
| `cargo fmt --all -- --check` | 0 | `fmt.log` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし | `clippy.log` |
| `git diff --check`（本追記後） | 0 | `diff.log` |

test／記録だけの修正なのでaudit／release build／CONTRIBUTING直接release手順は再実行していません。
既存releaseのbl／bl-cli SHA-256は上記 `4f10174b…`／`900777b7…` と再照合一致しましたが、新規buildの成功とはしません。
Root失敗runの原HTTPは収集していないため、そのrun内の到着順そのものを直接証明したとはせず、同一の誤注入を制御遅延で再現した根拠と区別します。
実Backlog／credentials／実端末受入へはアクセスせず、stage／commit／stash／破棄／push／merge／release／Issue変更も行っていません。
変更後のRoot再実行・独立レビュー・最終Code Review gateと必須の実受入は別工程のままです。Issue #22の完了・READY_FOR_MERGEとは判定していません。

### Root再実行の別失敗・取消前HTTP到着同期と未解消の8秒cutoff（2026-10-30 UTC）

前節の修正後、Rootの `/tmp/issue22-root.x71JMP/test-corrected.log` は **exit101** でした。
前節のatomic refresh試験は成功し、別の新規試験
`bl_comment_add_cancelled_load_and_reversed_detail_or_comment_reads_cannot_replace_new_draft_or_registration`
が8秒のPTY期限で失敗しました。残stepsは `Comment display refreshed` → `LATEST REGISTERED`、
最終画面は `Updated: B-1`／`Registered; refreshing comments`／`Refreshing: please wait` です。
337成功／1失敗／既存manual2 ignored、`tui_shell` 到達前でした。このRoot runのHTTP原記録はなく、
初期CommentがDetailより先に表示されたことだけを原因とは断定していません。

今回の調査・ログは **`/tmp/issue22-correction2.DwZpq7/`** に保持しています。
実Backlog・credentialsは使わず、製品binaryと既存のlocalhost HTTPS／PTYの公開境界で調べました。
repoを変える前の4ケース、初期Detailのissue GETだけ700ms遅らせた4ケース、
全 `tui_process` 114件はそれぞれ成功しました（`repro-baseline.log`／`repro-old.log`／`tui-process-before.log`）。
これは失敗解消の根拠ではなく、初期Comment先行だけでは再現しなかった記録です。
各PTYの `driver.out`／`driver.err`、HTTP `requests`／`response.json`／`shell-steps.json` を残しています。

#### 別々に制御再現した二つの事象

Rootの明示許可で **/tmp内だけ** にHTTPS／PTYの診断copyを作りました。
repoのserver、driver、8秒期限は変更していません。

1. **取消前にHTTPへ到着しないsequence race（今回修正した範囲）**。
   `controlled.py tls` は初期Detail完了後、R直後のTLS acceptを600ms遅らせます。
   元stepsはbusy表示と200msだけでc／Escを送るため、Rと取消対象フォームのGETがserverに到達せず、
   次のフォームが古い403応答を消費しました。`tls-True-True/requests` には初期comments 1回と
   初期／最終フォームのissue GETだけがあり、8秒時点は `New generation owner` 待ち、画面は
   `API request was rejected` です。診断14秒期限でもdriver exit1、書込みなし。
   **これはRootの登録後refresh画面とは異なる失敗です。**
   `tls-fixed.py tls-fixed` は同じTLS遅延で初期Detail待ちと既存 `get-received` のremove／waitを加え、
   Rのcomments GETと取消フォームのissue GETが到着してから取消します。
   その結果はdriver exit0／製品exit0／空stderr／termios差分なし、Latest表示成功、
   issue GET3回・comments GET3回・PATCH1回でした。HTTPと各stepの時刻／画面は
   `tls-fixed-True-True/requests` と `driver.out` に保持しています。

2. **PTY処理遅延での8秒cutoff（解消を主張していない範囲）**。
   `load25.py load25` は診断copyの画面再構成1回に250msを加えました。
   `load25-True-True/at-eight.json` の8秒境界snapshotはRootと同じ残steps2件と
   `Updated: B-1`／`Registered; refreshing comments`／`Refreshing: please wait` です。
   旧GETはすべて消費され、PATCHは最初のkey送信から5.935秒、最新comments GETは6.255秒で到着しています。
   診断copyだけ14秒まで観測すると **製品exit0／空stderr／restored:true／termios差分なし**、
   Latestまで表示して約10.65秒でdriverも正常終了しました。
   **この制御条件では製品のrefresh stuckではありません。**
   人為的な処理遅延による同一症状の再現を、HTTP未収集の過去Root runの原因証明とは扱いません。

保存結果のassert確認は `python3 /tmp/issue22-correction2.DwZpq7/check-evidence.py`、exit0です。
`controlled-tls.log` のdriver exit1は意図したREDであり、script自体のexit0と区別します。
調査中の診断script生成に同じPython構文エラーがあり、診断呼出し2回が製品を起動せず終了1でした。
修正後の制御結果だけを上記に記載し、成功するまでの製品test再試行は行っていません。

#### repoへの限定修正と取消／key／世代の確認

変更は **`tests/tui_process.rs` の最後の1試験と本追記だけ**です。
初期Detailと旧Commentの表示完了を待ち、既存の汎用 `get-received` markerを消してRを送り、
次のmarker到着を待ってcへ進みます。同じmarkerを再度消し、取消対象フォームのGET到着も待ってEscを送ります。
これでbusy表示だけをHTTP送信済みの根拠にしません。1.3秒の旧応答遅延・1.4秒の旧応答後確認・4ケースと
結果／Latestの期待は維持しています。初期待ち追加に合わせ、旧応答後のdraft snapshot照合をindex4→5へ更新しました。
PATCHがB-1へstatusId=2だけを1回送ること、issue GETが4／3回・comments GETが2／3回で
取消対象も含む期待のsequenceを消費することをassertしました。
前節のatomic formatting test、他の8新規試験、#17、製品、fixture、timeout、依存、Plan／設計／APIは変更していません。

実装の確認では、cは固定Detail keyを取得してpreviewを失効させます（`src/tui.rs:267–280`、
`src/tui/browse.rs:583–611`）。runtimeは無効readをabortしてjoinし、フォームloadも世代違い／閉鎖時に
abort＋joinします（`src/tui.rs:735–752`）。read完了はbusy世代と課題keyで制限され、Latest完了も
更新世代とkeyの一致時だけ反映されます（`src/tui/browse.rs:321–331,362–435`）。
フォームのissue応答keyも照合されます（`src/tui/create.rs:55–69`）。今回その製品契約の変更は不要でした。

#### 修正後の正式実行と、実HTTPで再現した残存失敗

各ログと同名 `.exit` は `/tmp/issue22-correction2.DwZpq7/` に保存しています。

| コマンド | exit／実測 | ログ |
|---|---|---|
| `cargo test --locked --test tui_process bl_comment_add_cancelled_load_and_reversed_detail_or_comment_reads_cannot_replace_new_draft_or_registration -- --exact --nocapture` | 0、1試験内4ケース成功 | `focused.log` |
| `cargo test --locked --test tui_process bl_comment_add_ -- --nocapture` | 0、9成功 | `comment-add.log` |
| `cargo test --locked --all-targets`（command timeout1200秒、HTTP保存監視付きで**1回**） | **101、337成功／1失敗／既存manual2 ignored**。同じ取消／逆転試験の8秒cutoff。tui_process113成功／1失敗、後続tui_shell未実行 | `all-targets.log` |
| `cargo test --locked --test tui_shell`（未到達分を独立実行） | 0、9成功 | `tui-shell.log` |
| `cargo fmt --all -- --check` | 0 | `fmt.log` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし | `clippy.log` |
| `python3 /tmp/issue22-correction2.DwZpq7/check-evidence.py` | 0、保存済みRED／GREEN・8秒snapshotと継続成功・実full失敗HTTPをassert | `check-evidence.log` |
| `git diff --check`（本追記後） | 0 | `diff.log` |

full失敗の原HTTP・入力fixtureを **`captured-after/backlog-project-91610-49/`** に回収できました。
これは `comment_read=true, failing=false` の3番目のケースです。最初のspace GETからの実到着時刻は次のとおりです。

| GET／書込み | 秒 |
|---|---:|
| 初期comments | 0.192 |
| 初期Detail issue／project記法 | **5.190／5.192** |
| Rの旧comments | 5.410 |
| 取消フォームissue | 5.648 |
| 新フォームissue／最後のcandidate | 6.016／6.030 |
| B-1 PATCH（1回） | 7.613 |
| 登録後Latest comments（3番目） | **7.928** |

最新commentsには維持した300msのheader遅延があるため、最初のGETからでも完了は8.228秒以降です。
PTY期限はprocess起動から8秒なので、正常なrefreshが期限に収まらない時刻で開始されています。
この失敗では **期待の旧GETはすべて到着済み**で、sequence不足ではありません。
初期DetailのHTTP到着が初期commentsより約5秒遅れたことは実測ですが、TLS／transport／OS schedulingの
どれがこの空白を生んだかは未確定です。成功後のfetchが永久にstuckしたとは推測しません。
またRootの過去runそのもののHTTPではないため、Root runの正確な原因確定とも区別します。
full失敗後は無条件の再試行・製品変更・timeout延長・sleep短縮・期待緩和をせず、証拠を保持して停止しました。
fullログはwarningなしで、以前のatomic formatting試験・他の8新規コメント試験・既存update等は成功しています。

**全対象成功ではなく、元の8秒cutoffリスクは残っています。Issue #22完了／READY_FOR_MERGEとは判定しません。**
Rootの独立判断・freshレビュー／最終gate・必須の実端末／実Backlog受入は別工程です。
今回のtest／docs限定変更にaudit／release build／直接release実行は繰り返していません。
実Backlog・credentials・実端末・gitのstage／commit／stash／破棄／push等は行っていません。

### #22 ユーザー承認済みの試験方法訂正：複合シナリオだけ有限15秒（2026-10-31 UTC）

前節の全対象実行の失敗とHTTP時刻は履歴として保持します。本追補は**製品要件ではなく試験方法の訂正**です。
明示承認された範囲で、取消／逆転の4ケースを実行する最後のPTY試験だけ、各ケース全体の期限を8秒→15秒にしました。
`tests/fixtures/tty_shell.py` のtest専用 `SHELL_DEADLINE_SECONDS` は正の整数秒、未指定時は従来の8秒です。
`tests/tui_process.rs` の既存helperは8秒を維持し、`scripted_with_deadline(..., 15)` を使うのはこの1試験だけです。
これは製品のrequest timeout 30秒や応答性SLAを変更するものではありません。Ctrl+C等のelapsed < 5秒のassertは変更していません。

最小差分として、初期Detail待ち、既存markerのremove／HTTP到着待ち、旧応答の1.3秒遅延と
到着後のdraft確認用1.4秒待ち、成功／旧失敗 × Detail／Commentの全4ケースをそのまま保持しました。
固定B-1へのstatusId=2だけのPATCH1回、旧GETの消費数、旧失敗の非表示、新draft、Latest、終了0／stderr空／端末復元の期待は緩めていません。
製品ソース、他fixture、README／CONTRIBUTING、Plan／設計、依存、config／CIは今回変更していません。
継承した未コミット差分・未追跡fixtureを保持しています。

#### 公開bl／localhost HTTPS／PTY境界でのRED→GREENと期限の確認

記録は **`/tmp/issue22-test-method.VOg6jc/`**。`check-deadline.py` は前節で保存した同じ実入力を使用し、
初期Detailのheader delayだけ6秒に制御します（他の応答・marker・取消／登録stepは維持）。
これは8秒を超える正常終了の再現条件であり、**元の約5秒のHTTP到着差の原因証明ではありません**。
製品binaryは既存debug `bl` を直接起動し、実Backlog／credentialsは使用していません。

| コマンド／条件 | exit／実測 | ログ |
|---|---|---|
| 修正前 `python3 /tmp/issue22-test-method.VOg6jc/check-deadline.py extended`（15秒指定を旧driverは未対応） | **1 RED**、driver exit1、約8.14秒で新draft操作前にcutoff | `red.log` |
| 修正後の同コマンド | **0 GREEN**、driver／製品exit0、約9.75秒、9 snapshots、Latest、stderr空／restored:true／termios差分なし、旧issue／comments各3 GET、固定PATCH1回をassert | `green.log` |
| `python3 /tmp/issue22-test-method.VOg6jc/check-deadline.py default`（env未指定） | 0、期待どおりdriver exit1／約8.13秒でcutoff。default8を維持 | `default.log` |
| `python3 /tmp/issue22-test-method.VOg6jc/check-deadline.py blocked`（15秒指定、Latest後に意図的に終了せず未到達stepを待つ） | 0、期待どおりdriver exit1／約15.13秒でcutoff。PATCH1回／旧GET数もassert | `bounded.log` |

上記GREEN／default／blockedは独立fixtureで並行実行しました。意図したdriver非0と検証scriptの成功を区別しています。
script／各fixture入力／HTTP／driver出力と `.exit` を保存しています。恒久的な別frameworkやprivate内部testは追加していません。

#### 修正後の限定回帰（各コマンドexit0）

| コマンド | 実測 | ログ |
|---|---|---|
| `cargo test --locked --test tui_process bl_comment_add_cancelled_load_and_reversed_detail_or_comment_reads_cannot_replace_new_draft_or_registration -- --exact --nocapture` | 1試験内4ケース成功、16.04秒 | `focused.log` |
| `cargo test --locked --test tui_process bl_comment_add_ -- --nocapture` | 9成功、20.14秒。write certainty／取消／refresh原子性を含む | `comment-add.log` |
| `cargo test --locked --test tui_shell` | 9成功 | `tui-shell.log` |
| `cargo test --locked --lib tui::tests:: -- --nocapture` | 27成功、実端末用2 ignored。driver利用の入力／editor／初回SIGINT／復元失敗／blocking終了も成功 | `terminal.log` |
| `cargo test --locked --test tui_process bl_delayed_startup_is_quittable_and_interruptible_without_waiting_for_network -- --exact --nocapture` | 1成功。header／body待機時q／Ctrl+C／SIGINTのelapsed < 5秒と端末復元を維持 | `responsive-process.log` |
| `cargo fmt --all -- --check` | 成功 | `fmt.log` |
| `git diff --check` | 成功（本追記後も再確認） | `diff.log` |

今回workerは全対象testを繰り返していません。Rootが最終差分に対して正式fmt／**all-targetsを1回**／Clippy／audit／release build・直接実行を確認し、
その後freshな独立2レビューと実ユーザー受入／code gateへ進む必要があります。今回の限定成功を全対象成功やIssue #22完了へ読み替えません。
元の初期Detail HTTP到着の約5秒差の原因は未解決で、調査を再開したり製品不具合と断定したりしていません。
前回全対象の約254.79秒はAPI timeout試験（comment_add約60秒、comment_update／issue_create／issue_update／project_list各約30秒）と
TUI約57秒が主な内訳でした。今回suite高速化、skip、parallel／infra変更は行っていません。
実端末／実IME／実Backlog受入、release再実測、GitHub操作、stage／commit／stash／破棄／push等は未実施です。

### #22 試験方法訂正後のRoot独立検証（2026-10-31 UTC）

Rootは最後のdriver／helper差分を直接読み、複合シナリオ1試験だけ15秒、他は既定8秒、既存の中断応答性assertと全4ケースの期待が維持されていることを確認しました。以前の失敗・未解明の初期Detail到着差は履歴として保持し、この試験方法訂正を原因解明とは扱いません。

以下はWorkerの自己申告ではなくRootによる変更後の実行です。ログ・個別終了コードは `/tmp/issue22-root-final.kMhEFW/`。全対象テストはこの最終差分に対して1回実行しました。

| コマンド | 終了コード・観測 |
|---|---|
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0、22 suite／347成功／0失敗／実端末用2 ignored、warningなし |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし |
| `cargo audit --file Cargo.lock` | 0、cargo-audit 0.22.2、1395 advisory／230依存、DB取得・index更新成功、脆弱性・警告なし |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0 |
| `git diff --check` | 0 |
| CONTRIBUTING #22のPythonを新たに抽出した `python3 /tmp/issue22-root-final.kMhEFW/direct-22.py` | 0、release blのPOST／PATCH両方PASS |

監査前後のCargo.lock SHA-256はともに `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` で不変。release SHA-256はbl `4f10174bb04f846c9af266c8e0f4df84252714441be00c03804328a27b564162`、bl-cli `900777b7f5b8afed70c73d8055ac3335cc4a0b61d60b597fd1c221fc61737eaf` で製品ソース未変更の前buildと一致しました。

正式テストとは別に、このrelease blをlocalhost HTTPS／PTYで直接起動しました。POST／PATCH各17 GET＋1書込み、確認Enter非送信・独立s、原文form・安全表示、空の初期コメント本文・editor一時file保護／削除、最新Comment、PATCH時Detail反映とPOST時旧Detail、旧List保持、exit0・空stderr・termios差分なしを確認しました。書込み後はLatest comments一回と本文記法用の既存issue／project GETだけです。`direct.log` と同directoryの `issue22-direct-{post,patch}-{requests,result}.json` はfakeデータだけを保持します。

この時点ではfreshな独立2レビュー・実端末／実IME・実Backlog受入・最終Code Review gateは未実施です。Agent環境に指定keyはなく、実Backlogへの通信・書込みは行っていません。以前の8秒cutoffを隠さず、任意の負荷で必ず15秒以内に終わる保証はしません。GitHub Actions・stage／commit／push／merge等は未実施で、Issue完了／READY_FOR_MERGEとは判定しません。

### #22 fresh独立2レビューとRootの指摘採否（2026-10-31 UTC）

別々のfresh built-in reviewerでCorrectness Review（`22628965-4890-4495-acce-65ce592c5b4c`）とPonytail Review（`b646095c-3efd-4c03-af47-205321751a32`、ponytail-review Skill）を完了しました。最新Issue本文／コメント、正式Plan／基本設計、Rootが保存した全未コミット差分と未追跡fixture3点、正式チェックとrelease直接実行のログ・HTTP／snapshotを読み取り専用で確認しています。

両者とも具体的なP0／P1／P2指摘・修正要求なし。Rootもprepared method／form、最新値と明示解除、独立s、安全表示、成功後LatestとDetail／List境界、取消・世代・確度の根拠を照合し、採用／不採用に分ける指摘はありません。不要な追加仕様・高速化の提案や無条件review loopは加えていません。

Reviewer自身のコマンド再実行・実端末／実API成功を示す結果ではありません。実端末／実IME・実Backlog受入と最終Code Review gateは未完了のため、技術レビューのOKをIssue完了／merge許可へ読み替えません。Rootはdefault profile origin一致・指定API key不在とrelease bl hash不変を再確認し、認証情報を補完せず利用者の認証済み端末での許可済み範囲の受入を依頼します。

### #22 最終buildへの利用者の実端末・実Backlog受入報告

利用者は提示した手順に対し、次の結果を明示報告しました。同一内容の報告2件は1回の受入結果として記録し、重複投稿・再実行の指示には扱いません。

- 起動buildのbl SHA-256：`4f10174bb04f846c9af266c8e0f4df84252714441be00c03804328a27b564162`。Rootの最終release hashと一致。
- 接続先・profile・対象：許可済みの `https://lazybl.backlog.com/`、`default`、`BLCLI-2`。Rootは事前にprofile origin一致を確認済み。
- 開始時の状態：**未対応**。
- 本文 `#22 本文のみ検証` の投稿、本文 `#22 状態変更付き検証` と状態「処理中」の同時変更、登録内容のWeb照合・Rなしの最新Comment表示・Detailへの状態反映：**OK**との報告。
- 状態だけを開始時の「未対応」へ復元：**OK**。コメントを削除せず残す許可済み手順への報告であり、他項目の変更・削除は指示していません。
- 送信前のeditor往復・日本語入力、確認からEscで戻った際の入力保持・破棄確認、100×24／40×12／極小往復・resize・終了後のshell復元を含む提示済み手順：**OK**との報告。
- 端末名・版とmacOSは「以前と同じ」と明示回答。既存の確認済み値であるHerdr 0.8.0／Ghostty 1.3.1、macOS 26.7.1／Apple Siliconを引き継ぎます。今回のTERM／NO_COLOR個別値・IME／keyboard設定値は再取得しておらず、推測した新しい値は記録しません。

これは**利用者の認証済み端末での実操作・目視報告**です。Root／Worker／Reviewer自身による実API・実IME観測とはしません。再取得失敗・応答逆転・中断・単一送信のHTTP回数・一覧非更新等の詳細はRootのlocalhost HTTPS／PTY検証と自動試験を別の根拠とし、実サービスへの障害注入・負荷試験は行っていません。

Rootは最新Issue本文・受入コメントを再取得し、上記hash一致と、受入記録以外の差分が独立Review対象から不変であることを再確認しました。最新値による変更判定、確認・独立送信と保持・保護、成功後Comment／Detail反映、失敗時確度／旧表示保持とList非更新の4完了条件は既存の一次コード・ローカル検証・独立2レビューと照合し、追加の実Backlog受入条件は上記報告で確認しています。

テスト基盤全体の高速化は別テーマのPlanning kickoffを提示しただけで、この差分へ追加実装していません。最終Code Review gateはこの記録と差分確認の後にRootが呼びます。skippedまたは現在差分への明示Approveを確認するまでREADY_FOR_MERGEとは判定しません。GitHub Actions、stage／commit／stash／破棄／push／merge／release公開／deploy／Issue状態変更は行っていません。

### #54 共通HTTPS fixtureのTLS blocking改善とPTY失敗診断

Issue: https://github.com/minorunakamura/backlog-cli/issues/54 。承認済みPlan `docs/plans/test-infrastructure-performance.md` の共通fixture改善の記録です。過去のIssue #22記録・受入とは別のWork Itemとして扱います。
開始HEADは `a8b4cdcae02e1b3afd8beeb9db18bd452116238c`、開始時は正式入力として保護した未追跡の同Planだけでした。
実装差分は `tests/fixtures/https_server.py`、`tests/fixtures/tty_shell.py`、`tests/https_fixture.rs`、新規 `tests/tty_shell_fixture.rs`。製品src・依存／lock・Harness・Worker設定は不変です。本追記の編集対象はこの文書だけです。

#### 修正前の失敗と公開境界でのRED→GREEN

- Rootの修正前観測：実HTTPS fixture起動後にsilent TCP peerを保持すると、別のcafile検証付きHTTPS要求が0.504秒でtimeoutしました。実driver＋blのdeadline 1秒／存在しないremove対象による例外はexit1で、`sent_at`／`requests`等の安全な診断がなく、deadlineには残りstepのkeys sentinelも漏れました。
- Workerの新回帰は、HTTPSのhandshake timeout、PTY deadlineのscreen secret漏洩、例外のmissing path漏洩をそれぞれREDとして確認し、公開起動／失敗出力境界でGREENへ変更。focusedは32成功でした。TLS handshakeをhandler側へ遅延し、deadline／例外には秘密を含めない限定診断を追加しています。
- Python 3.9／LibreSSLではsetupのcafileが拒否されたため、seed通信は親Python 3.14、実driverは既存PATHの `/usr/bin/python3` 3.9を使用しました。証明書検証の回避はしていません。

Worker TDDログ：`/var/folders/04/cs5vwntd34d_fdbtdczbxg000000gn/T/pi-subagents-uid-501/async-subagent-runs/5b5b4769-a76e-48bf-b4f8-dadb51d54050/output-0.log`。

#### Rootの変更後独立検証

ログ：`/tmp/backlog-54-root-checks.0z3bDW/`。以下はWorkerのfocused結果とは別のRoot実行で、fullは共通fixture回帰として1回だけです。

| コマンド | 終了コード・観測 |
|---|---|
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0、350成功／0失敗／既存manual 2 ignored。内訳にhttps_fixture 2、tty_shell_fixture 2、tui_process 114、service 20を含む |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし |
| `cargo audit --version` | 実出力は `cargo-audit-audit 0.22.2` |
| `cargo audit --file Cargo.lock` | 0、1395 advisories／230 dependencies、警告なし |
| `git diff --check` | 0（Root実装検証時） |

監査前後のCargo.lock SHA-256はともに `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` で不変でした。

Rootは自動試験とは別のPython subprocessでも実fixture／実debug binaryを直接起動しました。

- 修正前と同じsilent peer保持＋0.5秒timeoutの証明書検証付きHTTPS要求は、peerを成功まで保持したまま **0.0069秒、status 200／body `[]`**。実debug `bl-cli` のtrusted project list／header認証が成功し、trustなしではHTTP記録を増やさずnetwork拒否でした。
- root／TLS／鍵にgroup・other権限がないことを確認。実driver＋blのdeadline／例外JSONは `step_index=1`、`sent_at` 1件と、method／queryを除いたpath／ordinal／件数を取得できました。query／header／key／screen／未送信keys／例外pathのsentinelは非露出、`child_pid` は消滅しました。
- 既存loaded modeは成功JSON／exit0／termios復元。serverのkill／wait後、TemporaryDirectoryのroot／鍵／記録が削除されました。

#### 未実施・未確定と今回の境界

6.9msは制御再現の単回観測であり、テスト基盤全体の高速化達成値ではありません。性能計測full 3回・release確認一式、実CI、実IME／実Backlogは未実施です。
既存の約5秒差やPTY不安定性をTLSと同じ原因とも、解消済みとも判断しません。timeout集約・PTY同期は別Issueの対象です。
この追記時点で独立Correctness／Ponytail Reviewと最終Code Review gateは未実施であり、Issue完了・承認済み・READY_FOR_MERGEとは判定しません。
新しいログ専用保存機構は作らず、既存文書への記録に限定しました。この文書追記では独自検証を追加せず、行末diff checkだけを行い、stage／commit／merge／release／deployは行いません。

### #55 実30秒timeoutの6ケース集約（2026-10-31 UTC）

Issue: https://github.com/minorunakamura/backlog-cli/issues/55 。正式Plan `docs/plans/test-infrastructure-performance.md` の実装方針2だけの記録です。
開始HEAD `7483b4b7ea68e88ad4aa6c3d6bf4002568173010`、開始時clean。製品src、共通Harness、fixture、依存／lock、CI、Planは変更していません。
新規 `tests/request_timeout.rs` の単一テストが独立Harness／検証付きHTTPS／実bl-cliの6ケースをscoped threadsで開始し、panic時も全てjoinしてケース名を報告します。
**旧5関数を残した状態で新groupの全6ケース成功とlibtest並列数1での成功を確認し、その後に旧5関数だけを削除**しました。
既存のsignal・切断・拒否・decode・表示失敗試験は残しています。

環境: Darwin arm64／macOS 26.7.1、logical CPU 10／32 GiB、rustc／cargo 1.99.1、親Python 3.14.9、PTY用 `/usr/bin/python3` 3.9.6、OpenSSL 4.0.3。RUST_TEST_THREADS未指定、他のbuild／test計測と重ねていません。
Cargo.lock SHA-256は `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` で不変です。
ログ: `/tmp/backlog-55-worker.8Xihl2/`（Worker実測。Rootの独立検証ではありません）。

| コマンド | exit・wall／結果 |
|---|---|
| `/usr/bin/time -l cargo test --locked --test request_timeout --no-run` | 0、1.13秒（既存targetを用いたbuild準備。cold build値ではない） |
| `/usr/bin/time -l cargo test --locked --test request_timeout -- --nocapture` | 0、wall32.40秒／suite30.84秒、単一テスト内全6ケース成功 |
| `/usr/bin/time -l cargo test --locked --test request_timeout -- --nocapture --test-threads=1` | 0、wall30.74秒／suite30.62秒、単一テスト内全6ケース成功 |
| `/usr/bin/time -l cargo test --locked --test project_list --test issue_create --test issue_update --test comment_update --test comment_add` | 0、wall24.77秒（build1.94秒を含む）、移設後71成功／0失敗 |
| `cargo fmt --all -- --check`／`git diff --check` | 0／0 |

各CLI開始直前からの経過秒は以下です。group wallが40秒以下という性能判定と、全ケースの `29秒以上40秒未満` のassertを別々に満たしました。
全6ケースでdelay32、exit1、stdout空、stderr改行付きJSON、network、httpStatus、必要なapiErrors配列を確認しています。
writeはoperation／outcome、readはwriteなしを確認し、下記の要求順・件数・全要求の認証header・formを照合しています（retry／余分なGET／fallbackなし）。

| 移設元ケース | 要求・結果 | default秒 | libtest=1秒 |
|---|---|---:|---:|
| project_list::body_reading_is_inside_the_thirty_second_request_timeout | projects GET 1、200／writeなし | 30.716934 | 30.486709 |
| issue_create::create_body_timeout_retains_created_without_retry | issues POST 1、既存argsの空白付きタイトル等のform、201／issue.create／created | 30.706411 | 30.487524 |
| issue_update::update_body_timeout_keeps_updated_without_retry | issue GET＋PATCH各1、summary=new、200／issue.update／updated | 30.721931 | 30.514817 |
| comment_update::success_body_timeout_retains_updated_without_retry | comment GET＋myself GET＋comment PATCH各1、content=new、200／comment.update／updated | 30.725477 | 30.483088 |
| comment_add::body_timeouts_keep_success_certainty_without_retry_for_post_and_patch のPOST枝 | comments POST 1、content=body、201／comment.add／created | 30.708886 | 30.502401 |
| 同PATCH枝 | issue GET＋PATCH各1、statusId=8／comment=body、200／comment.add／updated | 30.700642 | 30.516787 |

旧suite実行中、各suiteのTLS拒否試験付近でPython serverの `Exception occurred during processing of request` が計5件出ました（詳細tracebackなし）。Rust test失敗・Cargo警告はなく、fixture診断の変更はしていません。
Planの旧5suite timeout待機合計約180秒・独立probe group30.16秒は過去の参考値であり、今回HEADのbaseline再測定や全体高速化達成値ではありません。
最大6並列はhard deadlineではなく、既存Harnessのport読取／Command待機のhang限界を残します。汎用watchdogは追加していません。
正式all-targets／Clippyと別の直接CLI検証・独立ReviewはRootへ留保。full性能3回／全体150秒判定・release一式は最終Work Item、実CI・実端末／IME・実Backlogは未確認です。
commit／stage／stash／push／merge／release公開／deployは行っておらず、この記録だけでIssue完了・最終承認とは扱いません。

#### #55 Rootの独立検証・Review（2026-10-31 UTC）

同じ変更後treeでRootが独立実行しました。環境は上記と同じで、親PythonのSSLはOpenSSL 3.6.3、PTY PythonのSSLはLibreSSL 2.8.3です。OpenSSL CLIの版とは区別します。
`cargo fmt --all -- --check`、`cargo test --locked --all-targets --no-run`（wall0.78秒）、`cargo test --locked --all-targets -- --list`、`cargo clippy --locked --all-targets -- -D warnings`、`git diff --check` は全てexit0、Rustの警告なしでした。
正式一覧に新groupの単一テストがあり、旧5関数がないことを確認しました。旧5ファイルはHEADから該当関数だけ除いた内容と一致し、その他のbyteは不変です。

- `/usr/bin/time -l cargo test --locked --test request_timeout -- --nocapture --test-threads=1`: exit0、全6ケース成功、wall **30.95秒**／suite30.83秒、各CLI **30.708970〜30.758377秒**。
- `/usr/bin/time -l cargo test --locked --all-targets`: exit0、24 targets計 **346成功・0失敗・既存実端末用2 ignored**、wall118.67秒、timeout group30.16秒、移設元5suite計71成功。回帰確認1回であり、最終性能3回・150秒判定の代替ではありません。
- cargo testとは別に、Python subprocessから実debug `bl-cli` と既存HTTPS fixtureを6独立一時rootで起動しました。全6ケースのmethod／path／form／認証／件数、exit／stdout／改行JSON／apiErrors／status／write、各29〜40秒をassertし、exit0。group **30.171484秒**、各CLI **30.011262〜30.020426秒**、server stderr空、server kill／waitと一時root削除も完了しました。

ログと直接検証script: `/tmp/backlog-55-root/` の `fmt.log`、`no-run.log`、`list.log`、`timeout-single-libtest.log`、`all-targets.log`、`clippy.log`、`direct-cli.log`、`direct-cli.py`。直接検証scriptは実行時の元tool-callからそのまま抽出したものです。Cargo.lockは上記SHA-256で不変です。
正式全対象でも旧TLS拒否経路のPython例外見出しが5件あり、project_listでは `TLSV1_ALERT_UNKNOWN_CA` のtracebackを確認しました。新group／直接検証では発生せず、該当拒否試験とfixtureは今回不変のため、既存診断として残し、抑制・一括修正はしていません。

別々のfresh built-in reviewerでCorrectness（`e9bb1f3c-cffa-4cb7-90b3-1fa0d8fd730a`）とPonytail（`a60bff18-722e-4ff0-b876-eded6313d52d`）Reviewを実施し、両方とも修正要求なしでした。Rootも要求・実差分・根拠を確認し、採用して直す指摘・不採用の修正提案はありません。両Reviewerはread-onlyで、検証の再実行ではなくRootのログとコードを確認しました。
この追記は検証結果の記録だけで、実装・期待値は変更していません。最終Code Review gateはこの追記時点で未実施です。full性能3回・release一式は最終Work Itemへ留保し、実CI・実端末／IME・実Backlogは未確認のままです。

### #56 証拠のあるPTY取消前HTTP受信同期の限定修正（2026-10-31 UTC）

Issue: https://github.com/minorunakamura/backlog-cli/issues/56 。正式Plan `docs/plans/test-infrastructure-performance.md` の実装方針3／TDDと公開seamに沿う記録です。
開始HEAD `028cbdc4031072a78ce9c9aa382b69826fa49e96`、開始時clean、#54のTLS改善・失敗診断は実装済みでした。今回のコード編集は `tests/fixtures/tty_shell.py`、`tests/tty_shell_fixture.rs`、`tests/tui_process.rs` の当該取消試験だけです。
製品src／API／公開契約、依存／lock、Plan、config／CI、既存assert／timeout、editor試験のstepは変更していません。

#### Rootの実process制御と原因根拠（Worker実行とは別）

以下はRootが今回の開始treeから作った診断であり、Planning Scoutのprobeや過去の成功を流用したものではありません。
出典: `/tmp/backlog-56-root/probe-before-worker.py`（当初scriptのコピー）、`cancel-control.log`、`cancel-matrix.log`、`editor-matrix.log`（Workerは内容を読み取り、再実行はしていません）。`probe.py` は後に変更後driverの検証用へ編集されたため、初期実験の出典とは区別します。

- 取消試験は対象 `GET /api/v2/issues/B-1` のsequenceを初期／旧Detail refresh／取消load／新loadの4回で消費する前提ですが、元の取消前条件はLoading画面と0.2秒だけでした。
- 一時serverの `Handler.setup` で、次の1接続のTLS handshakeだけを0.65秒遅らせました。step0の`LR`で始まる接続だけ、またはstep1の`e`で始まる接続だけを個別に制御すると、元条件では取消直前の対象GET ordinal2または3が未受信でした。新loadがsequenceの旧応答を消費し、GET3／PATCH0のまま約8.01秒でdeadlineになりました。
- Rootの一時driverだけに対象GET ordinal2／3待ちを加えた同じ制御は、旧成功／旧403ともGET4／PATCH1で約4.21〜4.23秒で完走しました。旧header_delay1.4秒、新draft後1.6秒の観測窓は維持しています。この実験は具体的な受信同期不足の根拠であり、過去のSending cutoffと同一原因である証明ではありません。
- editorはnonzero／utf8／unlinkそれぞれで、現行pause0.08、notice直後pause0、notice後resizeだけ送って`Review /`描画を待ってからTab3＋Enter、の計9条件を実行し、全てexit0／対象GET2／PATCH1でした。notice時にBody focusを観測しています。12×40は編集を許可するサイズであり、resize原因の証拠はありません。editor操作や製品は修正せず、editorの過去cutoffの原因は未確定のままです。

#### 限定変更とWorkerの公開境界RED→GREEN

stepに任意の `wait_request:{"method":"GET","path":"/api/v2/issues/B-1","ordinal":2}` を追加しました。queryを除いたpathとmethodに一致する受信件数がordinal以上になるまで入力を送りません。
当該取消試験の2 stepだけにordinal2／3を指定しました。画面／既存marker／pauseは併用し、既存pauseの意味は変えていません。
HTTP記録を各判定で一度だけ読み、同じsnapshotの解析済みrecordsを条件と `request_counts`／`browse_request_counts` に使います。書込み途中の不完全なJSON行は既存失敗診断と同様に読み飛ばし、次の判定で再読します。browseの既存path判定は変更していません。
HTTP受信はclient側の成功headers観測・描画完了・editor復帰・resize適用を証明しません。

新回帰 `tty_shell_waits_for_the_matching_request_ordinal_before_sending_input` はprivate importではなく、実driverへstepを渡し、実PTY子processの入力を観測します。
空の記録、method不一致、path不一致、ordinal不足、不完全な末尾の各0.15秒の否定観測窓で入力が来ないこと、一致時に1文字が届くこと、同snapshotの全件数6／browse件数3、送信後の追加記録を含む最終7件、子終了とtermios復元を確認します。
この0.15秒は未成立時の有限の否定観測窓であり、製品同期の固定sleep置換ではありません。

Workerログ: `/tmp/backlog-56-worker/`。環境はDarwin arm64／macOS 26.7.1、logical CPU10、rustc／cargo1.99.1、親Python3.14.9＋OpenSSL3.6.3、通常PTYのPATH内Pythonは `/usr/bin/python3` 3.9.6＋LibreSSL2.8.3、OpenSSL CLI4.0.3、RUST_TEST_THREADS未指定です（`environment.log`）。新回帰の制御用子process／driverは親Python、既存の実bl試験は従来の隔離PATHのPythonを使います。

| Workerコマンド／ログ | exit・結果 |
|---|---|
| `cargo test --locked --test tty_shell_fixture tty_shell_waits_for_the_matching_request_ordinal_before_sending_input -- --exact --nocapture`／`red.log` | 101、未実装のdriverが条件成立前に入力を送信し子assertでRED |
| 同コマンド／`green.log` | 101、入力gateは通ったが新テストのbrowse期待値が誤り。query付きissue pathは既存browse判定で計数されるため、新テストの入力と期待値だけ訂正（既存計数仕様は不変） |
| 同コマンド／`green-corrected.log` | 0、1成功、suite1.23秒 |
| `cargo test --locked --test tty_shell_fixture -- --nocapture`／`fixture.log` | **101、2成功／1失敗**。新回帰と既存deadlineは成功。既存exceptionはfailure種別のassertで失敗。元assertは受信JSONを表示せず、この実行の種別・原因は未確定 |
| `python3 /tmp/backlog-56-worker/diagnose-exception.py`／`diagnose-exception.log` | 外側0／診断子0。既存exception手順を一時root／実HTTPS／実blで別途診断し、安全なfailure JSONを観測: FileNotFoundError、step1、0.257秒、受信4件、子回収。初回並列suite失敗の成功への置換や解消証明にはしない |
| `cargo test --locked --test tui_process -- bl_update_cancelled_loading_and_older_detail_responses_cannot_replace_a_new_form_or_success bl_update_editor_failure_keeps_latest_raw_body_other_fields_and_focus bl_comment_add_cancelled_load_and_reversed_detail_or_comment_reads_cannot_replace_new_draft_or_registration --nocapture --test-threads=3`／`focused-three.log` | 0、初回3成功、suite16.11秒 |
| 関連7 full-name focused、`--nocapture --test-threads=3`／`related.log` | 0、7成功、suite17.30秒。対象は下記 |
| `cargo fmt --all -- --check`／`fmt.log`、`git diff --check` | 0／0 |

関連7試験は `bl_update_validation_retains_draft_and_tiny_terminal_cannot_edit_confirm_or_send`、`bl_update_interrupt_aborts_joins_and_keeps_not_updated_unknown_or_updated`、`bl_update_comment_refresh_wait_blocks_closing_editing_focus_and_tiny_quit_but_allows_scroll_resize`、`bl_update_comment_refresh_interrupt_preserves_updated_certainty_and_restores_terminal`、`settings_editor_owns_cooked_input_and_immediately_redraws_after_resize_without_reloading_config`、`bl_comment_add_post_and_patch_interrupt_abort_join_before_headers_after_headers_and_refresh`、`bl_comment_add_editor_failures_resize_tiny_and_discard_keep_body_and_field_draft` です。
Cargo／Rustの警告はありませんでした。fixtureの既存exception失敗は未解決として保持し、成功までのsuite再試行、assert緩和、期限延長は行っていません。
既存の旧成功／旧失敗・取消の4GET消費、新draft／busy・固定対象の単一書込み・Latest／旧List・確度・復元assertと1.3／1.4／1.6秒等の遅延・否定観測窓は不変です。既定8秒、既存comment複合1試験の15秒、中断elapsed<5秒も維持しています。

#### 未解決・未実施と判断の境界

具体的な取消前受信同期不足だけを修正しました。過去2 cutoffの正確な原因、editorの失敗原因、今回fixture exception失敗の種別・原因はなお未確定であり、「解消」「完了」「READY」とは判断しません。
根拠を得る追加の安全なfailure種別観測は今後の調査候補ですが、根拠なしのeditor／resize／製品変更はしていません。
`CONTRIBUTING.md` と `.github/workflows/quality.yml` の正式定義を確認し、全対象・Clippy・監査・release等の正式チェックをfocusedで代替していません。全体関連チェック・別の直接起動検証・独立ReviewはRoot、固定10反復・full性能3回／release一式は#57へ留保します。
実CI、実端末／IME、実Backlogは未実施です。stage／commit／stash／push／merge／release／deploy／Issue変更は行っていません。最終承認・完了判断はRootのみが行います。

#### #56 failure種別assertの最小診断訂正（2026-10-31 UTC）

既存 `driver_failure` のfailure種別assertに、安全なallowlist metadataの `report` をassert messageとして追加しました。raw stderr／screenの追加、期限1秒・期待種別・secret拒否・復元等のassert緩和、driverの挙動変更はありません。
`cargo test --locked --test tty_shell_fixture -- --nocapture` を訂正後に1回だけ実行し、**exit101、2成功／1失敗、suite1.32秒**でした。ログ: `/tmp/backlog-56-worker/fixture-assert-report.log`、exit: `fixture-assert-report.exit`。
既存exceptionの安全reportは `failure=deadline`、`step_index=1`、`elapsed=1.009617708`、`sent_at=[1793433720.824693]`、`request_counts=[4]`、`request_count=4`、`child_pid=82932`。受信は `POST /diagnostic-probe` ordinal1／2、`GET /api/v2/space` ordinal1、`GET /api/v2/projects` ordinal1でした。今回の直接根拠は、期待するFileNotFoundErrorではなくstep1で1秒deadlineに到達したことまでです。deadlineに至る根本原因と、reportがなかった初回失敗の種別・原因は未確定のままです。
Rootへ新しい根拠を相談し、追加修正・再実行なしで文書とfmt／diff確認だけに留める了承を得ました。成功までの反復・期限延長はせず、解消とは扱いません。原因の追加制御観測と正式関連チェックはRootへ留保します。
`cargo fmt --all -- --check` と `git diff --check` はともにexit0（`fmt-assert-report.log`／`diff-check-assert-report.log`）。stagedファイルなし。先行workerの4ファイル差分は保持し、今回の追記・訂正はこの文書と `tests/tty_shell_fixture.rs` のassert messageだけです。

#### #56 Rootの独立検証・Reviewと最終判断（2026-10-31 UTC）

以下はRootの独立実行結果の記録です。ログ・scriptは `/tmp/backlog-56-root/`。この追記では再実行せず、初期実験の出典訂正と本項の追加だけを行っています。

- `direct-final-cancel.log`: 最終の実driverでTLS handshakeを0.65秒制御した3条件（取消対象成功／旧refresh成功／取消対象403）は全てexit0、GET4／PATCH1。変更後検証用の `probe.py` でraw form、固定target、新draft、候補project3、旧ERRORなし、secret非露出、8秒期限、termios／alternate screen／cursor復元をassertしました。
- `direct-final-editor.log`: 変更していないeditorの3 modeは全てexit0、GET2／PATCH1。原文保持、mode600、cooked／echo、一時ファイル削除をassertしました。
- `fixture-matrix.py`: 元HEAD driver／最終driver × 単独／3並列の有限比較では、例外FileNotFoundErrorを0.225〜0.257秒で観測しました。新gate回帰は元driverでRED、最終driverでGREENでした。この比較や後の成功を、既存fixture失敗の解消証明とはしません。

Rootが最終差分で実行した正式チェックは次のとおりです。

| コマンド／ログ | exit・結果 |
|---|---|
| `cargo fmt --all -- --check`／`fmt.log` | 0 |
| `cargo test --locked --test tty_shell_fixture -- --nocapture`／`fixture.log` | 0、3成功 |
| `/usr/bin/time -l cargo test --locked --all-targets`／`all-targets.log` | 0、347成功・0失敗・既存実端末用2 ignored、wall144.32秒 |
| `cargo clippy --locked --all-targets -- -D warnings`／`clippy.log` | 0、警告なし |
| `cargo audit --version` | 実出力 `cargo-audit-audit 0.22.2` |
| `cargo audit --file Cargo.lock`／`audit.log` | 0、1395 advisories／230 dependencies |
| `git diff --check` | 0 |

Cargo.lockの前後SHA-256は `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` で不変でした。fullは共通driverの回帰確認1回であり、性能3回の代替ではありません。TLS trust拒否経路には既存Python例外見出しが5件ありました（前#55にも存在）。製品／HTTPS fixtureは不変で、一括抑制していません。

freshな独立2 ReviewはCorrectness（`d9f57c7d-f7b6-4c1d-abe3-f9aa780bd08d`）とPonytail（`685d9ad3-f11f-4a1e-8f23-349d741bf495`）。両者とも修正要求なし、notes付きで限定差分はOKとしました。Rootも根拠を確認し、code findingsの採用・不採用はありません。ただし両Reviewerとも#56の全ACは未確認で完了承認不可と明言しています。

editor、歴史的2 cutoff、fixture deadlineの原因は未確定であり、後の成功で過去の失敗を取り消しません。Rootの最終判断も **#56未完了** です。Code Review gateはこの追記時点で未実施であり、通過しても未解決事項の解消証明とはしません。固定10反復／full性能3回／release一式は#57へ留保し、実CI・実端末／IME・実Backlogは未実施です。

#### #56 fixture deadlineのstep時間切り分けと例外注入pauseの限定訂正（2026-10-31 UTC）

今回の継続はfixtureの1秒deadlineだけを切り分けるもので、上記の失敗記録・未完了判断を成功へ書き換えません。開始HEADは同じ `028cbdc4031072a78ce9c9aa382b69826fa49e96`、開始時の4ファイル部分差分を保持しています。新しいrequest gate／取消stepのsourceは変更せず、今回の編集は `tests/tty_shell_fixture.rs` のexception注入stepと本追記だけです。

**Rootの追加観測（Workerは既存ログを読むだけで、制御実験は再実行していません）:** 出典は `/tmp/backlog-56-timing/`。`no-run.log` が指定する現行Cargo binary `tty_shell_fixture-b813aee28f6e1ac5` を使用し、PATH shimでdriverだけを一時的な時刻計測コピーへ差し替えました。`prepare-phase1.py` は当初の `prepare.py` のコピーで、HEADの元driver＋既存2試験並列、最終driver＋既存2試験並列、最終driver＋全3試験並列を各1回固定実行しました。元driverはexit101（exceptionがdeadline）、最終driverの2／3並列は各exit0でした（`original-two.log`／`final-two.log`／`final-three.log`）。後二者の成功を解消証明にはしません。

自然失敗の主証拠 `original/traces/20792.json` の時刻はdriver開始からの経過秒です。

| 事象 | 経過秒 |
|---|---:|
| child spawn終了 | 0.00225 |
| 最初のPTY出力 | 0.77515 |
| step0の `No projects` 条件成立 | 0.83605 |
| step0送信開始 | 0.93011 |
| step1の空wait条件成立 | 0.94415 |
| step1の既定pause0.08による実行予定 | 約1.02415 |
| 1秒deadline判定 | 1.00805 |

時間の大半はselect待機で、driverのCPU消費は約0.020秒でした。大きな画面parser／ログread遅延は観測していません。新request gateのない元driverでも同じ `failure=deadline`／step1／送信1件／受信4件を自然再現したため、新gateが原因との根拠はありません。初期PTY出力までのstartup遅延の内部内訳は未確定です。このtraceは過去Worker失敗時の欠落traceを回復したものではなく、それらの全内部理由を確定しません。

**制御比較と採否:** `prepare-phase2.py` のproduct exec wrapperはsleep0.72秒にwrapper自体の起動約0.2〜0.4秒が加わり、元／最終driver×既定／pause0の全4条件がstep0送信以前にdeadlineになりました。失敗は `phase2/` に保持し、step1のpause比較には不適合として原因根拠に採用しません。
`prepare-phase3.py` は相対sleepではなく「driver開始＋0.76秒で実blをexecする」絶対時刻制御です。最終driverでexception step1のpauseだけを比較し、それ以外のstep・assert・1秒deadlineは維持しました。

- `phase3/final-default/result.log` はexit101。`traces/22769.json` は最初の出力0.77859秒、初画面条件0.82367秒、step0送信開始0.91658秒、step1条件0.93406秒＋pause0.08、deadline1.01292秒でした（RED）。
- `phase3/final-immediate/result.log` はexit0。shimが一時test scriptのstep1にだけ `pause: 0` を指定し、`traces/22784.json` は最初の出力0.77359秒、初画面条件0.81837秒、step0送信開始0.91167秒、step1条件0.92807秒、remove試行0.92808秒、FileNotFoundError0.92811秒でした。安全診断・子回収を含む全既存assertが成功しました（GREEN）。

これらはRootの一時script／driverによる診断で、製品・リポジトリのsourceは変更していません。Rootは `before.sha256` で既存4ファイルの不変を確認しています。公開seamは承認済みのdriver step入力→実PTY子process／例外診断とcleanupのままです。

**採用した最小訂正:** `driver_failure` の `case == "exception"` だけ、step1の空wait／存在しないpathのremoveに `"pause": 0` を明示しました。このstepは画面や入力結果を待つ操作でなくFileNotFoundErrorの注入なので、既定0.08秒のpauseは不要です。新branch・重複テストは追加していません。first stepと2 step構成、step_index1／sent_at1件／受信4件、安全診断・全assert、**1秒deadlineと外側communicateの5秒上限を維持**しています。deadline caseの未成立wait、新gate回帰の0.15秒否定観測窓、driverの既定pause、製品／editor／他試験／Plan／依存／timeoutは変更していません。1秒以内の任意のstartup遅延まで保証できるという意味ではありません。

変更後Workerは `cargo test --locked --test tty_shell_fixture -- --nocapture` を固定1回だけ実行し、**exit0、3成功／0失敗、build2.19秒／suite1.31秒、警告なし**でした。ログ: `/tmp/backlog-56-pause-worker/fixture.log`、終了コード: `fixture.exit`。追加の制御実験、成功までのretry、timeout延長はしていません。
今回の訂正はfixtureの不要なpauseと1秒予算の競合に対する根拠の進展です。editor／歴史的cutoffは別扱いで原因未確定のまま、過去Worker失敗の全内部理由も未確定です。Rootの変更後独立検証・freshな2 Review・Code Review gateはこれからで、旧差分へのApproveは今回の再承認として流用せず、**#56全体の完了とはしません**。full性能3回／固定10反復／release、実CI・実端末／IME・実Backlogは今回未実施です。
`cargo fmt --all -- --check` はexit0。今回の差分の `git diff --check` もexit0、stagedファイルなしを確認しました。stage／commit／stash／Issue更新／mergeは行っていません。

**今回訂正後のRoot独立確認:** `/tmp/backlog-56-timing/after-fixture.log` は固定1回のfixture全3成功、`after-fmt.log` と `after-clippy.log`（`cargo clippy --locked --test tty_shell_fixture -- -D warnings`）はexit0、警告なし、diff checkもexit0でした。修正前にREDとなったphase3のdefault側shimを変更せず、訂正後のcompiled testを同じコマンドで実行した `phase3/final-default/result-after-change.log` もexit0です。`traces/26032.json` は初画面条件0.82458秒、step0送信0.91818秒、step1条件0.935667秒、remove0.935675秒、FileNotFoundError0.935695秒を記録しています。期待する例外・安全診断・子回収のassertと1秒deadlineを維持したまま、観測した予算競合に対する訂正を確認しました。

freshなCorrectness（`10c77e25-af5f-4c99-9fa5-31ba121a14bb`）とPonytail（`87230e8d-18d8-4c65-bbe8-f5fa4cadbc92`）の2 Reviewも追加修正要求なし、今回の局所訂正はnotes付きで採用可でした。Rootもこの判断を採用し、コード指摘の採否による追加修正はありません。この追記はRootが実ログとReviewを確認した結果の記録だけです。今回fullを再実行しておらず、前の347成功をこのliteral訂正後のfull結果には流用しません。Code Review gateはこの追記後の差分で取り直します。editor／歴史的cutoffとstartup内部の原因は未確定であり、#56全体の完了判断は引き続き保留します。

#### #56 editor追加診断の記録（2026-10-31 UTC、docs-only）

[Issue #56](https://github.com/minorunakamura/backlog-cli/issues/56) と正式Plan `docs/plans/test-infrastructure-performance.md` 方針3／制約を再確認しました。以下はRootの既存診断 `/tmp/backlog-56-editor/` のscript・実ログ・代表traceを直接読んだ記録で、Workerはテスト／build／実験／計測を新実行していません。開始時の4ファイル部分差分を保持し、今回の編集は本末尾追記だけです。

- `prepare.py` のPATH shimは既存shell helperの固定PATHに遮られ、最初の `baseline.log`／`queued.log`／`suite.log` はtrace未取得（0件）、queued制御も未適用でした。制御診断の根拠には採用しません。
- ただし `no-run.log` 指定の現行Cargo binaryによる `suite.log` は実試験結果です。**exit101、113成功／1失敗、59.68秒**。失敗は `selection_change_hides_other_body_immediately_and_late_preview_cannot_clear_new_busy_or_failure`、`tests/tui_process.rs:1043` の `h.requests().len()` が実10／期待11でした。editor対象は成功。新差分起因とは断定せず、修正／成功retryをしていません。
- `direct.py` は既存 `probe.routes`・実bl・証明書検証付きHTTPS fixture・実 `/usr/bin/python3`（3.9）driverを直接起動し、trace取得をassertしました。`direct-baseline.log` と `{utf8,nonzero,unlink}-baseline.json` は現行7stepの3 mode全てexit0、各1.534〜1.539秒でした。
- 代表 `baseline/traces/87456.json` の送信直前状態（driver開始からの秒）: step2 EDITOR READY **0.637、cooked=true／echo=true／input_saved=false**、step3 UTF-8 notice **0.730、cooked=false／echo=false／input_saved=true／Body focus／12x40**、次のConfirm **0.861、24x100**。条件成立と送信直前を混同せず、editor復帰後の状態を確認しました。
- `direct-queued.log`／`utf8-queued.json`／`queued/traces/87490.json` はUTF-8 notice後にSIGSTOP→waitpid停止確認→resize＋Tab3回＋Enterをキュー→SIGCONTという実制御で、exit0、1.536秒でした。
- `direct-split.log`／`utf8-split.json`／`baseline/traces/87508.json` はresize後のlayout待ち→Tab個別送信の比較です。送信直前はBody **0.861**→Status **0.972**→Resolution **1.085**→Review **1.213**→Confirm **1.341**、exit0、2.015秒。診断上の分割で本体stepは不変、成功だけでresize原因や解消とはしません。
- 上記direct各実行は固定B-1への単一PATCH／form `summary=Latest title!` のみ、原文 `Original body`、mode600、cooked／echo、一時body削除、8秒、termios／alternate screen／cursor復元、secret非露出をassertしました。
- 固定PATH回避の `prepare-observed-test.py` は `/tmp` のtest sourceコピーのcommon module参照pathとdriver launcher pathだけを変更し、`rustc` でbuildしました。rlibはCargo artifact JSON `artifacts.jsonl` の現行serde_json／reqwestから指定し、glob推測していません。初buildはreqwestの `--extern` 不足で失敗（`observed-test-build-first-failed.log`）、訂正後build成功。製品・依存・リポジトリsourceは不変です。
- `observed-suite-valid.log` はこの**診断用testコピー**で114成功／0失敗、58.24秒。`loaded-traces/{89031,89119,89213}.json` に3 mode各約1.535〜1.539秒と条件・キー時刻を取得しました。正式Cargo full／all-targets結果ではなく、先のsuite失敗の解消・成功への置換には使いません。
- 現source `src/tui.rs:785-845`／`src/tui/create.rs` と利用中crossterm／ratatui（`crossterm-features.log`）では、各event後にbackendの実sizeを照会し、editor後はraw再入→resize→drawします。raw再入は非flushのTCSANOWで、Resizeによるキーqueue削除を確認していません。元stepsに未完入力prefix混入の証拠もなく、同時resizeだけを原因にしません。

今回editorの失敗・時間損失・入力欠落は観測できず、修正根拠なしとしてeditor／driver／製品を変更していません。現行8秒・既存assert・遅延は不変です。startup内部／歴史的8秒cutoffの根因と新suiteの1失敗は未解決のまま、**#56未完了**を維持します。timeout延長・成功retry・根拠のない本体resize分割はしていません。
今回の確認はdocs diff checkと非docs差分hash前後一致のみです。stage／commit／stash／Issue変更／mergeは行わず、旧Approveを本追記後の承認へ流用しません。追記後差分のfreshなReview／Code Review gateは別途必要です。

#### #56 改訂追補に基づくテスト代表化（実装・静的確認、Root検証前）

正式Issue [#56](https://github.com/minorunakamura/backlog-cli/issues/56) の[優先追補](https://github.com/minorunakamura/backlog-cli/issues/56#issuecomment-6545857169) と、Approved Plan `docs/plans/test-infrastructure-performance.md`（SHA-256 `a6f2a2ac73a2156f9b25d1fb85db00ca3e3d2276585eca0ff331993669450491`）のA〜Fに限定して代表化しました。開始HEADは `028cbdc4031072a78ce9c9aa382b69826fa49e96`。開始時の5ファイル未コミット差分を入力として保持し、今回の編集は `tests/tui_process.rs` の6関数と本末尾追記だけです。過去の失敗・旧判断の訂正や、旧Issueの安定化全根因解消を目的にしていません。

| 候補 | ケース数 | 今回残す保証・変更 |
|---|---|---|
| A update editor失敗 | 3→1 | nonzeroと対応noticeをliteral化。既存7step、`Original body`、0600・cooked・echo・一時file削除、失敗後focusからConfirm、summary=`Latest title!`だけの単一PATCHを維持 |
| B update取消・旧Detail | 2→1 | 旧成功だけ。4応答sequence、header_delay1.4秒、pause0.2／1.6／0.3秒、既存8stepと対象GET ordinal2／3受信gateを維持。最新title表示・新draftにERRORなし・対象GET4回・固定B-1へのPATCH1回を確認。project3のrule＋4候補のGETとproject2の同4候補GETなしを明示（初期表示用project2 ruleは禁止しない） |
| C selection変更・遅い旧Detail | 2→2 | 旧200／500、全画面assert、header_delay0.45／0.85秒とpause0.19／0.6秒を維持。B-2から動かすキー前とB-3 busy観測前にそれぞれ対象GET ordinal1を既存gateで待つ。総要求数11のassertだけを両対象method/pathのGET各1回へ変更 |
| D create editor失敗 | 3→1 | nonzeroだけ。既存全step、空本文・他入力・cooked・一時file削除・GETのみのassertを維持 |
| E comment editor失敗 | 3→1 | nonzeroと対応noticeだけ。tiny時の非操作・discard取消／破棄・status保持・空本文・一時file削除・GETのみの全step/assertを維持 |
| F update不完全／型不正成功 | 28→3 | 疎object `{id:1,issueKey:B-1}`、`update_issue()` のsummary=false、milestone=`[{id:5,name:null}]`だけ。各caseの全step、Updated・Do not resend・旧Detail・PATCH1回・後続GETなしを維持 |

A〜Fの通常成功経路の静的見積りは **41→9ケース（HTTPS Harness／bl／PTY起動を32組、editor起動を6回削減）** です。実秒の短縮値・速度合否・flake解消の証拠ではありません。共通の `scripted` → `scripted_with_deadline(..., 8)` とexit0／restored=true／stderr空のassertは不変です。deadline／pauseを延長せず、新規skip／ignore・成功retry・抽象化・基盤・製品変更はありません。

削った交差保証は、utf8／unlinkとupdate・create・comment各フォームの組合せ、update固有の旧403と取消／新フォームの組合せ、全field個別不正とPATCH成功後表示／確度の組合せです。またCではcomments等の付随要求の総数増減を監視しなくなります。完全同等の保証とはしません。残る `tests/editor.rs` の3失敗原文保持、`src/tui.rs` の共有端末引渡し13ケース、`tests/issue_view.rs` の全必須field検証、別の旧成功／旧失敗代表は変更していません。fixtureの同snapshotによるrequest gate・実PTY gate回帰・deadline／exception診断と例外注入pause0も保持しています。

Workerの確認はsource／実差分の静的確認、`cargo fmt --all`、`git diff --check`、開始時snapshotとの比較（A〜F以外のsource不変、Plan／両fixtureのhash不変、本追記前の文書prefix不変）、stagedファイルなしの確認です。いずれも成功し、テスト／build／性能計測は実行していません。新規REDは製品・driver動作を変えないため追加していません。

**この追記時点でRootの変更後検証は未実施です。** Plan指定の4 focused（A〜Fを6試験として選択するtui_process、tty_shell_fixture、editor、共有端末引渡し）各1回とfreshなReview／Code Review gateはRootに留保し、Workerは重複実行していません。最終正式チェックと文書整備は別Issue #57の範囲であり、自動開始していません。過去の成功は今回の差分の実行証拠へ流用しません。過去のeditor／cutoff・startup内部原因等は未確定のままです。実CI・実端末／IME・実Backlog・別途release実操作も未実施です。stage／commit／stash／push／Issue変更／merge／release／deployは行っていません。

#### #56 改訂後のRoot独立検証（2026-10-31 UTC）

本項は上記代表化後の差分に対する新しい実行記録です。過去のチェック・Review・Approveは流用していません。RootがIssue本文・優先追補とPlan全文を再取得し、PlanのSHA-256一致を確認しました。依存#54はstate_reason=completedに加え、現行HTTPS fixtureのhandshake遅延化、安全診断と本書の実装・検証記録を確認しました。

環境: Darwin arm64、rustc 1.99.1 (`58cf8f9c3`)、cargo 1.99.1 (`5f94df478`)、親Python 3.14.9 / OpenSSL 3.6.3、PTY用 `/usr/bin/python3` 3.9.6 / LibreSSL 2.8.3、openssl CLI 4.0.3。`RUST_TEST_THREADS` は未設定、A〜Fだけ明示3並列です。開始HEADは上記と同じです。

ログ・終了コード: `/tmp/backlog-56-resume-root.czOzeP/` の各 `.log` / `.exit`。Plan「focusedコマンド」の4コマンドを順に各1回だけ実行し、全てexit0、Rust/Cargo警告なしでした。

| コマンド | 結果／suite時間 | ログ |
|---|---|---|
| `cargo test --locked --test tui_process --` にPlan記載のA〜Fの6 full-nameと `--nocapture --test-threads=3` | 6成功、0失敗・0 ignored、108 filtered out／8.82秒 | `focused.log` |
| `cargo test --locked --test tty_shell_fixture -- --nocapture` | 3成功、0失敗・0 ignored／1.31秒 | `fixture.log` |
| `cargo test --locked --test editor -- --nocapture` | 5成功、0失敗・0 ignored／0.14秒 | `editor.log` |
| `cargo test --locked --lib body_editor_roundtrip_and_failures_use_shared_terminal_handoff -- --nocapture` | 1成功、0失敗・0 ignored、29 filtered out／6.38秒。内部13経路は維持 | `shared-terminal.log` |

別工程の完了条件確認では、Rootが実差分と呼出し先を読み、残すケースの入力・画面条件・HTTP／原文／復元assertを要求と照合しました。さらに一時的なsource比較でHEADと現行のA〜Fのscripted入力を比較し、step数7／8／4／9／13／5、keys・resize・pause・画面waitが承認済みのnotice literal化とrequest gate以外は一致することをassertしました。A〜Fのケース構造は1／1／2／1／1／3です。比較用の初稿はBのfixture用jsonをstepsと誤認してJSONDecodeErrorで終了しましたが、抽出位置を `let result = scripted` 以降へ訂正して比較が完了しました。製品・テストsource変更やテストretryではありません。

実動作は上記focused内の既存入口（実debug `bl`・証明書検証付きHTTPS・実PTY／editor）で観測しています。Bはordinal2／3受信後の取消、最新title・project3 rule＋4候補、project2候補なし、B-1 GET4回／固定PATCH1回まで、CはB-2／B-3各GET受信・各1回と旧200／500のbusy／失敗表示まで、A/D/E/Fは残す入力・通知・送信／非書込み・cleanupと復元まで、対応assertが成立しました。fixture自己試験では誤method／誤path／ordinal不足／途中行で入力せず、一致時だけ進む実PTY観測と失敗診断・子回収も成功しました。これはテスト整理の直接確認であり、別途のrelease製品実操作・実IME／実Backlogの受入を行ったという意味ではありません。

`cargo fmt --all -- --check` と `git diff --check` はRootでもexit0でした。Plan・両fixtureとCargo.lockのSHA-256は開始時から不変で、製品src／依存／CI／既存JSON fixture／実timeout6ケースは変更していません。全体試験・Clippy・audit・release buildとCONTRIBUTING／AGENTS整備は#57へ留保し、自動開始していません。これらを今回実行済みとは扱いません。

この記録時点で今回差分のfreshな独立2 ReviewとCode Review gateはこれからです。過去のeditor／cutoff／startup内部原因は未確定のままで、今回の成功は根因解明やflake不存在・実時間短縮の証明ではありません。削った交差保証・実CI／実端末／IME／実Backlog／別途release実操作の未実施は上記のとおりです。

**今回差分のfresh独立2 Review完了:** Correctness `6cf607c7-97c5-413d-9fe1-fc869da1c12f` とPonytail `3da30f9f-6115-420a-908b-146e9b0e6dea` は、それぞれ別のfresh contextで改訂要件・実差分・現行source・上記4ログ／終了コードを確認し、いずれも修正要求なし、`Merge verdict: OK with notes` でした。read-only reviewerにはgit実行toolがないため、Rootが取得した5ファイルの最新diff・HEAD source・最新Issue／追補のread可能なartifactも渡しています。Rootは両報告を直接読み、残す保証・削減範囲・実行証拠と照合して採用しました。accepted findings／不採用のcode findingsはいずれも0件で、追加のコード修正はありません。notesは承認済みの交差保証削減、歴史的原因未確定、#57・実環境検証の未実施を指し、#56の未解決修正要求ではありません。この追記後の差分でCode Review gateを取得し、結果はRootの最終報告で示します。#57を含む全体完了やmerge許可としては扱いません。

#### #57 将来のテスト作成・Review基準（文書変更・静的確認、Root最終検証前）

正式Issue [#57](https://github.com/minorunakamura/backlog-cli/issues/57) の[優先追補](https://github.com/minorunakamura/backlog-cli/issues/57#issuecomment-6545857516)を取得して読み、正式Plan `docs/plans/test-infrastructure-performance.md` の全文と「#57相当: 将来の作成基準と最終検証」「動作確認方法」「制約」を確認しました。PlanのSHA-256は `a6f2a2ac73a2156f9b25d1fb85db00ca3e3d2276585eca0ff331993669450491` と一致しています。開始HEADは `1fc87581dccfe7eb830cc989fdd2a731148b9af6`、開始treeはclean、AGENTS.mdは存在しませんでした。#56の成果はこのHEADと直前のRoot独立検証記録にあり、過去の成功を今回の最終差分の実行証拠へ流用しません。

今回の編集は次の3点だけです。

- `CONTRIBUTING.md` の「テスト設計とReviewの基準」へ、ユーザー保証→既存assert→公開境界、共通matrixと機能固有代表、削る交差保証、受信／headers／描画／editor復帰／resizeの区別、有限否定観測窓、総HTTP件数の契約、新たな待機・起動の説明、focusedと正式チェックの区別を集約しました。旧group wall40秒以下の性能判定だけを改訂方針に整合し、各CLIの既存時間assertは変更していません。新しい振る舞いの失敗テストと、削減だけの変更に不要なREDを区別しています。
- 新規の短い `AGENTS.md` はCONTRIBUTINGの基準・品質確認へのリンクだけで、規則を二重管理しません。自動強制frameworkは追加していません。
- 本項を末尾に追記しました。既存のrelease再現手順・fixtures、旧失敗・旧判断、他Issueの受入条件は変更していません。

**Workerの確認は文書とsourceの静的確認だけです。テスト／build／正式チェックは実行していません。** 指定6関数を直接読み、`tests/common/mod.rs` の実CLI／HTTPS harnessも確認しました。

| 種類 | 実在source・境界の根拠 |
|---|---|
| 純粋な変換・表示 | `tests/body.rs::markdown_headings_and_nested_emphasis_keep_lines_and_unicode` の公開body変換と原文assert、`tests/tui_shell.rs::shell_shows_all_five_panels_only_at_the_full_layout_boundary` のShell→TestBackendとlayout assert |
| 実CLI・HTTPS | `tests/cli_help_version.rs::bl_cli_help_needs_neither_configuration_nor_a_terminal` の空設定・非TTY子processとstdout／stderr／exit、`tests/issue_view.rs::json_view_preserves_original_fields_and_builds_url_without_project_get` の実CLI／証明書検証付きHTTPS・JSON原文／URL・認証header・対象GETだけのassert |
| PTY固有 | `src/tui.rs::tests::body_editor_roundtrip_and_failures_use_shared_terminal_handoff` の実editor入力専有・原文／他欄／cleanup・中断／復元失敗と、同moduleの `shell_restores_when_sigint_arrives_during_entry_before_the_first_frame` の実early SIGINT・130／復元assert。Cargoの完全一致filterは両方 `tui::tests::` 付き |

静的確認コマンドは `git status --short`、`git rev-parse HEAD`、`shasum -a 256 docs/plans/test-infrastructure-performance.md`、`gh api repos/minorunakamura/backlog-cli/issues/comments/6545857516 --jq .body`、source／workflowのread・grepと、一時的な `python3 - <<'PY'` のassert照合です。Python照合はexit0で、以下を確認しました（Cargoは起動していません）。

- 追加した6 focusedコマンドのtarget・完全一致filter・実在する非ignoredのtest定義の対応。文書に想定各1試験と選択0件を成功にしないことを明記しています。実際の選択件数・成功は未確認です。
- 追加した相対リンクの実在ファイル・見出しと優先追補URL。
- 既存CONTRIBUTINGは、新節の追加と既存2文の整合以外はHEADから不変。品質確認コマンド・CI説明・既存release再現手順を保持しています。
- `.github/workflows/quality.yml` の既存4チェック、macos-15、cargo-audit 0.22.2と文書の整合。release buildをCIで実行すると説明していません。workflow自体は不変で、実CIの成功を確認したものではありません。
- 正式Planのhash一致とstagedファイルなし。

**Rootの最終検証・freshなReview／Code Review gateはこの追記時点で未実施です。** Rootが文書整合を含む最終差分を固定し、Plan指定のfmt／all-targets／Clippy／audit版確認・通常DB監査／release build／diff checkを各1回実行して結果を追記します。監査前後のlockfile hash、環境・終了コード・警告・成功／失敗／既存manual用2 ignored、同じ全体試験1回のwall／suite時間もその結果で記録します。今回のWorkerは重複実行せず、未実行を成功としていません。

実CI・別途release直接操作・実端末／IME・実Backlogは今回未実施です。固定3回・10反復・性能閾値・別release POST／PATCH直接再演を新たに要求せず、正式release buildや既存の実timeout保証・他Issueの必須受入を省略するものではありません。過去のeditor／cutoff・startup内部原因等は未確定のままで、完全同等保証・flake根因解消・任意負荷での安定性を主張しません。src／test／fixture／依存／lockfile／CI／Planは変更せず、stage／commit／stash／push／Issue変更／merge／release／deployは行っていません。


#### #56/#57 最終差分のRoot正式検証（2026-10-31 UTC）

正式Plan `docs/plans/test-infrastructure-performance.md` と[#57優先追補](https://github.com/minorunakamura/backlog-cli/issues/57#issuecomment-6545857516)に従う、**Root自身の正式コマンド各1回と直接文書確認の結果**です。出典は `/tmp/backlog-57-root.OBBMv5/verification-summary.md` と同directoryの実ログ・終了コード。本追記のWorkerはそれらとsourceを読むだけで、テスト／build／計測／正式チェックを新実行していません。

対象HEADは `1fc87581dccfe7eb830cc989fdd2a731148b9af6`（#56反映済み）。#57開始treeはcleanで、検証対象はその後のCONTRIBUTINGの基準追加・2文整合、新規AGENTS、直前の受入記録追記というdocs-only差分です（`validation.diff`、保存した `AGENTS.md`、`validation-tree-before.log`）。src／test／fixture／依存／lockfile／CI／Planは不変。本項も末尾追記だけで既存変更・旧記録を保持しています。

環境（`environment.log`）: macOS 26.7.1（25G241）／Darwin arm64、logical CPU 10・memory 32 GiB、rustc／cargo 1.99.1。親Python 3.14.9／OpenSSL 3.6.3、PTY editor用 `/usr/bin/python3` 3.9.6／LibreSSL 2.8.3、openssl CLI 4.0.3。`RUST_TEST_THREADS` は未設定です。

| Rootの正式コマンド（各1回） | exit・結果 | 実ログ／終了コード |
|---|---|---|
| `cargo fmt --all -- --check` | 0、出力なし | `fmt.log`／`fmt.exit` |
| `/usr/bin/time -l cargo test --locked --all-targets` | 0、24 targets、**347成功・0失敗・既存manual用2 ignored**。新規skip／ignoreなし | `all-targets.log`／`all-targets.exit` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし、dev build 2.36秒 | `clippy.log`／`clippy.exit` |
| `cargo audit --version` | 0、literal `cargo-audit-audit 0.22.2`、版は0.22.2 | `audit-version.log`／`audit-version.exit` |
| `cargo audit --file Cargo.lock` | 0、通常のRustSec DB fetch・crates.io index更新、1395 advisories／230 dependencies、警告・脆弱性報告なし | `audit.log`／`audit.exit` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0、release build 7.42秒、警告なし | `release-build.log`／`release-build.exit` |
| `git diff --check` | 0（本結果追記前の検証差分） | `diff-check.log`／`diff-check.exit` |

all-targetsの同じ1回から、Cargo build **9.63秒**、wall **161.69秒**、報告されたsuite時間合計 **116.62秒**、request_timeout suite **30.16秒**を記録します。tui_processは114成功／54.73秒、libは28成功・2 ignored／6.48秒。timeoutの実6ケース・各CLIの29秒以上40秒未満という既存assertを含みます。これらは参考値で、異なる旧workloadとの厳密な性能比較・中央値・flake不存在の証明ではありません。改訂で撤廃した150秒／group40秒の性能閾値、full3回・focused10反復を未達条件にはしません。

**Root wrapperの停止も記録:** 独自shell guardが `cargo audit --version` のliteralを `cargo-audit 0.22.2` と仮定し、実出力 `cargo-audit-audit 0.22.2` を想定外としてaudit／build前にwrapper exit2で停止しました。正式version command自体はexit0・版0.22.2であり、原因はRoot guardの文字列仮定で、製品や正式チェックの失敗ではありません。確認後に未実行のaudit／buildへ進み、正式コマンドの重複・無変更test retry・source／config修正はありません。追加のCI形式 `cargo-audit --version` はexact `cargo-audit 0.22.2`、exit0（`ci-audit-version.log`／`.exit`）で、既存CIのexact版guardとも整合しました。

監査前後のCargo.lock SHA-256はともに `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16`（`lock-before-audit.sha256`／`lock-after-audit.sha256`）。PlanのSHA-256も `a6f2a2ac73a2156f9b25d1fb85db00ca3e3d2276585eca0ff331993669450491` で不変です。Rust／Cargo警告なし。all-targetsにはTLS trust拒否経路の既存Python server例外見出しが**5件**あり、うち1件は `ssl.SSLError: TLSV1_ALERT_UNKNOWN_CA` を含みます。#55／#56にも記録された診断で、対応試験は全て成功し、抑制・fixture変更はしていません。

**Rootの直接文書確認（正式チェックとは別工程）:** CONTRIBUTINGの6コマンドはtargetと `--exact` を保ち、`--nocapture` を `--list` に替えて各exit0・選択1件を確認し、その同名試験の `... ok` を今回のall-targetsに照合しました（`direct-filter-1..6.log`、`direct-documentation.log`／`.exit`、exit0）。試験本体の追加実行ではありません。

| 境界 | 各1件選択し、今回all-targetsでも成功したfilter |
|---|---|
| 純粋な変換・表示 | `markdown_headings_and_nested_emphasis_keep_lines_and_unicode`、`shell_shows_all_five_panels_only_at_the_full_layout_boundary` |
| CLI引数・JSON／HTTPS | `bl_cli_help_needs_neither_configuration_nor_a_terminal`、`json_view_preserves_original_fields_and_builds_url_without_project_get` |
| PTY editor・signal復元 | `tui::tests::body_editor_roundtrip_and_failures_use_shared_terminal_handoff`、`tui::tests::shell_restores_when_sigint_arrives_during_entry_before_the_first_frame` |

RootはA〜Fと共有editor／端末／timeoutも直接読み、A/D/Eのnonzero代表、Bの旧成功・ordinal2／3受信gate・対象GET4回・project3 rule＋4候補／project2候補なし・固定単一PATCH、Cの旧200／500・対象GET受信gateと各1回、Fの疎object／summary=false／milestone.name=nullとUpdated・Do not resend・旧Detail・単一PATCH・後続GETなしを改訂Planへ照合しました。遅延・deadlineと維持対象のassertは不変です。

#56 focusedの源泉は `/tmp/backlog-56-resume-root.czOzeP/` の `focused`／`fixture`／`editor`／`shared-terminal` 各 `.log`／`.exit`（全exit0、6成功／8.82秒・3成功／1.31秒・5成功／0.14秒・1成功［内部13経路］／6.38秒）。`focused-source-correspondence.log` はそのfocused／Review対象の3 test／fixture差分と現HEADの一致、#57での無変更を確認しています。これは変更していないsourceへの#56実行証拠の対応確認で、#57で新実行したfocusedとは記しません。今回のall-targets成功は別の新しい実行証拠です。

Rootの直接assertはAGENTSから単一のCONTRIBUTING基準への導線、相対リンク／anchor、旧受入記録prefix、対象外source・Plan／lock不変も確認しました。既存 `.github/workflows/quality.yml` のmacos-15、fmt／all-targets／Clippy／通常auditとcargo-audit 0.22.2は文書と整合し、workflow不変。release buildはローカル確認で、CI stepとは説明していません。

**残件・限界:** 今回差分のfreshな独立2 ReviewとCode Review gateは本追記時点でこれからであり、通過・Issue完了・merge可能とは記しません。実CI・別途release直接操作・実端末／IME・実Backlogは今回未実施。代表化による交差検出範囲の縮小は残り、完全同等保証・flake根因解消とはしません。過去のeditor／cutoff／startup内部原因は未確定、旧失敗・旧Issue #22の履歴／状態・他機能の受入条件は不変です。stage／commit／stash／push／Issue更新／merge／release／deployは行っていません。

## Issue #23: TUIコメント本文更新

### Workerのローカル検証と必須実受入の境界（2026-10-31 UTC）

開始HEAD `0b59513`・clean treeからの未commit差分です。Issue #23の最新本文／コメント、Approved Planの通信・状態・editor／TDD／受入契約と基本設計のコメント本文更新に従い、Commentのeから最新コメント／myself確認、共通editor、本文専用modal、確認Enter非送信と独立s、content-onlyの一回PATCH、同ID応答によるページ内更新を実装しました。履歴・課題項目・Issue Listを変更せず、成功確認GET・最新移動・一覧自動再取得・競合検出・永続化は追加していません。

- TDDの最初のRED: `cargo test --locked --test tui_process bl_comment_edit_keeps_selected_id_and_page_sends_raw_content_once_without_refresh -- --exact --nocapture` はeditor未到達でexit101（`/tmp/issue23-red.log`／`.exit`）。最小実装後、追加した履歴assertが1行のComment本文viewport外で失敗しました（`/tmp/issue23-green.log`、exit101）。スクロールして履歴を観測するよう試験を修正し、同一filterは1成功・exit0（`/tmp/issue23-green2.log`／`.exit`）。無変更retry・timeout延長はしていません。
- 追加のfocused初回は2成功・3失敗（`/tmp/issue23-edit-tests.log`、exit101）。試験側の誤りは、130経路に正常終了専用scripted wrapperを使ったこと、editorエラー文言の不一致、403を既存API契約の正常なerrors objectなしでnot_updatedと期待したことでした。wrapper／文言／拒否fixtureを修正し、5関数（8起動）すべて成功・exit0（`/tmp/issue23-edit-tests2.log`／`.exit`）。製品の確度・API拒否条件は変更していません。
- `cargo test --locked --lib shell_comment_edit_requires_a_changed_nonempty_body_and_guards_discard_and_tiny_confirmation` はShell→TestBackendの1試験成功（`/tmp/issue23-ui.log`、exit0）。空／同値／破棄／極小での承認・送信不可と安全な対象表示を確認します。実signal／editor／端末復元の代替ではありません。
- 既存共通 `cargo test --locked --test service --test comment_update --test tui_shell --test editor` は全成功（`/tmp/issue23-shared.log`、exit0）。共有editorのPTY復元filterは1成功（`/tmp/issue23-editor.log`、exit0）。これらはfocused時点の記録で、後続の最終正式チェックとは区別します。
- 新規実processは固有lifecycleの代表8起動に限定しました。0.15秒のclient確度観測と1.3秒の旧応答後の有限観測窓を残し、受信gate・画面marker・termiosを区別しています。全失敗matrix／editor全失敗と新画面の交差網羅は主張しません。

正式チェックとreleaseの直接ローカル実測は下の追記で記録します。再現手順はCONTRIBUTINGの「TUIコメント本文更新 (#23): releaseの直接ローカル検証」、fixtureは `tests/fixtures/tui_comment_edit{,_steps}.json` です。秘密はfake keyだけで、生secret／OSCの画面出力と原文editor／PATCHを分離します。

### 必須の実Backlog／実端末受入: pending（Workerは未実施）

Workerは実APIへ接続せず、credentials探索・既存認証設定の参照も行っていません。localhost HTTPS＋PTYは必須の実Backlog受入を代替しません。実IME・実端末の字幅／目視／editor復帰も未実施です。

Rootと利用者の別工程で、利用者自身の認証済み端末にて以下を実施してください（Issue完了承認はRoot）。

1. profileのoriginが `https://lazybl.backlog.com/` であることを確認し、`BLCLI-2` に残る自分の「#22 本文のみ検証」の対象コメントID／所有者と原本文を操作前に確認する。ID未確定のまま送信しない。
2. そのコメントを表示してeでeditorを開き、「#23 本文更新検証」に変更する。確認のprofile／origin／課題／コメントID／本文・課題項目変更なしを確認する。Enterだけで送信されず、別のsで一度送る。
3. 正常な更新結果とBacklog側の本文、TUIが同じコメントを表示し続けること、課題項目に変更がないことを確認する。
4. 同じ許可済みコメントを同経路で元の「#22 本文のみ検証」へ復元し、復元後の本文／対象IDを確認する。
5. 使用profile（key不要）・対象コメントID・build hash・端末／OS・実施結果と復元結果をRootへ報告する。機密本文／API keyは記録しない。

この許可は上記本文更新と復元だけです。課題項目変更・他コメントの書換え・削除・追加データ準備は行いません。未実施／失敗ならIssue #23は未完了のまま、Rootレビューと利用者受入後に最終判断します。

### Workerの最終正式チェック／release直接実測（2026-10-31 UTC）

環境: Darwin 25.6.0 arm64、rustc／cargo 1.99.1、cargo-audit 0.22.2（`cargo audit --version` のliteralは `cargo-audit-audit 0.22.2`）。依存・Cargo.lock・config・CI・Plan・基本設計は変更していません。最終ソースに対するコマンドと各exitは `/tmp/backlog-issue23-worker/` に保存しています。

**初回正式チェックの失敗も保持:** fmt／all-targetsはexit0でしたが、Clippyが新規serviceのValue ID→string比較2箇所を `cmp_owned` で拒否しexit101で停止しました（`initial-{fmt,all-targets,clippy}.{log,exit}`）。JSONの文字列との比較へ置き換えず、既存正整数IDをNumberから表示文字列へ写す比較に最小修正し、以下を取り直しました。無変更retry・timeout延長・警告抑制はしていません。初回停止時点でaudit／releaseは未実行です。

| コマンド | exit・実測 | ログ（上記directory配下） |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | `fmt.log`／`.exit` |
| `/usr/bin/time -l cargo test --locked --all-targets` | 0、24 targets、353成功・0失敗・既存manual用2 ignored | `all-targets.log`／`.exit` |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、警告なし | `clippy.log`／`.exit` |
| `cargo audit --version` | 0、0.22.2 | `audit-version.log`／`.exit` |
| `cargo audit --file Cargo.lock` | 0、通常DB更新、1395 advisories／230 dependencies、脆弱性・警告報告なし | `audit.log`／`.exit` |
| `cmp lock-before.sha256 lock-after.sha256`（上記directory内） | 0、監査前後lock hash一致 | `lock-unchanged.log`／`.exit` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0、7.29秒、警告なし | `release-build.log`／`.exit` |
| `git diff --check` | 0 | `diff-check.log`／`.exit` |
| CONTRIBUTINGの#23 Pythonブロックをそのまま抽出し `python3 /tmp/backlog-issue23-worker/direct.py` | 0、release blをlocalhost HTTPS＋PTYで直接操作し下記PASS | `direct.py`／`direct.log`／`direct.exit` |

最終all-targetsの同じ1回の参考値はwall183.19秒、suite合計118.30秒、tui_process119成功／55.85秒、lib29成功＋既存2 ignored／6.42秒、request_timeout1成功（既存実30秒6ケース）／30.16秒です。性能閾値・flake不存在の証拠ではありません。Rust警告なし。既存TLS trust拒否試験由来のPython server例外見出し5件はログに残り、全該当試験は成功しています。抑制していません。

監査前後のCargo.lock SHA-256はともに `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16`（`lock-before.sha256`／`lock-after.sha256`）。release SHA-256（`binaries.sha256`）:

- bl: `1a7b2d7bdab1e281b2aa9f2304e47f3c3af8a73496c14b80a78c4939623a68b8`
- bl-cli: `4695066f2ec5989b118724db3b3047540736242644d513fd0345a2349934749d`

直接ローカル実測はcargo testと別工程です。**13 GET＋1 content-only PATCH**、確認EnterではPATCHゼロ（HTTP増加なし）、独立s後のPATCH一回・原文一致・PATCH後GETゼロ、非最新のコメントID 1を更新してID 2へp／ID 1へnで戻れるページ保持、旧List・既存履歴・同IDの新本文表示、safe描画（fake secret／OSC漏えいなし）、editorへ最新原文・0600・cooked入力・tempfile削除、exit0・termios差分ゼロ・復元trueをassertしました。出力は `PASS: release bl; latest raw editor; confirm Enter=0 PATCH; explicit s=1 content PATCH; same ID/page/history/old List; 13 GET+1 PATCH; no followup; exit=0; termios restored`。raw画面結果とfake-key HTTP記録は `/tmp/issue23-direct-result.json`／`/tmp/issue23-direct-requests.json` にあります。

正式検証後の変更はこの結果記録の追記だけです。Rootの独立Review gate・最終承認、上記実Backlog更新／復元と実端末目視はpendingです。実API／credentials探索・CI実行・stage／commit／stash／push／Issue更新／merge／release／deployは行っていません。

### #23 Root採用指摘の限定補正（2026-10-31 UTC、Root最終検証前）

HEAD `0b59513` の既存未commit実装・未追跡3ファイルを保持し、採用されたP1/P2だけを補正しました。関連するCLI／TUI callerを検索し、共有Serviceの取得時・PATCH応答時の両ID比較で、検証済み入力の先頭ゼロを除いて比較します。対象ID一致検証・巨大整数／正整数契約は維持し、整数型変換・helper・PTY試験は追加していません。README／CONTRIBUTINGの導入文も「本文編集実装済み・必須実受入pending」へ整合しました。過去#22等の履歴は変更していません。

追加試験は実CLI＋証明書検証付きlocalhost HTTPS境界の `tests/comment_update.rs::leading_zero_comment_id_updates_the_same_numeric_id_with_one_content_patch` 1件です。`0007`入力→最新GET→myself→contentだけのPATCH一回→数値ID7の正常応答を照合します。総3要求の順序・method/path/formを固定し、retry／事後GETなしも確認します。先頭ゼロ入力とAPI数値IDの結合回帰には実CLIが必要で、実IME／目視の保証ではありません。

| コマンド | exit／実測 | `/tmp/backlog-issue23-correction/` のログ |
|---|---|---|
| `cargo test --locked --test comment_update leading_zero_comment_id_updates_the_same_numeric_id_with_one_content_patch -- --exact --nocapture`（補正前→後） | **101 RED**（GET後の`invalid_response/not_updated`）→**0 GREEN**（1成功） | `red.log`／`red.exit`、`green.log`／`green.exit` |
| `cargo test --locked --test comment_update --test service` | 0、11＋20成功、0失敗／ignored。巨大IDの既存試験も成功 | `focused-shared.log`／`.exit` |
| `cargo test --locked --test tui_process bl_comment_edit_ -- --nocapture` | 0、5成功（別ID応答拒否を含む）、0失敗／ignored | `focused-tui.log`／`.exit` |
| `cargo fmt --all -- --check` | 0 | `fmt.log`／`.exit` |
| `cargo clippy --locked --all-targets -- -D warnings` | **tool timeout、Clippy終了コード未取得、原因未確定**。ログは`Checking backlog-cli`のみ。実行枠240秒でwrapper停止、確認時にcargo／clippy／rustc processなし | `clippy.log`（`.exit`未生成）、`validation-status.txt` |

focused-sharedには既存HTTPS fixtureの`Exception occurred during processing of request`見出し1件が残りますが、全31件成功で、診断を抑制していません。Clippyの停止をexit0や製品診断エラーへ丸めず、原因解消済みとも扱いません。Rootへの停止相談を受け、無変更再実行・timeout延長はせず、後続release buildも未実行で停止しました。修正後の正式all-targets／Clippy／audit／release build／release直接検証はRootの別工程です。補正前の成功やWorker自己申告を修正後の正式検証・完了承認へ流用しません。

必須実受入は引き続き**pending**です。利用者自身による`https://lazybl.backlog.com/`の`BLCLI-2`・自分の許可済み「#22 本文のみ検証」→「#23 本文更新検証」→元本文復元と、同ID表示・実端末目視の結果は未取得です。Workerは実API／credentials探索を行わず、Plan／基本設計／依存／他実装を変更していません。Rootのfresh Review・正式検証・利用者の実受入前にIssue #23完了とは扱いません。stage／commit／stash／push／Issue更新／mergeは行っていません。

### #23 Rootの独立検証・Reviewと修正後検証の中断（2026-10-31 UTC）

補正前の差分についてRootが独立実行した正式fmt／all-targets（353成功・0失敗・既存手動用2 ignored）／Clippy／audit（0.22.2、1395 advisory／230依存、警告なし・監査前後lock不変）／release build／diff checkとrelease直接検証は全exit0でした。根拠は `/tmp/backlog-issue23-root/` の各 `.log`／`.exit`。直接検証は新規抽出した `direct.py` による13 GET＋1 content-only PATCH、確認Enter非送信、PATCH後GETゼロ、同ID・ページ・旧List・履歴保持、secret／OSC非描画、exit0・termios差分なしです。これは**先頭ゼロID補正前**の結果であり、補正後の完了証拠に流用しません。

その後、別々のfresh built-in reviewerによるCorrectness Review（`f1aa0cde-27c5-4ad2-b96f-b30fd1b714b8`）とPonytail Review（`237ba611-e632-41da-b24c-e2d46701a7ac`）が完了しました。Rootが新規保存した差分・未追跡一覧・最新Issue本文／コメント、現在コード、正式Plan／設計、上記Root実測を確認しています。両者のP1「共有ServiceのID文字列一致が既存CLIの先頭ゼロ入力を拒否する」とCorrectnessのP2「導入文の未実装記載」を採用し、上記限定補正を行いました。不採用の指摘はなく、その他の具体的な不具合・不要な抽象化は報告されていません。必須実受入pendingは別gateです。

補正後はRootが実際の2箇所の先頭ゼロ除去・追加CLI回帰試験・導入文整合を確認し、予定していた独立正式チェックを開始しました。ログは `/tmp/backlog-issue23-root-final/`。

| コマンド | Rootの実測 |
|---|---|
| `cargo clippy --locked --all-targets -- -D warnings` | exit0、警告なし（`clippy.log`／`.exit`）。Workerのtimeout原因を解消した証明ではない |
| `cargo fmt --all -- --check` | exit0（`fmt.log`／`.exit`） |
| `cargo test --locked --all-targets` | **tool timeout（実行枠300秒）、cargo終了コード未取得、完走未確認**（`all-targets.log`、`.exit`未生成）。comment_updateの新規先頭ゼロ試験を含む9件の成功まで記録され、同suiteの残り2件の完了・suite結果はない。確認時にcargo／rustc／製品・fixtureの関連processなし。原因未確定 |
| 補正後のaudit／release build／release直接検証 | 上記停止により未実施。補正前binary hashを補正後buildとして提示しない |

全対象検証が完了していないため、この実行は **FAILED（検証実行未完了）** として停止します。無変更retry・timeout延長・skip／ignore・試験期待緩和は行っていません。検証中断の原因切り分け後、修正後差分に対する正式全対象チェック・release直接検証を完了し、必要なReviewと許可済み実端末／実Backlog受入、その後の最終Code Review gateへ進む必要があります。最終gateは未到達で未実行、merge可能とは判定しません。部分実装・既存差分は保持し、commit／stash／破棄／push／Issue状態変更／実Backlog操作は行っていません。

### #23 再開：Sleepとの時間的一致と、環境を整えたRoot独立検証（2026-10-31 UTC）

利用者の再開依頼後、最新Issueの本文／コメントと保持した差分を再確認しました。macOSの`pmset -g log`で、WorkerのClippy停止中に2026-11-01 03:38:02 JSTから947秒のSleep、Rootの全対象中断中に04:00:34 JSTから1011秒のSleepが記録され、停止したログ更新時刻と重なることを確認しました（`/tmp/backlog-issue23-resume/power-window.log`）。過去processのstackは取得していないため、Sleepを唯一の原因と断定したり、コード上のhangが完全にないと保証したりはしません。

製品・テスト・timeout値・OSの永続設定を変更せず、Rootの一連の検証コマンドだけをmacOS標準の`/usr/bin/caffeinate -is`で囲みました。tool実行枠300秒は維持し、今回の19:37:14–19:39:52 UTC（04:37:14–04:39:52 JST）の電源ログにはSleep／Wake／DarkWakeなし（`power-validation-window.log`は0行）。チェックは同じ一回で完走しました。旧失敗記録は保持しています。

ログ・各終了コード・開始／終了時刻は `/tmp/backlog-issue23-resume/` の各 `.log`／`.exit`／`.started`／`.finished` です。

| Rootのコマンド | 結果 |
|---|---|
| `cargo fmt --all -- --check` | exit0 |
| `cargo test --locked --all-targets` | exit0、24 suite、354成功・0失敗・既存実端末用2 ignored。先頭ゼロIDと、前回未完走の中断系2件も成功。wall約147秒 |
| `cargo clippy --locked --all-targets -- -D warnings` | exit0、警告なし |
| `cargo audit --version`／`cargo audit --file Cargo.lock` | 両exit0、0.22.2、通常DB更新、1395 advisory／230依存、脆弱性・警告報告なし |
| 監査前後のlock hash比較 | exit0、両方`1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | exit0 |
| `git diff --check` | exit0 |
| CONTRIBUTING #23から新規抽出した`python3 /tmp/backlog-issue23-resume/direct.py` | exit0。修正後release製品で13 GET＋1 content-only PATCH、確認Enter非送信、独立s、一回送信、同ID・ページ・旧List・履歴保持、PATCH後GETゼロ、原文editor／PATCH、安全表示、tempfile削除、exit0・termios差分なしを直接確認 |

全対象ログの既存TLS拒否等に伴うPython server例外見出し5件は保持し、全該当試験成功と区別しました。Rust警告なし。今回releaseのSHA-256はbl `30c2b4f5bc93c9ef7c684650096768247a59e97d7f5d7365993a571d155e53a0`、bl-cli `be2b535b6f282923c467fd960b59d7bfc7714b057fcb9bf16aa3af54bd471e5d`。raw観測／fake-key要求は同directoryの`direct-result.json`／`direct-requests.json`です。

Rootは既存config解決規則に従い、`default` profileのspaceUrlが`https://lazybl.backlog.com/`のoriginと一致することだけを確認しました。API keyを出力・保存せず、実APIへ接続していません。共有CLI補正の限定fresh再Review、今回hashでの利用者による許可済み実端末／実Backlog更新・同ID表示・復元、最終Code Review gateはこの追記時点でpendingです。検証再開の成功を実受入・Issue完了・merge許可へ読み替えません。

### #23 補正後の限定fresh Review完了（2026-10-31 UTC）

built-in reviewerのfresh follow-up（`50360d13-05f1-4f53-8f79-4f86975cb841`）で、先頭ゼロIDの両比較・正整数／巨大ID契約・新CLI回帰試験・導入文修正と、補正後のRoot正式チェック／release直接検証の一次artifactを照合しました。前回P1／P2はいずれもresolved、新しい具体的P1／P2なし、技術面の限定判定は`OK with notes`です。Rootも補正範囲と結果を確認し、追加修正は不要と判断しました。これはReviewerによる再実行・実API操作の記録ではありません。

今回hashに対する利用者の実端末／実Backlog受入結果と最終Code Review gateは引き続きpendingです。Rootが提示した更新・同ID表示・原文復元の手順への報告待ちであり、Issue完了・merge可能とはまだ判定しません。

### #23 今回releaseの利用者による実端末・実Backlog受入完了

提示した許可済み手順に対し、利用者から次の明示報告を得ました。

- build hashは一致。対象blは`30c2b4f5bc93c9ef7c684650096768247a59e97d7f5d7365993a571d155e53a0`で、Rootも報告後にrelease hash不変を確認しました。
- 接続先・profile・対象は許可済みの`https://lazybl.backlog.com/`、`default`、`BLCLI-2`の自分の検証用コメント。対象コメントIDは **843156294**。
- `#22 本文のみ検証`から`#23 本文更新検証`への更新、Webへの本文反映、同じコメントを表示し続けること：**OK**。
- 同じコメントの元本文`#22 本文のみ検証`への復元：**OK**。
- 日本語入力・editor復帰・resize・終了後shell：**OK**。提示済みの100×24／40×12／極小からの復帰を含む手順への報告として受け取っています。
- 端末／OSは「以前と同じ」と明示回答。過去の確認済み利用者環境Herdr 0.8.0／Ghostty 1.3.1、macOS 26.7.1／Apple Siliconを引き継ぎます。TERM／NO_COLOR／IME／keyboard設定の個別値は今回再取得していないため補完しません。

これは利用者の認証済み実端末での操作・目視報告であり、Root／Worker／Reviewer自身による実API・IMEの観測とは区別します。単一PATCHの通信回数、確認Enter非送信、履歴・課題項目非変更、所有者／履歴拒否、空／同値／取消／editor失敗、応答逆転・中断確度は既存の一次コード・公開境界試験・Rootのlocalhost HTTPS／PTY実測を根拠にします。実サービスへ障害や負荷を与えていません。

Rootは最新Issue本文／受入改訂コメントを再取得し、4つの元完了条件と追加実Backlog受入条件を照合しました。採用したP1／P2は解決済みで、今回追加した受入記録以外のtracked差分とuntrackedのsource／fixtures3点が限定Review対象から不変であることを確認しています。最終Code Review gateはこの記録・readback後に呼び、skippedまたは現在差分への明示Approveを確認するまでREADY_FOR_MERGEとは判定しません。実CI・stage／commit／stash／push／Issue状態変更／merge／release公開／deployは行っていません。

### #23 PR #62 CI失敗のテスト同期限定補正（2026-10-31 UTC）

開始HEAD `e7b35e0fe391ce19ac4e0dfbaee283868cada8ec`、branch `feat/tui-comment-update`、clean。Root取得済みのIssue本文／コメント、承認済みPlan／基本設計に変更はありません。CI run `41237380074`／job `124643885658`（同HEAD）のquality Testはexit101、`tui_process`は116成功・3失敗でした。一次ログは `/tmp/backlog-pr62-ci.gIJ1NoSs/failed.log`。以下はWorkerによるテストの操作可能状態への同期補正であり、製品の入力契約を変更するものではありません。

| 既存試験（以下の略号） | 実CIの失敗 | Rootの制御RED → Workerの同条件GREEN／通常focused |
|---|---|---|
| A: `bl_comment_add_editor_failures_resize_tiny_and_discard_keep_body_and_field_draft` | 8秒deadline、step_index2、12 GETまで、editor未到達 | exit101 → exit0（1成功、3.55秒）／exit0（1成功、2.83秒） |
| B: `bl_comment_add_refresh_is_atomic_preserves_old_selection_scroll_and_certainty_on_failure` | 8秒deadline、step_index4、12 GETまで、editor未到達 | exit101 → exit0（1成功、27.83秒）／exit0（1成功、20.56秒）。試験全体はPOST/PATCH×5種であり、個々のPTY deadlineは8秒のまま |
| C: `search_dialog_edits_keyword_without_searching_and_clear_stays_clear_on_reopen_and_reload` | 8秒deadline、step_index5、count/list各2回まで | exit101 → exit0（1成功、2.77秒）／exit0（1成功、2.02秒） |

**原因と最小差分:** A/Bは`Add comment` titleだけを待って入力していましたが、このtitleはLoading／LoadFailed／Form共通で、Loading中はEnter／Tab等を意図どおり無視します。Formにだけ描画される`Body edit:`を待つよう各1箇所を変更しました。Cは検索適用後の`> Issue List`がApplying中の背面にも残るため、適用完了を示す`K:qhljk S:1 A:0 G:0`とクリア適用済みの`K:Any S:0 A:0 G:0`へ各1箇所を変更しました。描画・HTTP到着・適用完了を区別し、HTTP件数だけの待機にはしていません。原文／所有者／課題項目／確度／scroll／選択／refresh／通信回数の既存assertは全て維持し、timeout延長・retry・skip／ignore・期待値緩和はありません。

**公開境界とRED:** 既存の実`bl`＋証明書検証付きlocalhost HTTPS＋PTYの3試験をそのまま使いました。Rootがrepo外の一時`python3` wrapper（`/tmp/backlog-pr62-ci.gIJ1NoSs/python3`、Python 3.14.9）から既存HTTPS server起動時の一時`response.json`だけを変更し、A/Bは`/api/v2/resolutions`のheadersを0.7秒、Cは`/api/v2/issues/count`の2回目だけ0.7秒遅延させています。製品TLS／通信・repoのfixture／harnessは不変です。RootのREDログ／exitは同directoryの`repro-comment-load-{AまたはBの完全名}.{log,exit}`と`repro-search-apply-{Cの完全名}.{log,exit}`。Workerが原ログとexit101・選択各1件を確認しました。A/BのREDはCIと同じstep_index2／4・12 GETでした。Cの制御REDは`assert_search_fields_blank`で`open`が残る失敗（Applying／旧qhljk表示）であり、CIと具体的な失敗位置は異なります。同じ早期入力の危険を確認したもので、CIの全タイミングを再現したとは扱いません。

**GREEN・通常focused手順:** リポジトリ直下で以下を実行しました。各exactは選択1件・成功1件、選択0件を成功扱いしていません。wrapperは調査用一時ファイルであり、repoへ追加していません。

```sh
/usr/bin/caffeinate -is env PATH="/tmp/backlog-pr62-ci.gIJ1NoSs:$PATH" CI_REPRO_MODE=comment-load cargo test --locked --test tui_process bl_comment_add_editor_failures_resize_tiny_and_discard_keep_body_and_field_draft -- --exact --nocapture
/usr/bin/caffeinate -is env PATH="/tmp/backlog-pr62-ci.gIJ1NoSs:$PATH" CI_REPRO_MODE=comment-load cargo test --locked --test tui_process bl_comment_add_refresh_is_atomic_preserves_old_selection_scroll_and_certainty_on_failure -- --exact --nocapture
/usr/bin/caffeinate -is env PATH="/tmp/backlog-pr62-ci.gIJ1NoSs:$PATH" CI_REPRO_MODE=search-apply cargo test --locked --test tui_process search_dialog_edits_keyword_without_searching_and_clear_stays_clear_on_reopen_and_reload -- --exact --nocapture
/usr/bin/caffeinate -is cargo test --locked --test tui_process bl_comment_add_editor_failures_resize_tiny_and_discard_keep_body_and_field_draft -- --exact --nocapture
/usr/bin/caffeinate -is cargo test --locked --test tui_process bl_comment_add_refresh_is_atomic_preserves_old_selection_scroll_and_certainty_on_failure -- --exact --nocapture
/usr/bin/caffeinate -is cargo test --locked --test tui_process search_dialog_edits_keyword_without_searching_and_clear_stays_clear_on_reopen_and_reload -- --exact --nocapture
/usr/bin/caffeinate -is cargo test --locked --test tui_process bl_comment_add_ -- --nocapture
/usr/bin/caffeinate -is cargo test --locked --test tui_process search_dialog_ -- --nocapture
/usr/bin/caffeinate -is cargo test --locked --test tui_process bl_comment_edit_ -- --nocapture
```

Worker環境はmacOS 26.7.1／arm64／rustc 1.99.1。関連focusedは順に9／20／5成功、全てexit0・失敗／ignoredなし（suite時間20.82／23.92／4.74秒）。各コマンド実行中だけ`caffeinate -is`でSleepを抑止し、tool実行枠300秒を維持しました。Workerのprivateログdirectoryは `/tmp/backlog-pr62-worker.L1pnCyZ6/`、制御GREENは`green-{mode}-{完全試験名}.{log,exit}`、通常は`normal-{完全試験名}.{log,exit}`、関連は`related-{filter}.{log,exit}`です。

文書追記後の`/usr/bin/caffeinate -is cargo fmt --all -- --check`、`/usr/bin/caffeinate -is cargo clippy --locked --all-targets -- -D warnings`、`git diff --check`も全てexit0（`fmt`／`clippy`／`diff`の各`.log`／`.exit`、Clippy警告なし）。`git diff --cached --name-only`は空で、stageされたファイルはありません。

**限界・残gate:** 製品source／依存／fixture／CI／Plan／設計は不変です。今回の実測はローカルのテスト同期補正のみで、修正後のGitHub CIは未実行。全対象／audit／release build／release直接検証と独立Review・最終判断はRootの別工程です。上記の実Backlog受入は同`bl` SHA-256 `30c2b4f5bc93c9ef7c684650096768247a59e97d7f5d7365993a571d155e53a0`に対する既存利用者報告であり、本補正で新たに製品／実APIを実行した記録ではありません。テストと文書だけの差分で製品build入力が変わらない根拠と区別します。Workerはcredentials探索・実API操作・stage／commit／stash／push／Issue更新／mergeを行っていません。

### #23 / PR #62 同期補正後のRoot独立検証（2026-10-31 UTC）

Rootは実際の差分が3試験のwait文字列4箇所と受入記録だけであり、既存assert・操作キー・8秒deadline・製品build入力が不変であることを確認しました。以下はWorkerの成功申告ではなく、Rootの別実行です。実行中だけ`caffeinate -is`を使用し、同じtool枠300秒を維持しました。ログ／exitは `/tmp/backlog-pr62-ci.gIJ1NoSs/root-results/`。

| Rootの確認 | exit・実測 |
|---|---|
| Root制御REDと同じwrapper／mode／exactでAを再現 | 0、1成功、3.75秒。`controlled-{A完全名}.log`／`.exit` |
| 同条件でBを再現 | 0、1成功、27.84秒（10 scenario全体、個々の8秒deadline不変） |
| 同条件でCを再現 | 0、1成功、2.76秒 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test --locked --all-targets` | 0、24 suite、354成功・0失敗・既存実端末用2 ignored。通常条件のA/B/Cとコメント本文更新5関数を含む |
| `cargo clippy --locked --all-targets -- -D warnings` | 0、Rust警告なし |
| `cargo audit --version`／`cargo audit --file Cargo.lock` | 両0、0.22.2、1395 advisory／230依存、通常DB更新・脆弱性／警告報告なし |
| 監査前後lock hash比較 | 0、両方`1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16` |
| `cargo build --locked --release --target aarch64-apple-darwin --bins` | 0、両binaryのhashは下記の受入済みbuildと不変 |
| `python3 /tmp/backlog-issue23-resume/direct.py` | 0、今回もrelease製品をlocalhost HTTPS／PTYで直接操作。13 GET＋1 content-only PATCH、Enter非送信・独立s、同ID／ページ／履歴／旧List保持、PATCH後GETゼロ、原文保持／safe描画・tempfile削除・exit0・termios差分なし |
| `git diff --check` | 0 |

全対象ログの既存TLS拒否等によるserver例外見出し5件は保持し、該当試験成功・Rust警告なしと区別しました。今回buildのblは`30c2b4f5bc93c9ef7c684650096768247a59e97d7f5d7365993a571d155e53a0`、bl-cliは`be2b535b6f282923c467fd960b59d7bfc7714b057fcb9bf16aa3af54bd471e5d`。既存の利用者受入と同一binaryですが、実Backlog更新・復元を今回新しく実施したとは記しません。

CI原ログの3件のdeadlineを製品回帰と断定せず、証拠のある早期入力の競合を制御再現して同期を補正しました。検索の制御REDはCIと具体的な失敗位置が異なる限界を維持します。修正後の実GitHub Actions成功や全負荷でのflake不存在はまだ確認していません。この記録時点で独立fresh 2 Reviewと最終Code Review gate、追加commit／pushは未実施です。

### #23 / PR #62 同期補正の独立2 Review完了

別々のfresh built-in reviewerでCorrectness Review（`6388cec5-a73f-4e55-b98e-4cc62a8b2b33`）とPonytail Review（`aae11d2e-e2b2-4c24-be97-d5d2a2d581ff`、ponytail-review Skill）を完了しました。両者ともRoot保存の実CIログ・補正差分・正式要求・製品の入力／描画経路・Root制御RED／GREENと全チェックの一次artifactを確認し、具体的P0／P1／P2指摘なし、限定補正の技術判定は`OK with notes`です。

Rootも、Body edit:が入力可能なFormだけで表示されること、2つの検索summaryが直前の状態と異なり適用成功後だけ成立すること、既存assert／操作キー／8秒deadline／製品build入力が不変であることを照合しました。採用・不採用に分ける追加指摘はなく、修正は不要と判断しました。記録以外の差分がReview対象から不変であることを確認してから最終Code Review gateを呼びます。Reviewer自身による再実行・実CI成功の報告ではありません。修正後実CIは未確認、追加commit／push・mergeは未実施です。
