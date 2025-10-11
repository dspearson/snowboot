// Benchmarks for Snowboot performance

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use snowboot::validation::*;
use snowboot::config::Config;

fn validation_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation");

    group.bench_function("validate_hostname", |b| {
        b.iter(|| validate_hostname("streaming.example.com"));
    });

    group.bench_function("validate_port", |b| {
        b.iter(|| validate_port(8000));
    });

    group.bench_function("validate_sample_rate", |b| {
        b.iter(|| validate_sample_rate(44100));
    });

    group.bench_function("validate_bitrate", |b| {
        b.iter(|| validate_bitrate(320));
    });

    group.bench_function("parse_host_port", |b| {
        b.iter(|| parse_host_port("example.com:8080"));
    });

    group.finish();
}

fn config_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("config");

    group.bench_function("default_config", |b| {
        b.iter(|| Config::default());
    });

    group.bench_function("validate_config", |b| {
        let config = Config::default();
        b.iter(|| config.validate());
    });

    group.finish();
}

fn data_processing_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_processing");

    // Benchmark different chunk sizes
    for size in [1024, 4096, 8192, 16384].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let data = vec![0u8; size];
            b.iter(|| {
                // Simulate processing a chunk
                let _checksum: u32 = data.iter().map(|&x| x as u32).sum();
            });
        });
    }

    group.finish();
}

criterion_group!(benches, validation_benchmarks, config_benchmarks, data_processing_benchmarks);
criterion_main!(benches);
