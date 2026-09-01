# 現行altc言語

ここに書かれた範囲は、[compiler test](../crates/altc-compiler/src/)と[CLI integration test](../crates/altc-cli/tests/cli.rs)から要約したものです。この文書は将来の構文を予約せず、現在の実装より強い保証を与えません。

## 最小program

sourceはUTF-8の単一`.altc` fileです。実行形式には、parameterを持たず`c_int`を返す非公開の`@entry` functionが一つ必要です。

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
- named `struct`、payloadを持てる`enum`、extern block内のopaque type

整数・浮動小数literalはsuffixまたは周囲の型から決まります。通常の数値castは`value as T`です。`struct` literalとfield access、enum constructorとpayload pattern、`@repr(C)`のpayloadなしenumを利用できます。bool、enum、nullable pointerの`match`は網羅性を検査します。実行テストは[data/control fixture](../tests/fixtures/data_control.altc)にあります。

## 式と制御

実装されている制御構文はblock、`if` / `else`、`match`、`while`、C風`for`、`return`、`break`、`continue`です。operatorは通常の算術・比較・論理・bit演算、assignmentとcompound assignmentに加え、wrapping `+%` / `-%` / `*%`、saturating `+|` / `-|` / `*|`とそれぞれのassignment形を持ちます。backendは式の左から右の評価順をtemporaryへ線形化して保持します。

`defer { ... }`はscope退出時にLIFOで実行され、通常終了、`return`、`break`、`continue`、error伝播を覆います。defer body内からのcontrol transferとraising propagationは拒否されます。

## pointerとmemory summary

`&place`でaddressを取得し、`*pointer`でdereferenceします。nullable pointerは直接dereferenceできず、`null` / `nonnull(name)` patternで絞り込みます。

```altc run=7
extern "C" from <stdlib.h> {
  fn malloc(size: usize) -> ?*mut unit with { alloc };
  fn free(pointer: *mut unit) with { free };
}

@entry
fn application_main() -> c_int with { alloc, free } {
  let data: *mut u8 = match malloc(sizeof[u8]) {
    nonnull(raw) => ptr_cast[u8](raw),
    null => return 2,
  };
  defer {
    free(ptr_cast[unit](data));
  }
  *data = 7u8;
  *data as c_int
}
```

現在のpointer builtinは`sizeof[T]`、`alignof[T]`、`ptr_as_const`、`ptr_cast[T]`、`ptr_cast_mut[T]`、`offset[T]`、`ptr_addr`、`ptr_from_addr[T]`、`ptr_from_addr_mut[T]`です。解析結果は直接・推移eventを分け、現在は間接read、間接write、extern callをmemory summaryへ記録します。再帰call graphはfixed pointまで伝播します。

## effect、typed error、defer

functionがallocation、解放、typed errorを実行する場合、`with { alloc, free, raise[E] }`のように宣言します。bodyの実effectは宣言集合のsubsetでなければなりません。raising callにはpostfix `?`が必要で、同じ`raise[E]` contractまたはlexicalな`try ... catch`へ接続します。

```altc run=7
extern "C" from <stdio.h> {
  fn puts(message: *const c_char) -> c_int;
}

enum Failure {
  Negative,
}

fn checked(value: i32) -> i32 with { raise[Failure] } {
  if value < 0 {
    raise Failure::Negative;
  };
  value
}

fn work(value: i32) -> i32 with { raise[Failure] } {
  defer {
    puts(c"cleanup");
  }
  checked(value)?
}

@entry
fn application_main() -> c_int {
  try {
    work(7)? as c_int
  } catch {
    Failure::Negative => 1,
  }
}
```

## C連携

`extern "C" from <header.h>`はC functionと`@c_typedef`付きopaque typeを宣言できます。`build`は生成したFFI検証translation unitをC compilerへ渡し、parameterとreturn typeの不一致を拒否します。

`export "C" fn`はpublic headerを生成し、`@c_name("symbol")`でC symbolを固定できます。raising exportはstatusをreturnし、成功値をout parameterへ書くABIになります。

```altc check
@repr(C)
enum ApiError {
  Negative,
}

@c_name("checked_double")
export "C" fn checked_double(value: i32) -> i32 with { raise[ApiError] } {
  if value < 0 {
    raise ApiError::Negative;
  };
  value * 2
}
```

主要機能を個別ではなく同じ実行経路で組み合わせたprogramは[feature showcase](../examples/feature_showcase.altc)にあります。C ABI、opaque type、pointer、effect、nested `try`、全退出pathの`defer`、整数境界値をstrict C11の`-Werror`でbuild/runします。

## 現在対象外

次は現在実装されていません。parserが認識できるものは、別の意味へ変換せず`ALTC4001`で拒否します。

- import、複数module、project設定、cross compilation
- generic、trait、impl、method、function pointer、callback
- array、slice、range-for、`Option` / `Result`、checked数値builtin
- extern macro、variable、constant、struct、enum、variadic function
- formatter、test runner、documentation generator、LSP、optimizer、incremental build

```altc compile_fail=ALTC4001
fn identity[T](value: T) -> T {
  value
}
```
