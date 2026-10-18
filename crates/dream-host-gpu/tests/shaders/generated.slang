// The system.gpu.shaders generator's emission style: every shader parameter is a module global
// named `<entry>_<parameter>`, and value parameters share one ConstantBuffer per entry. Two
// entries, so the globals of one stay unbound while the other runs.

struct fill_Params
{
    uint value;
    int count;
};

RWStructuredBuffer<uint> fill_out;
ConstantBuffer<fill_Params> fill_params;

struct offset_Params
{
    uint amount;
};

RWStructuredBuffer<uint> offset_data;
ConstantBuffer<offset_Params> offset_params;

[shader("compute")]
[numthreads(32, 1, 1)]
void fill(uint3 tid: SV_DispatchThreadID)
{
    if (int(tid.x) < fill_params.count)
        fill_out[tid.x] = fill_params.value + tid.x;
}

[shader("compute")]
[numthreads(32, 1, 1)]
void offset(uint3 tid: SV_DispatchThreadID)
{
    offset_data[tid.x] = offset_data[tid.x] + offset_params.amount;
}
