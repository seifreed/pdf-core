use crate::filters::FilterError;
use crate::performance::ResourceBudget;

pub fn decode_jpx_to_codestream(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    decode_jpx_to_codestream_with_budget(data, &ResourceBudget::default())
}

pub fn decode_jpx_to_codestream_with_budget(
    data: &[u8],
    budget: &ResourceBudget,
) -> Result<Vec<u8>, FilterError> {
    budget
        .check()
        .map_err(|error| FilterError::DecompressionError(error.to_string()))?;
    budget
        .consume_input(data.len() as u64)
        .map_err(|error| FilterError::DecompressionError(error.to_string()))?;
    let max_output_bytes = usize::try_from(
        budget
            .max_decoded_bytes_per_stream
            .min(budget.remaining_decoded_bytes()),
    )
    .unwrap_or(usize::MAX);
    let output = decode_jpx_to_codestream_with_limit(data, max_output_bytes)?;
    budget
        .consume_decoded(output.len() as u64)
        .map_err(|error| FilterError::DecompressionError(error.to_string()))?;
    Ok(output)
}

pub fn decode_jpx_to_codestream_with_limit(
    data: &[u8],
    max_output_bytes: usize,
) -> Result<Vec<u8>, FilterError> {
    if data.len() < 2 {
        return Err(FilterError::InvalidData("JPX data too short".to_string()));
    }

    // Raw codestream (SOC marker 0xFF4F)
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0x4F {
        if data.len() > max_output_bytes {
            return Err(FilterError::DecompressionError(
                "JPX output exceeds limit".to_string(),
            ));
        }
        return Ok(data.to_vec());
    }

    // JP2 container signature box
    if data.len() < 12 {
        return Err(FilterError::InvalidData(
            "JP2 container too short".to_string(),
        ));
    }

    let mut pos = 0;
    let mut codestreams = Vec::new();
    let mut codestream_length = 0usize;
    let mut saw_signature = false;

    while pos <= data.len().saturating_sub(8) {
        let header_end = pos
            .checked_add(8)
            .ok_or_else(|| FilterError::InvalidData("JP2 header offset overflow".to_string()))?;
        let header = data
            .get(pos..header_end)
            .ok_or_else(|| FilterError::InvalidData("JP2 header outside buffer".to_string()))?;
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let box_type = &header[4..8];
        pos = header_end;

        let (box_len, header_extra) = if length == 1 {
            let ext_end = pos.checked_add(8).ok_or_else(|| {
                FilterError::InvalidData("JP2 extended length offset overflow".to_string())
            })?;
            let ext = data.get(pos..ext_end).ok_or_else(|| {
                FilterError::InvalidData("JP2 box missing extended length".to_string())
            })?;
            let ext_len = u64::from_be_bytes([
                ext[0], ext[1], ext[2], ext[3], ext[4], ext[5], ext[6], ext[7],
            ]);
            pos = ext_end;
            if ext_len < 16 {
                return Err(FilterError::InvalidData(
                    "JP2 box extended length invalid".to_string(),
                ));
            }
            let box_len = usize::try_from(ext_len).map_err(|_| {
                FilterError::InvalidData("JP2 box length exceeds platform size".to_string())
            })?;
            (box_len, 16)
        } else if length == 0 {
            // box extends to end of file
            let remaining = data.len().saturating_sub(pos);
            let box_len = remaining
                .checked_add(8)
                .ok_or_else(|| FilterError::InvalidData("JP2 box length overflow".to_string()))?;
            (box_len, 8)
        } else {
            (length as usize, 8)
        };

        if box_len < header_extra {
            return Err(FilterError::InvalidData(
                "JP2 box length invalid".to_string(),
            ));
        }

        let payload_len = box_len - header_extra;
        let payload_end = pos
            .checked_add(payload_len)
            .ok_or_else(|| FilterError::InvalidData("JP2 payload offset overflow".to_string()))?;

        let payload = data
            .get(pos..payload_end)
            .ok_or_else(|| FilterError::InvalidData("JP2 box length exceeds buffer".to_string()))?;
        pos = payload_end;

        match box_type {
            b"jP  " => {
                if payload != b"\x0D\x0A\x87\x0A" {
                    return Err(FilterError::InvalidData(
                        "JP2 signature box invalid".to_string(),
                    ));
                }
                saw_signature = true;
            }
            b"jp2c" => {
                codestream_length =
                    codestream_length
                        .checked_add(payload.len())
                        .ok_or_else(|| {
                            FilterError::InvalidData("JPX output size overflow".to_string())
                        })?;
                if codestream_length > max_output_bytes {
                    return Err(FilterError::DecompressionError(
                        "JPX output exceeds limit".to_string(),
                    ));
                }
                codestreams.push(payload.to_vec());
            }
            _ => {}
        }
    }

    if pos != data.len() {
        return Err(FilterError::InvalidData(
            "JP2 container has trailing bytes".to_string(),
        ));
    }

    if !saw_signature {
        return Err(FilterError::InvalidData(
            "JP2 signature not found".to_string(),
        ));
    }
    if codestreams.is_empty() {
        return Err(FilterError::InvalidData(
            "JP2 codestream not found".to_string(),
        ));
    }

    Ok(codestreams.concat())
}

/// Decode a JPX image to interleaved 8-bit pixels.
pub fn decode_jpx_image(data: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, FilterError> {
    let image = pdfluent_jpeg2000::Image::new(data, &pdfluent_jpeg2000::DecodeSettings::default())
        .map_err(|error| FilterError::ImageDecodeError(format!("JPX decode error: {}", error)))?;
    let channels = usize::from(image.color_space().num_channels()) + usize::from(image.has_alpha());
    let output_bytes = usize::try_from(image.width())
        .ok()
        .and_then(|width| {
            usize::try_from(image.height())
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or_else(|| FilterError::ImageDecodeError("JPX output size overflow".to_string()))?;
    if output_bytes > max_output_bytes {
        return Err(FilterError::DecompressionError(
            "JPX output exceeds limit".to_string(),
        ));
    }
    let decoded = image
        .decode()
        .map_err(|error| FilterError::ImageDecodeError(format!("JPX decode error: {}", error)))?;
    if decoded.len() > max_output_bytes {
        return Err(FilterError::DecompressionError(
            "JPX output exceeds limit".to_string(),
        ));
    }
    Ok(decoded)
}

pub fn decode_jpx_image_with_budget(
    data: &[u8],
    budget: &ResourceBudget,
) -> Result<Vec<u8>, FilterError> {
    budget
        .check()
        .map_err(|error| FilterError::ImageDecodeError(error.to_string()))?;
    budget
        .consume_input(data.len() as u64)
        .map_err(|error| FilterError::ImageDecodeError(error.to_string()))?;
    let max_output_bytes = usize::try_from(
        budget
            .max_decoded_bytes_per_stream
            .min(budget.remaining_decoded_bytes()),
    )
    .unwrap_or(usize::MAX);
    let output = decode_jpx_image(data, max_output_bytes)?;
    budget
        .consume_decoded(output.len() as u64)
        .map_err(|error| FilterError::ImageDecodeError(error.to_string()))?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{decode_jpx_image_with_budget, decode_jpx_to_codestream_with_budget};
    use crate::performance::ResourceBudget;

    #[test]
    fn jpx_budget_rejects_input_before_decoding() {
        let budget = ResourceBudget::new(0, 1024, 1024, 100, 10, 10, 10, 10);
        assert!(decode_jpx_to_codestream_with_budget(b"x", &budget)
            .expect_err("JPX input must respect the budget")
            .to_string()
            .contains("InputBytes"));
        assert!(decode_jpx_image_with_budget(b"x", &budget)
            .expect_err("JPX image input must respect the budget")
            .to_string()
            .contains("InputBytes"));
    }
}
