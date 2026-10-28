use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::naoqi_error::NaoqiRobotError;

/// The configuration of a NAOqi robot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NaoqiRobotConfig {
    /// The address of the robot's NAOqi (`tcp://<host>:9559`).
    #[serde(default = "default_url")]
    pub url: String,
    /// The user of NAOqi authentication (NAOqi 2.5 and later), `nao` when a password is set
    /// without a user.
    #[serde(default)]
    pub user: Option<String>,
    /// The password of NAOqi authentication; none when the robot does not require it.
    #[serde(default)]
    pub password: Option<String>,
    /// The robot's model family, reported in the HAL description (queried from the robot
    /// when absent).
    #[serde(default)]
    pub model_family: Option<String>,
    /// The robot's hardware version, reported in the HAL description (queried when absent).
    #[serde(default)]
    pub hardware_version: Option<String>,
    /// The robot's software version, reported in the HAL description (queried when absent).
    #[serde(default)]
    pub software_version: Option<String>,
    /// How NAOqi joint names map to Arora joint ids.
    #[serde(default)]
    pub joint_ids: JointIdMapping,
    /// The period of the sampling of `ALMemory` sensors, in milliseconds.
    #[serde(default = "default_sensor_period_ms")]
    pub sensor_period_ms: u64,
    /// The fraction of the maximum joint speed used to reach targets (`ALMotion.setAngles`).
    #[serde(default = "default_joint_speed_fraction")]
    pub joint_speed_fraction: f64,
    /// Which sensor families are sampled.
    #[serde(default)]
    pub sensors: SensorsConfig,
    /// Turn Autonomous Life off when connecting (`ALAutonomousLife.setState("disabled")`),
    /// so that the robot's own life does not move the joints Arora drives.
    #[serde(default = "default_true")]
    pub disable_autonomous_life: bool,
}

fn default_url() -> String {
    "tcp://127.0.0.1:9559".to_string()
}

fn default_sensor_period_ms() -> u64 {
    50
}

fn default_joint_speed_fraction() -> f64 {
    0.3
}

impl Default for NaoqiRobotConfig {
    fn default() -> Self {
        Self {
            url: default_url(),
            user: None,
            password: None,
            model_family: None,
            hardware_version: None,
            software_version: None,
            joint_ids: JointIdMapping::default(),
            sensor_period_ms: default_sensor_period_ms(),
            joint_speed_fraction: default_joint_speed_fraction(),
            sensors: SensorsConfig::default(),
            disable_autonomous_life: true,
        }
    }
}

impl NaoqiRobotConfig {
    /// A configuration connecting to the given address with the defaults.
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), NaoqiRobotError> {
        if self.url.is_empty() {
            return Err(NaoqiRobotError::Config("URL cannot be empty".to_string()));
        }
        if !self.url.starts_with("tcp://") {
            return Err(NaoqiRobotError::Config(format!(
                "URL must be a plain TCP address (tcp://<host>:<port>), got {}",
                self.url
            )));
        }
        if self.sensor_period_ms == 0 {
            return Err(NaoqiRobotError::Config(
                "sensor_period_ms must be positive".to_string(),
            ));
        }
        if !(self.joint_speed_fraction > 0.0 && self.joint_speed_fraction <= 1.0) {
            return Err(NaoqiRobotError::Config(
                "joint_speed_fraction must be in (0, 1]".to_string(),
            ));
        }
        Ok(())
    }

    /// Apply overrides to the current configuration: what they set replaces the field, even
    /// with its default value; what they leave out stays as it is.
    pub fn apply_overrides(&mut self, overrides: NaoqiRobotConfigOverrides) {
        let NaoqiRobotConfigOverrides {
            url,
            user,
            password,
            model_family,
            hardware_version,
            software_version,
            joint_ids,
            sensor_period_ms,
            joint_speed_fraction,
            sensors,
            disable_autonomous_life,
        } = overrides;
        let replace = |field: &mut Option<String>, value: Option<String>| {
            if value.is_some() {
                *field = value;
            }
        };
        if let Some(url) = url {
            self.url = url;
        }
        replace(&mut self.user, user);
        replace(&mut self.password, password);
        replace(&mut self.model_family, model_family);
        replace(&mut self.hardware_version, hardware_version);
        replace(&mut self.software_version, software_version);
        if let Some(joint_ids) = joint_ids {
            self.joint_ids = joint_ids;
        }
        if let Some(period) = sensor_period_ms {
            self.sensor_period_ms = period;
        }
        if let Some(fraction) = joint_speed_fraction {
            self.joint_speed_fraction = fraction;
        }
        if let Some(sensors) = sensors {
            self.sensors.apply_overrides(sensors);
        }
        if let Some(disable) = disable_autonomous_life {
            self.disable_autonomous_life = disable;
        }
    }
}

/// Overrides of a [`NaoqiRobotConfig`]: each field it sets replaces the configuration's, each
/// field it leaves out keeps it. Same fields and JSON form as the configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NaoqiRobotConfigOverrides {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub model_family: Option<String>,
    #[serde(default)]
    pub hardware_version: Option<String>,
    #[serde(default)]
    pub software_version: Option<String>,
    #[serde(default)]
    pub joint_ids: Option<JointIdMapping>,
    #[serde(default)]
    pub sensor_period_ms: Option<u64>,
    #[serde(default)]
    pub joint_speed_fraction: Option<f64>,
    #[serde(default)]
    pub sensors: Option<SensorsOverrides>,
    #[serde(default)]
    pub disable_autonomous_life: Option<bool>,
}

/// How NAOqi joint names (`LShoulderPitch`) become Arora joint ids.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum JointIdMapping {
    /// The snake case of the NAOqi name: `LShoulderPitch` → `l_shoulder_pitch`.
    #[default]
    SnakeCase,
    /// Only the listed joints, with the given ids.
    Override { ids: HashMap<String, String> },
    /// The snake case ids, with the listed ones replaced.
    Extend { ids: HashMap<String, String> },
}

/// The sensor families sampled from the robot.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SensorsConfig {
    #[serde(default = "default_true")]
    pub joints: bool,
    #[serde(default = "default_true")]
    pub battery: bool,
    #[serde(default = "default_true")]
    pub inertial: bool,
    #[serde(default = "default_true")]
    pub sonar: bool,
    #[serde(default = "default_true")]
    pub touch: bool,
}

/// Overrides of a [`SensorsConfig`]: the families it names are switched, the others kept.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SensorsOverrides {
    #[serde(default)]
    pub joints: Option<bool>,
    #[serde(default)]
    pub battery: Option<bool>,
    #[serde(default)]
    pub inertial: Option<bool>,
    #[serde(default)]
    pub sonar: Option<bool>,
    #[serde(default)]
    pub touch: Option<bool>,
}

impl SensorsConfig {
    fn apply_overrides(&mut self, overrides: SensorsOverrides) {
        let SensorsOverrides {
            joints,
            battery,
            inertial,
            sonar,
            touch,
        } = overrides;
        for (field, value) in [
            (&mut self.joints, joints),
            (&mut self.battery, battery),
            (&mut self.inertial, inertial),
            (&mut self.sonar, sonar),
            (&mut self.touch, touch),
        ] {
            if let Some(value) = value {
                *field = value;
            }
        }
    }
}

fn default_true() -> bool {
    true
}

impl Default for SensorsConfig {
    fn default() -> Self {
        Self {
            joints: true,
            battery: true,
            inertial: true,
            sonar: true,
            touch: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_overridable() {
        let mut config = NaoqiRobotConfig::default();
        assert!(config.validate().is_ok());
        config.apply_overrides(NaoqiRobotConfigOverrides {
            url: Some("tcp://nao.local:9559".into()),
            password: Some("secret".into()),
            ..Default::default()
        });
        assert_eq!(config.url, "tcp://nao.local:9559");
        assert_eq!(config.password.as_deref(), Some("secret"));
        assert_eq!(config.sensor_period_ms, 50);
        assert!(
            config.disable_autonomous_life,
            "Autonomous Life is turned off by default"
        );
        config.apply_overrides(
            serde_json::from_str(r#"{"disable_autonomous_life": false}"#).unwrap(),
        );
        assert!(!config.disable_autonomous_life, "an override keeps it on");
        assert_eq!(config.password.as_deref(), Some("secret"));
    }

    /// Only what an override names changes, even when it names a default value.
    #[test]
    fn overrides_keep_what_they_leave_out() {
        let mut config = NaoqiRobotConfig {
            sensor_period_ms: 100,
            disable_autonomous_life: false,
            sensors: SensorsConfig {
                sonar: false,
                touch: false,
                ..Default::default()
            },
            ..Default::default()
        };
        config.apply_overrides(serde_json::from_str(r#"{"password": "secret"}"#).unwrap());
        assert!(!config.sensors.sonar && !config.sensors.touch);
        assert_eq!(config.sensor_period_ms, 100);
        config.apply_overrides(
            serde_json::from_str(
                r#"{"sensor_period_ms": 50, "disable_autonomous_life": true,
                    "sensors": {"touch": true}}"#,
            )
            .unwrap(),
        );
        assert_eq!(config.sensor_period_ms, 50);
        assert!(config.disable_autonomous_life);
        assert!(config.sensors.touch, "the named family is switched");
        assert!(!config.sensors.sonar, "the others are kept");
    }

    #[test]
    fn invalid_configurations_are_rejected() {
        assert!(NaoqiRobotConfig::with_url("").validate().is_err());
        assert!(NaoqiRobotConfig::with_url("tcps://nao:9503")
            .validate()
            .is_err());
        let config = NaoqiRobotConfig {
            sensor_period_ms: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
        let config = NaoqiRobotConfig {
            joint_speed_fraction: 1.5,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn joint_id_mapping_deserializes() {
        let mapping: JointIdMapping =
            serde_json::from_str(r#"{"mode": "extend", "ids": {"HeadYaw": "neck"}}"#).unwrap();
        assert_eq!(
            mapping,
            JointIdMapping::Extend {
                ids: HashMap::from([("HeadYaw".to_string(), "neck".to_string())])
            }
        );
    }
}
