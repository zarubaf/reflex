//! Session persistence: save and restore UI state across app restarts.
//!
//! Session files are stored alongside the trace file (`.trace.uscope.session`)
//! with fallback to `~/.config/reflex/sessions/` when the trace directory is
//! read-only.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

const SESSION_VERSION: u32 = 1;

// ── Data structures ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub tabs: Vec<TabState>,
    pub active_tab: usize,
    /// Dock placement preset: "bottom", "left", or "right".
    pub dock_placement: String,
}

#[derive(Serialize, Deserialize)]
pub struct TabState {
    pub file_path: String,
    pub viewport: ViewportSnapshot,
    pub cursors: CursorSnapshot,
    pub counter_state: CounterPanelSnapshot,
}

#[derive(Serialize, Deserialize)]
pub struct ViewportSnapshot {
    pub scroll_cycle: f64,
    pub scroll_row: f64,
    pub pixels_per_cycle: f32,
    pub row_height: f32,
}

#[derive(Serialize, Deserialize)]
pub struct CursorSnapshot {
    pub cursors: Vec<CursorEntry>,
    pub active_idx: usize,
}

#[derive(Serialize, Deserialize)]
pub struct CursorEntry {
    pub cycle: f64,
    pub color_idx: usize,
}

#[derive(Serialize, Deserialize)]
pub struct CounterPanelSnapshot {
    /// Per-counter display mode: "Total", "Rate", "Delta", or null for default.
    #[serde(default)]
    pub display_modes: Vec<Option<String>>,
    /// "Detail" or "Heatmap".
    #[serde(default = "default_view_mode")]
    pub view_mode: String,
    /// Selected counter index for the minimap trendline.
    #[serde(default)]
    pub selected_counter: Option<usize>,
    /// Counter panel cycle range override. None = full trace.
    #[serde(default)]
    pub counter_range: Option<(u32, u32)>,
    /// Counter index shown as overlay on the pipeline. None = no overlay.
    #[serde(default)]
    pub overlay_counter: Option<usize>,
}

fn default_view_mode() -> String {
    "Detail".to_string()
}

// ── File location ────────────────────────────────────────────────────

/// Compute the session file path for a given trace file.
/// Prefers trace-adjacent, falls back to config directory.
pub fn session_path_for_save(trace_path: &Path) -> PathBuf {
    // 1. Try trace-adjacent: .filename.session
    if let Some(adjacent) = adjacent_session_path(trace_path) {
        if let Some(parent) = adjacent.parent() {
            // Check if directory is writable by attempting a metadata read.
            if parent.exists()
                && std::fs::metadata(parent)
                    .map(|m| !m.permissions().readonly())
                    .unwrap_or(false)
            {
                return adjacent;
            }
        }
    }
    // 2. Fall back to config dir.
    config_session_path(trace_path)
}

/// Find an existing session file for a trace (check adjacent first, then config dir).
pub fn find_session_file(trace_path: &Path) -> Option<PathBuf> {
    if let Some(adj) = adjacent_session_path(trace_path) {
        if adj.exists() {
            return Some(adj);
        }
    }
    let cfg = config_session_path(trace_path);
    if cfg.exists() {
        return Some(cfg);
    }
    None
}

fn adjacent_session_path(trace_path: &Path) -> Option<PathBuf> {
    let parent = trace_path.parent()?;
    let name = trace_path.file_name()?.to_string_lossy();
    Some(parent.join(format!(".{}.session", name)))
}

fn config_session_path(trace_path: &Path) -> PathBuf {
    let canonical = trace_path
        .canonicalize()
        .unwrap_or_else(|_| trace_path.to_path_buf());
    let mut hasher = DefaultHasher::new();
    canonical.to_string_lossy().hash(&mut hasher);
    let hash = format!("{:016x}", hasher.finish());
    let base = if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".config"))
            })
    };
    let dir = base
        .unwrap_or_else(|| PathBuf::from("."))
        .join("reflex")
        .join("sessions");
    dir.join(format!("{}.json", hash))
}

// ── Save / Load ──────────────────────────────────────────────────────

pub fn save_session(session: &Session, path: &Path) {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    match serde_json::to_string_pretty(session) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                eprintln!(
                    "Warning: failed to save session to {}: {}",
                    path.display(),
                    e
                );
            }
        }
        Err(e) => {
            eprintln!("Warning: failed to serialize session: {}", e);
        }
    }
}

pub fn load_session(trace_path: &Path) -> Option<Session> {
    let path = find_session_file(trace_path)?;
    let data = std::fs::read_to_string(&path).ok()?;
    let session: Session = match serde_json::from_str(&data) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Warning: failed to parse session {}: {}", path.display(), e);
            return None;
        }
    };
    if session.version > SESSION_VERSION {
        eprintln!(
            "Warning: session file version {} is newer than supported ({}), ignoring",
            session.version, SESSION_VERSION
        );
        return None;
    }
    Some(session)
}
