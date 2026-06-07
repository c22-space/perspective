pub mod communities;
pub mod dedup;
pub mod promotion;
pub mod scheduler;

pub use communities::detect_communities;
pub use dedup::find_duplicates;
pub use promotion::find_promotable;
pub use scheduler::ConsolidationScheduler;
