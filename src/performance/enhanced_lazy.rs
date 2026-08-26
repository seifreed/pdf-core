use crate::ast::NodeId;
use crate::parser::content_stream::ContentOperator;
use crate::types::{ObjectId, PdfDictionary};
use bytes::Bytes;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Enhanced lazy loading with comprehensive limits
#[derive(Debug, Clone)]
pub struct LazyLimits {
    /// Maximum pages to load at once
    pub max_pages: Option<usize>,

    /// Maximum bytes per operation
    pub max_bytes_per_load: usize,

    /// Maximum operators per content stream
    pub max_operators: Option<usize>,

    /// Maximum images to decode simultaneously
    pub max_concurrent_images: usize,

    /// Maximum depth for nested objects
    pub max_object_depth: usize,

    /// Timeout for individual load operations
    pub load_timeout: Duration,

    /// Memory pressure threshold (0.0-1.0)
    pub memory_pressure_threshold: f64,

    /// Enable aggressive caching
    pub aggressive_caching: bool,

    /// Priority levels for different content types
    pub priority_levels: ContentPriorities,
}

#[derive(Debug, Clone)]
pub struct ContentPriorities {
    pub text: u8,
    pub images: u8,
    pub vector_graphics: u8,
    pub forms: u8,
    pub annotations: u8,
    pub metadata: u8,
}

impl Default for LazyLimits {
    fn default() -> Self {
        Self {
            max_pages: Some(10),
            max_bytes_per_load: 50 * 1024 * 1024, // 50MB
            max_operators: Some(100_000),
            max_concurrent_images: 4,
            max_object_depth: 50,
            load_timeout: Duration::from_secs(30),
            memory_pressure_threshold: 0.8,
            aggressive_caching: false,
            priority_levels: ContentPriorities::default(),
        }
    }
}

impl Default for ContentPriorities {
    fn default() -> Self {
        Self {
            text: 10, // Highest priority
            images: 7,
            vector_graphics: 8,
            forms: 6,
            annotations: 5,
            metadata: 3, // Lowest priority
        }
    }
}

/// Enhanced lazy loader with comprehensive resource management
pub struct EnhancedLazyLoader {
    limits: LazyLimits,

    /// Currently loaded pages
    loaded_pages: Arc<RwLock<HashMap<u32, Arc<LazyPage>>>>,

    /// Load queue with priorities
    load_queue: Arc<Mutex<VecDeque<LoadRequest>>>,

    /// Memory usage tracker
    memory_usage: Arc<RwLock<MemoryUsage>>,

    /// Performance metrics
    metrics: Arc<RwLock<LoaderMetrics>>,

    /// Content stream cache
    stream_cache: Arc<RwLock<HashMap<ObjectId, Arc<LazyContentStream>>>>,

    /// Image cache with LRU eviction
    image_cache: Arc<RwLock<LruCache<ObjectId, Arc<LazyImage>>>>,
}

#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub request_type: LoadRequestType,
    pub priority: u8,
    pub node_id: NodeId,
    pub object_id: Option<ObjectId>,
    pub requested_at: Instant,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum LoadRequestType {
    Page {
        page_number: u32,
    },
    ContentStream {
        stream_id: ObjectId,
        operator_limit: Option<usize>,
    },
    Image {
        image_id: ObjectId,
        decode_params: ImageDecodeParams,
    },
    Font {
        font_id: ObjectId,
        subset_only: bool,
    },
    Annotation {
        annotation_id: ObjectId,
    },
    FormField {
        field_id: ObjectId,
    },
    Metadata {
        metadata_type: String,
    },
}

#[derive(Debug, Clone)]
pub struct ImageDecodeParams {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub decode_quality: f32, // 0.0 to 1.0
    pub color_space_conversion: Option<String>,
}

#[derive(Debug)]
pub struct LazyPage {
    pub page_number: u32,
    pub page_dict: Arc<RwLock<Option<PdfDictionary>>>,
    pub content_streams: Vec<ObjectId>,
    pub resources: Arc<RwLock<Option<PdfDictionary>>>,
    pub annotations: Vec<ObjectId>,
    pub load_state: Arc<RwLock<PageLoadState>>,
    pub load_priority: u8,
}

#[derive(Debug, Clone)]
pub enum PageLoadState {
    NotLoaded,
    Loading { started_at: Instant },
    PartiallyLoaded { components: LoadedComponents },
    FullyLoaded { loaded_at: Instant },
    LoadError { error: String },
}

#[derive(Debug, Clone)]
pub struct LoadedComponents {
    pub basic_info: bool,
    pub content_streams: bool,
    pub resources: bool,
    pub annotations: bool,
    pub images: bool,
    pub fonts: bool,
}

pub struct LazyContentStream {
    pub stream_id: ObjectId,
    pub operators: Arc<RwLock<Option<Vec<ContentOperator>>>>,
    pub operator_count: usize,
    pub text_content: Arc<RwLock<Option<String>>>,
    pub graphics_state_stack_depth: u32,
    pub parsing_state: Arc<RwLock<StreamParsingState>>,
}

#[derive(Debug, Clone)]
pub enum StreamParsingState {
    NotParsed,
    Parsing {
        progress_pct: f32,
    },
    ParsedPartial {
        operators_parsed: usize,
        stopped_reason: String,
    },
    ParsedComplete {
        parsed_at: Instant,
    },
    ParseError {
        error: String,
    },
}

pub struct LazyImage {
    pub image_id: ObjectId,
    pub metadata: ImageMetadata,
    pub raw_data: Arc<RwLock<Option<Bytes>>>,
    pub decoded_data: Arc<RwLock<Option<DecodedImage>>>,
    pub decode_state: Arc<RwLock<ImageDecodeState>>,
}

#[derive(Debug, Clone)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub bits_per_component: u8,
    pub color_space: String,
    pub compression: Option<String>,
    pub size_bytes: usize,
    pub is_mask: bool,
    pub is_inline: bool,
}

pub struct DecodedImage {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub color_space: String,
    pub decoded_at: Instant,
}

#[derive(Debug, Clone)]
pub enum ImageDecodeState {
    NotDecoded,
    Decoding { progress_pct: f32 },
    Decoded { quality: f32 },
    DecodeError { error: String },
    Skipped { reason: String },
}

#[derive(Debug)]
pub struct MemoryUsage {
    pub pages_mb: f64,
    pub content_streams_mb: f64,
    pub images_mb: f64,
    pub fonts_mb: f64,
    pub metadata_mb: f64,
    pub total_mb: f64,
    pub peak_mb: f64,
}

#[derive(Debug, Default)]
pub struct LoaderMetrics {
    pub pages_loaded: u32,
    pub content_streams_parsed: u32,
    pub images_decoded: u32,
    pub cache_hits: u32,
    pub cache_misses: u32,
    pub load_timeouts: u32,
    pub memory_pressure_events: u32,
    pub average_load_time_ms: f64,
    pub total_bytes_loaded: u64,
}

/// LRU Cache implementation for images
pub struct LruCache<K, V> {
    data: HashMap<K, V>,
    order: VecDeque<K>,
    capacity: usize,
}

impl<K: Clone + std::hash::Hash + Eq, V> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: HashMap::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.data.contains_key(key) {
            // Move to front
            self.order.retain(|k| k != key);
            self.order.push_front(key.clone());
            self.data.get(key)
        } else {
            None
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let Some(old_value) = self.data.remove(&key) {
            self.order.retain(|k| k != &key);
            self.order.push_front(key.clone());
            self.data.insert(key, value);
            Some(old_value)
        } else {
            if self.data.len() >= self.capacity {
                // Evict least recently used
                if let Some(evicted_key) = self.order.pop_back() {
                    self.data.remove(&evicted_key);
                }
            }
            self.order.push_front(key.clone());
            self.data.insert(key, value);
            None
        }
    }
}

impl EnhancedLazyLoader {
    pub fn new(limits: LazyLimits) -> Self {
        Self {
            limits: limits.clone(),
            loaded_pages: Arc::new(RwLock::new(HashMap::new())),
            load_queue: Arc::new(Mutex::new(VecDeque::new())),
            memory_usage: Arc::new(RwLock::new(MemoryUsage {
                pages_mb: 0.0,
                content_streams_mb: 0.0,
                images_mb: 0.0,
                fonts_mb: 0.0,
                metadata_mb: 0.0,
                total_mb: 0.0,
                peak_mb: 0.0,
            })),
            metrics: Arc::new(RwLock::new(LoaderMetrics::default())),
            stream_cache: Arc::new(RwLock::new(HashMap::new())),
            image_cache: Arc::new(RwLock::new(LruCache::new(limits.max_concurrent_images * 2))),
        }
    }

    /// Request loading of specific pages with limits
    pub fn request_pages(
        &self,
        page_range: std::ops::Range<u32>,
    ) -> Result<Vec<Arc<LazyPage>>, String> {
        let requested_count = (page_range.end - page_range.start) as usize;

        // Check page limits
        if let Some(max_pages) = self.limits.max_pages {
            if requested_count > max_pages {
                return Err(format!(
                    "Requested {} pages exceeds limit of {}",
                    requested_count, max_pages
                ));
            }
        }

        let mut pages = Vec::new();
        for page_num in page_range {
            let page = self.get_or_create_page(page_num);
            self.queue_page_load(page_num)?;
            pages.push(page);
        }

        Ok(pages)
    }

    /// Request content stream parsing with operator limits
    pub fn request_content_stream(
        &self,
        stream_id: ObjectId,
        operator_limit: Option<usize>,
    ) -> Result<Arc<LazyContentStream>, String> {
        // Check if already cached
        {
            let cache = self.stream_cache.read().unwrap();
            if let Some(stream) = cache.get(&stream_id) {
                return Ok(stream.clone());
            }
        }

        // Create lazy content stream
        let stream = Arc::new(LazyContentStream {
            stream_id,
            operators: Arc::new(RwLock::new(None)),
            operator_count: 0,
            text_content: Arc::new(RwLock::new(None)),
            graphics_state_stack_depth: 0,
            parsing_state: Arc::new(RwLock::new(StreamParsingState::NotParsed)),
        });

        // Add to cache
        {
            let mut cache = self.stream_cache.write().unwrap();
            cache.insert(stream_id, stream.clone());
        }

        // Queue for parsing
        let request = LoadRequest {
            request_type: LoadRequestType::ContentStream {
                stream_id,
                operator_limit: operator_limit.or(self.limits.max_operators),
            },
            priority: self.limits.priority_levels.vector_graphics,
            node_id: NodeId(0), // Would need proper mapping
            object_id: Some(stream_id),
            requested_at: Instant::now(),
            timeout: self.limits.load_timeout,
        };

        self.queue_request(request)?;
        Ok(stream)
    }

    /// Request image loading with decode parameters
    pub fn request_image(
        &self,
        image_id: ObjectId,
        decode_params: ImageDecodeParams,
    ) -> Result<Arc<LazyImage>, String> {
        // Check concurrent image limit
        let current_images = {
            let cache = self.image_cache.read().unwrap();
            cache.data.len()
        };

        if current_images >= self.limits.max_concurrent_images {
            return Err(format!(
                "Maximum concurrent images ({}) exceeded",
                self.limits.max_concurrent_images
            ));
        }

        // Create lazy image
        let image = Arc::new(LazyImage {
            image_id,
            metadata: ImageMetadata {
                width: 0, // Would be populated from image dictionary
                height: 0,
                bits_per_component: 8,
                color_space: "RGB".to_string(),
                compression: None,
                size_bytes: 0,
                is_mask: false,
                is_inline: false,
            },
            raw_data: Arc::new(RwLock::new(None)),
            decoded_data: Arc::new(RwLock::new(None)),
            decode_state: Arc::new(RwLock::new(ImageDecodeState::NotDecoded)),
        });

        // Add to cache
        {
            let mut cache = self.image_cache.write().unwrap();
            cache.insert(image_id, image.clone());
        }

        // Queue for loading
        let request = LoadRequest {
            request_type: LoadRequestType::Image {
                image_id,
                decode_params,
            },
            priority: self.limits.priority_levels.images,
            node_id: NodeId(0),
            object_id: Some(image_id),
            requested_at: Instant::now(),
            timeout: self.limits.load_timeout,
        };

        self.queue_request(request)?;
        Ok(image)
    }

    fn get_or_create_page(&self, page_number: u32) -> Arc<LazyPage> {
        {
            let pages = self.loaded_pages.read().unwrap();
            if let Some(page) = pages.get(&page_number) {
                return page.clone();
            }
        }

        let page = Arc::new(LazyPage {
            page_number,
            page_dict: Arc::new(RwLock::new(None)),
            content_streams: Vec::new(),
            resources: Arc::new(RwLock::new(None)),
            annotations: Vec::new(),
            load_state: Arc::new(RwLock::new(PageLoadState::NotLoaded)),
            load_priority: self.limits.priority_levels.text, // Default to text priority
        });

        {
            let mut pages = self.loaded_pages.write().unwrap();
            pages.insert(page_number, page.clone());
        }

        page
    }

    fn queue_page_load(&self, page_number: u32) -> Result<(), String> {
        let request = LoadRequest {
            request_type: LoadRequestType::Page { page_number },
            priority: self.limits.priority_levels.text,
            node_id: NodeId(0),
            object_id: None,
            requested_at: Instant::now(),
            timeout: self.limits.load_timeout,
        };

        self.queue_request(request)
    }

    fn queue_request(&self, request: LoadRequest) -> Result<(), String> {
        let mut queue = self.load_queue.lock();

        // Find insertion point for priority order
        let mut insert_index = None;
        for (i, existing) in queue.iter().enumerate() {
            if request.priority > existing.priority {
                insert_index = Some(i);
                break;
            }
        }

        // Insert at the appropriate position
        if let Some(index) = insert_index {
            queue.insert(index, request);
        } else {
            queue.push_back(request);
        }

        Ok(())
    }

    /// Check memory pressure and trigger cleanup if needed
    pub fn check_memory_pressure(&self) -> Result<(), String> {
        let usage = self.memory_usage.read().unwrap();
        let pressure = usage.total_mb / (self.limits.max_bytes_per_load as f64 / (1024.0 * 1024.0));

        if pressure > self.limits.memory_pressure_threshold {
            drop(usage);
            self.cleanup_memory()?;

            let mut metrics = self.metrics.write().unwrap();
            metrics.memory_pressure_events += 1;
        }

        Ok(())
    }

    fn cleanup_memory(&self) -> Result<(), String> {
        // Clear least recently used content streams
        {
            let mut cache = self.stream_cache.write().unwrap();
            if cache.len() > 10 {
                // Keep only the 10 most recent
                let keys_to_remove: Vec<_> = cache.keys().take(cache.len() - 10).cloned().collect();
                for key in keys_to_remove {
                    cache.remove(&key);
                }
            }
        }

        // Clear decoded images if memory pressure is high
        {
            let image_cache = self.image_cache.write().unwrap();
            for image in image_cache.data.values() {
                let mut decoded = image.decoded_data.write().unwrap();
                *decoded = None; // Clear decoded data, keep raw data
            }
        }

        Ok(())
    }

    /// Get current memory usage
    pub fn get_memory_usage(&self) -> MemoryUsage {
        let usage = self.memory_usage.read().unwrap();
        MemoryUsage {
            pages_mb: usage.pages_mb,
            content_streams_mb: usage.content_streams_mb,
            images_mb: usage.images_mb,
            fonts_mb: usage.fonts_mb,
            metadata_mb: usage.metadata_mb,
            total_mb: usage.total_mb,
            peak_mb: usage.peak_mb,
        }
    }

    /// Get current metrics
    pub fn get_metrics(&self) -> LoaderMetrics {
        let metrics = self.metrics.read().unwrap();
        LoaderMetrics {
            pages_loaded: metrics.pages_loaded,
            content_streams_parsed: metrics.content_streams_parsed,
            images_decoded: metrics.images_decoded,
            cache_hits: metrics.cache_hits,
            cache_misses: metrics.cache_misses,
            load_timeouts: metrics.load_timeouts,
            memory_pressure_events: metrics.memory_pressure_events,
            average_load_time_ms: metrics.average_load_time_ms,
            total_bytes_loaded: metrics.total_bytes_loaded,
        }
    }

    /// Process the load queue
    pub fn process_queue(&self, max_items: usize) -> Result<usize, String> {
        let mut processed = 0;

        while processed < max_items {
            let request = {
                let mut queue = self.load_queue.lock();
                queue.pop_front()
            };

            let request = match request {
                Some(req) => req,
                None => break, // Queue empty
            };

            // Check timeout
            if request.requested_at.elapsed() > request.timeout {
                let mut metrics = self.metrics.write().unwrap();
                metrics.load_timeouts += 1;
                continue;
            }

            self.process_request(request)?;
            processed += 1;
        }

        Ok(processed)
    }

    fn process_request(&self, request: LoadRequest) -> Result<(), String> {
        let start_time = Instant::now();

        match request.request_type {
            LoadRequestType::Page { page_number } => {
                self.load_page_data(page_number)?;
            }
            LoadRequestType::ContentStream {
                stream_id,
                operator_limit,
            } => {
                self.parse_content_stream(stream_id, operator_limit)?;
            }
            LoadRequestType::Image {
                image_id,
                decode_params,
            } => {
                self.decode_image(image_id, decode_params)?;
            }
            LoadRequestType::Font {
                font_id,
                subset_only,
            } => {
                self.load_font(font_id, subset_only)?;
            }
            LoadRequestType::Annotation { annotation_id } => {
                self.load_annotation(annotation_id)?;
            }
            LoadRequestType::FormField { field_id } => {
                self.load_form_field(field_id)?;
            }
            LoadRequestType::Metadata { metadata_type } => {
                self.load_metadata(&metadata_type)?;
            }
        }

        // Update metrics
        let load_time = start_time.elapsed().as_millis() as f64;
        let mut metrics = self.metrics.write().unwrap();
        metrics.average_load_time_ms = (metrics.average_load_time_ms + load_time) / 2.0;

        Ok(())
    }

    fn load_page_data(&self, _page_number: u32) -> Result<(), String> {
        // Implementation would load page dictionary and basic structure
        let mut metrics = self.metrics.write().unwrap();
        metrics.pages_loaded += 1;
        Ok(())
    }

    fn parse_content_stream(
        &self,
        _stream_id: ObjectId,
        _operator_limit: Option<usize>,
    ) -> Result<(), String> {
        // Implementation would parse content stream with operator limits
        let mut metrics = self.metrics.write().unwrap();
        metrics.content_streams_parsed += 1;
        Ok(())
    }

    fn decode_image(
        &self,
        _image_id: ObjectId,
        _decode_params: ImageDecodeParams,
    ) -> Result<(), String> {
        // Implementation would decode image with specified parameters
        let mut metrics = self.metrics.write().unwrap();
        metrics.images_decoded += 1;
        Ok(())
    }

    fn load_font(&self, _font_id: ObjectId, _subset_only: bool) -> Result<(), String> {
        // Implementation would load font data
        Ok(())
    }

    fn load_annotation(&self, _annotation_id: ObjectId) -> Result<(), String> {
        // Implementation would load annotation data
        Ok(())
    }

    fn load_form_field(&self, _field_id: ObjectId) -> Result<(), String> {
        // Implementation would load form field data
        Ok(())
    }

    fn load_metadata(&self, _metadata_type: &str) -> Result<(), String> {
        // Implementation would load metadata
        Ok(())
    }
}

impl Default for EnhancedLazyLoader {
    fn default() -> Self {
        Self::new(LazyLimits::default())
    }
}
