// A textured quad in the generator's emission style: `@location` inputs as LOCATION<n>
// semantics, fragment resources as `<entry>_<parameter>` globals.

struct quad_vertex_Input
{
    [[vk::location(0)]] float2 position : LOCATION0;
    [[vk::location(1)]] float2 uv : LOCATION1;
};

struct Varyings
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 uv : LOCATION0;
};

Texture2D<float4> quad_fragment_albedo;
SamplerState quad_fragment_nearest;

[shader("vertex")]
Varyings quad_vertex(quad_vertex_Input input)
{
    Varyings output;
    output.position = float4(input.position, 0.5, 1.0);
    output.uv = input.uv;
    return output;
}

[shader("fragment")]
float4 quad_fragment(Varyings input) : SV_Target0
{
    return quad_fragment_albedo.Sample(quad_fragment_nearest, input.uv);
}
