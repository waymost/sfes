// @source: wdc-w65c816s#table-5-4
//! Instruction families, one submodule each.
//!
//! During bring-up the families arrive with issues #33 to #42.

mod arithmetic;
mod block_move;
mod flow;
mod load_store;
mod logic;
mod shift;
mod stack;
mod status;
mod transfer;

pub(crate) use flow::Condition;
pub(crate) use shift::Modify;
