//! Retained node completion. Node eligibility and display names belong to game metadata.
use crate::AccountId;

/// One retained completed node. Presence records Normal-path completion credit.
/// This includes Junctions and any other node tags retained by the game; a record
/// does not imply that the node awards mastery or is required for an unlock.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct StarChartNodeProgress {
    /// Canonical game node tag, for example `SolNode1`; not a localized name.
    pub node_key: String,
    /// Native completion count across difficulties, not a per-difficulty count.
    pub completions: u32,
    /// The game has recorded completion at Steel Path difficulty or higher.
    pub steel_path_completed: bool,
}

#[boltffi::data]
#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub enum StarChartDifficulty {
    Normal,
    SteelPath,
}

/// Complete replacement of retained node records, uniquely ordered by node key.
/// Absence means no completion record, and says nothing about node accessibility.
#[derive(Debug, Clone, ..Eq, ..Serde)]
#[serde(try_from = "UncheckedSnapshot")]
pub struct StarChartSnapshot {
    account_id: AccountId,
    nodes: Vec<StarChartNodeProgress>,
}

#[derive(serde::Deserialize)]
struct UncheckedSnapshot {
    account_id: AccountId,
    nodes: Vec<StarChartNodeProgress>,
}

#[derive(Debug, derive_more::Error, displaydoc::Display, ..Copy, ..Eq)]
pub enum InvalidStarChart {
    /// node tags must contain between 1 and 255 visible ASCII characters
    NodeKey,
    /// node tags must be unique and ordered
    NodeOrder,
}

impl TryFrom<UncheckedSnapshot> for StarChartSnapshot {
    type Error = InvalidStarChart;
    fn try_from(value: UncheckedSnapshot) -> Result<Self, Self::Error> {
        Self::new(value.account_id, value.nodes)
    }
}

impl StarChartSnapshot {
    /// Validates a replacement. An empty completion history is valid.
    /// # Errors
    /// Rejects invalid node tags or duplicate/unordered keys.
    pub fn new(
        account_id: AccountId,
        nodes: Vec<StarChartNodeProgress>,
    ) -> Result<Self, InvalidStarChart> {
        if nodes.iter().any(|node| {
            node.node_key.is_empty()
                || node.node_key.len() > 255
                || !node.node_key.bytes().all(|byte| byte.is_ascii_graphic())
        }) {
            return Err(InvalidStarChart::NodeKey);
        }
        if nodes
            .windows(2)
            .any(|pair| pair[0].node_key >= pair[1].node_key)
        {
            return Err(InvalidStarChart::NodeOrder);
        }
        Ok(Self { account_id, nodes })
    }

    #[must_use]
    pub fn account_id(&self) -> &AccountId {
        &self.account_id
    }
    #[must_use]
    pub fn nodes(&self) -> &[StarChartNodeProgress] {
        &self.nodes
    }
    #[must_use]
    pub fn get(&self, node_key: &str) -> Option<&StarChartNodeProgress> {
        self.nodes
            .binary_search_by(|node| node.node_key.as_str().cmp(node_key))
            .ok()
            .map(|index| &self.nodes[index])
    }
    /// Checks retained completion credit for the requested difficulty.
    #[must_use]
    pub fn completed(&self, node_key: &str, difficulty: StarChartDifficulty) -> bool {
        self.get(node_key).is_some_and(|node| match difficulty {
            StarChartDifficulty::Normal => true,
            StarChartDifficulty::SteelPath => node.steel_path_completed,
        })
    }
    #[must_use]
    pub fn into_parts(self) -> (AccountId, Vec<StarChartNodeProgress>) {
        (self.account_id, self.nodes)
    }
}
