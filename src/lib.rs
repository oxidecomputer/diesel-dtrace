//! Add DTrace probes to Diesel connections.
//!
//! The `diesel-dtrace` crate provides a diesel [`Connection`] that includes DTrace probe points.
//! Probes are fired when a connection to the database is established and for each query.
//!
//! Example
//! -------
//!
//! The example at `examples/conn.rs` attempts to connect to a PostgreSQL database at the URL
//! provided (or localhost:5432), and run a few simple queries. The probes fired by the example can
//! be seen with:
//!
//! ```bash
//! ## dtrace -Zqn 'diesel*::: { printf("%s (%d)\n", probename, arg0) }'
//! connection_establish_start (4294967297)
//! connection_establish_end (4294967297)
//! query_start (4294967298)
//! query_end (4294967298)
//! query_start (4294967299)
//! query_end (4294967299)
//! ```
//!
//! All probes emit a unique ID as their first argument, which allows correlating the start/end
//! probes. This is crucial for timing the latency of queries, or predicating other DTrace actions
//! on a connection being established or query executing (e.g., tracing all system calls while a
//! query is running).

// Copyright 2021 Oxide Computer Company
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![feature(asm)]
#![cfg_attr(target_os = "macos", feature(asm_sym))]

use diesel::backend::Backend;
use diesel::connection::{
    AnsiTransactionManager, ConnectionGatWorkaround, SimpleConnection, TransactionManager,
};
use diesel::debug_query;
use diesel::expression::QueryMetadata;
use diesel::prelude::*;
use diesel::query_builder::{AsQuery, QueryFragment, QueryId};
use diesel::r2d2::R2D2Connection;
use std::ops::{Deref, DerefMut};
use usdt::UniqueId;

#[usdt::provider(provider = "diesel_db")]
pub mod probes {
    use usdt::UniqueId;
    pub fn connection_establish_start(_: &UniqueId, url: &str) {}
    pub fn connection_establish_end(_: &UniqueId, success: u8) {}
    pub fn query_start(_: &UniqueId, query: &str) {}
    pub fn query_end(_: &UniqueId) {}
    pub fn transaction_start(_: &UniqueId) {}
    pub fn transaction_end(_: &UniqueId) {}
}

/// A [`Connection`] that includes DTrace probe points.
///
/// The following probe points are defined:
///
/// - `connection_establish_{start,end}`: Emitted when a connection is started and completed.
/// - `query_{start,end}`: Emitted when a query is begun and completed.
/// - `transaction_{start,end}`: Emitted when a transaction is begun and completed.
///
/// Every probe includes a [`UniqueId`] as its first argument, which allows correlating each
/// start/end probe in an application. Probe `connection_establish_start` includes the database URL
/// as its second argument, and the `query_start` probe includes the actual SQL query as a string
/// as its second argument.
pub struct DTraceConnection<C: Connection>(C);

impl<C: Connection> Deref for DTraceConnection<C> {
    type Target = C;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<C: Connection> DerefMut for DTraceConnection<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<C: Connection> SimpleConnection for DTraceConnection<C> {
    fn batch_execute(&mut self, query: &str) -> QueryResult<()> {
        let id = UniqueId::new();
        probes::query_start!(|| (id.clone(), query));
        let result = self.0.batch_execute(query);
        probes::query_end!(|| id);
        result
    }
}

impl<'a, C: Connection> ConnectionGatWorkaround<'a, C::Backend> for DTraceConnection<C> {
    type Cursor = <C as ConnectionGatWorkaround<'a, C::Backend>>::Cursor;
    type Row = <C as ConnectionGatWorkaround<'a, C::Backend>>::Row;
}

impl<C> Connection for DTraceConnection<C>
where
    C: Connection<TransactionManager = AnsiTransactionManager>,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    type Backend = C::Backend;
    type TransactionManager = C::TransactionManager;

    fn establish(database_url: &str) -> ConnectionResult<Self> {
        let id = UniqueId::new();
        probes::connection_establish_start!(|| { (&id, database_url) });
        let conn = C::establish(database_url);
        probes::connection_establish_end!(|| (&id, u8::from(conn.is_ok())));
        Ok(DTraceConnection(conn?))
    }

    fn execute(&mut self, query: &str) -> QueryResult<usize> {
        let id = UniqueId::new();
        probes::query_start!(|| (&id, query));
        let result = self.0.execute(query);
        probes::query_end!(|| &id);
        result
    }

    fn load<T>(
        &mut self,
        source: T,
    ) -> QueryResult<<Self as ConnectionGatWorkaround<Self::Backend>>::Cursor>
    where
        T: AsQuery,
        T::Query: QueryFragment<Self::Backend> + QueryId,
        Self::Backend: QueryMetadata<T::SqlType>,
    {
        let query = source.as_query();
        let id = UniqueId::new();
        probes::query_start!(|| { (&id, debug_query::<Self::Backend, _>(&query).to_string(),) });
        let result = self.0.load(query);
        probes::query_end!(|| &id);
        result
    }

    fn execute_returning_count<T>(&mut self, source: &T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + QueryId,
    {
        let id = UniqueId::new();
        probes::query_start!(|| { (&id, debug_query::<Self::Backend, _>(&source).to_string(),) });
        let result = self.0.execute_returning_count(source);
        probes::query_end!(|| &id);
        result
    }

    fn transaction_state(
        &mut self,
    ) -> &mut <C::TransactionManager as TransactionManager<C>>::TransactionStateData {
        self.0.transaction_state()
    }
}

impl<C> R2D2Connection for DTraceConnection<C>
where
    C: R2D2Connection + Connection<TransactionManager = AnsiTransactionManager>,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    fn ping(&mut self) -> QueryResult<()> {
        self.0.ping()
    }
}
