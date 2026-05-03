fn main() {
    // Load .env for local builds. In CI these vars already come from the environment.
    let _ = dotenvy::dotenv();

    // These must be present or the build should fail.
    for var in [
        "ONDE_APP_ID",
        "ONDE_APP_SECRET",
        "GRESIQ_API_KEY",
        "GRESIQ_API_SECRET",
    ] {
        println!("cargo:rerun-if-env-changed={var}");

        match std::env::var(var) {
            Ok(val) => {
                println!("cargo:rustc-env={var}={val}");
            }
            Err(_) => {
                eprintln!("error: required build variable `{var}` is not set.");
                eprintln!("  → Add it to .env for local builds.");
                eprintln!("  → Add it as a GitHub Actions secret for CI builds.");
                std::process::exit(1);
            }
        }
    }

    // Optional vars: bake them in when present, use an empty string otherwise.
    {
        let var = "HF_TOKEN";
        println!("cargo:rerun-if-env-changed={var}");
        match std::env::var(var) {
            Ok(val) => {
                println!("cargo:rustc-env={var}={val}");
            }
            Err(_) => {
                // Keep env!() happy even when the token is missing.
                println!("cargo:rustc-env={var}=");
            }
        }
    }

    // Re-run the build script if .env changes.
    println!("cargo:rerun-if-changed=.env");
}
