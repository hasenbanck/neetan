#language slang 2026

module sprite;

public static const var PI_HALF: float = 0.5 * 3.14159265359;

public struct BandlimitedPixelInfo {
    public var uv0: float2;
    public var length: float;
}

[ForceInline]
public func compute_pixel_weights(uv: float2, texture_size: float2, inverse_texture_size: float2) -> BandlimitedPixelInfo {
    // This variable allows tuning sharpness:
    // - Smaller values → sharper (more aliasing)
    // - Larger values → blurrier (less aliasing)
    let extent_mod = 1.0;

    // Get derivatives in texel space.
    // Need a non-zero derivative.
    let extent = max(fwidth(uv) * texture_size * extent_mod, float2(1.0 / 256.0));

    // Get base pixel and phase, range [0, 1).
    let pixel = uv * texture_size - 0.5;
    let base_pixel = floor(pixel);
    let phase = pixel - base_pixel;

    if (any(extent > float2(1.0))) {
        // We need to just do regular minimization filtering.
        var info: BandlimitedPixelInfo;
        info.uv0 = float2(0.0);
        info.length = 0.0;
        return info;
    } else {
        // We can resolve the filter by just sampling a single 2x2 block.
        // Lerp between normal sampling at LOD 0, and bandlimited pixel filter at LOD -1.
        let shift = 0.5 + 0.5 * sin(PI_HALF * clamp((phase - 0.5) / min(extent, float2(0.5)), float2(-1.0), float2(1.0)));
        let max_extent = max(extent.x, extent.y);
        let length = clamp(2.0 - 2.0 * max_extent, 0.0, 1.0);

        var info: BandlimitedPixelInfo;
        info.uv0 = (base_pixel + 0.5 + shift) * inverse_texture_size;
        info.length = length;
        return info;
    }
}

[ForceInline]
public func sample_bandlimited_pixel(texture: Texture2D, sampler_state: SamplerState, uv: float2, info: BandlimitedPixelInfo, lod_bias: float) -> float4 {
    var color = texture.SampleLevel(sampler_state, uv, lod_bias);

    if (info.length > 0.0) {
        let bandlimited = texture.SampleLevel(sampler_state, info.uv0, 0.0);
        color = lerp(color, bandlimited, info.length);
    }

    return color;
}
