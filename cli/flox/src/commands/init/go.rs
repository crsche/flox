use std::fs;
use std::path::Path;

use anyhow::{anyhow, Result};
use flox_rust_sdk::flox::Flox;
use flox_rust_sdk::models::environment::path_environment::InitCustomization;
use flox_rust_sdk::models::manifest::PackageToInstall;
use flox_rust_sdk::models::search::SearchResult;
use indoc::{formatdoc, indoc};

use super::{format_customization, InitHook, AUTO_SETUP_HINT};
use crate::utils::dialog::{Dialog, Select};
use crate::utils::message;

const GO_MOD_FILENAME: &str = "go.mod";
const GO_WORK_FILENAME: &str = "go.work";

const GO_HOOK: &str = indoc! {"
    # Install Go depedencies
    go get ."
};

/// The Go hook handles installation and configuration suggestions
/// for projects using Go. It works as follows:
///
/// - [Self::new]: Detect [GoModuleSystem] files in the current working directory
/// - [Self::should_run]: Returns whether a valid module system was detected
///   in the current working directory, i.e. `false` if the [Self::module_system]
///   is [GoModuleSystem::None], else returns `true`
/// - [Self::prompt_user]: If the action is [NodeAction::InstallYarnOrNode],
///   set it to one of [NodeAction::InstallNode] or [NodeAction::InstallYarn].
///   Otherwise, just return true or false based on whether the user wants the
///   customization
/// - [Self::get_init_customization]: Return a customization based on
///   [Self::module_system]
pub(super) struct Go {
    /// Stores what customization should be generated by
    /// [Self::get_init_customization].
    ///
    /// Initialized in [Self::new] and potentially modified by [Self::prompt_user].
    module_system: Option<GoModuleSystemKind>,
}

impl Go {
    pub fn new(path: &Path, _flox: &Flox) -> Result<Self> {
        let module_system = Self::detect_module_system(path)?;

        Ok(Self { module_system })
    }

    /// Since go.work files declare workspaces comprised of multiple
    /// go.mod files, they precede over any other go.mod file found.
    fn detect_module_system(path: &Path) -> Result<Option<GoModuleSystemKind>> {
        if let Some(go_work) = GoWorkspace::try_new_from_path(path)? {
            return Ok(Some(GoModuleSystemKind::Workspace(go_work)));
        }

        if let Some(go_mod) = GoModule::try_new_from_path(path)? {
            return Ok(Some(GoModuleSystemKind::Module(go_mod)));
        }

        Ok(None)
    }
}

impl InitHook for Go {
    /// Returns `true` if any valid module system file was found
    ///
    /// [Self::prompt_user] and [Self::get_init_customization]
    /// are expected to be called only if this method returns `true`!
    fn should_run(&mut self, _path: &Path) -> Result<bool> {
        todo!("Ensure that the module system has a valid, specified version");
    }

    fn prompt_user(&mut self, path: &Path, flox: &Flox) -> Result<bool> {
        let module_system: &dyn GoModuleSystem = match &self.module_system {
            Some(GoModuleSystemKind::Module(_mod)) => _mod,
            Some(GoModuleSystemKind::Workspace(_work)) => _work,
            None => unreachable!(),
        };

        message::plain(formatdoc! {
            "
            Flox detected a {} file in the current directory.

            Go projects typically need:
            * Go
            * A shell hook to apply environment variables

        ", module_system.get_filename()});

        let message = formatdoc! {"
        Would you like Flox to apply the standard Go environment? 
        You can always revisit the environment's declaration with 'flox edit'"};

        let accept_options = ["Yes".to_string()];
        let cancel_options = ["No".to_string()];

        // TODO: see: cli/flox-rust-sdk/src/models/environment/generations.rs:90
        let show_environment_manifest_option = ["Show environment manifest".to_string()];

        let options = accept_options
            .iter()
            .chain(cancel_options.iter())
            .chain(show_environment_manifest_option.iter())
            .collect::<Vec<_>>();

        loop {
            let dialog = Dialog {
                message: &message,
                help_message: Some(AUTO_SETUP_HINT),
                typed: Select {
                    options: options.clone(),
                },
            };

            let (choice, _) = dialog.raw_prompt()?;

            match choice {
                0 => return Ok(true),
                1 => return Ok(false),
                2 => todo!(),
                _ => unreachable!(),
            }
        }
    }

    fn get_init_customization(&self) -> InitCustomization {
        let package = PackageToInstall {
            id: "go".to_string(),
            pkg_path: "".to_string(),
            version: Some("".to_string()),
            input: None,
        };

        let profile = Some(GO_HOOK.to_string());

        InitCustomization {
            profile,
            packages: Some(vec![package]),
        }
    }
}

#[derive(PartialEq, Clone)]
enum GoVersion {
    /// module exists but doesn't specify a version
    Unspecified,
    /// module specifies a version, but flox can't provide it
    Unavailable,
    /// module specifies a version,
    /// and we found a search result satisfying that version constraint
    Found(Box<SearchResult>),
}

impl GoVersion {
    fn from_module_system_contents(contents: String) -> Self {
        let version = GoVersion::Unspecified;
        let Some(version_str) = GoVersion::get_version_from_contents(&contents) else {
            return version;
        };

        version
    }

    fn get_version_from_contents<'a>(contents: &'a String) -> Option<&'a str> {
        let Some(version_line) = contents
            .lines()
            .skip_while(|line| (**line).trim_start().starts_with("go"))
            .next()
        else {
            return None;
        };

        let version_str = version_line.split_whitespace().nth(1);
        version_str
    }
}

/// Represents Go module system files.
#[derive(PartialEq)]
enum GoModuleSystemKind {
    /// Go module file [GoMod].
    Module(GoModule),
    /// Go workspace file [GoWork].
    Workspace(GoWorkspace),
}

trait GoModuleSystem {
    fn new(module_contents: String) -> Self
    where
        Self: Sized;
    fn try_new_from_path(path: &Path) -> Result<Option<Self>>
    where
        Self: Sized;

    fn get_filename(&self) -> &'static str;
    fn get_version(&self) -> GoVersion;
}

impl PartialEq for dyn GoModuleSystem {
    fn eq(&self, other: &Self) -> bool {
        self.get_version() == other.get_version()
    }
}

#[derive(PartialEq)]
struct GoModule {
    version: GoVersion,
}

impl GoModuleSystem for GoModule {
    fn new(module_contents: String) -> Self {
        let version = GoVersion::from_module_system_contents(module_contents);
        Self { version }
    }

    fn try_new_from_path(path: &Path) -> Result<Option<Self>> {
        let mod_path = path.join(GO_MOD_FILENAME);
        if !mod_path.exists() {
            return Ok(None);
        }

        let mod_contents = fs::read_to_string(mod_path)?;
        let go_module = Self::new(mod_contents);
        Ok(Some(go_module))
    }

    #[inline(always)]
    fn get_filename(&self) -> &'static str {
        GO_MOD_FILENAME
    }

    fn get_version(&self) -> GoVersion {
        self.version.clone()
    }
}

#[derive(PartialEq)]
struct GoWorkspace {
    version: GoVersion,
}

impl GoModuleSystem for GoWorkspace {
    fn new(workspace_contents: String) -> Self {
        let version = GoVersion::from_module_system_contents(workspace_contents);
        Self { version }
    }

    fn try_new_from_path(path: &Path) -> Result<Option<Self>> {
        let work_path = path.join(GO_WORK_FILENAME);
        if !work_path.exists() {
            return Ok(None);
        }

        let work_contents = fs::read_to_string(work_path)?;
        let go_workspace = Self::new(work_contents);
        Ok(Some(go_workspace))
    }

    #[inline(always)]
    fn get_filename(&self) -> &'static str {
        GO_WORK_FILENAME
    }

    fn get_version(&self) -> GoVersion {
        self.version.clone()
    }
}

mod tests {}
