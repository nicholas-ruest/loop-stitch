use std::f64::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("loop-stitch-test-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).expect("create test directory");
    path
}

fn write_fixture(path: &Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 1_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("create fixture");
    for frame in 0..2_500 {
        let sample = ((frame as f64 / 100.0 * TAU).sin() * 20_000.0) as i16;
        writer.write_sample(sample).expect("write fixture sample");
    }
    writer.finalize().expect("finalize fixture");
}

#[test]
fn analyze_and_render_complete_a_vertical_slice() {
    let directory = temporary_directory();
    let input = directory.join("input.wav");
    let output = directory.join("loop.wav");
    write_fixture(&input);

    let analyze = Command::new(env!("CARGO_BIN_EXE_loop-stitch"))
        .args([
            "analyze",
            input.to_str().expect("UTF-8 input path"),
            "--target-ms",
            "1000",
            "--tolerance-ms",
            "20",
            "--start-search-ms",
            "20",
            "--compare-ms",
            "20",
            "--step-ms",
            "1",
        ])
        .output()
        .expect("run analyze");
    assert!(analyze.status.success());
    let plans: serde_json::Value =
        serde_json::from_slice(&analyze.stdout).expect("analyze JSON output");
    assert_eq!(plans.as_array().expect("plan array").len(), 5);

    let render = Command::new(env!("CARGO_BIN_EXE_loop-stitch"))
        .args([
            "render",
            input.to_str().expect("UTF-8 input path"),
            output.to_str().expect("UTF-8 output path"),
            "--target-ms",
            "1000",
            "--tolerance-ms",
            "20",
            "--start-search-ms",
            "20",
            "--compare-ms",
            "20",
            "--step-ms",
            "1",
        ])
        .output()
        .expect("run render");
    assert!(render.status.success());
    let reader = hound::WavReader::open(&output).expect("open rendered loop");
    assert!((980..=1_020).contains(&reader.duration()));

    let overwrite = Command::new(env!("CARGO_BIN_EXE_loop-stitch"))
        .args([
            "render",
            input.to_str().expect("UTF-8 input path"),
            output.to_str().expect("UTF-8 output path"),
            "--target-ms",
            "1000",
        ])
        .output()
        .expect("run protected render");
    assert!(!overwrite.status.success());
    assert!(String::from_utf8_lossy(&overwrite.stderr).contains("output already exists"));

    fs::remove_dir_all(directory).expect("remove test directory");
}
