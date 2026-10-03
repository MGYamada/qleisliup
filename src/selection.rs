use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::declaration;
use crate::error::{Error, Result};
use crate::identity::{ExactVersion, Selector, release_directory};
use crate::state::{DefaultValue, Home};
use crate::toolchain::Toolchain;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    CommandLine,
    Environment,
    Repository(PathBuf),
    Default,
}

impl Source {
    pub(crate) fn description(&self) -> String {
        match self {
            Self::CommandLine => "command-line override".into(),
            Self::Environment => "QLEISLIUP_TOOLCHAIN".into(),
            Self::Repository(path) => format!("repository declaration ({})", path.display()),
            Self::Default => "global default".into(),
        }
    }
}

pub(crate) struct Request {
    pub(crate) selector: Selector,
    pub(crate) source: Source,
}

pub(crate) fn request(
    home: &Home,
    cwd: &Path,
    leading: Option<&Selector>,
    environment: Option<OsString>,
) -> Result<Request> {
    if let Some(selector) = leading {
        return Ok(Request {
            selector: selector.clone(),
            source: Source::CommandLine,
        });
    }
    if let Some(value) = environment {
        let value = value
            .to_str()
            .ok_or_else(|| Error::operational("QLEISLIUP_TOOLCHAIN must be valid UTF-8"))?;
        return Ok(Request {
            selector: Selector::parse(value)?,
            source: Source::Environment,
        });
    }
    if let Some((version, path)) = declaration::nearest(cwd)? {
        return Ok(Request {
            selector: Selector::Release(version),
            source: Source::Repository(path),
        });
    }
    match home.settings()?.default {
        DefaultValue::Version(value) => Ok(Request {
            selector: Selector::Release(ExactVersion::parse(&value)?),
            source: Source::Default,
        }),
        DefaultValue::Unset(()) => Err(Error::operational(
            "no toolchain is selected; run qleisliup default <installed-version>",
        )),
    }
}

pub(crate) fn resolve(home: &Home, request: &Request, host: &str) -> Result<Toolchain> {
    match &request.selector {
        Selector::Release(version) => {
            let root = home
                .path
                .join("toolchains")
                .join(format!("{version}-{host}"));
            match fs::symlink_metadata(&root) {
                Ok(_) => Toolchain::release(home, version, host, &home.identities()?),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let remedy = if matches!(request.source, Source::Repository(_)) {
                        "qleisliup sync".to_owned()
                    } else {
                        format!("qleisliup install {version}")
                    };
                    Err(Error::operational(format!(
                        "Qleisli toolchain {version} is required but not installed for {host}\nselected by: {}\nrun: {remedy}",
                        request.source.description()
                    )))
                }
                Err(error) => Err(Error::file(&root, error)),
            }
        }
        Selector::Stable => resolve(
            home,
            &Request {
                selector: Selector::Release(home.stable()?),
                source: request.source.clone(),
            },
            host,
        ),
        Selector::Linked(name) => {
            let links = home.links()?;
            let root = links.get(name).ok_or_else(|| Error::operational(format!("local toolchain {name:?} is not registered; run qleisliup toolchain link {name} <path>")))?;
            Toolchain::linked(name, root, host)
        }
    }
}

pub(crate) fn set_default(home: &Home, version: &ExactVersion, host: &str) -> Result<()> {
    set_default_selector(home, &Selector::Release(version.clone()), host).map(|_| ())
}

pub(crate) fn set_default_selector(
    home: &Home,
    selector: &Selector,
    host: &str,
) -> Result<ExactVersion> {
    set_default_with(home, selector, host, || {})
}

pub(crate) fn set_default_with(
    home: &Home,
    selector: &Selector,
    host: &str,
    before_lock: impl FnOnce(),
) -> Result<ExactVersion> {
    let version = default_version(home, selector)?;
    let selected = Request {
        selector: Selector::Release(version.clone()),
        source: Source::CommandLine,
    };
    resolve(home, &selected, host)?;
    home.settings()?;
    before_lock();
    let _lock = home.lock()?;
    home.settings()?;
    let version = default_version(home, selector)?;
    let selected = Request {
        selector: Selector::Release(version.clone()),
        source: Source::CommandLine,
    };
    resolve(home, &selected, host)?;
    home.save_default(&version)?;
    Ok(version)
}

fn default_version(home: &Home, selector: &Selector) -> Result<ExactVersion> {
    match selector {
        Selector::Release(version) => Ok(version.clone()),
        Selector::Stable => home.stable(),
        Selector::Linked(_) => Err(Error::usage("default accepts an exact version or stable")),
    }
}

pub(crate) fn list(home: &Home) -> Result<String> {
    let identities = home.identities()?;
    let settings = home.settings()?;
    let links = home.links()?;
    let mut entries = Vec::new();
    let directory = home.path.join("toolchains");
    let directories = match fs::read_dir(&directory) {
        Ok(directories) => {
            crate::files::managed_directory(&directory)?;
            Some(directories)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(Error::file(&directory, error)),
    };
    if let Some(directories) = directories {
        let mut releases = Vec::new();
        for entry in directories {
            let entry = entry.map_err(|error| Error::file(&directory, error))?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| Error::file(&entry.path(), "release directory name is not UTF-8"))?;
            if name == ".transactions" {
                crate::files::managed_directory(&entry.path())?;
                continue;
            }
            let (version, host) = release_directory(name)?;
            releases.push((version, host));
        }
        releases.sort();
        for (version, host) in releases {
            let toolchain = Toolchain::release(home, &version, host, &identities)?;
            let default = match &settings.default {
                DefaultValue::Version(value) if *value == version.to_string() => {
                    "; global default version"
                }
                _ => "",
            };
            entries.push(format!("{version}-{host} (release; authentication recorded at installation{default})\n  {}\n", toolchain.root.display()));
        }
    }
    for (name, path) in links {
        entries.push(format!(
            "{name} (linked, local, unauthenticated; registration only)\n  {}\n",
            path.display()
        ));
    }
    if entries.is_empty() {
        Ok("No installed toolchains or registered links.\n".to_owned())
    } else {
        Ok(entries.concat())
    }
}

pub(crate) fn show(toolchain: &Toolchain, source: &Source) -> String {
    let mut text = format!(
        "active toolchain\nname: {}\nsource: {}\nhost: {}\npath: {}\nkind: {}\nauthentication: {}\n",
        toolchain.name,
        source.description(),
        toolchain.host,
        toolchain.root.display(),
        if toolchain.linked {
            "linked, local"
        } else {
            "release, immutable under manager operations"
        },
        if toolchain.linked {
            "unauthenticated"
        } else {
            "recorded at installation; not revalidated"
        }
    );
    if let Some(manifest) = &toolchain.manifest {
        text.push_str(&format!(
            "qleisli: {}\nstd: {} ({:?})\nqargo: {}\nqargo checker Qleisli: {}\n",
            manifest.qleisli,
            manifest.std,
            manifest.std_kind,
            manifest.qargo,
            manifest.qargo_checker_qleisli
        ));
        match &manifest.verifier {
            crate::toolchain::Verifier::EmbeddedRust => {
                text.push_str("verifier: embedded Rust (no external protocol)\n")
            }
            crate::toolchain::Verifier::External { protocol, path } => text.push_str(&format!(
                "verifier: external, protocol {protocol}, {path}\n"
            )),
        }
        if manifest.qleisli != manifest.qargo_checker_qleisli {
            text.push_str(
                "qargo checking uses its linked checker, independently of the selected compiler.\n",
            );
        }
    } else {
        text.push_str("component versions: unknown (no internal manifest)\n");
    }
    text
}
