use std::hint::black_box;
use std::time::{Duration, Instant};

use altc_compiler::{AnalyzeOptions, CodegenOptions, SourceFile, analyze, emit_c};

fn main() {
    let small = include_str!("../../../tests/fixtures/data_control.altc");
    let medium = synthetic_program(200);
    let analyzed_medium = analyze(
        SourceFile::new("medium.altc", medium.clone()),
        &AnalyzeOptions::default(),
    );
    assert!(analyzed_medium.succeeded());

    println!("altc compiler pipeline benchmark");
    println!("small={} bytes, medium={} bytes", small.len(), medium.len());
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
    );
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
    );
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
    );
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
    );
}

fn measure(name: &str, bytes: usize, minimum_time: Duration, mut operation: impl FnMut()) {
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
            println!(
                "{name:20} {nanos:12.0} ns/op {mebibytes_per_second:9.2} MiB/s ({iterations} iterations)"
            );
            break;
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
