# backlog-cli

Apple Silicon Mac向けのBacklog TUI `bl` / 非対話CLI `bl-cli`（Rust）。
現在は `bl-cli project list` を利用できます。その他のBacklog操作とTUI画面は未実装です。
両binaryの `--help` / `-h` と `--version` / `-V` は設定・API接続・TTYなしで利用できます。

```sh
cargo run --locked --bin bl -- --help
cargo run --locked --bin bl-cli -- project list --json
```

## 設定

共通設定は `$XDG_CONFIG_HOME/backlog/config.json`。XDG_CONFIG_HOMEが未設定・空・相対pathなら
`~/.config/backlog/config.json` を使います。必要なfileは利用者が用意してください。
自動生成や壊れた設定の補修はしません。

```json
{
  "editor": ["vim"],
  "profiles": {
    "default": {
      "spaceUrl": "https://example.backlog.com",
      "apiKeyEnv": "BACKLOG_API_KEY"
    },
    "work": {
      "spaceUrl": "https://company.backlog.jp",
      "apiKeyEnv": "BACKLOG_WORK_API_KEY"
    }
  }
}
```

API key本体は `apiKeyEnv` で指定した環境変数にだけ設定します。fileへ保存しません。
指定profileや環境変数が不在・空ならエラーで、他profileや別keyへfallbackしません。
`spaceUrl` はHTTPS originだけを受け付け、userinfo・query・fragment・root以外のpathは拒否します。

editorは文字列配列で、未指定なら `["vim"]`。空配列・空program・NULや実行不能なprogramは
設定エラーです。PATH未設定時の探索先は `/usr/bin:/bin`。VISUAL/EDITORは読みません。
今回のCLIではeditorを起動せず、argvとprogramのfile/実行権限だけを検証します。

cwd直下の `.backlog/settings.json` は省略可能です。親ディレクトリを探索せず、profileも変えません。
後続の課題操作用に次の既定値を読み込めますが、`project list` の取得対象には適用しません。

```json
{
  "project": "MYAPP",
  "issue": {
    "create": {"issueType": "タスク", "priority": "中", "assignee": "self"},
    "list": {"status": ["未対応", "処理中"]}
  }
}
```

設定は未知キー・型不正・壊れたJSON・読取失敗・不正値を拒否します。既定値の文字列は
空白のみを含め空を拒否します（保持値はtrimしません）。statusの未設定・空配列は有効です。
候補の存在や所属確認のためのAPIは呼びません。

## プロジェクト一覧

```sh
bl-cli project list
bl-cli --profile work --json project list --no-input
bl-cli project list --profile work --json
```

`--profile`（省略時 `default`）・`--json`・`--no-input` はsubcommandの前後で指定できます。
TTYでも非対話で、stdin・editor・選択UI・確認待ちは使いません。`--no-input` は動作を変えません。

固定表示は `ID` / `KEY` / `NAME` のTAB区切りで、空一覧でも見出しを出します。
JSON成功はstdoutへ改行終端の1文書 `{"items":[...]}`。未知fieldと大整数も保持します。
失敗を空一覧へ置き換えず、JSONモードの失敗はstderrへ次の形で出します（stdoutは空）。

```json
{"error":{"kind":"network","message":"HTTPS request failed or timed out","httpStatus":null,"apiErrors":[]}}
```

終了コードは成功/help/versionが0、引数エラーが2、設定/API/通信/出力エラーが1、
処理可能なCtrl+C中断が130です。help/version自体はJSONではなく通常のテキストです。
外部文字列の危険な端末制御を無害化し、日本語・結合文字・emoji・LF/TABは保持します。
選択profileのAPI keyは値・未知key・配列・数値も伏字化し、key衝突は出力エラーとします。
非TTY/NO_COLOR時を含め、現CLIは色装飾を出しません。

通信は `GET /api/v2/projects` のみで、API keyは `Backlog-API-Key` headerに送ります。
redirect・自動retryは禁止、body読取を含め1 requestは30秒でtimeoutします。
raw responseやURL/headerのdebug dumpは出しません。

開発環境・ローカルHTTPS試験・品質確認は [CONTRIBUTING.md](CONTRIBUTING.md)、
実装方針は [承認済みPlan](docs/plans/backlog-cli-initial.md) を参照してください。
