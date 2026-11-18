// @source: wdc-w65c816s#table-5-4
//! Instruction families, one submodule each.
//!
//! The families arrived with issues #33 to #42.

mod arithmetic;
mod block_move;
mod flow;
mod interrupt;
mod load_store;
mod logic;
mod shift;
mod stack;
mod status;
mod transfer;

pub(crate) use flow::Condition;
pub(crate) use interrupt::Interrupt;
pub(crate) use shift::Modify;
