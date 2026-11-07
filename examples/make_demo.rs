#![forbid(unsafe_code)]

use std::error::Error;
use std::f64::consts::TAU;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("demo.wav"));
    let sample_rate = 8_000;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&output, spec)?;
    for frame in 0..(sample_rate * 5) {
        let time = f64::from(frame) / f64::from(sample_rate);
        let base = (time * 220.0 * TAU).sin();
        let overtone = (time * 330.0 * TAU).sin() * 0.28;
        let pulse = (time * 0.5 * TAU).sin() * 0.08;
        let left = ((base + overtone + pulse) * 10_000.0) as i16;
        let right = ((base - overtone + pulse) * 10_000.0) as i16;
        writer.write_sample(left)?;
        writer.write_sample(right)?;
    }
    writer.finalize()?;
    println!("wrote {}", output.display());
    Ok(())
}
