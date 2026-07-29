//! Cross-lane write ownership admission.

use std::collections::HashMap;

use super::{OwnershipCheck, OwnershipClaim, OwnershipGate};

pub(crate) fn evaluate_ownership_gate(
    admitted_lanes: u32,
    claims: Vec<OwnershipClaim>,
) -> OwnershipGate {
    if claims.is_empty() {
        return OwnershipGate {
            status: "not-requested",
            checks: Vec::new(),
        };
    }

    let mut owners: HashMap<String, u32> = HashMap::new();
    let mut checks = Vec::new();
    for claim in claims {
        let mut status = "assigned";
        let mut reason = "write path assigned to lane before dispatch".to_string();
        if claim.lane > admitted_lanes {
            status = "invalid";
            reason = format!(
                "lane {} exceeds admitted lanes {admitted_lanes}; reduce lanes or wait for resources",
                claim.lane
            );
        } else if let Some(existing_lane) = owners.get(&claim.normalized_path) {
            if *existing_lane != claim.lane {
                status = "conflict";
                reason = format!(
                    "path already owned by lane {existing_lane}; cross-lane writes are blocked"
                );
            }
        } else {
            owners.insert(claim.normalized_path.clone(), claim.lane);
        }
        checks.push(OwnershipCheck {
            lane: claim.lane,
            raw_path: claim.raw_path,
            normalized_path: claim.normalized_path,
            status,
            reason,
        });
    }

    let status = if checks.iter().any(|check| check.status == "conflict") {
        "conflict"
    } else if checks.iter().any(|check| check.status == "invalid") {
        "invalid"
    } else {
        "allocated"
    };
    OwnershipGate { status, checks }
}
