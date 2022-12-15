use std::{collections::HashMap, env, path::PathBuf};

use anyhow::{Context, Result};
use config::{Config as HierarchicalConfig, Environment, Value};
use itertools::{Either, Itertools};
use log::debug;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

/// Name of flox managed directories (config, data, cache)
const FLOX_DIR_NAME: &'_ str = "flox-preview";

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Config {
    /// flox configuration options
    #[serde(default, flatten)]
    pub flox: FloxConfig,

    /// nix configuration options
    #[serde(default)]
    pub nix: NixConfig,

    /// github configuration options
    #[serde(default)]
    pub github: GithubConfig,

    #[serde(default)]
    pub features: HashMap<Feature, Impl>,
}

// TODO: move to flox_sdk?
/// Describes the Configuration for the flox library
#[derive(Clone, Debug, Deserialize, Default)]
pub struct FloxConfig {
    /// Control telemetry for the rust CLI
    ///
    /// An [Option] since tri-state:
    /// - Some(true): User said yes
    /// - Some(false): User said no
    /// - None: Didn't ask the user yet - required to decide whether to ask or not
    pub allow_telemetry: Option<bool>,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

// TODO: move to runix?
/// Describes the nix config under flox
#[derive(Clone, Debug, Deserialize, Default)]
pub struct NixConfig {}

/// Describes the github config under flox
#[derive(Clone, Debug, Deserialize, Default)]
pub struct GithubConfig {}

#[derive(Clone, Debug, PartialEq, Eq, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Impl {
    Rust,
    Bash,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Hash)]
pub enum Feature {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "env")]
    Env,
    #[serde(rename = "nix")]
    Nix,
}

impl Feature {
    pub fn implementation(&self) -> Result<Impl> {
        let map = Config::parse()?.features;

        Ok(match self {
            Feature::All => *map.get(self).unwrap_or(&Impl::Bash),
            Feature::Env => *map.get(self).or(map.get(&Self::All)).unwrap_or(&Impl::Bash),
            Feature::Nix => *map.get(self).or(map.get(&Self::All)).unwrap_or(&Impl::Rust),
        })
    }
}

impl Config {
    /// Creates a raw [Config] object and caches it for the lifetime of the program
    fn raw_config<'a>() -> Result<&'a HierarchicalConfig> {
        static INSTANCE: OnceCell<HierarchicalConfig> = OnceCell::new();
        INSTANCE.get_or_try_init(|| {
            let cache_dir = dirs::cache_dir().unwrap().join(FLOX_DIR_NAME);
            let data_dir = dirs::data_dir().unwrap().join(FLOX_DIR_NAME);
            let config_dir = match env::var("FLOX_PREVIEW_CONFIG_DIR") {
                Ok(v) => v.into(),
                Err(_) => {
                    debug!("`FLOX_PREVIEW_CONFIG_DIR` not set");
                    let config_dir = dirs::config_dir().unwrap();
                    config_dir.join(FLOX_DIR_NAME)
                }
            };

            let builder = HierarchicalConfig::builder()
                .set_default("cache_dir", cache_dir.to_str().unwrap())?
                .set_default("data_dir", data_dir.to_str().unwrap())?
                .set_default("config_dir", config_dir.to_str().unwrap())?
                .add_source(
                    config::File::with_name(config_dir.join("flox").to_str().unwrap())
                        .required(false),
                );

            let mut flox_envs = env::vars()
                .filter_map(|(k, v)| k.strip_prefix("FLOX_PREVIEW_").map(|k| (k.to_owned(), v)))
                .collect::<Vec<_>>();

            let builder = builder
                .add_source(mk_environment(&mut flox_envs, "NIX"))
                .add_source(mk_environment(&mut flox_envs, "GITHUB"))
                .add_source(mk_environment(&mut flox_envs, "FEATURES"))
                .add_source(Environment::default().source(Some(HashMap::from_iter(flox_envs))));

            let final_config = builder.build()?;

            let final_config = HierarchicalConfig::builder()
                .add_source(final_config)
                .build()?;

            Ok(final_config)
        })
    }

    /// Creates a [Config] from the environment and config file
    pub fn parse() -> Result<Config> {
        let final_config = Self::raw_config()?;
        let cli_confg: Config = final_config
            .to_owned()
            .try_deserialize()
            .context("Could not parse config")?;
        Ok(cli_confg)
    }
}

fn mk_environment(envs: &mut Vec<(String, String)>, prefix: &str) -> Environment {
    let (prefixed_envs, flox_envs): (HashMap<String, String>, Vec<(String, String)>) = envs
        .iter()
        .partition_map(|(k, v)| match k.strip_prefix(&format!("{prefix}_")) {
            Some(suffix) => Either::Left((format!("{prefix}#{suffix}"), v.to_owned())),
            None => Either::Right((k.to_owned(), v.to_owned())),
        });
    let environment = Environment::with_prefix(prefix)
        .keep_prefix(true)
        .separator("#")
        .source(Some(prefixed_envs));
    *envs = flox_envs;
    environment
}
