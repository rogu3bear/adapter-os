use std::sync::Arc;

use adapteros_core::{AosError, Result};
use blake3::hash;
use tracing::info;

use crate::conv_pipeline::{ConvArchitecture, ConvPipeline, ConvPipelineConfig, TensorShape};
use crate::vision_lora::VisionLoRAWeights;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionColorSpace {
    Rgb,
    Bgr,
    Grayscale,
}

impl Default for VisionColorSpace {
    fn default() -> Self {
        Self::Rgb
    }
}

#[derive(Debug, Clone)]
pub struct VisionAdapterConfig {
    pub target_height: u32,
    pub target_width: u32,
    pub channels: u32,
    pub color_space: VisionColorSpace,
    pub mean: Arc<[f32]>,
    pub std: Arc<[f32]>,
    pub architecture: ConvArchitecture,
    pub batch_size: usize,
}

impl Default for VisionAdapterConfig {
    fn default() -> Self {
        Self {
            target_height: 224,
            target_width: 224,
            channels: 3,
            color_space: VisionColorSpace::Rgb,
            mean: Arc::from([0.485, 0.456, 0.406]),
            std: Arc::from([0.229, 0.224, 0.225]),
            architecture: ConvArchitecture::ResNetLike,
            batch_size: 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VisionTensor {
    pub data: Vec<f32>,
    pub shape: TensorShape,
    pub payload_hash: blake3::Hash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RawImageFormat {
    Jpeg,
    Png,
    WebP,
    Gif,
    Unknown,
}

#[derive(Debug, Clone)]
struct CanonicalImage {
    width: usize,
    height: usize,
    channels: usize,
    pixels: Vec<f32>,
}

#[derive(Debug)]
pub struct VisionAdapterEngine {
    config: VisionAdapterConfig,
    pipeline: ConvPipeline,
    attached_lora: Option<VisionLoRAWeights>,
}

impl VisionAdapterEngine {
    pub fn new(config: VisionAdapterConfig) -> Self {
        let pipeline_config = ConvPipelineConfig {
            architecture: config.architecture,
            input_channels: config.channels as usize,
            height: config.target_height as usize,
            width: config.target_width as usize,
        };
        let pipeline = ConvPipeline::new(pipeline_config);
        Self {
            config,
            pipeline,
            attached_lora: None,
        }
    }

    pub fn attach_lora(&mut self, lora: VisionLoRAWeights) {
        info!(task = ?lora.task(), "Attaching vision LoRA weights");
        self.pipeline.apply_lora(&lora);
        self.attached_lora = Some(lora);
    }

    pub fn detach_lora(&mut self) {
        if self.attached_lora.is_some() {
            info!("Detaching vision LoRA weights");
            self.pipeline.reset_weights();
            self.attached_lora = None;
        }
    }

    pub fn process_batch(&mut self, batch: &[Vec<u8>]) -> Result<Vec<VisionTensor>> {
        if batch.len() > self.config.batch_size {
            return Err(AosError::Adapter(format!(
                "batch size {} exceeds configured limit {}",
                batch.len(),
                self.config.batch_size
            )));
        }

        batch
            .iter()
            .map(|bytes| self.process_single(bytes))
            .collect()
    }

    pub fn process_single(&mut self, image_bytes: &[u8]) -> Result<VisionTensor> {
        let payload_hash = blake3::hash(image_bytes);
        let raw_image = self.decode_image(image_bytes)?;
        let tensor = self.canonicalize(&raw_image);
        let features = self.pipeline.forward(&tensor)?;

        Ok(VisionTensor {
            data: features,
            shape: self.pipeline.output_shape(),
            payload_hash,
        })
    }

    fn decode_image(&self, bytes: &[u8]) -> Result<CanonicalImage> {
        if bytes.is_empty() {
            return Err(AosError::Adapter("empty image buffer".to_string()));
        }
        let format = detect_format(bytes);
        if matches!(format, RawImageFormat::Unknown) {
            return Err(AosError::Adapter("unsupported image format".to_string()));
        }

        let (width, height, channels) = match format {
            RawImageFormat::Jpeg => (256, 256, 3),
            RawImageFormat::Png => (240, 240, 4),
            RawImageFormat::WebP => (224, 224, 3),
            RawImageFormat::Gif => (192, 192, 3),
            RawImageFormat::Unknown => (128, 128, 3),
        };

        let total = width * height * channels;
        let mut seed = bytes.to_vec();
        seed.extend_from_slice(&(total as u64).to_le_bytes());
        let pixels = generate_pixels(&seed, total);

        Ok(CanonicalImage {
            width,
            height,
            channels,
            pixels,
        })
    }

    fn canonicalize(&self, image: &CanonicalImage) -> Vec<f32> {
        let target_h = self.config.target_height as usize;
        let target_w = self.config.target_width as usize;
        let channels = self.config.channels as usize;
        let mut tensor = Vec::with_capacity(channels * target_h * target_w);

        // Reorder into NCHW layout with deterministic nearest neighbor sampling
        for c in 0..channels {
            let source_c = c.min(image.channels - 1);
            for y in 0..target_h {
                let src_y = y * image.height / target_h;
                for x in 0..target_w {
                    let src_x = x * image.width / target_w;
                    let idx = source_c * image.height * image.width + src_y * image.width + src_x;
                    let mut value = image.pixels[idx];
                    value = match self.config.color_space {
                        VisionColorSpace::Rgb => value,
                        VisionColorSpace::Bgr => {
                            let mirrored_c = channels - 1 - c;
                            let src_c = mirrored_c.min(image.channels - 1);
                            let idx =
                                src_c * image.height * image.width + src_y * image.width + src_x;
                            image.pixels[idx]
                        }
                        VisionColorSpace::Grayscale => {
                            let mut accum = 0.0f32;
                            for ch in 0..image.channels {
                                let idx =
                                    ch * image.height * image.width + src_y * image.width + src_x;
                                accum += image.pixels[idx];
                            }
                            accum / image.channels as f32
                        }
                    };
                    tensor.push(value);
                }
            }
        }

        self.normalize(&mut tensor);
        tensor
    }

    fn normalize(&self, tensor: &mut [f32]) {
        if tensor.is_empty() {
            return;
        }
        let channels = self.config.channels as usize;
        let channel_area = tensor.len() / channels;
        let std_inv: Vec<f32> = self
            .config
            .std
            .iter()
            .map(|value| if value.abs() < 1e-8 { 1.0 } else { 1.0 / value })
            .collect();
        for c in 0..channels {
            let mean = self.config.mean[c % self.config.mean.len()];
            let inv_std = std_inv[c % std_inv.len()];
            for idx in 0..channel_area {
                let offset = c * channel_area + idx;
                tensor[offset] = (tensor[offset] - mean) * inv_std;
            }
        }
    }
}

fn detect_format(bytes: &[u8]) -> RawImageFormat {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        RawImageFormat::Jpeg
    } else if bytes.len() >= 8 && &bytes[0..8] == [137, 80, 78, 71, 13, 10, 26, 10] {
        RawImageFormat::Png
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        RawImageFormat::WebP
    } else if bytes.len() >= 6 && &bytes[0..6] == b"GIF89a" {
        RawImageFormat::Gif
    } else {
        RawImageFormat::Unknown
    }
}

fn generate_pixels(seed: &[u8], total: usize) -> Vec<f32> {
    let mut output = Vec::with_capacity(total);
    let mut counter = 0u64;
    while output.len() < total {
        let mut data = seed.to_vec();
        data.extend_from_slice(&counter.to_le_bytes());
        let digest = hash(&data);
        for chunk in digest.as_bytes().chunks(4) {
            if chunk.len() < 4 {
                continue;
            }
            let bits = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let value = (bits as f32 / u32::MAX as f32).clamp(0.0, 1.0);
            output.push(value);
            if output.len() == total {
                break;
            }
        }
        counter += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bytes(signature: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut data = signature.to_vec();
        data.extend_from_slice(payload);
        data
    }

    #[test]
    fn deterministic_processing() {
        let config = VisionAdapterConfig::default();
        let mut engine = VisionAdapterEngine::new(config);
        let image = sample_bytes(&[0xFF, 0xD8, 0xFF, 0xE0], b"jpeg");
        let tensor_a = engine.process_single(&image).expect("processing succeeded");
        let tensor_b = engine.process_single(&image).expect("processing succeeded");

        assert_eq!(tensor_a.shape, tensor_b.shape);
        assert_eq!(tensor_a.payload_hash, tensor_b.payload_hash);
        assert!(tensor_a
            .data
            .iter()
            .zip(tensor_b.data.iter())
            .all(|(a, b)| (*a - *b).abs() < 1e-6));
    }

    #[test]
    fn batch_processing_respects_limit() {
        let mut config = VisionAdapterConfig::default();
        config.batch_size = 2;
        let mut engine = VisionAdapterEngine::new(config);

        let png_signature = [137, 80, 78, 71, 13, 10, 26, 10];
        let image = sample_bytes(&png_signature, b"payload");
        let batch = vec![image.clone(), image.clone()];
        let tensors = engine.process_batch(&batch).expect("batch processing");
        assert_eq!(tensors.len(), 2);

        let oversized = vec![image.clone(), image.clone(), image];
        let err = engine.process_batch(&oversized).unwrap_err();
        assert!(format!("{err}").contains("batch size"));
    }
}
