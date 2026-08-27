#![allow(dead_code)]

/// CCITT Fax decoding implementation for PDF
/// Supports Group 3 (1D and 2D) and Group 4 fax compression
use crate::filters::ccitt_tables::{FaxTabEnt, FAX_BLACK_TABLE, FAX_MAIN_TABLE, FAX_WHITE_TABLE};
use crate::performance::ResourceBudget;

const S_NULL: u8 = 0;
const S_PASS: u8 = 1;
const S_HORIZ: u8 = 2;
const S_V0: u8 = 3;
const S_VR: u8 = 4;
const S_VL: u8 = 5;
const S_EXT: u8 = 6;
const S_TERMW: u8 = 7;
const S_TERMB: u8 = 8;
const S_MAKEUPW: u8 = 9;
const S_MAKEUPB: u8 = 10;
const S_MAKEUP: u8 = 11;
const S_EOL: u8 = 12;

/// CCITT Fax decoder with full Group 3 and Group 4 support
#[derive(Clone)]
pub struct CcittDecoder {
    columns: usize,
    rows: usize,
    k: i32,
    end_of_line: bool,
    encoded_byte_align: bool,
    end_of_block: bool,
    black_is_1: bool,
    damaged_rows_before_error: i32,
    max_output_bytes: Option<usize>,
}

impl CcittDecoder {
    /// Creates a new CCITT decoder with specified dimensions
    pub fn new(columns: usize, rows: usize) -> Self {
        Self {
            columns,
            rows,
            k: 0,
            end_of_line: false,
            encoded_byte_align: false,
            end_of_block: true,
            black_is_1: false,
            damaged_rows_before_error: 0,
            max_output_bytes: Some(
                usize::try_from(ResourceBudget::default().max_decoded_bytes_per_stream)
                    .unwrap_or(usize::MAX),
            ),
        }
    }

    /// Sets the K parameter for encoding type
    pub fn with_k(mut self, k: i32) -> Self {
        self.k = k;
        self
    }

    pub fn with_end_of_line(mut self, eol: bool) -> Self {
        self.end_of_line = eol;
        self
    }

    pub fn with_encoded_byte_align(mut self, align: bool) -> Self {
        self.encoded_byte_align = align;
        self
    }

    pub fn with_end_of_block(mut self, eob: bool) -> Self {
        self.end_of_block = eob;
        self
    }

    pub fn with_black_is_1(mut self, black: bool) -> Self {
        self.black_is_1 = black;
        self
    }

    pub fn with_damaged_rows_before_error(mut self, rows: i32) -> Self {
        self.damaged_rows_before_error = rows;
        self
    }

    pub fn with_max_output_bytes(mut self, max_output_bytes: usize) -> Self {
        self.max_output_bytes = Some(max_output_bytes);
        self
    }

    /// Decode Group 3 1D (Modified Huffman)
    pub fn decode_group3_1d(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut decoder = Group3Decoder::new(self.columns, self.rows, false);
        decoder.black_is_1 = self.black_is_1;
        decoder.end_of_line = self.end_of_line;
        decoder.encoded_byte_align = self.encoded_byte_align;
        decoder.end_of_block = self.end_of_block;
        decoder.damaged_rows_before_error = self.damaged_rows_before_error;
        decoder.max_output_bytes = self.max_output_bytes;
        decoder.decode_1d(data)
    }

    /// Decode Group 3 2D (Modified READ)
    pub fn decode_group3_2d(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut decoder = Group3Decoder::new(self.columns, self.rows, true);
        decoder.k = self.k;
        decoder.black_is_1 = self.black_is_1;
        decoder.end_of_line = self.end_of_line;
        decoder.encoded_byte_align = self.encoded_byte_align;
        decoder.end_of_block = self.end_of_block;
        decoder.damaged_rows_before_error = self.damaged_rows_before_error;
        decoder.max_output_bytes = self.max_output_bytes;
        decoder.decode_2d(data)
    }

    /// Decode Group 4 (Modified Modified READ)
    pub fn decode_group4(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut decoder = Group4Decoder::new(self.columns, self.rows);
        decoder.black_is_1 = self.black_is_1;
        decoder.end_of_block = self.end_of_block;
        decoder.damaged_rows_before_error = self.damaged_rows_before_error;
        decoder.max_output_bytes = self.max_output_bytes;
        decoder.decode(data)
    }

    /// Decode while charging input and output to a shared resource budget.
    pub fn decode_with_budget(
        &self,
        data: &[u8],
        budget: &ResourceBudget,
    ) -> Result<Vec<u8>, String> {
        budget.check().map_err(|error| error.to_string())?;
        budget
            .consume_input(data.len() as u64)
            .map_err(|error| error.to_string())?;
        let configured_limit = self.max_output_bytes.unwrap_or(usize::MAX);
        let budget_limit = usize::try_from(
            budget
                .max_decoded_bytes_per_stream
                .min(budget.remaining_decoded_bytes()),
        )
        .unwrap_or(usize::MAX);
        let decoder = self
            .clone()
            .with_max_output_bytes(configured_limit.min(budget_limit));
        let output = match decoder.k.cmp(&0) {
            std::cmp::Ordering::Less => decoder.decode_group4(data),
            std::cmp::Ordering::Equal => decoder.decode_group3_1d(data),
            std::cmp::Ordering::Greater => decoder.decode_group3_2d(data),
        }?;
        budget
            .consume_decoded(output.len() as u64)
            .map_err(|error| error.to_string())?;
        Ok(output)
    }
}

fn append_row(
    output: &mut Vec<u8>,
    row: &[u8],
    max_output_bytes: Option<usize>,
) -> Result<(), String> {
    if let Some(limit) = max_output_bytes {
        let next_len = output
            .len()
            .checked_add(row.len())
            .ok_or_else(|| "CCITT output size overflow".to_string())?;
        if next_len > limit {
            return Err("CCITT output exceeds limit".to_string());
        }
    }
    output.extend_from_slice(row);
    Ok(())
}

fn row_width(columns: usize, max_output_bytes: Option<usize>) -> Result<usize, String> {
    let bytes_per_row = columns.div_ceil(8);
    if max_output_bytes.is_some_and(|limit| bytes_per_row > limit) {
        return Err("CCITT row exceeds output limit".to_string());
    }
    Ok(bytes_per_row)
}

/// Group 3 Fax decoder
struct Group3Decoder {
    columns: usize,
    rows: usize,
    is_2d: bool,
    k: i32,
    black_is_1: bool,
    end_of_line: bool,
    encoded_byte_align: bool,
    end_of_block: bool,
    damaged_rows_before_error: i32,
    max_output_bytes: Option<usize>,
}

impl Group3Decoder {
    fn new(columns: usize, rows: usize, is_2d: bool) -> Self {
        Self {
            columns,
            rows,
            is_2d,
            k: if is_2d { 2 } else { 0 },
            black_is_1: false,
            end_of_line: false,
            encoded_byte_align: false,
            end_of_block: true,
            damaged_rows_before_error: 0,
            max_output_bytes: None,
        }
    }

    fn decode_1d(&mut self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut reader = BitReader::new(data);
        let bytes_per_row = row_width(self.columns, self.max_output_bytes)?;
        let mut output = Vec::new();
        let rows = if self.rows == 0 {
            usize::MAX
        } else {
            self.rows
        };
        let mut decoded_rows = 0;

        while decoded_rows < rows {
            if self.end_of_line {
                self.sync_to_eol(&mut reader)?;
                if self.encoded_byte_align {
                    reader.align_byte();
                }
            }

            let mut row = vec![0u8; bytes_per_row];
            let row_result = self.decode_row_1d(&mut reader, &mut row);
            match row_result {
                Ok(()) => {
                    append_row(&mut output, &row, self.max_output_bytes)?;
                    decoded_rows += 1;
                }
                Err(err) => {
                    if self.damaged_rows_before_error > 0 {
                        self.damaged_rows_before_error -= 1;
                        append_row(&mut output, &row, self.max_output_bytes)?;
                        decoded_rows += 1;
                    } else {
                        return Err(err);
                    }
                }
            }

            if reader.is_at_end() {
                break;
            }
        }

        if !self.black_is_1 {
            invert_bits(&mut output);
        }

        Ok(output)
    }

    fn decode_2d(&mut self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut reader = BitReader::new(data);
        let bytes_per_row = row_width(self.columns, self.max_output_bytes)?;
        let mut output = Vec::new();
        let rows = if self.rows == 0 {
            usize::MAX
        } else {
            self.rows
        };
        let mut decoded_rows = 0;
        let mut reference_row = vec![0u8; bytes_per_row];

        while decoded_rows < rows {
            if self.end_of_line {
                self.sync_to_eol(&mut reader)?;
                if self.encoded_byte_align {
                    reader.align_byte();
                }
            }

            let mut row = vec![0u8; bytes_per_row];
            let use_1d = if self.k > 0 {
                decoded_rows % (self.k as usize + 1) == 0
            } else {
                false
            };

            let row_result = if use_1d {
                self.decode_row_1d(&mut reader, &mut row)
            } else {
                self.decode_row_2d(&mut reader, &mut row, &reference_row)
            };

            match row_result {
                Ok(()) => {
                    append_row(&mut output, &row, self.max_output_bytes)?;
                    reference_row.copy_from_slice(&row);
                    decoded_rows += 1;
                }
                Err(err) => {
                    if self.damaged_rows_before_error > 0 {
                        self.damaged_rows_before_error -= 1;
                        append_row(&mut output, &row, self.max_output_bytes)?;
                        reference_row.copy_from_slice(&row);
                        decoded_rows += 1;
                    } else {
                        return Err(err);
                    }
                }
            }

            if reader.is_at_end() {
                break;
            }
        }

        if !self.black_is_1 {
            invert_bits(&mut output);
        }

        Ok(output)
    }

    fn decode_row_1d(&mut self, reader: &mut BitReader, row: &mut [u8]) -> Result<(), String> {
        let mut a0 = 0usize;
        let mut is_white = true;

        while a0 < self.columns {
            let run = if is_white {
                decode_white_run(reader)?
            } else {
                decode_black_run(reader)?
            };

            if run == 0 && a0 == 0 && self.end_of_line {
                break;
            }

            let run = run.min(self.columns.saturating_sub(a0));
            if !is_white {
                set_bits(row, a0, run);
            }
            a0 += run;
            is_white = !is_white;
        }

        Ok(())
    }

    fn decode_row_2d(
        &mut self,
        reader: &mut BitReader,
        row: &mut [u8],
        reference: &[u8],
    ) -> Result<(), String> {
        let mut a0 = 0usize;
        let mut is_white = true;
        let changes = collect_changes(reference, self.columns);

        while a0 < self.columns {
            let (b1, b2) = next_b1_b2(&changes, a0);
            match read_2d_mode(reader)? {
                Mode::Pass => {
                    a0 = b2;
                }
                Mode::Horizontal => {
                    let run1 = if is_white {
                        decode_white_run(reader)?
                    } else {
                        decode_black_run(reader)?
                    };
                    let run2 = if is_white {
                        decode_black_run(reader)?
                    } else {
                        decode_white_run(reader)?
                    };

                    let run1 = run1.min(self.columns.saturating_sub(a0));
                    if !is_white {
                        set_bits(row, a0, run1);
                    }
                    a0 += run1;

                    let run2 = run2.min(self.columns.saturating_sub(a0));
                    if is_white {
                        set_bits(row, a0, run2);
                    }
                    a0 += run2;
                }
                Mode::Vertical(offset) => {
                    let a1 = clamp_vertical(b1 as isize + offset, self.columns)?;
                    let run = a1.saturating_sub(a0);
                    if !is_white {
                        set_bits(row, a0, run);
                    }
                    a0 = a1;
                    is_white = !is_white;
                }
                Mode::Extension => {
                    return Err("CCITT uncompressed extension not supported".to_string());
                }
                Mode::EndOfLine => {
                    break;
                }
            }
        }

        Ok(())
    }

    fn sync_to_eol(&mut self, reader: &mut BitReader) -> Result<(), String> {
        let mut zeros = 0;
        loop {
            let bit = match reader.read_bit() {
                Ok(bit) => bit,
                Err(_) => return Err("Unexpected end of data while syncing EOL".to_string()),
            };
            if bit == 0 {
                zeros += 1;
            } else {
                if zeros >= 11 {
                    return Ok(());
                }
                zeros = 0;
            }
        }
    }
}

/// Group 4 Fax decoder
struct Group4Decoder {
    columns: usize,
    rows: usize,
    black_is_1: bool,
    end_of_block: bool,
    damaged_rows_before_error: i32,
    max_output_bytes: Option<usize>,
}

impl Group4Decoder {
    fn new(columns: usize, rows: usize) -> Self {
        Self {
            columns,
            rows,
            black_is_1: false,
            end_of_block: true,
            damaged_rows_before_error: 0,
            max_output_bytes: None,
        }
    }

    fn decode(&mut self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut reader = BitReader::new(data);
        let bytes_per_row = row_width(self.columns, self.max_output_bytes)?;
        let mut output = Vec::new();
        let rows = if self.rows == 0 {
            usize::MAX
        } else {
            self.rows
        };
        let mut decoded_rows = 0;
        let mut reference_row = vec![0u8; bytes_per_row];

        while decoded_rows < rows {
            let mut row = vec![0u8; bytes_per_row];
            let row_result = self.decode_row_mmr(&mut reader, &mut row, &reference_row);
            match row_result {
                Ok(()) => {
                    append_row(&mut output, &row, self.max_output_bytes)?;
                    reference_row.copy_from_slice(&row);
                    decoded_rows += 1;
                }
                Err(err) => {
                    if self.damaged_rows_before_error > 0 {
                        self.damaged_rows_before_error -= 1;
                        append_row(&mut output, &row, self.max_output_bytes)?;
                        reference_row.copy_from_slice(&row);
                        decoded_rows += 1;
                    } else {
                        return Err(err);
                    }
                }
            }

            if reader.is_at_end() {
                break;
            }
        }

        if self.end_of_block {
            // End-of-block marker is optional in PDF; ignore if missing
        }

        if !self.black_is_1 {
            invert_bits(&mut output);
        }

        Ok(output)
    }

    fn decode_row_mmr(
        &mut self,
        reader: &mut BitReader,
        row: &mut [u8],
        reference: &[u8],
    ) -> Result<(), String> {
        let mut a0 = 0usize;
        let mut is_white = true;
        let changes = collect_changes(reference, self.columns);

        while a0 < self.columns {
            let (b1, b2) = next_b1_b2(&changes, a0);
            match read_2d_mode(reader)? {
                Mode::Pass => {
                    a0 = b2;
                }
                Mode::Horizontal => {
                    let run1 = if is_white {
                        decode_white_run(reader)?
                    } else {
                        decode_black_run(reader)?
                    };
                    let run2 = if is_white {
                        decode_black_run(reader)?
                    } else {
                        decode_white_run(reader)?
                    };

                    let run1 = run1.min(self.columns.saturating_sub(a0));
                    if !is_white {
                        set_bits(row, a0, run1);
                    }
                    a0 += run1;

                    let run2 = run2.min(self.columns.saturating_sub(a0));
                    if is_white {
                        set_bits(row, a0, run2);
                    }
                    a0 += run2;
                }
                Mode::Vertical(offset) => {
                    let a1 = clamp_vertical(b1 as isize + offset, self.columns)?;
                    let run = a1.saturating_sub(a0);
                    if !is_white {
                        set_bits(row, a0, run);
                    }
                    a0 = a1;
                    is_white = !is_white;
                }
                Mode::Extension => {
                    return Err("CCITT uncompressed extension not supported".to_string());
                }
                Mode::EndOfLine => {
                    break;
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
enum Mode {
    Pass,
    Horizontal,
    Vertical(isize),
    Extension,
    EndOfLine,
}

fn read_2d_mode(reader: &mut BitReader) -> Result<Mode, String> {
    let bits = reader.peek_bits(7)? as usize;
    let entry = FAX_MAIN_TABLE
        .get(bits)
        .ok_or_else(|| "CCITT main table lookup out of range".to_string())?;
    reader.consume_bits(entry.width);
    match entry.state {
        S_PASS => Ok(Mode::Pass),
        S_HORIZ => Ok(Mode::Horizontal),
        S_V0 => Ok(Mode::Vertical(0)),
        S_VR => Ok(Mode::Vertical(entry.param as isize)),
        S_VL => Ok(Mode::Vertical(-(entry.param as isize))),
        S_EXT => Ok(Mode::Extension),
        S_EOL => Ok(Mode::EndOfLine),
        _ => Err(format!("Invalid CCITT 2D mode state: {}", entry.state)),
    }
}

fn decode_white_run(reader: &mut BitReader) -> Result<usize, String> {
    decode_run(reader, true)
}

fn decode_black_run(reader: &mut BitReader) -> Result<usize, String> {
    decode_run(reader, false)
}

fn decode_run(reader: &mut BitReader, white: bool) -> Result<usize, String> {
    let mut run = 0usize;
    loop {
        let entry = if white {
            lookup_white(reader)?
        } else {
            lookup_black(reader)?
        };

        match entry.state {
            S_TERMW | S_TERMB => {
                run += entry.param as usize;
                return Ok(run);
            }
            S_MAKEUPW | S_MAKEUPB | S_MAKEUP => {
                run += entry.param as usize;
            }
            S_EOL => {
                return Err("Unexpected EOL in run decoding".to_string());
            }
            S_NULL => {
                return Err("Invalid CCITT code (null state)".to_string());
            }
            _ => {
                return Err(format!("Invalid CCITT run state: {}", entry.state));
            }
        }
    }
}

fn lookup_white(reader: &mut BitReader) -> Result<FaxTabEnt, String> {
    let idx = reader.peek_bits(12)? as usize;
    let entry = *FAX_WHITE_TABLE
        .get(idx)
        .ok_or_else(|| "White table lookup out of range".to_string())?;
    reader.consume_bits(entry.width);
    Ok(entry)
}

fn lookup_black(reader: &mut BitReader) -> Result<FaxTabEnt, String> {
    let idx = reader.peek_bits(13)? as usize;
    let entry = *FAX_BLACK_TABLE
        .get(idx)
        .ok_or_else(|| "Black table lookup out of range".to_string())?;
    reader.consume_bits(entry.width);
    Ok(entry)
}

fn invert_bits(data: &mut [u8]) {
    for byte in data {
        *byte = !*byte;
    }
}

fn set_bits(row: &mut [u8], start: usize, len: usize) {
    for i in start..start + len {
        let byte_idx = i / 8;
        let bit_idx = 7 - (i % 8);
        if let Some(byte) = row.get_mut(byte_idx) {
            *byte |= 1 << bit_idx;
        }
    }
}

fn collect_changes(reference: &[u8], columns: usize) -> Vec<usize> {
    let mut changes = Vec::new();
    let mut last = false;
    let mut pos = 0;

    for byte in reference {
        for bit in (0..8).rev() {
            if pos >= columns {
                break;
            }
            let value = (byte >> bit) & 1 != 0;
            if pos == 0 {
                last = value;
            } else if value != last {
                changes.push(pos);
                last = value;
            }
            pos += 1;
        }
    }
    changes.push(columns);
    changes
}

fn next_b1_b2(changes: &[usize], a0: usize) -> (usize, usize) {
    let mut i = 0;
    while i < changes.len() && changes[i] <= a0 {
        i += 1;
    }
    let b1 = changes.get(i).copied().unwrap_or(a0);
    let b2 = changes.get(i + 1).copied().unwrap_or(b1);
    (b1, b2)
}

fn clamp_vertical(pos: isize, columns: usize) -> Result<usize, String> {
    if pos < 0 {
        return Err("CCITT vertical offset underflow".to_string());
    }
    Ok(pos.min(columns as isize) as usize)
}

/// Bit reader for CCITT decoding (LSB-first with per-byte bit reversal)
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_acc: u32,
    bits_avail: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_acc: 0,
            bits_avail: 0,
        }
    }

    fn read_bit(&mut self) -> Result<u8, String> {
        let bit = self.read_bits(1)?;
        Ok(bit as u8)
    }

    fn read_bits(&mut self, count: u8) -> Result<u32, String> {
        let bits = self.peek_bits(count)?;
        self.consume_bits(count);
        Ok(bits)
    }

    fn peek_bits(&mut self, count: u8) -> Result<u32, String> {
        self.ensure_bits(count)?;
        let mask = if count == 32 {
            u32::MAX
        } else {
            (1u32 << count) - 1
        };
        Ok(self.bit_acc & mask)
    }

    fn consume_bits(&mut self, count: u8) {
        self.bit_acc >>= count;
        self.bits_avail = self.bits_avail.saturating_sub(count);
    }

    fn ensure_bits(&mut self, count: u8) -> Result<(), String> {
        while self.bits_avail < count {
            if self.byte_pos >= self.data.len() {
                if self.bits_avail == 0 {
                    return Err("End of data".to_string());
                }
                self.bits_avail = count;
                return Ok(());
            }
            let byte = self.data[self.byte_pos].reverse_bits();
            self.byte_pos += 1;
            self.bit_acc |= (byte as u32) << self.bits_avail;
            self.bits_avail = self.bits_avail.saturating_add(8);
        }
        Ok(())
    }

    fn align_byte(&mut self) {
        let rem = self.bits_avail % 8;
        if rem != 0 {
            self.consume_bits(rem);
        }
    }

    fn is_at_end(&self) -> bool {
        self.byte_pos >= self.data.len() && self.bits_avail == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_bits_lsb(bits: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut current = 0u8;
        let mut pos = 0u8;
        for &bit in bits {
            current |= (bit & 1) << pos;
            pos += 1;
            if pos == 8 {
                out.push(current.reverse_bits());
                current = 0;
                pos = 0;
            }
        }
        if pos != 0 {
            out.push(current.reverse_bits());
        }
        out
    }

    #[test]
    fn test_bit_reader_roundtrip() {
        let data = pack_bits_lsb(&[1, 0, 1, 1, 0, 0, 1, 0]);
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_bits(4).unwrap(), 0b1101);
    }

    #[test]
    fn test_group3_basic_decode() {
        let decoder = CcittDecoder::new(8, 1).with_black_is_1(true);
        let white8_bits = [1, 0, 0, 1, 1]; // LSB-first for run length 8
        let data = pack_bits_lsb(&white8_bits);
        let result = decoder.decode_group3_1d(&data).unwrap();
        assert_eq!(result, vec![0x00]);
    }

    #[test]
    fn test_group4_basic_decode() {
        let decoder = CcittDecoder::new(8, 1).with_black_is_1(true);
        let v0_bits = [1];
        let data = pack_bits_lsb(&v0_bits);
        let result = decoder.decode_group4(&data).unwrap();
        assert_eq!(result, vec![0x00]);
    }

    #[test]
    fn enforces_output_limit_before_appending_rows() {
        let decoder = CcittDecoder::new(8, 1)
            .with_black_is_1(true)
            .with_max_output_bytes(0);
        let data = pack_bits_lsb(&[1, 0, 0, 1, 1]);
        assert!(decoder.decode_group3_1d(&data).is_err());
    }

    #[test]
    fn rejects_rows_too_wide_for_the_output_budget() {
        let decoder = CcittDecoder::new(usize::MAX, 1);
        assert!(decoder.decode_group4(&[]).is_err());
    }

    #[test]
    fn budgeted_decoder_rejects_input_before_decoding() {
        let budget = ResourceBudget::new(0, 1024, 1024, 100, 10, 10, 10, 10);
        let decoder = CcittDecoder::new(8, 1);
        assert!(decoder
            .decode_with_budget(&[0], &budget)
            .expect_err("CCITT input must respect the budget")
            .contains("InputBytes"));
    }
}
