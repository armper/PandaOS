use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Get the output directory for build artifacts
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let build_userland = env::var("CARGO_FEATURE_BUILD_USERLAND").is_ok();

    if build_userland {
        if Command::new("nasm").arg("-v").output().is_err() {
            println!(
                "cargo:warning=build-userland enabled but nasm is missing; install nasm or \
disable the feature"
            );
            panic!("build-userland requires nasm");
        }

        println!("cargo:warning=Building userland programs (build-userland enabled)...");

        let status = Command::new("bash")
            .args(["build.sh"])
            .current_dir("../userland")
            .status()
            .expect("Failed to build userland programs");

        if !status.success() {
            panic!("Failed to build userland programs");
        }
    } else {
        println!("cargo:warning=Using prebuilt userland binaries in userland/bin");
    }

    // Copy ELF binaries to output directory
    let userland_bin = PathBuf::from("../userland/bin");

    for program in &["hello", "hello1", "hello2", "init", "sh", "cat", "true", "brk_test", "mmap_test"] {
        let src = userland_bin.join(program);
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
    println!("cargo:rerun-if-changed=../userland/cat.asm");
    println!("cargo:rerun-if-changed=../userland/true.asm");
    println!("cargo:rerun-if-changed=../userland/brk_test.asm");
    println!("cargo:rerun-if-changed=../userland/mmap_test.asm");
    println!("cargo:rerun-if-changed=../userland/build.sh");
    println!("cargo:rerun-if-changed=../userland/bin/hello");
    println!("cargo:rerun-if-changed=../userland/bin/hello1");
    println!("cargo:rerun-if-changed=../userland/bin/hello2");
    println!("cargo:rerun-if-changed=../userland/bin/init");
    println!("cargo:rerun-if-changed=../userland/bin/sh");
    println!("cargo:rerun-if-changed=../userland/bin/cat");
    println!("cargo:rerun-if-changed=../userland/bin/true");
    println!("cargo:rerun-if-changed=../userland/bin/brk_test");
    println!("cargo:rerun-if-changed=../userland/bin/mmap_test");
}
