# IPv4 Heatmap Web Application

A WebAssembly-powered web application for visualizing AS network prefixes using Hilbert curve heatmaps.

## Features

- Real-time visualization of AS prefixes from RIPEstat API
- Multiple color scales (Magma, Inferno, Plasma, Viridis, Cividis, Turbo, Warm, Cool)
- Linear and logarithmic curve scaling
- Configurable bits-per-pixel resolution
- Download generated heatmaps as PNG
- 4096x4096 high-resolution output
- Zero dependencies - pure vanilla JavaScript

## Usage

### 1. Build the WASM Module

From the project root directory:

```bash
wasm-pack build --target web --out-dir www/pkg
```

For production builds:

```bash
wasm-pack build --target web --release --out-dir www/pkg
```

### 2. Start Local Development Server

```bash
python3 -m http.server 8080 -d www
```

Or using Node.js:

```bash
npx http-server www -p 8080
```

### 3. Open in Browser

Navigate to `http://localhost:8080`

### 4. Generate a Heatmap

1. Enter an AS number (e.g., `3333` or `AS3333`)
2. Select your preferred color scale and curve type
3. Click "Generate Heatmap"
4. Wait for the visualization to render
5. Optionally download the result as PNG

## How It Works

### Architecture

1. **Frontend (Vanilla JS)**: Fetches AS prefix data from RIPEstat API
2. **WASM Module**: Processes CIDR blocks and generates RGBA pixel data using Hilbert curve mapping
3. **Canvas Rendering**: Displays the 4096x4096 heatmap directly in browser

### Data Flow

```
User Input (AS Number)
  ↓
RIPEstat API (Fetch Prefixes)
  ↓
WASM Module (Process + Generate RGBA)
  ↓
Canvas ImageData (Render)
  ↓
PNG Download (Optional)
```

### RIPEstat API

Endpoint: `https://stat.ripe.net/data/announced-prefixes/data.json?resource=AS{asn}`

Returns:
```json
{
  "data": {
    "prefixes": [
      {"prefix": "193.0.0.0/21"},
      {"prefix": "193.0.10.0/23"}
    ]
  }
}
```

## Examples

### Small AS Networks
- **AS3333** (RIPE NCC): ~7 prefixes
- **AS15169** (Google): ~500 prefixes

### Large AS Networks
- **AS16509** (Amazon): ~13,000+ prefixes
- **AS8075** (Microsoft): ~8,000+ prefixes

## Performance

- **WASM Bundle Size**: ~157KB (uncompressed)
- **Image Size**: 4096x4096 pixels (67MB RGBA data)
- **Generation Time**: <1 second for typical AS networks
- **Memory Usage**: ~68MB for image buffer

## Browser Compatibility

- Chrome/Edge: ✅ Full support
- Firefox: ✅ Full support
- Safari: ✅ Full support
- Requires: WebAssembly, Canvas API, ES6 modules

## Troubleshooting

### WASM Module Not Loading

**Error**: "Failed to load WASM module"

**Solution**: Make sure you've built the WASM module:
```bash
wasm-pack build --target web --out-dir www/pkg
```

### CORS Errors

**Error**: "Cross-Origin Request Blocked"

**Solution**: Use a local HTTP server, not `file://` protocol:
```bash
python3 -m http.server 8080 -d www
```

### RIPEstat API Errors

**Error**: "No prefixes found for AS..."

**Solutions**:
- Check AS number is valid
- Try with well-known AS (e.g., AS3333)
- Check network connectivity

## Configuration Options

### Curve Type
- **Linear**: Direct mapping of values to colors
- **Logarithmic**: Better visualization for data with large value ranges

### Color Scales
- **Magma**: Black → Purple → Yellow (default)
- **Viridis**: Blue → Green → Yellow (perceptually uniform)
- **Inferno**: Black → Red → Yellow
- **Plasma**: Purple → Pink → Yellow
- **Cividis**: Blue → Yellow (colorblind-friendly)
- **Turbo**: Rainbow with improved perceptual uniformity
- **Warm**: Yellow → Orange → Red
- **Cool**: Cyan → Blue → Magenta

### Bits per Pixel
- **8**: Minimum value for 4096×4096 image (each pixel = 256 IPs = 2^8)
- **8-24**: Valid range - higher values mean more IPs per pixel (more aggregation)
- **Calculation**: For a 4096×4096 (2^12×2^12) image covering 2^32 IPv4 addresses, minimum bits_per_pixel = 32 - 24 = 8

**Why minimum 8?**
- 4096×4096 = 2^24 total pixels
- 2^32 total IPv4 addresses
- Each pixel must cover at least 2^32 ÷ 2^24 = 2^8 = 256 addresses

### Accumulate
- **Enabled**: Overlapping ranges add values
- **Disabled**: Overlapping ranges overwrite values

## License

This project is licensed under GPL-2.0 (same as parent project).

## Credits

- Built with [wasm-pack](https://rustwasm.github.io/wasm-pack/)
- Data from [RIPEstat](https://stat.ripe.net/)
- Color scales from [colorous](https://github.com/dtolnay/colorous)
