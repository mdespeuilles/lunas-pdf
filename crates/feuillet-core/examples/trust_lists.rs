//! Régénère l'instantané des listes de confiance embarqué dans le moteur :
//! `cargo run -p feuillet-core --features trust-fetch --example trust_lists`

fn main() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let bundle = feuillet_core::trust_lists::fetch(now).unwrap_or_else(|e| panic!("listes de confiance : {e}"));
    let eu = bundle.anchors.iter().filter(|a| a.source.starts_with("eu:")).count();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/trust-anchors.json.gz");
    std::fs::write(&path, bundle.to_gz()).unwrap();
    println!(
        "{} autorités (UE : {eu}, Microsoft : {}) → {}",
        bundle.anchors.len(),
        bundle.anchors.len() - eu,
        path.display()
    );
}
