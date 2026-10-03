//! Préférences et documents récents, persistés en JSON dans le dossier de config de l'app.
//! Écriture atomique : fichier temporaire, fsync, rename.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum LanguagePref {
    #[default]
    System,
    Fr,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum RecentsView {
    #[default]
    Grid,
    List,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: ThemePref,
    /// Couleur d'accent CSS (#rrggbb).
    pub accent: String,
    pub language: LanguagePref,
    /// Boutons Réduire/Agrandir/Fermer dans la barre de titre (à masquer sous un WM en tiling).
    pub window_controls: bool,
    pub recents_view: RecentsView,
    pub sidebar_open: bool,
    /// Nom d'auteur des annotations (vide : nom de l'utilisateur du système).
    pub author_name: String,
    /// Fond teinté sur les champs de formulaire (bandeau de la planche 04).
    pub highlight_fields: bool,
    /// Documents signés pour lesquels l'avertissement avant modification est désactivé.
    pub signed_ok: Vec<String>,
    /// Agent du panneau IA (aucun : panneau masqué).
    pub ai_agent: crate::ai::agent::Agent,
    /// Chemin de l'agent saisi à la main ; vide : détection automatique.
    pub ai_path: String,
    /// Modèle (ex. « sonnet ») ; vide : celui de l'agent.
    pub ai_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemePref::System,
            accent: "#3466f6".into(),
            language: LanguagePref::System,
            window_controls: true,
            recents_view: RecentsView::Grid,
            sidebar_open: true,
            author_name: String::new(),
            highlight_fields: true,
            signed_ok: vec![],
            ai_agent: crate::ai::agent::Agent::None,
            ai_path: String::new(),
            ai_model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentDoc {
    pub path: String,
    pub name: String,
    /// Horodatage d'ouverture (ms depuis l'époque Unix).
    pub opened_at: f64,
    pub page_count: u32,
    pub locked: bool,
    pub signed: bool,
    pub form: bool,
    /// Identifiant de la miniature (servie par `lunas-pdf://localhost/thumb/<id>.png`).
    pub thumb: Option<String>,
}

const MAX_RECENTS: usize = 30;

/// Autorité de certification approuvée par l'utilisateur pour vérifier les signatures.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrustedRoot {
    /// Empreinte SHA-256 (« 4F:2A:… »), aussi nom du fichier.
    pub id: String,
    pub name: String,
    /// Date d'ajout (millisecondes Unix).
    pub added: f64,
}

/// Signature manuscrite enregistrée (planche 07) : image PNG à fond transparent.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedSignature {
    pub id: String,
    /// Date d'ajout (millisecondes Unix).
    pub created: f64,
    pub width: u32,
    pub height: u32,
    /// Image en `data:image/png;base64,…` (renseignée à la lecture, jamais stockée).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data: String,
}

pub struct Store {
    config_dir: PathBuf,
    pub thumbs_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub recents: Mutex<Vec<RecentDoc>>,
}

impl Store {
    pub fn load(config_dir: PathBuf, cache_dir: PathBuf) -> Store {
        let thumbs_dir = cache_dir.join("thumbs");
        let _ = fs::create_dir_all(&thumbs_dir);
        let settings = read_json(&config_dir.join("settings.json")).unwrap_or_default();
        let recents = read_json(&config_dir.join("recents.json")).unwrap_or_default();
        Store {
            config_dir,
            thumbs_dir,
            settings: Mutex::new(settings),
            recents: Mutex::new(recents),
        }
    }

    /// Informations mémorisées pour remplir les documents avec l'IA (une par ligne, ex.
    /// « Parent – E-mail : jean@exemple.fr »). Fichier séparé des préférences.
    pub fn memory(&self) -> Vec<String> {
        read_json(&self.config_dir.join("memory.json")).unwrap_or_default()
    }

    pub fn set_memory(&self, facts: Vec<String>) -> std::io::Result<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        for f in facts.into_iter().map(|f| f.trim().to_owned()).filter(|f| !f.is_empty()) {
            if !out.contains(&f) {
                out.push(f);
            }
        }
        write_json_atomic(&self.config_dir.join("memory.json"), &out)?;
        Ok(out)
    }

    fn trusted_dir(&self) -> PathBuf {
        self.config_dir.join("trusted")
    }

    pub fn trusted_roots(&self) -> Vec<TrustedRoot> {
        read_json(&self.trusted_dir().join("index.json")).unwrap_or_default()
    }

    /// Certificats DER des autorités approuvées.
    pub fn trusted_der(&self) -> Vec<Vec<u8>> {
        let dir = self.trusted_dir();
        self.trusted_roots()
            .iter()
            .filter_map(|r| fs::read(dir.join(format!("{}.der", file_id(&r.id)))).ok())
            .collect()
    }

    pub fn add_trusted(&self, der: &[u8], name: String, added: f64) -> std::io::Result<TrustedRoot> {
        let id = Sha256::digest(der)
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":");
        let dir = self.trusted_dir();
        write_atomic(&dir.join(format!("{}.der", file_id(&id))), der)?;
        let mut index = self.trusted_roots();
        index.retain(|r| r.id != id);
        let root = TrustedRoot { id, name, added };
        index.push(root.clone());
        write_json_atomic(&dir.join("index.json"), &index)?;
        Ok(root)
    }

    pub fn remove_trusted(&self, id: &str) -> std::io::Result<()> {
        let dir = self.trusted_dir();
        let mut index = self.trusted_roots();
        index.retain(|r| r.id != id);
        let _ = fs::remove_file(dir.join(format!("{}.der", file_id(id))));
        write_json_atomic(&dir.join("index.json"), &index)
    }

    fn signatures_dir(&self) -> PathBuf {
        self.config_dir.join("signatures")
    }

    fn signature_index(&self) -> Vec<SavedSignature> {
        read_json(&self.signatures_dir().join("index.json")).unwrap_or_default()
    }

    /// Signatures enregistrées, les plus récentes d'abord, avec leur image.
    pub fn signatures(&self) -> Vec<SavedSignature> {
        use base64::Engine as _;
        let dir = self.signatures_dir();
        self.signature_index()
            .into_iter()
            .filter_map(|mut s| {
                let png = fs::read(dir.join(format!("{}.png", s.id))).ok()?;
                s.data = format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(png)
                );
                Some(s)
            })
            .collect()
    }

    pub fn add_signature(&self, png: &[u8], width: u32, height: u32, created: f64) -> std::io::Result<SavedSignature> {
        let dir = self.signatures_dir();
        let id: String = Sha256::digest(png).iter().take(8).map(|b| format!("{b:02x}")).collect();
        write_atomic(&dir.join(format!("{id}.png")), png)?;
        let mut index = self.signature_index();
        index.retain(|s| s.id != id);
        let sig = SavedSignature {
            id,
            created,
            width,
            height,
            data: String::new(),
        };
        index.insert(0, sig.clone());
        write_json_atomic(&dir.join("index.json"), &index)?;
        Ok(sig)
    }

    pub fn remove_signature(&self, id: &str) -> std::io::Result<()> {
        let dir = self.signatures_dir();
        let mut index = self.signature_index();
        index.retain(|s| s.id != id);
        let _ = fs::remove_file(dir.join(format!("{id}.png")));
        write_json_atomic(&dir.join("index.json"), &index)
    }

    pub fn save_settings(&self, s: Settings) -> std::io::Result<()> {
        write_json_atomic(&self.config_dir.join("settings.json"), &s)?;
        *self.settings.lock().unwrap() = s;
        Ok(())
    }

    /// Place (ou remonte) le document en tête de liste.
    pub fn touch_recent(&self, doc: RecentDoc) -> std::io::Result<()> {
        let mut r = self.recents.lock().unwrap();
        r.retain(|d| d.path != doc.path);
        r.insert(0, doc);
        let keep = MAX_RECENTS.min(r.len());
        for old in r.drain(keep..) {
            if let Some(t) = old.thumb {
                let _ = fs::remove_file(self.thumbs_dir.join(format!("{t}.png")));
            }
        }
        write_json_atomic(&self.config_dir.join("recents.json"), &*r)
    }

    /// Remonte un document déjà présent (après un enregistrement) ; sans effet sinon.
    pub fn bump_recent(&self, path: &str, opened_at: f64) -> std::io::Result<()> {
        // Copie hors du verrou : `touch_recent` le reprend.
        let existing = self.recents.lock().unwrap().iter().find(|r| r.path == path).cloned();
        match existing {
            Some(r) => self.touch_recent(RecentDoc { opened_at, ..r }),
            None => Ok(()),
        }
    }

    pub fn remove_recent(&self, path: &str) -> std::io::Result<()> {
        let mut r = self.recents.lock().unwrap();
        r.retain(|d| d.path != path);
        write_json_atomic(&self.config_dir.join("recents.json"), &*r)
    }

    pub fn clear_recents(&self) -> std::io::Result<()> {
        let mut r = self.recents.lock().unwrap();
        r.clear();
        if let Ok(entries) = fs::read_dir(&self.thumbs_dir) {
            for e in entries.flatten() {
                let _ = fs::remove_file(e.path());
            }
        }
        write_json_atomic(&self.config_dir.join("recents.json"), &*r)
    }
}

/// Nom de fichier d'une empreinte « 4F:2A:… ».
fn file_id(id: &str) -> String {
    id.replace(':', "").to_lowercase()
}

/// Identifiant stable d'un chemin (miniatures, trousseau).
pub fn path_id(path: &str) -> String {
    let h = Sha256::digest(path.as_bytes());
    h.iter().take(12).map(|b| format!("{b:02x}")).collect()
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp", path.file_name().unwrap_or_default().to_string_lossy()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    // fsync du dossier pour rendre le rename durable (POSIX).
    #[cfg(unix)]
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

fn write_json_atomic<T: Serialize>(path: &Path, v: &T) -> std::io::Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(v)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recents_are_deduplicated_capped_and_persisted() {
        let dir = std::env::temp_dir().join(format!("lunas-pdf-store-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = Store::load(dir.join("cfg"), dir.join("cache"));
        let doc = |p: &str| RecentDoc {
            path: p.into(),
            name: p.into(),
            opened_at: 0.0,
            page_count: 1,
            locked: false,
            signed: false,
            form: false,
            thumb: None,
        };
        for i in 0..40 {
            s.touch_recent(doc(&format!("/d/{i}.pdf"))).unwrap();
        }
        s.touch_recent(doc("/d/5.pdf")).unwrap();
        // Ne doit pas s'interbloquer (verrou repris par touch_recent).
        s.bump_recent("/d/35.pdf", 42.0).unwrap();
        s.bump_recent("/inconnu.pdf", 1.0).unwrap();
        assert_eq!(s.recents.lock().unwrap()[0].path, "/d/35.pdf");
        s.touch_recent(doc("/d/5.pdf")).unwrap();
        let reloaded = Store::load(dir.join("cfg"), dir.join("cache"));
        let r = reloaded.recents.lock().unwrap();
        assert_eq!(r.len(), MAX_RECENTS);
        assert_eq!(r[0].path, "/d/5.pdf");
        assert_eq!(r.iter().filter(|d| d.path == "/d/5.pdf").count(), 1);
        drop(r);
        reloaded.clear_recents().unwrap();
        assert!(
            Store::load(dir.join("cfg"), dir.join("cache"))
                .recents
                .lock()
                .unwrap()
                .is_empty()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_round_trip_with_defaults_for_missing_fields() {
        let dir = std::env::temp_dir().join(format!("lunas-pdf-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("cfg")).unwrap();
        fs::write(dir.join("cfg/settings.json"), r#"{"theme":"dark"}"#).unwrap();
        let s = Store::load(dir.join("cfg"), dir.join("cache"));
        let cur = s.settings.lock().unwrap().clone();
        assert_eq!(cur.theme, ThemePref::Dark);
        assert_eq!(cur.accent, "#3466f6");
        assert!(cur.window_controls);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_is_deduplicated_and_persisted() {
        let dir = std::env::temp_dir().join(format!("lunas-pdf-memory-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = Store::load(dir.join("cfg"), dir.join("cache"));
        assert!(s.memory().is_empty());
        let saved = s
            .set_memory(vec![
                " Nom : Dupont ".into(),
                "".into(),
                "Nom : Dupont".into(),
                "E-mail : j@d.fr".into(),
            ])
            .unwrap();
        assert_eq!(saved, ["Nom : Dupont", "E-mail : j@d.fr"]);
        assert_eq!(Store::load(dir.join("cfg"), dir.join("cache")).memory(), saved);
        let _ = fs::remove_dir_all(&dir);
    }
}
