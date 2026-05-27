fn main() {
    println!(
        "cargo:rustc-env=SUBUNIT_TARGET_TRIPLE={}",
        std::env::var("TARGET").expect("TARGET is set by Cargo for build scripts")
    );
    println!("cargo:rerun-if-changed=../scripts/sidecar-sha256.txt");
    tauri_build::build()
}
