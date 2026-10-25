//! Incorpora icona e manifest (DPI per monitor) nell'exe e compila l'interfaccia Slint (ui/settings.slint).
fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    slint_build::compile_with_config("ui/settings.slint", slint_build::CompilerConfiguration::new().with_style("fluent".into()))
        .expect("interfaccia Slint non valida");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let res = embed_resource::compile("assets/app.rc", embed_resource::NONE);
        // Su Windows (build vera e CI) un errore deve fermare la build; il controllo da Linux puo' non avere windres.
        if cfg!(windows) {
            res.manifest_required().unwrap();
        }
    }
}
