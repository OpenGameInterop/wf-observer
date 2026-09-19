use crate::api::{DataEnvelope, EnvelopeMetadata, ObserverError};

/// Complete account mastery replacement with subscription freshness metadata.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct WarframeMastery {
    pub metadata: EnvelopeMetadata,
    /// Owning account, encoded as 24 lowercase hexadecimal characters.
    pub account_id: String,
    /// Completed mastery rank reported by the game.
    pub rank: u32,
    pub total_points: u64,
    /// Game-calculated mastery points from item progression.
    pub item_points: u64,
    /// Combined Normal and Steel Path mission/Junction mastery.
    pub mission_points: u64,
    /// Railjack mastery, including credit retained after a respec.
    pub railjack_intrinsic_points: u64,
    pub drifter_intrinsic_points: u64,
    /// Unique canonical item paths in ascending order.
    pub items: Vec<MasteryItemProgress>,
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct MasteryItemProgress {
    pub item_key: String,
    /// Raw retained affinity, including zero and values above maximum-rank thresholds.
    /// Exact unsigned integer; JVM callers can use `Long.toUnsignedString` above
    /// `Long.MAX_VALUE`, and other bindings expose native unsigned/exact integers.
    pub affinity: u64,
}

impl From<crate::raw::TypedData<warframe_model::MasterySnapshot>> for WarframeMastery {
    fn from(value: crate::raw::TypedData<warframe_model::MasterySnapshot>) -> Self {
        let (account_id, rank, total_points, breakdown, items) = value.data.into_parts();
        Self {
            metadata: value.metadata.into(),
            account_id: account_id.into(),
            rank,
            total_points,
            item_points: breakdown.item_points,
            mission_points: breakdown.mission_points,
            railjack_intrinsic_points: breakdown.railjack_intrinsic_points,
            drifter_intrinsic_points: breakdown.drifter_intrinsic_points,
            items: items
                .into_iter()
                .map(|item| MasteryItemProgress {
                    item_key: item.item_key.into(),
                    affinity: item.affinity,
                })
                .collect(),
        }
    }
}

impl TryFrom<DataEnvelope> for WarframeMastery {
    type Error = ObserverError;

    fn try_from(envelope: DataEnvelope) -> Result<Self, Self::Error> {
        Ok(
            crate::raw::decode_snapshot::<crate::warframe::MasteryTopic>(envelope.try_into()?)?
                .into(),
        )
    }
}

#[boltffi::data(impl)]
impl WarframeMastery {
    /// Converts a generic envelope. Typed reads and watches already return this value.
    ///
    /// # Errors
    /// Rejects the wrong game/topic/schema, invalid metadata, and malformed progression.
    pub fn from_envelope(envelope: DataEnvelope) -> Result<Self, ObserverError> {
        envelope.try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{SessionRef, TopicRef, TopicSource};
    use warframe_model::{AccountId, ItemKey, MasteryPointBreakdown, MasterySnapshot};

    #[test]
    fn mastery_envelopes_preserve_exact_values_and_validate_identity() -> anyhow::Result<()> {
        let snapshot = MasterySnapshot::new(
            AccountId::new("0123456789abcdef01234567")?,
            34,
            u64::MAX,
            MasteryPointBreakdown {
                mission_points: u64::MAX,
                ..Default::default()
            },
            vec![warframe_model::MasteryItemProgress {
                item_key: ItemKey::new("/Lotus/A")?,
                affinity: u64::MAX,
            }],
        )?;
        let envelope = DataEnvelope {
            metadata: EnvelopeMetadata {
                source: TopicSource {
                    session: SessionRef {
                        run_id: "run".into(),
                        session_id: "session".into(),
                    },
                    game_id: "warframe".into(),
                    topic: TopicRef {
                        provider_id: "opengameinterop.warframe".into(),
                        topic: "warframe.mastery".into(),
                        schema_version: 1,
                    },
                },
                generation: u64::MAX.to_string(),
                sequence: u64::MAX.to_string(),
            },
            payload_json: serde_json::to_string(&snapshot)?,
        };
        let value = WarframeMastery::from_envelope(envelope.clone())?;
        assert_eq!(value.metadata, envelope.metadata);
        assert_eq!(
            (value.rank, value.total_points, value.item_points),
            (34, u64::MAX, 0)
        );
        assert_eq!(value.items[0].affinity, u64::MAX);
        for case in 0..7 {
            let mut bad = envelope.clone();
            match case {
                0 => bad.metadata.source.topic.topic = "warframe.inventory".into(),
                1 => bad.metadata.source.topic.schema_version += 1,
                2 => bad.metadata.source.topic.provider_id = "unrelated".into(),
                3 => bad.metadata.source.game_id = "unrelated".into(),
                4 => bad.payload_json = "{}".into(),
                5 => bad.metadata.generation = "01".into(),
                _ => bad.metadata.sequence = "-1".into(),
            }
            assert!(WarframeMastery::from_envelope(bad).is_err());
        }
        Ok(())
    }
}
