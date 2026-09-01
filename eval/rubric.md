# 言語評価rubric

各項目を1（利用を妨げる）から5（意図が自然に表れ、修正も容易）で評価します。

| 項目 | 観察する内容 |
| --- | --- |
| 構文の書きやすさ | 定型記述量、制御構文、型注釈、C連携記述の局所性 |
| 意味的一貫性 | place/value、mutability、nullability、評価順の予測可能性 |
| effect/memoryの理解容易性 | 宣言effect、`?`、memory summaryからcostを追えるか |
| 診断のactionability | codeとspanだけでなく、次の修正が分かるか |
| C連携の正直さ | ABI、header検証、生成物がsourceの契約を隠さないか |
| repair convergence | 一つの診断から小さい変更で正しいprogramへ到達できるか |

評価ごとに使用commit、実行コマンド、各項目の根拠を残します。発見事項には最小probeとP0/P1/P2分類を必須とし、P0は同じ変更で修正と回帰テストを追加します。
