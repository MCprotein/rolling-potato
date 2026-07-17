use super::*;

#[cfg(unix)]
#[test]
fn source_recovery_rejects_oversized_transaction_before_parsing() {
    let root = workflow_test_root("source-recovery-read-bound");
    fs::create_dir_all(&root).unwrap();
    let transaction = root.join("oversized-source-transaction.json");
    fs::write(
        &transaction,
        vec![b'x'; usize::try_from(MAX_PREPARED_SOURCE_BUNDLE_BYTES).unwrap() + 1],
    )
    .unwrap();

    let error = recover_source_replace(&transaction).unwrap_err();

    assert!(error.message.contains("regular-file/byte budget"));
    assert!(transaction.exists());
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn source_recovery_rejects_artifacts_outside_target_parent() {
    let root = std::env::temp_dir().join(format!(
        "rpotato-source-recovery-parent-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let source_dir = root.join("source");
    let outside_dir = root.join("outside");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&outside_dir).unwrap();
    let target = source_dir.join("lib.rs");
    let victim = outside_dir.join("victim.rs");
    let temporary = source_dir.join(".lib.rs.rpotato-new-1.2");
    let transaction = root.join("legacy-source-record");
    fs::write(&target, b"original").unwrap();
    fs::write(&victim, b"must-survive").unwrap();
    fs::write(&temporary, b"replacement").unwrap();
    fs::write(
            &transaction,
            format!(
                "schema_version=1\nintent_id=intent-source-boundary\ntarget={}\nguard={}\ntemporary={}\nexpected_current_hash={}\nexpected_replacement_hash={}\noperations={}\n",
                target.display(),
                victim.display(),
                temporary.display(),
                sha256_bytes(b"original"),
                sha256_bytes(b"replacement"),
                transition::SOURCE_INSTALL_OPERATIONS.join(",")
            ),
        )
        .unwrap();

    let error = recover_source_replace(&transaction).unwrap_err();
    assert!(error.message.contains("strict JSON") || error.message.contains("source_install"));
    assert_eq!(fs::read(&victim).unwrap(), b"must-survive");
    assert!(transaction.exists());
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn source_recovery_rejects_mismatched_artifact_nonce() {
    let root = std::env::temp_dir().join(format!(
        "rpotato-source-recovery-nonce-{}-{}",
        std::process::id(),
        now_ms()
    ));
    fs::create_dir_all(&root).unwrap();
    let target = root.join("lib.rs");
    let guard = root.join(".lib.rs.rpotato-guard-1.2");
    let temporary = root.join(".lib.rs.rpotato-new-1.3");
    let transaction = root.join("legacy-source-record");
    fs::write(&target, b"original").unwrap();
    fs::write(&guard, b"must-survive").unwrap();
    fs::write(&temporary, b"replacement").unwrap();
    fs::write(
            &transaction,
            format!(
                "schema_version=1\nintent_id=intent-source-mismatch\ntarget={}\nguard={}\ntemporary={}\nexpected_current_hash={}\nexpected_replacement_hash={}\noperations={}\n",
                target.display(),
                guard.display(),
                temporary.display(),
                sha256_bytes(b"original"),
                sha256_bytes(b"replacement"),
                transition::SOURCE_INSTALL_OPERATIONS.join(",")
            ),
        )
        .unwrap();

    let error = recover_source_replace(&transaction).unwrap_err();
    assert!(error.message.contains("strict JSON") || error.message.contains("source_install"));
    assert_eq!(fs::read(&guard).unwrap(), b"must-survive");
    assert!(transaction.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn source_identity_v1_matches_independent_golden_and_rejects_tamper() {
    let content_hash = "473b0fef5f0626d3fe806f10b931f085d511ba15b1117c53d5f2ec27d5b9452e";
    assert_eq!(sha256_bytes(b"current source\n"), content_hash);
    assert_eq!(
        transition::source_identity_v1(0x0102_0304_0506_0708, 0x1112_1314_1516_1718, content_hash,)
            .unwrap(),
        "2b3452be6ffa18621fcd39e56162e5b46ef9428657dd6cdc9e02847e521420d0"
    );
    assert!(transition::source_identity_v1(
        0x0102_0304_0506_0708,
        0x1112_1314_1516_1718,
        &content_hash.to_ascii_uppercase()
    )
    .is_err());
    assert_ne!(
        transition::source_identity_v1(0x0102_0304_0506_0709, 0x1112_1314_1516_1718, content_hash,)
            .unwrap(),
        "2b3452be6ffa18621fcd39e56162e5b46ef9428657dd6cdc9e02847e521420d0"
    );
}
