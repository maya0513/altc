# テスト、カバレッジ、ベンチマーク

この文書は非規範的です。実行可能v0の品質計測方法と、比較の基準値を記録します。

## テストカバレッジ

行カバレッジの長期目標は100%です。ただし、到達不能コードを除外属性で隠したり、
assertionを持たないテストで数字だけを上げたりはしません。正常系、診断、parser recovery、
effect・memory summary、C11 lowering、CLIとtoolchain失敗を振る舞いとして検証します。

2026-09-01時点の`cargo llvm-cov 0.9.0`による基準値は次の通りです。

| 指標 | 到達値 |
| --- | ---: |
| line | 98.49%（8,014行中7,893行） |
| function | 97.97%（493個中483個） |
| region | 97.86%（12,741領域中12,468領域） |

通常の回帰下限は98%です。100%目標も同じscriptで確認できます。

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
sh scripts/coverage.sh
ALTC_COVERAGE_MINIMUM=100 sh scripts/coverage.sh
```

最後のコマンドは未到達の間は失敗することが正しい動作です。未通過箇所の確認には
`cargo llvm-cov report --show-missing-lines`を使います。

## Compiler pipeline benchmark

benchmarkは外部frameworkに依存せず、`Instant`、warm-up、`black_box`を使います。
所要時間が基準を超えるまでiterationを2倍にするため、極端に短い一回だけを測りません。
入力は実fixtureの496-byte programと、200関数を持つ12,433-byte synthetic programです。

再実行コマンドは次の通りです。

```sh
cargo bench -p altc-compiler --bench pipeline
```

2026-09-01の基準値です。絶対値は同一machine上の回帰比較にのみ使用します。

| benchmark | ns/op | throughput | iterations |
| --- | ---: | ---: | ---: |
| `analyze/small` | 178,274 | 2.65 MiB/s | 4,096 |
| `analyze/medium` | 4,027,307 | 2.94 MiB/s | 256 |
| `emit-c/medium` | 2,137,315 | 5.55 MiB/s | 512 |
| `pipeline/medium` | 6,133,017 | 1.93 MiB/s | 128 |

計測環境はAMD Ryzen 7 7735HS（8 core / 16 thread）、x86_64 Linux on WSL2、
Rust 1.93.0、`bench` profileです。`pipeline/medium`はanalyzeからC11生成までを含み、
C compilerの起動時間は含みません。
