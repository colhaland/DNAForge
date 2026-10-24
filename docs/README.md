# Dokumentasi DNAForge

Dokumen ini menjadi indeks untuk spesifikasi dan pelaksanaan pengembangan.

| Dokumen | Tujuan |
| --- | --- |
| [Spesifikasi baseline](baseline-specification.md) | Sumber normatif untuk ruang lingkup, perilaku, format arsip, CLI, persyaratan, verifikasi, dan gap yang diketahui. |
| [Rencana implementasi](implementation-plan.md) | Urutan pengembangan dan gerbang penerimaan yang memetakan spesifikasi ke pekerjaan. |

## Urutan membaca

1. Baca ruang lingkup dan istilah pada spesifikasi baseline.
2. Gunakan persyaratan `FR-*`, `NFR-*`, dan `TST-*` sebagai kontrak implementasi.
3. Gunakan rencana implementasi untuk dependensi, urutan, dan definisi selesai.
4. Perbarui status implementasi dan gap saat perilaku kode berubah.

## Otoritas dan perubahan

Spesifikasi baseline adalah sumber acuan tunggal. README tingkat repositori hanya ringkasan dan panduan masuk. Perubahan format arsip, makna CLI, perilaku pemulihan, atau metrik harus memperbarui spesifikasi dan pengujian terkait dalam perubahan yang sama.

Istilah **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, dan **MAY** memiliki makna normatif: wajib, dilarang, dianjurkan, tidak dianjurkan, dan opsional. Persyaratan target tidak berarti fitur tersebut sudah ada; status aktual dicatat terpisah.