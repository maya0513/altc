# altc compiler

実装は[altc-compiler crate](../crates/altc-compiler/src/lib.rs)です。内部構造を説明する文書より、公開型とtestを正とします。

## pipeline

```text
UTF-8 root source
  -> lexer + rowan lossless CST
  -> recovery可能なparser + AST
  -> module graph load・cycle/privacy検査
  -> graph全体の宣言収集・名前解決
  -> 型/place/pattern・control/effect・C ABI検査
  -> memory summary fixed point + typed model
  -> C11 source + optional public header + optional FFI verification TU
```

各moduleのlexerはcomment、空白、delimiterとbyte spanをCSTへ保持します。parserはsemicolon、brace、top-level itemを同期点として複数診断から回復します。意味解析は宣言をbodyより先に収集し、forward call、再帰、moduleをまたぐeffect伝播を許可します。

意味情報は[semantic model](../crates/altc-compiler/src/semantics/model.rs)へ集約し、型、symbol、place、effect、C spelling/layout、expressionごとの解決結果をbackendへ渡します。C生成はwriter、C型宣言、item/ABI、式線形化、control/defer cleanupへ責務を分けています。後段は名前やerror型をASTから再推論しません。

backendはaltcの左から右の評価順をtemporaryで保持し、payload enumをtagとunion、arrayをsole-array wrapper struct、sliceをtyped pointerと`size_t`のwrapper、nullable pointerをC pointer、`str`をpointerとbyte lengthへloweringします。raising functionは内部でもstatusとout parameterを使い、defer cleanupは退出pathへ静的に挿入します。runtime library、GC、例外runtime、cleanup stackはありません。

## 公開Rust API

single-file解析の入口は従来どおりです。

```rust
let analysis = altc_compiler::analyze(source_file, &AnalyzeOptions::default());
let generated = altc_compiler::emit_c(&analysis, &CodegenOptions::default())?;
```

importを含むrootには`ModuleLoader`を実装して`analyze_project`を使います。`ModulePath`はidentifier segment列を保持し、loaderは対応する`SourceFile`または`ModuleLoadError`を返します。

```rust
let analysis = altc_compiler::analyze_project(
    root_source,
    &mut loader,
    &AnalyzeOptions::default(),
);
```

`Analysis::source`と`cst`はrootを指し続け、`modules()`はgraph内の全`SourceFile`とlossless CSTを返します。diagnosticとmemory eventは元moduleのfileとlocal byte spanを保持します。`GeneratedUnit`は単一の`source`、optionalな`public_header`、optionalな`ffi_check`を保持します。

ほかにlexer APIの`lex` / `Lexed` / `Token` / `TokenKind`、source位置と診断の`Diagnostic` / `Span` / `Label` / `Severity`、rowan用の`SyntaxNode` / `SyntaxKind` / `AltcLanguage`を公開します。

## diagnostic分類

| family | 内容 |
| --- | --- |
| `ALTC1xxx` | UTF-8、字句、literal |
| `ALTC2xxx` | 構文とrecovery |
| `ALTC3xxx` | 名前、型、control、effect、ABI、module |
| `ALTC4001` | 認識した対象外機能 |
| `ALTC5xxx` | C11生成 |
| `ALTC6xxx` | CLI filesystem・toolchain |

診断はbyte spanとUTF-8のline/columnを両方持ち、JSONへserializeできます。diagnosticの振る舞いを変更するときは[repair snapshot](../eval/repair/)も同時に更新します。
