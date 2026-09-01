# 明示的なtyped raise伝播

## 決定

raising callからerrorを伝播する位置にはpostfix `?`を必須とします。`?`は`Option`や`Result`へoverloadせず、typed `raise`専用とします。

## 理由

raising callはC11でstatus判定とearly returnへloweringされます。function contractだけでなくcall siteにも`?`を残すことで、通常経路を離れうる位置をsourceから追跡できます。error型の変換は`try ... catch`へ集約し、暗黙変換を導入しません。

## 不採用

call siteに記号を付けない自動伝播は短く書けますが、制御分岐を局所的に読めないため採用しません。`Result`への汎用`?`はtyped `raise`とvalueとしてのerrorを混同するため採用しません。
