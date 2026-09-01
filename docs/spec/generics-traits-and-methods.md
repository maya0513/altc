# Generic、trait、method

## Generic宣言

type parameterは角括弧で宣言します。

```altc
fn identity[T](value: T) -> T {
  value
}

struct Pair[A, B] {
  first: A,
  second: B,
}
```

compilerは使用された具体型の組ごとにfunctionとtypeをmonomorphizeします。genericだけを理由にruntime type descriptor、boxing、allocation、dynamic dispatchを生成してはなりません。

generic struct・enumのfieldまたはpayloadへtype parameterを使う場合、そのinstantiationでは具体型がstorableでなければなりません。例えば`Pair[unit, i32]`は`unit`をfield storageへ置くためerrorです。function parameterやreturn valueとしての`unit`は通常どおり使えます。

型引数はcall argumentと期待return型から局所的に推論できます。解が一意でなければ明示します。

```altc
let inferred: i32 = identity(42);
let explicit: u64 = identity[u64](42u64);
```

公開genericのsignatureとconstraintはすべて明記します。別moduleでmonomorphizeするために必要なbodyまたは中間表現の保存方法は非規範的です。

## Trait

traitはgeneric codeが利用できる操作を名前付きで宣言します。

```altc
trait Equal {
  fn equal(self: *const Self, other: *const Self) -> bool;
}
```

trait memberはfunctionだけです。associated type、associated const、default method、inheritanceはありません。traitはobject typeを作らず、`dyn`やvirtual tableを生成しません。

## Impl

```altc
impl Equal for Point {
  fn equal(self: *const Point, other: *const Point) -> bool {
    self->x == other->x && self->y == other->y
  }
}
```

一つのtrait/type組に適用できるimplは一つだけです。implはtraitを定義したmoduleまたは対象typeを定義したmoduleに置かなければなりません。これにより別moduleが競合implを追加することを防ぎます。

impl methodのsignatureはtrait memberへ正確に適合しなければなりません。effect集合は同じかsubsetである必要があります。

## Constraint

```altc
fn same[T: Equal](left: *const T, right: *const T) -> bool {
  left.equal(right)
}
```

constraintはtype parameterの後へ`: Trait`で書きます。複数constraintは`T: First + Second`と記述します。すべてcompile-timeに解決し、満たすimplがない、または一意でないcallはerrorです。

constraintに基づくcallは具体的なimpl functionへのstatic callとしてloweringします。operator overloadには使いません。

## Inherent method

traitを介さないmethodはtypeの`impl` blockへ置きます。

```altc
impl Point {
  fn translated(self: Point, dx: f64, dy: f64) -> Point {
    Point { x: self.x + dx, y: self.y + dy }
  }
}
```

method callはreceiverを最初のargumentにする機械的な糖です。

```altc
let moved = point.translated(1.0, 2.0);
// Point::translated(point, 1.0, 2.0) と同じ意味
```

receiver式は一度だけ評価します。自動address取得、自動dereference、自動borrow、自動cloneはありません。methodが`*const Point`を要求するならcallerは`(&point).method()`のようにpointerを明示します。by-value receiverは通常のvalue copyです。

## Method解決

inherent methodをtrait methodより先に検索します。複数traitが同じ名前を提供して曖昧になる場合は、`Trait::method(receiver, ...)`で明示しなければなりません。method名からruntime dispatchを選ぶことはありません。

## Code size

各型引数に対する別code生成は言語の意味です。compilerとlinkerは意味を変えない範囲で同一instantiationを統合できます。code size、生成symbol、inlining判断は非規範的であり、`altc emit-c`とsize reportから確認可能でなければなりません。
