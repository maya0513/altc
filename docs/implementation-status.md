# 実行可能v0の実装状況

この文書は非規範的です。現在のコンパイラが受理・生成できる範囲を記録し、言語の
意味を定めるDraft規範仕様とは分離します。

## パイプラインと公開API

Rust workspaceは`altc-compiler`と`altc-cli`からなります。compiler crateは
`analyze`と`emit_c`、診断、effect、memory summary、生成translation unitをRust API
として公開します。

frontendはrowanのlossless CSTに空白、comment、delimiter、byte spanを保持し、そこから
AST、名前解決済みtyped modelへ進みます。top-level宣言を先に収集するため、forward call
と再帰を利用できます。parserはsemicolon、brace、top-level itemを主な同期点として
複数の診断を返します。

## 現在実装済み

- 基本整数・浮動小数型、C ABI数値型、`bool`、`char`、`unit`、`str`
- const/mut binding、non-null/nullable pointer、address取得、dereference
- function、local `let`、module/block `const`、struct、payload enum、`@repr(C)` enum
- block、`if`、`match`、`while`、C風`for`、return/break/continue
- assignment、cast、算術・比較・論理・bit・wrapping・saturating operator
- struct/enum literal、field/pointer-field access、nullable pointer pattern
- `alloc`、`free`、typed `raise[E]` effect、postfix `?`、`try ... catch`
- 通常終了、return、raise、break、continueでLIFO実行される静的`defer`
- `@entry`、`export "C"`、C function用`@c_name`、extern C function、header include、
  `@c_typedef`付きopaque type
- `sizeof`、`alignof`と、C連携用の基本pointer builtin
- direct/transitiveを分けたmemory summaryと、再帰call graphのfixed point
- statusとout parameterによるraising ABI、export header、extern FFI検証TU
- executableとstatic libraryのhost向けbuild、およびbuild後のrun
- 左から右の式評価順を保つtemporary、payload enumのtagとunion lowering、`#line`

生成Cは`-std=c11 -pedantic-errors -Wall -Wextra`を前提にしています。専用runtime、GC、
例外runtime、runtime cleanup stack、暗黙allocationは使用しません。

## 初回対象外

- import、複数module、project設定、cross compilation、incremental compilation、optimizer
- generic、trait、impl、method解決、function pointer
- slice、array、range-for、`Option` / `Result`、checked builtin
- extern macro/variable/constant/struct/enum、variadic、callback
- `fmt`、`test`、`doc`、`lsp`

対象外の既知構文は、認識可能な場合にはsyntax errorではなく`ALTC4001`の未実装診断で
拒否します。v0のtarget ABIとlayout queryは、CLIを動かしているhost C toolchainに限定
されます。

## CLIの動作

`check`はfrontendだけを検査し、extern contractを未検証として扱います。`check --ffi`
だけがcheck中にC compilerを起動します。`build`はexternが存在すればFFI検証を省略せず、
その後compile/linkまたはarchiveします。`run`は`--`以降を生成したprocessへ渡し、その
終了codeを返します。

通常成功時はquietです。human診断はsource抜粋とbyte/line/column情報を持ち、JSON
診断・summary・tool出力はNDJSONとして1 recordずつ出力されます。CLI usage errorは2、
frontend/backend/toolchain failureは1です。
