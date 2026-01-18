use std::time::{Duration, Instant};

use casparian_db::{DbConnection, DbValue};
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(feature = "sqlite")]
async fn test_sqlite_actor_transaction_serializes_requests() {
    let conn = DbConnection::open_sqlite_memory().await.unwrap();
    conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, marker TEXT)")
        .await
        .unwrap();

    let (started_tx, started_rx) = oneshot::channel();
    let conn_tx = conn.clone();

    let tx_handle = tokio::spawn(async move {
        conn_tx
            .transaction(move |tx| {
                tx.execute("INSERT INTO t (marker) VALUES (?)", &[DbValue::from("tx-1")])?;
                let _ = started_tx.send(());
                std::thread::sleep(Duration::from_millis(150));
                tx.execute("INSERT INTO t (marker) VALUES (?)", &[DbValue::from("tx-2")])?;
                Ok(())
            })
            .await
            .unwrap();
    });

    let _ = started_rx.await;

    let conn_insert = conn.clone();
    let mut insert_handle = tokio::spawn(async move {
        conn_insert
            .execute("INSERT INTO t (marker) VALUES (?)", &[DbValue::from("concurrent")])
            .await
            .unwrap();
    });

    let early = tokio::time::timeout(Duration::from_millis(50), &mut insert_handle).await;
    assert!(early.is_err(), "concurrent insert should wait for tx");

    tx_handle.await.unwrap();
    insert_handle.await.unwrap();

    let rows = conn.query_all("SELECT id FROM t", &[]).await.unwrap();
    assert_eq!(rows.len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(feature = "sqlite")]
async fn test_sqlite_actor_dropped_response_does_not_break_actor() {
    let conn = DbConnection::open_sqlite_memory().await.unwrap();
    conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, marker TEXT)")
        .await
        .unwrap();

    let (started_tx, started_rx) = oneshot::channel();
    let conn_tx = conn.clone();

    let handle = tokio::spawn(async move {
        let _ = conn_tx
            .transaction(move |tx| {
                tx.execute("INSERT INTO t (marker) VALUES (?)", &[DbValue::from("slow")])?;
                let _ = started_tx.send(());
                std::thread::sleep(Duration::from_millis(150));
                Ok(())
            })
            .await;
    });

    let _ = started_rx.await;
    handle.abort();

    tokio::time::sleep(Duration::from_millis(200)).await;

    conn.execute("INSERT INTO t (marker) VALUES (?)", &[DbValue::from("after")])
        .await
        .unwrap();

    let rows = conn.query_all("SELECT id FROM t", &[]).await.unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(feature = "sqlite")]
async fn test_sqlite_actor_query_waits_for_transaction() {
    let conn = DbConnection::open_sqlite_memory().await.unwrap();
    conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, marker TEXT)")
        .await
        .unwrap();
    conn.execute("INSERT INTO t (marker) VALUES (?)", &[DbValue::from("row")])
        .await
        .unwrap();

    let (started_tx, started_rx) = oneshot::channel();
    let conn_tx = conn.clone();

    let tx_handle = tokio::spawn(async move {
        conn_tx
            .transaction(move |_tx| {
                let _ = started_tx.send(());
                std::thread::sleep(Duration::from_millis(150));
                Ok(())
            })
            .await
            .unwrap();
    });

    let _ = started_rx.await;
    let started = Instant::now();
    let _rows = conn.query_all("SELECT id FROM t", &[]).await.unwrap();
    let elapsed = started.elapsed();

    assert!(
        elapsed >= Duration::from_millis(120),
        "query returned too quickly: {:?}",
        elapsed
    );

    tx_handle.await.unwrap();
}
