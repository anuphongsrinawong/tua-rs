/// Build script — prints install hint after successful compilation.
fn main() {
    // Only print on release builds (not during cargo check/test)
    if std::env::var("PROFILE").is_ok_and(|p| p == "release") {
        println!("cargo:warning=");
        println!("cargo:warning=🦀  Build complete! target/release/tua-rs");
        println!("cargo:warning=💡  Install globally: cargo install --path .");
        println!("cargo:warning=    Then run: tua-rs tui");
        println!("cargo:warning=    Or setup first: tua-rs setup");
        println!("cargo:warning=");
    }
}
