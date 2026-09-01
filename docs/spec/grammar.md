# 規範文法

このpageはaltc sourceの構文をEBNF風に定義します。字句tokenは[字句構造](lexical-structure.md)、operator precedenceは[式と制御フロー](expressions-and-control-flow.md)を正本とします。proseの意味規則とこのgrammarが矛盾する場合、受理するtoken列はこのpage、型と動作は各意味仕様を優先します。

## 表記

- `"token"`は固定tokenです。
- `name`は別のproductionです。
- `( ... )?`は0回または1回、`( ... )*`は0回以上、`( ... )+`は1回以上です。
- `A | B`は選択です。
- comma区切りlistは末尾commaを許可します。

## Source fileとpath

```text
source_file      = item* EOF ;
path             = identifier ("::" identifier)* ;
attribute        = "@" identifier ("(" attribute_argument_list? ")")? ;
attribute_argument_list
                 = attribute_argument ("," attribute_argument)* ","? ;
attribute_argument
                 = identifier | string_literal | integer_literal ;
visibility       = "pub" ;
literal          = integer_literal | float_literal | char_literal
                 | byte_literal | string_literal | byte_string_literal
                 | c_string_literal | "true" | "false" | "null" ;
const_expression = expression ;
```

`const_expression`は構文上通常のexpressionとしてparseし、[const意味規則](declarations-and-modules.md#const)に適合しないoperationを意味検査で拒否します。

`.`はpath separatorではありません。module、type、variant、静的methodは`::`、valueのfieldとmethodは`.`、pointer越しのfieldとmethodは`->`で参照します。

## Top-level item

```text
item             = attribute* (
                     import_decl
                   | const_decl
                   | function_decl
                   | struct_decl
                   | enum_decl
                   | type_alias_decl
                   | trait_decl
                   | impl_decl
                   | extern_block
                   | test_decl
                   ) ;

import_decl      = "import" path ";" ;
type_alias_decl  = visibility? "type" identifier generic_parameters? "=" type ";" ;
test_decl        = "test" string_literal block ;
```

## Functionとeffect

```text
function_decl    = visibility? export_spec? function_signature block ;
export_spec      = "export" string_literal ;
function_signature
                 = calling_convention? "fn" identifier generic_parameters?
                   "(" parameter_list? ")" return_type? effect_contract? ;
calling_convention
                 = "extern" string_literal ;
parameter_list   = parameter ("," parameter)* ("," "...")? ","? ;
parameter        = identifier ":" type ;
return_type      = "->" type ;
effect_contract  = "with" "{" effect_list? "}" ;
effect_list      = effect ("," effect)* ","? ;
effect           = "alloc" | "free" | "raise" "[" type "]" ;
```

`export`とcalling conventionの文字列は、現在は`"C"`だけを受理します。extern block内、trait member、function pointerではbodyを持たないsignatureを使います。

## Data type

```text
struct_decl      = visibility? "struct" identifier generic_parameters?
                   "{" struct_field* "}" ;
struct_field     = attribute* visibility? identifier ":" type "," ;

enum_decl        = visibility? "enum" identifier generic_parameters?
                   "{" enum_variant* "}" ;
enum_variant     = attribute* visibility? identifier
                   ("(" type_list? ")")? "," ;

generic_parameters
                 = "[" generic_parameter ("," generic_parameter)* ","? "]" ;
generic_parameter
                 = identifier (":" trait_bound)? ;
trait_bound      = path ("+" path)* ;
generic_arguments
                 = "[" type ("," type)* ","? "]" ;
type_list        = type ("," type)* ","? ;
```

## Traitとimpl

```text
trait_decl       = visibility? "trait" identifier
                   "{" trait_member* "}" ;
trait_member     = attribute* function_signature ";" ;

impl_decl        = "impl" generic_parameters?
                   (path "for")? type
                   "{" impl_member* "}" ;
impl_member      = attribute* visibility? function_signature block ;
```

## Extern block

```text
extern_block     = "extern" string_literal "from" header_name
                   "{" extern_item* "}" ;
extern_item      = attribute* (
                     extern_function
                   | extern_macro_function
                   | extern_variable
                   | extern_constant
                   | extern_struct
                   | extern_enum
                   | extern_opaque_type
                   ) ;
extern_function  = function_signature ";" ;
extern_macro_function
                 = "macro" function_signature ";" ;
extern_variable  = "var" identifier ":" type ";" ;
extern_constant  = "const" identifier ":" type ";" ;
extern_struct    = "struct" identifier "{" struct_field* "}" ;
extern_enum      = "enum" identifier "{" enum_variant* "}" ;
extern_opaque_type
                 = "opaque" "type" identifier ";" ;
```

## Type

```text
type             = path generic_arguments?
                 | "*" "const" type
                 | "*" "mut" type
                 | "?" "*" "const" type
                 | "?" "*" "mut" type
                 | "[" type ";" const_expression "]"
                 | "[" "]" "const" type
                 | "[" "]" "mut" type
                 | function_type ;

function_type    = calling_convention? "fn"
                   "(" function_type_parameters? ")" return_type? effect_contract? ;
function_type_parameters
                 = type ("," type)* ("," "...")? ","?
                 | "..." ;
```

## Blockとstatement

```text
block            = "{" block_item* tail_expression? "}" ;
block_item       = let_statement
                 | const_decl
                 | expression_statement
                 | defer_statement ;
tail_expression  = expression ;

let_statement    = "let" "mut"? identifier type_annotation? "=" expression ";" ;
type_annotation  = ":" type ;
const_decl       = "const" identifier ":" type "=" const_expression ";" ;
expression_statement
                 = expression ";" ;
defer_statement  = "defer" block ;
```

declarationまたはsemicolon付きexpressionの後に続く最後のsemicolonなしexpressionだけがblockの値です。

## Expression

次のproductionは[operator表](expressions-and-control-flow.md#operator)と同じprecedenceを、弱いものから強いものへ定義します。relationalとequalityはそれぞれ一つのoperatorだけを許し、chainを受理しません。

```text
expression       = assignment_expression
                 | raise_expression
                 | return_expression
                 | break_expression
                 | continue_expression ;

assignment_expression
                 = logical_or_expression
                   (assignment_operator assignment_expression)? ;
assignment_operator
                 = "=" | "+=" | "-=" | "*=" | "/="
                 | "+%=" | "-%=" | "*%="
                 | "+|=" | "-|=" | "*|=" ;

logical_or_expression
                 = logical_and_expression ("||" logical_and_expression)* ;
logical_and_expression
                 = bit_or_expression ("&&" bit_or_expression)* ;
bit_or_expression
                 = bit_xor_expression ("|" bit_xor_expression)* ;
bit_xor_expression
                 = bit_and_expression ("^" bit_and_expression)* ;
bit_and_expression
                 = equality_expression ("&" equality_expression)* ;
equality_expression
                 = relational_expression (("==" | "!=") relational_expression)? ;
relational_expression
                 = shift_expression (("<" | "<=" | ">" | ">=") shift_expression)? ;
shift_expression = additive_expression (("<<" | ">>") additive_expression)* ;
additive_expression
                 = multiplicative_expression
                   (("+" | "-" | "+%" | "-%" | "+|" | "-|")
                    multiplicative_expression)* ;
multiplicative_expression
                 = cast_expression
                   (("*" | "/" | "%" | "*%" | "*|") cast_expression)* ;
cast_expression  = unary_expression ("as" type)* ;
unary_expression = ("!" | "~" | "+" | "-" | "&" | "*") unary_expression
                 | postfix_expression ;
postfix_expression
                 = primary_expression postfix_operator* ;
postfix_operator = "(" argument_list? ")"
                 | "[" expression "]"
                 | "[" expression? ".." expression? "]"
                 | "." identifier
                 | "->" identifier
                 | "?" ;

primary_expression
                 = literal
                 | path generic_arguments?
                 | array_literal
                 | struct_literal
                 | block
                 | if_expression
                 | match_expression
                 | try_expression
                 | while_expression
                 | for_expression
                 | "(" expression ")" ;

argument_list    = expression ("," expression)* ","? ;
array_literal    = "[" argument_list? "]" ;
struct_literal   = path generic_arguments? "{" field_initializer* "}" ;
field_initializer
                 = identifier ":" expression "," ;

if_expression    = "if" expression block ("else" (if_expression | block))? ;
while_expression = "while" expression block ;
for_expression   = "for" identifier "in" expression block
                 | "for" "(" for_init? ";" expression? ";" expression? ")" block ;
for_init         = "let" "mut"? identifier type_annotation? "=" expression
                 | expression ;
raise_expression = "raise" expression ;
return_expression
                 = "return" expression? ;
break_expression = "break" ;
continue_expression
                 = "continue" ;
```

`return`、`break`、`continue`はそれぞれ対応するfunctionまたはloopだけで使えるcontrol expressionです。`return`は任意のvalueを、`break`と`continue`はvalueを取りません。これらをstatementとして使う場合は後ろへsemicolonを置きます。

## Match、try、pattern

```text
match_expression = "match" expression "{" match_arm* "}" ;
try_expression   = "try" block "catch" "{" match_arm* "}" ;
match_arm        = pattern "=>" expression "," ;

pattern          = "_"
                 | identifier
                 | literal
                 | path
                 | path "(" pattern_list? ")"
                 | path "{" field_pattern* "}"
                 | "null"
                 | "nonnull" "(" identifier ")" ;
pattern_list     = pattern ("," pattern)* ","? ;
field_pattern    = identifier (":" pattern)? "," ;
```

bare identifier patternは、そのarmへ到達したvalue全体をbindします。`nonnull(identifier)`と`null`はnullable pointerだけへ適用できます。guardとor-patternはありません。

## Attributeの配置制約

grammar上attributeを受理できても、各attributeを置ける宣言は意味検査で制限します。少なくとも`@entry`はfunction、`@repr(C)`はstructとpayloadなしenum、`@c_name`はC declaration、`@c_typedef`はextern typeだけへ適用できます。
