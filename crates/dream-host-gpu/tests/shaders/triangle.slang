// A full-target triangle: vertices from SV_VertexID, color from a fragment `uniform`.

struct VertexOutput
{
    float4 position : SV_Position;
    float4 color : COLOR0;
};

[shader("vertex")]
VertexOutput triangle_vertex(uint vertex_id : SV_VertexID)
{
    float2 corners[3] = { float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) };
    VertexOutput output;
    output.position = float4(corners[vertex_id], 0.0, 1.0);
    output.color = float4(1.0, 0.0, 0.0, 1.0);
    return output;
}

[shader("fragment")]
float4 triangle_fragment(VertexOutput input, uniform float4 tint) : SV_Target
{
    return input.color * tint;
}
