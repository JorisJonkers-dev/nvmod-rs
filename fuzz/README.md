# Fuzzing

Every parser of untrusted input has a target here (`fuzz_targets/`). The directory is its own Cargo workspace so
`libfuzzer-sys` stays out of the shipped crates. Pull requests only type-check the targets (`task fuzz:check`);
`.github/workflows/fuzz.yml` runs each one for ten minutes every Monday. Locally, with a nightly toolchain:

```sh
cargo +nightly fuzz run pe_inspect -- -max_total_time=60 -rss_limit_mb=4096
```

Run it under `capped` (see docs/PRACTICES.md). Seeds are built in code, never taken from real game or mod files.
