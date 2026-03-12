#language slang 2026

module globals;

public struct GlobalUniforms {
    public float4x4 orthographic;
    public uint2 color_target_size;
    uint2 padding;
};
