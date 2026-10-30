//! The library in a fake NAOqi: `interop/fake-dcm/build.sh` builds the library with its bridge
//! to the DCM, and `fake-dcm`, a process loading it as NAOqi does and running the loop of a
//! DCM with the memory of a NAO; the NAOqi services are `fake-naoqi`'s.
//!
//! `ARORA_FAKE_DCM` and `ARORA_NAOQI_LIBRARY` name another harness and library: the 32-bit
//! harness of `interop/fake-dcm/build-i686.sh` (`fake-dcm-i686.sh`) with the library a robot
//! gets, built against the stub of the SDK of the fake NAOqi:
//! `NAOQI_SDK=interop/fake-dcm/build/sdk crates/arora-naoqi-module/robot/build.sh` (whose check
//! of symbols then fails: the stub's Boost.Signals2 needs a libstdc++ newer than NAOqi 2.1's).
//!
//! Skipped when they are not built, unless `ARORA_INTEROP_REQUIRE=1`.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qi::ObjectExt;

fn built() -> Option<(PathBuf, PathBuf)> {
    let build = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../interop/fake-dcm/build");
    let from_env = |var: &str| std::env::var_os(var).map(PathBuf::from);
    let harness = from_env("ARORA_FAKE_DCM").unwrap_or_else(|| build.join("fake-dcm"));
    let library =
        from_env("ARORA_NAOQI_LIBRARY").unwrap_or_else(|| build.join("libarora_naoqi.so"));
    if harness.is_file() && library.is_file() {
        return Some((harness, library));
    }
    assert!(
        std::env::var("ARORA_INTEROP_REQUIRE").as_deref() != Ok("1"),
        "interop/fake-dcm/build.sh was not run"
    );
    eprintln!("skipped: run interop/fake-dcm/build.sh first");
    None
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test(flavor = "multi_thread")]
async fn naoqi_runs_the_device_on_the_dcm() {
    let Some((harness, library)) = built() else {
        return;
    };
    let naoqi = fake_naoqi::FakeNaoqi::start().await;
    let dir = std::env::temp_dir().join(format!("arora-naoqi-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("arora_naoqi.json");
    std::fs::write(
        &config,
        serde_json::json!({
            "robot": { "url": naoqi.url },
            "device_dir": dir.join("device"),
            "bridge_port": free_port(),
            "studio": false,
        })
        .to_string(),
    )
    .unwrap();

    let run = {
        let url = naoqi.url.clone();
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || {
            Command::new(harness)
                .arg(library)
                .arg("8")
                .env("ARORA_NAOQI_CONFIG", &config)
                .env("QI_URL", url)
                .env("HOME", dir)
                .env("RUST_LOG", "info")
                .output()
                .expect("fake-dcm runs")
        })
    };

    // While NAOqi runs it, the service reports the device.
    let client = qi::node::init()
        .connect_to_space_at(qi::Address::resolve_all(&naoqi.url).unwrap(), None)
        .start()
        .await
        .unwrap();
    let mut status = String::new();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(arora) = client.service("Arora").await {
            status = arora.call("status", ()).await.unwrap();
            if status == "running" {
                let cycles: u64 = arora.call("cycles", ()).await.unwrap();
                assert!(cycles > 0);
                let joints: Vec<String> = arora.call("joints", ()).await.unwrap();
                assert!(joints.contains(&"l_wrist_yaw".to_string()), "{joints:?}");
                break;
            }
        }
    }
    assert_eq!(status, "running");

    let output = run.await.unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("modules: AroraDcm DCM"), "{stdout}");
    // The default behavior: the wrists and hands stiffened (at 0.6), moved, and relaxed once
    // over, and the robot said it is ready.
    let alias = |name: &str| {
        stdout
            .lines()
            .find(|line| line.starts_with(&format!("alias AroraDcm/{name} ")))
            .unwrap_or_else(|| panic!("no command for {name}:\n{stdout}\n{stderr}"))
            .to_string()
    };
    assert!(
        alias("LWristYaw/Stiffness").ends_with(" 2 [[0]]"),
        "{stdout}"
    );
    let moves: usize = alias("LHand/Position")
        .split(' ')
        .nth(2)
        .unwrap()
        .parse()
        .unwrap();
    assert!(moves > 10, "{stdout}");
    assert!(
        naoqi
            .said
            .lock()
            .unwrap()
            .iter()
            .any(|text| text == "Arora is ready"),
        "{stderr}"
    );
    // The robot rested when the HAL started and when NAOqi stopped the device.
    assert!(stdout.contains("destroyed\n_closeModule 0"), "{stdout}");
    assert_eq!(naoqi.rested.load(Ordering::SeqCst), 2, "{stderr}");
    assert_eq!(*naoqi.life_state.lock().unwrap(), "disabled");
    std::fs::remove_dir_all(dir).ok();
}
