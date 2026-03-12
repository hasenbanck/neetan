#language slang 2026

module transform;

public namespace coordinate_space {
    [ForceInline]
    public func screen_to_clip_space(screen_coords: float2) -> float2 {
        let x = (screen_coords.x * 2.0) - 1.0;
        let y = (screen_coords.y * 2.0) - 1.0;
        return float2(x, y);
    }

    [ForceInline]
    public func clip_to_screen_space(ndc: float2) -> float2 {
        let u = (ndc.x + 1.0) / 2.0;
        let v = (ndc.y + 1.0) / 2.0;
        return float2(u, v);
    }
}

public namespace color {
    [ForceInline]
    public func linear_to_srgb(color: float3) -> float3 {
        return float3(
            linear_to_srgb_component(color.r),
            linear_to_srgb_component(color.g),
            linear_to_srgb_component(color.b)
        );
    }

    [ForceInline]
    func linear_to_srgb_component(value: float) -> float {
        let is_linear = value < 0.0031308;
        let linear_result = value * 12.92;
        let power_result = 1.055 * pow(abs(value), 1.0 / 2.4) - 0.055;
        return is_linear ? linear_result : power_result;
    }

    [ForceInline]
    public func srgb_to_linear_component(value: float) -> float {
        let is_linear = value <= 0.04045;
        let linear_result = value / 12.92;
        let power_result = pow((value + 0.055) / 1.055, 2.4);
        return is_linear ? linear_result : power_result;
    }

    [ForceInline]
    public func unpack_srgb(packed: uint) -> float4 {
        // Unpack RGBA (bits 0-7: R, 8-15: G, 16-23: B, 24-31: A).
        let r_u8 = packed & 0xFF;
        let g_u8 = (packed >> 8) & 0xFF;
        let b_u8 = (packed >> 16) & 0xFF;
        let a_u8 = (packed >> 24) & 0xFF;

        let r_srgb = float(r_u8) / 255.0;
        let g_srgb = float(g_u8) / 255.0;
        let b_srgb = float(b_u8) / 255.0;
        let alpha = float(a_u8) / 255.0;

        let r_linear = srgb_to_linear_component(r_srgb);
        let g_linear = srgb_to_linear_component(g_srgb);
        let b_linear = srgb_to_linear_component(b_srgb);

        return float4(r_linear, g_linear, b_linear, alpha);
    }
}
