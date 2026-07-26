use std::collections::BTreeMap;
use std::time::Duration;

use super::{
    BrowserAction, BrowserActionBlock, BrowserActionLimit, BrowserKey, BrowserObservation,
    ElementHandle, ElementRole, ObservedElement, ScrollDirection,
};

const MAX_OBSERVED_ELEMENTS: usize = 128;
const MAX_ELEMENT_NAME_CHARS: usize = 160;
const MAX_NAVIGATION_URL_BYTES: usize = 2_048;
const MAX_TYPED_TEXT_CHARS: usize = 1_000;
const MAX_EXTRACT_CHARS_PER_ACTION: usize = 8 * 1024;
const MAX_SCROLL_VIEWPORTS: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserActionBudget {
    max_actions: u8,
    max_observations: u8,
    max_interactions: u8,
    max_extracted_chars: usize,
    max_elapsed: Duration,
}

impl Default for BrowserActionBudget {
    fn default() -> Self {
        Self {
            max_actions: 12,
            max_observations: 4,
            max_interactions: 6,
            max_extracted_chars: 16 * 1024,
            max_elapsed: Duration::from_secs(45),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedTargetSeed {
    pub(crate) target_ref: u64,
    pub(crate) role: ElementRole,
    pub(crate) name: String,
    pub(crate) disabled: bool,
    pub(crate) sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedBrowserTarget {
    pub(crate) target_ref: u64,
    pub(crate) role: ElementRole,
    pub(crate) name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AdmittedBrowserAction {
    Navigate {
        url: String,
    },
    Observe,
    Click {
        target: ResolvedBrowserTarget,
    },
    Type {
        target: ResolvedBrowserTarget,
        text: String,
    },
    Press {
        key: BrowserKey,
    },
    Scroll {
        direction: ScrollDirection,
        viewports: u8,
    },
    Extract {
        max_chars: usize,
    },
    Screenshot,
    Close,
}

#[derive(Debug, Clone)]
struct StoredTarget {
    target_ref: u64,
    role: ElementRole,
    name: String,
    disabled: bool,
    sensitive: bool,
}

#[derive(Debug)]
pub(crate) struct BrowserInteractionSession {
    budget: BrowserActionBudget,
    revision: u64,
    actions: u8,
    observations: u8,
    interactions: u8,
    extracted_chars: usize,
    targets: BTreeMap<ElementHandle, StoredTarget>,
    closed: bool,
}

impl Default for BrowserInteractionSession {
    fn default() -> Self {
        Self::new(BrowserActionBudget::default())
    }
}

impl BrowserInteractionSession {
    pub(crate) fn new(budget: BrowserActionBudget) -> Self {
        Self {
            budget,
            revision: 0,
            actions: 0,
            observations: 0,
            interactions: 0,
            extracted_chars: 0,
            targets: BTreeMap::new(),
            closed: false,
        }
    }

    pub(crate) fn admit(
        &mut self,
        action: BrowserAction,
        elapsed: Duration,
    ) -> Result<AdmittedBrowserAction, BrowserActionBlock> {
        if self.closed {
            return Err(BrowserActionBlock::Closed);
        }
        if elapsed >= self.budget.max_elapsed {
            return Err(BrowserActionBlock::BudgetReached(
                BrowserActionLimit::Elapsed,
            ));
        }
        if self.actions >= self.budget.max_actions {
            return Err(BrowserActionBlock::BudgetReached(
                BrowserActionLimit::Actions,
            ));
        }
        self.actions += 1;

        let admitted = match action {
            BrowserAction::Navigate { url } => {
                let url = url.trim();
                if url.is_empty()
                    || url.len() > MAX_NAVIGATION_URL_BYTES
                    || url.chars().any(char::is_control)
                {
                    return Err(BrowserActionBlock::InvalidAction);
                }
                AdmittedBrowserAction::Navigate {
                    url: url.to_string(),
                }
            }
            BrowserAction::Observe => {
                if self.observations >= self.budget.max_observations {
                    return Err(BrowserActionBlock::BudgetReached(
                        BrowserActionLimit::Observations,
                    ));
                }
                self.observations += 1;
                AdmittedBrowserAction::Observe
            }
            BrowserAction::Click { handle } => {
                self.reserve_interaction()?;
                let target = self.resolve_target(&handle)?;
                if target.disabled
                    || target.sensitive
                    || !matches!(
                        target.role,
                        ElementRole::Button
                            | ElementRole::Link
                            | ElementRole::Checkbox
                            | ElementRole::Radio
                    )
                {
                    return Err(BrowserActionBlock::ForbiddenTarget);
                }
                AdmittedBrowserAction::Click {
                    target: resolved(target),
                }
            }
            BrowserAction::Type { handle, text } => {
                self.reserve_interaction()?;
                if text.is_empty()
                    || text.chars().count() > MAX_TYPED_TEXT_CHARS
                    || text.chars().any(char::is_control)
                {
                    return Err(BrowserActionBlock::InvalidAction);
                }
                let target = self.resolve_target(&handle)?;
                if target.disabled
                    || target.sensitive
                    || !matches!(target.role, ElementRole::SearchBox | ElementRole::TextField)
                {
                    return Err(BrowserActionBlock::ForbiddenTarget);
                }
                AdmittedBrowserAction::Type {
                    target: resolved(target),
                    text,
                }
            }
            BrowserAction::Press { key } => {
                self.reserve_interaction()?;
                AdmittedBrowserAction::Press { key }
            }
            BrowserAction::Scroll {
                direction,
                viewports,
            } => {
                self.reserve_interaction()?;
                if !(1..=MAX_SCROLL_VIEWPORTS).contains(&viewports) {
                    return Err(BrowserActionBlock::InvalidAction);
                }
                AdmittedBrowserAction::Scroll {
                    direction,
                    viewports,
                }
            }
            BrowserAction::Extract { max_chars } => {
                if max_chars == 0
                    || max_chars > MAX_EXTRACT_CHARS_PER_ACTION
                    || self.extracted_chars.saturating_add(max_chars)
                        > self.budget.max_extracted_chars
                {
                    return Err(BrowserActionBlock::BudgetReached(
                        BrowserActionLimit::ExtractedText,
                    ));
                }
                self.extracted_chars += max_chars;
                AdmittedBrowserAction::Extract { max_chars }
            }
            BrowserAction::Screenshot => AdmittedBrowserAction::Screenshot,
            BrowserAction::Close => {
                self.closed = true;
                self.targets.clear();
                AdmittedBrowserAction::Close
            }
        };
        Ok(admitted)
    }

    pub(crate) fn install_observation(
        &mut self,
        seeds: impl IntoIterator<Item = ObservedTargetSeed>,
    ) -> BrowserObservation {
        self.invalidate_handles();
        let mut elements = Vec::new();
        for seed in seeds.into_iter().take(MAX_OBSERVED_ELEMENTS) {
            let name = seed
                .name
                .trim()
                .chars()
                .take(MAX_ELEMENT_NAME_CHARS)
                .collect::<String>();
            if name.is_empty() {
                continue;
            }
            let handle = ElementHandle::issued(self.revision, elements.len() + 1);
            self.targets.insert(
                handle.clone(),
                StoredTarget {
                    target_ref: seed.target_ref,
                    role: seed.role,
                    name: name.clone(),
                    disabled: seed.disabled,
                    sensitive: seed.sensitive,
                },
            );
            elements.push(ObservedElement {
                handle,
                role: seed.role,
                name,
                disabled: seed.disabled,
            });
        }
        BrowserObservation {
            revision: self.revision,
            elements,
        }
    }

    pub(crate) fn invalidate_handles(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.targets.clear();
    }

    fn reserve_interaction(&mut self) -> Result<(), BrowserActionBlock> {
        if self.interactions >= self.budget.max_interactions {
            return Err(BrowserActionBlock::BudgetReached(
                BrowserActionLimit::Interactions,
            ));
        }
        self.interactions += 1;
        Ok(())
    }

    fn resolve_target(&self, handle: &ElementHandle) -> Result<&StoredTarget, BrowserActionBlock> {
        self.targets
            .get(handle)
            .ok_or(BrowserActionBlock::StaleHandle)
    }
}

fn resolved(target: &StoredTarget) -> ResolvedBrowserTarget {
    ResolvedBrowserTarget {
        target_ref: target.target_ref,
        role: target.role,
        name: target.name.clone(),
    }
}
