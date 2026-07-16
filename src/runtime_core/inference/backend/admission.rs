use std::collections::BTreeSet;

use super::lifecycle::BackendGenerationRecord;

pub(crate) struct GenerationAdmission {
    active_generation_ids: BTreeSet<String>,
    primary_generation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GenerationRelease {
    Untracked,
    Retained,
    Last { primary_generation_id: String },
}

impl GenerationAdmission {
    pub(crate) const fn new() -> Self {
        Self {
            active_generation_ids: BTreeSet::new(),
            primary_generation_id: None,
        }
    }

    pub(crate) fn can_join(
        &self,
        active: &BackendGenerationRecord,
        client_pid: u32,
        sidecar_pid: u32,
    ) -> bool {
        active.client_pid == client_pid
            && active.sidecar_pid == sidecar_pid
            && !self.active_generation_ids.is_empty()
            && self.primary_generation_id.as_deref() == Some(active.generation_id.as_str())
    }

    pub(crate) fn register(&mut self, generation_id: String, publish_primary: bool) -> bool {
        if publish_primary {
            self.primary_generation_id = Some(generation_id.clone());
        }
        if self.active_generation_ids.insert(generation_id) {
            true
        } else {
            if publish_primary {
                self.primary_generation_id = None;
            }
            false
        }
    }

    pub(crate) fn release(
        &mut self,
        generation_id: &str,
    ) -> Result<GenerationRelease, &'static str> {
        if self.active_generation_ids.is_empty() {
            return Ok(GenerationRelease::Untracked);
        }
        if !self.active_generation_ids.remove(generation_id) {
            return Err("backend generation admission release binding 누락");
        }
        if !self.active_generation_ids.is_empty() {
            return Ok(GenerationRelease::Retained);
        }
        Ok(GenerationRelease::Last {
            primary_generation_id: self
                .primary_generation_id
                .take()
                .unwrap_or_else(|| generation_id.to_string()),
        })
    }

    pub(crate) fn cancellation_applies(
        &self,
        cancel_generation_id: &str,
        generation_id: &str,
    ) -> bool {
        cancel_generation_id == generation_id
            || (self.primary_generation_id.as_deref() == Some(cancel_generation_id)
                && self.active_generation_ids.contains(generation_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generation(id: &str) -> BackendGenerationRecord {
        BackendGenerationRecord {
            generation_id: id.to_string(),
            client_pid: 10,
            sidecar_pid: 20,
            started_at_ms: 30,
            timeout_ms: 40,
            streaming_display: false,
        }
    }

    #[test]
    fn primary_binding_controls_join_and_group_cancellation() {
        let mut admission = GenerationAdmission::new();
        let primary = generation("primary");

        assert!(!admission.can_join(&primary, 10, 20));
        assert!(admission.register(primary.generation_id.clone(), true));
        assert!(admission.can_join(&primary, 10, 20));
        assert!(!admission.can_join(&primary, 11, 20));
        assert!(admission.register("secondary".to_string(), false));
        assert!(admission.cancellation_applies("primary", "secondary"));
        assert!(!admission.cancellation_applies("other", "secondary"));
    }

    #[test]
    fn release_retains_state_until_the_last_generation() {
        let mut admission = GenerationAdmission::new();
        assert_eq!(
            admission.release("untracked").unwrap(),
            GenerationRelease::Untracked
        );
        assert!(admission.register("primary".to_string(), true));
        assert!(admission.register("secondary".to_string(), false));
        assert_eq!(
            admission.release("secondary").unwrap(),
            GenerationRelease::Retained
        );
        assert_eq!(
            admission.release("primary").unwrap(),
            GenerationRelease::Last {
                primary_generation_id: "primary".to_string()
            }
        );
    }

    #[test]
    fn duplicate_and_unknown_release_fail_closed() {
        let mut admission = GenerationAdmission::new();
        assert!(admission.register("primary".to_string(), true));
        assert!(!admission.register("primary".to_string(), false));
        assert_eq!(
            admission.release("unknown"),
            Err("backend generation admission release binding 누락")
        );
    }
}
