//! The configuration of the device NAOqi runs, read when NAOqi loads the library.

use std::path::{Path, PathBuf};

use anyhow::Context;
use arora_hal_naoqi_dcm::DcmRobotConfig;
use serde::{Deserialize, Serialize};

/// The environment variable naming the configuration file.
pub const CONFIG_VAR: &str = "ARORA_NAOQI_CONFIG";

/// The name of the configuration file looked for beside the library and in its parent
/// directory.
pub const CONFIG_FILE: &str = "arora_naoqi.json";

/// The configuration of the device: its robot, its behavior and the device it is. Every field
/// is optional; without a file, the device runs the default behavior on the defaults.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfig {
    /// The robot, as the DCM HAL drives it.
    #[serde(default)]
    pub robot: DcmRobotConfig,
    /// The behavior tree: `ready` (the default behavior), `touch`, `none`, or the path of a
    /// Groot XML file.
    #[serde(default)]
    pub tree: Option<String>,
    /// The device's name (`default` when unset): it names its directory, and its Studio
    /// credential.
    #[serde(default)]
    pub device: Option<String>,
    /// The device directory, instead of the per-user directory of its name.
    #[serde(default)]
    pub device_dir: Option<PathBuf>,
    /// The port of the open local bridge (9000 when unset).
    #[serde(default)]
    pub bridge_port: Option<u16>,
    /// Module directories to load besides the device directory's.
    #[serde(default)]
    pub modules: Vec<PathBuf>,
    /// Whether the device connects to Semio Studio, in a library built with Studio (the
    /// default); `false` serves the open local bridge.
    #[serde(default)]
    pub studio: Option<bool>,
    /// The log filter of the device, as `RUST_LOG` (`info` when unset; `RUST_LOG` wins).
    #[serde(default)]
    pub log: Option<String>,
}

/// Which tree the device runs.
#[derive(Debug, PartialEq, Eq)]
pub enum Tree {
    None,
    Text(String),
}

impl ModuleConfig {
    /// The tree the configuration names.
    pub fn tree(&self) -> anyhow::Result<Tree> {
        Ok(match self.tree.as_deref() {
            None | Some("ready") => Tree::Text(arora_aldebaran::READY_TREE.to_string()),
            Some("touch") => Tree::Text(arora_aldebaran::TOUCH_TREE.to_string()),
            Some("none") => Tree::None,
            Some(path) => Tree::Text(
                std::fs::read_to_string(path)
                    .with_context(|| format!("failed to read the tree {path}"))?,
            ),
        })
    }

    /// The command line of a runner with this device: `arora::RunnerArgs` parses it, with the
    /// environment variables it reads for what it leaves out.
    pub fn runner_arguments(&self) -> Vec<String> {
        let mut args = vec!["arora-naoqi".to_string()];
        if let Some(device) = &self.device {
            args.extend(["--device".to_string(), device.clone()]);
        }
        if let Some(dir) = &self.device_dir {
            args.extend(["--device-dir".to_string(), dir.display().to_string()]);
        }
        if let Some(port) = self.bridge_port {
            args.extend(["--bridge-port".to_string(), port.to_string()]);
        }
        for module in &self.modules {
            args.extend(["--module".to_string(), module.display().to_string()]);
        }
        args
    }
}

/// Where the configuration is: the file [`CONFIG_VAR`] names, else the first
/// [`CONFIG_FILE`] found beside the library, then in its parent directory (the root of a
/// package whose libraries are in `lib/`); none when there is none.
pub fn find(library_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(CONFIG_VAR).filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let dir = library_dir?;
    [Some(dir), dir.parent()]
        .into_iter()
        .flatten()
        .map(|dir| dir.join(CONFIG_FILE))
        .find(|path| path.is_file())
}

/// Reads the configuration at `path`, or the default one.
pub fn load(path: Option<&Path>) -> anyhow::Result<ModuleConfig> {
    let Some(path) = path else {
        return Ok(ModuleConfig::default());
    };
    let file = std::fs::File::open(path)
        .with_context(|| format!("failed to open the configuration {}", path.display()))?;
    let config: ModuleConfig = serde_json::from_reader(file)
        .with_context(|| format!("failed to parse the configuration {}", path.display()))?;
    config
        .robot
        .validate()
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("invalid robot in {}", path.display()))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_runs_the_ready_tree() {
        let config: ModuleConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(
            config.tree().unwrap(),
            Tree::Text(arora_aldebaran::READY_TREE.to_string())
        );
        assert_eq!(config.runner_arguments(), ["arora-naoqi"]);
    }

    #[test]
    fn the_device_becomes_runner_arguments() {
        let config: ModuleConfig = serde_json::from_str(
            r#"{"device": "nao", "bridge_port": 9001, "modules": ["/m"], "tree": "none",
                "robot": {"joint_speed_limit": 2.0}}"#,
        )
        .unwrap();
        assert_eq!(config.tree().unwrap(), Tree::None);
        assert_eq!(
            config.runner_arguments(),
            [
                "arora-naoqi",
                "--device",
                "nao",
                "--bridge-port",
                "9001",
                "--module",
                "/m"
            ]
        );
        assert_eq!(config.robot.joint_speed_limit, 2.0);
    }

    #[test]
    fn the_file_is_found_beside_the_library_or_above() {
        let root = std::env::temp_dir().join(format!("arora-naoqi-config-{}", std::process::id()));
        let lib = root.join("lib");
        std::fs::create_dir_all(&lib).unwrap();
        assert_eq!(find(Some(&lib)), None);
        std::fs::write(root.join(CONFIG_FILE), "{}").unwrap();
        assert_eq!(find(Some(&lib)), Some(root.join(CONFIG_FILE)));
        std::fs::write(lib.join(CONFIG_FILE), "{\"tree\": \"touch\"}").unwrap();
        let found = find(Some(&lib)).unwrap();
        assert_eq!(found, lib.join(CONFIG_FILE));
        assert_eq!(load(Some(&found)).unwrap().tree.as_deref(), Some("touch"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
