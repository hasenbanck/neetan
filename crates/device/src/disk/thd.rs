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
            original_header_size: THD_HEADER_SIZE as u32,
        })
    }

    pub(super) fn serialize_thd(&self) -> Vec<u8> {
        let mut out = vec![0u8; THD_HEADER_SIZE];
        out[0..2].copy_from_slice(&self.geometry.cylinders.to_le_bytes());
        out.extend_from_slice(&self.data);
        out
    }
}
