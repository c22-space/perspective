pub mod scheduler;
pub mod dedup;
pub mod promotion;
pub mod communities;

pub use scheduler::ConsolidationScheduler;
pub use dedup::find_duplicates;
pub use promotion::find_promotable;
pub use communities::detect_communities;
