use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/eclipse.ico");
    println!("cargo:rerun-if-changed=assets/eclipse.rc");
    if !env::var("TARGET").unwrap_or_default().contains("windows-msvc") { return; }
    let output=PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("eclipse.res");
    let status=Command::new("rc.exe")
        .args(["/nologo", "/I", "assets"])
        .arg(format!("/fo{}", output.display()))
        .arg("assets/eclipse.rc")
        .status().expect("Windows SDK Resource Compiler (rc.exe) is required");
    assert!(status.success(), "Could not compile Eclipse Windows resources");
    println!("cargo:rustc-link-arg-bin=eclipse-native={}", output.display());
}
