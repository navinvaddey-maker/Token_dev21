use std::sync::atomic::{AtomicU64, Ordering};
use lazy_static::lazy_static;

pub struct Counter {
    val: AtomicU64,
}

impl Counter {
    pub fn new() -> Self { Self { val: AtomicU64::new(0) } }
    pub fn inc(&self) { self.val.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_by(&self, n: u64) { self.val.fetch_add(n, Ordering::Relaxed); }
    pub fn get(&self) -> u64 { self.val.load(Ordering::Relaxed) }
}

pub struct Gauge {
    val: AtomicU64,
}

impl Gauge {
    pub fn new() -> Self { Self { val: AtomicU64::new(0) } }
    pub fn set(&self, n: u64) { self.val.store(n, Ordering::Relaxed); }
    pub fn get(&self) -> u64 { self.val.load(Ordering::Relaxed) }
}

lazy_static! {
    pub static ref RESOLVE_TOTAL_RESOLVED: Counter = Counter::new();
    pub static ref RESOLVE_TOTAL_LOW_CONFIDENCE: Counter = Counter::new();
    pub static ref RESOLVE_TOTAL_EMPTY: Counter = Counter::new();
    
    pub static ref CONFIG_RELOAD_SUCCESS: Counter = Counter::new();
    pub static ref CONFIG_RELOAD_FAILURE: Counter = Counter::new();
    
    pub static ref CONFIG_RULES_TOTAL: Gauge = Gauge::new();
    pub static ref FEEDBACK_STORE_SIZE: Gauge = Gauge::new();
    
    pub static ref CONSTRAINTS_INCLUSION_TOTAL: Counter = Counter::new();
    pub static ref CONSTRAINTS_FORBIDDEN_TOTAL: Counter = Counter::new();
}

pub struct MetricsRegistry;

impl MetricsRegistry {
    pub fn gather() -> String {
        let mut out = String::new();
        out.push_str(&format!("resolver_resolve_total{{status=\"resolved\"}} {}\n", RESOLVE_TOTAL_RESOLVED.get()));
        out.push_str(&format!("resolver_resolve_total{{status=\"low_confidence\"}} {}\n", RESOLVE_TOTAL_LOW_CONFIDENCE.get()));
        out.push_str(&format!("resolver_resolve_total{{status=\"empty\"}} {}\n", RESOLVE_TOTAL_EMPTY.get()));
        out.push_str(&format!("config_reload_success_total {}\n", CONFIG_RELOAD_SUCCESS.get()));
        out.push_str(&format!("config_reload_failure_total {}\n", CONFIG_RELOAD_FAILURE.get()));
        out.push_str(&format!("config_rules_total {}\n", CONFIG_RULES_TOTAL.get()));
        out.push_str(&format!("feedback_store_size {}\n", FEEDBACK_STORE_SIZE.get()));
        out.push_str(&format!("constraints_extracted_total{{type=\"inclusion\"}} {}\n", CONSTRAINTS_INCLUSION_TOTAL.get()));
        out.push_str(&format!("constraints_extracted_total{{type=\"forbidden\"}} {}\n", CONSTRAINTS_FORBIDDEN_TOTAL.get()));
        out
    }
}
