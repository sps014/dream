// Milestone 0 probe kernel: the shape the system.gpu.shaders generator will emit for
// `saxpy` (read-only buffers, a writable buffer, scalars grouped in a parameter block).

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
