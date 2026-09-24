//! Live API smoke test. Run with --help; requires an already-running service.
use std::{future::Future, process::ExitCode, time::Duration};
use tokio::time::{Instant, timeout, timeout_at};
use wf_observer_sdk::{self as wf, ObserverError, RequestError};

const TIMEOUT: Duration = Duration::from_secs(10);
type Check = Result<(), String>;

struct Row {
    topic: &'static str,
    checks: Vec<(&'static str, Check)>,
    note: String,
}

impl Row {
    fn passed(&self) -> bool {
        self.checks.iter().all(|(_, result)| result.is_ok())
    }

    fn print(&self) {
        let mut failures: Vec<(String, Vec<&str>)> = Vec::new();
        for (operation, result) in &self.checks {
            if let Err(error) = result {
                if let Some((_, operations)) = failures.iter_mut().find(|(e, _)| e == error) {
                    operations.push(operation);
                } else {
                    failures.push((error.clone(), vec![operation]));
                }
            }
        }
        let detail = if failures.is_empty() {
            self.note.clone()
        } else {
            failures
                .into_iter()
                .map(|(error, operations)| format!("{}: {error}", operations.join("/")))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let status = if self.passed() { "OK" } else { "FAIL" };
        println!("{:<14} {status:<4} {detail}", self.topic);
    }
}

fn error_text(error: ObserverError) -> String {
    match error {
        ObserverError::Request {
            error: RequestError::Unavailable { reason },
        } => reason.to_string(),
        other => other.to_string(),
    }
}

async fn call<T>(future: impl Future<Output = Result<T, ObserverError>>) -> Result<T, String> {
    timeout(TIMEOUT, future)
        .await
        .map_err(|_| "timeout".to_owned())?
        .map_err(error_text)
}

// Exercise the public typed API for every snapshot, including its decoder.
macro_rules! snapshot_probe {
    ($name:ident, $state:ident, $summarize:expr) => {
        async fn $name(game: &wf::WarframeSession, observe: Duration) -> Row {
            let capability = game.$name();
            let opened = call(capability.watch()).await;
            let mut note = String::new();
            let summarize = &mut $summarize;
            let mut watched = Err("no sample".to_owned());
            if let Ok(watch) = &opened {
                let deadline = Instant::now() + if observe.is_zero() { TIMEOUT } else { observe };
                loop {
                    match timeout_at(deadline, watch.next()).await {
                        Ok(Ok(Some(wf::$state::Waiting))) => {
                            watched = Err("initializing".to_owned());
                        }
                        Ok(Ok(Some(wf::$state::Ready { value }))) => {
                            summarize(&value, &mut note);
                            watched = Ok(());
                            if observe.is_zero() {
                                break;
                            }
                        }
                        Ok(Ok(Some(wf::$state::Unavailable { reason }))) => {
                            watched = Err(reason.to_string());
                            if observe.is_zero() {
                                break;
                            }
                        }
                        Ok(Ok(None)) => {
                            watched = Err("closed".to_owned());
                            break;
                        }
                        Ok(Err(error)) => {
                            watched = Err(error_text(error));
                            break;
                        }
                        Err(_) => break,
                    }
                }
                // A successful stream must also expose usable current state.
                if watched.is_ok() {
                    watched = match watch.current() {
                        Ok(wf::$state::Ready { .. }) => Ok(()),
                        Ok(wf::$state::Waiting) => Err("initializing".to_owned()),
                        Ok(wf::$state::Unavailable { reason }) => Err(reason.to_string()),
                        Err(error) => Err(error_text(error)),
                    };
                }
            } else if let Err(error) = &opened {
                watched = Err(error.clone());
            }
            // Keep the watch alive: cached() alone never starts acquisition.
            // Both operations still run if opening/receiving the watch failed.
            let (read, cache) = tokio::join!(call(capability.read()), call(capability.cached()));
            let mut checks = vec![
                ("watch", watched),
                ("read", read.map(|_| ())),
                (
                    "cache",
                    cache.and_then(|value| value.map(|_| ()).ok_or("no sample".into())),
                ),
            ];
            if let Ok(watch) = opened {
                if let Err(error) = call(watch.shutdown()).await {
                    checks.push(("shutdown", Err(error)));
                }
            }
            Row {
                topic: stringify!($name),
                checks,
                note,
            }
        }
    };
}

fn no_note<T>(_: &T, _: &mut String) {}

snapshot_probe!(inventory, InventoryState, no_note);
snapshot_probe!(mastery, MasteryState, no_note);
snapshot_probe!(intrinsics, IntrinsicsState, no_note);
snapshot_probe!(star_chart, StarChartState, no_note);
snapshot_probe!(currencies, CurrenciesState, no_note);
snapshot_probe!(player, PlayerState, no_note);
snapshot_probe!(screens, ScreensState, {
    let mut previous = None;
    let mut changes = 0;
    move |value: &wf::WarframeScreens, note: &mut String| {
        if previous.as_ref().is_some_and(|old| old != &value.screens) {
            changes += 1;
        }
        previous = Some(value.screens.clone());
        *note = format!("{} visible, {changes} changes", value.screens.len());
    }
});
snapshot_probe!(
    relic_rewards,
    RelicRewardsState,
    |value: &wf::WarframeRelicRewards, note: &mut String| {
        match &value.picker {
            wf::RelicRewardPicker::Open { choices } if !choices.is_empty() => {
                *note = format!("{} choices observed", choices.len());
            }
            _ if note.is_empty() => *note = "choices untested".into(),
            _ => {}
        }
    }
);

fn chat_health(health: wf::CapabilityHealth) -> Check {
    match health {
        wf::CapabilityHealth::Available => Ok(()),
        wf::CapabilityHealth::Unavailable { reason } => Err(reason.to_string()),
        _ => Err("initializing".into()),
    }
}

async fn chat(game: &wf::WarframeSession, observe: Duration) -> Row {
    let mut messages = 0;
    let mut gaps = 0;
    let mut checks = Vec::new();
    let result = async {
        let watch = call(game.chat().watch()).await?;
        let deadline = Instant::now() + if observe.is_zero() { TIMEOUT } else { observe };
        let mut result = Err("no state".to_owned());
        loop {
            match timeout_at(deadline, watch.next()).await {
                Ok(Ok(Some(wf::ChatObservation::State { value }))) => {
                    let settled = matches!(
                        value.health,
                        wf::CapabilityHealth::Available | wf::CapabilityHealth::Unavailable { .. }
                    );
                    result = chat_health(value.health);
                    if observe.is_zero() && settled {
                        break;
                    }
                }
                Ok(Ok(Some(wf::ChatObservation::Message { .. }))) => {
                    messages += 1;
                }
                Ok(Ok(Some(wf::ChatObservation::Gap { .. }))) => {
                    gaps += 1;
                }
                Ok(Ok(None)) => {
                    result = Err("closed".into());
                    break;
                }
                Ok(Err(error)) => {
                    result = Err(error_text(error));
                    break;
                }
                Err(_) => break,
            }
        }
        if result.is_ok() {
            result = watch
                .current()
                .map_err(error_text)
                .and_then(|state| chat_health(state.health));
        }
        if let Err(error) = call(watch.shutdown()).await {
            checks.push(("shutdown", Err(error)));
        }
        result
    }
    .await;
    checks.insert(0, ("watch", result));
    let note = if messages == 0 {
        "messages untested".into()
    } else {
        format!("{messages} messages, {gaps} gaps")
    };
    Row {
        topic: "chat",
        checks,
        note,
    }
}

async fn probe(client: &wf::ObserverClient, observe: Duration) -> Result<bool, String> {
    let (ping, catalog, status) = tokio::join!(
        call(client.ping()),
        call(client.catalog()),
        call(client.status())
    );
    ping?;
    let catalog = catalog?;
    let status = status?;
    let game = call(client.warframe().single_session()).await?;
    let info = game.info();
    let provider = catalog
        .providers
        .iter()
        .find(|p| p.id == info.provider_id)
        .ok_or("session provider missing from catalog")?;
    println!(
        "service OK ({}) | PID {}",
        status.application_version, info.target.pid,
    );
    let (inventory, mastery, intrinsics, star_chart, currencies, player, screens, relics, chat) = tokio::join!(
        inventory(&game, observe),
        mastery(&game, observe),
        intrinsics(&game, observe),
        star_chart(&game, observe),
        currencies(&game, observe),
        player(&game, observe),
        screens(&game, observe),
        relic_rewards(&game, observe),
        chat(&game, observe),
    );
    let rows = [
        inventory, mastery, intrinsics, star_chart, currencies, player, screens, relics, chat,
    ];
    // Refresh metadata: identifying the executable itself requires topic demand.
    let current = call(client.warframe().sessions())
        .await
        .and_then(|sessions| {
            sessions
                .into_iter()
                .find(|s| s.session == info.session)
                .ok_or("session ended".into())
        });
    match &current {
        Ok(session) => println!(
            "game {}",
            session.game_build.as_deref().unwrap_or("unknown")
        ),
        Err(error) => println!("game FAIL {error}"),
    }
    let mut covered = true;
    for capability in &provider.capabilities {
        let matches = rows.iter().any(|row| {
            capability.topic == format!("warframe.{}", row.topic)
                && capability.schema_version == 1
                && capability.snapshots == (row.topic != "chat")
                && capability.events == (row.topic == "chat")
        });
        if !matches {
            println!(
                "coverage FAIL {} v{} untested",
                capability.topic, capability.schema_version
            );
            covered = false;
        }
    }
    for row in &rows {
        row.print();
    }
    let passed = rows.iter().filter(|row| row.passed()).count();
    println!("{passed}/{} passed", rows.len());
    Ok(covered && current.is_ok() && passed == rows.len())
}

async fn run(observe: Duration) -> Result<bool, String> {
    let client = call(wf::connect_local()).await?;
    let result = Box::pin(probe(&client, observe)).await;
    let shutdown = call(client.shutdown()).await;
    shutdown?;
    result
}

fn arguments() -> Result<Option<Duration>, String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => Ok(Some(Duration::ZERO)),
        [help] if help == "--help" || help == "-h" => {
            println!(
                "live_smoke [--observe SECONDS]\nChecks all live APIs; optional 1..3600s observation for chat/screens/relics.\nRequires a running local service and exactly one Warframe session.\nExit 0: all APIs available; exit 1: failure. No game actions are automated."
            );
            Ok(None)
        }
        [flag, seconds] if flag == "--observe" => seconds
            .parse::<u64>()
            .ok()
            .filter(|s| (1..=3600).contains(s))
            .map(|s| Some(Duration::from_secs(s)))
            .ok_or("--observe requires seconds in 1..3600".into()),
        _ => Err("usage: live_smoke [--observe SECONDS]".into()),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let result = match arguments() {
        Ok(Some(observe)) => run(observe).await,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => Err(error),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("FAIL {error}");
            ExitCode::FAILURE
        }
    }
}
