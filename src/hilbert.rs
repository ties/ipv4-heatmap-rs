pub fn hilbert_d2xy(d: u64, order: u32) -> Option<(u32, u32)> {
    if order == 0 {
        return Some((0, 0));
    }
    
    let n = 1u32 << order;
    let mut x = 0u32;
    let mut y = 0u32;
    let mut t = d;
    
    let mut s = 1u32;
    while s < n {
        let rx = 1 & (t >> 1);
        let ry = 1 & (t ^ rx);
        
        if ry == 0 {
            if rx == 1 {
                x = s - 1 - x;
                y = s - 1 - y;
            }
            
            // Swap x and y
            let temp = x;
            x = y;
            y = temp;
        }
        
        x += s * rx as u32;
        y += s * ry as u32;
        t >>= 2;
        s <<= 1;
    }
    
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hilbert_quadrant_mapping() {
        // Test the quadrant mapping for IPv4 space
        // With bits_per_pixel = 8, we have order = (32-8)/2 = 12
        // The image is 4096x4096 (2^12 x 2^12)
        
        let order = 12;
        
        // Test the four main /2 networks:
        // 0.0.0.0/2 should be top-left quadrant
        let d_00 = 0u64 >> 8; // 0.0.0.0 shifted right by 8 bits
        let (x_00, y_00) = hilbert_d2xy(d_00, order).unwrap();
        
        // 64.0.0.0/2 should be lower-left quadrant  
        let d_64 = (64u64 << 24) >> 8; // 64.0.0.0 shifted right by 8 bits
        let (x_64, y_64) = hilbert_d2xy(d_64, order).unwrap();
        
        // 128.0.0.0/2 should be lower-right quadrant
        let d_128 = (128u64 << 24) >> 8; // 128.0.0.0 shifted right by 8 bits
        let (x_128, y_128) = hilbert_d2xy(d_128, order).unwrap();
        
        // 192.0.0.0/2 should be top-right quadrant
        let d_192 = (192u64 << 24) >> 8; // 192.0.0.0 shifted right by 8 bits
        let (x_192, y_192) = hilbert_d2xy(d_192, order).unwrap();
        
        println!("0.0.0.0/2 -> ({}, {})", x_00, y_00);
        println!("64.0.0.0/2 -> ({}, {})", x_64, y_64);
        println!("128.0.0.0/2 -> ({}, {})", x_128, y_128);
        println!("192.0.0.0/2 -> ({}, {})", x_192, y_192);
        
        // Expected quadrants (assuming 4096x4096 image):
        // Top-left: x < 2048, y < 2048
        // Top-right: x >= 2048, y < 2048
        // Lower-left: x < 2048, y >= 2048
        // Lower-right: x >= 2048, y >= 2048
        
        let mid = 2048u32;
        
        // 0.0.0.0/2 should be top-left
        assert!(x_00 < mid && y_00 < mid, "0.0.0.0/2 should be in top-left quadrant, got ({}, {})", x_00, y_00);
        
        // 64.0.0.0/2 should be lower-left
        assert!(x_64 < mid && y_64 >= mid, "64.0.0.0/2 should be in lower-left quadrant, got ({}, {})", x_64, y_64);
        
        // 128.0.0.0/2 should be lower-right
        assert!(x_128 >= mid && y_128 >= mid, "128.0.0.0/2 should be in lower-right quadrant, got ({}, {})", x_128, y_128);
        
        // 192.0.0.0/2 should be top-right
        assert!(x_192 >= mid && y_192 < mid, "192.0.0.0/2 should be in top-right quadrant, got ({}, {})", x_192, y_192);
    }
    
    #[test]
    fn test_240_0_0_0_slash_4() {
        // 240.0.0.0/4 should be in the upper half of the top-right quadrant
        let order = 12;
        let d_240 = (240u64 << 24) >> 8; // 240.0.0.0 shifted right by 8 bits
        let (x_240, y_240) = hilbert_d2xy(d_240, order).unwrap();
        
        println!("240.0.0.0/4 -> ({}, {})", x_240, y_240);
        
        // Should be in top-right quadrant (x >= 2048, y < 2048)
        // And in upper half of that quadrant (y < 1024)
        let mid = 2048u32;
        let quarter = 1024u32;
        
        assert!(x_240 >= mid, "240.0.0.0/4 should be in right half, got x={}", x_240);
        assert!(y_240 < quarter, "240.0.0.0/4 should be in upper quarter, got y={}", y_240);
    }

    #[test]
    fn test_hilbert_order_consistency() {
        // Test that the function works with different orders
        for order in 1..=12 {
            let max_d = (1u64 << (2 * order)) - 1;
            let (x, y) = hilbert_d2xy(max_d, order).unwrap();
            let max_coord = (1u32 << order) - 1;
            assert!(x <= max_coord, "x coordinate {} exceeds max {} for order {}", x, max_coord, order);
            assert!(y <= max_coord, "y coordinate {} exceeds max {} for order {}", y, max_coord, order);
        }
    }

    #[test]
    fn test_specific_ip_mappings() {
        let order = 12;
        
        // Test some specific IP addresses to verify they map to expected quadrants
        let test_cases = [
            (0u32, "0.0.0.0", "top-left"),
            (64u32 << 24, "64.0.0.0", "lower-left"), 
            (128u32 << 24, "128.0.0.0", "lower-right"),
            (192u32 << 24, "192.0.0.0", "top-right"),
            (240u32 << 24, "240.0.0.0", "top-right"),
        ];
        
        for (ip, ip_str, expected_quadrant) in test_cases {
            let d = (ip as u64) >> 8;
            let (x, y) = hilbert_d2xy(d, order).unwrap();
            let mid = 2048u32;
            
            let actual_quadrant = match (x >= mid, y >= mid) {
                (false, false) => "top-left",
                (true, false) => "top-right", 
                (false, true) => "lower-left",
                (true, true) => "lower-right",
            };
            
            println!("{} -> ({}, {}) in {}", ip_str, x, y, actual_quadrant);
            assert_eq!(actual_quadrant, expected_quadrant, 
                "{} should be in {} quadrant, got {}", ip_str, expected_quadrant, actual_quadrant);
        }
    }
}