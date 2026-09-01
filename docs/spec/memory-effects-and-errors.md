# Effect、memory access、error

## Effect contract

functionの`with { ... }`は、そのfunctionが実行しうるeffectの上限を宣言します。

```altc
enum ParseError {
  Invalid,
}

fn reject() -> i32 with { raise[ParseError] } {
  raise ParseError::Invalid;
}
```

effect集合の順序は意味を持たず、重複はerrorです。`with`を省略したfunctionは空集合です。`memory`はeffect名ではなく、`with { memory }`は受理しません。compilerはbodyからeffectを収集し、宣言に含まれないeffectをerrorにしなければなりません。bodyからfunction signatureを自動的に変更してはなりません。

## 追跡するeffect

| Effect | 意味 |
| --- | --- |
| `alloc` | heap、arena、poolなどからstorageを取得する操作 |
| `free` | 取得済みstorageをdeallocatorへ渡す操作 |
| `raise[E]` | 通常のvalue経路を離れ、`E`型のerrorを伝播する操作 |

`alloc`と`free`は互いを包含しません。effectはcallerから見たstorage lifecycleと型付きerror control flowを分類するもので、allocator内部の状態一般を表すものではありません。I/O、time、randomness、threading、通常のmemory read/writeはlanguage effectとして追跡しません。

effectは所有権証明ではありません。`free`が宣言されていても、leak、double free、use-after-freeがないことを保証しません。

effect集合が空のfunctionは、この文書では`tracked-effect-free`と呼びます。これは`alloc`、`free`、未処理の`raise[E]`がないことだけを意味します。memory access、I/O、external stateの変更、終了性、同じ入力に対する同じ結果を保証しないため、`pure`とは呼びません。

## Memory access summary

compilerはeffect検査とは別に、function bodyの低レベルなmemory accessを推論してsemantic informationとして保持します。このmemory access summaryはsource構文ではなく、`with { ... }`へ記述できません。function signature、function pointer型、trait適合、C ABIの一部でもなく、summaryだけを理由にcore language上のerrorとしてはなりません。

```altc
fn update(p: *mut i32) {
  *p = *p + 1;
}
```

このfunctionのeffect集合は空ですが、summaryには`indirect-read`と`indirect-write`が入ります。

最低限、次のcategoryを区別します。

| Category | 意味 |
| --- | --- |
| `indirect-read` | pointer dereference、pointer field、slice elementを介したread |
| `indirect-write` | pointer dereference、pointer field、mutable slice elementを介したwrite |
| `global-read` | extern variableのread |
| `global-write` | extern variableのwrite |
| `volatile-read` | volatile objectからのread |
| `volatile-write` | volatile objectへのwrite |
| `extern-call` | extern function、extern `macro fn`、またはC calling conventionのindirect call |
| `indirect-call` | statically一つのaltc functionへ解決できないfunction pointer call |

local scalarと、pointerを介さず直接扱うlocal aggregateの通常read/writeはsummaryへ追加しません。address取得、pointer cast、pointer arithmetic、`get`によるpointer取得も、それ自体ではread/writeを追加しません。取得したpointerやsliceを介してvalueを読むか書く位置で追加します。

v0のsurface typeにはvolatile qualifierがありません。そのため通常のaltc sourceから`volatile-read`と`volatile-write`は直接発生しませんが、compilerが認識するintrinsicや将来のFFI表現でvolatile accessを導入する場合、通常のread/writeへ潰さずこのcategoryを保持しなければなりません。

categoryは排他的ではありません。例えばvolatile pointer越しのreadには`indirect-read`と`volatile-read`の両方を、volatile extern variableのwriteには`global-write`と`volatile-write`の両方を付けます。

一つに解決できる通常のaltc function callでは、calleeのsummaryをcallerへ推移的に加えます。再帰call graphはcategory集合の有限なfixed pointとして計算できます。extern callはbodyを解析せず`extern-call`を、function pointer callは`indirect-call`を加えます。C calling conventionのfunction pointer callには両方を加えます。externまたはindirect callが実際にread/writeするかを根拠なく推測してはなりません。

compilerは各categoryの直接のsource spanと、call経由で到達する代表的な経路を問い合わせ可能にしなければなりません。summaryはLSP semantic query、documentation、opt-in lintで利用できます。空のeffect集合や特定categoryの不在だけを、callの削除・並べ替え、alias解析、const評価、data-race safetyの根拠にしてはなりません。

summaryは解析したsource revisionの観測情報であり、source compatibility、semantic versioning、ABIの契約ではありません。function bodyの変更によりsignatureを変えずにsummaryが変化しても構いません。

## Callによる伝播

calleeの`alloc`と`free`、handlerで処理されない`raise[E]`はcallerでも発生します。callerの宣言集合に不足があればerrorです。function pointer callではpointer型に記録されたeffectを使います。

tracked-effect-freeなfunctionは、より多いeffectを許すfunction pointer型へcoerceできます。一般にcallableのeffect集合が要求された型のsubsetならcoerceでき、逆方向は許可しません。memory access summaryはこの判定へ使いません。effect polymorphismは提供せず、高階functionのeffect集合は具体的に記述します。

## Typed `raise`

```altc
enum ParseError {
  Empty,
  InvalidByte(u8),
}

fn parse(input: str) -> Ast with { raise[ParseError] } {
  if input.byte_len == 0 {
    raise ParseError::Empty;
  }
  parse_nonempty(input)?
}
```

`raise value;`はvalueをerror経路へ送り、現在のcontrol flowを終了します。`raise[E]`の`E`はsizeがcompile-timeに既知のstorable value型でなければなりません。一つのfunctionは最大一つの`raise[E]`を宣言できます。

`raise` expressionは通常valueを生成せず、その後の同じcontrol-flow pathは到達不能です。言語に名前付きの`never` typeは設けませんが、`if`、`match`、`try`の型一致では`raise`だけで終わるbranchをvalue-producing branchの型へ適合させられます。`return`、`break`、`continue`で到達不能になるbranchも同様です。

raising callableのcallは、postfix `?`を付けたときだけsuccess valueを式の値として使えます。`?`はerrorを最も近い`try` bodyへ、`try`がなければcallerへ伝播します。callerへ伝播する場合、callerは同じ`raise[E]`を宣言しなければなりません。raising callへ`?`を付けず値として使うprogramはerrorです。

```altc
let ast: Ast = parse(input)?;
```

`?`はtyped `raise`専用であり、`Option`、`Result`、nullable pointerから値を取り出す目的には使えません。一つの`try` bodyからcatchへ到達するerror edgeはすべて同じ型でなければなりません。

異なるerror型は暗黙変換しません。caller自身のerrorへ変換する場合はhandler内で明示的にconstructして`raise`します。

## `try ... catch`

```altc
let ast: Ast = try {
  parse(input)?
} catch {
  ParseError::Empty => empty_ast(),
  ParseError::InvalidByte(byte) => replacement_ast(byte),
};
```

`try` bodyが成功した場合はbodyの値を返します。body内の`raise`または`?`がerrorを伝播した場合は対応するcatch armを評価します。catchはerror型に対してexhaustiveでなければならず、成功値とすべてのarmは同じ型を返します。catch patternには通常のenum patternに加え、error value全体を受け取るbare identifier bindingを使えます。

handlerが同じまたは別のerrorを再度`raise`する場合、そのerrorはhandlerを囲むfunctionのcontractへ含まれなければなりません。

catch armは、その`try`が処理するerror targetの外側として評価します。catch arm内の`raise`と`?`は同じcatchへ戻らず、さらに外側の`try`、存在しなければfunction contractへ伝播します。

## `Option`、`Result`との関係

`Option[T]`と`Result[T, E]`は通常のenum valueです。typed `raise`の内部表現ではありません。errorをfirst-class valueへ変える場合は明示的にhandlerを書きます。

```altc
let result: Result[Ast, ParseError] = try {
  Result::Ok(parse(input)?)
} catch {
  error => Result::Err(error),
};
```

## `defer`との関係

`raise`または`?`でscopeを離れるときも、登録済み`defer`を通常のscope退出と同じLIFO順で実行します。defer body自体はraiseも伝播もできないため、元のerrorがcleanupによって置き換わることはありません。

## C11への意味変換

専用例外runtime、stack unwinding、暗黙allocationは禁止します。内部functionの具体的なC signatureは非規範的ですが、成功とerrorを通常のC return、out parameter、branch、tagged valueなどで表現しなければなりません。

### `export "C"`のraising ABI

raising functionを`export "C"`する場合は、次のABIを使います。

- 元のparameterを宣言順に置く。
- success型が`unit`でなければ`T *out_value`を追加する。
- `E *out_error`を最後に追加する。
- C return型を`int32_t`とする。
- success時は`0`を返して`out_value`だけを初期化する。
- error時は`1`を返して`out_error`だけを初期化する。
- out pointerはnon-null契約であり、C callerがnullを渡した場合の動作はundefined behaviorとする。

success型とerror型はいずれも[C ABI-safe](c-interop.md#c-abi-safeな型)でなければなりません。statusのための共有typedefや外部symbolは生成しません。この公開ABI以外のtemporary、label、helper、内部signatureは実装を拘束しません。
