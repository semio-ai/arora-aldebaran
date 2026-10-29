//! NAOqi robots (NAO, Pepper) as Arora HALs, over the `qi` framework.
//!
//! One [`arora_hal::Hal`] implementation, [`NaoqiHal`], connects to a robot's NAOqi with
//! [`libqi-rs`](https://github.com/victorpaleologue/libqi-rs-vibe) and exposes it as an Arora
//! device: joint states, battery, inertial unit, sonars and touch sensors flow in as keys;
//! joint targets, stiffness, LEDs and base velocity commands flow out to the NAOqi services
//! (`ALMotion`, `ALLeds`). The keys are those of an Arora robot [`standard`], local to this
//! HAL; [`keys`] maps NAOqi onto them.
//!
//! Behavior trees speak in the robot's voice (`ALTextToSpeech`) through the [`say`] leaf,
//! once [`NaoqiHal::voice`] is installed.
//!
//! The robot is described by a [`NaoqiRobotConfig`] (address, credentials, joint id
//! mapping, sampling period); `configs/nao.json` is a sample.
//!
//! The device runner, which runs a robot with this HAL, is the `arora-aldebaran` crate: see
//! the [repository's README](https://github.com/semio-ai/arora-aldebaran#readme).

mod config;
pub use config::{
    JointIdMapping, NaoqiRobotConfig, NaoqiRobotConfigOverrides, SensorsConfig, SensorsOverrides,
};

mod conversions;
pub use conversions::{keys, JointTable};

mod naoqi_error;
pub use naoqi_error::NaoqiRobotError;

mod naoqi_hal;
pub use naoqi_hal::{NaoqiHal, Voice};

pub mod say;
pub mod services;
pub mod standard;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nao_sample_config_parses() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/configs/nao.json");
        let file = std::fs::File::open(path).expect("nao sample config should exist");
        let config: NaoqiRobotConfig =
            serde_json::from_reader(file).expect("nao sample config should parse");
        assert!(config.validate().is_ok());
        assert_eq!(config.model_family.as_deref(), Some("nao"));
    }
}
