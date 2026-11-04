fn main() {
    // llama.cpp is linked dynamically. While developing, its libraries sit next to the binary
    // (cargo copies them to target/debug). Inside the .app they are in Contents/Frameworks:
    // the Tauri bundler adds that search path itself when the libraries are listed in
    // `bundle.macOS.frameworks` (tauri.conf.json).
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,@executable_path");
    tauri_build::build()
}
