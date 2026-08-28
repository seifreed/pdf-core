use crate::performance::ResourceBudget;
use crate::types::{PdfDictionary, PdfName, PdfValue};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct PdfStream {
    pub dict: PdfDictionary,
    pub data: StreamData,
    pub lossless: StreamLosslessState,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StreamLosslessState {
    #[serde(default)]
    pub original_bytes: Option<Vec<u8>>,
    #[serde(default)]
    pub declared_length: Option<u64>,
    pub observed_length: usize,
    #[serde(default)]
    pub parse_errors: Vec<String>,
    #[serde(default)]
    pub recovery_actions: Vec<String>,
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

    pub fn get(&self, index: usize) -> Option<&u8> {
        match self {
            StreamData::Raw(data) | StreamData::Decoded(data) => data.get(index),
            StreamData::Lazy(_) => None,
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
    JBIG2Decode(JBIG2DecodeParams),
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

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct JBIG2DecodeParams {
    /// Directly embedded `/JBIG2Globals` bytes, when available.
    /// Indirect references require document-level resolution before decoding.
    pub globals: Option<Vec<u8>>,
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
        Self::from_data(dict, StreamData::Raw(data))
    }

    pub fn new_lazy(dict: PdfDictionary, reference: StreamReference) -> Self {
        Self::from_data(dict, StreamData::Lazy(reference))
    }

    pub fn from_data(dict: PdfDictionary, data: StreamData) -> Self {
        let original_bytes = match &data {
            StreamData::Raw(bytes) => Some(bytes.clone()),
            _ => None,
        };
        let observed_length = match &data {
            StreamData::Lazy(reference) => reference.length,
            _ => data.len(),
        };
        let declared_length = match dict.get("Length") {
            Some(PdfValue::Integer(length)) if *length >= 0 => u64::try_from(*length).ok(),
            _ => None,
        };
        PdfStream {
            dict,
            data,
            lossless: StreamLosslessState {
                original_bytes,
                declared_length,
                observed_length,
                ..StreamLosslessState::default()
            },
        }
    }

    pub fn original_data(&self) -> Option<&[u8]> {
        self.lossless.original_bytes.as_deref()
    }

    pub fn decode_state(&self) -> &'static str {
        match self.data {
            StreamData::Raw(_) => "raw",
            StreamData::Decoded(_) => "decoded",
            StreamData::Lazy(_) => "lazy",
        }
    }

    pub fn set_decoded(&mut self, data: Vec<u8>) {
        if self.lossless.original_bytes.is_none() {
            self.lossless.original_bytes = self.raw_data().map(ToOwned::to_owned);
        }
        self.data = StreamData::Decoded(data);
    }

    pub fn record_parse_error(&mut self, message: impl Into<String>) {
        self.lossless.parse_errors.push(message.into());
    }

    pub fn record_recovery(&mut self, action: impl Into<String>) {
        self.lossless.recovery_actions.push(action.into());
    }

    pub fn raw_data(&self) -> Option<&[u8]> {
        match &self.data {
            StreamData::Raw(data) => Some(data),
            _ => None,
        }
    }

    pub fn decode(&self) -> Result<Vec<u8>, String> {
        self.decode_with_budget(&ResourceBudget::default())
    }

    pub fn decode_with_budget(&self, budget: &ResourceBudget) -> Result<Vec<u8>, String> {
        match &self.data {
            StreamData::Raw(data) => {
                let filters = self.get_filters_with_params_checked()?;
                crate::filters::decode_stream_with_budget(data, &filters, budget)
                    .map_err(|e| e.to_string())
            }
            StreamData::Decoded(data) => {
                budget
                    .consume_memory(data.len() as u64)
                    .map_err(|e| e.to_string())?;
                Ok(data.clone())
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
            StreamData::Raw(data) => {
                let filters = self.get_filters_with_params_checked()?;
                crate::filters::decode_stream_with_limits(
                    data,
                    &filters,
                    max_output_bytes,
                    max_ratio,
                )
                .map_err(|e| e.to_string())
            }
            StreamData::Decoded(data) => {
                if data.len() > max_output_bytes {
                    return Err(format!(
                        "Decoded stream exceeds output limit: {} > {}",
                        data.len(),
                        max_output_bytes
                    ));
                }
                Ok(data.clone())
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
        self.get_filters_with_params_checked().unwrap_or_default()
    }

    pub fn get_filters_with_params_checked(&self) -> Result<Vec<StreamFilter>, String> {
        let mut filters = Vec::new();

        let filter_names: Vec<&PdfName> = match self.dict.get("Filter") {
            Some(PdfValue::Name(name)) => vec![name],
            Some(PdfValue::Array(array)) => array
                .iter()
                .map(|value| {
                    value
                        .as_name()
                        .ok_or_else(|| "Filter array contains a non-name value".to_string())
                })
                .collect::<Result<_, _>>()?,
            None => Vec::new(),
            Some(_) => return Err("Filter must be a name or an array of names".to_string()),
        };

        if filter_names.is_empty() {
            return Ok(filters);
        }

        let mut decode_params = match self.dict.get("DecodeParms") {
            Some(PdfValue::Dictionary(dict)) => vec![Some(dict)],
            Some(PdfValue::Array(array)) => array
                .iter()
                .map(|value| match value {
                    PdfValue::Dictionary(dict) => Ok(Some(dict)),
                    PdfValue::Null => Ok(None),
                    _ => Err("DecodeParms array contains a non-dictionary value".to_string()),
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(PdfValue::Null) => vec![None],
            None => Vec::new(),
            Some(_) => return Err("DecodeParms must be a dictionary or an array".to_string()),
        };

        if decode_params.len() < filter_names.len() {
            decode_params.resize(filter_names.len(), None);
        }

        for (i, name) in filter_names.iter().enumerate() {
            let params = decode_params.get(i).copied().unwrap_or(None);
            let filter = Self::filter_from_name_with_params(name, params)
                .ok_or_else(|| format!("Unsupported stream filter: {}", name.without_slash()))?;
            filters.push(filter);
        }

        Ok(filters)
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
            "JBIG2Decode" => Some(StreamFilter::JBIG2Decode(
                params.map(parse_jbig2_params).unwrap_or_default(),
            )),
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

fn parse_jbig2_params(params: &PdfDictionary) -> JBIG2DecodeParams {
    JBIG2DecodeParams {
        globals: params
            .get("JBIG2Globals")
            .and_then(PdfValue::as_stream)
            .and_then(|stream| stream.data.as_bytes())
            .map(ToOwned::to_owned),
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
            "JBIG2Decode" => Some(StreamFilter::JBIG2Decode(JBIG2DecodeParams::default())),
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
            StreamFilter::JBIG2Decode(_) => "JBIG2Decode",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lazy_stream_access_is_optional() {
        let stream = StreamData::Lazy(StreamReference {
            offset: 10,
            length: 1,
            filters: Vec::new(),
        });
        assert!(stream.get(0).is_none());
        assert_eq!(StreamData::Raw(vec![7]).get(0), Some(&7));
        assert!(StreamData::Raw(vec![7]).get(1).is_none());
    }

    #[test]
    fn decoded_stream_retains_lossless_state() {
        let mut dict = PdfDictionary::new();
        dict.insert("Length", PdfValue::Integer(4));
        let mut stream = PdfStream::new(dict, b"raw!".to_vec());
        stream.set_decoded(b"decoded".to_vec());
        stream.record_parse_error("malformed operator");
        stream.record_recovery("content_stream_skipped");

        assert_eq!(stream.original_data(), Some(b"raw!".as_slice()));
        assert_eq!(stream.lossless.declared_length, Some(4));
        assert_eq!(stream.lossless.observed_length, 4);
        assert_eq!(stream.decode_state(), "decoded");
        assert_eq!(stream.lossless.parse_errors, vec!["malformed operator"]);
        assert_eq!(
            stream.lossless.recovery_actions,
            vec!["content_stream_skipped"]
        );
    }

    #[test]
    fn already_decoded_stream_uses_total_memory_budget() {
        let stream = PdfStream::from_data(
            PdfDictionary::new(),
            StreamData::Decoded(b"decoded".to_vec()),
        );
        let budget = ResourceBudget::new(1, 7, 1, 10, 10, 10, 10, 8);

        assert_eq!(stream.decode_with_budget(&budget).unwrap(), b"decoded");
    }

    #[test]
    fn lazy_stream_records_known_observed_length() {
        let stream = PdfStream::new_lazy(
            PdfDictionary::new(),
            StreamReference {
                offset: 12,
                length: 7,
                filters: Vec::new(),
            },
        );

        assert_eq!(stream.lossless.observed_length, 7);
        assert_eq!(stream.decode_state(), "lazy");
    }

    #[test]
    fn unknown_filter_is_not_treated_as_unfiltered_data() {
        let mut dict = PdfDictionary::new();
        dict.insert("Filter", PdfValue::Name(PdfName::new("FutureDecode")));
        let stream = PdfStream::new(dict, b"raw".to_vec());

        let error = stream
            .decode_with_budget(&ResourceBudget::default())
            .expect_err("unknown filters must not be silently discarded");
        assert!(error.contains("Unsupported stream filter"));
        assert!(stream.get_filters_with_params_checked().is_err());
    }

    #[test]
    fn malformed_decode_params_are_not_replaced_with_defaults() {
        let mut dict = PdfDictionary::new();
        dict.insert("Filter", PdfValue::Name(PdfName::new("FlateDecode")));
        dict.insert("DecodeParms", PdfValue::Boolean(true));
        let stream = PdfStream::new(dict, Vec::new());

        let error = stream
            .get_filters_with_params_checked()
            .expect_err("malformed decode parameters must be rejected");
        assert!(error.contains("DecodeParms"));
    }

    #[test]
    fn decoded_stream_is_not_filtered_again() {
        let mut dict = PdfDictionary::new();
        dict.insert("Filter", PdfValue::Name(PdfName::new("ASCIIHexDecode")));
        let mut stream = PdfStream::new(dict, b"3631>".to_vec());
        stream.set_decoded(b"61".to_vec());

        assert_eq!(stream.decode().expect("decoded bytes"), b"61");
        assert_eq!(
            stream
                .decode_with_limits(2, 1)
                .expect("decoded bytes within limit"),
            b"61"
        );
        assert!(stream.decode_with_limits(1, 1).is_err());
    }
}
