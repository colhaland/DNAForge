use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use dnaforge::codec::{decode_bases, encode_bytes};

fn codec_benchmarks(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("codec");
    for size in [1024usize, 1024 * 1024] {
        let input: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let encoded = encode_bytes(&input);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("encode", size), &input, |bench, bytes| {
            bench.iter(|| encode_bytes(black_box(bytes)));
        });
        group.bench_with_input(
            BenchmarkId::new("decode", size),
            &encoded,
            |bench, bases| {
                bench.iter(|| decode_bases(black_box(bases)).expect("valid encoded data"));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, codec_benchmarks);
criterion_main!(benches);
