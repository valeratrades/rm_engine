use clap::{Args, Parser, Subcommand};
pub mod config;
use config::AppConfig;
use v_utils::io::ExpandedPath;
use v_exchanges::{binance::Binance, core::Exchange};
use v_exchanges::{bybit::Bybit};


#[derive(Parser, Default)]
#[command(author, version, about, long_about = None)]
struct Cli {
	#[command(subcommand)]
	command: Commands,
	#[arg(long)]
	config: Option<ExpandedPath>,
}
#[derive(Subcommand)]
enum Commands {
	Start(StartArgs),
}
impl Default for Commands {
	fn default() -> Self {
		Commands::Start(StartArgs::default())
	}
}

#[derive(Args, Default)]
struct StartArgs {
	arg: String,
}

fn main() {
	let cli = Cli::parse();
	let config = match AppConfig::read(cli.config) {
		Ok(config) => config,
		Err(e) => {
			eprintln!("Error reading config: {e}");
			return;
		}
	};
	match cli.command {
		Commands::Start(args) => start(config, args),
	}
}

fn start(config: AppConfig, args: StartArgs) {
	let total_balance = request_total_balance(&config);
	
	let message = format!("Hello, {}", args.arg);
	println!("{message}");
}

fn request_total_balance(config: &AppConfig) -> f64 {
	//HACK: in actuallity get in usdt for now, assume 1:1 with usd
	let total_binance_usd = 100.0;
	let total_bybit_usd = 100.0;

	total_binance_usd + total_bybit_usd
}
