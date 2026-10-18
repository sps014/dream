#[path = "../build/cmake_config.rs"]
pub mod cmake_config;

#[test]
fn generators_and_toolchains_have_separate_build_directories() {
    let key = |name: &str, value: &str| {
        cmake_config::configuration_key(vec![(name.to_owned(), value.into())])
    };
    assert_ne!(
        key("CMAKE_GENERATOR", "Ninja"),
        key("CMAKE_GENERATOR", "Visual Studio 18 2026")
    );
    assert_ne!(
        key("CMAKE_TOOLCHAIN_FILE", "a.cmake"),
        key("CMAKE_TOOLCHAIN_FILE", "b.cmake")
    );
    assert_ne!(
        cmake_config::configuration_key(vec![]),
        key("CMAKE_GENERATOR", "Ninja")
    );
}

#[test]
fn configuration_identity_does_not_depend_on_environment_order() {
    let variables = vec![
        ("CMAKE_GENERATOR".to_owned(), "Ninja".into()),
        ("CMAKE_TOOLCHAIN_FILE".to_owned(), "host.cmake".into()),
    ];
    let expected = cmake_config::configuration_key(variables.clone());
    let mut reversed = variables;
    reversed.reverse();
    assert_eq!(expected, cmake_config::configuration_key(reversed));
}
