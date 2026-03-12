use crate::disk::{HDI_HEADER_SIZE, HddError, HddFormat, HddGeometry, HddImage, validate_geometry};

impl HddImage {
    /// Parses an HDI image from raw bytes.
    pub fn from_hdi(data: &[u8]) -> Result<Self, HddError> {
        if data.len() < HDI_HEADER_SIZE {
            return Err(HddError::TooSmall {
                format: "HDI",
                minimum: HDI_HEADER_SIZE,
                actual: data.len(),
            });
        }

        let header_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let sector_size = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        let sectors_per_track = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
        let heads = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
        let cylinders = u32::from_le_bytes([data[28], data[29], data[30], data[31]]);

        validate_geometry(cylinders, heads, sectors_per_track, sector_size as u16)?;

        let geometry = HddGeometry {
            cylinders: cylinders as u16,
            heads: heads as u8,
            sectors_per_track: sectors_per_track as u8,
            sector_size: sector_size as u16,
        };

        let data_start = header_size as usize;
        let expected_data_size = geometry.total_bytes() as usize;
        if data.len() < data_start + expected_data_size {
            return Err(HddError::DataTruncated {
                expected: data_start + expected_data_size,
                actual: data.len(),
            });
        }

        Ok(HddImage {
            geometry,
            format: HddFormat::Hdi,
            data: data[data_start..data_start + expected_data_size].to_vec(),
            original_header_size: header_size,
        })
    }

    pub(super) fn serialize_hdi(&self) -> Vec<u8> {
        let header_size = self.original_header_size.max(HDI_HEADER_SIZE as u32);
        let mut out = vec![0u8; header_size as usize];

        // hddtype at offset 4 (leave as 0)
        out[8..12].copy_from_slice(&header_size.to_le_bytes());
        let hdd_size = self.geometry.total_sectors();
        out[12..16].copy_from_slice(&hdd_size.to_le_bytes());
        out[16..20].copy_from_slice(&(self.geometry.sector_size as u32).to_le_bytes());
        out[20..24].copy_from_slice(&(self.geometry.sectors_per_track as u32).to_le_bytes());
        out[24..28].copy_from_slice(&(self.geometry.heads as u32).to_le_bytes());
        out[28..32].copy_from_slice(&(self.geometry.cylinders as u32).to_le_bytes());

        out.extend_from_slice(&self.data);
        out
    }
}
