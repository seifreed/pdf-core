use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PdfName(String);

impl PdfName {
    pub fn new<S: Into<String>>(name: S) -> Self {
        let mut name = name.into();
        if !name.starts_with('/') {
            name = format!("/{}", name);
        }
        PdfName(name)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn without_slash(&self) -> &str {
        self.0.strip_prefix('/').unwrap_or(&self.0)
    }

    pub fn without_slash_string(&self) -> String {
        self.without_slash().to_string()
    }
}

impl fmt::Display for PdfName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for PdfName {
    fn from(s: &str) -> Self {
        PdfName::new(s)
    }
}

impl From<String> for PdfName {
    fn from(s: String) -> Self {
        PdfName::new(s)
    }
}

impl PartialEq<str> for PdfName {
    fn eq(&self, other: &str) -> bool {
        self.without_slash() == other || self.as_str() == other
    }
}

impl PartialEq<&str> for PdfName {
    fn eq(&self, other: &&str) -> bool {
        self.without_slash() == *other || self.as_str() == *other
    }
}

impl PartialEq<String> for PdfName {
    fn eq(&self, other: &String) -> bool {
        self.without_slash() == other || self.as_str() == other
    }
}

// Remove the unsafe Borrow implementation
// Borrow trait issues should be handled at the call sites

// Better approach - implement AsRef for PdfName to String conversion
impl AsRef<str> for PdfName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfString {
    Literal(Vec<u8>),
    Hexadecimal(Vec<u8>),
}

impl PdfString {
    pub fn new_literal<B: Into<Vec<u8>>>(bytes: B) -> Self {
        PdfString::Literal(bytes.into())
    }

    pub fn new_hex<B: Into<Vec<u8>>>(bytes: B) -> Self {
        PdfString::Hexadecimal(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            PdfString::Literal(b) | PdfString::Hexadecimal(b) => b,
        }
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(self.as_bytes()).into_owned()
    }

    pub fn decode_pdf_encoding(&self) -> String {
        let bytes = self.as_bytes();

        if bytes.starts_with(&[0xFE, 0xFF]) {
            String::from_utf16_be(&bytes[2..])
                .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned())
        } else if bytes.starts_with(&[0xFF, 0xFE]) {
            String::from_utf16_le(&bytes[2..])
                .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned())
        } else {
            String::from_utf8_lossy(bytes).into_owned()
        }
    }

    pub fn hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.as_bytes().hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

impl fmt::Display for PdfString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PdfString::Literal(bytes) => {
                write!(f, "({})", String::from_utf8_lossy(bytes))
            }
            PdfString::Hexadecimal(bytes) => {
                write!(f, "<")?;
                for byte in bytes {
                    write!(f, "{:02X}", byte)?;
                }
                write!(f, ">")
            }
        }
    }
}

impl From<&str> for PdfString {
    fn from(s: &str) -> Self {
        PdfString::new_literal(s.as_bytes())
    }
}

impl From<String> for PdfString {
    fn from(s: String) -> Self {
        PdfString::new_literal(s.into_bytes())
    }
}

impl From<Vec<u8>> for PdfString {
    fn from(bytes: Vec<u8>) -> Self {
        PdfString::new_literal(bytes)
    }
}

trait Utf16Be {
    fn from_utf16_be(v: &[u8]) -> Result<String, std::string::FromUtf16Error>;
}

trait Utf16Le {
    fn from_utf16_le(v: &[u8]) -> Result<String, std::string::FromUtf16Error>;
}

impl Utf16Be for String {
    fn from_utf16_be(v: &[u8]) -> Result<String, std::string::FromUtf16Error> {
        let mut u16_vec = Vec::with_capacity(v.len() / 2);
        for chunk in v.as_chunks::<2>().0 {
            u16_vec.push(u16::from_be_bytes(*chunk));
        }
        String::from_utf16(&u16_vec)
    }
}

impl Utf16Le for String {
    fn from_utf16_le(v: &[u8]) -> Result<String, std::string::FromUtf16Error> {
        let mut u16_vec = Vec::with_capacity(v.len() / 2);
        for chunk in v.as_chunks::<2>().0 {
            u16_vec.push(u16::from_le_bytes(*chunk));
        }
        String::from_utf16(&u16_vec)
    }
}
