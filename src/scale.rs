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
            _ => Err(format!(
                "Invalid curve type: {}. Use 'linear' or 'logarithmic'",
                s
            )),
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
    pub fn new(
        domain_type: DomainType,
        min_value: f64,
        max_value: f64,
    ) -> Result<Self, &'static str> {
        if max_value <= min_value {
            return Err(
                "Min value must be greater than 0 and max value must be greater than min value",
            );
        }
        Ok(Self {
            domain_type,
            min_value,
            max_value,
        })
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
            Some(1.0)
        } else {
            Some((value - self.min_value) / (self.max_value - self.min_value))
        }
    }

    /// Uses log1p-style scaling: `ln(offset + 1) / ln(range + 1)` where
    /// `offset = value - min`. This ensures that values just above `min_value`
    /// map to near-zero rather than negative infinity, without requiring
    /// `min_value > 0`.
    pub fn scale_logarithmic(&self, value: f64) -> Option<f64> {
        if value <= self.min_value {
            return None;
        }
        if value >= self.max_value {
            return Some(1.0);
        }

        let offset = value - self.min_value;
        let range = self.max_value - self.min_value;
        Some((offset + 1.0).ln() / (range + 1.0).ln())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_scale_out_of_range_bottom() {
        let domain = ScaleDomain::new(DomainType::Linear, 10.0, 100.0).unwrap();
        
        // Values at or below min_value should map to None (no data)
        assert_eq!(domain.scale_linear(10.0), None);
        assert_eq!(domain.scale_linear(-5.0), None);
    }

    #[test]
    fn test_linear_scale_interpolation() {
        let domain = ScaleDomain::new(DomainType::Linear, 10.0, 100.0).unwrap();
        
        // Test linear interpolation
        assert_eq!(domain.scale_linear(55.0), Some(0.5)); // Midpoint
        assert_eq!(domain.scale_linear(37.0), Some(0.3)); // 30% of range
        assert_eq!(domain.scale_linear(82.0), Some(0.8)); // 80% of range
        
        // Values at max_value should map to 1.0
        assert_eq!(domain.scale_linear(100.0), Some(1.0));
        assert_eq!(domain.scale_linear(150.0), Some(1.0)); // Above max
    }

    #[test]
    fn test_linear_scale_edge_cases() {
        let domain = ScaleDomain::new(DomainType::Linear, 0.0, 1.0).unwrap();
        
        // Just above min_value should give very small positive value
        assert!(domain.scale_linear(0.1).unwrap() > 0.0);
        assert!(domain.scale_linear(0.1).unwrap() < 0.2);
        
        // Test with very small range
        let small_domain = ScaleDomain::new(DomainType::Linear, 1.0, 1.1).unwrap();
        assert_eq!(small_domain.scale_linear(1.05), Some(0.5));
    }

    #[test]
    fn test_logarithmic_scale_min_max() {
        let domain = ScaleDomain::new(DomainType::Logarithmic, 1.0, 100.0).unwrap();

        // Min value should return None (no data)
        assert_eq!(domain.scale_logarithmic(1.0), None);

        // Max value should clamp to 1.0
        assert_eq!(domain.scale_logarithmic(100.0), Some(1.0));
        assert_eq!(domain.scale_logarithmic(200.0), Some(1.0));

        // Value just above min_value should be a small positive number
        let just_above_min = domain.scale_logarithmic(1.1).unwrap();
        assert!(just_above_min.is_finite());
        assert!(just_above_min > 0.0);
        assert!(just_above_min < 0.1);
    }

    #[test]
    fn test_logarithmic_scale_out_of_range() {
        let domain = ScaleDomain::new(DomainType::Logarithmic, 10.0, 1000.0).unwrap();

        // Values at or below min_value should map to None
        assert_eq!(domain.scale_logarithmic(10.0), None);
        assert_eq!(domain.scale_logarithmic(5.0), None);
        assert_eq!(domain.scale_logarithmic(9.99), None);
        assert_eq!(domain.scale_logarithmic(0.0), None);
    }

    #[test]
    fn test_logarithmic_scale_interpolation() {
        let domain = ScaleDomain::new(DomainType::Logarithmic, 1.0, 1000.0).unwrap();

        // ln(10-1+1) / ln(1000-1+1) = ln(10) / ln(1000) = 1/3
        let result_10 = domain.scale_logarithmic(10.0).unwrap();
        assert!((result_10 - 1.0 / 3.0).abs() < 1e-10);

        // ln(100-1+1) / ln(1000-1+1) = ln(100) / ln(1000) = 2/3
        let result_100 = domain.scale_logarithmic(100.0).unwrap();
        assert!((result_100 - 2.0 / 3.0).abs() < 1e-10);

        // Values should increase with input
        assert!(result_10 < result_100);

        // Output should be in [0, 1]
        assert!(result_10 > 0.0 && result_10 < 1.0);
        assert!(result_100 > 0.0 && result_100 < 1.0);
    }

    #[test]
    fn test_scale_domain_creation_validation() {
        // Base cases
        assert!(ScaleDomain::new(DomainType::Linear, 0.0, 10.0).is_ok());
        assert!(ScaleDomain::new(DomainType::Linear, -10.0, 10.0).is_ok());
        assert!(ScaleDomain::new(DomainType::Logarithmic, 0.0, 100.0).is_ok());
        
        // Max value must be > min value
        assert!(ScaleDomain::new(DomainType::Linear, 10.0, 5.0).is_err());
        assert!(ScaleDomain::new(DomainType::Logarithmic, 10.0, 5.0).is_err());
        
    }

    #[test]
    fn test_scale_method_dispatch() {
        let linear_domain = ScaleDomain::new(DomainType::Linear, 10.0, 100.0).unwrap();
        let log_domain = ScaleDomain::new(DomainType::Logarithmic, 10.0, 100.0).unwrap();
        
        // Both should handle the same input differently
        let linear_result = linear_domain.scale(55.0).unwrap();
        let log_result = log_domain.scale(55.0).unwrap();
        
        assert_ne!(linear_result, log_result);
        
        // Out of range should return None for both
        assert_eq!(linear_domain.scale(5.0), None);
        assert_eq!(log_domain.scale(5.0), None);
    }
}
