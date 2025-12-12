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
	/// Max risk for A-quality trades. Each tier below divides by e (2.718...)
	pub abs_max_risk: Percent,
}

#[derive(Clone, Debug, v_macros::MyConfigPrimitives)]
pub struct ExchangeConfig {
	pub api_pubkey: String,
	pub api_secret: SecretString,
	#[serde(default)]
	pub passphrase: Option<SecretString>,
}
