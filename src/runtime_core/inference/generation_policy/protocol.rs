//! Versioned completion capacities owned by machine-readable protocols.

use super::{ActiveTokenCapacity, PolicyValueSourceKind, VersionedValueSource};

const STRUCTURED_TOOL_ROUTE_MAX_TOKENS: u32 = 768;
const STRUCTURED_TOOL_ROUTE_CONTRACT_VERSION: &str = "structured-tool-route-v1";

pub(crate) fn structured_tool_route_capacity() -> ActiveTokenCapacity {
    ActiveTokenCapacity::new(
        STRUCTURED_TOOL_ROUTE_MAX_TOKENS,
        VersionedValueSource::new(
            PolicyValueSourceKind::ProtocolContract,
            STRUCTURED_TOOL_ROUTE_CONTRACT_VERSION,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_tool_route_cap_is_versioned_by_its_protocol_owner() {
        let capacity = structured_tool_route_capacity();

        assert_eq!(capacity.tokens, 768);
        assert_eq!(
            capacity.source.kind,
            PolicyValueSourceKind::ProtocolContract
        );
        assert_eq!(capacity.source.version, "structured-tool-route-v1");
    }
}
