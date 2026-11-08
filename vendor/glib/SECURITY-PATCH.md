# GLib 0.18.5 security backport

Original source: https://crates.io/crates/glib/0.18.5

Original crate SHA-256: `233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`. The original MIT license is retained.

The runtime fix changes only two lines in `src/variant_iter.rs`: the C output pointer is mutable and passed by mutable reference, matching upstream commit [b5a4071](https://github.com/gtk-rs/gtk-rs-core/commit/b5a4071e439bef2b5eea76c3aa25e5ae84839e34), fixing RUSTSEC-2024-0429. Upstream deprecation suppression annotations/comments are removed from the vendored source so diagnostics remain visible. No package versions, features or transitive dependencies are added. GTK 0.18 requires this GLib API generation. The crate version stays 0.18.5; version-only scanners may still report the advisory.

Run `node scripts/test-glib-backport.mjs` for the optimized native regression. Remove this patch when the GTK dependency supports an upstream fixed GLib release.
