# 次期開発計画と進捗

最終更新は2026-09-02です。この文書は「診断改善・100% coverageから複数module対応まで」の作業順序、現在の到達点、残作業を一か所で追跡します。observableな言語契約はテスト、それ以外の現在挙動は実装を正本とし、この文書は進行管理のために使います。

## 作業順序

```text
診断改善・100% coverage
→ semantics/codegen refactor
→ array/slice/range-for
→ 実用coreのextern C拡張
→ root file型module/import
→ 中規模統合project・評価・文書更新
→ 100% gateを含む最終検証
```

P0とP1は発見した段階で最小probeと回帰testを追加して解決し、次段階へ持ち越しません。各段階では既存CLI、`analyze` / `emit_c`、root single-fileの生成物を維持します。root `README.md`は今回の変更対象に含めません。

## 現在の進捗

| 段階 | 状態 | 到達点 | 残作業 |
| --- | --- | --- | --- |
| 診断と品質gate | 進行中 | mutability、型不一致、effect、`?`、nullable dereference、entry、matchへhelpと宣言位置labelを追加し、repair snapshotを更新した。2026-09-02時点の製品line coverageは99.04%。 | 残る111行を有効なtestから実行し、coverage、CI、wrapper、`gate`の既定値を100%へ上げる。 |
| compiler内部refactor | 進行中 | typed semantic model、C type、writer、project loaderを独立moduleへ分離し、解決済みerror型とmodule情報を後段へ渡すようにした。 | 型/place/pattern、effect/control、memory fixed pointと、式/control cleanup loweringの実行本体をdata所有権に沿ってさらに分割する。 |
| array、slice、range-for | 実装済み | `[T; N]`、`[]const T` / `[]mut T`、literal、index、slice range、`.len`、`.get`、slice builtin、array/sliceのrange-for、sole-array C wrapperを実装した。 | 動的boundsと生存期間は今回の設計どおりuncheckedであり、今後も暗黙runtimeを追加せず契約を明記する。 |
| extern C実用core | 実装済み | extern global object、struct/enum、C function pointer/callback、C spelling/layout検証、memory eventを実装した。global、field、enum、callbackの不一致は個別のRed testで拒否する。 | callback契約違反後の派生診断を抑制し、最初の修正をさらに明確にする。 |
| module/import | 実装済み | root親directoryをsource rootとするlocal import、qualified public access、cycle/重複/privacy検査、単一C11 TU生成、純粋な`ModuleLoader` APIを実装した。 | alias、wildcard、re-export、package、remote dependency、複数TUは対象外として維持する。 |
| 統合project・評価・文書・性能 | 実装済み | [telemetry pipeline](../examples/telemetry_pipeline/README.md)、writability 9領域、repair 12系統、監査所見、module対応benchmarkと生成物size baselineを追加した。 | 新しいP0/P1/P2を継続して[実装監査](implementation-audit.md)へ記録する。 |
| 最終検証 | 進行中 | `cargo xtask check`と`cargo xtask perf --check`は2026-09-02に成功した。strict C11、FFI Red test、CLI project testは標準test内で成功している。 | 100% coverageを達成後、既定値を更新した`cargo xtask gate`を成功させる。 |

## 実装済みの回帰契約

- sequenceはliteral推論、nested array、const length、zero length、mutability、known out-of-bounds、`.get`、empty slice、range-for、defer、評価順をcompilerとCLI testで検証する。
- FFIは正しいheaderを実際にcompile/linkし、global constness、field layout/type、enum spelling、callback signatureの不一致を`ALTC6101`として拒否する。
- moduleはdiamond graph、missing module、cycle、duplicate import、privacy、import先の構文errorと`@entry`、cross-module effect、決定的C生成、全CLI commandを検証する。
- [telemetry pipeline](../examples/telemetry_pipeline/)は5つのaltc moduleとC libraryを組み合わせ、array/slice、range-for、struct/enum/match、pointer/nullability、alloc/free、typed raise、nested defer、extern global/layout、callback、C exportを一つのbuild/run経路で使う。
- 評価結果と発見事項は[言語評価](../eval/README.md)および[2026-09-02所見](../eval/findings/2026-09-02-r3.md)へ固定する。

## 最新の検証結果

2026-09-02に次を確認しました。

- `cargo xtask check`: 成功。Clippy `-D warnings`、workspaceの100 test、文書test、実行可能Markdown、writability 9領域、repair 12系統を含む。
- `cargo xtask coverage --minimum 0`: 成功。製品codeは11,542行中111行未到達、line coverage 99.04%。
- `cargo xtask perf --check`: 成功。既存fixtureとtelemetry projectの生成C、header、FFI TUは固定baselineの許容範囲内。
- root `README.md`: 差分なし。

100% coverageはまだ完了条件を満たしていません。現在の通常gateは98%を維持し、100%へ到達した変更で`xtask`、CI、`scripts/coverage.sh`、関連文書を同時に更新します。

## 次に行う作業

1. coverage reportの未到達行を利用者から観測可能な経路、防御経路、真に不要な分岐へ分類する。
2. filesystem/toolchain failure、parser recovery、backend防御経路をtest doubleと一時fixtureから実行し、assertionで結果を固定する。
3. 責務分割を進める際はdiagnostic snapshotと生成Cのbyte比較を維持し、performance baselineを不用意に変更しない。
4. line coverage 100%到達後にすべての既定値を100へ変更し、self-testが99%以下を確実に拒否することを確認する。
5. `cargo xtask gate`を最終実行し、この文書の状態と実測値を更新する。

## 完了条件

- P0/P1が未解決で残っていない。
- `cargo fmt --check`、Clippy `-D warnings`、全test/doc/eval、strict C11、FFI Red testが成功する。
- 製品line coverageが100%で、coverage gate自体のRed testも成功する。
- 代表生成物の2% size ratchetとwall-clock benchmarkが新しい統合projectを含む。
- 実装、テスト、[現行言語](language.md)、[compiler API](compiler.md)、[CLI](cli.md)、[テストと性能](testing-and-benchmarks.md)が一致する。
