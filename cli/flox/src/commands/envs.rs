use std::collections::BTreeSet;
use std::fmt::Display;
use std::path::Path;

use anyhow::Result;
use bpaf::Bpaf;
use crossterm::style::Stylize;
use flox_rust_sdk::data::CanonicalPath;
use flox_rust_sdk::flox::Flox;
use flox_rust_sdk::models::environment::DotFlox;
use flox_rust_sdk::models::link_registry::{LinkRegistry, RegistryKey};
use serde_json::json;
use tracing::instrument;

use super::UninitializedEnvironment;
use crate::commands::activated_environments;
use crate::subcommand_metric;
use crate::utils::message;

#[derive(Bpaf, Debug, Clone)]
pub struct Envs {
    #[bpaf(long)]
    active: bool,
    #[bpaf(long)]
    json: bool,
}

impl Envs {
    /// List all environments
    ///
    /// If `--json` is passed, dispatch to [Self::handle_json]
    ///
    /// If `--active` is passed, print only the active environments
    /// Always prints headers and formats the output.
    #[instrument(name = "envs")]
    pub fn handle(self, flox: Flox) -> Result<()> {
        subcommand_metric!("envs");

        let active = activated_environments();

        if self.json {
            return self.handle_json(flox);
        }

        if self.active {
            if active.last_active().is_none() {
                message::plain("No active environments");
                return Ok(());
            }

            message::plain("Active environments:");
            println!("{}", DisplayEnvironments::new(active.iter(), true));
            return Ok(());
        }

        let inactive = get_inactive_environments(&RegisteredEnvironments::new(&flox)?)?;

        if active.iter().next().is_none() && inactive.is_empty() {
            message::plain("No environments known to Flox");
        }

        if active.iter().next().is_some() {
            message::plain("Active environments:");
            println!("{}", DisplayEnvironments::new(active.iter(), true));
        }

        if !inactive.is_empty() {
            message::plain("Inactive environments:");
            println!("{}", DisplayEnvironments::new(inactive.iter(), false));
        }

        Ok(())
    }

    /// Print the list of environments in JSON format
    fn handle_json(&self, flox: Flox) -> Result<()> {
        let active = activated_environments();

        if self.active {
            println!("{:#}", json!(active));
            return Ok(());
        }

        let inactive = get_inactive_environments(&RegisteredEnvironments::new(&flox)?)?;

        println!(
            "{:#}",
            json!({
                "active": active,
                "inactive": inactive,
            })
        );

        Ok(())
    }
}

struct DisplayEnvironments<'a> {
    envs: Vec<&'a UninitializedEnvironment>,
    format_active: bool,
}

impl<'a> DisplayEnvironments<'a> {
    fn new(
        envs: impl IntoIterator<Item = &'a UninitializedEnvironment>,
        format_active: bool,
    ) -> Self {
        Self {
            envs: envs.into_iter().collect(),
            format_active,
        }
    }
}

impl<'a> Display for DisplayEnvironments<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let widest = self
            .envs
            .iter()
            .map(|env| env.name().as_ref().len())
            .max()
            .unwrap_or(0);

        let mut envs = self.envs.iter();

        if self.format_active {
            let Some(first) = envs.next() else {
                return Ok(());
            };
            let first_formatted =
                format!("{:<widest$}  {}", first.name(), format_path(first.path())).bold();
            writeln!(f, "{first_formatted}")?;
        }

        for env in envs {
            write!(f, "{:<widest$}  {}", env.name(), format_path(env.path()))?;
        }

        Ok(())
    }
}

fn format_path(path: Option<&Path>) -> String {
    path.map(|p| p.display().to_string())
        .unwrap_or_else(|| "(remote)".to_string())
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

/// Get the list of environments that are not active
fn get_inactive_environments(
    registry: &RegisteredEnvironments,
) -> Result<BTreeSet<UninitializedEnvironment>> {
    let active = activated_environments();

    let inactive = {
        let mut available = registry.try_iter()?.collect::<BTreeSet<_>>();

        for active in active.iter() {
            available.remove(active);
        }
        available
    };

    Ok(inactive)
}
