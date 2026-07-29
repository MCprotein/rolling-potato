#[test]
fn termination_fallback_forces_a_process_after_graceful_command_failure() {
    let calls = std::cell::RefCell::new(Vec::new());
    let running = std::cell::Cell::new(true);

    terminate_with_fallback(
        || {
            calls.borrow_mut().push("graceful");
            Err(AppError::runtime("graceful unsupported"))
        },
        || {
            calls.borrow_mut().push("force");
            running.set(false);
            Ok(())
        },
        || Ok(running.get()),
        || Ok(!running.get()),
        42,
    )
    .unwrap();

    assert_eq!(*calls.borrow(), ["graceful", "force"]);
    assert!(!running.get());
}

#[test]
fn termination_fallback_accepts_force_race_when_process_is_already_gone() {
    let running = std::cell::Cell::new(true);

    terminate_with_fallback(
        || Err(AppError::runtime("graceful unsupported")),
        || {
            running.set(false);
            Err(AppError::runtime("process already exited"))
        },
        || Ok(running.get()),
        || Ok(!running.get()),
        43,
    )
    .unwrap();

    assert!(!running.get());
}

#[test]
fn termination_fallback_fails_closed_when_liveness_check_fails() {
    let force_called = std::cell::Cell::new(false);

    let error = terminate_with_fallback(
        || Err(AppError::runtime("graceful unsupported")),
        || {
            force_called.set(true);
            Ok(())
        },
        || Err(AppError::runtime("liveness unavailable")),
        || Ok(false),
        44,
    )
    .unwrap_err();

    assert!(error.message.contains("liveness unavailable"));
    assert!(!force_called.get());
}
