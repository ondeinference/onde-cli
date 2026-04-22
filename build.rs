fn main() {
    // Load .env if present (local dev). In CI the vars are injected directly
    // as environment variables, so this is a no-op there.
    let _ = dotenvy::dotenv();

    // Required variables — build fails if missing.
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

    // Optional variables — baked in when present, empty string when absent.
    {
        let var = "HF_TOKEN";
        println!("cargo:rerun-if-env-changed={var}");
        match std::env::var(var) {
            Ok(val) => {
                println!("cargo:rustc-env={var}={val}");
            }
            Err(_) => {
                // Forward an empty value so env!() still compiles.
                println!("cargo:rustc-env={var}=");
            }
        }
    }

    // Rebuild whenever .env itself changes.
    println!("cargo:rerun-if-changed=.env");
}
