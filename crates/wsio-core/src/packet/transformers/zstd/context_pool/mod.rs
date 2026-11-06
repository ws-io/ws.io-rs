use std::{
    io::Cursor,
    thread::available_parallelism,
};

use anyhow::Result;
use bytes::Bytes;
use parking_lot::Mutex;
use zstd::{
    bulk::{
        Compressor,
        Decompressor,
    },
    zstd_safe::compress_bound,
};

use super::super::compression::frame::{
    COMPRESSION_FRAME_HEADER_SIZE,
    WsIoPacketCompressionFrameHeader,
};

// Structs
struct WsIoPacketContextCache<Context> {
    contexts: Mutex<Vec<Context>>,
}

impl<Context> WsIoPacketContextCache<Context> {
    #[inline]
    fn new() -> Self {
        Self {
            contexts: Mutex::new(Vec::new()),
        }
    }

    #[inline]
    fn with_context<Output>(
        &self,
        max_contexts: usize,
        create: impl FnOnce() -> Result<Context>,
        operation: impl FnOnce(&mut Context) -> Result<Output>,
    ) -> Result<Output> {
        let cached_context = self.contexts.lock().pop();
        let mut context = cached_context.map_or_else(create, Ok)?;
        let result = operation(&mut context);

        let mut contexts = self.contexts.lock();
        if contexts.len() < max_contexts {
            contexts.push(context);
        }

        result
    }
}

pub(in super::super) struct WsIoPacketZstdContextPool {
    compression_level: i32,
    compressors: WsIoPacketContextCache<Compressor<'static>>,
    decompressors: WsIoPacketContextCache<Decompressor<'static>>,
    max_contexts: usize,
}

impl WsIoPacketZstdContextPool {
    #[inline]
    pub(in super::super) fn new(compression_level: i32) -> Self {
        Self {
            compression_level,
            compressors: WsIoPacketContextCache::new(),
            decompressors: WsIoPacketContextCache::new(),
            max_contexts: available_parallelism().map_or(1, usize::from),
        }
    }

    #[inline]
    pub(in super::super) fn compress_frame(
        &self,
        bytes: &[u8],
        header: WsIoPacketCompressionFrameHeader,
    ) -> Result<Vec<u8>> {
        self.compressors.with_context(
            self.max_contexts,
            || Ok(Compressor::new(self.compression_level)?),
            |compressor| {
                let payload_capacity = compress_bound(bytes.len()).max(bytes.len());
                let mut output = header.allocate(payload_capacity)?;
                {
                    let mut cursor = Cursor::new(&mut output);
                    cursor.set_position(COMPRESSION_FRAME_HEADER_SIZE as u64);
                    compressor.compress_to_buffer(bytes, &mut cursor)?;
                }

                Ok(output)
            },
        )
    }

    #[inline]
    pub(in super::super) fn decompress(&self, payload: &[u8], original_length: usize) -> Result<Bytes> {
        self.decompressors.with_context(
            self.max_contexts,
            || Ok(Decompressor::new()?),
            |decompressor| Ok(decompressor.decompress(payload, original_length)?.into()),
        )
    }
}

#[cfg(test)]
mod tests;
