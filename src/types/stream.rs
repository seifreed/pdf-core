use crate::performance::ResourceBudget;
use crate::types::{PdfDictionary, PdfName, PdfValue};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct PdfStream {
    pub dict: PdfDictionary,
    pub data: StreamData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamData {
    Raw(Vec<u8>),
    Decoded(Vec<u8>),
    Lazy(StreamReference),
}

impl StreamData {
    pub fn len(&self) -> usize {
        match self {
            StreamData::Raw(data) | StreamData::Decoded(data) => data.len(),
            StreamData::Lazy(reference) => reference.length,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        match self {
            StreamData::Raw(data) | StreamData::Decoded(data) => {
                data.hash(&mut hasher);
            }
            StreamData::Lazy(reference) => {
                reference.offset.hash(&mut hasher);
                reference.length.hash(&mut hasher);
            }
        }
        format!("{:x}", hasher.finish())
    }

    pub fn truncate(&mut self, len: usize) {
        match self {
            StreamData::Raw(data) | StreamData::Decoded(data) => {
                data.truncate(len);
            }
            StreamData::Lazy(_) => {
                // Cannot truncate lazy streams
            }
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            StreamData::Raw(data) | StreamData::Decoded(data) => Some(data),
            StreamData::Lazy(_) => None,
        }
    }
}

impl std::ops::Index<usize> for StreamData {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        match self {
            StreamData::Raw(data) | StreamData::Decoded(data) => &data[index],
            StreamData::Lazy(_) => panic!("Cannot index into lazy stream data"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamReference {
    pub offset: u64,
    pub length: usize,
    pub filters: Vec<StreamFilter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamFilter {
    ASCIIHexDecode,
    ASCII85Decode,
    LZWDecode(LZWDecodeParams),
    FlateDecode(FlateDecodeParams),
    RunLengthDecode,
    CCITTFaxDecode(CCITTFaxDecodeParams),
    JBIG2Decode,
    DCTDecode,
    JPXDecode,
    Crypt(CryptFilter),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LZWDecodeParams {
    pub predictor: Option<i32>,
    pub colors: Option<i32>,
    pub bits_per_component: Option<i32>,
    pub columns: Option<i32>,
    pub early_change: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FlateDecodeParams {
    pub predictor: Option<i32>,
    pub colors: Option<i32>,
    pub bits_per_component: Option<i32>,
    pub columns: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CCITTFaxDecodeParams {
    pub k: Option<i32>,
    pub end_of_line: Option<bool>,
    pub encoded_byte_align: Option<bool>,
    pub columns: Option<i32>,
    pub rows: Option<i32>,
    pub end_of_block: Option<bool>,
    pub black_is_1: Option<bool>,
    pub damaged_rows_before_error: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CryptFilter {
    pub name: PdfName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CryptFilterParams {
    /// Identity filter - no encryption
    Identity,
    /// V2 standard security handler (RC4)
    V2 { name: String },
    /// AESV2 - AES-128 encryption
    AESV2 { name: String },
    /// AESV3 - AES-256 encryption
    AESV3 { name: String },
}

impl PdfStream {
    pub fn new(dict: PdfDictionary, data: Vec<u8>) -> Self {
        PdfStream {
            dict,
            data: StreamData::Raw(data),
        }
    }

    pub fn new_lazy(dict: PdfDictionary, reference: StreamReference) -> Self {
        PdfStream {
            dict,
            data: StreamData::Lazy(reference),
        }
    }

    pub fn raw_data(&self) -> Option<&[u8]> {
        match &self.data {
            StreamData::Raw(data) => Some(data),
            _ => None,
        }
    }

    pub fn decode(&self) -> Result<Vec<u8>, String> {
        match &self.data {
            StreamData::Raw(data) | StreamData::Decoded(data) => {
                let filters = self.get_filters_with_params();
                if filters.is_empty() {
                    Ok(data.clone())
                } else {
                    crate::filters::decode_stream(data, &filters).map_err(|e| e.to_string())
                }
            }
            StreamData::Lazy(_) => Err("Lazy stream decoding not implemented".to_string()),
        }
    }

    pub fn decode_with_budget(&self, budget: &ResourceBudget) -> Result<Vec<u8>, String> {
        match &self.data {
            StreamData::Raw(data) | StreamData::Decoded(data) => {
                let filters = self.get_filters_with_params();
                crate::filters::decode_stream_with_budget(data, &filters, budget)
                    .map_err(|e| e.to_string())
            }
            StreamData::Lazy(_) => Err("Lazy stream decoding not implemented".to_string()),
        }
    }

    pub fn decode_with_limits(
        &self,
        max_output_bytes: usize,
        max_ratio: usize,
    ) -> Result<Vec<u8>, String> {
        match &self.data {
            StreamData::Raw(data) | StreamData::Decoded(data) => {
                let filters = self.get_filters_with_params();
                crate::filters::decode_stream_with_limits(
                    data,
                    &filters,
                    max_output_bytes,
                    max_ratio,
                )
                .map_err(|e| e.to_string())
            }
            StreamData::Lazy(_) => Err("Lazy stream decoding not implemented".to_string()),
        }
    }

    pub fn decoded_data(&self) -> Option<&[u8]> {
        match &self.data {
            StreamData::Decoded(data) => Some(data),
            _ => None,
        }
    }

    pub fn is_lazy(&self) -> bool {
        matches!(self.data, StreamData::Lazy(_))
    }

    pub fn length(&self) -> Option<usize> {
        match &self.data {
            StreamData::Raw(data) | StreamData::Decoded(data) => Some(data.len()),
            StreamData::Lazy(reference) => Some(reference.length),
        }
    }

    pub fn get_filters(&self) -> Vec<StreamFilter> {
        self.get_filters_with_params()
    }

    pub fn get_filters_with_params(&self) -> Vec<StreamFilter> {
        let mut filters = Vec::new();

        let filter_names: Vec<&PdfName> = match self.dict.get("Filter") {
            Some(PdfValue::Name(name)) => vec![name],
            Some(PdfValue::Array(array)) => array.iter().filter_map(|v| v.as_name()).collect(),
            _ => Vec::new(),
        };

        if filter_names.is_empty() {
            return filters;
        }

        let mut decode_params = match self.dict.get("DecodeParms") {
            Some(PdfValue::Dictionary(dict)) => vec![Some(dict)],
            Some(PdfValue::Array(array)) => array.iter().map(|v| v.as_dict()).collect(),
            Some(PdfValue::Null) => vec![None],
            _ => Vec::new(),
        };

        if decode_params.len() < filter_names.len() {
            decode_params.resize(filter_names.len(), None);
        }

        for (i, name) in filter_names.iter().enumerate() {
            let params = decode_params.get(i).copied().unwrap_or(None);
            if let Some(filter) = Self::filter_from_name_with_params(name, params) {
                filters.push(filter);
            }
        }

        filters
    }

    fn filter_from_name_with_params(
        name: &PdfName,
        params: Option<&PdfDictionary>,
    ) -> Option<StreamFilter> {
        match name.without_slash() {
            "ASCIIHexDecode" | "AHx" => Some(StreamFilter::ASCIIHexDecode),
            "ASCII85Decode" | "A85" => Some(StreamFilter::ASCII85Decode),
            "LZWDecode" | "LZW" => {
                let mut parsed = LZWDecodeParams::default();
                if let Some(params) = params {
                    parsed = parse_lzw_params(params);
                }
                Some(StreamFilter::LZWDecode(parsed))
            }
            "FlateDecode" | "Fl" => {
                let mut parsed = FlateDecodeParams::default();
                if let Some(params) = params {
                    parsed = parse_flate_params(params);
                }
                Some(StreamFilter::FlateDecode(parsed))
            }
            "RunLengthDecode" | "RL" => Some(StreamFilter::RunLengthDecode),
            "CCITTFaxDecode" | "CCF" => {
                let mut parsed = CCITTFaxDecodeParams::default();
                if let Some(params) = params {
                    parsed = parse_ccitt_params(params);
                }
                Some(StreamFilter::CCITTFaxDecode(parsed))
            }
            "JBIG2Decode" => Some(StreamFilter::JBIG2Decode),
            "DCTDecode" | "DCT" => Some(StreamFilter::DCTDecode),
            "JPXDecode" => Some(StreamFilter::JPXDecode),
            "Crypt" => {
                let crypt_name = params
                    .and_then(|p| p.get("Name"))
                    .and_then(|v| v.as_name())
                    .cloned()
                    .unwrap_or_else(|| PdfName::new("Identity"));
                Some(StreamFilter::Crypt(CryptFilter { name: crypt_name }))
            }
            _ => None,
        }
    }
}

fn parse_flate_params(params: &PdfDictionary) -> FlateDecodeParams {
    FlateDecodeParams {
        predictor: params
            .get("Predictor")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
        colors: params
            .get("Colors")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
        bits_per_component: params
            .get("BitsPerComponent")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
        columns: params
            .get("Columns")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
    }
}

fn parse_lzw_params(params: &PdfDictionary) -> LZWDecodeParams {
    LZWDecodeParams {
        predictor: params
            .get("Predictor")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
        colors: params
            .get("Colors")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
        bits_per_component: params
            .get("BitsPerComponent")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
        columns: params
            .get("Columns")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
        early_change: params.get("EarlyChange").and_then(bool_from_value),
    }
}

fn parse_ccitt_params(params: &PdfDictionary) -> CCITTFaxDecodeParams {
    CCITTFaxDecodeParams {
        k: params
            .get("K")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
        end_of_line: params.get("EndOfLine").and_then(bool_from_value),
        encoded_byte_align: params.get("EncodedByteAlign").and_then(bool_from_value),
        columns: params
            .get("Columns")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
        rows: params
            .get("Rows")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
        end_of_block: params.get("EndOfBlock").and_then(bool_from_value),
        black_is_1: params.get("BlackIs1").and_then(bool_from_value),
        damaged_rows_before_error: params
            .get("DamagedRowsBeforeError")
            .and_then(|v| v.as_integer())
            .map(|v| v as i32),
    }
}

fn bool_from_value(value: &PdfValue) -> Option<bool> {
    match value {
        PdfValue::Boolean(b) => Some(*b),
        PdfValue::Integer(i) => Some(*i != 0),
        PdfValue::Real(r) => Some(*r != 0.0),
        _ => None,
    }
}

impl StreamFilter {
    pub fn from_name(name: &PdfName) -> Option<Self> {
        match name.without_slash() {
            "ASCIIHexDecode" | "AHx" => Some(StreamFilter::ASCIIHexDecode),
            "ASCII85Decode" | "A85" => Some(StreamFilter::ASCII85Decode),
            "LZWDecode" | "LZW" => Some(StreamFilter::LZWDecode(LZWDecodeParams::default())),
            "FlateDecode" | "Fl" => Some(StreamFilter::FlateDecode(FlateDecodeParams::default())),
            "RunLengthDecode" | "RL" => Some(StreamFilter::RunLengthDecode),
            "CCITTFaxDecode" | "CCF" => {
                Some(StreamFilter::CCITTFaxDecode(CCITTFaxDecodeParams::default()))
            }
            "JBIG2Decode" => Some(StreamFilter::JBIG2Decode),
            "DCTDecode" | "DCT" => Some(StreamFilter::DCTDecode),
            "JPXDecode" => Some(StreamFilter::JPXDecode),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            StreamFilter::ASCIIHexDecode => "ASCIIHexDecode",
            StreamFilter::ASCII85Decode => "ASCII85Decode",
            StreamFilter::LZWDecode(_) => "LZWDecode",
            StreamFilter::FlateDecode(_) => "FlateDecode",
            StreamFilter::RunLengthDecode => "RunLengthDecode",
            StreamFilter::CCITTFaxDecode(_) => "CCITTFaxDecode",
            StreamFilter::JBIG2Decode => "JBIG2Decode",
            StreamFilter::DCTDecode => "DCTDecode",
            StreamFilter::JPXDecode => "JPXDecode",
            StreamFilter::Crypt(_) => "Crypt",
        }
    }
}

impl fmt::Display for PdfStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} stream[", self.dict)?;
        match &self.data {
            StreamData::Raw(data) => write!(f, "{} bytes raw", data.len())?,
            StreamData::Decoded(data) => write!(f, "{} bytes decoded", data.len())?,
            StreamData::Lazy(reference) => write!(f, "{} bytes lazy", reference.length)?,
        }
        write!(f, "]endstream")
    }
}
