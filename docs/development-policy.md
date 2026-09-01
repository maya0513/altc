# altc開発方針

この文書は、altcを変更するときの判断基準と完了条件を定めます。

## 正本と文書

現在のobservable behaviorはテスト、その他の現在挙動は実装を正とします。文書はその要約であり、
独立した将来仕様ではありません。

1. 振る舞いを変更するときは、期待結果を表す成功testと必要な失敗testを同じ変更へ含める。
2. 実装と文書が矛盾したら、まずtestで意図を確定し、文書を実装へ合わせるか削除する。
3. 未実装の構想を現行言語の契約として記載しない。探索はissueや評価所見に置く。
4. Markdown中のaltc例は実行契約を付け、CIで解析・生成・実行または診断codeを検証する。

## 判断の優先順位

1. 誤ったC、ABI、実行結果を黙って生成しない。
2. sourceの意味、effect、memory access、C toolchainとの境界を隠さない。
3. crashや正しいprogramの拒否を防ぐ。
4. 診断を、利用者が次の修正を選べる内容にする。
5. その上で構文、性能、実装保守性を改善する。

P0は誤生成・誤受理・誤実行などのsilent wrong、P1はcrash・正しいcodeの拒否・修正不能な
診断、P2は新機能・探索・通常refactorです。P0を発見した変更には最小再現testと修正を必ず
含めます。

## 実装規則

- 対象外構文を似た別構文として受理せず、安定した`ALTCxxxx` codeと正しいspanで拒否する。
- 診断は内部pass名より、利用者が行う修正を先に伝える。
- C backendは左から右の評価順、defer、typed error、公開C ABIを保持する。
- snapshot、skip数、生成物sizeのbaselineは差分の意味を確認して明示的に更新する。
- build生成物は`target/`へ置き、repositoryへ追加しない。
- dirty worktreeでは無関係な変更を保持し、要求範囲だけを編集する。

## tool version

通常開発とCIは、作業時点で確認できる最新安定版のtoolを使用します。Rust stableは
`rust-toolchain.toml`、外部toolはCIと開発commandでversionを一致させます。GitHub Actionsも
保守されている最新majorを使用します。version更新時はlockfileを更新し、完全gateを通します。

MSRV jobだけは互換性検査のため、意図的に`Cargo.toml`の`rust-version`を使用します。これは
通常toolchainを古いversionへ固定するものではありません。

## 変更別の検証

- lexer/parser: token、UTF-8、未終了入力、precedence、recovery、複数診断。
- semantics/effect: 成功例と期待するcode・spanを持つ失敗例、再帰fixed point。
- backend/ABI: strict C11 compile、実行結果、評価順、全退出pathのdefer、C client。
- CLI/toolchain: human/NDJSON、終了code、外部command失敗、生成file。
- 文書: link、全altc fence、skip baseline。
- 性能: wall-clock値は記録し、blocking判定は決定的な生成物sizeで行う。

commit前は`cargo xtask check`、merge可能性を確認するときは`cargo xtask gate`を実行します。
