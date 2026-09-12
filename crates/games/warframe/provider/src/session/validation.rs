//! Executable identity and independently cached layout checks for one attachment.

use crate::{
    matching,
    target::{BUILD, ExecutableFingerprint, FingerprintError, read_executable_fingerprint},
};
use memory_reader::ProcessMemory;
use provider_sdk::UnavailableReason;
use std::{fmt, time::Duration};

const VALIDATION_RETRY: Duration = Duration::from_secs(5);

/// Identifying an image does not establish compatibility with any topic.
#[derive(..Copy)]
pub(super) struct Executable {
    pub(super) base: u64,
    pub(super) actual: ExecutableFingerprint,
}

/// Failures carry their own deadline so another topic's polling cannot accelerate retries.
#[derive(Clone, Debug)]
pub(super) struct Retry {
    pub(super) reason: UnavailableReason,
    pub(super) at: Duration,
}

#[derive(Default)]
pub(super) enum CachedCheck<T = ()> {
    #[default]
    Unchecked,
    Passed(T),
    Failed(Retry),
}

impl<T: Copy> CachedCheck<T> {
    pub(super) fn ensure(
        &mut self,
        now: Duration,
        check: impl FnOnce() -> Result<T, UnavailableReason>,
    ) -> Result<T, Retry> {
        match self {
            Self::Passed(value) => return Ok(*value),
            Self::Failed(retry) if now < retry.at => return Err(retry.clone()),
            _ => {}
        }
        match check() {
            Ok(value) => {
                *self = Self::Passed(value);
                Ok(value)
            }
            Err(reason) => {
                let retry = Retry {
                    reason,
                    at: now.saturating_add(VALIDATION_RETRY),
                };
                *self = Self::Failed(retry.clone());
                Err(retry)
            }
        }
    }
}

impl CachedCheck {
    pub(super) fn validate<E: fmt::Display>(
        &mut self,
        now: Duration,
        name: &'static str,
        check: impl FnOnce() -> Result<(), E>,
    ) -> Result<(), Retry>
    where
        for<'a> UnavailableReason: From<&'a E>,
    {
        self.ensure(now, || {
            check().map_err(|error| {
                tracing::debug!(%error, validation = name, "executable layout rejected");
                // An unreadable instruction window is not evidence of an incompatible layout.
                match UnavailableReason::from(&error) {
                    reason @ UnavailableReason::ReadFailed { .. } => reason,
                    _ => UnavailableReason::UnsupportedBuild,
                }
            })?;
            tracing::debug!(validation = name, "executable layout validated");
            Ok(())
        })
    }
}

pub(super) fn identify(memory: &mut dyn ProcessMemory) -> Result<Executable, UnavailableReason> {
    let executable = memory.target().executable().to_owned();
    let module = memory
        .modules()
        .map_err(|error| {
            tracing::debug!(%error, "game module enumeration failed");
            UnavailableReason::ReadFailed {
                message: "game modules could not be read".into(),
            }
        })?
        .into_iter()
        .filter(|module| {
            matching::matched_target_executable(&module.name) == Some(executable.as_str())
                || matching::matched_target_executable(&module.path) == Some(executable.as_str())
        })
        .min_by_key(|module| module.base)
        .ok_or(UnavailableReason::TargetNotReady)?;
    // Linux mappings may describe only a segment; PE headers give the full image size.
    let actual = read_executable_fingerprint(memory, module.base).map_err(|error| {
        tracing::debug!(%error, "game build identification failed");
        match error {
            FingerprintError::Read(_) => UnavailableReason::ReadFailed {
                message: "game executable could not be read".into(),
            },
            FingerprintError::InvalidImage => UnavailableReason::UnsupportedBuild,
        }
    })?;
    tracing::info!(build = %actual, candidate = %BUILD, provisional = (actual != BUILD), "game executable identified");
    Ok(Executable {
        base: module.base,
        actual,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use provider_sdk::memory::{MemoryError, ReadError};

    #[test]
    fn successful_checks_are_shared_without_accelerating_failed_checks() {
        let mut shared = CachedCheck::default();
        let mut topics = [CachedCheck::default(), CachedCheck::default()];
        let mut shared_calls = 0;
        let mut topic_calls = [0, 0];
        for second in 0..=6 {
            let now = Duration::from_secs(second);
            for (index, topic) in topics.iter_mut().enumerate() {
                assert!(
                    shared
                        .ensure(now, || {
                            shared_calls += 1;
                            Ok(())
                        })
                        .is_ok()
                );
                let result = topic.ensure(now, || {
                    topic_calls[index] += 1;
                    if index == 0 && second == 0 {
                        Err(UnavailableReason::UnsupportedBuild)
                    } else {
                        Ok(())
                    }
                });
                if index == 0 && second < 5 {
                    assert!(matches!(result, Err(Retry {
                        reason: UnavailableReason::UnsupportedBuild, at,
                    }) if at == VALIDATION_RETRY));
                } else {
                    assert!(result.is_ok());
                }
            }
        }
        assert_eq!(shared_calls, 1);
        assert_eq!(topic_calls, [2, 1]);
    }

    #[test]
    fn unreadable_code_is_distinct_from_incompatible_code() {
        for (error, expected) in [
            (
                ReadError::from(MemoryError::Read("private native diagnostic".into())),
                UnavailableReason::ReadFailed {
                    message: "target memory could not be read".into(),
                },
            ),
            (
                ReadError::layout("test instruction"),
                UnavailableReason::UnsupportedBuild,
            ),
        ] {
            let result = CachedCheck::default().validate(Duration::ZERO, "test", || Err(error));
            assert_eq!(result.map_err(|retry| retry.reason), Err(expected));
        }
    }
}
