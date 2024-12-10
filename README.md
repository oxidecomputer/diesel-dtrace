# diesel-dtrace

Add DTrace probes to Diesel connections.

## Overview

The `diesel-dtrace` crate provides a diesel [`Connection`][1] that includes
DTrace DTrace probe points. Probes are fired when we establish a connection to
the database; for each query; and each transaction. There are start / done
probes for each of these -- those fire when we start the operation (e.g., start
to establish a connection) and when the operation is completed (e.g., when we
finish establishing a connection). This lets users trace the latency of those
operations, or conditionally trace other operations while those are in progress.

Your program must invoke [`usdt::register_probes`] for the probe points to be
available for tracing. See the documentation of that method for more details.

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
/// As transactions can be nested, _both_ of these are required to uniquel
/// identify a transaction.
///
/// The depth is `0` if there is no outer transaction, meaning this is not
/// nested inside another transaction. In the internal implementation, querying
/// the transaction depth can fail, in which case `depth == -1`. This indicates
/// an unknown, internal error.
transaction-start(conn_id: Uuid, depth: i64)
/// Fires when a transaction completes.
///
/// This includes the connection ID as well as the depth of the transaction.
/// As transactions can be nested, _both_ of these are required to uniquely
/// identify a transaction.
///
/// The depth is `0` if there is no outer transaction, meaning this is not
/// nested inside another transaction. In the internal implementation, querying
/// the transaction depth can fail, in which case `depth == -1`. This indicates
/// an unknown, internal error.
///
/// This also includes a flag indicating whether the transaction was
/// committed (`committed == 1`) or rolled back (`committed == 0`).
transaction-done(conn_id: Uuid, depth: i64, committed: u8)
```

## Transaction probes

It's important to note that the transaction-related probes include a connection
ID and a depth. As transactions can be nested on the same connection, _both_ of
these are required to uniquely identify transactions. For example, here is how
one could trace the latency of each transaction:

```console
diesel_db*:::transaction-start
{
    txn[copyinstr(arg0), arg1] = timestamp;
}

diesel_db*:::transaction-done
/txn[copyinstr(arg0), arg1]/
{
    this->key = copyinstr(arg0);
    this->conn_id = json(this->key, "ok");
    this->depth = arg1;
    this->latency = (timestamp - txn[this->key, this->depth]) / 1000;
    this->commit = arg2;
    printf(
        "%s on conn '%s' (depth=%d), %d us\n",
        this->commit ? "COMMIT" : "ROLLBACK",
        this->conn_id,
        this->depth,
        this->latency
    );
    txn[this->key, this->depth] = 0;
}
```

And here is what some of the output might look like:

```console
COMMIT on conn '094df7ba-8965-4f47-885a-96d79be24ff6' (depth=1), 21944 us
COMMIT on conn '6b70374e-6fd8-48ab-aa32-ed846ae513b8' (depth=1), 217627 us
COMMIT on conn '6232eae8-49f4-409d-bda1-103967035fca' (depth=1), 212682 us
COMMIT on conn 'a78e735d-b965-4e2f-b7bd-b10cebda3abc' (depth=1), 7028 us
COMMIT on conn '0a656b97-a990-41cd-9cb4-c819f4d82234' (depth=1), 245949 us
COMMIT on conn 'a78e735d-b965-4e2f-b7bd-b10cebda3abc' (depth=1), 5246 us
COMMIT on conn 'a78e735d-b965-4e2f-b7bd-b10cebda3abc' (depth=1), 8440 us
COMMIT on conn '207e4221-40a8-455e-85f1-24b68fb2efb4' (depth=1), 21737 us
COMMIT on conn 'a78e735d-b965-4e2f-b7bd-b10cebda3abc' (depth=1), 8512 us
COMMIT on conn 'a78e735d-b965-4e2f-b7bd-b10cebda3abc' (depth=1), 5998 us
COMMIT on conn '0a656b97-a990-41cd-9cb4-c819f4d82234' (depth=1), 47322 us
COMMIT on conn 'b6a0d6b4-51c9-4f48-a8b9-9f48bd7615d6' (depth=1), 38717 us
```

## Example

The example at `examples/conn.rs` attempts to connect to a PostgreSQL database at the URL
provided (or localhost:5432), and run a few simple queries. The probes fired by the example can
be seen with:

```console
# dtrace -Zqn 'diesel*::: { printf("%s (%d)\n", probename, arg0) }'
connection-establish-start (4294967297)
connection-establish-end (4294967297)
query-start (4294967298)
query-end (4294967298)
query-start (4294967299)
query-end (4294967299)
```

## Notes

This crate relies on the [`usdt`][2] crate. On macOS systems, a nightly
compiler is required prior to Rust 1.67. On other systems a nightly compiler is
required prior to Rust 1.59.

[1]: https://docs.rs/diesel/latest/diesel/connection/trait.Connection.html
[2]: https://crates.io/crates/usdt
