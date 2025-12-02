use std::collections::HashMap;

use color_eyre::eyre::Result;
use secrecy::SecretString;
use v_utils::{Percent, macros as v_macros, percent::PercentU};

#[derive(Clone, Debug, Default, v_macros::MyConfigPrimitives, v_macros::Settings)]
pub struct AppConfig {
	#[settings(flatten)]
	pub size: Option<SizeConfig>,
	pub exchanges: HashMap<String, ExchangeConfig>,
	pub other_balances: Option<f64>,
}

#[derive(Clone, Debug, Default, v_macros::MyConfigPrimitives, v_macros::SettingsBadlyNested)]
pub struct SizeConfig {
	pub default_sl: Percent,
	#[settings(default = "PercentU::new(0.01).unwrap()")]
	pub round_bias: PercentU,
	pub risk_tiers: RiskTiers,
}

#[derive(Clone, Debug, Default, v_macros::MyConfigPrimitives)]
pub struct RiskTiers {
	pub a: Percent,
	pub b: Percent,
	pub c: Percent,
	pub d: Percent,
	pub e: Percent,
}

#[derive(Clone, Debug, v_macros::MyConfigPrimitives)]
pub struct ExchangeConfig {
	pub api_pubkey: String,
	pub api_secret: SecretString,
	#[serde(default)]
	pub passphrase: Option<SecretString>,
}
