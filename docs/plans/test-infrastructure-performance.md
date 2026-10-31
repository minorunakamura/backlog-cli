# テスト保証の選別と重複・高費用テストの削減（#56／#57 再計画）

## 目的

テストを安定化し続けることから、必要な保証を選び直し、重複・過剰な実process／PTY起動を削除・統合・縮小することへ目的を変更する。削除本数や速度ノルマの達成自体は目的にしない。

`tests/` と `src/` 内のテスト全体を候補とし、実行費用・不安定性・重複の大きい箇所から選別する。固有の重要な保証は代表ケースで残し、共通処理の失敗種別・fieldごとの反復を中心に減らす。製品の機能・公開契約・安全要件は変えない。

本Planningは改訂Planの明示Approve・正式保存と、整合するWork Itemsの個別承認・保存公開までで終了する。実装と以下の検証は、別の新しいImplementationセッションで行う。

## 正式入力・確認済みの現状

- 対象cwd: `/Users/minoru/Documents/mywork/cli-dev/backlog-cli`
- 再計画対象: https://github.com/minorunakamura/backlog-cli/issues/56
- 関連する最終検証・作成基準: https://github.com/minorunakamura/backlog-cli/issues/57
- 正式保存先: `docs/plans/test-infrastructure-performance.md`（本改訂全文で置き換える）。
- 調査時HEAD: `028cbdc4031072a78ce9c9aa382b69826fa49e96`。
- #54（HTTPS fixtureのTLS blocking改善・安全なPTY失敗診断）と#55（実30秒timeoutの6ケース集約）はCLOSEDで、現在のコードにも存在する。本改訂で再実装・再公開・完了認定のやり直しはしない。

要求・判断の一次資料:

1. 本Plan改訂前の `docs/plans/test-infrastructure-performance.md`。Gitの上記HEADにも存在する。旧目的・実測と本改訂の差を確認する履歴資料であり、#56/#57の将来の完了条件は本改訂を優先する。
2. #56/#57の現在のIssue本文。Planning時は両IssueがOPEN、コメントは空。旧本文の同期安定化・保証全維持・固定反復条件は本改訂に合わせてWork Itemsで明示的に更新する。
3. `docs/designs/backlog-cli.md`「JSON・通信・書込み結果」「品質・配布」、`docs/plans/backlog-cli-initial.md`「動作確認方法／TDDと公開境界／自動試験／正式チェック」。製品契約を変更する入力ではない。
4. `CONTRIBUTING.md`「品質確認」「テストと今回の範囲」「editor連携 (#19)」「TUI課題更新 (#21)」「TUIコメント追加 (#22)」、`.github/workflows/quality.yml`、`Cargo.toml`、`rust-toolchain.toml`。
5. `docs/testing/manual-acceptance.md` の「#54」「#55」および「#56」以降の記録。特に「fixture deadlineのstep時間切り分けと例外注入pauseの限定訂正」「editor追加診断の記録」を、未解決事項と部分実装の根拠として参照する。過去の失敗や訂正を成功へ書き換えない。

### 未コミット変更の保護

Planning開始時の変更は次の4ファイル。調査終了時にも内容不変をhashで確認した。Planningでは編集・commit・stash・破棄していない。

| ファイル | Planning時SHA-256 |
| --- | --- |
| `tests/fixtures/tty_shell.py` | `5f9270a11f269c673585bcd5db3d934dfd1410ceb15ba203092d8ae864eb36b3` |
| `tests/tty_shell_fixture.rs` | `c5c8be0103bf02f3ca637ef8d6768c6ffed6ac84b3fea4fec750edd69b2f7b5d` |
| `tests/tui_process.rs` | `f358832d6c0142cb65a6278a1bd25c63ca07b2287a932fc2344fc17118872494` |
| `docs/testing/manual-acceptance.md` | `4d12ab6b0cd3427c88eda8b9454acee98578a4fdc0062b0f73b4edb6be1cd5a9` |

Implementation開始時にHEAD・全差分・untrackedを読み直し、現在の部分実装を正式入力に含める。hashは保護確認の参考であり、将来の開始条件として一致を要求しない。他者の変更があれば保持して影響を確認する。

## 旧Planからの変更・ユーザー決定

ユーザー確認済みの方針は次の2点である。

1. updateだけでなくテスト全体を候補にし、固有の重要な保証は代表ケースで残して、重複する反復を減らす。
2. 次のImplementationで差分ができた時点に、変更箇所のfocused確認と全体試験を各1回行い、所定のチェックを維持する。旧Planの全体固定3回・重点固定10反復・速度ノルマは外す。

これを次の具体策へ落とす。

- 旧「既存シナリオ・assert・全組合せの維持」は、下記6候補の代表化へ変更する。保証が完全に同等とは説明しない。
- 削減対象となる過去cutoffの根因をすべて解明することは完了条件にしない。残す試験の失敗を放置して合格にする変更ではない。
- warm wall中央値150秒以下、timeout group全体40秒以下という性能合否、および指定3試験の10反復を撤廃する。`tests/request_timeout.rs` の各CLIケース29秒以上40秒未満という既存assertは変更しない。
- 正式fmt／all-targets／Clippy／audit／release buildを残す。別途のrelease製品でのPOST／PATCH直接再演は、今回の必須検証から外す。既存の再現手順・fixturesそのものは削除しない。
- これらは#56/#57の検証方針の変更であり、初期Plan・基本設計にある製品契約や、他の機能Issueの受入条件を変更しない。別の設計文書・ADR・保証台帳は追加しない。

## 調査範囲と終了条件

Planningで答える問いは、(a) 高費用な反復がどこにあるか、(b) どの既存assert・共通処理と重なるか、(c) 固有に残す／削る保証は何か、(d) 部分実装と検証をどこまで再利用するか、の4つとした。

`tests/` の全21 Rust suite、commonとfixture一覧、`src/` 全Rustファイルのtest属性・待機・process起動箇所を粗く確認した。内蔵testsは `src/tui.rs` と `src/tui/create.rs` にあり、`src/tui.rs` のPTY試験を高速unitと一括分類していない。全assertを同じ深さで監査していない。

優先3本に加え、editorの機能別反復、28ケースの不正応答、pending read matrix、comment逆順matrix、JSON再現fixture、共通端末所有権試験を深掘りした。採用する6候補について、Rootが判断に使うコード・assert・共通処理を直接確認した。6候補で十分な実起動削減が得られ、さらに広げると固有保証の削減割合が増すため、今回の深掘りを終了した。

Planning中のfull／focused test・失敗待ちの反復・Probeは行っていない。採否に必要な事実は静的に解決できたため、実行時間順位・flake率・原因再現は問いに追加しない。実行確認は下記のImplementation用手順で行う。

## 削減候補ごとの技術判断

以下の略称はこの節内だけの参照用であり、コードの新しい識別子や管理対象を作るものではない。A〜Fはいずれも `tests/tui_process.rs` の既存関数。

### A. updateのeditor失敗

対象: `bl_update_editor_failure_keeps_latest_raw_body_other_fields_and_focus`

- **現在の保証・費用:** nonzero／utf8／unlinkの3回、実HTTPS・bl・PTY・editorを起動。更新前本文、0600、cooked／echo、一時file削除、変更タイトル保持、失敗後focusから確認・送信まで、およびPATCHがsummaryだけであることを確認する。
- **重複先:** `tests/editor.rs::body_failures_keep_original_and_delete_temporary_data` が同3失敗の原文保持・一時file削除を確認。`src/tui.rs::tests::body_editor_roundtrip_and_failures_use_shared_terminal_handoff` が実PTYで失敗・入力専有・端末復元・他欄保持を確認する。create／commentも `src/tui.rs` の `EditorRequest::CreateBody` → `editor::edit_body` を共有する。
- **推奨: 統合・縮小（3→1）。** nonzeroだけを残し、現在のstep・本文／他欄／focus・record・summary-onlyの単一PATCH assertをすべて保持する。関数の全削除や成功試験への追加assert移動は不要。
- **固有に失う保証・影響:** 不正UTF-8／unlinkとupdateフォームとの組合せによる結合確認を減らす。共通editorの原文保持は残るが、その2失敗だけでupdate画面のfocusやdraftに異常が出る回帰の直接検出は減る。非空本文のupdate失敗後保持そのものはnonzero代表で残す。

### B. updateの取消・旧Detail応答

対象: `bl_update_cancelled_loading_and_older_detail_responses_cannot_replace_a_new_form_or_success`

- **現在の保証・費用:** 旧成功／旧403の2回。旧Detail再取得・取消load・新loadを結合し、新title、旧ERRORなし、project3候補、対象GET4回、PATCH1回を確認。旧応答1.4秒、新draft後1.6秒などの待機がある。
- **重複先:** `tests/tui_process.rs::bl_create_cancelled_candidate_load_cannot_fill_a_new_confirmed_target` と `bl_comment_add_cancelled_load_and_reversed_detail_or_comment_reads_cannot_replace_new_draft_or_registration`。`src/tui.rs` の `create_generation` とload taskのabort＋joinを共有する。browse側は `src/tui/browse.rs::Browse::complete` のtarget／generation照合が成功・失敗の分岐より前にある。ただしupdateの全候補loadと書込み経路は完全同一ではない。
- **推奨: 統合・縮小（2→1）。** 旧成功応答の1ケースを残す。取消前の対象GET ordinal2／3の受信待ち、GET4回、新draftの旧ERRORなし、PATCH1回、既存の画面到達・終了／復元・遅延と観測窓を維持する。旧403用の重複loop／fixture分岐だけを除く。
- **最新projectの固有保証を明確化:** 新loadのprojectId=3から、ruleの `GET /api/v2/projects/3` と、project別4候補 `.../issueTypes`・`.../users`・`.../versions`・`.../statuses` を取得することを確認する。project2の同4候補を使わないことも確認する。初期Detail表示に必要な `GET /api/v2/projects/2` は禁止しない。最新GET由来のtitleが表示されたことと、固定 `PATCH /api/v2/issues/B-1` 1回を残す。候補取得だけの新しい別試験は増やさない。
- **固有に失う保証・影響:** update固有の旧403と取消／新フォームの交差ケースを減らす。旧失敗だけが新しい表示やbusyを汚す回帰のupdate専用検出は減る。旧成功と取消後のupdate保護、共通の旧成功／旧失敗代表は残す。全取消保証をproject取得だけで代替済みとはしない。

### C. selection変更と遅い旧Detail

対象: `selection_change_hides_other_body_immediately_and_late_preview_cannot_clear_new_busy_or_failure`

- **現在の保証・費用:** 旧status200／500の2回。選択変更で旧本文を隠し、旧応答が新しいbusy／失敗を壊さないことを確認。付随するcommentsを含む総数11件のassertで10件となった失敗が記録されている。
- **重複先:** `tests/tui_process.rs::comment_selection_clears_old_document_and_late_initial_response_cannot_clear_new_busy` はComments側、`preview_debounces_selection_and_enter_reuses_safely_rendered_detail` はdebounceと成功表示を確認する。世代gateは共通だがDetailのdocument／statusは別であり、これらで全代替しない。
- **推奨: 維持＋検証対象の限定。** 旧200／500の両ケース、本文非表示・新busy・B-3失敗の画面assert、header_delay0.45／0.85秒とpause0.19／0.6秒を維持する。総要求数11だけを削除し、対象Detail GETの受信待ちと件数assertへ替える。
- B-2からB-3へ動かすキーより前に、既存の `wait_request` で `GET /api/v2/issues/B-2` ordinal1を確認する。B-3のbusy観測前にも `GET /api/v2/issues/B-3` ordinal1を確認する。画面条件を併用し、終了後に両method／pathのGETが各1回であることをassertする。現在のfixtureではcommentsは空配列であり、記法用の同対象issue GETは増えない。
- **固有に失う保証・影響:** 付随するcomments等を含む正確な総数11の監視を外す。この試験からは付随要求の増減を検出しなくなるが、旧Detail要求が一度も届かないまま通る試験にはしない。commentsの取得回数・非対話・追加GETの別試験は残す。ユーザー契約でない総数を、旧応答検証の成立条件と混同しない。

### D. createのeditor失敗

対象: `bl_create_editor_failures_keep_other_inputs_and_do_not_change_the_empty_body`

- **現在の保証・費用:** nonzero／utf8／unlinkの3回。空本文・タイトル・cooked・一時file削除・GETのみを確認する。
- **重複先:** `tests/editor.rs::body_failures_keep_original_and_delete_temporary_data`、`src/tui.rs::tests::body_editor_roundtrip_and_failures_use_shared_terminal_handoff`、Aの共通editor経路。成功時の端末所有・resize・原文送信は `tests/tui_process.rs::bl_create_masks_entire_input_and_keeps_editor_body_dates_and_focus_through_resize` にもある。
- **推奨: 統合・縮小（3→1）。** nonzeroだけにし、現在のstepと全assertを残す。
- **固有に失う保証・影響:** utf8／unlink時のcreateフォームとの結合を減らす。その2種だけで入力やfocusが壊れる回帰の直接検出は減る。共通の失敗時原文保持と、create固有の失敗後保持代表は残す。

### E. comment追加のeditor失敗

対象: `bl_comment_add_editor_failures_resize_tiny_and_discard_keep_body_and_field_draft`

- **現在の保証・費用:** nonzero／utf8／unlinkの3回。本文・変更status、極小時の非操作、破棄取消、file削除、GETのみを確認する。
- **重複先:** `tests/editor.rs::body_failures_keep_original_and_delete_temporary_data`、`src/tui.rs::tests::body_editor_roundtrip_and_failures_use_shared_terminal_handoff`、A／Dと同じeditor経路。フォームのtiny／discardまで共通editor試験で代替するわけではない。
- **推奨: 統合・縮小（3→1）。** nonzeroと対応noticeを残し、tiny／discard／status保持・空本文・file削除・非書込みのstepとassertをすべて残す。
- **固有に失う保証・影響:** utf8／unlinkとcommentフォームの交差確認を減らす。その失敗種別だけでdraftや操作保護が壊れる回帰の直接検出は減る。comment固有の保持・保護は代表で残す。

### F. update成功応答の全field不正matrix

対象: `bl_update_incomplete_or_mistyped_success_preserves_detail_and_reports_updated_without_retry`

- **現在の保証・費用:** id／issueKeyだけの疎object、13fieldの欠落・型不正、milestone要素不正の計28回、HTTPS／bl／PTYを起動。同じUpdated確度・Do not resend・旧Detail・PATCH1回・後続GETなしを繰り返す。
- **重複先:** `tests/issue_view.rs::invalid_required_issue_fields_fail_before_any_success_output` は全必須fieldの欠落・型不正・不正named値を実CLIで確認する。`src/service.rs::validate_issue_view` を `src/tui/create.rs::Create::update_display` も呼び、失敗にUpdated確度を付ける。`tests/tui_process.rs::bl_update_failures_preserve_certainty_never_retry_or_refresh_and_cannot_edit_the_result` にはdecode／切断／不完全応答等の確度・再送禁止もある。
- **推奨: 統合・縮小（28→3）。** 既存の疎object `{"id":1,"issueKey":"B-1"}`、完全なupdate_issueからsummaryをfalseにした値、milestoneを `[{"id":5,"name":null}]` にした値だけを残す。疎objectは必須field欠落、summary=falseはscalar型不正、milestone.name=nullはnested不正の代表とする。既存のstep・Updated／再送禁止・旧Detail・単一PATCH・後続GETなしを全3ケースで残す。
- **固有に失う保証・影響:** 全field個別の不正とPATCH成功後の表示／確度を掛け合わせる確認を減らす。validator単独のfield検証は残るが、fieldごとに異なる成功後処理の回帰を直接見つける範囲は狭くなる。現コードは同じvalidator失敗を共通に扱っているため、28回の端末往復より、全field検証＋3つの結合代表を推奨する。

## 今回削減しない候補と理由

- `tests/tui_process.rs::bl_update_invalidates_pending_list_and_comment_requests_and_keeps_the_fixed_detail_target` の8ケース: page／list-refresh／comment-oldest／comment-refreshは異なる取消入口。固定target・history・書込み後の表示保持に役割があり、今回は拡張削減しない。
- `tests/tui_process.rs::bl_comment_add_cancelled_load_and_reversed_detail_or_comment_reads_cannot_replace_new_draft_or_registration` の4ケース: Detail／Commentsの入口、旧成功／旧失敗、登録までの結合代表として維持する。既存15秒予算は延長しない。10反復の対象としては残さない。
- `tests/tui_process.rs::bl_update_local_acceptance_fixture_refreshes_comments_but_preserves_old_list_with_one_patch` と `bl_update_fetches_latest_confirms_only_changes_and_refreshes_only_comments_once`: 共通する成功確認はあるが、前者は文書用JSON fixture・複数field、後者はsummaryのみのform・確認Enter非送信・GET経路に固有assertがある。安易な統合でfixtureの実行可能性を失わないよう今回は維持する。
- `tests/tui_process.rs::search_dialog_direct_reproduction_fixture_exercises_normal_clear_and_cross_without_writes`: focus部分に類似検証があるが、文書用fixtureと実HTTPのnormal／clear／crossを結合する代表。assertだけ削っても起動が減らず、今回は維持する。
- `src/tui.rs::tests::body_editor_roundtrip_and_failures_use_shared_terminal_handoff` の13ケース: `tests/editor.rs` と原文・cleanupは重なるが、raw／noechoを残す子、所有権、親中断、復元失敗の複合エラーは実端末境界固有。端末をrawのまま利用者へ返す不具合を防ぐため維持する。
- `src/tui.rs` の `shell_exits_promptly_with_started_blocking_work_on_quit_interrupt_error_and_panic`、`shell_restores_when_sigint_arrives_during_entry_before_the_first_frame`、`shell_restores_after_draw_errors_panics_and_reports_restore_failures`: 異なる終了経路・初回SIGINT・復元失敗を保証し、通常q終了と同一視しない。
- `tests/request_timeout.rs::body_timeouts_preserve_status_and_write_certainty_in_parallel`: 6ケースは各操作の経路・成功status・確度が異なり、既に最大6並列へ集約済み。実30秒・認証・単一送信・retry禁止のassertを維持する。
- CLI各suite、`tests/service.rs` の原文／入力／確度・TLS/env子process隔離・中断abort＋joinは境界が違う。特に `create_task_abort_joins_without_detached_http_or_later_certainty_changes` の否定観測窓は、後から送信や確度変更がないことの確認なので削らない。
- `tests/body.rs`・`comment_preview.rs`・`safety.rs`・`tui_input.rs`・`tui_shell.rs` とsrcの純粋描画試験: 実process反復削減に比べ効果が小さく、Unicode／secret再構築／原文・入力・layout保証を失うため維持する。`src/tui/create.rs` の候補／選択欄secret保護試験も維持する。

## 実装方針・変更範囲

### #56相当: 根拠を絞ったテスト削減

- A〜Fだけを変更する。総数や関数本数の目標で削除を増やさない。
- これらの現行ケース数は3＋2＋2＋3＋3＋28＝41、採用後は1＋1＋2＋1＋1＋3＝9。通常成功経路の静的見積りで、HTTPS Harness／bl／PTY起動を32組、editor起動を6回減らす。全suiteの実行秒や並列時の短縮率を示す実測ではない。
- 既存関数名は基本的に維持し、不要になったloop・fixture分岐・変数だけ除く。新たな抽象化・framework・test専用公開API・大量の代替試験は作らない。
- `tests/fixtures/tty_shell.py` の未コミット `wait_request` と同snapshot解析は、Bで引き続き使い、Cにも限定適用する。自己試験だけの基盤として残すわけではない。driverのdeadline／exceptionの安全診断、子回収、既存pause／deadlineは維持する。新しい同期APIは増やさない。
- `tests/tty_shell_fixture.rs::tty_shell_waits_for_the_matching_request_ordinal_before_sending_input` を維持する。誤method・誤path・ordinal不足・途中行で送信せず、一致時に進む既存の実PTY回帰を使う。deadline／exceptionの2試験と、exception注入だけpause0・安全reportのassert messageという部分訂正も保持する。
- 部分実装Bの旧403用loopが不要になる範囲だけ、Implementationで整理する。Planningでは消さない。履歴文書にある過去の根拠・失敗・旧Approveは削除しない。

### #57相当: 将来の作成基準と最終検証

`CONTRIBUTING.md` に、今回だけでなく今後の追加・変更・Reviewに使う基準を記す。既存の品質確認コマンドは変更しない。

1. 最初にユーザーに必要な保証と、既存のどのassertを使えるかを確認する。
2. 純粋な入力・変換・表示は既存library／Shell→TestBackend境界を使い、CLIのargv／JSON／exit、HTTPS認証・送受信、signal／端末所有・復元など境界固有の保証に実process／PTYを使う。
3. 共通処理の全失敗種別・全field matrixは適切な境界に置き、機能ごとの端末試験は固有保証を持つ代表ケースを優先する。代表化で削る組合せは正確に説明し、完全代替と偽らない。
4. 要求受信・headers観測・描画・editor復帰・resizeを混同しない。否定・逆順応答の有限観測窓は必要なものを残す。総HTTP件数を固定するなら、その件数自体が保証する契約を説明する。
5. 新しい実時間待機・起動を増やす理由、既存試験との役割分担、focusedコマンドを変更説明・Reviewに短く残す。削減だけの変更に不要なREDや代替テストを増やさない。
6. 開発途中はfocused、最終差分では正式チェックとする。固定反復・性能閾値は必要な問いと予算がある場合にだけ別途決める。skip／ignore・成功retry・timeout延長を代替にしない。

短い `AGENTS.md` からこの節を案内し、ルールを二重管理しない。Implementation開始時に既存AGENTSがあれば上書きせず最小統合する。自動強制のframeworkを追加しない。

文書Reviewでは、純粋な検証／表示、CLI引数／JSON、PTY signal／editor復元の3種類を実在テストへ対応させ、境界・実processの必要性・focusedコマンドが分かることを確認する。削減後も正しい機能説明に必要な場合だけ、CONTRIBUTINGの既存テスト説明を整合する。

実装結果と失敗・未実施事項は `docs/testing/manual-acceptance.md` に#56/#57の新しい追記として記録する。実CIはworkflow設定整合の確認までとし、起動のためのpush／PRをしない。

### 変更する／しないファイル

- テストの主変更: `tests/tui_process.rs`。
- 既存部分実装を保持し、関係する整合・検証に含める: `tests/fixtures/tty_shell.py`、`tests/tty_shell_fixture.rs`。新しいdriver機能の追加は不要。
- 文書: `CONTRIBUTING.md`、短い `AGENTS.md`、`docs/testing/manual-acceptance.md` の追記。
- Planning成果物: 本Planと承認済みのWork Item更新。
- 変更しない: `src/` の製品コード・tests、依存／Cargo.lock、Cargo exports／features、公開API・CLI／TUI契約、HTTPS fixture・timeout group、既存JSON再現fixtures、CI設定、初期Plan・基本設計・READMEの製品契約。新たな必要性が出たら独断で広げない。

## 動作確認方法

### 検証の時点・回数

以下は今回のPlanningでは実行しない。次のImplementationで行う。

- #56相当の変更後に、下記focusedを各1回。途中の変更単位で必要なfocusedは用いるが、成功までの反復はしない。
- #57相当の文書整合まで含む最終差分を固定して、正式チェックを各1回。全体試験は計1回で、focusedの代替にはせず、full3回を各Work Itemで繰り返さない。
- 最終all-targetsには維持したtimeout6ケース、CLI／Service／PTY代表も含まれる。既存manual用2 ignoredはそのまま明記し、新しいskip／ignoreは作らない。
- 製品コードを変更しないため、別のrelease POST／PATCH直接再演、実端末／IME・実Backlog受入の新実施、CI実行は今回の必須にしない。未実施と明記し、過去の合格を新差分の実行証拠へ流用しない。

### focusedコマンド

A〜Fがそれぞれ1試験として計6試験選択されたことを確認する。ケース数はそれぞれ1／1／2／1／1／3である。選択0件を成功にしない。

```sh
cargo test --locked --test tui_process -- \
  bl_update_editor_failure_keeps_latest_raw_body_other_fields_and_focus \
  bl_update_cancelled_loading_and_older_detail_responses_cannot_replace_a_new_form_or_success \
  selection_change_hides_other_body_immediately_and_late_preview_cannot_clear_new_busy_or_failure \
  bl_create_editor_failures_keep_other_inputs_and_do_not_change_the_empty_body \
  bl_comment_add_editor_failures_resize_tiny_and_discard_keep_body_and_field_draft \
  bl_update_incomplete_or_mistyped_success_preserves_detail_and_reports_updated_without_retry \
  --nocapture --test-threads=3
cargo test --locked --test tty_shell_fixture -- --nocapture
cargo test --locked --test editor -- --nocapture
cargo test --locked --lib body_editor_roundtrip_and_failures_use_shared_terminal_handoff -- --nocapture
```

`tests/editor.rs` とsrc共通端末所有権のfocusedは、減らすmode／form反復の受け皿が現在も実行できることの確認であり、新しい代替試験ではない。

### TDD・公開境界と試験の成立確認

今回は製品の新動作を実装せず、既存テストの代表化とassertの限定を行うため、形式的な新規REDは要求しない。driverの既存部分実装に新たな振る舞い修正が必要な場合だけ、既存 `tdd` Skillを読み、step入力→実PTY子の入力・安全な失敗出力／cleanupという既存公開境界で一つのRED→最小修正を行う。内部private APIを新たに公開しない。

- Cは対象GET受信前にキーを送らないことを既存gate回帰で確認し、対象要求のmethod／path／回数と画面assertの両方を確認する。gateを外すと必ずREDになるとは要求せず、そのための新markerや制御frameworkは作らない。
- Bは旧要求を実際に消費してから取消・新loadへ進み、旧成功の観測窓を残す。対象GET4回だけを順序の証拠にしない。
- A／D／Eは入力データ、画面状態、HTTP form／非書込み、cleanupを比較する。単に終了0へ縮めない。
- Fは全field試験をCLI側に残したまま、成功PATCH後のUpdated・旧表示保持・再送禁止まで3代表で確認する。

### 最終正式チェック（各1回）

プロジェクトの既存定義を維持する。release buildを省略しない。

```sh
cargo fmt --all -- --check
/usr/bin/time -l cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo audit --version
cargo audit --file Cargo.lock
cargo build --locked --release --target aarch64-apple-darwin --bins
git diff --check
```

cargo-audit 0.22.2、通常のDB更新、監査前後のCargo.lock hash不変を確認する。警告・DB取得失敗・非0を無条件に許容しない。Planning時のCargo.lock SHA-256は `1d1474e540d5a406df52e7be59ace75a6a4d12f4d13c7360abc94a208229be16`。

HEAD／diff、OS／CPU、Rust、親PythonとPTY用Python、SSL／OpenSSL、RUST_TEST_THREADS、コマンド・終了コード・成功／失敗／ignoredを記録する。all-targetsのwall・suite時間は同じ1回から参考値を記録する。別の性能fullやbuild warm-up測定は追加しない。旧Planの256.10秒等とはworkloadが異なるため、厳密な速度比較・中央値・flake率ゼロの根拠にしない。

### 失敗・変更があった場合の停止条件

- 失敗したコマンドはその場で停止し、試験名・assert・safeな診断・差分との関係を確認する。失敗標本を後の成功で上書きしない。
- A〜Fまたは保持する部分実装に起因する、承認範囲内のテスト整理不備は修正する。修正後の必要な検証は別差分の新結果として区別し、成功までの無変更retryをしない。
- 無関係の既存失敗や新しい製品原因なら、勝手に修正対象へ加えず、別作業が必要として報告し今回の完了判定を停止する。原因不明の正式チェック失敗も完了にしない。
- 製品改変、公開契約変更、大規模移設、新しい基盤、timeout延長が必要ならREPLAN_REQUIREDとして停止する。
- 最終検証後にコード・fixtureが変われば、影響する検証を取り直す。旧差分のReview・Code Review Approveは流用しない。時間不足なら差分・実行結果・未完了項目を保持して引き継ぐ。

## 完了条件

1. A〜Fが指定した代表化となり、削るmode／field組合せだけが除かれ、残す全assert・観測窓・target／書込み／原文・端末復元の保証が維持されている。
2. Bの最新project由来のrule＋4候補と旧project候補非使用、Cの対象GET受信待ち・各1回・旧200／500の両ケースが明確である。付随総数11に依存せず、未受信の旧応答を検証したふりをしない。
3. 未コミットのrequest gate／自己試験／exception注入の局所訂正を保持し、不要となる旧403の重複分岐だけを整理している。過去の失敗・根因未確定・旧判断を成功へ書き換えていない。
4. 指定focused各1回と、最終差分の正式チェック各1回が成功し、失敗・警告・ignored・未実施を正確に記録している。32組の静的起動削減を実秒の達成値や不安定性解消へ読み替えていない。
5. CONTRIBUTINGに将来にも使う保証／境界／重複／待機／focused・正式チェックの基準があり、短いAGENTSから辿れる。3種類の既存テスト例で適用を確認でき、CI設定と矛盾しない。
6. 製品コード・公開契約・依存／lockfile・実行方式は不変。認証・secret非露出・原文保持・単一書込み・retry禁止・書込み確度、端末所有権・中断・復元の代表が残る。
7. 改訂に整合するWork Itemsと記録があり、#56を「過去cutoffの全根因解消」、#57を「性能閾値達成／反復でflake解消」として完了させていない。

## 制約・対象外・残るリスク

- 新framework、test専用公開API、VT emulator／screen cache、常駐server、証明書共有、CI改造、テストの大量移設、新しい永続状態は追加しない。
- skip／ignore、成功までのretry、timeout延長、期待の無差別緩和で失敗を隠さない。代表化は本Planで選んだ組合せだけで行う。
- 実時間timeout、signal、出力失敗、認証／secret、rawとsafe表示、HTTPとServiceの違いは単なる重複と扱わない。
- 代表化で削るmode／fieldと各フォームの交差検出範囲は狭くなる。一方、update固有の非空本文保持・focus・summary-only送信と、取消後の新フォーム保護は代表を残す。全保証同等とも未知のリスク一般をユーザーが包括承諾したとも説明しない。
- 過去のeditor cutoff・startup遅延の内部原因等は未確定のまま残る。後の成功や今回の削減を根因解明にしない。任意のOS負荷・全端末／IME・実Backlog権限を保証しない。
- 最終full1回は回帰確認であって、低頻度flake不存在の証明ではない。release buildは行うが、別途のrelease実操作・実CI・実IME・実Backlogは今回未実施とする。
- Issue #22等の履歴・状態、他機能の受入条件、release／deploy／mergeは変更しない。

## 承認・保存・Work Item受渡し

本改訂全文を `taskflow_plan_review` へ提出し、明示Approve後に同じ正式パスへ保存して全文一致を確認する。Plan承認前に実装やWork Item更新をしない。

保存後に `taskflow-to-ticket` Skillを明示的に読み、正式パスと全文を渡す。#56はテスト整理、#57は将来の作成基準と最終検証という既存Work Itemを再利用する案を基本とするが、分割・依存・保存公開方法はticket Skillで別途ユーザー確認する。各Work Itemから本Planを参照でき、旧条件より改訂内容が優先されることを明記する。#54/#55のCLOSED状態や過去記録は変更しない。

tracker設定・分割・最終本文と保存公開先の承認はPlan Approveと区別する。一部だけ公開できた場合はPlanning完了としない。

両Work Itemの確定参照と本Planの保存を確認した後、更新済み#56と本Planを入力として新しいRootセッションでImplementationを再開するkickoffを示して停止する。既存部分実装を保持し、現在の差分の再確認と新しい検証・Reviewを求める。Worker起動・production code実装・commit／stash／push／merge／release／deployは、このPlanningから自動開始しない。
