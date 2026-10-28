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
