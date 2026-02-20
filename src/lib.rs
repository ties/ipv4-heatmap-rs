use anyhow::{Context, Result, anyhow};
use colorous::Gradient;
use image::{ImageBuffer, RgbaImage, Rgba};
use std::io::BufRead;
use std::net::Ipv4Addr;
use std::str::FromStr;

mod hilbert;
mod scale;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

use hilbert::hilbert_d2xy;
use ipnet::Ipv4Net;
use scale::ScaleDomain;

// Re-export types for public API
pub use scale::DomainType;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueMode {
    Categorical,
    Raw,
    Scaled,
}

impl std::str::FromStr for ValueMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "categorical" => Ok(ValueMode::Categorical),
            "raw" => Ok(ValueMode::Raw),
            "scaled" => Ok(ValueMode::Scaled),
            _ => Err(format!(
                "Invalid value mode: {}. Use 'categorical', 'raw', or 'scaled'",
                s
            )),
        }
    }
}

impl std::fmt::Display for ValueMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueMode::Categorical => write!(f, "categorical"),
            ValueMode::Raw => write!(f, "raw"),
            ValueMode::Scaled => write!(f, "scaled"),
        }
    }
}

/// Calculate the image side length for a given bits_per_pixel value.
///
/// The IPv4 address space has 2^32 addresses. Each pixel covers 2^bits_per_pixel addresses,
/// so there are 2^(32 - bits_per_pixel) pixels total. These are arranged in a square via
/// a Hilbert curve, giving a side length of 2^((32 - bits_per_pixel) / 2).
///
/// bits_per_pixel must be even (so that 32 - bits_per_pixel is even and the pixels form
/// a perfect square) and at most 32.
pub fn image_size_for_bpp(bits_per_pixel: u8) -> u32 {
    let order = (32 - bits_per_pixel as u32) / 2;
    1u32 << order
}

/// ColorBrewer2 Accent categorical palette (wraps for categories > 7)
const CATEGORICAL_PALETTE: [[u8; 3]; 8] = [
    [127, 201, 127],  // green
    [190, 174, 212],  // purple
    [253, 192, 134],  // orange
    [255, 255, 153],  // yellow
    [56, 108, 176],   // blue
    [240, 2, 127],    // pink
    [191, 91, 23],    // brown
    [102, 102, 102],  // gray
];

pub struct Heatmap {
    buffer: Vec<Vec<i32>>,
    curve: scale::DomainType,
    min_value: Option<f64>,
    max_value: Option<f64>,
    accumulate: bool,
    bits_per_pixel: u8,
    colour_scale: &'static Gradient,
    value_mode: ValueMode,
}

impl Heatmap {
    /// Returns the side length of the output image in pixels.
    pub fn image_size(&self) -> u32 {
        image_size_for_bpp(self.bits_per_pixel)
    }

    pub fn new(
        curve: scale::DomainType,
        min_value: Option<f64>,
        max_value: Option<f64>,
        accumulate: bool,
        bits_per_pixel: u8,
        colour_scale: &'static Gradient,
        value_mode: ValueMode,
    ) -> Self {
        // Use -1 as sentinel for "no data" in categorical mode
        let init_value = match value_mode {
            ValueMode::Categorical => -1,
            ValueMode::Raw | ValueMode::Scaled => 0,
        };
        let size = image_size_for_bpp(bits_per_pixel) as usize;
        let buffer = vec![vec![init_value; size]; size];

        Self {
            buffer,
            curve,
            min_value,
            max_value,
            accumulate,
            bits_per_pixel,
            colour_scale,
            value_mode,
        }
    }

    fn ip_to_xy(&self, ip: u32) -> Option<(u32, u32)> {
        let hilbert_curve_order = (32 - self.bits_per_pixel) as u32 / 2; // (addr_space_bits_per_image - addr_space_bits_per_pixel) / 2;

        let shift = self.bits_per_pixel as u32;
        let d = ip >> shift;

        hilbert_d2xy(d as u64, hilbert_curve_order)
    }

    fn paint_pixel(&mut self, x: u32, y: u32, value: i32) {
        if self.accumulate {
            self.buffer[y as usize][x as usize] += value;
        } else {
            self.buffer[y as usize][x as usize] = value;
        }
    }

    pub fn paint_address(&mut self, addr: &Ipv4Addr, value: i32) -> Result<()> {
        let paint_value = match self.value_mode {
            ValueMode::Categorical | ValueMode::Raw => value,
            ValueMode::Scaled => {
                let ips_per_pixel = 1u64 << self.bits_per_pixel;
                (value as f64 / ips_per_pixel as f64) as i32
            }
        };

        if let Some((x, y)) = self.ip_to_xy(u32::from(*addr)) {
            self.paint_pixel(x, y, paint_value);
        }
        Ok(())
    }

    pub fn paint_cidr_range(&mut self, cidr: &Ipv4Net, value: i32) -> Result<()> {
        // Calculate how many IPs are represented by each pixel
        let ips_per_pixel = 1u64 << self.bits_per_pixel;

        // Calculate the range of pixels that this CIDR block covers
        let first_ip = u32::from(cidr.network()) as u64;
        let last_ip = u32::from(cidr.broadcast()) as u64;
        let first_pixel_d = first_ip >> self.bits_per_pixel;
        let last_pixel_d = last_ip >> self.bits_per_pixel;

        // Iterate through the affected pixels
        for pixel_d in first_pixel_d..=last_pixel_d {
            // Calculate the IP range this pixel represents
            let pixel_first_ip = pixel_d << self.bits_per_pixel;
            let pixel_last_ip = pixel_first_ip + ips_per_pixel - 1;

            // Calculate overlap between CIDR block and this pixel's IP range
            let overlap_first = first_ip.max(pixel_first_ip);
            let overlap_last = last_ip.min(pixel_last_ip);

            if overlap_first <= overlap_last {
                let paint_value = match self.value_mode {
                    ValueMode::Categorical | ValueMode::Raw => value,
                    ValueMode::Scaled => {
                        let overlap_count = overlap_last - overlap_first + 1;
                        (value as f64 * overlap_count as f64 / ips_per_pixel as f64) as i32
                    }
                };

                if let Some((x, y)) = self.ip_to_xy(pixel_first_ip as u32) {
                    self.paint_pixel(x, y, paint_value);
                }
            }
        }

        Ok(())
    }

    fn calculate_domain(&self) -> Result<ScaleDomain, &'static str> {
        // Calculate overall min/max value if no value is provided.
        let min_value = match self.min_value {
            Some(v) => v,
            None => self
                .buffer
                .iter()
                .map(|row| row.iter().min().cloned().unwrap_or(0))
                .min()
                .unwrap_or(0) as f64,
        };

        // If max_value wasn't explicitly set, use the dataset maximum
        let max_value = match self.max_value {
            Some(v) => v,
            None => self
                .buffer
                .iter()
                .map(|row| row.iter().max().cloned().unwrap_or(0))
                .max()
                .unwrap_or(0) as f64,
        };

        log::debug!(
            "Colour scaling: curve={}, min={:?}, max={}",
            self.curve,
            min_value,
            max_value
        );

        ScaleDomain::new(self.curve, min_value, max_value)
    }

    fn process_input_from_reader<R: BufRead>(&mut self, reader: R) -> Result<()> {
        for (line_num, line) in reader.lines().enumerate() {
            let line = line.context("Failed to read line")?;
            let parts: Vec<&str> = line
                .split(|c: char| c == ',' || c.is_whitespace())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            if parts.is_empty() {
                continue;
            }

            let ip_str = parts[0];
            let value = if parts.len() > 1 {
                parts[1].parse::<i32>().unwrap_or(1)
            } else {
                1
            };

            // Check if this is a CIDR prefix
            if ip_str.contains('/') {
                match ip_str.parse::<Ipv4Net>() {
                    Ok(cidr) => {
                        self.paint_cidr_range(&cidr, value)?;
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to parse CIDR on line {}: {} - {}",
                            line_num + 1,
                            ip_str,
                            e
                        );
                        continue;
                    }
                }
            } else {
                // Process as individual IP
                let addr = if ip_str.chars().all(|c| c.is_ascii_digit()) {
                    let ip = ip_str.parse::<u32>().context("Invalid IP as integer")?;
                    Ipv4Addr::from(ip)
                } else {
                    Ipv4Addr::from_str(ip_str).context(format!(
                        "Invalid IP address on line {}: {}",
                        line_num + 1,
                        ip_str
                    ))?
                };

                self.paint_address(&addr, value)?;
            }
        }

        Ok(())
    }

    pub fn process_input_from_string(&mut self, input: &str) -> Result<()> {
        use std::io::Cursor;
        let cursor = Cursor::new(input);
        self.process_input_from_reader(cursor)
    }

    pub fn process_input(&mut self) -> Result<()> {
        let stdin = std::io::stdin();
        let reader = stdin.lock();
        self.process_input_from_reader(reader)
    }

    pub fn get_rgba_data(&self) -> Result<Vec<u8>> {
        let image_size = self.image_size();
        let size = (image_size * image_size * 4) as usize;
        let mut rgba_data = Vec::with_capacity(size);

        match self.value_mode {
            ValueMode::Categorical => {
                for y in 0..image_size {
                    for x in 0..image_size {
                        let value = self.buffer[y as usize][x as usize];
                        if value < 0 {
                            rgba_data.extend_from_slice(&[0, 0, 0, 0]);
                        } else {
                            let [r, g, b] = CATEGORICAL_PALETTE[value as usize % CATEGORICAL_PALETTE.len()];
                            rgba_data.extend_from_slice(&[r, g, b, 255]);
                        }
                    }
                }
            }
            ValueMode::Raw | ValueMode::Scaled => {
                let domain = self.calculate_domain().map_err(|e| anyhow!(e))?;
                for y in 0..image_size {
                    for x in 0..image_size {
                        let value = self.buffer[y as usize][x as usize];

                        if let Some(scaled) = domain.scale(value.into()) {
                            let (r, g, b) = self.colour_scale.eval_continuous(scaled).as_tuple();
                            rgba_data.extend_from_slice(&[r, g, b, 255]);
                        } else {
                            rgba_data.extend_from_slice(&[0, 0, 0, 0]);
                        }
                    }
                }
            }
        }

        Ok(rgba_data)
    }

    pub fn create_image(&self) -> Result<RgbaImage, &'static str> {
        let image_size = self.image_size();
        let mut image = ImageBuffer::from_pixel(image_size, image_size, Rgba([0, 0, 0, 0]));

        match self.value_mode {
            ValueMode::Categorical => {
                for y in 0..image_size {
                    for x in 0..image_size {
                        let value = self.buffer[y as usize][x as usize];
                        if value >= 0 {
                            let [r, g, b] = CATEGORICAL_PALETTE[value as usize % CATEGORICAL_PALETTE.len()];
                            image.put_pixel(x, y, Rgba([r, g, b, 255]));
                        }
                    }
                }
            }
            ValueMode::Raw | ValueMode::Scaled => {
                let domain = self.calculate_domain()?;
                for y in 0..image_size {
                    for x in 0..image_size {
                        let value = self.buffer[y as usize][x as usize];

                        if let Some(scaled) = domain.scale(value.into()) {
                            let (r, g, b) = self.colour_scale.eval_continuous(scaled).as_tuple();
                            image.put_pixel(x, y, Rgba([r, g, b, 255]));
                        }
                    }
                }
            }
        }

        Ok(image)
    }

    pub fn save(&self, filename: &str) -> Result<(), anyhow::Error> {
        let image = self.create_image().map_err(|err| anyhow!(err))?;
        image
            .save(filename)
            .context(format!("Failed to save image to {}", filename))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_size_for_bpp() {
        // bpp=8:  2^((32-8)/2)  = 2^12 = 4096
        assert_eq!(image_size_for_bpp(8), 4096);
        // bpp=10: 2^((32-10)/2) = 2^11 = 2048
        assert_eq!(image_size_for_bpp(10), 2048);
        // bpp=12: 2^((32-12)/2) = 2^10 = 1024
        assert_eq!(image_size_for_bpp(12), 1024);
        // bpp=16: 2^((32-16)/2) = 2^8  = 256
        assert_eq!(image_size_for_bpp(16), 256);
        // bpp=24: 2^((32-24)/2) = 2^4  = 16
        assert_eq!(image_size_for_bpp(24), 16);
        // bpp=32: 2^0 = 1
        assert_eq!(image_size_for_bpp(32), 1);
    }

    #[test]
    fn test_image_size_times_bpp_covers_ipv4_space() {
        // For every even bpp, total pixels * ips_per_pixel must equal 2^32
        for bpp in (8..=32).step_by(2) {
            let side = image_size_for_bpp(bpp) as u64;
            let total_pixels = side * side;
            let ips_per_pixel = 1u64 << bpp;
            assert_eq!(
                total_pixels * ips_per_pixel,
                1u64 << 32,
                "bpp={}: {}x{} * {} != 2^32",
                bpp, side, side, ips_per_pixel
            );
        }
    }

    fn make_heatmap(bits_per_pixel: u8) -> Heatmap {
        Heatmap::new(
            DomainType::Linear,
            None,
            None,
            true,
            bits_per_pixel,
            &colorous::MAGMA,
            ValueMode::Scaled,
        )
    }

    #[test]
    fn test_heatmap_buffer_matches_image_size() {
        for bpp in (8..=24).step_by(2) {
            let hm = make_heatmap(bpp);
            let expected = image_size_for_bpp(bpp) as usize;
            assert_eq!(hm.buffer.len(), expected, "bpp={}: wrong row count", bpp);
            assert_eq!(hm.buffer[0].len(), expected, "bpp={}: wrong col count", bpp);
            assert_eq!(hm.image_size(), expected as u32, "bpp={}: image_size() wrong", bpp);
        }
    }

    #[test]
    fn test_created_image_dimensions() {
        // Use bpp=24 (16x16) to keep the test fast
        let mut hm = make_heatmap(24);
        hm.process_input_from_string("10.0.0.0/8 1\n").unwrap();
        let img = hm.create_image().unwrap();
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn test_rgba_data_length() {
        let mut hm = make_heatmap(24);
        hm.process_input_from_string("10.0.0.0/8 1\n").unwrap();
        let data = hm.get_rgba_data().unwrap();
        // 16*16 pixels, 4 bytes each
        assert_eq!(data.len(), 16 * 16 * 4);
    }

    #[test]
    fn test_ip_maps_within_bounds_for_various_bpp() {
        for bpp in (8..=24).step_by(2) {
            let hm = make_heatmap(bpp);
            let max_coord = hm.image_size() - 1;

            // Test a few representative IPs
            let test_ips: [u32; 4] = [
                0,                  // 0.0.0.0
                0x40_00_00_00,      // 64.0.0.0
                0x80_00_00_00,      // 128.0.0.0
                0xFF_FF_FF_FF,      // 255.255.255.255
            ];

            for ip in test_ips {
                if let Some((x, y)) = hm.ip_to_xy(ip) {
                    assert!(
                        x <= max_coord && y <= max_coord,
                        "bpp={}: IP {} mapped to ({}, {}), max is {}",
                        bpp, ip, x, y, max_coord
                    );
                }
            }
        }
    }

    #[test]
    fn test_paint_address_various_bpp() {
        for bpp in [8, 12, 16, 24] {
            let mut hm = make_heatmap(bpp);
            // Painting should not panic for any bpp
            hm.paint_address(&Ipv4Addr::new(10, 0, 0, 1), 1).unwrap();
            hm.paint_address(&Ipv4Addr::new(192, 168, 1, 1), 5).unwrap();
            hm.paint_address(&Ipv4Addr::new(255, 255, 255, 255), 1).unwrap();
        }
    }

    #[test]
    fn test_paint_cidr_various_bpp() {
        for bpp in [8, 12, 16, 24] {
            let mut hm = make_heatmap(bpp);
            let cidr: Ipv4Net = "10.0.0.0/8".parse().unwrap();
            hm.paint_cidr_range(&cidr, 1).unwrap();
        }
    }

    #[test]
    fn test_full_pipeline_small_bpp() {
        // bpp=24 gives a tiny 16x16 image; run the full pipeline end-to-end
        let mut hm = make_heatmap(24);
        hm.process_input_from_string("10.0.0.0/8 1\n192.168.0.0/16 5\n").unwrap();
        let img = hm.create_image().unwrap();
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn test_bits_per_pixel_influences_image_size() {
        let input = "10.0.0.0/8 1\n";
        let mut prev_size = None;
        for bpp in (8..=24).step_by(2) {
            let mut hm = make_heatmap(bpp);
            hm.process_input_from_string(input).unwrap();
            let img = hm.create_image().unwrap();
            let size = img.width();
            assert_eq!(size, img.height(), "image should be square at bpp={}", bpp);
            assert_eq!(size, image_size_for_bpp(bpp), "image dimensions should match image_size_for_bpp at bpp={}", bpp);
            if let Some(prev) = prev_size {
                assert!(size < prev, "higher bpp should produce a smaller image: bpp={} gave {}x{}, previous gave {}x{}", bpp, size, size, prev, prev);
            }
            prev_size = Some(size);
        }
    }
}