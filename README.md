### DNAForge — Spesifikasi dan Deskripsi Lengkap Proyek

DNAForge adalah proyek perangkat lunak untuk membangun sistem penyimpanan data digital berbasis DNA sintetis melalui simulasi komputasi, algoritma encoding/decoding, simulasi kesalahan, dan pemulihan data. Tujuan utamanya adalah meneliti bagaimana data digital dapat direpresentasikan sebagai urutan basa DNA, bagaimana data tersebut bertahan terhadap kesalahan yang terjadi selama proses penyimpanan dan pembacaan, serta bagaimana proses pemulihan dapat dibuat cepat, hemat memori, dan terukur.

DNAForge dirancang untuk dikembangkan terlebih dahulu tanpa perangkat sintesis DNA atau sequencer. Seluruh tahap awal dapat dilakukan di komputer menggunakan Rust, data sintetis, pengujian otomatis, dan benchmark. Hasilnya dapat menjadi fondasi penelitian sebelum sistem diintegrasikan dengan eksperimen laboratorium.

1\. Encoding dan decoding

Mengubah byte digital menjadi urutan basa DNA A, C, G, T dan mengembalikannya menjadi data semula.

2\. Simulasi kanal DNA

Mensimulasikan substitusi, penyisipan, penghapusan, kehilangan fragmen, dan kesalahan lain yang dapat terjadi pada data DNA.

3\. Pemulihan data

Mengembangkan checksum, redundansi, kode koreksi kesalahan, dan rekonstruksi fragmen untuk memulihkan data yang rusak.

4\. Benchmark dan optimasi

Mengukur throughput, latensi, penggunaan memori, ukuran overhead, tingkat pemulihan, serta biaya komputasi.

### 1\. Latar belakang dan masalah yang ingin diselesaikan

Penyimpanan DNA memanfaatkan urutan empat basa—adenin (A), sitosin (C), guanin (G), dan timin (T)—untuk merepresentasikan informasi digital. Secara teori, setiap basa dapat mewakili dua bit sehingga kepadatan informasi potensialnya sangat tinggi.

Namun, sistem penyimpanan DNA menghadapi beberapa tantangan:

* Efisiensi encoding: representasi digital harus diubah menjadi urutan DNA yang memenuhi batasan biologis.

* Kesalahan pembacaan: substitusi, insertion, dan deletion dapat mengubah urutan DNA yang terbaca.

* Kehilangan fragmen: sebagian urutan dapat hilang atau tidak berhasil dibaca.

* Overhead redundansi: koreksi kesalahan membutuhkan data tambahan, sehingga kapasitas efektif menurun.

* Biaya komputasi: encoding, decoding, penyelarasan urutan, dan rekonstruksi data dapat menjadi mahal untuk dataset besar.

* Skalabilitas: sistem harus mampu menangani data dari kilobyte sampai gigabyte tanpa harus memuat seluruh data ke memori sekaligus.

DNAForge berfokus pada persoalan perangkat lunak tersebut. Proyek ini bukan alat untuk membuat DNA secara fisik, melainkan platform untuk menguji algoritma dan mengukur trade-off sebelum eksperimen biologis dilakukan.

### 2\. Tujuan utama

* Membuat implementasi referensi DNA storage berbasis Rust.

* Menyediakan pipeline encoding hingga pemulihan data yang dapat direproduksi.

* Menguji ketahanan algoritma terhadap berbagai model kesalahan DNA.

* Mengukur kecepatan, penggunaan memori, overhead, dan tingkat keberhasilan pemulihan.

* Mengembangkan optimasi melalui streaming, multithreading, dan SIMD jika terbukti bermanfaat.

* Menyediakan antarmuka CLI, format data eksperimen, pengujian otomatis, dan laporan benchmark.

* Menyiapkan arsitektur yang dapat diperluas untuk memakai data sequencing publik dan data eksperimen laboratorium.

Prinsip penting: DNAForge harus membedakan hasil yang benar-benar diukur dari hasil simulasi atau proyeksi. Benchmark harus bisa direproduksi dan tidak boleh mengklaim ketahanan biologis hanya berdasarkan simulasi sederhana.

### 3\. Arsitektur sistem

DNAForge dibagi menjadi modul yang memiliki tanggung jawab terpisah agar algoritma dapat diganti, diuji, dan dioptimalkan tanpa harus menulis ulang keseluruhan sistem.

### Digital Input

File, arsip, teks, gambar, atau data biner

### Encoder

Bytes → DNA sequence → fragment berindeks

### DNA Channel Simulator

Substitution · Insertion · Deletion · Dropout

### Decoder & Recovery Engine

Validasi · koreksi kesalahan · rekonstruksi fragmen

### Recovered Data & Evaluation

Verifikasi hash, throughput, memori, dan recovery rate

Arsitektur ini memiliki dua jalur evaluasi:

* Jalur performa: mengukur biaya komputasi encoding, decoding, fragmentasi, dan pemulihan.

* Jalur ketahanan: mengukur kemampuan sistem memulihkan data setelah diberi kesalahan terkontrol.

Keduanya harus dilaporkan secara terpisah. Encoder yang sangat cepat belum tentu menghasilkan data yang mudah dipulihkan.

### 4\. Modul inti

Struktur yang disarankan untuk pengembangan bertahap.

| Modul          | Tanggung jawab                                         |
| -------------- | ------------------------------------------------------ |
| codec.rs       | Konversi byte ke basa DNA dan sebaliknya               |
| channel.rs     | Model kesalahan DNA yang dapat dikonfigurasi           |
| fragment.rs    | Pembagian data menjadi fragmen dengan ID dan metadata  |
| recovery.rs    | Validasi, koreksi, dan rekonstruksi data               |
| metrics.rs     | Perhitungan metrik performa dan ketahanan              |
| format.rs      | Format serialisasi untuk fragmen dan metadata          |
| constraints.rs | Validasi GC content, homopolymer, dan batasan urutan   |
| ecc.rs         | Antarmuka algoritma error-correcting code              |
| pipeline.rs    | Orkestrasi seluruh proses encoding sampai recovery     |
| storage.rs     | Pembacaan dan penulisan data dengan dukungan streaming |
| report.rs      | Ekspor hasil eksperimen ke CSV dan laporan ringkasan   |
| main.rs        | Antarmuka command-line                                 |
| benches/       | Benchmark komponen dan pipeline                        |

### 4.1. Codec: konversi digital ke DNA

Versi baseline menggunakan pemetaan dua bit menjadi satu basa:

| Bit | Basa |
| --- | ---- |
| 00  | A    |
| 01  | C    |
| 10  | G    |
| 11  | T    |

Contohnya, satu byte membutuhkan empat karakter DNA. Konversi tersebut mudah diuji, tetapi hanya berfungsi sebagai baseline komputasi.

Pemetaan ini belum cukup untuk sintesis biologis. Dalam pengembangan selanjutnya, encoder perlu mempertimbangkan GC content, homopolymer, distribusi basa, batasan panjang urutan, dan kompatibilitas dengan skema koreksi kesalahan.

### 4.2. Channel simulator: simulasi kerusakan

Simulator harus mendukung tingkat kesalahan yang dapat dikontrol dan menggunakan seed yang dapat diulang.

* Substitution: basa berubah, misalnya A menjadi G.

* Insertion: basa tambahan muncul.

* Deletion: basa asli hilang.

* Dropout: satu fragmen tidak tersedia pada proses pembacaan.

* Duplicate: fragmen terbaca lebih dari sekali.

* Mixed errors: beberapa jenis kesalahan terjadi bersamaan.

Setiap eksperimen perlu mencatat jenis kesalahan, tingkat kesalahan yang diminta, jumlah perubahan aktual, seed, serta panjang urutan sebelum dan sesudah simulasi.

Model kesalahan independen adalah titik awal yang berguna, tetapi bukan representasi sempurna dari laboratorium. Kesalahan nyata dapat bergantung pada konteks urutan, teknologi sequencing, dan proses kimia.

### 4.3. Fragment: pembagian dan pengindeksan

File besar tidak seharusnya diubah menjadi satu urutan DNA tanpa struktur. DNAForge perlu membagi data menjadi fragmen yang memiliki:

* ID fragmen dan posisi logis.

* Panjang payload dan versi format.

* Checksum atau hash untuk mendeteksi kerusakan.

* Metadata untuk menyusun kembali urutan data.

* Redundansi atau informasi pemulihan jika diperlukan.

Ukuran fragmen merupakan parameter eksperimen. Fragmen kecil mempermudah pengelolaan kehilangan lokal, tetapi menambah overhead indeks dan metadata. Fragmen besar mengurangi overhead tersebut, tetapi dapat memperbesar biaya pemulihan ketika sebagian data rusak.

### 4.4. Recovery engine: pemulihan data

Inilah salah satu bagian terpenting DNAForge. Engine harus membedakan tiga kondisi:

* Deteksi: sistem mengetahui data tidak valid.

* Koreksi: sistem dapat menentukan data yang benar menggunakan redundansi yang tersedia.

* Rekonstruksi: sistem dapat menyusun kembali file dari fragmen yang valid dan informasi pemulihan.

Checksum biasa hanya mendeteksi banyak jenis kerusakan; checksum tidak dapat dengan sendirinya memperbaiki data yang rusak. Karena itu, DNAForge perlu membandingkan metode seperti Reed–Solomon, LDPC, fountain codes, atau metode lain sesuai model kesalahan dan struktur data.

Untuk insertion dan deletion, koreksi lebih sulit karena posisi basa setelah kesalahan dapat bergeser. Sistem mungkin membutuhkan penyelarasan urutan, batasan desain, indeks fragmen, atau skema kode yang memang dirancang menghadapi kesalahan sinkronisasi.

### 4.5. Metrics: pengukuran yang dapat dipercaya

Modul metrik harus melaporkan setidaknya:

* Waktu encoding dan decoding.

* Throughput dalam MiB/s atau GiB/s.

* Latensi per fragmen dan per file.

* Penggunaan memori puncak.

* Rasio ukuran DNA terhadap ukuran file sumber.

* Overhead indeks, metadata, dan koreksi kesalahan.

* Persentase fragmen yang berhasil dipulihkan.

* Persentase file yang dipulihkan dengan hash identik.

* Tingkat kegagalan dan jumlah eksperimen yang tidak berhasil.

Untuk eksperimen ketahanan, recovery rate per fragmen tidak sama dengan exact-file recovery rate. File hanya dianggap pulih sempurna jika seluruh byte hasil rekonstruksi cocok dengan sumber aslinya.

### 5\. Alur kerja lengkap

* Masukkan file sumber. Hitung ukuran, hash, dan metadata awal.

* Encode. Ubah byte menjadi urutan DNA menggunakan skema yang dipilih.

* Terapkan batasan biologis. Pada mode biologis, perbaiki atau tolak urutan yang melanggar aturan desain.

* Fragmentasikan. Tambahkan indeks, checksum, dan redundansi sesuai konfigurasi.

* Simulasikan kanal. Terapkan kesalahan dengan seed dan tingkat kesalahan yang tercatat.

* Pulihkan. Identifikasi fragmen valid, koreksi kesalahan jika memungkinkan, dan susun kembali data.

* Verifikasi. Bandingkan hash hasil pemulihan dengan hash file asli.

* Benchmark. Simpan waktu, throughput, memori, overhead, dan keberhasilan recovery.

Setiap tahap harus bisa dijalankan secara mandiri agar kegagalan dapat dilokalisasi. Misalnya, bila hash akhir berbeda, laporan harus membantu membedakan kesalahan encoder, kehilangan fragmen, kegagalan koreksi, dan kesalahan rekonstruksi.

### 6\. Teknologi dan pilihan implementasi

DNAForge menggunakan Rust sebagai bahasa utama untuk mendapatkan kontrol atas memori, performa yang konsisten, dan kemampuan membangun aplikasi CLI mandiri.

| Komponen                | Pilihan                                                                                  |
| ----------------------- | ---------------------------------------------------------------------------------------- |
| Bahasa utama            | Rust stable                                                                              |
| Build system            | Cargo                                                                                    |
| Benchmark               | Criterion                                                                                |
| Pengujian               | Rust unit tests, integration tests, property-based tests                                 |
| Data eksperimen         | CSV dan metadata konfigurasi                                                             |
| Format checksum         | SHA-256 untuk verifikasi file; checksum cepat dapat digunakan untuk pemeriksaan internal |
| Paralelisasi            | Rayon atau thread pool khusus, setelah baseline stabil                                   |
| Optimasi                | Iterator, buffer reuse, streaming, SIMD                                                  |
| Integrasi data biologis | Parser FASTQ/FASTA dan dataset sequencing publik                                         |
| Antarmuka               | CLI terlebih dahulu, API library setelah stabil                                          |

Versi Rust dan versi dependensi harus dikunci dalam proyek. Pengujian CI sebaiknya dijalankan pada Linux dan Windows, dengan benchmark performa dipisahkan dari pengujian fungsional.

### 7\. Status proyek saat ini

Berdasarkan implementasi awal yang sudah dirancang, DNAForge memiliki fondasi benchmark untuk konversi data dan simulasi kesalahan sederhana. Namun, fondasi tersebut belum sama dengan sistem DNA storage yang lengkap.

Fondasi awal

Pemetaan byte–DNA, pengujian round-trip, simulator kesalahan sederhana, metrik throughput, CLI benchmark, dan ekspor CSV telah dirancang dalam paket awal.

Belum terverifikasi

Paket awal belum dikonfirmasi berhasil dikompilasi dan dijalankan pada mesin pengguna. Hasil benchmark yang sebelumnya dibahas adalah ilustrasi, bukan pengukuran nyata dari program.

Pengembangan inti berikutnya

Koreksi kesalahan sesungguhnya, fragmentasi, validasi hash, desain urutan yang kompatibel dengan DNA sintetis, dan benchmark penggunaan memori.

Paket awal yang telah dibuat dalam percakapan ini: DNAForge Benchmark Suite (.zip).

### 8\. Rencana benchmark yang lengkap

Benchmark DNAForge sebaiknya mempunyai beberapa kelompok eksperimen yang terpisah.

### A. Benchmark performa

Uji ukuran input 1 KiB, 64 KiB, 1 MiB, 16 MiB, 100 MiB, dan kemudian 1 GiB jika sumber daya komputer memungkinkan.

* Encoding dan decoding.

* Operasi dengan buffer baru versus buffer yang digunakan ulang.

* Pemrosesan satu thread versus multithread.

* Mode seluruh data di memori versus streaming.

* Throughput, latensi, dan penggunaan memori puncak.

### B. Benchmark ketahanan

Uji tingkat kesalahan, misalnya 0%, 0,1%, 0,5%, 1%, 2%, dan 5%, untuk tiap jenis kesalahan secara terpisah sebelum menguji kombinasi.

Variabel yang perlu dibandingkan:

* Tanpa koreksi kesalahan.

* Deteksi kerusakan dengan checksum.

* Koreksi kesalahan menggunakan skema yang dipilih.

* Redundansi untuk menghadapi fragmen yang hilang.

* Berbagai ukuran fragmen dan jumlah salinan.

* Beberapa seed acak pada setiap kondisi.

### C. Benchmark efisiensi penyimpanan

Bandingkan ukuran file asli dengan ukuran representasi DNA dan overhead sistem. Laporkan kapasitas payload efektif, bukan hanya kepadatan teoritis dua bit per basa.

Metrik utamanya adalah:

\\text{Overhead} =
\\frac{\\text{Ukuran encoded}-\\text{Ukuran payload}}
{\\text{Ukuran payload}}\\times100\\%

Ukuran encoded harus mencakup semua metadata, indeks, dan redundansi yang memang diperlukan oleh skema yang diuji. Jika biaya sintesis atau sequencing tidak dimodelkan, biaya tersebut harus dilaporkan terpisah.

### D. Benchmark statistik

Setiap skenario harus dijalankan berulang kali dengan seed terdokumentasi. Laporkan median, variasi, jumlah percobaan, dan interval ketidakpastian jika memungkinkan.

Satu hasil yang sukses tidak cukup untuk membuktikan bahwa sebuah skema andal pada tingkat kesalahan tertentu.

### 9\. Roadmap pengembangan

### Fase 1

Validasi

Stabilkan baseline

Kompilasi, pengujian round-trip, validasi input, benchmark yang dapat direproduksi, dan pencatatan konfigurasi. Semua hasil harus dapat diverifikasi.

Syarat selesai: pengujian lulus dan benchmark berjalan konsisten.

### Fase 2

Recovery

Bangun fragmentasi dan koreksi kesalahan

Implementasikan ID fragmen, checksum, skema koreksi yang teruji, rekonstruksi, dan laporan exact-file recovery.

Syarat selesai: kerusakan terkontrol dapat dideteksi dan batas kemampuan pemulihan diketahui.

### Fase 3

Biologis

Desain urutan DNA yang layak diuji

Tambahkan batasan GC content, homopolymer, panjang urutan, dan skema desain yang sesuai dengan target eksperimen.

Syarat selesai: urutan lolos validator dan asumsi biologis terdokumentasi.

### Fase 4

Optimasi

Optimasi performa

Streaming, paralelisasi, pengurangan alokasi, SIMD, dan pengujian penggunaan memori. Optimasi hanya dipertahankan jika benchmark menunjukkan keuntungan nyata.

Syarat selesai: hasil sebelum dan sesudah optimasi dapat dibandingkan secara adil.

### Fase 5

Validasi

Evaluasi dengan data nyata

Gunakan dataset sequencing publik, lalu bandingkan model kesalahan dengan hasil pembacaan yang tersedia. Kerja sama laboratorium menjadi langkah lanjutan bila ingin menguji proses sintesis dan penyimpanan nyata.

Syarat selesai: hasil simulasi dibandingkan dengan bukti empiris yang relevan.

### 10\. Kriteria keberhasilan proyek

DNAForge tidak seharusnya hanya dinilai berdasarkan kecepatan encoder. Kriteria keberhasilan perlu mencakup empat dimensi.

| Dimensi           | Kriteria                                                           |
| ----------------- | ------------------------------------------------------------------ |
| Kebenaran         | File yang pulih sempurna memiliki hash identik dengan sumber       |
| Ketahanan         | Tingkat pemulihan diukur pada berbagai jenis dan tingkat kesalahan |
| Efisiensi         | Throughput, memori puncak, dan overhead dicatat                    |
| Reproduksibilitas | Konfigurasi, seed, versi kode, dan hasil eksperimen tersimpan      |

Target angka untuk throughput, memori, atau recovery rate sebaiknya ditentukan setelah baseline yang nyata diperoleh. Dengan begitu, target optimasi tidak didasarkan pada angka perkiraan yang belum terbukti.

### 11\. Batasan dan risiko teknis

* Simulasi bukan eksperimen biologis. Kanal acak sederhana tidak sepenuhnya mencerminkan proses sintesis, penyimpanan, amplifikasi, dan sequencing.

* Kapasitas teoritis bukan kapasitas efektif. Metadata, batasan biologis, indeks, dan koreksi kesalahan mengurangi kapasitas payload.

* Error correction memiliki trade-off. Koreksi lebih kuat biasanya memerlukan redundansi, waktu komputasi, atau keduanya.

* Throughput harus dibuktikan. Angka benchmark harus berasal dari eksekusi nyata pada perangkat yang spesifik.

* Penyimpanan DNA fisik memerlukan mitra eksperimen. Software saja tidak membuktikan umur simpan, stabilitas kimia, atau keberhasilan sintesis.

### 12\. Visi akhir DNAForge

Tujuan jangka panjang DNAForge adalah menjadi framework riset DNA data storage yang modular, cepat, terukur, dan dapat direproduksi, dengan Rust sebagai fondasi implementasi.

Produk akhirnya idealnya mencakup library codec, engine koreksi kesalahan, simulator kanal, CLI eksperimen, format fragmen, benchmark suite, dan integrasi dengan data sequencing. Setelah validasi komputasi matang, DNAForge dapat menjadi fondasi untuk mengevaluasi desain penyimpanan DNA pada skala lebih besar.

Fokus paling tepat untuk tahap sekarang adalah menyelesaikan pipeline end-to-end yang benar terlebih dahulu: encode → fragmentasi → simulasi kerusakan → recovery → verifikasi hash → benchmark. Setelah jalur tersebut terbukti benar, barulah optimasi SIMD dan multithread menjadi prioritas.
