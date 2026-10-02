#[test]
fn storage_compare_exchange_translates_to_hlsl() {
    let module = naga::front::wgsl::parse_str(
        r#"
        @group(0) @binding(0) var<storage, read_write> value: atomic<i32>;
        @group(0) @binding(1) var<storage, read_write> previous: array<i32>;

        @compute @workgroup_size(1)
        fn main() {
            previous[0] = atomicCompareExchangeWeak(&value, 7, 11).old_value;
        }
        "#,
    )
    .unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    let options = naga::back::hlsl::Options::default();
    let pipeline = naga::back::hlsl::PipelineOptions::default();
    let mut hlsl = String::new();
    naga::back::hlsl::Writer::new(&mut hlsl, &options, &pipeline)
        .write(&module, &info, None)
        .unwrap();
    assert!(hlsl.contains("InterlockedCompareExchange"), "{hlsl}");
}
