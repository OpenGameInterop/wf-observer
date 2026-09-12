use std::error::Error;

use memflow::prelude::v1::{Address, ArchitectureIdent, ProcessInfo, ProcessState};

use crate::{
    AccessError, ProcessInstance, ProcessMemory, ProcessMetadata, Target, attach,
    discover_targets_by,
};

use super::{access::with_current_process, discovery::targets_from_processes};

type TestResult = Result<(), Box<dyn Error>>;

fn current_instance() -> Result<ProcessInstance, Box<dyn Error>> {
    ProcessInstance::for_pid(std::process::id()).ok_or_else(|| "test process is missing".into())
}

fn process_info(pid: u32) -> ProcessInfo {
    ProcessInfo {
        pid,
        address: Address::NULL,
        state: ProcessState::Alive,
        name: "short-name".into(),
        path: "/test/observer-test.exe".into(),
        command_line: "observer-test.exe --test".into(),
        sys_arch: ArchitectureIdent::X86(64, false),
        proc_arch: ArchitectureIdent::X86(64, false),
        dtb1: Address::INVALID,
        dtb2: Address::INVALID,
    }
}

#[test]
fn discovery_uses_the_callers_match_and_canonical_name() -> TestResult {
    let instance = current_instance()?;
    let processes = vec![process_info(u32::MAX), process_info(instance.pid())];
    let matcher = |metadata: &ProcessMetadata<'_>| {
        metadata
            .path
            .ends_with("/observer-test.exe")
            .then(|| "canonical-module.exe".to_owned())
    };
    let targets = targets_from_processes(processes, matcher);

    assert_eq!(
        targets,
        vec![Target::new(instance, "canonical-module.exe".to_owned())]
    );

    let mut unrelated = process_info(instance.pid());
    unrelated.path = "/test/unrelated.exe".into();
    assert!(targets_from_processes(vec![unrelated], matcher).is_empty());
    Ok(())
}

#[test]
fn explicit_identity_checks_reject_stale_instances_before_access() -> TestResult {
    let current = current_instance()?;
    let stale = ProcessInstance::new(current.pid(), current.start_marker().wrapping_add(1));
    let mut accessed = false;
    assert!(matches!(
        with_current_process(stale, || {
            accessed = true;
            Ok(())
        }),
        Err(AccessError::TargetChanged(pid)) if pid == current.pid()
    ));
    assert!(!accessed);
    let target = Target::new(stale, "observer-test.exe".to_owned());
    assert!(matches!(
        attach(&target),
        Err(AccessError::TargetChanged(_))
    ));
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[test]
fn native_attachment_reads_complete_buffers_and_rejects_invalid_ranges() -> TestResult {
    let executable = std::env::current_exe()?;
    let name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("test executable has no name")?;
    let instance = current_instance()?;
    let targets =
        discover_targets_by(|metadata| (metadata.pid == instance.pid()).then(|| name.to_owned()))?;
    let target = targets
        .iter()
        .find(|target| target.instance() == instance)
        .ok_or("test executable was not discovered")?;
    let mut attachment = attach(target)?;
    let memory: &mut dyn ProcessMemory = &mut attachment;
    assert_eq!(memory.target(), target);
    memory.verify()?;
    assert!(memory.modules()?.iter().any(|module| module.name == name));

    let source = *b"retained read-only attachment";
    let mut buffer = vec![0; source.len()];
    memory.read_into(source.as_ptr() as usize as u64, &mut buffer)?;
    memory.verify()?;
    assert_eq!(buffer, source);
    assert!(matches!(
        memory.read_into(u64::MAX - 1, &mut buffer),
        Err(AccessError::InvalidReadRange { .. })
    ));
    assert_eq!(buffer, source);
    assert!(matches!(
        memory.read_into(0, &mut buffer),
        Err(AccessError::Read { .. })
    ));
    memory.read_into(0, &mut [])?;

    #[cfg(target_os = "linux")]
    {
        let maps = std::fs::read_to_string("/proc/self/maps")?;
        let ranges = maps
            .lines()
            .filter_map(|line| line.split_whitespace().next()?.split_once('-'))
            .map(|(start, end)| {
                Ok((
                    u64::from_str_radix(start, 16)?,
                    u64::from_str_radix(end, 16)?,
                ))
            })
            .collect::<Result<Vec<_>, std::num::ParseIntError>>()?;
        let boundary = ranges
            .windows(2)
            .find(|pair| pair[0].1 < pair[1].0)
            .ok_or("test process has no unmapped gap")?[0]
            .1;
        memory.read_into(boundary - 1, &mut [0])?;
        assert!(matches!(
            memory.read_into(boundary - 1, &mut [0; 2]),
            Err(AccessError::Read { .. })
        ));
    }

    Ok(())
}
