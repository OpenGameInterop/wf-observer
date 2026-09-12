use std::fs;

use anyhow::Context as _;
use memory_reader::ProcessInstance;

use crate::singleton::AgentLock;

use super::{
    record::{
        Activity, HostStatus, RECORD_VERSION, RecordedProcess, ServiceIdentity, ServiceInfo,
        TargetInfo, TargetStatus,
    },
    shutdown::{ShutdownRequest, remove_shutdown_request_at, shutdown_requested_at},
    status,
    storage::{
        Registration, current_agent_at, remove_if_owned_by, remove_stale_record, write_atomic,
    },
};

fn current_instance() -> anyhow::Result<ProcessInstance> {
    ProcessInstance::for_pid(std::process::id()).context("failed to identify the test process")
}

fn current_process() -> anyhow::Result<RecordedProcess> {
    current_instance().map(Into::into)
}

fn record(service: RecordedProcess, target: Option<RecordedProcess>) -> ServiceInfo {
    ServiceInfo {
        schema_version: RECORD_VERSION,
        service: ServiceIdentity {
            application_version: "1.2.3".to_owned(),
            process: service,
            endpoint_id: "test-endpoint".to_owned(),
        },
        host: HostStatus {
            discovery_error: None,
            targets: target
                .into_iter()
                .map(|process| TargetStatus {
                    target: TargetInfo {
                        process,
                        executable: "Warframe.x64.exe".to_owned(),
                        provider_id: "fixture-provider".to_owned(),
                        game_id: "fixture-game".to_owned(),
                    },
                    activity: Activity::Observing {
                        session_id: "test-session".to_owned(),
                    },
                })
                .collect(),
        },
    }
}

#[test]
fn service_liveness_is_independent_of_its_sessions() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("runtime.json");
    let lock_path = directory.path().join("agent.lock");
    let process = current_process()?;
    let stale_target = RecordedProcess {
        start_marker: process.start_marker.wrapping_add(1),
        ..process
    };

    for (target, target_status) in [
        (Some(process), "Target state: running"),
        (None, "Sessions: 0"),
        (Some(stale_target), "Target state: exited or PID reused"),
    ] {
        let expected = record(process, target);
        write_atomic(&path, &expected)?;
        let actual = current_agent_at(&path, &lock_path)?.context("live service was hidden")?;
        assert_eq!(actual, expected);
        assert!(actual.is_compatible_with("1.2.3"));
        assert!(!actual.is_compatible_with("2.0.0"));
        assert!(path.exists());

        let mut status = Vec::new();
        status::write_status(&mut status, &actual)?;
        let status = String::from_utf8(status)?;
        assert!(status.contains("Service: running"));
        assert!(status.contains(target_status));
    }
    Ok(())
}

#[test]
fn host_updates_keep_service_identity_and_clear_only_ended_sessions() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("runtime.json");
    let lock_path = directory.path().join("agent.lock");
    let _lock = AgentLock::acquire(&lock_path)?;
    let owner = current_instance()?;
    let initial = ServiceInfo::new(owner, "test-endpoint".to_owned());
    write_atomic(&path, &initial)?;
    let mut registration = Registration {
        path: path.clone(),
        agent: owner,
        record: ServiceInfo::new(owner, "test-endpoint".to_owned()),
        registered: true,
    };

    let first = TargetStatus {
        target: TargetInfo {
            process: owner.into(),
            executable: "fixture-1".to_owned(),
            provider_id: "fixture-provider".to_owned(),
            game_id: "fixture-game".to_owned(),
        },
        activity: Activity::observing()?,
    };
    let second = TargetStatus {
        target: TargetInfo {
            process: RecordedProcess {
                pid: owner.pid().wrapping_add(1),
                start_marker: owner.start_marker(),
            },
            executable: "fixture-2".to_owned(),
            provider_id: "fixture-provider".to_owned(),
            game_id: "fixture-game".to_owned(),
        },
        activity: Activity::observing()?,
    };
    for host in [
        HostStatus {
            discovery_error: None,
            targets: vec![first, second.clone()],
        },
        HostStatus {
            discovery_error: Some("enumeration failed".to_owned()),
            targets: vec![second.clone()],
        },
        HostStatus {
            discovery_error: None,
            targets: vec![
                second,
                TargetStatus {
                    target: TargetInfo {
                        process: owner.into(),
                        executable: "fixture-1".to_owned(),
                        provider_id: "fixture-provider".to_owned(),
                        game_id: "fixture-game".to_owned(),
                    },
                    activity: Activity::Retrying {
                        error: "access denied".to_owned(),
                    },
                },
            ],
        },
        HostStatus::default(),
    ] {
        registration.update(host.clone())?;
        let actual = current_agent_at(&path, &lock_path)?.context("service disappeared")?;
        assert_eq!(actual.service, initial.service);
        assert_eq!(actual.host, host);
    }

    registration.unregister()?;
    assert!(!path.exists());
    Ok(())
}

#[test]
fn unsupported_record_version_is_rejected_and_left_intact() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("runtime.json");
    let lock_path = directory.path().join("agent.lock");
    let process = current_process()?;
    let mut future = record(process, None);
    future.schema_version += 1;
    write_atomic(&path, &future)?;
    let bytes = fs::read(&path)?;

    assert!(current_agent_at(&path, &lock_path).is_err());
    remove_if_owned_by(&path, process)?;
    assert_eq!(fs::read(path)?, bytes);
    Ok(())
}

#[test]
fn removes_a_stale_record_while_no_agent_owns_the_lock() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("runtime.json");
    let lock_path = directory.path().join("agent.lock");
    let current = current_process()?;
    let stale = RecordedProcess {
        start_marker: current.start_marker.wrapping_add(1),
        ..current
    };
    write_atomic(&path, &record(stale, Some(current)))?;
    let stale_bytes = fs::read(&path)?;

    assert_eq!(current_agent_at(&path, &lock_path)?, None);
    assert!(!path.exists());

    write_atomic(&path, &record(current, None))?;
    let replacement = fs::read(&path)?;
    remove_stale_record(&path, &lock_path, &stale_bytes)?;
    assert_eq!(fs::read(path)?, replacement);
    Ok(())
}

#[test]
fn leaves_a_stale_record_while_an_agent_owns_the_lock() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("runtime.json");
    let lock_path = directory.path().join("agent.lock");
    let stale = RecordedProcess {
        pid: u32::MAX,
        start_marker: u64::MAX,
    };
    write_atomic(&path, &record(stale, Some(current_process()?)))?;
    let lock = AgentLock::acquire(&lock_path)?;

    assert_eq!(current_agent_at(&path, &lock_path)?, None);
    assert!(path.exists());
    drop(lock);
    Ok(())
}

#[test]
fn unregister_does_not_remove_a_replacement_record() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("runtime.json");
    let owner = current_instance()?;
    let replacement = record(
        RecordedProcess {
            pid: owner.pid(),
            start_marker: owner.start_marker().wrapping_add(1),
        },
        Some(current_process()?),
    );
    write_atomic(&path, &replacement)?;

    Registration {
        path: path.clone(),
        agent: owner,
        record: ServiceInfo::new(owner, "test-endpoint".to_owned()),
        registered: true,
    }
    .unregister()?;

    let bytes = fs::read(path)?;
    assert_eq!(ServiceInfo::decode(&bytes)?, replacement);
    Ok(())
}

#[test]
fn shutdown_request_applies_only_to_its_agent() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("shutdown.json");
    let requested = current_instance()?;
    let recorded: RecordedProcess = requested.into();
    let other = RecordedProcess {
        pid: recorded.pid.wrapping_add(1),
        start_marker: recorded.start_marker,
    };
    write_atomic(&path, &ShutdownRequest { agent: recorded })?;

    assert!(shutdown_requested_at(&path, requested)?);
    assert!(!shutdown_requested_at(&path, other)?);

    remove_shutdown_request_at(&path, other)?;
    assert!(path.exists());
    remove_shutdown_request_at(&path, requested)?;
    assert!(!path.exists());
    Ok(())
}
