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

    let ver = Command::new(env::var("RUSTC").unwrap())
        .arg("--version")
        .output()
        .unwrap();

    println!(
        "cargo:rustc-env=RUSTC_VERSION={}",
        str::from_utf8(&ver.stdout).unwrap().trim()
    );
}
