use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use arora_hal::{Hal, HalDescription, HalResult, UpdatesStream};
use arora_hal_naoqi::services::{self, Node, Sonar};
use arora_hal_naoqi::{arora_f64, keys, NaoqiRobotError, Voice};
use arora_types::data::{Key, State, StateChange};
use arora_types::value::Value;
use async_trait::async_trait;
use log::{info, warn};
use qi::AnyObject;

use crate::config::DcmRobotConfig;
use crate::plugin::{lock, DcmLink, JointCommand, LedCommand};

/// How long dropping the HAL waits for the sonars to be released.
const STOP_TIMEOUT: Duration = Duration::from_secs(1);

/// A NAOqi robot as an [`arora_hal::Hal`], driven from the loop of its DCM, in the process of
/// NAOqi.
///
/// The keys go through the DCM plugin of its [`DcmLink`]: the sensors are read every cycle
/// of the DCM (10 ms on NAO, 12 ms on Pepper) and what changed is sent to the feeds of
/// [`updates`](Hal::updates); the commands written are applied at the next cycle, each joint
/// stepping toward its target at most at
/// [`joint_speed_limit`](DcmRobotConfig::joint_speed_limit). What is not a key goes through
/// NAOqi's services, over a `qi` node: the description, Autonomous Life, the rest postures,
/// the sonars and the [`voice`](DcmHal::voice).
///
/// While the HAL exists, the joints are Arora's: it puts the robot to rest when it starts
/// ([`rest_at_start`](DcmRobotConfig::rest_at_start)), so that `ALMotion` sends them no
/// command, and again when it goes ([`rest_on_exit`](DcmRobotConfig::rest_on_exit)).
pub struct DcmHal {
    link: DcmLink,
    description: HalDescription,
    inner: Arc<Inner>,
    voice: Option<Voice>,
    runtime: tokio::runtime::Handle,
}

struct Inner {
    /// The node connected to NAOqi; its services live as long as it does.
    _node: Node,
    motion: AnyObject,
    sonar: Option<Sonar>,
    rest_on_exit: bool,
    /// Whether base velocity keys were written, which the DCM does not drive: warned once.
    base_warned: AtomicBool,
}

impl DcmHal {
    /// Starts the HAL on the robot of `link`, calling NAOqi's services through `node`: turns
    /// Autonomous Life off and puts the robot to rest as configured, subscribes to the sonars,
    /// describes the robot, then lets the plugin send the HAL's commands.
    ///
    /// Must be called within a multi-threaded Tokio runtime that outlives the HAL.
    pub async fn new(
        link: DcmLink,
        node: Node,
        config: &DcmRobotConfig,
    ) -> Result<Self, NaoqiRobotError> {
        config.validate().map_err(NaoqiRobotError::Config)?;
        let memory = services::required_service(&node, "ALMemory").await?;
        let motion = services::required_service(&node, "ALMotion").await?;
        let voice = match node.service("ALTextToSpeech").await {
            Ok(tts) => Some(Voice::new(tts)),
            Err(error) => {
                warn!("ALTextToSpeech is not available, the robot does not speak: {error}");
                None
            }
        };
        if config.disable_autonomous_life {
            services::disable_autonomous_life(&node).await;
        }
        if config.rest_at_start {
            // The joints are the device's only once ALMotion stays out: a HAL whose robot did
            // not rest does not start, rather than command against ALMotion.
            services::try_rest(&motion).await.map_err(|error| {
                NaoqiRobotError::Connection(format!(
                    "the robot did not rest, ALMotion may still command its joints: {error}"
                ))
            })?;
        }
        let known = HalDescription {
            model_family: config.model_family.clone(),
            hardware_version: config.hardware_version.clone(),
            software_version: config.software_version.clone(),
        };
        let description = services::describe(&known, &node, &memory, &motion).await;
        // Last of the steps that may be cancelled: no subscription is left without a HAL.
        let sonar = if config.sensors.sonar {
            Sonar::subscribe(&node).await
        } else {
            None
        };
        let unknown = || "unknown".to_string();
        info!(
            "DCM HAL started: {} robot, hardware {}, NAOqi {}, {} joints",
            description.model_family.clone().unwrap_or_else(unknown),
            description.hardware_version.clone().unwrap_or_else(unknown),
            description.software_version.clone().unwrap_or_else(unknown),
            link.joints().len()
        );
        link.set_commanding(true);
        Ok(Self {
            link,
            description,
            inner: Arc::new(Inner {
                _node: node,
                motion,
                sonar,
                rest_on_exit: config.rest_on_exit,
                base_warned: AtomicBool::new(false),
            }),
            voice,
            runtime: tokio::runtime::Handle::current(),
        })
    }

    /// The robot's voice (`ALTextToSpeech`), for the `Say` leaves:
    /// [`say::install`](arora_hal_naoqi::say::install) takes it. None when the robot has no
    /// `ALTextToSpeech`.
    pub fn voice(&self) -> Option<Voice> {
        self.voice.clone()
    }

    /// The link to the plugin of the HAL.
    pub fn link(&self) -> &DcmLink {
        &self.link
    }

    /// Hands the commands of a change to the plugin, for its next cycle, and caches the keys
    /// written, so that reads reflect the commands sent.
    fn command(&self, changes: &StateChange) {
        let shared = &self.link.shared;
        let mut joints: Vec<(usize, JointCommand)> = Vec::new();
        let mut leds: Vec<(usize, LedCommand)> = Vec::new();
        for (key, value) in &changes.set {
            let Some(value) = value else {
                continue;
            };
            let entity = key.get_entity();
            let attributes = key.get_attributes();
            match (entity, attributes.as_slice()) {
                (joint, [component @ (keys::TARGET_POSITION | keys::TARGET_STIFFNESS)]) => {
                    let Some(index) = shared.joints.index(joint) else {
                        continue;
                    };
                    if !shared.commandable[index] {
                        warn!("The joint {joint} has no actuator: {key} is ignored");
                        continue;
                    }
                    // A NaN or an infinity is no position, stiffness or LED level.
                    let Some(number) = arora_f64(value).filter(|n| (*n as f32).is_finite()) else {
                        warn!("Ignoring a non-numeric or non-finite {key}");
                        continue;
                    };
                    let mut command = JointCommand::default();
                    if *component == keys::TARGET_POSITION {
                        command.position = Some(number as f32);
                    } else {
                        command.stiffness = Some(number as f32);
                    }
                    joints.push((index, command));
                }
                (keys::LED_ENTITY, [group, component @ ("color" | "intensity")]) => {
                    let Some(index) = shared.led_groups.iter().position(|name| name == group)
                    else {
                        warn!("The robot has no LED group {group}: {key} is ignored");
                        continue;
                    };
                    // A NaN or an infinity is no position, stiffness or LED level.
                    let Some(number) = arora_f64(value).filter(|n| (*n as f32).is_finite()) else {
                        warn!("Ignoring a non-numeric or non-finite {key}");
                        continue;
                    };
                    let mut command = LedCommand::default();
                    if *component == "color" {
                        command.color = Some(number as u32);
                    } else {
                        command.intensity = Some(number as f32);
                    }
                    leds.push((index, command));
                }
                ("velocity", ["x" | "y"]) | ("rotation", ["z"])
                    if !self.inner.base_warned.swap(true, Ordering::Relaxed) =>
                {
                    warn!("The DCM HAL does not drive the base: {key} is ignored");
                }
                _ => {}
            }
        }
        // The state is locked first, through both updates: concurrent writes update the
        // commands and the cache in the same order.
        let mut state = lock(&shared.state);
        {
            let mut pending = lock(&shared.pending);
            for (index, command) in joints {
                let merged = pending.joints.entry(index).or_default();
                merged.position = command.position.or(merged.position);
                merged.stiffness = command.stiffness.or(merged.stiffness);
            }
            for (index, command) in leds {
                // Groups overlap (AllLeds, ChestLeds): the plugin applies them in the order
                // they were written.
                pending.led_writes += 1;
                let order = pending.led_writes;
                let merged = pending.leds.entry(index).or_default();
                merged.color = command.color.or(merged.color);
                merged.intensity = command.intensity.or(merged.intensity);
                merged.order = order;
            }
        }
        for (key, value) in &changes.set {
            state.set(key.clone(), value.clone());
        }
        for key in &changes.unset {
            state.unset(key);
        }
    }
}

#[async_trait]
impl Hal for DcmHal {
    async fn describe(&self) -> HalDescription {
        self.description.clone()
    }

    async fn read(&self, keys: &[Key]) -> HalResult<Vec<Option<Value>>> {
        let state = lock(&self.link.shared.state);
        Ok(keys
            .iter()
            .map(|key| state.get(key).cloned().flatten())
            .collect())
    }

    async fn read_all(&self) -> HalResult<State> {
        Ok(lock(&self.link.shared.state).clone())
    }

    /// Hands the commands to the plugin: they are applied at the next cycle of the DCM.
    async fn write(&self, changes: StateChange) -> HalResult<()> {
        self.command(&changes);
        Ok(())
    }

    /// As [`write`](Hal::write): never blocks.
    fn try_send(&self, changes: &StateChange) {
        self.command(changes);
    }

    /// A feed of the changes the sensors report, cycle by cycle. Each feed first receives the
    /// current state as one `StateChange` (when non-empty), then every subsequent change.
    fn updates(&self) -> UpdatesStream {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        let shared = &self.link.shared;
        let state = lock(&shared.state);
        if !state.is_empty() {
            let _ = tx.unbounded_send(StateChange {
                set: state.storage.clone(),
                unset: Default::default(),
            });
        }
        lock(&shared.subscribers).push(tx);
        drop(state);
        Box::pin(rx)
    }
}

/// Dropping the HAL stops the plugin's commands (the DCM holds the last ones), then puts the
/// robot to rest (unless [`rest_on_exit`](DcmRobotConfig::rest_on_exit) is off) and
/// releases the sonars. Within a multi-threaded runtime the drop waits for that, up to
/// [`REST_TIMEOUT`](services::REST_TIMEOUT); elsewhere it is done from a task of the HAL's
/// runtime.
impl Drop for DcmHal {
    fn drop(&mut self) {
        self.link.set_commanding(false);
        let inner = Arc::clone(&self.inner);
        let shutdown = async move {
            if inner.rest_on_exit {
                services::rest(&inner.motion).await;
            }
            if let Some(sonar) = &inner.sonar {
                sonar.unsubscribe(STOP_TIMEOUT).await;
            }
        };
        let multi_threaded = tokio::runtime::Handle::try_current().is_ok_and(|current| {
            current.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread
        });
        if multi_threaded {
            tokio::task::block_in_place(|| self.runtime.block_on(shutdown));
        } else {
            self.runtime.spawn(shutdown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::fake::FakeDcm;
    use futures::StreamExt;

    async fn start(
        dcm: &FakeDcm,
    ) -> (
        fake_naoqi::FakeNaoqi,
        DcmHal,
        crate::plugin::Core<crate::plugin::fake::FakeSensor, String>,
    ) {
        let naoqi = fake_naoqi::FakeNaoqi::start().await;
        let config = DcmRobotConfig::default();
        let (core, link) = crate::plugin::create(dcm, "Arora", &config).unwrap();
        let node = services::connect(&naoqi.url, None, None).await.unwrap();
        let hal = DcmHal::new(link, node, &config).await.unwrap();
        (naoqi, hal, core)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_hal_takes_the_robot_from_almotion_and_gives_it_back() {
        let dcm = FakeDcm::nao(&["HeadYaw"]);
        let (naoqi, hal, _core) = start(&dcm).await;
        assert_eq!(naoqi.rested.load(Ordering::SeqCst), 1, "rest at start");
        assert_eq!(*naoqi.life_state.lock().unwrap(), "disabled");
        assert_eq!(hal.describe().await.model_family.as_deref(), Some("nao"));
        assert!(hal.voice().is_some());
        let link = hal.link().clone();
        assert!(link.is_commanding());
        drop(hal);
        assert!(!link.is_commanding());
        assert_eq!(naoqi.rested.load(Ordering::SeqCst), 2, "rest on exit");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sensors_flow_in_and_commands_flow_out() {
        let mut dcm = FakeDcm::nao(&["HeadYaw"]);
        let (_naoqi, hal, mut core) = start(&dcm).await;
        let mut updates = hal.updates();
        core.post_process();
        let first = updates.next().await.unwrap();
        assert_eq!(
            first.set.get(&Key::from("head_yaw.position")),
            Some(&Some(Value::F64(0.0)))
        );

        let mut change = StateChange::set("head_yaw.target_position", Value::F64(0.01));
        change.set.insert(
            Key::from("head_yaw.target_stiffness"),
            Some(Value::F64(1.0)),
        );
        change
            .set
            .insert(Key::from("led.ChestLeds.color"), Some(Value::U32(0x00FF00)));
        change
            .set
            .insert(Key::from("led.NoSuchLeds.color"), Some(Value::U32(0)));
        change
            .set
            .insert(Key::from("velocity.x"), Some(Value::F64(0.5)));
        // Ignored: the chest's green stays at full intensity below.
        change.set.insert(
            Key::from("led.ChestLeds.intensity"),
            Some(Value::F64(f64::NAN)),
        );
        hal.write(change).await.unwrap();
        assert_eq!(
            hal.read(&[Key::from("head_yaw.target_position")])
                .await
                .unwrap(),
            [Some(Value::F64(0.01))]
        );
        core.pre_process(&mut dcm);
        let commands = dcm.commands();
        let aliases: Vec<&str> = commands
            .iter()
            .map(|(alias, _, _)| alias.as_str())
            .collect();
        assert!(aliases.contains(&"Arora/HeadYaw/Position"), "{commands:?}");
        assert!(aliases.contains(&"Arora/HeadYaw/Stiffness"), "{commands:?}");
        assert!(
            commands.contains(&("Arora/ChestLeds".to_string(), 0, vec![0.0, 1.0, 0.0])),
            "{commands:?}"
        );
    }
}
