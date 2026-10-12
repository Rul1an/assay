use super::StrictJsonError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BudgetTier {
    Hard,
    Selected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BudgetDimension {
    Depth,
    KeyBytes,
    NonPayloadStringBytes,
    PayloadStringBytes,
    EntryBytes,
    Items,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BudgetHit {
    pub(crate) dimension: BudgetDimension,
    pub(crate) tier: BudgetTier,
}

#[derive(Debug)]
pub(crate) enum BoundedJsonError {
    Strict(StrictJsonError),
    Budget(BudgetHit),
}

impl From<StrictJsonError> for BoundedJsonError {
    fn from(value: StrictJsonError) -> Self {
        Self::Strict(value)
    }
}

impl From<BudgetHit> for BoundedJsonError {
    fn from(value: BudgetHit) -> Self {
        Self::Budget(value)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BudgetLimit {
    pub(crate) hard: u64,
    pub(crate) selected: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BoundedJsonLimits {
    pub(crate) depth: BudgetLimit,
    pub(crate) key_bytes: BudgetLimit,
    pub(crate) non_payload_string_bytes: BudgetLimit,
    pub(crate) payload_string_bytes: BudgetLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathSegment {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringClass {
    Payload,
    NonPayload,
}

pub(crate) trait BoundedJsonHooks {
    fn object_start(&mut self, _path: &[PathSegment]) -> Result<Option<BudgetLimit>, BudgetHit> {
        Ok(None)
    }

    fn item_start(
        &mut self,
        path: &[PathSegment],
        index: usize,
    ) -> Result<Option<BudgetLimit>, BudgetHit>;

    fn string_class(&self, path: &[PathSegment]) -> StringClass;
}

pub(crate) fn check_next(
    previous: u64,
    delta: u64,
    limit: BudgetLimit,
    dimension: BudgetDimension,
) -> Result<u64, BudgetHit> {
    let next = previous.checked_add(delta).ok_or(BudgetHit {
        dimension,
        tier: BudgetTier::Hard,
    })?;
    if next > limit.hard {
        return Err(BudgetHit {
            dimension,
            tier: BudgetTier::Hard,
        });
    }
    if next > limit.selected {
        return Err(BudgetHit {
            dimension,
            tier: BudgetTier::Selected,
        });
    }
    Ok(next)
}
