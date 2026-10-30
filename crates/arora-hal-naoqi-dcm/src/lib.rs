//! NAOqi robots (NAO, Pepper) as Arora HALs, from inside NAOqi, paced by the loop of the DCM.
//!
//! The HAL lives in a library NAOqi loads (a `#[qi::module]` of `libqi-vibe`, listed in
//! NAOqi's `autoload.ini`), in two parts:
//!
//! - its DCM plugin ([`naoqi::plugin`], for a `#[qi::dcm]` function of the module), which runs
//!   in the loop of the DCM, every 10 ms on NAO (12 ms on Pepper): right after the DCM
//!   updated the sensors, it reads them in place in `ALMemory` and sends what changed to the
//!   HAL's feeds; right before the DCM computes its orders, it sends the HAL's latest
//!   commands to the actuators, through DCM aliases;
//! - the [`DcmHal`], the [`arora_hal::Hal`] an Arora device runs with, on threads of its own,
//!   at its own pace: it reads the feeds and hands its commands to the plugin, through the
//!   [`DcmLink`] they share. The loop of the DCM never waits on it.
//!
//! The keys are those of the Arora robot standard, as for the networked HAL of
//! [`arora_hal_naoqi`]; what is not a key (speech, Autonomous Life, the rest postures, the
//! description) goes through NAOqi's services, with `qi`.
//!
//! The robot is described by a [`DcmRobotConfig`].

mod config;
pub use config::DcmRobotConfig;

pub mod devices;

mod hal;
pub use hal::DcmHal;

pub mod naoqi;

mod plugin;
pub use plugin::DcmLink;
