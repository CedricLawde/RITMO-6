//! Gives the Windows standalone program its icon (assets/icon.ico, the person's artwork).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Ritmo-6");
        res.set("CompanyName", "Geranium");
        res.set("FileDescription", "Ritmo-6 rhythm synth");
        if let Err(e) = res.compile() {
            println!("cargo:warning=no program icon: {e}");
        }
    }
}