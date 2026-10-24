# Rencana Implementasi DNAForge

Rencana ini mengurutkan pekerjaan untuk memenuhi [spesifikasi baseline](baseline-specification.md). Urutan mengutamakan kemampuan membuktikan kebenaran sebelum optimasi. Semua fase berakhir dengan gate yang dapat diuji; status fitur saat ini tidak diasumsikan selesai.

## Fase 0: Pulihkan gerbang build

**Cakupan:** GAP-001, GAP-011; test compilation, format, warning baseline, CI.

**Pekerjaan:** betulkan import/helper test; jalankan `cargo test`; tambahkan workflow CI untuk format, Clippy, dan test; tentukan apakah warning ditangani atau lint terpilih dikecualikan dengan alasan.

**Selesai bila:** test dapat dikompilasi dan berjalan; CI memberikan hasil reproduktif untuk pull request; tidak ada test dimatikan hanya untuk membuat gate hijau.

## Fase 1: Tetapkan kontrak arsip stabil

**Cakupan:** FR-015, FR-020..028, TST-003, TST-007; GAP-004, GAP-005, GAP-010.

**Pekerjaan:** finalisasi schema manifest dan wire format versi baru; tambah stripe/index/offset 64-bit/ukuran asli; validasi ukuran sebelum alokasi; tentukan overwrite atomicity dan kompatibilitas; pertahankan parser v1 read-only bila memang diperlukan.

**Selesai bila:** golden fixtures format; reader menolak input malformed; round-trip dan kompatibilitas versi teruji; format v1 tidak berubah diam-diam.

## Fase 2: Multi-stripe ECC dan pemulihan

**Cakupan:** FR-010..015, FR-022..026, TST-004, TST-005, TST-007; GAP-002.

**Pekerjaan:** bentuk shard per stripe, encoding parity, padding yang tidak masuk output, deteksi erasure per stripe, validasi ulang hasil, reassembly berurutan, exact hash.

**Selesai bila:** berkas kosong, batas shard, beberapa stripe, stripe terakhir parsial, kombinasi erasure sampai `r`, serta `r+1` erasure menghasilkan hasil/error yang ditetapkan.

## Fase 3: Simulasi kanal yang menjaga identitas

**Cakupan:** FR-030..034, TST-006; GAP-003.

**Pekerjaan:** representasikan observasi dengan ID asal dan ID salinan; jaga hilang/indel; tentukan model probabilitas dan urutan event; simpan seed, model-version, statistik dan events; pastikan simulasi tidak memutasi sumber.

**Selesai bila:** semua event dapat diuji deterministik; duplication tidak mengubah identitas shard; dropout menjadi erasure; insertion/deletion terdeteksi sebagai perubahan payload sesuai format.

## Fase 4: CLI dan pelaporan yang dapat dipercaya

**Cakupan:** FR-040..044, TST-008; GAP-006..009.

**Pekerjaan:** konsistenkan opsi input/output; tambahkan validasi parse; bedakan benchmark file aktual dari generator sintetis; gunakan seed yang tercatat untuk tiap run; serialisasi CSV/JSON dengan benar; tambahkan metadata lingkungan dan definisi metrik.

**Selesai bila:** integration test untuk seluruh command/exit code lulus; semua laporan dapat diparse ulang; eksperimen dapat diulang dari metadatanya.

## Fase 5: Constraint dan validasi domain

**Cakupan:** FR-005..007, FR-027, TST-002.

**Pekerjaan:** validasi nilai konfigurasi; gunakan konfigurasi manifest/override yang terdokumentasi; bedakan report dan strict rejection; tidak mengklaim sequence repair kecuali algoritma dan bukti ditambahkan sebagai fitur tersendiri.

**Selesai bila:** boundary tests lulus; encode dan validate memberikan temuan konsisten; tidak ada perubahan urutan diam-diam.

## Fase 6: Baseline performa dan eksperimen

**Cakupan:** bagian metrik dan NFR; GAP-011.

**Pekerjaan:** buat benchmark Criterion untuk komponen yang tepat; buat benchmark CLI untuk alur end-to-end; gunakan ukuran dan repetisi yang realistis; simpan raw sample, median/sebaran, profil build, arsitektur, OS, dan seed.

**Selesai bila:** hasil dapat direproduksi dan tidak mengandung target proyeksi; optimasi hanya dilakukan setelah baseline kebenaran tersimpan.

## Fase 7: Validasi biologis (opsional, terpisah)

**Cakupan:** ruang lingkup perluasan; bukan prasyarat simulator software.

**Pekerjaan:** tentukan encoding yang constraint-aware, bandingkan model error dengan dataset sequencing publik, nyatakan teknologi dan asumsi, dan buat jalur validasi laboratorium hanya melalui kolaborasi yang sesuai.

**Selesai bila:** data dan metode terversi; hasil sintetis dan empiris dipisahkan; tidak ada klaim eksperimental yang tidak didukung bukti.

## Urutan rilis

| Rilis | Isi | Gate wajib |
| --- | --- | --- |
| `0.1.x` | Perbaikan build/test dan perilaku terdokumentasi; format tetap eksperimental. | Fase 0 dan test relevan. |
| `0.2.0` | Format arsip versi baru dan multi-stripe. | Fase 1-2; migration/compatibility note. |
| `0.3.0` | Simulasi, CLI, laporan yang reproduktif. | Fase 3-4. |
| `1.0.0` | Baseline implementasi stabil yang memenuhi seluruh requirement in-scope. | Semua TST dan DoD spesifikasi lulus; dokumentasi konsisten. |

Perubahan format bersifat breaking dan harus menaikkan versi format; nomor rilis di atas adalah usulan urutan, bukan komitmen kalender. Fitur biologis tetap opsional dan tidak mengubah klaim produk tanpa validasi tersendiri.