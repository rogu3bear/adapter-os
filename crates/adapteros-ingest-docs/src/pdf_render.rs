//! PDF visual content extraction module.
//!
//! Extracts images from PDF pages and prepares them for vision model inference.
//! Uses lopdf for PDF parsing and the image crate for image processing.

use crate::types::ExtractedImage;
use adapteros_core::{AosError, Result};
use image::{DynamicImage, ImageFormat};
use lopdf::{Document, Object};
use std::io::Cursor;

/// Maximum image dimension (width or height) in pixels.
/// Images larger than this will be resized to reduce memory usage and
/// improve vision model processing time.
const MAX_IMAGE_DIMENSION: u32 = 2048;

/// Minimum image dimension to consider for extraction.
/// Very small images (icons, decorations) are skipped.
const MIN_IMAGE_DIMENSION: u32 = 50;

/// Maximum total bytes for extracted images per page.
/// Prevents memory exhaustion from pages with many large images.
const MAX_PAGE_IMAGE_BYTES: usize = 50 * 1024 * 1024; // 50 MiB

/// Extract all images from a PDF page.
///
/// Returns a vector of extracted images with their metadata.
/// Images are decoded and converted to PNG format for consistent handling.
///
/// # Arguments
/// * `document` - The lopdf Document
/// * `page_id` - The ObjectId of the page
/// * `page_number` - The 1-based page number (for metadata)
///
/// # Returns
/// Vector of extracted images, or empty vector if no images found
pub fn extract_page_images(
    document: &Document,
    page_id: lopdf::ObjectId,
    page_number: u32,
) -> Vec<ExtractedImage> {
    let mut images = Vec::new();
    let mut total_bytes = 0usize;

    // Get the page's Resources dictionary
    let Ok(page_dict) = document.get_dictionary(page_id) else {
        return images;
    };

    // Navigate to Resources -> XObject
    let resources = get_resources_dict(document, page_dict);
    let Some(resources) = resources else {
        return images;
    };

    let xobjects = get_xobject_dict(document, &resources);
    let Some(xobjects) = xobjects else {
        return images;
    };

    // Iterate through XObjects looking for images
    for (name, obj_ref) in xobjects.iter() {
        // Check byte limit
        if total_bytes >= MAX_PAGE_IMAGE_BYTES {
            tracing::warn!(
                page = page_number,
                total_bytes = total_bytes,
                "Page image extraction stopped: byte limit reached"
            );
            break;
        }

        let Ok(obj_id) = obj_ref.as_reference() else {
            continue;
        };

        let Ok(stream) = document.get_object(obj_id) else {
            continue;
        };

        let Object::Stream(stream) = stream else {
            continue;
        };

        // Check if this is an Image XObject
        let Ok(subtype) = stream.dict.get(b"Subtype") else {
            continue;
        };
        let Ok(subtype_name) = subtype.as_name_str() else {
            continue;
        };
        if subtype_name != "Image" {
            continue;
        }

        // Get image dimensions
        let width = stream
            .dict
            .get(b"Width")
            .ok()
            .and_then(|w| w.as_i64().ok())
            .unwrap_or(0) as u32;
        let height = stream
            .dict
            .get(b"Height")
            .ok()
            .and_then(|h| h.as_i64().ok())
            .unwrap_or(0) as u32;

        // Skip very small images
        if width < MIN_IMAGE_DIMENSION || height < MIN_IMAGE_DIMENSION {
            continue;
        }

        // Extract image name
        let image_name = String::from_utf8_lossy(name).to_string();

        // Try to decode the image
        match decode_pdf_image(document, &stream.dict, &stream.content) {
            Ok(decoded) => {
                // Resize if needed
                let processed = resize_if_needed(decoded);

                // Convert to PNG
                match encode_as_png(&processed) {
                    Ok(png_bytes) => {
                        total_bytes += png_bytes.len();
                        images.push(ExtractedImage {
                            page_number,
                            image_name,
                            image_bytes: png_bytes,
                            width: processed.width(),
                            height: processed.height(),
                        });
                    }
                    Err(e) => {
                        tracing::debug!(
                            page = page_number,
                            image = %image_name,
                            error = %e,
                            "Failed to encode image as PNG"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::debug!(
                    page = page_number,
                    image = %image_name,
                    error = %e,
                    "Failed to decode PDF image"
                );
            }
        }
    }

    images
}

/// Convert extracted images to base64-encoded PNG strings for vision model input.
///
/// # Arguments
/// * `images` - Vector of extracted images
///
/// # Returns
/// Vector of base64-encoded PNG strings
pub fn images_to_base64(images: &[ExtractedImage]) -> Vec<String> {
    use base64::Engine;
    images
        .iter()
        .map(|img| base64::engine::general_purpose::STANDARD.encode(&img.image_bytes))
        .collect()
}

/// Generate a prompt for the vision model to describe visual content.
///
/// # Arguments
/// * `image_count` - Number of images being sent
/// * `context` - Optional context about the document or page
pub fn generate_vision_prompt(image_count: usize, context: Option<&str>) -> String {
    let base_prompt = if image_count == 1 {
        "Describe this image in detail. If it's a chart, graph, or table, extract all data points, labels, and values. If it's a diagram, describe the components and their relationships."
    } else {
        "Describe these images in detail. For each image: if it's a chart, graph, or table, extract all data points, labels, and values. If it's a diagram, describe the components and their relationships."
    };

    match context {
        Some(ctx) => format!("{} Context: {}", base_prompt, ctx),
        None => base_prompt.to_string(),
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

fn get_resources_dict(
    document: &Document,
    page_dict: &lopdf::Dictionary,
) -> Option<lopdf::Dictionary> {
    let res_ref = page_dict.get(b"Resources").ok()?;
    match res_ref {
        Object::Dictionary(d) => Some(d.clone()),
        Object::Reference(r) => document.get_dictionary(*r).ok().cloned(),
        _ => None,
    }
}

fn get_xobject_dict(
    document: &Document,
    resources: &lopdf::Dictionary,
) -> Option<lopdf::Dictionary> {
    let xobj_ref = resources.get(b"XObject").ok()?;
    match xobj_ref {
        Object::Dictionary(d) => Some(d.clone()),
        Object::Reference(r) => document.get_dictionary(*r).ok().cloned(),
        _ => None,
    }
}

fn decode_pdf_image(
    _document: &Document,
    dict: &lopdf::Dictionary,
    content: &[u8],
) -> Result<DynamicImage> {
    // Get color space and bits per component
    let bits_per_component = dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|b| b.as_i64().ok())
        .unwrap_or(8) as u8;

    let width = dict
        .get(b"Width")
        .ok()
        .and_then(|w| w.as_i64().ok())
        .ok_or_else(|| AosError::Validation("Missing image width".into()))? as u32;

    let height =
        dict.get(b"Height")
            .ok()
            .and_then(|h| h.as_i64().ok())
            .ok_or_else(|| AosError::Validation("Missing image height".into()))? as u32;

    // Check for filter (compression)
    let filter = dict.get(b"Filter").ok();

    // Try to decode based on filter
    let decoded_bytes = match filter {
        Some(Object::Name(name)) => {
            let filter_name = String::from_utf8_lossy(name);
            match filter_name.as_ref() {
                "DCTDecode" => {
                    // JPEG data - decode directly
                    return image::load_from_memory_with_format(content, ImageFormat::Jpeg)
                        .map_err(|e| AosError::Validation(format!("JPEG decode error: {}", e)));
                }
                "FlateDecode" => {
                    // Deflate compressed - decompress first
                    decompress_flate(content)?
                }
                "JPXDecode" => {
                    // JPEG 2000 - try to decode directly
                    return image::load_from_memory(content)
                        .map_err(|e| AosError::Validation(format!("JPX decode error: {}", e)));
                }
                _ => {
                    // Unsupported filter
                    return Err(AosError::Validation(format!(
                        "Unsupported image filter: {}",
                        filter_name
                    )));
                }
            }
        }
        Some(Object::Array(filters)) => {
            // Multiple filters - apply in order
            let mut data = content.to_vec();
            for f in filters {
                if let Object::Name(name) = f {
                    let filter_name = String::from_utf8_lossy(name);
                    data = match filter_name.as_ref() {
                        "FlateDecode" => decompress_flate(&data)?,
                        _ => {
                            return Err(AosError::Validation(format!(
                                "Unsupported filter in chain: {}",
                                filter_name
                            )))
                        }
                    };
                }
            }
            data
        }
        None => content.to_vec(),
        Some(_) => {
            // Unknown filter type - treat as uncompressed
            content.to_vec()
        }
    };

    // Get color space
    let color_space = dict.get(b"ColorSpace").ok();
    let is_grayscale = matches!(color_space, Some(Object::Name(n)) if n == b"DeviceGray");

    // Reconstruct image from raw bytes
    if is_grayscale && bits_per_component == 8 {
        image::GrayImage::from_raw(width, height, decoded_bytes)
            .map(DynamicImage::ImageLuma8)
            .ok_or_else(|| AosError::Validation("Failed to reconstruct grayscale image".into()))
    } else if bits_per_component == 8 {
        // Assume RGB
        let expected_len = (width * height * 3) as usize;
        if decoded_bytes.len() >= expected_len {
            image::RgbImage::from_raw(width, height, decoded_bytes[..expected_len].to_vec())
                .map(DynamicImage::ImageRgb8)
                .ok_or_else(|| AosError::Validation("Failed to reconstruct RGB image".into()))
        } else {
            Err(AosError::Validation(format!(
                "Image data too short: {} < {}",
                decoded_bytes.len(),
                expected_len
            )))
        }
    } else {
        Err(AosError::Validation(format!(
            "Unsupported bits per component: {}",
            bits_per_component
        )))
    }
}

fn decompress_flate(data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut decoder = flate2::read::ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| AosError::Validation(format!("Flate decompression error: {}", e)))?;
    Ok(decompressed)
}

fn resize_if_needed(image: DynamicImage) -> DynamicImage {
    let width = image.width();
    let height = image.height();

    if width <= MAX_IMAGE_DIMENSION && height <= MAX_IMAGE_DIMENSION {
        return image;
    }

    // Calculate new dimensions maintaining aspect ratio
    let ratio = if width > height {
        MAX_IMAGE_DIMENSION as f32 / width as f32
    } else {
        MAX_IMAGE_DIMENSION as f32 / height as f32
    };

    let new_width = (width as f32 * ratio) as u32;
    let new_height = (height as f32 * ratio) as u32;

    image.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
}

fn encode_as_png(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(|e| AosError::Validation(format!("PNG encode error: {}", e)))?;
    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_vision_prompt_single() {
        let prompt = generate_vision_prompt(1, None);
        assert!(prompt.contains("Describe this image"));
    }

    #[test]
    fn test_generate_vision_prompt_multiple() {
        let prompt = generate_vision_prompt(3, None);
        assert!(prompt.contains("Describe these images"));
    }

    #[test]
    fn test_generate_vision_prompt_with_context() {
        let prompt = generate_vision_prompt(1, Some("Financial report Q4 2024"));
        assert!(prompt.contains("Context: Financial report Q4 2024"));
    }
}
