# altc CLI

CLIの実装は[altc-cli](../crates/altc-cli/src/main.rs)、end-to-end contractは
[integration test](../crates/altc-cli/tests/cli.rs)です。最新の引数一覧は`altc --help`と各
subcommandの`--help`を正とします。

## command

```sh
altc check FILE [--ffi]
altc emit-c FILE [--out-dir DIR]
altc build FILE [-o PATH] [--kind exe|staticlib]
altc run FILE [-o PATH] -- [PROGRAM_ARGS...]
```

- `check`は解析、名前・型・control・effect検査だけを行う。`--ffi`を付け、extern宣言がある
  場合だけC compilerでFFI contractを検証する。
- `emit-c`は`UNIT.c`、必要なら`UNIT.h`と`UNIT_ffi_check.c`を出力する。既定directoryは
  `target/altc`。
- `build --kind exe`は`@entry`を要求する。`build --kind staticlib`は`@entry`を拒否し、
  `libUNIT.a`を生成する。extern宣言があればbuild前のFFI検証は省略できない。
- `run`はexecutableをbuildして起動し、`--`以降をそのまま渡す。processの終了codeをCLIの
  終了codeとして返す。

build途中のC source、header、object、FFI検証TUは`target/altc/UNIT/`に置かれます。

## 共通option

| option | 動作 |
| --- | --- |
| `--message-format human|json` | human診断または1 record 1行のNDJSON |
| `--show-memory-summary` | 解析成功時にfunctionごとのsummaryを表示 |
| `--cc COMMAND` / `--ar COMMAND` | C compilerとarchiverを選択 |
| `-I DIR` / `-D NAME[=VALUE]` | C compileとFFI検証へ渡す |
| `-L DIR` / `-l LIB` | executable linkへ渡す |
| `--cflag FLAG` / `--ldflag FLAG` | C compileまたはlinkへ追加 |
| `--verbose` | 生成pathと外部commandをstderrへ表示 |

生成Cは常に`-std=c11 -pedantic-errors -Wall -Wextra`でcompileします。FFI検証にはさらに
`-Werror -fsyntax-only`を加えます。

## 出力と終了code

通常成功時はquietです。human診断は`ALTCxxxx` code、source抜粋、byte span、行・columnを
表示します。JSON modeはdiagnostic、memory summary、tool output、tool diagnosticをNDJSONで
出します。

- `0`: command成功。ただし`run`はprogramの終了codeを返す。
- `1`: source読込、frontend、C生成、toolchain、実行開始の失敗、または終了codeを取得できない
  program。
- `2`: CLI引数のusage error。
