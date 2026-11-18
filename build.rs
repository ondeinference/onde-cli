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

        // An unset variable and one set to an empty string both leave the
        // build disconnected, so treat them the same. Otherwise `ONDE_APP_ID=`
        // in .env or an empty CI secret would slip past the required-creds gate
        // and ship a release that cannot reach the backend.
        let val = std::env::var(var).unwrap_or_default();
        println!("cargo:rustc-env={var}={val}");
        if val.is_empty() {
            missing.push(var);
        }
    }
    if !missing.is_empty() {
        if required {
            eprintln!(
                "error: required build variable(s) not set or empty: {}.",
                missing.join(", ")
            );
            eprintln!("  → Add them to .env for local builds.");
            eprintln!("  → Add them as GitHub Actions secrets for CI builds.");
            std::process::exit(1);
        }
        // Name only the features the missing vars actually gate. The Onde auth
        // keys gate sign-in (and everything behind it); the GresIQ keys gate
        // publishing and app assignment on their own.
        let off = if missing.iter().any(|v| v.starts_with("ONDE_")) {
            "cloud sign-in, publishing and app assignment are off"
        } else {
            "publishing and app assignment are off"
        };
        println!(
            "cargo:warning=building without some Onde credentials ({}); {off} in this build",
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
