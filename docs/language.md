# 現行altc言語

ここに書かれた範囲は、[compiler test](../crates/altc-compiler/src/)と[CLI integration test](../crates/altc-cli/tests/cli.rs)から要約したものです。この文書は将来の構文を予約せず、現在の実装より強い保証を与えません。

## programとmodule

sourceはUTF-8の`.altc` fileです。CLIへ渡したfileがroot moduleになり、その親directoryをsource rootとして`import math;`は`math.altc`、`import net::http;`は`net/http.altc`を読みます。import graphはcycleを許さず、他moduleの宣言は`module::symbol`というqualified pathでだけ参照できます。外部から見えるfunction、struct、enumには`pub`が必要で、fieldとvariantにも個別の`pub`が必要です。

graph全体を一つのC11 translation unitへ生成します。root moduleの内部名は従来どおりで、import先だけlogical module pathを含む決定的な名前になります。`@entry`はroot moduleのparameterなし・非公開・非raising・`c_int`を返すfunction一つに限ります。

```altc run=75
const Limit: i32 = 100;

fn clamp(value: i32) -> i32 {
  if value < 0 {
    0
  } else if value > Limit {
    Limit
  } else {
    value
  }
}

@entry
fn application_main() -> c_int {
  clamp(75) as c_int
}
```

blockの最後のsemicolonなしexpressionがblockの値です。`let`はinitializerから局所型を推論でき、再代入には`let mut`が必要です。top-level宣言をbodyより先に集めるため、forward callと再帰を利用できます。

## 型とdata

現在の組み込み型は次のとおりです。

- `unit`、`bool`、`char`、`str`
- `i8`、`i16`、`i32`、`i64`、`u8`、`u16`、`u32`、`u64`、`isize`、`usize`
- `f32`、`f64`
- `c_char`、`c_schar`、`c_uchar`、`c_short`、`c_ushort`、`c_int`、`c_uint`、`c_long`、`c_ulong`、`c_longlong`、`c_ulonglong`
- `c_float`、`c_double`、`c_longdouble`
- non-null pointer `*const T` / `*mut T`、nullable pointer `?*const T` / `?*mut T`
- 固定長array `[T; N]`、slice `[]const T` / `[]mut T`
- altc function pointer `fn(...) -> T`、C callback `extern "C" fn(...) -> T`
- named `struct`、payloadを持てる`enum`、extern block内のopaque type

整数・浮動小数literalはsuffixまたは周囲の型から決まり、通常の数値castは`value as T`です。`struct` literalとfield access、enum constructorとpayload pattern、`@repr(C)`のpayloadなしenumを利用できます。bool、enum、nullable pointerの`match`は網羅性を検査します。

array lengthは1以上のcompile-time `usize`でなければなりません。`[a, b, c]`は期待型または要素から型を決めます。arrayはvalue copy、sliceはtyped pointerとlengthを持つviewです。array/sliceのindexとruntime rangeは実行時にbounds checkをせず、compile-timeに範囲外と分かる場合だけ拒否します。sliceやraw pointerの生存期間、動的range、danglingはCと同じく利用者の責任です。

`.len`は`usize`を返し、addressableなarray/sliceの`.get(index)`はboundsを検査してnullable element pointerを返します。`base[start..end]`、`base[..end]`、`base[start..]`、`base[..]`は半開sliceです。`slice_from_parts[T]`、`empty_slice[T]`、`empty_slice_mut[T]`、`slice_as_const`でviewを明示的に構築・変換できます。

## 式と制御

実装されている制御構文はblock、`if` / `else`、`match`、`while`、C風`for`、array/slice専用の`for value in sequence`、`return`、`break`、`continue`です。range-forはsequenceを一度評価し、lengthを一度保存してimmutableな要素copyを走査します。iterator objectやallocationは生成しません。

operatorは通常の算術・比較・論理・bit演算、assignmentとcompound assignmentに加え、wrapping `+%` / `-%` / `*%`、saturating `+|` / `-|` / `*|`とそれぞれのassignment形を持ちます。backendは式の左から右の評価順をtemporaryへ線形化して保持します。

`defer { ... }`はscope退出時にLIFOで実行され、通常終了、`return`、`break`、`continue`、error伝播を覆います。defer body内からのcontrol transferとraising propagationは拒否されます。

## pointer、effect、memory summary

`&place`でaddressを取得し、`*pointer`でdereferenceします。nullable pointerは直接dereferenceできず、`null` / `nonnull(name)` patternで絞り込みます。

```altc run=7
extern "C" from <stdlib.h> {
  fn malloc(size: usize) -> ?*mut unit with { alloc };
  fn free(pointer: *mut unit) with { free };
}

@entry
fn application_main() -> c_int with { alloc, free } {
  let data: *mut u8 = match malloc(sizeof[u8]) {
    nonnull(raw) => ptr_cast_mut[u8](raw),
    null => return 2,
  };
  defer {
    free(ptr_cast[unit](data));
  }
  *data = 7u8;
  *data as c_int
}
```

pointer/layout builtinは`sizeof[T]`、`alignof[T]`、`ptr_as_const`、`ptr_cast[T]`、`ptr_cast_mut[T]`、`offset[T]`、`ptr_addr`、`ptr_from_addr[T]`、`ptr_from_addr_mut[T]`です。登録済みbuiltinだけが`name[T](...)`を受理するため、通常の`value[index](...)`をgeneric callとして誤解釈しません。

functionがallocation、解放、typed errorを実行する場合、`with { alloc, free, raise[E] }`のように宣言します。bodyの実effectは宣言集合のsubsetでなければなりません。raising callにはpostfix `?`が必要で、同じ`raise[E]` contractまたはlexicalな`try ... catch`へ接続します。function pointerにもeffect集合が含まれ、indirect callへ同じ規則で伝播します。

解析結果は直接・推移eventを分け、間接read/write、extern call、extern global read/write、indirect callをmemory summaryへ記録します。再帰call graphは有限集合のfixed pointまで伝播し、moduleをまたぐeventも元fileとlocal byte spanを保持します。

## C連携

`extern "C" from <header.h>`またはquoted headerのblockでは、C function、addressableな`var` / `const` object、opaque type、struct、enumを宣言できます。extern `const`はmacroやenumeratorではなく、addressを取得できるC objectを意味します。extern struct/enumは`@c_name`によるtag名または`@c_typedef`によるtypedef名を必要とし、field・variantのspellingも`@c_name`で固定できます。

C callbackは`extern "C" fn(...)`で表し、ABI-safeでraisingしないaltc functionを渡せます。closure、capturing environment、nullable callbackはありません。通常のaltc function pointerは宣言effectを持てますが、C callbackは`raise`をABI境界へ運べません。

`build`は生成したFFI検証translation unitをC compilerへ渡し、functionとcallback signature、global型とconstness、structのsize/alignment/offset/field型、enum spellingを検証します。不一致は`ALTC6101`で失敗します。array/sliceのC ABI by-value parameter/returnは拒否されるため、C公開面ではpointerとlengthまたは明示的な`@repr(C)` wrapperを使います。

`export "C" fn`はpublic headerを生成し、`@c_name("symbol")`でC symbolを固定できます。raising exportはstatusをreturnし、成功値をout parameterへ書くABIになります。import先のexportもgraph全体のheaderへ集約し、C symbol重複を検査します。

単一fileの統合例は[feature showcase](../examples/feature_showcase.altc)、moduleと外部C libraryを含む例は[telemetry pipeline](../examples/telemetry_pipeline/README.md)にあります。

## 現在対象外

次は現在実装されていません。parserが認識できるものは、別の意味へ変換せず`ALTC4001`などの専用診断で拒否します。

- project設定、package manager、remote dependency、re-export、import alias/wildcard、cross compilation、複数C translation unit
- generic、trait、impl、method解決、closure、capturing callback
- `Option` / `Result`、checked range、checked数値builtin、borrow/lifetime検査
- extern macro、preprocessor constant、variadic、volatile、nullable callback
- formatter、test runner、documentation generator、LSP、optimizer、incremental build

```altc compile_fail=ALTC4001
fn identity[T](value: T) -> T {
  value
}
```
