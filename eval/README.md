# altc言語評価

このディレクトリは、言語の書き心地と診断の修復可能性を同じ入力で継続評価するための固定コーパスです。`cargo xtask eval`は次を検査します。

- `writability/`: 9領域の課題に対する検証済みgolden。基本機能に加え、array/slice、extern callback/layout、module graphの解析、C生成、strict C11検査または実行結果、必要なmemory summaryを確認する。
- `repair/`: 12種類の壊れたsourceの診断JSONをsnapshot比較し、対応する`fixed.altc`がC生成まで成功することを確認する。array bounds、callback contract、module privacyも含む。
- `scores/`: 固定rubricによる評価結果。
- `findings/`: 最小probe、P0/P1/P2分類、次の行動を伴う所見。

意図した診断変更だけを確認した後、`cargo xtask eval --update`でsnapshotを更新します。評価方法の正本は[言語評価rubric](rubric.md)です。
