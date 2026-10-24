# DNAForge

DNAForge adalah baseline komputasi penyimpanan data berbasis DNA sintetis, ditulis dengan Rust stable. Repositori menyediakan codec dua-bit, fragmentasi, Reed-Solomon untuk erasure, simulator kanal berbasis seed, pemeriksaan batasan urutan, pipeline arsip, CLI, dan ekspor benchmark.

> DNAForge saat ini adalah simulator dan prototipe perangkat lunak. Arsip binary dan hasil simulasi bukan DNA sintetis, bukan bukti kelayakan laboratorium, dan tidak menjamin ketahanan biologis.

## Dokumentasi

- [Indeks dokumentasi](docs/README.md)
- [Spesifikasi baseline](docs/baseline-specification.md): kontrak sistem, format data, algoritma, CLI, persyaratan, dan acceptance criteria.
- [Rencana implementasi](docs/implementation-plan.md): urutan pekerjaan, dependensi, dan gerbang penyelesaian.

Spesifikasi membedakan perilaku yang tersedia pada kode `0.1.0` dari target yang wajib dipenuhi sebelum disebut implementasi penuh. Jika README ini berbeda dari spesifikasi, spesifikasi menjadi sumber acuan.

## Build dan verifikasi

Persyaratan: Rust stable dengan edition 2024 dan Cargo.

```bash
cargo build
cargo build --release
cargo fmt --check
cargo clippy --all-targets
cargo test
```

Pada kondisi baseline yang terdokumentasi, `cargo test` belum lulus kompilasi test. Lihat [gap baseline](docs/baseline-specification.md#13-gap-baseline-yang-diketahui) untuk rincian dan gerbang perbaikannya.

## CLI

```bash
cargo run -- encode input.bin --output archive/ --fragment-size 1024 --ecc-k 4 --ecc-r 2
cargo run -- decode archive/ --output output.bin
cargo run -- validate archive/
cargo run -- simulate archive/ --output corrupted/ --seed 42
cargo run -- recover corrupted/ --output recovered.bin --original input.bin
cargo run -- benchmark input.bin --report report.csv --seeds 1,2,3
cargo run -- benchmark ignored.bin --report synthetic.json --synthetic-sizes 1KiB,64KiB
cargo run -- inspect archive/
```

Subperintah yang membuat berkas/arsip menerima tujuan eksplisit melalui `--output`; `validate` dan `inspect` hanya membutuhkan arsip. Benchmark memakai isi file input secara default. Opsi `--synthetic-sizes` secara eksplisit memilih data sintetis deterministik; `--seed` menentukan seed generator dan `--seeds` seed kanal tiap run. Opsi lengkap: `cargo run -- --help` dan `cargo run -- <command> --help`.

## Status

Versi paket saat ini `0.1.0`. Test unit/integration, `cargo fmt`, dan Clippy kini berjalan; CI menjalankannya pada Linux, macOS, dan Windows. Format arsip masih v1 eksperimental dan beberapa persyaratan integritas, ketahanan, metrik, serta kompatibilitas belum ditutup. Karena itu proyek belum dapat diklaim sebagai rilis produksi stabil. DNAForge adalah simulator komputasi saja—bukan DNA sintetis, bukti kelayakan laboratorium, atau jaminan ketahanan biologis.