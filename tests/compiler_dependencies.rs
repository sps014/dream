#[test]
fn compiler_does_not_embed_native_hosts() {
    let manifest: toml::Value = toml::from_str(include_str!("../Cargo.toml")).unwrap();
    let dependencies = manifest["dependencies"].as_table().unwrap();
    for name in ["dream-host", "wgpu", "winit", "wry", "reqwest"] {
        assert!(
            !dependencies.contains_key(name),
            "compiler depends on {}",
            name
        );
    }
    assert_eq!(
        manifest["lib"]["crate-type"].as_array().unwrap(),
        &[toml::Value::String("rlib".into())]
    );
}
