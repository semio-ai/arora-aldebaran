//! The robot, through the HAL the build selects with a feature: `naoqi`, NAOqi's services
//! over the network (the default). Each HAL gives the same three things: how a run names the
//! robot (an address or a configuration file, with optional overrides), the HAL connected to
//! it, and the host modules of its own leaves (its voice, for `Say`).

use arora::HostModule;
use arora_hal::Hal;

#[cfg(not(feature = "naoqi"))]
compile_error!("arora-aldebaran drives the robot through a HAL: build it with the `naoqi` feature (the default)");

/// A robot connected through the build's HAL.
pub struct Robot {
    /// The HAL, for the device.
    pub hal: Box<dyn Hal>,
    /// The host modules of the HAL's own leaves, for the device's trees.
    pub modules: Vec<HostModule>,
}

impl Robot {
    /// Gives the device the robot: its HAL, and the host modules of the HAL's leaves.
    pub fn install(self, builder: arora::AroraBuilder) -> arora::AroraBuilder {
        self.modules
            .into_iter()
            .fold(builder.with_hal(self.hal), |builder, module| {
                builder.with_host_module(module)
            })
    }
}

/// The HAL this build drives the robot with.
pub const HAL: &str = HAL_NAME;

/// What the robot argument of a run is, for the command line's help.
pub const ROBOT_HELP: &str = ROBOT_ARG_HELP;

#[cfg(feature = "naoqi")]
mod naoqi {
    use anyhow::Context;
    use arora::HostModule;
    use arora_hal_naoqi::say::{self, naoqi_say};
    use arora_hal_naoqi::{NaoqiHal, NaoqiRobotConfig, NaoqiRobotConfigOverrides};

    use super::Robot;

    pub(super) const HAL_NAME: &str = "NAOqi (arora-hal-naoqi)";
    pub(super) const ROBOT_ARG_HELP: &str =
        "its NAOqi's address (tcp://<host>:9559) or a NaoqiRobotConfig JSON file";

    /// Connects to the robot `robot` names (its NAOqi's address, or a configuration file),
    /// with `overrides` applied (an address, or a file of the fields to replace), and installs
    /// its voice for the `Say` leaves. Must be called within the Tokio runtime the device
    /// runs on: the HAL's tasks spawn on it.
    pub async fn connect(robot: &str, overrides: Option<&str>) -> anyhow::Result<Robot> {
        let mut config = load_config(robot)?;
        if let Some(overrides) = overrides {
            config.apply_overrides(load_overrides(overrides)?);
        }
        let hal = NaoqiHal::new(config)
            .await
            .map_err(|e| anyhow::anyhow!("failed to start the NAOqi HAL: {e}"))?;
        say::install(hal.voice());
        Ok(Robot {
            hal: Box::new(hal),
            modules: vec![HostModule::of::<naoqi_say::Module>()],
        })
    }

    fn load_config(selector: &str) -> anyhow::Result<NaoqiRobotConfig> {
        if is_address(selector) {
            return Ok(NaoqiRobotConfig::with_url(selector));
        }
        read_json(selector)
    }

    /// Overrides from an address (it replaces the robot's) or a JSON file (what it sets
    /// replaces the robot's configuration, the rest is kept).
    fn load_overrides(selector: &str) -> anyhow::Result<NaoqiRobotConfigOverrides> {
        if is_address(selector) {
            return Ok(NaoqiRobotConfigOverrides {
                url: Some(selector.to_string()),
                ..Default::default()
            });
        }
        read_json(selector)
    }

    fn is_address(selector: &str) -> bool {
        selector.starts_with("tcp://") || selector.starts_with("tcps://")
    }

    fn read_json<T: serde::de::DeserializeOwned>(path: &str) -> anyhow::Result<T> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("failed to open robot config {path}"))?;
        serde_json::from_reader(file)
            .with_context(|| format!("failed to parse robot config {path}"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn an_address_or_a_file_names_the_robot() {
            let config = load_config("tcp://nao.local:9559").unwrap();
            assert_eq!(config.url, "tcp://nao.local:9559");
            let overrides = load_overrides("tcp://other.local:9559").unwrap();
            assert_eq!(overrides.url.as_deref(), Some("tcp://other.local:9559"));
            let error = load_config("/no/such/robot.json").unwrap_err();
            assert!(error.to_string().contains("/no/such/robot.json"), "{error}");
        }
    }
}

#[cfg(feature = "naoqi")]
pub use naoqi::connect;
#[cfg(feature = "naoqi")]
use naoqi::{HAL_NAME, ROBOT_ARG_HELP};
