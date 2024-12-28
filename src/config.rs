use std::path::Path;

use color_eyre::eyre::{Result, bail};
use v_utils::macros::MyConfigPrimitives;

#[derive(Clone, Debug, Default, MyConfigPrimitives)]
pub struct AppConfig {}

impl AppConfig {
	pub fn read(path: &Path) -> Result<Self> {
		match path.exists() {
			true => {
				let builder = config::Config::builder().add_source(config::File::with_name(path.to_str().unwrap()));
				let raw: config::Config = builder.build()?;
				Ok(raw.try_deserialize()?)
			}
			false => bail!("Config file does not exist: {:?}", path),
		}
	}
}
