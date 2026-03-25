#![allow(dead_code)]

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level config file structure.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct CounterConfig {
    /// Named presets for counter display.
    #[serde(default)]
    pub presets: HashMap<String, Preset>,
}

/// A counter display preset.
#[derive(Debug, Deserialize, Clone)]
pub struct Preset {
    /// Human-readable name for the preset.
    pub name: String,
    /// Counter names to show in the counter panel. Empty = show all.
    #[serde(default)]
    pub counters: Vec<String>,
    /// Per-counter display mode overrides ("total", "rate", or "delta").
    #[serde(default)]
    pub display_modes: HashMap<String, String>,
    /// Counter names to show as timeline overlay. Empty = no overlay.
    #[serde(default)]
    pub overlay: Vec<String>,
}

/// Load counter config from the default path or next to the trace file.
pub fn load_config(trace_path: Option<&std::path::Path>) -> CounterConfig {
    // Try trace-adjacent config first.
    if let Some(tp) = trace_path {
        let adjacent = tp.with_extension("counters.toml");
        if let Some(cfg) = try_load(&adjacent) {
            return cfg;
        }
        // Also try same directory.
        if let Some(dir) = tp.parent() {
            let dir_cfg = dir.join("counters.toml");
            if let Some(cfg) = try_load(&dir_cfg) {
                return cfg;
            }
        }
    }

    // Try user config directory.
    if let Some(config_dir) = dirs_config_path() {
        if let Some(cfg) = try_load(&config_dir) {
            return cfg;
        }
    }

    CounterConfig::default()
}

fn try_load(path: &std::path::Path) -> Option<CounterConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    match toml::from_str::<CounterConfig>(&content) {
        Ok(cfg) => {
            eprintln!("Loaded counter config from {}", path.display());
            Some(cfg)
        }
        Err(e) => {
            eprintln!(
                "Warning: failed to parse counter config {}: {}",
                path.display(),
                e
            );
            None
        }
    }
}

fn dirs_config_path() -> Option<PathBuf> {
    // macOS: ~/Library/Application Support/reflex/counters.toml
    // Linux: ~/.config/reflex/counters.toml
    dirs::config_dir().map(|d| d.join("reflex").join("counters.toml"))
}

/// Helper module for platform config directory.
mod dirs {
    use std::path::PathBuf;

    pub fn config_dir() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join("Library/Application Support"))
        }
        #[cfg(not(target_os = "macos"))]
        {
            std::env::var("XDG_CONFIG_HOME")
                .ok()
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var("HOME")
                        .ok()
                        .map(|h| PathBuf::from(h).join(".config"))
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let toml_str = r#"
[presets.performance]
name = "Overall Performance"
counters = ["committed_insns", "cycles"]
overlay = ["committed_insns"]

[presets.performance.display_modes]
committed_insns = "rate"
cycles = "total"

[presets.cache]
name = "Cache Analysis"
counters = ["dcache_misses", "icache_misses"]
overlay = []

[presets.cache.display_modes]
dcache_misses = "delta"
"#;
        let cfg: CounterConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.presets.len(), 2);

        let perf = &cfg.presets["performance"];
        assert_eq!(perf.name, "Overall Performance");
        assert_eq!(perf.counters, vec!["committed_insns", "cycles"]);
        assert_eq!(perf.overlay, vec!["committed_insns"]);
        assert_eq!(perf.display_modes["committed_insns"], "rate");

        let cache = &cfg.presets["cache"];
        assert_eq!(cache.name, "Cache Analysis");
        assert_eq!(cache.display_modes["dcache_misses"], "delta");
    }

    #[test]
    fn test_empty_config() {
        let cfg: CounterConfig = toml::from_str("").unwrap();
        assert!(cfg.presets.is_empty());
    }

    #[test]
    fn test_missing_file() {
        let cfg = load_config(None);
        assert!(cfg.presets.is_empty());
    }
}
