# diesel-dtrace

Add DTrace probes to Diesel connections.

## Overview

The `diesel-dtrace` crate provides a diesel [`Connection`][1] that includes DTrace probe points.
Probes are fired when a connection to the database is established and for each
query and transaction.

In order to register probes, your program must invoke `usdt::register_probes()`
before the probe points are executed.

## Probes

```ignore
/// Fires right before we attempt to establish a connection.
connection-establish-start(id: &UniqueId, conn_id: Uuid, url: &str)
/// Fires when we finish establishing a connection, with a flag indicating
/// whether it succeeded or failed.
connection-establish-done(id: &UniqueId, conn_id: Uuid, success: u8)
/// Fires just before issuing a SQL query.
query-start(id: &UniqueId, conn_id: Uuid, query: &str)
/// Fires when a query completes.
query-done(id: &UniqueId, conn_id: Uuid)
/// Fires when we start a transaction.
///
/// This includes the connection ID as well as the depth of the transaction.
/// As transactions can be nested, _both_ of these are required to unique ID
/// a transaction in full.
///
/// The depth is `0` if there is no outstanding transaction, meaning this is
/// not nested inside another transaction. Querying the transaction status
/// may fail, in which case `depth == -1`.
transaction-start(conn_id: Uuid, depth: i64)
/// Fires when a transaction completes.
///
/// This includes the connection ID as well as the depth of the transaction.
/// As transactions can be nested, _both_ of these are required to uniquely
/// ID a transaction in full.
///
/// The depth is `0` if there is no outstanding transaction, meaning this is
/// not nested inside another transaction. Querying the transaction status
/// may fail, in which case `depth == -1`.
///
/// This also includes a flag indicating whether the transaction was
/// committed (`committed == 1`) or rolled back (`committed == 0`).
transaction-done(conn_id: Uuid, depth: i64, committed: u8)
```

## Transaction probes

It's important to note that the transaction-related probes include a connection
ID and a depth. As transactions can be nested on the same connection, _both_ of
these are required to uniquely identify transactions.

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

## Notes

This crate relies on the [`usdt`][2] crate. On macOS systems, a nightly
compiler is required prior to Rust 1.67. On other systems a nightly compiler is
required prior to Rust 1.59.

[1]: https://docs.rs/diesel/latest/diesel/connection/trait.Connection.html
[2]: https://crates.io/crates/usdt
