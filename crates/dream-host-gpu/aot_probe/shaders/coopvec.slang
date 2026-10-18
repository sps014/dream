// Cooperative-vector probe: one 4x8 float16 matrix-vector multiply-add plus relu per thread,
// the core of the plan's GpuMlpWeights.apply. Compiled for every target to see which ones
// Slang lowers natively, which get a portable fallback, and which are rejected.

StructuredBuffer<half> features;
ByteAddressBuffer weights;
ByteAddressBuffer bias;
RWStructuredBuffer<half> output;

[shader("compute")]
[numthreads(32, 1, 1)]
void coopvec(uint3 tid: SV_DispatchThreadID)
{
    CoopVec<half, 8> input;
    for (int k = 0; k < 8; k++)
        input[k] = features[tid.x * 8 + k];

    let hidden = coopVecMatMulAdd<half, 4, 8>(
        input,
        CoopVecComponentType::Float16,
        weights,
        0,
        CoopVecComponentType::Float16,
        bias,
        0,
        CoopVecComponentType::Float16,
        CoopVecMatrixLayout::RowMajor,
        false,
        8 * 2
    );
    let activated = max(hidden, CoopVec<half, 4>(0.0h));

    for (int m = 0; m < 4; m++)
        output[tid.x * 4 + m] = activated[m];
}
