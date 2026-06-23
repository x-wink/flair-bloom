use burst_engine::stress::{run_stress, StressConfig};
use std::env;
use std::time::Duration;

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }
    if args.iter().any(|arg| arg == "--matrix") {
        run_matrix(&args);
        return;
    }

    let config = parse_config(&args);
    println!("{}", run_stress(config).to_json_line());
}

fn print_help() {
    println!(
        "Usage: cargo run -p burst-engine --bin scheduler_stress --release -- [--rules N] [--interval-ms N] [--duration-ms N] [--same-target] [--dispatch-cost-us N] [--matrix]"
    );
    println!("Dry-run scheduler stress test. It does not inject real keyboard or mouse events.");
    println!("--dispatch-cost-us N: 模拟每事件注入耗时（µs），>0 复现下游背压以验证自适应降频。");
    println!(
        "--dispatch-cost-delay-ms N: 背压起始延迟，注入 N ms 后才施加耗时，模拟积压逐渐建立的 ramp。"
    );
}

fn run_matrix(args: &[String]) {
    let duration = duration_arg(args).unwrap_or(Duration::from_secs(5));
    for rules in [1, 8, 32, 64] {
        for interval_ms in [1, 5, 10, 30, 50] {
            let config = StressConfig {
                rules,
                interval_ms,
                duration,
                same_target: args.iter().any(|arg| arg == "--same-target"),
                simulated_dispatch_cost: dispatch_cost_arg(args),
                simulated_dispatch_cost_delay: dispatch_cost_delay_arg(args),
            };
            println!("{}", run_stress(config).to_json_line());
        }
    }
}

fn parse_config(args: &[String]) -> StressConfig {
    StressConfig {
        rules: usize_arg(args, "--rules").unwrap_or(64),
        interval_ms: u32_arg(args, "--interval-ms").unwrap_or(10),
        duration: duration_arg(args).unwrap_or(Duration::from_secs(5)),
        same_target: args.iter().any(|arg| arg == "--same-target"),
        simulated_dispatch_cost: dispatch_cost_arg(args),
        simulated_dispatch_cost_delay: dispatch_cost_delay_arg(args),
    }
}

fn duration_arg(args: &[String]) -> Option<Duration> {
    u64_arg(args, "--duration-ms").map(Duration::from_millis)
}

fn dispatch_cost_arg(args: &[String]) -> Duration {
    u64_arg(args, "--dispatch-cost-us")
        .map(Duration::from_micros)
        .unwrap_or(Duration::ZERO)
}

fn dispatch_cost_delay_arg(args: &[String]) -> Duration {
    u64_arg(args, "--dispatch-cost-delay-ms")
        .map(Duration::from_millis)
        .unwrap_or(Duration::ZERO)
}

fn usize_arg(args: &[String], name: &str) -> Option<usize> {
    value_after(args, name).and_then(|value| value.parse().ok())
}

fn u32_arg(args: &[String], name: &str) -> Option<u32> {
    value_after(args, name).and_then(|value| value.parse().ok())
}

fn u64_arg(args: &[String], name: &str) -> Option<u64> {
    value_after(args, name).and_then(|value| value.parse().ok())
}

fn value_after<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find_map(|pair| (pair[0] == name).then_some(pair[1].as_str()))
}
