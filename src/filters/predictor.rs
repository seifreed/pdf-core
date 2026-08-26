use super::FilterResult;

/// PNG Predictor implementation for PDF streams
///
/// PDF uses PNG-style predictors to improve compression efficiency.
/// This is commonly used with FlateDecode and LZWDecode filters.

#[derive(Debug, Clone, Copy)]
pub enum PredictorType {
    None = 1,
    TIFF = 2,
    PNG = 10,
    PNGOptimum = 15,
}

pub struct PredictorDecoder {
    predictor: PredictorType,
    colors: u8,
    bits_per_component: u8,
    columns: u32,
}

impl PredictorDecoder {
    pub fn new(predictor: i32, colors: u8, bits_per_component: u8, columns: u32) -> Self {
        let predictor_type = match predictor {
            1 => PredictorType::None,
            2 => PredictorType::TIFF,
            10..=14 => PredictorType::PNG,
            15 => PredictorType::PNGOptimum,
            _ => PredictorType::None,
        };

        Self {
            predictor: predictor_type,
            colors,
            bits_per_component,
            columns,
        }
    }

    pub fn decode(&self, data: &[u8]) -> FilterResult<Vec<u8>> {
        match self.predictor {
            PredictorType::None => Ok(data.to_vec()),
            PredictorType::TIFF => self.decode_tiff_predictor(data),
            PredictorType::PNG | PredictorType::PNGOptimum => self.decode_png_predictor(data),
        }
    }

    fn decode_tiff_predictor(&self, data: &[u8]) -> FilterResult<Vec<u8>> {
        let bytes_per_pixel = (self.colors as u32 * self.bits_per_component as u32).div_ceil(8);
        let bytes_per_row =
            (self.columns * self.colors as u32 * self.bits_per_component as u32).div_ceil(8);

        if !data.len().is_multiple_of(bytes_per_row as usize) {
            return Err(crate::filters::FilterError::InvalidData(
                "Data length not divisible by row length for TIFF predictor".to_string(),
            ));
        }

        let mut result = Vec::with_capacity(data.len());

        for row in data.chunks_exact(bytes_per_row as usize) {
            let mut decoded_row = vec![0u8; bytes_per_row as usize];

            // Copy first pixel as-is
            let pixel_size = bytes_per_pixel as usize;
            decoded_row[..pixel_size].copy_from_slice(&row[..pixel_size]);

            // Decode subsequent pixels
            for i in (pixel_size..row.len()).step_by(pixel_size) {
                for j in 0..pixel_size {
                    let current = row[i + j] as u16;
                    let previous = decoded_row[i + j - pixel_size] as u16;
                    decoded_row[i + j] = ((current + previous) & 0xFF) as u8;
                }
            }

            result.extend_from_slice(&decoded_row);
        }

        Ok(result)
    }

    fn decode_png_predictor(&self, data: &[u8]) -> FilterResult<Vec<u8>> {
        let bytes_per_pixel = (self.colors as u32 * self.bits_per_component as u32).div_ceil(8);
        let bytes_per_row =
            (self.columns * self.colors as u32 * self.bits_per_component as u32).div_ceil(8);
        let row_length = bytes_per_row as usize + 1; // +1 for predictor byte

        if !data.len().is_multiple_of(row_length) {
            return Err(crate::filters::FilterError::InvalidData(
                "Data length not compatible with PNG predictor format".to_string(),
            ));
        }

        let mut result = Vec::new();
        let mut previous_row: Vec<u8> = vec![0; bytes_per_row as usize];

        for chunk in data.chunks_exact(row_length) {
            let predictor_byte = chunk[0];
            let row_data = &chunk[1..];
            let mut decoded_row = vec![0u8; bytes_per_row as usize];

            match predictor_byte {
                0 => {
                    // None predictor - copy as-is
                    decoded_row.copy_from_slice(row_data);
                }
                1 => {
                    // Sub predictor - add left pixel
                    decoded_row[..bytes_per_pixel as usize]
                        .copy_from_slice(&row_data[..bytes_per_pixel as usize]);

                    for i in bytes_per_pixel as usize..decoded_row.len() {
                        let current = row_data[i] as u16;
                        let left = decoded_row[i - bytes_per_pixel as usize] as u16;
                        decoded_row[i] = ((current + left) & 0xFF) as u8;
                    }
                }
                2 => {
                    // Up predictor - add upper pixel
                    for i in 0..decoded_row.len() {
                        let current = row_data[i] as u16;
                        let up = previous_row[i] as u16;
                        decoded_row[i] = ((current + up) & 0xFF) as u8;
                    }
                }
                3 => {
                    // Average predictor
                    for i in 0..decoded_row.len() {
                        let current = row_data[i] as u16;
                        let left = if i >= bytes_per_pixel as usize {
                            decoded_row[i - bytes_per_pixel as usize] as u16
                        } else {
                            0
                        };
                        let up = previous_row[i] as u16;
                        let average = (left + up) / 2;
                        decoded_row[i] = ((current + average) & 0xFF) as u8;
                    }
                }
                4 => {
                    // Paeth predictor
                    for i in 0..decoded_row.len() {
                        let current = row_data[i] as u16;
                        let left = if i >= bytes_per_pixel as usize {
                            decoded_row[i - bytes_per_pixel as usize] as u16
                        } else {
                            0
                        };
                        let up = previous_row[i] as u16;
                        let up_left = if i >= bytes_per_pixel as usize {
                            previous_row[i - bytes_per_pixel as usize] as u16
                        } else {
                            0
                        };

                        let paeth = paeth_predictor(left, up, up_left);
                        decoded_row[i] = ((current + paeth) & 0xFF) as u8;
                    }
                }
                _ => {
                    return Err(crate::filters::FilterError::InvalidData(format!(
                        "Unknown PNG predictor type: {}",
                        predictor_byte
                    )));
                }
            }

            result.extend_from_slice(&decoded_row);
            previous_row = decoded_row;
        }

        Ok(result)
    }
}

fn paeth_predictor(a: u16, b: u16, c: u16) -> u16 {
    let p = a as i32 + b as i32 - c as i32;
    let pa = (p - a as i32).abs();
    let pb = (p - b as i32).abs();
    let pc = (p - c as i32).abs();

    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_predictor() {
        let decoder = PredictorDecoder::new(1, 1, 8, 4);
        let data = vec![1, 2, 3, 4];
        let result = decoder.decode(&data).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_png_sub_predictor() {
        let decoder = PredictorDecoder::new(10, 1, 8, 4);
        // Predictor byte 1 (Sub) + 4 bytes of data
        let data = vec![1, 10, 5, 3, 7]; // predictor=1, then differences
        let result = decoder.decode(&data).unwrap();
        // Expected: 10, 15 (10+5), 18 (15+3), 25 (18+7)
        assert_eq!(result, vec![10, 15, 18, 25]);
    }

    #[test]
    fn test_png_up_predictor() {
        let decoder = PredictorDecoder::new(10, 1, 8, 2);
        // Two rows with Up predictor
        let data = vec![
            0, 10, 20, // First row: no predictor + data
            2, 5, 8, // Second row: up predictor + differences
        ];
        let result = decoder.decode(&data).unwrap();
        // Expected: [10, 20] (first row) + [15, 28] (10+5, 20+8)
        assert_eq!(result, vec![10, 20, 15, 28]);
    }
}
