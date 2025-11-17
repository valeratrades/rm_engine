use chrono::{DateTime, TimeDelta, Utc};
use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::{Result, bail};
pub mod config;
use config::AppConfig;
use v_exchanges::{core::Exchange, prelude::*};
use v_utils::prelude_clientside::*;

#[derive(Default, Parser)]
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

#[derive(Args, Debug, Default)]
struct SizeArgs {
	pair: Pair,
	#[arg(short, long)]
	exact_sl: Option<f64>,
	#[arg(short, long)]
	percent_sl: Option<Percent>,
}

#[tokio::main]
async fn main() {
	clientside!();
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

	//let total_balance = request_total_balance(&*bn, &*bb, &*mx).await;
	async fn request_total_balances(clients: &[&dyn Exchange]) -> Result<Usd> {
		let mut total = Usd(0.);
		for c in clients {
			let balances = c.balances(c.source_market()).await.unwrap();
			total += balances.total;
		}
		Ok(total)
	}
	let total_balance = request_total_balances(&[&*bn, &*bb, &*mx]).await?;
	let price = bn.price(args.pair, "Binance/Futures".into()).await.unwrap();

	let sl_percent: Percent = match args.percent_sl {
		Some(percent) => percent,
		None => match args.exact_sl {
			Some(sl) => ((price - sl).abs() / price).into(),
			None => config.default_sl,
		},
	};
	let time = ema_prev_times_for_same_move(&config, &*bn, &args, price, sl_percent).await?;

	let mul = mul_criterion(time);
	let target_balance_risk = Percent(*config.default_risk_percent_balance * mul);
	let size = *total_balance * *(target_balance_risk / sl_percent);

	dbg!(price, total_balance, time.num_hours(), mul);
	println!("Chosen SL range: {sl_percent}");
	println!("Target Risk: {target_balance_risk} of depo ({})", total_balance * *target_balance_risk);
	println!("\nSize: {size:.2}");
	Ok(())
}

/// Returns EMA over previous 10 last moves of the same distance.
async fn ema_prev_times_for_same_move(_config: &AppConfig, bn: &dyn Exchange, args: &SizeArgs, price: f64, sl_percent: Percent) -> Result<TimeDelta> {
	static RUN_TIMES: usize = 10;
	let calc_range = |price: f64, sl_percent: Percent| {
		let sl = price * *sl_percent;
		(price - sl, price + sl)
	};
	let mut range = calc_range(price, sl_percent);
	let mut prev_time = Utc::now();
	let mut times: Vec<TimeDelta> = Vec::default();

	let mut check_if_satisfies = |k: &Kline, times: &mut Vec<TimeDelta>, prev_time: &mut DateTime<Utc>| -> bool {
		let new_anchor = match k {
			_ if k.low < range.0 => range.0,
			_ if k.high > range.1 => range.1,
			_ => return false,
		};
		let duration: TimeDelta = *prev_time - k.open_time;
		*prev_time -= duration;
		times.push(duration);
		range = calc_range(new_anchor, sl_percent);
		true
	};

	let preset_timeframes: Vec<Timeframe> = vec!["1m".into(), "1h".into(), "1w".into()];
	let mut approx_correct_tf: Option<Timeframe> = None;
	for tf in preset_timeframes {
		if approx_correct_tf.is_none() {
			let klines = bn.klines(args.pair, tf, 1000.into(), bn.source_market()).await.unwrap();
			for k in klines.iter().rev() {
				match check_if_satisfies(k, &mut times, &mut prev_time) {
					true => {
						approx_correct_tf = Some(tf);
						break;
					}
					false => continue,
				}
			}
		}
	}

	let tf = approx_correct_tf.unwrap();
	let mut i = 0;
	while times.len() < RUN_TIMES && i < 10 {
		let request_range = (prev_time - (tf.duration() * 999), prev_time);
		let klines = bn.klines(args.pair, tf, request_range.into(), bn.source_market()).await.unwrap();
		for k in klines.iter().rev() {
			match check_if_satisfies(k, &mut times, &mut prev_time) {
				true =>
					if times.len() == RUN_TIMES {
						break;
					},
				false => continue,
			}
		}
		i += 1;
	}

	if times.is_empty() {
		bail!("No data found for the given data & sl, you're on your own.");
	}
	// The last is the oldest
	//let ema = times.iter().fold(0, |acc, x| acc + x.num_seconds()) / times.len() as i64;
	dbg!(&times);
	let ema = times.iter().enumerate().fold(0_i64, |acc, (i, x)| (acc + x.num_seconds() * (i as i64 + 1)).try_into().unwrap()) as f64 / ((times.len() + 1) as f64 * times.len() as f64 / 2.0);
	dbg!(&ema);
	Ok(TimeDelta::seconds(ema as i64))
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
