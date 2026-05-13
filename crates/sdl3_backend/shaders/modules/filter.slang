#language slang 2026

module sprite;

// Remaps a UV so a single hardware bilinear sample produces crisp pixel art
// at arbitrary (non-integer) scale, with smooth transitions only across texel
// seams. Direct port of t3ssel8r's "Pixel Art AA" technique (same as SDL3's
// pixelart scaler).
//
// References:
//     https://www.youtube.com/watch?v=d6tp43wZqps
//     https://www.patreon.com/posts/83276362
public func compute_pixelart_uv(
    uv: float2,
    texture_size: float2,
    inverse_texture_size: float2
) -> float2 {
    // Manual fwidth: Slang's stdlib fwidth is declared with capabilities that
    // exclude Metal's fragment stage, so we expand it to its definition.
    let box_size = clamp((abs(ddx(uv)) + abs(ddy(uv))) * texture_size, float2(1e-5), float2(1.0));
    let tx = uv * texture_size - 0.5 * box_size;
    let tx_offset = smoothstep(float2(1.0) - box_size, float2(1.0), fract(tx));
    return (floor(tx) + 0.5 + tx_offset) * inverse_texture_size;
}
