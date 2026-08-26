use crate::filters::FilterError;

pub fn decode_jpx_to_codestream(data: &[u8]) -> Result<Vec<u8>, FilterError> {
    decode_jpx_to_codestream_with_limit(data, usize::MAX)
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
