//! The machine the HAL runs on, sampled at a low rate (1 Hz, [`PERIOD`]) on a task of the HAL's runtime: the
//! robot's CPU load, temperatures, battery, memory and RAM tmpfs, and the CPU per thread and per process, published as
//! HAL readings like every other key, so that a recording carries its own cost.
//!
//! The ALMemory floats come through `ALMemory.getData`. The machine's own keys (`/proc`, `/sys`) are those of the
//! machine the HAL runs on: published only when that is the robot ([`spawn`]'s `local`: the NAOqi the HAL reaches is
//! on this machine, or the HAL runs inside NAOqi), never a desk's numbers under a robot's names. The same keys and names
//! as deploy/nao-dcm-device's DCM HAL (github.com/victorpaleologue/nao-rl).
//!
//! Keys (the same names in every NAO HAL that publishes them: this one, arora-aldebaran's), under `sys/` so a bridge
//! client subscribes to all of them with the one prefix `sys`:
//!
//! | key | value |
//! |---|---|
//! | `sys/load` | f32[3]: load average 1 / 5 / 15 min (`/proc/loadavg`) |
//! | `sys/cpu/busy` | f32[n]: each hardware thread's busy share over the interval (`/proc/stat`) |
//! | `sys/cpu/temperature` | f64 °C: the CPU's own sensor (`/sys/class/thermal`, `coretemp`); NaN when none |
//! | `sys/head/temperature` | f64 °C: the head board's (ALMemory `Head/Temperature/Sensor/Value`) |
//! | `sys/battery/charge` / `current` / `temperature` | f64: 0..1, A (negative: discharging), °C (ALMemory) |
//! | `sys/mem/available_mb` | f64: MemAvailable, else MemFree + Buffers + Cached (2.6 kernels) |
//! | `sys/tmpfs/used_mb` / `sys/tmpfs/size_mb` | f64: the RAM tmpfs (`/var/volatile`, else `/tmp`) |
//! | `sys/threads/names` / `sys/threads/cpu` | str[] / f32[]: this process's threads by name, share of a core |
//! | `sys/procs/names` / `sys/procs/cpu` | str[] / f32[]: other processes by name above 0.5 % (sshd, python ...) |
//! | `sys/pid` | i64: this process (NAOqi's pid: a change means NAOqi restarted) |
//! | `sys/dcm/share_9_11` / `over_20` / `max_ms` | the DCM period over the interval (a HAL that measures it) |

use std::collections::BTreeMap;
use std::time::Instant;

use std::time::Duration;

use arora_types::data::{Key, StateChange};
use arora_types::value::Value;
use qi::value::{Dynamic, Value as QiValue};
use qi::{AnyObject, ObjectExt};

use crate::conversions::qi_f64;

/// How often the keys are sampled.
pub const PERIOD: Duration = Duration::from_secs(1);

/// ALMemory floats the sampler reads (absent ones are NaN).
pub const MEMORY_KEYS: [(&str, &str); 4] = [
    ("sys/head/temperature", "Device/SubDeviceList/Head/Temperature/Sensor/Value"),
    ("sys/battery/charge", "Device/SubDeviceList/Battery/Charge/Sensor/Value"),
    ("sys/battery/current", "Device/SubDeviceList/Battery/Current/Sensor/Value"),
    ("sys/battery/temperature", "Device/SubDeviceList/Battery/Temperature/Sensor/Value"),
];

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// (utime + stime) ticks and the comm of a `/proc/.../stat` line.
fn stat_ticks(text: &str) -> Option<(String, u64)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let name = text[open + 1..close].to_string();
    let f: Vec<&str> = text[close + 2..].split_whitespace().collect();
    Some((name, f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?))
}

fn cpu_lines() -> Vec<(u64, u64)> {
    // per hardware thread: (busy, total) jiffies
    read("/proc/stat")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(|c| c.is_ascii_digit()))
        .map(|l| {
            let v: Vec<u64> = l.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
            let total: u64 = v.iter().sum();
            let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
            (total - idle, total)
        })
        .collect()
}

fn mem_available_mb() -> f64 {
    let t = read("/proc/meminfo").unwrap_or_default();
    let kb = |name: &str| -> Option<f64> {
        t.lines().find(|l| l.starts_with(name)).and_then(|l| l.split_whitespace().nth(1)).and_then(|x| x.parse().ok())
    };
    kb("MemAvailable:")
        .or_else(|| Some(kb("MemFree:")? + kb("Buffers:").unwrap_or(0.0) + kb("Cached:").unwrap_or(0.0)))
        .map_or(f64::NAN, |k| k / 1024.0)
}

fn tmpfs_mb() -> (f64, f64) {
    for p in ["/var/volatile", "/tmp"] {
        let Ok(c) = std::ffi::CString::new(p) else { continue };
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: a NUL-terminated path and a zeroed struct statvfs fills.
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0 {
            let bs = s.f_frsize as f64;
            let size = s.f_blocks as f64 * bs / 1048576.0;
            let used = (s.f_blocks - s.f_bfree) as f64 * bs / 1048576.0;
            return (used, size);
        }
    }
    (f64::NAN, f64::NAN)
}

fn cpu_temperature() -> f64 {
    let mut best = f64::NAN;
    let mut consider = |path: std::path::PathBuf| {
        if let Some(v) = read(&path.to_string_lossy()).and_then(|t| t.trim().parse::<f64>().ok()) {
            let c = if v > 1000.0 { v / 1000.0 } else { v };
            if best.is_nan() || c > best {
                best = c;
            }
        }
    };
    if let Ok(d) = std::fs::read_dir("/sys/class/thermal") {
        for e in d.flatten() {
            consider(e.path().join("temp"));
        }
    }
    for base in ["/sys/devices/platform/coretemp.0", "/sys/devices/platform/coretemp.0/hwmon/hwmon0", "/sys/class/hwmon/hwmon0"] {
        if let Ok(d) = std::fs::read_dir(base) {
            for e in d.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.starts_with("temp") && n.ends_with("_input") {
                    consider(e.path());
                }
            }
        }
    }
    best
}

/// Ticks per process name over every `/proc/<pid>` but this one.
fn procs() -> BTreeMap<String, u64> {
    let me = std::process::id().to_string();
    let mut m = BTreeMap::new();
    if let Ok(d) = std::fs::read_dir("/proc") {
        for e in d.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if !n.bytes().all(|b| b.is_ascii_digit()) || n == me {
                continue;
            }
            if let Some((name, t)) = read(&format!("/proc/{n}/stat")).as_deref().and_then(stat_ticks) {
                *m.entry(name).or_insert(0) += t;
            }
        }
    }
    m
}

fn threads() -> BTreeMap<String, u64> {
    let mut m = BTreeMap::new();
    if let Ok(d) = std::fs::read_dir("/proc/self/task") {
        for e in d.flatten() {
            if let Some((name, t)) = read(&e.path().join("stat").to_string_lossy()).as_deref().and_then(stat_ticks) {
                *m.entry(name).or_insert(0) += t;
            }
        }
    }
    m
}

/// The DCM period counters the plugin keeps (cumulative): steps, of which 9-11 ms, over 20 ms; max [us].
pub type PeriodCounters = (u64, u64, u64, u32);

/// The sampler's memory between samples.
pub struct Sampler {
    last: Option<(Instant, Vec<(u64, u64)>, BTreeMap<String, u64>, BTreeMap<String, u64>, Option<PeriodCounters>)>,
    ticks_per_s: f64,
}

impl Default for Sampler {
    fn default() -> Self {
        // SAFETY: sysconf has no preconditions.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        Sampler { last: None, ticks_per_s: if hz > 0 { hz as f64 } else { 100.0 } }
    }
}

impl Sampler {
    /// One sample: the change to publish (the first call primes the deltas and publishes the instantaneous keys).
    /// `memory`: the ALMemory floats of [`MEMORY_KEYS`] (NaN: absent); `period`: a DCM plugin's counters now, if the
    /// HAL measures the DCM's period; `local`: the machine's own keys (this machine is the robot).
    pub fn sample(&mut self, memory: &[f64], period: Option<PeriodCounters>, local: bool) -> StateChange {
        let now = Instant::now();
        let mut c = StateChange::new();
        let mut put = |k: &str, v: Value| {
            c.set.insert(Key::from(k), Some(v));
        };
        for (i, (k, _)) in MEMORY_KEYS.iter().enumerate() {
            put(k, Value::F64(memory.get(i).copied().unwrap_or(f64::NAN)));
        }
        if !local {
            return c;
        }
        let load: Vec<f32> = read("/proc/loadavg")
            .unwrap_or_default()
            .split_whitespace()
            .take(3)
            .filter_map(|x| x.parse().ok())
            .collect();
        put("sys/load", Value::ArrayF32(load));
        put("sys/cpu/temperature", Value::F64(cpu_temperature()));
        put("sys/mem/available_mb", Value::F64(mem_available_mb()));
        let (used, size) = tmpfs_mb();
        put("sys/tmpfs/used_mb", Value::F64(used));
        put("sys/tmpfs/size_mb", Value::F64(size));
        put("sys/pid", Value::I64(std::process::id() as i64));
        let (cpu, th, pr) = (cpu_lines(), threads(), procs());
        if let Some((t0, cpu0, th0, pr0, p0)) = &self.last {
            let dt = now.duration_since(*t0).as_secs_f64().max(1e-3);
            let busy: Vec<f32> = cpu
                .iter()
                .zip(cpu0)
                .map(|(a, b)| {
                    let total = a.1.saturating_sub(b.1).max(1) as f32;
                    a.0.saturating_sub(b.0) as f32 / total
                })
                .collect();
            put("sys/cpu/busy", Value::ArrayF32(busy));
            let share = |m: &BTreeMap<String, u64>, m0: &BTreeMap<String, u64>, min: f32| -> (Vec<String>, Vec<f32>) {
                let mut v: Vec<(String, f32)> = m
                    .iter()
                    .map(|(n, t)| (n.clone(), (t.saturating_sub(*m0.get(n).unwrap_or(t))) as f32 / (self.ticks_per_s * dt) as f32))
                    .filter(|(_, s)| *s >= min)
                    .collect();
                v.sort_by(|a, b| b.1.total_cmp(&a.1));
                v.into_iter().unzip()
            };
            let (tn, tc) = share(&th, th0, 0.0);
            put("sys/threads/names", Value::ArrayString(tn));
            put("sys/threads/cpu", Value::ArrayF32(tc));
            let (pn, pc) = share(&pr, pr0, 0.005);
            put("sys/procs/names", Value::ArrayString(pn));
            put("sys/procs/cpu", Value::ArrayF32(pc));
            if let (Some(period), Some(p0)) = (period, p0) {
                let n = period.0.saturating_sub(p0.0).max(1) as f64;
                put("sys/dcm/share_9_11", Value::F64(period.1.saturating_sub(p0.1) as f64 / n));
                put("sys/dcm/over_20", Value::I64(period.2.saturating_sub(p0.2) as i64));
                put("sys/dcm/max_ms", Value::F64(period.3 as f64 / 1e3));
            }
        }
        self.last = Some((now, cpu, th, pr, period));
        c
    }
}

/// The ALMemory floats of [`MEMORY_KEYS`] (NaN for a key the robot does not have).
pub async fn memory_floats(memory: &AnyObject) -> Vec<f64> {
    let mut out = Vec::with_capacity(MEMORY_KEYS.len());
    for (_, key) in MEMORY_KEYS {
        let v = memory.call::<Dynamic<QiValue<'static>>, _, _>("getData", key.to_string()).await;
        out.push(v.ok().and_then(|Dynamic(v)| qi_f64(&v)).unwrap_or(f64::NAN));
    }
    out
}

/// Whether a NAOqi URL is on this machine (the HAL then runs on the robot).
pub fn is_local(url: &str) -> bool {
    let host = url.split("://").nth(1).unwrap_or(url).split([':', '/']).next().unwrap_or("");
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "")
}

/// Samples every [`PERIOD`] on the current runtime and hands each change to `publish`. `period`: a DCM plugin's
/// period counters, if the HAL has them. The `/proc` reads run on the runtime's blocking pool.
pub fn spawn(
    memory: AnyObject,
    local: bool,
    period: Option<std::sync::Arc<dyn Fn() -> PeriodCounters + Send + Sync>>,
    publish: impl Fn(StateChange) + Send + Sync + 'static,
) -> tokio::task::AbortHandle {
    tokio::spawn(async move {
        let mut sampler = Sampler::default();
        let mut interval = tokio::time::interval(PERIOD);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let m = memory_floats(&memory).await;
            let p = period.as_ref().map(|f| f());
            match tokio::task::spawn_blocking(move || {
                let c = sampler.sample(&m, p, local);
                (sampler, c)
            })
            .await
            {
                Ok((back, c)) => {
                    sampler = back;
                    publish(c);
                }
                Err(_) => break,
            }
        }
    })
    .abort_handle()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_this_machine() {
        let mut s = Sampler::default();
        let first = s.sample(&[61.0, 0.8, -1.2, 30.0], Some((0, 0, 0, 0)), true);
        assert!(first.set.contains_key(&Key::from("sys/load")) && !first.set.contains_key(&Key::from("sys/cpu/busy")));
        let x: u64 = (0..2_000_000u64).map(|i| i ^ (i >> 3)).sum();
        assert!(x > 0);
        let c = s.sample(&[61.0, 0.8, -1.2, 30.0], Some((100, 99, 0, 10_500)), true);
        let get = |k: &str| c.set.get(&Key::from(k)).cloned().flatten();
        assert!(matches!(get("sys/cpu/busy"), Some(Value::ArrayF32(v)) if !v.is_empty()));
        assert!(matches!(get("sys/threads/names"), Some(Value::ArrayString(v)) if !v.is_empty()));
        assert!(matches!(get("sys/dcm/share_9_11"), Some(Value::F64(x)) if (x - 0.99).abs() < 1e-9));
        assert!(matches!(get("sys/head/temperature"), Some(Value::F64(x)) if x == 61.0));
        assert!(matches!(get("sys/mem/available_mb"), Some(Value::F64(x)) if x > 0.0));
        assert_eq!(stat_ticks("12 (a b) S 1 2 3 4 5 6 7 8 9 10 7 3 0"), Some(("a b".into(), 10)));
        // not the robot: its ALMemory keys only
        let remote = Sampler::default().sample(&[61.0, f64::NAN, f64::NAN, f64::NAN], None, false);
        assert_eq!(remote.set.len(), MEMORY_KEYS.len());
    }
}
