use chrono::{TimeDelta, Utc};
use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::Result;
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

	let sl_percent: Percent = match args.percent_sl {
		Some(percent) => percent,
		None => match args.exact_sl {
			Some(sl) => ((price - sl).abs() / price).into(),
			None => config.default_sl,
		},
	};
	let time = time_since_comp_move(&config, &mut bn, &args, price, sl_percent).await?;

	let mul = mul_criterion(time);
	let target_risk = &*config.default_risk_percent_balance * mul;
	let size = total_balance * (target_risk / *sl_percent);

	dbg!(&price, &total_balance, &time.num_hours(), target_risk, mul);
	println!("Size: {size:.2}");
	Ok(())
}

async fn request_total_balance(config: &AppConfig, bn: &mut Binance, bb: &mut Bybit) -> f64 {
	bn.update_default_option(BinanceOption::Key(config.binance.key.clone()));
	bn.update_default_option(BinanceOption::Secret(config.binance.secret.clone()));

	bb.update_default_option(BybitOption::Key(config.bybit.key.clone()));
	bb.update_default_option(BybitOption::Secret(config.bybit.secret.clone()));

	//TODO!!!!!: generalize to get a) all assets, b) their usd values, not notional
	let binance_usdc = bn.futures_asset_balance("USDC".into()).await.unwrap();
	let binance_usdt = bn.futures_asset_balance("USDT".into()).await.unwrap();
	let bybit_usdc = bb.futures_asset_balance("USDC".into()).await.unwrap();
	//let bybit_usdt = bb.futures_asset_balance("USDT".into()).await.unwrap();

	//HACK: in actuality get in usdt for now, assume 1:1 with usd
	binance_usdc.balance + binance_usdt.balance + bybit_usdc.balance
}

async fn time_since_comp_move(_config: &AppConfig, bn: &mut Binance, args: &SizeArgs, price: f64, sl_percent: Percent) -> Result<TimeDelta> {
	let calc_range = |price: f64, sl_percent: Percent| {
		let sl = price * *sl_percent;
		(price - sl, price + sl)
	};
	let range = calc_range(price, sl_percent);

	let timeframes: Vec<Timeframe> = vec!["1m".into(), "1h".into(), "1w".into()];
	for tf in timeframes {
		let klines = bn.futures_klines(args.pair, tf, 500.into()).await.unwrap();
		for k in klines.iter().rev() {
			if k.low < range.0 || k.high > range.1 {
				return Ok(Utc::now() - k.open_time);
			}
		}
	}

	//TODO!!!: implement
	todo!("if sl is at 100% or not all historic data is avaliable, could not have any crosses, shouldn't error (but rn it does, deal with it)");
}

fn mul_criterion(time: TimeDelta) -> f64 {
	// 0.1 -> 0.2
	// 0.5 -> 0.5
	// 1 -> 0.7
	// 2 -> 0.8
	// 5 -> 0.9
	// 10+ -> ~1
	let hours = time.num_hours() as f64;

	// potentially transfer to just use something like `-1/(x+1) + 1` (to integrate would first need to fix snapshot, current one doesn't satisfy
	// methods for finding a better approximation: [../docs/assets/prof_advice_on_approximating_size_mul.pdf]

	(2.0 - (3.0_f64).powf(0.25) * (10.0_f64).powf(0.5) * hours.powf(0.25)).abs() / 10.0
}

#[cfg(test)]
mod tests {
	use super::*;

	//TODO!: make a better snapshot test. Want a single assert on a plot.
	#[test]
	fn test_mul_criterion() {
		let v = 0.1;
		let time = TimeDelta::minutes((v * 60.0) as i64);
		let f = format!("{v:.1} -> {:.3}", mul_criterion(time));
		insta::assert_snapshot!(f, @"0.1 -> 0.200");

		let v = 1.0;
		let time = TimeDelta::minutes((v * 60.0) as i64);
		let f = format!("{v:.1} -> {:.3}", mul_criterion(time));
		insta::assert_snapshot!(f, @"1.0 -> 0.216");

		let v = 2.0; 
		let time = TimeDelta::minutes((v * 60.0) as i64);
		let f = format!("{v:.1} -> {:.3}", mul_criterion(time));
		insta::assert_snapshot!(f, @"2.0 -> 0.295");

		let v = 5.0;
		let time = TimeDelta::minutes((v * 60.0) as i64);
		let f = format!("{v:.1} -> {:.3}", mul_criterion(time));
		insta::assert_snapshot!(f, @"5.0 -> 0.422");

		let v = 100.0;
		let time = TimeDelta::minutes((v * 60.0) as i64);
		let f = format!("{v:.1} -> {:.3}", mul_criterion(time));
		insta::assert_snapshot!(f, @"100.0 -> 1.116");
	}
}
