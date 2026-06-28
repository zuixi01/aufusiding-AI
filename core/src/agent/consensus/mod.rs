//! 共识机制模块
//!
//! Phase 6: 圆桌会议、加权投票、辩论流程、冲突解决

pub mod debate;
pub mod resolution;
pub mod roundtable;
pub mod voting;

pub use debate::{Challenge, ChallengeResponse, Proposal};
pub use resolution::{Resolution, ResolutionStrategy};
pub use roundtable::{ConsensusOutcome, Dissent, Roundtable};
pub use voting::{tally_votes, VoteResult, WeightedVote};
