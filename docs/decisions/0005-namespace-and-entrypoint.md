# `::` namespaceと明示entrypoint

## 決定

module、type、variant、静的memberのpath separatorを`::`へ統一します。`.`はvalue、`->`はpointer越しのmember accessに限定します。executableの開始functionは名前だけで特別扱いせず、`@entry`で明示します。

## 理由

namespaceとvalue memberを別tokenにするとparser、name resolution、diagnosticが単純になります。`@entry`はaltc内functionとC ABIの`main`を分離し、multi-module projectでも開始位置を一意に表せます。
