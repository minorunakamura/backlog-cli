# backlog-cli

Apple Silicon Mac向けのBacklog TUI `bl` / 非対話CLI `bl-cli`（Rust）。
現在は `bl-cli project list` / `issue-type list` / `priority list` / `resolution list` と
`issue list` / `issue mine` / `issue view` / `issue create` / `issue update` / `issue comment list` / `issue comment view` / `issue comment add` / `issue comment update` を利用できます。
TUI画面は未実装です。
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
`project` は `issue-type list`・通常の `issue list/mine`・`issue create` の既定値です。
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
TTYでも非対話で、明示 `--body-file -` 以外のstdin・editor・選択UI・確認待ちは使いません。`--no-input` は動作を変えません。

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

読取りの通信は操作に必要なGETだけです。検索ではcountとlist、必要なproject解決・候補・self取得を行い、
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
`textFormattingRule` を確認します。Markdownは見出し・強調の記号を外し、箇条書きを `•`、
checklistを `☑` / `☐`、通常表をTAB区切りで表示します。番号list・引用・code blockは
読み取り用の記号を保ち、inline codeは内容をそのまま表示します。通常改行・空行・TAB、
code/escape内の記法は保ちます。link・画像・参照は名前/代替textとURL・題名を可視textで表示し、
画像や参照先の取得はしません。

Backlog記法では `*` / `**` / `***` の見出し、`-` / `--` / `---` の箇条書き、
`+` の番号list、checklist、`''強調''` / `'''斜体'''` / `%%取消%%`、`>` / `{quote}` の引用、
通常表（末尾 `h` / cell先頭 `~` の見出しを含む）と `&br;` を表示用に変換します。
`{code}` / `{code:java}` 等のcode blockは区切りも内容も保持し、backslash escapeも誤変換しません。
認識できる `BLG-104` / `[[BLG-87]]` はキーと設定済みoriginの可視URLを表示します。
`[[URL]]` / `[[名前>URL]]` / `[[名前:URL]]` のHTTP(S) linkも可視textです。
Wiki/Git/SVN・添付/画像参照・目次は、名前・revision・ID等を原文のまま保持し、URLを推測しません。
HTMLのタグ/内容・未知macro/拡張は落とさず、複雑表や未閉鎖構文など難しい構造と未知の記法値は
安全な原文fallbackです（未知macro/HTMLを含む行は行全体を保持）。完全な記法互換は保証しません。
表示用copyだけを変換し、entity等の解釈後にも危険な制御文字は可視escape、既知API keyは伏字にします。
TTY/非TTY・NO_COLORの有無によらず、本文も常に色装飾なしです。

JSONは元の応答objectへ構築した `url` を追加し、未知field・大整数・通常の文字列値を保持します
（secret/危険制御文字の安全化は共通契約どおり）。JSONでは課題GETだけで、記法用project GETはしません。
必要fieldの欠落/型不正・不正な応答キーはinvalid_response（HTTP 200、exit 1）、
不正な入力ID/キーはinput（exit 2）です。取得/検証/表示構築がすべて成功してからstdoutへ書きます。
出力失敗やredaction key衝突はoutput（exit 1）として扱います。

## 課題作成

```sh
bl-cli issue create -p MYAPP --issue-type タスク --priority 中 -t "修正" -b "修正内容"
printf '修正内容\n' | bl-cli issue create -p MYAPP --issue-type タスク --priority 中 -t "修正" --body-file - --json
bl-cli issue create -p 123 --issue-type-id 7 --priority-id 3 -t "修正" --clear-assignee \
  --start-date 2024-02-29 --due-date 2024-03-01 --milestone-id 8 --milestone-id 9
```

project・タイトル・種別・優先度が必須です。projectはcwd設定、種別・優先度・担当者は
`issue.create` の既定値を使え、明示flagを優先します。タイトルは空白のみ不可（送信値はtrimしません）。
`--assignee/-a self` は選択profileの認証ユーザー、`--clear-assignee` は担当者既定値を適用せず、
assigneeId自体を送信しません。担当者の値指定と解除は排他です。状態・完了理由はAPIの初期値を使います。

名前flagと `--issue-type-id` / `--priority-id` / `--assignee-id` / `--milestone-id` は項目ごとに排他です。
名前は完全一致で解決し、不在・複数一致は入力エラー。IDは任意精度の正整数で、候補取得はしません。
projectキーだけはproject GETでIDへ解決します。`--project all` は作成では使えません。
`--milestone/-m` または `--milestone-id` は反復可能で、同じIDは一度だけ送ります。
他の単一flagの反復はエラー。開始日・期限日は実在する `YYYY-MM-DD` を検証し、前後関係の制限は加えません。

`--body/-b` と `--body-file` は空文字指定も含め排他です。UTF-8の本文を空白・改行・記法・制御文字も
変換せず送信します。空ファイル・空stdinは空本文、未指定ならdescriptionを送信しません。
`--body-file -` のときだけstdinを読みます。本文省略でeditorを起動せず、TTYでも確認待ちはありません。

POST `/api/v2/issues` をformで一回だけ送信し、milestoneは反復 `milestoneId[]` にします。
成功確認・表示用の追加GET、自動retry、redirect追従、結果の永続保存はしません。
成功はHTTP 201のobject・正整数id・安全なissueKeyを検証し、固定表示は `CREATED` / `URL`、
JSONは元objectへ検証済みorigin/keyからの `url` を追加します。未知field・巨大整数は保持し、表示時だけ安全化します。

識別できる作成エラーは、引数・設定・本文エラーも含め `write` が必須です（読取りエラーでは省略）。
送信前または具体的なBacklog errorsを伴う信頼できる4xx拒否は `not_created`、送信開始後の切断・timeout・
不正応答・5xx等は `unknown`、201観測後は本文読取・decode・必要field検証・出力失敗でも `created` を維持します。
中断時も通信taskを停止・回収して観測済み確度を報告します。固定エラーにも同じ確度を表示します。
終了コードは確度と独立で、createdでも後続エラーは1、中断は130です。実施済み・結果不明から再送を促しません。

```json
{"error":{"kind":"network","message":"Response body failed or timed out","httpStatus":201,"apiErrors":[],"write":{"operation":"issue.create","outcome":"created"}}}
```

## 課題の部分更新

```sh
bl-cli issue update MYAPP-123 -t "修正" -b "修正内容" --status 完了 --resolution-id 0
bl-cli issue update 123 --assignee self --milestone-id 8 --milestone-id 9 --json
printf '' | bl-cli issue update MYAPP-123 --body-file - --clear-due-date --clear-milestones
```

対象は正整数IDまたは課題キーです。最新の課題をGETし、明示入力と比較して異なる項目だけを
単一PATCH `/api/v2/issues/:issueIdOrKey` へform送信します。未指定項目は保持し、全く変更がなければ
input（exit 2、PATCHなし）です。projectは最新課題のprojectIdで判断し、cwdのproject・create既定値は
適用しません。`--project`・`--created`・`--updated` やコメント送信flagは受け付けません。
競合検出・自動mergeはなく、GET後に同じ項目が他者に変更される可能性は残ります。

更新項目は `--title/-t`・`--body/-b` / `--body-file`・`--issue-type`・`--priority`・
`--assignee/-a`・`--status/-s`・`--resolution`・`--start-date`・`--due-date`・`--milestone/-m` です。
名前と対応する `--issue-type-id` / `--priority-id` / `--assignee-id` / `--status-id` /
`--resolution-id` / `--milestone-id` は項目ごとに排他。名前は対象project（優先度・完了理由はspace）の
候補で完全一致解決し、不在・複数一致は入力エラーです。`--assignee self` は選択profileの認証ユーザー。
直接IDは候補GETなしで、任意精度の正整数を使います。resolution ID 0は値であり解除ではありません。

`--clear-assignee`・`--clear-resolution`・`--clear-start-date`・`--clear-due-date`・`--clear-milestones`
が明示解除で、対応する値指定と排他です。担当者・完了理由・日付は空値、milestoneは空の `milestoneId[]` を
送ります。milestoneは反復可能で指定集合へ置換し、重複IDは一度だけ送信、順番だけの変更は送信しません。
他の単一flagの反復は拒否します。タイトル・実在日付の検証は作成と同じで、日付の前後関係は制限しません。
応答の日付に時刻が付いていても、変更判定はカレンダー日付で比較します。

本文はUTF-8の原文（空白・改行・制御文字を含む）をtrim・変換せず送信します。
本文省略なら保持、明示空文字・空file・空stdinなら解除で、null本文と空本文は同値です。
`--body` / `--body-file` は空指定も含め排他、`--body-file -` だけがstdinを読みます。
TTYでもeditorや確認を起動せず、成功確認GET・retry・redirect追従・永続的な結果管理はしません。

HTTP 200の応答object・正整数id・安全なissueKeyを検証し、固定表示は `UPDATED` / `URL`、
JSONは元の応答objectへ検証済みorigin/keyからの `url` を追加します。未知field・大整数は保持し、表示時だけ安全化します。
識別できる更新エラーは引数・設定・本文・事前GET/名前解決を含め `write.operation: "issue.update"` を持ちます。
送信前または信頼できる具体的な4xx拒否は `not_updated`、送信開始後の成否不明は `unknown`、
200観測後は本文読取・decode・必須field検証・出力失敗・中断でも `updated` を保持します。
終了コードは確度と独立で、通常の後続エラーは1、処理可能なCtrl+C中断は130です。結果不明・更新済みから再送は促しません。

```json
{"error":{"kind":"network","message":"Response body failed or timed out","httpStatus":200,"apiErrors":[],"write":{"operation":"issue.update","outcome":"updated"}}}
```

## コメントの読取り

```sh
bl-cli issue comment list MYAPP-123
bl-cli issue comment list MYAPP-123 --limit 20 --max-id 456 --json
bl-cli issue comment list MYAPP-123 --min-id 100 --max-id 456 --json
bl-cli --profile work issue comment view MYAPP-123 234 --no-input
```

課題は `issue view` と同じ正整数IDまたは課題キー、コメントIDと `--min-id` / `--max-id` は
任意精度の正整数です。両境界指定時はmin < maxが必要です。limitは既定20・1〜100。
`count` / `order=desc` と指定した `minId` / `maxId` で降順の1ページだけ取得し、境界ID自体を除外します。
offset・total・コメント件数API・全件自動取得は使いません。`--project` は受け付けず、cwdのprojectで対象を置き換えません。

固定表示は1件ごとにID、COMMENT / CHANGE HISTORYの文字ラベル、投稿者、created/updated、本文と変更履歴を出します。
`content:null` の履歴も落とさず、本文と履歴の両方があれば同じ1件として両方表示します。
履歴の `limitDate` は DUE DATE、`assigner` は ASSIGNEEなどの表示名にし、未知fieldは識別子のまま、
変更前後の値も安全に表示します。空ページは `No comments` を出す成功で、API失敗と区別します。
固定表示に文字列本文がある場合だけ対象課題のprojectIdとproject記法を取得し、本文は課題詳細と同じMarkdown/Backlog変換・
安全な原文fallbackを使います。全件 `content:null` の履歴はコメントGETだけで表示します。
文字列本文が混在する場合は記法取得の失敗を隠しません。画像・参照先は取得しません。

JSONは一覧が `{"items":[...]}`、個別詳細は元のAPI objectです（url追加なし）。
本文・変更履歴・未知field・大整数は共通のsecret/制御文字保護以外は保持します。
JSONではコメントGETだけで、記法確認用のissue/project GETは行いません。
入力エラーはexit 2、取得・応答検証・表示/出力エラーはexit 1で、全構築成功後だけstdoutへ出します。

## コメント追加・課題項目の変更

```sh
bl-cli issue comment add MYAPP-123 -b "確認しました"
printf '対応内容\n' | bl-cli issue comment add MYAPP-123 --body-file - -s 完了 --resolution-id 0 --json
bl-cli issue comment add MYAPP-123 --assignee self --clear-resolution
```

`--body/-b` または `--body-file`、`--status/-s` / `--status-id`、`--assignee/-a` / `--assignee-id`、
`--resolution` / `--resolution-id`、`--clear-assignee` / `--clear-resolution` だけを受け付けます。
名前/ID/解除は項目ごとに排他、すべて単一指定です。IDは正整数、完了理由だけ0も有効です。
名前は完全一致（不在・複数一致は入力エラー）、`--assignee self` は選択profileの認証ユーザーです。
課題項目を指定した場合は対象課題の最新値と比較し、実変更だけ送ります。
名前解決には対象課題のprojectを使い、cwdのprojectやcreate既定値は適用しません。
状態変更に応じた完了理由の自動設定・解除も行いません。

本文はUTF-8原文の空白・改行・記法・制御文字をtrim/変換せず送ります。本文入力の両flagは空文字も含め排他、
空文字・空ファイル・空stdinは本文なしです。`--body-file -` のときだけstdinを読み、本文省略でeditorや確認待ちを起動しません。
本文も課題項目の実変更もなければinput（exit 2）として書き込みません。

- 課題項目の実変更なし（同値指定も含む）: POST `/api/v2/issues/:issueIdOrKey/comments` に `content=<本文>`。
  本文だけの場合は事前GETも行いません。成功は201と正整数コメントIDを検証し、固定表示は `CREATED` とID、
  JSONは `{"resource":"comment","data":<元のコメントobject>}` です。
- 実変更あり: PATCH `/api/v2/issues/:issueIdOrKey` に変更項目と、非空本文があれば `comment=<本文>`。
  解除値は空文字で送信します。成功は200と正整数課題ID・有効なissueKeyを検証し、固定表示は `UPDATED` と課題キー、
  JSONは `{"resource":"issue","data":<元の課題object>}` です。課題objectにurlを追加・上書きしません。

未知field・大整数・通常文字列は保持し、出力時に共通safetyを適用します。
書込みは一回だけで、二重書込み、権限不足時の本文POSTへのfallback、コメントID補完/成功確認の追加GET、retryはしません。
エラーは引数・設定・本文・事前GET・名前解決を含め `write.operation: "comment.add"` を持ちます。
方式未確定の送信前エラーは `not_created`、POST選択後は `not_created/unknown/created`、
PATCH選択後は `not_updated/unknown/updated` です。具体的な4xx拒否・不明応答の判定は課題作成/更新と同じです。
201/200観測後は本文読取・decode・必須field検証・出力失敗・中断でも `created/updated` を保持します。
終了コードは確度と独立し、成功0、入力エラー2、実行エラー1、中断130です。

## 既存コメントの本文更新

```sh
bl-cli issue comment update MYAPP-123 234 -b "修正したコメント"
printf '修正内容\n' | bl-cli issue comment update MYAPP-123 234 --body-file - --json
```

対象課題とコメントIDを指定し、最新コメントと認証ユーザーをGETして、自分が登録した既存本文だけを更新します。
他者のコメントと `content:null` の変更履歴は入力エラーで書き込みません。所有者確認に使う投稿者IDも正整数として検証します。
APIの権限拒否はそのまま報告し、回避や別方式へのfallbackはしません。

`--body/-b` または `--body-file` の明示入力が必須で、空文字指定も含め排他・単一指定です。
UTF-8原文の空白・改行はtrimせず、空白/改行だけの本文も保持します。空文字・空file・空stdin・同値更新は
input（exit 2、PATCHなし）です。明示 `--body-file -` だけがstdinを読み、editorや確認待ちは使いません。
課題項目・解除flag・project指定は受け付けず、cwdのprojectやcreate既定値は適用しません。

PATCH `/api/v2/issues/:issueIdOrKey/comments/:commentId` へ `content` だけをformで一回送ります。
成功はHTTP 200とコメントの必要fieldを検証し、固定表示は `UPDATED` とコメントIDだけ、JSONは元の応答objectです。
urlやresource wrapperは追加せず、未知field・巨大整数・通常文字列は保持し、出力時に共通safetyを適用します。
成功確認/表示用の追加GET・retry・redirect追従・結果の永続保存はしません。
最新取得後の競合検出・自動mergeはなく、同じ本文が他者に変更される可能性は残ります。

引数・設定・本文・事前GETも含め、識別できるエラーは `write.operation: "comment.update"` を持ちます。
送信前または信頼できる具体的な4xx拒否は `not_updated`、送信開始後の成否不明は `unknown`、
200観測後は本文読取・decode・必要field検証・出力失敗・中断でも `updated` を保持します。
終了コードは確度と独立で、成功0、入力エラー2、実行エラー1、中断130です。結果不明・更新済みから再送は促しません。

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
`comment_list(&str, limit: u8, min_id: Option<&str>, max_id: Option<&str>)` は上記の1ページの
`{"items":[...]}`、`comment_view(issue_id_or_key: &str, comment_id: &str)` は元objectを返します。
入力と表示に必要な応答fieldを検証し、未知fieldや本文・履歴は表示前の原文で保持します。
`issue_text_formatting_rule(&str)` は対象課題のprojectIdだけを取得・検証し、既存のproject記法取得へ渡します。
コメントJSON取得と独立しており、cwdのprojectや課題詳細の他fieldを要求しません。
`issue_create(&Settings, IssueCreate)` はCLIと共通の作成入口です。`IssueCreate` は `project: Option<&str>`、
`title: &str`、`body: Option<&str>`、`issue_type/priority/assignee: Option<Selection>`、
`milestone: Vec<Selection>`、`clear_assignee: bool`、`start_date/due_date: Option<&str>` を受け取ります。
`Default` は全項目未指定（titleは空文字）で、必須値は呼出し時に検証します。
既定値・入力検証・名前解決・重複除去・単一POST・確度判定を共通で行い、元objectとurlの `Value` を返します。
返却値と送信本文はsecret/制御文字を含む原文のままで、表示の安全化とは分離しています。
`issue_update(issue_id_or_key: &str, IssueUpdate)` は最新取得・変更判定・名前解決・単一PATCH・確度判定の共通入口です。
`IssueUpdate` は `title/body/start_date/due_date: Option<&str>`、
`issue_type/priority/status/assignee/resolution: Option<Selection>`、`milestone: Vec<Selection>` と
`clear_assignee/clear_resolution/clear_start_date/clear_due_date/clear_milestones: bool` を受け取ります。
`Default` は全項目未指定です。設定引数は持たず、名前解決に使うprojectは対象課題から取得します。
比較に使うID・本文・日付などを検証し、使用しない表示名や未指定項目は要求しません。
返却値は作成同様に原文objectと構築urlの `Value` です。
`comment_add(issue_id_or_key: &str, CommentAdd)` は上記の追加操作の共通入口です。
`CommentAdd` は `body: Option<&str>`、`status/assignee/resolution: Option<Selection>` と
`clear_assignee/clear_resolution: bool` を受け取り、`Default` は全項目未指定です。
設定引数は持たず、必要な最新値取得・名前解決・差分比較と単一POST/PATCH・確度判定を行います。
返却 `Value` は `resource/data` の形で、dataは安全化前の元object（url追加なし）です。
`comment_update(issue_id_or_key: &str, comment_id: &str, body: &str)` は既存本文更新の共通入口です。
必須本文・対象IDの検証、最新コメント/認証ユーザー取得、投稿者IDの検証・所有者確認、同値/履歴の拒否と単一PATCHを行います。
設定引数は持たず、返却 `Value` は安全化前の元コメントobject（url追加なし）です。
失敗時は `Error::write` の `WriteResult { operation, outcome: WriteOutcome }` で確度を取得できます。
`WriteOutcome` は `NotCreated/Unknown/Created/NotUpdated/Updated`、従来の `CreateOutcome` 名は同じenumの再exportです。
作成エラーのJSON語彙・`with_create_outcome` の動作は維持しています。

共通 `body::display(raw, text_formatting_rule, secrets)` が本文表示入口です。
Markdownはpulldown-cmark、Backlog記法は主要構造の変換と原文fallbackを使い、最後に共通safetyを適用します。
`body::display_with_origin(raw, text_formatting_rule, &origin, secrets)` は課題link用のHTTPS originも受け取ります。
CLIは検証済み `Config::origin` を渡します。origin不正時や従来の `display` では課題参照を原文のまま保持します。
原文/API JSONは変更しません。未知の記法値は安全な原文fallbackです。

開発環境・ローカルHTTPS試験・品質確認は [CONTRIBUTING.md](CONTRIBUTING.md)、
実装方針は [承認済みPlan](docs/plans/backlog-cli-initial.md) を参照してください。
