use std::env;
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use altc_compiler::{AnalyzeOptions, CodegenOptions, SourceFile, analyze, emit_c};
use serde::Serialize;

#[derive(Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    records: Vec<BenchmarkRecord>,
}

#[derive(Serialize)]
struct BenchmarkRecord {
    name: &'static str,
    input_bytes: usize,
    iterations: usize,
    nanos_per_operation: f64,
    mebibytes_per_second: f64,
}

fn main() {
    let json_path = parse_json_path();
    let small = include_str!("../../../tests/fixtures/data_control.altc");
    let showcase = include_str!("../../../examples/feature_showcase.altc");
    let medium = synthetic_program(200);
    let analyzed_medium = analyze(
        SourceFile::new("medium.altc", medium.clone()),
        &AnalyzeOptions::default(),
    );
    assert!(analyzed_medium.succeeded());
    let analyzed_showcase = analyze(
        SourceFile::new("feature_showcase.altc", showcase),
        &AnalyzeOptions::default(),
    );
    assert!(analyzed_showcase.succeeded());

    let records = vec![
        measure(
            "analyze/small",
            small.len(),
            Duration::from_millis(500),
            || {
                black_box(analyze(
                    SourceFile::new("small.altc", black_box(small)),
                    &AnalyzeOptions::default(),
                ));
            },
        ),
        measure(
            "analyze/medium",
            medium.len(),
            Duration::from_millis(750),
            || {
                black_box(analyze(
                    SourceFile::new("medium.altc", black_box(medium.as_str())),
                    &AnalyzeOptions::default(),
                ));
            },
        ),
        measure(
            "emit-c/medium",
            medium.len(),
            Duration::from_millis(750),
            || {
                black_box(
                    emit_c(
                        black_box(&analyzed_medium),
                        &CodegenOptions {
                            unit_name: "benchmark".to_owned(),
                        },
                    )
                    .unwrap(),
                );
            },
        ),
        measure(
            "pipeline/medium",
            medium.len(),
            Duration::from_millis(750),
            || {
                let analysis = analyze(
                    SourceFile::new("medium.altc", black_box(medium.as_str())),
                    &AnalyzeOptions::default(),
                );
                black_box(
                    emit_c(
                        &analysis,
                        &CodegenOptions {
                            unit_name: "benchmark".to_owned(),
                        },
                    )
                    .unwrap(),
                );
            },
        ),
        measure(
            "analyze/showcase",
            showcase.len(),
            Duration::from_millis(750),
            || {
                black_box(analyze(
                    SourceFile::new("feature_showcase.altc", black_box(showcase)),
                    &AnalyzeOptions::default(),
                ));
            },
        ),
        measure(
            "emit-c/showcase",
            showcase.len(),
            Duration::from_millis(750),
            || {
                black_box(
                    emit_c(
                        black_box(&analyzed_showcase),
                        &CodegenOptions {
                            unit_name: "feature_showcase".to_owned(),
                        },
                    )
                    .unwrap(),
                );
            },
        ),
        measure(
            "pipeline/showcase",
            showcase.len(),
            Duration::from_millis(750),
            || {
                let analysis = analyze(
                    SourceFile::new("feature_showcase.altc", black_box(showcase)),
                    &AnalyzeOptions::default(),
                );
                black_box(
                    emit_c(
                        &analysis,
                        &CodegenOptions {
                            unit_name: "feature_showcase".to_owned(),
                        },
                    )
                    .unwrap(),
                );
            },
        ),
    ];

    if let Some(path) = json_path {
        write_json(&path, &records);
    } else {
        println!("altc compiler pipeline benchmark");
        println!(
            "small={} bytes, medium={} bytes, showcase={} bytes",
            small.len(),
            medium.len(),
            showcase.len()
        );
        for record in records {
            println!(
                "{:<20} {:12.0} ns/op {:9.2} MiB/s ({} iterations)",
                record.name,
                record.nanos_per_operation,
                record.mebibytes_per_second,
                record.iterations
            );
        }
    }
}

fn parse_json_path() -> Option<PathBuf> {
    let arguments = env::args()
        .skip(1)
        .filter(|argument| argument != "--bench")
        .collect::<Vec<_>>();
    match arguments.as_slice() {
        [] => None,
        [flag, path] if flag == "--json" => Some(PathBuf::from(path)),
        _ => panic!("expected no arguments or `--json PATH`, found {arguments:?}"),
    }
}

fn write_json(path: &PathBuf, records: &[BenchmarkRecord]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create benchmark JSON directory");
    }
    let report = BenchmarkReport {
        schema_version: 1,
        records: records
            .iter()
            .map(|record| BenchmarkRecord {
                name: record.name,
                input_bytes: record.input_bytes,
                iterations: record.iterations,
                nanos_per_operation: record.nanos_per_operation,
                mebibytes_per_second: record.mebibytes_per_second,
            })
            .collect(),
    };
    let mut output = serde_json::to_vec_pretty(&report).expect("serialize benchmark JSON");
    output.push(b'\n');
    fs::write(path, output).expect("write benchmark JSON");
}

fn measure(
    name: &'static str,
    bytes: usize,
    minimum_time: Duration,
    mut operation: impl FnMut(),
) -> BenchmarkRecord {
    for _ in 0..5 {
        operation();
    }
    let mut iterations = 1usize;
    loop {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        let elapsed = start.elapsed();
        if elapsed >= minimum_time {
            let nanos = elapsed.as_nanos() as f64 / iterations as f64;
            let mebibytes_per_second = bytes as f64 / (1024.0 * 1024.0) / (nanos / 1_000_000_000.0);
            return BenchmarkRecord {
                name,
                input_bytes: bytes,
                iterations,
                nanos_per_operation: nanos,
                mebibytes_per_second,
            };
        }
        iterations = iterations.saturating_mul(2);
        assert!(iterations > 0, "benchmark iteration overflow");
    }
}

fn synthetic_program(functions: usize) -> String {
    let mut source = String::new();
    source.push_str("fn function_0(value: i32) -> i32 { value + 1 }\n");
    for index in 1..functions {
        source.push_str(&format!(
            "fn function_{index}(value: i32) -> i32 {{ function_{}(value) + 1 }}\n",
            index - 1
        ));
    }
    source.push_str(&format!(
        "@entry\nfn application_main() -> c_int {{ function_{}(0) as c_int }}\n",
        functions - 1
    ));
    source
}
