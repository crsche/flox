use std::{fs::File, path::PathBuf};

use runix::{
    arguments::{common::NixCommonArgs, config::NixConfigArgs},
    command_line::{DefaultArgs, NixCommandLine},
    installable::Installable,
    NixBackend,
};

use crate::{
    actions::environment::Environment,
    actions::{environment::EnvironmentError, package::Package},
    environment::{self, build_flox_env},
    models::channels::ChannelRegistry,
    prelude::Stability,
    providers::git::GitProvider,
};

use runix::arguments::{eval::EvaluationArgs, flake::FlakeArgs};

/// The main API struct for our flox implementation
///
/// A [Flox] instance serves as the context for nix invocations
/// and possibly other tools such as git.
/// As a CLI application one invocation of `flox` would run on the same instance
/// but may call different methods.
///
/// [Flox] will provide a preconfigured instance of the Nix API.
/// By default this nix API uses the nix CLI.
/// Preconfiguration includes environment variables and flox specific arguments.
#[derive(Debug)]
pub struct Flox {
    /// The directory pointing to the users flox configuration
    ///
    /// TODO: set a default in the lib or CLI?
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub temp_dir: PathBuf,

    pub channels: ChannelRegistry,

    /// Whether to collect metrics of any kind
    /// (yet to be made use of)
    pub collect_metrics: bool,

    pub system: String,
}

pub trait FloxNixApi: NixBackend {
    fn new(flox: &Flox, default_nix_args: DefaultArgs) -> Self;
}

impl FloxNixApi for NixCommandLine {
    fn new(_: &Flox, default_nix_args: DefaultArgs) -> NixCommandLine {
        NixCommandLine {
            nix_bin: Some(environment::NIX_BIN.to_string()),
            defaults: default_nix_args,
        }
    }
}

impl Flox {
    /// Provide the package scope to interact with raw packages, (build, develop, etc)
    pub fn package(&self, installable: Installable, stability: Stability) -> Package {
        Package::new(self, installable, stability)
    }

    pub fn environment(&self, dir: PathBuf) -> Result<Environment, EnvironmentError> {
        Environment::new(self, dir)
    }

    /// Produce a new Nix Backend
    ///
    /// This method performs backend independen configuration of nix
    /// and passes itself and the default config to the constructor of the Nix Backend
    ///
    /// The constructor will perform backend specifc configuration measures
    /// and return a fresh initialized backend.
    pub fn nix<Nix: FloxNixApi>(&self) -> Nix {
        // Set the registry file as default argument
        let registry_file = self.temp_dir.join("registry.json");
        serde_json::to_writer(File::create(&registry_file).unwrap(), &self.channels).unwrap();

        let config_args = NixConfigArgs {
            extra_experimental_features: ["nix-command", "flakes"]
                .map(String::from)
                .to_vec()
                .into(),

            extra_substituters: ["https://cache.floxdev.com?trusted=1"]
                .map(String::from)
                .to_vec()
                .into(),

            flake_registry: Some(registry_file.into()),

            ..Default::default()
        };

        let common_args = NixCommonArgs {
            ..Default::default()
        };

        let default_nix_args = DefaultArgs {
            environment: build_flox_env(),
            config_args,
            common_args,
            ..Default::default()
        };

        Nix::new(self, default_nix_args)
    }

    /// Initialize and provide a git abstraction
    pub fn git_provider<Git: GitProvider>(&self) -> Git {
        Git::new()
    }
}