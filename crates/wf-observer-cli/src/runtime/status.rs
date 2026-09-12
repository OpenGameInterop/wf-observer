//! Human-readable CLI status layout, separate from persisted metadata.

use std::io;

use super::{
    record::{Activity, ServiceInfo, TargetStatus},
    storage::current_agent,
};

/// Prints discovery health and each target independently of service liveness.
pub(crate) fn print_status() -> anyhow::Result<()> {
    let Some(record) = current_agent()? else {
        println!("Service: not running");
        return Ok(());
    };

    write_status(&mut io::stdout().lock(), &record)?;
    Ok(())
}

pub(super) fn write_status(output: &mut impl io::Write, record: &ServiceInfo) -> io::Result<()> {
    writeln!(output, "Service: running")?;
    writeln!(output, "Version: {}", record.version())?;
    writeln!(output, "Service PID: {}", record.pid())?;
    writeln!(output, "Iroh endpoint ID: {}", record.service.endpoint_id)?;
    if let Some(error) = &record.host.discovery_error {
        writeln!(output, "Discovery: retrying ({error})")?;
    } else if record.host.targets.is_empty() {
        writeln!(output, "Discovery: waiting for supported games")?;
    } else {
        writeln!(output, "Discovery: active")?;
    }
    let sessions = record
        .host
        .targets
        .iter()
        .filter(|target| matches!(target.activity, Activity::Observing { .. }))
        .count();
    writeln!(output, "Sessions: {sessions}")?;
    for target in &record.host.targets {
        writeln!(output)?;
        write_target_status(output, target)?;
    }
    Ok(())
}

fn write_target_status(output: &mut impl io::Write, target: &TargetStatus) -> io::Result<()> {
    writeln!(output, "Target: {}", target.target.executable)?;
    writeln!(output, "  Target PID: {}", target.target.process.pid)?;
    writeln!(output, "  Provider: {}", target.target.provider_id)?;
    writeln!(output, "  Game: {}", target.target.game_id)?;
    writeln!(output, "  State: {}", target.activity)?;
    match &target.activity {
        Activity::Observing { session_id } => writeln!(output, "  Session ID: {session_id}")?,
        Activity::Retrying { error } => writeln!(output, "  Last error: {error}")?,
        Activity::Attaching => {}
    }
    let state = if target.target.process.is_current() {
        "running"
    } else {
        "exited or PID reused"
    };
    writeln!(output, "  Target state: {state}")
}
