use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Get the output directory for build artifacts
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Build userland programs
    println!("cargo:warning=Building userland programs...");

    let status = Command::new("bash")
        .args(&["build.sh"])
        .current_dir("../userland")
        .status()
        .expect("Failed to build userland programs");

    if !status.success() {
        panic!("Failed to build userland programs");
    }

    // Copy ELF binaries to output directory
    let userland_build = PathBuf::from("../userland/build");

    for program in &["hello", "hello1", "hello2", "init", "sh"] {
        let src = userland_build.join(program);
        let dst = out_dir.join(format!("{}_elf", program));

        if src.exists() {
            std::fs::copy(&src, &dst)
                .unwrap_or_else(|_| panic!("Failed to copy {} to output directory", program));

            println!("cargo:warning=Embedded userland binary: {:?}", dst);
        } else {
            println!("cargo:warning=Userland binary {} not found", program);
        }
    }

    // Tell cargo to rerun if userland sources change
    println!("cargo:rerun-if-changed=../userland/hello.asm");
    println!("cargo:rerun-if-changed=../userland/hello1.asm");
    println!("cargo:rerun-if-changed=../userland/hello2.asm");
    println!("cargo:rerun-if-changed=../userland/init.asm");
    println!("cargo:rerun-if-changed=../userland/sh.asm");
    println!("cargo:rerun-if-changed=../userland/build.sh");
    println!("cargo:rerun-if-changed=../userland/build/hello");
}
