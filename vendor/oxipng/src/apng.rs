use std::io::Write;

use crate::{
    error::PngError,
    headers::{read_be_u16, read_be_u32},
    PngResult,
};

#[derive(Debug, Clone)]
/// Animated PNG frame
pub struct Frame {
    /// Width of the frame
    pub width: u32,
    /// Height of the frame
    pub height: u32,
    /// X offset of the frame
    pub x_offset: u32,
    /// Y offset of the frame
    pub y_offset: u32,
    /// Frame delay numerator
    pub delay_num: u16,
    /// Frame delay denominator
    pub delay_den: u16,
    /// Frame disposal operation
    pub dispose_op: u8,
    /// Frame blend operation
    pub blend_op: u8,
    /// Frame data, from fdAT chunks
    pub data: Vec<u8>,
}

impl Frame {
    /// Construct a new Frame from the data in a fcTL chunk
    pub fn from_fctl_data(byte_data: &[u8]) -> PngResult<Frame> {
        if byte_data.len() < 26 {
            return Err(PngError::TruncatedData);
        }
        Ok(Frame {
            width: read_be_u32(&byte_data[4..8]),
            height: read_be_u32(&byte_data[8..12]),
            x_offset: read_be_u32(&byte_data[12..16]),
            y_offset: read_be_u32(&byte_data[16..20]),
            delay_num: read_be_u16(&byte_data[20..22]),
            delay_den: read_be_u16(&byte_data[22..24]),
            dispose_op: byte_data[24],
            blend_op: byte_data[25],
            data: vec![],
        })
    }

    /// Construct the data for a fcTL chunk using the given sequence number
    #[must_use]
    pub fn fctl_data(&self, sequence_number: u32) -> Vec<u8> {
        let mut byte_data = Vec::with_capacity(26);
        byte_data.write_all(&sequence_number.to_be_bytes()).unwrap();
        byte_data.write_all(&self.width.to_be_bytes()).unwrap();
        byte_data.write_all(&self.height.to_be_bytes()).unwrap();
        byte_data.write_all(&self.x_offset.to_be_bytes()).unwrap();
        byte_data.write_all(&self.y_offset.to_be_bytes()).unwrap();
        byte_data.write_all(&self.delay_num.to_be_bytes()).unwrap();
        byte_data.write_all(&self.delay_den.to_be_bytes()).unwrap();
        byte_data.push(self.dispose_op);
        byte_data.push(self.blend_op);
        byte_data
    }

    /// Construct the data for a fdAT chunk using the given sequence number
    #[must_use]
    pub fn fdat_data(&self, sequence_number: u32) -> Vec<u8> {
        let mut byte_data = Vec::with_capacity(4 + self.data.len());
        byte_data.write_all(&sequence_number.to_be_bytes()).unwrap();
        byte_data.write_all(&self.data).unwrap();
        byte_data
    }
}
