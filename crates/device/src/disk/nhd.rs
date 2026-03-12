use crate::disk::{
    HddError, HddFormat, HddGeometry, HddImage, NHD_HEADER_SIZE, NHD_SIGNATURE, validate_geometry,
};

impl HddImage {
    /// Parses an NHD image from raw bytes.
    pub fn from_nhd(data: &[u8]) -> Result<Self, HddError> {
        if data.len() < NHD_HEADER_SIZE {
            return Err(HddError::TooSmall {
                format: "NHD",
                minimum: NHD_HEADER_SIZE,
                actual: data.len(),
            });
        }
        if &data[..15] != NHD_SIGNATURE {
            return Err(HddError::InvalidSignature {
                format: "NHD",
                expected: "T98HDDIMAGE.R0",
            });
        }

        let header_size = u32::from_le_bytes([data[0x110], data[0x111], data[0x112], data[0x113]]);
        let cylinders = u32::from_le_bytes([data[0x114], data[0x115], data[0x116], data[0x117]]);
        let heads = u16::from_le_bytes([data[0x118], data[0x119]]);
        let sectors_per_track = u16::from_le_bytes([data[0x11A], data[0x11B]]);
        let sector_size = u16::from_le_bytes([data[0x11C], data[0x11D]]);

        validate_geometry(
            cylinders,
            heads as u32,
            sectors_per_track as u32,
            sector_size,
        )?;

        let geometry = HddGeometry {
            cylinders: cylinders as u16,
            heads: heads as u8,
            sectors_per_track: sectors_per_track as u8,
            sector_size,
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
            format: HddFormat::Nhd,
            data: data[data_start..data_start + expected_data_size].to_vec(),
            original_header_size: header_size,
        })
    }

    pub(super) fn serialize_nhd(&self) -> Vec<u8> {
        let header_size = self.original_header_size.max(NHD_HEADER_SIZE as u32);
        let mut out = vec![0u8; header_size as usize];

        out[..15].copy_from_slice(NHD_SIGNATURE);
        out[0x110..0x114].copy_from_slice(&header_size.to_le_bytes());
        out[0x114..0x118].copy_from_slice(&(self.geometry.cylinders as u32).to_le_bytes());
        out[0x118..0x11A].copy_from_slice(&(self.geometry.heads as u16).to_le_bytes());
        out[0x11A..0x11C].copy_from_slice(&(self.geometry.sectors_per_track as u16).to_le_bytes());
        out[0x11C..0x11E].copy_from_slice(&self.geometry.sector_size.to_le_bytes());

        out.extend_from_slice(&self.data);
        out
    }
}
