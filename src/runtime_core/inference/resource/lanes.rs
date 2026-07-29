use super::{ResourceLaneAdmission, ResourceLaneDecision, ResourcePressure};

pub fn team_lane_decision(
    pressure: ResourcePressure,
    requested_lanes: u32,
) -> ResourceLaneDecision {
    let requested_lanes = requested_lanes.max(1);
    match pressure {
        ResourcePressure::Normal => ResourceLaneDecision {
            pressure,
            requested_lanes,
            admitted_lanes: requested_lanes,
            admission: ResourceLaneAdmission::AllowParallel,
            fallback: "none",
            reason: "resource pressure normal",
            hint: "parallel team lanes may proceed within file ownership, tool risk, and approval limits",
        },
        ResourcePressure::Unknown => ResourceLaneDecision {
            pressure,
            requested_lanes,
            admitted_lanes: 1,
            admission: ResourceLaneAdmission::SequentialFallback,
            fallback: "sequential",
            reason: "resource pressure unknown",
            hint: "resource sample is missing or incomplete, so dispatch should stay sequential until telemetry exists",
        },
        ResourcePressure::Degraded => ResourceLaneDecision {
            pressure,
            requested_lanes,
            admitted_lanes: 1,
            admission: ResourceLaneAdmission::SequentialFallback,
            fallback: "sequential",
            reason: "degraded resource pressure",
            hint: "run subagents sequentially or reduce backend/model/context pressure before parallel dispatch",
        },
        ResourcePressure::Critical => ResourceLaneDecision {
            pressure,
            requested_lanes,
            admitted_lanes: 0,
            admission: ResourceLaneAdmission::Blocked,
            fallback: "wait",
            reason: "critical resource pressure",
            hint: "do not dispatch new team lanes until backend status recovers or host load is reduced",
        },
    }
}
