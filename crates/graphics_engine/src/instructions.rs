use common::{DisplaySnapshotUpload, PegcSnapshotUpload};

/// The instructions to render a frame.
pub struct RenderInstructions<'a> {
    // TODO: We could make the GRCG & ECG portion also optional of the display_snapshot!
    //       Then we would have a grcg and text snapshot!
    /// The display snapshot used by the composer.
    pub display_snapshot: &'a DisplaySnapshotUpload,
    /// Optional PEGC snapshot for 256-color mode rendering.
    pub pegc_snapshot: Option<&'a PegcSnapshotUpload>,
}
