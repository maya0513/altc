# `memory`をeffectにせずaccess summaryとして推論

## 決定

v0のsource-level effectは`alloc`、`free`、`raise[E]`に限定し、`memory` annotationを設けません。pointer・slice越しのread/write、extern variable、volatile operation、extern call、function pointer callはcompilerがmemory access summaryとして推論し、LSP semantic query、documentation、opt-in lintへ公開します。

effect集合が空のfunctionは`tracked-effect-free`と表現し、pureとは呼びません。memory access summaryはfunction signature、function pointer型、trait適合、C ABIに含めません。

summaryは解析したsource revisionに対する観測情報です。bodyの変更でsummaryが変わってもsource compatibilityやABIの変更とは扱いません。

## 理由

低レベルprogramではpointer accessやC callが広く現れます。これらを一つの`memory` effectへまとめると、多くのsignatureへ同じannotationが伝播する一方、readかwriteか、globalかvolatileか、C境界かを利用者へ伝えられません。型契約としての負担に対して、得られる保証が弱すぎます。

一方、memory operationの可視性は監査、navigation、lintに有用です。型互換性を変えない推論summaryなら、sourceの書き心地を保ちながら詳細な根拠spanとcall pathを提示できます。`alloc`、`free`、`raise[E]`はstorage lifecycleやcontrol flowをcallerへ伝える明確な契約なのでeffectとして残します。

## 不採用

単一の`memory` effectを維持する案は、annotationが広く伝播する割に情報量が少ないため採用しません。`read`と`write`をsource-level effectへ分割する案も、alias、callback、extern bodyを含む正確な契約が必要になり、v0の複雑性に見合いません。

summaryが空であることをpurityや最適化許可として扱う案も採用しません。extern callとindirect callの内部、終了性、I/Oなどをsummaryだけでは証明できないためです。const evaluation、pure callback、sandbox、concurrencyなど具体的なconsumerが将来必要になった時点で、別の狭いcontractを設計します。
