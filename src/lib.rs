use anyhow::{Context, Result, anyhow};
use colorous::Gradient;
use image::{ImageBuffer, RgbaImage, Rgba};
use std::io::BufRead;
use std::net::Ipv4Addr;
use std::str::FromStr;

mod hilbert;
mod scale;

use hilbert::hilbert_d2xy;
use ipnet::Ipv4Net;
use scale::ScaleDomain;

// Re-export types for public API
pub use scale::DomainType;

const IMAGE_SIZE: u32 = 4096;

pub struct Heatmap {
    buffer: Vec<Vec<i32>>,
    curve: scale::DomainType,
    min_value: Option<f64>,
    max_value: Option<f64>,
    accumulate: bool,
    bits_per_pixel: u8,
    colour_scale: &'static Gradient,
}

impl Heatmap {
    pub fn new(
        curve: scale::DomainType,
        min_value: Option<f64>,
        max_value: Option<f64>,
        accumulate: bool,
        bits_per_pixel: u8,
        colour_scale: &'static Gradient,
    ) -> Self {
        let buffer = vec![vec![0i32; IMAGE_SIZE as usize]; IMAGE_SIZE as usize];

        Self {
            buffer,
            curve,
            min_value,
            max_value,
            accumulate,
            bits_per_pixel,
            colour_scale,
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

    fn paint_cidr_range(&mut self, cidr: &Ipv4Net, value: i32) -> Result<()> {
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
                // Calculate how many IPs from the CIDR block overlap with this pixel
                let overlap_count = overlap_last - overlap_first + 1;

                // Scale the value by the proportion of IPs in this pixel that come from the CIDR block
                let scaled_value =
                    (value as f64 * overlap_count as f64 / ips_per_pixel as f64) as i32;

                if let Some((x, y)) = self.ip_to_xy(pixel_first_ip as u32) {
                    self.paint_pixel(x, y, scaled_value);
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

    pub fn process_input(&mut self) -> Result<()> {
        let stdin = std::io::stdin();
        let reader = stdin.lock();

        for (line_num, line) in reader.lines().enumerate() {
            let line = line.context("Failed to read line")?;
            let parts: Vec<&str> = line.split_whitespace().collect();

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

    fn create_image(&self) -> Result<RgbaImage, &'static str> {
        let mut image = ImageBuffer::from_pixel(IMAGE_SIZE, IMAGE_SIZE, Rgba([0, 0, 0, 0]));
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

        Ok(image)
    }

    pub fn save(&self, filename: &str) -> Result<(), anyhow::Error> {
        let image = self.create_image().map_err(|err| anyhow!(err))?;
        image
            .save(filename)
            .context(format!("Failed to save image to {}", filename))
    }
}