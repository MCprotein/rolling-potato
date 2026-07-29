include!("observability/boundaries.rs");
include!("observability/runtime.rs");
include!("observability/sqlite.rs");

#[test]
fn v0377_observability_ports_own_projection_and_monitoring_boundaries() {
    assert_runtime_observability_owners();
    assert_sqlite_observability_projection();
    assert_canonical_observability_boundaries();
}
