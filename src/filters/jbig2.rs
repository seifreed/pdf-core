use super::{FilterError, FilterResult};

/// Decode an embedded JBIG2 stream into packed, MSB-first bitmap rows.
pub fn decode_jbig2(
    data: &[u8],
    globals: Option<&[u8]>,
    max_output_bytes: usize,
) -> FilterResult<Vec<u8>> {
    validate_jbig2_input(data, globals, max_output_bytes)?;
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

const JBIG2_FILE_HEADER: &[u8; 8] = b"\x97JB2\r\n\x1a\n";
const MAX_JBIG2_SEGMENTS: usize = 65_536;
const MAX_JBIG2_REFERENCES: u32 = 4_096;

fn validate_jbig2_input(
    data: &[u8],
    globals: Option<&[u8]>,
    max_output_bytes: usize,
) -> FilterResult<()> {
    if data.starts_with(JBIG2_FILE_HEADER) {
        let mut pos = JBIG2_FILE_HEADER.len();
        let flags = *data
            .get(pos)
            .ok_or_else(|| FilterError::Jbig2Error("JBIG2 file header is truncated".to_string()))?;
        if flags & 0xFC != 0 {
            return Err(FilterError::Jbig2Error(
                "JBIG2 file header has reserved flags".to_string(),
            ));
        }
        pos += 1;
        if flags & 0x02 == 0 {
            pos = pos
                .checked_add(4)
                .ok_or_else(|| FilterError::Jbig2Error("JBIG2 header overflow".to_string()))?;
        }
        let sequence = data
            .get(pos..)
            .ok_or_else(|| FilterError::Jbig2Error("JBIG2 file header is truncated".to_string()))?;
        if flags & 0x01 != 0 {
            validate_segment_sequence(sequence, max_output_bytes, true)
        } else {
            validate_random_access_sequence(sequence, max_output_bytes)
        }
    } else {
        if let Some(globals) = globals {
            validate_segment_sequence(globals, max_output_bytes, false)?;
        }
        validate_segment_sequence(data, max_output_bytes, true)
    }
}

fn validate_segment_sequence(
    data: &[u8],
    max_output_bytes: usize,
    require_page_info: bool,
) -> FilterResult<()> {
    let mut pos = 0usize;
    let mut segments = 0usize;
    let mut found_page_info = false;

    while pos < data.len() {
        segments += 1;
        if segments > MAX_JBIG2_SEGMENTS {
            return Err(FilterError::Jbig2Error(
                "JBIG2 segment count exceeds limit".to_string(),
            ));
        }

        let (segment_type, data_length) = parse_segment_header_limited(data, &mut pos)?;
        let segment_end = pos
            .checked_add(data_length)
            .ok_or_else(|| FilterError::Jbig2Error("JBIG2 segment length overflow".to_string()))?;
        let segment_data = data.get(pos..segment_end).ok_or_else(|| {
            FilterError::Jbig2Error("JBIG2 segment extends beyond input".to_string())
        })?;
        pos = segment_end;

        if segment_type == 48 {
            validate_bitmap_dimensions(segment_data, max_output_bytes)?;
            found_page_info = true;
        } else if matches!(
            segment_type,
            4 | 6 | 7 | 20 | 22 | 23 | 36 | 38 | 39 | 40 | 42 | 43
        ) {
            validate_region_dimensions(segment_data, max_output_bytes)?;
        }

        if segment_type == 51 {
            break;
        }
    }

    if require_page_info && !found_page_info {
        return Err(FilterError::Jbig2Error(
            "JBIG2 page information segment is missing".to_string(),
        ));
    }
    Ok(())
}

fn validate_random_access_sequence(data: &[u8], max_output_bytes: usize) -> FilterResult<()> {
    let mut header_pos = 0usize;
    let mut headers = Vec::new();

    loop {
        if headers.len() >= MAX_JBIG2_SEGMENTS {
            return Err(FilterError::Jbig2Error(
                "JBIG2 segment count exceeds limit".to_string(),
            ));
        }
        let (segment_type, data_length) = parse_segment_header_limited(data, &mut header_pos)?;
        headers.push((segment_type, data_length));
        if segment_type == 51 {
            break;
        }
    }

    let mut data_pos = header_pos;
    let mut found_page_info = false;
    for (segment_type, data_length) in headers {
        let segment_end = data_pos
            .checked_add(data_length)
            .ok_or_else(|| FilterError::Jbig2Error("JBIG2 segment length overflow".to_string()))?;
        let segment_data = data.get(data_pos..segment_end).ok_or_else(|| {
            FilterError::Jbig2Error("JBIG2 segment extends beyond input".to_string())
        })?;
        data_pos = segment_end;

        if segment_type == 48 {
            validate_bitmap_dimensions(segment_data, max_output_bytes)?;
            found_page_info = true;
        } else if matches!(
            segment_type,
            4 | 6 | 7 | 20 | 22 | 23 | 36 | 38 | 39 | 40 | 42 | 43
        ) {
            validate_region_dimensions(segment_data, max_output_bytes)?;
        }
    }

    if data_pos != data.len() {
        return Err(FilterError::Jbig2Error(
            "JBIG2 random-access data has trailing bytes".to_string(),
        ));
    }
    if !found_page_info {
        return Err(FilterError::Jbig2Error(
            "JBIG2 page information segment is missing".to_string(),
        ));
    }
    Ok(())
}

fn parse_segment_header_limited(data: &[u8], pos: &mut usize) -> FilterResult<(u8, usize)> {
    let segment_number = read_u32(data, pos)?;
    let flags = read_byte(data, pos)?;
    let segment_type = flags & 0x3F;
    let count_and_retention = read_byte(data, pos)?;
    let short_count = (count_and_retention >> 5) & 0x07;
    if short_count == 5 || short_count == 6 {
        return Err(FilterError::Jbig2Error(
            "JBIG2 referred-segment count is invalid".to_string(),
        ));
    }
    let referred_count = if short_count == 7 {
        let low = count_and_retention & 0x1F;
        let b1 = read_byte(data, pos)?;
        let b2 = read_byte(data, pos)?;
        let b3 = read_byte(data, pos)?;
        u32::from_be_bytes([low, b1, b2, b3])
    } else {
        u32::from(short_count)
    };
    if referred_count > MAX_JBIG2_REFERENCES {
        return Err(FilterError::Jbig2Error(
            "JBIG2 referred-segment count exceeds limit".to_string(),
        ));
    }
    if short_count == 7 {
        let retention_bytes = (referred_count as usize + 1).div_ceil(8);
        skip_bytes(data, pos, retention_bytes)?;
    }

    let reference_width = if segment_number <= 256 {
        1
    } else if segment_number <= 65_536 {
        2
    } else {
        4
    };
    let reference_bytes = (referred_count as usize)
        .checked_mul(reference_width)
        .ok_or_else(|| FilterError::Jbig2Error("JBIG2 reference overflow".to_string()))?;
    skip_bytes(data, pos, reference_bytes)?;
    skip_bytes(data, pos, if flags & 0x40 != 0 { 4 } else { 1 })?;

    let data_length = read_u32(data, pos)?;
    if data_length == u32::MAX {
        return Err(FilterError::Jbig2Error(
            "JBIG2 unknown segment length is not bounded".to_string(),
        ));
    }
    let data_length = usize::try_from(data_length).map_err(|_| {
        FilterError::Jbig2Error("JBIG2 segment length exceeds platform size".to_string())
    })?;
    Ok((segment_type, data_length))
}

fn validate_bitmap_dimensions(data: &[u8], max_output_bytes: usize) -> FilterResult<()> {
    let width = u32::from_be_bytes(
        data.get(0..4)
            .ok_or_else(|| {
                FilterError::Jbig2Error("JBIG2 page information is truncated".to_string())
            })?
            .try_into()
            .unwrap(),
    );
    let height = u32::from_be_bytes(
        data.get(4..8)
            .ok_or_else(|| {
                FilterError::Jbig2Error("JBIG2 page information is truncated".to_string())
            })?
            .try_into()
            .unwrap(),
    );
    if height == u32::MAX {
        return Err(FilterError::Jbig2Error(
            "JBIG2 unknown page height is not bounded".to_string(),
        ));
    }
    let bytes = u64::from(width).div_ceil(8) * u64::from(height);
    if bytes > max_output_bytes as u64 {
        return Err(FilterError::DecompressionError(
            "JBIG2 output exceeds limit".to_string(),
        ));
    }
    Ok(())
}

fn validate_region_dimensions(data: &[u8], max_output_bytes: usize) -> FilterResult<()> {
    if data.len() < 8 {
        return Err(FilterError::Jbig2Error(
            "JBIG2 region information is truncated".to_string(),
        ));
    }
    validate_bitmap_dimensions(data, max_output_bytes)
}

fn read_byte(data: &[u8], pos: &mut usize) -> FilterResult<u8> {
    let value = *data
        .get(*pos)
        .ok_or_else(|| FilterError::Jbig2Error("JBIG2 segment header is truncated".to_string()))?;
    *pos += 1;
    Ok(value)
}

fn read_u32(data: &[u8], pos: &mut usize) -> FilterResult<u32> {
    let end = pos
        .checked_add(4)
        .ok_or_else(|| FilterError::Jbig2Error("JBIG2 offset overflow".to_string()))?;
    let value = u32::from_be_bytes(
        data.get(*pos..end)
            .ok_or_else(|| {
                FilterError::Jbig2Error("JBIG2 segment header is truncated".to_string())
            })?
            .try_into()
            .unwrap(),
    );
    *pos = end;
    Ok(value)
}

fn skip_bytes(data: &[u8], pos: &mut usize, count: usize) -> FilterResult<()> {
    let end = pos
        .checked_add(count)
        .ok_or_else(|| FilterError::Jbig2Error("JBIG2 offset overflow".to_string()))?;
    if data.get(*pos..end).is_none() {
        return Err(FilterError::Jbig2Error(
            "JBIG2 segment header is truncated".to_string(),
        ));
    }
    *pos = end;
    Ok(())
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
