//! Pure-Rust SquashFS decompression.
//!
//! `backhand`'s own `xz` / `zstd` features link C (`liblzma` / `zstd-sys`).
//! Instead we plug a custom [`CompressionAction`] into backhand that decodes xz
//! with [`lzma_rs`] and zstd with [`ruzstd`] — both pure Rust — and delegates
//! gzip, lzo, and lz4 to backhand's pure-Rust [`DefaultCompressor`]. The crate
//! therefore stays free of C whether or not `xz` / `zstd` are enabled.

use backhand::compression::{CompressionAction, Compressor, DefaultCompressor};
use backhand::kind::Kind;
use backhand::{BackhandError, BufReadSeek, FilesystemCompressor, FilesystemReader, SuperBlock};

/// Open the SquashFS at `offset`, decoding with the pure-Rust compressor set.
pub(crate) fn open_reader<R: BufReadSeek + 'static>(
    reader: R,
    offset: u64,
) -> Result<FilesystemReader<'static>, BackhandError> {
    FilesystemReader::from_reader_with_offset_and_kind(reader, offset, Kind::new_v4(&PURE))
}

static PURE: PureCompressor = PureCompressor;

struct PureCompressor;

impl CompressionAction for PureCompressor {
    type Error = BackhandError;
    type Compressor = Compressor;
    type FilesystemCompressor = FilesystemCompressor;
    type SuperBlock = SuperBlock;

    fn decompress(
        &self,
        bytes: &[u8],
        out: &mut Vec<u8>,
        compressor: Compressor,
    ) -> Result<(), BackhandError> {
        match compressor {
            #[cfg(feature = "zstd")]
            Compressor::Zstd => decode_zstd(bytes, out),
            #[cfg(feature = "xz")]
            Compressor::Xz => decode_xz(bytes, out),
            // gzip / lzo / lz4 / uncompressed are already pure Rust in backhand;
            // xz / zstd land here too when their feature is off (→ unsupported).
            other => DefaultCompressor.decompress(bytes, out, other),
        }
    }

    fn compress(
        &self,
        bytes: &[u8],
        fc: FilesystemCompressor,
        block_size: u32,
    ) -> Result<Vec<u8>, BackhandError> {
        // This crate only reads; the trait still requires a compressor.
        DefaultCompressor.compress(bytes, fc, block_size)
    }

    fn compression_options(
        &self,
        superblock: &mut SuperBlock,
        kind: &Kind,
        fc: FilesystemCompressor,
    ) -> Result<Option<Vec<u8>>, BackhandError> {
        DefaultCompressor.compression_options(superblock, kind, fc)
    }
}

#[cfg(feature = "zstd")]
fn decode_zstd(bytes: &[u8], out: &mut Vec<u8>) -> Result<(), BackhandError> {
    use std::io::Read;
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(bytes)
        .map_err(|error| BackhandError::CompressionInit(format!("zstd: {error}")))?;
    decoder
        .read_to_end(out)
        .map_err(|error| BackhandError::CompressionInit(format!("zstd: {error}")))?;
    Ok(())
}

#[cfg(feature = "xz")]
fn decode_xz(bytes: &[u8], out: &mut Vec<u8>) -> Result<(), BackhandError> {
    let mut input = bytes;
    lzma_rs::xz_decompress(&mut input, out)
        .map_err(|error| BackhandError::CompressionInit(format!("xz: {error}")))?;
    Ok(())
}
