use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use altc_compiler::{AnalyzeOptions, CodegenOptions, GeneratedUnit, SourceFile, analyze, emit_c};
use serde::{Deserialize, Serialize};
use serde_json::Value;

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn main() {
    if let Err(error) = dispatch(env::args().skip(1).collect()) {
        eprintln!("xtask: {error}");
        std::process::exit(1);
    }
}

fn dispatch(arguments: Vec<String>) -> Result<(), String> {
    let Some((command, rest)) = arguments.split_first() else {
        print_help();
        return Ok(());
    };
    match command.as_str() {
        "check" => {
            require_empty(rest)?;
            run_check()
        }
        "gate" => {
            require_empty(rest)?;
            run_check()?;
            run_coverage(98)?;
            run_perf(PerfMode::Check)
        }
        "docs" => run_docs(parse_update(rest)?),
        "eval" => run_eval(parse_update(rest)?),
        "coverage" => run_coverage(parse_minimum(rest)?),
        "perf" => run_perf(parse_perf_mode(rest)?),
        "bench" => run_benchmark(parse_json_path(rest)?),
        "hooks" if rest == ["install"] => install_hooks(),
        "pre-commit" => {
            require_empty(rest)?;
            run_pre_commit()
        }
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        _ => Err(format!(
            "unknown command `{command}`; run `cargo xtask help`"
        )),
    }
}

fn print_help() {
    println!(
        "\
altc developer tasks

  cargo xtask check
  cargo xtask gate
  cargo xtask docs [--update]
  cargo xtask eval [--update]
  cargo xtask coverage [--minimum N]
  cargo xtask perf [--check|--update]
  cargo xtask bench [--json PATH]
  cargo xtask hooks install"
    );
}

fn require_empty(arguments: &[String]) -> Result<(), String> {
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(format!("unexpected argument(s): {}", arguments.join(" ")))
    }
}

fn parse_update(arguments: &[String]) -> Result<bool, String> {
    match arguments {
        [] => Ok(false),
        [value] if value == "--update" => Ok(true),
        _ => Err("expected no arguments or `--update`".to_owned()),
    }
}

fn parse_minimum(arguments: &[String]) -> Result<u32, String> {
    match arguments {
        [] => Ok(98),
        [flag, value] if flag == "--minimum" => value
            .parse::<u32>()
            .ok()
            .filter(|value| *value <= 100)
            .ok_or_else(|| "coverage minimum must be an integer from 0 through 100".to_owned()),
        _ => Err("expected `coverage [--minimum N]`".to_owned()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PerfMode {
    Check,
    Update,
}

fn parse_perf_mode(arguments: &[String]) -> Result<PerfMode, String> {
    match arguments {
        [] => Ok(PerfMode::Check),
        [value] if value == "--check" => Ok(PerfMode::Check),
        [value] if value == "--update" => Ok(PerfMode::Update),
        _ => Err("expected `perf [--check|--update]`".to_owned()),
    }
}

fn parse_json_path(arguments: &[String]) -> Result<Option<PathBuf>, String> {
    match arguments {
        [] => Ok(None),
        [flag, path] if flag == "--json" => Ok(Some(PathBuf::from(path))),
        _ => Err("expected `bench [--json PATH]`".to_owned()),
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives below the workspace root")
        .to_owned()
}

fn cargo() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn run_check() -> Result<(), String> {
    let root = workspace_root();
    run_command(
        Command::new(cargo())
            .current_dir(&root)
            .args(["fmt", "--all", "--", "--check"]),
    )?;
    run_command(Command::new(cargo()).current_dir(&root).args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]))?;
    run_command(
        Command::new(cargo())
            .current_dir(&root)
            .args(["test", "--workspace"]),
    )?;
    run_docs(false)?;
    run_eval(false)
}

fn run_coverage(minimum: u32) -> Result<(), String> {
    let root = workspace_root();
    let mut version = Command::new(cargo());
    version.current_dir(&root).args(["llvm-cov", "--version"]);
    let output = command_output(&mut version)?;
    let version_text = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !version_text.contains("cargo-llvm-cov 0.9.0") {
        return Err(
            "cargo-llvm-cov 0.9.0 is required; install it with `cargo install cargo-llvm-cov --locked --version 0.9.0`"
                .to_owned(),
        );
    }
    run_command(
        Command::new(cargo())
            .current_dir(&root)
            .args(coverage_clean_arguments()),
    )?;
    run_command(
        Command::new(cargo())
            .current_dir(root)
            .args(coverage_measure_arguments(minimum)),
    )
}

fn coverage_clean_arguments() -> [&'static str; 3] {
    ["llvm-cov", "clean", "--workspace"]
}

fn coverage_measure_arguments(minimum: u32) -> Vec<String> {
    vec![
        "llvm-cov".to_owned(),
        "--workspace".to_owned(),
        "--all-targets".to_owned(),
        "--exclude".to_owned(),
        "xtask".to_owned(),
        "--fail-under-lines".to_owned(),
        minimum.to_string(),
    ]
}

fn run_command(command: &mut Command) -> Result<(), String> {
    let display = display_command(command);
    let status = command
        .status()
        .map_err(|error| format!("failed to start `{display}`: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{display}` exited with {status}"))
    }
}

fn command_output(command: &mut Command) -> Result<Output, String> {
    let display = display_command(command);
    command
        .output()
        .map_err(|error| format!("failed to start `{display}`: {error}"))
}

fn display_command(command: &Command) -> String {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(kind: &str) -> Result<Self, String> {
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = workspace_root()
            .join("target/xtask")
            .join(format!("{kind}-{}-{sequence}", std::process::id()));
        fs::create_dir_all(&path)
            .map_err(|error| format!("failed to create `{}`: {error}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum FenceMode {
    Check,
    Run(i32),
    CompileFail(String),
    Skip(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DocBlock {
    path: PathBuf,
    line: usize,
    mode: FenceMode,
    source: String,
}

#[derive(Clone, Debug)]
struct OpenFence {
    ticks: usize,
    altc: Option<(usize, FenceMode, String)>,
}

fn run_docs(update: bool) -> Result<(), String> {
    let root = workspace_root();
    let files = markdown_files(&root)?;
    let mut blocks = Vec::new();
    let mut errors = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
        match parse_doc_blocks(path, &text) {
            Ok(mut parsed) => blocks.append(&mut parsed),
            Err(mut found) => errors.append(&mut found),
        }
        errors.extend(check_local_links(&root, path, &text));
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    let skip_count = blocks
        .iter()
        .filter(|block| matches!(block.mode, FenceMode::Skip(_)))
        .count();
    check_skip_baseline(&root, skip_count, update)?;

    for (index, block) in blocks.iter().enumerate() {
        match &block.mode {
            FenceMode::Check => {
                let unit = analyze_and_emit(block)?;
                validate_generated(&unit, &format!("doc-{index}"), false)?;
            }
            FenceMode::Run(expected) => {
                let unit = analyze_and_emit(block)?;
                let output = compile_and_run(&unit, &format!("doc-{index}"))?;
                let actual = output.status.code().unwrap_or(-1);
                if actual != *expected {
                    return Err(format!(
                        "{}:{}: expected exit code {expected}, found {actual}\nstdout:\n{}\nstderr:\n{}",
                        block.path.display(),
                        block.line,
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
            }
            FenceMode::CompileFail(expected) => check_compile_fail(block, expected)?,
            FenceMode::Skip(_) => {}
        }
    }
    Ok(())
}

fn parse_doc_blocks(path: &Path, text: &str) -> Result<Vec<DocBlock>, Vec<String>> {
    let mut blocks = Vec::new();
    let mut errors = Vec::new();
    let mut open: Option<OpenFence> = None;
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(active) = open.as_mut() {
            if closing_fence(trimmed, active.ticks) {
                if let Some((start, mode, source)) = active.altc.take() {
                    blocks.push(DocBlock {
                        path: path.to_owned(),
                        line: start,
                        mode,
                        source,
                    });
                }
                open = None;
            } else if let Some((_, _, source)) = active.altc.as_mut() {
                source.push_str(line);
                source.push('\n');
            }
            continue;
        }

        let Some((ticks, info)) = opening_fence(trimmed) else {
            continue;
        };
        let altc = if info == "altc" {
            errors.push(format!(
                "{}:{}: altc fence is missing check/run/compile_fail/skip classification",
                path.display(),
                line_index + 1
            ));
            None
        } else if let Some(specifier) = info.strip_prefix("altc ") {
            match parse_fence_mode(specifier) {
                Ok(mode) => Some((line_index + 1, mode, String::new())),
                Err(error) => {
                    errors.push(format!("{}:{}: {error}", path.display(), line_index + 1));
                    None
                }
            }
        } else {
            None
        };
        open = Some(OpenFence { ticks, altc });
    }
    if let Some(active) = open {
        if let Some((line, _, _)) = active.altc {
            errors.push(format!(
                "{}:{line}: unterminated altc fence",
                path.display()
            ));
        }
    }
    if errors.is_empty() {
        Ok(blocks)
    } else {
        Err(errors)
    }
}

fn opening_fence(line: &str) -> Option<(usize, &str)> {
    let ticks = line.bytes().take_while(|byte| *byte == b'`').count();
    (ticks >= 3).then(|| (ticks, line[ticks..].trim()))
}

fn closing_fence(line: &str, minimum: usize) -> bool {
    let ticks = line.bytes().take_while(|byte| *byte == b'`').count();
    ticks >= minimum && line[ticks..].trim().is_empty()
}

fn parse_fence_mode(value: &str) -> Result<FenceMode, String> {
    if value == "check" {
        return Ok(FenceMode::Check);
    }
    if let Some(code) = value.strip_prefix("run=") {
        let code = code
            .parse::<i32>()
            .ok()
            .filter(|code| (0..=255).contains(code))
            .ok_or_else(|| "run exit code must be from 0 through 255".to_owned())?;
        return Ok(FenceMode::Run(code));
    }
    if let Some(code) = value.strip_prefix("compile_fail=") {
        if code.starts_with("ALTC")
            && code.len() == 8
            && code[4..].bytes().all(|byte| byte.is_ascii_digit())
        {
            return Ok(FenceMode::CompileFail(code.to_owned()));
        }
        return Err("compile_fail requires one ALTCxxxx diagnostic code".to_owned());
    }
    if let Some(reason) = value.strip_prefix("skip=") {
        if !reason.is_empty()
            && reason
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Ok(FenceMode::Skip(reason.to_owned()));
        }
        return Err("skip requires a lowercase kebab-case reason".to_owned());
    }
    Err(format!("unknown altc fence classification `{value}`"))
}

#[derive(Deserialize, Serialize)]
struct SkipBaseline {
    max_skips: usize,
}

fn check_skip_baseline(root: &Path, actual: usize, update: bool) -> Result<(), String> {
    let path = root.join("docs/doctest-baseline.json");
    if update {
        write_json(&path, &SkipBaseline { max_skips: actual })?;
        return Ok(());
    }
    let baseline: SkipBaseline = read_json(&path)?;
    compare_skip_count(baseline.max_skips, actual)
}

fn compare_skip_count(maximum: usize, actual: usize) -> Result<(), String> {
    if actual > maximum {
        Err(format!(
            "documentation skip count increased from {} to {actual}; classify new examples as check/run/compile_fail or explicitly update the baseline",
            maximum
        ))
    } else if actual < maximum {
        Err(format!(
            "documentation skip count decreased from {maximum} to {actual}; run `cargo xtask docs --update` to preserve the improvement"
        ))
    } else {
        Ok(())
    }
}

fn analyze_and_emit(block: &DocBlock) -> Result<GeneratedUnit, String> {
    let label = format!("{}:{}", block.path.display(), block.line);
    let analysis = analyze(
        SourceFile::new(label.clone(), block.source.clone()),
        &AnalyzeOptions::default(),
    );
    if !analysis.succeeded() {
        return Err(format!(
            "{label}: expected successful analysis:\n{}",
            diagnostic_summary(&analysis.diagnostics)
        ));
    }
    emit_c(
        &analysis,
        &CodegenOptions {
            unit_name: format!("doc_{}", block.line),
        },
    )
    .map_err(|diagnostics| {
        format!(
            "{label}: C generation failed:\n{}",
            diagnostic_summary(&diagnostics)
        )
    })
}

fn check_compile_fail(block: &DocBlock, expected: &str) -> Result<(), String> {
    let analysis = analyze(
        SourceFile::new(
            format!("{}:{}", block.path.display(), block.line),
            block.source.clone(),
        ),
        &AnalyzeOptions::default(),
    );
    let mut codes: Vec<String> = analysis
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.clone())
        .collect();
    if analysis.succeeded() {
        if let Err(diagnostics) = emit_c(&analysis, &CodegenOptions::default()) {
            codes.extend(diagnostics.iter().map(|diagnostic| diagnostic.code.clone()));
        }
    }
    if codes.iter().any(|code| code == expected) {
        Ok(())
    } else {
        Err(format!(
            "{}:{}: expected diagnostic {expected}, found [{}]",
            block.path.display(),
            block.line,
            codes.join(", ")
        ))
    }
}

fn diagnostic_summary(diagnostics: &[altc_compiler::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{} at {}..{}: {}",
                diagnostic.code, diagnostic.span.start, diagnostic.span.end, diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn validate_generated(
    unit: &GeneratedUnit,
    name: &str,
    warnings_as_errors: bool,
) -> Result<(), String> {
    let directory = TempDir::new(name)?;
    let source = write_generated(unit, &directory.path)?;
    let mut command = c_command();
    command
        .current_dir(&directory.path)
        .args(["-std=c11", "-pedantic-errors", "-Wall", "-Wextra"]);
    if warnings_as_errors {
        command.arg("-Werror");
    }
    command.arg("-fsyntax-only").arg(&source);
    check_tool_output(command_output(&mut command)?, "strict C11 check")?;
    if let Some(ffi) = &unit.ffi_check {
        let path = directory
            .path
            .join(format!("{}_ffi_check.c", unit.unit_name));
        fs::write(&path, ffi)
            .map_err(|error| format!("failed to write `{}`: {error}", path.display()))?;
        let mut command = c_command();
        command
            .current_dir(&directory.path)
            .args([
                "-std=c11",
                "-pedantic-errors",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsyntax-only",
                "-I",
            ])
            .arg(&directory.path)
            .arg(path);
        check_tool_output(command_output(&mut command)?, "FFI verification TU check")?;
    }
    Ok(())
}

fn compile_and_run(unit: &GeneratedUnit, name: &str) -> Result<Output, String> {
    let directory = TempDir::new(name)?;
    let source = write_generated(unit, &directory.path)?;
    let executable = directory.path.join("program");
    let mut command = c_command();
    command
        .current_dir(&directory.path)
        .args([
            "-std=c11",
            "-pedantic-errors",
            "-Wall",
            "-Wextra",
            "-Werror",
        ])
        .arg(source)
        .arg("-o")
        .arg(&executable);
    check_tool_output(command_output(&mut command)?, "C build")?;
    command_output(Command::new(executable).current_dir(&directory.path))
}

fn write_generated(unit: &GeneratedUnit, directory: &Path) -> Result<PathBuf, String> {
    let source = directory.join(format!("{}.c", unit.unit_name));
    fs::write(&source, &unit.source)
        .map_err(|error| format!("failed to write `{}`: {error}", source.display()))?;
    if let Some(header) = &unit.public_header {
        let path = directory.join(format!("{}.h", unit.unit_name));
        fs::write(&path, header)
            .map_err(|error| format!("failed to write `{}`: {error}", path.display()))?;
    }
    Ok(source)
}

fn c_command() -> Command {
    Command::new(env::var_os("CC").unwrap_or_else(|| OsString::from("cc")))
}

fn check_tool_output(output: Output, operation: &str) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{operation} failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn markdown_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files(root, "md", &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(path: &Path, extension: &str, output: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(path)
        .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read directory entry: {error}"))?;
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name == ".git" || name == "target" {
                continue;
            }
            collect_files(&path, extension, output)?;
        } else if path.extension().is_some_and(|value| value == extension) {
            output.push(path);
        }
    }
    Ok(())
}

fn check_local_links(root: &Path, path: &Path, text: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let mut fence: Option<usize> = None;
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(ticks) = fence {
            if closing_fence(trimmed, ticks) {
                fence = None;
            }
            continue;
        }
        if let Some((ticks, _)) = opening_fence(trimmed) {
            fence = Some(ticks);
            continue;
        }
        let prose = strip_inline_code(line);
        for target in markdown_link_targets(&prose) {
            if target.is_empty()
                || target.starts_with('#')
                || target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                continue;
            }
            let target = target.split('#').next().unwrap_or_default();
            let target = target.split('?').next().unwrap_or_default();
            if target.is_empty() {
                continue;
            }
            let resolved = if let Some(target) = target.strip_prefix('/') {
                root.join(target)
            } else {
                path.parent().unwrap_or(root).join(target)
            };
            if !resolved.exists() {
                errors.push(format!(
                    "{}:{}: local link target `{target}` does not exist",
                    path.display(),
                    line_index + 1
                ));
            }
        }
    }
    errors
}

fn strip_inline_code(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut in_code = false;
    for character in line.chars() {
        if character == '`' {
            in_code = !in_code;
            output.push(' ');
        } else if in_code {
            output.push(' ');
        } else {
            output.push(character);
        }
    }
    output
}

fn markdown_link_targets(line: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut rest = line;
    while let Some(index) = rest.find("](") {
        rest = &rest[index + 2..];
        let Some(end) = rest.find(')') else {
            break;
        };
        let raw = rest[..end].trim();
        let raw = raw
            .strip_prefix('<')
            .and_then(|value| value.strip_suffix('>'))
            .unwrap_or(raw);
        let target = raw.split_whitespace().next().unwrap_or_default();
        targets.push(target);
        rest = &rest[end + 1..];
    }
    targets
}

#[derive(Debug, Deserialize)]
struct WritabilityManifest {
    cases: Vec<WritabilityCase>,
}

#[derive(Debug, Deserialize)]
struct WritabilityCase {
    id: String,
    task: String,
    source: String,
    expected_exit: Option<i32>,
    #[serde(default)]
    expected_stdout: String,
    #[serde(default)]
    expect_public_header: bool,
    expected_memory: Option<ExpectedMemory>,
}

#[derive(Debug, Deserialize)]
struct ExpectedMemory {
    function: String,
    categories: Vec<String>,
}

fn run_eval(update: bool) -> Result<(), String> {
    let root = workspace_root();
    run_writability(&root)?;
    run_repairs(&root, update)
}

fn run_writability(root: &Path) -> Result<(), String> {
    let base = root.join("eval/writability");
    let manifest: WritabilityManifest = read_json(&base.join("cases.json"))?;
    if manifest.cases.is_empty() {
        return Err("writability manifest must contain at least one case".to_owned());
    }
    let mut ids = BTreeSet::new();
    let mut task_paths = BTreeSet::new();
    let mut source_paths = BTreeSet::new();
    for case in &manifest.cases {
        if !ids.insert(&case.id) {
            return Err(format!("duplicate writability case id `{}`", case.id));
        }
        let task = base.join(&case.task);
        let source_path = base.join(&case.source);
        require_file(&task)?;
        require_file(&source_path)?;
        task_paths.insert(task);
        source_paths.insert(source_path.clone());

        let source = fs::read_to_string(&source_path)
            .map_err(|error| format!("failed to read `{}`: {error}", source_path.display()))?;
        let analysis = analyze(
            SourceFile::new(case.source.clone(), source),
            &AnalyzeOptions::default(),
        );
        if !analysis.succeeded() {
            return Err(format!(
                "writability case `{}` failed analysis:\n{}",
                case.id,
                diagnostic_summary(&analysis.diagnostics)
            ));
        }
        if let Some(expected) = &case.expected_memory {
            let summary = analysis
                .memory_summaries
                .iter()
                .find(|summary| summary.function == expected.function)
                .ok_or_else(|| {
                    format!(
                        "writability case `{}` has no memory summary for `{}`",
                        case.id, expected.function
                    )
                })?;
            let actual: Vec<String> = summary
                .categories
                .iter()
                .map(|category| {
                    serde_json::to_value(category)
                        .expect("memory category serializes")
                        .as_str()
                        .expect("memory category is a string")
                        .to_owned()
                })
                .collect();
            if actual != expected.categories {
                return Err(format!(
                    "writability case `{}` memory categories differ: expected {:?}, found {:?}",
                    case.id, expected.categories, actual
                ));
            }
        }
        let unit = emit_c(
            &analysis,
            &CodegenOptions {
                unit_name: case.id.replace('-', "_"),
            },
        )
        .map_err(|diagnostics| {
            format!(
                "writability case `{}` failed C generation:\n{}",
                case.id,
                diagnostic_summary(&diagnostics)
            )
        })?;
        if case.expect_public_header != unit.public_header.is_some() {
            return Err(format!(
                "writability case `{}` public header expectation differs",
                case.id
            ));
        }
        if let Some(expected_exit) = case.expected_exit {
            let output = compile_and_run(&unit, &format!("eval-{}", case.id))?;
            let actual_exit = output.status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&output.stdout);
            if actual_exit != expected_exit || stdout != case.expected_stdout {
                return Err(format!(
                    "writability case `{}` expected exit/stdout {:?}/{:?}, found {:?}/{:?}",
                    case.id, case.expected_exit, case.expected_stdout, actual_exit, stdout
                ));
            }
        } else {
            validate_generated(&unit, &format!("eval-{}", case.id), true)?;
        }
    }
    check_manifest_inventory(&base.join("tasks"), "md", &task_paths)?;
    check_manifest_inventory(&base.join("golden"), "altc", &source_paths)
}

fn check_manifest_inventory(
    directory: &Path,
    extension: &str,
    expected: &BTreeSet<PathBuf>,
) -> Result<(), String> {
    let mut actual = Vec::new();
    collect_files(directory, extension, &mut actual)?;
    let actual: BTreeSet<_> = actual.into_iter().collect();
    if &actual == expected {
        Ok(())
    } else {
        Err(format!(
            "manifest inventory for `{}` differs: expected {:?}, found {:?}",
            directory.display(),
            expected,
            actual
        ))
    }
}

fn run_repairs(root: &Path, update: bool) -> Result<(), String> {
    let directory = root.join("eval/repair");
    let mut cases = fs::read_dir(&directory)
        .map_err(|error| format!("failed to read `{}`: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    cases.sort();
    if cases.is_empty() {
        return Err("repair corpus must contain at least one case".to_owned());
    }
    for case in cases {
        let name = case
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("invalid repair case path `{}`", case.display()))?;
        let broken_path = case.join("broken.altc");
        let fixed_path = case.join("fixed.altc");
        let snapshot_path = case.join("expected.json");
        require_file(&broken_path)?;
        require_file(&fixed_path)?;
        let broken = fs::read_to_string(&broken_path)
            .map_err(|error| format!("failed to read `{}`: {error}", broken_path.display()))?;
        let analysis = analyze(
            SourceFile::new(format!("eval/repair/{name}/broken.altc"), broken),
            &AnalyzeOptions::default(),
        );
        if analysis.succeeded() {
            return Err(format!(
                "repair case `{name}` broken source unexpectedly succeeds"
            ));
        }
        let actual = serde_json::to_value(&analysis.diagnostics)
            .map_err(|error| format!("failed to serialize `{name}` diagnostics: {error}"))?;
        if update {
            write_json_value(&snapshot_path, &actual)?;
        } else {
            let expected: Value = read_json(&snapshot_path)?;
            compare_diagnostic_snapshot(name, &expected, &actual)?;
        }

        let fixed = fs::read_to_string(&fixed_path)
            .map_err(|error| format!("failed to read `{}`: {error}", fixed_path.display()))?;
        let analysis = analyze(
            SourceFile::new(format!("eval/repair/{name}/fixed.altc"), fixed),
            &AnalyzeOptions::default(),
        );
        if !analysis.succeeded() {
            return Err(format!(
                "repair case `{name}` fixed source fails:\n{}",
                diagnostic_summary(&analysis.diagnostics)
            ));
        }
        let unit = emit_c(
            &analysis,
            &CodegenOptions {
                unit_name: format!("repair_{}", name.replace('-', "_")),
            },
        )
        .map_err(|diagnostics| {
            format!(
                "repair case `{name}` fixed source fails C generation:\n{}",
                diagnostic_summary(&diagnostics)
            )
        })?;
        validate_generated(&unit, &format!("repair-{name}"), false)?;
    }
    Ok(())
}

fn compare_diagnostic_snapshot(name: &str, expected: &Value, actual: &Value) -> Result<(), String> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "repair case `{name}` diagnostic snapshot differs; inspect it and run `cargo xtask eval --update` only for an intentional change"
        ))
    }
}

fn require_file(path: &Path) -> Result<(), String> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("required file `{}` is missing", path.display()))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PerfBaseline {
    tolerance_percent: u64,
    cases: BTreeMap<String, PerfMetric>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PerfMetric {
    input: String,
    source_bytes: u64,
    header_bytes: u64,
    ffi_bytes: u64,
}

const PERF_CASES: &[(&str, &str)] = &[
    ("basic", "tests/fixtures/basic.altc"),
    ("data-control", "tests/fixtures/data_control.altc"),
    ("effects", "tests/fixtures/effects.altc"),
    ("feature-showcase", "examples/feature_showcase.altc"),
    ("library", "tests/fixtures/library.altc"),
    ("overflow", "tests/fixtures/overflow.altc"),
];

fn run_perf(mode: PerfMode) -> Result<(), String> {
    let root = workspace_root();
    let actual = collect_perf_metrics(&root)?;
    let path = root.join("bench/perf/baseline.json");
    if mode == PerfMode::Update {
        write_json(
            &path,
            &PerfBaseline {
                tolerance_percent: 2,
                cases: actual,
            },
        )?;
        return Ok(());
    }
    let baseline: PerfBaseline = read_json(&path)?;
    compare_perf(&baseline, &actual)
}

fn collect_perf_metrics(root: &Path) -> Result<BTreeMap<String, PerfMetric>, String> {
    let mut metrics = BTreeMap::new();
    for (id, input) in PERF_CASES {
        let path = root.join(input);
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
        let analysis = analyze(
            SourceFile::new((*input).to_owned(), source),
            &AnalyzeOptions::default(),
        );
        if !analysis.succeeded() {
            return Err(format!(
                "performance case `{id}` failed analysis:\n{}",
                diagnostic_summary(&analysis.diagnostics)
            ));
        }
        let unit = emit_c(
            &analysis,
            &CodegenOptions {
                unit_name: id.replace('-', "_"),
            },
        )
        .map_err(|diagnostics| {
            format!(
                "performance case `{id}` failed C generation:\n{}",
                diagnostic_summary(&diagnostics)
            )
        })?;
        metrics.insert(
            (*id).to_owned(),
            PerfMetric {
                input: (*input).to_owned(),
                source_bytes: unit.source.len() as u64,
                header_bytes: unit.public_header.as_ref().map_or(0, |value| value.len()) as u64,
                ffi_bytes: unit.ffi_check.as_ref().map_or(0, |value| value.len()) as u64,
            },
        );
    }
    Ok(metrics)
}

fn compare_perf(
    baseline: &PerfBaseline,
    actual: &BTreeMap<String, PerfMetric>,
) -> Result<(), String> {
    let expected_ids: BTreeSet<_> = baseline.cases.keys().collect();
    let actual_ids: BTreeSet<_> = actual.keys().collect();
    if expected_ids != actual_ids {
        return Err(format!(
            "performance case inventory differs: baseline {:?}, actual {:?}",
            expected_ids, actual_ids
        ));
    }
    let mut errors = Vec::new();
    for (id, current) in actual {
        let expected = &baseline.cases[id];
        if expected.input != current.input {
            errors.push(format!(
                "{id}: input changed from `{}` to `{}`",
                expected.input, current.input
            ));
            continue;
        }
        for (kind, before, after) in [
            ("C source", expected.source_bytes, current.source_bytes),
            ("public header", expected.header_bytes, current.header_bytes),
            ("FFI TU", expected.ffi_bytes, current.ffi_bytes),
        ] {
            let limit = perf_limit(before, baseline.tolerance_percent);
            if after > limit {
                errors.push(format!(
                    "{id} {kind}: {before} -> {after} bytes exceeds +{}% limit {limit}",
                    baseline.tolerance_percent
                ));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "generated artifact size regression:\n{}\nrun `cargo xtask perf --update` only when the growth is intentional",
            errors.join("\n")
        ))
    }
}

fn perf_limit(baseline: u64, tolerance_percent: u64) -> u64 {
    baseline.saturating_add(
        baseline
            .saturating_mul(tolerance_percent)
            .saturating_add(99)
            / 100,
    )
}

fn run_benchmark(json_path: Option<PathBuf>) -> Result<(), String> {
    let root = workspace_root();
    let mut command = Command::new(cargo());
    command
        .current_dir(&root)
        .args(["bench", "-p", "altc-compiler", "--bench", "pipeline"]);
    if let Some(path) = json_path {
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create benchmark output directory: {error}"))?;
        }
        command.args(["--", "--json"]).arg(path);
    }
    run_command(&mut command)
}

fn install_hooks() -> Result<(), String> {
    let root = workspace_root();
    run_command(Command::new("git").current_dir(root).args([
        "config",
        "core.hooksPath",
        ".githooks",
    ]))
}

fn run_pre_commit() -> Result<(), String> {
    let root = workspace_root();
    let snapshot = TempDir::new("pre-commit-index")?;
    checkout_index(&root, &snapshot.path)?;
    let target = root.join("target/xtask/pre-commit-target");
    run_command(
        Command::new(cargo())
            .current_dir(&snapshot.path)
            .env("CARGO_TARGET_DIR", target)
            .args(["xtask", "check"]),
    )
}

fn checkout_index(repository: &Path, destination: &Path) -> Result<(), String> {
    let mut prefix = destination.as_os_str().to_owned();
    prefix.push(std::path::MAIN_SEPARATOR.to_string());
    run_command(
        Command::new("git")
            .current_dir(repository)
            .args(["checkout-index", "--all"])
            .arg(format!("--prefix={}", prefix.to_string_lossy())),
    )
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid JSON in `{}`: {error}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let value = serde_json::to_value(value)
        .map_err(|error| format!("failed to serialize `{}`: {error}", path.display()))?;
    write_json_value(path, &value)
}

fn write_json_value(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create `{}`: {error}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize `{}`: {error}", path.display()))?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| format!("failed to write `{}`: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_gate_cleans_stale_profiles_before_measuring_product_code() {
        assert_eq!(
            coverage_clean_arguments(),
            ["llvm-cov", "clean", "--workspace"]
        );
        assert_eq!(
            coverage_measure_arguments(98),
            [
                "llvm-cov",
                "--workspace",
                "--all-targets",
                "--exclude",
                "xtask",
                "--fail-under-lines",
                "98",
            ]
        );
    }

    #[test]
    fn parses_every_fence_contract_and_ignores_nested_examples() {
        let text = r#"
```altc check
fn ok() {}
```
```altc run=7
@entry fn main() -> c_int { 7 }
```
```altc compile_fail=ALTC3037
fn bad() {}
```
```altc skip=future-feature
fn future[T]() {}
```
````text
```altc
```
````
"#;
        let blocks = parse_doc_blocks(Path::new("test.md"), text).unwrap();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].mode, FenceMode::Check);
        assert_eq!(blocks[1].mode, FenceMode::Run(7));
        assert_eq!(
            blocks[2].mode,
            FenceMode::CompileFail("ALTC3037".to_owned())
        );
        assert_eq!(blocks[3].mode, FenceMode::Skip("future-feature".to_owned()));
    }

    #[test]
    fn rejects_unclassified_invalid_and_unterminated_altc_fences() {
        for text in [
            "```altc\nfn bad() {}\n```\n",
            "```altc skip=\n```\n",
            "```altc compile_fail=E1\n```\n",
            "```altc check\nfn bad() {}\n",
        ] {
            assert!(parse_doc_blocks(Path::new("bad.md"), text).is_err());
        }
    }

    #[test]
    fn compile_fail_gate_rejects_the_wrong_diagnostic_code() {
        let block = DocBlock {
            path: PathBuf::from("wrong-code.md"),
            line: 1,
            mode: FenceMode::CompileFail("ALTC3037".to_owned()),
            source: "fn identity[T](value: T) -> T { value }\n".to_owned(),
        };
        assert!(check_compile_fail(&block, "ALTC4001").is_ok());
        assert!(check_compile_fail(&block, "ALTC3037").is_err());
    }

    #[test]
    fn extracts_links_and_reports_only_missing_local_targets() {
        assert_eq!(
            markdown_link_targets("[one](a.md) [two](https://example.com)"),
            ["a.md", "https://example.com"]
        );
        let directory = TempDir::new("links").unwrap();
        let document = directory.path.join("doc.md");
        fs::write(&document, "[missing](no.md) [web](https://example.com)\n").unwrap();
        let errors = check_local_links(
            &directory.path,
            &document,
            &fs::read_to_string(&document).unwrap(),
        );
        assert_eq!(errors.len(), 1);
        assert!(markdown_link_targets(&strip_inline_code("`call[T](value)`")).is_empty());
    }

    #[test]
    fn performance_gate_rejects_growth_and_inventory_drift() {
        let metric = PerfMetric {
            input: "case.altc".to_owned(),
            source_bytes: 100,
            header_bytes: 0,
            ffi_bytes: 0,
        };
        let baseline = PerfBaseline {
            tolerance_percent: 2,
            cases: BTreeMap::from([("case".to_owned(), metric.clone())]),
        };
        assert!(compare_perf(&baseline, &baseline.cases).is_ok());
        let mut grown = baseline.cases.clone();
        grown.get_mut("case").unwrap().source_bytes = 104;
        assert!(compare_perf(&baseline, &grown).is_err());
        assert!(compare_perf(&baseline, &BTreeMap::new()).is_err());
    }

    #[test]
    fn skip_ratchet_requires_improvements_to_update_the_baseline() {
        assert!(compare_skip_count(3, 3).is_ok());
        assert!(compare_skip_count(3, 2).is_err());
        assert!(compare_skip_count(3, 4).is_err());
    }

    #[test]
    fn diagnostic_snapshot_comparison_detects_any_change() {
        let original = serde_json::json!([{"code": "ALTC3008", "message": "old"}]);
        let changed = serde_json::json!([{"code": "ALTC3008", "message": "new"}]);
        assert!(compare_diagnostic_snapshot("case", &original, &original).is_ok());
        assert!(compare_diagnostic_snapshot("case", &original, &changed).is_err());
    }

    #[test]
    fn checkout_index_uses_staged_content() {
        let repository = TempDir::new("index-repository").unwrap();
        run_command(
            Command::new("git")
                .current_dir(&repository.path)
                .args(["init", "-q"]),
        )
        .unwrap();
        let source = repository.path.join("document.md");
        fs::write(&source, "```altc\nfn staged_error() {}\n```\n").unwrap();
        run_command(
            Command::new("git")
                .current_dir(&repository.path)
                .args(["add", "document.md"]),
        )
        .unwrap();
        fs::write(
            &source,
            "```altc check\nfn working_tree_is_fixed() {}\n```\n",
        )
        .unwrap();
        let snapshot = TempDir::new("index-snapshot").unwrap();
        checkout_index(&repository.path, &snapshot.path).unwrap();
        let staged = fs::read_to_string(snapshot.path.join("document.md")).unwrap();
        assert!(parse_doc_blocks(Path::new("document.md"), &staged).is_err());
        let working = fs::read_to_string(source).unwrap();
        assert!(parse_doc_blocks(Path::new("document.md"), &working).is_ok());
    }
}
