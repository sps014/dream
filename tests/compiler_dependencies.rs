#[test]
fn compiler_does_not_embed_native_hosts() {
    let manifest: toml::Value = toml::from_str(include_str!("../Cargo.toml")).unwrap();
    let dependencies = manifest["dependencies"].as_table().unwrap();
    assert!(
        !dependencies
            .keys()
            .any(|name| name.starts_with("dream-host"))
    );
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

#[test]
fn host_capability_dependency_boundaries_are_explicit() {
    let manifest: toml::Value =
        toml::from_str(include_str!("../crates/dream-host-core/Cargo.toml")).unwrap();
    for name in [
        "reqwest",
        "hyper",
        "hyper-util",
        "wgpu",
        "naga",
        "winit",
        "wry",
    ] {
        assert!(manifest["dependencies"].get(name).is_none());
    }
    let manifest: toml::Value =
        toml::from_str(include_str!("../crates/dream-host/Cargo.toml")).unwrap();
    for capability in ["core", "unicode", "crypto", "process", "timezone"] {
        assert!(manifest["features"][capability].is_array());
        assert_eq!(
            manifest["dependencies"][format!("dream-host-{capability}")]["optional"].as_bool(),
            Some(true)
        );
    }
}
