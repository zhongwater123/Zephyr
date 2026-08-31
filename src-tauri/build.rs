use std::env;
use std::fs;
use std::path::Path;

fn local_environment_value(source: &str, name: &str) -> Option<String> {
    source.lines().find_map(|raw_line| {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (candidate, raw_value) = line.split_once('=')?;
        if candidate.trim() != name {
            return None;
        }
        let value = raw_value.trim().trim_matches(['"', '\'']);
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn deployment_value(local_source: Option<&str>, name: &str) -> Option<String> {
    local_source
        .and_then(|source| local_environment_value(source, name))
        .or_else(|| env::var(name).ok().filter(|value| !value.trim().is_empty()))
}

fn main() {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .expect("CARGO_MANIFEST_DIR is required");
    let local_env_path = manifest_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".env.local");
    println!("cargo:rerun-if-changed={}", local_env_path.display());
    println!("cargo:rerun-if-env-changed=GY_TYPING_ASR_API_KEY");
    println!("cargo:rerun-if-env-changed=GY_TYPING_ASR_AUTH_MODE");
    println!("cargo:rerun-if-env-changed=GY_TYPING_DEEPSEEK_API_KEY");

    let local_source = fs::read_to_string(&local_env_path).ok();
    for name in [
        "GY_TYPING_ASR_API_KEY",
        "GY_TYPING_ASR_AUTH_MODE",
        "GY_TYPING_DEEPSEEK_API_KEY",
    ] {
        if let Some(value) = deployment_value(local_source.as_deref(), name) {
            println!("cargo:rustc-env={name}={value}");
        }
    }
    tauri_build::build()
}
