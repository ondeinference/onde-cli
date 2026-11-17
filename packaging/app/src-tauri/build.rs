fn main() {
    // The release scripts export the CFBundleVersion here so the app can show
    // it. Tauri's own config does not reach the running app for this value.
    println!("cargo:rerun-if-env-changed=ONDE_DESKTOP_BUILD");
    if let Ok(build) = std::env::var("ONDE_DESKTOP_BUILD") {
        println!("cargo:rustc-env=ONDE_DESKTOP_BUILD={build}");
    }
    tauri_build::build()
}
