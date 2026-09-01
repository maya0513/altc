# examples

このdirectoryのprogramは、現在の実装で解析・C11生成・build・実行できる統合例です。小さな構文例ではなく、複数の言語機能が同じ実行経路で正しく組み合わさることを確認します。

## feature showcase

[feature_showcase.altc](feature_showcase.altc)は、一件の計測値を検証・正規化する小さなbatch processorです。次を一つのprogramで使用します。

- module/local `const`、型推論、mutable binding、全primitive型family
- `@repr(C)` struct・payloadなしenum、payload enumとpattern matching
- nullable/non-null・const/mut pointer、address・dereference・全pointer builtin
- `if`、`match`、`while`、C風/conditionless `for`、`break`、`continue`
- 通常・wrapping・saturating・bit・logical operatorとcompound assignment
- `alloc` / `free`、typed `raise`、postfix `?`、`try ... catch`
- 通常退出とerror propagation時のLIFO `defer`
- C header、opaque type、通常/raising `export "C"`、`@c_name`、`@entry`
- `str`、C string、Unicode `char`、整数境界値

実行するとcleanup順を標準出力へ出し、成功時に終了code `42`を返します。

```sh
cargo run --quiet -p altc-cli -- check examples/feature_showcase.altc --ffi \
  --show-memory-summary
cargo run --quiet -p altc-cli -- run examples/feature_showcase.altc --cflag=-Werror
```
