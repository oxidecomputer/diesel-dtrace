// Copyright 2021 Oxide Computer Company

use diesel::r2d2::Pool;
use diesel::{connection::SimpleConnection, pg::PgConnection, r2d2::ConnectionManager, Connection};
use diesel_dtrace::DTraceConnection;

fn main() {
    usdt::register_probes().unwrap();
    let url = if let Some(url) = std::env::args().nth(1) {
        url
    } else {
        String::from("postgresql://localhost:5432")
    };
    let manager = ConnectionManager::<DTraceConnection<PgConnection>>::new(&url);
    let pool = Pool::builder()
        .build(manager)
        .expect("Failed to build pool");
    let mut conn = pool.get().expect("Failed to connect to DB");
    let _ = conn
        .load(diesel::dsl::sql_query("SELECT 1"))
        .expect("Failed to execute statement");
    let _ = conn
        .batch_execute(concat!(
            "CREATE DATABASE my_test; ",
            "CREATE TABLE my_test.foo (x Integer PRIMARY KEY, y String); ",
            "DROP DATABASE my_test;"
        ))
        .expect("Batch execute failed");
    foo::<DTraceConnection<PgConnection>>();
}

fn foo<T: Send>() {}
