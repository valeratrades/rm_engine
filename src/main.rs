use std::{collections::HashMap, str::FromStr};

use clap::{Args, Parser, Subcommand, ValueEnum};
use color_eyre::eyre::{Result, bail, eyre};
use jiff::{Span, Timestamp, Unit};
pub mod config;
use config::{AppConfig, SettingsFlags};
use tracing::debug;
use v_exchanges::core::{Exchange, ExchangeName, Instrument, Ticker};
use v_utils::{Percent, percent::PercentU, trades::*};

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
	#[command(flatten)]
	settings: SettingsFlags,
}
#[derive(Subcommand)]
enum Commands {
	Size(SizeArgs),
	Balance,
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
	let config = match AppConfig::try_build_with_validation(cli.settings) {
		Ok(config) => config,
		Err(e) => {
			eprintln!("Error reading config: {e}");
			return;
		}
	};
	match cli.command {
		Commands::Size(args) => start(config, args).await.unwrap(),
		Commands::Balance => show_balance(config).await.unwrap(),
	}
}

struct InitializedExchange {
	exchange: Box<dyn Exchange>,
	key: String,
}

fn initialize_exchanges(config: &AppConfig) -> Result<Vec<InitializedExchange>> {
	let mut exchanges: Vec<InitializedExchange> = Vec::new();
	for exchange_config in &config.exchanges {
		let exchange_name = ExchangeName::from_str(&exchange_config.exch_name)?;
		let mut exchange = exchange_name.init_client();
		exchange.auth(exchange_config.key.clone(), exchange_config.secret.clone());
		exchange.set_max_tries(3);
		exchange.set_recv_window(std::time::Duration::from_secs(15));

		// special case: KuCoin requires a passphrase
		if exchange_name == ExchangeName::Kucoin {
			let passphrase = exchange_config.passphrase.clone().ok_or_else(|| eyre!("Kucoin exchange requires passphrase in config"))?;
			exchange.update_default_option(v_exchanges::kucoin::KucoinOption::Passphrase(passphrase));
		}

		// Create key: if tag exists, use "exchname_tag", otherwise just "exchname"
		let key = match &exchange_config.tag {
			Some(tag) => format!("{}_{}", exchange_config.exch_name, tag),
			None => exchange_config.exch_name.clone(),
		};

		exchanges.push(InitializedExchange { exchange, key });
	}
	Ok(exchanges)
}

async fn collect_balances(exchanges: &[InitializedExchange]) -> Result<HashMap<String, Usd>> {
	let mut balances = HashMap::new();
	for init_exch in exchanges {
		let balance = init_exch.exchange.balances(Instrument::Perp, None).await.unwrap();
		tracing::debug!("Per-Exchange balances: {:?}: {balance:?}", init_exch.key);
		balances.insert(init_exch.key.clone(), balance.total);
	}
	Ok(balances)
}

async fn get_total_balance(config: &AppConfig, balances: &HashMap<String, Usd>) -> Result<Usd> {
	let mut total_balance = Usd(0.);
	for balance in balances.values() {
		total_balance += *balance;
	}

	// Add other balances if configured
	if let Some(other) = config.other_balances {
		total_balance = Usd(*total_balance + other);
	}

	Ok(total_balance)
}

async fn show_balance(config: AppConfig) -> Result<()> {
	let exchanges = initialize_exchanges(&config)?;
	let balances = collect_balances(&exchanges).await?;

	// Sort balances by value (descending)
	let mut sorted_balances: Vec<_> = balances.iter().collect();
	sorted_balances.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

	// Print individual balances
	for (key, balance) in sorted_balances {
		println!("{key}: {balance}$");
	}

	// Print total
	let total_balance = get_total_balance(&config, &balances).await?;
	println!("\nTotal: {total_balance}$");
	Ok(())
}

async fn start(config: AppConfig, args: SizeArgs) -> Result<()> {
	let ticker: Ticker = args.ticker.parse()?;

	let exchanges = initialize_exchanges(&config)?;
	let balances = collect_balances(&exchanges).await?;
	let total_balance = get_total_balance(&config, &balances).await?;

	// Use the first exchange for price lookup (could be made configurable based on ticker.exchange_name)
	let price = exchanges[0].exchange.price(ticker.symbol, None).await.unwrap();

	let sl_percent: Percent = match args.percent_sl {
		Some(percent) => percent,
		None => match args.exact_sl {
			Some(sl) => ((price - sl).abs() / price).into(),
			None => config.default_sl,
		},
	};
	let time = ema_prev_times_for_same_move(&config, exchanges[0].exchange.as_ref(), ticker.symbol, price, sl_percent).await?;

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

	// Apply round bias
	let biased_size = apply_round_bias(size, config.round_bias);

	println!("Total Depo: {total_balance}$");
	println!("Chosen SL range: {sl_percent}");
	println!("Target Risk: {target_balance_risk} of depo ({})", total_balance * *target_balance_risk);
	println!("\nSize: {biased_size:.2}");
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

/// Apply rounding bias to skew the result towards rounder numbers.
/// The bias parameter (default 1%) determines how much to favor round numbers.
fn apply_round_bias(value: f64, bias: PercentU) -> f64 {
	if value == 0.0 {
		return value;
	}

	// Find the magnitude of the value (e.g., 1234.56 -> 1000)
	let magnitude = 10_f64.powi(value.abs().log10().floor() as i32);

	// Generate candidate round numbers at different scales
	let candidates = vec![
		// Round to nearest 1000, 500, 100, 50, 10, 5, 1
		(value / (magnitude * 10.0)).round() * (magnitude * 10.0),
		(value / (magnitude * 5.0)).round() * (magnitude * 5.0),
		(value / magnitude).round() * magnitude,
		(value / (magnitude / 2.0)).round() * (magnitude / 2.0),
		(value / (magnitude / 10.0)).round() * (magnitude / 10.0),
		(value / (magnitude / 20.0)).round() * (magnitude / 20.0),
		(value / (magnitude / 100.0)).round() * (magnitude / 100.0),
	];

	// Find the closest rounder number
	let closest_round = candidates
		.iter()
		.filter(|&&c| c > 0.0)
		.min_by(|&&a, &&b| {
			let dist_a = (a - value).abs();
			let dist_b = (b - value).abs();
			dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
		})
		.copied()
		.unwrap_or(value);

	// Apply bias: move towards the rounder number by the bias percentage
	value + (closest_round - value) * **bias
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
	fn test_apply_round_bias() {
		// Test with 1% bias (default)
		let bias = PercentU::new(0.01).unwrap();

		// 1234.5 should move towards 1200 (closer round number)
		let result = apply_round_bias(1234.5, bias);
		assert!(result < 1234.5 && result > 1233.0, "Expected value between 1233 and 1234.5, got {}", result);

		// 1250.0 is already round, should stay close
		let result = apply_round_bias(1250.0, bias);
		assert!((result - 1250.0).abs() < 1.0, "Expected value close to 1250, got {}", result);

		// Test with higher bias (10%)
		let bias = PercentU::new(0.10).unwrap();
		let result = apply_round_bias(1234.5, bias);
		assert!(result < 1234.5 && result > 1230.0, "Expected larger shift with 10% bias, got {}", result);

		// Test with zero value
		let result = apply_round_bias(0.0, bias);
		assert_eq!(result, 0.0, "Zero should remain zero");
	}

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
