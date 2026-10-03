//! Thème d'Omarchy (Linux, Hyprland) : quand il est présent, l'interface en reprend les
//! couleurs (préférence de thème « Système »).
//!
//! Thème courant : `~/.local/state/omarchy/current/theme` (Omarchy récent, nom dans
//! `theme.name`), sinon le lien `~/.config/omarchy/current/theme` (versions antérieures).
//! `colors.toml` : `mode`, `accent`, `background`, `foreground`… (format récent) ou
//! `accent`, `background`, `foreground`, `color0`–`color15` (format antérieur, fichier vide
//! `light.mode` pour un thème clair). Les couleurs sont transmises telles quelles ; l'interface
//! les traduit en couleurs de l'app (`src/lib/omarchy.ts`).
//!
//! `omarchy-theme-set` remplace le dossier du thème courant : on relit le thème quand son
//! nom ou la date de `colors.toml` change (vérification chaque seconde, sans toucher aux
//! scripts de l'utilisateur).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct OmarchyTheme {
    /// Nom du thème (« tokyo-night »).
    pub name: String,
    /// Thème sombre (`mode`, ou absence de `light.mode`) ; `None` : à déduire du fond.
    pub dark: Option<bool>,
    /// Couleurs de `colors.toml` (« #rrggbb »), par nom de clé.
    pub colors: BTreeMap<String, String>,
}

/// Le thème d'Omarchy a changé (ou a disparu).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct OmarchyThemeEvent {
    pub theme: Option<OmarchyTheme>,
}

/// Dossier du thème courant et nom du thème.
fn current(home: &Path) -> Option<(PathBuf, String)> {
    let state = home.join(".local/state/omarchy/current");
    if state.join("theme/colors.toml").is_file() {
        let name = std::fs::read_to_string(state.join("theme.name"))
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        return Some((state.join("theme"), name));
    }
    let link = home.join(".config/omarchy/current/theme");
    if link.join("colors.toml").is_file() {
        let target = std::fs::read_link(&link).unwrap_or_else(|_| link.clone());
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Some((link, name));
    }
    None
}

/// Paires `clé = "valeur"` d'un `colors.toml` (format plat, sans tables).
pub fn parse_colors(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#') && !l.starts_with('['))
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            let v = v.trim();
            // Valeur entre guillemets (le commentaire éventuel suit le guillemet fermant).
            let v = match v.chars().next() {
                Some(q @ ('"' | '\'')) => v[1..].split(q).next()?,
                _ => v.split_whitespace().next()?,
            };
            Some((k.trim().to_owned(), v.to_owned()))
        })
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .collect()
}

pub fn read_in(home: &Path) -> Option<OmarchyTheme> {
    let (dir, name) = current(home)?;
    let mut colors = parse_colors(&std::fs::read_to_string(dir.join("colors.toml")).ok()?);
    let dark = match colors.remove("mode").as_deref() {
        Some("light") => Some(false),
        Some("dark") => Some(true),
        _ if dir.join("light.mode").exists() => Some(false),
        _ => None,
    };
    colors.retain(|_, v| v.starts_with('#') && (v.len() == 7 || v.len() == 4));
    (!colors.is_empty()).then_some(OmarchyTheme { name, dark, colors })
}

pub fn read() -> Option<OmarchyTheme> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    read_in(&dirs::home_dir()?)
}

/// Signature du thème courant : nom et date de `colors.toml`.
fn signature(home: &Path) -> Option<(String, Option<SystemTime>)> {
    let (dir, name) = current(home)?;
    let mtime = std::fs::metadata(dir.join("colors.toml")).and_then(|m| m.modified()).ok();
    Some((name, mtime))
}

/// Surveille le thème d'Omarchy et signale chaque changement à l'interface.
pub fn watch(app: tauri::AppHandle) {
    if !cfg!(target_os = "linux") {
        return;
    }
    let Some(home) = dirs::home_dir() else { return };
    std::thread::spawn(move || {
        let mut last = signature(&home);
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let now = signature(&home);
            if now != last {
                last = now;
                let _ = OmarchyThemeEvent { theme: read_in(&home) }.emit(&app);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lunas-pdf-omarchy-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn format_recent() {
        let h = home("recent");
        let dir = h.join(".local/state/omarchy/current/theme");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("colors.toml"),
            "mode = \"light\"\n\naccent = \"#1e66f5\"\nbackground = \"#eff1f5\" # fond\n",
        )
        .unwrap();
        std::fs::write(h.join(".local/state/omarchy/current/theme.name"), "catppuccin-latte\n").unwrap();
        let t = read_in(&h).unwrap();
        assert_eq!(t.name, "catppuccin-latte");
        assert_eq!(t.dark, Some(false));
        assert_eq!(t.colors["accent"], "#1e66f5");
        assert_eq!(t.colors["background"], "#eff1f5");
        assert!(!t.colors.contains_key("mode"));
        let _ = std::fs::remove_dir_all(&h);
    }

    #[test]
    fn format_anterieur() {
        let h = home("ancien");
        let themes = h.join(".config/omarchy/themes/rose-pine");
        std::fs::create_dir_all(&themes).unwrap();
        std::fs::write(
            themes.join("colors.toml"),
            "accent = \"#56949f\"\nbackground = \"#faf4ed\"\ncolor1 = \"#b4637a\"\n",
        )
        .unwrap();
        std::fs::write(themes.join("light.mode"), "").unwrap();
        std::fs::create_dir_all(h.join(".config/omarchy/current")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&themes, h.join(".config/omarchy/current/theme")).unwrap();
        let t = read_in(&h).unwrap();
        assert_eq!(t.name, "rose-pine");
        assert_eq!(t.dark, Some(false));
        assert_eq!(t.colors["color1"], "#b4637a");
        let _ = std::fs::remove_dir_all(&h);
    }

    #[test]
    fn sans_omarchy() {
        assert!(read_in(&home("absent")).is_none());
    }
}
