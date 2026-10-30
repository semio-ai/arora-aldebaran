//! An Aldebaran robot (NAO, Pepper) as an Arora device, inside NAOqi.
//!
//! The library is a `qi` module (`arora_naoqi`, `libarora_naoqi.so`) that NAOqi loads from its
//! `autoload.ini`. When NAOqi loads it:
//!
//! - the DCM plugin `AroraDcm`, an `ALModule` of NAOqi, runs in the loop of the DCM: it reads
//!   the sensors and sends the commands of the DCM HAL ([`arora_hal_naoqi_dcm`]), every cycle;
//! - the plugin starts the Arora device on a thread of its own: the device reaches NAOqi,
//!   starts the DCM HAL (which puts the robot to rest, so that `ALMotion` stays out), then
//!   runs the default behavior of `arora-aldebaran` on it, at its own pace, and serves its
//!   bridges (Semio Studio in a library built with it);
//! - the service `Arora` reports what the device does: `qicli call Arora.status`.
//!
//! The device is configured by a JSON file ([`config::ModuleConfig`]): the one
//! `ARORA_NAOQI_CONFIG` names, else `arora_naoqi.json` beside the library or in its parent
//! directory. When NAOqi unloads the plugin, the device stops and the robot rests.

pub mod config;
pub mod device;

use qi::Result;

/// The `Arora` service: what the device NAOqi runs is doing.
#[qi::object(case = "camelCase")]
pub trait AroraService {
    /// `waiting for the DCM`, `starting`, `running`, `stopped` or `failed: <why>`.
    async fn status(&self) -> Result<String>;
    /// The cycles of the DCM the plugin has run.
    async fn cycles(&self) -> Result<u64>;
    /// The commands the DCM refused.
    async fn refused_commands(&self) -> Result<u64>;
    /// The Arora ids of the robot's joints.
    async fn joints(&self) -> Result<Vec<String>>;
    /// Requests the default behavior: the robot says "Arora is ready" with its hands.
    async fn ready(&self) -> Result<()>;
}

/// The device the library runs, as the `Arora` service reports it.
pub struct Report;

#[qi::async_trait]
impl AroraService for Report {
    async fn status(&self) -> Result<String> {
        Ok(device::state().status.to_string())
    }

    async fn cycles(&self) -> Result<u64> {
        Ok(device::state()
            .link
            .as_ref()
            .map_or(0, |link| link.cycles()))
    }

    async fn refused_commands(&self) -> Result<u64> {
        Ok(device::state()
            .link
            .as_ref()
            .map_or(0, |link| link.refused_commands()))
    }

    async fn joints(&self) -> Result<Vec<String>> {
        Ok(device::state()
            .link
            .as_ref()
            .map(|link| link.joints().ids().to_vec())
            .unwrap_or_default())
    }

    async fn ready(&self) -> Result<()> {
        device::request_ready().map_err(|error| format!("{error:#}").into())
    }
}

/// The DCM plugin of the device: the DCM HAL's plugin, and the device it started, which
/// stops with it.
pub struct AroraDcm {
    plugin: arora_hal_naoqi_dcm::naoqi::DcmPlugin,
    _device: device::Device,
}

impl qi::module::naoqi::DcmPlugin for AroraDcm {
    fn pre_process(&mut self, cycle: &mut qi::module::naoqi::Cycle<'_>) {
        self.plugin.pre_process(cycle);
    }

    fn post_process(&mut self, cycle: &mut qi::module::naoqi::Cycle<'_>) {
        self.plugin.post_process(cycle);
    }
}

/// An Arora device: the robot driven from the loop of its DCM.
#[qi::module]
pub mod arora_naoqi {
    use super::{device, AroraDcm, AroraServiceObject, Report};
    use qi::module::naoqi::Naoqi;

    /// What the device is doing; as `Arora.status`.
    pub fn status() -> String {
        device::state().status.to_string()
    }

    #[qi::factory(name = "Arora", autoload)]
    pub fn arora() -> AroraServiceObject<Report> {
        AroraServiceObject::new(Report)
    }

    #[qi::dcm(name = "AroraDcm")]
    pub fn arora_dcm(naoqi: &Naoqi<'_>) -> qi::Result<AroraDcm> {
        let path = crate::config::find(device::library_dir().as_deref());
        let config = crate::config::load(path.as_deref()).map_err(|e| format!("{e:#}"))?;
        crate::init_logging(config.log.as_deref());
        match &path {
            Some(path) => log::info!("Arora configuration: {}", path.display()),
            None => log::info!("No Arora configuration: the defaults"),
        }
        let (plugin, link) = arora_hal_naoqi_dcm::naoqi::plugin(naoqi, &config.robot)?;
        let device = device::Device::start(link, config)
            .map_err(|e| format!("cannot start the Arora device: {e}"))?;
        Ok(AroraDcm {
            plugin,
            _device: device,
        })
    }
}

/// Logs to the standard error of NAOqi, at `filter` (`info` when none), unless `RUST_LOG`
/// says otherwise or a logger is already installed.
fn init_logging(filter: Option<&str>) {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(filter.unwrap_or("info")),
    )
    .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_has_its_service_and_its_plugin() {
        let module = &arora_naoqi::__QI_MODULE;
        assert_eq!(module.name, "arora_naoqi");
        let factories: Vec<_> = module
            .factories
            .iter()
            .map(|factory| (factory.name, factory.autoload))
            .collect();
        assert_eq!(factories, [("Arora", true)]);
        let plugins: Vec<_> = module
            .dcm_plugins
            .iter()
            .map(|plugin| plugin.name)
            .collect();
        assert_eq!(plugins, ["AroraDcm"]);
    }

    #[tokio::test]
    async fn the_service_reports_a_device_waiting_for_the_dcm() {
        let report = Report;
        assert_eq!(report.status().await.unwrap(), "waiting for the DCM");
        assert_eq!(report.cycles().await.unwrap(), 0);
        assert!(report.ready().await.is_err());
    }
}
