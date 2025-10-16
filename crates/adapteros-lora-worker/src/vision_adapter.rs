//! Vision adapter implementation for the worker runtime.
//!
//! The adapter performs deterministic preprocessing of raw image bytes and
//! feeds the canonical NCHW tensor through the convolution pipeline defined in
//! [`crate::conv_pipeline`].  The implementation mirrors the behaviour of the
//! high level domain adapter so that deterministic tests can be executed on the
//! worker side without specialised hardware.

use std::io::Cursor;
use std::time::Instant;

use adapteros_core::{AosError, Result};
use image::{imageops::FilterType, DynamicImage, GenericImageView, ImageFormat};
use tracing::{debug, instrument};

use crate::conv_pipeline::{
    ActivationKind, ConvPipeline, ConvPipelineConfig, ImageBatch, PoolingStrategy,
    VisionArchitecture,
};

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
            original_sizes.push(image.dimensions());
            let resized = self.resize_image(image)?;
            let tensor = self.image_to_tensor(resized)?;
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

    fn decode_image(&self, bytes: &[u8]) -> Result<DynamicImage> {
        let mut reader = image::io::Reader::new(Cursor::new(bytes));
        reader.set_format(
            reader
                .format()
                .or_else(|| ImageFormat::from_path("dummy.jpg").ok())
                .unwrap_or(ImageFormat::Png),
        );

        reader
            .decode()
            .map_err(|e| AosError::Validation(format!("failed to decode image: {e}")))
    }

    fn resize_image(&self, image: DynamicImage) -> Result<DynamicImage> {
        let (target_h, target_w) = self.config.target_size;
        let resized = if self.config.crop_square {
            let min_edge = image.width().min(image.height());
            let x = (image.width() - min_edge) / 2;
            let y = (image.height() - min_edge) / 2;
            let square = image.crop_imm(x, y, min_edge, min_edge);
            square.resize_exact(target_w, target_h, FilterType::Lanczos3)
        } else {
            image.resize_exact(target_w, target_h, FilterType::Triangle)
        };

        Ok(resized)
    }

    fn image_to_tensor(&self, image: DynamicImage) -> Result<Vec<f32>> {
        let (target_h, target_w) = self.config.target_size;
        let mut data = Vec::with_capacity(target_h as usize * target_w as usize * 3);

        match self.config.color_space {
            ColorSpace::Rgb => {
                let rgb = image.to_rgb8();
                for c in 0..3 {
                    for y in 0..target_h {
                        for x in 0..target_w {
                            let pixel = rgb.get_pixel(x, y);
                            data.push(self.normalize(pixel[c], c));
                        }
                    }
                }
            }
            ColorSpace::Bgr => {
                let rgb = image.to_rgb8();
                for c in (0..3).rev() {
                    for y in 0..target_h {
                        for x in 0..target_w {
                            let pixel = rgb.get_pixel(x, y);
                            data.push(self.normalize(pixel[c], 2 - c));
                        }
                    }
                }
            }
            ColorSpace::Grayscale => {
                let gray = image.to_luma8();
                for y in 0..target_h {
                    for x in 0..target_w {
                        let pixel = gray.get_pixel(x, y);
                        data.push(self.normalize(pixel[0], 0));
                    }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{codecs::png::PngEncoder, ColorType};

    fn make_test_png(width: u32, height: u32) -> Vec<u8> {
        let mut buffer = vec![0u8; (width * height * 3) as usize];
        for (idx, chunk) in buffer.chunks_mut(3).enumerate() {
            let value = (idx % 255) as u8;
            chunk[0] = value;
            chunk[1] = value.saturating_add(10);
            chunk[2] = value.saturating_add(20);
        }

        let mut encoded = Vec::new();
        let encoder = PngEncoder::new(&mut encoded);
        encoder
            .encode(&buffer, width, height, ColorType::Rgb8)
            .unwrap();
        encoded
    }

    #[test]
    fn test_preprocess_batch_rgb() {
        let adapter = VisionAdapter::new(VisionAdapterConfig::default());
        let png = make_test_png(32, 48);
        let batch = adapter.preprocess_batch(&vec![png]).unwrap();
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
        let png = make_test_png(32, 32);
        let out1 = adapter.forward(&vec![png.clone()]).unwrap();
        let out2 = adapter.forward(&vec![png]).unwrap();
        assert_eq!(out1.data, out2.data);
    }

    #[test]
    fn test_grayscale_processing() {
        let mut config = VisionAdapterConfig::default();
        config.color_space = ColorSpace::Grayscale;
        config.target_size = (32, 32);
        let adapter = VisionAdapter::new(config);
        let png = make_test_png(20, 20);
        let batch = adapter.preprocess_batch(&vec![png]).unwrap();
        assert_eq!(batch.tensor.channels, 1);
        assert_eq!(batch.tensor.height, 32);
    }
}
