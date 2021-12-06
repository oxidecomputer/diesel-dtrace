# diesel-dtrace

Add DTrace probes to Diesel connections.

Overview
--------

The `diesel-dtrace` crate provides a diesel [`Connection`][1] that includes DTrace probe points.
Probes are fired when a connection to the database is established and for each query.

Example
-------

The example at `examples/conn.rs` attempts to connect to a PostgreSQL database at the URL
provided (or localhost:5432), and run a few simple queries. The probes fired by the example can
be seen with:

```bash
# dtrace -Zqn 'diesel*::: { printf("%s (%d)\n", probename, arg0) }'
connection_establish_start (4294967297)
connection_establish_end (4294967297)
query_start (4294967298)
query_end (4294967298)
query_start (4294967299)
query_end (4294967299)
```

All probes emit a unique ID as their first argument, which allows correlating the start/end
probes. This is crucial for timing the latency of queries, or predicating other DTrace actions
on a connection being established or query executing (e.g., tracing all system calls while a
query is running).

Notes
-----

This crate relies on the [`usdt`][2] crate, which requires a nightly compiler. On macOS systems,
the compiler version must be at least 2021-11-24. On non-macOS systems, a stable toolchain
will be supported once the [`asm` feature lands in stable][2].

[1]: https://docs.rs/diesel/latest/diesel/connection/trait.Connection.html
[2]: https://github.com/rust-lang/rust/issues/72016#issuecomment-964421481
