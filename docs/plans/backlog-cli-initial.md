# Backlog CLI/TUI 初版 実装計画

## 目的

Rust製の共通libraryと、人向けTUI `bl`・非対話CLI `bl-cli`を新規開発する。Apple Silicon Macで、プロジェクト選択、課題検索・閲覧・作成・部分更新、コメント一覧・個別閲覧・追加・本文更新を安全に実行できる初版を作る。

正式入力は [基本設計](../designs/backlog-cli.md)。特に「実行ファイル」「対象API」「profileと設定」「検索・名前解決・ページ送り」「TUIの画面・操作」「課題の項目・作成・更新」「課題コメント」「本文入力とeditor」「本文変換・表示の安全性」「JSON・通信・書込み結果」「品質・配布」を要求・受入条件として参照する。本Planは、その「後続の実装計画で具体化する事項」を補う。基本設計の目的・責務・対象外は変更しない。

## 要件・決定事項

### 現状・採用技術

- 調査時のリポジトリは基本設計書のみで、既存実装・Cargo設定・テスト・CIはない。既存利用者の移行やデータmigrationは不要。
- 単一Cargo packageに `src/lib.rs`、`src/bin/bl.rs`、`src/bin/bl-cli.rs`を置く。共通処理をTUI/CLIが直接呼び、内部RPC・常駐service・汎用API frameworkを作らない。
- toolchainはRust **1.99.1**、edition 2024、targetは `aarch64-apple-darwin`。主要依存は下表の版で固定し、`Cargo.lock`を追跡する。Cargoのtest/clippy/buildとcargo-auditの導入には `--locked` を使い、監査実行は `cargo audit --file Cargo.lock` で追跡対象のlockfileを直接指定する。

| 用途 | 版・設定 |
| --- | --- |
| Ratatui | 0.30.2、Crossterm 0.29 backendを明示 |
| Crossterm | 0.29.0 |
| Clap | 4.5.54、derive |
| Tokio | 1.49.0、runtime・signal・sync・time・processに必要なfeature |
| Reqwest | 0.12.28、default-features=false、rustls-tls |
| Serde / serde_json | 1.0.228 / 1.0.149、derive / arbitrary_precision |
| pulldown-cmark | 0.13.5 |
| 単行入力の採用候補 | tui-input 0.15.5、後述の早期実端末確認を経て確定 |

本文一時ファイルには `tempfile`を利用する。その他の補助・dev dependencyは、必要性があるものだけ追加してlockfileに固定する。EventStream用の追加依存や独自入力frameworkは不要。

一時ディレクトリで上表の同時ビルドとReqwest設定APIの利用、結合文字・ZWJ emojiのgrapheme単位削除、表示列cursor、大整数・未知fieldのJSON保持をassert確認した。これは製品実装や実IMEの検証ではない。Probeの途中ファイルは残さない。

### Issue #1 再計画：正式監査コマンドの訂正

対象は [Issue #1](https://github.com/minorunakamura/backlog-cli/issues/1) の `REPLAN_REQUIRED`。初回Plan作成後、Cargo設定・lib/2bin・テスト・README/CONTRIBUTING・CIの部分実装が未コミットで存在する。新しいImplementationセッションでは、それらを保持したまま現在のコードと差分を確認して再開する。

- **停止理由・確認済みの原因**：固定済みのcargo-audit 0.22.2で `cargo audit --locked` は `unexpected argument '--locked'` により終了コード2となり、監査されない。同版のhelpに `--locked` はなく、`--file <FILE>` は監査するCargo lockfileの指定として提供されている。
- **合意した変更と理由**：正式監査コマンドをリポジトリ直下の `cargo audit --file Cargo.lock` に訂正し、cargo-audit **0.22.2** と導入方法 `cargo install cargo-audit --version 0.22.2 --locked` を固定する。導入時の `--locked` と監査対象のlockfile指定は別の契約である。test/clippy/buildの `--locked` は維持する。製品仕様・基本設計「品質・配布」の目的は変更しない。
- **Probeの観測**：一時ディレクトリに現在の `Cargo.lock` だけをコピーし、旧コマンドの終了2と新コマンドの216依存の監査・終了0を確認した。実行前後でコピーと元lockfileのSHA-256は不変だった。ただしTokio 1.49.0のunsound警告 `RUSTSEC-2026-0386` が1件出ており、終了0を警告なし・安全性保証とは扱わない。advisory databaseの更新で将来の結果は変わる。
- **適用範囲・タイミング**：今回のPlanningで本Planを同じ正式パスに改訂保存し、既存Issue #1の受入条件・検証手順を整合させる。`CONTRIBUTING.md` の正式品質コマンドと `.github/workflows/quality.yml` の監査stepの訂正・検証は、新しいImplementationセッションでIssue #1の残作業として行う。これらの旧記述より本節と下記の正式チェックを優先する。READMEの利用説明、既存ソース・テスト・Cargo設定・lockfileをこの訂正のためだけに変更しない。
- **維持する範囲**：警告・失敗・確認不能を隠さず報告し、無条件ignoreを追加しない。新しい `--deny` 方針、依存更新、advisory databaseの固定、独自監査wrapperは今回追加しない。既存の未コミット変更をcommit / stash / 破棄しない。後続Issueの製品要件や依存関係を変更しない。
- **回帰確認**：コマンド訂正のみなので、この訂正に新規unit testやTDDは追加せず、同じcargo-audit版での実コマンドとlockfile不変確認を回帰検証にする。Implementationでリポジトリ直下から `cargo audit --version` を確認し、`shasum -a 256 Cargo.lock` → `cargo audit --file Cargo.lock` → `shasum -a 256 Cargo.lock` を順に実行する。引数エラーではなく対象lockfileの監査が実行され、ハッシュが変わらないこと、実際の終了コード・警告・失敗を記録することを確認する。監査失敗を成功扱いにせず、対応に要件・固定依存の変更が必要なら停止して再計画する。既存の正式チェックを再実行し、過去のチェック・Review・Code Review Approveを変更後の差分へ流用しない。Plan・Issue #1・ローカル手順・CIの版・導入・監査コマンドを照合し、CIのtrigger等を維持する。GitHub上での実行のためだけのpush/PRは行わない。

### CLIのflag契約

基本設計の13コマンドと短縮flagを実装する。help/versionは設定・API接続なしで利用できる。TTYでも対話せず、明示された `--body-file -`以外でstdinを読まない。globalの `--profile`・`--json`・`--no-input`はsubcommandの前後で使える。`--no-input`は動作を変えない。

| 適用先 | 入力 |
| --- | --- |
| issue-type list | project |
| issue list | project、keyword、status、assignee、milestone、limit、offset |
| issue mine | listと同じ。ただしassigneeはself固定で、担当者flagは受け付けない |
| issue create | project、title、body/body-file、issue-type、priority、assignee、start-date、due-date、milestone |
| issue update | title、body/body-file、issue-type、priority、assignee、status、resolution、start-date、due-date、milestoneと対応する解除 |
| comment list | limit、min-id、max-id |
| comment add | body/body-file、status、assignee、resolutionと担当者・完了理由の解除 |
| comment update | body/body-fileのみ、明示入力必須 |

project list / priority list / resolution list、およびviewには表にない入力flagを追加しない。update/commentのprojectは対象課題から取得し、project指定やローカルprojectによる上書きを受け付けない。

- `--issue-type-id`・`--priority-id`・`--status-id`・`--assignee-id`・`--milestone-id`・`--resolution-id`を、それぞれの名前flagの適用先に設ける。同一項目の名前flagとID flagは排他。projectは既存の `--project`でキーまたは数値IDを受け取る。
- 数値IDは正整数。resolution IDだけは0を有効値とする。名前は完全一致、候補なし・複数一致はエラー。不要な候補APIを呼ばない。単独のID入力では名前解決用候補取得を要求せず、最終的な所属・権限等はAPIの検証も受ける。
- listのstatus/assignee/milestoneは各flagを反復でき、項目内OR・項目間AND。create/updateのmilestoneも反復でき、更新では指定集合へ置換する。他の書込み項目は単一指定。同じ単一flagの重複はエラー。反復値の重複は集合として一度だけ送る。
- `--status all`は一覧の状態既定値解除。`all`と他の状態指定の混在は拒否する。`--assignee self`は選択profileの認証ユーザー。横断の `--project all`はlist/mineだけで、keywordまたはselfを必須とし、ローカル検索既定値を一切適用しない。状態・milestone・self以外の担当者入力を拒否する。
- `--clear-assignee`はcreate/update/comment add、`--clear-resolution`はupdate/comment add、`--clear-start-date`・`--clear-due-date`・`--clear-milestones`はupdateだけ。対応する値指定と排他。createのclear-assigneeは担当者既定値を適用しない指定であり、不要な更新操作を発生させない。
- 本文専用の解除flagは設けない。`--body`と`--body-file`は空文字指定も含め排他。UTF-8検証後は入力元によらず同じ値を使い、空白・改行をtrimしない。空ファイル・空stdinも空本文になる。課題更新の空本文は解除、コメント追加の空本文は本文なし、既存コメント更新の空本文は入力エラー。
- titleは空白のみ不可。日付は実在する `YYYY-MM-DD`を検証する。日付間の大小など設計にない業務制限は加えない。
- issue list/mineはlimit既定20・1〜100、offset既定0・0以上。comment listもlimit既定20・1〜100、`--min-id`・`--max-id`は正整数・境界自身を含まない。同時指定ならmin < max。CLIコメント一覧は降順固定、指定した1ページのみ返す。offsetやtotalは使わない。
- 未指定・設定由来・明示解除を区別する。更新とコメント追加の課題項目は最新値と比較し、変更項目だけ送る。全く変更のない更新、本文も実変更もないコメント追加は入力エラーとして書き込まない。既存コメントも本文が同じなら書き込まない。

### 出力・エラーと終了コード

成功JSONの形は基本設計「JSON・通信・書込み結果」をそのまま採用する。API応答を `serde_json::Value`として保持し、必要fieldを別途検証する。未知fieldと大整数を落とさず、数値を安易にf64へ変換しない。issue view/create/updateだけに検証済みorigin/keyから生成したurlを追加する。JSON取得では本文記法確認や描画のためのGETを行わない。

固定表示は、一覧ではID/キー・名前やタイトル・状態など識別と選択に必要な列、課題詳細では設計の全項目と本文、コメント詳細では本文と変更履歴を表示する。書込み成功では対象キーまたはIDと結果を表示する。空一覧と取得失敗を区別する。非TTYは装飾なし、NO_COLOR指定時は両binaryの色装飾なしとする。

JSON失敗はstderrへ改行終端の1文書、stdoutには成功文書や案内を混ぜない。構造は次のとおり。

```json
{"error":{"kind":"network","message":"安全化した説明","httpStatus":null,"apiErrors":[],"write":{"operation":"issue.create","outcome":"unknown"}}}
```

- `kind`は input / config / auth / permission / not_found / rate_limit / network / api / invalid_response / output / terminal / editor / interrupted。
- httpStatusは取得済みstatusまたはnull、apiErrorsは安全化したBacklog errors配列、取得できなければ空配列。
- `write`は書込み操作と識別できる失敗時に必須、読取りでは省略。operationは issue.create / issue.update / comment.add / comment.update。outcomeは作成系でnot_created / unknown / created、更新系でnot_updated / unknown / updated。comment.addは実際にPOSTしたか課題PATCHしたかで確度の語彙を選ぶ。
- 終了コードは0=成功/help/version、2=引数・本文・名前解決・変更なし等の入力エラー、1=設定・API・通信・表示・端末等の実行エラー、130=Ctrl+C等の処理可能な中断。書込み確度は終了コードと独立で、created/updatedでも後続処理失敗なら非0。
- Clapの引数エラーも、`--json`モードを識別できる場合は同じエラー形式で返す。help/version自体は通常のテキスト表示とする。
- secret保護はJSON値だけでなく未知field・object key・配列も対象。数値にsecretが一致する例外でも秘匿を優先し、必要ならそのscalarを伏字文字列へ置換する。keyのredactionで衝突する場合は項目を黙って失わず、安全な出力エラーにする。通常の非secretの数値・本文は保持する。raw responseや未安全化のエラーをdumpしない。

## 制約・対象外

基本設計の対象19 API・公開コマンド・キー操作・画面境界を維持する。削除・添付送信・独自alias・設定wizard/自動生成/補修・永続cache/履歴/下書き/書込み状態・競合検出/自動merge・全件自動取得・定期更新・Taskflow連携は追加しない。

保証targetはApple Silicon Macのみ。Release公開・署名・notarization・Homebrew・他OS保証は含めない。GitHub上のCI実行成功、pushやPR作成を完了条件にしない。実Backlogへの書込みは接続先と内容を確認した別の明示許可がある場合だけ行い、本Planの承認をその許可にしない。実サービスの複数project検証は現環境で不可能な制約を引き継ぎ、追加projectは要求せず自動試験で確認する。

基本設計に書かれたAPI検証結果は根拠であって、製品の検証済み記録として流用しない。SIGKILL・端末切断後の復元/一時ファイル削除、通知不能時の通知到達、editor独自backup管理、全端末での幅やIMEの完全一致は保証しない。

## 実装方針

### 1. 共通libraryと設定

config / api / service / body・safetyを機能単位のmoduleに分け、CLI/TUIが同じ検索・名前解決・変更判定を利用する。汎用trait群やplugin境界は作らない。

XDG・cwd直下settings・default profile・apiKeyEnv・editor argvの適用と検証は基本設計に従う。不在と壊れた設定を区別し、未知キー・型・不正値を拒否する。create既定値を他操作へ流用しない。起動後に設定やprofileを差し替えない。

spaceUrlはHTTPS originとして検証し、userinfo・query・fragment・root以外のpathを拒否する。rootの末尾slashは正規化する。固定API pathと検証済みID/キーをURL部品として扱い、文字列連結で任意pathやoriginへ逸脱させない。ブラウザも同じoriginと選択課題キーのHTTPS URLだけをshellなしで開く。

### 2. API通信と書込み確度

Reqwestは `redirect(Policy::none())`・`retry(reqwest::retry::never())`・`timeout(30秒)`を明示する。API keyはBacklog-API-Key headerだけへ入れる。POST/PATCHはform、配列項目はAPIの配列パラメーターで送る。

- 元JSONを保持しつつ、操作に必要なobject/field/IDと期待する成功statusを検証する。
- 更新の未指定項目はformへ含めない。日付・担当者・完了理由の解除は該当fieldの空値、milestoneの解除は空のmilestone配列パラメーターで表す。resolution ID 0と解除を区別する。
- 本文だけのコメント追加はPOST comments。課題項目の変更を伴う場合はPATCH issueへ変更項目と非空本文のcommentを一回で送り、2書込みや権限不足時のfallbackをしない。コメント本文更新はPATCH commentsだけ。
- 作成系201／更新系200を観測した直後、body読取り等の次のawaitより前に確度をcreated/updatedへ記録する。`bytes()`読取り、JSON decode、必要field検証、表示の失敗でこれを下げない。
- request送信を開始する前に「送信される可能性あり」を記録する。送信前と証明できる失敗は未実施、信頼できるAPI拒否は未実施、送信開始後に成否を確認できないtimeout・切断・不正応答・5xx等は保守的にunknownとする。redirectを追わない。
- 中断時は通信taskをabortしてjoinし、記録済み確度を読んで通知する。cancelのJoinErrorで確度を未実施へ上書きしない。通知と端末復元の一方の失敗で他方を省略しない。成功確認だけの追加GET、自動retry、永続的な結果管理は設けない。

### 3. 検索・ページと読取り

通常/横断/mineを同じ検索処理に集約する。新規検索は同一条件のcountとlistを取得し、両方の成功後だけ適用する。更新日時降順、実取得数による終端、count失敗非隠蔽は基本設計どおり。

課題一覧履歴はページ内容・選択・scrollをメモリ保持する。再訪では通信せず、Rや作成後再取得の成功時だけ先頭へ置換し履歴を破棄する。失敗では以前の表示を保つ。

コメントは独立したID境界pagingを実装する。TUIの古い側は降順とmaxId、新しい側の隣接取得は昇順とminIdを使い、画面の移動順へ整える。gは最新側、Gは昇順で最古側を直接取得し、全件走査しない。content:nullの変更履歴も移動対象とする。limitDate/assigner等の既知fieldは表示名へ変換し、未知fieldと前後値も安全表示する。

### 4. 本文変換と安全表示

編集原文・APIデータと表示用copyを分離する。Markdownはpulldown-cmark、Backlog記法は主要構造に絞った変換と安全な原文fallbackで扱う。code/escapeの保護、改行保持、HTML・未知macro等の内容保持をfixtureで確認する。画像等をfetchしない。

外部文字列は全出力経路で、解釈後の文字も含めてsecret伏字化・危険な制御文字/端末命令の無害化を適用する。改行・TAB、日本語・結合文字・emojiを必要なく壊さない。外部ANSI/OSCやHTMLを端末命令として実行しない。URLは可視textとする。

### 5. TUIの状態・通信・入力

main taskが状態と端末を単独所有する。Tokioの短いinterval（目安16ms）とCrosstermの `poll(Duration::ZERO)`、ready時だけのreadで入力を処理し、tickごとの処理数を制限する。通信は別taskから結果を返す。通信完了までUIをawaitで止めない。

結果には対象と世代を付け、検索変更・選択変更・取消・画面閉鎖後の古い結果が表示やbusyを変更しないようにする。プレビューは目安150ms間引き、選択が変わった時点で別課題の本文を表示しない。Enterだけで再取得しない。

基本設計のWorkspace/Project/Issue/Detail/Comment、左右内のfocus、キー表、100x24 / 40x12のlayout、極小時の保護を実装する。検索・R・作成後/コメント追加後の再取得は成功後だけ表示交換し、失敗時の状態保持と通知を実装する。課題更新で一覧を即時書換えず、Comment本文更新でも勝手に最新コメントへ移動しない。

単行入力はtui-inputの原文値・grapheme編集とvisual_cursor/visual_scrollを使い、Ratatuiの実cursorを入力位置へ置く。title/keywordとフォーム内の日付入力で同じ扱いを使う。貼付けのCRLF/CR/LF/TABは単行用spaceへ正規化し、表示変換と区別する。通常文字や編集キーをグローバル操作に横取りしない。

入力全体にsecretまたは安全変換が必要な制御文字があれば、欄全体を固定の伏字表示にしcursorもその表示内の固定位置へ置く。横scrollで見えない部分も検査し、原文を描画widgetへ流さない。編集原文は保持する。OS/IMEが受信前に描く未確定文字の秘匿は保証しない。

**入力部品の早期確定条件**：TUIの基本shellを作った段階で、本格的な画面・フォーム実装より前に、実端末で日本語IMEの未確定文字・候補のcursor追従、日本語/結合文字/ZWJ emoji、長文水平scroll・狭幅resize・focus移動・伏字時の安全表示を確認する。採用候補のビルド成功をこの確認の代わりにしない。失敗時は入力値保持や安全要件を弱めず、本格実装前に停止して部品/実現方法の再計画を行う。

### 6. editorと書込みフォーム

端末復元を単一のguard/共通処理に集約し、正常終了・処理可能な中断・失敗・panicで可能な復元を試みる。復元失敗は通知する。editor開始時はTUIのpollを止め、raw/alternate screen等を復元してからargvをshellなしで実行する。終了後は端末サイズ再取得・clear・全体再描画を直ちに行い、追加キーを要求しない。

本文用tempfileは本人だけが読み書きできるよう保護する。原文を渡し、正常終了＋読取＋UTF-8検証の全成功時だけ本文を交換する。失敗・中断で元値と他フォーム値を保持し、一時データを削除する。設定編集は既存fileだけを開き、非0終了でも保存内容を巻き戻さず、反映は再起動後とする。

課題/コメントの編集開始時は最新情報を取得する。コメント編集は認証ユーザー自身の本文だけで、本文なし履歴は閲覧専用、API権限も尊重する。

フォーム→確認→独立した明示送信の段階を分ける。確認画面だけでは書き込まず、入力差がある時だけ破棄確認する。送信中は編集・重複送信・破棄を無効にする。中断は確度を保って処理する。確認には接続先・対象・全入力/変更と解除内容を安全表示する。

## 開発順序と受渡し

以下は技術的な順序であり、Work Itemsの分割承認を代替しない。各項目のテスト・説明はその機能と一緒に追加する。

1. Cargo/toolchain・lib/2bin・品質チェックの土台。
2. 厳密設定・安全表示・API client・書込み確度の共通基盤。
3. 候補/名前解決・検索/ページ・課題/コメント読取りと非対話CLI。
4. 課題/コメントの書込みサービスとCLI契約・エラー。
5. TUI基本shell・単行入力・端末復元・editor連携の最小経路、および入力部品の早期実端末確認。
6. TUIのプロジェクト/検索/一覧/プレビュー/コメント閲覧と状態保持。
7. TUIの作成・課題更新・コメント追加/本文更新・確認/結果表示。
8. 全体受入、README/CONTRIBUTING、GitHub Actions、配布tar.gz/SHA-256の確認。

各Work Itemは正式な本Planを参照し、新しいImplementationセッションで着手する。上流機能が未完成なのに下流を完了扱いにしない。設計変更が必要ならPlan Reviewへ戻り、承認を流用しない。

## 完了条件

- 基本設計と本Planのコマンド・flag・設定・19 API・画面操作・書込み確度・安全表示が実装され、後述の公開境界から検証できる。
- CLIはTTY/非TTYとも対話せず、JSONと終了コード契約、原文/未知field/大整数保持とsecret保護を満たす。
- TUIは通信中の操作・中断、古い応答の無視、ページ再訪、更新失敗時の保持、確認前の非送信、重複送信防止を満たす。
- 早期の入力部品確認と、完成後の実端末受入を両方実施し、環境・手順・結果を記録する。実IME未確認は自動試験合格で完了に置き換えない。
- ローカルでtest/rustfmt/Clippy/cargo-auditを実行し、失敗や確認不能を隠さない。既知脆弱性の無条件ignoreを追加しない。
- main pushとpull_requestを対象にGitHub Actionsの品質workflowを作成して設定確認する。GitHub上で未実行ならその旨を報告するだけで、実行のためのpush/PRを要求しない。
- 利用説明をREADME、開発/品質/CI/配布手順をCONTRIBUTINGへ分離して記載する。
- release buildのbl/bl-cli・README・secretなし設定例だけを `backlog-cli_darwin_arm64.tar.gz`へ梱包しSHA-256を生成する。利用者設定・toolchain・fixtureを混入させない。展開先の別ディレクトリで両binaryの起動を確認し、native依存が利用者環境で満たせることを確認する。公開はしない。

## 動作確認方法

### TDDと公開境界

機能ロジックは既存のtdd Skillに従い、一つの失敗テスト→最小実装の縦sliceで進める。本Planで確認するseamは次の4つ。内部のprivate関数名・細かいtask数を固定するテストは作らない。

| seam | 捕捉するもの / 捕捉しないもの |
| --- | --- |
| 実CLIプロセスのargv/stdin/stdout/stderr/exit | flag契約・非対話・JSON/終了コード / 実IME |
| 共通libの操作・検索・本文/安全表示の公開入口 | 既定値・変更判定・原文保持・変換/伏字 / 実端末の字幅 |
| APIのHTTP送受信境界 | method/path/header/form・paging・失敗/確度 / 実Backlogの全権限組合せ |
| TUIの入力→状態/表示、Ratatui TestBackendとPTY | 状態保持・古い応答・focus・layout・端末復元 / OSの実IME描画 |

UIの見た目・実IME・配布確認は無理にunit test化せず、実行可能な手動受入を組み合わせる。

### 自動試験

- 設定: XDG未設定/空/相対/絶対、profile/key欠落、cwd限定、未知キー/型不正、明示優先・既定値非流用、editor argv。
- CLI: 全13コマンドのhelpと主要成功/失敗、flag適用先/反復/排他/解除、ID0、stdin明示時だけ読取、空本文・不正UTF-8、同値更新非送信、JSON引数エラー。
- API: ローカルHTTPS harnessとテスト専用trustを使用し、製品にHTTP/TLS検証回避flagを追加しない。header認証・query非使用、form配列/空値/未指定、コメントPOST/PATCHの一回送信、所有者検証、4xx/429/5xx/不正JSON/切断/timeout/redirect非追従とretryなしを検証する。必要なテスト証明書・server crateだけdev dependencyにする。
- 確度: 送信直前、送信開始後、201/200 headers受信後body待ち、それぞれの中断/切断を制御して試す。成功status後のbody/decode/出力失敗でcreated/updatedが残ることを検証する。
- ページ: count/list同条件、count失敗、空ページ、totalとのずれ、再訪時の通信なし、ID境界除外・両方向/最古、content:null履歴。複数project横断・曖昧候補・権限制限はmockで検証する。
- 安全表示: 日本語/結合文字/emoji、HTML entity後の制御文字、ANSI/OSC、code/escape、未知macro/field、secretを含む未知key/値、巨大数値、水平scroll外のsecret、入力原文非改変、NO_COLOR/非TTY。
- TUI: 世代を逆転させた遅延応答、取消/画面閉鎖後応答、R失敗と成功時の交換、極小時の送信不可、入力キー衝突、フォーム値・ページ選択・scroll保持、作成/コメント追加後の再取得失敗、editor失敗/UTF-8不正/resize/中断・一時file削除。

### 実端末・配布受入

Apple Silicon Macの実端末で、端末名/版・macOS・IME・対象buildを記録する。特定端末を製品の利用条件にしない。

早期の単行入力確認に加え、完成後に100x24以上/40x12以上/それ未満で操作、長い日本語・結合文字・emoji・変換候補、editor中resizeからの即時再描画、editor非0/起動失敗/中断、終了後に通常のshell操作ができることを確認する。状態・入力保持と伏字も目視する。自動試験で代替できない操作は実装担当と利用者で実施し、未実施のまま完了にしない。記録先は `docs/testing/manual-acceptance.md`とする。

実サービスへの書込み許可がなければローカルharnessで書込み経路を確認し、実サービスE2E未実施を記録する。既存の設計検証用課題を無断で変更しない。

正式チェック:
```sh
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo audit --file Cargo.lock
cargo build --locked --release --target aarch64-apple-darwin --bins
```

cargo-audit自体は **0.22.2**、導入は `cargo install cargo-audit --version 0.22.2 --locked` にCONTRIBUTINGとCIの両方で固定する。監査はリポジトリ直下で `cargo audit --file Cargo.lock` を実行し、対象lockfileを生成・更新しない。advisory databaseは通常どおり更新し、警告・失敗と終了コードを報告する。配布物は `file`・`otool -L`・展開後のhelp/version起動・SHA-256照合でtarget、native依存、両binary、梱包物を確認する。

## 根拠・残るリスク

主要技術の版別資料:
- [Ratatui 0.30.2 Frame](https://docs.rs/ratatui/0.30.2/ratatui/struct.Frame.html)、[tui-input 0.15.5 Input](https://docs.rs/tui-input/0.15.5/tui_input/struct.Input.html)
- [Reqwest 0.12.28 ClientBuilder](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html)、[Response](https://docs.rs/reqwest/0.12.28/reqwest/struct.Response.html)、[retry::never](https://docs.rs/reqwest/0.12.28/reqwest/retry/fn.never.html)
- [serde_json 1.0.149 Cargo features](https://github.com/serde-rs/json/blob/v1.0.149/Cargo.toml)、[pulldown-cmark 0.13.5](https://docs.rs/pulldown-cmark/0.13.5/pulldown_cmark/)
- BacklogのAPI送信・解除形式・境界・権限と既存Probeの根拠は基本設計の「品質・配布」「主要参照」を参照する。

依存の同時ビルドと小さなassertは確認済みだが、製品全体の動作・脆弱性・配布物のnative依存・実端末IME/editor/resizeは未検証。入力部品は早期実端末確認まで候補とし、そこで実現できない場合は停止して再計画する。API競合・count/listのsnapshot不一致・実サービス複数project未検証等の既知の限界は基本設計のとおり維持する。
