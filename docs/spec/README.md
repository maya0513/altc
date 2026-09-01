# altc Draft規範仕様

## 位置付け

このディレクトリは、altcの構文、意味、診断可能な制約、明示公開ABIを定義する唯一の規範的な文書です。正式リリース前のDraftであるため、理由を記録した仕様改訂は許されますが、実装は改訂前にこの仕様と異なる動作を暗黙に採用してはなりません。

[コンセプト](../concept.md)、[設計資料](../design/compiler-architecture.md)、[対照例](../examples/README.md)は、この仕様を説明または実現するための非規範的な文書です。矛盾する場合は本仕様を優先します。

## 規範範囲

次は規範的です。

- altcソースの字句、型、式、宣言、制御フロー、effectとmemory access summaryの意味
- コンパイル時に拒否すべきプログラムの条件
- `export "C"`、`@repr(C)`、Cヘッダ生成について明記したABI
- CLIのコマンド能力、診断情報、LSPが公開する意味情報

次は、明記されない限り非規範的です。

- 生成Cの内部symbol名、一時変数名、helper名
- 非公開型・非公開関数のC spellingと等価な表現方法。object sizeとalignmentは選択されたimplementation-defined layoutとして観測可能
- 等価なC11のうち、どの分岐、loop、temporaryを選ぶか
- inlining、dead-code elimination、重複したmonomorphizationの統合
- コメントの正確な出力位置と生成Cの空白

処理系へ選択を許す事項には`implementation-defined`と記載します。処理系は選んだ動作を利用者から確認できる形で文書化しなければなりません。

## 仕様索引

1. [字句構造](lexical-structure.md)：encoding、コメント、identifier、literal、keyword
2. [文法](grammar.md)：source file、宣言、型、式、patternの規範grammar
3. [型と値](types-and-values.md)：基本型、C ABI型、pointer、array、slice、struct、enum、変換、layout
4. [Language builtin](builtins.md)：layout query、pointer操作、検査付き数値操作
5. [宣言とmodule](declarations-and-modules.md)：binding、const、function、module、公開、entrypoint、test
6. [式と制御フロー](expressions-and-control-flow.md)：評価順、operator、block、分岐、loop、match、`?`、defer
7. [effect、memory access、error](memory-effects-and-errors.md)：資源effect、memory access summary、typed raise、明示伝播、handler、公開C lowering
8. [generic、trait、method](generics-traits-and-methods.md)：静的抽象化とmonomorphization
9. [C連携](c-interop.md)：extern、header、FFI検証、macro、callback、生成header、公開ABI
10. [targetとundefined behavior](target-and-undefined-behavior.md)：target要件、layout依存、低レベル操作の未定義動作
11. [tooling](tooling.md)：CLI、diagnostic、LSP、formatter、test、doc

用語は[用語集](../glossary.md)を参照してください。

## 表記

仕様中の`必須`と`禁止`は適合実装への要件を表します。`可能`は許可された動作を表します。コード例の`// error:`は、その行を含むプログラムをコンパイラが拒否すべきことを表します。

altcのソースファイル拡張子は`.altc`です。C11 backendが外部へ保存するsourceとheaderには`.c`と`.h`を使います。
