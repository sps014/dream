// The shape the system.gpu.shaders generator emits for `saxpy`: read-only and writable
// buffers as globals, scalars grouped in a parameter block.

struct SaxpyParams
{
    float a;
    int n;
};

StructuredBuffer<float> x;
StructuredBuffer<float> y;
RWStructuredBuffer<float> result;
ParameterBlock<SaxpyParams> params;

[shader("compute")]
[numthreads(64, 1, 1)]
void saxpy(uint3 tid: SV_DispatchThreadID)
{
    int i = int(tid.x);
    if (i < params.n)
    {
        result[i] = params.a * x[i] + y[i];
    }
}

// Entry-point parameters instead of globals, so both binding styles are exercised.
[shader("compute")]
[numthreads(64, 1, 1)]
void scale(uint3 tid: SV_DispatchThreadID, uniform RWStructuredBuffer<float> data, uniform float factor, uniform int count)
{
    int i = int(tid.x);
    if (i < count)
    {
        data[i] = data[i] * factor;
    }
}
