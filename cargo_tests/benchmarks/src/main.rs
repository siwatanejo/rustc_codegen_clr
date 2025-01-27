#![cfg_attr(not(test), no_std)]
#![feature(start, core_intrinsics, lang_items)]
use core::intrinsics::black_box;
use mycorrhiza::{panic_handler, start};
#[cfg(not(test))]
panic_handler! {}
#[cfg(not(test))]
start! {}
#[cfg(not(test))]
#[lang = "eh_personality"]
fn rust_eh_personality() {}
fn main() {
    let time = black_box(Fibonachi::benchmark());
    mycorrhiza::system::console::Console::writeln_f64(time);
}
trait BenchmarkableFn {
    fn run();
    #[cfg(not(test))]
    fn benchmark() -> f64 {
        use mycorrhiza::system::diagnostics::Stopwatch;
        // Let the JIT warm up.
        for _ in 0..100_000_000 {
            Self::run();
        }
        let stopwatch = Stopwatch::new();
        stopwatch.start();
        for _ in 0..100_000_000 {
            Self::run();
        }
        stopwatch.stop();
        let ms = stopwatch.elapsed_milliseconds();
        let ns = (ms * 1_000_000) as f64;
        let ns_per_iter = ns / (100_000_000 as f64);
        ns_per_iter
    }
    #[cfg(test)]
    fn benchmark() -> f64 {
        // Here just to elimnate any wierd codegen flukes
        for _ in 0..100_000_000 {
            Self::run();
        }
        let stopwatch = std::time::Instant::now();
        for _ in 0..100_000_000 {
            Self::run();
        }
        let ms = stopwatch.elapsed().as_millis();
        let ns = (ms * 1_000_000) as f64;
        let ns_per_iter = ns / (100_000_000 as f64);
        ns_per_iter
    }
}
struct Fibonachi;
fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fibonacci(n - 1) + fibonacci(n - 2),
    }
}
impl BenchmarkableFn for Fibonachi {
    fn run() {
        black_box(fibonacci(black_box(10)));
    }
}
struct So;
#[cfg(test)]
#[test]
fn native_bench() {
    let time = black_box(Fibonachi::benchmark());
    panic!("{time}");
}
