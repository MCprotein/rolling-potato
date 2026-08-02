use super::*;
#[path = "inference/application_backend_chat.rs"]
mod application_backend_chat;
include!("inference/application_backend.rs");
include!("inference/application_model.rs");
include!("inference/generation_policy.rs");
include!("inference/model_codec.rs");
include!("inference/model_manifest.rs");
include!("inference/runtime_owners.rs");
#[test]
fn v0373_inference_owners_replace_legacy_domain_and_adapter_slices() {
    let facade = fs::read_to_string("tests/architecture_contract/inference.rs").unwrap();
    let owners = [
        "application_backend",
        "application_model",
        "generation_policy",
        "model_codec",
        "model_manifest",
        "runtime_owners",
    ];
    for owner in owners {
        assert!(
            facade.contains(&format!("include!(\"inference/{owner}.rs\");")),
            "inference architecture facade does not register {owner}"
        );
        let owner_path = format!("tests/architecture_contract/inference/{owner}.rs");
        let source = fs::read_to_string(&owner_path).unwrap();
        assert!(
            source.lines().count() < 400,
            "inference architecture owner regrew beyond its boundary: {owner_path}"
        );
    }
    assert!(facade.lines().count() < 45, "inference facade too large");
    assert_runtime_inference_owners();
    assert_application_backend_owners();
    assert_application_model_owners();
    assert_model_codec_owners();
    assert_model_manifest_owners();
}
