/// 3D and multimedia support for PDF 2.0
///
/// This module provides experimental metadata extraction for PDF multimedia
/// content, including 3D models, video, audio, interactive content, and rich
/// media annotations. It does not certify or fully decode those formats.
use crate::ast::{AstError, AstNode, AstResult, NodeId, NodeMetadata, NodeType};
use crate::types::{PdfArray, PdfDictionary, PdfName, PdfStream, PdfString, PdfValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod av;
pub mod richmedia;
pub mod threed;

// 3D and multimedia modules would be implemented here

/// Multimedia content manager for PDF documents
pub struct MultimediaManager {
    content_registry: HashMap<NodeId, Box<dyn MultimediaContent>>,
    rendition_registry: HashMap<String, Rendition>,
    assets: HashMap<String, MultimediaAsset>,
    config: MultimediaConfig,
}

/// Configuration for multimedia processing
#[derive(Debug, Clone)]
pub struct MultimediaConfig {
    pub enable_3d_support: bool,
    pub enable_video_support: bool,
    pub enable_audio_support: bool,
    pub enable_interactive_content: bool,
    pub max_asset_size_mb: usize,
    pub supported_3d_formats: Vec<String>,
    pub supported_video_formats: Vec<String>,
    pub supported_audio_formats: Vec<String>,
}

impl Default for MultimediaConfig {
    fn default() -> Self {
        Self {
            enable_3d_support: true,
            enable_video_support: true,
            enable_audio_support: true,
            enable_interactive_content: true,
            max_asset_size_mb: 100,
            supported_3d_formats: vec!["U3D".to_string(), "PRC".to_string(), "glTF".to_string()],
            supported_video_formats: vec![
                "MP4".to_string(),
                "H.264".to_string(),
                "H.265".to_string(),
                "WebM".to_string(),
            ],
            supported_audio_formats: vec![
                "MP3".to_string(),
                "AAC".to_string(),
                "OGG".to_string(),
                "WAV".to_string(),
            ],
        }
    }
}

/// Base trait for multimedia content
pub trait MultimediaContent: Send + Sync {
    /// Get the content type
    fn content_type(&self) -> MultimediaType;

    /// Get content metadata
    fn metadata(&self) -> &MultimediaMetadata;

    /// Get the size of the content in bytes
    fn size_bytes(&self) -> usize;

    /// Check if the content is playable/renderable
    fn is_playable(&self) -> bool;

    /// Get content-specific properties
    fn properties(&self) -> HashMap<String, String>;

    /// Validate the content
    fn validate(&self) -> ValidationResult;
}

/// 3D Content implementation
pub struct ThreeDContent {
    metadata: MultimediaMetadata,
    model_data: Vec<u8>,
    format: String,
}

impl ThreeDContent {
    pub fn new(
        model_data: Vec<u8>,
        format: String,
        settings: HashMap<String, String>,
    ) -> AstResult<Self> {
        let metadata = MultimediaMetadata {
            content_settings: settings,
            ..Default::default()
        };
        Ok(Self {
            metadata,
            model_data,
            format,
        })
    }
}

impl MultimediaContent for ThreeDContent {
    fn content_type(&self) -> MultimediaType {
        MultimediaType::ThreeD
    }
    fn metadata(&self) -> &MultimediaMetadata {
        &self.metadata
    }
    fn size_bytes(&self) -> usize {
        self.model_data.len()
    }
    fn is_playable(&self) -> bool {
        !self.model_data.is_empty()
    }
    fn properties(&self) -> HashMap<String, String> {
        let mut props = HashMap::new();
        props.insert("format".to_string(), self.format.clone());
        props.insert("data_size".to_string(), self.model_data.len().to_string());
        props
    }
    fn validate(&self) -> ValidationResult {
        ValidationResult {
            is_valid: !self.model_data.is_empty(),
            warnings: Vec::new(),
            errors: Vec::new(),
            format_compliance: FormatCompliance {
                format_name: "Unknown".to_string(),
                version: "1.0".to_string(),
                compliance_level: ComplianceLevel::BasicCompliant,
                missing_features: vec!["Format-specific validation is not implemented".to_string()],
                deprecated_features: Vec::new(),
            },
        }
    }
}

/// Video Content implementation
pub struct VideoContent {
    metadata: MultimediaMetadata,
    video_data: Vec<u8>,
    format: String,
}

impl VideoContent {
    pub fn new(
        video_data: Vec<u8>,
        format: String,
        settings: HashMap<String, String>,
    ) -> AstResult<Self> {
        let metadata = MultimediaMetadata {
            content_settings: settings,
            ..Default::default()
        };
        Ok(Self {
            metadata,
            video_data,
            format,
        })
    }
}

impl MultimediaContent for VideoContent {
    fn content_type(&self) -> MultimediaType {
        MultimediaType::Video
    }
    fn metadata(&self) -> &MultimediaMetadata {
        &self.metadata
    }
    fn size_bytes(&self) -> usize {
        self.video_data.len()
    }
    fn is_playable(&self) -> bool {
        !self.video_data.is_empty()
    }
    fn properties(&self) -> HashMap<String, String> {
        let mut props = HashMap::new();
        props.insert("format".to_string(), self.format.clone());
        props.insert("data_size".to_string(), self.video_data.len().to_string());
        props
    }
    fn validate(&self) -> ValidationResult {
        ValidationResult {
            is_valid: !self.video_data.is_empty(),
            warnings: Vec::new(),
            errors: Vec::new(),
            format_compliance: FormatCompliance {
                format_name: "Unknown".to_string(),
                version: "1.0".to_string(),
                compliance_level: ComplianceLevel::BasicCompliant,
                missing_features: vec!["Format-specific validation is not implemented".to_string()],
                deprecated_features: Vec::new(),
            },
        }
    }
}

/// Audio Content implementation
pub struct AudioContent {
    metadata: MultimediaMetadata,
    audio_data: Vec<u8>,
    format: String,
}

impl AudioContent {
    pub fn new(
        audio_data: Vec<u8>,
        format: String,
        settings: HashMap<String, String>,
    ) -> AstResult<Self> {
        let metadata = MultimediaMetadata {
            content_settings: settings,
            ..Default::default()
        };
        Ok(Self {
            metadata,
            audio_data,
            format,
        })
    }
}

impl MultimediaContent for AudioContent {
    fn content_type(&self) -> MultimediaType {
        MultimediaType::Audio
    }
    fn metadata(&self) -> &MultimediaMetadata {
        &self.metadata
    }
    fn size_bytes(&self) -> usize {
        self.audio_data.len()
    }
    fn is_playable(&self) -> bool {
        !self.audio_data.is_empty()
    }
    fn properties(&self) -> HashMap<String, String> {
        let mut props = HashMap::new();
        props.insert("format".to_string(), self.format.clone());
        props.insert("data_size".to_string(), self.audio_data.len().to_string());
        props
    }
    fn validate(&self) -> ValidationResult {
        ValidationResult {
            is_valid: !self.audio_data.is_empty(),
            warnings: Vec::new(),
            errors: Vec::new(),
            format_compliance: FormatCompliance {
                format_name: "Unknown".to_string(),
                version: "1.0".to_string(),
                compliance_level: ComplianceLevel::BasicCompliant,
                missing_features: vec!["Format-specific validation is not implemented".to_string()],
                deprecated_features: Vec::new(),
            },
        }
    }
}

/// Interactive Content implementation
pub struct InteractiveContent {
    metadata: MultimediaMetadata,
    content_type: String,
    script: String,
}

impl InteractiveContent {
    pub fn new(
        content_type: String,
        script: String,
        settings: HashMap<String, String>,
    ) -> AstResult<Self> {
        let metadata = MultimediaMetadata {
            content_settings: settings,
            ..Default::default()
        };
        Ok(Self {
            metadata,
            content_type,
            script,
        })
    }
}

impl MultimediaContent for InteractiveContent {
    fn content_type(&self) -> MultimediaType {
        MultimediaType::Interactive
    }
    fn metadata(&self) -> &MultimediaMetadata {
        &self.metadata
    }
    fn size_bytes(&self) -> usize {
        self.script.len()
    }
    fn is_playable(&self) -> bool {
        !self.script.is_empty()
    }
    fn properties(&self) -> HashMap<String, String> {
        let mut props = HashMap::new();
        props.insert("type".to_string(), self.content_type.clone());
        props.insert("script_size".to_string(), self.script.len().to_string());
        props
    }
    fn validate(&self) -> ValidationResult {
        ValidationResult {
            is_valid: !self.script.is_empty(),
            warnings: Vec::new(),
            errors: Vec::new(),
            format_compliance: FormatCompliance {
                format_name: "Unknown".to_string(),
                version: "1.0".to_string(),
                compliance_level: ComplianceLevel::BasicCompliant,
                missing_features: vec!["Format-specific validation is not implemented".to_string()],
                deprecated_features: Vec::new(),
            },
        }
    }
}

/// Type of multimedia content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultimediaType {
    ThreeD,
    Video,
    Audio,
    Interactive,
    RichMedia,
    Animation,
    VirtualReality,
    AugmentedReality,
}

/// Metadata for multimedia content
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MultimediaMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub creation_date: Option<chrono::DateTime<chrono::Utc>>,
    pub modification_date: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_seconds: Option<f64>,
    pub dimensions: Option<MediaDimensions>,
    pub format: String,
    pub encoding: Option<String>,
    pub compression: Option<String>,
    pub content_settings: HashMap<String, String>,
}

/// Dimensions for multimedia content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaDimensions {
    pub width: u32,
    pub height: u32,
    pub depth: Option<u32>, // For 3D content
}

/// Validation result for multimedia content
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub format_compliance: FormatCompliance,
}

/// Format compliance information
#[derive(Debug, Clone)]
pub struct FormatCompliance {
    pub format_name: String,
    pub version: String,
    pub compliance_level: ComplianceLevel,
    pub missing_features: Vec<String>,
    pub deprecated_features: Vec<String>,
}

/// Compliance level
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComplianceLevel {
    FullyCompliant,
    MostlyCompliant,
    BasicCompliant,
    NonCompliant,
}

/// Multimedia asset (embedded file or resource)
#[derive(Debug, Clone)]
pub struct MultimediaAsset {
    pub asset_id: String,
    pub asset_type: AssetType,
    pub data: AssetData,
    pub metadata: MultimediaMetadata,
    pub dependencies: Vec<String>,
}

/// Type of multimedia asset
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetType {
    Model3D,
    Texture,
    Material,
    Animation,
    Video,
    Audio,
    Script,
    Configuration,
    Other(String),
}

/// Asset data storage
#[derive(Debug, Clone)]
pub enum AssetData {
    Embedded(Vec<u8>),
    External(String), // URL or file path
    Stream(PdfStream),
}

/// Rendition for multimedia content
#[derive(Debug, Clone)]
pub struct Rendition {
    pub rendition_id: String,
    pub rendition_type: RenditionType,
    pub media_criteria: MediaCriteria,
    pub settings: RenditionSettings,
    pub assets: Vec<String>, // Asset IDs
}

/// Type of rendition
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenditionType {
    Media,
    Selector,
    ThreeD,
    JavaScript,
}

/// Criteria for media selection
#[derive(Debug, Clone)]
pub struct MediaCriteria {
    pub min_bandwidth: Option<u64>,
    pub max_bandwidth: Option<u64>,
    pub required_plugins: Vec<String>,
    pub platform_requirements: Vec<String>,
    pub quality_settings: QualitySettings,
}

/// Quality settings for media
#[derive(Debug, Clone)]
pub struct QualitySettings {
    pub resolution: Option<MediaDimensions>,
    pub bitrate: Option<u64>,
    pub framerate: Option<f64>,
    pub quality_level: QualityLevel,
}

/// Quality level enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QualityLevel {
    Low,
    Medium,
    High,
    Ultra,
    Custom(String),
}

/// Rendition settings
#[derive(Debug, Clone)]
pub struct RenditionSettings {
    pub auto_play: bool,
    pub auto_repeat: bool,
    pub show_controls: bool,
    pub volume: Option<f32>, // 0.0 to 1.0
    pub playback_rate: Option<f32>,
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    pub poster_image: Option<String>,
    pub custom_settings: HashMap<String, String>,
}

#[allow(dead_code)]
impl MultimediaManager {
    /// Create a new multimedia manager
    pub fn new(config: MultimediaConfig) -> Self {
        Self {
            content_registry: HashMap::new(),
            rendition_registry: HashMap::new(),
            assets: HashMap::new(),
            config,
        }
    }

    /// Register multimedia content
    pub fn register_content(
        &mut self,
        node_id: NodeId,
        content: Box<dyn MultimediaContent>,
    ) -> AstResult<()> {
        // Validate content size
        if content.size_bytes() > self.config.max_asset_size_mb * 1024 * 1024 {
            return Err(AstError::ParseError(format!(
                "Content size {} MB exceeds maximum allowed size {} MB",
                content.size_bytes() / (1024 * 1024),
                self.config.max_asset_size_mb
            )));
        }

        // Check format support
        if !self.is_format_supported(content.content_type(), &content.metadata().format) {
            return Err(AstError::ParseError(format!(
                "Unsupported format: {} for content type {:?}",
                content.metadata().format,
                content.content_type()
            )));
        }

        // Validate content
        let validation = content.validate();
        if !validation.is_valid {
            return Err(AstError::ParseError(format!(
                "Content validation failed: {:?}",
                validation.errors
            )));
        }

        self.content_registry.insert(node_id, content);
        Ok(())
    }

    /// Create a 3D annotation
    pub fn create_3d_annotation(
        &mut self,
        model_data: Vec<u8>,
        format: String,
        settings: HashMap<String, String>,
    ) -> AstResult<AstNode> {
        if !self.config.enable_3d_support {
            return Err(AstError::ParseError("3D support is disabled".to_string()));
        }

        let threed_content = ThreeDContent::new(model_data, format, settings)?;
        let node_id = NodeId(rand::random::<u64>() as usize);

        // Create annotation dictionary
        let mut annotation_dict = PdfDictionary::new();
        annotation_dict.insert("Type", PdfValue::Name(PdfName::new("Annot")));
        annotation_dict.insert("Subtype", PdfValue::Name(PdfName::new("3D")));

        // Add 3D stream reference
        let stream_dict = self.create_3d_stream_dict(&threed_content.properties())?;
        annotation_dict.insert("3DD", PdfValue::Dictionary(stream_dict));

        // Add default view (simplified for now)
        // TODO: Implement 3D view support

        let node = AstNode {
            id: node_id,
            node_type: NodeType::Annotation,
            value: PdfValue::Dictionary(annotation_dict),
            metadata: NodeMetadata::default(),
            children: Vec::new(),
            references: Vec::new(),
        };

        // Register the content
        self.register_content(node_id, Box::new(threed_content))?;

        Ok(node)
    }

    /// Create a video annotation
    pub fn create_video_annotation(
        &mut self,
        video_data: Vec<u8>,
        format: String,
        settings: HashMap<String, String>,
    ) -> AstResult<AstNode> {
        if !self.config.enable_video_support {
            return Err(AstError::ParseError(
                "Video support is disabled".to_string(),
            ));
        }

        let video_content = VideoContent::new(video_data, format, settings)?;
        let node_id = NodeId(rand::random::<u64>() as usize);

        // Create rich media annotation
        let mut annotation_dict = PdfDictionary::new();
        annotation_dict.insert("Type", PdfValue::Name(PdfName::new("Annot")));
        annotation_dict.insert("Subtype", PdfValue::Name(PdfName::new("RichMedia")));

        // Create rendition
        let rendition = self.create_video_rendition(&video_content.properties())?;
        self.rendition_registry
            .insert(rendition.rendition_id.clone(), rendition);

        let node = AstNode {
            id: node_id,
            node_type: NodeType::Annotation,
            value: PdfValue::Dictionary(annotation_dict),
            metadata: NodeMetadata::default(),
            children: Vec::new(),
            references: Vec::new(),
        };

        self.register_content(node_id, Box::new(video_content))?;

        Ok(node)
    }

    /// Create an audio annotation
    pub fn create_audio_annotation(
        &mut self,
        audio_data: Vec<u8>,
        format: String,
        settings: HashMap<String, String>,
    ) -> AstResult<AstNode> {
        if !self.config.enable_audio_support {
            return Err(AstError::ParseError(
                "Audio support is disabled".to_string(),
            ));
        }

        let audio_content = AudioContent::new(audio_data, format, settings)?;
        let node_id = NodeId(rand::random::<u64>() as usize);

        // Create sound annotation
        let mut annotation_dict = PdfDictionary::new();
        annotation_dict.insert("Type", PdfValue::Name(PdfName::new("Annot")));
        annotation_dict.insert("Subtype", PdfValue::Name(PdfName::new("Sound")));

        // Add sound object reference
        let sound_dict = self.create_sound_dict(&audio_content.properties())?;
        annotation_dict.insert("Sound", PdfValue::Dictionary(sound_dict));

        let node = AstNode {
            id: node_id,
            node_type: NodeType::Annotation,
            value: PdfValue::Dictionary(annotation_dict),
            metadata: NodeMetadata::default(),
            children: Vec::new(),
            references: Vec::new(),
        };

        self.register_content(node_id, Box::new(audio_content))?;

        Ok(node)
    }

    /// Create interactive content
    pub fn create_interactive_content(
        &mut self,
        content_type: String,
        script: String,
        settings: HashMap<String, String>,
    ) -> AstResult<AstNode> {
        if !self.config.enable_interactive_content {
            return Err(AstError::ParseError(
                "Interactive content is disabled".to_string(),
            ));
        }

        let interactive_content = InteractiveContent::new(content_type, script, settings)?;
        let node_id = NodeId(rand::random::<u64>() as usize);

        let mut annotation_dict = PdfDictionary::new();
        annotation_dict.insert("Type", PdfValue::Name(PdfName::new("Annot")));
        annotation_dict.insert("Subtype", PdfValue::Name(PdfName::new("Widget")));

        // Add JavaScript action
        let action_dict = self.create_javascript_action(&interactive_content.properties())?;
        annotation_dict.insert("A", PdfValue::Dictionary(action_dict));

        let node = AstNode {
            id: node_id,
            node_type: NodeType::Annotation,
            value: PdfValue::Dictionary(annotation_dict),
            metadata: NodeMetadata::default(),
            children: Vec::new(),
            references: Vec::new(),
        };

        self.register_content(node_id, Box::new(interactive_content))?;

        Ok(node)
    }

    /// Get multimedia content by node ID
    pub fn get_content(&self, node_id: NodeId) -> Option<&dyn MultimediaContent> {
        self.content_registry.get(&node_id).map(|c| c.as_ref())
    }

    /// Get all multimedia content
    pub fn get_all_content(&self) -> Vec<(NodeId, &dyn MultimediaContent)> {
        self.content_registry
            .iter()
            .map(|(&id, content)| (id, content.as_ref()))
            .collect()
    }

    /// Get content by type
    pub fn get_content_by_type(
        &self,
        content_type: MultimediaType,
    ) -> Vec<(NodeId, &dyn MultimediaContent)> {
        self.content_registry
            .iter()
            .filter(|(_, content)| content.content_type() == content_type)
            .map(|(&id, content)| (id, content.as_ref()))
            .collect()
    }

    /// Get statistics about multimedia content
    pub fn get_statistics(&self) -> MultimediaStatistics {
        let mut stats = MultimediaStatistics::default();

        for content in self.content_registry.values() {
            match content.content_type() {
                MultimediaType::ThreeD => stats.threed_count += 1,
                MultimediaType::Video => stats.video_count += 1,
                MultimediaType::Audio => stats.audio_count += 1,
                MultimediaType::Interactive => stats.interactive_count += 1,
                MultimediaType::RichMedia => stats.richmedia_count += 1,
                _ => stats.other_count += 1,
            }

            stats.total_size_bytes += content.size_bytes();
        }

        stats.total_content = self.content_registry.len();
        stats.total_renditions = self.rendition_registry.len();
        stats.total_assets = self.assets.len();

        stats
    }

    // Helper methods
    fn is_format_supported(&self, content_type: MultimediaType, format: &str) -> bool {
        match content_type {
            MultimediaType::ThreeD => self
                .config
                .supported_3d_formats
                .contains(&format.to_string()),
            MultimediaType::Video => self
                .config
                .supported_video_formats
                .contains(&format.to_string()),
            MultimediaType::Audio => self
                .config
                .supported_audio_formats
                .contains(&format.to_string()),
            _ => true, // Other types are always supported
        }
    }

    fn create_3d_stream_dict(&self, content: &HashMap<String, String>) -> AstResult<PdfDictionary> {
        let mut dict = PdfDictionary::new();
        dict.insert("Type", PdfValue::Name(PdfName::new("3D")));
        dict.insert(
            "Subtype",
            PdfValue::Name(PdfName::new(
                content.get("format").cloned().unwrap_or_default(),
            )),
        );

        // Add 3D-specific properties from content HashMap
        if let Some(width) = content.get("width").and_then(|w| w.parse::<i64>().ok()) {
            if let Some(height) = content.get("height").and_then(|h| h.parse::<i64>().ok()) {
                let mut bbox = PdfArray::new();
                bbox.push(PdfValue::Integer(0));
                bbox.push(PdfValue::Integer(0));
                bbox.push(PdfValue::Integer(width));
                bbox.push(PdfValue::Integer(height));
                dict.insert("BBox", PdfValue::Array(bbox));
            }
        }

        Ok(dict)
    }

    fn create_3d_view_dict(&self, view: &HashMap<String, String>) -> AstResult<PdfDictionary> {
        let mut dict = PdfDictionary::new();
        dict.insert("Type", PdfValue::Name(PdfName::new("3DView")));

        if let Some(name) = view.get("name") {
            dict.insert(
                "IN",
                PdfValue::String(PdfString::new_literal(name.as_bytes())),
            );
        }

        // Add camera settings (simplified - using default camera)
        let default_camera = HashMap::new();
        let camera_dict = self.create_camera_dict(&default_camera)?;
        dict.insert("C2W", PdfValue::Dictionary(camera_dict));

        Ok(dict)
    }

    fn create_camera_dict(&self, camera: &HashMap<String, String>) -> AstResult<PdfDictionary> {
        let mut dict = PdfDictionary::new();

        // Add camera position (using defaults if not provided)
        let mut position = PdfArray::new();
        let x = camera
            .get("position_x")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let y = camera
            .get("position_y")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let z = camera
            .get("position_z")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        position.push(PdfValue::Real(x));
        position.push(PdfValue::Real(y));
        position.push(PdfValue::Real(z));
        dict.insert("Position", PdfValue::Array(position));

        // Add field of view
        let fov = camera
            .get("fov")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(45.0);
        dict.insert("FOV", PdfValue::Real(fov));

        Ok(dict)
    }

    fn create_video_rendition(&self, content: &HashMap<String, String>) -> AstResult<Rendition> {
        Ok(Rendition {
            rendition_id: format!("video_{}", uuid::Uuid::new_v4()),
            rendition_type: RenditionType::Media,
            media_criteria: MediaCriteria {
                min_bandwidth: None,
                max_bandwidth: None,
                required_plugins: Vec::new(),
                platform_requirements: Vec::new(),
                quality_settings: QualitySettings {
                    resolution: None, // TODO: Parse dimensions from HashMap if needed
                    bitrate: content.get("bitrate").and_then(|b| b.parse().ok()),
                    framerate: content.get("framerate").and_then(|f| f.parse().ok()),
                    quality_level: QualityLevel::Medium,
                },
            },
            settings: RenditionSettings {
                auto_play: false,
                auto_repeat: false,
                show_controls: true,
                volume: Some(1.0),
                playback_rate: Some(1.0),
                start_time: None,
                end_time: None,
                poster_image: None,
                custom_settings: HashMap::new(),
            },
            assets: Vec::new(),
        })
    }

    fn create_sound_dict(&self, content: &HashMap<String, String>) -> AstResult<PdfDictionary> {
        let mut dict = PdfDictionary::new();
        dict.insert("Type", PdfValue::Name(PdfName::new("Sound")));

        // Add audio format
        if let Some(format) = content.get("format") {
            dict.insert("F", PdfValue::Name(PdfName::new(format)));
        }

        // Add sample rate if available
        if let Some(sample_rate_str) = content.get("sample_rate") {
            if let Ok(sample_rate) = sample_rate_str.parse::<u32>() {
                dict.insert("R", PdfValue::Integer(sample_rate as i64));
            }
        }

        Ok(dict)
    }

    fn create_javascript_action(
        &self,
        content: &HashMap<String, String>,
    ) -> AstResult<PdfDictionary> {
        let mut dict = PdfDictionary::new();
        dict.insert("Type", PdfValue::Name(PdfName::new("Action")));
        dict.insert("S", PdfValue::Name(PdfName::new("JavaScript")));

        // Add JavaScript code
        if let Some(script) = content.get("script") {
            dict.insert(
                "JS",
                PdfValue::String(PdfString::new_literal(script.as_bytes())),
            );
        }

        Ok(dict)
    }
}

impl Default for MultimediaManager {
    fn default() -> Self {
        Self::new(MultimediaConfig::default())
    }
}

/// Statistics about multimedia content in a document
#[derive(Debug, Clone, Default)]
pub struct MultimediaStatistics {
    pub total_content: usize,
    pub threed_count: usize,
    pub video_count: usize,
    pub audio_count: usize,
    pub interactive_count: usize,
    pub richmedia_count: usize,
    pub other_count: usize,
    pub total_size_bytes: usize,
    pub total_renditions: usize,
    pub total_assets: usize,
}

impl MultimediaStatistics {
    /// Get total size in megabytes
    pub fn total_size_mb(&self) -> f64 {
        self.total_size_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Check if document has any multimedia content
    pub fn has_multimedia(&self) -> bool {
        self.total_content > 0
    }

    /// Get predominant content type
    pub fn predominant_type(&self) -> Option<MultimediaType> {
        let max_count = *[
            self.threed_count,
            self.video_count,
            self.audio_count,
            self.interactive_count,
            self.richmedia_count,
            self.other_count,
        ]
        .iter()
        .max()?;

        if max_count == 0 {
            return None;
        }

        if self.threed_count == max_count {
            Some(MultimediaType::ThreeD)
        } else if self.video_count == max_count {
            Some(MultimediaType::Video)
        } else if self.audio_count == max_count {
            Some(MultimediaType::Audio)
        } else if self.interactive_count == max_count {
            Some(MultimediaType::Interactive)
        } else if self.richmedia_count == max_count {
            Some(MultimediaType::RichMedia)
        } else {
            None
        }
    }
}

/// Create a multimedia manager with default settings
pub fn create_multimedia_manager() -> MultimediaManager {
    MultimediaManager::new(MultimediaConfig::default())
}
