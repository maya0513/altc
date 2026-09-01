# 型と値

## 基本型

| altc型 | 意味 | 通常のC11表現 |
| --- | --- | --- |
| `i8`、`i16`、`i32`、`i64` | 固定幅符号付き整数 | `int8_t`、`int16_t`、`int32_t`、`int64_t` |
| `u8`、`u16`、`u32`、`u64` | 固定幅符号なし整数 | `uint8_t`、`uint16_t`、`uint32_t`、`uint64_t` |
| `isize` | pointer差分を表せる符号付き整数 | `ptrdiff_t` |
| `usize` | object sizeを表す符号なし整数 | `size_t` |
| `f32`、`f64` | 浮動小数点数 | `float`、`double` |
| `bool` | `true`または`false` | `_Bool` |
| `char` | Unicode scalar value | `uint32_t`相当 |
| `unit` | 情報を持たない単一値 | 戻り値では`void`、pointerのpointeeでは`void`、値位置ではstorageを持たない内部表現 |

`char`は0から`0x10ffff`までのうちsurrogate領域を除く値だけを取ります。`unit`はvalueとしてbindingや式の型に使えますがstorable typeではありません。struct field、enum payload、array element、`sizeof`、`alignof`には使えません。`*const unit`と`*mut unit`はそれぞれCの`const void *`と`void *`に対応し、dereferenceできません。

## C ABI型

`c_char`、`c_schar`、`c_uchar`、`c_short`、`c_ushort`、`c_int`、`c_uint`、`c_long`、`c_ulong`、`c_longlong`、`c_ulonglong`、`c_float`、`c_double`、`c_longdouble`は、対象C ABIの同名C型に対応します。

固定幅型とC ABI型は、表現が一致するtargetでも別の型です。例えば`i32`から`c_int`への変換には`as c_int`が必要です。

## `str`と所有文字列

`str`はUTF-8 byte列を指す非所有・read-only viewです。論理的にdata pointerとbyte lengthを持ち、allocation、copy、NUL終端を暗黙に行いません。read-only property `byte_len`は保持済みbyte lengthを`usize`で返します。`str`の整数indexingは提供しません。byte viewへの変換、Unicode scalarのdecode、validationは明示APIを使います。

所有文字列は標準libraryの値型として定義し、取得と解放を`alloc`と`free`へ反映します。`str`自体は参照先のlifetimeを保証しません。

## Pointer

```altc
*const T
*mut T
?*const T
?*mut T
```

`*const T`はread-only、`*mut T`はread/write可能なnon-null pointerです。`?*const T`と`?*mut T`だけが`null`を取れます。nullable pointerはC pointer一語と同じABI表現を持ちます。

nullable pointerをnon-null pointerへ暗黙変換できません。`match`の`nonnull(name)` patternでnon-null値を取り出し、`null` patternでnullを処理します。v0では`pointer != null`の後に型を自動refineしません。non-nullはaddressが0でないことだけを表し、alignment、lifetime、allocation state、参照可能性を保証しません。

```altc
let value: i32 = match pointer {
  nonnull(address) => *address,
  null => 0,
};
```

nullable pointerと`Option[T]`は別の型です。`nonnull` / `null`と`Option::Some` / `Option::None`を相互に使うことはできず、両者の変換には明示APIが必要です。

unary `&`はlvalueのaddressを返します。変更可能なlvalueからは`*mut T`、それ以外からは`*const T`になります。unary `*`はdereferenceです。dereferenceはsource-level effectを発生させず、compilerが[memory access summary](memory-effects-and-errors.md#memory-access-summary)へreadまたはwriteとして記録します。

pointer arithmeticは通常の`+`と`-`では行いません。要素単位の`offset`、型を変える`ptr_cast`、addressとの変換`ptr_addr` / `ptr_from_addr`を[language builtin](builtins.md#pointer-arithmeticとaddress)として使います。`offset`はstorable pointeeだけへ適用でき、`*const unit`と`*mut unit`には使えません。const/mut qualifierを弱めるcastは明示的でなければならず、pointer castはalignment、lifetime、effective typeを検証しません。結果をdereferenceできる条件は[targetとundefined behavior](target-and-undefined-behavior.md)に従います。

## Arrayとslice

`[T; N]`はstorable type `T`を`N`個inlineに持つ固定長arrayです。`N`は`usize`で表現可能な1以上のcompile-time定数でなければなりません。v0ではzero-length arrayを提供せず、空のsequence viewにはsliceを使います。

`[]const T`と`[]mut T`は、storable type `T`に対するread-onlyまたはread/write可能な非所有sliceです。論理的にpointerとelement countを持ちます。空sliceのdata pointer表現はimplementation-definedですが、lengthは必ず0です。

arrayとsliceのread-only property `len`はelement countを`usize`で返します。`get(index)`はboundsを検査し、const sliceでは`?*const T`、mut sliceでは`?*mut T`を返します。

```altc
let values: [i32; 4] = [1, 2, 3, 4];
let view: []const i32 = values[0..4];
```

`array[index]`と`slice[index]`はbounds checkを行いません。compile-timeにindexが範囲外と確定する場合はerrorです。検査が必要な場合は`get(index)`を使い、`?*const T`または`?*mut T`を受け取ります。

slice作成時のrangeは半開区間です。`base[start..end]`はindexingと同じくruntime bounds checkを行いません。開始、終了、base lengthから範囲外とcompile-timeに確定する場合はerrorです。runtime値を検査する場合は[`slice_range`](builtins.md#slice-constructorとrange)、raw pointerから作る場合は`slice_from_parts`を明示的に使います。

## Struct

```altc
struct Point {
  x: f64,
  y: f64,
}
```

structは宣言順のnamed fieldを持つvalue型です。fieldはstorable typeでなければなりません。通常structのpadding、alignment、sizeはimplementation-definedであり、実際のsizeとalignmentはlayout queryから観測できます。C上の内部名は非規範的です。C ABIへ直接公開する型には[C連携仕様](c-interop.md)の制約が適用されます。

structの代入とby-value引数は常に値copyです。pointerやallocationされたstorageをfieldに含んでもmove-onlyにはならず、自動borrow、自動drop、RAII、暗黙のheap化は行いません。

## Enum

```altc
enum Option[T] {
  Some(T),
  None,
}

enum Color {
  Red,
  Green,
  Blue,
}
```

通常enumはpayloadの有無を同じ構文で表すtagged valueです。payloadはstorable typeでなければなりません。tagの幅、payload union、paddingはimplementation-definedであり、実際のsizeとalignmentはlayout queryから観測できます。

payloadを持たないenumに`@repr(C)`を付けると、対象C ABIのenumとして表現されます。payload付きenumへ`@repr(C)`を付けることはできません。

altcが定義してheaderへ生成する`@repr(C)` enumのdiscriminantは、最初のvariantを0として宣言順に1ずつ増加します。v0では明示discriminant構文を提供しません。extern enumの値は元headerで指定したC enumeratorを直接使い、altc側で連番を仮定しません。

標準libraryの`Option[T]`と`Result[T, E]`は通常のgeneric enumです。`Result`は値としての成功・失敗を表し、typed `raise`とは別の機能です。

## Sized、storable、layout

基本数値型、C ABI数値型、pointer、function pointer、`str`、slice、要素数1以上のarray、fieldがすべてstorableなstruct、payloadがすべてstorableなenumはstorable typeです。`unit`、functionそのもの、generic parameterが未解決な型、extern opaque typeそのものはstorableではありません。opaque typeへのpointerはstorableです。

`sizeof[T]`と`alignof[T]`は、具体化済みのstorable type `T`について、それぞれ選択targetにおける実際のobject sizeとalignmentを`usize`で返すcompile-time builtinです。通常型に対する値はimplementation-definedですが、同じcompiler version、target、layout optionでは決定的でなければなりません。

通常型のlayoutはaltc module間でも同一build内だけ有効で、安定した外部ABIではありません。`@repr(C)`だけが対象C compilerと共有するlayout契約を持ちます。詳細は[target仕様](target-and-undefined-behavior.md#implementation-defined-layout)に従います。

## 型推論

型推論はfunction body内のlocal `let` initializer、generic callの型引数、式の文脈に限定します。function parameter、return type、struct field、module-level const、公開宣言、extern宣言は型を明記しなければなりません。

推論結果はdiagnostic、hover、構造化意味情報から観測可能でなければなりません。

## 変換

bool、整数、浮動小数点、pointerの間に暗黙変換はありません。`as`はCに対応する明示数値変換を行い、narrowing、符号変更、float-to-intでC11が定義しない入力はaltcでもundefined behaviorです。検査が必要な場合は`checked_cast[T]`を使い`Option[T]`を受け取ります。bit truncationには`truncate[T]`を使います。

`char`から`u32`への変換は`as u32`でlosslessに行えます。整数から`char`への`as`は許可せず、`char_from_u32(value)`で`Option[char]`を受け取ります。pointerと整数の変換には`as`を使わず、`ptr_addr`と`ptr_from_addr`を使います。

## 整数演算

通常の整数演算は対応するC11整数型と同じ意味です。符号なしoverflowはmodulo演算、符号付きoverflowはundefined behaviorです。compile-time評価で発生するoverflow、0除算、不正shiftはerrorです。

意図を明示するため、次を別の演算として提供します。

- `+%`、`-%`、`*%`：two's-complement wrapping
- `+|`、`-|`、`*|`：saturating
- `checked_add`、`checked_sub`、`checked_mul`、`checked_div`：`Option`を返す検査付き演算

通常演算の意味をdebug/release modeで変えてはなりません。

## 拒否する例

```altc
let nullable: ?*const i32 = null;
match nullable {
  Option::Some(value) => *value, // error: nullable pointerにOption patternは使えない
  Option::None => 0,
}

let empty: [i32; 0] = []; // error: v0のarray lengthは1以上

struct Invalid {
  marker: unit, // error: unitはstorable typeではない
}
```
