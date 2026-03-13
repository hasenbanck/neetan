#language slang 2026

import screen_space;
import transform;

[[vk::binding(0, 1)]] var color_target: Texture2D;
[[vk::push_constant]] var viewport_offset: int2;

[[shader("vertex")]]
func vs_main(uint vertex_index : SV_VulkanVertexID) -> FullscreenVertex {
    return FullscreenVertex::new(vertex_index);
}

[[shader("pixel")]]
func fs_main(FullscreenVertex input) -> float4 {
    let pixel_coord = int3(int2(input.position.xy) - viewport_offset, 0);

    let texel = color_target.Load(pixel_coord);

    let srgb_color = color::linear_to_srgb(texel.rgb);

    return float4(srgb_color.rgb, texel.a);
}
