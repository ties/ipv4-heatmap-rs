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

const IMAGE_SIZE: u32 = 4096;

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
    categorical: bool,
}

impl Heatmap {
    pub fn new(
        curve: scale::DomainType,
        min_value: Option<f64>,
        max_value: Option<f64>,
        accumulate: bool,
        bits_per_pixel: u8,
        colour_scale: &'static Gradient,
        categorical: bool,
    ) -> Self {
        // Use -1 as sentinel for "no data" in categorical mode
        let init_value = if categorical { -1 } else { 0 };
        let buffer = vec![vec![init_value; IMAGE_SIZE as usize]; IMAGE_SIZE as usize];

        Self {
            buffer,
            curve,
            min_value,
            max_value,
            accumulate,
            bits_per_pixel,
            colour_scale,
            categorical,
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
        let ips_per_pixel = 1u64 << self.bits_per_pixel;

        let paint_value = if self.categorical {
            value
        } else {
            (value as f64 / ips_per_pixel as f64) as i32
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
                let paint_value = if self.categorical {
                    // In categorical mode, paint the raw category value (no scaling)
                    value
                } else {
                    // Calculate how many IPs from the CIDR block overlap with this pixel
                    let overlap_count = overlap_last - overlap_first + 1;

                    // Scale the value by the proportion of IPs in this pixel that come from the CIDR block
                    (value as f64 * overlap_count as f64 / ips_per_pixel as f64) as i32
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
                let ip = if ip_str.chars().all(|c| c.is_ascii_digit()) {
                    ip_str.parse::<u32>().context("Invalid IP as integer")?
                } else {
                    let addr = Ipv4Addr::from_str(ip_str).context(format!(
                        "Invalid IP address on line {}: {}",
                        line_num + 1,
                        ip_str
                    ))?;
                    u32::from(addr)
                };

                if let Some((x, y)) = self.ip_to_xy(ip) {
                    self.paint_pixel(x, y, value);
                }
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
        let size = (IMAGE_SIZE * IMAGE_SIZE * 4) as usize;
        let mut rgba_data = Vec::with_capacity(size);

        if self.categorical {
            for y in 0..IMAGE_SIZE {
                for x in 0..IMAGE_SIZE {
                    let value = self.buffer[y as usize][x as usize];
                    if value < 0 {
                        rgba_data.extend_from_slice(&[0, 0, 0, 0]);
                    } else {
                        let [r, g, b] = CATEGORICAL_PALETTE[value as usize % CATEGORICAL_PALETTE.len()];
                        rgba_data.extend_from_slice(&[r, g, b, 255]);
                    }
                }
            }
        } else {
            let domain = self.calculate_domain().map_err(|e| anyhow!(e))?;
            for y in 0..IMAGE_SIZE {
                for x in 0..IMAGE_SIZE {
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

        Ok(rgba_data)
    }

    fn create_image(&self) -> Result<RgbaImage, &'static str> {
        let mut image = ImageBuffer::from_pixel(IMAGE_SIZE, IMAGE_SIZE, Rgba([0, 0, 0, 0]));

        if self.categorical {
            for y in 0..IMAGE_SIZE {
                for x in 0..IMAGE_SIZE {
                    let value = self.buffer[y as usize][x as usize];
                    if value >= 0 {
                        let [r, g, b] = CATEGORICAL_PALETTE[value as usize % CATEGORICAL_PALETTE.len()];
                        image.put_pixel(x, y, Rgba([r, g, b, 255]));
                    }
                }
            }
        } else {
            let domain = self.calculate_domain()?;
            for y in 0..IMAGE_SIZE {
                for x in 0..IMAGE_SIZE {
                    let value = self.buffer[y as usize][x as usize];

                    if let Some(scaled) = domain.scale(value.into()) {
                        let (r, g, b) = self.colour_scale.eval_continuous(scaled).as_tuple();
                        image.put_pixel(x, y, Rgba([r, g, b, 255]));
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