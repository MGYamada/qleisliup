//! Native manager version fixture, compiled only by the test harness.
#![forbid(unsafe_code)]

fn main() {
    let executable = std::env::current_exe().unwrap();
    let directory = executable.parent().unwrap();
    if std::env::args().nth(1).as_deref() == Some("--probe-helper") {
        std::fs::write(
            directory.join("helper.ready"),
            std::process::id().to_string(),
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !directory.join("helper.release").exists() {
            if std::time::Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::fs::write(directory.join("helper.survived"), b"survived the probe").unwrap();
        return;
    }
    let version = option_env!("QLEISLIUP_FIXTURE_MANAGER_VERSION").unwrap_or("0.1.0");
    let behavior = option_env!("QLEISLIUP_FIXTURE_MANAGER_BEHAVIOR").unwrap_or("valid");
    let behavior = if let Some(behavior) = behavior.strip_prefix("helper-") {
        let mut helper = std::process::Command::new(&executable)
            .arg("--probe-helper")
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !directory.join("helper.ready").exists() {
            if std::time::Instant::now() >= deadline {
                helper.kill().unwrap();
                helper.wait().unwrap();
                std::process::exit(18);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        behavior
    } else {
        behavior
    };
    match behavior {
        "hang" => std::thread::sleep(std::time::Duration::from_secs(60)),
        "noise" => println!("{}", "x".repeat(4096)),
        "exit" => std::process::exit(17),
        "version" => println!("qleisliup 9.9.9"),
        _ => println!("qleisliup {version}"),
    }
}
