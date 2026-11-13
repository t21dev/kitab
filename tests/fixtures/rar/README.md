# RAR archives (CBR reader tests)

The `libarchive-*.rar` files are libarchive's own RAR test archives, from
`libarchive/test/test_read_format_rar*.rar.uu` in
https://github.com/libarchive/libarchive (uudecoded, renamed with the
`libarchive-` prefix). They are BSD-2-Clause; the notice is in
`LICENSE-libarchive.txt`.

They cover RAR 2.9–4 and RAR 5, stored, compressed (LZ and PPMd), solid
archives, filters, Unicode names, Windows archives and links (`opens: true`,
with the number of files); and archives Kitab refuses: encrypted, split into
parts, and damaged ones from libarchive's fuzzing (`opens: false`). Every
file that opens is checked against the CRC-32 in its header.

`expected.json` is what `crates/kitab-mupdf/tests/rar.rs` checks.
