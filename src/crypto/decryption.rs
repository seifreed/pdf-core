use crate::crypto::encryption::{CryptFilter, SecurityHandler, StandardSecurityHandler};
use crate::types::primitive::PdfString;
use crate::types::{ObjectId, PdfDictionary, PdfStream, PdfValue};
use std::collections::HashMap;

/// Integrated decryption pipeline for PDF objects
pub struct DecryptionPipeline {
    handler: Option<Box<dyn SecurityHandler>>,
    file_id: Vec<u8>,
    encrypt_dict: PdfDictionary,
    decrypted_cache: HashMap<ObjectId, bool>,
}

impl Default for DecryptionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl DecryptionPipeline {
    pub fn new() -> Self {
        DecryptionPipeline {
            handler: None,
            file_id: Vec::new(),
            encrypt_dict: PdfDictionary::new(),
            decrypted_cache: HashMap::new(),
        }
    }

    /// Initialize from document trailer
    pub fn initialize_from_trailer(&mut self, trailer: &PdfDictionary) -> Result<(), String> {
        // Check if document is encrypted
        let encrypt_dict = match trailer.get("Encrypt") {
            Some(PdfValue::Dictionary(d)) => d.clone(),
            Some(PdfValue::Reference(_)) => {
                // Would need to resolve reference
                return Err("Encrypt dictionary reference not resolved".to_string());
            }
            None => {
                // Document not encrypted
                return Ok(());
            }
            _ => return Err("Invalid Encrypt entry".to_string()),
        };

        // Get file ID
        let file_id = match trailer.get("ID") {
            Some(PdfValue::Array(arr)) if !arr.is_empty() => match arr.get(0) {
                Some(PdfValue::String(s)) => s.as_bytes().to_vec(),
                _ => return Err("Invalid file ID".to_string()),
            },
            _ => {
                // Generate default file ID
                vec![0; 16]
            }
        };

        self.file_id = file_id;
        self.encrypt_dict = encrypt_dict.clone();

        // Determine encryption type
        let filter = encrypt_dict
            .get("Filter")
            .and_then(|v| match v {
                PdfValue::Name(n) => Some(n.without_slash()),
                _ => None,
            })
            .unwrap_or("Standard");

        match filter {
            "Standard" => {
                self.handler = Some(Box::new(self.create_standard_handler(&encrypt_dict)?));
            }
            _ => {
                return Err(format!("Unsupported security handler: {}", filter));
            }
        }

        Ok(())
    }

    fn create_standard_handler(
        &self,
        encrypt_dict: &PdfDictionary,
    ) -> Result<StandardSecurityHandler, String> {
        let v = encrypt_dict
            .get("V")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u32;

        let r = encrypt_dict
            .get("R")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u32;

        let p = encrypt_dict
            .get("P")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as i32;

        let o = encrypt_dict
            .get("O")
            .and_then(|v| match v {
                PdfValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or("Missing O entry")?;

        let u = encrypt_dict
            .get("U")
            .and_then(|v| match v {
                PdfValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or("Missing U entry")?;

        let length = encrypt_dict
            .get("Length")
            .and_then(|v| v.as_integer())
            .unwrap_or(40) as u32;

        let mut handler = StandardSecurityHandler::new_with_params(
            v,
            r,
            length,
            p,
            o.as_bytes().to_vec(),
            u.as_bytes().to_vec(),
        );
        handler.set_file_id(self.file_id.clone());

        // Handle V4/V5 specific fields
        if v >= 4 {
            // StmF - Stream filter
            if let Some(PdfValue::Name(stmf)) = encrypt_dict.get("StmF") {
                handler.stream_filter = stmf.without_slash().to_string();
            }

            // StrF - String filter
            if let Some(PdfValue::Name(strf)) = encrypt_dict.get("StrF") {
                handler.string_filter = strf.without_slash().to_string();
            }

            // CF - Crypt filters
            if let Some(PdfValue::Dictionary(cf)) = encrypt_dict.get("CF") {
                handler.crypt_filters = self.parse_crypt_filters(cf);
            }
        }

        if v == 5 {
            // OE - Owner encryption key
            if let Some(PdfValue::String(oe)) = encrypt_dict.get("OE") {
                handler.oe = Some(oe.as_bytes().to_vec());
            }

            // UE - User encryption key
            if let Some(PdfValue::String(ue)) = encrypt_dict.get("UE") {
                handler.ue = Some(ue.as_bytes().to_vec());
            }

            // Perms - Permissions
            if let Some(PdfValue::String(perms)) = encrypt_dict.get("Perms") {
                handler.perms = Some(perms.as_bytes().to_vec());
            }
        }

        // EncryptMetadata
        handler.encrypt_metadata = encrypt_dict
            .get("EncryptMetadata")
            .and_then(|v| v.as_boolean())
            .unwrap_or(true);

        Ok(handler)
    }

    fn parse_crypt_filters(&self, cf_dict: &PdfDictionary) -> HashMap<String, CryptFilter> {
        let mut filters = HashMap::new();

        for (name, value) in cf_dict.iter() {
            if let PdfValue::Dictionary(filter_dict) = value {
                let filter = CryptFilter {
                    cfm: filter_dict
                        .get("CFM")
                        .and_then(|v| match v {
                            PdfValue::Name(n) => Some(n.without_slash().to_string()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "None".to_string()),

                    auth_event: filter_dict
                        .get("AuthEvent")
                        .and_then(|v| match v {
                            PdfValue::Name(n) => Some(n.without_slash().to_string()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "DocOpen".to_string()),

                    length: filter_dict
                        .get("Length")
                        .and_then(|v| v.as_integer())
                        .unwrap_or(128) as u32,
                };

                filters.insert(name.without_slash().to_string(), filter);
            }
        }

        filters
    }

    /// Authenticate with password
    pub fn authenticate(&mut self, password: &str) -> Result<bool, String> {
        if let Some(handler) = &mut self.handler {
            Ok(handler.authenticate(password))
        } else {
            Ok(true) // Not encrypted
        }
    }

    /// Decrypt a PDF object
    pub fn decrypt_object(
        &mut self,
        obj_id: &ObjectId,
        value: &mut PdfValue,
    ) -> Result<(), String> {
        // Check if already decrypted
        if self.decrypted_cache.get(obj_id).copied().unwrap_or(false) {
            return Ok(());
        }

        if let Some(handler) = &self.handler {
            self.decrypt_value(handler.as_ref(), obj_id, value)?;
            self.decrypted_cache.insert(*obj_id, true);
        }

        Ok(())
    }

    fn decrypt_value(
        &self,
        handler: &dyn SecurityHandler,
        obj_id: &ObjectId,
        value: &mut PdfValue,
    ) -> Result<(), String> {
        match value {
            PdfValue::String(s) => {
                // Decrypt string
                let key = handler.compute_object_key(obj_id.number, &self.file_id);
                let decrypted = handler
                    .decrypt_string(&s.to_string(), &key)
                    .map_err(|e| format!("Decryption error: {:?}", e))?;
                *s = PdfString::new_literal(decrypted.into_bytes());
            }
            PdfValue::Stream(stream) => {
                // Decrypt stream
                self.decrypt_stream(handler, obj_id, stream)?;
            }
            PdfValue::Array(arr) => {
                // Recursively decrypt array elements
                for elem in arr.iter_mut() {
                    self.decrypt_value(handler, obj_id, elem)?;
                }
            }
            PdfValue::Dictionary(dict) => {
                // Recursively decrypt dictionary values
                let mut decrypted = PdfDictionary::new();
                for (key, val) in dict.iter() {
                    let mut val_copy = val.clone();
                    self.decrypt_value(handler, obj_id, &mut val_copy)?;
                    decrypted.insert(key.clone(), val_copy);
                }
                *dict = decrypted;
            }
            _ => {
                // Other types are not encrypted
            }
        }

        Ok(())
    }

    fn decrypt_stream(
        &self,
        handler: &dyn SecurityHandler,
        obj_id: &ObjectId,
        stream: &mut PdfStream,
    ) -> Result<(), String> {
        // Check if stream should be decrypted
        if self.should_decrypt_stream(&stream.dict) {
            let key = handler.compute_object_key(obj_id.number, &self.file_id);
            if let Some(bytes) = stream.data.as_bytes() {
                let decrypted = handler
                    .decrypt_stream(bytes, &key)
                    .map_err(|e| format!("Stream decryption error: {:?}", e))?;
                stream.set_decoded(decrypted);
            }

            // Remove crypt filter from stream dictionary
            stream.dict.remove("Filter");
            stream.dict.remove("DecodeParms");
        }

        Ok(())
    }

    fn should_decrypt_stream(&self, dict: &PdfDictionary) -> bool {
        // Check for Identity crypt filter
        if let Some(PdfValue::Name(filter)) = dict.get("Filter") {
            if filter.without_slash() == "Crypt" {
                if let Some(PdfValue::Dictionary(decode_params)) = dict.get("DecodeParms") {
                    if let Some(PdfValue::Name(name)) = decode_params.get("Name") {
                        return name.without_slash() != "Identity";
                    }
                }
            }
        }

        true
    }

    /// Get decryption status
    pub fn is_encrypted(&self) -> bool {
        self.handler.is_some()
    }

    pub fn is_authenticated(&self) -> bool {
        self.handler
            .as_ref()
            .map(|h| h.is_authenticated())
            .unwrap_or(true)
    }

    pub fn get_permissions(&self) -> i32 {
        self.handler
            .as_ref()
            .map(|h| h.get_permissions() as i32)
            .unwrap_or(-1)
    }

    /// Decrypt all objects in a collection
    pub fn decrypt_objects(
        &mut self,
        objects: &mut HashMap<ObjectId, PdfValue>,
    ) -> Result<(), String> {
        for (obj_id, value) in objects.iter_mut() {
            self.decrypt_object(obj_id, value)?;
        }
        Ok(())
    }
}

// CryptFilter is now defined in encryption.rs

// StandardSecurityHandler implementation moved to encryption.rs to avoid conflicts

/// Integration with PDF parser pipeline
#[allow(dead_code)]
pub struct DecryptingReader<R> {
    inner: R,
    pipeline: DecryptionPipeline,
}

impl<R> DecryptingReader<R> {
    pub fn new(reader: R, trailer: &PdfDictionary) -> Result<Self, String> {
        let mut pipeline = DecryptionPipeline::new();
        pipeline.initialize_from_trailer(trailer)?;

        Ok(DecryptingReader {
            inner: reader,
            pipeline,
        })
    }

    pub fn authenticate(&mut self, password: &str) -> Result<bool, String> {
        self.pipeline.authenticate(password)
    }

    pub fn decrypt_value(&mut self, obj_id: &ObjectId, value: &mut PdfValue) -> Result<(), String> {
        self.pipeline.decrypt_object(obj_id, value)
    }

    pub fn is_encrypted(&self) -> bool {
        self.pipeline.is_encrypted()
    }
}
