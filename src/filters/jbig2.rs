use super::{FilterError, FilterResult};

/// JBIG2 decoding is intentionally unsupported.
pub struct Jbig2Decoder {
    _config: Jbig2Config,
}

impl Jbig2Decoder {
    pub fn new() -> Self {
        Self::with_config(Jbig2Config::default())
    }

    pub fn with_config(config: Jbig2Config) -> Self {
        Self { _config: config }
    }

    pub fn decode(&mut self, data: &[u8], globals: Option<&[u8]>) -> FilterResult<Vec<u8>> {
        let _ = (data, globals);
        Err(FilterError::UnsupportedFormat(
            "JBIG2 decoding is not supported; inspect the raw stream instead".to_string(),
        ))
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
