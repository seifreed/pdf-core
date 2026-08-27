use crate::performance::ResourceBudget;
use crate::types::{ObjectId, PdfDictionary, PdfStream};
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

/// Lazy stream that loads data on-demand
#[derive(Debug, Clone)]
pub struct LazyStream {
    dict: PdfDictionary,
    loader: StreamLoader,
    cache: Arc<Mutex<Option<Vec<u8>>>>,
}

#[derive(Debug, Clone)]
pub enum StreamLoader {
    /// Stream data stored inline
    Inline(Vec<u8>),

    /// Stream data to be loaded from file
    File {
        offset: u64,
        length: usize,
        file_handle: Arc<Mutex<Box<dyn StreamSource>>>,
    },

    /// Stream data from object stream
    ObjectStream {
        stream_obj: ObjectId,
        index: u32,
        parent_loader: Box<StreamLoader>,
    },
}

pub trait StreamSource: Send + Sync {
    fn read_at(&mut self, offset: u64, length: usize) -> std::io::Result<Vec<u8>>;
    fn clone_source(&self) -> Box<dyn StreamSource>;
}

impl std::fmt::Debug for dyn StreamSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StreamSource")
    }
}

impl LazyStream {
    pub fn new_inline(dict: PdfDictionary, data: Vec<u8>) -> Self {
        LazyStream {
            dict,
            loader: StreamLoader::Inline(data),
            cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new_file(
        dict: PdfDictionary,
        offset: u64,
        length: usize,
        source: Box<dyn StreamSource>,
    ) -> Self {
        LazyStream {
            dict,
            loader: StreamLoader::File {
                offset,
                length,
                file_handle: Arc::new(Mutex::new(source)),
            },
            cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new_object_stream(
        dict: PdfDictionary,
        stream_obj: ObjectId,
        index: u32,
        parent: StreamLoader,
    ) -> Self {
        LazyStream {
            dict,
            loader: StreamLoader::ObjectStream {
                stream_obj,
                index,
                parent_loader: Box::new(parent),
            },
            cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Load stream data on-demand
    pub fn load(&self) -> Result<Vec<u8>, String> {
        self.load_with_budget(&ResourceBudget::default())
    }

    pub fn load_with_budget(&self, budget: &ResourceBudget) -> Result<Vec<u8>, String> {
        budget.check().map_err(|error| error.to_string())?;
        // Check cache first
        if let Ok(cache) = self.cache.lock() {
            if let Some(ref data) = *cache {
                budget
                    .consume_decoded(data.len() as u64)
                    .map_err(|error| error.to_string())?;
                return Ok(data.clone());
            }
        }

        // Load data based on loader type
        let data = match &self.loader {
            StreamLoader::Inline(data) => {
                budget
                    .consume_decoded(data.len() as u64)
                    .map_err(|error| error.to_string())?;
                data.clone()
            }

            StreamLoader::File {
                offset,
                length,
                file_handle,
            } => {
                budget
                    .consume_input(*length as u64)
                    .map_err(|error| error.to_string())?;
                budget
                    .consume_decoded(*length as u64)
                    .map_err(|error| error.to_string())?;
                let mut handle = file_handle
                    .lock()
                    .map_err(|e| format!("Failed to lock file handle: {}", e))?;

                handle
                    .read_at(*offset, *length)
                    .map_err(|e| format!("Failed to read stream data: {}", e))?
            }

            StreamLoader::ObjectStream {
                stream_obj: _,
                index,
                parent_loader,
            } => {
                // Load parent stream first
                let parent_data = self.load_parent_stream_with_budget(parent_loader, budget)?;

                // Parse object stream to extract specific object
                let data = self.extract_from_object_stream(&parent_data, *index)?;
                budget
                    .consume_decoded(data.len() as u64)
                    .map_err(|error| error.to_string())?;
                data
            }
        };

        // Cache the loaded data
        if let Ok(mut cache) = self.cache.lock() {
            budget
                .consume_decoded(data.len() as u64)
                .map_err(|error| error.to_string())?;
            *cache = Some(data.clone());
        }

        Ok(data)
    }

    fn load_parent_stream_with_budget(
        &self,
        parent: &StreamLoader,
        budget: &ResourceBudget,
    ) -> Result<Vec<u8>, String> {
        match parent {
            StreamLoader::Inline(data) => {
                budget
                    .consume_decoded(data.len() as u64)
                    .map_err(|error| error.to_string())?;
                Ok(data.clone())
            }

            StreamLoader::File {
                offset,
                length,
                file_handle,
            } => {
                budget
                    .consume_input(*length as u64)
                    .map_err(|error| error.to_string())?;
                budget
                    .consume_decoded(*length as u64)
                    .map_err(|error| error.to_string())?;
                let mut handle = file_handle
                    .lock()
                    .map_err(|e| format!("Failed to lock parent file handle: {}", e))?;

                handle
                    .read_at(*offset, *length)
                    .map_err(|e| format!("Failed to read parent stream: {}", e))
            }

            StreamLoader::ObjectStream { .. } => {
                Err("Nested object streams not supported".to_string())
            }
        }
    }

    fn extract_from_object_stream(&self, data: &[u8], index: u32) -> Result<Vec<u8>, String> {
        // Parse object stream format
        // First parse the offset table
        let n = self
            .dict
            .get("N")
            .and_then(|v| v.as_integer())
            .ok_or("Missing N in object stream")
            .and_then(|value| usize::try_from(value).map_err(|_| "N must be non-negative"))?;

        let first = self
            .dict
            .get("First")
            .and_then(|v| v.as_integer())
            .ok_or("Missing First in object stream")
            .and_then(|value| usize::try_from(value).map_err(|_| "First must be non-negative"))?;

        let index = usize::try_from(index).map_err(|_| "Object stream index overflow")?;
        if index >= n {
            return Err(format!(
                "Index {} out of range for object stream with {} objects",
                index, n
            ));
        }

        // Parse offset table (simplified - would need proper parsing)
        let _offset_entry_size = 16; // Approximate size of "objnum offset" entry
        let offset_table_end = first;

        if offset_table_end > data.len() {
            return Err("Invalid First offset in object stream".to_string());
        }

        // Find object offset in stream
        let offset_table = data
            .get(..offset_table_end)
            .ok_or("Invalid First offset in object stream")?;
        let entries: Vec<&str> = std::str::from_utf8(offset_table)
            .map_err(|e| format!("Invalid offset table: {}", e))?
            .split_whitespace()
            .collect();

        let entry_index = index
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or("Object stream entry index overflow")?;
        let obj_offset = entries
            .get(entry_index)
            .ok_or("Insufficient entries in offset table")?
            .parse::<usize>()
            .map_err(|e| format!("Invalid offset: {}", e))?;

        let absolute_offset = first
            .checked_add(obj_offset)
            .ok_or("Object stream offset overflow")?;

        // Find next object offset to determine length
        let next_offset = if let Some(next_index) = index.checked_add(1).filter(|next| *next < n) {
            let next_entry_index = next_index
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .ok_or("Object stream entry index overflow")?;
            let next_obj_offset = entries
                .get(next_entry_index)
                .ok_or("Insufficient entries in offset table")?
                .parse::<usize>()
                .map_err(|e| format!("Invalid next offset: {}", e))?;
            first
                .checked_add(next_obj_offset)
                .ok_or("Object stream offset overflow")?
        } else {
            data.len()
        };

        if absolute_offset >= data.len()
            || next_offset > data.len()
            || next_offset < absolute_offset
        {
            return Err("Object offset out of bounds".to_string());
        }

        Ok(data[absolute_offset..next_offset].to_vec())
    }

    /// Get dictionary without loading stream data
    pub fn get_dict(&self) -> &PdfDictionary {
        &self.dict
    }

    /// Check if stream data is currently cached
    pub fn is_cached(&self) -> bool {
        self.cache
            .lock()
            .map(|cache| cache.is_some())
            .unwrap_or(false)
    }

    /// Clear cached data to free memory
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            *cache = None;
        }
    }

    /// Get estimated memory usage
    pub fn memory_usage(&self) -> usize {
        let dict_size = std::mem::size_of_val(&self.dict) + self.dict.len() * 50; // Rough estimate

        let cached_size = self
            .cache
            .lock()
            .ok()
            .and_then(|cache| cache.as_ref().map(|data| data.len()))
            .unwrap_or(0);

        dict_size + cached_size
    }

    /// Convert to regular PdfStream by loading data
    pub fn to_stream(&self) -> Result<PdfStream, String> {
        self.to_stream_with_budget(&ResourceBudget::default())
    }

    pub fn to_stream_with_budget(&self, budget: &ResourceBudget) -> Result<PdfStream, String> {
        let data = self.load_with_budget(budget)?;
        Ok(PdfStream::from_data(
            self.dict.clone(),
            crate::types::stream::StreamData::Decoded(data),
        ))
    }
}

/// File-based stream source implementation
pub struct FileStreamSource<R: Read + Seek> {
    reader: Arc<Mutex<R>>,
}

impl<R: Read + Seek + Send + 'static> FileStreamSource<R> {
    pub fn new(reader: R) -> Self {
        FileStreamSource {
            reader: Arc::new(Mutex::new(reader)),
        }
    }
}

impl<R: Read + Seek + Send + 'static> StreamSource for FileStreamSource<R> {
    fn read_at(&mut self, offset: u64, length: usize) -> std::io::Result<Vec<u8>> {
        let mut reader = self
            .reader
            .lock()
            .map_err(|_| std::io::Error::other("Failed to acquire lock"))?;
        reader.seek(SeekFrom::Start(offset))?;

        let mut buffer = vec![0u8; length];
        reader.read_exact(&mut buffer)?;

        Ok(buffer)
    }

    fn clone_source(&self) -> Box<dyn StreamSource> {
        Box::new(FileStreamSource {
            reader: self.reader.clone(),
        })
    }
}

/// Memory-based stream source for testing
pub struct MemoryStreamSource {
    data: Vec<u8>,
}

impl MemoryStreamSource {
    pub fn new(data: Vec<u8>) -> Self {
        MemoryStreamSource { data }
    }
}

impl StreamSource for MemoryStreamSource {
    fn read_at(&mut self, offset: u64, length: usize) -> std::io::Result<Vec<u8>> {
        let offset = usize::try_from(offset).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "Offset exceeds usize")
        })?;
        let end = offset.checked_add(length).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "Read range overflows")
        })?;
        if end > self.data.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Read beyond end of stream",
            ));
        }

        Ok(self.data[offset..end].to_vec())
    }

    fn clone_source(&self) -> Box<dyn StreamSource> {
        Box::new(MemoryStreamSource {
            data: self.data.clone(),
        })
    }
}

/// Stream cache manager for memory management
pub struct StreamCacheManager {
    max_memory: usize,
    current_usage: Arc<Mutex<usize>>,
    streams: Arc<Mutex<Vec<Arc<LazyStream>>>>,
}

impl StreamCacheManager {
    pub fn new(max_memory: usize) -> Self {
        StreamCacheManager {
            max_memory,
            current_usage: Arc::new(Mutex::new(0)),
            streams: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn register_stream(&self, stream: Arc<LazyStream>) {
        if let Ok(mut streams) = self.streams.lock() {
            streams.push(stream);
        }
    }

    pub fn update_usage(&self, delta: isize) {
        if let Ok(mut usage) = self.current_usage.lock() {
            if delta > 0 {
                *usage = usage.saturating_add(delta as usize);
            } else {
                *usage = usage.saturating_sub((-delta) as usize);
            }

            // Trigger cleanup if over limit
            if *usage > self.max_memory {
                self.cleanup_caches(*usage - self.max_memory);
            }
        }
    }

    fn cleanup_caches(&self, bytes_needed: usize) {
        if let Ok(streams) = self.streams.lock() {
            let mut freed = 0;

            for stream in streams.iter() {
                if freed >= bytes_needed {
                    break;
                }

                if stream.is_cached() {
                    let usage = stream.memory_usage();
                    stream.clear_cache();
                    freed += usage;
                }
            }
        }
    }

    pub fn clear_all_caches(&self) {
        if let Ok(streams) = self.streams.lock() {
            for stream in streams.iter() {
                stream.clear_cache();
            }
        }

        if let Ok(mut usage) = self.current_usage.lock() {
            *usage = 0;
        }
    }

    pub fn get_current_usage(&self) -> usize {
        self.current_usage.lock().map(|usage| *usage).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::{LazyStream, MemoryStreamSource, StreamLoader, StreamSource};
    use crate::performance::ResourceBudget;
    use crate::types::{ObjectId, PdfDictionary, PdfValue};

    #[test]
    fn lazy_load_rejects_inline_data_before_cloning() {
        let stream = LazyStream::new_inline(PdfDictionary::new(), vec![1, 2]);
        let budget = ResourceBudget::new(1024, 1, 1, 100, 10, 10, 10, 10);
        assert!(stream
            .load_with_budget(&budget)
            .expect_err("lazy data must respect the decoded budget")
            .contains("DecodedBytes"));
    }

    #[test]
    fn rejects_negative_object_stream_counts() {
        let mut dict = PdfDictionary::new();
        dict.insert("N", PdfValue::Integer(-1));
        dict.insert("First", PdfValue::Integer(0));
        let stream = LazyStream::new_object_stream(
            dict,
            ObjectId::new(1, 0),
            0,
            StreamLoader::Inline(Vec::new()),
        );

        assert!(stream.load().is_err());
    }

    #[test]
    fn rejects_overflowing_memory_reads() {
        let mut source = MemoryStreamSource::new(vec![1, 2, 3]);
        let result = source.read_at(u64::MAX, 1);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_object_stream_first_beyond_data() {
        let mut dict = PdfDictionary::new();
        dict.insert("N", PdfValue::Integer(1));
        dict.insert("First", PdfValue::Integer(10));
        let stream = LazyStream::new_object_stream(
            dict,
            ObjectId::new(1, 0),
            0,
            StreamLoader::Inline(b"1 0".to_vec()),
        );

        assert!(stream.load().is_err());
    }

    #[test]
    fn rejects_object_stream_object_offset_beyond_data() {
        let mut dict = PdfDictionary::new();
        dict.insert("N", PdfValue::Integer(1));
        dict.insert("First", PdfValue::Integer(4));
        let stream = LazyStream::new_object_stream(
            dict,
            ObjectId::new(1, 0),
            0,
            StreamLoader::Inline(b"1 100".to_vec()),
        );

        assert!(stream.load().is_err());
    }
}
