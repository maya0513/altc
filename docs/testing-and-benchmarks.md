# テストと性能

品質commandの実装は[xtask](../xtask/src/main.rs)、CI定義は[CI workflow](../.github/workflows/ci.yml)と[benchmark workflow](../.github/workflows/benchmark.yml)です。

## 標準command

```sh
cargo xtask check
cargo xtask gate
cargo xtask docs
cargo xtask eval
cargo xtask perf --check
cargo xtask bench --json target/bench/pipeline.json
```

`check`はformat、Clippy `-D warnings`、workspace全test、doc test、実行可能Markdown、言語評価を実行します。`gate`はさらに98% line coverageと決定的performance ratchetを実行します。

## coverage

長期目標は製品codeのline coverage 100%です。通常gateは98%を下限にします。対象は配布する`altc-compiler`と`altc-cli`で、開発orchestratorの`xtask`はunit testとRed testで検証します。

```sh
cargo install cargo-llvm-cov --locked --version 0.9.0
cargo xtask coverage --minimum 98
cargo xtask coverage --minimum 100
```

到達不能codeの除外やassertionのないtestで数値を作りません。未到達行は`cargo llvm-cov report --show-missing-lines`で確認します。`cargo xtask coverage`はsource変更前のprofileが混入しないよう、計測の直前に`cargo llvm-cov clean --workspace`を実行します。

## 実行可能Markdown

全Markdownのaltc fenceは次のどれかに分類します。

````text
```altc check
```altc run=0
```altc compile_fail=ALTC3037
```altc skip=concrete-kebab-case-reason
````

`docs` commandは相対linkも検査します。skip数は[baseline](doctest-baseline.json)と完全一致させ、増加を拒否します。機能実装でskipが減った場合は内容を確認して`cargo xtask docs --update`でbaselineも下げます。

## 言語評価

[評価corpus](../eval/README.md)は9領域のwritability goldenと12種類のrepair pairを持ちます。broken sourceの診断JSONは完全一致、fixed sourceはstrict C11生成まで成功しなければなりません。意図した診断変更だけを確認後、`cargo xtask eval --update`でsnapshotへ反映します。

[feature showcase](../examples/feature_showcase.altc)は主要言語機能を同じprogramへ統合し、CLI integration testでFFI検証、memory summary、strict C11 build、実行結果、cleanup順を固定します。

## 性能

wall-clock benchmarkはwarm-up後、最低計測時間を超えるまでiterationを倍増します。共有runnerの時間は参考値でありgateに使いません。syntheticな200-function program、feature showcase、複数moduleのtelemetry projectについてanalyze、C生成、pipelineを計測し、JSONとMarkdown summaryはCI artifactとして保存します。

blocking gateは[生成物size baseline](../bench/perf/baseline.json)にある代表fixtureのC source、public header、FFI検証TUを比較します。小さなfixtureに加えてfeature showcaseとtelemetry projectも対象とし、case一覧の差異と2%を超える増加を拒否します。意味のある生成物変更を確認した場合だけ`cargo xtask perf --update`で更新します。

Markdown parser、compile-fail code、link、skip ratchet、diagnostic snapshot、性能ratchet、staged tree展開には、正常例と意図的な失敗を持つself-testがあります。
