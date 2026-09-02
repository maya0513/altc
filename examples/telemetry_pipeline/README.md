# telemetry pipeline example

複数module、array/slice、range-for、公開data型、nullable pointer、effect、typed error、nested `defer`、extern C object/layout/callback、C exportを一つのprogramで使う統合例です。先に小さなC libraryを作り、altc CLIへheaderとlibraryの探索pathを渡します。

```sh
cc -std=c11 -pedantic-errors -Wall -Wextra -Werror -I examples/telemetry_pipeline -c examples/telemetry_pipeline/telemetry.c -o target/telemetry.o
ar rcs target/libtelemetry.a target/telemetry.o
cargo run -p altc-cli -- run examples/telemetry_pipeline/main.altc -I examples/telemetry_pipeline -L target -l telemetry --cflag=-Werror
```

正常時は`pipeline cleanup`を表示し、process終了code `48`を返します。`api.altc`の`telemetry_altc_score`は生成public headerへ含まれます。
