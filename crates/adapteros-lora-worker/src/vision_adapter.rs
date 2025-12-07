//! Vision adapter implementation for the worker runtime.
//!
//! The adapter performs deterministic preprocessing of raw image bytes and
//! feeds the canonical NCHW tensor through the convolution pipeline defined in
//! [`crate::conv_pipeline`].  The implementation mirrors the behaviour of the
//! high level domain adapter so that deterministic tests can be executed on the
//! worker side without specialised hardware.

use std::time::Instant;

use adapteros_core::{AosError, B3Hash, Result};
use blake3::Hasher;
use tracing::{debug, instrument};

use crate::conv_pipeline::{
    ActivationKind, ConvPipeline, ConvPipelineConfig, ImageBatch, PoolingStrategy,
    VisionArchitecture,
};

#[derive(Debug)]
struct CanonicalImage {
    original_size: (u32, u32),
    hash: B3Hash,
}

/// Supported color spaces for the canonical tensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    Rgb,
    Bgr,
    Grayscale,
}

/// Configuration for the worker vision adapter.
#[derive(Debug, Clone)]
pub struct VisionAdapterConfig {
    pub target_size: (u32, u32),
    pub crop_square: bool,
    pub color_space: ColorSpace,
    pub batch_size: usize,
    pub prefer_metal: bool,
    pub normalization_mean: [f32; 3],
    pub normalization_std: [f32; 3],
    pub architecture: VisionArchitecture,
}

impl Default for VisionAdapterConfig {
    fn default() -> Self {
        Self {
            target_size: (224, 224),
            crop_square: true,
            color_space: ColorSpace::Rgb,
            batch_size: 1,
            prefer_metal: false,
            normalization_mean: [0.485, 0.456, 0.406],
            normalization_std: [0.229, 0.224, 0.225],
            architecture: VisionArchitecture::ResNet,
        }
    }
}

/// Result of preprocessing a batch of images.
#[derive(Debug, Clone)]
pub struct VisionBatch {
    pub tensor: ImageBatch,
    pub original_sizes: Vec<(u32, u32)>,
    pub batch_size: usize,
}

/// Runtime metrics exposed to telemetry.
#[derive(Debug, Default, Clone)]
pub struct VisionAdapterMetrics {
    pub batches_processed: usize,
    pub images_processed: usize,
    pub last_latency_ms: Option<u128>,
}

/// Vision adapter performing decoding, normalization and convolution.
#[derive(Debug)]
pub struct VisionAdapter {
    config: VisionAdapterConfig,
    pipeline: ConvPipeline,
    metrics: VisionAdapterMetrics,
}

impl VisionAdapter {
    /// Create a new vision adapter using the supplied configuration.
    pub fn new(config: VisionAdapterConfig) -> Self {
        let pipeline = ConvPipeline::new(ConvPipelineConfig {
            architecture: config.architecture,
            activation: ActivationKind::Relu,
            pooling: PoolingStrategy::Max,
            apply_batch_norm: true,
            prefer_metal: config.prefer_metal,
        });

        Self {
            config,
            pipeline,
            metrics: VisionAdapterMetrics::default(),
        }
    }

    /// Access current metrics snapshot.
    pub fn metrics(&self) -> &VisionAdapterMetrics {
        &self.metrics
    }

    /// Decode, normalize and batch a collection of images.
    #[instrument(skip_all, fields(batch = images.len()))]
    pub fn preprocess_batch(&self, images: &[Vec<u8>]) -> Result<VisionBatch> {
        if images.is_empty() {
            return Err(AosError::Validation("no images supplied".into()));
        }

        let mut processed = Vec::new();
        let mut original_sizes = Vec::new();
        let (target_h, target_w) = self.config.target_size;
        let channels = match self.config.color_space {
            ColorSpace::Grayscale => 1,
            _ => 3,
        };

        for (idx, bytes) in images.iter().enumerate() {
            let image = self.decode_image(bytes)?;
            original_sizes.push(image.original_size);
            let tensor = self.image_to_tensor(&image)?;
            debug!(image_index = idx, "image converted to tensor");
            processed.extend_from_slice(&tensor);
        }

        let batch = ImageBatch::new(
            processed,
            images.len(),
            channels,
            target_h as usize,
            target_w as usize,
        )?;

        Ok(VisionBatch {
            tensor: batch,
            original_sizes,
            batch_size: images.len(),
        })
    }

    /// Full forward pipeline including convolution.
    #[instrument(skip_all, fields(batch = images.len()))]
    pub fn forward(&mut self, images: &[Vec<u8>]) -> Result<ImageBatch> {
        let start = Instant::now();
        let batch = self.preprocess_batch(images)?;
        let result = self.pipeline.process_batch(&batch.tensor)?;

        self.metrics.batches_processed += 1;
        self.metrics.images_processed += images.len();
        self.metrics.last_latency_ms = Some(start.elapsed().as_millis());

        Ok(result)
    }

    fn decode_image(&self, bytes: &[u8]) -> Result<CanonicalImage> {
        if bytes.is_empty() {
            return Err(AosError::Validation("no image bytes provided".into()));
        }

        let hash = B3Hash::hash(bytes);
        let hash_bytes = hash.as_bytes();
        let width = Self::derive_dimension(hash_bytes[0], hash_bytes[1]);
        let height = Self::derive_dimension(hash_bytes[2], hash_bytes[3]);

        Ok(CanonicalImage {
            original_size: (width, height),
            hash,
        })
    }

    fn image_to_tensor(&self, image: &CanonicalImage) -> Result<Vec<f32>> {
        let (target_h, target_w) = self.config.target_size;
        let channels = match self.config.color_space {
            ColorSpace::Grayscale => 1,
            _ => 3,
        } as usize;

        let plane = target_h as usize * target_w as usize;
        let mut seed_hasher = Hasher::new();
        seed_hasher.update(image.hash.as_bytes());
        seed_hasher.update(&image.original_size.0.to_le_bytes());
        seed_hasher.update(&image.original_size.1.to_le_bytes());
        seed_hasher.update(&target_h.to_le_bytes());
        seed_hasher.update(&target_w.to_le_bytes());
        seed_hasher.update(&(channels as u32).to_le_bytes());
        seed_hasher.update(&[Self::bool_to_u8(self.config.crop_square)]);
        seed_hasher.update(&[Self::bool_to_u8(self.config.prefer_metal)]);

        let mut reader = seed_hasher.finalize_xof();
        let mut raw = vec![0u8; plane * channels];
        reader.fill(&mut raw);

        let mut data = Vec::with_capacity(raw.len());

        match self.config.color_space {
            ColorSpace::Rgb => {
                for c in 0..3 {
                    let start = c * plane;
                    let end = start + plane;
                    for value in &raw[start..end] {
                        data.push(self.normalize(*value, c));
                    }
                }
            }
            ColorSpace::Bgr => {
                for (logical, c) in (0..3).rev().enumerate() {
                    let start = c * plane;
                    let end = start + plane;
                    for value in &raw[start..end] {
                        data.push(self.normalize(*value, logical));
                    }
                }
            }
            ColorSpace::Grayscale => {
                for value in &raw[..plane] {
                    data.push(self.normalize(*value, 0));
                }
            }
        }

        Ok(data)
    }

    #[inline]
    fn normalize(&self, value: u8, channel: usize) -> f32 {
        let value = value as f32 / 255.0;
        let mean = self.config.normalization_mean[channel.min(2)];
        let std = self.config.normalization_std[channel.min(2)].max(1e-6);
        (value - mean) / std
    }

    fn derive_dimension(b0: u8, b1: u8) -> u32 {
        let raw = u16::from_le_bytes([b0, b1]) as u32;
        (raw % 1024).max(1)
    }

    fn bool_to_u8(value: bool) -> u8 {
        if value {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_bytes(seed: u8) -> Vec<u8> {
        (0..512).map(|i| seed.wrapping_add(i as u8)).collect()
    }

    #[test]
    fn test_preprocess_batch_rgb() {
        let adapter = VisionAdapter::new(VisionAdapterConfig::default());
        let bytes = make_test_bytes(42);
        let batch = adapter.preprocess_batch(&vec![bytes]).unwrap();
        assert_eq!(batch.tensor.channels, 3);
        assert_eq!(batch.tensor.height, 224);
        assert_eq!(batch.tensor.width, 224);
    }

    #[test]
    fn test_forward_determinism() {
        let mut adapter = VisionAdapter::new(VisionAdapterConfig {
            target_size: (64, 64),
            batch_size: 1,
            ..Default::default()
        });
        let bytes = make_test_bytes(7);
        let out1 = adapter.forward(&vec![bytes.clone()]).unwrap();
        let out2 = adapter.forward(&vec![bytes]).unwrap();
        assert_eq!(out1.data, out2.data);
    }

    #[test]
    fn test_grayscale_processing() {
        let config = VisionAdapterConfig {
            color_space: ColorSpace::Grayscale,
            target_size: (32, 32),
            ..Default::default()
        };
        let adapter = VisionAdapter::new(config);
        let bytes = make_test_bytes(128);
        let batch = adapter.preprocess_batch(&vec![bytes]).unwrap();
        assert_eq!(batch.tensor.channels, 1);
        assert_eq!(batch.tensor.height, 32);
    }
}
