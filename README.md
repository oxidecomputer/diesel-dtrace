# diesel-dtrace

Add DTrace probes to Diesel connections.

## Overview

The `diesel-dtrace` crate provides a diesel [`Connection`][1] that includes DTrace probe points.
Probes are fired when a connection to the database is established and for each query.

In order to register probes, your program must invoke `usdt::register_probes()`
before the probe points are executed.

## Probes

```ignore
diesel-db*:::connection-establish-start(id: uint64_t, conn_id: Uuid, url: &str);
diesel-db*:::connection-establish-done(id: uint64_t, conn_id: Uuid, success: u8);
diesel-db*:::query-start(id: uint64_t, conn_id: Uuid, query: &str);
diesel-db*:::query-done(id: uint64_t, conn_id: Uuid);
diesel-db*:::transaction-start(id: uint64_t, conn_id: Uuid);
diesel-db*:::transaction-done(id: uint64_t, conn_id: Uuid);
```

## Example

The example at `examples/conn.rs` attempts to connect to a PostgreSQL database at the URL
provided (or localhost:5432), and run a few simple queries. The probes fired by the example can
be seen with:

```console
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

## Notes

This crate relies on the [`usdt`][2] crate. On macOS systems, a nightly
compiler is required prior to Rust 1.67. On other systems a nightly compiler is
required prior to Rust 1.59.

[1]: https://docs.rs/diesel/latest/diesel/connection/trait.Connection.html
[2]: https://crates.io/crates/usdt
