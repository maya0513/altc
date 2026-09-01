# altc開発方針

この文書は、人間とcoding agentが同じ判断基準でaltcを変更するための入口です。
observableな言語契約の正本はテスト、その他の現在挙動の正本は実装です。文書が矛盾する
場合は文書を修正または削除します。詳細は`docs/development-policy.md`を参照してください。

## 判断の優先順位

1. 黙って誤ったCや実行結果を生成しない。
2. source上の意味、C ABI、memory/effect costを正直に表す。
3. 構文や実装上の利便性を高める。

誤生成、誤受理、誤った実行結果はP0です。crash、正しいprogramの拒否、修正方法が
分からない診断はP1です。新機能、探索、通常のrefactorはP2です。P0を発見した変更は、
最小の再現テストと修正を同じ変更に含めます。

## 実装規則

- 診断は内部pass名より、利用者が行う修正を先に説明します。安定した`ALTCxxxx` codeと
  正しいsource spanを付けます。
- 対象外構文を別の意味へ黙って変換せず、`ALTC4001`などの明示的な診断で拒否します。
- C backendはaltcの左から右の評価順、defer、typed raise、公開C ABIを保持します。
- 言語例を変更したらfenceの`check`、`run`、`compile_fail`、`skip`分類も更新します。
- snapshotやperformance baselineの更新は、意味のある変更を確認して明示的に行います。
- build生成物は`target/`に置き、gitへ追加しません。
- 通常開発とCIは最新安定版のtoolを使い、MSRV jobだけは互換性検査用のversionを使います。

## 標準コマンド

```sh
cargo xtask check
cargo xtask gate
cargo xtask docs
cargo xtask eval
cargo xtask perf --check
cargo xtask bench
```

commit前の正本は`cargo xtask check`、CI相当の完全確認は`cargo xtask gate`です。テストは
まだ十分短いため、変更影響選択をせず毎回全件を実行します。詳しい手順は
`CONTRIBUTING.md`を参照してください。
