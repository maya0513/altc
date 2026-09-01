# C compilerによるFFI検証

## 決定

extern宣言は手書きのまま維持し、buildと明示FFI checkで検証用C translation unitを生成します。対象C compilerにfunction・variableの型、struct layout、field、enum、使用したmacroを検査させます。

## 理由

元headerをincludeするだけでは、Cのimplicit conversionが誤ったaltc宣言を受理する場合があります。一方、header importerを言語の必須機能にするとC parserとpreprocessor全体を抱えます。小さな検証用Cを正本compilerへ渡す方式なら、手書き宣言と実際のtarget ABIを照合できます。

## 制約

function-like macroはvalue引数をtemporary経由で一度ずつ評価するcall形だけを扱います。元argumentのlvalue identityは渡さず、macro内のparameter代入はtemporaryだけを変更します。declaration、statement、control flow、元argumentのaddress identityを要求する意味は表現しません。
