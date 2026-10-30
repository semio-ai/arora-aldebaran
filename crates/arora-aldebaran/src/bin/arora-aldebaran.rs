//! arora-aldebaran: runs an Aldebaran robot (NAO, Pepper) as an Arora device.
//!
//! Usage: `arora-aldebaran [--tree <FILE> | --no-tree] [--module <DIR>]... [--device <NAME>]
//! [--device-dir <PATH>] [--bridge-port <PORT>] [--no-studio] <ROBOT> [OVERRIDES]`
//! (`--help` lists the options; `--no-studio` in a build with the `studio` feature).
//!
//! The robot is reached through the HAL the build selects ([`arora_aldebaran::robot`]): with
//! `naoqi`, the default, the first argument (or the `ROBOT` env var) is the address of its
//! NAOqi or a path to a `NaoqiRobotConfig` JSON file, and an optional second one overrides it.
//!
//! The device serves the open local bridge. Built with the `studio` feature, it serves its
//! Semio Studio bridge instead, unless the run passes `--no-studio`: the device runs at once,
//! and a device that is not enrolled with Studio enrolls while it runs, as `arora::studio`
//! says: the device client enrolls it, presented by the run's front end, and the credential
//! is kept under the device's name.
//!
//! `--device`, `--device-dir` and `--bridge-port` (`ARORA_DEVICE`, `ARORA_DEVICE_DIR`,
//! `ARORA_BRIDGE_PORT`) give the device the run is: its name on this machine, its
//! directory and the port of its open local bridge.
//!
//! The device runs a behavior tree: by default [`READY_TREE`], which says "Arora is ready"
//! with a hand gesture at start; `--tree` runs another one, `--no-tree` none. The tree's
//! leaves are the HAL's voice (`Say`), the gestures (`HandsReady`) and the native nodes. On
//! the terminal UI, `r` requests the default behavior again through `command.ready`; the key
//! is open to remote writers too.
//!
//! The device carries the modules its device directory holds under `modules/`, then each
//! `--module` directory, as the `arora` runner does (`AroraBuilder::with_module_dirs`): a
//! module directory is a module's `header.json` beside its artifact, and optionally its
//! records in `records/`, which describe every function it exports. A tree's leaves call
//! their functions as they call `Say` and `HandsReady`.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use arora::tui::{commands_frontend, TuiCommand};
use arora::{Arora, HostModule, RunnerArgs};
use arora_aldebaran::gestures::naoqi_gestures;
use arora_aldebaran::{ready_tree_initial_state, robot, COMMAND_READY, READY_REQUEST, READY_TREE};
use arora_simple_data_store::SimpleDataStore;
use arora_types::data::{DataStore, Key, KeyMeta, StateChange};
use arora_types::value::Value;
use clap::Parser;
use futures::StreamExt;

/// The step period: 50 Hz, a pace NAOqi's `setAngles` round trips keep up with.
const STEP_PERIOD: Duration = Duration::from_millis(20);

/// The terminal UI key that plays the default behavior again.
const READY_KEY: char = 'r';

/// The runner's command line.
#[derive(Debug, Parser)]
#[command(
    name = "arora-aldebaran",
    about = "Runs an Aldebaran robot (NAO, Pepper) as an Arora device"
)]
struct Cli {
    /// A Groot behavior tree to run instead of the default one.
    #[arg(long, value_name = "FILE", conflicts_with = "no_tree")]
    tree: Option<PathBuf>,
    /// Run no behavior tree.
    #[arg(long)]
    no_tree: bool,
    /// The robot, as the build's HAL names it.
    #[arg(env = "ROBOT", help = format!("The robot: {}", robot::ROBOT_HELP))]
    robot: String,
    /// A JSON file overriding the robot's configuration.
    overrides: Option<String>,
    #[command(flatten)]
    runner: RunnerArgs,
    /// `--no-studio`, under this crate's own `studio` feature: arora's `studio-bridge` may
    /// be enabled by another runner in the same build, and this one, built without its own,
    /// neither connects nor takes the option.
    #[cfg(feature = "studio")]
    #[command(flatten)]
    studio: arora::studio::StudioArgs,
}

impl Cli {
    /// The Groot XML of the tree the device runs: `--tree`'s file, none under
    /// `--no-tree`, else the default behavior.
    fn tree(&self) -> anyhow::Result<Option<String>> {
        if self.no_tree {
            return Ok(None);
        }
        match &self.tree {
            Some(path) => std::fs::read_to_string(path)
                .with_context(|| format!("failed to read the tree {}", path.display()))
                .map(Some),
            None => Ok(Some(READY_TREE.to_string())),
        }
    }
}

fn ready() -> StateChange {
    StateChange::set(COMMAND_READY, Value::String(READY_REQUEST.to_string()))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let tree = cli.tree()?;

    // The front end first: it installs the log sink, so the connection's logs are shown.
    let (frontend, mut commands) = commands_frontend(vec![TuiCommand {
        key: READY_KEY,
        label: "say \"Arora is ready\"".to_string(),
        prompt: None,
    }]);
    #[cfg(feature = "studio")]
    let enroller = frontend.operator.clone().enroller();
    // The modules the device carries, read before the robot is reached: a module directory
    // that does not read fails the start at once.
    let device = cli.runner.device()?;
    let builder = Arora::builder()
        .with_frontend(frontend)
        .with_module_dirs(device.dir(), &cli.runner.module)
        .await?;

    // One runtime carries everything: the HAL's tasks spawn on it at construction, its
    // voice speaks on it, and arora drives the device on it.
    log::info!("Connecting to the robot through {}", robot::HAL);
    let robot = robot::connect(&cli.robot, cli.overrides.as_deref()).await?;

    // The default behavior starts at once; the request key is open to remote writers, so
    // Studio or a bridge client can play it again as the terminal UI does.
    let store = SimpleDataStore::new();
    if tree.is_some() {
        let state = robot
            .hal
            .read_all()
            .await
            .map_err(|e| anyhow::anyhow!("failed to read the robot's state: {e}"))?;
        store.write(ready_tree_initial_state(&state))?;
    }
    store.set_meta([(Key::from(COMMAND_READY), KeyMeta::new().editable())].into())?;
    let requests = store.clone();
    tokio::spawn(async move {
        while let Some(command) = commands.next().await {
            if command.key == READY_KEY {
                if let Err(error) = requests.write(ready()) {
                    log::warn!("could not request the ready behavior: {error}");
                }
            }
        }
    });

    // Built with `studio`, the run's bridge is the device's Studio bridge, reporting the
    // model the HAL describes, unless the run passes `--no-studio`; the device enrolls beside
    // the run, through the front end's enroller, the enrollment's end failing the run.
    #[cfg(feature = "studio")]
    let studio = if cli.studio.connects() {
        let model = arora::studio::reported_model(&robot.hal.describe().await);
        Some(arora::studio::of(&device)?.bridge(model))
    } else {
        None
    };

    let mut builder = robot
        .install(builder.with_device(device))
        .with_data_store(Box::new(store))
        .with_host_module(HostModule::of::<naoqi_gestures::Module>())
        .with_step_period(STEP_PERIOD);
    if let Some(tree) = tree {
        builder = builder.with_groot(tree);
    }
    #[cfg(feature = "studio")]
    if let Some((bridge, handle)) = studio {
        let run = builder.with_bridge(Box::new(bridge)).run();
        return tokio::select! {
            ran = run => ran,
            ended = handle.enroll_when_needed(&*enroller) => Err(ended.into()),
        };
    }
    builder.run().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// The robot, its overrides, the tree and `--module` in any order, the
    /// directory after a space or `=`.
    #[test]
    fn the_robot_the_tree_and_module_directories_parse() {
        let cli = Cli::try_parse_from([
            "arora-aldebaran",
            "--module",
            "a",
            "--tree",
            "wave.groot.xml",
            "tcp://nao.local:9559",
            "--module=b",
            "overrides.json",
        ])
        .expect("the command line parses");
        assert_eq!(cli.robot, "tcp://nao.local:9559");
        assert_eq!(cli.overrides.as_deref(), Some("overrides.json"));
        assert_eq!(cli.tree, Some(PathBuf::from("wave.groot.xml")));
        assert_eq!(cli.runner.module, vec![PathBuf::from("a"), "b".into()]);
    }

    /// Without `--tree` the default behavior runs; `--no-tree` runs none.
    #[test]
    fn the_default_tree_runs_unless_none_is_asked() {
        let default = Cli::try_parse_from(["arora-aldebaran", "tcp://nao.local:9559"]).unwrap();
        assert_eq!(default.tree().unwrap().as_deref(), Some(READY_TREE));
        let none =
            Cli::try_parse_from(["arora-aldebaran", "--no-tree", "tcp://nao.local:9559"]).unwrap();
        assert_eq!(none.tree().unwrap(), None);
    }

    /// `--tree` and `--no-tree` together are refused.
    #[test]
    fn a_tree_and_no_tree_conflict() {
        let error = Cli::try_parse_from([
            "arora-aldebaran",
            "--tree",
            "wave.groot.xml",
            "--no-tree",
            "tcp://nao.local:9559",
        ])
        .expect_err("the two are refused together");
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }

    /// `-h` and `--help` print the help and exit 0; neither is a robot.
    #[test]
    fn help_is_not_a_robot() {
        for flag in ["-h", "--help"] {
            let error = Cli::try_parse_from(["arora-aldebaran", flag]).expect_err("help stops");
            assert_eq!(error.kind(), ErrorKind::DisplayHelp, "{flag}");
            assert_eq!(error.exit_code(), 0, "{flag}");
        }
    }

    /// An option the runner does not take is refused, not read as a robot.
    #[test]
    fn an_unknown_option_is_refused() {
        let error =
            Cli::try_parse_from(["arora-aldebaran", "--modules", "a", "tcp://nao.local:9559"])
                .expect_err("an unknown option is refused");
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    /// The device options give the device's name, directory and bridge port.
    #[test]
    fn the_device_options_give_the_device() {
        let cli = Cli::try_parse_from([
            "arora-aldebaran",
            "--device",
            "nao-2",
            "--device-dir=/srv/nao-2",
            "--bridge-port",
            "9002",
            "tcp://nao.local:9559",
        ])
        .expect("the command line parses");
        let device = cli.runner.device().expect("the device resolves");
        assert_eq!(device.name(), "nao-2");
        assert_eq!(device.dir(), std::path::Path::new("/srv/nao-2"));
        assert_eq!(device.bridge_port(), 9002);
    }

    /// Built with the `studio` feature, a run connects to Semio Studio unless it passes
    /// `--no-studio`, anywhere on the command line; `--studio` is no option.
    #[cfg(feature = "studio")]
    #[test]
    fn a_run_connects_to_studio_unless_it_passes_no_studio() {
        let connects = |args: &[&str]| {
            Cli::try_parse_from(args.iter().copied())
                .expect("the command line parses")
                .studio
                .connects()
        };
        assert!(connects(&["arora-aldebaran", "tcp://nao.local:9559"]));
        assert!(!connects(&[
            "arora-aldebaran",
            "--no-studio",
            "tcp://nao.local:9559"
        ]));
        assert!(!connects(&[
            "arora-aldebaran",
            "tcp://nao.local:9559",
            "--no-studio"
        ]));
        let error = Cli::try_parse_from(["arora-aldebaran", "--studio", "tcp://nao.local:9559"])
            .expect_err("--studio is refused");
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    /// Built without the `studio` feature, the runner takes no Studio option: `--no-studio`
    /// is refused, not ignored.
    #[cfg(not(feature = "studio"))]
    #[test]
    fn no_studio_is_refused_without_the_studio_feature() {
        let error = Cli::try_parse_from(["arora-aldebaran", "--no-studio", "tcp://nao.local:9559"])
            .expect_err("--no-studio is refused");
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }
}
