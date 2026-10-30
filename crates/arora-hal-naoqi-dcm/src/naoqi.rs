//! The plugin in NAOqi: the DCM plugins of `qi::module::naoqi`.

use qi::module::naoqi::{self as dcm, Alias, Cycle, MemoryF32, Naoqi, Update};

use crate::config::DcmRobotConfig;
use crate::plugin::{self, Commands, Core, DcmLink, Probe, Sensor};

impl Sensor for MemoryF32 {
    fn get(&self) -> f32 {
        MemoryF32::get(self)
    }
}

impl Probe for Naoqi<'_> {
    type Sensor = MemoryF32;
    type Alias = Alias;

    fn sensor(&self, key: &str) -> Option<MemoryF32> {
        self.memory_f32(key).ok()
    }

    fn alias(&self, name: &str, keys: &[&str]) -> Result<Alias, String> {
        self.create_alias(name, keys)
            .map_err(|error| format!("cannot create the DCM alias {name}: {error}"))
    }
}

/// The commands of a cycle, timed from the DCM's time at the cycle's first command.
struct CycleCommands<'c, 'a> {
    cycle: &'c mut Cycle<'a>,
    now: Option<i32>,
}

impl Commands<Alias> for CycleCommands<'_, '_> {
    fn send(&mut self, alias: &Alias, delay_ms: u32, values: &[f32]) -> Result<(), String> {
        let now = match self.now {
            Some(now) => now,
            None => {
                let now = self.cycle.time(0).map_err(|error| error.to_string())?;
                *self.now.insert(now)
            }
        };
        let time = now.wrapping_add(i32::try_from(delay_ms).unwrap_or(i32::MAX));
        self.cycle
            .set_alias(alias, Update::ClearAll, time, values)
            .map_err(|error| error.to_string())
    }
}

/// The DCM plugin of the HAL, for a `#[qi::dcm]` function of a `#[qi::module]`.
pub struct DcmPlugin(Core<MemoryF32, Alias>);

impl dcm::DcmPlugin for DcmPlugin {
    fn pre_process(&mut self, cycle: &mut Cycle<'_>) {
        self.0.pre_process(&mut CycleCommands { cycle, now: None });
    }

    fn post_process(&mut self, _cycle: &mut Cycle<'_>) {
        self.0.post_process();
    }
}

/// Creates the DCM plugin of the HAL in the NAOqi it is loaded in, with the link the
/// [`DcmHal`](crate::DcmHal) takes: probes the robot and creates the aliases of its
/// actuators, named after the plugin's `ALModule`.
pub fn plugin(naoqi: &Naoqi<'_>, config: &DcmRobotConfig) -> qi::Result<(DcmPlugin, DcmLink)> {
    let (core, link) = plugin::create(naoqi, naoqi.module_name(), config)?;
    Ok((DcmPlugin(core), link))
}
