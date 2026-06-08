# Scheduler Stress

Dry-run scheduler stress test. It exercises the single scheduler thread, rule timing,
StopAll ACK path, generation discard path, and target owner merge without sending real
keyboard or mouse input.

## Commands

Single scenario:

```powershell
cargo run -p burst-engine --bin scheduler_stress --release -- --rules 64 --interval-ms 10 --duration-ms 10000
```

Full matrix from the refactor plan:

```powershell
cargo run -p burst-engine --bin scheduler_stress --release -- --matrix --duration-ms 10000
```

Owner merge scenario:

```powershell
cargo run -p burst-engine --bin scheduler_stress --release -- --rules 64 --interval-ms 10 --duration-ms 10000 --same-target
```

## Output

Each line is JSON:

- `rules`: active rule count.
- `interval_ms`: configured interval.
- `scheduler_threads`: scheduler worker count; expected `1`.
- `hp_degraded`: `true` when high-resolution waitable timer creation/use degraded to standard waiting.
- `sent_events` / `failed_events`: dry-run dispatch result counts.
- `injection_rate_per_sec`: dry-run event dispatch rate.
- `delay_p50_us` / `delay_p95_us` / `delay_p99_us` / `delay_max_us`: scheduler deadline delay.
- `stop_ack_us`: StopAll command response time.
- `process_cpu_ms`: process CPU time on Windows; `null` on platforms where std-only collection is unavailable.

## Windows Process Thread Count

For total process thread count while the release stress run is active:

```powershell
$p = Start-Process cargo -ArgumentList @(
  "run","-p","burst-engine","--bin","scheduler_stress","--release","--",
  "--matrix","--duration-ms","10000"
) -NoNewWindow -PassThru
while (-not $p.HasExited) {
  Get-Process -Id $p.Id | Select-Object Id,CPU,Threads
  Start-Sleep -Seconds 1
}
```

## Windows Real-Input Validation

The stress binary is intentionally dry-run and does not send real keyboard or mouse input.
Before treating a release as fully validated on Windows, run the release app on Windows
and verify these real-input cases with SendInput, DD-HID, and DDSimple as applicable:

- Panel focused, `trigger == target` Toggle: injected events must be consumed by relay filtering and must not flip the Toggle repeatedly.
- Physical target key held while a different trigger is active: scheduler must skip new simulated `down` pulses for that target until the physical key is released.
- Simulated target already down, then physical target is pressed and Stop/StopAll is issued: the simulated `up` must still be emitted.
- 64 rules at 10 ms: process thread count must not scale with rule count, and same-target rules must emit one merged target hold.
- StopAll under load: no new target `down` after StopAll is accepted, and all engine-owned target keys are released.
