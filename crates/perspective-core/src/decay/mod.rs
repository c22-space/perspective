pub mod ebbinghaus;
pub mod maintenance;

pub use ebbinghaus::{calculate_strength, initial_stability, reinforce};
pub use maintenance::{apply_decay_to_memories, get_gc_candidates, get_retrieval_candidates, memory_strength};
