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
cargo run -- encode input.bin archive/ --fragment-size 1024 --ecc-k 4 --ecc-r 2
cargo run -- decode archive/ output.bin
cargo run -- validate archive/ ignored-output
cargo run -- simulate archive/ ignored-output --output corrupted/ --seed 42
cargo run -- recover corrupted/ recovered.bin --original input.bin
cargo run -- benchmark input.bin ignored-output --report report.csv
cargo run -- inspect archive/ ignored-output
```

Argumen `INPUT OUTPUT` dipakai oleh seluruh subperintah saat ini karena memakai argumen CLI bersama; pada `simulate`, `benchmark`, dan `inspect`, output positional tidak menjadi tujuan hasil utama. Kontrak tersebut diketahui belum konsisten dan harus dirapikan sesuai spesifikasi sebelum CLI dinyatakan stabil. Opsi lengkap: `cargo run -- --help` dan `cargo run -- <command> --help`.

## Status

Versi paket saat ini `0.1.0`. Format arsip v1 adalah baseline eksperimental, bukan kontrak kompatibilitas jangka panjang. Jangan gunakan untuk penyimpanan data produksi atau eksperimen fisik tanpa validasi tambahan.