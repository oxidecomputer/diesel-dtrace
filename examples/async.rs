// Copyright 2021 Oxide Computer Company

use diesel::pg::PgConnection;
use diesel_dtrace::DTraceConnection;
use async_bb8_diesel::{AsyncSimpleConnection, ConnectionManager};
use bb8::Pool;

#[tokio::main]
async fn main() {
    usdt::register_probes().unwrap();
    let url = if let Some(url) = std::env::args().nth(1) {
        url
    } else {
        String::from("postgresql://localhost:5432")
    };
    let manager = ConnectionManager::<DTraceConnection<PgConnection>>::new(&url);
    let pool = Pool::builder().build(manager).await.expect("Failed to build pool");
    let conn = pool.get().await.expect("Failed to connect to DB");
    let _ = conn
        .batch_execute_async(concat!(
            "CREATE DATABASE my_test; ",
            "CREATE TABLE my_test.foo (x Integer PRIMARY KEY, y String); ",
            "DROP DATABASE my_test;"
        ))
        .await
        .expect("Batch execute failed");
}
