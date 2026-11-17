//! build.rs — optional engine embedding.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert D"): when `MT_EMBED_ENGINE`
//! points at an Xray zip at BUILD time (and the `embedded` feature is on),
//! the zip is embedded via `include_bytes!` and `engine::embedded()` hands
//! those bytes to `ensure()`. Without the env var the build still succeeds
//! and `embedded()` returns `None`.
//!
//! Mechanism: the zip is copied into `OUT_DIR` (so the path is stable for
//! `include_bytes!`) and exposed through a rustc env var plus a custom cfg
//! that the source gates on. `cargo:rerun-if-env-changed` makes a rebuild
//! pick up a changed/removed embed path.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Register the custom cfg so newer cargo does not warn
    // (unexpected_cfgs) when the cfg is absent.
    println!("cargo::rustc-check-cfg=cfg(mt_platform_embed_engine)");
    println!("cargo:rerun-if-env-changed=MT_EMBED_ENGINE");

    let Ok(source) = env::var("MT_EMBED_ENGINE") else {
        return; // no embedding requested
    };
    if env::var_os("CARGO_FEATURE_EMBEDDED").is_none() {
        println!(
            "cargo:warning=MT_EMBED_ENGINE is set but the `embedded` feature is off; engine NOT embedded"
        );
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo always sets OUT_DIR"));
    let dest = out_dir.join("embedded-engine.zip");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).expect("OUT_DIR exists and is writable");
    }
    fs::copy(&source, &dest).unwrap_or_else(|e| {
        panic!("MT_EMBED_ENGINE: cannot read engine zip `{source}`: {e}");
    });

    println!(
        "cargo:rustc-env=MT_PLATFORM_EMBED_ENGINE={}",
        dest.display()
    );
    println!("cargo:rustc-cfg=mt_platform_embed_engine");
}
