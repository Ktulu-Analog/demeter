fn main() {
    println!("cargo:rerun-if-changed=prompts.yml");
    tauri_build::build()
}
