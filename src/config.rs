use color_eyre::eyre::{Result, bail};
use secrecy::SecretString;
use v_utils::{Percent, macros as v_macros, percent::PercentU};

#[derive(Clone, Debug, Default, v_macros::MyConfigPrimitives, v_macros::Settings)]
pub struct AppConfig {
	#[settings(flatten)]
	pub size: Option<SizeConfig>,
	pub exchanges: Vec<ExchangeConfig>,
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
	pub exch_name: String,
	pub tag: Option<String>,
	pub key: String,
	pub secret: SecretString,
	pub passphrase: Option<SecretString>,
}

impl AppConfig {
	pub fn try_build_with_validation(flags: SettingsFlags) -> Result<Self> {
		let config = Self::try_build(flags)?;
		Self::validate(&config)?;
		Ok(config)
	}

	fn validate(config: &Self) -> Result<()> {
		// Check if any exch_name appears more than once
		let mut exch_name_counts = std::collections::HashMap::new();
		for exchange in &config.exchanges {
			*exch_name_counts.entry(&exchange.exch_name).or_insert(0) += 1;
		}

		// If any exch_name appears more than once, all instances must have tags
		let has_duplicates = exch_name_counts.values().any(|&count| count > 1);
		if has_duplicates {
			let mut seen_keys = std::collections::HashSet::new();
			for exchange in &config.exchanges {
				// Only check exchanges that have duplicate exch_names
				if exch_name_counts[&exchange.exch_name] > 1 {
					match &exchange.tag {
						None => bail!(
							"Exchange '{}' appears multiple times. All instances of '{}' must have a 'tag' field",
							exchange.exch_name,
							exchange.exch_name
						),
						Some(tag) => {
							let key = format!("{}_{}", exchange.exch_name, tag);
							if !seen_keys.insert(key.clone()) {
								bail!("Duplicate key '{}' found. All exchname_tag combinations must be unique", key);
							}
						}
					}
				}
			}
		}
		Ok(())
	}
}
