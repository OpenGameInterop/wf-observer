use crate::api::{DataEnvelope, EnvelopeMetadata, ObserverError};

pub use warframe_model::CurrencyBalances;

/// Complete account currency replacement with the standard subscription metadata.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct WarframeCurrencies {
    pub metadata: EnvelopeMetadata,
    /// Owning game account, encoded as 24 lowercase hexadecimal characters.
    pub account_id: String,
    pub balances: CurrencyBalances,
}

impl From<crate::raw::TypedData<warframe_model::CurrencySnapshot>> for WarframeCurrencies {
    fn from(value: crate::raw::TypedData<warframe_model::CurrencySnapshot>) -> Self {
        Self {
            metadata: value.metadata.into(),
            account_id: value.data.account_id.into(),
            balances: value.data.balances,
        }
    }
}

impl TryFrom<DataEnvelope> for WarframeCurrencies {
    type Error = ObserverError;

    fn try_from(envelope: DataEnvelope) -> Result<Self, Self::Error> {
        Ok(
            crate::raw::decode_snapshot::<crate::warframe::CurrenciesTopic>(envelope.try_into()?)?
                .into(),
        )
    }
}

#[boltffi::data(impl)]
impl WarframeCurrencies {
    /// Builds a typed value from a generic envelope. Reads and watches already
    /// return typed values, so they do not require this conversion.
    ///
    /// # Errors
    /// Rejects the wrong game, topic or schema, invalid metadata, and malformed data.
    pub fn from_envelope(envelope: DataEnvelope) -> Result<Self, ObserverError> {
        envelope.try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{SessionRef, TopicRef, TopicSource};
    use warframe_model::{AccountId, CurrencySnapshot};

    #[test]
    fn currency_decoding_preserves_balances_and_rejects_wrong_envelopes() -> anyhow::Result<()> {
        let balances = CurrencyBalances {
            credits: i32::MAX,
            endo: 0,
            tradable_platinum: i32::MIN,
            non_tradable_platinum: 50,
        };
        let original = DataEnvelope {
            metadata: EnvelopeMetadata {
                source: TopicSource {
                    session: SessionRef {
                        run_id: "run".into(),
                        session_id: "session".into(),
                    },
                    game_id: "warframe".into(),
                    topic: TopicRef {
                        provider_id: "opengameinterop.warframe".into(),
                        topic: "warframe.currencies".into(),
                        schema_version: 1,
                    },
                },
                generation: u64::MAX.to_string(),
                sequence: u64::MAX.to_string(),
            },
            payload_json: serde_json::to_string(&CurrencySnapshot {
                account_id: AccountId::new("0123456789abcdef01234567")?,
                balances,
            })?,
        };
        let decoded = WarframeCurrencies::try_from(original.clone())?;
        assert_eq!(decoded.balances, balances);
        assert_eq!(decoded.metadata, original.metadata);
        assert_eq!(decoded.account_id, "0123456789abcdef01234567");
        for case in 0..6 {
            let mut bad = original.clone();
            match case {
                0 => bad.metadata.source.topic.topic = "warframe.inventory".into(),
                1 => bad.metadata.source.topic.schema_version += 1,
                2 => bad.metadata.source.topic.provider_id = "unrelated".into(),
                3 => bad.metadata.source.game_id = "unrelated".into(),
                4 => bad.payload_json = "{}".into(),
                _ => bad.metadata.generation = "01".into(),
            }
            assert!(WarframeCurrencies::from_envelope(bad).is_err());
        }
        Ok(())
    }
}
