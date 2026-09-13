//! Human-readable CLI status layout, separate from persisted metadata.

use std::io;

use crate::settings::{self, AccessMode};

use super::{
    record::{Activity, ServiceInfo, TargetStatus},
    storage::current_agent,
};

/// Prints discovery health and each target independently of service liveness.
pub(crate) fn print_status() -> anyhow::Result<()> {
    let configured = settings::load()?.access;
    let Some(record) = current_agent()? else {
        println!("Service: not running");
        write_access(&mut io::stdout().lock(), configured, None)?;
        return Ok(());
    };

    write_status(&mut io::stdout().lock(), &record, configured)?;
    Ok(())
}

pub(crate) fn print_access() -> anyhow::Result<()> {
    let configured = settings::load()?.access;
    let record = current_agent()?;
    write_access(&mut io::stdout().lock(), configured, record.as_ref())?;
    Ok(())
}

fn write_access(
    output: &mut impl io::Write,
    configured: AccessMode,
    record: Option<&ServiceInfo>,
) -> io::Result<()> {
    writeln!(output, "Configured access: {configured}")?;
    match record {
        Some(record) => match record.service.access_mode {
            Some(active) => writeln!(output, "Active access: {active}")?,
            None => writeln!(
                output,
                "Active access: unknown (older service; run wf-observer start)"
            )?,
        },
        None => writeln!(output, "Active access: none (service not running)")?,
    }
    if let Some(record) = record {
        match (&record.service.access_mode, &record.service.reader_policy) {
            (Some(AccessMode::Local), _) => {
                writeln!(output, "Reader authorization: local connections only")?;
            }
            (Some(AccessMode::Remote), Some(policy)) if policy.mode == AccessMode::Remote => {
                writeln!(
                    output,
                    "Reader authorization: allowlist enforced ({} approved peers)",
                    policy.approved_peers.len()
                )?;
            }
            _ => writeln!(
                output,
                "Reader authorization: unknown (run wf-observer start to replace this service)"
            )?,
        }
    }
    Ok(())
}

pub(super) fn write_status(
    output: &mut impl io::Write,
    record: &ServiceInfo,
    configured: AccessMode,
) -> io::Result<()> {
    writeln!(output, "Service: running")?;
    writeln!(output, "Version: {}", record.version())?;
    writeln!(output, "Service PID: {}", record.pid())?;
    write_access(output, configured, Some(record))?;
    writeln!(output, "Iroh endpoint ID: {}", record.service.endpoint_id)?;
    if let Some(ticket) = &record.service.local_ticket {
        writeln!(output, "Local connection ticket: {ticket}")?;
    }
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
