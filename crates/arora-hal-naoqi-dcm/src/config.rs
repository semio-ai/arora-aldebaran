use arora_hal_naoqi::{JointIdMapping, SensorsConfig};
use serde::{Deserialize, Serialize};

/// The configuration of a robot driven from the loop of its DCM.
///
/// The JSON form has the fields below, each optional; `{}` is a NAO or a Pepper with the
/// defaults.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DcmRobotConfig {
    /// The address of the NAOqi the HAL calls for what is not a key (speech, Autonomous
    /// Life, the rest posture, the description): the NAOqi it runs in.
    #[serde(default = "default_url")]
    pub url: String,
    /// The user of NAOqi authentication (NAOqi 2.5 and later), `nao` when a password is set
    /// without a user.
    #[serde(default)]
    pub user: Option<String>,
    /// The password of NAOqi authentication; none when the robot does not require it.
    #[serde(default)]
    pub password: Option<String>,
    /// The robot's model family, reported in the HAL description (queried when absent).
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
    /// Which sensor families are read.
    #[serde(default)]
    pub sensors: SensorsConfig,
    /// The fastest a commanded joint moves toward its target, in radians per second (a
    /// hand's opening per second): the DCM applies a position at once, so the HAL steps the
    /// commanded position toward the target every cycle, at most this far.
    #[serde(default = "default_joint_speed_limit")]
    pub joint_speed_limit: f64,
    /// How long a stiffness command takes to reach its value, in milliseconds (the DCM
    /// interpolates).
    #[serde(default = "default_stiffness_ramp_ms")]
    pub stiffness_ramp_ms: u32,
    /// Turn Autonomous Life off when the HAL starts, so that the robot's own life does not
    /// move the joints Arora drives.
    #[serde(default = "default_true")]
    pub disable_autonomous_life: bool,
    /// Put the robot to rest when the HAL starts (`ALMotion.rest`): `ALMotion` then sends no
    /// command to the joints, which are Arora's until something wakes `ALMotion` up.
    #[serde(default = "default_true")]
    pub rest_at_start: bool,
    /// Put the robot to rest when the HAL goes (`ALMotion.rest`): it sits or crouches, then
    /// its joints are relaxed, so that a device that stops leaves no joint stiffened.
    #[serde(default = "default_true")]
    pub rest_on_exit: bool,
}

fn default_url() -> String {
    "tcp://127.0.0.1:9559".to_string()
}

fn default_joint_speed_limit() -> f64 {
    1.0
}

fn default_stiffness_ramp_ms() -> u32 {
    100
}

fn default_true() -> bool {
    true
}

impl Default for DcmRobotConfig {
    fn default() -> Self {
        Self {
            url: default_url(),
            user: None,
            password: None,
            model_family: None,
            hardware_version: None,
            software_version: None,
            joint_ids: JointIdMapping::default(),
            sensors: SensorsConfig::default(),
            joint_speed_limit: default_joint_speed_limit(),
            stiffness_ramp_ms: default_stiffness_ramp_ms(),
            disable_autonomous_life: true,
            rest_at_start: true,
            rest_on_exit: true,
        }
    }
}

impl DcmRobotConfig {
    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), String> {
        if !self.url.starts_with("tcp://") {
            return Err(format!(
                "url must be a plain TCP address (tcp://<host>:<port>), got {:?}",
                self.url
            ));
        }
        if !(self.joint_speed_limit.is_finite() && self.joint_speed_limit > 0.0) {
            return Err(format!(
                "joint_speed_limit must be positive, got {}",
                self.joint_speed_limit
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_configuration_is_the_default() {
        let config: DcmRobotConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, DcmRobotConfig::default());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn unknown_fields_and_bad_values_are_refused() {
        assert!(serde_json::from_str::<DcmRobotConfig>(r#"{"sensor_period_ms": 10}"#).is_err());
        let config: DcmRobotConfig = serde_json::from_str(r#"{"joint_speed_limit": 0}"#).unwrap();
        assert!(config.validate().unwrap_err().contains("joint_speed_limit"));
    }
}
