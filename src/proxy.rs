use std::ffi::{OsStr, OsString};
use std::path::Path;

use crate::error::{Error, Result};

pub(crate) fn invoked_tool(invoked: &OsStr) -> Option<&str> {
    let name = Path::new(invoked).file_name()?.to_str()?;
    (name == "qli" || crate::toolchain::TOOLS.contains(&name)).then_some(name)
}

#[cfg(unix)]
pub(crate) fn run(tool: &str, mut args: &[OsString]) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;

    use crate::identity::{Selector, current_host};
    use crate::selection;
    use crate::state::Home;

    let leading = if let Some(first) = args
        .first()
        .filter(|value| value.as_bytes().starts_with(b"+"))
    {
        let selector = first
            .to_str()
            .ok_or_else(|| Error::usage("a leading +selector must be valid UTF-8"))?;
        args = &args[1..];
        Some(Selector::parse(&selector[1..]).map_err(|error| Error::usage(error.to_string()))?)
    } else {
        None
    };
    let home = Home::from_environment()?;
    let cwd = std::env::current_dir().map_err(|error| Error::operational(error.to_string()))?;
    let request = selection::request(
        &home,
        &cwd,
        leading.as_ref(),
        std::env::var_os("QLEISLIUP_TOOLCHAIN"),
    )?;
    let toolchain = selection::resolve(&home, &request, current_host()?)?;
    let executable = toolchain.executable(tool)?;
    let bin = toolchain.root.join("bin");
    let original_path = std::env::var_os("PATH");
    let mut paths = vec![bin];
    if let Some(path) = &original_path {
        paths.extend(std::env::split_paths(path));
    }
    let path = std::env::join_paths(paths).map_err(|error| {
        Error::operational(format!("cannot prepend toolchain bin to PATH: {error}"))
    })?;
    let program = c_string(executable.as_os_str())?;
    let mut arguments = vec![program.clone()];
    arguments.extend(
        args.iter()
            .map(|arg| c_string(arg))
            .collect::<Result<Vec<_>>>()?,
    );
    let mut environment: std::collections::BTreeMap<OsString, OsString> =
        std::env::vars_os().collect();
    environment.insert("PATH".into(), path);
    environment.insert("QLEISLIUP_TOOLCHAIN".into(), toolchain.name.into());
    environment.insert("QLEISLIUP_HOME".into(), home.path.into_os_string());
    let environment = environment
        .into_iter()
        .map(|(name, value)| {
            let mut entry = name;
            entry.push("=");
            entry.push(value);
            c_string(&entry)
        })
        .collect::<Result<Vec<_>>>()?;
    // execve does not search PATH or reinterpret ENOEXEC as a shell script.
    match nix::unistd::execve(&program, &arguments, &environment) {
        Ok(never) => match never {},
        Err(error) => Err(Error::file(
            &executable,
            format!("cannot execute selected tool: {error}"),
        )),
    }
}

#[cfg(unix)]
fn c_string(value: &OsStr) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(value.as_bytes())
        .map_err(|_| Error::operational("process argument or environment contains a NUL byte"))
}

#[cfg(not(unix))]
pub(crate) fn run(_tool: &str, _args: &[OsString]) -> Result<()> {
    Err(Error::operational(
        "proxy execution requires a supported Unix host",
    ))
}
