# 用語集

この用語集はaltc文書で使う語の意味を統一します。言語の意味そのものは[Draft規範仕様](spec/README.md)が定義します。

## ABI

Application Binary Interface。Cから見た関数名、calling convention、引数と戻り値、型のlayoutなど、別々にコンパイルしたbinaryを接続する契約です。

## AST

Abstract Syntax Tree。CSTから意味に不要な区切りや空白を整理し、名前解決や型検査で扱いやすくした構文表現です。

## C ABI型

`c_int`や`c_long`など、対象platformのC型と同一のABIを持つaltc型です。固定幅の`i32`などとは別の型です。

## CST

Concrete Syntax Tree。token、コメント、空白、括弧、区切り、ソース位置を保持する構文保存層です。formatter、LSP、refactoring、コメント対応に利用します。

## Draft規範仕様

正式リリース前に改訂可能である一方、現時点の実装判断を拘束する仕様です。このリポジトリでは`docs/spec/`だけが該当します。

## effect

関数が行いうるstorage lifecycle操作とtyped error control flowを、型付き契約として表したものです。altcでは`alloc`、`free`、`raise[E]`を扱います。通常のmemory read/writeはeffectではありません。

## memory access summary

pointer・slice越しのread/write、extern variable、volatile access、extern call、function pointer callをcompilerが推論する意味情報です。source-level effectやfunction型の一部ではなく、LSP、documentation、inspection、lintから利用します。

## tracked-effect-free

effect集合が空であることです。memory access、I/O、external stateの変更、終了性は制限しないため、pureと同義ではありません。

## entrypoint

executableの開始位置として`@entry`を付けたaltc functionです。C ABIへ直接exportされるfunctionではなく、compilerがCの`main(void)`から呼び出すwrapperを生成します。

## FFI verification

手書きのaltc extern宣言とC headerの宣言が一致することを、検証用C translation unitと対象C compilerで確認する処理です。headerをincludeするだけの通常compileとは区別します。

## HIR

High-level Intermediate Representation。名前解決、型、effect、memory access eventなどの意味情報を付与し、表面構文の糖を整理した中間表現です。

## implementation-defined

仕様が許容範囲を定めたうえで、処理系が一つの動作を選び文書化しなければならない事項です。単に決定を先送りした事項ではありません。

## lowering

高水準の構文や型を、より低水準の等価な表現へ変換することです。例えば`match`を`switch`へ、型付き`raise`をstatusと分岐へ変換します。

## monomorphization

genericな関数や型を、実際に使われた型引数ごとのC関数・C型へ静的に特殊化する方式です。

## non-null pointer

値0を取らないことを型が要求するpointerです。altcの`*const T`と`*mut T`はnon-nullであり、nullを許す場合は`?*const T`または`?*mut T`を使います。これは参照先の寿命や有効性までは保証しません。

## storable type

objectとしてstorageへ置け、sizeとalignmentを持つ型です。`unit`はvalueとして使えますがstorable typeではありません。

## non-normative

理解を助ける説明や例であり、実装を拘束しない記述です。`docs/examples/`の推定C出力が該当します。

## source span

source file内の開始位置と終了位置です。token、構文node、symbol、診断を元のaltcコードへ対応付けます。

## trivia

構文の意味には直接参加しないコメントや空白です。altcはこれを捨てずCSTへ保持します。

## undefined behavior

仕様がprogramの実行結果を定めない状態です。altcでは無効pointerのdereferenceや通常の符号付き整数overflowなど、C11 backendの低レベル操作に対応する一部の誤用をundefined behaviorとして明示します。

## zero-cost abstraction

同じ意味を手書きの低レベルコードで表した場合に不要なallocation、iterator object、virtual dispatchなどを追加しない抽象化です。常に最適な機械語を保証する語ではありません。
