use crate::api::{DataEnvelope, EnvelopeMetadata, ObserverError};
pub use warframe_model::{StarChartDifficulty, StarChartNodeProgress};

/// Complete account star chart replacement with subscription freshness metadata.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct WarframeStarChart {
    pub metadata: EnvelopeMetadata,
    pub account_id: String,
    /// Retained completed nodes, ordered by canonical node tag.
    pub nodes: Vec<StarChartNodeProgress>,
}

impl From<crate::raw::TypedData<warframe_model::StarChartSnapshot>> for WarframeStarChart {
    fn from(value: crate::raw::TypedData<warframe_model::StarChartSnapshot>) -> Self {
        let (account_id, nodes) = value.data.into_parts();
        Self {
            metadata: value.metadata.into(),
            account_id: account_id.into(),
            nodes,
        }
    }
}

impl TryFrom<DataEnvelope> for WarframeStarChart {
    type Error = ObserverError;
    fn try_from(envelope: DataEnvelope) -> Result<Self, Self::Error> {
        Ok(
            crate::raw::decode_snapshot::<crate::warframe::StarChartTopic>(envelope.try_into()?)?
                .into(),
        )
    }
}

#[boltffi::data(impl)]
impl WarframeStarChart {
    /// Checks retained Normal completion credit; absence does not imply a locked node.
    #[must_use]
    pub fn normal_completed(&self, node_key: &str) -> bool {
        self.node(node_key).is_some()
    }

    /// Checks the game's retained Steel Path completion flag.
    #[must_use]
    pub fn steel_path_completed(&self, node_key: &str) -> bool {
        self.node(node_key)
            .is_some_and(|node| node.steel_path_completed)
    }

    /// Converts a generic envelope after validating topic identity and progression.
    /// # Errors
    /// Rejects wrong game/topic/schema, invalid metadata or malformed progression.
    pub fn from_envelope(envelope: DataEnvelope) -> Result<Self, ObserverError> {
        envelope.try_into()
    }
}

impl WarframeStarChart {
    fn node(&self, node_key: &str) -> Option<&StarChartNodeProgress> {
        self.nodes
            .binary_search_by(|node| node.node_key.as_str().cmp(node_key))
            .ok()
            .map(|index| &self.nodes[index])
    }
}
