use anyhow::Result;
use bpaf::Bpaf;
use flox_rust_sdk::data::CanonicalPath;
use flox_rust_sdk::flox::Flox;
use flox_rust_sdk::models::environment::DotFlox;
use flox_rust_sdk::models::link_registry::{LinkRegistry, RegistryKey};
use serde_json::json;
use tracing::instrument;

use super::UninitializedEnvironment;
use crate::commands::activated_environments;
use crate::subcommand_metric;

#[derive(Bpaf, Clone)]
pub struct Envs {
    #[bpaf(long, short)]
    active: bool,
    #[bpaf(long)]
    json: bool,
}

impl Envs {
    #[instrument(name = "envs", skip_all)]
    pub fn handle(self, flox: Flox) -> Result<()> {
        subcommand_metric!("envs");

        let active = activated_environments();
        let available = RegisteredEnvironments::new(&flox)?;

        println!(
            "{}",
            json!({
                "active": active,
                "available": available.try_iter()?.collect::<Vec<_>>()
            })
        );

        Ok(())
    }
}

pub struct RegisteredEnvironments {
    registry: LinkRegistry,
}

impl RegisteredEnvironments {
    pub fn new(flox: &Flox) -> Result<Self> {
        let registry = LinkRegistry::open(flox.cache_dir.join("registered_environments"))?;

        Ok(registry.into())
    }

    pub fn register(&self, env: &UninitializedEnvironment) -> Result<()> {
        let Some(path) = env.path() else {
            return Ok(());
        };

        let canonical_path = CanonicalPath::new(path)?;

        self.registry.register(&canonical_path).map(|_| ())?;
        Ok(())
    }

    pub fn unregister(&self, key: RegistryKey) -> Result<()> {
        self.registry.unregister(key).map(|_| ())?;
        Ok(())
    }

    fn try_iter(&self) -> Result<impl Iterator<Item = UninitializedEnvironment>> {
        let iter = self.registry.try_iter()?.filter_map(|entry| {
            let dot_flox = DotFlox::open(entry.path()).ok()?;
            Some(UninitializedEnvironment::DotFlox(dot_flox))
        });
        Ok(iter)
    }
}

impl From<LinkRegistry> for RegisteredEnvironments {
    fn from(registry: LinkRegistry) -> Self {
        Self { registry }
    }
}
