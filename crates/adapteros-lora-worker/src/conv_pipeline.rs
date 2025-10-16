use adapteros_core::{AosError, Result};
use blake3::hash;
use std::fmt;

use crate::vision_lora::VisionLoRAWeights;

/// Supported convolutional backbones. The naming mirrors common CNN families
/// but the implementation focuses on deterministic execution rather than
/// matching the exact topology of those models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvArchitecture {
    ResNetLike,
    VggLike,
    MobileNetLike,
}

impl fmt::Display for ConvArchitecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvArchitecture::ResNetLike => write!(f, "resnet-like"),
            ConvArchitecture::VggLike => write!(f, "vgg-like"),
            ConvArchitecture::MobileNetLike => write!(f, "mobilenet-like"),
        }
    }
}

/// Tensor shape metadata used throughout the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TensorShape {
    pub batch: usize,
    pub channels: usize,
    pub height: usize,
    pub width: usize,
}

/// Configuration for building a deterministic convolution pipeline.
#[derive(Debug, Clone)]
pub struct ConvPipelineConfig {
    pub architecture: ConvArchitecture,
    pub input_channels: usize,
    pub height: usize,
    pub width: usize,
}

#[derive(Debug, Clone)]
struct ConvLayerSpec {
    out_channels: usize,
    kernel_size: usize,
    stride: usize,
    padding: usize,
    pooling: Option<PoolingSpec>,
}

#[derive(Debug, Clone)]
struct PoolingSpec {
    kernel: usize,
    stride: usize,
    pooling_type: PoolingType,
}

#[derive(Debug, Clone, Copy)]
enum PoolingType {
    Average,
    Max,
}

/// Deterministic convolution pipeline composed of lightweight layers and
/// pooling stages. Each layer keeps a copy of its baseline weights so that LoRA
/// adapters can be attached/detached without reallocating the entire pipeline.
#[derive(Debug)]
pub struct ConvPipeline {
    config: ConvPipelineConfig,
    layers: Vec<ConvLayerSpec>,
    weights: Vec<Vec<f32>>,      // mutable weights
    biases: Vec<Vec<f32>>,       // mutable biases
    base_weights: Vec<Vec<f32>>, // immutable baseline for reset
    base_biases: Vec<Vec<f32>>,  // immutable baseline for reset
    output_shape: TensorShape,
}

impl ConvPipeline {
    pub fn new(config: ConvPipelineConfig) -> Self {
        let layers = build_layers(&config);
        let mut weights = Vec::with_capacity(layers.len());
        let mut biases = Vec::with_capacity(layers.len());
        let mut base_weights = Vec::with_capacity(layers.len());
        let mut base_biases = Vec::with_capacity(layers.len());

        let mut in_channels = config.input_channels;
        let mut height = config.height;
        let mut width = config.width;

        for (index, layer) in layers.iter().enumerate() {
            let kernel_elements = layer.kernel_size * layer.kernel_size;
            let weight_len = layer.out_channels * in_channels * kernel_elements;
            let bias_len = layer.out_channels;

            let weight_seed = format!(
                "conv-layer-{arch}-{idx}-{in_channels}-{out_channels}-{kernel}",
                arch = config.architecture,
                idx = index,
                out_channels = layer.out_channels,
                kernel = layer.kernel_size,
            );
            let bias_seed = format!(
                "bias-layer-{arch}-{idx}",
                arch = config.architecture,
                idx = index
            );

            let weight_values = deterministic_values(&weight_seed, weight_len);
            let bias_values = deterministic_values(&bias_seed, bias_len);

            weights.push(weight_values.clone());
            biases.push(bias_values.clone());
            base_weights.push(weight_values);
            base_biases.push(bias_values);

            height = (height + 2 * layer.padding - layer.kernel_size) / layer.stride + 1;
            width = (width + 2 * layer.padding - layer.kernel_size) / layer.stride + 1;

            if let Some(pool) = &layer.pooling {
                height = (height.saturating_sub(pool.kernel)) / pool.stride + 1;
                width = (width.saturating_sub(pool.kernel)) / pool.stride + 1;
            }

            in_channels = layer.out_channels;
        }

        // Global average pooling collapses the spatial dimensions into 1x1
        let output_shape = TensorShape {
            batch: 1,
            channels: layers
                .last()
                .map(|l| l.out_channels)
                .unwrap_or(config.input_channels),
            height: 1,
            width: 1,
        };

        Self {
            config,
            layers,
            weights,
            biases,
            base_weights,
            base_biases,
            output_shape,
        }
    }

    /// Reset the pipeline to its baseline weights
    pub fn reset_weights(&mut self) {
        for (weights, baseline) in self.weights.iter_mut().zip(self.base_weights.iter()) {
            weights.copy_from_slice(baseline);
        }
        for (bias, baseline) in self.biases.iter_mut().zip(self.base_biases.iter()) {
            bias.copy_from_slice(baseline);
        }
    }

    /// Apply a LoRA adapter in place. The adapter is merged with the baseline to
    /// avoid cumulative error when switching between tasks.
    pub fn apply_lora(&mut self, lora: &VisionLoRAWeights) {
        self.reset_weights();
        for update in lora.layer_updates() {
            if let Some(weights) = self.weights.get_mut(update.layer_index) {
                for (value, delta) in weights.iter_mut().zip(update.weight_delta.iter()) {
                    *value += update.scaling * *delta;
                }
            }
            if let Some(bias) = self.biases.get_mut(update.layer_index) {
                for (value, delta) in bias.iter_mut().zip(update.bias_delta.iter()) {
                    *value += update.scaling * *delta;
                }
            }
        }
    }

    /// Execute the convolution pipeline on the provided tensor. The input must
    /// be arranged in NCHW layout with batch size 1.
    pub fn forward(&self, tensor: &[f32]) -> Result<Vec<f32>> {
        let expected_len = self.config.input_channels * self.config.height * self.config.width;
        if tensor.len() != expected_len {
            return Err(AosError::Adapter(format!(
                "expected tensor of length {expected_len}, got {}",
                tensor.len()
            )));
        }

        let mut activation = tensor.to_vec();
        let mut shape = TensorShape {
            batch: 1,
            channels: self.config.input_channels,
            height: self.config.height,
            width: self.config.width,
        };

        for (layer_idx, layer) in self.layers.iter().enumerate() {
            let weights = &self.weights[layer_idx];
            let bias = &self.biases[layer_idx];
            activation = self.apply_layer(layer, &activation, shape, weights, bias);

            shape = TensorShape {
                batch: 1,
                channels: layer.out_channels,
                height: (shape.height + 2 * layer.padding - layer.kernel_size) / layer.stride + 1,
                width: (shape.width + 2 * layer.padding - layer.kernel_size) / layer.stride + 1,
            };

            if let Some(pool) = &layer.pooling {
                let pooled_height = pooled_dim(shape.height, pool.kernel, pool.stride);
                let pooled_width = pooled_dim(shape.width, pool.kernel, pool.stride);
                activation = self.pool(&activation, shape, pool);
                shape = TensorShape {
                    batch: 1,
                    channels: shape.channels,
                    height: pooled_height,
                    width: pooled_width,
                };
            }
        }

        let pooled = self.global_average_pool(&activation, shape);
        Ok(pooled)
    }

    pub fn output_shape(&self) -> TensorShape {
        self.output_shape
    }

    fn apply_layer(
        &self,
        layer: &ConvLayerSpec,
        input: &[f32],
        shape: TensorShape,
        weights: &[f32],
        bias: &[f32],
    ) -> Vec<f32> {
        let kernel = layer.kernel_size;
        let pad = layer.padding as isize;
        let out_height = (shape.height + 2 * layer.padding - kernel) / layer.stride + 1;
        let out_width = (shape.width + 2 * layer.padding - kernel) / layer.stride + 1;
        let mut output = vec![0.0; layer.out_channels * out_height * out_width];

        for oc in 0..layer.out_channels {
            for oy in 0..out_height {
                for ox in 0..out_width {
                    let mut sum = bias[oc];
                    for ic in 0..shape.channels {
                        for ky in 0..kernel {
                            for kx in 0..kernel {
                                let iy = oy as isize * layer.stride as isize + ky as isize - pad;
                                let ix = ox as isize * layer.stride as isize + kx as isize - pad;
                                if iy >= 0
                                    && iy < shape.height as isize
                                    && ix >= 0
                                    && ix < shape.width as isize
                                {
                                    let input_idx = ic * shape.height * shape.width
                                        + iy as usize * shape.width
                                        + ix as usize;
                                    let weight_idx = oc * shape.channels * kernel * kernel
                                        + ic * kernel * kernel
                                        + ky * kernel
                                        + kx;
                                    sum += input[input_idx] * weights[weight_idx];
                                }
                            }
                        }
                    }
                    // Deterministic activation (ReLU)
                    let output_idx = oc * out_height * out_width + oy * out_width + ox;
                    output[output_idx] = sum.max(0.0);
                }
            }
        }

        // Apply per-channel normalization for stability
        self.normalize_channels(output, layer.out_channels, out_height, out_width)
    }

    fn pool(&self, activation: &[f32], shape: TensorShape, spec: &PoolingSpec) -> Vec<f32> {
        let kernel_h = spec.kernel.min(shape.height.max(1));
        let kernel_w = spec.kernel.min(shape.width.max(1));
        let out_height = if shape.height >= spec.kernel {
            (shape.height - spec.kernel) / spec.stride + 1
        } else {
            1
        };
        let out_width = if shape.width >= spec.kernel {
            (shape.width - spec.kernel) / spec.stride + 1
        } else {
            1
        };

        let mut pooled = Vec::with_capacity(shape.channels * out_height * out_width);

        for c in 0..shape.channels {
            for oy in 0..out_height {
                for ox in 0..out_width {
                    let start_y = if shape.height >= spec.kernel {
                        oy * spec.stride
                    } else {
                        0
                    };
                    let start_x = if shape.width >= spec.kernel {
                        ox * spec.stride
                    } else {
                        0
                    };

                    let mut acc = match spec.pooling_type {
                        PoolingType::Average => 0.0,
                        PoolingType::Max => f32::MIN,
                    };

                    for ky in 0..kernel_h {
                        for kx in 0..kernel_w {
                            let idx = c * shape.height * shape.width
                                + (start_y + ky) * shape.width
                                + (start_x + kx);
                            let value = activation[idx];
                            match spec.pooling_type {
                                PoolingType::Average => acc += value,
                                PoolingType::Max => acc = acc.max(value),
                            }
                        }
                    }

                    let divisor = (kernel_h * kernel_w) as f32;
                    let norm = match spec.pooling_type {
                        PoolingType::Average => acc / divisor.max(1.0),
                        PoolingType::Max => acc,
                    };
                    pooled.push(norm);
                }
            }
        }

        pooled
    }

    fn global_average_pool(&self, activation: &[f32], shape: TensorShape) -> Vec<f32> {
        let mut pooled = vec![0.0; shape.channels];
        let spatial = (shape.height * shape.width) as f32;
        for c in 0..shape.channels {
            let mut acc = 0.0;
            for y in 0..shape.height {
                for x in 0..shape.width {
                    let idx = c * shape.height * shape.width + y * shape.width + x;
                    acc += activation[idx];
                }
            }
            pooled[c] = acc / spatial;
        }
        pooled
    }

    fn normalize_channels(
        &self,
        mut activation: Vec<f32>,
        channels: usize,
        height: usize,
        width: usize,
    ) -> Vec<f32> {
        for c in 0..channels {
            let mut mean = 0.0;
            let mut sq_mean = 0.0;
            for y in 0..height {
                for x in 0..width {
                    let idx = c * height * width + y * width + x;
                    let val = activation[idx];
                    mean += val;
                    sq_mean += val * val;
                }
            }
            let denom = (height * width) as f32;
            mean /= denom;
            sq_mean /= denom;
            let variance = (sq_mean - mean * mean).max(1e-6);
            let inv_std = 1.0 / variance.sqrt();
            for y in 0..height {
                for x in 0..width {
                    let idx = c * height * width + y * width + x;
                    activation[idx] = (activation[idx] - mean) * inv_std;
                }
            }
        }
        activation
    }
}

fn build_layers(config: &ConvPipelineConfig) -> Vec<ConvLayerSpec> {
    match config.architecture {
        ConvArchitecture::ResNetLike => vec![
            ConvLayerSpec {
                out_channels: 32,
                kernel_size: 3,
                stride: 1,
                padding: 1,
                pooling: Some(PoolingSpec {
                    kernel: 2,
                    stride: 2,
                    pooling_type: PoolingType::Average,
                }),
            },
            ConvLayerSpec {
                out_channels: 64,
                kernel_size: 3,
                stride: 1,
                padding: 1,
                pooling: Some(PoolingSpec {
                    kernel: 2,
                    stride: 2,
                    pooling_type: PoolingType::Average,
                }),
            },
            ConvLayerSpec {
                out_channels: 128,
                kernel_size: 3,
                stride: 1,
                padding: 1,
                pooling: None,
            },
        ],
        ConvArchitecture::VggLike => vec![
            ConvLayerSpec {
                out_channels: 64,
                kernel_size: 3,
                stride: 1,
                padding: 1,
                pooling: Some(PoolingSpec {
                    kernel: 2,
                    stride: 2,
                    pooling_type: PoolingType::Max,
                }),
            },
            ConvLayerSpec {
                out_channels: 64,
                kernel_size: 3,
                stride: 1,
                padding: 1,
                pooling: Some(PoolingSpec {
                    kernel: 2,
                    stride: 2,
                    pooling_type: PoolingType::Max,
                }),
            },
            ConvLayerSpec {
                out_channels: 128,
                kernel_size: 3,
                stride: 1,
                padding: 1,
                pooling: Some(PoolingSpec {
                    kernel: 2,
                    stride: 2,
                    pooling_type: PoolingType::Average,
                }),
            },
            ConvLayerSpec {
                out_channels: 256,
                kernel_size: 3,
                stride: 1,
                padding: 1,
                pooling: None,
            },
        ],
        ConvArchitecture::MobileNetLike => vec![
            ConvLayerSpec {
                out_channels: 32,
                kernel_size: 3,
                stride: 1,
                padding: 1,
                pooling: Some(PoolingSpec {
                    kernel: 2,
                    stride: 2,
                    pooling_type: PoolingType::Average,
                }),
            },
            ConvLayerSpec {
                out_channels: 64,
                kernel_size: 1,
                stride: 1,
                padding: 0,
                pooling: Some(PoolingSpec {
                    kernel: 2,
                    stride: 2,
                    pooling_type: PoolingType::Average,
                }),
            },
            ConvLayerSpec {
                out_channels: 96,
                kernel_size: 3,
                stride: 1,
                padding: 1,
                pooling: Some(PoolingSpec {
                    kernel: 2,
                    stride: 2,
                    pooling_type: PoolingType::Average,
                }),
            },
            ConvLayerSpec {
                out_channels: 128,
                kernel_size: 1,
                stride: 1,
                padding: 0,
                pooling: None,
            },
        ],
    }
}

fn deterministic_values(seed: &str, len: usize) -> Vec<f32> {
    let mut values = Vec::with_capacity(len);
    let mut counter = 0u64;
    while values.len() < len {
        let mut data = seed.as_bytes().to_vec();
        data.extend_from_slice(&counter.to_le_bytes());
        let hash_bytes = hash(&data).as_bytes().to_owned();
        for chunk in hash_bytes.chunks(4) {
            if chunk.len() < 4 {
                continue;
            }
            let bits = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let normalized = bits as f32 / u32::MAX as f32;
            let value = (normalized - 0.5) * 0.2; // keep values in a small range
            values.push(value);
            if values.len() == len {
                break;
            }
        }
        counter += 1;
    }
    values
}

fn pooled_dim(size: usize, kernel: usize, stride: usize) -> usize {
    if size == 0 {
        return 0;
    }
    if size >= kernel && kernel > 0 {
        (size - kernel) / stride + 1
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_input(channels: usize, height: usize, width: usize) -> Vec<f32> {
        let mut tensor = Vec::with_capacity(channels * height * width);
        for idx in 0..tensor.capacity() {
            tensor.push((idx % 255) as f32 / 255.0);
        }
        tensor
    }

    #[test]
    fn resnet_like_pipeline_produces_deterministic_output() {
        let config = ConvPipelineConfig {
            architecture: ConvArchitecture::ResNetLike,
            input_channels: 3,
            height: 224,
            width: 224,
        };
        let pipeline = ConvPipeline::new(config);
        let input = create_input(3, 224, 224);
        let output_a = pipeline.forward(&input).expect("forward succeeds");
        let output_b = pipeline.forward(&input).expect("forward succeeds");

        assert_eq!(output_a.len(), output_b.len());
        assert!(output_a
            .iter()
            .zip(output_b.iter())
            .all(|(a, b)| (*a - *b).abs() < 1e-6));
    }

    #[test]
    fn lora_updates_are_applied_and_reset() {
        use crate::vision_lora::{LayerLoRA, VisionTask};

        let config = ConvPipelineConfig {
            architecture: ConvArchitecture::VggLike,
            input_channels: 3,
            height: 64,
            width: 64,
        };
        let mut pipeline = ConvPipeline::new(config);
        let base_input = create_input(3, 64, 64);
        let base_output = pipeline.forward(&base_input).unwrap();

        let delta = vec![0.01; pipeline.weights[0].len()];
        let bias_delta = vec![0.02; pipeline.biases[0].len()];
        let lora = VisionLoRAWeights::new(
            VisionTask::ImageClassification,
            vec![LayerLoRA::new(0, delta.clone(), bias_delta.clone(), 0.5)],
        );

        pipeline.apply_lora(&lora);
        let lora_output = pipeline.forward(&base_input).unwrap();
        assert!(lora_output
            .iter()
            .zip(base_output.iter())
            .any(|(a, b)| (*a - *b).abs() > 1e-4));

        pipeline.reset_weights();
        let reset_output = pipeline.forward(&base_input).unwrap();
        assert!(reset_output
            .iter()
            .zip(base_output.iter())
            .all(|(a, b)| (*a - *b).abs() < 1e-6));
    }
}
