#language slang 2026

module screen_space;

public struct FullscreenVertex {
    public float4 position : SV_Position;
    public float2 uv : TEXCOORD0;

    [ForceInline]
    public static func new(vertex_index: uint) -> FullscreenVertex {
        let uv = float2(float((vertex_index << 1) & 2), float(vertex_index & 2));
        let position = float4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);

        var vertex: FullscreenVertex;
        vertex.position = position;
        vertex.uv = uv;
        return vertex;
    }
};
