use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

use crate::declaration;
use crate::error::{Error, Result};
use crate::identity::{ExactVersion, Selector, current_host, valid_link_name};
use crate::selection;
use crate::state::Home;

const HELP: &str = "qleisliup - Qleisli toolchain lifecycle management

Current implementation: Stage 4, bootstrap and manager self-update.
Offline selection, local links, and Unix proxies are implemented.
Production distribution URLs and trusted root are not configured.
New installs, bootstrap, and updates fail closed until production trust is established.

Usage:
  qleisliup --help | -h
  qleisliup --version | -V
  qleisliup                      Print this help
  qleisliup list
  qleisliup [+selector] show
  qleisliup [+selector] which [tool]
  qleisliup default <version|stable>
  qleisliup pin <version>
  qleisliup toolchain link <name> <path>
  qleisliup toolchain unlink <name>
  qleisliup install <version|stable>
  qleisliup uninstall <version>
  qleisliup [+selector] sync
  qleisliup self update

Selectors: exact SemVer, stable (cached authenticated observation), or a local name.
Selection: +selector > QLEISLIUP_TOOLCHAIN > nearest declaration > global default.
which defaults to qleisli; qli is its alias. Inspection never runs a tool.
Proxy names: qli, qleisli, qargo, qlippy, qlifmt, qlidoc (symlinks to this manager).
Proxies accept a leading +selector and exec the absolute selected tool offline.
Links are local and unauthenticated; missing tools never fall back to PATH.
qleisliup-init installs the manager and proxies; shell configuration is unchanged.
Only install/sync, bootstrap, and self update may refresh/download.
Existing exact toolchain installs are reused offline.
sync uses the nearest repository declaration, ignoring selection overrides.
default stable stores an installed exact version and never follows later updates.

Self update requires a bootstrap-owned manager; external installations are refused.

Design contract: docs/specification.md
Implementation stages: docs/implementation-plan.md
";

enum Command {
    Help,
    Version,
    List,
    Show(Option<Selector>),
    Which(Option<Selector>, String),
    Default(Selector),
    Pin(ExactVersion),
    Link(String, PathBuf),
    Unlink(String),
    Install(Selector),
    Uninstall(ExactVersion),
    Sync,
    SelfUpdate,
}

fn parse(mut args: &[OsString]) -> Result<Command> {
    let leading = if let Some(value) = args
        .first()
        .and_then(|value| value.to_str())
        .and_then(|value| value.strip_prefix('+'))
    {
        args = &args[1..];
        Some(Selector::parse(value).map_err(|error| Error::usage(error.to_string()))?)
    } else {
        None
    };
    if leading.is_some()
        && !args
            .first()
            .is_some_and(|arg| arg == "show" || arg == "which" || arg == "sync")
    {
        return Err(Error::usage(
            "a leading +selector is supported only for show, which, and sync (where it is ignored)",
        ));
    }
    match args {
        [] => Ok(Command::Help),
        [arg] if arg == "--help" || arg == "-h" => Ok(Command::Help),
        [arg] if arg == "--version" || arg == "-V" => Ok(Command::Version),
        [arg] if arg == "list" => Ok(Command::List),
        [arg] if arg == "show" => Ok(Command::Show(leading)),
        [arg] if arg == "which" => Ok(Command::Which(leading, "qleisli".into())),
        [arg, tool] if arg == "which" => {
            let tool = tool
                .to_str()
                .ok_or_else(|| Error::usage("tool name must be UTF-8"))?;
            if tool != "qli" && !crate::toolchain::TOOLS.contains(&tool) {
                return Err(Error::usage(
                    "unknown tool; expected qli, qleisli, qargo, qlippy, qlifmt, or qlidoc",
                ));
            }
            Ok(Command::Which(leading, tool.to_owned()))
        }
        [arg, value] if arg == "default" => {
            let value = value
                .to_str()
                .ok_or_else(|| Error::usage("version must be UTF-8"))?;
            if value == "stable" {
                Ok(Command::Default(Selector::Stable))
            } else {
                Ok(Command::Default(Selector::Release(
                    ExactVersion::parse(value).map_err(|error| Error::usage(error.to_string()))?,
                )))
            }
        }
        [arg, value] if arg == "pin" => {
            let value = value
                .to_str()
                .ok_or_else(|| Error::usage("version must be UTF-8"))?;
            Ok(Command::Pin(
                ExactVersion::parse(value).map_err(|error| Error::usage(error.to_string()))?,
            ))
        }
        [family, command, name, path] if family == "toolchain" && command == "link" => {
            Ok(Command::Link(local_name(name)?, PathBuf::from(path)))
        }
        [family, command, name] if family == "toolchain" && command == "unlink" => {
            Ok(Command::Unlink(local_name(name)?))
        }
        [command, value] if command == "install" => {
            let value = value
                .to_str()
                .ok_or_else(|| Error::usage("version must be UTF-8"))?;
            let selector = if value == "stable" {
                Selector::Stable
            } else {
                Selector::Release(
                    ExactVersion::parse(value).map_err(|e| Error::usage(e.to_string()))?,
                )
            };
            Ok(Command::Install(selector))
        }
        [command, value] if command == "uninstall" => {
            let value = value
                .to_str()
                .ok_or_else(|| Error::usage("version must be UTF-8"))?;
            Ok(Command::Uninstall(
                ExactVersion::parse(value).map_err(|e| Error::usage(e.to_string()))?,
            ))
        }
        [command] if command == "sync" => Ok(Command::Sync),
        [family, command] if family == "self" && command == "update" => Ok(Command::SelfUpdate),
        _ => Err(Error::usage("unsupported arguments; run qleisliup --help")),
    }
}

fn local_name(value: &OsString) -> Result<String> {
    let name = value.to_str().filter(|name| valid_link_name(name)).ok_or_else(|| {
        Error::usage("invalid local name; use an ASCII letter followed by letters, digits, _ or -; stable/beta/nightly are reserved")
    })?;
    Ok(name.to_owned())
}

pub(crate) fn run(args: &[OsString]) -> Result<()> {
    let output = match parse(args)? {
        Command::Help => HELP.to_owned(),
        Command::Version => format!("qleisliup {}\n", env!("CARGO_PKG_VERSION")),
        Command::List => selection::list(&Home::from_environment()?)?,
        Command::Show(leading) => inspect(leading.as_ref(), None)?,
        Command::Which(leading, tool) => inspect(leading.as_ref(), Some(&tool))?,
        Command::Default(Selector::Release(version)) => {
            selection::set_default(&Home::from_environment()?, &version, current_host()?)?;
            format!("global default: {version}\n")
        }
        Command::Default(Selector::Stable) => {
            let home = Home::from_environment()?;
            let version = home.stable()?;
            selection::set_default(&home, &version, current_host()?)?;
            format!("global default: {version}\n")
        }
        Command::Default(Selector::Linked(_)) => {
            return Err(Error::usage("default accepts an exact version or stable"));
        }
        Command::Pin(version) => {
            let cwd =
                std::env::current_dir().map_err(|error| Error::operational(error.to_string()))?;
            let path = declaration::pin(&Home::from_environment()?, &cwd, &version)?;
            format!("pinned {version} in {}\n", path.display())
        }
        Command::Link(name, path) => {
            let root =
                crate::links::link(&Home::from_environment()?, &name, &path, current_host()?)?;
            format!(
                "linked {name} -> {} (local, unauthenticated)\n",
                root.display()
            )
        }
        Command::Unlink(name) => {
            crate::links::unlink(&Home::from_environment()?, &name)?;
            format!("unlinked {name}\n")
        }
        Command::Install(selector) => {
            let version =
                crate::install::install(&Home::from_environment()?, &selector, current_host()?)?;
            format!("installed toolchain available: {version}\n")
        }
        Command::Uninstall(version) => {
            crate::install::uninstall(&Home::from_environment()?, &version, current_host()?)?;
            format!("uninstalled {version}; remembered identity retained\n")
        }
        Command::Sync => {
            let cwd = std::env::current_dir().map_err(|e| Error::operational(e.to_string()))?;
            let version = crate::install::sync_version(&cwd)?;
            crate::install::install(
                &Home::from_environment()?,
                &Selector::Release(version.clone()),
                current_host()?,
            )?;
            format!("repository toolchain available: {version}\n")
        }
        Command::SelfUpdate => {
            let version = crate::manager::update(&Home::from_environment()?, current_host()?)?;
            format!("managed qleisliup available: {version}\n")
        }
    };
    std::io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|error| Error::operational(format!("cannot write output: {error}")))
}

fn inspect(leading: Option<&Selector>, tool: Option<&str>) -> Result<String> {
    let home = Home::from_environment()?;
    let cwd = std::env::current_dir().map_err(|error| Error::operational(error.to_string()))?;
    let request = selection::request(
        &home,
        &cwd,
        leading,
        std::env::var_os("QLEISLIUP_TOOLCHAIN"),
    )?;
    let toolchain = selection::resolve(&home, &request, current_host()?)?;
    match tool {
        Some(tool) => Ok(format!("{}\n", toolchain.executable(tool)?.display())),
        None => Ok(selection::show(&toolchain, &request.source)),
    }
}
