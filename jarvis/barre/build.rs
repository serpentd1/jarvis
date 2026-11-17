// Met la nouvelle icône de Jarvis dans JarvisBarre.exe : explorateur, barre des tâches et zone de notification
// (l'icône près de l'horloge est chargée depuis cette ressource, n° 1).
fn main() {
    println!("cargo:rerun-if-changed=jarvis.rc");
    println!("cargo:rerun-if-changed=jarvis.ico");
    embed_resource::compile("jarvis.rc", embed_resource::NONE).manifest_optional().unwrap();
}
