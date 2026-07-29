#[test]
fn corrupt_sqlite_is_preserved_before_canonical_ledger_failure() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-sqlite-ledger-recovery-order-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    let database = paths::observability_db_file();
    fs::create_dir_all(database.parent().unwrap()).unwrap();
    fs::write(&database, b"corrupt sqlite bytes").unwrap();
    let ledger = FailingLedgerReader {
        database: &database,
        called_after_recovery: Cell::new(false),
    };

    let error = status(&ledger).unwrap_err();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    assert_eq!(error.message, "injected canonical ledger read failure");
    assert!(
        ledger.called_after_recovery.get(),
        "canonical ledger was read before corrupt SQLite preservation"
    );
}

#[test]
fn sqlite_replay_faults_are_atomic_and_concurrent_readers_see_complete_rows() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-sqlite-atomic-replay-{}-{}",
        std::process::id(),
        now_ms()
    ));
    fs::create_dir_all(&root).unwrap();
    let database = root.join("observability.sqlite");
    let connection = Connection::open(&database).unwrap();
    migrate(&connection).unwrap();
    let original = vec![replay_test_event(1), replay_test_event(2)];
    replay_ledger_events(&connection, &original, &TEST_LEDGER).unwrap();
    let original_rows = ledger_projection_rows(&connection);
    drop(connection);

    let replacement = vec![
        replay_test_event(10),
        replay_test_event(11),
        replay_test_event(12),
    ];
    let pause_dir = root.join("pause");
    std::env::set_var("RPOTATO_TEST_SQLITE_REPLAY_PAUSE_DIR", &pause_dir);
    let replay_database = database.clone();
    let replay_events = replacement.clone();
    let replay = std::thread::spawn(move || {
        let connection = Connection::open(replay_database).unwrap();
        replay_ledger_events(&connection, &replay_events, &TEST_LEDGER)
    });
    let entered = pause_dir.join("after-clear.entered");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !entered.exists() {
        assert!(
            Instant::now() < deadline,
            "sqlite replay pause 진입 timeout"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let reader = open_read_only_path(&database).unwrap();
    assert_eq!(ledger_projection_rows(&reader), original_rows);
    fs::write(pause_dir.join("after-clear.release"), b"release").unwrap();
    replay.join().unwrap().unwrap();
    std::env::remove_var("RPOTATO_TEST_SQLITE_REPLAY_PAUSE_DIR");
    assert_eq!(ledger_projection_rows(&reader), original_rows);
    drop(reader);
    let reader = open_read_only_path(&database).unwrap();
    let replacement_rows = ledger_projection_rows(&reader);
    assert_eq!(replacement_rows.len(), replacement.len());
    assert_ne!(replacement_rows, original_rows);
    drop(reader);

    let connection = Connection::open(&database).unwrap();
    for point in ["after-clear", "after-first-event"] {
        std::env::set_var("RPOTATO_TEST_SQLITE_REPLAY_FAULT", point);
        let error = replay_ledger_events(&connection, &original, &TEST_LEDGER).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_SQLITE_REPLAY_FAULT");
        assert!(error.message.contains(point));
        assert_eq!(
            ledger_projection_rows(&connection),
            replacement_rows,
            "fault point: {point}"
        );
    }

    drop(connection);
    let _ = fs::remove_dir_all(root);
}
