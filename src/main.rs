use chrono::{TimeDelta, Utc};
use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::Result;
pub mod config;

use config::AppConfig;
use v_exchanges::{core::Exchange, prelude::*};
use v_utils::{
	Percent,
	io::ExpandedPath,
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
	let mut bn = AbsMarket::from("Binance/Futures").client();
	bn.auth(config.binance.key.clone(), config.binance.secret.clone());
	let mut bb = AbsMarket::from("Bybit/Linear").client();
	bb.auth(config.bybit.key.clone(), config.bybit.secret.clone());
	let mut mx = AbsMarket::from("Mexc/Futures").client();
	mx.auth(config.mexc.key.clone(), config.mexc.secret.clone());

	let total_balance = request_total_balance(&*bn, &*bb, &*mx).await;
	let price = bn.price(args.pair, "Binance/Futures".into()).await.unwrap();

	let sl_percent: Percent = match args.percent_sl {
		Some(percent) => percent,
		None => match args.exact_sl {
			Some(sl) => ((price - sl).abs() / price).into(),
			None => config.default_sl,
		},
	};
	dbg!(sl_percent);
	let time = time_since_comp_move(&config, &*bn, &args, price, sl_percent).await?;

	let mul = mul_criterion(time);
	let target_risk = *config.default_risk_percent_balance * mul;
	let size = total_balance * (target_risk / *sl_percent);

	dbg!(price, total_balance, time.num_hours(), target_risk, mul);
	println!("Size: {size:.2}");
	Ok(())
}

async fn request_total_balance(bn: &dyn Exchange, bb: &dyn Exchange, mx: &dyn Exchange) -> f64 {
	//TODO!!!!!: generalize to get a) all assets, b) their usd values, not notional
	let binance_usdc = bn.asset_balance("USDC".into(), bn.source_market()).await.unwrap();
	let binance_usdt = bn.asset_balance("USDT".into(), bn.source_market()).await.unwrap();

	dbg!(&bb);
	let bybit_usdt = bb.asset_balance("USDT".into(), bb.source_market()).await.unwrap();
	let bybit_usdc = bb.asset_balance("USDC".into(), bb.source_market()).await.unwrap();

	let mexc_usdt = mx.asset_balance("USDT".into(), mx.source_market()).await.unwrap();

	//HACK: in actuality get in usdt for now, assume 1:1 with usd
	*binance_usdc + *binance_usdt + *bybit_usdt + *bybit_usdc + *mexc_usdt
}

//TODO!: measure 10 back, EMA over them
async fn time_since_comp_move(_config: &AppConfig, bn: &dyn Exchange, args: &SizeArgs, price: f64, sl_percent: Percent) -> Result<TimeDelta> {
	let calc_range = |price: f64, sl_percent: Percent| {
		let sl = price * *sl_percent;
		(price - sl, price + sl)
	};
	let range = calc_range(price, sl_percent);

	let timeframes: Vec<Timeframe> = vec!["1m".into(), "1h".into(), "1w".into()];
	for tf in timeframes {
		let klines = bn.klines(args.pair, tf, 1000.into(), bn.source_market()).await.unwrap();
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
	use v_utils::utils::SnapshotP;

	use super::*;

	#[test]
	fn proper_mul_snapshot_test() {
		//TODO!: switch to using non-homogeneous steps, so the data is dencer near 0 (requires: 1) new snapshot fn, 2) fn to gen it)
		let x_points: Vec<f64> = (0..1000).map(|x| (x as f64) / 10.0).collect();
		let mul_out: Vec<f64> = x_points.iter().map(|x| mul_criterion(TimeDelta::minutes((x * 60.0) as i64))).collect();
		let plot = SnapshotP::build(&mul_out).draw();

		insta::assert_snapshot!(plot, @r#"
                                                                         ▁▁▂▂▃▃▃▄▄▄▅▆▆▆▇▇▇██1.113
                                                         ▁▁▂▂▃▃▄▄▅▅▆▆▇▇█████████████████████     
                                             ▁▂▃▃▄▅▅▆▆▇▇████████████████████████████████████     
                                  ▁▂▂▃▅▅▆▇▇█████████████████████████████████████████████████     
                          ▁▂▃▅▆▇▇███████████████████████████████████████████████████████████     
                   ▁▃▄▅▆▇███████████████████████████████████████████████████████████████████     
              ▂▃▅▆▇█████████████████████████████████████████████████████████████████████████     
           ▄▆███████████████████████████████████████████████████████████████████████████████     
        ▃▆██████████████████████████████████████████████████████████████████████████████████     
      ▄█████████████████████████████████████████████████████████████████████████████████████     
    ▂███████████████████████████████████████████████████████████████████████████████████████     
  ▁▂████████████████████████████████████████████████████████████████████████████████████████0.200
  "#);
	}
}
