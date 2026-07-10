use std::{
    collections::BTreeMap,
    env, fs,
    io::{self},
    path::PathBuf,
    process::{Command, ExitCode},
};

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use serde::Serialize;
use use_godot::{
    Paths, State, Variant,
    atomic::{self, StateLock},
    install::{self, InstallOptions},
    remote::ReleaseCatalog,
    resolve_installed,
    state::load_installations,
};

#[derive(Parser, Debug)]
#[command(name = "ug", version, about = "Use Godot versions safely", long_about = None)]
struct Cli {
    #[arg(long, global = true, env = "UG_ROOT", value_name = "DIR")]
    root: Option<PathBuf>,
    #[arg(long, global = true)]
    json: bool,
    #[arg(short, long, global = true)]
    quiet: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Install an official release or import a local build.
    Install {
        selector: String,
        #[arg(long)]
        variant: Option<Variant>,
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long)]
        checksum: Option<String>,
        #[arg(long)]
        platform: Option<String>,
        #[arg(long)]
        arch: Option<String>,
        #[arg(long)]
        refresh: bool,
        #[arg(long, hide = true, env = "UG_RELEASE_API")]
        api_base: Option<String>,
    },
    /// List installed versions, or official releases with --remote.
    List {
        #[arg(long)]
        remote: bool,
        #[arg(long)]
        prerelease: bool,
        #[arg(long)]
        refresh: bool,
        #[arg(long, hide = true, env = "UG_RELEASE_API")]
        api_base: Option<String>,
    },
    /// Select an installed version for the managed godot shim.
    Use { selector: String },
    /// Get, set, or clear the default selection.
    Default {
        selector: Option<String>,
        #[arg(long)]
        unset: bool,
    },
    /// Manage named selectors.
    Alias {
        #[command(subcommand)]
        command: AliasCommand,
    },
    /// Print the active identity.
    Current,
    /// Print an installed Godot executable path.
    Which { selector: Option<String> },
    /// Run one command with an installed Godot without switching.
    Exec {
        selector: String,
        #[arg(last = true, required = true)]
        args: Vec<String>,
    },
    /// Remove an installed version.
    Uninstall {
        selector: String,
        #[arg(long)]
        force: bool,
    },
    /// Diagnose managed state.
    Doctor,
    /// Emit shell setup or completion scripts.
    Shell {
        #[command(subcommand)]
        command: ShellCommand,
    },
}

#[derive(Subcommand, Debug)]
enum AliasCommand {
    Set { name: String, selector: String },
    Remove { name: String },
    List,
    Resolve { name: String },
}

#[derive(Subcommand, Debug)]
enum ShellCommand {
    Init {
        #[arg(value_enum)]
        shell: IntegrationShell,
    },
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum IntegrationShell {
    Bash,
    Fish,
    Zsh,
}

impl From<IntegrationShell> for Shell {
    fn from(value: IntegrationShell) -> Self {
        match value {
            IntegrationShell::Bash => Shell::Bash,
            IntegrationShell::Fish => Shell::Fish,
            IntegrationShell::Zsh => Shell::Zsh,
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("ug: error: {error:#}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<u8> {
    let paths = Paths::discover(cli.root.clone())?;
    let flags = OutputFlags {
        json: cli.json,
        quiet: cli.quiet,
    };
    match cli.command {
        Commands::Install {
            selector,
            variant,
            from,
            checksum,
            platform,
            arch,
            refresh,
            api_base,
        } => {
            let _lock = lock(&paths)?;
            let platform = platform.unwrap_or_else(host_platform);
            let arch = arch.unwrap_or_else(host_arch);
            let (selector, suffix_variant) = match selector.rsplit_once('@') {
                Some((base, value)) => (base.to_owned(), Some(value.parse::<Variant>()?)),
                None => (selector, None),
            };
            if variant.is_some() && suffix_variant.is_some() {
                bail!("specify the install variant either as @variant or --variant, not both");
            }
            let variant = variant.or(suffix_variant).unwrap_or(Variant::Standard);
            let item = install::install(
                &paths,
                InstallOptions {
                    selector: &selector,
                    variant,
                    platform: &platform,
                    arch: &arch,
                    from: from.as_deref(),
                    checksum: checksum.as_deref(),
                    refresh,
                    api_base: api_base.as_deref(),
                },
            )?;
            output(
                flags,
                &item,
                format!("installed {}", item.identity.display_short()),
            )?;
        }
        Commands::List {
            remote,
            prerelease,
            refresh,
            api_base,
        } => {
            if remote {
                let catalog = ReleaseCatalog::fetch(&paths, refresh, api_base.as_deref())?;
                let releases: Vec<_> = catalog
                    .releases
                    .iter()
                    .filter(|r| prerelease || !r.prerelease)
                    .map(RemoteRow::from)
                    .collect();
                if flags.json {
                    print_json(&releases)?;
                } else if !flags.quiet {
                    for row in releases {
                        println!("{}", row.tag);
                    }
                }
            } else {
                let state = State::load(&paths)?;
                let items = load_installations(&paths)?;
                if flags.json {
                    print_json(&items)?;
                } else if !flags.quiet {
                    for item in items {
                        let marker = if state.active.as_deref() == Some(&item.identity.canonical())
                        {
                            "*"
                        } else if state.default.as_deref() == Some(&item.identity.canonical()) {
                            ">"
                        } else {
                            " "
                        };
                        println!(
                            "{marker} {}\t{}",
                            item.identity.display_short(),
                            item.binary.display()
                        );
                    }
                }
            }
        }
        Commands::Use { selector } => {
            let _lock = lock(&paths)?;
            paths.ensure()?;
            let mut state = State::load(&paths)?;
            let items = load_installations(&paths)?;
            let item = resolve_installed(&selector, &state, &items)?;
            atomic::replace_symlink(&item.binary, &paths.shim())?;
            state.active = Some(item.identity.canonical());
            state.save(&paths)?;
            output(
                flags,
                &item,
                format!("using {}", item.identity.display_short()),
            )?;
        }
        Commands::Default { selector, unset } => {
            let _lock = lock(&paths)?;
            paths.ensure()?;
            let mut state = State::load(&paths)?;
            if unset {
                state.default = None;
                state.save(&paths)?;
                if !flags.quiet {
                    println!("default cleared");
                }
            } else if let Some(selector) = selector {
                let items = load_installations(&paths)?;
                let item = resolve_installed(&selector, &state, &items)?;
                state.default = Some(item.identity.canonical());
                state.save(&paths)?;
                output(
                    flags,
                    &item,
                    format!("default {}", item.identity.display_short()),
                )?;
            } else if let Some(value) = &state.default {
                print_value(flags, "default", value)?;
            } else {
                bail!("no default is set");
            }
        }
        Commands::Alias { command } => alias_command(flags, &paths, command)?,
        Commands::Current => {
            let state = State::load(&paths)?;
            let canonical = state
                .active
                .as_deref()
                .context("no active Godot; run `ug use <selector>`")?;
            let items = load_installations(&paths)?;
            let item = items
                .iter()
                .find(|i| i.identity.canonical() == canonical)
                .context("active installation is missing; run `ug doctor`")?;
            output(flags, item, item.identity.display_short())?;
        }
        Commands::Which { selector } => {
            let state = State::load(&paths)?;
            let items = load_installations(&paths)?;
            let selector = selector
                .or_else(|| state.active.clone())
                .or_else(|| state.default.clone())
                .context("no selector, active version, or default")?;
            let item = resolve_installed(&selector, &state, &items)?;
            if flags.json {
                print_json(item)?;
            } else {
                println!("{}", item.binary.display());
            }
        }
        Commands::Exec { selector, args } => {
            let state = State::load(&paths)?;
            let items = load_installations(&paths)?;
            let item = resolve_installed(&selector, &state, &items)?;
            let status = Command::new(&item.binary)
                .args(&args)
                .status()
                .with_context(|| format!("execute {}", item.binary.display()))?;
            return Ok(status.code().unwrap_or(1).clamp(0, 255) as u8);
        }
        Commands::Uninstall { selector, force } => uninstall(flags, &paths, &selector, force)?,
        Commands::Doctor => return doctor(flags, &paths),
        Commands::Shell { command } => shell_command(&paths, command)?,
    }
    Ok(0)
}

#[derive(Clone, Copy)]
struct OutputFlags {
    json: bool,
    quiet: bool,
}

fn alias_command(flags: OutputFlags, paths: &Paths, command: AliasCommand) -> Result<()> {
    let _lock = lock(paths)?;
    paths.ensure()?;
    let mut state = State::load(paths)?;
    match command {
        AliasCommand::Set { name, selector } => {
            use_godot::model::validate_component(&name, "alias")?;
            if name.parse::<Variant>().is_ok() || name.split('.').all(|p| p.parse::<u64>().is_ok())
            {
                bail!("alias '{name}' conflicts with selector syntax");
            }
            let items = load_installations(paths)?;
            let item = resolve_installed(&selector, &state, &items)?;
            state
                .aliases
                .insert(name.clone(), item.identity.canonical());
            state.save(paths)?;
            if !flags.quiet {
                println!("{name} -> {}", item.identity.display_short());
            }
        }
        AliasCommand::Remove { name } => {
            if state.aliases.remove(&name).is_none() {
                bail!("alias '{name}' does not exist");
            }
            state.save(paths)?;
            if !flags.quiet {
                println!("removed alias {name}");
            }
        }
        AliasCommand::List => {
            if flags.json {
                print_json(&state.aliases)?;
            } else if !flags.quiet {
                for (name, selector) in &state.aliases {
                    println!("{name}\t{selector}");
                }
            }
        }
        AliasCommand::Resolve { name } => {
            let items = load_installations(paths)?;
            let item = resolve_installed(&name, &state, &items)?;
            output(flags, item, item.identity.canonical())?;
        }
    }
    Ok(())
}

fn uninstall(flags: OutputFlags, paths: &Paths, selector: &str, force: bool) -> Result<()> {
    let _lock = lock(paths)?;
    let mut state = State::load(paths)?;
    let items = load_installations(paths)?;
    let item = resolve_installed(selector, &state, &items)?;
    let canonical = item.identity.canonical();
    let in_use =
        state.active.as_deref() == Some(&canonical) || state.default.as_deref() == Some(&canonical);
    if in_use && !force {
        bail!(
            "{} is active or default; pass --force to clear references and uninstall",
            item.identity.display_short()
        );
    }
    let directory = paths.install_dir(&canonical);
    let trash = paths
        .versions()
        .join(format!(".trash-{canonical}-{}", std::process::id()));
    fs::rename(&directory, &trash).with_context(|| format!("stage uninstall of {canonical}"))?;
    if state.active.as_deref() == Some(&canonical) {
        state.active = None;
        atomic::remove_symlink(&paths.shim())?;
    }
    if state.default.as_deref() == Some(&canonical) {
        state.default = None;
    }
    state.aliases.retain(|_, value| value != &canonical);
    state.save(paths)?;
    fs::remove_dir_all(&trash)?;
    if !flags.quiet {
        println!("uninstalled {}", item.identity.display_short());
    }
    Ok(())
}

#[derive(Serialize)]
struct Check {
    name: String,
    status: String,
    detail: String,
}
fn doctor(flags: OutputFlags, paths: &Paths) -> Result<u8> {
    let mut checks = Vec::new();
    let mut failed = false;
    let state = State::load(paths);
    match state {
        Ok(ref s) => checks.push(Check {
            name: "state".into(),
            status: "ok".into(),
            detail: format!("{} aliases", s.aliases.len()),
        }),
        Err(ref e) => {
            failed = true;
            checks.push(Check {
                name: "state".into(),
                status: "error".into(),
                detail: e.to_string(),
            });
        }
    }
    let installations = load_installations(paths);
    match &installations {
        Ok(items) => {
            for item in items {
                if !item.binary.is_file() {
                    failed = true;
                    checks.push(Check {
                        name: item.identity.canonical(),
                        status: "error".into(),
                        detail: format!("missing {}", item.binary.display()),
                    });
                }
            }
            checks.push(Check {
                name: "installations".into(),
                status: "ok".into(),
                detail: format!("{} found", items.len()),
            });
        }
        Err(e) => {
            failed = true;
            checks.push(Check {
                name: "installations".into(),
                status: "error".into(),
                detail: e.to_string(),
            });
        }
    }
    if let (Ok(state), Ok(items)) = (&state, &installations) {
        for (label, reference) in [("active", &state.active), ("default", &state.default)] {
            if let Some(canonical) = reference {
                if !items
                    .iter()
                    .any(|item| item.identity.canonical() == *canonical)
                {
                    failed = true;
                    checks.push(Check {
                        name: format!("{label}-reference"),
                        status: "error".into(),
                        detail: format!("missing installation {canonical}"),
                    });
                }
            }
        }
        if let Some(active) = &state.active {
            let expected = items
                .iter()
                .find(|item| item.identity.canonical() == *active)
                .map(|item| item.binary.as_path());
            match fs::read_link(paths.shim()) {
                Ok(target) if target.is_file() && expected == Some(target.as_path()) => checks
                    .push(Check {
                        name: "shim".into(),
                        status: "ok".into(),
                        detail: format!("{active} -> {}", target.display()),
                    }),
                Ok(target) => {
                    failed = true;
                    checks.push(Check {
                        name: "shim".into(),
                        status: "error".into(),
                        detail: format!(
                            "target {} does not match active installation",
                            target.display()
                        ),
                    });
                }
                Err(e) => {
                    failed = true;
                    checks.push(Check {
                        name: "shim".into(),
                        status: "error".into(),
                        detail: e.to_string(),
                    });
                }
            }
        } else if paths.shim().is_symlink() || paths.shim().exists() {
            failed = true;
            checks.push(Check {
                name: "shim".into(),
                status: "error".into(),
                detail: "shim exists but state has no active installation".into(),
            });
        }
    }
    let leftovers = fs::read_dir(paths.versions())
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| {
                    let n = e.file_name();
                    let n = n.to_string_lossy();
                    n.starts_with(".staging-") || n.starts_with(".trash-")
                })
                .count()
        })
        .unwrap_or(0);
    checks.push(Check {
        name: "recovery".into(),
        status: if leftovers == 0 { "ok" } else { "warning" }.into(),
        detail: format!("{leftovers} recoverable staging/trash directories"),
    });
    if flags.json {
        print_json(&checks)?;
    } else if !flags.quiet {
        for c in checks {
            println!("{:<14} {:<9} {}", c.name, c.status, c.detail);
        }
    }
    Ok(if failed { 2 } else { 0 })
}

fn shell_command(paths: &Paths, command: ShellCommand) -> Result<()> {
    match command {
        ShellCommand::Init { shell } => {
            let executable = env::current_exe().context("locate ug executable")?;
            let binary_dir = executable.parent().context("ug executable has no parent")?;
            match shell {
                IntegrationShell::Bash | IntegrationShell::Zsh => {
                    println!(
                        "export PATH={}:{}:$PATH",
                        shell_single_quote(&paths.shims().to_string_lossy()),
                        shell_single_quote(&binary_dir.to_string_lossy())
                    );
                    if matches!(shell, IntegrationShell::Zsh) {
                        println!("autoload -Uz compinit && compinit");
                    }
                }
                IntegrationShell::Fish => println!(
                    "fish_add_path --prepend --move {} {}",
                    shell_single_quote(&paths.shims().to_string_lossy()),
                    shell_single_quote(&binary_dir.to_string_lossy())
                ),
            }
            let mut command = Cli::command();
            generate(Shell::from(shell), &mut command, "ug", &mut io::stdout());
        }
        ShellCommand::Completions { shell } => {
            let mut command = Cli::command();
            generate(shell, &mut command, "ug", &mut io::stdout());
        }
    }
    Ok(())
}

fn lock(paths: &Paths) -> Result<StateLock> {
    paths.ensure()?;
    StateLock::acquire(&paths.lock())
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn host_platform() -> String {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(windows) {
        "windows"
    } else {
        env::consts::OS
    }
    .into()
}
fn host_arch() -> String {
    match env::consts::ARCH {
        "aarch64" => "arm64",
        "x86" => "x86_32",
        "x86_64" => "x86_64",
        other => other,
    }
    .into()
}
fn print_json(value: &impl Serialize) -> Result<()> {
    serde_json::to_writer_pretty(io::stdout().lock(), value)?;
    println!();
    Ok(())
}
fn output(flags: OutputFlags, value: &impl Serialize, text: String) -> Result<()> {
    if flags.json {
        print_json(value)
    } else if !flags.quiet {
        println!("{text}");
        Ok(())
    } else {
        Ok(())
    }
}
fn print_value(flags: OutputFlags, key: &str, value: &str) -> Result<()> {
    if flags.json {
        let mut map = BTreeMap::new();
        map.insert(key, value);
        print_json(&map)
    } else {
        println!("{value}");
        Ok(())
    }
}

#[derive(Serialize)]
struct RemoteRow {
    tag: String,
    prerelease: bool,
    published_at: Option<String>,
}
impl From<&use_godot::Release> for RemoteRow {
    fn from(r: &use_godot::Release) -> Self {
        Self {
            tag: r.tag_name.clone(),
            prerelease: r.prerelease,
            published_at: r.published_at.clone(),
        }
    }
}
