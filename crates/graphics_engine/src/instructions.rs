use common::DisplaySnapshotUpload;

/// The instructions to render a frame.
pub struct RenderInstructions<'a> {
    /// The display snapshot used by the composer.
    pub display_snapshot: &'a DisplaySnapshotUpload,
}
