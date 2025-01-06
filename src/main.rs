use chrono::{TimeDelta, Utc};
use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::{Result, bail};
pub mod config;

use config::AppConfig;
use v_exchanges::{
	adapters::{binance::BinanceOption, bybit::BybitOption},
	binance::Binance,
	bybit::Bybit,
	core::Exchange,
};
use v_utils::{
	io::{ExpandedPath, Percent},
	trades::{Pair, Timeframe},
};

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
	#[arg(short, long)]
	exact_sl: Option<f64>,
	#[arg(short, long)]
	percent_sl: Option<Percent>,
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
		Commands::Size(args) => start(config, args).await.unwrap(),
	}
}

async fn start(config: AppConfig, args: SizeArgs) -> Result<()> {
	let mut bn = Binance::default();
	let mut bb = Bybit::default();
	let total_balance = request_total_balance(&config, &mut bn, &mut bb).await;
	let price = bn.futures_price(args.pair).await.unwrap();

	let time = time_since_comp_move(&config, &mut bn, &args, price).await?;

	dbg!(&price, &total_balance, &time.num_hours());
	Ok(())
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

async fn time_since_comp_move(config: &AppConfig, bn: &mut Binance, args: &SizeArgs, price: f64) -> Result<TimeDelta> {
	//DO: pick 1m timeframe, request 500 candles
	//DO: match on crosses, false => up tf to 1h, then 1w
	let sl_percent: Percent = match args.percent_sl {
		Some(percent) => percent,
		None => match args.exact_sl {
			Some(sl) => ((price - sl).abs() / price).into(),
			None => match config.default_sl {
				Some(p) => p,
				None => bail!("Stop loss not provided. Add default to config or pass an arg."),
			},
		},
	};

	let calc_range = |price: f64, sl_percent: Percent| {
		let sl = price * *sl_percent;
		(price - sl, price + sl)
	};
	let range = calc_range(price, sl_percent);

	let timeframes: Vec<Timeframe> = vec!["1m".into(), "1h".into(), "1w".into()];
	for tf in timeframes {
		let klines = bn.futures_klines(args.pair, tf, 500.into()).await.unwrap();
		dbg!(&klines[klines.len()-3..]);
		for k in &*klines {
			if k.low < range.0 || k.high > range.1 {
				return Ok(Utc::now() - k.open_time);
			}
		}
	}

	//TODO!!!: implement
	todo!("if sl is at 100% or not all historic data is avaliable, could not have any crosses, shouldn't error (but rn it does, deal with it)");
}
