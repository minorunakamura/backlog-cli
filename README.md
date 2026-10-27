# backlog-cli

Apple Silicon Mac向けのBacklog TUI `bl` / 非対話CLI `bl-cli`（Rust）。
現在は `bl-cli project list` / `issue-type list` / `priority list` / `resolution list` を利用できます。
その他のCLI操作とTUI画面は未実装です。
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
`project` は `issue-type list` の既定値です。その他は後続の課題操作用で、
`project list` / `priority list` / `resolution list` の取得対象には適用しません。

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

## 一覧コマンド

```sh
bl-cli project list
bl-cli --profile work --json project list --no-input
bl-cli project list --profile work --json
bl-cli issue-type list -p MYAPP
bl-cli issue-type list --project 123 --json
bl-cli priority list --json
bl-cli resolution list
```

`--profile`（省略時 `default`）・`--json`・`--no-input` はsubcommandの前後で指定できます。
TTYでも非対話で、stdin・editor・選択UI・確認待ちは使いません。`--no-input` は動作を変えません。

`issue-type list` のprojectは明示 `--project/-p`、次にcwd設定 `project` を使い、
どちらもなければ入力エラーです。projectキー/数値IDはURLの単一path部品として扱います。
空文字・`.`・`..` はpathを安全に構築できないため入力エラーです。余分なproject取得や所属確認はしません。
`priority list` / `resolution list` には入力flagを設けず、`--project` 等の適用外flagを拒否します。

固定表示はprojectが `ID` / `KEY` / `NAME`、他の3一覧が `ID` / `NAME` のTAB区切りで、
空一覧でも見出しを出します。完了理由のID 0は有効な値として表示します。
JSON成功はstdoutへ改行終端の1文書 `{"items":[...]}`。未知fieldと大整数も保持します。
失敗を空一覧へ置き換えず、JSONモードの失敗はstderrへ次の形で出します（stdoutは空）。

```json
{"error":{"kind":"network","message":"HTTPS request failed or timed out","httpStatus":null,"apiErrors":[]}}
```

終了コードは成功/help/versionが0、引数・project不足・名前解決エラーが2、設定/API/通信/出力エラーが1、
処理可能なCtrl+C中断が130です。help/version自体はJSONではなく通常のテキストです。
外部文字列の危険な端末制御を無害化し、日本語・結合文字・emoji・LF/TABは保持します。
選択profileのAPI keyは値・未知key・配列・数値も伏字化し、key衝突は出力エラーとします。
非TTY/NO_COLOR時を含め、現CLIは色装飾を出しません。

CLIの通信は必要な一覧GETだけです（projects、projects/:projectIdOrKey/issueTypes、priorities、resolutions）。
API keyは `Backlog-API-Key` headerに送ります。
redirect・自動retryは禁止、body読取を含め1 requestは30秒でtimeoutします。
raw responseやURL/headerのdebug dumpは出しません。

## 共通libraryの候補取得・名前解決

CLI/TUIから共通に `service::Service::new(&Config)` を使えます（生成だけでは通信しません）。
`candidates(Candidate)` は元のAPI配列、`myself()` は認証ユーザーの元objectを返します。
`Candidate` は `IssueType` / `Status` / `Assignee` / `Milestone`（各 `project: &str`）と、
space全体の `Priority` / `Resolution` です。milestoneはversions APIを使います。

`resolve(Candidate, Selection)` は `serde_json::Number` のIDを返し、巨大なIDを縮小しません。
`Selection::Name("123")` は数値の名前として完全一致し、`Selection::Id("123")` と区別します。
候補不在・同名の複数一致はinputエラー（exit 2）。IDは正整数、Resolutionだけ0も許可し、
0を未指定/解除と扱いません。直接IDでは候補API・project取得・所属確認を呼びません。
`Assignee` の `Selection::Name("self")` だけは選択profileのmyselfを使い、projectユーザー一覧は取得しません。
候補やmyselfは呼出し時だけ取得し、永続/メモリcacheは持ちません。API失敗を空候補やinputエラーに置き換えません。
返却データは表示用に変更せず、表示側で共通 `safety::json/text` に選択profileのkeyを渡して安全化します。
この公開入口は後続の課題操作でも再利用できますが、今回それらのCLIを追加したものではありません。

開発環境・ローカルHTTPS試験・品質確認は [CONTRIBUTING.md](CONTRIBUTING.md)、
実装方針は [承認済みPlan](docs/plans/backlog-cli-initial.md) を参照してください。
