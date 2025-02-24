use std::{sync::Arc, time::Duration, vec};

use crate::start_consensus_worker;
use crossbeam_channel::Receiver;
use massa_channel::{receiver::MassaReceiver, MassaChannel};
use massa_consensus_exports::{
    events::ConsensusEvent, ConsensusChannels, ConsensusConfig, ConsensusController,
};
use massa_execution_exports::test_exports::MockExecutionController;
use massa_hash::Hash;
use massa_metrics::MassaMetrics;
use massa_models::{
    address::Address,
    block::{Block, BlockSerializer, SecureShareBlock},
    block_header::{BlockHeader, BlockHeaderSerializer},
    block_id::BlockId,
    config::{ENDORSEMENT_COUNT, THREAD_COUNT},
    secure_share::SecureShareContent,
    slot::Slot,
};
use massa_pool_exports::test_exports::MockPoolController;
use massa_pos_exports::{
    test_exports::{MockSelectorController, MockSelectorControllerMessage},
    Selection, SelectorController,
};
use massa_protocol_exports::{MockProtocolController, ProtocolController};
use massa_signature::KeyPair;
use massa_storage::Storage;
use parking_lot::Mutex;

pub fn consensus_without_pool_test<F>(cfg: ConsensusConfig, test: F)
where
    F: FnOnce(
        MockProtocolController,
        Box<dyn ConsensusController>,
        MassaReceiver<ConsensusEvent>,
        Box<dyn SelectorController>,
        Receiver<MockSelectorControllerMessage>,
    ) -> (
        MockProtocolController,
        Box<dyn ConsensusController>,
        MassaReceiver<ConsensusEvent>,
        Box<dyn SelectorController>,
        Receiver<MockSelectorControllerMessage>,
    ),
{
    let storage: Storage = Storage::create_root();
    // mock protocol & pool
    let mut protocol_controller = MockProtocolController::default();
    let mut protocol_controller_2 = MockProtocolController::default();
    let mut protocol_controller_3 = MockProtocolController::default();
    //TODO: Test better here for example number of times
    protocol_controller_3
        .expect_integrated_block()
        .returning(|_, _| Ok(()));
    protocol_controller_3
        .expect_notify_block_attack()
        .returning(|_| Ok(()));
    protocol_controller_2
        .expect_clone_box()
        .return_once(move || Box::new(protocol_controller_3));
    protocol_controller
        .expect_clone_box()
        .return_once(move || Box::new(protocol_controller_2));
    let (pool_controller, _pool_event_receiver) = MockPoolController::new_with_receiver();
    let (selector_controller, selector_receiver) = MockSelectorController::new_with_receiver();
    // for now, execution_rx is ignored: clique updates to Execution pile up and are discarded
    let (execution_controller, execution_rx) = MockExecutionController::new_with_receiver();
    let stop_sinks = Arc::new(Mutex::new(false));
    let stop_sinks_clone = stop_sinks.clone();
    let execution_sink = std::thread::spawn(move || {
        while !*stop_sinks_clone.lock() {
            let _ = execution_rx.recv_timeout(Duration::from_millis(500));
        }
    });
    // launch consensus controller
    let (consensus_event_sender, consensus_event_receiver) =
        MassaChannel::new(String::from("consensus_event"), Some(10));

    // All API channels
    let (block_sender, _block_receiver) = tokio::sync::broadcast::channel(10);
    let (block_header_sender, _block_header_receiver) = tokio::sync::broadcast::channel(10);
    let (filled_block_sender, _filled_block_receiver) = tokio::sync::broadcast::channel(10);
    let (consensus_controller, mut consensus_manager) = start_consensus_worker(
        cfg.clone(),
        ConsensusChannels {
            block_sender,
            block_header_sender,
            filled_block_sender,
            controller_event_tx: consensus_event_sender,
            execution_controller,
            protocol_controller: protocol_controller.clone_box(),
            pool_controller,
            selector_controller: selector_controller.clone(),
        },
        None,
        storage.clone(),
        MassaMetrics::new(
            false,
            "0.0.0.0:9898".parse().unwrap(),
            THREAD_COUNT,
            Duration::from_secs(1),
        )
        .0,
    );

    // Call test func.
    let (
        _protocol_controller,
        consensus_controller,
        _consensus_event_receiver,
        _selector_controller,
        _selector_receiver,
    ) = test(
        protocol_controller,
        consensus_controller,
        consensus_event_receiver,
        selector_controller,
        selector_receiver,
    );

    // stop controller while ignoring all commands
    drop(consensus_controller);
    consensus_manager.stop();
    // stop sinks
    *stop_sinks.lock() = true;
    execution_sink.join().unwrap();
}

// returns hash and resulting discarded blocks
pub fn create_block(slot: Slot, best_parents: Vec<BlockId>, creator: &KeyPair) -> SecureShareBlock {
    create_block_with_merkle_root(
        Hash::compute_from("default_val".as_bytes()),
        slot,
        best_parents,
        creator,
    )
}

// returns hash and resulting discarded blocks
pub fn create_block_with_merkle_root(
    operation_merkle_root: Hash,
    slot: Slot,
    best_parents: Vec<BlockId>,
    creator: &KeyPair,
) -> SecureShareBlock {
    let header = BlockHeader::new_verifiable(
        BlockHeader {
            current_version: 0,
            announced_version: None,
            denunciations: vec![],
            slot,
            parents: best_parents,
            operation_merkle_root,
            endorsements: Vec::new(),
        },
        BlockHeaderSerializer::new(),
        creator,
    )
    .unwrap();

    Block::new_verifiable(
        Block {
            header,
            operations: Default::default(),
        },
        BlockSerializer::new(),
        creator,
    )
    .unwrap()
}

pub fn answer_ask_producer_pos(
    selector_receiver: &Receiver<MockSelectorControllerMessage>,
    staking_address: &Address,
    timeout_ms: u64,
) {
    let res = selector_receiver
        .recv_timeout(Duration::from_millis(timeout_ms))
        .unwrap();
    println!("res: {:?}", res);
    match res {
        MockSelectorControllerMessage::GetProducer {
            slot: _,
            response_tx,
        } => {
            response_tx.send(Ok(staking_address.clone())).unwrap();
        }
        _ => panic!("unexpected message"),
    }
}

pub fn answer_ask_selection_pos(
    selector_receiver: &Receiver<MockSelectorControllerMessage>,
    staking_address: &Address,
    timeout_ms: u64,
) {
    match selector_receiver
        .recv_timeout(Duration::from_millis(timeout_ms))
        .unwrap()
    {
        MockSelectorControllerMessage::GetSelection {
            slot: _,
            response_tx,
        } => {
            response_tx
                .send(Ok(Selection {
                    endorsements: vec![staking_address.clone(); ENDORSEMENT_COUNT as usize],
                    producer: staking_address.clone(),
                }))
                .unwrap();
        }
        msg => {
            println!("msg: {:?}", msg);
            panic!("unexpected message")
        }
    }
}

pub fn register_block(
    consensus_controller: &Box<dyn ConsensusController>,
    selector_receiver: &Receiver<MockSelectorControllerMessage>,
    block: SecureShareBlock,
    mut storage: Storage,
) {
    storage.store_block(block.clone());
    consensus_controller.register_block(
        block.id,
        block.content.header.content.slot,
        storage.clone(),
        false,
    );
    match selector_receiver
        .recv_timeout(Duration::from_millis(1000))
        .unwrap()
    {
        MockSelectorControllerMessage::GetProducer {
            slot: _,
            response_tx,
        } => {
            response_tx.send(Ok(block.content_creator_address)).unwrap();
        }
        _ => panic!("unexpected message"),
    }
}

// DRY.
pub struct TestController {
    pub creator: KeyPair,
    pub consensus_controller: Box<dyn ConsensusController>,
    pub selector_receiver: Receiver<MockSelectorControllerMessage>,
    pub storage: Storage,
    pub staking_address: Address,
    pub timeout_ms: u64,
}

pub fn register_block_and_process_with_tc(
    slot: Slot,
    best_parents: Vec<BlockId>,
    test_controller: &TestController,
) -> SecureShareBlock {
    return register_block_and_process(
        slot,
        best_parents,
        &test_controller.creator,
        &test_controller.consensus_controller,
        &test_controller.selector_receiver,
        test_controller.storage.clone(),
        &test_controller.staking_address,
        test_controller.timeout_ms,
    );
}

pub fn register_block_and_process(
    slot: Slot,
    best_parents: Vec<BlockId>,
    creator: &KeyPair,
    consensus_controller: &Box<dyn ConsensusController>,
    selector_receiver: &Receiver<MockSelectorControllerMessage>,
    storage: Storage,
    staking_address: &Address,
    timeout_ms: u64,
) -> SecureShareBlock {
    let block = create_block(slot, best_parents, creator);
    register_block(
        consensus_controller,
        selector_receiver,
        block.clone(),
        storage,
    );
    answer_ask_producer_pos(selector_receiver, staking_address, timeout_ms);
    answer_ask_selection_pos(selector_receiver, staking_address, timeout_ms);
    return block;
}
