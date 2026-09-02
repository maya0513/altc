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

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn ndjson(stdout: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("one valid JSON record per line"))
        .collect()
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
fn builds_and_runs_arrays_slices_and_range_for() {
    let output = altc()
        .args(["run", "tests/fixtures/sequences.altc"])
        .output()
        .expect("run sequence program");
    assert_eq!(
        output.status.code(),
        Some(20),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn builds_and_runs_a_diamond_module_graph() {
    let output = altc()
        .args(["run", "tests/fixtures/modules/root.altc"])
        .output()
        .expect("run module graph");
    assert_eq!(
        output.status.code(),
        Some(9),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
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
fn verifies_and_runs_extern_objects_layout_enum_and_callbacks() {
    let output = altc()
        .args([
            "run",
            "tests/fixtures/ffi_core.altc",
            "-I",
            "tests/fixtures",
        ])
        .output()
        .expect("run extern core program");
    assert_eq!(
        output.status.code(),
        Some(17),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let summary = altc()
        .args([
            "check",
            "tests/fixtures/ffi_core.altc",
            "--show-memory-summary",
            "--message-format",
            "json",
        ])
        .output()
        .expect("summarize extern core program");
    assert_success(&summary);
    let records = ndjson(&summary.stdout);
    assert!(records.iter().any(|record| {
        record["summary"]["categories"]
            .as_array()
            .is_some_and(|categories| categories.iter().any(|value| value == "indirect-call"))
    }));
}

#[test]
fn propagates_typed_errors_through_indirect_calls() {
    let output = altc()
        .args(["run", "tests/fixtures/raising_callback.altc"])
        .output()
        .expect("run raising callback program");
    assert_eq!(
        output.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
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
fn checks_and_runs_the_integrated_feature_showcase() {
    let checked = altc()
        .args([
            "check",
            "examples/feature_showcase.altc",
            "--ffi",
            "--show-memory-summary",
            "--message-format",
            "json",
            "--cflag=-Werror",
        ])
        .output()
        .expect("check integrated feature showcase");
    assert_success(&checked);

    let records = ndjson(&checked.stdout);
    let run_batch = records
        .iter()
        .find(|record| {
            record["kind"] == "memory-summary" && record["summary"]["function"] == "run_batch"
        })
        .expect("run_batch memory summary");
    let categories = run_batch["summary"]["categories"]
        .as_array()
        .expect("memory categories");
    for expected in ["indirect-read", "indirect-write", "extern-call"] {
        assert!(
            categories.iter().any(|category| category == expected),
            "missing {expected}: {categories:?}"
        );
    }

    let executed = altc()
        .args(["run", "examples/feature_showcase.altc", "--cflag=-Werror"])
        .output()
        .expect("run integrated feature showcase");
    assert_eq!(
        executed.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&executed.stdout), "step\nrelease\n");

    let generated = workspace().join("target/altc/feature_showcase/feature_showcase.c");
    let source = fs::read_to_string(generated).expect("generated feature showcase C");
    assert!(source.contains("UINT64_C(18446744073709551615)"));
    assert!(source.contains("INT64_MIN"));
}

#[test]
fn checks_emits_builds_and_runs_the_module_telemetry_project() {
    let root = workspace();
    let project = "examples/telemetry_pipeline/main.altc";
    let include = "examples/telemetry_pipeline";
    let build_dir = root.join("target/altc/telemetry-pipeline-test");
    fs::create_dir_all(&build_dir).unwrap();
    let object = build_dir.join("telemetry.o");
    let library = build_dir.join("libtelemetry.a");
    let compile = Command::new("cc")
        .current_dir(&root)
        .args([
            "-std=c11",
            "-pedantic-errors",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-I",
            include,
            "-c",
            "examples/telemetry_pipeline/telemetry.c",
            "-o",
        ])
        .arg(&object)
        .output()
        .expect("compile telemetry C library");
    assert_success(&compile);
    let archive = Command::new("ar")
        .arg("rcs")
        .arg(&library)
        .arg(&object)
        .output()
        .expect("archive telemetry C library");
    assert_success(&archive);

    let checked = altc()
        .args(["check", project, "--ffi", "-I", include, "--cflag=-Werror"])
        .output()
        .expect("check telemetry project");
    assert_success(&checked);

    let emitted_dir = build_dir.join("emitted");
    let emitted = altc()
        .args(["emit-c", project, "--out-dir"])
        .arg(&emitted_dir)
        .output()
        .expect("emit telemetry project");
    assert_success(&emitted);
    assert!(emitted_dir.join("main.c").is_file());
    assert!(emitted_dir.join("main.h").is_file());

    let executable = build_dir.join("telemetry-pipeline");
    let built = altc()
        .args(["build", project, "-I", include, "-L"])
        .arg(&build_dir)
        .args(["-l", "telemetry", "--cflag=-Werror", "-o"])
        .arg(&executable)
        .output()
        .expect("build telemetry project");
    assert_success(&built);
    assert!(executable.is_file());

    let executed = altc()
        .args(["run", project, "-I", include, "-L"])
        .arg(&build_dir)
        .args(["-l", "telemetry", "--cflag=-Werror"])
        .output()
        .expect("run telemetry project");
    assert_eq!(
        executed.status.code(),
        Some(48),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&executed.stdout),
        "pipeline cleanup\n"
    );
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
fn rejects_each_extern_object_layout_enum_and_callback_mismatch() {
    for mismatch in ["global", "field", "enum", "callback"] {
        let include = format!("tests/fixtures/ffi_core_mismatch/{mismatch}");
        let output = altc()
            .args(["check", "tests/fixtures/ffi_core.altc", "--ffi", "-I"])
            .arg(&include)
            .output()
            .unwrap_or_else(|error| panic!("run {mismatch} FFI mismatch check: {error}"));
        assert!(
            !output.status.success(),
            "{mismatch} mismatch unexpectedly passed"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("ALTC6101"),
            "{mismatch}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
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
