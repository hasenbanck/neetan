#language slang 2026

import screen_space;
import filter;

[[vk::binding(1, 0)]] var linear_sampler: SamplerState;
[[vk::binding(1, 1)]] var native_target: Texture2D;

static const float2 TEXTURE_SIZE = float2(640.0, 480.0);
static const float2 TEXTURE_SIZE_INV = float2(1.0 / 640.0, 1.0 / 480.0);

[[vk::push_constant]] var content_height: float;

[[shader("vertex")]]
func vs_main(uint vertex_index : SV_VulkanVertexID) -> FullscreenVertex {
    return FullscreenVertex::new(vertex_index);
}

[[shader("pixel")]]
func fs_main(FullscreenVertex input) -> float4 {
    let uv = float2(input.uv.x, input.uv.y * (content_height / 480.0));

    let pixel_info = compute_pixel_weights(uv, TEXTURE_SIZE, TEXTURE_SIZE_INV);

    return sample_bandlimited_pixel(
        native_target,
        linear_sampler,
        uv,
        pixel_info,
        0.0
    );
}
