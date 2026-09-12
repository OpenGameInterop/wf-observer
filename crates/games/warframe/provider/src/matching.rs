//! Warframe executable matching, including a conservative Linux loader fallback.

use memory_reader::ProcessMetadata;

const TARGET_EXECUTABLES: &[&str] = &["Warframe.x64.exe", "Warframe.exe"];
#[cfg(target_os = "linux")]
const TRUNCATED_TARGET_EXECUTABLES: &[&str] = &["Warframe.x64.ex"];

pub(super) fn match_process(process: &ProcessMetadata<'_>) -> Option<&'static str> {
    matched_target_executable(process.name)
        .or_else(|| matched_target_executable(process.path))
        .or_else(|| match_command_line(process))
}

pub(super) fn matched_target_executable(value: &str) -> Option<&'static str> {
    let basename = executable_basename(value);
    let matched = TARGET_EXECUTABLES
        .iter()
        .copied()
        .find(|candidate| basename.eq_ignore_ascii_case(candidate));
    if matched.is_some() {
        return matched;
    }

    #[cfg(target_os = "linux")]
    if TRUNCATED_TARGET_EXECUTABLES
        .iter()
        .any(|candidate| basename.eq_ignore_ascii_case(candidate))
    {
        return Some("Warframe.x64.exe");
    }

    None
}

fn executable_basename(value: &str) -> &str {
    value
        .trim_matches(['"', '\''])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim_matches(['"', '\''])
}

fn match_command_line(process: &ProcessMetadata<'_>) -> Option<&'static str> {
    let (executable, remaining) = first_argument(process.command_line)?;
    if let Some(matched) = matched_target_executable(executable) {
        return Some(matched);
    }

    #[cfg(target_os = "linux")]
    if is_wine_loader(process.name) || is_wine_loader(process.path) {
        // Linux memflow-native derives path/name from argv[0] and joins argv
        // with spaces. Prefer the intact path to handle loader paths with spaces.
        let arguments = is_wine_loader(process.path)
            .then(|| process.command_line.trim_start().strip_prefix(process.path))
            .flatten()
            .filter(|tail| tail.starts_with(char::is_whitespace))
            .or_else(|| is_wine_loader(executable).then_some(remaining))?;
        let (game, _) = first_argument(arguments)?;
        return matched_target_executable(game);
    }

    #[cfg(not(target_os = "linux"))]
    let _ = remaining;
    None
}

#[cfg(target_os = "linux")]
fn is_wine_loader(value: &str) -> bool {
    matches!(
        executable_basename(value),
        "wine" | "wine64" | "wine-preloader" | "wine64-preloader"
    )
}

// Read one literal executable argument, preserving Windows backslashes. This is
// not shell parsing: no escapes, expansion, option skipping, or search of later
// arguments. Lost unquoted argument boundaries cannot be recovered reliably.
fn first_argument(value: &str) -> Option<(&str, &str)> {
    let value = value.trim_start();
    let first = value.chars().next()?;
    if matches!(first, '"' | '\'') {
        let quoted = &value[1..];
        let end = quoted.find(first)?;
        let tail = &quoted[end + 1..];
        if !tail.is_empty() && !tail.starts_with(char::is_whitespace) {
            return None;
        }
        Some((&quoted[..end], tail))
    } else {
        let end = value.find(char::is_whitespace).unwrap_or(value.len());
        Some((&value[..end], &value[end..]))
    }
}

#[cfg(test)]
mod tests {
    use super::{match_process, matched_target_executable};
    use memory_reader::ProcessMetadata;

    #[test]
    fn matches_executable_names_and_paths() {
        for (input, expected) in [
            ("WARFRAME.EXE", "Warframe.exe"),
            ("/games/Warframe/warframe.X64.exe", "Warframe.x64.exe"),
            (
                r#""C:\Program Files\Warframe\Warframe.x64.exe""#,
                "Warframe.x64.exe",
            ),
            ("'/games/Warframe/Warframe.exe'", "Warframe.exe"),
        ] {
            assert_eq!(matched_target_executable(input), Some(expected), "{input}");
        }
    }

    #[test]
    fn rejects_other_executables() {
        for input in [
            "",
            "Warframe.x64.exe.bak",
            "NotWarframe.exe",
            "/games/Warframe.exe/other.exe",
        ] {
            assert_eq!(matched_target_executable(input), None, "{input}");
        }
    }

    #[test]
    fn matches_truncated_names_only_on_linux() {
        assert_eq!(
            matched_target_executable("WARFRAME.X64.EX"),
            cfg!(target_os = "linux").then_some("Warframe.x64.exe"),
        );
        assert_eq!(matched_target_executable("Warframe.x64.e"), None);
    }

    #[test]
    fn uses_name_then_path_then_command_line_executable() {
        let mut metadata = ProcessMetadata {
            pid: 42,
            name: "Warframe.exe",
            path: "/games/Warframe.x64.exe",
            command_line: "Warframe.exe --argument",
        };
        assert_eq!(match_process(&metadata), Some("Warframe.exe"));

        metadata.name = "unrecognized";
        assert_eq!(match_process(&metadata), Some("Warframe.x64.exe"));

        metadata.path = "";
        metadata.command_line = "  \"/games/Warframe.exe\" --argument";
        assert_eq!(match_process(&metadata), Some("Warframe.exe"));

        metadata.command_line = r#""C:\Program Files\Warframe\Warframe.x64.exe" --argument"#;
        assert_eq!(match_process(&metadata), Some("Warframe.x64.exe"));

        metadata.command_line = "launcher Warframe.exe";
        assert_eq!(match_process(&metadata), None);
    }

    #[test]
    fn recognizes_only_direct_game_arguments_of_linux_wine_loaders() {
        for command_line in [
            r#"wine64-preloader "Z:\Games\Warframe.x64.exe""#,
            r#"wine64-preloader "Z:\Program Files\Warframe\Warframe.x64.exe" --argument"#,
            r"wine64-preloader Z:\Games\Warframe.x64.exe",
        ] {
            let process = ProcessMetadata {
                pid: 42,
                name: "wine64-preloader",
                path: "wine64-preloader",
                command_line,
            };
            assert_eq!(
                match_process(&process),
                cfg!(target_os = "linux").then_some("Warframe.x64.exe"),
            );
        }
        // memflow preserves argv[0] in path even when its flattened command line
        // loses the quotes around a loader path containing spaces.
        let mut process = ProcessMetadata {
            pid: 42,
            name: "wine64-preloader",
            path: "/Steam/Proton Experimental/files/bin/wine64-preloader",
            command_line: r#"/Steam/Proton Experimental/files/bin/wine64-preloader "Z:\Games\Warframe.x64.exe""#,
        };
        assert_eq!(
            match_process(&process),
            cfg!(target_os = "linux").then_some("Warframe.x64.exe"),
        );

        process.path = "wine64-preloader";
        for command_line in [
            "wine64-preloader other.exe Warframe.x64.exe",
            "wine64-preloader --argument Warframe.x64.exe",
            "wine64-preloader Warframe.x64.exe.bak",
            "wine64-preloader /Warframe.x64.exe/other.exe",
            "launcher Warframe.x64.exe",
            r#"wine64-preloader "Z:\Games\Warframe.x64.exe"#,
            r#"wine64-preloader "Z:\Games\Warframe.x64.exe".bak"#,
        ] {
            process.command_line = command_line;
            assert_eq!(match_process(&process), None, "{command_line}");
        }
        process.name = "unrelated";
        process.path = "/tools/launcher";
        process.command_line = "wine64-preloader Warframe.x64.exe";
        assert_eq!(match_process(&process), None);
    }
}
