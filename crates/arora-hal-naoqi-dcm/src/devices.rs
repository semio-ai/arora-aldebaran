//! The devices of the DCM the HAL knows: the joints, touch sensors and LED groups of NAO and
//! Pepper, by their `ALMemory` keys. The HAL probes them when it is created and keeps those the
//! robot has, so one table serves both robots and their versions.

use arora_hal_naoqi::keys;

/// The prefix of the `ALMemory` keys of the devices of the DCM.
pub const DEVICE_PREFIX: &str = "Device/SubDeviceList/";

/// The joints of NAO, then those only Pepper has, in the order of
/// `ALMotion.getBodyNames("Body")` on NAO. The wheels of Pepper are not position joints and
/// are left out.
pub const JOINTS: &[&str] = &[
    "HeadYaw",
    "HeadPitch",
    "LShoulderPitch",
    "LShoulderRoll",
    "LElbowYaw",
    "LElbowRoll",
    "LWristYaw",
    "LHand",
    "LHipYawPitch",
    "LHipRoll",
    "LHipPitch",
    "LKneePitch",
    "LAnklePitch",
    "LAnkleRoll",
    "RHipYawPitch",
    "RHipRoll",
    "RHipPitch",
    "RKneePitch",
    "RAnklePitch",
    "RAnkleRoll",
    "RShoulderPitch",
    "RShoulderRoll",
    "RElbowYaw",
    "RElbowRoll",
    "RWristYaw",
    "RHand",
    "HipRoll",
    "HipPitch",
    "KneePitch",
];

/// The `ALMemory` key of a value of a device: `Device/SubDeviceList/<path>/Value`.
pub fn device_key(path: &str) -> String {
    format!("{DEVICE_PREFIX}{path}/Value")
}

/// The touch sensors and buttons, with the boolean Arora key each drives: the key is `true`
/// while any of its sensors reads 1.
pub const TOUCH_SENSORS: &[(&str, &[&str])] = &[
    ("touch.head.front", &["Head/Touch/Front/Sensor"]),
    ("touch.head.middle", &["Head/Touch/Middle/Sensor"]),
    ("touch.head.rear", &["Head/Touch/Rear/Sensor"]),
    ("touch.hand.left.back", &["LHand/Touch/Back/Sensor"]),
    ("touch.hand.left.left", &["LHand/Touch/Left/Sensor"]),
    ("touch.hand.left.right", &["LHand/Touch/Right/Sensor"]),
    ("touch.hand.right.back", &["RHand/Touch/Back/Sensor"]),
    ("touch.hand.right.left", &["RHand/Touch/Left/Sensor"]),
    ("touch.hand.right.right", &["RHand/Touch/Right/Sensor"]),
    (
        "bumper.left",
        &["LFoot/Bumper/Left/Sensor", "LFoot/Bumper/Right/Sensor"],
    ),
    (
        "bumper.right",
        &["RFoot/Bumper/Left/Sensor", "RFoot/Bumper/Right/Sensor"],
    ),
    ("chest_button", &["ChestBoard/Button/Sensor"]),
];

/// What an LED shows of a group's color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Red,
    Green,
    Blue,
    /// A single-color LED (the ears, the head): the brightest component of the color.
    Mono,
}

impl Channel {
    /// The level of this channel for a color `0xRRGGBB` at an intensity, from 0 to 1.
    pub fn level(self, color: u32, intensity: f32) -> f32 {
        let [_, red, green, blue] = color.to_be_bytes();
        let component = match self {
            Channel::Red => red,
            Channel::Green => green,
            Channel::Blue => blue,
            Channel::Mono => red.max(green).max(blue),
        };
        f32::from(component) / 255.0 * intensity.clamp(0.0, 1.0)
    }
}

/// An LED group, by its `ALLeds` name: the actuators of its LEDs and the channel of each.
#[derive(Debug, Clone, PartialEq)]
pub struct LedGroup {
    pub name: &'static str,
    pub actuators: Vec<(String, Channel)>,
}

const RGB: [(&str, Channel); 3] = [
    ("Red", Channel::Red),
    ("Green", Channel::Green),
    ("Blue", Channel::Blue),
];

fn face(side: &str) -> Vec<(String, Channel)> {
    RGB.iter()
        .flat_map(|(color, channel)| {
            (0..8).map(move |index| {
                (
                    device_key(&format!(
                        "Face/Led/{color}/{side}/{}Deg/Actuator",
                        index * 45
                    )),
                    *channel,
                )
            })
        })
        .collect()
}

fn single_rgb(device: &str) -> Vec<(String, Channel)> {
    RGB.iter()
        .map(|(color, channel)| {
            (
                device_key(&format!("{device}/Led/{color}/Actuator")),
                *channel,
            )
        })
        .collect()
}

fn ear(side: &str) -> Vec<(String, Channel)> {
    (0..10)
        .map(|index| {
            (
                device_key(&format!("Ears/Led/{side}/{}Deg/Actuator", index * 36)),
                Channel::Mono,
            )
        })
        .collect()
}

fn brain() -> Vec<(String, Channel)> {
    let leds: &[(&str, &[u8])] = &[("Front", &[0, 1]), ("Middle", &[0]), ("Rear", &[0, 1, 2])];
    leds.iter()
        .flat_map(|(part, indices)| {
            ["Left", "Right"].into_iter().flat_map(move |side| {
                indices.iter().map(move |index| {
                    (
                        device_key(&format!("Head/Led/{part}/{side}/{index}/Actuator")),
                        Channel::Mono,
                    )
                })
            })
        })
        .collect()
}

/// The LED groups of `ALLeds` the HAL drives through the DCM (`led.<group>.color` and
/// `.intensity`).
pub fn led_groups() -> Vec<LedGroup> {
    let group = |name, actuators| LedGroup { name, actuators };
    let concat = |parts: &[Vec<(String, Channel)>]| parts.concat();
    let (left_face, right_face) = (face("Left"), face("Right"));
    let (left_foot, right_foot) = (single_rgb("LFoot"), single_rgb("RFoot"));
    let (left_ear, right_ear) = (ear("Left"), ear("Right"));
    let chest = single_rgb("ChestBoard");
    let brain = brain();
    let all = concat(&[
        left_face.clone(),
        right_face.clone(),
        chest.clone(),
        left_foot.clone(),
        right_foot.clone(),
        left_ear.clone(),
        right_ear.clone(),
        brain.clone(),
    ]);
    vec![
        group("FaceLeds", concat(&[left_face.clone(), right_face.clone()])),
        group("LeftFaceLeds", left_face),
        group("RightFaceLeds", right_face),
        group("ChestLeds", chest),
        group("FeetLeds", concat(&[left_foot.clone(), right_foot.clone()])),
        group("LeftFootLeds", left_foot),
        group("RightFootLeds", right_foot),
        group("EarLeds", concat(&[left_ear.clone(), right_ear.clone()])),
        group("LeftEarLeds", left_ear),
        group("RightEarLeds", right_ear),
        group("BrainLeds", brain),
        group("AllLeds", all),
    ]
}

/// The sensors read every cycle besides the joints' and the touch sensors, by family, with
/// the Arora key each feeds: those of [`arora_hal_naoqi::keys`].
pub fn other_sensors() -> [(Family, &'static [(&'static str, &'static str)]); 3] {
    [
        (Family::Battery, keys::BATTERY_MEMORY_KEYS),
        (Family::Inertial, keys::INERTIAL_MEMORY_KEYS),
        (Family::Sonar, keys::SONAR_MEMORY_KEYS),
    ]
}

/// A family of sensors of [`arora_hal_naoqi::SensorsConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Battery,
    Inertial,
    Sonar,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_have_the_leds_of_nao() {
        let groups = led_groups();
        let size = |name: &str| {
            groups
                .iter()
                .find(|group| group.name == name)
                .map(|group| group.actuators.len())
        };
        assert_eq!(size("FaceLeds"), Some(48));
        assert_eq!(size("ChestLeds"), Some(3));
        assert_eq!(size("EarLeds"), Some(20));
        assert_eq!(size("BrainLeds"), Some(12));
        assert_eq!(size("AllLeds"), Some(48 + 3 + 6 + 20 + 12));
        assert_eq!(
            groups[1].actuators[0].0,
            "Device/SubDeviceList/Face/Led/Red/Left/0Deg/Actuator/Value"
        );
    }

    #[test]
    fn channels_scale_the_color() {
        assert_eq!(Channel::Red.level(0xFF8000, 1.0), 1.0);
        assert!((Channel::Green.level(0xFF8000, 0.5) - 0.251).abs() < 1e-3);
        assert_eq!(Channel::Blue.level(0xFF8000, 1.0), 0.0);
        assert_eq!(Channel::Mono.level(0x0000FF, 0.5), 0.5);
        assert_eq!(Channel::Red.level(0xFFFFFF, 2.0), 1.0);
    }
}
