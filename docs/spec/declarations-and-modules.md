# 宣言とmodule

## Binding

```altc
let answer: i32 = 42;
let inferred = answer + 1;
let mut count: i32 = 0;
count = count + 1;
```

`let`は再代入できないbinding、`let mut`は再代入できるbindingです。bindingが保持するvalueの内部メモリをpointer経由で変更できるかどうかと、bindingへの再代入可能性は別です。

initializerは必須です。型annotationを省略できるのはfunction body内のlocal bindingだけです。module-levelの`let`とmutable globalは提供しません。

## Const

```altc
const BufferSize: usize = 4096;
```

`const`はmoduleまたはblock scopeで宣言でき、型annotationが必須です。initializerはcompile-timeに評価可能でなければなりません。

const expressionで使えるものは、literal、他のconst、enum constructor、struct・array literal、const評価可能と定めたoperator、[`sizeof[T]`、`alignof[T]`](builtins.md#layout-query)、仕様でconstと定めたbuiltinです。pointer dereference、function call、runtime loop、mutable stateは使えません。effect集合が空であることだけではconst評価可能になりません。

## Function

```altc
fn add(a: i32, b: i32) -> i32 {
  a + b
}

fn log(message: str) {
  write_log(message);
}
```

parameterは`name: Type`、return typeは`-> Type`で記述します。return typeを省略したfunctionは`unit`を返します。function bodyの末尾式がreturn valueになります。`return expression;`で早期returnできます。`unit` functionの`return;`も許可します。

同じparameter名を重複できません。すべてのparameterとreturn typeは明記します。effect contractは[effect仕様](memory-effects-and-errors.md)に従います。

## Function pointer

```altc
let compare: fn(i32, i32) -> i32 = compare_i32;
let reader: fn(*const u8) -> u8 = read_byte;
```

function pointer型はparameter型、return型、effect集合を含みます。memory access summaryはfunction pointer型の一部ではありません。捕捉環境は持ちません。non-capturing functionだけをfunction pointerへ変換できます。callableのeffectが要求された型のsubsetであれば、安全な方向へのcoercionを許可します。

## Structとenum宣言

```altc
pub struct Point {
  pub x: f64,
  pub y: f64,
}

pub enum ParseError {
  Empty,
  InvalidByte(u8),
}
```

typeとfield・variantの可視性は独立です。公開functionのsignatureで使うtypeと、そのABIや利用に必要なfield・variantは、利用側から参照可能でなければなりません。

同じtype内でfield名またはvariant名を重複できません。enum variantはtype nameでnamespace化し、`ParseError::Empty`のように参照します。

## Module

一つの`.altc` fileは一つのmoduleです。module名はsource rootからの相対pathから導出します。

```text
src/math.altc       -> math
src/net/http.altc   -> net::http
```

source内で別のmodule名を宣言する構文はありません。module dependency graphはacyclicでなければならず、cycleはdiagnosticになります。

## Import

```altc
import math;
import net::http;
```

`import`はmoduleの宣言を名前解決へ追加します。importした名前は`module::symbol`で修飾します。`::`はmodule、type、enum variant、静的methodのnamespace pathだけに使い、`.`はvalue fieldとmethod、`->`はpointer越しのfieldとmethodに使います。wildcard importと暗黙prelude importは行いません。標準libraryも明示的なroot moduleとして扱いますが、基本型とlanguage builtinはimport不要です。

`Option`、`Result`はlanguageが定義するpredeclared generic enumであり、import不要です。test block内の`assert_eq`などtest harness builtinもimport不要です。それ以外のstandard library APIは明示importします。

## altc内公開

`pub`は別のaltc moduleから参照できる宣言を表します。`pub`はC symbol名や安定C ABIを約束しません。コンパイラはaltc module間の呼び出し用に内部headerを生成できますが、その内容は非規範的です。

```altc
pub fn distance_squared(p: Point) -> f64 {
  p.x * p.x + p.y * p.y
}
```

## C ABI公開

`export "C"`はCから呼び出す安定ABIの宣言です。`pub`とは独立して指定します。

```altc
@c_name("project_add")
export "C" fn add(a: i32, b: i32) -> i32 {
  a + b
}
```

`@c_name`を省略したC symbol名は、projectとmoduleと宣言名から決定的にmangleします。正確な既定manglingはimplementation-definedですが、同じ公開入力とcompiler versionに対して安定し、実際の名前を生成headerから確認できなければなりません。

`export "C"`の型制約、header生成、typed `raise`のABIは[C連携](c-interop.md)で定義します。

## Entrypoint

executable outputには`@entry`を付けたfunctionが正確に一つ必要です。library outputに`@entry`を含めることはできません。entrypointはfunction名ではなくattributeで決まり、`main`という名前自体に特別な意味はありません。

```altc
@entry
fn application_main() -> c_int {
  run_application()
}
```

v0のentrypointはparameterを持たず、return型を`c_int`としなければなりません。generic、`pub`、`export "C"`、`raise[E]`は使用できません。`alloc`と`free`は通常のfunctionと同じeffect contractへ明記します。失敗しうる処理はentrypoint内で`try ... catch`により処理し、最終的なprocess exit statusへ変換します。

compilerはentrypointを呼び出すC ABIの`int main(void)`を生成します。altc entrypoint自体をC symbol `main`として公開する必要はありません。test harnessが生成するentry functionは`@entry`とは別で、通常artifactへ含めません。

次はいずれもerrorです。

```altc
@entry
fn fallible_main() -> c_int with { raise[StartupError] } { // error: entrypointはraiseできない
  start()?
}

@entry
pub fn public_main() -> c_int { // error: entrypointはpubにできない
  0
}
```

## Test block

```altc
test "addition" {
  assert_eq(add(1, 2), 3);
}
```

test blockはmodule scopeに置きます。通常buildの公開programには含めず、`altc test`が生成するtest harnessへ登録します。名前はmodule内で一意でなければなりません。test blockは`unit`を返し、`raise[TestError]`相当の失敗をtest runnerが報告します。
