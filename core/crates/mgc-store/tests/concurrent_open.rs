use std::sync::{Arc, Barrier};
use std::thread;

use mgc_store::Database;

#[test]
fn parallel_first_open_and_generation_allocation_are_safe() {
    let directory = tempfile::tempdir().expect("create temporary store directory");
    let database_path = Arc::new(directory.path().join("store.db"));
    let workers = 24;
    let barrier = Arc::new(Barrier::new(workers));
    let handles: Vec<_> = (0..workers)
        .map(|index| {
            let path = Arc::clone(&database_path);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let database = Database::open(&path).unwrap_or_else(|error| {
                    panic!("worker {index} failed opening store: {error:#}")
                });
                database
                    .begin_cas_generation(&format!("/parallel/open/{index}"))
                    .unwrap_or_else(|error| {
                        panic!("worker {index} failed allocating generation: {error:#}")
                    })
            })
        })
        .collect();

    let generations: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("worker thread must not panic"))
        .collect();
    assert_eq!(generations.len(), workers);
    assert!(generations.iter().all(|generation| *generation == 1));

    let database = Database::open(&database_path).expect("reopen initialized store");
    let schema_version: i64 = database
        .conn()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read store schema version");
    assert_eq!(
        schema_version, 1,
        "schema migrations must version the store"
    );
}

#[test]
fn opening_a_store_from_a_newer_schema_fails_closed() {
    let directory = tempfile::tempdir().expect("create temporary store directory");
    let database_path = directory.path().join("store.db");
    let database = Database::open(&database_path).expect("create initial store schema");
    database
        .conn()
        .pragma_update(None, "user_version", 99_i64)
        .expect("set future schema version");
    drop(database);

    let error = match Database::open(&database_path) {
        Ok(_) => panic!("newer store schema must not open"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("newer than this MagiCore build supports"),
        "future schema must be rejected clearly: {error:#}"
    );
}
