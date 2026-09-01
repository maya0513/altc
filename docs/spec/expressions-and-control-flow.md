# 式と制御フロー

## 評価順

operand、function argument、receiver、array・struct initializerはsource上の左から右へ一度ずつ評価します。短絡operator、分岐、loop、`match`は選ばれた経路だけを評価します。

C11が同じ評価順を直接保証しない場合、backendはtemporaryやstatementへloweringしてaltcの順序を保たなければなりません。

## Block式とsemicolon

```altc
let total: i32 = {
  let a: i32 = 10;
  let b: i32 = 20;
  a + b
};
```

block末尾のsemicolonなし式がblockの値です。末尾が宣言、semicolon付き式、または空であれば値は`unit`です。semicolonは式の値を捨てて`unit`にします。

## Operator

強い順に次のprecedenceを使います。

| 順位 | Operator | Associativity |
| ---: | --- | --- |
| 1 | call `()`、index `[]`、member `.`、C pointer member `->`、typed raise伝播 `?` | left |
| 2 | unary `!`、`~`、`+`、`-`、`&`、`*` | right |
| 3 | cast `as` | left |
| 4 | `*`、`/`、`%`、`*%`、`*|` | left |
| 5 | `+`、`-`、`+%`、`-%`、`+|`、`-|` | left |
| 6 | `<<`、`>>` | left |
| 7 | `<`、`<=`、`>`、`>=` | non-associative |
| 8 | `==`、`!=` | non-associative |
| 9 | bitwise `&` | left |
| 10 | `^` | left |
| 11 | `|` | left |
| 12 | `&&` | left |
| 13 | `||` | left |
| 14 | `=`、`+=`、`-=`、`*=`、`/=`と対応する明示overflow代入 | right |

comparisonとequalityをchainできません。`&&`と`||`は短絡評価し、operandに`bool`を要求します。条件演算子はありません。

## Operatorの型規則

通常の二項算術は左右が同じ具体的な整数型または浮動小数点型、bitwiseは左右が同じ整数型、ordering comparisonは左右が同じ数値型でなければなりません。suffixなし数値literalは、反対側のoperandまたは期待result型からcontextual typingできます。Cのinteger promotionとusual arithmetic conversionは適用しません。

算術とbitwise operatorのresultはoperandと同じ型です。comparisonとequalityのresultは`bool`です。equalityは同じ型のbool、char、数値、payloadなしenumに使えます。struct、array、slice、str、payload付きenumには組み込みequalityを提供せず、traitまたは明示functionを使います。pointer equalityは同じpointeeとqualifierを持つpointer同士に限り、nullable pointerは同じ型の`null`と比較できます。

`&&`、`||`、`!`は`bool`だけへ、`~`は整数だけへ、unary `+`と`-`は数値だけへ適用できます。符号なし整数へのunary `-`は型のmodulo演算です。

shiftの左辺は整数型、右辺は符号なし整数型でなければならず、resultは左辺型です。runtimeのshift countが左辺型のbit幅以上である場合と、C11で定義されない符号付きleft shiftはundefined behaviorです。compile-timeに確定する不正shiftはerrorです。

`as`以外の二項operatorによって型を変換してはなりません。pointer arithmeticには通常operatorでなくpointer builtinを使います。

## Assignment

assignmentの左辺は変更可能なplaceでなければなりません。右辺を評価して格納した後、assignment式は`unit`を返します。

```altc
let mut total: i32 = 0;
let ignored: unit = total = 10;
```

compound assignmentは左辺を一度だけ評価します。assignmentが`unit`を返すため、assignment chainと条件内assignmentは型errorになります。

## `if`式

```altc
let sign: i32 = if value < 0 {
  -1
} else if value > 0 {
  1
} else {
  0
};
```

conditionは`bool`です。値として使う`if`は`else`が必須で、すべての到達可能branchが同じ型を返さなければなりません。`else`のない`if`は`unit`です。

## `while`

```altc
while ready() {
  consume();
}
```

conditionは各iterationの前に評価する`bool`です。`while`の値は常に`unit`です。

## C風`for`

```altc
for (let mut i: usize = 0; i < count; i += 1) {
  use(i);
}
```

initializerは0回または1回、conditionは各iterationの前、updateはbodyの通常完了と`continue`後に評価します。conditionを省略した場合は`true`です。`for`の値は`unit`です。

## Range-for

```altc
for value in values {
  consume(value);
}
```

range-forの対象は固定長arrayまたはsliceです。対象式を一度評価し、開始時のlengthを保持して、index 0からlength未満まで昇順に走査します。意味上同等な手書きの添字loopに存在しないruntime処理を加えてはならず、iterator object、allocation、dynamic dispatch、間接callを作ってはなりません。

loop bindingはimmutableな値copyであり、各iterationの`let value = values[index]`と同じ意味です。要素をpointer経由で変更したい場合やcopyを避けたい場合は、C風`for`と明示pointerを使います。

## `break`と`continue`

`break;`は最内のloopを終了し、`continue;`は次のiterationへ進みます。値付き`break`とlabel付きloopはありません。loop外ではerrorです。

## `match`

```altc
let value: i32 = match option {
  Option::Some(x) => x,
  Option::None => 0,
};
```

scrutineeは一度だけ評価します。patternはliteral、bare identifier binding、enum variant、payload binding、struct destructuring、`_`を扱います。bare identifierはそのarmへ到達したvalue全体をbindします。到達可能なvalueをすべて覆わない`match`はerrorです。重複して到達不能なarmもdiagnosticになります。

nullable pointerには`null`と`nonnull(name)`だけをvariant patternとして使います。`nonnull(name)`がbindする型は元のnullable pointerから`?`を除いたnon-null pointer型です。`Option::Some`と`Option::None`をnullable pointerへ使うことはできません。

値として使うすべてのarmは同じ型を返します。guardとor-patternは提供しません。

## `return`

`return expression;`はexpressionを一度評価し、現在のfunctionから退出します。function return型と一致しなければなりません。末尾式と早期returnは同じreturn contractを満たします。

## Typed raise伝播 `?`

postfix `?`はtyped `raise`を持つcallのsuccess valueを取り出し、error edgeを最も近いlexical error targetへ伝播します。error targetは、最も近い`try` body、存在しなければ現在のfunctionです。

raising callableのcallはpostfix `?`なしに値として使用できません。`?`を付けられるのは`raise[E]`を一つ持つcallだけで、`Option`、`Result`、nullable pointer、通常値には使えません。functionへ伝播する場合、function自身が同じ`raise[E]`を宣言していなければerrorです。

```altc
let ast: Ast = parse(input)?;
```

`?`はsuccess時にcallのreturn valueとなり、error時に現在のexpression以降を評価しません。通常完了とerror退出のどちらでも、対象scopeの`defer`規則を守ります。

## `defer`

```altc
defer {
  release(resource);
}
```

`defer`はcontrol flowがその宣言へ到達したとき現在のlexical scopeへ登録し、scopeを抜けるとき登録と逆順にbodyを実行します。通常完了、`return`、`break`、`continue`、`raise`、`?`による伝播のすべてが対象です。loop body scope内のdeferは各iterationのscope退出ごとに実行します。

defer bodyとその引数は登録時に評価せず、scope退出時に評価します。bodyが参照するlocal bindingは退出時点の値を読みます。登録時点の値を固定したい場合は、deferより前に別のimmutable bindingへ明示的にcopyし、そのbindingをbodyから参照します。deferのためのclosure objectやcapture storageを暗黙に生成してはなりません。

defer bodyから`return`、`break`、`continue`、`raise`を実行してはならず、raising callへ`?`を付けることもできません。defer bodyは`unit`でなければなりません。bodyが発生させる`alloc`と`free`は外側functionのeffect検査へ、memory accessは外側functionのsummaryへ含めます。backendはcleanup block、複製したstatement、または等価なC11へ静的に変換し、runtime cleanup stackを必須にしてはなりません。

## 拒否する例

```altc
fn invalid(input: str) -> Ast with { raise[ParseError] } {
  parse(input) // error: raising callには?が必要
}

fn also_invalid(result: Result[i32, ParseError]) -> i32 {
  result? // error: ?はResultへ適用できない
}

let mixed = 1u8 + 2u16; // error: 通常算術のoperand型が異なる
```
