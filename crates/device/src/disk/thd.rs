use crate::disk::{
    HddError, HddFormat, HddGeometry, HddImage, THD_HEADER_SIZE, THD_HEADS, THD_SECTOR_SIZE,
    THD_SECTORS_PER_TRACK,
};

impl HddImage {
    /// Parses a THD image from raw bytes.
    pub fn from_thd(data: &[u8]) -> Result<Self, HddError> {
        if data.len() < THD_HEADER_SIZE {
            return Err(HddError::TooSmall {
                format: "THD",
                minimum: THD_HEADER_SIZE,
                actual: data.len(),
            });
        }

        let cylinders = u16::from_le_bytes([data[0], data[1]]);
        if cylinders == 0 {
            return Err(HddError::InvalidGeometry {
                field: "cylinders",
                value: cylinders as u32,
            });
        }

        let geometry = HddGeometry {
            cylinders,
            heads: THD_HEADS,
            sectors_per_track: THD_SECTORS_PER_TRACK,
            sector_size: THD_SECTOR_SIZE,
        };

        let data_start = THD_HEADER_SIZE;
        let expected_data_size = geometry.total_bytes() as usize;
        if data.len() < data_start + expected_data_size {
            return Err(HddError::DataTruncated {
                expected: data_start + expected_data_size,
                actual: data.len(),
            });
        }

        Ok(HddImage {
            geometry,
            format: HddFormat::Thd,
            data: data[data_start..data_start + expected_data_size].to_vec(),
            header_bytes: data[..THD_HEADER_SIZE].to_vec(),
        })
    }
}
