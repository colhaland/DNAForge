# Spesifikasi Baseline DNAForge

| Properti | Nilai |
| --- | --- |
| Status dokumen | Baseline normatif untuk pengembangan penuh |
| Versi spesifikasi | 1.0 |
| Versi implementasi yang diperiksa | `dnaforge 0.1.0` |
| Format arsip yang ada | v1, eksperimental |
| Bahasa dan toolchain | Rust stable, edition 2024 |
| Platform target | Linux, macOS, Windows; metrik RSS bersifat platform-dependent |

Dokumen ini menetapkan target perilaku dan kontrak yang harus diwujudkan. Label **Saat ini** menjelaskan perilaku kode yang teramati; label **Target** menetapkan hasil yang harus dicapai sebelum rilis stabil. Persyaratan target tidak boleh disimpulkan sebagai fitur yang sudah selesai.

## 1. Tujuan dan ruang lingkup

DNAForge adalah perangkat lunak penelitian untuk mengubah berkas byte menjadi representasi urutan basa, membagi payload, menambahkan redundansi, mensimulasikan gangguan, memulihkan data, dan mengukur hasil secara terulang.

Tujuan utama:

- Menyediakan implementasi referensi yang lossless dan dapat diuji.
- Memisahkan deteksi korupsi, koreksi erasure, rekonstruksi, dan verifikasi integritas.
- Merekam konfigurasi eksperimen, seed, versi format, serta metrik yang dapat ditelusuri.
- Menetapkan batas jelas antara perilaku komputasi dan klaim biologis.

Di luar ruang lingkup: sintesis DNA, kimia, penyimpanan fisik, sequencing, koreksi error tak diketahui berbasis sequencing, kriptografi/kerahasiaan, serta klaim umur simpan. Integrasi dataset sequencing dapat ditambahkan sebagai adapter terpisah; tidak mengubah perilaku inti tanpa spesifikasi baru.

## 2. Prinsip normatif dan istilah

Kata **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, dan **MAY** menyatakan tingkat kewajiban persyaratan.

- **Basa**: satu simbol dari alfabet `A`, `C`, `G`, `T`, direpresentasikan internal sebagai nilai 0 sampai 3.
- **Fragmen data**: bagian urutan DNA yang membawa payload sumber.
- **Fragmen parity**: shard redundansi yang dibuat oleh kode erasure. Nilai byte parity bukan otomatis basa DNA yang valid.
- **Erasure**: fragmen yang hilang atau diketahui tidak valid lokasinya.
- **Korupsi**: perubahan yang terdeteksi melalui struktur/checksum/hash. Korupsi tidak sama dengan kesalahan yang lokasinya tidak diketahui.
- **Stripe**: satu kelompok tepat `k` fragmen data dan `r` fragmen parity yang dikodekan bersama.
- **Pemulihan tepat**: byte hasil identik dengan sumber, dibuktikan dengan SHA-256 dan, dalam test, perbandingan byte.
- **Arsip**: manifest beserta blok fragmen dan metadata yang diperlukan untuk membaca ulang konfigurasi serta merekonstruksi sumber.

## 3. Pengguna dan alur utama

Alur utama target:

1. Encode berkas sumber menjadi DNA, bentuk stripe, hitung integritas, dan tulis arsip.
2. Validate arsip tanpa mengubahnya; keluarkan temuan yang dapat ditindaklanjuti.
3. Simulate kanal dengan konfigurasi dan seed eksplisit; simpan hasil terpisah dari sumber.
4. Recover arsip bersih atau rusak sampai batas toleransi, kemudian verifikasi ukuran dan hash.
5. Benchmark konfigurasi berulang, simpan setiap hasil dan metadata agar dapat direproduksi.

Kegagalan pada satu tahap MUST menyebut tahap dan alasan, tidak menghasilkan output sukses parsial yang tampak valid. Output target ditulis secara atomik atau melalui berkas sementara lalu dipindahkan setelah sukses.

## 4. Arsitektur logis

Komponen dan tanggung jawab:

| Komponen | Tanggung jawab dan batas |
| --- | --- |
| Codec | Pemetaan byte/basa. Tidak mengklaim constraint biologis. |
| Constraint validator | Mengukur GC, homopolymer, dan panjang; tidak diam-diam mengubah urutan. |
| Fragmenter | Membuat data shard ber-offset, mempertahankan panjang asli, dan mengisi stripe akhir secara deterministik. |
| ECC | Mengodekan tepat `k` shard data menjadi `r` parity dan memulihkan erasure yang lokasinya diketahui. |
| Format/storage | Validasi manifest/blok, versi, ukuran, nama, ID, serta penulisan/pembacaan arsip. |
| Channel | Memodifikasi observasi sesuai model probabilistik; tidak mengklaim model laboratorium. |
| Recovery | Parsing → autentikasi → klasifikasi erasure → ECC → reassembly → decode → verifikasi. |
| Metrics/report | Menghasilkan metrik terukur dan laporan yang membawa konfigurasi eksperimen. |
| CLI | Memvalidasi argumen dan memanggil API; tidak memiliki algoritma domain sendiri. |

Modul Rust saat ini memetakan komponen tersebut ke `codec`, `constraints`, `fragment`, `ecc`, `storage`, `channel`, `recovery`, `pipeline`, `metrics`, dan `report`.

## 5. Persyaratan fungsional

### 5.1 Encode/decode

- **FR-001** Encoder MUST menerima byte arbitrer termasuk berkas kosong dan menghasilkan payload yang dapat dibalik tanpa kehilangan data.
- **FR-002** Baseline codec MUST menggunakan pemetaan `00=A`, `01=C`, `10=G`, `11=T`, urutan bit paling signifikan lebih dahulu; satu byte menghasilkan empat basa.
- **FR-003** Decoder MUST menolak panjang basa yang bukan kelipatan empat dan simbol di luar alfabet.
- **FR-004** Encode/decode bersih MUST mempertahankan byte secara identik untuk semua ukuran input yang didukung.
- **FR-005** Validator constraint MUST melaporkan GC fraction, panjang homopolymer maksimum, serta pelanggaran panjang opsional. Nilai GC untuk urutan kosong ditetapkan `0`.
- **FR-006** Mode strict MUST gagal sebelum menulis arsip bila constraint konfigurasi dilanggar. Mode non-strict MUST melaporkan pelanggaran dan tidak boleh menyiratkan urutan sudah diperbaiki.
- **FR-007** Konfigurasi invalid (ukuran fragmen nol, rentang GC tidak finite/di luar `[0,1]`, `gc_min > gc_max`, atau parameter ECC invalid) MUST ditolak sebelum alokasi besar atau output.

### 5.2 Fragmentasi dan redundancy

- **FR-010** Data input MUST dibagi menjadi fragmen payload maksimum sesuai konfigurasi, dengan offset byte/basa yang tidak tumpang tindih dan berurutan.
- **FR-011** Implementasi penuh MUST mendukung jumlah fragmen data lebih besar dari `k` melalui stripe independen, bukan menganggap seluruh berkas sebagai satu codeword.
- **FR-012** Setiap stripe MUST berisi `k` posisi data dan `r` parity; stripe terakhir boleh menggunakan padding yang ditentukan format. Panjang payload asli dan ukuran sumber MUST tersimpan agar padding tidak masuk hasil decode.
- **FR-013** Konfigurasi RS MUST memenuhi `k > 0`, `r >= 0`, dan `k+r <= 255` untuk GF(256). `r=0` berarti tanpa koreksi.
- **FR-014** Parity MUST dihitung atas representasi blok/shard kanonik yang dinyatakan versi format; parity tidak boleh dihitung dari serialisasi yang mengandung checksum yang berubah secara sirkular.
- **FR-015** `fragment_id`, stripe, indeks shard, tipe shard, offset, panjang payload, dan integritas MUST dapat ditentukan tanpa bergantung pada urutan iterasi direktori.

### 5.3 Integritas, arsip, dan pemulihan

- **FR-020** Setiap fragmen MUST memiliki SHA-256 payload; manifest MUST memiliki ukuran asli dan SHA-256 berkas sumber.
- **FR-021** Parsing MUST menolak magic/versi/ukuran/indeks/flag/length/hash yang tidak valid, serta nilai basa data di luar 0..3.
- **FR-022** Hash dan validasi struktur hanya mendeteksi korupsi. Hanya ECC yang boleh mengklaim koreksi erasure; substitusi tak diketahui tidak boleh disebut terkoreksi.
- **FR-023** Decoder arsip MUST membedakan fragmen hilang, korup, duplikat, malformed, dan tidak dikenal. File non-fragmen yang tidak terkait MUST diabaikan atau dilaporkan menurut kebijakan eksplisit, bukan dianggap shard.
- **FR-024** Recovery MUST menghitung erasure per stripe. Stripe dengan lebih dari `r` erasure MUST gagal secara eksplisit; stripe lain yang valid dapat diproses hanya jika kebijakan partial recovery diminta eksplisit.
- **FR-025** Setelah ECC, setiap shard hasil MUST divalidasi ulang. Reassembly MUST memeriksa kontinuitas offset, ukuran asli, dan batas akhir stream.
- **FR-026** Jika hash sumber tersedia, ketidakcocokan MUST menjadi kegagalan recovery; byte tidak boleh dilaporkan sebagai exact recovery.
- **FR-027** `validate` MUST bersifat read-only, memakai constraint yang disimpan/ditentukan secara eksplisit, dan menghasilkan laporan konsisten untuk CLI serta API.
- **FR-028** Penulisan arsip yang sudah ada MUST memiliki kebijakan overwrite yang eksplisit. Perintah tidak boleh mencampur fragmen lama dan baru.

### 5.4 Kanal simulasi

- **FR-030** Kanal MUST mendukung substitution, insertion, deletion, dropout, dan duplication dengan parameter probabilitas finite pada `[0,1]`.
- **FR-031** Simulasi MUST deterministik untuk versi implementasi, konfigurasi, seed, dan input yang sama. Versi PRNG/algoritma MUST direkam agar perubahan algoritma tidak menyamar sebagai reproduksi identik lintas versi.
- **FR-032** Tingkat substitution/insertion/deletion berlaku per basa; dropout/duplication berlaku per fragmen/observasi. Semantik insertion, urutan pemrosesan, dan interaksi dengan deletion MUST terdokumentasi.
- **FR-033** Output simulasi MUST mempertahankan identitas fragmen dan bisa menyatakan observasi hilang, panjang berubah, atau duplikat sebagai observasi baru. Duplikasi tidak boleh menggeser indeks fragmen asli atau menyebabkan panic.
- **FR-034** Laporan MUST mencatat parameter yang diminta, seed, jumlah aktual tiap event, basis sebelum/sesudah, fragmen sebelum/sesudah, dan versi model.

### 5.5 CLI dan laporan

- **FR-040** Sintaks subperintah MUST konsisten: setiap input/output wajib memiliki satu representasi positional atau named yang jelas; tidak boleh mewajibkan argumen output yang diabaikan.
- **FR-041** `encode`, `decode`, `validate`, `simulate`, `recover`, `benchmark`, dan `inspect` MUST memiliki help yang mencerminkan perilaku aktual dan validasi opsi yang sama dengan API.
- **FR-042** Kode keluar target: `0` sukses; `1` kegagalan operasional/konfigurasi; `2` arsip tidak valid atau constraint gagal pada validate; `3` recovery tidak exact jika command secara eksplisit meminta exact recovery. Arti dan cakupan kode harus konsisten.
- **FR-043** CSV MUST quote/escape field sesuai RFC 4180. JSON MUST memiliki schema/version dan menyimpan semua parameter yang diperlukan untuk mereproduksi run.
- **FR-044** Benchmark MUST memakai input yang dijelaskan oleh command. Bila input sintetis dipakai, pola generator dan seed-nya MUST dinyatakan; bila path input diberikan, isi file MUST benar-benar menjadi data benchmark.

## 6. Kontrak algoritma

### 6.1 Codec

Representasi internal `Base(0..=3)` adalah A/C/G/T. Encode memproses tiap byte pada shift `6,4,2,0`. Decode hanya menerima nilai base valid dan jumlah basa kelipatan empat. Codec string menerima A/C/G/T case-insensitive; karakter lain gagal dengan posisi absolut dalam input.

### 6.2 Batasan urutan

GC fraction = `(jumlah C + jumlah G) / jumlah basa`. Rentang batas inklusif. Homopolymer adalah run kontigu terpanjang dari simbol sama. Constraint tidak mengubah input. Pada baseline, validasi dilakukan atas stream utuh saat encode (mode strict menolak) dan perilaku validate arsip harus diselaraskan dengan konfigurasi manifest.

### 6.3 Reed-Solomon erasure

Baseline memakai GF(256), polinomial primitif `0x11d`, generator `alpha=2`, dan evaluasi polinomial/Lagrange. Untuk satu stripe `[n,k]`, `n=k+r`; setiap posisi byte merupakan simbol GF(256) dan `k` shard valid cukup untuk merekonstruksi codeword. Koreksi error pada lokasi tidak diketahui, error-scrubbing tanpa hash, dan koreksi basa langsung tidak termasuk kontrak.

Semua shard dalam stripe MUST sama panjang sebelum ECC. Padding, urutan shard, byte yang dilindungi, dan domain checksum MUST kanonik dan identik encoder/decoder.

## 7. Format arsip dan kompatibilitas

### 7.1 Prinsip versi

Format arsip v1 pada kode adalah eksperimental. Perubahan yang memperbaiki dukungan stripe, offset, identitas shard, atau batas keamanan MUST menggunakan versi format baru; parser MUST menolak versi yang tidak didukung dengan error eksplisit. Tidak boleh mengubah makna v1 secara diam-diam.

### 7.2 Arsip v1 yang ada

Layout direktori saat ini: `manifest.json`, `frag_{id:06}.bin`, dan nama duplikat `frag_{id:06}_dupN.bin`. Header blok v1 adalah 62 byte, integer little-endian:

| Offset byte | Lebar | Field | Semantik |
| --- | ---: | --- | --- |
| 0 | 4 | magic | ASCII `DNAF` |
| 4 | 2 | version | `u16`, saat ini `1` |
| 6 | 8 | fragment ID | `u64` |
| 14 | 4 | total data | `u32` |
| 18 | 4 | payload length | jumlah unit payload tidak dipadding |
| 22 | 4 | offset basa | `u32` di wire format |
| 26 | 32 | payload SHA-256 | hash payload tidak dipadding |
| 58 | 4 | flags | bit 0 menandakan parity |
| 62 | variabel tetap | payload | ukuran tetap dari manifest, sisanya zero-padding |

Manifest v1 memakai `format="dnaforge-archive"` dan menyimpan versi, ukuran/sha sumber, jumlah basa, ukuran fragmen/blok, `ecc_k`, `ecc_r`, hitungan fragmen, konfigurasi GC/homopolymer, serta waktu pembuatan opsional. Manifest harus dianggap tidak tepercaya dan semua ukuran harus divalidasi sebelum alokasi.

Keterbatasan v1: offset `u64` di API dipotong ke `u32` saat serialisasi; blok tidak menyimpan `data_index` atau stripe; parity menggunakan simbol byte yang tidak terbatas 0..3; batas data shard dan `ecc_k` tidak konsisten untuk input umum. Karena itu v1 bukan format stabil untuk penyimpanan besar atau klaim DNA siap sintesis.

### 7.3 Target format stabil

Format stabil berikutnya MUST memiliki versi baru dan mencatat sekurangnya: versi/schema manifest, ukuran sumber, hash sumber, jumlah total basa, ukuran shard, `k/r`, jumlah stripe, aturan padding, identitas shard/stripe, indeks shard, tipe data/parity, offset stream lebar minimal 64-bit, payload length, hash payload, serta parameter codec/ECC. Format MUST mendukung round-trip untuk multi-stripe dan deteksi berkas hilang/duplikat. Detail byte layout versi baru harus ditetapkan dan diuji sebelum diimplementasikan; v1 tidak boleh dipakai ulang untuk menampung field baru.

Arsip format tidak boleh menyimpan data rahasia secara implisit. SHA-256 menyediakan deteksi perubahan tanpa kunci, bukan autentikasi terhadap penyerang yang dapat mengganti payload dan manifest sekaligus.

## 8. Kontrak CLI target

Sintaks yang dianjurkan:

```text
dnaforge encode INPUT --output ARCHIVE [--fragment-size BASES] [--ecc-k K] [--ecc-r R] [--strict] [constraint options]
dnaforge decode ARCHIVE --output FILE
dnaforge validate ARCHIVE [--report FILE]
dnaforge simulate ARCHIVE --output ARCHIVE [channel options] [--seed N]
dnaforge recover ARCHIVE --output FILE [--original FILE]
dnaforge benchmark INPUT --report FILE [size/repetition/ECC/channel options]
dnaforge inspect ARCHIVE [--json]
```

Opsi numerik MUST menolak parse ambigu, overflow, nilai negatif, NaN/infinity, dan rentang probabilitas invalid. Opsi ukuran benchmark yang diterima harus disebut help (mis. byte, KiB/MiB/GiB atau KB/MB/GB); konversi desimal dan biner tidak boleh tercampur tanpa suffix.

## 9. Metrik dan benchmark

- Waktu tahap menggunakan wall-clock monotonic, resolusi dan unit dinyatakan; nilai nol karena pembulatan tidak boleh diartikan sebagai kerja tanpa biaya.
- Throughput MiB/s = bytes diproses / detik / `1,048,576`; jika waktu terukur nol, laporan harus menggunakan presisi lebih tinggi atau menandai nilai tidak tersedia, bukan menyimpulkan throughput nol.
- Overhead file = `(ukuran arsip aktual - ukuran sumber) / ukuran sumber * 100%`; untuk input kosong, hasil harus `null`/tidak tersedia, bukan membagi nol.
- Bases per byte = total basa data / byte sumber; untuk input kosong tidak tersedia.
- Peak RSS adalah metrik proses/platform, bukan konsumsi tahap tertentu. Linux boleh membaca `VmHWM`; platform yang tidak mendukung melaporkan unavailable.
- Recovery rate harus mendefinisikan denominator (data shard yang seharusnya dipulihkan), tidak mencampur fragmen parity dalam denominator kecuali label menyatakannya.
- Run benchmark wajib menyimpan konfigurasi lengkap, seed per run, jumlah pengulangan, versi aplikasi/format/model, ukuran input, hasil exact recovery, dan lingkungan minimum (OS, arsitektur, profil build).
- Ringkasan statistik SHOULD melaporkan median dan sebaran (min/max atau IQR); setiap sampel mentah tetap disimpan. Tidak ada target throughput universal tanpa benchmark perangkat keras yang dinyatakan.

## 10. Ketahanan, keamanan, dan batasan

- Parser MUST memeriksa ukuran dan rentang sebelum alokasi, menolak overflow, path traversal, symlink yang tidak diharapkan, dan manifest tidak konsisten.
- Penulisan output MUST tidak merusak input; target path yang sama harus ditolak atau memerlukan opsi eksplisit.
- Integritas SHA-256 bukan enkripsi, autentikasi, atau tanda tangan digital.
- Error rate independen baseline adalah model sintetis. Klaim perbandingan biologis membutuhkan data empiris, model tervalidasi, dokumentasi alat/teknologi, dan protokol yang sesuai.
- Data sumber dapat bersifat sensitif. Tool tidak boleh mengunggah data atau mengirim telemetri; laporan hanya memuat path jika dibutuhkan dan harus menghindari konten file.
- API/library tidak boleh panic pada input dari arsip. Kegagalan input harus menjadi error bertipe.

## 11. Persyaratan non-fungsional

- **NFR-001 Reproduksibilitas:** untuk versi algoritma yang sama, input, konfigurasi, dan seed yang sama MUST menghasilkan output simulasi yang sama. Perubahan perilaku algoritma harus mengubah versi model yang dilaporkan.
- **NFR-002 Batas sumber daya:** parsing manifest/blok MUST memvalidasi dimensi, overflow, dan batas ukuran sebelum alokasi. Implementasi SHOULD menyediakan pemrosesan streaming untuk input besar; sampai tersedia, batas input maksimum harus terdokumentasi dan kegagalan memori bukan dianggap dukungan.
- **NFR-003 Portabilitas:** library dan CLI MUST berjalan pada Rust stable dan tidak bergantung pada layanan jaringan. Metrik OS-specific seperti RSS boleh unavailable di platform yang tidak mendukung.
- **NFR-004 Kualitas pengukuran:** laporan MUST membedakan nilai terukur, nilai tidak tersedia, dan agregasi statistik; waktu resolusi terlalu rendah tidak boleh disajikan sebagai angka presisi palsu.
- **NFR-005 Isolasi dan privasi:** operasi MUST lokal secara default, tanpa telemetri atau unggah data. Path dan konten sumber tidak dimasukkan ke laporan kecuali diminta eksplisit.
- **NFR-006 Pemeliharaan:** format, algoritma kanal, dan schema laporan MUST berversi secara terpisah bila kompatibilitasnya berubah; error eksternal harus bertipe dan tidak memicu panic.

## 12. Persyaratan pengujian dan penerimaan

- **TST-001** Codec round-trip seluruh nilai byte dan input kosong; menolak base invalid dan panjang tidak lengkap.
- **TST-002** Constraint menguji batas inklusif, stream kosong, GC, panjang run, dan panjang min/max.
- **TST-003** Format menguji round-trip, setiap header field, hash salah, base invalid, panjang/versi/magic salah, truncation, nilai maksimum, serta manifest malformed.
- **TST-004** ECC menguji seluruh pola erasure hingga `r` pada beberapa `k/r`, tanpa erasure, data dan parity hilang, serta menolak `r+1` erasure dan ukuran salah.
- **TST-005** Pipeline menguji berkas kosong, 1 byte, batas fragmen, beberapa stripe, stripe akhir terpadding, output besar, dan exact byte/hash recovery.
- **TST-006** Channel menguji determinisme, validasi rate, event count, seed berbeda, dropout/duplication, dan interaksi insertion/deletion.
- **TST-007** Recovery menguji kehilangan/korupsi per stripe, hash akhir mismatch, duplikat, ID salah, manifest mismatch, serta output tidak parsial saat gagal.
- **TST-008** CLI integration menguji help, semua command, exit code, path, laporan CSV/JSON yang dapat diparse, dan argumen invalid.
- **TST-009** Properti generatif MUST memeriksa `decode(encode(bytes)) == bytes` dan pemulihan ECC; test harus memakai seed dan kasus minimal yang dapat direproduksi.
- **TST-010** CI MUST menjalankan `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (setelah warning baseline ditangani), dan `cargo test` pada platform target.

Rilis stabil tidak boleh dilakukan sebelum semua TST-001 sampai TST-010 lulus, format stabil didokumentasikan, dan known gaps kritis ditutup atau secara eksplisit dikeluarkan dari ruang lingkup rilis.

## 13. Gap baseline yang diketahui

Temuan berikut berasal dari inspeksi kode dan eksekusi `cargo test` pada baseline `0.1.0`; semuanya harus ditinjau ulang saat implementasi berubah.

| ID | Gap saat ini | Dampak | Kriteria penutupan |
| --- | --- | --- | --- |
| GAP-001 | `cargo test` gagal kompilasi: test `storage` tidak mengimpor `Base`, test pipeline tidak menemukan `sha256_hex`, dan helper ECC tidak unwrap hasil `ReedSolomon::new`. | Test suite tidak dapat menjadi gerbang kebenaran. | Perbaiki test dan jalankan seluruh suite hingga lulus; jangan menyamarkan dengan menonaktifkan test. |
| GAP-002 | Encoder membuat jumlah data fragmen berdasarkan ukuran input, sedangkan ECC mensyaratkan tepat `ecc_k` blok; identitas parity juga mulai pada ID `k`. | Input umum multi-fragmen gagal/berpotensi bentrok ID; toleransi recovery belum berlaku untuk file umum. | Implementasikan stripe `k+r` end-to-end dan tes multi-stripe. |
| GAP-003 | `simulate_pipeline` memproses fragmen data awal saja, tidak mempertahankan dropout/duplication sebagai observasi yang benar, serta dapat mengindeks di luar batas saat ada duplikasi. | Simulasi arsip tidak merepresentasikan model kanal secara benar. | Implementasikan observasi fragmen ber-ID stabil dan tes seluruh kombinasi event. |
| GAP-004 | Arsip dense dibentuk dari ID maksimum yang ditemukan; fragmen terminal yang hilang tidak selalu menjadi slot `None`. | Recovery dapat menerima jumlah blok salah atau salah mengklasifikasikan erasure. | Bentuk posisi dari manifest/stripe yang divalidasi, bukan ukuran map yang tersisa. |
| GAP-005 | Header v1 menyimpan offset `u32` meski tipe API `u64`; parser tidak memeriksa semua nilai Base atau konsistensi identitas header/file. | Batas ukuran dan integritas metadata tidak kuat. | Versi format baru, parser ketat, uji kompatibilitas dan batas. |
| GAP-006 | CLI meminta `INPUT OUTPUT` umum sekaligus `--output` di `simulate`; beberapa command tidak memakai output positional. | UX/help dan contoh command tidak konsisten. | Satu kontrak argumen per command dan integration test. |
| GAP-007 | `benchmark` membaca path input namun membuat data deterministik sintetis; seed dalam daftar tidak mengatur seed kanal per-run. | Label eksperimen dapat menyiratkan benchmark data yang tidak dipakai dan run tidak tereplikasi sesuai opsi. | Tegaskan mode file/sintetis; catat seed aktual per dimensi. |
| GAP-008 | Ekspor CSV menulis field teks tanpa escaping; JSON tidak menyertakan keseluruhan konfigurasi/versi eksperimen. | Laporan dapat rusak atau tidak dapat direproduksi. | Serializer CSV standar dan schema JSON berversi dengan metadata lengkap. |
| GAP-009 | Pengukuran ms memakai `as_millis`, sehingga run singkat bernilai nol; recovery rate mencampur hitungan recovery dengan total fragmen termasuk parity. | Metrik dapat menyesatkan. | Presisi dan denominator eksplisit, coverage test formula. |
| GAP-010 | `validate_pipeline` memakai constraint default dan belum menerapkan konsistensi manifest secara menyeluruh. | Validasi tidak setara dengan konfigurasi encode. | Validasi memakai konfigurasi tersimpan/override eksplisit dan output konsisten. |
| GAP-011 | Tidak ada workflow CI; `benches/` dan direktori `tests/` kosong, walau unit test tertanam dalam modul. | Gate lint/test belum berjalan otomatis; benchmark belum berupa suite Criterion. | CI dan benchmark terukur ditambahkan sesuai rencana implementasi. |

Status: temuan GAP-001 terkonfirmasi oleh `cargo test` gagal saat kompilasi test. `cargo run -- --help` berhasil tetapi menghasilkan warning compiler. Temuan kode lainnya belum seluruhnya dibuktikan lewat tes runtime dan harus diperlakukan sebagai risiko yang teridentifikasi dari alur implementasi.

## 14. Definition of Done

Satu perubahan dinyatakan selesai bila:

1. Perilaku memenuhi ID persyaratan terkait dan tidak mengubah format/CLI tanpa pembaruan dokumen.
2. Test unit/integrasi mencakup perilaku normal, batas, dan kegagalan.
3. Error pengguna bertipe; tidak ada panic untuk input eksternal.
4. `cargo fmt --check`, Clippy, serta test yang relevan lulus.
5. Metrik/laporan tidak mengklaim hasil yang tidak diukur.
6. README, spesifikasi, contoh CLI, dan schema laporan tetap konsisten.

## 15. Rujukan modul

| Area | Implementasi |
| --- | --- |
| Codec | `src/codec.rs` |
| Constraint | `src/constraints.rs` |
| Fragmen dan blok v1 | `src/fragment.rs` |
| ECC | `src/ecc.rs` |
| Kanal | `src/channel.rs` |
| Storage dan manifest | `src/storage.rs` |
| Recovery | `src/recovery.rs` |
| Orkestrasi dan benchmark | `src/pipeline.rs` |
| Metrik dan laporan | `src/metrics.rs`, `src/report.rs` |
| CLI | `src/main.rs` |