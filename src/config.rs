use color_eyre::eyre::Result;
use v_utils::{Percent, io::ExpandedPath, macros::MyConfigPrimitives};

#[derive(Clone, Debug, Default, MyConfigPrimitives)]
pub struct AppConfig {
	pub default_risk_percent_balance: Percent,
	pub default_sl: Percent,
	pub binance: BinanceConfig,
	pub bybit: BybitConfig,
	pub mexc: MexcConfig,
}

#[derive(Clone, Debug, Default, MyConfigPrimitives)]
pub struct BinanceConfig {
	pub key: String,
	pub secret: String,
}

#[derive(Clone, Debug, Default, MyConfigPrimitives)]
pub struct BybitConfig {
	pub key: String,
	pub secret: String,
}

#[derive(Clone, Debug, Default, MyConfigPrimitives)]
pub struct MexcConfig {
	pub key: String,
	pub secret: String,
}

impl AppConfig {
	pub fn read(path: Option<ExpandedPath>) -> Result<Self> {
		let app_name = env!("CARGO_PKG_NAME");
		let xdg_dirs = xdg::BaseDirectories::with_prefix(app_name).unwrap();
		let xdg_conf_dir = xdg_dirs.get_config_home().parent().unwrap().display().to_string();

		let locations = [
			format!("{xdg_conf_dir}/{app_name}"),
			format!("{xdg_conf_dir}/{app_name}/config"), //
		];

		let mut builder = config::Config::builder().add_source(config::Environment::default());

		match path {
			Some(path) => {
				let builder = builder.add_source(config::File::with_name(&path.to_string()).required(true));
				Ok(builder.build()?.try_deserialize()?)
			}
			None => {
				for location in locations.iter() {
					builder = builder.add_source(config::File::with_name(location).required(false));
				}
				let raw: config::Config = builder.build()?;

				match raw.try_deserialize() {
					Ok(config) => Ok(config),
					Err(e) => {
						eprintln!("Config file does not exist or is invalid:");
						Err(e.into())
					}
				}
			}
		}
	}
}
