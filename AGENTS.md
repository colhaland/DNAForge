---

# AGENTS.md - Panduan Perilaku AI untuk DNAForge

Dokumen ini mengatur perilaku, standar, dan batasan mutlak bagi agen AI yang berkontribusi pada repositori **DNAForge**. Pelanggaran terhadap aturan inti (Core Directives) tidak dapat diterima.

## 🚫 1. Direktif Inti (Prime Directives)
1. **DILARANG KERAS Implementasi Setengah Jadi**: Jangan pernah menghasilkan kode yang mengandung `todo!()`, `unimplemented!()`, `panic!("not implemented")`, komentar `// TODO:`, atau placeholder seperti `// implement later`. Jika sebuah fitur diminta, implementasikan secara **lengkap, fungsional, dan siap uji**.
2. **DILARANG KERAS Membuat Roadmap Sebagai Ganti Kode**: Jangan menjawab permintaan fitur dengan "Berikut adalah rencana implementasi..." atau "Langkah selanjutnya adalah...". Langsung tulis kode yang menyelesaikan masalah tersebut.
3. **Jaga Arsitektur Tetap Bersih**: Jangan menumpuk logika bisnis di `main.rs`. `main.rs` HANYA untuk parsing CLI (`clap`) dan memanggil fungsi dari modul `pipeline` atau `storage`.
4. **Kualitas Kode > Kecepatan Generasi**: Kode yang dihasilkan harus production-grade, idiomatic Rust, aman (memory-safe), dan efisien. Jangan mengorbankan kualitas demi menghemat token.

## 🏗️ 2. Arsitektur & Organisasi Repositori
Pertahankan struktur modular yang sudah ada. Jangan membuat modul top-level baru tanpa justifikasi arsitektural yang kuat.
- `src/main.rs`: Entry point CLI saja.
- `src/lib.rs`: Ekspos modul publik.
- `src/codec.rs`: Konversi byte <-> DNA (2-bit mapping).
- `src/constraints.rs`: Validasi GC content dan homopolimer.
- `src/ecc.rs`: Reed-Solomon GF(256) erasure coding.
- `src/channel.rs`: Simulator error DNA (substitusi, insersi, delesi, dropout).
- `src/fragment.rs`: Struktur blok fragmen dengan integritas SHA-256.
- `src/recovery.rs`: Logika koreksi error dan rekonstruksi.
- `src/pipeline.rs`: Orkestrasi end-to-end (encode, decode, simulate, recover).
- `src/storage.rs`: I/O file dan manajemen `manifest.json`.
- `src/report.rs`: Ekspor benchmark (CSV/JSON).

**Aturan Modifikasi**: Jika Anda menambahkan fungsi baru, tempatkan di modul yang paling relevan secara semantik. Jika fungsi tersebut membutuhkan dependensi silang, gunakan `crate::module::function`.

## ⚙️ 3. Workflow & Verifikasi Wajib
Sebelum menganggap tugas selesai, AI harus memastikan (atau menyimulasikan memastikan) bahwa kode memenuhi standar berikut:
1. **Format**: Kode harus lulus `cargo fmt --check`.
2. **Linting**: Kode harus lulus `cargo clippy --all-targets` tanpa peringatan (warning). Tangani semua saran clippy (misalnya, gunakan `if let` alih-alih `match` yang tidak perlu, atau perbaiki lifetime).
3. **Kompilasi**: Kode harus lulus `cargo build` dan `cargo build --release`.
4. **Testing**: Kode harus lulus `cargo test`. **Setiap fungsi publik baru atau perubahan logika WAJIB disertai unit test.**

## 🦀 4. Standar Kualitas Kode Rust
1. **Penanganan Error (Error Handling)**:
   - JANGAN gunakan `.unwrap()` atau `.expect()` di kode library (`src/lib.rs` dan modulnya). Gunakan `Result<T, CustomError>` atau `Option<T>`.
   - Gunakan enum error kustom yang mengimplementasikan `std::error::Error` dan `std::fmt::Display` (contoh: `CodecError`, `ReedSolomonError`).
   - Gunakan operator `?` untuk propagasi error yang bersih.
2. **Performa & Memori**:
   - Gunakan `Vec::with_capacity()` saat ukuran akhir koleksi diketahui atau dapat diperkirakan.
   - Hindari kloning (`clone()`) data yang besar (seperti array byte atau string DNA) tanpa alasan yang jelas. Gunakan referensi (`&[u8]`, `&[Base]`) sebisa mungkin.
   - Gunakan `chunks` atau `chunks_exact` untuk iterasi array yang efisien.
3. **Konsistensi Penamaan**:
   - Fungsi/Variabel: `snake_case`
   - Struct/Enum/Trait: `PascalCase`
   - Konstanta: `UPPER_SNAKE_CASE`
   - Ikuti konvensi penamaan yang sudah ada di file terkait.

## 🧬 5. Aturan Spesifik Domain (DNA Data Storage)
1. **Batasan Reed-Solomon**: Ingat bahwa implementasi GF(256) membatasi total fragmen (`k + r`) maksimal **255**. Pastikan validasi ini selalu ada dan error `TooManyFragments` dikembalikan jika dilanggar.
2. **Deterministik**: Semua simulasi kanal (`channel.rs`) dan pembangkit data uji **WAJIB** deterministik berdasarkan `seed`. Jangan gunakan `rand::thread_rng()` untuk logika yang harus dapat direproduksi; gunakan `StdRng::seed_from_u64()`.
3. **Integritas Data**: Setiap fragmen atau blok data yang disimpan harus diverifikasi menggunakan SHA-256. Jangan pernah melewati langkah verifikasi ini dalam pipeline `recover` atau `validate`.
4. **Disclaimer Simulasi**: Jangan menambahkan kode yang mengklaim bahwa output ini "siap sintesis laboratorium" tanpa validasi eksternal. Pertahankan nada dokumentasi bahwa ini adalah baseline komputasional/simulator.

## 🧪 6. Standar Pengujian (Testing)
1. Setiap modul harus memiliki blok `#[cfg(test)] mod tests { ... }` di bagian bawah file.
2. Uji kasus harus mencakup:
   - **Happy Path**: Input valid, output sesuai harapan.
   - **Edge Cases**: Input kosong, ukuran fragmen 0, batas maksimal `k+r=255`.
   - **Error Paths**: Memastikan error yang tepat dikembalikan saat input tidak valid (misalnya, string DNA dengan karakter 'X').
3. Gunakan `proptest` untuk pengujian berbasis properti (property-based testing) pada fungsi kritis seperti `encode_bytes` ↔ `decode_bases` (round-trip property).

## 💬 7. Format Output & Komunikasi AI
- Saat memberikan kode, berikan **hanya** kode yang diubah atau ditambahkan, beserta konteks path file-nya (misal: `// src/codec.rs`).
- Jangan memberikan penjelasan panjang lebar tentang "mengapa" Rust bekerja seperti itu, kecuali diminta. Fokus pada "apa yang diubah" dan "bagaimana ini memenuhi persyaratan".
- Jika ada ambiguitas dalam permintaan pengguna, **ajukan pertanyaan klarifikasi singkat** sebelum menghasilkan kode, jangan menebak dan membuat asumsi yang merusak arsitektur.

---
