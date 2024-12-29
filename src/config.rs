use color_eyre::eyre::Result;
use v_utils::macros::MyConfigPrimitives;

#[derive(Clone, Debug, Default, MyConfigPrimitives)]
pub struct AppConfig {
	test: String,
}

impl AppConfig {
	pub fn read() -> Result<Self> {
		let builder = config::Config::builder()
			.add_source(config::Environment::default())
			.add_source(config::File::with_name(&format!("{}/{}", env!("XDG_CONFIG_HOME"), env!("CARGO_PKG_NAME"))).required(true));

		let raw: config::Config = builder.build()?;
		Ok(raw.try_deserialize()?)
	}
}
