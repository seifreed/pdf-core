use crate::performance::ResourceBudget;
use crate::types::{CCITTFaxDecodeParams, FlateDecodeParams, LZWDecodeParams, StreamFilter};
use flate2::read::ZlibDecoder;
use std::cmp::Ordering;
use std::io::Read;
use thiserror::Error;

pub mod ccitt;
pub mod ccitt_tables;
pub mod crypt;
pub mod jbig2;
pub mod jpx;
pub mod predictor;

#[derive(Error, Debug)]
pub enum FilterError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("Decompression error: {0}")]
    DecompressionError(String),

    #[error("Cryptographic error: {0}")]
    CryptError(String),

    #[error("JBIG2 error: {0}")]
    Jbig2Error(String),

    #[error("Image decode error: {0}")]
    ImageDecodeError(String),
}

pub type FilterResult<T> = Result<T, FilterError>;

pub fn decode_stream(data: &[u8], filters: &[StreamFilter]) -> Result<Vec<u8>, FilterError> {
    decode_stream_with_limits(data, filters, 10 * 1024 * 1024, 100)
}

pub fn decode_stream_with_budget(
    data: &[u8],
    filters: &[StreamFilter],
    budget: &ResourceBudget,
) -> Result<Vec<u8>, FilterError> {
    budget
        .check()
        .map_err(|err| FilterError::DecompressionError(err.to_string()))?;

    if filters.is_empty() {
        budget
            .consume_decoded(data.len() as u64)
            .map_err(|err| FilterError::DecompressionError(err.to_string()))?;
        return Ok(data.to_vec());
    }

    let max_output_bytes = budget
        .max_decoded_bytes_per_stream
        .min(budget.remaining_decoded_bytes());
    let max_output_bytes = usize::try_from(max_output_bytes).unwrap_or(usize::MAX);
    let max_ratio = usize::try_from(budget.max_decode_ratio).unwrap_or(usize::MAX);
    let decoded = decode_stream_with_limits(data, filters, max_output_bytes, max_ratio)?;
    budget
        .consume_decoded(decoded.len() as u64)
        .map_err(|err| FilterError::DecompressionError(err.to_string()))?;
    Ok(decoded)
}

pub fn decode_stream_with_limits(
    data: &[u8],
    filters: &[StreamFilter],
    max_output_bytes: usize,
    max_ratio: usize,
) -> Result<Vec<u8>, FilterError> {
    if data.len() > max_output_bytes && filters.is_empty() {
        return Err(FilterError::DecompressionError(format!(
            "Decoded stream exceeds limit: {} bytes > {} bytes",
            data.len(),
            max_output_bytes
        )));
    }
    let mut result = data.to_vec();
    let input_len = data.len().max(1);

    for filter in filters {
        result = decode_single_filter(&result, filter, max_output_bytes)?;

        if result.len() > max_output_bytes {
            return Err(FilterError::DecompressionError(format!(
                "Decoded stream exceeds limit: {} bytes > {} bytes",
                result.len(),
                max_output_bytes
            )));
        }

        if result.len() > input_len.saturating_mul(max_ratio) {
            return Err(FilterError::DecompressionError(format!(
                "Decoded stream exceeds ratio limit: {} > {}x",
                result.len() / input_len,
                max_ratio
            )));
        }
    }

    Ok(result)
}

fn decode_single_filter(
    data: &[u8],
    filter: &StreamFilter,
    max_output_bytes: usize,
) -> Result<Vec<u8>, FilterError> {
    match filter {
        StreamFilter::ASCIIHexDecode => decode_ascii_hex(data),
        StreamFilter::ASCII85Decode => decode_ascii85(data),
        StreamFilter::FlateDecode(params) => {
            decode_flate_with_struct(data, params, max_output_bytes)
        }
        StreamFilter::LZWDecode(params) => decode_lzw_with_params(data, params),
        StreamFilter::RunLengthDecode => decode_run_length(data),
        StreamFilter::CCITTFaxDecode(params) => decode_ccitt_fax(data, params, max_output_bytes),
        StreamFilter::JBIG2Decode => decode_jbig2(data),
        StreamFilter::DCTDecode => decode_dct(data),
        StreamFilter::JPXDecode => decode_jpx(data),
        StreamFilter::Crypt(_) => Err(FilterError::CryptError(
            "Crypt filter requires decryption context".to_string(),
        )),
    }
}

fn decode_ascii_hex(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    let mut result = Vec::new();
    let mut chars = data.iter().filter(|&&c| !c.is_ascii_whitespace());

    while let Some(&c1) = chars.next() {
        if c1 == b'>' {
            break;
        }

        let c2 = chars.next().copied().unwrap_or(b'0');

        let hex_str = format!("{}{}", c1 as char, c2 as char);
        let byte = u8::from_str_radix(&hex_str, 16)
            .map_err(|_| FilterError::InvalidData(format!("Invalid hex string: {}", hex_str)))?;
        result.push(byte);
    }

    Ok(result)
}

/// Powers of 85 for ASCII85 decoding: [85^4, 85^3, 85^2, 85^1, 85^0]
const ASCII85_POWERS: [u32; 5] = [52200625, 614125, 7225, 85, 1];

fn decode_ascii85(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    let mut result = Vec::new();
    let mut tuple: Vec<u8> = Vec::with_capacity(5);

    for &byte in data {
        if byte.is_ascii_whitespace() {
            continue;
        }

        if byte == b'~' {
            break;
        }

        if byte == b'z' {
            result.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }

        if !(b'!'..=b'u').contains(&byte) {
            return Err(FilterError::InvalidData(format!(
                "Invalid ASCII85 character: {}",
                byte as char
            )));
        }

        tuple.push(byte - b'!');

        if tuple.len() == 5 {
            let value = ascii85_tuple_to_u32(&tuple);
            result.extend_from_slice(&value.to_be_bytes());
            tuple.clear();
        }
    }

    if !tuple.is_empty() {
        let bytes_to_take = tuple.len() - 1;
        tuple.resize(5, 84);
        let value = ascii85_tuple_to_u32(&tuple);
        result.extend_from_slice(&value.to_be_bytes()[..bytes_to_take]);
    }

    Ok(result)
}

fn ascii85_tuple_to_u32(tuple: &[u8]) -> u32 {
    tuple
        .iter()
        .zip(ASCII85_POWERS.iter())
        .fold(0u32, |acc, (&digit, &power)| acc + digit as u32 * power)
}

fn decode_flate_raw(data: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, FilterError> {
    let mut decoder = ZlibDecoder::new(data);
    let mut result = Vec::new();

    match decoder
        .by_ref()
        .take(max_output_bytes.saturating_add(1) as u64)
        .read_to_end(&mut result)
    {
        Ok(_) if result.len() > max_output_bytes => Err(FilterError::DecompressionError(
            "Flate output exceeds limit".to_string(),
        )),
        Ok(_) => Ok(result),
        Err(zlib_err) => {
            let mut deflate_decoder = flate2::read::DeflateDecoder::new(data);
            result.clear();
            deflate_decoder
                .by_ref()
                .take(max_output_bytes.saturating_add(1) as u64)
                .read_to_end(&mut result)
                .map_err(|_| {
                    FilterError::DecompressionError(format!("Flate decode error: {}", zlib_err))
                })?;
            if result.len() > max_output_bytes {
                return Err(FilterError::DecompressionError(
                    "Flate output exceeds limit".to_string(),
                ));
            }
            Ok(result)
        }
    }
}

fn decode_flate_with_struct(
    data: &[u8],
    params: &FlateDecodeParams,
    max_output_bytes: usize,
) -> Result<Vec<u8>, FilterError> {
    let decoded = decode_flate_raw(data, max_output_bytes)?;
    apply_predictor(
        decoded,
        params.predictor,
        params.colors,
        params.bits_per_component,
        params.columns,
    )
}

fn decode_lzw_with_params(data: &[u8], params: &LZWDecodeParams) -> Result<Vec<u8>, FilterError> {
    let early_change = params.early_change.unwrap_or(true);
    let decoded = decode_lzw(data, early_change)?;
    apply_predictor(
        decoded,
        params.predictor,
        params.colors,
        params.bits_per_component,
        params.columns,
    )
}

fn decode_lzw(data: &[u8], early_change: bool) -> Result<Vec<u8>, FilterError> {
    if early_change {
        let mut decoder = lzw::DecoderEarlyChange::new(lzw::MsbReader::new(), 8);
        decode_lzw_stream_ec(data, &mut decoder).or_else(|msb_err| {
            let mut fallback = lzw::DecoderEarlyChange::new(lzw::LsbReader::new(), 8);
            decode_lzw_stream_ec(data, &mut fallback).map_err(|lsb_err| {
                FilterError::DecompressionError(format!(
                    "LZW decode error (MSB: {}, LSB: {})",
                    msb_err, lsb_err
                ))
            })
        })
    } else {
        let mut decoder = lzw::Decoder::new(lzw::MsbReader::new(), 8);
        decode_lzw_stream(data, &mut decoder).or_else(|msb_err| {
            let mut fallback = lzw::Decoder::new(lzw::LsbReader::new(), 8);
            decode_lzw_stream(data, &mut fallback).map_err(|lsb_err| {
                FilterError::DecompressionError(format!(
                    "LZW decode error (MSB: {}, LSB: {})",
                    msb_err, lsb_err
                ))
            })
        })
    }
}

fn decode_lzw_stream<R: lzw::BitReader>(
    data: &[u8],
    decoder: &mut lzw::Decoder<R>,
) -> Result<Vec<u8>, String> {
    let mut offset = 0usize;
    let mut output = Vec::new();

    while offset < data.len() {
        let (consumed, bytes) = decoder
            .decode_bytes(&data[offset..])
            .map_err(|e| format!("{:?}", e))?;
        if consumed == 0 {
            break;
        }
        offset += consumed;
        output.extend_from_slice(bytes);
    }

    Ok(output)
}

fn decode_lzw_stream_ec<R: lzw::BitReader>(
    data: &[u8],
    decoder: &mut lzw::DecoderEarlyChange<R>,
) -> Result<Vec<u8>, String> {
    let mut offset = 0usize;
    let mut output = Vec::new();

    while offset < data.len() {
        let (consumed, bytes) = decoder
            .decode_bytes(&data[offset..])
            .map_err(|e| format!("{:?}", e))?;
        if consumed == 0 {
            break;
        }
        offset += consumed;
        output.extend_from_slice(bytes);
    }

    Ok(output)
}

fn apply_predictor(
    data: Vec<u8>,
    predictor: Option<i32>,
    colors: Option<i32>,
    bits_per_component: Option<i32>,
    columns: Option<i32>,
) -> Result<Vec<u8>, FilterError> {
    let predictor = predictor.unwrap_or(1);
    if predictor <= 1 {
        return Ok(data);
    }

    if !matches!(predictor, 2 | 10..=15) {
        return Err(FilterError::InvalidData(format!(
            "Unsupported predictor value: {}",
            predictor
        )));
    }
    if data.is_empty() {
        return Ok(data);
    }

    let colors = colors.unwrap_or(1);
    let bpc = bits_per_component.unwrap_or(8);
    let columns = columns.unwrap_or(1);
    if !(1..=255).contains(&colors) || !matches!(bpc, 1 | 2 | 4 | 8 | 16) || columns <= 0 {
        return Err(FilterError::InvalidData(
            "Invalid predictor parameters".to_string(),
        ));
    }

    let row_bits = u64::try_from(columns)
        .ok()
        .and_then(|columns| columns.checked_mul(colors as u64))
        .and_then(|bits| bits.checked_mul(bpc as u64))
        .ok_or_else(|| FilterError::InvalidData("Predictor row size overflow".to_string()))?;
    if row_bits > u32::MAX as u64 {
        return Err(FilterError::InvalidData(
            "Predictor row size exceeds supported limit".to_string(),
        ));
    }
    let row_bytes = row_bits.div_ceil(8);
    if row_bytes == 0 || row_bytes as usize > data.len() {
        return Err(FilterError::InvalidData(
            "Predictor row exceeds decoded data".to_string(),
        ));
    }

    let predictor_decoder =
        predictor::PredictorDecoder::new(predictor, colors as u8, bpc as u8, columns as u32);
    predictor_decoder
        .decode(&data)
        .map_err(|e| FilterError::DecompressionError(format!("Predictor decode error: {:?}", e)))
}

fn decode_run_length(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    let mut result = Vec::new();
    let mut i = 0;
    let mut seen_eod = false;

    while i < data.len() {
        if data[i] == 128 {
            seen_eod = true;
            break;
        }

        if data[i] < 128 {
            let count = (data[i] + 1) as usize;
            if i + 1 + count > data.len() {
                return Err(FilterError::InvalidData(
                    "RunLength decode error: insufficient data".to_string(),
                ));
            }
            result.extend_from_slice(&data[i + 1..i + 1 + count]);
            i += 1 + count;
        } else {
            let count = (257_u16 - data[i] as u16) as usize;
            if i + 1 >= data.len() {
                return Err(FilterError::InvalidData(
                    "RunLength decode error: insufficient data".to_string(),
                ));
            }
            let byte = data[i + 1];
            result.resize(result.len() + count, byte);
            i += 2;
        }
    }

    if !seen_eod && !data.is_empty() && data[data.len() - 1] != 128 {
        return Err(FilterError::InvalidData(
            "RunLength decode error: missing EOD marker".to_string(),
        ));
    }

    Ok(result)
}

fn decode_ccitt_fax(
    data: &[u8],
    params: &CCITTFaxDecodeParams,
    max_output_bytes: usize,
) -> Result<Vec<u8>, FilterError> {
    let k = params.k.unwrap_or(0);
    let columns = usize::try_from(params.columns.unwrap_or(1728))
        .map_err(|_| FilterError::InvalidData("CCITT columns must be non-negative".to_string()))?;
    let rows = usize::try_from(params.rows.unwrap_or(0))
        .map_err(|_| FilterError::InvalidData("CCITT rows must be non-negative".to_string()))?;
    if columns == 0 {
        return Err(FilterError::InvalidData(
            "CCITT columns must be positive".to_string(),
        ));
    }
    if rows > 0 {
        let upper_bound = columns
            .div_ceil(8)
            .checked_mul(rows)
            .ok_or_else(|| FilterError::InvalidData("CCITT output size overflow".to_string()))?;
        if upper_bound > max_output_bytes {
            return Err(FilterError::DecompressionError(
                "CCITT output exceeds limit".to_string(),
            ));
        }
    }
    let black_is_1 = params.black_is_1.unwrap_or(false);
    let end_of_line = params.end_of_line.unwrap_or(false);
    let encoded_byte_align = params.encoded_byte_align.unwrap_or(false);
    let end_of_block = params.end_of_block.unwrap_or(true);
    let damaged_rows_before_error = params.damaged_rows_before_error.unwrap_or(0);
    if damaged_rows_before_error < 0 {
        return Err(FilterError::InvalidData(
            "CCITT damaged rows must be non-negative".to_string(),
        ));
    }

    let decoder = ccitt::CcittDecoder::new(columns, rows)
        .with_k(k)
        .with_black_is_1(black_is_1)
        .with_end_of_line(end_of_line)
        .with_encoded_byte_align(encoded_byte_align)
        .with_end_of_block(end_of_block)
        .with_damaged_rows_before_error(damaged_rows_before_error)
        .with_max_output_bytes(max_output_bytes);

    let result = match k.cmp(&0) {
        Ordering::Less => decoder.decode_group4(data),
        Ordering::Equal => decoder.decode_group3_1d(data),
        Ordering::Greater => decoder.decode_group3_2d(data),
    };

    result.map_err(FilterError::DecompressionError)
}

fn decode_jbig2(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    let _ = data;
    Err(FilterError::Jbig2Error(
        "JBIG2 decoding is not supported; inspect the raw stream instead".to_string(),
    ))
}

fn decode_dct(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    use jpeg_decoder::Decoder;

    let mut decoder = Decoder::new(data);

    let _metadata = decoder
        .info()
        .ok_or_else(|| FilterError::ImageDecodeError("JPEG info not available".to_string()))?;

    match decoder.decode() {
        Ok(image_data) => Ok(image_data),
        Err(jpeg_decoder::Error::Format(_)) => Ok(data.to_vec()),
        Err(e) => Err(FilterError::ImageDecodeError(format!(
            "JPEG decode error: {:?}",
            e
        ))),
    }
}

fn decode_jpx(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    jpx::decode_jpx_to_codestream(data)
        .map_err(|e| FilterError::ImageDecodeError(format!("JPX decode error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_predictor, decode_ccitt_fax, decode_stream_with_budget, decode_stream_with_limits,
    };
    use crate::performance::ResourceBudget;
    use crate::types::CCITTFaxDecodeParams;

    #[test]
    fn rejects_negative_predictor_parameters() {
        let error = apply_predictor(vec![0], Some(10), Some(-1), Some(8), Some(1))
            .expect_err("negative colors must be rejected");
        assert!(error.to_string().contains("predictor"));
    }

    #[test]
    fn rejects_negative_ccitt_dimensions() {
        let params = CCITTFaxDecodeParams {
            columns: Some(-1),
            ..Default::default()
        };
        let error = decode_ccitt_fax(&[], &params, 1024).expect_err("negative columns");
        assert!(error.to_string().contains("columns"));
    }

    #[test]
    fn budgeted_decode_rejects_unfiltered_data_before_cloning() {
        let budget = ResourceBudget::new(100, 4, 4, 10, 1, 1, 1, 1);
        let error = decode_stream_with_budget(b"12345", &[], &budget)
            .expect_err("decoded data must respect the shared budget");
        assert!(error.to_string().contains("DecodedBytes"));
        assert_eq!(budget.remaining_decoded_bytes(), 4);
    }

    #[test]
    fn limited_decode_rejects_unfiltered_data() {
        let error = decode_stream_with_limits(b"12345", &[], 4, 10)
            .expect_err("unfiltered data must respect the output limit");
        assert!(error.to_string().contains("Decoded stream exceeds limit"));
    }
}
