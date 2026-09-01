# Compiler architecture

この文書は[Draft規範仕様](../spec/README.md)を満たすための非規範的な設計です。内部data structure、crate境界、incremental strategyは実装中に変更できます。

## 目的

- C11へ薄く変換し、実行modelとcostを追跡可能にする
- syntax errorを含む編集中sourceからもCSTと意味情報を可能な範囲で得る
- CLI、LSP、formatter、test、docでparserとsemantic modelを共有する
- altc diagnosticとC toolchain diagnosticを元sourceへ対応付ける
- backendやtoolingの都合で公開言語仕様を歪めない

## Pipeline

```text
*.altc source
  ↓
Source database / line index
  ↓
Lexer ── token + trivia + source span
  ↓
Error-recovering parser
  ↓
CST
  ├─ formatter / refactoring
  └─ AST lowering
       ↓
     AST + module graph
       ↓
     name resolution
       ↓
     typed HIR + effect checking + access-summary inference
       ├─ semantic index → LSP / doc / diagnostics
       ├─ test discovery
       └─ C11 lowering → C/H → C compiler / linker
```

## Source database

source fileごとにstable file ID、UTF-8 bytes、line start index、revisionを保持します。すべてのtokenとnodeはfile IDとbyte rangeからなるsource spanを持ちます。LSPのUTF-16 positionなどへの変換は境界で行い、compiler内部のspan単位を混在させません。

## LexerとCST

Lexerは意味tokenとtriviaを同じ順序で保持します。CSTは括弧やsemicolonを含むsourceの形を保存し、missing tokenとerror nodeを表現できるようにします。

CSTは型検査の直接入力にしません。formatterとrefactoringはCSTを利用し、semantic operationはCSTからloweringしたAST/HIRと元CSTへのmappingを利用します。

## ASTとHIR

ASTは表面構文の宣言、型、式、patternを表します。name resolution後、各binding、type、function、trait、implへstable symbol IDを与えます。

HIRは次を保持します。

- 解決済みsymbolとtype
- local inferenceの結果
- expressionのvalue categoryとmutability
- functionとcallableのeffect集合
- memory access eventと推移的summary
- `raise`とhandlerのerror型
- raising callのpostfix `?`と伝播先のerror target
- generic constraintと選択されたimpl
- source spanとdoc commentへの参照
- C ABI-safe判定と公開名

block、`if`、`match`などをC statementへ落としやすい形へ整理できますが、diagnosticが元source構造を参照できるmappingを失ってはなりません。

## Module graphとquery

module pathはsource rootから導出し、`::`区切りのnamespaceとしてimportからacyclic graphを構築します。parse、export index、name resolution、type/effect check、access-summary inference、monomorphization requestをqueryとして分離し、変更したfileと依存moduleだけを再計算できる構造を目指します。

incremental cacheの単位は仕様ではありません。cache keyにはsource revision、compiler version、target ABI、layout option、relevant optionを含め、異なるtargetのC ABI型やimplementation-defined layoutを共有しません。FFI検証cacheにはheader contentと実際のC compiler optionも含めます。

## Typeとeffect検査

type checkerはCのimplicit conversionを前提にせず、literalのcontextual typingと明示castだけを適用します。place/value、const/mut、non-null/nullableを区別します。

effect checkerはHIRから`alloc`、`free`、未処理の`raise[E]`を収集し、宣言集合のsubsetであることを検査します。call graphを通じたeffectはcallee signatureから得ます。extern signatureは利用者が書いたcontractを信頼します。

access-summary inferenceはpointer・slice越しのread/write、extern variable、volatile operation、extern call、function pointer callをsource span付きeventとしてHIRへ記録します。通常のdirect callではcallee summaryを推移的にunionし、recursive strongly connected componentは有限category集合のfixed pointとして解きます。direct eventとcall由来の経路をsemantic indexから区別できる形で保持します。

extern bodyとfunction pointerの実体をmemory read/writeまで推測しません。extern callは`extern-call`、function pointer callは`indirect-call`という不透明な境界として残します。summaryはsignatureやeffect checkerへ戻さず、空であることをoptimizerのpurity証明に使いません。

typed `raise`はcontrol-flow graphにsuccess edgeとerror edgeを持つoperationとして表現します。raising callの`?`はerror edgeを最も近い`try`またはfunction contractへ接続し、handlerはerror edgeを消費します。`defer`は通常退出とすべての早期scope exit edgeへcleanupを挿入します。

## Semantic index

semantic indexはsymbol IDをkeyに、definition、declaration、reference、type、effect、memory access summary、visibility、doc、module、source spanを検索可能にします。CLI diagnostic、LSP、doc generatorが独自に名前解決やsummary推論をやり直さないようにします。

## C backendとdriver

C backendはtyped HIRとmonomorphization結果を受け取り、C11 sourceと必要なheaderを生成します。内部生成名は利用者symbolと衝突しないnamespaceへ置きます。公開C名だけは[C連携仕様](../spec/c-interop.md)に従います。

driverはC compilerとlinkerを選択し、commandを構築・表示・実行します。C diagnosticは`#line`とsource map情報を用いてaltc spanへ対応付け、対応できない情報も失わず併記します。

extern宣言を含むbuildでは、driverが元headerと手書きcontractを比較するFFI検証用translation unitも生成します。frontend-only `check`と検証済みbuildをsemantic state上で区別し、未実行の検証を成功としてcacheしてはなりません。

## Tooling consumer

- formatterはCSTだけで安全に扱える範囲と、semantic indexが必要な範囲を分ける
- LSPはrevision付きquery結果だけを現在のdocumentへ返す
- test runnerはtest blockをentry functionへloweringし、通常artifactから分離する
- executable driverは`@entry` functionをCの`main(void)`から呼び出すwrapperを生成する
- doc generatorは公開semantic indexとMarkdown ASTからHTMLを生成する

これらはcompiler frontendと同じdiagnostic code、type printer、effect printer、access-summary printerを共有します。

## 実装言語

compilerとCLIはRustで実装します。CST/AST/HIRの公開Rust APIはcompiler内部契約であり、altc language ABIではありません。panicを利用者入力のerror処理に使わず、回復可能なdiagnosticへ変換します。
