//! Standalone process fixture; never invokes an upstream compiler or package manager.
#![forbid(unsafe_code)]

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::process::Command;

fn hex(value: &[u8]) -> String {
    let mut text = String::with_capacity(value.len() * 2);
    for byte in value {
        write!(&mut text, "{byte:02x}").unwrap();
    }
    text
}

fn show(name: &str, value: &OsStr) {
    println!("{name}:{}", hex(value.as_bytes()));
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "__signal") {
        Command::new("/bin/kill")
            .args(["-TERM", &std::process::id().to_string()])
            .status()
            .unwrap();
        panic!("SIGTERM did not terminate the selected process");
    }
    if args
        .get(1)
        .is_some_and(|arg| arg == "__nested" || arg == "__sibling")
    {
        let executable = if args[1] == "__sibling" {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .join("qlippy")
        } else {
            std::path::PathBuf::from(std::env::var_os("QLEISLIUP_FIXTURE_PROXY").unwrap())
        };
        let mut child = Command::new(executable);
        if let Some(cwd) = std::env::var_os("QLEISLIUP_FIXTURE_CWD") {
            child.current_dir(cwd);
        }
        let output = child.arg("nested argument").output().unwrap();
        std::io::stdout().write_all(&output.stdout).unwrap();
        std::io::stderr().write_all(&output.stderr).unwrap();
        std::process::exit(output.status.code().unwrap_or(1));
    }
    println!("pid:{}", std::process::id());
    show("argv0", &args[0]);
    show("cwd", std::env::current_dir().unwrap().as_os_str());
    for name in ["QLEISLIUP_TOOLCHAIN", "QLEISLIUP_HOME", "PATH"] {
        show(name, &std::env::var_os(name).unwrap_or_default());
    }
    for arg in &args[1..] {
        show("arg", arg);
    }
    if args.get(1).is_some_and(|arg| arg == "__echo") {
        let mut input = Vec::new();
        std::io::stdin().read_to_end(&mut input).unwrap();
        println!("stdin:{}", hex(&input));
        std::io::stderr()
            .write_all(b"fixture stderr\0\xff\n")
            .unwrap();
    }
    let exit = std::env::var("QLEISLIUP_FIXTURE_EXIT")
        .ok()
        .map(|value| value.parse().unwrap())
        .unwrap_or(0);
    std::process::exit(exit);
}
