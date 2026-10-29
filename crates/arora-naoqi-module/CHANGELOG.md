# Changelog

All notable changes to `arora-naoqi-module`. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- The crate: `libarora_naoqi.so`, the library NAOqi loads from its `autoload.ini`. Its DCM
  plugin `AroraDcm` runs the DCM HAL in the loop of the DCM and starts the device on a thread
  of its own, on that HAL: the default behavior of `arora-aldebaran` (or the tree
  `arora_naoqi.json` names), its modules and its bridges, Semio Studio with the `studio`
  feature. The service `Arora` reports it (`status`, `cycles`, `refusedCommands`, `joints`)
  and plays the default behavior again (`ready`). When NAOqi destroys the plugin, the device
  stops and the robot rests.
- `robot/build.sh`: the library for a robot (glibc 2.13, the bridge built with GCC 4.8 against
  the headers of the robot's C++ SDK), its package, and the check of the symbols it needs of
  the robot (`robot/check-symbols.sh`).
- `tests/fake_dcm.rs`: the library in a fake NAOqi (`interop/fake-dcm`), the 64-bit build or,
  with `interop/fake-dcm/build-i686.sh`, the robot's in a 32-bit process.
