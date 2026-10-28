# arora-aldebaran

Arora support for Aldebaran robots: NAO and Pepper run as Arora devices, over their NAOqi
middleware.

The [NAOqi HAL](crates/arora-hal-naoqi/readme.md) (`arora-hal-naoqi`) talks to the robot
with [`libqi-vibe`](https://crates.io/crates/libqi-vibe), a Rust implementation of the `qi`
framework: no C++ SDK. Its `arora-naoqi` runner works two ways:

- **From your computer**, the quickest way to try it: it builds like any Rust program and
  drives the robot over the network, through NAOqi's port 9559.
  [Try it on a NAO](#try-it-on-a-nao).
- **On the robot itself**, where the device runs on its own, with no computer attached.
  [Deploy it on the robot](#deploy-it-on-the-robot).

The second way is the remarkable one. Arora is written in Rust, so the whole device (the
runtime, its behavior trees, its WebAssembly modules, its Semio Studio connection)
cross-compiles to a single static binary for the NAO's own computer, a 32-bit Intel Atom,
with nothing to install on the robot. That is rare in the robotics ecosystem: today's
stacks, ROS 2 and the Python AI tooling among them, seldom run on the NAO's on-board
computer, and stay on a separate machine that drives the robot over the network.

- [Try it on a NAO](#try-it-on-a-nao)
- [Deploy it on the robot](#deploy-it-on-the-robot)
- [Semio Studio](#semio-studio)
- [The crates](#the-crates)

## Try it on a NAO

What you get: the robot says "Arora is ready" while its wrists rotate slightly and its hands
open and close; the terminal UI plays it again when you press `r`; built with Semio Studio,
the device appears in your Studio account. `arora-naoqi` runs on your computer and drives the robot over
the network: nothing to cross-compile, nothing to install on the robot.

### 1. Prepare the robot

1. Connect the NAO to your network. Its host is `<robot_name>.local`, where `<robot_name>` is
   the name it says when you press its chest button once. From your computer,
   `nc -z <robot_name>.local 9559` checks that its NAOqi answers.
2. Sit the robot down, or let it crouch in its rest posture: the default behavior stiffens
   only the wrists and hands, nothing that holds the robot up.

There is no need to turn Autonomous Life off: the HAL does it when it connects, so that the
robot's own life does not move the joints Arora drives. Turning it off can send the robot
to its rest posture, one more reason to sit it down first. To keep Autonomous Life on, set
`"disable_autonomous_life": false` in the configuration (see [Use it](#3-use-it)).

### 2. Run it from your computer

You need Rust, installed with [rustup](https://rustup.rs): the repository pins its nightly
toolchain (`rust-toolchain.toml`), which rustup fetches on the first build. Then, from the
repository root, on macOS or Linux:

```sh
cargo run --release -p arora-hal-naoqi --features runner --bin arora-naoqi -- tcp://<robot_name>.local:9559
```

The first build takes a while (about ten minutes); the next runs start at once. The device
serves the open local bridge; to connect it to Semio Studio, see
[Semio Studio](#semio-studio), which also needs Internet access.

A robot that requires NAOqi authentication takes its credentials from a JSON file given
after the address: `{"user": "nao", "password": "<password>"}`.

### 3. Use it

The device starts, the robot says "Arora is ready" with its hands, and:

| Key | Does |
|---|---|
| `r` | plays the default behavior again |
| `PgUp` / `PgDn` / `End`, mouse wheel | scroll the logs, follow them again |
| `q`, `Ctrl-C` | stop the device; the robot goes to rest (it sits or crouches, then relaxes) |

More logs: set `RUST_LOG=debug` before the command. Without a terminal (`nohup`, a
service), the runner is headless and asks nothing.

A run that connects to Studio ([Semio Studio](#semio-studio)) enrolls a device that is not
enrolled: on the terminal UI it asks for a device name, unless `DEVICE_NAME` gives one, then
shows the code a person approves the device with in Studio.

Options: `--tree <file.groot.xml>` runs another behavior tree, `--no-tree` none; in a build
with the `studio` feature, `--no-studio` serves the open local bridge instead of connecting
to Semio Studio; `--device <name>`, `--device-dir <path>` and `--bridge-port <port>`
(`ARORA_DEVICE`, `ARORA_DEVICE_DIR`, `ARORA_BRIDGE_PORT`) name the device, give its
directory, and the port of its open local bridge (9000 by default), so several devices run
on one machine; a JSON configuration ([`configs/nao.json`](crates/arora-hal-naoqi/configs/nao.json): joint
ids, sampling period, joint speed, sensor families, Autonomous Life, rest on exit) can
replace the address, and a second JSON file overrides it.

Modules: the device loads every module directory in its device directory's `modules/`,
then each `--module <dir>` (repeatable), as the `arora` runner does: a module's
`header.json` beside its artifact, and optionally its records in `records/`, which describe
every function it exports. The layout is in
[arora's readme](https://github.com/semio-ai/arora-sdk/blob/main/crates/arora/readme.md#where-a-device-keeps-its-data). A tree's leaves call
the functions of these modules as they call `Say` and `HandsReady`. A module directory that
does not read fails the start, naming the module, before the robot is reached; a module the
engine rejects, or whose record does not describe it, fails once the robot is connected.
`--help` lists the options.

## Deploy it on the robot

Once it works from your computer, deploy it: the same device, cross-compiled, runs on the
NAO itself, without your computer. The robot needs SSH access (`ssh nao@<robot_name>.local`, the
password is the robot's, `nao` unless it was changed) and, for Semio Studio, Internet
access and the right date (TLS checks certificates against it: `date` on the robot should
print the current time).

### 1. Cross-compile `arora-naoqi` for the NAO

First you need to determine the platform of your NAO.
On a NAO v5, which has a 32-bit Atom computer, the target is `i686-unknown-linux-musl`.
We use `musl` toolchain to make a static binary that needs nothing from the robot's system,
which is pretty outdated.

On a Mac, from the repository root:

```sh
# Once: the cross C toolchain (for the crates with C code: ring, aws-lc-sys, zstd-sys),
# CMake (aws-lc-sys builds with it), and the Rust target for the pinned toolchain.
brew install messense/macos-cross-toolchains/i686-unknown-linux-musl cmake
rustup target add i686-unknown-linux-musl

# The build. The linker comes from .cargo/config.toml; the C compiler from these variables.
CC_i686_unknown_linux_musl=i686-unknown-linux-musl-gcc \
CXX_i686_unknown_linux_musl=i686-unknown-linux-musl-g++ \
AR_i686_unknown_linux_musl=i686-unknown-linux-musl-ar \
cargo build --release -p arora-hal-naoqi --features runner --bin arora-naoqi \
  --target i686-unknown-linux-musl

# Strip the debug information the release profile keeps: over 300 MB down to about 45 MB.
i686-unknown-linux-musl-strip target/i686-unknown-linux-musl/release/arora-naoqi
```

The binary is `target/i686-unknown-linux-musl/release/arora-naoqi`: a statically linked
32-bit ELF. The first build takes a while (half an hour on an M1, mostly `aws-lc-sys` and
`wasmtime`).

On an Intel Mac, point
`CARGO_TARGET_I686_UNKNOWN_LINUX_MUSL_LINKER` at `/usr/local/bin/i686-unknown-linux-musl-gcc`
(`.cargo/config.toml` names the Apple Silicon path). On Linux, any
`i686-linux-musl` cross toolchain does, with the same variables pointing at it.

### 2. Copy it to the robot

```sh
scp target/i686-unknown-linux-musl/release/arora-naoqi nao@<robot_name>.local:~/
```

### 3. Run it on the robot

```sh
ssh -t nao@<robot_name>.local   # -t: the terminal UI needs a terminal
./arora-naoqi tcp://localhost:9559
```

The device then behaves as [from your computer](#3-use-it). To enroll it with Semio
Studio, build it with `--features runner,studio`, run it once and approve the code it shows
in Studio. To leave it running after you log out, run it headless with `nohup`: an enrolled
device connects to Studio with its saved credential.

On the robot, the device directory follows the same rule as anywhere: `--device-dir` (or
`ARORA_DEVICE_DIR`) when given, else `semio/arora/devices/<name>/` in the user data
directory (`~/.local/share` on the robot), the name being `--device` (or `ARORA_DEVICE`),
`default` when unset; [arora's readme](https://github.com/semio-ai/arora-sdk/blob/main/crates/arora/readme.md#where-a-device-keeps-its-data) has the whole
rule. Its modules go in `modules/` there. The binary is static, so it loads WebAssembly
modules: a native module (a dynamic library) fails the start, naming it.

## Semio Studio

Semio Studio is opt-in, at build time. A runner built with the Studio connection
(`arora-hal-naoqi`'s `studio` feature: `--features runner,studio`) connects to Studio on
every run, unless the run passes `--no-studio`, which serves the open local bridge instead.
A runner built without it serves the open local bridge and takes no Studio option. Delete
the device's directory in the device store (below) to make it a device with no saved
credential.

A device enrolls with Studio the first time it connects: a person approves it in Studio and
chooses its owner, a person or an organization, and Studio creates the device.

1. **Start the device.** It shows a code: in the terminal UI's prompt line,
   and in the log; headless, on standard error too when the log shows no info lines (the
   default without `RUST_LOG`).

   ```sh
   cargo run --release -p arora-hal-naoqi --features runner,studio --bin arora-naoqi -- tcp://<robot_name>.local:9559
   ```

   Under the terminal UI the runner first asks for a device name (`arora-device-<id>` when
   left empty); `DEVICE_NAME` skips the question, and headless nobody is asked. The model
   family (`nao` or `pepper`) and versions are those the robot reports, unless
   `MODEL_FAMILY`, `HARDWARE_VERSION` or `SOFTWARE_VERSION` set them.
2. **Enter the code in Studio**, signed in, and choose the device's owner. When it knows
   Studio's web address (`STUDIO_URL`, else the one its Studio client was published with),
   the runner also shows the page that approves the code. A code that expires undecided is
   replaced by another.
3. The logs say `Studio: enrolled as device <id>`, then that the Zenoh session is open. The
   robot is now in its owner's device list in Studio, where you bind it to a robot model and
   drive it.

A robot with nobody at a terminal enrolls with an enrollment token instead: create one in
Studio, and start the runner with it in `STUDIO_ENROLLMENT_TOKEN` (an environment variable,
never an option: the token is a secret). Studio creates the device under the token's owner.
A refused code or token fails the run.

The device's identity (its Studio credential) is kept on the machine that runs
`arora-naoqi`, under its name in Semio's device store: `semio/devices/<name>/` in the user
data directory — `~/.local/share` on Linux and on the robot, `~/Library/Application Support`
on macOS — the name being `default` unless `--device` (or `ARORA_DEVICE`) gives one. The
next runs connect with it without enrolling again; a device run from your computer, then
deployed on the robot, enrolls twice. To enroll the robot afresh, delete that directory and
run it again, or give the run another identity with `--device <name>`. A run given a device
directory (`--device-dir`) that connects to Studio names the device too. A credential an
earlier `arora-naoqi` saved, before devices enrolled, is no enrolled device's: the next run
enrolls the device.

## The crates

| Crate | What it is |
|---|---|
| [`arora-hal-naoqi`](crates/arora-hal-naoqi/readme.md) | The NAOqi HAL: NAO and Pepper robots as Arora devices, its keys and behavior leaves, and the `arora-naoqi` runner |

They build against the published Arora crates from
[arora-sdk](https://github.com/semio-ai/arora-sdk), on the same pinned nightly
(`rust-toolchain.toml`).

```bash
cargo test                            # the HAL, against a fake NAOqi
cargo test --features runner          # with the arora-naoqi runner
cargo test --features runner,studio   # with the runner's Semio Studio connection
```
