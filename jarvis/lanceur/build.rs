// Met l'icône et le nom « Jarvis » dans l'exe (visible dans l'explorateur et le gestionnaire des tâches).
fn main() {
    println!("cargo:rerun-if-changed=jarvis.rc");
    println!("cargo:rerun-if-changed=jarvis.ico");
    embed_resource::compile("jarvis.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
