# 設計判断

このdirectoryは、複数の選択肢がある言語設計について、採用した方向と理由を記録します。規範的な構文と意味は[Draft仕様](../spec/README.md)だけが定義し、このdirectoryと矛盾する場合は仕様を優先します。

- [明示的なtyped raise伝播](0001-explicit-error-propagation.md)
- [copy可能な資源値](0002-copyable-resource-values.md)
- [C compilerによるFFI検証](0003-ffi-verification.md)
- [観測可能なimplementation-defined layout](0004-observable-layout.md)
- [`::` namespaceと明示entrypoint](0005-namespace-and-entrypoint.md)
- [`memory`をeffectにせずaccess summaryとして推論](0006-memory-access-summary.md)
