# C連携

## 基本方針

altcはC headerを自動importしません。利用者は元headerを指定した手書き宣言をaltcへ置き、compilerは生成Cで同じheaderをincludeします。altc compilerは手書き宣言をaltc側の契約として型検査します。

headerをincludeするだけではCのimplicit conversionにより不一致を見逃す場合があるため、通常の生成Cとは別にFFI検証用translation unitを生成します。`altc build`、`altc test`、`altc check --ffi`は、対象C compilerでこのtranslation unitを検査しなければなりません。

system headerにはangle bracket、project headerにはquoted pathを使います。

```altc
extern "C" from <stdio.h> {
  fn puts(message: *const c_char) -> c_int;
}

extern "C" from "widget.h" {
  opaque type Widget;
  fn widget_create() -> ?*mut Widget with { alloc };
}
```

extern block内の宣言はbodyを持ちません。元headerを指定しないC宣言は禁止します。

## C ABI-safeな型

`extern "C"`と`export "C"`のsignatureにはC ABI-safeな型だけを使えます。

- 固定幅数値型、C ABI数値型、`bool`、`char`
- function return位置の`unit`
- const/mutのnon-null・nullable pointer
- `@repr(C)`を付けたpayloadなしenum
- `@repr(C)`を付け、fieldがすべてC ABI-safeなstruct
- C calling conventionを持つfunction pointer
- C側で定義されたopaque type、struct、enumへのpointer

通常の`str`、slice、payload付きenum、通常layoutのstruct、genericが残る型、arrayのby-value parameter・returnは直接公開できません。必要な場合はpointerとlength、または`@repr(C)` wrapper structを明示します。

```altc
@repr(C)
struct ByteView {
  data: *const u8,
  len: usize,
}
```

`@repr(C)` structはfield順を維持し、対象C compilerのlayoutとalignmentを使います。fieldの追加・削除・並べ替えはABI変更です。通常型のimplementation-defined layoutをC ABIへ公開することはできません。

## Function、variable、constant

```altc
extern "C" from "state.h" {
  var state_counter: c_int;
}

extern "C" from <stdio.h> {
  const EOF: c_int;
  fn fclose(stream: *mut FILE) -> c_int;
  opaque type FILE;
}
```

`var`はC objectを参照し、そのread/writeはmemory access summaryの`global-read`または`global-write`です。`var`自体はeffect contractを持ちません。`const`はheaderが提供するobject-like macroまたはenum constantを表し、altc側の型を必須とします。compilerは別のstorageを生成しません。

extern functionがstorage lifecycleを操作する場合、`alloc`と`free`は宣言者が明記します。compilerはC bodyからeffectを推論せず、宣言を信頼します。extern functionまたは`macro fn`のcallは、宣言したeffectとは別にmemory access summaryへ`extern-call`を追加します。C functionのreturn code、`errno`、nullable resultをtyped `raise`へ自動変換しません。通常のaltc wrapperで検査して`raise`します。

## C structとenum

fieldへアクセスするC structはextern block内にlayoutを宣言します。pointer越しにしか使わないtypeは`opaque type`にします。

```altc
extern "C" from "point.h" {
  @c_name("point")
  struct CPoint {
    x: c_double,
    y: c_double,
  }

  @c_name("color")
  enum CColor {
    @c_name("COLOR_RED") Red,
    @c_name("COLOR_GREEN") Green,
    @c_name("COLOR_BLUE") Blue,
  }
}
```

extern typeの宣言はaltc type checkerのために使い、生成Cでは元headerのtypeを参照します。field、variant、型対応が誤っていれば、該当する生成CをC compilerが拒否できる形にしなければなりません。

extern `struct`または`enum`へ付ける`@c_name("name")`は、それぞれC tag `struct name`または`enum name`を指定します。C typedef名を使うextern typeには`@c_typedef("Name")`を使います。両方を同じ宣言へ指定できません。attributeを省略した`opaque type FILE;`はaltc名と同じC typedef `FILE`を参照します。

```altc
extern "C" from "handle.h" {
  @c_typedef("project_handle")
  opaque type Handle;
}
```

## FFI検証

FFI検証用translation unitは元headerをincludeし、手書きaltc宣言から期待されるC型とheaderが提供する実際の宣言を比較します。検証は少なくとも次を対象にします。

- functionとcallback：addressのfunction pointer型、parameter、return、variadicの有無
- variable：object addressのpointer型とconstness
- C struct・enum・opaque type：tagまたはtypedef spellingの存在
- fieldへaccessするstruct：`sizeof`、`_Alignof`、`offsetof`、各fieldの型
- enum variantとobject-like macro：指定したC identifierの存在と利用位置の型
- function-like macro：各callでmaterializeしたargumentとresultの型

実装はC11の`_Static_assert`、`_Generic`、`_Alignof`、`offsetof`、互換pointer型への代入などを組み合わせられます。単なるwarningへ依存せず、宣言不一致が検証translation unitのcompile failureにならなければなりません。

`altc check`のfrontend-only検査はC toolchainを起動せず、extern宣言を未検証のcontractとして扱います。`altc check --ffi`は検証を実行します。`altc build`と`altc test`はFFI検証を省略できません。LSPは同じheader、target、C optionに対する検証結果をcacheでき、未検証または古い結果をverifiedとして表示してはなりません。

## Macro

```altc
extern "C" from <ctype.h> {
  macro fn isspace(value: c_int) -> c_int;
}
```

object-like macroは`const`、function-like macroは`macro fn`で宣言します。macro functionはcall位置だけで使え、address取得やfunction pointerへの変換はできません。argumentは通常のaltc評価順で一度ずつ評価したvalue temporaryを介してC macroへ渡し、C macroによる多重評価を表面意味へ漏らしてはなりません。

v0の`macro fn`はvalue argumentからvalue resultを得るexpression-like contractだけを表します。元のaltc argumentのlvalue identityはmacroへ渡りません。C macroがparameterへ代入しても変更されるのは生成temporaryだけで、元のaltc placeは変更されません。declaration、複数statement、callerのcontrol flow、元argumentのaddress identityへ依存する意味は表現できません。処理系がmacro bodyを解析する必要はありませんが、生成temporaryを使ったcallがC expressionとして成立しない、またはresult型が一致しない場合はFFI検証かC compileでerrorにしなければなりません。

## Variadic function

```altc
extern "C" from <stdio.h> {
  fn printf(format: *const c_char, ...) -> c_int;
}

printf(c"count=%d\n", count as c_int);
```

variadic argumentはC default argument promotion後の型を明示しなければなりません。整数は`c_int`以上、浮動小数点は`c_double`、pointerは対応するC pointer型へ明示変換します。compilerは固定幅型を推測でpromoteしません。

## Callback

C callbackには`extern "C" fn`型を使います。

```altc
type CompareFn = extern "C" fn(
  *const unit,
  *const unit,
) -> c_int;
```

non-capturing functionだけをC callbackへ渡せます。contextが必要なAPIでは、Cと同じくcontext pointerとcallbackを別argumentで渡します。memory access summaryはcallback型の契約ではありません。altc独自effectをC callerへ伝播できないため、C callbackは`raise[E]`を持てません。

## C ABI公開とheader生成

`export "C"` functionと、そこで必要な`@repr(C)` typeからC headerを生成します。headerには必要なstandard header、include guard、宣言、明示したC symbol名を含めます。

generic function、C ABI-safeでない型、capturing callableはexportできません。`raise[E]`を持つexport functionは[status/out ABI](memory-effects-and-errors.md#export-cのraising-abi)を使います。

C ABI-safeなnon-null pointerはC header上では通常pointerと同じ表現です。C callerがnullを渡した場合は[target仕様](target-and-undefined-behavior.md)で定めるundefined behaviorとなります。

`@c_name("name")`と`@c_typedef("name")`は宣言のC spellingを固定します。指定名がC identifierでない、対象C implementationで予約済み、または他symbolと衝突する場合はerrorです。

## 複数翻訳単位

compilerはmodule graphをC translation unitとheaderへ分割できます。分割数とprivate declarationの配置は非規範的です。公開header、C symbol、初期化順序、altcの評価順は分割方法に依存してはなりません。
