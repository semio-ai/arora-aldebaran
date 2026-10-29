//! The NAOqi services a HAL calls besides its keys: the connection to the robot, its
//! description, Autonomous Life, the sonars and the rest posture.
//!
//! [`NaoqiHal`](crate::NaoqiHal) drives the whole robot with them; a HAL that drives the
//! joints another way (in the loop of the DCM) calls them for what is not a key.

use std::time::Duration;

use arora_hal::HalDescription;
use log::{info, warn};
use qi::value::{Dynamic, KeyDynValueMap, Value as QiValue};
use qi::{AnyObject, ObjectExt};

use crate::conversions::identity;
use crate::naoqi_error::NaoqiRobotError;

/// A node connected to a robot's NAOqi.
pub type Node = qi::node::Node<qi::service_directory::Client>;

/// How long a HAL waits for the robot to reach its rest posture. Giving up cancels the call,
/// which leaves the robot where it got to: the wait is generous.
pub const REST_TIMEOUT: Duration = Duration::from_secs(20);

/// Connects to the NAOqi at `url`, with NAOqi authentication when a password is given (the
/// user is `nao` when none is).
pub async fn connect(
    url: &str,
    user: Option<&str>,
    password: Option<&str>,
) -> Result<Node, NaoqiRobotError> {
    // A host name may resolve to several addresses (IPv6 and IPv4), not all reachable: the
    // node tries them in order.
    let addresses = qi::Address::resolve_all(url)
        .map_err(|error| NaoqiRobotError::Connection(format!("invalid address {url}: {error}")))?;
    let credentials = password.map(|password| {
        let mut credentials = KeyDynValueMap::new();
        credentials.set("auth_user", user.unwrap_or("nao").to_string());
        credentials.set("auth_token", password.to_string());
        credentials
    });
    qi::node::init()
        .connect_to_space_at(addresses, credentials)
        .start()
        .await
        .map_err(|error| NaoqiRobotError::Connection(format!("cannot connect to {url}: {error}")))
}

/// A service the HAL cannot do without.
pub async fn required_service(node: &Node, name: &str) -> Result<AnyObject, NaoqiRobotError> {
    node.service(name).await.map_err(|error| {
        NaoqiRobotError::Connection(format!(
            "the NAOqi service {name} is not available: {error}"
        ))
    })
}

/// Turns Autonomous Life off, so that the robot's own life does not move the joints the
/// device drives. A robot without the service, or a refusal, leaves it as it is, warned.
pub async fn disable_autonomous_life(node: &Node) {
    let life = match node.service("ALAutonomousLife").await {
        Ok(life) => life,
        Err(error) => {
            warn!("ALAutonomousLife is not available, Autonomous Life is left as is: {error}");
            return;
        }
    };
    let state: String = match life.call::<String, _, _>("getState", ()).await {
        Ok(state) => state,
        Err(error) => {
            warn!("ALAutonomousLife.getState failed, Autonomous Life is left as is: {error}");
            return;
        }
    };
    if state == "disabled" {
        return;
    }
    match life
        .call::<(), _, _>("setState", "disabled".to_string())
        .await
    {
        Ok(()) => info!("Autonomous Life turned off (it was {state})"),
        Err(error) => warn!("ALAutonomousLife.setState(\"disabled\") failed: {error}"),
    }
}

/// Puts the robot to rest (`ALMotion.rest`): it sits or crouches, then its joints are
/// relaxed, and `ALMotion` sends no more commands until it is woken up. Waits up to
/// [`REST_TIMEOUT`]; a failure is warned.
pub async fn rest(motion: &AnyObject) {
    info!("Putting the robot to rest");
    match tokio::time::timeout(REST_TIMEOUT, motion.call::<(), _, _>("rest", ())).await {
        Ok(Ok(())) => info!("The robot is at rest"),
        Ok(Err(error)) => warn!("ALMotion.rest failed: {error}"),
        Err(_) => warn!("The robot did not reach its rest posture in time"),
    }
}

/// A subscription to `ALSonar`: the sonars measure only while someone is subscribed.
pub struct Sonar {
    service: AnyObject,
    name: String,
}

impl Sonar {
    /// Subscribes to `ALSonar`, which turns the sonars on. A robot without the service, or a
    /// refusal, leaves the sonar keys to whoever else turned them on, warned.
    pub async fn subscribe(node: &Node) -> Option<Self> {
        let service = match node.service("ALSonar").await {
            Ok(sonar) => sonar,
            Err(error) => {
                warn!("ALSonar is not available, the sonars may not measure: {error}");
                return None;
            }
        };
        let name = sonar_subscriber_name();
        match service.call::<(), _, _>("subscribe", name.clone()).await {
            Ok(()) => Some(Self { service, name }),
            Err(error) => {
                warn!("ALSonar.subscribe failed, the sonars may not measure: {error}");
                None
            }
        }
    }

    /// Unsubscribes, waiting up to `timeout`: the sonars stop unless someone else is
    /// subscribed.
    pub async fn unsubscribe(&self, timeout: Duration) {
        let released = tokio::time::timeout(
            timeout,
            self.service
                .call::<(), _, _>("unsubscribe", self.name.clone()),
        )
        .await;
        match released {
            Ok(Ok(())) => {}
            Ok(Err(error)) => warn!("ALSonar.unsubscribe failed: {error}"),
            Err(_) => warn!("NAOqi did not answer in time: the sonars still measure"),
        }
    }
}

/// A name of its own for each HAL subscribing to `ALSonar`, so that one HAL going does not
/// turn off the sonars another one, on the same robot, still reads.
fn sonar_subscriber_name() -> String {
    static HALS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or_default();
    format!(
        "arora-naoqi-{}-{nanos}-{}",
        std::process::id(),
        HALS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

/// The HAL description: what `known` sets (from a configuration), the rest queried from the
/// robot.
pub async fn describe(
    known: &HalDescription,
    node: &Node,
    memory: &AnyObject,
    motion: &AnyObject,
) -> HalDescription {
    let memory_string = |key: &'static str| async move {
        memory
            .call::<String, _, _>("getData", key.to_string())
            .await
            .ok()
    };
    let model_family = match &known.model_family {
        Some(family) => Some(family.clone()),
        None => {
            let robot_type = match node.service("ALRobotModel").await {
                Ok(model) => model.call::<String, _, _>("getRobotType", ()).await.ok(),
                Err(_) => None,
            };
            // NAOqi 2.1's ALRobotModel has no getRobotType; ALMemory has the type on every
            // version.
            let robot_type = match robot_type {
                Some(robot_type) => Some(robot_type),
                None => memory_string("RobotConfig/Body/Type").await,
            };
            robot_type.map(|robot_type| identity::model_family(&robot_type))
        }
    };
    let software_version = match &known.software_version {
        Some(version) => Some(version.clone()),
        None => match node.service("ALSystem").await {
            Ok(system) => system.call::<String, _, _>("systemVersion", ()).await.ok(),
            Err(_) => None,
        },
    };
    let hardware_version = match &known.hardware_version {
        Some(version) => Some(version.clone()),
        None => {
            let body_type = match motion
                .call::<Dynamic<QiValue<'static>>, _, _>("getRobotConfig", ())
                .await
            {
                Ok(Dynamic(robot_config)) => {
                    identity::robot_config_entry(robot_config, "Model Type")
                }
                Err(_) => None,
            };
            identity::hardware_version(
                memory_string("RobotConfig/Body/BaseVersion")
                    .await
                    .as_deref(),
                memory_string("RobotConfig/Head/BaseVersion")
                    .await
                    .as_deref(),
                body_type.as_deref(),
            )
        }
    };
    HalDescription {
        model_family,
        hardware_version,
        software_version,
    }
}
