use wasm_bindgen::prelude::*;
use crate::{Heatmap, DomainType};
use colorous;

#[wasm_bindgen(start)]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn get_image_size() -> u32 {
    4096
}

#[wasm_bindgen]
pub fn generate_heatmap(
    input_data: &str,
    curve_type: &str,
    min_value: Option<f64>,
    max_value: Option<f64>,
    accumulate: bool,
    bits_per_pixel: u8,
    colour_scale: &str,
    categorical: bool,
) -> Result<Vec<u8>, JsValue> {
    // Parse curve type
    let domain_type = match curve_type.to_lowercase().as_str() {
        "linear" => DomainType::Linear,
        "logarithmic" => DomainType::Logarithmic,
        _ => return Err(JsValue::from_str(&format!("Invalid curve type: {}. Must be 'linear' or 'logarithmic'", curve_type))),
    };

    // Validate bits_per_pixel (minimum 8 for 4096x4096 image)
    if bits_per_pixel < 8 {
        return Err(JsValue::from_str(&format!(
            "bits_per_pixel must be at least 8 for a 4096x4096 image (got {}). Each pixel represents 2^bits_per_pixel IPs.",
            bits_per_pixel
        )));
    }
    if bits_per_pixel > 24 {
        return Err(JsValue::from_str(&format!(
            "bits_per_pixel cannot exceed 24 (got {})",
            bits_per_pixel
        )));
    }

    // Parse color scale
    let gradient = match colour_scale.to_lowercase().as_str() {
        "magma" => &colorous::MAGMA,
        "inferno" => &colorous::INFERNO,
        "plasma" => &colorous::PLASMA,
        "viridis" => &colorous::VIRIDIS,
        "cividis" => &colorous::CIVIDIS,
        "turbo" => &colorous::TURBO,
        "warm" => &colorous::WARM,
        "cool" => &colorous::COOL,
        _ => return Err(JsValue::from_str(&format!("Invalid colour scale: {}. Supported: magma, inferno, plasma, viridis, cividis, turbo, warm, cool", colour_scale))),
    };

    // Create heatmap
    let mut heatmap = Heatmap::new(
        domain_type,
        min_value,
        max_value,
        accumulate,
        bits_per_pixel,
        gradient,
        categorical,
    );

    // Process input
    heatmap.process_input_from_string(input_data)
        .map_err(|e| JsValue::from_str(&format!("Failed to process input: {}", e)))?;

    // Get RGBA data
    let rgba_data = heatmap.get_rgba_data()
        .map_err(|e| JsValue::from_str(&format!("Failed to generate RGBA data: {}", e)))?;

    Ok(rgba_data)
}
