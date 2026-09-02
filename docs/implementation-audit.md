# 実装監査と既知課題

最終更新は2026-09-02です。テストと実装を正本として、診断、array/slice、extern C、module graph、C backend、CLI、評価基盤を監査しました。将来仕様の一覧ではなく、再現できた問題と次に実行可能な保守課題を記録します。

## この監査で解決した問題

| 優先度 | 問題 | 解決と回帰契約 |
| --- | --- | --- |
| P0 | `ptr_addr[i32](pointer)`と`ptr_as_const[i32](pointer)`が型引数を黙って無視していた | 型引数なし・値引数一つを意味解析で強制し、`ALTC3019`で拒否する。[repair case](../eval/repair/pointer-builtin-type-arguments/)とcompiler unit testで固定した。 |
| P0 | import先のannotationなしextern関数を内部module名へmangleした結果、生成FFI TUが存在しないC symbolを検証していた | 内部symbolだけをmangleし、元のC名を暗黙のC spellingとして保持する。`stdlib.h`の`malloc`と明示`@c_name`の両方をproject unit testで固定し、[telemetry project](../examples/telemetry_pipeline/README.md)を実libraryへlinkする。 |
| P1 | moduleをまたぐmemory summaryが合成sourceのoffsetを表示し、元fileを失っていた | direct/transitive eventへfileを保持し、module-local byte spanへ戻す。project unit testとCLIのNDJSON/human表示で固定した。 |
| P1 | `try` bodyのloopなどにraising callが入ると、backendのAST再走査がerror型を発見できなかった | 意味解析で解決したerror型をexpression IDごとに保持し、backendはその結果だけを使用する。[feature showcase](../examples/feature_showcase.altc)で回帰する。 |
| P1 | `u64`最大値とsigned最小値を裸のC整数literalとして生成し、strict C11 compilerが拒否した | 型付き`UINT64_C` / `INT64_C`と各`*_MIN` macroへloweringし、`-Werror`実行testで固定した。 |
| P1 | source変更前のcoverage profileが残ると旧instrumentationがmergeされ、gateが偽の低下を報告した | coverage計測前にworkspace profileを必ずcleanし、command引数をxtask unit testで固定した。 |
| P2 | 頻出するmutability、型不一致、effect、`?`、nullable dereference、entry、match診断が修正方法や宣言位置を示さなかった | 具体的な`help`とsecondary labelを追加し、repair JSONをfile・byte spanまで完全一致で更新した。 |
| P2 | primitive型、semantic result、C出力処理が大きなfileへ混在していた | typed semantic model、C type、writer、project loaderを独立moduleへ分け、公開APIと既存生成物を維持した。 |

## 現在の制約と継続課題

### P2: callback contractの診断cascade

[callback repair](../eval/repair/callback-contract/)は本質的な`ALTC3214`に加え、C ABI unsafeとraising extern未対応の派生診断も返します。誤受理や誤生成ではありませんが、最初の修正をより明確にするため、無効なcallback effectを検出した後の派生検査を抑制する余地があります。snapshotは現在の三診断を意図的に固定しています。

### P2: 動的boundsと生存期間は検査しない

array/sliceの既知範囲外はcompile errorですが、runtime index/rangeはuncheckedです。raw pointerから作ったsliceのdangling、mut alias、動的な`start <= end <= len`も検証しません。暗黙runtimeやborrow checkerがあるように見せず、[現行言語](language.md#型とdata)でundefined behaviorの境界を明記しています。

### P2: project modelはlocal root限定

moduleはroot親directoryからだけ解決し、alias、wildcard、re-export、package、remote dependency、複数生成TUを持ちません。現在の`ModuleLoader` APIとin-memory testは、将来のresolverをCLI filesystem規則と分離するための境界です。

### P2: 意味解析とcontrol loweringの追加分割

semantic modelとC writer/typeは分離しましたが、型/place/pattern、effect/control、memory fixed pointの実行本体と、式/control cleanup loweringの実行本体にはまだ大きなfileが残ります。次の移動はdata所有権を境界にし、diagnosticと生成Cのbyte比較を保ったまま行います。

### P2: coverage 100% gateへの未到達経路

新機能を含む製品codeの全行を100%実行するため、parser recovery、filesystem/toolchain failure、backend防御経路を引き続きtest doubleと一時fixtureから実行します。到達不能扱いで除外せず、`cargo xtask coverage --minimum 100`で未到達行を確認します。

## 継続監査の入口

- 正しさ: `cargo xtask check`
- single-file統合: `cargo test -p altc-cli --test cli checks_and_runs_the_integrated_feature_showcase`
- module/実library統合: `cargo test -p altc-cli --test cli checks_emits_builds_and_runs_the_module_telemetry_project`
- coverage: `cargo xtask coverage --minimum 100`
- 生成物size: `cargo xtask perf --check`
- wall-clock参考値: `cargo xtask bench --json target/bench/pipeline.json`

新しい問題は最小probe、P0/P1/P2、期待結果、回帰testをこの文書または[言語評価所見](../eval/findings/)へ追加します。
