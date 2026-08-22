fn main() {
    println!("cargo:rerun-if-env-changed=QUANTIX_UPDATE_ENDPOINT");
    println!("cargo:rerun-if-env-changed=QUANTIX_UPDATE_PUBLIC_KEY");
    println!("cargo:rerun-if-env-changed=TAURI_SIGNING_PRIVATE_KEY");
    println!("cargo:rerun-if-env-changed=TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
    // tauri-build embeds this file in the Windows executable, but does not
    // register the icon itself as a Cargo input. Without this explicit edge,
    // `tauri icon` can leave a previously linked development executable using
    // stale title-bar and taskbar artwork.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    tauri_build::build();

    embed_resource::compile_for_tests("windows-test-manifest.rc", embed_resource::NONE)
        .manifest_required()
        .expect("compile the Windows test manifest");
}
