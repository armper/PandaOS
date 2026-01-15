use std::env;
use std::path::PathBuf;

fn main() {
    // Tell Cargo to rerun this build script if the userland binary changes
    println!("cargo:rerun-if-changed=../userland/build/hello");

    // Get the output directory
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Copy the userland binary to the output directory if it exists
    let userland_binary = PathBuf::from("../userland/build/hello");
    if userland_binary.exists() {
        let dest = out_dir.join("hello_elf");
        std::fs::copy(&userland_binary, &dest).expect("Failed to copy userland binary");
        println!("cargo:warning=Embedded userland binary: {:?}", dest);
    } else {
        println!("cargo:warning=Userland binary not found, will be embedded when available");
    }
}
