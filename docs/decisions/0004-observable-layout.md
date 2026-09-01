# 観測可能なimplementation-defined layout

## 決定

通常型のlayoutはimplementation-definedですが、選択されたsizeとalignmentは`sizeof[T]`、`alignof[T]`、`emit-c`から観測可能とします。compiler versionやtargetを越える安定ABIは`@repr(C)`だけが提供します。

## 理由

低レベルprogramとgeneric allocationには実際のobject sizeが必要です。通常型の全layoutを公開ABIへ固定せず、同時に`sizeof`の結果を意味のない非規範値にしないため、処理系が一つのlayoutを選び文書化する契約とします。
