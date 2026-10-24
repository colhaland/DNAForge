# DNAForge

Baseline komputasi penyimpanan data berbasis DNA sintetis, ditulis dalam Rust stabil (edition 2024). Codec byte-ke-DNA dua bit, koreksi erasure Reed–Solomon over GF(256), simulator kesalahan kanal DNA (substitusi, inserti, deleti, dropout, duplikasi), integritas fragmen tingkat SHA-256, validasi batasan biologis (GC content, homopolymer), dan pipeline akhir ke akhir `encode → fragmentasi → simulasi → pemulihan → verifikasi` yang diekspos melalui CLI dengan pelaporan benchmark CSV/JSON.

DNAForge adalah platform komputasi untuk menguji algoritma penyimpanan data DNA sebelum eksperimen laboratorium dilakukan. Ini bukan alat sintesis DNA fisik dan tidak mengklaim ketahanan biologis berdasarkan simulasi sederhana.

[![Lisensi: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust stabil](https://img.shields.io/badge/rust-stable-8A2BE2.svg)](https://www.rust-lang.org/)
[![Edisi 2024](https://img.shields.io/badge/edition-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/)
[![Versi 0.1.0](https://img.shields.io/badge/version-0.1.0-gray.svg)](Cargo.toml)

## Daftar Isi

1. [Tentang DNAForge](#1-tentang-dnaforge)
2. [Fitur Utama](#2-fitur-utama)
3. [Arsitektur Sistem](#3-arsitektur-sistem)
4. [Modul Intipara](#4-modul-intipara)
5. [Pipeline Akhir ke Akhir](#5-pipeline-akhir-ke-akhir)
6. [Model Kesalahan Kanal DNA](#6-model-kesalahan-kanal-dna)
7. [Instalasi dan Build](#7-instalasi-dan-build)
8. [Panduan Cepat (CLI)](#8-panduan-cepat-cli)
9. [Referensi Perintah CLI](#9-referensi-perintah-cli)
10. [Referensi Teknis Detail](#10-referensi-teknis-detail)
11. [Benchmark dan Metrik](#11-benchmark-dan-metrik)
12. [Status Proyek Saat Ini](#12-status-proyek-saat-ini)
13. [Roadmap Pengembangan](#13-roadmap-pengembangan)
14. [Kriteria Keberhasilan](#14-kriteria-keberhasilan)
15. [Batasan dan Risiko Teknis](#15-batasan-dan-risiko-teknis)
16. [Kontribusi](#16-kontribusi)
17. [Kebijakan Keamanan](#17-kebijakan-keamanan)
18. [Lisensi](#18-lisensi)

---

## 1. Tentang DNAForge

Penyimpanan data DNA merepresentasikan informasi digital sebagai urutan basa adenin (A), sitosin (C), guanin (G), dan timin (T). Teoretis, setiap basa mewakili dua bit sehingga kepadatan informasi potensial sangat tinggi. Namun, sistem penyimpanan DNA nyata menghadapi tantangan efisiensi encoding, kesalahan pembacaan, kehilangan fragmen, overhead redundansi, biaya komputasi, dan skalabilitas.

DNAForge berfokus pada lapisan perangkat lunak dari masalah tersebut:

* Implementasi referensi DNA storage berbasis Rust stabil.
* Pipeline encoding hingga pemulihan data yang dapat direproduksi.
* Pengujian ketahanan algoritma terhadap berbagai model kesalahan DNA.
* Pengukuran kecepatan, memori puncak, overhead, dan tingkat recovery.
* Antarmuka CLI, format arsip eksperimen, pengujian otomatis, dan laporan benchmark.
* Arsitektur modular yang dapat diperluas untuk data sequencing publik dan eksperimen laboratorium.

**Prinsip inti:** DNAForge membedakan hasil yang diukur dari hasil simulasi atau proyeksi. Benchmark harus dapat direproduksi dan tidak mengklaim ketahanan biologis berdasarkan simulasi sederhana.

---

## 2. Fitur Utama

| Fitur | Deskripsi |
| --- | --- |
| Codec | Konversi byte ke basa DNA dan sebaliknya dengan pemetaan dua bit (00 → A, 01 → C, 10 → G, 11 → T). |
| Kanal simulasi | Substitusi, inserti, deleti, dropout fragmen, dan duplikasi dengan tingkat dan seed yang dapat dikonfigurasi. |
| Fragmentasi | Blok versi dengan ID logis, panjang payload, offset, dan SHA-256 integritas per fragmen. |
| Koreksi kesalahan | Reed–Solomon erasure coding over GF(256), toleransi hingga `r` erasure dengan `k` data fragmen. |
| Validasi | Deteksi dan klasifikasi blok korup, hilang, dan duplikat; pemisahan deteksi dari koreksi. |
| Batasan biologis | GC content, homopolymer, dan panjang urutan (pelaporan atau penolakan dalam mode ketat). |
| Pemulihan | Parsing, autentikasi SHA-256, rekonstruksi erasure, susunan ulang stream, decode, verifikasi hash. |
| CLI | Encode, decode, validate, simulate, recover, benchmark, dan inspect. |
| Laporan | Ekspor eksperimen ke CSV dan ringkasan JSON dengan metrik performa dan ketahanan. |
| Metrik | Throughput MiB/s, latensi per tahap, memori puncak (Linux), overhead penyimpanan, recovery rate. |

---

## 3. Arsitektur Sistem

```
Digital input (file/biner)
        │
        ▼
┌─────────────────┐
│ Codec           │ Bytes → DNA bases (2-bit mapping)
└─────────────────┘
        │
        ▼
┌─────────────────┐
│ Constraints     │ GC content, homopolymer, panjang urutan
└─────────────────┘
        │
        ▼
┌─────────────────┐
│ Fragment        │ Blok versi, ID, SHA-256, data/parity
└─────────────────┘
        │
        ▼
┌─────────────────┐
│ ECC (Reed–Sol)  │ Paritas fragmen, toleransi erasure
└─────────────────┘
        │
        ▼
┌─────────────────┐
│ Channel         │ Substitusi, inserti, deleti, dropout, duplikasi
└─────────────────┘
        │
        ▼
┌─────────────────┐
│ Recovery        │ Deteksi → koreksi erasure → susunan ulang → decode → verifikasi
└─────────────────┘
        │
        ▼
┌─────────────────┐
│ Metrics / Report│ Throughput, memori, overhead, recovery rate, CSV/JSON
└─────────────────┘
```

DNAForge memiliki dua jalur evaluasi yang dilaporkan secara terpisah:

* **Jalur performa** — biaya komputasi encoding, decoding, fragmentasi, dan pemulihan.
* **Jalur ketahanan** — kemampuan sistem memulihkan data setelah diberi kesalahan terkontrol.

Encoder yang cepat belum tentu menghasilkan data yang mudah dipulihkan.

---

## 4. Modul Intipara

| Modul | Tanggung jawab |
| --- | --- |
| `codec.rs` | Konversi byte ke basa DNA dan sebaliknya |
| `channel.rs` | Model kesalahan DNA yang dapat dikonfigurasi |
| `fragment.rs` | Pembagian data menjadi fragmen dengan ID dan metadata |
| `recovery.rs` | Validasi, koreksi erasure, dan rekonstruksi data |
| `metrics.rs` | Perhitungan metrik performa dan ketahanan |
| `storage.rs` | Format serialisasi dan pembacaan/penulisan arsip |
| `constraints.rs` | Validasi GC content, homopolymer, dan batasan urutan |
| `ecc.rs` | Algoritma Reed–Solomon error-correcting code |
| `pipeline.rs` | Orkestrasi seluruh proses encoding sampai recovery |
| `report.rs` | Ekspor hasil eksperimen ke CSV dan laporan ringkasan |
| `main.rs` | Antarmuka command-line (clap) |
| `benches/` | Benchmark komponen dan pipeline |
| `tests/` | Uji integritas fungsional |

---

## 5. Pipeline Akhir ke Akhir

1. **Encode** — file sumber diubah menjadi urutan DNA menggunakan skema yang dipilih; hash dan metadata awal dihitung.
2. **Batasan biologis** — dalam mode biologis, urutan yang melanggar aturan desain diperbaiki atau ditolak.
3. **Fragmentasi** — ditambahkan indeks, checksum, dan redundansi sesuai konfigurasi.
4. **Simulasi kanal** — kesalahan diterapkan dengan seed dan tingkat kesalahan yang tercatat.
5. **Pemulihan** — fragmen valid diidentifikasi, kesalahan dikoreksi jika memungkinkan, data disusun kembali.
6. **Verifikasi** — hash hasil pemulihan dibandingkan dengan hash file asli.
7. **Benchmark** — waktu, throughput, memori, overhead, dan keberhasilan recovery disimpan.

Setiap tahap dapat dijalankan secara mandiri melalui CLI sehingga kegagalan dapat dilokalkan.

---

## 6. Model Kesalahan Kanal DNA

Simulator mendukung tingkat kesalahan yang dapat dikontrol dan menggunakan seed yang dapat diulang:

| Jenis | Deskripsi |
| --- | --- |
| Substitution | Basa berubah, misalnya A menjadi G. |
| Insertion | Basa tambahan muncul sebelum basis yang ada. |
| Deletion | Basa asli hilang dari urutan. |
| Dropout | Seluruh fragmen tidak tersedia pada proses pembacaan. |
| Duplication | Fragmen terbaca lebih dari sekali (salinan tambahan). |
| Mixed errors | Beberapa jenis kesalahan terjadi bersamaan. |

Setiap eksperimen mencatat jenis kesalahan, tingkat yang diminta, jumlah perubahan aktual, seed, serta panjang urutan sebelum dan sesudah simulasi. Model ini adalah model komputasi, bukan model laboratorium tervalidasi; kesalahan nyata dapat bergantung pada konteks urutan, teknologi sequencing, dan proses kimia.

---

## 7. Instalasi dan Build

### Prasyarat

* Rust stable (edition 2024)
* Cargo

### Build

```bash
# Clone repositori
git clone <repositori>
cd DNAForge

# Build dalam mode development
cargo build

# Build dalam mode release (direkomendasikan untuk benchmark)
cargo build --release

# Jalankan seluruh pengujian unit
cargo test

# Periksa format kode
cargo fmt --check

# Periksa lint dengan Clippy
cargo clippy --all-targets
```

Biner CLI dihasilkan di `target/release/dnaforge`.

---

## 8. Panduan Cepat (CLI)

Semua perintah menggunakan pasangan `INPUT` dan `OUTPUT`.

### Encode file menjadi arsip DNAForge

```bash
./target/release/dnaforge encode input.bin archive/ \
  --fragment-size 1024 \
  --ecc-k 4 --ecc-r 2 \
  --strict \
  --gc-min 0.3 --gc-max 0.7 --max-homopolymer 5
```

### Decode arsip kembali ke byte

```bash
./target/release/dnaforge decode archive/ output.bin
```

### Validasi struktur, checksum, dan batasan arsip

```bash
./target/release/dnaforge validate archive/
```

### Simulasi kesalahan kanal pada arsip

```bash
./target/release/dnaforge simulate archive/ corrupted/ \
  --sub 0.01 --ins 0.005 --del 0.005 \
  --dropout 0.01 --dup 0.005 \
  --seed 42
```

### Pemulihan data dari arsip yang mungkin korup

```bash
./target/release/dnaforge recover corrupted/ recovered.bin \
  --original input.bin
```

### Benchmark dan ekspor laporan

```bash
./target/release/dnaforge benchmark input.bin report.csv \
  --report report.csv \
  --sizes "1KiB,64KiB,1MiB" \
  --seeds "1,2,3" \
  --ecc-k 4 --ecc-r 2 \
  --sub 0.01 --seed 42
```

Format laporan ditentukan dari ekstensi berkas (`.csv` atau `.json`).

### Inspect arsip

```bash
./target/release/dnaforge inspect archive/
```

Gunakan `dnaforge --help` dan `dnaforge <perintah> --help` untuk detail opsi lengkap.

---

## 9. Referensi Perintah CLI

| Perintah | Fungsi | Keluar kode |
| --- | --- | --- |
| `encode` | Encode file ke arsip DNA (byte → DNA → fragmen). | 0 = sukses |
| `decode` | Decode arsip kembali ke byte. | 0 = sukses |
| `validate` | Validasi struktur, checksum, dan batasan arsip. | 2 = ada korupsi / pelanggaran |
| `simulate` | Simulasi kesalahan kanal pada arsip. | 0 = sukses |
| `recover` | Pulihkan data dari (mungkin korup) arsip. | 3 = hash tidak cocok |
| `benchmark` | Benchmark encode/decode/recover dan ekspor CSV/JSON. | 0 = sukses |
| `inspect` | Lihat manifest dan ringkasan per-fragmen arsip. | 0 = sukses |

---

## 10. Referensi Teknis Detail

### 10.1 Codec: Konversi Byte ke DNA

Tipe `Base` mewakili satu basa sebagai nilai u8 0..3:

| Nilai | Basa | Bit |
| --- | --- | --- |
| 0 | A | 00 |
| 1 | C | 01 |
| 2 | G | 10 |
| 3 | T | 11 |

Pemetaan dua bit ke satu basa. Setiap byte diubah menjadi tepat 4 basis, bit paling signifikan dulu (MSB-first):

```
base[0] = (b >> 6) & 0b11
base[1] = (b >> 4) & 0b11
base[2] = (b >> 2) & 0b11
base[3] = (b >> 0) & 0b11
```

Panjang output selalu `4 * input.len()`. Decoding mengelompokkan setiap 4 basis menjadi satu byte. Jika jumlah basis bukan kelipatan 4, decoding gagal dengan `CodecError::IncompleteByte`. Basis di luar rentang 0..3 gagal dengan `CodecError::InvalidBase`. Codec adalah bijeksi murni dan tidak menjamin properti biologis apa pun.

### 10.2 Format Blok Fragmen (Arsip)

Setiap fragmen diserialisasikan menjadi blok byte berukuran tetap: header 62 byte diikuti payload yang di-zero-padding ke `block_payload_size` (kelipatan 16, minimal 16).

Tata letak header (semua integer little-endian):

| Offset | Panjang | Field | Keterangan |
| --- | --- | --- | --- |
| [0..4] | 4 | Magic | `"DNAF"` |
| [4..6] | 2 | Version | `u16` (saat ini 1) |
| [6..14] | 8 | Fragment ID | `u64`, unik dalam arsip |
| [14..18] | 4 | Total data | `u32`, jumlah fragmen data |
| [18..22] | 4 | Payload length | `u32`, basis (tidak dipad) |
| [22..26] | 4 | Offset basis | `u32`, posisi dalam stream DNA |
| [26..58] | 32 | Payload hash | SHA-256 dari payload |
| [58..62] | 4 | Flags | `u32`, bit 0 = parity |
| [62..] | N | Payload | Nilai `Base` 0..3, zero-padded |

`HEADER_SIZE = 62`. Ukuran blok terpisah adalah `HEADER_SIZE + block_payload_size`. Deserialisasi memvalidasi magic, versi, ukuran blok, panjang payload, dan cocokkan SHA-256 payload.

### 10.3 Koreksi Kesalahan Reed–Solomon (GF(256))

Implementasi RS[n, k] atas GF(256) dengan polinomial primitif `0x11d` dan generator alpha = 2. Tabel exponent/log dibangun sekali secara statis melalui `LazyLock`.

* `n = k + r <= 255` karena GF(256) memiliki 255 titik evaluasi non-nol.
* Setiap posisi byte dalam blok adalah simbol GF(256) independen.
* Paritas `r` dihitung melalui koefisien Lagrange `T[i][j] = L_i(alpha^(k+j))`.
* Properti MDS: semua `k` dari `n` simbol mencukupi untuk rekonstruksi seluruh codeword, sehingga hingga `r = n - k` erasure dapat dikoreksi.
* Koreksi erasure (posisi hilang/korup diketahui melalui lapisan checksum), bukan koreksi substitusi pada posisi tidak diketahui.

`ReedSolomonConfig::new(k, r)` menolak `k == 0` atau `n > 255`.

### 10.4 Simulator Kanal DNA

Simulator menggunakan `rand::rngs::StdRng::seed_from_u64` untuk output deterministik per seed dan input yang sama. Kesalahan diterapkan secara independen:

* Substitusi, inserti, deleti: probabilitas per basis.
* Dropout, duplikasi: probabilitas per fragmen.

Tipe peristiwa `ErrorEvent`: Substitution, Insertion, Deletion, Dropout, Duplication. `ChannelConfig::validate` menolak setiap tingkat di luar rentang `[0, 1]`.

Simulator adalah model komputasi, bukan model laboratorium tervalidasi.

### 10.5 Pipeline Pemulihan Tahap demi Tahap

1. **Parsing & autentikasi** — setiap blok di-deserialisasi; magic, versi, ukuran, dan SHA-256 divalidasi.
2. **Klasifikasi erasure** — fragmen data yang hilang/korup dihitung; jika melebihi redundansi `r`, pemulihan gagal dengan `TooManyErasures`.
3. **Koreksi erasure Reed–Solomon** — blok yang tersisa dan paritas digunakan untuk rekonstruksi.
4. **Susunan ulang stream DNA** — fragmen data diurutkan berdasarkan `offset_bases`; posisi harus kontigu.
5. **Decode DNA → byte** — 4 basis dikembalikan menjadi 1 byte via codec.
6. **Validasi batasan** (opsional) — GC content dan homopolymer diperiksa terhadap `ConstraintConfig`.
7. **Verifikasi** — SHA-256 hasil pemulihan dibandingkan dengan hash asli.

Setiap tahap diukur waktu (ms) dan dilaporkan dalam `StageTiming` (parse, ecc, reassemble, decode, verify, total).

### 10.6 Format Arsip dan Manifest

Arsip adalah direktori yang berisi:

* `manifest.json` — metadata arsip.
* `frag_{id:06}.bin` — blok fragmen data atau parity.
* `frag_{id:06}_dup{N}.bin` — berkas duplikat.

`ArchiveManifest`:

| Field | Tipe | Keterangan |
| --- | --- | --- |
| format | String | `"dnaforge-archive"` |
| version | u16 | Versi format fragmen |
| original_size_bytes | u64 | Ukuran berkas asli |
| original_sha256 | String | Hash SHA-256 heksadesimal asli |
| dna_bases | u64 | Jumlah total basis DNA |
| fragment_size_bases | u64 | Basis per payload fragmen |
| block_payload_size | u64 | Ukuran payload blok (di-pad) |
| header_size | u64 | `62` |
| block_size | u64 | `header_size + block_payload_size` |
| ecc_k | u64 | Jumlah fragmen data |
| ecc_r | u64 | Jumlah fragmen paritas |
| fragment_count | u64 | Total semua fragmen |
| gc_min / gc_max | Option<f64> | Rentang GC yang dikonfigurasi |
| max_homopolymer | Option<usize> | Batas run homopolymer |
| created_at | Option<String> | Stempel waktu pembuatan (detik Unix) |

### 10.7 Metrik dan Rumus

| Metrik | Rumus |
| --- | --- |
| Throughput MiB/s | `bytes_processed / (elapsed_ms / 1000) / (1024 * 1024)` |
| Overhead penyimpanan | `(encoded_bytes - original_bytes) / original_bytes * 100%` |
| Basis per byte | `dna_bases / original_bytes` |
| Recovery rate fragmen | `fragments_recovered / fragments_total` |

Exact-file recovery: hash SHA-256 hasil pemulihan identik dengan hash berkas asli. Memori puncak (peak RSS) dibaca dari `/proc/self/status` field `VmHWM` (khusus Linux); `None` pada platform lain.

### 10.8 Hierarki Penanganan Kesalahan

* `PipelineError` — Io, Storage, Format, Codec, Channel, Recovery, Ecc, Config, ConstraintViolations, HashMismatch, TooManyErasures.
* `RecoveryError` — NotEnoughDataBlocks, TooManyErasures, BlockCountMismatch, BlockSizeMismatch, Ecc, Format, Codec, HashMismatch, Io.
* `ChannelError` — InvalidRate.
* `FormatError` — BadMagic, UnsupportedVersion, BlockSizeMismatch, PayloadTooLarge, PayloadHashMismatch, Hash, Io.
* `ReedSolomonError` — TooFewErasureSymbols, BlockCountMismatch, BlockSizeMismatch, TooManyFragments.
* `StorageError` — Io, Manifest, Format, BlockSizeMismatch.

Semua tipe mengimplementasi `std::error::Error` dengan `From` untuk komposisi kesalahan antar-modul.

### 10.9 Batasan Versi dan Kompatibilitas

* Versi format fragmen saat ini: `1` (`FRAGMENT_FORMAT_VERSION`).
* `n = k + r` tidak boleh melebihi 255 (batas GF(256)).
* Payload blok di-pad ke kelipatan 16 byte; fragmen terakhir dapat lebih pendek dan di-pad dengan nol.
* `data_index` tidak disimpan dalam tata letak blok; pulih dari urutan arsip oleh lapisan arsip.
* Codec dua bit adalah baseline komputasi; belum siap sintesis biologis tanpa pertimbangan GC, homopolymer, distribusi basis, dan batasan panjang urutan.

---

## 11. Benchmark dan Metrik

Benchmark dijalankan pada eksekusi nyata, bukan proyeksi. Setiap skenario diulang dengan seed terdokumen dan melaporkan median serta variansi.

| Grup | Cakupan |
| --- | --- |
| A. Performa | Ukuran input 1 KiB, 64 KiB, 1 MiB, 16 MiB, 100 MiB (dan 1 GiB jika sumber daya memungkinkan); encoding/decoding, buffer baru versus reuse, satu thread versus multithread, memori penuh versus streaming. |
| B. Ketahanan | Tingkat kesalahan 0 %, 0,1 %, 0,5 %, 1 %, 2 %, dan 5 % per jenis; dengan dan tanpa koreksi kesalahan, redundansi, berbagai ukuran fragmen, dan beberapa seed acak. |
| C. Efisiensi penyimpanan | Bandingkan ukuran file asli dengan representasi DNA dan overhead sistem. Laporan kapasitas payload efektif, bukan hanya kepadatan teoritis dua bit per basa. |

```
Overhead = (Ukuran encoded − Ukuran payload) / Ukuran payload × 100%
```

Ukuran encoded mencakup semua metadata, indeks, dan redundansi yang diperlukan skema yang diuji.

---

## 12. Status Proyek Saat Ini

DNAForge versi `0.1.0` adalah baseline komputasi yang fungsional:

* Codec dua bit dengan uji round-trip.
* Simulator kesalahan kanal yang dapat dikonfigurasi dan deterministik.
* Fragmentasi berbasis blok dengan SHA-256 dan versi format.
* Koreksi erasure Reed–Solomon over GF(256).
* Pipeline end-to-end encode → fragmentasi → simulasi kerusakan → recovery → verifikasi hash.
* CLI lengkap dan laporan benchmark CSV/JSON.
* Pengujian unit di setiap modul.

**Belum terverifikasi sebagai sistem DNA storage lengkap.** Direktori `benches/` dan `tests/` masih kosong, dan workflow CI `.github/workflows` belum dikonfigurasi. Target angka untuk throughput, memori, atau recovery rate ditetapkan setelah baseline nyata diperoleh, bukan berdasarkan estimasi yang belum terbukti.

---

## 13. Roadmap Pengembangan

| Fase | Fokus | Syarat Selesai |
| --- | --- | --- |
| 1 | Validasi — stabilkan baseline | Kompilasi, uji round-trip, validasi input, benchmark dapat direproduksi, pencatatan konfigurasi. |
| 2 | Recovery — fragmentasi dan koreksi kesalahan | ID fragmen, checksum, skema koreksi teruji, rekonstruksi, laporan exact-file recovery. |
| 3 | Biologis — desain urutan layak uji | Batasan GC content, homopolymer, panjang urutan, skema desain untuk target eksperimen. |
| 4 | Optimasi — performa | Streaming, paralelisme, pengurangan alokasi, SIMD, uji memori. |
| 5 | Validasi — data nyata | Dataset sequencing publik, bandingkan model kesalahan dengan pembacaan tersedia. |

---

## 14. Kriteria Keberhasilan

| Dimensi | Kriteria |
| --- | --- |
| Kebenaran | File yang pulih sempurna memiliki hash identik dengan sumber. |
| Ketahanan | Tingkat pemulihan diukur pada berbagai jenis dan tingkat kesalahan. |
| Efisiensi | Throughput, memori puncak, dan overhead dicatat. |
| Reproduksibilitas | Konfigurasi, seed, versi kode, dan hasil eksperimen tersimpan. |

---

## 15. Batasan dan Risiko Teknis

* Simulasi bukan eksperimen biologis. Kanal acak sederhana tidak sepenuhnya mencerminkan proses sintesis, penyimpanan, amplifikasi, dan sequencing.
* Kapasitas teoritis bukan kapasitas efektif. Metadata, batasan biologis, indeks, dan koreksi kesalahan mengurangi kapasitas payload.
* Koreksi kesalahan memiliki trade-off. Koreksi lebih kuat biasanya memerlukan redundansi, waktu komputasi, atau keduanya.
* Throughput harus dibuktikan. Angka benchmark berasal dari eksekusi nyata pada perangkat yang spesifik.
* Penyimpanan DNA fisik memerlukan mitra eksperimen. Software saja tidak membuktikan umur simpan, stabilitas kimia, atau keberhasilan sintesis.

---

## 16. Kontribusi

1. Fork repositori dan buat cabang fitur dari `main`.
2. Tambahkan uji untuk fitur atau perbaikan baru.
3. Pastikan `cargo test`, `cargo fmt --check`, dan `cargo clippy --all-targets` lulus.
4. Buat pull request dengan deskripsi perubahan dan metrik yang relevan.

Laporkan bugs dan usulan fitur melalui Issues.

---

## 17. Kebijakan Keamanan

DNAForge adalah perangkat lunak sumber terbuka. Tidak ada rahasia atau kunci yang disimpan dalam repositori. Jika Anda menemukan masalah keamanan, laporkan melalui Issues dengan detail yang cukup untuk direproduksi.

---

## 18. Lisensi

Disenjakan di bawah [MIT OR Apache-2.0](LICENSE).