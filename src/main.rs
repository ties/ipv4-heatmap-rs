use anyhow::Result;
use clap::{Parser, ValueEnum};
use ip_heatmap::{Heatmap, DomainType, ValueMode};

#[derive(Clone, Debug, ValueEnum)]
pub enum ColourScale {
    Accessible,
    Cividis,
    Magma,
}

#[derive(Parser)]
#[command(name = "ip-heatmap")]
#[command(about = "Generate Hilbert curve heatmaps of the IPv4 address space")]
#[command(version = "0.1.0")]
pub struct Args {
    #[arg(
        long,
        help = "Colour curve type: linear or logarithmic",
        default_value = "linear"
    )]
    curve: DomainType,

    #[arg(long, help = "Minimum value for colour scaling (defaults to 0)")]
    min_value: Option<f64>,

    #[arg(
        long,
        help = "Maximum value for colour scaling (defaults to dataset maximum)"
    )]
    max_value: Option<f64>,

    #[arg(
        short = 'A',
        help = "Logarithmic scaling, min value (deprecated: use --min-value)"
    )]
    log_min: Option<f64>,

    #[arg(
        short = 'B',
        help = "Logarithmic scaling, max value (deprecated: use --max-value)"
    )]
    log_max: Option<f64>,

    #[arg(long, short = 'C', help = "Values accumulate in exact input mode")]
    accumulate: bool,

    #[arg(short = 'v', long = "verbose", help = "Verbose output (-v for debug, -vv for trace)", action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(help = "Output filename")]
    output: String,

    #[arg(
        short = 'z',
        help = "Address space bits per pixel",
        default_value = "8"
    )]
    bits_per_pixel: u8,

    #[arg(long, help = "Colour scale to use", default_value = "magma")]
    colour_scale: ColourScale,

    #[arg(long, help = "Value mode: scaled (default), raw, or categorical", default_value = "scaled")]
    value_mode: ValueMode,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Configure logging based on verbose level
    let log_level = match args.verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    env_logger::Builder::from_default_env()
        .filter_level(log_level)
        .init();

    let output_file = args.output.clone();
    
    // Select colour scale based on command line argument
    let colour_scale = match args.colour_scale {
        ColourScale::Accessible | ColourScale::Cividis => &colorous::CIVIDIS,
        ColourScale::Magma => &colorous::MAGMA,
    };

    // Handle backward compatibility with old log parameters
    let curve = if args.log_min.is_some() || args.log_max.is_some() {
        DomainType::Logarithmic
    } else {
        args.curve
    };

    let mut heatmap = Heatmap::new(
        curve,
        args.min_value,
        args.max_value,
        args.accumulate,
        args.bits_per_pixel,
        colour_scale,
        args.value_mode,
    );
    heatmap.process_input()?;
    heatmap.save(&output_file)?;

    Ok(())
}
