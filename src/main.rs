use clap::{Args, Parser, Subcommand};
pub mod config;
use config::AppConfig;
use std::env;
use v_utils::io::ExpandedPath;
use v_exchanges::{binance::Binance, core::Exchange};
use v_exchanges::adapters::binance::BinanceOption;
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

#[tokio::main]
async fn main() {
	let cli = Cli::parse();
	let config = match AppConfig::read(cli.config) {
		Ok(config) => config,
		Err(e) => {
			eprintln!("Error reading config: {e}");
			return;
		}
	};
	match cli.command {
		Commands::Start(args) => start(config, args).await,
	}
}

async fn start(config: AppConfig, args: StartArgs) {
	let mut bn = Binance::default();
	let total_balance = request_total_balance(&mut bn).await;
	let price = bn.futures_price(("BTC", "USDT").into()).await.unwrap();

	dbg!(&price, &total_balance);
}

async  fn request_total_balance(bn: &mut Binance) -> f64 {
	let key = env::var("BINANCE_TIGER_READ_KEY").unwrap();
	let secret = env::var("BINANCE_TIGER_READ_SECRET").unwrap();
	bn.update_default_option(BinanceOption::Key(key));
	bn.update_default_option(BinanceOption::Secret(secret));

	
	let binance_usdc = bn.futures_asset_balance("USDC".into()).await.unwrap();
	let binance_usdt = bn.futures_asset_balance("USDT".into()).await.unwrap();

	dbg!(&binance_usdc, &binance_usdt);

	//HACK: in actuallity get in usdt for now, assume 1:1 with usd
	let total_binance_usd = 100.0;
	let total_bybit_usd = 100.0;

	total_binance_usd + total_bybit_usd
}
