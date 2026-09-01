# altc compiler

実装は[altc-compiler crate](../crates/altc-compiler/src/lib.rs)です。内部構造を説明する文書より、公開型とtestを正とします。

## pipeline

```text
UTF-8 source
  -> lexer + rowan lossless CST
  -> recovery可能なparser + AST
  -> top-level宣言収集
  -> 名前・型・control/effect検査 + memory summary fixed point
  -> C11 source + optional public header + optional FFI verification TU
```

lexerはcomment、空白、delimiterとbyte spanをCSTへ保持します。parserはsemicolon、brace、top-level itemを同期点として複数診断から回復します。意味解析は宣言をbodyより先に収集し、forward callと再帰を許可します。

backendはaltcの左から右の評価順をtemporaryで保持し、payload enumをtagとunion、nullable pointerをC pointer、`str`をpointerとbyte lengthへloweringします。raising functionは内部でもstatusとout parameterを使い、defer cleanupは退出pathへ静的に挿入します。runtime library、GC、例外runtime、cleanup stackはありません。

backendは名前や型をASTから再推論せず、意味解析済みmodelを使用します。たとえば`try`がcatchするerror型はexpression IDに対応付けて保持するため、raising callがloopや条件式の内側にあっても構文上の探索漏れでC型が変わりません。

## 公開Rust API

主な入口は次の二つです。

```rust
let analysis = altc_compiler::analyze(source_file, &AnalyzeOptions::default());
let generated = altc_compiler::emit_c(&analysis, &CodegenOptions::default())?;
```

`Analysis`は入力source、lossless CST、diagnostic、memory summaryを公開し、`succeeded()`、`entrypoint()`、`has_extern_declarations()`で状態を問い合わせられます。`GeneratedUnit`は`source`、optionalな`public_header`、optionalな`ffi_check`を保持します。

ほかにlexer APIの`lex` / `Lexed` / `Token` / `TokenKind`、source位置と診断の`Diagnostic` / `Span` / `Label` / `Severity`、rowan用の`SyntaxNode` / `SyntaxKind` / `AltcLanguage`を公開します。

## diagnostic分類

codeのfamilyは実装上次の責務に分かれます。

| family | 内容 |
| --- | --- |
| `ALTC1xxx` | UTF-8、字句、literal |
| `ALTC2xxx` | 構文とrecovery |
| `ALTC3xxx` | 名前、型、control、effect、ABI |
| `ALTC4001` | 認識した対象外機能 |
| `ALTC5xxx` | C11生成 |
| `ALTC6xxx` | CLI filesystem・toolchain |

診断はbyte spanとUTF-8のline/columnを両方持ち、JSONへserializeできます。diagnosticの振る舞いを変更するときは[repair snapshot](../eval/repair/)も同時に更新します。
