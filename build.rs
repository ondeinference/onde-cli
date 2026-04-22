fn main() {
    // Load .env if present (local dev). In CI the vars are injected directly
    // as environment variables, so this is a no-op there.
    let _ = dotenvy::dotenv();

    for var in [
        "ONDE_APP_ID",
        "ONDE_APP_SECRET",
        "GRESIQ_API_KEY",
        "GRESIQ_API_SECRET",
    ] {
        // Re-run the build script if any of these change.
        println!("cargo:rerun-if-env-changed={var}");

        match std::env::var(var) {
            Ok(val) => {
                // Forward the value to the compilation unit so env!() resolves.
                println!("cargo:rustc-env={var}={val}");
            }
            Err(_) => {
                // Fail the build loudly rather than silently baking in an
                // empty string. Add the missing variable to .env (local) or
                // to the CI environment (GitHub Actions secrets).
                eprintln!("error: required build variable `{var}` is not set.");
                eprintln!("  → Add it to .env for local builds.");
                eprintln!("  → Add it as a GitHub Actions secret for CI builds.");
                std::process::exit(1);
            }
        }
    }

    // Rebuild whenever .env itself changes.
    println!("cargo:rerun-if-changed=.env");
}
