// Load WASM module
let wasmModule = null;
let wasmReady = false;

const elements = {
    asNumberInput: document.getElementById('as-number'),
    curveTypeSelect: document.getElementById('curve-type'),
    colourScaleSelect: document.getElementById('colour-scale'),
    bitsPerPixelInput: document.getElementById('bits-per-pixel'),
    accumulateCheckbox: document.getElementById('accumulate'),
    valueModeSelect: document.getElementById('value-mode'),
    generateBtn: document.getElementById('generate-btn'),
    downloadBtn: document.getElementById('download-btn'),
    canvas: document.getElementById('heatmap-canvas'),
    canvasContainer: document.getElementById('canvas-container'),
    loadingDiv: document.getElementById('loading'),
    processingDiv: document.getElementById('processing'),
    errorDiv: document.getElementById('error'),
    infoDiv: document.getElementById('info'),
    dataSourceRadios: document.querySelectorAll('input[name="data-source"]'),
    asnInputGroup: document.getElementById('asn-input-group'),
    dataInputGroup: document.getElementById('data-input-group'),
    dataFile: document.getElementById('data-file'),
    dataTextarea: document.getElementById('data-textarea'),
    separatorSelect: document.getElementById('separator'),
};

async function initWasm() {
    try {
        showLoading(true);
        hideError();

        const { default: init, generate_heatmap, get_image_size } = await import('./pkg/ip_heatmap.js');
        await init();

        wasmModule = { generate_heatmap, get_image_size };
        wasmReady = true;

        showLoading(false);
        const defaultBpp = parseInt(elements.bitsPerPixelInput.value, 10);
        showInfo(`WASM module loaded. Image size: ${wasmModule.get_image_size(defaultBpp)}x${wasmModule.get_image_size(defaultBpp)}`);

        console.log('WASM module initialized successfully');
    } catch (error) {
        showLoading(false);
        showError(`Failed to load WASM module: ${error.message}. Make sure to build with 'wasm-pack build --target web --out-dir www/pkg'`);
        console.error('WASM initialization error:', error);
    }
}

async function fetchASPrefixes(asn) {
    // Remove 'AS' prefix if present
    const asnNumber = asn.replace(/^AS/i, '');

    const url = `https://stat.ripe.net/data/announced-prefixes/data.json?resource=AS${asnNumber}`;

    try {
        const response = await fetch(url);
        if (!response.ok) {
            throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }

        const data = await response.json();

        if (!data.data || !data.data.prefixes) {
            throw new Error('Invalid response format from RIPEstat API');
        }

        const prefixes = data.data.prefixes;

        if (prefixes.length === 0) {
            throw new Error(`No prefixes found for AS${asnNumber}`);
        }

        // Extract IPv4 prefixes only
        const ipv4Prefixes = prefixes
            .map(p => p.prefix)
            .filter(prefix => !prefix.includes(':')) // Filter out IPv6
            .join('\n');

        if (!ipv4Prefixes) {
            throw new Error(`No IPv4 prefixes found for AS${asnNumber}`);
        }

        return {
            prefixes: ipv4Prefixes,
            count: prefixes.filter(p => !p.prefix.includes(':')).length,
            asn: asnNumber
        };
    } catch (error) {
        if (error.name === 'TypeError' && error.message.includes('fetch')) {
            throw new Error('Network error: Unable to reach RIPEstat API. Check your internet connection.');
        }
        throw error;
    }
}

function getSelectedDataSource() {
    for (const radio of elements.dataSourceRadios) {
        if (radio.checked) return radio.value;
    }
    return 'asn';
}

function readFileAsText(file) {
    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result);
        reader.onerror = () => reject(new Error('Failed to read file'));
        reader.readAsText(file);
    });
}

async function generateHeatmap() {
    if (!wasmReady) {
        showError('WASM module not loaded yet. Please wait...');
        return;
    }

    hideError();
    hideInfo();
    showProcessing(true);
    elements.canvasContainer.classList.add('hidden');

    try {
        const curveType = elements.curveTypeSelect.value;
        const colourScale = elements.colourScaleSelect.value;
        const bitsPerPixel = parseInt(elements.bitsPerPixelInput.value, 10);
        const accumulate = elements.accumulateCheckbox.checked;
        const valueMode = elements.valueModeSelect.value;
        const dataSource = getSelectedDataSource();
        const separatorValue = elements.separatorSelect.value;
        const separator = (dataSource === 'data' && separatorValue !== 'auto') ? separatorValue : null;

        let inputData;
        let infoLabel;

        if (dataSource === 'asn') {
            const asNumber = elements.asNumberInput.value.trim();
            if (!asNumber) {
                throw new Error('Please enter an AS number');
            }

            showInfo('Fetching prefixes from RIPEstat API...');
            const { prefixes, count, asn } = await fetchASPrefixes(asNumber);
            inputData = prefixes;
            infoLabel = `AS${asn} (${count} prefixes)`;
            showInfo(`Fetched ${count} IPv4 prefixes for AS${asn}. Generating heatmap...`);
        } else {
            const file = elements.dataFile.files[0];
            const textareaValue = elements.dataTextarea.value.trim();

            if (file) {
                inputData = await readFileAsText(file);
                infoLabel = file.name;
            } else if (textareaValue) {
                inputData = textareaValue;
                infoLabel = 'pasted data';
            } else {
                throw new Error('Please upload a file or paste data');
            }

            showInfo('Generating heatmap...');
        }

        // Generate heatmap using WASM
        const rgbaData = wasmModule.generate_heatmap(
            inputData,
            curveType,
            null, // min_value
            null, // max_value
            accumulate,
            bitsPerPixel,
            colourScale,
            valueMode,
            separator
        );

        // Render to canvas
        renderToCanvas(rgbaData);

        showProcessing(false);
        elements.canvasContainer.classList.remove('hidden');
        showInfo(`Successfully generated heatmap for ${infoLabel}`);

    } catch (error) {
        showProcessing(false);
        showError(error.message);
        console.error('Error generating heatmap:', error);
    }
}

function renderToCanvas(rgbaData) {
    const bitsPerPixel = parseInt(elements.bitsPerPixelInput.value, 10);
    const imageSize = wasmModule.get_image_size(bitsPerPixel);

    // Resize canvas to match the image dimensions
    elements.canvas.width = imageSize;
    elements.canvas.height = imageSize;

    const ctx = elements.canvas.getContext('2d');

    // Create ImageData from the RGBA byte array
    const imageData = new ImageData(
        new Uint8ClampedArray(rgbaData),
        imageSize,
        imageSize
    );

    // Draw to canvas
    ctx.putImageData(imageData, 0, 0);
}

function downloadPNG() {
    try {
        elements.canvas.toBlob((blob) => {
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            const asNumber = elements.asNumberInput.value.trim().replace(/^AS/i, '');
            const label = getSelectedDataSource() === 'asn' ? `AS${asNumber}` : 'custom';
            a.href = url;
            a.download = `heatmap-${label}-${Date.now()}.png`;
            document.body.appendChild(a);
            a.click();
            document.body.removeChild(a);
            URL.revokeObjectURL(url);
        }, 'image/png');
    } catch (error) {
        showError(`Failed to download PNG: ${error.message}`);
    }
}

function showLoading(show) {
    elements.loadingDiv.classList.toggle('hidden', !show);
    elements.generateBtn.disabled = show;
}

function showProcessing(show) {
    elements.processingDiv.classList.toggle('hidden', !show);
    elements.generateBtn.disabled = show;
}

function showError(message) {
    elements.errorDiv.textContent = message;
    elements.errorDiv.classList.remove('hidden');
}

function hideError() {
    elements.errorDiv.classList.add('hidden');
}

function showInfo(message) {
    elements.infoDiv.textContent = message;
    elements.infoDiv.classList.remove('hidden');
}

function hideInfo() {
    elements.infoDiv.classList.add('hidden');
}

// Data source toggle
function updateDataSourceVisibility() {
    const source = getSelectedDataSource();
    elements.asnInputGroup.classList.toggle('hidden', source !== 'asn');
    elements.dataInputGroup.classList.toggle('hidden', source !== 'data');
}

elements.dataSourceRadios.forEach(radio => {
    radio.addEventListener('change', updateDataSourceVisibility);
});

// Clear file input when textarea gets content and vice versa
elements.dataTextarea.addEventListener('input', () => {
    if (elements.dataTextarea.value.trim()) {
        elements.dataFile.value = '';
    }
});
elements.dataFile.addEventListener('change', () => {
    if (elements.dataFile.files[0]) {
        elements.dataTextarea.value = '';
    }
});

// Event listeners
elements.generateBtn.addEventListener('click', generateHeatmap);
elements.downloadBtn.addEventListener('click', downloadPNG);

// Allow Enter key to trigger generation
elements.asNumberInput.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') {
        generateHeatmap();
    }
});

// Initialize WASM on page load
initWasm();
