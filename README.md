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
open and close; the terminal UI plays it again when you press `r`; with `--studio`, the
device appears in your Semio Studio account. `arora-naoqi` runs on your computer and drives the robot over
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

A run that connects to Studio ([Semio Studio](#semio-studio)) first asks, on the terminal
UI, what neither the Studio variables nor an earlier answer give: your Studio owner UID, a
device name, a model family (`nao`). Leave the owner empty to run without Studio after all.

Options: `--tree <file.groot.xml>` runs another behavior tree, `--no-tree` none; `--studio`
connects to Semio Studio, `--no-studio` keeps a registered device on the local bridge; a
JSON configuration ([`configs/nao.json`](crates/arora-hal-naoqi/configs/nao.json): joint
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

The device then behaves as [from your computer](#3-use-it). To register it with Semio
Studio, run it once with `--studio` and answer the prompts. To leave it running after you
log out, run it headless with `nohup`: a registered device connects to Studio with its saved
credential.

On the robot, the device directory follows the same rule as anywhere: `DEVICE_DIR` when it
is set, else `semio/arora/devices/<DEVICE_LOCAL_ID>/` in the user data directory
(`~/.local/share` on the robot), with `default` as the local id when `DEVICE_LOCAL_ID` is
unset; [arora's readme](https://github.com/semio-ai/arora-sdk/blob/main/crates/arora/readme.md#where-a-device-keeps-its-data) has the whole
rule. Its modules go in `modules/` there. The binary is static, so it loads WebAssembly
modules: a native module (a dynamic library) fails the start, naming it.

## Semio Studio

Semio Studio is opt-in. The runner is built with the Studio connection (`arora-hal-naoqi`'s
`studio` feature, which `runner` enables), and a device connects to Studio only when it
holds a saved Studio credential, or when the run passes `--studio` (or sets
`ARORA_STUDIO=1`). Otherwise it serves the open local bridge, and says so:
`Studio: no saved credential in <path>; serving the local bridge. Pass --studio to register this device with Semio Studio.`
`--no-studio` keeps a device that holds a credential on the local bridge. A device may hold
one without having asked: a headless run of an earlier `arora-naoqi` connected on its own.
Delete the device's `studio/` directory (below) to make it a device with none.

There is no pairing code yet: a device declares its owners itself, by their Studio user ids
(UIDs).

1. **Find your Studio UID.** Studio does not display it yet. Sign in to Studio in a
   browser, open the developer tools, then Application (Storage in Firefox) → IndexedDB →
   `firebaseLocalStorageDb` → `firebaseLocalStorage`: the entry's `value.uid` is your UID.
2. **Start the device with `--studio`** and answer the prompts: the owner UID (several,
   comma-separated, for a shared robot), a device name (`arora-device-<id>` when left empty),
   the model family `nao`. Or set `DEVICE_OWNERS`, `DEVICE_NAME`, `MODEL_FAMILY`: a variable
   that is set skips its question. Headless, nobody is asked, and the device registers what
   the variables give.

   ```sh
   cargo run --release -p arora-hal-naoqi --features runner,studio --bin arora-naoqi -- --studio tcp://<robot_name>.local:9559
   ```
3. The logs say `Registered device <id> with Studio`, then that the Zenoh session is open.
   The robot is now in your device list in Studio, where you bind it to a robot model and
   drive it.

The device's identity (its Studio credentials) is kept on the machine that runs
`arora-naoqi`, under `semio/arora/devices/default/studio/` in its user data directory:
`~/.local/share` on Linux and on the robot, `~/Library/Application Support` on macOS. The
next runs connect with it, without `--studio` and without asking again; a device run from
your computer, then deployed on the robot, registers twice. To register the robot afresh,
delete that directory and run with `--studio`, or give the run another identity with
`DEVICE_LOCAL_ID=<name>` (or a whole other directory with `DEVICE_DIR`).

The next major of `arora-hal-naoqi` stops enabling `studio` from `runner`, and a runner then
connects to Studio only with `--studio`: build it with `--features runner,studio`, as above,
and pass `--studio` on every run that should connect.

## The crates

| Crate | What it is |
|---|---|
| [`arora-hal-naoqi`](crates/arora-hal-naoqi/readme.md) | The NAOqi HAL: NAO and Pepper robots as Arora devices, its keys and behavior leaves, and the `arora-naoqi` runner |

They build against the published Arora crates from
[arora-sdk](https://github.com/semio-ai/arora-sdk), on the same pinned nightly
(`rust-toolchain.toml`). A daily CI job also builds and tests them against arora-sdk's
`main`, so a change there that breaks them shows before it is released; locally:

```bash
.github/scripts/patch-arora-sdk.sh ../arora-sdk    # appends a [patch.crates-io]
CARGO_UNSTABLE_BINDEPS=true cargo test --features runner
```

```bash
cargo test                      # the HAL, against a fake NAOqi
cargo test --features runner    # with the arora-naoqi runner
```
