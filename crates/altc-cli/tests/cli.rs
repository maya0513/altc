use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn altc() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_altc"));
    command.current_dir(workspace());
    command
}

#[test]
fn checks_and_reports_json_diagnostics() {
    let output = altc()
        .args([
            "check",
            "tests/fixtures/unsupported.altc",
            "--message-format",
            "json",
        ])
        .output()
        .expect("run altc check");
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let record: serde_json::Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    assert_eq!(record["kind"], "diagnostic");
    assert_eq!(record["diagnostic"]["code"], "ALTC4001");
}

#[test]
fn uses_exit_code_two_for_cli_usage_errors() {
    let output = altc().output().expect("run altc without subcommand");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn frontend_check_does_not_start_the_c_toolchain() {
    let output = altc()
        .args([
            "check",
            "tests/fixtures/effects.altc",
            "--cc",
            "altc-test-compiler-that-does-not-exist",
        ])
        .output()
        .expect("run frontend-only check");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn emits_memory_summaries_as_ndjson_records() {
    let output = altc()
        .args([
            "check",
            "tests/fixtures/pointers.altc",
            "--show-memory-summary",
            "--message-format",
            "json",
        ])
        .output()
        .expect("run memory summary check");
    assert!(output.status.success());
    let records: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid NDJSON record"))
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["kind"], "memory-summary");
    assert!(
        records[0]["summary"]["categories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|category| category == "indirect-write")
    );
}

#[test]
fn builds_and_runs_basic_program() {
    let output = altc()
        .args(["run", "tests/fixtures/basic.altc"])
        .output()
        .expect("run altc program");
    assert_eq!(output.status.code(), Some(75));
}

#[test]
fn runs_a_single_component_output_path() {
    let root = workspace();
    let run_dir = root.join("target/altc/relative-run-test");
    fs::create_dir_all(&run_dir).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_altc"));
    command
        .current_dir(&run_dir)
        .arg("run")
        .arg(root.join("tests/fixtures/basic.altc"));
    let output = command
        .args(["-o", "program"])
        .output()
        .expect("run custom relative output");
    assert_eq!(
        output.status.code(),
        Some(75),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn builds_struct_payload_enum_match_and_for_loop() {
    let output = altc()
        .args(["run", "tests/fixtures/data_control.altc"])
        .output()
        .expect("run data/control program");
    assert_eq!(output.status.code(), Some(8));
}

#[test]
fn preserves_lexical_shadowing_in_lowered_loop_blocks() {
    let output = altc()
        .args(["run", "tests/fixtures/shadowing.altc"])
        .output()
        .expect("run shadowing program");
    assert_eq!(
        output.status.code(),
        Some(13),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lowers_conditionless_for_without_unused_labels() {
    let output = altc()
        .args(["run", "tests/fixtures/infinite_for.altc", "--cflag=-Werror"])
        .output()
        .expect("run conditionless for program");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn verifies_ffi_and_defer_program() {
    let output = altc()
        .args(["check", "tests/fixtures/effects.altc", "--ffi"])
        .output()
        .expect("run altc FFI check");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = altc()
        .args(["run", "tests/fixtures/effects.altc"])
        .output()
        .expect("run effect program");
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "cleanup\n");
}

#[test]
fn defer_runs_lifo_on_early_return() {
    let output = altc()
        .args(["run", "tests/fixtures/defer_exits.altc"])
        .output()
        .expect("run defer exit program");
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "inner\nouter\n");
}

#[test]
fn defer_runs_lifo_during_typed_error_propagation() {
    let output = altc()
        .args(["run", "tests/fixtures/defer_raise.altc"])
        .output()
        .expect("run defer raise program");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "inner\nouter\n");
}

#[test]
fn defer_runs_on_continue_and_break() {
    let output = altc()
        .args(["run", "tests/fixtures/defer_loop.altc"])
        .output()
        .expect("run loop defer program");
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn implements_wrapping_and_saturating_integer_operators() {
    let output = altc()
        .args(["run", "tests/fixtures/overflow.altc"])
        .output()
        .expect("run overflow operator program");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let root = workspace();
    let out_dir = root.join("target/altc/overflow-strict");
    let emitted = altc()
        .args(["emit-c", "tests/fixtures/overflow.altc", "--out-dir"])
        .arg(&out_dir)
        .output()
        .expect("emit overflow C");
    assert!(emitted.status.success());
    let check = Command::new("cc")
        .args([
            "-std=c11",
            "-pedantic-errors",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fsyntax-only",
        ])
        .arg(out_dir.join("overflow.c"))
        .output()
        .expect("check overflow C");
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn preserves_literal_values_and_escapes_in_c11() {
    let output = altc()
        .args(["run", "tests/fixtures/literals.altc"])
        .output()
        .expect("run literal program");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "line\nλ\n");
}

#[test]
fn rejects_mismatched_ffi_contract() {
    let output = altc()
        .args([
            "check",
            "tests/fixtures/ffi_mismatch.altc",
            "--ffi",
            "-I",
            "tests/fixtures/ffi",
        ])
        .output()
        .expect("run mismatched FFI check");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ALTC6101"));
}

#[test]
fn keeps_toolchain_failures_as_ndjson_in_json_mode() {
    let output = altc()
        .args([
            "check",
            "tests/fixtures/ffi_mismatch.altc",
            "--ffi",
            "-I",
            "tests/fixtures/ffi",
            "--message-format",
            "json",
        ])
        .output()
        .expect("run mismatched JSON FFI check");
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let records: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("one valid JSON record per line"))
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["kind"], "tool-diagnostic");
    assert_eq!(records[0]["code"], "ALTC6101");
}

#[test]
fn builds_static_library_and_c_client() {
    let root = workspace();
    let output = altc()
        .args([
            "build",
            "tests/fixtures/library.altc",
            "--kind",
            "staticlib",
        ])
        .output()
        .expect("build static library");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let client = root.join("target/altc/library/library_client_test");
    let compile = Command::new("cc")
        .current_dir(&root)
        .args([
            "-std=c11",
            "-pedantic-errors",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Itarget/altc/library",
            "tests/fixtures/library_client.c",
            "target/altc/library/liblibrary.a",
            "-o",
        ])
        .arg(&client)
        .output()
        .expect("compile C client");
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert!(Command::new(client).status().unwrap().success());
}

#[test]
fn emit_c_writes_strict_c11() {
    let root = workspace();
    let out_dir = root.join("target/altc/emit-test");
    let output = altc()
        .args(["emit-c", "tests/fixtures/basic.altc", "--out-dir"])
        .arg(&out_dir)
        .output()
        .expect("emit C");
    assert!(output.status.success());
    let source = out_dir.join("basic.c");
    assert!(
        fs::read_to_string(&source)
            .unwrap()
            .starts_with("/* Generated by altc. Do not edit. */")
    );
    let check = Command::new("cc")
        .args([
            "-std=c11",
            "-pedantic-errors",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fsyntax-only",
        ])
        .arg(source)
        .status()
        .unwrap();
    assert!(check.success());
}
