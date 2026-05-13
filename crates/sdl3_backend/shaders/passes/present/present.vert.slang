#language slang 2026

import screen_space;

// SV_VertexID on the SPIR-V target is lowered to (VertexIndex - BaseVertex), which
// requires OpCapability DrawParameters (Vulkan 1.1 or VK_KHR_shader_draw_parameters).
// We always draw the fullscreen triangle with firstVertex = 0, so reading the raw
// VertexIndex builtin via SV_VulkanVertexID is equivalent and avoids the capability.
#ifdef TARGET_SPIRV
[[shader("vertex")]]
func vs_main(uint vertex_index : SV_VulkanVertexID) -> FullscreenVertex {
    return FullscreenVertex::new(vertex_index);
}
#else
[[shader("vertex")]]
func vs_main(uint vertex_index : SV_VertexID) -> FullscreenVertex {
    return FullscreenVertex::new(vertex_index);
}
#endif
