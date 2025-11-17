use clap::{Args, Parser, Subcommand, ValueEnum};
use color_eyre::eyre::{Result, bail};
use jiff::{Span, Timestamp, Unit};
pub mod config;
use config::AppConfig;
use tracing::debug;
use v_exchanges::{
	core::{Exchange, Instrument, Ticker},
	prelude::*,
};
use v_utils::{Percent, io::ExpandedPath, trades::*};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, ValueEnum)]
enum Quality {
	A,
	B,
	C,
	D,
	E,
}

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

fn parse_f64_with_underscores(s: &str) -> Result<f64, std::num::ParseFloatError> {
	s.replace('_', "").parse()
}

#[derive(Args, Debug)]
struct SizeArgs {
	ticker: String,
	#[arg(short, long)]
	quality: Quality,
	#[arg(short, long, value_parser = parse_f64_with_underscores)]
	exact_sl: Option<f64>,
	#[arg(short, long)]
	percent_sl: Option<Percent>,
}

impl Default for SizeArgs {
	fn default() -> Self {
		Self {
			ticker: String::new(),
			quality: Quality::C,
			exact_sl: None,
			percent_sl: None,
		}
	}
}

#[tokio::main]
async fn main() {
	v_utils::clientside!();
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
	let ticker: Ticker = args.ticker.parse()?;

	// Initialize exchanges from config
	let mut exchanges: Vec<Box<dyn Exchange>> = Vec::new();
	for exchange_config in &config.exchanges {
		match exchange_config.name.to_lowercase().as_str() {
			"binance" => {
				let mut bn = Binance::default();
				bn.auth(exchange_config.key.clone(), exchange_config.secret.clone());
				exchanges.push(Box::new(bn));
			}
			"bybit" => {
				let mut bb = Bybit::default();
				bb.auth(exchange_config.key.clone(), exchange_config.secret.clone());
				exchanges.push(Box::new(bb));
			}
			"mexc" => {
				let mut mx = Mexc::default();
				mx.auth(exchange_config.key.clone(), exchange_config.secret.clone());
				exchanges.push(Box::new(mx));
			}
			_ => {
				eprintln!("Unknown exchange: {}", exchange_config.name);
			}
		}
	}

	async fn request_total_balances(clients: &[&dyn Exchange]) -> Result<Usd> {
		let mut total = Usd(0.);
		for c in clients {
			let balances = c.balances(Instrument::Perp, None).await.unwrap();
			total += balances.total;
		}
		Ok(total)
	}

	let exchange_refs: Vec<&dyn Exchange> = exchanges.iter().map(|e| e.as_ref()).collect();
	let mut total_balance = request_total_balances(&exchange_refs).await?;

	// Add other balances if configured
	if let Some(other) = config.other_balances {
		total_balance = Usd(*total_balance + other);
	}

	// Use the first exchange for price lookup (could be made configurable based on ticker.exchange_name)
	let price = exchanges[0].price(ticker.symbol, None).await.unwrap();

	let sl_percent: Percent = match args.percent_sl {
		Some(percent) => percent,
		None => match args.exact_sl {
			Some(sl) => ((price - sl).abs() / price).into(),
			None => config.default_sl,
		},
	};
	let time = ema_prev_times_for_same_move(&config, exchanges[0].as_ref(), ticker.symbol, price, sl_percent).await?;

	let mul = mul_criterion(time);
	let quality_risk = match args.quality {
		Quality::A => config.risk_tiers.a,
		Quality::B => config.risk_tiers.b,
		Quality::C => config.risk_tiers.c,
		Quality::D => config.risk_tiers.d,
		Quality::E => config.risk_tiers.e,
	};
	let target_balance_risk = Percent(*quality_risk * mul);
	let size = *total_balance * *(target_balance_risk / sl_percent);

	let hours = (time.total(Unit::Second).unwrap() as i64 / 3600) as f64;
	debug!(?price, ?total_balance, ?hours, ?mul);
	println!("Total Depo: {total_balance}$");
	println!("Chosen SL range: {sl_percent}");
	println!("Target Risk: {target_balance_risk} of depo ({})", total_balance * *target_balance_risk);
	println!("\nSize: {size:.2}");
	Ok(())
}

/// Returns EMA over previous 10 last moves of the same distance.
async fn ema_prev_times_for_same_move(_config: &AppConfig, bn: &dyn Exchange, symbol: v_exchanges::core::Symbol, price: f64, sl_percent: Percent) -> Result<Span> {
	static RUN_TIMES: usize = 10;
	let calc_range = |price: f64, sl_percent: Percent| {
		let sl = price * *sl_percent;
		(price - sl, price + sl)
	};
	let mut range = calc_range(price, sl_percent);
	let mut prev_time = Timestamp::now();
	let mut times: Vec<Span> = Vec::default();

	let mut check_if_satisfies = |k: &Kline, times: &mut Vec<Span>, prev_time: &mut Timestamp| -> bool {
		let new_anchor = match k {
			_ if k.low < range.0 => range.0,
			_ if k.high > range.1 => range.1,
			_ => return false,
		};
		let duration: Span = prev_time.since(k.open_time).unwrap();
		*prev_time = prev_time.checked_sub(duration).unwrap();
		times.push(duration);
		range = calc_range(new_anchor, sl_percent);
		true
	};

	let preset_timeframes: Vec<Timeframe> = vec!["1m".into(), "1h".into(), "1w".into()];
	let mut approx_correct_tf: Option<Timeframe> = None;
	for tf in preset_timeframes {
		if approx_correct_tf.is_none() {
			let klines = bn.klines(symbol, tf, 1000.into(), None).await.unwrap();
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
		let request_range = (prev_time.checked_sub(tf.duration() * 999).unwrap(), prev_time);
		let klines = bn.klines(symbol, tf, request_range.into(), None).await.unwrap();
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
	debug!(?times);
	let ema = times.iter().enumerate().fold(0_i64, |acc: i64, (i, x): (usize, &Span)| {
		(acc + x.total(Unit::Second).unwrap() as i64 * (i as i64 + 1)).try_into().unwrap()
	}) as f64
		/ ((times.len() + 1) as f64 * times.len() as f64 / 2.0);
	debug!(?ema);
	Ok(Span::new().seconds(ema as i64))
}

fn mul_criterion(time: Span) -> f64 {
	// 0.1 -> 0.2
	// 0.5 -> 0.5
	// 1 -> 0.7
	// 2 -> 0.8
	// 5 -> 0.9
	// 10+ -> ~1
	// Note: Using integer division to match old chrono::TimeDelta::num_hours() behavior
	let hours = (time.total(Unit::Second).unwrap() as i64 / 3600) as f64;

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
		let mul_out: Vec<f64> = x_points.iter().map(|x| mul_criterion(Span::new().minutes((x * 60.0) as i64))).collect();
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
