# 字句構造

## Source encoding

altcソースはUTF-8で符号化します。不正なUTF-8 byte列とsource先頭のBOMは字句errorです。改行はLFまたはCRLFを受理し、source spanを保持したまま内部では一つの改行tokenとして扱います。

Lexerはすべてのtoken、コメント、空白、改行、source spanを保持し、Parserが構築するCSTへ渡さなければなりません。

## 空白とコメント

space、tab、改行はtokenを区切ります。通常は意味を持ちませんが、CSTではtriviaとして保持します。

```altc
// 行コメント

/* block comment */
```

`//`は改行直前まで、`/*`は次の`*/`までをコメントとします。block commentはnestしません。閉じられていないblock commentは字句errorです。

`///`で始まる連続した行コメントは、直後の公開可能なfunction、type、module-level declaration、testへ付くMarkdownドキュメントコメントです。別のblock形式のドキュメントコメントはありません。一つの概念に複数の綴りを持たせないためです。

## Identifier

identifierはASCIIだけを使い、次の正規表現に一致しなければなりません。

```text
[A-Za-z_][A-Za-z0-9_]*
```

大文字と小文字は区別します。keywordをidentifierとして使うことはできません。同一scope内の重複宣言はerrorです。内側のscopeは外側の名前をshadowできます。

## Keyword

次の語はkeywordです。

```text
as break catch const continue defer else enum export extern false
fn for from if impl import in let macro match mut null opaque pub
raise return struct test trait true try type var while with
```

`Self`はtraitとimplの内部でだけ特別なtype nameとして扱います。`@repr`、`@c_name`など、`@`で始まる名前はlanguage attributeです。

`nonnull`はkeywordではなく、nullable pointerを分解するpatternの先頭でだけ特別な意味を持つcontextual keywordです。`@entry`、`@c_typedef`を含むattribute名はidentifierと同じASCII規則に従います。

`::`はmodule、type、enum variantなどのnamespace pathを区切ります。`.`はvalueのfieldとmethod、`->`はpointer越しのfieldとmethodに使います。postfix `?`はtyped `raise`を伝播するoperatorであり、type先頭の`?*`はnullable pointerを表します。

`extern "C" from`の直後だけは、`<stdio.h>`形式または`"project.h"`形式を一つのheader nameとして字句解析します。通常式の`<`、`>`、文字列literalとは混同しません。

## 整数literal

整数literalは10進、2進、8進、16進を使えます。digit間の`_`は値へ影響しません。prefix直後、末尾、連続した`_`は禁止します。

```altc
42
1_000_000
0b1010
0o755
0xff
255u8
-12i32
```

型suffixには`i8`、`i16`、`i32`、`i64`、`u8`、`u16`、`u32`、`u64`、`isize`、`usize`、C ABI整数型を使えます。単項`-`はliteralの一部ではなくoperatorです。

suffixがないliteralは周囲から整数型を推論し、文脈がなければ`i32`になります。定数評価時に対象型で表現できない値はerrorです。

## 浮動小数点literal

浮動小数点literalは10進の小数部または指数部を持ち、`f32`または`f64` suffixを指定できます。文脈もsuffixもなければ`f64`です。

```altc
0.5
1.0e-6
3.25f32
```

## 文字と文字列

```altc
let scalar: char = 'λ';
let byte: u8 = b'A';
let text: str = "hello";
let bytes: []const u8 = b"raw bytes";
let c_text: *const c_char = c"hello";
```

- `'…'`はUnicode scalar valueを一つだけ含む`char`です。
- `b'…'`は一つのbyteを含む`u8`です。
- `"…"`は静的storageを指すUTF-8の`str`です。末尾NULは契約に含みません。
- `b"…"`は静的storageを指すread-only byte sliceです。
- `c"…"`はUTF-8 byte列の末尾へNULを一つ加えた`*const c_char`です。内容にNULを含めてはなりません。

通常文字列と文字literalでは`\\`、`\"`、`\'`、`\n`、`\r`、`\t`、`\0`、`\xNN`、`\u{HEX}`を使えます。byte literalとbyte stringでは結果が1 byteになるescapeだけを使えます。文字列のUnicode normalizationは行いません。

## 真偽値とnull

`true`と`false`は`bool`です。`null`はnullable pointer型の文脈でだけ使えます。整数0をpointerへ、pointerを`bool`へ暗黙変換しません。
