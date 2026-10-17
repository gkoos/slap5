//! The monotonic clock the search's deadline is read from.
//!
//! `std` has no clock on wasm32-unknown-unknown - `Instant::now()` there panics with "time not
//! implemented on this platform", because that target is host-agnostic and the host (a page, a worker)
//! has to supply the time. `Duration` is unaffected, so only this one type is re-pointed and the host's
//! `performance.now()` stands in for it. Monotonic, unlike `Date.now()`, which the deadline arithmetic
//! (`deadline = start + limits.time`, then `now >= deadline`) relies on.
//!
//! The interface is the subset `src/search.rs` uses: `now`, `elapsed`, `saturating_duration_since` and
//! adding a `Duration`. Everything else in the engine keeps `std::time::Duration`.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(target_arch = "wasm32")]
pub use host::Instant;

#[cfg(target_arch = "wasm32")]
mod host {
    use std::ops::Add;
    use std::time::Duration;

    #[wasm_bindgen::prelude::wasm_bindgen]
    unsafe extern "C" {
        /// Milliseconds since the page or worker was created.
        #[wasm_bindgen(js_namespace = performance, js_name = now)]
        fn performance_now() -> f64;
    }

    #[derive(Clone, Copy, PartialEq, PartialOrd)]
    pub struct Instant(f64);

    impl Instant {
        pub fn now() -> Self {
            Instant(performance_now())
        }

        pub fn elapsed(&self) -> Duration {
            Self::now().saturating_duration_since(*self)
        }

        /// Zero when `earlier` is later than `self`, as `std::time::Instant` returns.
        pub fn saturating_duration_since(&self, earlier: Self) -> Duration {
            Duration::from_secs_f64((self.0 - earlier.0).max(0.0) / 1000.0)
        }
    }

    impl Add<Duration> for Instant {
        type Output = Instant;

        fn add(self, rhs: Duration) -> Instant {
            Instant(self.0 + rhs.as_secs_f64() * 1000.0)
        }
    }
}
