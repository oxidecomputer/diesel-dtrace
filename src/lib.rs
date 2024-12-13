#![doc = include_str!("../README.md")]
// Copyright 2024 Oxide Computer Company
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
#![cfg_attr(usdt_need_asm, feature(asm))]
#![cfg_attr(all(target_os = "macos", usdt_need_asm_sym), feature(asm_sym))]

use diesel::backend::Backend;
use diesel::connection::{
    AnsiTransactionManager, LoadConnection, SimpleConnection, TransactionManager,
    TransactionManagerStatus,
};
use diesel::debug_query;
use diesel::expression::QueryMetadata;
use diesel::prelude::*;
use diesel::query_builder::{AsQuery, QueryFragment, QueryId};
use diesel::r2d2::R2D2Connection;
use std::ops::{Deref, DerefMut};
use usdt::UniqueId;
use uuid::Uuid;

#[usdt::provider(provider = "diesel_db")]
pub mod probes {
    /// Fires right before we attempt to establish a connection.
    pub fn connection__establish__start(_: &UniqueId, conn_id: Uuid, url: &str) {}
    /// Fires when we finish establishing a connection, with a flag indicating
    /// whether it succeeded or failed.
    pub fn connection__establish__done(_: &UniqueId, conn_id: Uuid, success: u8) {}
    /// Fires just before issuing a SQL query.
    pub fn query__start(_: &UniqueId, conn_id: Uuid, query: &str) {}
    /// Fires when a query completes.
    pub fn query__done(_: &UniqueId, conn_id: Uuid) {}
    /// Fires when we start a transaction.
    ///
    /// This includes the connection ID as well as the depth of the transaction.
    /// As transactions can be nested, _both_ of these are required to unique ID
    /// a transaction in full.
    ///
    /// The depth is `0` if there is no outstanding transaction, meaning this is
    /// not nested inside another transaction. Querying the transaction status
    /// may fail, in which case `depth == -1`.
    pub fn transaction__start(conn_id: Uuid, depth: i64) {}
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
    pub fn transaction__done(conn_id: Uuid, depth: i64, committed: u8) {}
}

/// A [`Connection`] wrapper that inserts DTrace probe points.
///
/// See the module-level documentation for more details.
#[derive(Debug)]
pub struct DTraceConnection<C: Connection> {
    inner: C,
    id: Uuid,
}

impl<C: Connection> DTraceConnection<C> {
    pub fn id(&self) -> Uuid {
        self.id
    }
}

impl<C: Connection> Deref for DTraceConnection<C> {
    type Target = C;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<C: Connection> DerefMut for DTraceConnection<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<C: Connection> SimpleConnection for DTraceConnection<C> {
    fn batch_execute(&mut self, query: &str) -> QueryResult<()> {
        let id = UniqueId::new();
        probes::query__start!(|| (&id, self.id, query));
        let result = self.inner.batch_execute(query);
        probes::query__done!(|| (&id, self.id));
        result
    }
}

impl<C> LoadConnection for DTraceConnection<C>
where
    C: Connection<TransactionManager = AnsiTransactionManager> + LoadConnection,
    C::Backend: Default,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    type Cursor<'conn, 'query>
        = C::Cursor<'conn, 'query>
    where
        Self: 'conn;
    type Row<'conn, 'query>
        = C::Row<'conn, 'query>
    where
        Self: 'conn;

    fn load<'conn, 'query, T>(
        &'conn mut self,
        source: T,
    ) -> QueryResult<Self::Cursor<'conn, 'query>>
    where
        T: diesel::query_builder::Query + QueryFragment<Self::Backend> + QueryId + 'query,
        Self::Backend: QueryMetadata<T::SqlType>,
    {
        let query = source.as_query();
        let id = UniqueId::new();
        probes::query__start!(|| (
            &id,
            self.id,
            debug_query::<Self::Backend, _>(&query).to_string()
        ));
        let result = self.inner.load(query);
        probes::query__done!(|| (&id, self.id));
        result
    }
}

impl<C> Connection for DTraceConnection<C>
where
    C: Connection<TransactionManager = AnsiTransactionManager>,
    C::Backend: Default,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    type Backend = C::Backend;
    type TransactionManager = DTraceTransactionManager<C>;

    fn establish(database_url: &str) -> ConnectionResult<Self> {
        let id = UniqueId::new();
        let conn_id = Uuid::new_v4();
        probes::connection__establish__start!(|| (&id, conn_id, database_url));
        let conn = C::establish(database_url);
        probes::connection__establish__done!(|| (&id, conn_id, u8::from(conn.is_ok())));
        let inner = conn?;
        Ok(DTraceConnection { inner, id: conn_id })
    }

    fn execute_returning_count<T>(&mut self, source: &T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + QueryId,
    {
        let id = UniqueId::new();
        probes::query__start!(|| (
            &id,
            self.id,
            debug_query::<Self::Backend, _>(&source).to_string()
        ));
        let result = self.inner.execute_returning_count(source);
        probes::query__done!(|| (&id, self.id));
        result
    }

    fn transaction_state(
        &mut self,
    ) -> &mut <DTraceTransactionManager<C> as TransactionManager<DTraceConnection<C>>>::TransactionStateData{
        self.inner.transaction_state()
    }

    fn instrumentation(&mut self) -> &mut dyn diesel::connection::Instrumentation {
        self.inner.instrumentation()
    }

    fn set_instrumentation(&mut self, instrumentation: impl diesel::connection::Instrumentation) {
        self.inner.set_instrumentation(instrumentation)
    }
}

impl<C> diesel::connection::ConnectionSealed for DTraceConnection<C>
where
    C: Connection<TransactionManager = AnsiTransactionManager>,
    C::Backend: Default,
    <C::Backend as Backend>::QueryBuilder: Default,
{
}

impl<C> R2D2Connection for DTraceConnection<C>
where
    C: R2D2Connection + Connection<TransactionManager = AnsiTransactionManager>,
    C::Backend: Default,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    fn ping(&mut self) -> QueryResult<()> {
        self.inner.ping()
    }
}

/// A [`TransactionManager`] for a [`DTraceConnection`].
///
/// This manager is responsible for the probes `transaction-start` and
/// `transaction-done`. See the module-level documentation for more details on
/// these probes.
pub struct DTraceTransactionManager<C> {
    _data: std::marker::PhantomData<C>,
}

impl<C> DTraceTransactionManager<C>
where
    C: Connection<TransactionManager = AnsiTransactionManager>,
{
    /// Compute the current transaction depth for the DTrace probes.
    fn depth(conn: &mut DTraceConnection<C>) -> i64 {
        let status = AnsiTransactionManager::transaction_manager_status_mut(&mut conn.inner);
        match status.transaction_depth() {
            Ok(Some(depth)) => i64::from(depth.get()),
            Ok(None) => 0,
            Err(_) => -1,
        }
    }
}

impl<C> TransactionManager<DTraceConnection<C>> for DTraceTransactionManager<C>
where
    C: Connection<TransactionManager = AnsiTransactionManager>,
    C::Backend: Default,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    type TransactionStateData = AnsiTransactionManager;

    fn begin_transaction(conn: &mut DTraceConnection<C>) -> QueryResult<()> {
        // TODO-performance: We're unconditionally computing the transaction
        // depth here, even if the probe is not enabled.
        //
        // This ultimately comes from the interaction of a few things. These
        // trait methods don't make it easy to store state -- since they take
        // the mutable connection, not `&mut self`, we have to store everything
        // on the connection type, but that interacts with the
        // `Connection::transaction_state()` method weirdly. Second, even if we
        // could do that, there's no good way to keep that in sync if the probes
        // are disabled _while_ a transaction is outstanding (we'd end up
        // thinking we were still in a transaction, when we're not anymore).
        //
        // Last, this interacts with a `Clone` bound on the `usdt` crate's
        // argument closure that we pass to the probe macro itself. That is
        // required today so that we can accurately type check the return value
        // of the closure. Still, there are probably ways around that which
        // still give nice error messages. See
        // https://github.com/oxidecomputer/usdt/issues/136 for some more
        // background and context.
        //
        // In any case, it is probably "fine" to pay this cost all the time,
        // even though it's antithetical to the "zero disabled-probe effect"
        // ethos of DTrace. These methods really just take a pointer to a field
        // of `AnsiTransactionManager`, and destructure a few enums. It should
        // be in the noise for any realistic database application.
        let depth = Self::depth(conn);
        probes::transaction__start!(|| (&conn.id, depth));
        AnsiTransactionManager::begin_transaction(&mut conn.inner)
    }

    fn rollback_transaction(conn: &mut DTraceConnection<C>) -> QueryResult<()> {
        let result = AnsiTransactionManager::rollback_transaction(&mut conn.inner);
        let depth = Self::depth(conn);
        probes::transaction__done!(|| (&conn.id, depth, 0));
        result
    }

    fn commit_transaction(conn: &mut DTraceConnection<C>) -> QueryResult<()> {
        let result = AnsiTransactionManager::commit_transaction(&mut conn.inner);
        let depth = Self::depth(conn);
        probes::transaction__done!(|| (&conn.id, depth, 1));
        result
    }

    fn transaction_manager_status_mut(
        conn: &mut DTraceConnection<C>,
    ) -> &mut TransactionManagerStatus {
        AnsiTransactionManager::transaction_manager_status_mut(&mut conn.inner)
    }
}
