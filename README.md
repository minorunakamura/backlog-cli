# backlog-cli

Apple Silicon Mac向けのBacklog TUI `bl` / 非対話CLI `bl-cli`（Rust）。
現在は `bl-cli project list` / `issue-type list` / `priority list` / `resolution list` と
`issue list` / `issue mine` / `issue view` を利用できます。
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
`project` は `issue-type list` と通常の `issue list/mine` の既定値です。
`issue.list.status` は通常検索の状態OR条件です。createの既定値は検索へ流用しません。
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

CLIの通信は操作に必要なGETだけです。検索ではcountとlist、必要なproject解決・候補・self取得を行い、
space取得・所属確認・成功確認の追加GETはしません。
API keyは `Backlog-API-Key` headerに送ります。
redirect・自動retryは禁止、body読取を含め1 requestは30秒でtimeoutします。
raw responseやURL/headerのdebug dumpは出しません。

## 課題検索

```sh
bl-cli issue list "ログイン" -p MYAPP -s 完了
bl-cli issue list -p MYAPP -s 未対応 -s 処理中 -a self -m v1.0
bl-cli issue list -p 123 --status-id 1 --status-id 2 --assignee-id 456 --milestone-id 789 --json
bl-cli issue list -p MYAPP --status all --limit 20 --offset 20
bl-cli issue list "ログイン" -p all --json
bl-cli issue mine -p all --profile work --json
```

通常検索は明示 `--project/-p` をcwdのprojectより優先し、どちらもなければ入力エラーです。
projectキーはproject APIでIDへ解決し、数値IDは直接 `projectId[]` へ送ります。
位置引数keyword、`--status/-s`、`--assignee/-a`、`--milestone/-m` は項目間AND、
反復した同じ項目内はORです。同一値は重複送信しません。名前は完全一致で、不在・複数一致は入力エラー。
名前flagと対応する `--status-id` / `--assignee-id` / `--milestone-id` は排他です。
IDは任意精度の正整数で、直接IDでは候補APIを呼びません。数値の名前も名前として照合します。

明示statusはcwdの検索statusを置き換え、`--status all` は既定の状態条件を解除します。
`all` と他の状態指定は混在できません。`--assignee self` は選択profileの認証ユーザーです。
`issue mine` は同じ検索の担当者self固定版で、担当者flagは一切受け付けません。

`--project all` は全参加project横断です。空白のみでないkeywordまたはselfが必須で、
ローカル検索既定値を一切適用しません。明示status（`all`も含む）・milestone・self以外の担当者は
ID flagも含め拒否します。project一覧や所属確認のための追加通信はしません。

limitは既定20・1〜100、offsetは既定0・0以上。`sort=updated` / `order=desc` 固定で、
同じ検索条件のcountと指定した1ページのlistが両方成功した場合だけ出力します。全件自動取得はしません。
固定表示は `ID` / `KEY` / `TITLE` / `STATUS` のTAB区切り。空一覧も見出しだけを出す成功です。
JSONは元の課題配列と次のページ情報だけを返します（`hasMore`は付けません）。

```json
{"items":[],"pagination":{"offset":0,"limit":20,"count":0,"total":43}}
```

countは実取得数、totalはcount APIの値で、両API間の更新による不一致を補正しません。
count/listいずれの失敗も空一覧にせず、成功stdoutを出しません。

## 課題詳細

```sh
bl-cli issue view MYAPP-123
bl-cli --profile work issue view 123 --json --no-input
```

対象は正整数のIDまたは課題キー（英数字/underscoreのprojectキー + `-` + 正整数）です。
任意精度のIDを扱い、URLは設定済みHTTPS originと応答の課題キーから構築します。
`--project` は受け付けず、cwdのproject・検索条件・create既定値も対象を上書きしません。

固定表示は課題キー・タイトル・種別・優先度・担当者・状態・完了理由・開始日・期限日・
複数マイルストーン・created/updated・URLと本文です。未設定の担当者/完了理由/日付は `-`、
null本文は空本文として表示します。対象課題のprojectIdでprojectを1回取得し、
`textFormattingRule` を確認します。現在は既知/未知の記法とも安全化した原文fallbackで、
Markdown・Backlog記法・HTML/entityを解釈せず、改行/TABと内容を保ちます。
危険な制御文字は可視escape、既知API keyは伏字にします。

JSONは元の応答objectへ構築した `url` を追加し、未知field・大整数・通常の文字列値を保持します
（secret/危険制御文字の安全化は共通契約どおり）。JSONでは課題GETだけで、記法用project GETはしません。
必要fieldの欠落/型不正・不正な応答キーはinvalid_response（HTTP 200、exit 1）、
不正な入力ID/キーはinput（exit 2）です。取得/検証/表示構築がすべて成功してからstdoutへ書きます。
出力失敗やredaction key衝突はoutput（exit 1）として扱います。

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
`search(&Settings, IssueSearch)` は通常・横断・mineの共通入口で、CLIと同じ入力検証・既定値・
名前解決・重複除去・count/listを適用し、上記の課題検索JSONと同じ構造の `Value` を返します。
`IssueSearch::default()` は未指定project/keyword/各条件、mine=false、limit=20、offset="0"。
条件は `Vec<Selection>`（名前とIDは項目ごとに排他）、statusの空Vecは設定を使い、
`Selection::Name("all")` は設定の状態条件を解除します。mine=trueはassignee入力と排他です。
offsetは任意精度を保つ10進文字列で受け取り、返却値は数値です。返却データは安全表示前の原文です。

`issue_view(&str)` は課題詳細の必要fieldを検証し、元objectへ `url` を追加した `Value` を返します。
返却値はsecret/制御文字を含む原文のままで、表示用コピーとは分離します。
`project_text_formatting_rule(&Number)` は正整数project IDの記法を取得し、文字列で返します。
課題取得と分離しているため、表示側が必要な場合だけ呼びます。
共通 `body::display(raw, text_formatting_rule, secrets)` が本文表示入口で、現在はすべて安全な原文fallbackです。

開発環境・ローカルHTTPS試験・品質確認は [CONTRIBUTING.md](CONTRIBUTING.md)、
実装方針は [承認済みPlan](docs/plans/backlog-cli-initial.md) を参照してください。
