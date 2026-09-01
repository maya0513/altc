# 公開C ABI

`@repr(C)`のerror enumと、C名を固定したraising exportを定義する。生成されるpublic headerが
statusとout parameterによる規範ABIを公開し、strict C11で検査できることを求める。
