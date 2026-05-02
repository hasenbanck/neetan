/// The instructions to render a frame.
pub struct RenderInstructions<'a> {
    /// 640*480*4 bytes of packed `R, G, B, A` sRGB pixels (little-endian per pixel)
    /// uploaded to the native-resolution sampled image.
    pub framebuffer: &'a [u8],
    /// Active vertical display height (400, or up to 480 in PEGC 480-line mode).
    pub native_height: u32,
    /// Whether the CRT upscale effect is enabled.
    pub crt: bool,
}
