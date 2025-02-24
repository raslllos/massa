// Copyright (c) 2022 MASSA LABS <info@massa.net>

//! # Overview
//!
//! This crate provides all the facilities to interact with a running execution worker (massa-execution-worker crate)
//! that is in charge of executing operations in a virtual machine,
//! and applying the effects of the execution to a ledger.
//!
//! # Usage
//!
//! When an execution worker is launched to run in a separate thread for the whole duration of the process,
//! an instance of `ExecutionManager` is returned (see the documentation of `start_execution_worker` in `massa-execution-worker`),
//! as well as an instance of `ExecutionController`.
//!
//! The non-clonable `ExecutionManager` allows stopping the execution worker thread.
//!
//! The clonable `ExecutionController` allows sending updates on the latest blockclique changes to the execution worker
//! for it to keep track of them and execute the operations present in blocks.
//! It also allows various read-only queries such as executing bytecode
//! while ignoring all the changes it would cause to the consensus state (read-only execution),
//! or reading the state at the output of the executed blockclique blocks.
//!
//! # Architecture
//!
//! ## `config.rs`
//! Contains configuration parameters for the execution system.
//!
//! ## `controller_traits.rs`
//! Defines the `ExecutionManager` and `ExecutionController` traits for interacting with the execution worker.
//!
//! ## `errors.rs`
//! Defines error types for the crate.
//!
//! ## `event_store.rs`
//! Defines an indexed, finite-size storage system for execution events.
//!
//! ## `types.rs`
//! Defines useful shared structures.
//!
//! ## Test exports
//!
//! When the crate feature `testing` is enabled, tooling useful for testing purposes is exported.
//! See `test_exports/mod.rs` for details.

#![warn(missing_docs)]
#![warn(unused_crate_dependencies)]
mod channels;
mod controller_traits;
mod error;
mod event_store;
mod mapping_grpc;
mod settings;
mod types;

pub use channels::ExecutionChannels;
#[cfg(any(test, feature = "testing"))]
pub use controller_traits::MockExecutionController;
pub use controller_traits::{ExecutionController, ExecutionManager};
pub use error::{ExecutionError, ExecutionQueryError};
pub use event_store::EventStore;
pub use massa_sc_runtime::GasCosts;
pub use settings::{ExecutionConfig, StorageCostsConstants};
pub use types::{
    ExecutedBlockInfo, ExecutionAddressInfo, ExecutionOutput, ExecutionQueryCycleInfos,
    ExecutionQueryExecutionStatus, ExecutionQueryRequest, ExecutionQueryRequestItem,
    ExecutionQueryResponse, ExecutionQueryResponseItem, ExecutionQueryStakerInfo,
    ExecutionStackElement, ReadOnlyCallRequest, ReadOnlyExecutionOutput, ReadOnlyExecutionRequest,
    ReadOnlyExecutionTarget, SlotExecutionOutput,
};

#[cfg(any(feature = "testing", feature = "gas_calibration"))]
pub mod test_exports;
