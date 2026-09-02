# altcドキュメント

このディレクトリは、現在のコンパイラとCLIが実際に提供する機能だけを説明します。構想上の将来仕様は含めません。

## 正本

言語とtoolingの正本は[テスト](../tests/)と[実装](../crates/)です。テストは維持するべきobservable behaviorを固定し、実装はそれ以外の現在の挙動を定めます。文書と食い違う場合は文書が古いものとして修正または削除します。文書だけを根拠に未実装機能を追加したり、既存のtest contractを変更したりしません。

## 現行文書

| 文書 | 内容 |
| --- | --- |
| [現行言語](language.md) | 現在受理される型、宣言、式、effect、C連携と対象外機能 |
| [CLI](cli.md) | `check`、`emit-c`、`build`、`run`と共通option |
| [compiler](compiler.md) | 公開Rust API、解析pipeline、C11 backend |
| [実装監査](implementation-audit.md) | 解決したP0/P1、既知課題、継続監査の入口 |
| [次期開発計画と進捗](development-progress.md) | 複数module対応までの作業順序、到達点、残作業、最新の検証結果 |
| [テストと性能](testing-and-benchmarks.md) | 標準gate、coverage、評価corpus、benchmark |
| [開発方針](development-policy.md) | 正本、優先順位、変更手順、tool version方針 |
| [言語評価](../eval/README.md) | writabilityとrepair corpus、評価rubric、所見 |
| [統合examples](../examples/README.md) | 主要言語機能を組み合わせたbuild/run可能なprogram |
| [contribution guide](../CONTRIBUTING.md) | contributor向けの実行手順 |

文書中のaltc codeは`cargo xtask docs`で解析・C11生成・実行または診断codeまで検査します。
