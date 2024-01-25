#![doc = include_str!("../README.md")]
// Copyright 2023 Oxide Computer Company
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
    pub fn connection__establish__start(_: &UniqueId, conn_id: Uuid, url: &str) {}
    pub fn connection__establish__done(_: &UniqueId, conn_id: Uuid, success: u8) {}
    pub fn query__start(_: &UniqueId, conn_id: Uuid, query: &str) {}
    pub fn query__done(_: &UniqueId, conn_id: Uuid) {}
    pub fn transaction__start(_: &UniqueId, conn_id: Uuid) {}
    pub fn transaction__done(_: &UniqueId, conn_id: Uuid) {}
}

/// A [`Connection`] that includes DTrace probe points.
///
/// This crate generates a provider named `diesel-db`. The following probe points are defined:
///
/// ```ignore
/// connection-establish-start(_: &UniqueId, conn_id: Uuid, url: &str)
/// connection-establish-done(_: &UniqueId, conn_id: Uuid, success: u8)
/// query-start(_: &UniqueId, conn_id: Uuid, query: &str)
/// query-done(_: &UniqueId, conn_id: Uuid)
/// transaction-start(_: &UniqueId, conn_id: Uuid)
/// transaction-done(_: &UniqueId, conn_id: Uuid)
/// ```
///
/// The first argument is a [`UniqueId`], which enables correlating the start and done probes.
/// `conn_id` is a unique identifier for the connection itself, which enables one to see which
/// connections are executing each query. `query-start` also includes the actual SQL query string
/// as its third argument.
#[derive(Debug)]
pub struct DTraceConnection<C: Connection> {
    inner: C,
    id: Uuid,
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
    type Cursor<'conn, 'query> = C::Cursor< 'conn, 'query> where Self: 'conn;
    type Row<'conn, 'query> = C::Row<'conn, 'query> where Self: 'conn;

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
    type TransactionManager = C::TransactionManager;

    fn establish(database_url: &str) -> ConnectionResult<Self> {
        let id = UniqueId::new();
        let conn_id = Uuid::new_v4();
        probes::connection__establish__start!(|| (&id, conn_id, database_url));
        let conn = C::establish(database_url);
        probes::connection__establish__done!(|| (&id, conn_id, u8::from(conn.is_ok())));
        let inner = conn?;
        Ok(DTraceConnection { inner, id: conn_id })
    }

    fn transaction<T, E, F>(&mut self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: From<diesel::result::Error>,
    {
        let id = UniqueId::new();
        probes::transaction__start!(|| (&id, self.id));
        let result = Self::TransactionManager::transaction(self, f);
        probes::transaction__done!(|| (&id, self.id));
        result
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
    ) -> &mut <C::TransactionManager as TransactionManager<C>>::TransactionStateData {
        self.inner.transaction_state()
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
