# altc言語コンセプト

## 目的

altcは、Cの低レベル性、単純な実行モデル、C ABI、既存資産との接続を保ちながら、人間がCを書く際に背負う型の取り違え、暗黙変換、副作用の把握、ビルドや編集支援の分断を言語とツールチェインへ移すための現代的なフロントエンドです。

altcはCソース互換を目標にしません。既存のヘッダ、ライブラリ、コンパイラ、リンカ、デバッガ、Make、CMake、Ninjaなどと共存できることを優先します。ソースファイルには`.altc`を使い、標準の出力先はC11とします。

## 設計原則

### Cへの薄さ

生成物のデータ配置、関数呼び出し、分岐、メモリ操作を、おおよそソースから予測できることを重視します。隠れた確保、GC、スタックアンワインド、暗黙の動的ディスパッチ、必須の専用ランタイムは導入しません。

抽象化のコストがゼロであるとは、常に最適な機械語になるという意味ではありません。同じ意味を手書きの低レベルコードで表現した場合に不要となるallocator、iterator object、virtual dispatchなどを言語機能だけのために追加しないという意味です。

### 明示性と強い型

固定幅整数とC ABI固有型を区別し、整数を真偽値へ暗黙変換しません。数値変換、nullable pointer、メモリ操作は型と構文へ、確保、解放、失敗可能性は関数契約へ表します。

通常経路を離れるcontrol flowもcall siteから追跡可能にします。型付き失敗を伝播するcallには`?`を付け、nullable pointerの分岐には`null`と`nonnull(...)`を使います。通常の`Option[T]`とnullable pointerを構文上も意味上も同一視しません。

強い型はメモリ安全の保証と同義ではありません。altcは生ポインタを扱い、無効なアドレス、範囲外アクセス、解放後参照などについてCと同じ責任を残します。保証しない安全領域を`unsafe`という名前で囲うことはしません。

### 式中心の表面構文

ブロック、`if`、`match`を値を返す式として扱います。Cに近い`while`とC風`for`を残し、配列とスライスには静的に添字ループへ変換できるrange-forを提供します。Cの事故につながりやすい暗黙の真偽値化や条件演算子は持ち込みません。

### 資源effectとメモリアクセスの可視化

storageの取得を`alloc`、解放を`free`として関数契約へ記述します。型付き失敗は`raise[E]`で表し、専用例外ランタイムではなく通常のC分岐へ変換します。

pointer越しのread/write、外部variable、volatile access、Cやfunction pointerを介するcallは、source-level effectにはしません。これらはcompilerがmemory access summaryとして推論し、LSP、doc、lintから問い合わせ可能にします。signatureを短く保ちながら低レベル操作の監査情報を失わないための分離です。effect集合が空でも、関数が純粋であることやメモリへ触れないことは意味しません。

effectは操作の存在を追跡しますが、所有権や資源ライフサイクルを証明しません。特に`free`は、二重解放や解放漏れがないことを保証するものではありません。

altcのstructとenumは、資源を含む場合も通常のcopy可能なvalueです。move-only型、自動drop、RAIIは導入せず、手動cleanupを補助する構文として`defer`を提供します。`alloc`と`free`は監査可能な操作分類であり、値の所有権を暗黙に付与しません。

### 既存Cとの接続

Cヘッダに対応する宣言をaltc側へ明示し、生成Cでも元ヘッダをincludeします。固定幅型と`c_int`などのABI型は自動的に同一視しません。altcモジュール内の公開と、C ABIとしての公開も分離します。

headerをincludeするだけではCの暗黙変換によって宣言不一致を見逃すことがあるため、buildと明示的なFFI検査ではC compilerへ検証用translation unitを渡します。関数、変数、型layoutの一致をC compiler自身に判定させ、手書き宣言を無検証のABI事実として扱いません。

### LSPファーストと機械可読性

Parserはコメント、空白、括弧、区切り、ソース位置を保持するCSTを作ります。型、symbol、参照、effect、memory access summary、ドキュメント、診断は、C生成だけでなくLSP、CLI、テスト、ドキュメント生成から共有できる意味情報として扱います。

AIに扱いやすい設計とはAI専用構文を増やすことではありません。暗黙動作を減らし、構造化診断と問い合わせ可能な意味情報を提供することで、人間とツールの双方が意図を追跡しやすくすることを指します。

## 採用する言語の形

- 固定幅数値、Unicode文字、非所有UTF-8文字列、C ABI型
- 非nullを既定とするconst/mut pointerと、明示的なnullable pointer
- 配列、スライス、構造体、payload付きenum、網羅的pattern match
- 局所型推論、明示cast、決定的なoperator precedence
- 静的generic、trait、methodとmonomorphization
- 型付き`raise`、`try ... catch`、`defer`
- `::`で修飾するmodule、import、altc内公開、C ABI公開、明示entrypoint
- 手書き宣言を介したC関数、型、変数、macro、callbackとの連携
- formatter、test、doc、LSPを含む統合CLI

個別の構文と意味は[Draft規範仕様](spec/README.md)だけで定義します。

## 非目標

次はaltcの目的とコストモデルに含めません。

- Rust型の所有権・借用、move-only・affine型、自動drop、RAII、参照型、`unsafe`境界
- class、継承、trait object、暗黙のvirtual table、標準の動的ディスパッチ
- 捕捉closureと暗黙の環境確保
- GC、自動メモリ管理、必須の専用ランタイム
- 言語組み込みの`async`、scheduler、非同期ランタイム
- ユーザー定義のtextual macro、一般的なcompile-time metaprogramming
- Cソースとの完全なソース互換
- 独自package manager、中央registry、既存Cビルド環境の排除
- 独自debuggerやIDE専用protocol。LSP、`#line`、既存C debuggerとの接続を使う

## 参考元

Rust、MoonBit、[`mizchi/vibe-lang`](https://github.com/mizchi/vibe-lang)を、型、effect、構文、開発体験を考える際の参考とします。altcはこれらの処理系やパッケージに依存せず、コンパイラとCLIをRustで実装します。
