# backlog-cli

Apple Silicon Mac向けのBacklog TUI `bl` / 非対話CLI `bl-cli`（Rust）。

現在は初期開発基盤のみです。両binaryの `--help` / `-h` と
`--version` / `-V` を利用できます。設定・API接続・TTYは不要です。
Backlog操作やTUI画面はまだ実装していません。

```sh
cargo run --locked --bin bl -- --help
cargo run --locked --bin bl-cli -- --version
```

開発環境と品質確認は [CONTRIBUTING.md](CONTRIBUTING.md)、
実装方針は [承認済みPlan](docs/plans/backlog-cli-initial.md) を参照してください。
