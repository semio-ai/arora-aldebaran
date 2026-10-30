# Test plan: the Arora device inside NAOqi, on a NAO

The steps go from the least to the most intrusive: NAOqi loads the library only from step 4,
and the joints move only from step 5. Each step says what to check and what to send back when
it fails. Commands marked *(to confirm)* are assumptions about the robot's system that this
plan could not check.

Conventions: `NAO=<address of the robot>` on the computer. The library was so far run in a
fake NAOqi only, on a stub of the NAOqi C++ SDK ([Its status](../../README.md#its-status)): this
plan is its first run on a robot.

It continues the plan of libqi-rs-vibe's modules (`docs/robot-test-plan.md` there): if the
`rustexample` library of that plan already loaded in NAOqi (its step 5) and ran its DCM plugin
(its step 6), steps 1 to 3 here are likely to pass.

## 0. Before starting: what the robot is

On the robot (`ssh nao@$NAO`):

```sh
uname -m; uname -r                       # i686 on a NAO V4/V5; the kernel
/lib/libc.so.6 | head -1                 # the glibc: 2.12 or later
ls /usr/lib/libstdc++.so.6* /usr/lib/libboost_system* /usr/lib/libalcommon.so   # (to confirm the paths)
cat /etc/naoqi-version 2>/dev/null
```

Send back the output. The build checks the library against the libraries of the cross
toolchain of NAOqi 2.1.4.13 (glibc 2.13, the libstdc++ of GCC 4.5, Boost 1.55): a robot on
another NAOqi needs a build of its own.

## 1. Build

On the computer:

```sh
crates/arora-naoqi-module/robot/build.sh
```

- [ ] It builds, and ends with `newest glibc symbol version: GLIBC_2.12` and `every other symbol
      it needs is in the libstdc++ or libgcc_s of NAOqi 2.1 (GCC 4.5)`.

With the cross toolchain of the robot's NAOqi (`ctc-linux64-atom-2.1.4.13` for NAOqi 2.1;
where to get it is in [Build it](../../README.md#build-it)), the build can be checked against
it:

```sh
crates/arora-naoqi-module/robot/check-abi.sh <path to ctc-linux64-atom-2.1.4.13>
NAOQI_SDK=<path to ctc-linux64-atom-2.1.4.13> crates/arora-naoqi-module/robot/build.sh
```

- [ ] `check-abi.sh` ends with `the bridge has the same ... functions, vtables and typeinfos
      with ours`, in 32 and 64 bits.
- [ ] The build ends with `every other symbol it needs is in the robot's libraries`.

Both passed on 2026-10-30 with `ctc-linux64-atom-2.1.4.13.zip` (SHA-256 `6b72eb98…`): the
bridge compiles against the real headers of NAOqi 2.1 and its Boost 1.55, to the same code as
against `robot/naoqi-2.1/`, and the robot's libraries (NAOqi's, the libstdc++ of GCC 4.5, glibc
2.13) define every symbol the library needs. If a step fails for you, send the compiler errors,
the differences or the missing symbols.

## 2. On the robot, outside NAOqi: the library loads

Copy the package, then load it with the loader of libqi-rs-vibe's plan (its step 4,
`loader32`), which calls `_createModule` with no broker:

```sh
scp -r dist/arora-naoqi-i686 nao@$NAO:
ssh nao@$NAO
LD_PRELOAD="/usr/lib/libalcommon.so /usr/lib/libalproxies.so" ./loader32 arora-naoqi-i686/lib/libarora_naoqi.so 10
```

The preloads stand in for NAOqi, which has its libraries loaded *(to confirm the names)*.

- [ ] `_createModule: 0`: the library loads (no `undefined symbol`, no `GLIBC_... not found`).
      The DCM plugin is not created (no broker): the log says so; it is expected.
- [ ] From the computer, within the 10 seconds: `qicli --qi-url tcp://$NAO:9559 call Arora.status`
      answers `waiting for the DCM`.

On failure: the full output (an `undefined symbol` names what the robot lacks).

## 3. NAOqi loads it, Arora does not move the robot

The configuration: `tree` set to `none` (no behavior), so the device runs and reads the robot
without commanding it. Note that the HAL still puts the robot to rest at its start: sit the
robot down first.

```sh
ssh nao@$NAO
echo '{"tree": "none", "studio": false}' > arora-naoqi-i686/arora_naoqi.json
cp naoqi/preferences/autoload.ini naoqi/preferences/autoload.ini.before-arora
printf '[user]\n/home/nao/arora-naoqi-i686/lib/libarora_naoqi.so\n' >> naoqi/preferences/autoload.ini
nao restart
```

If `autoload.ini` already has a `[user]` section, add the line to it instead. To undo:
`cp naoqi/preferences/autoload.ini.before-arora naoqi/preferences/autoload.ini && nao restart`.

- [ ] NAOqi starts as usual.
- [ ] `qicli call Arora.status` says `running` (or `starting` for a few seconds).
- [ ] `qicli call Arora.cycles` twice, 10 s apart: about 1000 more (one per 10 ms).
- [ ] `qicli call Arora.joints` lists 26 joints, `head_yaw` to `r_hand`, and
      `qicli call Arora.refusedCommands` says 0.
- [ ] `qicli info AroraDcm`: the ALModule of the plugin exists.
- [ ] The device's local bridge answers: it serves on the robot's loopback
      (`ws://127.0.0.1:9000`), so from the computer through `ssh -L 9000:127.0.0.1:9000
      nao@$NAO`, with any client of Arora's local bridge: the keys `head_yaw.position`,
      `battery.charge`, `touch.head.front` are there and change (turn the head by hand, touch
      it).
- [ ] `top`: the CPU of NAOqi, compared with the robot before step 3 (without the library);
      send both.
- [ ] The logs of NAOqi at start *(to confirm where: the output of `nao restart`,
      `/var/log/naoqi/`)*: `DCM plugin AroraDcm: 26 joints (25 commandable)...` (NAO's second hip yaw-pitch has no
      actuator of its own), `DCM HAL
      started: nao robot...`, no error.
- [ ] `nao restart`: NAOqi stops in its usual time. The device stops with the plugin and puts
      the robot to rest, waiting up to 20 s for `ALMotion.rest`: if NAOqi takes that much
      longer to stop, `ALMotion` no longer answers by then; say so, and try with
      `"robot": {"rest_on_exit": false}`.

On failure: the logs of NAOqi at start, and the output of `qicli call Arora.status`.

## 4. The default behavior, through the DCM

The robot sitting, nothing in reach of its arms. `"tree": "ready"` (or remove the
configuration), then `nao restart`.

- [ ] At start, the robot says "Arora is ready" as its wrists turn and its hands open and close
      three times, over about 4 seconds, smoothly (no jump).
- [ ] `qicli call Arora.ready`: it plays again.
- [ ] The wrists and hands are relaxed after the behavior (they are stiffened only during it).
- [ ] Touch the front of the head with `"tree": "touch"`: "Hello, you touched my head".
- [ ] `qicli call Arora.refusedCommands`: 0.
- [ ] The logs of NAOqi: no warning of the DCM being late *(to confirm the wording)*.
- [ ] `nao restart` twice: the robot rests, NAOqi stops cleanly.

If a motion is jerky or too fast, send the value of `robot.joint_speed_limit` used and a
video if possible: the speed limit replaces `ALMotion`'s interpolation.

## 5. LEDs (optional)

With a tree, or from a bridge client, write `led.ChestLeds.color` = `0xFF0000`, then
`led.FaceLeds.color` = `0x0000FF` and `led.FaceLeds.intensity` = `0.3`.

- [ ] The chest turns red, the eyes dim blue.

## What to send back

For each step: passed, or the failing check with its output. With step 0, that is enough to
fix what differs from the fake NAOqi.
