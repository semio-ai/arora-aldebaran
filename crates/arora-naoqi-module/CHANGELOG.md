# Changelog

All notable changes to `arora-naoqi-module`. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `robot/build.sh` builds the bridge against `robot/naoqi-2.1/`: what it uses of the C++ SDK of
  NAOqi 2.1, declared with its ABI, and the Boost 1.55 of NAOqi 2.1 (the image's), so that the
  build needs nothing of NAOqi. `robot/check-abi.sh <cross toolchain>` checks them against the
  SDK of `ctc-linux64-atom-2.1.4.13`: the same layouts, and the same code for the bridge built
  against either. Without the cross toolchain, the symbols the library needs of the C++ runtime
  are checked by their versions against those of NAOqi 2.1 (GCC 4.5).
- `NAOQI_SDK=<cross toolchain> robot/build.sh` builds against the robot's cross toolchain
  (`ctc-linux64-atom-2.1.4.13`) instead: the bridge against its NAOqi headers and its Boost, and
  the symbols the library needs are checked against the robot's libraries it holds
  (`check-symbols.sh <library> <-m32> <directories>`). The library builds against that of NAOqi
  2.1.4.13, and needs nothing its libraries (the libstdc++ of GCC 4.5, glibc 2.13) do not
  define.
- The crate: `libarora_naoqi.so`, the library NAOqi loads from its `autoload.ini`. Its DCM
  plugin `AroraDcm` runs the DCM HAL in the loop of the DCM and starts the device on a thread
  of its own, on that HAL: the default behavior of `arora-aldebaran` (or the tree
  `arora_naoqi.json` names), its modules and its bridges, Semio Studio with the `studio`
  feature. The service `Arora` reports it (`status`, `cycles`, `refusedCommands`, `joints`)
  and plays the default behavior again (`ready`). When NAOqi destroys the plugin, the device
  stops and the robot rests.
- `robot/build.sh`: the library for a robot (glibc 2.13, the bridge built with GCC 4.8 against
  the headers of the robot's C++ SDK, the workspace's `robot` profile: optimized for size,
  41 MB for i686), its package, and the check of the symbols it needs of
  the robot (`robot/check-symbols.sh`).
- `tests/fake_dcm.rs`: the library in a fake NAOqi (`interop/fake-dcm`), the 64-bit build or,
  with `interop/fake-dcm/build-i686.sh`, the robot's in a 32-bit process.
