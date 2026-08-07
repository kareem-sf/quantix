fn main() {
    tauri_build::build();

    embed_resource::compile_for_tests("windows-test-manifest.rc", embed_resource::NONE)
        .manifest_required()
        .expect("compile the Windows test manifest");
}
