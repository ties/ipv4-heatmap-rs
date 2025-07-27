use std::fmt::Display;
use std::str::FromStr;

#[derive(Clone, Copy, Debug)]
pub enum DomainType {
    Linear,
    Logarithmic,
}

impl FromStr for DomainType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "linear" => Ok(DomainType::Linear),
            "logarithmic" | "log" => Ok(DomainType::Logarithmic),
            _ => Err(format!("Invalid curve type: {}. Use 'linear' or 'logarithmic'", s)),
        }
    }
}

impl Display for DomainType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DomainType::Linear => write!(f, "linear"),
            DomainType::Logarithmic => write!(f, "log"),
        }
    }
}

pub struct ScaleDomain {
    domain_type: DomainType,
    min_value: f64,
    max_value: f64,
}

impl ScaleDomain {
    pub fn new(domain_type: DomainType, min_value: f64, max_value: f64) -> Result<Self, &'static str> {
        if min_value < 0.0 || max_value <= min_value {
            return Err("Min value must be greater than 0 and max value must be greater than min value");
        }
        Ok(Self { domain_type, min_value, max_value })
    }

    pub fn scale(&self, value: f64) -> Option<f64> {
        match self.domain_type {
            DomainType::Linear => self.scale_linear(value),
            DomainType::Logarithmic => self.scale_logarithmic(value),
        }
    }

    pub fn scale_linear(&self, value: f64) -> Option<f64> {
        if value <= self.min_value {
            None
        } else if value >= self.max_value {
            Some(self.max_value - self.min_value)
        } else {
            Some((value - self.min_value) / (self.max_value - self.min_value))
        }
    }

    pub fn scale_logarithmic(&self, value: f64) -> Option<f64> {
        if value < self.min_value {
            return None;
        }

        let offset = value - self.min_value;
        Some(offset.ln() / self.max_value.ln())
    }
}