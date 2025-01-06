use clap::{Args, Parser, Subcommand};
pub mod config;
use std::env;

use config::AppConfig;
use v_exchanges::{
	adapters::{binance::BinanceOption, bybit::BybitOption},
	binance::Binance,
	bybit::Bybit,
	core::Exchange,
};
use v_utils::{io::ExpandedPath, trades::Pair};

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
	Size(SizeArgs),
}
impl Default for Commands {
	fn default() -> Self {
		Commands::Size(SizeArgs::default())
	}
}

#[derive(Debug, Args, Default)]
struct SizeArgs {
	pair: Pair,
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
		Commands::Size(args) => start(config, args).await,
	}
}

async fn start(config: AppConfig, args: SizeArgs) {
	let mut bn = Binance::default();
	let mut bb = Bybit::default();
	let total_balance = request_total_balance(&config, &mut bn, &mut bb).await;
	let price = bn.futures_price(args.pair).await.unwrap();

	dbg!(&price, &total_balance);
}

async fn request_total_balance(config: &AppConfig, bn: &mut Binance, bb: &mut Bybit) -> f64 {
	bn.update_default_option(BinanceOption::Key(config.binance.key.clone()));
	bn.update_default_option(BinanceOption::Secret(config.binance.secret.clone()));

	bb.update_default_option(BybitOption::Key(config.bybit.key.clone()));
	bb.update_default_option(BybitOption::Secret(config.bybit.secret.clone()));

	let binance_usdc = bn.futures_asset_balance("USDC".into()).await.unwrap();
	let binance_usdt = bn.futures_asset_balance("USDT".into()).await.unwrap();
	let bybit_usdc = bb.futures_asset_balance("USDC".into()).await.unwrap();
	//let bybit_usdt = bb.futures_asset_balance("USDT".into()).await.unwrap();

	//HACK: in actuality get in usdt for now, assume 1:1 with usd
	binance_usdc.balance + binance_usdt.balance + bybit_usdc.balance
}
