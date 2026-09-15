use std::mem::size_of;

use anyhow::{
    Result,
    anyhow,
    bail,
    ensure,
};
use bytes::Bytes;

// Constants/Statics
const COMPRESSION_ALGORITHM_MASK: u8 = 0x1e;
const COMPRESSION_ALGORITHM_SHIFT: u8 = 1;
const COMPRESSED_FLAG_MASK: u8 = 1;
const COMPRESSION_FRAME_VERSION: u8 = 1;
const COMPRESSION_FRAME_VERSION_SHIFT: u8 = 5;
const ORIGINAL_LENGTH_OFFSET: usize = size_of::<u8>();
const ORIGINAL_LENGTH_SIZE: usize = size_of::<u32>();
pub(super) const COMPRESSION_FRAME_HEADER_SIZE: usize = ORIGINAL_LENGTH_OFFSET + ORIGINAL_LENGTH_SIZE;

// Enums
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WsIoPacketCompressionAlgorithm {
    Zstd = 1,
}

// Structs
#[derive(Clone, Copy, Debug)]
pub(super) struct WsIoPacketCompressionFrameHeader {
    pub(super) algorithm: WsIoPacketCompressionAlgorithm,
    pub(super) compressed: bool,
    pub(super) original_length: u32,
}

impl WsIoPacketCompressionFrameHeader {
    #[inline]
    pub(super) fn new(
        algorithm: WsIoPacketCompressionAlgorithm,
        compressed: bool,
        original_length: usize,
    ) -> Result<Self> {
        Ok(Self {
            algorithm,
            compressed,
            original_length: u32::try_from(original_length)
                .map_err(|_| anyhow!("packet is too large for the ws.io compression frame"))?,
        })
    }

    #[inline]
    pub(super) fn allocate(self, payload_capacity: usize) -> Result<Vec<u8>> {
        let capacity = COMPRESSION_FRAME_HEADER_SIZE
            .checked_add(payload_capacity)
            .ok_or_else(|| anyhow!("ws.io compression frame size overflow"))?;

        let mut output = Vec::with_capacity(capacity);
        self.append_to(&mut output);
        Ok(output)
    }

    #[inline]
    fn append_to(self, output: &mut Vec<u8>) {
        output.push(
            (COMPRESSION_FRAME_VERSION << COMPRESSION_FRAME_VERSION_SHIFT)
                | ((self.algorithm as u8) << COMPRESSION_ALGORITHM_SHIFT)
                | u8::from(self.compressed),
        );

        output.extend_from_slice(&self.original_length.to_be_bytes());
    }

    #[inline]
    pub(super) fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() >= COMPRESSION_FRAME_HEADER_SIZE,
            "ws.io compression frame is too short"
        );

        let descriptor = bytes[0];
        ensure!(
            descriptor >> COMPRESSION_FRAME_VERSION_SHIFT == COMPRESSION_FRAME_VERSION,
            "unsupported ws.io compression frame version"
        );

        let algorithm = match (descriptor & COMPRESSION_ALGORITHM_MASK) >> COMPRESSION_ALGORITHM_SHIFT {
            1 => WsIoPacketCompressionAlgorithm::Zstd,
            algorithm => bail!("unsupported ws.io compression algorithm: {algorithm}"),
        };

        let original_length = u32::from_be_bytes(
            bytes[ORIGINAL_LENGTH_OFFSET..ORIGINAL_LENGTH_OFFSET + ORIGINAL_LENGTH_SIZE]
                .try_into()
                .map_err(|_| anyhow!("invalid ws.io compression frame length"))?,
        );

        Ok(Self {
            algorithm,
            compressed: descriptor & COMPRESSED_FLAG_MASK != 0,
            original_length,
        })
    }

    #[inline]
    pub(super) fn encode_payload(self, payload: &[u8]) -> Result<Bytes> {
        let mut output = self.allocate(payload.len())?;
        output.extend_from_slice(payload);
        Ok(output.into())
    }

    #[inline]
    pub(super) fn set_compressed_flag(output: &mut [u8], compressed: bool) {
        output[0] = (output[0] & !COMPRESSED_FLAG_MASK) | u8::from(compressed);
    }

    #[inline]
    pub(super) fn with_compressed(mut self, compressed: bool) -> Self {
        self.compressed = compressed;
        self
    }
}
