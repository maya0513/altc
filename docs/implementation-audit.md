# 実装監査と既知課題

最終更新は2026-09-01です。テストと実装を正本として、coverageの未到達行、意味解析とC backend
の責務境界、CLIの外部toolchain経路、strict C11での統合実行を監査しました。将来仕様の一覧では
なく、再現できた問題と次に実行可能な保守課題を記録します。

## この監査で解決した問題

| 優先度 | 問題 | 解決と回帰契約 |
| --- | --- | --- |
| P0 | `ptr_addr[i32](pointer)`と`ptr_as_const[i32](pointer)`が型引数を黙って無視していた | 型引数なし・値引数一つを意味解析で強制し、`ALTC3019`で拒否する。[repair case](../eval/repair/pointer-builtin-type-arguments/)とcompiler unit testで固定した。 |
| P1 | `try` bodyのloopなどにraising callが入ると、C backendのAST再走査がerror型を発見できず、正しいprogramのC生成に失敗し得た | 意味解析で解決したerror型をexpression IDごとに保持し、backendはその結果だけを使用する。compiler unit testと[統合example](../examples/feature_showcase.altc)のloop内`?`で固定した。 |
| P1 | `u64`最大値とsigned最小値を裸のC整数literalとして生成し、strict C11 compilerが拒否した | 型付き`UINT64_C` / `INT64_C`と各`*_MIN` macroへloweringする。module-level `const`を含むunit testと統合exampleを`-Werror`でbuild/runする。 |
| P1 | source変更前のcoverage profileが残ると、旧instrumentationがmergeされてgateが偽の低下を報告した | `cargo xtask coverage`が計測前にworkspace profileを必ずcleanする。command引数をxtask unit testで固定した。 |
| P2 | primitive型解決が意味解析とbackendで重複し、CLIのbuild処理も検証・compile・linkを一関数で持っていた | primitive型解決を意味解析側へ集約し、CLIをartifact検証、FFI検証、object compile、link/archiveのhelperへ分割した。公開APIとCLI contractは変更していない。 |

## 未解決の問題

### P1: 診断の修正手順が不足している

[repair corpus](../eval/repair/)の多くは正しいcodeとspanを返しますが、`help`が空で、宣言位置を
secondary labelとして示しません。たとえばimmutable bindingへの代入では、利用者が行う編集
「宣言に`mut`を追加する」を直接提示できていません。頻度の高い診断からhelpと関連spanを追加し、
snapshotを意図的に更新する必要があります。

### P2: 意味解析とbackendのfileが大きい

`semantics.rs`と`codegen.rs`は、型検査、effect/control検査、memory event、C ABI、式lowering、cleanup
挿入をそれぞれ大きなfileに保持しています。今回のような同じ意味の再推論を防ぐため、次の分割は
ASTの形ではなくpassの所有dataを境界にします。

1. 型・place・pattern検査
2. effect/controlとmemory summary
3. C ABI/type layout
4. expression/control-flow lowering

分割時は公開APIを変えず、既存unit testと統合exampleを先に通したまま小さく移動します。

### P2: coverage 100%は未達

通常gateは製品codeのline coverage 98%を要求し、長期目標は100%です。未到達行にはfilesystem・
toolchain failureの防御経路とparser/backendの回復経路が残っています。到達不能扱いで除外せず、
observableな失敗をassertするtestを追加します。現在値は`cargo xtask coverage --minimum 100`で
継続確認します。

### P2: 単一file v0の機能制限

複数module、generic、slice/array、callbackを含む広いC surfaceなどは未実装です。これは現行
実装の欠陥を隠すものではなく、[現行言語](language.md#現在対象外)が明示する製品範囲です。
実装するときは、未実装診断を成功testへ置換してから文書を更新します。

## 継続監査の入口

- 正しさ: `cargo xtask check`
- strict C11を含む統合例: `cargo test -p altc-cli --test cli checks_and_runs_the_integrated_feature_showcase`
- coverage: `cargo xtask coverage --minimum 98`
- 生成物size: `cargo xtask perf --check`
- wall-clock参考値: `cargo xtask bench --json target/bench/pipeline.json`

新しい問題は最小probe、P0/P1/P2、期待結果、回帰testをこの文書または
[言語評価所見](../eval/findings/)へ追加します。
