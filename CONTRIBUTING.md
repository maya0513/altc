# altcへのcontribution

altcはRustで実装され、hostのstrict C11 toolchainをbackendとして使います。必要なのは
Rust 1.98.0、`cc`、`ar`です。MSRV 1.85.0もCIでcompile可能性を検査します。言語とtoolingの
正本、優先順位、tool更新方針は[開発方針](docs/development-policy.md)にあります。

## 開発ループ

```sh
cargo xtask check       # fmt、clippy、test、docs、eval
cargo xtask gate        # 上記にcoverageとperformance ratchetを追加
cargo xtask bench       # wall-clock値。回帰gateには使わない
```

`cargo xtask coverage --minimum 100`で長期目標の100%を確認できます。通常gateは、到達不能
箇所を除外して数字だけを作らないため、現在の回帰下限98%を使います。coverage対象は配布
crateの`altc-compiler`と`altc-cli`で、開発用`xtask`はunit/Red testで別に検証します。

任意のpre-commit hookは次で有効化できます。

```sh
cargo xtask hooks install
```

hookはworking treeではなくGit indexを一時directoryへ展開し、実際にcommitされる内容へ
`cargo xtask check`を実行します。解除するときは`git config --unset core.hooksPath`です。

## 変更と検証

- lexer/parser: 正常tokenに加え、未終了token、recovery、複数診断を追加します。
- semantics/effect: 成功例と、期待する診断code・spanを持つ失敗例を追加します。
- C backend: 生成文字列だけでなく、strict C11 compileと可能なら実行結果を検査します。
- 診断変更: `eval/repair`のbroken/fixed pairを追加し、意図的な更新だけ
  `cargo xtask eval --update`でsnapshotへ反映します。
- 性能変更: `cargo xtask perf --check`で生成物の肥大化を確認し、意図的な場合だけ
  `cargo xtask perf --update`でbaselineを更新します。

## 文書中のaltc code

すべてのaltc fenceには実行契約が必要です。

````text
```altc check
```altc run=0
```altc compile_fail=ALTC3037
```altc skip=generic-not-implemented-in-v0
````

`skip`は理由を必須とし、総数は増加しないratchetです。将来機能を実装したら`skip`を
`check`または`run`へ変え、改善を確認して`cargo xtask docs --update`で基準値も下げます。

## Issueの分類

- P0: 誤生成、誤受理、誤った実行結果など、利用者が気づけない破壊。
- P1: crash、正しいcodeの拒否、行動可能でない診断、resource exhaustion。
- P2: 新機能、探索、将来設計。

種類（bug/enhancement/refactor/performance）と優先度は独立です。大きな項目の親issueは
索引にし、実装可能な単位を子issueへ分けます。
