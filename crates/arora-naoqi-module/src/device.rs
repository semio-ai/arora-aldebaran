//! The Arora device, run in the process of NAOqi on a thread of its own, at its own pace.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use anyhow::Context;
use arora::{Arora, HostModule, RunnerArgs};
use arora_aldebaran::gestures::naoqi_gestures;
use arora_aldebaran::{ready_tree_initial_state, COMMAND_READY, READY_REQUEST};
use arora_hal::Hal;
use arora_hal_naoqi::say::{self, naoqi_say};
use arora_hal_naoqi::services;
use arora_hal_naoqi_dcm::{DcmHal, DcmLink};
use arora_simple_data_store::SimpleDataStore;
use arora_types::data::{DataStore, Key, KeyMeta, StateChange};
use arora_types::value::Value;
use clap::Parser;
use log::{error, info, warn};
use tokio::sync::oneshot;

use crate::config::{ModuleConfig, Tree};

/// The period of the device's steps, as the runner's.
const STEP_PERIOD: Duration = Duration::from_millis(20);
/// How long the device waits between two attempts to reach NAOqi, which loads the library
/// before it accepts connections.
const CONNECT_RETRY: Duration = Duration::from_secs(1);
/// How long the device waits for a first cycle of the DCM, to start from the robot's state.
const FIRST_CYCLE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long stopping the device waits for it: the rest posture included.
const STOP_TIMEOUT: Duration = Duration::from_secs(25);

/// What the device is doing, for the `Arora` service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// The library is loaded, the DCM plugin is not created yet (or the library was built
    /// without the bridge to the DCM).
    WaitingForDcm,
    /// The device is reaching NAOqi and preparing the robot.
    Starting,
    Running,
    Stopped,
    Failed(String),
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::WaitingForDcm => write!(f, "waiting for the DCM"),
            Status::Starting => write!(f, "starting"),
            Status::Running => write!(f, "running"),
            Status::Stopped => write!(f, "stopped"),
            Status::Failed(error) => write!(f, "failed: {error}"),
        }
    }
}

/// The state of the device the module's service reports.
pub struct DeviceState {
    pub status: Status,
    pub link: Option<DcmLink>,
    /// The device's data store, once it runs: the service requests the behavior through it.
    pub store: Option<SimpleDataStore>,
}

static STATE: Mutex<DeviceState> = Mutex::new(DeviceState {
    status: Status::WaitingForDcm,
    link: None,
    store: None,
});

pub fn state() -> MutexGuard<'static, DeviceState> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn set_status(status: Status) {
    match &status {
        Status::Failed(error) => error!("The Arora device failed: {error}"),
        status => info!("The Arora device is {status}"),
    }
    state().status = status;
}

/// Requests the default behavior, as the runner's `r` key does.
pub fn request_ready() -> anyhow::Result<()> {
    let store = state().store.clone().context("the device is not running")?;
    store.write(StateChange::set(
        COMMAND_READY,
        Value::String(READY_REQUEST.to_string()),
    ))?;
    Ok(())
}

/// The device's thread. Dropping it stops the device and waits for it, up to
/// [`STOP_TIMEOUT`]: the robot is put to rest, unless the configuration says otherwise. A
/// thread that does not stop in time is left running: the library stays loaded under it, as
/// `qi::module` keeps the library of a module NAOqi loaded (`RTLD_NODELETE`).
pub struct Device {
    stop: Option<oneshot::Sender<()>>,
    done: mpsc::Receiver<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Device {
    /// Starts the device on its thread, on the robot of `link`.
    pub fn start(link: DcmLink, config: ModuleConfig) -> std::io::Result<Self> {
        let (stop, stopped) = oneshot::channel();
        let (finished, done) = mpsc::channel();
        state().link = Some(link.clone());
        let thread = std::thread::Builder::new()
            .name("arora".to_string())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_on_thread(link, config, stopped)
                }));
                match outcome {
                    Ok(Ok(())) => set_status(Status::Stopped),
                    Ok(Err(error)) => set_status(Status::Failed(format!("{error:#}"))),
                    Err(_) => set_status(Status::Failed("the device panicked".to_string())),
                }
                state().store = None;
                let _ = finished.send(());
            })?;
        Ok(Self {
            stop: Some(stop),
            done,
            thread: Some(thread),
        })
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        match self.done.recv_timeout(STOP_TIMEOUT) {
            // Done: the thread only returns, and is joined.
            Ok(()) => {
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
            }
            Err(_) => warn!("The Arora device did not stop in time, it is left running"),
        }
    }
}

fn run_on_thread(
    link: DcmLink,
    config: ModuleConfig,
    stopped: oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("arora-worker")
        .enable_all()
        .build()
        .context("failed to start the device's runtime")?;
    // The device's future is not `Send`: it runs on this thread, as the runner's on its
    // main thread. Once stopped, it is dropped here, and the HAL waits for the robot to rest
    // (`block_in_place` leaves the runtime's context on a thread that is not a worker).
    runtime.block_on(async move {
        tokio::select! {
            result = run(link, config) => result,
            _ = stopped => {
                info!("Stopping the Arora device");
                Ok(())
            }
        }
    })
}

#[derive(Parser)]
struct Runner {
    #[command(flatten)]
    runner: RunnerArgs,
}

/// Runs the device until it stops: reaches NAOqi, starts the DCM HAL on the robot (which
/// puts it to rest), then runs Arora on it, as the `arora-aldebaran` runner does.
async fn run(link: DcmLink, config: ModuleConfig) -> anyhow::Result<()> {
    set_status(Status::Starting);
    let tree = config.tree()?;
    let device = Runner::try_parse_from(config.runner_arguments())
        .context("invalid device in the configuration")?
        .runner
        .device()?;
    let frontend = arora::operator::default_frontend();
    let builder = Arora::builder()
        .with_frontend(frontend)
        .with_module_dirs(device.dir(), &config.modules)
        .await?
        .with_device(device);
    #[cfg(feature = "studio")]
    let builder = builder.with_studio(if config.studio == Some(false) {
        arora::studio::Choice::Local
    } else {
        arora::studio::Choice::Studio
    });
    #[cfg(not(feature = "studio"))]
    if config.studio == Some(true) {
        warn!("This library is built without Semio Studio: the device serves the local bridge");
    }

    let robot = &config.robot;
    let node = loop {
        match services::connect(&robot.url, robot.user.as_deref(), robot.password.as_deref()).await
        {
            Ok(node) => break node,
            Err(error) => {
                info!("NAOqi does not answer yet ({error}), retrying");
                tokio::time::sleep(CONNECT_RETRY).await;
            }
        }
    };
    let hal = DcmHal::new(link.clone(), node, robot)
        .await
        .map_err(|e| anyhow::anyhow!("failed to start the DCM HAL: {e}"))?;
    if let Some(voice) = hal.voice() {
        say::install(voice);
    }

    let store = SimpleDataStore::new();
    if let Tree::Text(_) = tree {
        let cycles = link.cycles();
        let waited = tokio::time::timeout(FIRST_CYCLE_TIMEOUT, async {
            while link.cycles() == cycles {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        if waited.is_err() {
            warn!("No cycle of the DCM yet: the behavior starts from an unknown posture");
        }
        let state = hal
            .read_all()
            .await
            .map_err(|e| anyhow::anyhow!("failed to read the robot's state: {e}"))?;
        store.write(ready_tree_initial_state(&state))?;
    }
    store.set_meta([(Key::from(COMMAND_READY), KeyMeta::new().editable())].into())?;
    state().store = Some(store.clone());

    let mut builder = builder
        .with_hal(Box::new(hal))
        .with_host_module(HostModule::of::<naoqi_say::Module>())
        .with_data_store(Box::new(store))
        .with_host_module(HostModule::of::<naoqi_gestures::Module>())
        .with_step_period(STEP_PERIOD);
    if let Tree::Text(tree) = tree {
        builder = builder.with_groot(tree);
    }
    set_status(Status::Running);
    builder.run().await
}

/// The directory of this library, where its configuration may be.
pub fn library_dir() -> Option<PathBuf> {
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    // SAFETY: `dladdr` reads the address of a function of this library and fills `info`.
    let found = unsafe { libc::dladdr(library_dir as *const libc::c_void, &mut info) };
    if found == 0 || info.dli_fname.is_null() {
        return None;
    }
    // SAFETY: `dli_fname` is a NUL-terminated path, valid while the library is loaded.
    let path = unsafe { std::ffi::CStr::from_ptr(info.dli_fname) };
    let path = PathBuf::from(path.to_string_lossy().into_owned());
    let path = path.canonicalize().unwrap_or(path);
    path.parent().map(PathBuf::from)
}
