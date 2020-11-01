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

    let rev = Command::new("git")
        .arg("rev-parse")
        .arg("--short")
        .arg("HEAD")
        .output()
        .unwrap();

    let status = Command::new("git").arg("status").arg("--porcelain").output().unwrap();

    if rev.status.success() && status.status.success() {
        println!("cargo:rerun-if-changed=../../.git/index");
        println!("cargo:rerun-if-changed=../");

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

    println!(
        "cargo:rustc-env=RUSTC_VERSION={}",
        str::from_utf8(&ver.stdout).unwrap().trim()
    );
}
