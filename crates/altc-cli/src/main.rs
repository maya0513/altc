use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};

use altc_compiler::{
    Analysis, AnalyzeOptions, CodegenOptions, Diagnostic, GeneratedUnit, MemorySummary, Severity,
    SourceFile, analyze, emit_c,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;

#[derive(Parser, Debug)]
#[command(name = "altc", version, about = "The altc compiler and C11 driver")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, value_enum, default_value_t = MessageFormat::Human, global = true)]
    message_format: MessageFormat,
    #[arg(long, global = true)]
    show_memory_summary: bool,
    #[arg(long, default_value = "cc", global = true)]
    cc: OsString,
    #[arg(long, default_value = "ar", global = true)]
    ar: OsString,
    #[arg(short = 'I', value_name = "DIR", global = true)]
    include: Vec<PathBuf>,
    #[arg(short = 'D', value_name = "NAME[=VALUE]", global = true)]
    define: Vec<OsString>,
    #[arg(short = 'L', value_name = "DIR", global = true)]
    library_path: Vec<PathBuf>,
    #[arg(short = 'l', value_name = "LIB", global = true)]
    library: Vec<OsString>,
    #[arg(long, value_name = "FLAG", allow_hyphen_values = true, global = true)]
    cflag: Vec<OsString>,
    #[arg(long, value_name = "FLAG", allow_hyphen_values = true, global = true)]
    ldflag: Vec<OsString>,
    #[arg(long, global = true)]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Parse, resolve, and type/effect-check a source file.
    Check {
        input: PathBuf,
        #[arg(long)]
        ffi: bool,
    },
    /// Save generated C11 source and auxiliary headers.
    EmitC {
        input: PathBuf,
        #[arg(long, default_value = "target/altc")]
        out_dir: PathBuf,
    },
    /// Generate C11 and build an executable or static library.
    Build {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = ArtifactKind::Exe)]
        kind: ArtifactKind,
    },
    /// Build and run a single-file executable.
    Run {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(last = true)]
        arguments: Vec<OsString>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum MessageFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ArtifactKind {
    Exe,
    Staticlib,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run_cli(&cli) {
        Ok(code) => ExitCode::from(code),
        Err(()) => ExitCode::from(1),
    }
}

fn run_cli(cli: &Cli) -> Result<u8, ()> {
    match &cli.command {
        Commands::Check { input, ffi } => {
            let analysis = load_and_analyze(input, cli)?;
            report_analysis(&analysis, cli);
            if !analysis.succeeded() {
                return Err(());
            }
            if *ffi && analysis.has_extern_declarations() {
                let generated = generate(&analysis, input, cli)?;
                let out_dir = default_build_dir(input);
                let paths = write_generated(&generated, &out_dir, cli)?;
                if let Some(ffi_path) = paths.ffi_check {
                    run_ffi_check(cli, &ffi_path, &out_dir)?;
                }
            }
            Ok(0)
        }
        Commands::EmitC { input, out_dir } => {
            let analysis = checked_analysis(input, cli)?;
            let generated = generate(&analysis, input, cli)?;
            write_generated(&generated, out_dir, cli)?;
            Ok(0)
        }
        Commands::Build {
            input,
            output,
            kind,
        } => {
            let analysis = checked_analysis(input, cli)?;
            let artifact = build_artifact(&analysis, input, output.as_deref(), *kind, cli)?;
            if cli.verbose {
                eprintln!("built {}", artifact.display());
            }
            Ok(0)
        }
        Commands::Run {
            input,
            output,
            arguments,
        } => {
            let analysis = checked_analysis(input, cli)?;
            let artifact =
                build_artifact(&analysis, input, output.as_deref(), ArtifactKind::Exe, cli)?;
            let executable = fs::canonicalize(&artifact).map_err(|error| {
                tool_error(
                    cli,
                    "ALTC6005",
                    &format!("failed to resolve `{}`: {error}", artifact.display()),
                    None,
                );
            })?;
            let mut command = Command::new(&executable);
            command.args(arguments);
            if cli.verbose {
                eprintln!("running: {}", display_command(&command));
            }
            let status = command.status().map_err(|error| {
                tool_error(
                    cli,
                    "ALTC6005",
                    &format!("failed to execute `{}`: {error}", artifact.display()),
                    None,
                );
            })?;
            Ok(status.code().map_or(1, |code| code.clamp(0, 255) as u8))
        }
    }
}

fn load_and_analyze(path: &Path, cli: &Cli) -> Result<Analysis, ()> {
    let bytes = fs::read(path).map_err(|error| {
        tool_error(
            cli,
            "ALTC6001",
            &format!("failed to read `{}`: {error}", path.display()),
            None,
        );
    })?;
    let text = String::from_utf8(bytes).map_err(|error| {
        tool_error(
            cli,
            "ALTC1000",
            &format!("`{}` is not valid UTF-8: {error}", path.display()),
            None,
        );
    })?;
    Ok(analyze(
        SourceFile::new(path.display().to_string(), text),
        &AnalyzeOptions::default(),
    ))
}

fn checked_analysis(path: &Path, cli: &Cli) -> Result<Analysis, ()> {
    let analysis = load_and_analyze(path, cli)?;
    report_analysis(&analysis, cli);
    analysis.succeeded().then_some(analysis).ok_or(())
}

fn generate(analysis: &Analysis, input: &Path, cli: &Cli) -> Result<GeneratedUnit, ()> {
    let unit_name = input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("main")
        .to_owned();
    emit_c(analysis, &CodegenOptions { unit_name }).map_err(|diagnostics| {
        report_diagnostics(&diagnostics, &analysis.source.text, cli.message_format);
    })
}

fn report_analysis(analysis: &Analysis, cli: &Cli) {
    report_diagnostics(
        &analysis.diagnostics,
        &analysis.source.text,
        cli.message_format,
    );
    if cli.show_memory_summary && analysis.succeeded() {
        for summary in &analysis.memory_summaries {
            report_memory_summary(summary, &analysis.source, cli.message_format);
        }
    }
}

fn report_diagnostics(diagnostics: &[Diagnostic], source: &str, format: MessageFormat) {
    for diagnostic in diagnostics {
        match format {
            MessageFormat::Json => println!(
                "{}",
                serde_json::to_string(&json!({ "kind": "diagnostic", "diagnostic": diagnostic }))
                    .expect("diagnostic JSON")
            ),
            MessageFormat::Human => {
                let severity = match diagnostic.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Note => "note",
                };
                eprintln!("{severity}[{}]: {}", diagnostic.code, diagnostic.message);
                eprintln!(
                    "  --> {}:{}:{} (bytes {}..{})",
                    diagnostic.file,
                    diagnostic.start.line,
                    diagnostic.start.column,
                    diagnostic.span.start,
                    diagnostic.span.end
                );
                if let Some(line) = source.lines().nth(diagnostic.start.line.saturating_sub(1)) {
                    eprintln!("   | {line}");
                    let width = diagnostic
                        .end
                        .column
                        .saturating_sub(diagnostic.start.column)
                        .max(1);
                    eprintln!(
                        "   | {}{}",
                        " ".repeat(diagnostic.start.column.saturating_sub(1)),
                        "^".repeat(width)
                    );
                }
                for note in &diagnostic.notes {
                    eprintln!("   = note: {note}");
                }
                if let Some(help) = &diagnostic.help {
                    eprintln!("   = help: {help}");
                }
            }
        }
    }
}

fn report_memory_summary(summary: &MemorySummary, source: &SourceFile, format: MessageFormat) {
    match format {
        MessageFormat::Json => println!(
            "{}",
            serde_json::to_string(&json!({
                "kind": "memory-summary",
                "revision": revision(&source.text),
                "summary": summary,
            }))
            .expect("summary JSON")
        ),
        MessageFormat::Human => {
            let effects = summary
                .effects
                .iter()
                .map(|effect| format!("{effect:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            let categories = summary
                .categories
                .iter()
                .map(|category| format!("{category:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!(
                "{}: effects=[{}] memory=[{}] revision={}",
                summary.function,
                effects,
                categories,
                revision(&source.text)
            );
            for via in &summary.transitive {
                eprintln!(
                    "  {:?} via {} at bytes {}..{}",
                    via.category, via.callee, via.call_span.start, via.call_span.end
                );
            }
        }
    }
}

struct GeneratedPaths {
    source: PathBuf,
    public_header: Option<PathBuf>,
    ffi_check: Option<PathBuf>,
}

fn write_generated(unit: &GeneratedUnit, out_dir: &Path, cli: &Cli) -> Result<GeneratedPaths, ()> {
    fs::create_dir_all(out_dir).map_err(|error| {
        tool_error(
            cli,
            "ALTC6002",
            &format!("failed to create `{}`: {error}", out_dir.display()),
            None,
        );
    })?;
    let source = out_dir.join(format!("{}.c", unit.unit_name));
    fs::write(&source, &unit.source).map_err(|error| {
        tool_error(
            cli,
            "ALTC6002",
            &format!("failed to write `{}`: {error}", source.display()),
            None,
        );
    })?;
    let public_header = unit.public_header.as_ref().map(|header| {
        let path = out_dir.join(format!("{}.h", unit.unit_name));
        fs::write(&path, header).map(|()| path)
    });
    let public_header = match public_header {
        Some(Ok(path)) => Some(path),
        Some(Err(error)) => {
            tool_error(
                cli,
                "ALTC6002",
                &format!("failed to write header: {error}"),
                None,
            );
            return Err(());
        }
        None => None,
    };
    let ffi_check = unit.ffi_check.as_ref().map(|check| {
        let path = out_dir.join(format!("{}_ffi_check.c", unit.unit_name));
        fs::write(&path, check).map(|()| path)
    });
    let ffi_check = match ffi_check {
        Some(Ok(path)) => Some(path),
        Some(Err(error)) => {
            tool_error(
                cli,
                "ALTC6002",
                &format!("failed to write FFI check: {error}"),
                None,
            );
            return Err(());
        }
        None => None,
    };
    if cli.verbose {
        eprintln!("generated {}", source.display());
        if let Some(path) = &public_header {
            eprintln!("generated {}", path.display());
        }
        if let Some(path) = &ffi_check {
            eprintln!("generated {}", path.display());
        }
    }
    Ok(GeneratedPaths {
        source,
        public_header,
        ffi_check,
    })
}

fn build_artifact(
    analysis: &Analysis,
    input: &Path,
    requested_output: Option<&Path>,
    kind: ArtifactKind,
    cli: &Cli,
) -> Result<PathBuf, ()> {
    match kind {
        ArtifactKind::Exe if analysis.entrypoint().is_none() => {
            tool_error(
                cli,
                "ALTC6003",
                "executable build requires exactly one `@entry`",
                None,
            );
            return Err(());
        }
        ArtifactKind::Staticlib if analysis.entrypoint().is_some() => {
            tool_error(
                cli,
                "ALTC6003",
                "static library input cannot contain `@entry`",
                None,
            );
            return Err(());
        }
        _ => {}
    }
    let generated = generate(analysis, input, cli)?;
    let out_dir = default_build_dir(input);
    let paths = write_generated(&generated, &out_dir, cli)?;
    if let Some(ffi_path) = &paths.ffi_check {
        run_ffi_check(cli, ffi_path, &out_dir)?;
    }
    let object = out_dir.join(format!("{}.o", generated.unit_name));
    let mut compile = Command::new(&cli.cc);
    add_c_compile_options(&mut compile, cli, &out_dir);
    compile.arg("-c").arg(&paths.source).arg("-o").arg(&object);
    run_tool(cli, "ALTC6004", "C compilation failed", &mut compile)?;
    let output = requested_output.map_or_else(
        || match kind {
            ArtifactKind::Exe => out_dir.join(&generated.unit_name),
            ArtifactKind::Staticlib => out_dir.join(format!("lib{}.a", generated.unit_name)),
        },
        Path::to_path_buf,
    );
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            tool_error(
                cli,
                "ALTC6002",
                &format!("failed to create `{}`: {error}", parent.display()),
                None,
            );
        })?;
    }
    match kind {
        ArtifactKind::Exe => {
            let mut link = Command::new(&cli.cc);
            link.arg(&object).arg("-o").arg(&output);
            for path in &cli.library_path {
                link.arg(format!("-L{}", path.display()));
            }
            for library in &cli.library {
                let mut value = OsString::from("-l");
                value.push(library);
                link.arg(value);
            }
            link.args(&cli.ldflag);
            run_tool(cli, "ALTC6006", "C linker failed", &mut link)?;
        }
        ArtifactKind::Staticlib => {
            let mut archive = Command::new(&cli.ar);
            archive.arg("rcs").arg(&output).arg(&object);
            run_tool(cli, "ALTC6007", "archiver failed", &mut archive)?;
        }
    }
    let _ = paths.public_header;
    Ok(output)
}

fn run_ffi_check(cli: &Cli, ffi_path: &Path, out_dir: &Path) -> Result<(), ()> {
    let mut command = Command::new(&cli.cc);
    add_c_compile_options(&mut command, cli, out_dir);
    command.arg("-Werror").arg("-fsyntax-only").arg(ffi_path);
    run_tool(cli, "ALTC6101", "FFI verification failed", &mut command)
}

fn add_c_compile_options(command: &mut Command, cli: &Cli, out_dir: &Path) {
    command
        .arg("-std=c11")
        .arg("-pedantic-errors")
        .arg("-Wall")
        .arg("-Wextra")
        .arg(format!("-I{}", out_dir.display()));
    for include in &cli.include {
        command.arg(format!("-I{}", include.display()));
    }
    for define in &cli.define {
        let mut value = OsString::from("-D");
        value.push(define);
        command.arg(value);
    }
    command.args(&cli.cflag);
}

fn run_tool(cli: &Cli, code: &str, message: &str, command: &mut Command) -> Result<(), ()> {
    let display = display_command(command);
    if cli.verbose {
        eprintln!("running: {display}");
    }
    let output = command.output().map_err(|error| {
        tool_error(
            cli,
            code,
            &format!("failed to start command: {error}"),
            Some((&display, None)),
        );
    })?;
    if output.status.success() {
        report_tool_output(cli.message_format, &output);
        Ok(())
    } else {
        if cli.message_format == MessageFormat::Human {
            forward_tool_output(&output);
        }
        tool_error(cli, code, message, Some((&display, Some(&output))));
        Err(())
    }
}

fn report_tool_output(format: MessageFormat, output: &Output) {
    match format {
        MessageFormat::Human => forward_tool_output(output),
        MessageFormat::Json => {
            for (stream, bytes) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
                if !bytes.is_empty() {
                    println!(
                        "{}",
                        json!({
                            "kind": "tool-output",
                            "stream": stream,
                            "text": String::from_utf8_lossy(bytes),
                        })
                    );
                }
            }
        }
    }
}

fn forward_tool_output(output: &Output) {
    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
}

fn tool_error(cli: &Cli, code: &str, message: &str, command: Option<(&str, Option<&Output>)>) {
    match cli.message_format {
        MessageFormat::Human => {
            eprintln!("error[{code}]: {message}");
            if let Some((command, _)) = command {
                eprintln!("  command: {command}");
            }
        }
        MessageFormat::Json => {
            let (command_text, stdout, stderr) =
                command.map_or((None, None, None), |(command, output)| {
                    (
                        Some(command),
                        output.map(|output| String::from_utf8_lossy(&output.stdout).into_owned()),
                        output.map(|output| String::from_utf8_lossy(&output.stderr).into_owned()),
                    )
                });
            println!(
                "{}",
                json!({
                    "kind": "tool-diagnostic",
                    "code": code,
                    "severity": "error",
                    "message": message,
                    "command": command_text,
                    "stdout": stdout,
                    "stderr": stderr,
                })
            );
        }
    }
}

fn default_build_dir(input: &Path) -> PathBuf {
    let name = input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("main");
    Path::new("target").join("altc").join(name)
}

fn display_command(command: &Command) -> String {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|value| value.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

fn revision(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use altc_compiler::{Effect, MemoryCategory, Span};

    use super::*;

    static NEXT_TEMPORARY: AtomicUsize = AtomicUsize::new(0);

    fn cli(command: Commands) -> Cli {
        Cli {
            command,
            message_format: MessageFormat::Human,
            show_memory_summary: false,
            cc: OsString::from("cc"),
            ar: OsString::from("ar"),
            include: Vec::new(),
            define: Vec::new(),
            library_path: Vec::new(),
            library: Vec::new(),
            cflag: Vec::new(),
            ldflag: Vec::new(),
            verbose: false,
        }
    }

    fn temporary(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "altc-cli-{name}-{}-{}",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn reports_read_utf8_frontend_and_codegen_failures() {
        let check = cli(Commands::Check {
            input: PathBuf::new(),
            ffi: false,
        });
        assert!(load_and_analyze(&temporary("missing"), &check).is_err());

        let invalid = temporary("invalid-utf8.altc");
        fs::write(&invalid, [0xff, 0xfe]).unwrap();
        assert!(load_and_analyze(&invalid, &check).is_err());

        let source = SourceFile::new("bad.altc", "fn bad() -> Missing { 0 }");
        let analysis = analyze(source, &AnalyzeOptions::default());
        assert!(checked_analysis(&invalid, &check).is_err());
        assert!(generate(&analysis, Path::new("bad.altc"), &check).is_err());
        let _ = fs::remove_file(invalid);
    }

    #[test]
    fn reports_all_human_and_json_diagnostic_and_summary_shapes() {
        let source = "first\nsecond";
        let base = Diagnostic::error("TEST", "message", "test.altc", source, Span::new(6, 12))
            .with_note("note")
            .with_help("help");
        let mut warning = base.clone();
        warning.severity = Severity::Warning;
        let mut note = base.clone();
        note.severity = Severity::Note;
        let missing_line = Diagnostic::error(
            "TEST2",
            "outside",
            "test.altc",
            source,
            Span::new(source.len(), source.len()),
        );
        report_diagnostics(
            &[base, warning, note, missing_line],
            source,
            MessageFormat::Human,
        );
        report_diagnostics(&[], source, MessageFormat::Json);

        let analysis = analyze(
            SourceFile::new(
                "summary.altc",
                "fn read(p: *const i32) -> i32 { *p } fn caller(p: *const i32) -> i32 { read(p) }",
            ),
            &AnalyzeOptions::default(),
        );
        let summary = analysis
            .memory_summaries
            .iter()
            .find(|summary| summary.function == "caller")
            .unwrap();
        report_memory_summary(summary, &analysis.source, MessageFormat::Human);
        report_memory_summary(summary, &analysis.source, MessageFormat::Json);

        let mut report_cli = cli(Commands::Check {
            input: PathBuf::new(),
            ffi: false,
        });
        report_cli.show_memory_summary = true;
        report_analysis(&analysis, &report_cli);
        assert_eq!(revision("stable"), revision("stable"));
        assert_ne!(revision("stable"), revision("changed"));
    }

    #[test]
    fn writes_all_generated_files_and_reports_each_filesystem_failure() {
        let unit = GeneratedUnit {
            unit_name: "sample".to_owned(),
            source: "int value;\n".to_owned(),
            public_header: Some("int value;\n".to_owned()),
            ffi_check: Some("int check;\n".to_owned()),
        };
        let mut command_cli = cli(Commands::Check {
            input: PathBuf::new(),
            ffi: false,
        });
        command_cli.verbose = true;

        let success = temporary("generated-success");
        let paths = write_generated(&unit, &success, &command_cli).unwrap();
        assert!(paths.source.is_file());
        assert!(paths.public_header.unwrap().is_file());
        assert!(paths.ffi_check.unwrap().is_file());

        let create_failure = temporary("generated-create-failure");
        fs::write(&create_failure, b"file").unwrap();
        assert!(write_generated(&unit, &create_failure, &command_cli).is_err());

        let source_failure = temporary("generated-source-failure");
        fs::create_dir_all(source_failure.join("sample.c")).unwrap();
        assert!(write_generated(&unit, &source_failure, &command_cli).is_err());

        let header_failure = temporary("generated-header-failure");
        fs::create_dir_all(header_failure.join("sample.h")).unwrap();
        assert!(write_generated(&unit, &header_failure, &command_cli).is_err());

        let ffi_failure = temporary("generated-ffi-failure");
        fs::create_dir_all(ffi_failure.join("sample_ffi_check.c")).unwrap();
        assert!(write_generated(&unit, &ffi_failure, &command_cli).is_err());

        for path in [
            success,
            create_failure,
            source_failure,
            header_failure,
            ffi_failure,
        ] {
            if path.is_dir() {
                let _ = fs::remove_dir_all(path);
            } else {
                let _ = fs::remove_file(path);
            }
        }
    }

    #[test]
    fn validates_artifact_kind_before_starting_the_toolchain() {
        let executable = fixture("basic.altc");
        let library = fixture("library.altc");
        let executable_analysis = load_and_analyze(
            &executable,
            &cli(Commands::Check {
                input: executable.clone(),
                ffi: false,
            }),
        )
        .unwrap();
        let library_analysis = load_and_analyze(
            &library,
            &cli(Commands::Check {
                input: library.clone(),
                ffi: false,
            }),
        )
        .unwrap();
        let command_cli = cli(Commands::Check {
            input: PathBuf::new(),
            ffi: false,
        });
        assert!(
            build_artifact(
                &library_analysis,
                &library,
                None,
                ArtifactKind::Exe,
                &command_cli
            )
            .is_err()
        );
        assert!(
            build_artifact(
                &executable_analysis,
                &executable,
                None,
                ArtifactKind::Staticlib,
                &command_cli
            )
            .is_err()
        );
    }

    #[test]
    fn builds_commands_and_reports_success_failure_and_spawn_errors() {
        let mut command_cli = cli(Commands::Check {
            input: PathBuf::new(),
            ffi: false,
        });
        command_cli.verbose = true;
        command_cli.include = vec![PathBuf::from("include path")];
        command_cli.define = vec![OsString::from("FEATURE=1")];
        command_cli.cflag = vec![OsString::from("-Werror")];
        let mut compile = Command::new("cc");
        add_c_compile_options(&mut compile, &command_cli, Path::new("generated"));
        let displayed = display_command(&compile);
        assert!(displayed.contains("-std=c11"));
        assert!(displayed.contains("-DFEATURE=1"));

        let mut success = Command::new("sh");
        success.args(["-c", "printf stdout; printf stderr >&2"]);
        assert!(run_tool(&command_cli, "TEST", "success", &mut success).is_ok());

        let mut json_cli = cli(Commands::Check {
            input: PathBuf::new(),
            ffi: false,
        });
        json_cli.message_format = MessageFormat::Json;
        let mut json_success = Command::new("sh");
        json_success.args(["-c", "printf stdout; printf stderr >&2"]);
        assert!(run_tool(&json_cli, "TEST", "success", &mut json_success).is_ok());

        let mut failure = Command::new("sh");
        failure.args(["-c", "printf failed >&2; exit 7"]);
        assert!(run_tool(&command_cli, "TEST", "failure", &mut failure).is_err());
        let mut json_failure = Command::new("sh");
        json_failure.args(["-c", "printf failed; exit 8"]);
        assert!(run_tool(&json_cli, "TEST", "failure", &mut json_failure).is_err());

        let mut missing = Command::new("altc-command-that-does-not-exist");
        assert!(run_tool(&command_cli, "TEST", "missing", &mut missing).is_err());
        tool_error(&command_cli, "TEST", "plain", None);
        tool_error(&json_cli, "TEST", "plain", None);
    }

    #[test]
    fn derives_paths_and_runs_verbose_cli_flows() {
        assert_eq!(
            default_build_dir(Path::new("source.altc")),
            Path::new("target/altc/source")
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let path = Path::new(std::ffi::OsStr::from_bytes(b"\xff.altc"));
            assert_eq!(default_build_dir(path), Path::new("target/altc/main"));
        }

        let output = temporary("verbose-build/program");
        let mut build_cli = cli(Commands::Build {
            input: fixture("basic.altc"),
            output: Some(output.clone()),
            kind: ArtifactKind::Exe,
        });
        build_cli.verbose = true;
        build_cli.library_path = vec![temporary("unused-library-path")];
        build_cli.library = vec![OsString::from("m")];
        build_cli.define = vec![OsString::from("ALTC_TEST=1")];
        assert_eq!(run_cli(&build_cli), Ok(0));
        assert!(output.is_file());

        let mut run = cli(Commands::Run {
            input: fixture("basic.altc"),
            output: Some(output.clone()),
            arguments: Vec::new(),
        });
        run.verbose = true;
        assert_eq!(run_cli(&run), Ok(75));

        let bad_parent = temporary("output-parent-file");
        fs::write(&bad_parent, b"not a directory").unwrap();
        let failing_output = bad_parent.join("program");
        let failing_build = cli(Commands::Build {
            input: fixture("basic.altc"),
            output: Some(failing_output),
            kind: ArtifactKind::Exe,
        });
        assert!(run_cli(&failing_build).is_err());

        let _ = fs::remove_file(&output);
        if let Some(parent) = output.parent() {
            let _ = fs::remove_dir_all(parent);
        }
        let _ = fs::remove_file(bad_parent);
    }

    #[test]
    fn memory_summary_value_can_hold_every_public_effect_and_category() {
        let summary = MemorySummary {
            function: "all".to_owned(),
            effects: BTreeSet::from([
                Effect::Alloc,
                Effect::Free,
                Effect::Raise("Error".to_owned()),
            ]),
            categories: BTreeSet::from([
                MemoryCategory::IndirectRead,
                MemoryCategory::IndirectWrite,
                MemoryCategory::GlobalRead,
                MemoryCategory::GlobalWrite,
                MemoryCategory::VolatileRead,
                MemoryCategory::VolatileWrite,
                MemoryCategory::ExternCall,
                MemoryCategory::IndirectCall,
            ]),
            direct: Vec::new(),
            transitive: Vec::new(),
        };
        let source = SourceFile::new("all.altc", "");
        report_memory_summary(&summary, &source, MessageFormat::Human);
        report_memory_summary(&summary, &source, MessageFormat::Json);
    }
}
