// Icône, manifeste (style moderne des boutons, netteté sur les écrans haute résolution) et
// fichiers à installer, empaquetés dans l'exe (dist\charge.zip, créé par outils\construire.ps1).
fn main() {
    println!("cargo:rerun-if-changed=installeur.rc");
    println!("cargo:rerun-if-changed=installeur.manifest");
    println!("cargo:rerun-if-env-changed=JARVIS_CHARGE");
    let charge = std::env::var("JARVIS_CHARGE").unwrap_or_else(|_| {
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("dist").join("charge.zip");
        d.to_string_lossy().into_owned()
    });
    if !std::path::Path::new(&charge).is_file() {
        panic!("Fichiers à installer introuvables ({charge}) : lance outils\\construire.ps1");
    }
    println!("cargo:rerun-if-changed={charge}");
    println!("cargo:rustc-env=JARVIS_CHARGE={charge}");
    embed_resource::compile("installeur.rc", embed_resource::NONE).manifest_optional().unwrap();
}
