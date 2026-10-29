//! The part of the HAL that runs in the loop of the DCM: it reads the sensors right after the
//! DCM updated them and publishes what changed to the HAL's feeds, and sends the latest
//! commands of the HAL to the actuators right before the DCM computes its orders.
//!
//! It is written against [`Probe`], [`Sensor`] and [`Commands`], which
//! [`crate::naoqi`] implements with the DCM plugins of `qi::module::naoqi` and the tests with
//! a fake DCM. Its callbacks never wait on the rest of the HAL: they take its locks with
//! `try_lock`, and leave for the next cycle what they could not take.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use arora_hal_naoqi::{keys, JointTable};
use arora_types::data::{Key, State, StateChange};
use arora_types::value::Value;
use futures::channel::mpsc::UnboundedSender;
use log::{info, warn};

use crate::config::DcmRobotConfig;
use crate::devices::{self, Channel, Family};

/// A float of `ALMemory`, read in place.
pub(crate) trait Sensor: Send + 'static {
    fn get(&self) -> f32;
}

/// What the plugin finds of the robot when it is created.
pub(crate) trait Probe {
    type Sensor: Sensor;
    type Alias: Send + 'static;
    /// The `ALMemory` value of a key, if the robot has it.
    fn sensor(&self, key: &str) -> Option<Self::Sensor>;
    /// A DCM alias of actuators.
    fn alias(&self, name: &str, keys: &[&str]) -> Result<Self::Alias, String>;
}

/// The commands of a cycle of the DCM.
pub(crate) trait Commands<A> {
    /// Commands the actuators of an alias to `values`, reached `delay_ms` milliseconds from
    /// now (the DCM interpolates), replacing the commands they had.
    fn send(&mut self, alias: &A, delay_ms: u32, values: &[f32]) -> Result<(), String>;
}

/// The DCM's nominal period on NAO: the first cycle's, before one was measured.
const NOMINAL_PERIOD: Duration = Duration::from_millis(10);
/// The longest step a joint takes in one cycle, however late the cycle: a stalled DCM does
/// not make the next command jump.
const MAX_PERIOD: Duration = Duration::from_millis(50);

/// A joint command the HAL received, applied at the next cycle. Each field is the latest
/// value written.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct JointCommand {
    pub position: Option<f32>,
    pub stiffness: Option<f32>,
}

/// An LED command the HAL received, applied at the next cycle.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct LedCommand {
    pub color: Option<u32>,
    pub intensity: Option<f32>,
    /// When it was last written, among the LED commands: overlapping groups are applied in
    /// that order.
    pub order: u64,
}

#[derive(Default)]
pub(crate) struct Pending {
    pub joints: HashMap<usize, JointCommand>,
    pub leds: HashMap<usize, LedCommand>,
    /// The LED commands written, which orders them.
    pub led_writes: u64,
}

/// What the loop of the DCM and the rest of the HAL share.
pub(crate) struct Shared {
    /// The joints the robot has, in the order of the plugin's.
    pub joints: JointTable,
    /// Whether each joint has a position and a stiffness actuator.
    pub commandable: Vec<bool>,
    /// The names of the LED groups the robot has, in the order of the plugin's.
    pub led_groups: Vec<&'static str>,
    /// The current state: the sensors read and the commands written.
    pub state: Mutex<State>,
    /// The feeds of `updates()`.
    pub subscribers: Mutex<Vec<UnboundedSender<StateChange>>>,
    /// The commands written since the last cycle.
    pub pending: Mutex<Pending>,
    /// Whether the plugin sends commands: from the start of the HAL to its end.
    pub commanding: AtomicBool,
    /// The HALs started on the link: the plugin forgets the commands of the previous one.
    pub generation: AtomicU64,
    /// The cycles of the DCM run.
    pub cycles: AtomicU64,
    /// The commands the DCM refused.
    pub refused: AtomicU64,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn try_lock<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match mutex.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

impl Shared {
    /// Applies a change to the state and sends what actually changed to the feeds. A feed
    /// registers under the same lock of the state, so it gets the change either in its
    /// snapshot or as a delta.
    pub fn publish(&self, state: &mut State, change: StateChange) {
        let mut delta = StateChange::new();
        for (key, value) in change.set {
            if state.get(&key) != Some(&value) {
                state.set(key.clone(), value.clone());
                delta.set.insert(key, value);
            }
        }
        for key in change.unset {
            if state.get(&key).is_some() {
                state.unset(&key);
                delta.unset.insert(key);
            }
        }
        if !delta.is_empty() {
            lock(&self.subscribers).retain(|tx| tx.unbounded_send(delta.clone()).is_ok());
        }
    }
}

/// The link between the plugin, in the loop of the DCM, and the HAL that gives its commands
/// and reads its sensors: [`DcmHal::new`](crate::DcmHal::new) takes it.
#[derive(Clone)]
pub struct DcmLink {
    pub(crate) shared: Arc<Shared>,
}

impl DcmLink {
    /// The joints of the robot, with their Arora ids.
    pub fn joints(&self) -> &JointTable {
        &self.shared.joints
    }

    /// The LED groups of the robot the HAL drives.
    pub fn led_groups(&self) -> &[&'static str] {
        &self.shared.led_groups
    }

    /// How many cycles of the DCM the plugin has run.
    pub fn cycles(&self) -> u64 {
        self.shared.cycles.load(Ordering::Relaxed)
    }

    /// How many commands the DCM refused.
    pub fn refused_commands(&self) -> u64 {
        self.shared.refused.load(Ordering::Relaxed)
    }

    /// Whether the plugin sends the HAL's commands.
    pub fn is_commanding(&self) -> bool {
        self.shared.commanding.load(Ordering::Acquire)
    }

    pub(crate) fn set_commanding(&self, commanding: bool) {
        if commanding {
            // A new HAL: the plugin forgets the targets and commands of the previous one.
            self.shared.generation.fetch_add(1, Ordering::AcqRel);
        } else {
            // Commands written meanwhile are not applied later by another HAL.
            *lock(&self.shared.pending) = Pending::default();
        }
        self.shared.commanding.store(commanding, Ordering::Release);
    }
}

/// A sensor read every cycle, with the Arora key it feeds.
struct SensorSlot<S> {
    sensor: S,
    key: Key,
    /// What it means beyond its value.
    role: Role,
    /// The value last published, `NaN` before the first.
    last: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Plain,
    /// The battery current, which also drives `battery.charging`.
    BatteryCurrent,
}

struct TouchSlot<S> {
    sensors: Vec<S>,
    key: Key,
    last: Option<bool>,
}

struct JointSlot<S, A> {
    /// The measured position.
    position: S,
    /// `<id>.velocity`, when the joints are read.
    velocity_key: Option<Key>,
    /// The position and the time of the previous cycle, for the velocity.
    previous: Option<(Instant, f32)>,
    last_velocity: f32,
    actuators: Option<JointActuators<A>>,
}

struct JointActuators<A> {
    position: A,
    stiffness: A,
    /// The target position, once one was written.
    target: Option<f32>,
    /// The position last commanded, once the joint was first commanded.
    commanded: Option<f32>,
    /// The stiffness written, and whether it was sent.
    stiffness_target: Option<f32>,
    stiffness_sent: Option<f32>,
}

struct LedSlot<A> {
    alias: A,
    channels: Vec<Channel>,
    color: u32,
    intensity: f32,
    dirty: bool,
    /// When its pending command was written.
    order: u64,
}

impl<A> LedSlot<A> {
    fn reset(&mut self) {
        self.color = 0xFFFFFF;
        self.intensity = 1.0;
        self.dirty = false;
    }
}

impl<A> JointActuators<A> {
    fn reset(&mut self) {
        self.target = None;
        self.commanded = None;
        self.stiffness_target = None;
        self.stiffness_sent = None;
    }
}

/// The plugin: what the loop of the DCM runs.
pub(crate) struct Core<S, A> {
    shared: Arc<Shared>,
    sensors: Vec<SensorSlot<S>>,
    touches: Vec<TouchSlot<S>>,
    joints: Vec<JointSlot<S, A>>,
    leds: Vec<LedSlot<A>>,
    /// The speed limit, in units per second.
    speed_limit: f32,
    stiffness_ramp_ms: u32,
    /// A change the plugin could not publish, the state being locked: the next cycle does.
    deferred: StateChange,
    last_pre_process: Option<Instant>,
    /// The generation of the HAL whose commands the plugin applies.
    generation: u64,
    /// Whether a refused command was logged: the DCM's thread logs once.
    refusal_logged: bool,
}

/// A plugin on a [`Probe`]'s robot, and its link.
pub(crate) type Created<P> = (Core<<P as Probe>::Sensor, <P as Probe>::Alias>, DcmLink);

/// Creates the plugin and its link to the HAL: probes the joints, sensors and LED groups the
/// robot has, and creates the DCM aliases of its actuators, under `name`.
pub(crate) fn create<P: Probe>(
    probe: &P,
    name: &str,
    config: &DcmRobotConfig,
) -> Result<Created<P>, String> {
    config.validate()?;
    let mut positions: HashMap<&str, P::Sensor> = HashMap::new();
    for joint in devices::JOINTS {
        if let Some(sensor) =
            probe.sensor(&devices::device_key(&format!("{joint}/Position/Sensor")))
        {
            positions.insert(joint, sensor);
        }
    }
    let names = devices::JOINTS
        .iter()
        .filter(|joint| positions.contains_key(**joint))
        .map(|joint| joint.to_string())
        .collect();
    let table = JointTable::new(names, &config.joint_ids)?;

    let mut joints = Vec::with_capacity(table.len());
    let mut commandable = Vec::with_capacity(table.len());
    for (joint, id) in table.names().iter().zip(table.ids()) {
        let position = positions
            .remove(joint.as_str())
            .expect("the table holds the joints probed");
        let has = |path: &str| {
            probe
                .sensor(&devices::device_key(&format!("{joint}/{path}")))
                .is_some()
        };
        let actuators = if has("Position/Actuator") && has("Hardness/Actuator") {
            let alias = |kind: &str, path: &str| {
                let key = devices::device_key(&format!("{joint}/{path}"));
                probe.alias(&format!("{name}/{joint}/{kind}"), &[key.as_str()])
            };
            Some(JointActuators {
                position: alias("Position", "Position/Actuator")?,
                stiffness: alias("Stiffness", "Hardness/Actuator")?,
                target: None,
                commanded: None,
                stiffness_target: None,
                stiffness_sent: None,
            })
        } else {
            None
        };
        commandable.push(actuators.is_some());
        joints.push(JointSlot {
            position,
            velocity_key: config
                .sensors
                .joints
                .then(|| Key::from(format!("{id}.{}", keys::VELOCITY))),
            previous: None,
            last_velocity: f32::NAN,
            actuators,
        });
    }

    let mut sensors = Vec::new();
    let mut add_sensor = |memory_key: &str, arora_key: &str, role| {
        if let Some(sensor) = probe.sensor(memory_key) {
            sensors.push(SensorSlot {
                sensor,
                key: Key::from(arora_key),
                role,
                last: f32::NAN,
            });
        }
    };
    if config.sensors.joints {
        for (memory_key, arora_key) in table.memory_keys() {
            add_sensor(&memory_key, &arora_key, Role::Plain);
        }
    }
    for (family, table) in devices::other_sensors() {
        let enabled = match family {
            Family::Battery => config.sensors.battery,
            Family::Inertial => config.sensors.inertial,
            Family::Sonar => config.sensors.sonar,
        };
        if !enabled {
            continue;
        }
        for (memory_key, arora_key) in table {
            let role = if *arora_key == keys::BATTERY_CURRENT {
                Role::BatteryCurrent
            } else {
                Role::Plain
            };
            add_sensor(memory_key, arora_key, role);
        }
    }

    let mut touches = Vec::new();
    if config.sensors.touch {
        for (key, paths) in devices::TOUCH_SENSORS {
            let found: Vec<_> = paths
                .iter()
                .filter_map(|path| probe.sensor(&devices::device_key(path)))
                .collect();
            if !found.is_empty() {
                touches.push(TouchSlot {
                    sensors: found,
                    key: Key::from(*key),
                    last: None,
                });
            }
        }
    }

    let mut leds = Vec::new();
    let mut led_groups = Vec::new();
    for group in devices::led_groups() {
        let present: Vec<&(String, Channel)> = group
            .actuators
            .iter()
            .filter(|(key, _)| probe.sensor(key).is_some())
            .collect();
        if present.is_empty() {
            continue;
        }
        let keys: Vec<&str> = present.iter().map(|(key, _)| key.as_str()).collect();
        leds.push(LedSlot {
            alias: probe.alias(&format!("{name}/{}", group.name), &keys)?,
            channels: present.iter().map(|(_, channel)| *channel).collect(),
            color: 0xFFFFFF,
            intensity: 1.0,
            dirty: false,
            order: 0,
        });
        led_groups.push(group.name);
    }

    info!(
        "DCM plugin {name}: {} joints ({} commandable), {} sensors, {} touch keys, {} LED groups",
        table.len(),
        commandable.iter().filter(|c| **c).count(),
        sensors.len(),
        touches.len(),
        led_groups.len()
    );
    let shared = Arc::new(Shared {
        joints: table,
        commandable,
        led_groups,
        state: Mutex::new(State::new()),
        subscribers: Mutex::new(Vec::new()),
        pending: Mutex::new(Pending::default()),
        commanding: AtomicBool::new(false),
        generation: AtomicU64::new(0),
        cycles: AtomicU64::new(0),
        refused: AtomicU64::new(0),
    });
    let core = Core {
        shared: Arc::clone(&shared),
        sensors,
        touches,
        joints,
        leds,
        speed_limit: config.joint_speed_limit as f32,
        stiffness_ramp_ms: config.stiffness_ramp_ms,
        deferred: StateChange::new(),
        last_pre_process: None,
        generation: 0,
        refusal_logged: false,
    };
    Ok((core, DcmLink { shared }))
}

impl<S: Sensor, A: Send + 'static> Core<S, A> {
    /// Right after the DCM updated the sensors: publishes what changed.
    pub fn post_process(&mut self) {
        let now = Instant::now();
        let mut change = std::mem::take(&mut self.deferred);
        for slot in &mut self.sensors {
            let value = slot.sensor.get();
            if value.to_bits() == slot.last.to_bits() {
                continue;
            }
            slot.last = value;
            change
                .set
                .insert(slot.key.clone(), Some(Value::F64(f64::from(value))));
            if slot.role == Role::BatteryCurrent {
                change.set.insert(
                    Key::from(keys::BATTERY_CHARGING),
                    Some(Value::Boolean(value > 0.0)),
                );
            }
        }
        for joint in &mut self.joints {
            let Some(key) = &joint.velocity_key else {
                continue;
            };
            let position = joint.position.get();
            if let Some((then, previous)) = joint.previous {
                let elapsed = now.duration_since(then).as_secs_f32();
                if elapsed > 0.0 {
                    let velocity = (position - previous) / elapsed;
                    if velocity.to_bits() != joint.last_velocity.to_bits() {
                        joint.last_velocity = velocity;
                        change
                            .set
                            .insert(key.clone(), Some(Value::F64(f64::from(velocity))));
                    }
                }
            }
            joint.previous = Some((now, position));
        }
        for touch in &mut self.touches {
            let touched = touch.sensors.iter().any(|sensor| sensor.get() > 0.5);
            if touch.last != Some(touched) {
                touch.last = Some(touched);
                change
                    .set
                    .insert(touch.key.clone(), Some(Value::Boolean(touched)));
            }
        }
        if !change.is_empty() {
            match try_lock(&self.shared.state) {
                Some(mut state) => self.shared.publish(&mut state, change),
                None => self.deferred = change,
            }
        }
        self.shared.cycles.fetch_add(1, Ordering::Relaxed);
    }

    /// Right before the DCM computes its orders: sends the commands of the HAL.
    pub fn pre_process(&mut self, commands: &mut impl Commands<A>) {
        let now = Instant::now();
        let period = self
            .last_pre_process
            .map_or(NOMINAL_PERIOD, |then| now.duration_since(then))
            .min(MAX_PERIOD);
        self.last_pre_process = Some(now);
        if !self.shared.commanding.load(Ordering::Acquire) {
            return;
        }
        let generation = self.shared.generation.load(Ordering::Acquire);
        if generation != self.generation {
            self.generation = generation;
            for actuators in self.joints.iter_mut().filter_map(|j| j.actuators.as_mut()) {
                actuators.reset();
            }
            for led in &mut self.leds {
                led.reset();
            }
        }
        if let Some(mut pending) = try_lock(&self.shared.pending) {
            let pending = std::mem::take(&mut *pending);
            for (index, command) in pending.joints {
                if let Some(actuators) = self
                    .joints
                    .get_mut(index)
                    .and_then(|joint| joint.actuators.as_mut())
                {
                    if command.position.is_some() {
                        actuators.target = command.position;
                    }
                    if command.stiffness.is_some() {
                        actuators.stiffness_target = command.stiffness;
                    }
                }
            }
            for (index, command) in pending.leds {
                if let Some(led) = self.leds.get_mut(index) {
                    if let Some(color) = command.color {
                        led.color = color;
                    }
                    if let Some(intensity) = command.intensity {
                        led.intensity = intensity;
                    }
                    led.dirty = true;
                    led.order = command.order;
                }
            }
        }

        let step = self.speed_limit * period.as_secs_f32();
        let delay = (period.as_millis() as u32).max(1);
        let mut refusals = Vec::new();
        for joint in &mut self.joints {
            let Some(actuators) = joint.actuators.as_mut() else {
                continue;
            };
            if actuators.target.is_none() && actuators.stiffness_target.is_none() {
                continue;
            }
            // A joint first commanded, or stiffened again after it was relaxed, starts from
            // where it is: its actuator may hold an old command, which stiffening it would jump
            // to (a relaxed joint sags, or is moved by hand).
            // A stiffness never sent is unknown: the joint may be relaxed, as ALMotion.rest
            // leaves it.
            let stiffening = actuators
                .stiffness_target
                .is_some_and(|target| target > 0.0)
                && actuators.stiffness_sent.is_none_or(|sent| sent <= 0.0);
            if stiffening {
                actuators.commanded = None;
            }
            let (commanded, delay) = match actuators.commanded {
                None => (joint.position.get(), 0),
                Some(commanded) => (commanded, delay),
            };
            let next = match actuators.target {
                Some(target) => commanded + (target - commanded).clamp(-step, step),
                None => commanded,
            };
            if actuators.commanded != Some(next) {
                match commands.send(&actuators.position, delay, &[next]) {
                    Ok(()) => actuators.commanded = Some(next),
                    Err(error) => refusals.push(error),
                }
            }
            if let Some(stiffness) = actuators.stiffness_target {
                if actuators.stiffness_sent != Some(stiffness) {
                    let stiffness = stiffness.clamp(0.0, 1.0);
                    match commands.send(&actuators.stiffness, self.stiffness_ramp_ms, &[stiffness])
                    {
                        Ok(()) => actuators.stiffness_sent = actuators.stiffness_target,
                        Err(error) => refusals.push(error),
                    }
                }
            }
        }
        let mut dirty: Vec<&mut LedSlot<A>> =
            self.leds.iter_mut().filter(|led| led.dirty).collect();
        dirty.sort_by_key(|led| led.order);
        for led in dirty {
            let values: Vec<f32> = led
                .channels
                .iter()
                .map(|channel| channel.level(led.color, led.intensity))
                .collect();
            match commands.send(&led.alias, 0, &values) {
                Ok(()) => led.dirty = false,
                Err(error) => refusals.push(error),
            }
        }
        if !refusals.is_empty() {
            self.shared
                .refused
                .fetch_add(refusals.len() as u64, Ordering::Relaxed);
            if !self.refusal_logged {
                self.refusal_logged = true;
                warn!(
                    "The DCM refused a command (logged once): {}",
                    refusals.join("; ")
                );
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! A fake DCM: sensors the test sets, aliases and the commands sent to them.

    use super::*;
    use std::sync::atomic::AtomicU32;

    type Shared<T> = Arc<Mutex<Vec<T>>>;

    #[derive(Clone, Default)]
    pub struct FakeDcm {
        pub memory: Arc<Mutex<HashMap<String, Arc<AtomicU32>>>>,
        pub aliases: Shared<(String, Vec<String>)>,
        /// Every command: alias, delay and values.
        pub commands: Shared<(String, u32, Vec<f32>)>,
    }

    pub struct FakeSensor(Arc<AtomicU32>);

    impl Sensor for FakeSensor {
        fn get(&self) -> f32 {
            f32::from_bits(self.0.load(Ordering::Relaxed))
        }
    }

    impl FakeDcm {
        /// A NAO with the given joints: their sensors and actuators, the battery, the front
        /// head touch sensor and the chest LEDs.
        pub fn nao(joints: &[&str]) -> Self {
            let dcm = Self::default();
            for joint in joints {
                for path in [
                    "Position/Sensor",
                    "Position/Actuator",
                    "Hardness/Actuator",
                    "Temperature/Sensor",
                    "ElectricCurrent/Sensor",
                ] {
                    dcm.set(&devices::device_key(&format!("{joint}/{path}")), 0.0);
                }
            }
            for (key, _) in keys::BATTERY_MEMORY_KEYS {
                dcm.set(key, 0.5);
            }
            dcm.set(&devices::device_key("Head/Touch/Front/Sensor"), 0.0);
            for color in ["Red", "Green", "Blue"] {
                dcm.set(
                    &devices::device_key(&format!("ChestBoard/Led/{color}/Actuator")),
                    0.0,
                );
            }
            dcm
        }

        pub fn set(&self, key: &str, value: f32) {
            lock(&self.memory)
                .entry(key.to_string())
                .or_default()
                .store(value.to_bits(), Ordering::Relaxed);
        }

        pub fn commands(&self) -> Vec<(String, u32, Vec<f32>)> {
            lock(&self.commands).clone()
        }
    }

    impl Probe for FakeDcm {
        type Sensor = FakeSensor;
        type Alias = String;

        fn sensor(&self, key: &str) -> Option<FakeSensor> {
            lock(&self.memory).get(key).cloned().map(FakeSensor)
        }

        fn alias(&self, name: &str, keys: &[&str]) -> Result<String, String> {
            lock(&self.aliases).push((
                name.to_string(),
                keys.iter().map(|key| key.to_string()).collect(),
            ));
            Ok(name.to_string())
        }
    }

    impl Commands<String> for FakeDcm {
        fn send(&mut self, alias: &String, delay_ms: u32, values: &[f32]) -> Result<(), String> {
            lock(&self.commands).push((alias.clone(), delay_ms, values.to_vec()));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeDcm;
    use super::*;

    fn changes(link: &DcmLink) -> futures::channel::mpsc::UnboundedReceiver<StateChange> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        lock(&link.shared.subscribers).push(tx);
        rx
    }

    fn command(link: &DcmLink, joint: usize, command: JointCommand) {
        lock(&link.shared.pending).joints.insert(joint, command);
    }

    #[test]
    fn the_robot_is_probed() {
        let dcm = FakeDcm::nao(&["HeadYaw", "LHand"]);
        let (core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        assert_eq!(link.joints().ids(), ["head_yaw", "l_hand"]);
        assert_eq!(link.led_groups(), ["ChestLeds", "AllLeds"]);
        let aliases: Vec<String> = lock(&dcm.aliases).iter().map(|(n, _)| n.clone()).collect();
        assert!(aliases.contains(&"Arora/HeadYaw/Position".to_string()));
        assert!(aliases.contains(&"Arora/ChestLeds".to_string()));
        // 4 sensors per joint, 2 battery keys.
        assert_eq!(core.sensors.len(), 10);
        assert_eq!(core.touches.len(), 1);
    }

    #[test]
    fn a_joint_without_actuators_is_read_only() {
        let dcm = FakeDcm::nao(&["HeadYaw"]);
        dcm.set(&devices::device_key("RHipYawPitch/Position/Sensor"), 0.0);
        let (_, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        assert_eq!(link.joints().ids(), ["head_yaw", "r_hip_yaw_pitch"]);
        assert_eq!(link.shared.commandable, [true, false]);
    }

    #[test]
    fn sensors_publish_what_changed() {
        let dcm = FakeDcm::nao(&["HeadYaw"]);
        let (mut core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        let mut feed = changes(&link);
        core.post_process();
        let first = feed.try_recv().unwrap();
        assert_eq!(
            first.set.get(&Key::from("battery.charge")),
            Some(&Some(Value::F64(0.5)))
        );
        assert_eq!(
            first.set.get(&Key::from("battery.charging")),
            Some(&Some(Value::Boolean(true)))
        );
        assert_eq!(
            first.set.get(&Key::from("touch.head.front")),
            Some(&Some(Value::Boolean(false)))
        );
        // Nothing changed: nothing is sent, but the velocity of the second sample.
        core.post_process();
        let second = feed.try_recv().unwrap();
        assert_eq!(second.set.len(), 1, "{second:?}");
        assert!(second.set.contains_key(&Key::from("head_yaw.velocity")));
        core.post_process();
        assert!(feed.try_recv().is_err());
        dcm.set(&devices::device_key("Head/Touch/Front/Sensor"), 1.0);
        dcm.set(&devices::device_key("HeadYaw/Position/Sensor"), 0.25);
        core.post_process();
        let touched = feed.try_recv().unwrap();
        assert_eq!(
            touched.set.get(&Key::from("touch.head.front")),
            Some(&Some(Value::Boolean(true)))
        );
        assert_eq!(
            touched.set.get(&Key::from("head_yaw.position")),
            Some(&Some(Value::F64(0.25)))
        );
        assert_eq!(link.cycles(), 4);
    }

    #[test]
    fn a_locked_state_defers_the_change() {
        let dcm = FakeDcm::nao(&["HeadYaw"]);
        let (mut core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        let mut feed = changes(&link);
        {
            let _reader = lock(&link.shared.state);
            core.post_process();
        }
        assert!(feed.try_recv().is_err(), "the DCM did not wait");
        core.post_process();
        let change = feed.try_recv().unwrap();
        assert!(change.set.contains_key(&Key::from("battery.charge")));
    }

    #[test]
    fn nothing_is_commanded_until_the_hal_starts() {
        let mut dcm = FakeDcm::nao(&["HeadYaw"]);
        let (mut core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        command(
            &link,
            0,
            JointCommand {
                position: Some(1.0),
                stiffness: Some(1.0),
            },
        );
        core.pre_process(&mut dcm);
        assert!(dcm.commands().is_empty());
    }

    #[test]
    fn a_joint_moves_toward_its_target_at_the_speed_limit() {
        let mut dcm = FakeDcm::nao(&["HeadYaw"]);
        dcm.set(&devices::device_key("HeadYaw/Position/Sensor"), 0.2);
        let config = DcmRobotConfig {
            joint_speed_limit: 5.0,
            ..Default::default()
        };
        let (mut core, link) = create(&dcm, "Arora", &config).unwrap();
        link.set_commanding(true);
        command(
            &link,
            0,
            JointCommand {
                position: Some(0.5),
                stiffness: Some(0.8),
            },
        );
        core.pre_process(&mut dcm);
        let first = dcm.commands();
        // From where the joint is, at once; the stiffness ramps.
        assert_eq!(first[0].0, "Arora/HeadYaw/Position");
        assert_eq!(first[0].1, 0);
        assert!((first[0].2[0] - 0.25).abs() < 1e-6, "{first:?}");
        assert_eq!(
            first[1],
            ("Arora/HeadYaw/Stiffness".to_string(), 100, vec![0.8])
        );
        for _ in 0..200 {
            std::thread::sleep(Duration::from_millis(1));
            core.pre_process(&mut dcm);
        }
        let commands = dcm.commands();
        let positions: Vec<f32> = commands
            .iter()
            .filter(|(alias, _, _)| alias == "Arora/HeadYaw/Position")
            .map(|(_, _, values)| values[0])
            .collect();
        // Every step is within the limit (5 rad/s over at most 50 ms), the joint takes
        // several, and the target is reached, then held.
        for pair in positions.windows(2) {
            assert!((pair[1] - pair[0]).abs() <= 0.25 + 1e-6, "{pair:?}");
        }
        assert!(positions.len() > 3, "{positions:?}");
        assert_eq!(*positions.last().unwrap(), 0.5);
        let stiffness = commands
            .iter()
            .filter(|(alias, _, _)| alias == "Arora/HeadYaw/Stiffness")
            .count();
        assert_eq!(stiffness, 1, "the stiffness is sent once");
    }

    #[test]
    fn stiffening_a_joint_holds_it_where_it_is() {
        let mut dcm = FakeDcm::nao(&["HeadYaw"]);
        dcm.set(&devices::device_key("HeadYaw/Position/Sensor"), -0.3);
        let (mut core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        link.set_commanding(true);
        command(
            &link,
            0,
            JointCommand {
                position: None,
                stiffness: Some(1.0),
            },
        );
        core.pre_process(&mut dcm);
        core.pre_process(&mut dcm);
        let commands = dcm.commands();
        assert_eq!(commands.len(), 2, "{commands:?}");
        assert_eq!(
            commands[0],
            ("Arora/HeadYaw/Position".to_string(), 0, vec![-0.3])
        );
    }

    #[test]
    fn a_joint_stiffened_again_starts_from_where_it_is() {
        let mut dcm = FakeDcm::nao(&["HeadYaw"]);
        let position = devices::device_key("HeadYaw/Position/Sensor");
        let (mut core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        link.set_commanding(true);
        let target = |position, stiffness| JointCommand {
            position: Some(position),
            stiffness: Some(stiffness),
        };
        command(&link, 0, target(0.0, 1.0));
        core.pre_process(&mut dcm);
        // Relaxed, the joint sags to 0.4; stiffened, it is held there, then moved.
        command(&link, 0, target(0.0, 0.0));
        core.pre_process(&mut dcm);
        dcm.set(&position, 0.4);
        command(&link, 0, target(0.0, 1.0));
        core.pre_process(&mut dcm);
        let commands = dcm.commands();
        let last_position = commands
            .iter()
            .rev()
            .find(|(alias, _, _)| alias == "Arora/HeadYaw/Position")
            .unwrap();
        // From 0.4, one step (at most 1 rad/s over the 10 ms of a first cycle) toward 0.
        assert!((0.39..=0.4).contains(&last_position.2[0]), "{commands:?}");
        assert_eq!(last_position.1, 0, "from where it is, at once");
    }

    #[test]
    fn leds_take_the_color_and_intensity() {
        let mut dcm = FakeDcm::nao(&[]);
        let (mut core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        link.set_commanding(true);
        lock(&link.shared.pending).leds.insert(
            0,
            LedCommand {
                color: Some(0xFF0000),
                intensity: Some(0.5),
                order: 1,
            },
        );
        core.pre_process(&mut dcm);
        core.pre_process(&mut dcm);
        assert_eq!(
            dcm.commands(),
            [("Arora/ChestLeds".to_string(), 0, vec![0.5, 0.0, 0.0])]
        );
    }

    #[test]
    fn a_joint_stiffened_for_the_first_time_starts_from_where_it_is() {
        let mut dcm = FakeDcm::nao(&["HeadYaw"]);
        let (mut core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        link.set_commanding(true);
        // A target first, toward which the commanded position steps, the joint relaxed.
        command(
            &link,
            0,
            JointCommand {
                position: Some(1.0),
                stiffness: None,
            },
        );
        core.pre_process(&mut dcm);
        core.pre_process(&mut dcm);
        command(
            &link,
            0,
            JointCommand {
                position: None,
                stiffness: Some(1.0),
            },
        );
        core.pre_process(&mut dcm);
        let commands = dcm.commands();
        let stiffened = commands
            .iter()
            .position(|(alias, _, _)| alias == "Arora/HeadYaw/Stiffness")
            .unwrap();
        // Right before the stiffness, the position restarts from the sensor's (0), at once.
        assert_eq!(commands[stiffened - 1].0, "Arora/HeadYaw/Position");
        assert_eq!(commands[stiffened - 1].1, 0, "{commands:?}");
        assert!(commands[stiffened - 1].2[0] <= 0.011, "{commands:?}");
    }

    #[test]
    fn a_new_hal_forgets_the_commands_of_the_previous_one() {
        let mut dcm = FakeDcm::nao(&["HeadYaw"]);
        let (mut core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        link.set_commanding(true);
        command(
            &link,
            0,
            JointCommand {
                position: Some(1.0),
                stiffness: Some(1.0),
            },
        );
        core.pre_process(&mut dcm);
        // The HAL goes and another starts, between two cycles: the old target is not resumed.
        link.set_commanding(false);
        link.set_commanding(true);
        let sent = dcm.commands().len();
        core.pre_process(&mut dcm);
        core.pre_process(&mut dcm);
        assert_eq!(dcm.commands().len(), sent, "{:?}", dcm.commands());
    }

    #[test]
    fn overlapping_led_groups_apply_in_the_order_written() {
        let mut dcm = FakeDcm::nao(&[]);
        let (mut core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        link.set_commanding(true);
        assert_eq!(link.led_groups(), ["ChestLeds", "AllLeds"]);
        {
            let mut pending = lock(&link.shared.pending);
            let led = |color, order| LedCommand {
                color: Some(color),
                intensity: None,
                order,
            };
            // AllLeds blue, then ChestLeds red: the chest ends red.
            pending.leds.insert(1, led(0x0000FF, 1));
            pending.leds.insert(0, led(0xFF0000, 2));
        }
        core.pre_process(&mut dcm);
        let aliases: Vec<String> = dcm.commands().into_iter().map(|(a, _, _)| a).collect();
        assert_eq!(aliases, ["Arora/AllLeds", "Arora/ChestLeds"]);
    }

    #[test]
    fn stopping_drops_the_pending_commands() {
        let mut dcm = FakeDcm::nao(&["HeadYaw"]);
        let (mut core, link) = create(&dcm, "Arora", &DcmRobotConfig::default()).unwrap();
        link.set_commanding(true);
        command(
            &link,
            0,
            JointCommand {
                position: Some(1.0),
                stiffness: None,
            },
        );
        link.set_commanding(false);
        link.set_commanding(true);
        core.pre_process(&mut dcm);
        assert!(dcm.commands().is_empty());
    }
}
