use super::{FilterError, FilterResult};

/// Decode an embedded JBIG2 stream into packed, MSB-first bitmap rows.
pub fn decode_jbig2(
    data: &[u8],
    globals: Option<&[u8]>,
    max_output_bytes: usize,
) -> FilterResult<Vec<u8>> {
    let image = if data.starts_with(b"\x97JB2\r\n\x1a\n") {
        pdfluent_jbig2::decode(data)
    } else {
        pdfluent_jbig2::decode_embedded(data, globals)
    }
    .map_err(|error| FilterError::Jbig2Error(error.to_string()))?;
    let row_bytes = usize::try_from(image.width)
        .map_err(|_| FilterError::Jbig2Error("JBIG2 width exceeds platform size".to_string()))?
        .div_ceil(8);
    let output_bytes = row_bytes
        .checked_mul(usize::try_from(image.height).map_err(|_| {
            FilterError::Jbig2Error("JBIG2 height exceeds platform size".to_string())
        })?)
        .ok_or_else(|| FilterError::Jbig2Error("JBIG2 output size overflow".to_string()))?;
    if output_bytes > max_output_bytes {
        return Err(FilterError::DecompressionError(
            "JBIG2 output exceeds limit".to_string(),
        ));
    }

    let mut sink = PackedBitmapSink {
        data: Vec::with_capacity(output_bytes),
        current: 0,
        bits_in_current: 0,
    };
    image.decode(&mut sink);
    if sink.data.len() != output_bytes {
        return Err(FilterError::Jbig2Error(
            "JBIG2 decoder produced an incomplete bitmap".to_string(),
        ));
    }
    Ok(sink.data)
}

struct PackedBitmapSink {
    data: Vec<u8>,
    current: u8,
    bits_in_current: u8,
}

impl pdfluent_jbig2::Decoder for PackedBitmapSink {
    fn push_pixel(&mut self, black: bool) {
        if black {
            self.current |= 1 << (7 - self.bits_in_current);
        }
        self.bits_in_current += 1;
        if self.bits_in_current == 8 {
            self.data.push(self.current);
            self.current = 0;
            self.bits_in_current = 0;
        }
    }

    fn push_pixel_chunk(&mut self, black: bool, chunk_count: u32) {
        if self.bits_in_current == 0 {
            self.data.extend(std::iter::repeat_n(
                if black { 0xff } else { 0 },
                chunk_count as usize,
            ));
        } else {
            for _ in 0..chunk_count.saturating_mul(8) {
                self.push_pixel(black);
            }
        }
    }

    fn next_line(&mut self) {
        if self.bits_in_current != 0 {
            self.data.push(self.current);
            self.current = 0;
            self.bits_in_current = 0;
        }
    }
}

/// Stateful JBIG2 decoder configuration for callers that need a memory cap.
pub struct Jbig2Decoder {
    config: Jbig2Config,
}

impl Jbig2Decoder {
    pub fn new() -> Self {
        Self::with_config(Jbig2Config::default())
    }

    pub fn with_config(config: Jbig2Config) -> Self {
        Self { config }
    }

    pub fn decode(&mut self, data: &[u8], globals: Option<&[u8]>) -> FilterResult<Vec<u8>> {
        let max_output_bytes = self.config.max_memory_mb.saturating_mul(1024 * 1024);
        decode_jbig2(data, globals, max_output_bytes)
    }
}

impl Default for Jbig2Decoder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct Jbig2Config {
    pub max_memory_mb: usize,
    pub strict_mode: bool,
    pub max_symbols: u32,
}

impl Default for Jbig2Config {
    fn default() -> Self {
        Self {
            max_memory_mb: 64,
            strict_mode: false,
            max_symbols: 65536,
        }
    }
}
