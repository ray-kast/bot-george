use std::{env, process::Command, str};

fn main() {
    for var in &["HOST", "TARGET", "PROFILE"] {
        println!("cargo:rustc-env=BUILD_{}={}", var, env::var(var).unwrap());
    }

    println!(
        "cargo:rustc-env=BUILD_FEATURES=ptr{},{},{}",
        env::var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap(),
        env::var("CARGO_CFG_TARGET_ENDIAN").unwrap(),
        env::var("CARGO_CFG_TARGET_FEATURE").unwrap(),
    );

    let toplevel = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .unwrap();

    if toplevel.status.success() {
        let toplevel = str::from_utf8(&toplevel.stdout).unwrap().trim();

        let rev = Command::new("git")
            .args(&["rev-parse", "--short", "HEAD"])
            .current_dir(toplevel)
            .output()
            .unwrap();

        let status = Command::new("git")
            .args(&["status", "--porcelain"])
            .current_dir(toplevel)
            .output()
            .unwrap();

        let ls_files = Command::new("git")
            .args(&["ls-files", "--full-name"])
            .current_dir(toplevel)
            .output()
            .unwrap();

        assert!(
            rev.status.success(),
            "git rev-parse exited with code {}",
            rev.status
                .code()
                .map_or_else(|| "unknown".into(), |s| s.to_string())
        );

        assert!(
            status.status.success(),
            "git status exited with code {}",
            status
                .status
                .code()
                .map_or_else(|| "unknown".into(), |s| s.to_string())
        );

        assert!(
            ls_files.status.success(),
            "git ls-files exited with code {}",
            ls_files
                .status
                .code()
                .map_or_else(|| "unknown".into(), |s| s.to_string())
        );

        println!("cargo:rerun-if-changed={}/.git/index", toplevel);

        for file in str::from_utf8(&ls_files.stdout)
            .unwrap()
            .split('\n')
            .map(|f| f.trim())
        {
            println!("cargo:rerun-if-changed={}/{}", toplevel, file);
        }

        for file in str::from_utf8(&status.stdout)
            .unwrap()
            .split('\n')
            .map(|f| f.trim())
            .filter(|f| f.len() > 2)
            .map(|f| &f[2..])
        {
            println!("cargo:rerun-if-changed={}/{}", toplevel, file);
        }

        println!(
            "cargo:rustc-env=GIT_HEAD={}{}",
            str::from_utf8(&rev.stdout).unwrap().trim(),
            if status.stdout.is_empty() {
                ""
            } else {
                "-DIRTY"
            }
        );
    }

    let ver = Command::new(env::var("RUSTC").unwrap())
        .arg("--version")
        .output()
        .unwrap();

    assert!(
        ver.status.success(),
        "rustc --version exited with code {}",
        ver.status
            .code()
            .map_or_else(|| "unknown".into(), |s| s.to_string())
    );

    println!(
        "cargo:rustc-env=RUSTC_VERSION={}",
        str::from_utf8(&ver.stdout).unwrap().trim()
    );
}
