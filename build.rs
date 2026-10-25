//! Incorpora l'icona (assets/icon.ico) nell'exe: serve a Esplora risorse, al menu Start e alla finestra Impostazioni.
fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let res = embed_resource::compile("assets/app.rc", embed_resource::NONE);
        // Su Windows (build vera e CI) un errore deve fermare la build; il controllo da Linux puo' non avere windres.
        if cfg!(windows) {
            res.manifest_required().unwrap();
        }
    }
}
