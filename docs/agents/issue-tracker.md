# Issue tracker

## Tracker

GitHub Issues

## Repository

minorunakamura/backlog-cli
https://github.com/minorunakamura/backlog-cli
Host: github.com

## 作成・取得

- 公開 gh CLI を使い、常に minorunakamura/backlog-cli を明示する。
- 作成: gh issue create --repo minorunakamura/backlog-cli --title <title> --body <body> --label ready-for-agent
- 取得: gh issue view <number-or-url> --repo minorunakamura/backlog-cli --json number,url,title,body,comments,labels,state
- ready-for-agent 対応名は ready-for-agent。必要な1つだけ別途承認後に準備する。

## 依存関係

- blockerを先に作成し、本文のBlocked byに実Issueリンクを記す。
- 確認済みの公開gh apiとGitHub GraphQLのaddBlockedByで、承認されたnative blocking relationshipを設定する。
- 元Issueがないため、Parentやsub-issue関係は追加しない。
- 認証・権限・通信の失敗では停止する。元Issueの本文・状態は変更しない。
