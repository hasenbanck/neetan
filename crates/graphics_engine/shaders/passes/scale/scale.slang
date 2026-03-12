#language slang 2026

import screen_space;
import filter;

[[vk::binding(1, 0)]] var linear_sampler: SamplerState;
[[vk::binding(1, 1)]] var native_target: Texture2D;

static const float2 NATIVE_SIZE = float2(640.0, 400.0);
static const float2 NATIVE_SIZE_INV = float2(1.0 / 640.0, 1.0 / 400.0);

[[shader("vertex")]]
func vs_main(uint vertex_index : SV_VulkanVertexID) -> FullscreenVertex {
    return FullscreenVertex::new(vertex_index);
}

[[shader("pixel")]]
func fs_main(FullscreenVertex input) -> float4 {
    let pixel_info = compute_pixel_weights(input.uv, NATIVE_SIZE, NATIVE_SIZE_INV);

    return sample_bandlimited_pixel(
        native_target,
        linear_sampler,
        input.uv,
        pixel_info,
        0.0
    );
}
