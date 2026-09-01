# Language builtin

このpageはimportなしで使えるlanguage builtinの型と意味を定義します。builtinは通常の名前解決でshadowできず、ここにないoverloadを処理系が暗黙に追加してはなりません。明記しないbuiltinのeffect集合は空です。`Option`、`Result`、test harness builtinは[宣言仕様](declarations-and-modules.md#import)で別に定義します。

## Layout query

```altc
sizeof[T]
alignof[T]
```

`T`は具体化済みのstorable typeでなければなりません。どちらも`usize`型のcompile-time valueで、選択targetとimplementation-defined layoutにおける実際のobject sizeとalignmentを返します。通常のcall括弧は付けません。

## Pointer qualifierと型変換

`ptr_cast[U]`はpointee型だけを変え、const/mutとnullable性を保存します。

```text
ptr_cast[U](*const T)  -> *const U
ptr_cast[U](*mut T)    -> *mut U
ptr_cast[U](?*const T) -> ?*const U
ptr_cast[U](?*mut T)   -> ?*mut U
```

`ptr_as_const`はwrite capabilityを除きます。

```text
ptr_as_const[T](*mut T)  -> *const T
ptr_as_const[T](?*mut T) -> ?*const T
```

`ptr_cast_mut[U]`はpointee型を変えながらconst pointerをmut pointerへ変換する低レベル操作です。

```text
ptr_cast_mut[U](*const T)  -> *mut U
ptr_cast_mut[U](?*const T) -> ?*mut U
```

これらはaddress valueを変更せず、alignment、lifetime、effective type、aliasing、実際のobject mutabilityを検証しません。contractを満たさない結果のdereferenceはundefined behaviorです。`ptr_cast_mut`自体はmemory access summaryへread/writeを追加しません。

## Pointer arithmeticとaddress

```text
offset[T](*const T, isize) -> *const T
offset[T](*mut T, isize)   -> *mut T

ptr_addr[T](*const T)  -> usize
ptr_addr[T](*mut T)    -> usize
ptr_addr[T](?*const T) -> usize
ptr_addr[T](?*mut T)   -> usize

ptr_from_addr[T](usize)     -> ?*const T
ptr_from_addr_mut[T](usize) -> ?*mut T
```

`offset`の`T`はstorableでなければならず、element単位でaddressを進めます。結果は元のarray objectまたはone-pastの範囲でだけC pointerとして有効です。`ptr_addr`はnullを0、それ以外をtargetのaddress整数へ変換します。

`ptr_from_addr`と`ptr_from_addr_mut`は0を`null`、0以外を同じaddress表現のnullable pointerへ変換します。これらはnon-null、alignment、lifetime、provenanceを保証しません。利用者は`nonnull(...)`でnullを処理した後も、dereference前に残りのC contractを満たす責任を持ちます。

pointer builtinのcall自体はmemory access summaryへread/writeを追加しません。結果をdereferenceすると通常どおり`indirect-read`または`indirect-write`を追加します。

## 数値変換

```text
checked_cast[U](value: T) -> Option[U]
truncate[U](value: T)     -> U
char_from_u32(value: u32) -> Option[char]
```

`checked_cast`の`T`と`U`はboolとcharを除く数値型です。数学的な値が`U`で表現できる場合だけ`Some`を返します。floatから整数では0方向へ丸めた値を使い、NaN、無限大、範囲外を`None`にします。

`truncate`の`T`と`U`は整数型です。valueのtwo's-complement bit列の下位`U`幅を取り、`U`として解釈します。destination幅がsource幅以上の場合はerrorです。符号の有無にかかわらずbitを保存する操作で、数値範囲検査には`checked_cast`を使います。

`char_from_u32`はvalueがUnicode scalar valueなら`Some(char)`、surrogate領域または`0x10ffff`を超える場合は`None`を返します。

## 検査付き整数演算

同じ整数型`T`のoperandに対して次を提供します。

```text
checked_add[T](T, T) -> Option[T]
checked_sub[T](T, T) -> Option[T]
checked_mul[T](T, T) -> Option[T]
checked_div[T](T, T) -> Option[T]
```

数学的resultを`T`で表現できなければ`None`です。`checked_div`は0除算と符号付き最小値 / `-1`にも`None`を返します。これらはinteger overflowのundefined behaviorを起こしません。

wrapping `+%` / `-%` / `*%`とsaturating `+|` / `-|` / `*|`はoperatorであり、builtin functionではありません。

## `str`とsliceの組み込みmember

`str.byte_len`、array・sliceの`len`はread-only propertyで、storageへ間接accessせず保持済みlengthを`usize`で返します。call括弧は付けず、引数や内容をcopyせず、memory access summaryへread/writeを追加しません。

array・sliceの`get(index)`はboundsを検査し、const viewでは`?*const T`、mut viewでは`?*mut T`を返します。範囲外なら`null`です。`get`自体はelementをdereferenceしないためmemory access summaryへread/writeを追加しません。

## Slice constructorとrange

```text
slice_range[T]([]const T, usize, usize) -> Option[[]const T]
slice_range[T]([]mut T, usize, usize)   -> Option[[]mut T]

slice_range_unchecked[T]([]const T, usize, usize) -> []const T
slice_range_unchecked[T]([]mut T, usize, usize)   -> []mut T

slice_from_parts[T](*const T, usize) -> []const T
slice_from_parts[T](*mut T, usize)   -> []mut T

empty_slice[T]()     -> []const T
empty_slice_mut[T]() -> []mut T
str_bytes(str)       -> []const u8
```

rangeは半開区間`start..end`です。`slice_range`は`start <= end && end <= values.len`なら`Some`、それ以外なら`None`を返します。`slice_range_unchecked`とrange syntax `values[start..end]`は同じbounds前提を要求し、違反はundefined behaviorです。

`slice_from_parts`はdataから`len` elementが有効な一つのobject範囲にあり、指定mutabilityでaccess可能であることをcallerへ要求します。空sliceでもnon-null dataを要求します。pointerを持たない空sliceが必要な場合は`empty_slice`または`empty_slice_mut`を使います。空sliceの実際のdata pointer表現はimplementation-definedです。

`str_bytes`は同じUTF-8 storageを指すread-only byte sliceを返し、copy、validation、allocationを行いません。これらのconstructorは要素をdereferenceせず、memory access summaryへread/writeを追加しません。
