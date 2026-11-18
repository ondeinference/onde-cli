fn main() {
    // Load .env for local builds. In CI these vars already come from the environment.
    let _ = dotenvy::dotenv();

    // Official builds carry the Onde credentials and must never ship without
    // them, so the release workflows set ONDE_REQUIRE_CREDENTIALS and a missing
    // value stops the build. Anyone else (a contributor, `cargo install`) gets a
    // build with empty credentials: fine-tuning, merging, export and eval all
    // work, and the cloud features say that this build is not connected.
    println!("cargo:rerun-if-env-changed=ONDE_REQUIRE_CREDENTIALS");
    let required = std::env::var("ONDE_REQUIRE_CREDENTIALS").is_ok_and(|v| !v.is_empty());
    let mut missing = Vec::new();
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
                println!("cargo:rustc-env={var}=");
                missing.push(var);
            }
        }
    }
    if !missing.is_empty() {
        if required {
            eprintln!(
                "error: required build variable(s) not set: {}.",
                missing.join(", ")
            );
            eprintln!("  → Add them to .env for local builds.");
            eprintln!("  → Add them as GitHub Actions secrets for CI builds.");
            std::process::exit(1);
        }
        println!(
            "cargo:warning=building without Onde credentials ({}); cloud sign-in is off in this build",
            missing.join(", ")
        );
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
