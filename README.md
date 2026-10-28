# arora-aldebaran

Arora support for Aldebaran robots: NAO and Pepper.

| Crate | What it is |
|---|---|
| [`arora-hal-naoqi`](crates/arora-hal-naoqi/readme.md) | The NAOqi HAL: NAO and Pepper robots as Arora devices, over the pure-Rust [`libqi-vibe`](https://crates.io/crates/libqi-vibe), and its `arora-naoqi` runner |

The crates build against the published Arora crates from
[arora-sdk](https://github.com/semio-ai/arora-sdk), on the same pinned nightly
(`rust-toolchain.toml`).

```bash
cargo test                      # the HAL, against a fake NAOqi
cargo test --features runner    # with the arora-naoqi runner
```

To drive a robot, see
[Try it on a NAO](crates/arora-hal-naoqi/readme.md#try-it-on-a-nao) and
[Deploy it on the robot](crates/arora-hal-naoqi/readme.md#deploy-it-on-the-robot).
