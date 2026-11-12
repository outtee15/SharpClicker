// Embeds src/icon.ico as the icon of the built .exe (what Explorer / the taskbar shows).
fn main() {
    println!("cargo:rerun-if-changed=src/icon.ico");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("src/icon.ico");
        res.set("ProductName", "SharpClicker");
        res.set("FileDescription", "SharpClicker");
        res.compile().expect("failed to embed src/icon.ico into the exe");
    }
}