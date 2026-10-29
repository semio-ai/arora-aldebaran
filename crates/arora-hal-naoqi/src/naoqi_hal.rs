use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use arora_behavior::Status;
use arora_hal::{Hal, HalDescription, HalResult, UpdatesStream};
use arora_types::data::{Key, State, StateChange};
use arora_types::value::Value;
use async_trait::async_trait;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use log::{debug, info, warn};
use qi::value::{Dynamic, Value as QiValue};
use qi::{AnyObject, ObjectExt};
use tokio::task::AbortHandle;

use crate::config::NaoqiRobotConfig;
use crate::conversions::{arora_f64, keys, qi_f64, qi_list, JointTable};
use crate::naoqi_error::NaoqiRobotError;
use crate::services::{self, required_service, Node, Sonar};

/// How long dropping the HAL waits for the robot to stop moving and release its sonars.
const STOP_TIMEOUT: Duration = Duration::from_secs(1);

/// A NAOqi robot (NAO, Pepper) as an [`arora_hal::Hal`], driven through the `qi` framework.
///
/// The robot's topology (joints, available services) is discovered at construction; the
/// remaining state is interior-mutable so every trait method takes `&self`.
///
/// Sensors are sampled from `ALMemory` by a task at the configured period and touch events
/// are received through `ALMemory` signals; both feed the current state and the `updates()`
/// subscribers. Writes turn keys into NAOqi calls (`ALMotion.setAngles`,
/// `ALLeds.fadeRGB`...): `write()` awaits them, `try_send` hands them to the queued-write
/// task so the caller never waits on the robot. Speech is not a key: the robot's
/// [`voice`](NaoqiHal::voice) speaks the behavior tree's `Say` leaves ([`crate::say`]).
pub struct NaoqiHal {
    inner: Arc<Inner>,
    /// Feeds the queued-write task.
    outbound: UnboundedSender<StateChange>,
    /// The robot's voice, which says and stops in the order asked.
    voice: Voice,
    /// The sampling, event and queued-write tasks, aborted with the HAL: a command still
    /// queued when the HAL goes is not sent.
    tasks: Vec<AbortHandle>,
    /// The queued-write task, awaited once aborted so that the drop reads the motion state
    /// it may have changed.
    writer: Option<tokio::task::JoinHandle<()>>,
    /// The runtime the HAL's tasks run on, where its voice speaks.
    runtime: tokio::runtime::Handle,
}

struct Inner {
    config: NaoqiRobotConfig,
    /// The node connected to the robot; the sessions to its services live as long as it does.
    _node: Node,
    memory: AnyObject,
    motion: AnyObject,
    tts: AnyObject,
    leds: Option<AnyObject>,
    /// The HAL's subscription to `ALSonar`: the sonars measure only while someone is
    /// subscribed.
    sonar: Option<Sonar>,
    joints: JointTable,
    description: HalDescription,
    /// The current state: sampled sensors, received events and cached commands.
    current_state: Mutex<State>,
    /// Observers registered through `updates()`.
    subscribers: Mutex<Vec<UnboundedSender<StateChange>>>,
    /// The last base velocity command `(x, y, theta)`, merged across its keys.
    base_velocity: Mutex<[f64; 3]>,
    /// Whether the base may be moving: set before a non-zero velocity is sent, cleared
    /// only once a zero velocity is confirmed. The cached velocity may be zero while this
    /// is set, after a stop that failed or was never sent.
    base_moving: Mutex<bool>,
    /// Held through a write, from its cached state to its last NAOqi call, so that
    /// overlapping writes reach the robot in the order they update the cache.
    write_order: tokio::sync::Mutex<()>,
}

/// What the sensor sampling keeps between two samples: the previous joint positions, to
/// derive velocities.
#[derive(Default)]
struct Sampling {
    previous_positions: Option<(Instant, Vec<(String, f64)>)>,
}

/// Fold `later` into `earlier`, per-key newest wins: applying the folded change equals
/// applying both in order.
fn fold_newest_wins(earlier: &mut StateChange, later: StateChange) {
    for key in later.unset {
        earlier.set.remove(&key);
        earlier.unset.insert(key);
    }
    for (key, value) in later.set {
        earlier.unset.remove(&key);
        earlier.set.insert(key, value);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl NaoqiHal {
    /// Connects to the robot described by the configuration and starts driving it.
    ///
    /// Validates the configuration, connects to NAOqi, discovers the joints and the
    /// services, takes a first sample of the sensors and spawns the sampling, event and
    /// queued-write tasks. Must be called within a Tokio runtime that outlives the HAL.
    pub async fn new(config: NaoqiRobotConfig) -> Result<Self, NaoqiRobotError> {
        config.validate()?;
        let node = services::connect(
            &config.url,
            config.user.as_deref(),
            config.password.as_deref(),
        )
        .await?;

        let memory = required_service(&node, "ALMemory").await?;
        let motion = required_service(&node, "ALMotion").await?;
        let tts = required_service(&node, "ALTextToSpeech").await?;
        let leds = match node.service("ALLeds").await {
            Ok(leds) => Some(leds),
            Err(error) => {
                warn!("ALLeds is not available, LED keys are ignored: {error}");
                None
            }
        };
        if config.disable_autonomous_life {
            services::disable_autonomous_life(&node).await;
        }
        let known = HalDescription {
            model_family: config.model_family.clone(),
            hardware_version: config.hardware_version.clone(),
            software_version: config.software_version.clone(),
        };
        let description = services::describe(&known, &node, &memory, &motion).await;
        let names: Vec<String> = motion
            .call("getBodyNames", "Body".to_string())
            .await
            .map_err(|error| {
                NaoqiRobotError::Connection(format!("cannot list the joints: {error}"))
            })?;
        let joints = JointTable::new(names, &config.joint_ids).map_err(NaoqiRobotError::Config)?;
        // Last of the steps that may fail the start, so that a failed start leaves no
        // subscription behind.
        let sonar = if config.sensors.sonar {
            Sonar::subscribe(&node).await
        } else {
            None
        };
        let unknown = || "unknown".to_string();
        info!(
            "Connected to NAOqi at {}: {} robot, hardware {}, NAOqi {}, {} joints",
            config.url,
            description.model_family.clone().unwrap_or_else(unknown),
            description.hardware_version.clone().unwrap_or_else(unknown),
            description.software_version.clone().unwrap_or_else(unknown),
            joints.len()
        );

        let inner = Arc::new(Inner {
            config,
            _node: node,
            memory,
            motion,
            tts,
            leds,
            sonar,
            joints,
            description,
            current_state: Mutex::new(State::new()),
            subscribers: Mutex::new(Vec::new()),
            base_velocity: Mutex::new([0.0; 3]),
            base_moving: Mutex::new(false),
            write_order: tokio::sync::Mutex::new(()),
        });

        // A first sample, so that the first `updates()` subscribers get a snapshot.
        let mut sampling = Sampling::default();
        if let Err(error) = inner.sample_sensors(&mut sampling).await {
            warn!("Initial sensor sampling failed: {error}");
        }

        let mut tasks = Vec::new();
        // The sensor sampling task.
        let period = Duration::from_millis(inner.config.sensor_period_ms);
        let sampler = Arc::clone(&inner);
        tasks.push(
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(period);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    interval.tick().await;
                    if let Err(error) = sampler.sample_sensors(&mut sampling).await {
                        warn!("Sensor sampling failed: {error}");
                    }
                }
            })
            .abort_handle(),
        );
        // The touch event tasks.
        if inner.config.sensors.touch {
            for (event, key) in keys::TOUCH_EVENTS {
                let watcher = Arc::clone(&inner);
                tasks.push(tokio::spawn(watcher.watch_event(event, key)).abort_handle());
            }
        }
        // The queued-write task: receives the changes `try_send` enqueues, folds any backlog
        // into one change (per-key newest wins) so slow round-trips coalesce, and drives the
        // same write path as `write()`. It is aborted with the HAL.
        let (outbound, mut outbound_rx) = futures::channel::mpsc::unbounded::<StateChange>();
        let queued = Arc::clone(&inner);
        let writer = tokio::spawn(async move {
            while let Some(mut changes) = outbound_rx.next().await {
                while let Ok(later) = outbound_rx.try_recv() {
                    fold_newest_wins(&mut changes, later);
                }
                if let Err(error) = queued.write(changes).await {
                    warn!("Queued write failed: {error}");
                }
            }
        });
        let voice = Voice::new(inner.tts.clone());

        Ok(Self {
            inner,
            outbound,
            voice,
            tasks,
            writer: Some(writer),
            runtime: tokio::runtime::Handle::current(),
        })
    }

    /// The robot's voice (`ALTextToSpeech`), for the `Say` leaves: hand it to
    /// [`say::install`](crate::say::install).
    pub fn voice(&self) -> Voice {
        self.voice.clone()
    }
}

/// A NAOqi robot's voice: says a sentence without waiting for it, stops what it says.
///
/// Sentences and stops reach `ALTextToSpeech` in the order they are asked: a stop never
/// overtakes the sentence before it, and a sentence is sent once the stop before it is done.
#[derive(Clone)]
pub struct Voice {
    speech: UnboundedSender<Speech>,
    runtime: tokio::runtime::Handle,
}

/// What the voice's task is asked.
enum Speech {
    Say(String, Utterance),
    Stop,
}

impl Voice {
    /// The voice of `tts`, an `ALTextToSpeech`. Must be called within a Tokio runtime: the
    /// voice speaks from a task of it, which ends once every clone of the voice is gone.
    pub fn new(tts: AnyObject) -> Self {
        let (speech, speech_rx) = futures::channel::mpsc::unbounded();
        tokio::spawn(speak(tts, speech_rx));
        Self {
            speech,
            runtime: tokio::runtime::Handle::current(),
        }
    }

    /// Start saying `text`; the returned utterance tells when it has been said.
    pub(crate) fn say(&self, text: String) -> Utterance {
        let utterance = Utterance::default();
        if self
            .speech
            .unbounded_send(Speech::Say(text, utterance.clone()))
            .is_err()
        {
            warn!("ALTextToSpeech is gone, a sentence is not said");
            *lock(&utterance.0) = Some(false);
        }
        utterance
    }

    /// Stop what the robot is saying.
    pub(crate) fn stop(&self) {
        let _ = self.speech.unbounded_send(Speech::Stop);
    }

    /// The runtime the voice speaks on.
    pub(crate) fn runtime(&self) -> &tokio::runtime::Handle {
        &self.runtime
    }
}

/// The voice's task: sends the sentences and stops in order. A sentence is polled until it
/// waits on the robot, so its request is queued before any later one; a stop is awaited
/// before the next command, while the sentences under way go on.
async fn speak(tts: AnyObject, mut commands: UnboundedReceiver<Speech>) {
    let mut under_way = FuturesUnordered::new();
    loop {
        tokio::select! {
            biased;
            Some(()) = under_way.next(), if !under_way.is_empty() => {}
            command = commands.next() => match command {
                None => break,
                Some(Speech::Say(text, utterance)) => {
                    under_way.push(say(tts.clone(), text, utterance));
                    while let std::task::Poll::Ready(Some(())) =
                        futures::poll!(under_way.next())
                    {}
                }
                Some(Speech::Stop) => {
                    let mut stop = std::pin::pin!(stop_all(tts.clone()));
                    loop {
                        tokio::select! {
                            biased;
                            () = &mut stop => break,
                            Some(()) = under_way.next(), if !under_way.is_empty() => {}
                        }
                    }
                }
            }
        }
    }
    while under_way.next().await.is_some() {}
}

/// Says `text`; `ALTextToSpeech.say` returns once the sentence is spoken, or stopped.
async fn say(tts: AnyObject, text: String, utterance: Utterance) {
    let said = match tts.call::<(), _, _>("say", text.clone()).await {
        Ok(()) => {
            debug!("Said \"{text}\"");
            true
        }
        Err(error) => {
            warn!("ALTextToSpeech.say(\"{text}\") failed: {error}");
            false
        }
    };
    *lock(&utterance.0) = Some(said);
}

async fn stop_all(tts: AnyObject) {
    if let Err(error) = tts.call::<(), _, _>("stopAll", ()).await {
        warn!("ALTextToSpeech.stopAll failed: {error}");
    }
}

/// A sentence handed to the robot's voice: under way until `ALTextToSpeech.say` returns.
#[derive(Clone, Default)]
pub(crate) struct Utterance(Arc<Mutex<Option<bool>>>);

impl Utterance {
    /// `Running` while under way, then `Success` when said, `Failure` when refused.
    pub(crate) fn status(&self) -> Status {
        match *lock(&self.0) {
            None => Status::Running,
            Some(true) => Status::Success,
            Some(false) => Status::Failure,
        }
    }
}

impl Inner {
    /// The `ALMemory` keys sampled periodically, with the Arora keys they feed.
    fn sampled_memory_keys(&self) -> Vec<(String, String)> {
        let mut keys = Vec::new();
        if self.config.sensors.joints {
            keys.extend(self.joints.memory_keys());
        }
        let fixed = [
            (self.config.sensors.battery, keys::BATTERY_MEMORY_KEYS),
            (self.config.sensors.inertial, keys::INERTIAL_MEMORY_KEYS),
            (self.config.sensors.sonar, keys::SONAR_MEMORY_KEYS),
        ];
        for (enabled, table) in fixed {
            if enabled {
                keys.extend(
                    table
                        .iter()
                        .map(|(memory, arora)| (memory.to_string(), arora.to_string())),
                );
            }
        }
        keys
    }

    /// Samples the sensors in one `ALMemory.getListData` call and publishes what changed.
    async fn sample_sensors(&self, sampling: &mut Sampling) -> Result<(), NaoqiRobotError> {
        let keys = self.sampled_memory_keys();
        if keys.is_empty() {
            return Ok(());
        }
        let names: Vec<String> = keys.iter().map(|(memory, _)| memory.clone()).collect();
        let Dynamic(values) = self
            .memory
            .call::<Dynamic<QiValue<'static>>, _, _>("getListData", names)
            .await?;
        let values = qi_list(values).ok_or_else(|| {
            NaoqiRobotError::Conversion("ALMemory.getListData did not return a list".to_string())
        })?;
        if values.len() != keys.len() {
            return Err(NaoqiRobotError::Conversion(format!(
                "ALMemory.getListData returned {} values for {} keys",
                values.len(),
                keys.len()
            )));
        }

        let now = Instant::now();
        let mut change = StateChange::new();
        let mut positions = Vec::new();
        for ((_, arora_key), value) in keys.iter().zip(values) {
            // Keys the robot does not have come back as invalid values: skip them.
            let Some(number) = qi_f64(&value) else {
                continue;
            };
            let key = Key::from(arora_key.as_str());
            if key.get_component() == Some(keys::POSITION) {
                positions.push((key.get_entity().to_string(), number));
            }
            if arora_key == keys::BATTERY_CURRENT {
                change.set.insert(
                    Key::from(keys::BATTERY_CHARGING),
                    Some(Value::Boolean(number > 0.0)),
                );
            }
            change.set.insert(key, Some(Value::F64(number)));
        }
        // Joint velocities are derived from consecutive position samples.
        if let Some((previous_time, previous_positions)) = &sampling.previous_positions {
            let elapsed = now.duration_since(*previous_time).as_secs_f64();
            if elapsed > 0.0 {
                for (joint, position) in &positions {
                    if let Some((_, previous)) = previous_positions
                        .iter()
                        .find(|(previous_joint, _)| previous_joint == joint)
                    {
                        change.set.insert(
                            Key::from(format!("{joint}.{}", keys::VELOCITY)),
                            Some(Value::F64((position - previous) / elapsed)),
                        );
                    }
                }
            }
        }
        sampling.previous_positions = Some((now, positions));
        self.publish(change);
        Ok(())
    }

    /// Drives a boolean key from an `ALMemory` event: its current value, then every raise.
    /// The raises are subscribed to before the current value is read, so none is lost in
    /// between; those raised meanwhile wait in the stream and are applied after it.
    async fn watch_event(self: Arc<Self>, event: &'static str, key: &'static str) {
        let subscriber: AnyObject = match self.memory.call("subscriber", event.to_string()).await {
            Ok(subscriber) => subscriber,
            Err(error) => {
                warn!("Cannot subscribe to the {event} event, {key} is not driven: {error}");
                return;
            }
        };
        let mut raises = match subscriber
            .subscribe::<_, Dynamic<QiValue<'static>>>("signal")
            .await
        {
            Ok(raises) => raises,
            Err(error) => {
                warn!("Cannot subscribe to the {event} event, {key} is not driven: {error}");
                return;
            }
        };
        if let Ok(Dynamic(value)) = self
            .memory
            .call::<Dynamic<QiValue<'static>>, _, _>("getData", event.to_string())
            .await
        {
            if let Some(number) = qi_f64(&value) {
                self.publish(StateChange::set(key, Value::Boolean(number != 0.0)));
            }
        }
        while let Some(Dynamic(value)) = raises.next().await {
            if let Some(number) = qi_f64(&value) {
                self.publish(StateChange::set(key, Value::Boolean(number != 0.0)));
            }
        }
        debug!("The {event} event stream ended");
    }

    /// Applies a change to the current state and notifies subscribers of what actually
    /// changed. The state stays locked until they are notified, so that a feed registering
    /// meanwhile gets either the change in its snapshot or the change as a delta, in order.
    fn publish(&self, change: StateChange) {
        let mut delta = StateChange::new();
        let mut state = lock(&self.current_state);
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
        if delta.is_empty() {
            return;
        }
        lock(&self.subscribers).retain(|tx| tx.unbounded_send(delta.clone()).is_ok());
    }

    /// Turns a change into NAOqi commands. Returns `Ok` when every command succeeded, or when
    /// there was nothing to command (partial failures are logged), and `Err` when every
    /// command failed.
    async fn write(self: &Arc<Self>, changes: StateChange) -> Result<(), NaoqiRobotError> {
        let _order = self.write_order.lock().await;
        let mut joint_targets: Vec<(String, f32)> = Vec::new();
        let mut stiffness_targets: Vec<(String, f32)> = Vec::new();
        let mut led_colors: Vec<(String, i32)> = Vec::new();
        let mut led_intensities: Vec<(String, f32)> = Vec::new();
        let mut base_velocity = None;

        for (key, value) in &changes.set {
            let Some(value) = value else {
                continue;
            };
            let entity = key.get_entity();
            let attributes = key.get_attributes();
            match (entity, attributes.as_slice()) {
                (joint, [keys::TARGET_POSITION]) => {
                    if let Some(name) = self.joints.naoqi_name(joint) {
                        match arora_f64(value) {
                            Some(angle) => joint_targets.push((name.to_string(), angle as f32)),
                            None => warn!("Ignoring a non-numeric target position for {joint}"),
                        }
                    }
                }
                (joint, [keys::TARGET_STIFFNESS]) => {
                    if let Some(name) = self.joints.naoqi_name(joint) {
                        match arora_f64(value) {
                            Some(stiffness) => {
                                stiffness_targets.push((name.to_string(), stiffness as f32))
                            }
                            None => warn!("Ignoring a non-numeric target stiffness for {joint}"),
                        }
                    }
                }
                (keys::LED_ENTITY, [group, "color"]) => match arora_f64(value) {
                    Some(rgb) => led_colors.push((group.to_string(), rgb as i32)),
                    None => warn!("Ignoring a non-numeric color for the {group} LEDs"),
                },
                (keys::LED_ENTITY, [group, "intensity"]) => match arora_f64(value) {
                    Some(intensity) => led_intensities.push((group.to_string(), intensity as f32)),
                    None => warn!("Ignoring a non-numeric intensity for the {group} LEDs"),
                },
                ("velocity", ["x"]) | ("velocity", ["y"]) | ("rotation", ["z"]) => {
                    if let Some(component) = arora_f64(value) {
                        let mut velocity = lock(&self.base_velocity);
                        let index = match (entity, attributes[0]) {
                            ("velocity", "x") => 0,
                            ("velocity", "y") => 1,
                            _ => 2,
                        };
                        velocity[index] = component;
                        base_velocity = Some(*velocity);
                    }
                }
                _ => {}
            }
        }

        // Cache every written key, so that reads reflect the commands sent.
        {
            let mut state = lock(&self.current_state);
            for (key, value) in &changes.set {
                state.set(key.clone(), value.clone());
            }
            for key in &changes.unset {
                state.unset(key);
            }
        }

        let mut attempted = 0;
        let mut failures = Vec::new();
        if !joint_targets.is_empty() {
            attempted += 1;
            let (names, angles): (Vec<String>, Vec<f32>) = joint_targets.into_iter().unzip();
            let speed = self.config.joint_speed_fraction as f32;
            if let Err(error) = self
                .motion
                .call::<(), _, _>("setAngles", (names, angles, speed))
                .await
            {
                failures.push(format!("ALMotion.setAngles: {error}"));
            }
        }
        if !stiffness_targets.is_empty() {
            attempted += 1;
            let (names, values): (Vec<String>, Vec<f32>) = stiffness_targets.into_iter().unzip();
            if let Err(error) = self
                .motion
                .call::<(), _, _>("setStiffnesses", (names, values))
                .await
            {
                failures.push(format!("ALMotion.setStiffnesses: {error}"));
            }
        }
        if let Some(leds) = &self.leds {
            for (group, rgb) in led_colors {
                attempted += 1;
                if let Err(error) = leds
                    .call::<(), _, _>("fadeRGB", (group.clone(), rgb, 0.0f32))
                    .await
                {
                    failures.push(format!("ALLeds.fadeRGB({group}): {error}"));
                }
            }
            for (group, intensity) in led_intensities {
                attempted += 1;
                if let Err(error) = leds
                    .call::<(), _, _>("setIntensity", (group.clone(), intensity))
                    .await
                {
                    failures.push(format!("ALLeds.setIntensity({group}): {error}"));
                }
            }
        }
        if let Some([x, y, theta]) = base_velocity {
            attempted += 1;
            let stopping = [x, y, theta] == [0.0; 3];
            if !stopping {
                *lock(&self.base_moving) = true;
            }
            match self
                .motion
                .call::<(), _, _>("moveToward", (x as f32, y as f32, theta as f32))
                .await
            {
                Ok(()) if stopping => *lock(&self.base_moving) = false,
                Ok(()) => {}
                Err(error) => failures.push(format!("ALMotion.moveToward: {error}")),
            }
        }

        if attempted > 0 && failures.len() == attempted {
            return Err(NaoqiRobotError::Write(failures.join("; ")));
        }
        for failure in failures {
            warn!("Partial write failure: {failure}");
        }
        Ok(())
    }
}

#[async_trait]
impl Hal for NaoqiHal {
    async fn describe(&self) -> HalDescription {
        self.inner.description.clone()
    }

    async fn read(&self, keys: &[Key]) -> HalResult<Vec<Option<Value>>> {
        let state = lock(&self.inner.current_state);
        Ok(keys
            .iter()
            .map(|key| state.get(key).cloned().flatten())
            .collect())
    }

    async fn read_all(&self) -> HalResult<State> {
        Ok(lock(&self.inner.current_state).clone())
    }

    async fn write(&self, changes: StateChange) -> HalResult<()> {
        self.inner.write(changes).await.map_err(Into::into)
    }

    /// Hands the change to the queued-write task; never blocks.
    fn try_send(&self, changes: &StateChange) {
        if self.outbound.unbounded_send(changes.clone()).is_err() {
            warn!("Dropping a state change: the queued-write task is gone");
        }
    }

    /// A feed of the changes the robot reports. Each feed first receives the current state
    /// as one `StateChange` (when non-empty), then every subsequent change.
    fn updates(&self) -> UpdatesStream {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        // Registered under the state's lock, as `publish` notifies: no change falls between
        // the snapshot and the feed's first delta.
        let state = lock(&self.inner.current_state);
        if !state.is_empty() {
            let _ = tx.unbounded_send(StateChange {
                set: state.storage.clone(),
                unset: Default::default(),
            });
        }
        lock(&self.inner.subscribers).push(tx);
        drop(state);
        Box::pin(rx)
    }
}

/// Dropping the HAL stops its tasks, then the robot's base, puts the robot to rest (unless
/// [`rest_on_exit`](NaoqiRobotConfig::rest_on_exit) is off) and releases the sonars: a
/// velocity command persists in NAOqi, so a robot left moving would keep moving without its
/// controller, and a robot left stiff would keep its joints powered. Within a
/// multi-threaded runtime the drop waits for all that, up to [`STOP_TIMEOUT`] for the base
/// and the sonars and [`REST_TIMEOUT`](services::REST_TIMEOUT) for the rest; elsewhere it is done from a task of the
/// HAL's runtime.
impl Drop for NaoqiHal {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        let writer = self.writer.take();
        if let Some(writer) = &writer {
            writer.abort();
        }
        let inner = Arc::clone(&self.inner);
        let shutdown = async move {
            let stopped = tokio::time::timeout(STOP_TIMEOUT, async {
                // The writer may be in the middle of a poll on another thread: once it is
                // gone, the motion state is the last it will be.
                if let Some(writer) = writer {
                    let _ = writer.await;
                }
                let _order = inner.write_order.lock().await;
                if *lock(&inner.base_moving) {
                    match inner
                        .motion
                        .call::<(), _, _>("moveToward", (0.0f32, 0.0f32, 0.0f32))
                        .await
                    {
                        Ok(()) => {
                            *lock(&inner.base_velocity) = [0.0; 3];
                            *lock(&inner.base_moving) = false;
                            info!("The robot's base is stopped");
                        }
                        Err(error) => warn!("Cannot stop the robot's base: {error}"),
                    }
                }
            })
            .await;
            if stopped.is_err() {
                warn!("NAOqi did not answer in time: the base may still move");
            }
            if inner.config.rest_on_exit {
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
