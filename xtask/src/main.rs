#[macro_use(derive)]
extern crate derive_aliases;

mod derive_alias;

use std::{
    path::{Path, PathBuf},
    process::{Child, Command as ProcessCommand, ExitStatus},
    thread,
    time::Duration,
};

use anyhow::{Context as _, bail, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use fastant::Instant;
use tempfile::TempDir;

mod release;

const EXAMPLE_TIMEOUT: Duration = Duration::from_mins(5);

#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Packages bindings and runs their currencies examples, or checks them without a game.
    Example {
        /// Languages whose console examples should run.
        #[arg(required = true, value_enum)]
        languages: Vec<Language>,
        /// Endpoint ID or ticket of a running service with a logged-in game.
        #[arg(long, required_unless_present = "check", conflicts_with = "check")]
        endpoint: Option<String>,
        /// Builds or imports examples without connecting to a service.
        #[arg(long)]
        check: bool,
        /// Uses bindings already present under `dist` instead of packaging them.
        #[arg(long)]
        no_package: bool,
        /// Python interpreter used to package and run the Python example.
        #[arg(long, default_value = "python")]
        python: PathBuf,
        /// Cargo profile used for generated native libraries.
        #[arg(long, default_value = "dev")]
        profile: String,
    },
    /// Validates and prepares repository releases.
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ReleaseCommand {
    /// Validates the CLI package version and an optional release tag.
    CheckCli {
        /// Tag being published, such as `cli-v0.1.0`.
        #[arg(long)]
        tag: Option<String>,
    },
}

#[derive(Debug, ValueEnum, ..Copy, ..Eq)]
enum Language {
    Python,
    Csharp,
    Java,
    Kotlin,
    Swift,
}

#[derive(Debug, ..Copy, ..Eq)]
enum BindingTarget {
    Python,
    Csharp,
    Java,
    Apple,
}

impl Language {
    const fn binding_target(self) -> BindingTarget {
        match self {
            Self::Python => BindingTarget::Python,
            Self::Csharp => BindingTarget::Csharp,
            Self::Java | Self::Kotlin => BindingTarget::Java,
            Self::Swift => BindingTarget::Apple,
        }
    }

    const fn display_name(self) -> &'static str {
        match self {
            Self::Python => "Python",
            Self::Csharp => "C#",
            Self::Java => "Java",
            Self::Kotlin => "Kotlin/JVM",
            Self::Swift => "Swift",
        }
    }
}

impl BindingTarget {
    const fn boltffi_name(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Csharp => "csharp",
            Self::Java => "java",
            Self::Apple => "apple",
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Example {
            languages,
            endpoint,
            check,
            no_package,
            python,
            profile,
        } => run_examples(
            &languages,
            endpoint.as_deref(),
            check,
            no_package,
            &python,
            &profile,
        ),
        Command::Release {
            command: ReleaseCommand::CheckCli { tag },
        } => release::check_cli(workspace_root()?, tag.as_deref()),
    }
}

fn run_examples(
    languages: &[Language],
    endpoint: Option<&str>,
    check: bool,
    no_package: bool,
    python: &Path,
    profile: &str,
) -> anyhow::Result<()> {
    if languages.contains(&Language::Swift) && !cfg!(target_os = "macos") {
        bail!("the Swift console example requires macOS");
    }

    let root = workspace_root()?;

    if !no_package {
        package_bindings(root, languages, python, profile)?;
    }

    let python_environment = languages
        .contains(&Language::Python)
        .then(|| prepare_python_environment(root, python))
        .transpose()?;
    languages.iter().try_for_each(|&language| {
        run_example(root, language, endpoint, check, python_environment.as_ref())
    })
}

fn workspace_root() -> anyhow::Result<&'static Path> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask must be located directly beneath the workspace root")
}

fn package_bindings(
    root: &Path,
    languages: &[Language],
    python: &Path,
    profile: &str,
) -> anyhow::Result<()> {
    let mut targets = Vec::new();

    for &language in languages {
        let target = language.binding_target();
        if !targets.contains(&target) {
            targets.push(target);
        }
    }

    for target in targets {
        let target_name = target.boltffi_name();
        let mut command = ProcessCommand::new("boltffi");
        command
            .current_dir(root)
            .args(["pack", target_name, "--deny-skipped"])
            .arg(format!("--cargo-arg=--profile={profile}"));

        if target == BindingTarget::Python {
            command.arg("--python").arg(python);
        }

        run_command(&mut command, &format!("package the {target_name} binding"))?;
    }

    Ok(())
}

struct PythonEnvironment {
    _directory: TempDir,
    executable: PathBuf,
}

fn prepare_python_environment(root: &Path, python: &Path) -> anyhow::Result<PythonEnvironment> {
    let wheelhouse = root.join("dist/python/wheelhouse");
    ensure!(
        wheelhouse.is_dir(),
        "Python wheelhouse does not exist at {}; package the Python binding first",
        wheelhouse.display()
    );

    let directory =
        tempfile::tempdir().context("failed to create a temporary Python environment")?;
    let mut create = ProcessCommand::new(python);
    create
        .current_dir(root)
        .args(["-m", "venv"])
        .arg(directory.path());
    run_command(&mut create, "create the temporary Python environment")?;

    let executable = if cfg!(windows) {
        directory.path().join("Scripts/python.exe")
    } else {
        directory.path().join("bin/python")
    };
    ensure!(
        executable.is_file(),
        "temporary Python interpreter was not created at {}",
        executable.display()
    );

    let mut install = ProcessCommand::new(&executable);
    install
        .current_dir(root)
        .args([
            "-m",
            "pip",
            "install",
            "--disable-pip-version-check",
            "--no-index",
            "--find-links",
        ])
        .arg(&wheelhouse)
        .arg(format!("wf-observer=={}", env!("CARGO_PKG_VERSION")));
    run_command(&mut install, "install the generated Python wheel")?;

    Ok(PythonEnvironment {
        _directory: directory,
        executable,
    })
}

fn run_example(
    root: &Path,
    language: Language,
    endpoint: Option<&str>,
    check: bool,
    python: Option<&PythonEnvironment>,
) -> anyhow::Result<()> {
    let operation = if check { "check" } else { "run" };
    let action = format!(
        "{operation} the {} console example",
        language.display_name()
    );
    let endpoint = if check {
        None
    } else {
        Some(endpoint.context("supply --endpoint from wf-observer status, or use --check")?)
    };

    match language {
        Language::Python => {
            let python = python.context("the Python environment was not prepared")?;
            let mut command = ProcessCommand::new(&python.executable);
            command.current_dir(root);
            if let Some(endpoint) = endpoint {
                command
                    .arg(root.join("examples/python/console/main.py"))
                    .arg(endpoint);
            } else {
                // Imports the generated wheel and the example without running its main function.
                command.args([
                    "-c",
                    "import runpy; runpy.run_path('examples/python/console/main.py')",
                ]);
            }
            run_example_command(&mut command, &action)
        }
        Language::Csharp => {
            let packages =
                tempfile::tempdir().context("failed to create a temporary NuGet cache")?;
            let mut command = ProcessCommand::new("dotnet");
            command
                .current_dir(root)
                .env("NUGET_PACKAGES", packages.path());
            if let Some(endpoint) = endpoint {
                command
                    .args(["run", "--project"])
                    .arg(root.join("examples/csharp/console"))
                    .arg("--")
                    .arg(endpoint);
            } else {
                command
                    .arg("build")
                    .arg(root.join("examples/csharp/console"));
            }
            run_example_command(&mut command, &action)
        }
        Language::Java | Language::Kotlin => {
            let project = if language == Language::Java {
                "java"
            } else {
                "kotlin"
            };
            let task = if check { "classes" } else { "run" };
            let mut command = gradle_command(root);
            command
                .current_dir(root)
                .args(["-p", "examples", "--no-daemon"])
                .arg(format!(":{project}:console:{task}"));
            if let Some(endpoint) = endpoint {
                command.arg(format!("--args={endpoint}"));
            }
            run_example_command(&mut command, &action)
        }
        Language::Swift => {
            let mut command = ProcessCommand::new("swift");
            command
                .current_dir(root)
                .args([if check { "build" } else { "run" }, "--package-path"])
                .arg(root.join("examples/swift/console"));
            if let Some(endpoint) = endpoint {
                command.arg("WFObserverConsole").arg(endpoint);
            }
            run_example_command(&mut command, &action)
        }
    }
}

fn gradle_command(root: &Path) -> ProcessCommand {
    if cfg!(windows) {
        ProcessCommand::new(root.join("examples/gradlew.bat"))
    } else {
        let mut command = ProcessCommand::new("bash");
        command.arg(root.join("examples/gradlew"));
        command
    }
}

fn run_command(command: &mut ProcessCommand, action: &str) -> anyhow::Result<()> {
    let status = command
        .status()
        .with_context(|| format!("failed to {action}"))?;
    ensure!(status.success(), "failed to {action}: {status}");
    Ok(())
}

fn run_example_command(command: &mut ProcessCommand, action: &str) -> anyhow::Result<()> {
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to {action}"))?;
    let status = wait_for_exit(&mut child, EXAMPLE_TIMEOUT)?;

    let Some(status) = status else {
        child
            .kill()
            .with_context(|| format!("failed to stop the timed-out process for {action}"))?;
        child
            .wait()
            .with_context(|| format!("failed to reap the timed-out process for {action}"))?;
        bail!("timed out after {EXAMPLE_TIMEOUT:?} while attempting to {action}");
    };

    ensure!(status.success(), "failed to {action}: {status}");
    Ok(())
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + timeout;

    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(50));
    }
}
