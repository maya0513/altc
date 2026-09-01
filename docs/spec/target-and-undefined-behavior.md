# Targetとundefined behavior

## Supported target

適合実装はtargetごとに使用するC compiler、ABI、data modelを文書化しなければなりません。v0のsupported targetは次を満たす必要があります。

- Cの1 byteが8 bitである。
- `<stdint.h>`が`int8_t`から`int64_t`、`uint8_t`から`uint64_t`を提供する。
- 固定幅符号付き整数がtwo's-complement表現である。
- `size_t`と`ptrdiff_t`がそれぞれ`usize`と`isize`を表せる。
- `uintptr_t`が存在し、pointer addressを`usize`へlossなく保持できる。
- C11 mode、`_Static_assert`、`_Generic`、`_Alignof`を利用できる。

target、C compiler、主要ABI optionはsemantic cache keyへ含めます。同じsourceでもtargetが異なれば`c_long`、layout、`sizeof`、`alignof`の値は異なりえます。

## Implementation-defined layout

通常struct、payload付きenum、`Option`、`Result`、slice、`str`の内部layoutはimplementation-definedです。ただし、処理系が選んだ実際のsizeとalignmentは`sizeof[T]`と`alignof[T]`から観測可能で、`altc emit-c`でも確認可能でなければなりません。

通常型のlayoutは公開ABIではなく、compiler version、target、option間で安定する保証はありません。`@repr(C)`型だけが[C ABI規則](c-interop.md#c-abi-safeな型)に従います。

## Undefined behavior

次の操作はundefined behaviorです。compilerはcompile-timeに確定できる場合にerrorとして拒否できますが、runtime検査を暗黙に追加する義務はありません。

- 通常の符号付き整数演算で結果が型の範囲を超える。
- 整数の0除算、0剰余、符号付き最小値を`-1`で除算する。
- shift countが左辺型のbit幅以上である。
- C11で定義されない符号付きleft shiftを行う。
- NaN、無限大、または変換先整数型の範囲外の浮動小数点値を`as`で整数へ変換する。
- 有効なUnicode scalar valueでない整数を、検査を経ず`char`として構築する。
- null、dangling、misaligned、one-past、権限不一致、または対象objectを指さないpointerをdereferenceする。
- unchecked array・slice indexingで範囲外へaccessする。
- unchecked rangeまたは`slice_from_parts`で、base objectの範囲、順序、mutability、lifetimeを満たさないsliceを構築または使用する。
- pointer `offset`で元のarray objectとそのone-pastの範囲外のpointerを作り、それを比較・dereferenceなどCが許可しない用途へ使う。
- `ptr_cast`後のpointerが参照先型のalignment、lifetime、effective type、aliasing規則を満たさない状態でdereferenceする。
- `ptr_from_addr`の値が有効なpointer provenanceを復元できないtargetまたは状況で、その結果をdereferenceする。
- allocation API、deallocation API、extern functionが宣言するC contractへ違反する。double free、invalid free、解放後参照を含む。
- C callerが`export "C"`のnon-null parameterまたはout pointerへnullを渡す。

符号なしoverflow、明示wrapping・saturating・checked演算の結果はundefined behaviorではなく、[型仕様](types-and-values.md#整数演算)で定義します。

## Diagnosticとoptimization

undefined behaviorへ到達しないprogramについて、debug/release modeで意味を変えてはなりません。optimizerはundefined behaviorが発生しない前提を使えますが、altcが定義した左から右の評価順、`?`による伝播、`defer`の実行を変更してはなりません。

処理系がtarget固有の追加制約を必要とする場合、それを単なる未決事項ではなくimplementation-definedな制約として文書化しなければなりません。
