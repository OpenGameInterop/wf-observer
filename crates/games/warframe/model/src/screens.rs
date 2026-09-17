//! Visible top-level interface movies, independent of account and mission state.

/// Known interface assets. Several may be visible at once.
#[boltffi::data]
#[derive(Debug, Clone, Hash, ..Ord, ..Serde)]
pub enum Screen {
    Loadout,
    Navigation,
    Progress,
    RelicRewards,
    /// An interface movie not yet assigned a named variant.
    Other {
        asset_path: String,
    },
}

impl Screen {
    #[must_use]
    pub fn from_asset_path(asset_path: String) -> Self {
        match asset_path.as_str() {
            "/Lotus/Interface/LoadOutRedux.swf" => Self::Loadout,
            "/Lotus/Interface/MapRedux.swf" => Self::Navigation,
            "/Lotus/Interface/Progress.swf" => Self::Progress,
            "/Lotus/Interface/ProjectionRewardChoice.swf" => Self::RelicRewards,
            _ => Self::Other { asset_path },
        }
    }
}

/// Complete replacement. The collection is a set in deterministic order, not
/// focus or stacking order. An empty successful sample means no visible movies.
#[derive(Debug, Clone, ..Eq, ..Serde)]
#[serde(try_from = "ScreensPayload")]
pub struct ScreensSnapshot {
    pub screens: Vec<Screen>,
}

#[derive(serde::Deserialize)]
struct ScreensPayload {
    screens: Vec<Screen>,
}

impl TryFrom<ScreensPayload> for ScreensSnapshot {
    type Error = &'static str;

    #[allow(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "Warframe asset identifiers are case-sensitive, not filesystem paths"
    )]
    fn try_from(value: ScreensPayload) -> Result<Self, Self::Error> {
        if value.screens.len() > 128 {
            return Err("too many visible screens");
        }
        let screens = value
            .screens
            .into_iter()
            .map(|screen| {
                if let Screen::Other { asset_path } = screen {
                    if asset_path.len() > 255
                        || !asset_path.starts_with("/Lotus/Interface/")
                        || !asset_path.ends_with(".swf")
                        || asset_path.contains('\0')
                    {
                        return Err("invalid interface asset path");
                    }
                    Ok(Screen::from_asset_path(asset_path))
                } else {
                    Ok(screen)
                }
            })
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        Ok(Self {
            screens: screens.into_iter().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_screens_are_a_canonical_set_of_valid_assets()
    -> Result<(), Box<dyn std::error::Error>> {
        let value: ScreensSnapshot = serde_json::from_value(
            serde_json::json!({"screens": ["Navigation", "Navigation", {"Other": {"asset_path": "/Lotus/Interface/LoadOutRedux.swf"}}]}),
        )?;
        assert_eq!(value.screens, [Screen::Loadout, Screen::Navigation]);
        for path in [
            "/tmp/movie.swf",
            "/Lotus/Interface/Movie",
            "/Lotus/Interface/Bad\0.swf",
        ] {
            assert!(
                serde_json::from_value::<ScreensSnapshot>(
                    serde_json::json!({"screens": [{"Other": {"asset_path": path}}]})
                )
                .is_err()
            );
        }
        Ok(())
    }
}
