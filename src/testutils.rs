use crate::{
    bitcoin::{BitcoinInterface, BlockChainTip, UTxO},
    config::{BitcoinConfig, Config},
    database::{Coin, DatabaseConnection, DatabaseInterface, SpendBlock},
    descriptors, DaemonHandle,
};

use std::{collections::HashMap, env, fs, io, path, process, str::FromStr, sync, thread, time};

use miniscript::{
    bitcoin::{
        self, secp256k1,
        util::{bip32, psbt::PartiallySignedTransaction as Psbt},
        Transaction, Txid,
    },
    descriptor,
};

pub struct DummyBitcoind {
    pub txs: HashMap<Txid, Transaction>,
}

impl DummyBitcoind {}

impl DummyBitcoind {
    pub fn new() -> Self {
        Self {
            txs: HashMap::new(),
        }
    }
}

impl BitcoinInterface for DummyBitcoind {
    fn genesis_block(&self) -> BlockChainTip {
        let hash = bitcoin::BlockHash::from_str(
            "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
        )
        .unwrap();
        BlockChainTip { hash, height: 0 }
    }

    fn sync_progress(&self) -> f64 {
        1.0
    }

    fn chain_tip(&self) -> BlockChainTip {
        let hash = bitcoin::BlockHash::from_str(
            "000000007bc154e0fa7ea32218a72fe2c1bb9f86cf8c9ebf9a715ed27fdb229a",
        )
        .unwrap();
        let height = 100;
        BlockChainTip { hash, height }
    }

    fn is_in_chain(&self, _: &BlockChainTip) -> bool {
        // No reorg
        true
    }

    fn received_coins(
        &self,
        _: &BlockChainTip,
        _: &[descriptors::InheritanceDescriptor],
    ) -> Vec<UTxO> {
        Vec::new()
    }

    fn confirmed_coins(&self, _: &[bitcoin::OutPoint]) -> Vec<(bitcoin::OutPoint, i32, u32)> {
        Vec::new()
    }

    fn spending_coins(&self, _: &[bitcoin::OutPoint]) -> Vec<(bitcoin::OutPoint, bitcoin::Txid)> {
        Vec::new()
    }

    fn spent_coins(
        &self,
        _: &[(bitcoin::OutPoint, bitcoin::Txid)],
    ) -> Vec<(bitcoin::OutPoint, bitcoin::Txid, i32, u32)> {
        Vec::new()
    }

    fn common_ancestor(&self, _: &BlockChainTip) -> Option<BlockChainTip> {
        todo!()
    }

    fn broadcast_tx(&self, _: &bitcoin::Transaction) -> Result<(), String> {
        todo!()
    }

    fn start_rescan(&self, _: &descriptors::MultipathDescriptor, _: u32) -> Result<(), String> {
        todo!()
    }

    fn rescan_progress(&self) -> Option<f64> {
        None
    }

    fn block_before_date(&self, _: u32) -> Option<BlockChainTip> {
        todo!()
    }

    fn tip_time(&self) -> u32 {
        todo!()
    }

    fn wallet_transaction(&self, txid: &bitcoin::Txid) -> Option<bitcoin::Transaction> {
        self.txs.get(txid).cloned()
    }
}

struct DummyDbState {
    deposit_index: bip32::ChildNumber,
    change_index: bip32::ChildNumber,
    curr_tip: Option<BlockChainTip>,
    coins: HashMap<bitcoin::OutPoint, Coin>,
    spend_txs: HashMap<bitcoin::Txid, Psbt>,
}

pub struct DummyDatabase {
    db: sync::Arc<sync::RwLock<DummyDbState>>,
}

impl DatabaseInterface for DummyDatabase {
    fn connection(&self) -> Box<dyn DatabaseConnection> {
        Box::new(DummyDatabase {
            db: self.db.clone(),
        })
    }
}

impl DummyDatabase {
    pub fn new() -> DummyDatabase {
        DummyDatabase {
            db: sync::Arc::new(sync::RwLock::new(DummyDbState {
                deposit_index: 0.into(),
                change_index: 0.into(),
                curr_tip: None,
                coins: HashMap::new(),
                spend_txs: HashMap::new(),
            })),
        }
    }

    pub fn insert_coins(&mut self, coins: Vec<Coin>) {
        for coin in coins {
            self.db.write().unwrap().coins.insert(coin.outpoint, coin);
        }
    }
}

impl DatabaseConnection for DummyDatabase {
    fn network(&mut self) -> bitcoin::Network {
        bitcoin::Network::Bitcoin
    }

    fn chain_tip(&mut self) -> Option<BlockChainTip> {
        self.db.read().unwrap().curr_tip
    }

    fn update_tip(&mut self, tip: &BlockChainTip) {
        self.db.write().unwrap().curr_tip = Some(*tip);
    }

    fn receive_index(&mut self) -> bip32::ChildNumber {
        self.db.read().unwrap().deposit_index
    }

    fn change_index(&mut self) -> bip32::ChildNumber {
        self.db.read().unwrap().deposit_index
    }

    fn increment_receive_index(&mut self, _: &secp256k1::Secp256k1<secp256k1::VerifyOnly>) {
        let next_index = self.db.write().unwrap().deposit_index.increment().unwrap();
        self.db.write().unwrap().deposit_index = next_index;
    }

    fn increment_change_index(&mut self, _: &secp256k1::Secp256k1<secp256k1::VerifyOnly>) {
        let next_index = self.db.write().unwrap().change_index.increment().unwrap();
        self.db.write().unwrap().change_index = next_index;
    }

    fn coins(&mut self) -> HashMap<bitcoin::OutPoint, Coin> {
        self.db.read().unwrap().coins.clone()
    }

    fn list_spending_coins(&mut self) -> HashMap<bitcoin::OutPoint, Coin> {
        let mut result = HashMap::new();
        for (k, v) in self.db.read().unwrap().coins.iter() {
            if v.spend_txid.is_some() {
                result.insert(*k, *v);
            }
        }
        result
    }

    fn new_unspent_coins<'a>(&mut self, coins: &[Coin]) {
        for coin in coins {
            self.db.write().unwrap().coins.insert(coin.outpoint, *coin);
        }
    }

    fn confirm_coins<'a>(&mut self, outpoints: &[(bitcoin::OutPoint, i32, u32)]) {
        for (op, height, time) in outpoints {
            let mut db = self.db.write().unwrap();
            let coin = &mut db.coins.get_mut(op).unwrap();
            assert!(coin.block_height.is_none());
            assert!(coin.block_time.is_none());
            coin.block_height = Some(*height);
            coin.block_time = Some(*time);
        }
    }

    fn spend_coins<'a>(&mut self, outpoints: &[(bitcoin::OutPoint, bitcoin::Txid)]) {
        for (op, spend_txid) in outpoints {
            let mut db = self.db.write().unwrap();
            let spent = &mut db.coins.get_mut(op).unwrap();
            assert!(spent.spend_txid.is_none());
            assert!(spent.spend_block.is_none());
            spent.spend_txid = Some(*spend_txid);
        }
    }

    fn confirm_spend<'a>(&mut self, outpoints: &[(bitcoin::OutPoint, bitcoin::Txid, i32, u32)]) {
        for (op, spend_txid, height, time) in outpoints {
            let mut db = self.db.write().unwrap();
            let spent = &mut db.coins.get_mut(op).unwrap();
            assert!(spent.spend_txid.is_some());
            assert!(spent.spend_block.is_none());
            spent.spend_txid = Some(*spend_txid);
            spent.spend_block = Some(SpendBlock {
                height: *height,
                time: *time,
            });
        }
    }

    fn derivation_index_by_address(
        &mut self,
        _: &bitcoin::Address,
    ) -> Option<(bip32::ChildNumber, bool)> {
        None
    }

    fn coins_by_outpoints(
        &mut self,
        outpoints: &[bitcoin::OutPoint],
    ) -> HashMap<bitcoin::OutPoint, Coin> {
        // Very inefficient but hey
        self.db
            .read()
            .unwrap()
            .coins
            .clone()
            .into_iter()
            .filter(|(op, _)| outpoints.contains(op))
            .collect()
    }

    fn store_spend(&mut self, psbt: &Psbt) {
        let txid = psbt.unsigned_tx.txid();
        self.db
            .write()
            .unwrap()
            .spend_txs
            .insert(txid, psbt.clone());
    }

    fn spend_tx(&mut self, txid: &bitcoin::Txid) -> Option<Psbt> {
        self.db.read().unwrap().spend_txs.get(txid).cloned()
    }

    fn list_spend(&mut self) -> Vec<Psbt> {
        self.db
            .read()
            .unwrap()
            .spend_txs
            .values()
            .cloned()
            .collect()
    }

    fn delete_spend(&mut self, txid: &bitcoin::Txid) {
        self.db.write().unwrap().spend_txs.remove(txid);
    }

    fn rollback_tip(&mut self, _: &BlockChainTip) {
        todo!()
    }

    fn rescan_timestamp(&mut self) -> Option<u32> {
        None
    }

    fn set_rescan(&mut self, _: u32) {
        todo!()
    }

    fn complete_rescan(&mut self) {
        todo!()
    }

    fn list_updated_coins(&mut self, start: u32, end: u32, limit: u64) -> Vec<Coin> {
        let mut txids_and_time = Vec::new();
        let coins = &self.db.read().unwrap().coins;
        // Get txid and block time of every transactions that happened between start and end
        // timestamps.
        for coin in coins.values() {
            if let Some(time) = coin.block_time {
                if time >= start && time <= end {
                    let row = (coin.outpoint.txid, time);
                    if !txids_and_time.contains(&row) {
                        txids_and_time.push(row);
                    }
                }
            }
            if let Some(time) = coin.spend_block.map(|b| b.time) {
                if time >= start && time <= end {
                    let row = (coin.spend_txid.expect("spent_at is not none"), time);
                    if !txids_and_time.contains(&row) {
                        txids_and_time.push(row);
                    }
                }
            }
        }
        // Apply order and limit
        txids_and_time.sort_by(|(_, t1), (_, t2)| t2.cmp(t1));
        txids_and_time.truncate(limit as usize);

        // Collect all the coins updated by the transactions with the collected txids.
        let mut updated_coins = Vec::new();
        for coin in coins.values() {
            for (txid, _) in txids_and_time.iter() {
                if !updated_coins.contains(coin)
                    && (coin.outpoint.txid == *txid || coin.spend_txid == Some(*txid))
                {
                    updated_coins.push(*coin);
                }
            }
        }
        updated_coins
    }
}

pub struct DummyMinisafe {
    pub tmp_dir: path::PathBuf,
    pub handle: DaemonHandle,
}

static mut COUNTER: sync::atomic::AtomicUsize = sync::atomic::AtomicUsize::new(0);
fn uid() -> usize {
    unsafe {
        let uid = COUNTER.load(sync::atomic::Ordering::Relaxed);
        COUNTER.fetch_add(1, sync::atomic::Ordering::Relaxed);
        uid
    }
}

pub fn tmp_dir() -> path::PathBuf {
    env::temp_dir().join(format!(
        "minisafed-{}-{:?}-{}",
        process::id(),
        thread::current().id(),
        uid(),
    ))
}

impl DummyMinisafe {
    /// Creates a new DummyMinisafe interface
    pub fn new(
        bitcoin_interface: impl BitcoinInterface + 'static,
        database: impl DatabaseInterface + 'static,
    ) -> DummyMinisafe {
        let tmp_dir = tmp_dir();
        fs::create_dir_all(&tmp_dir).unwrap();
        // Use a shorthand for 'datadir', to avoid overflowing SUN_LEN on MacOS.
        let data_dir: path::PathBuf = [tmp_dir.as_path(), path::Path::new("d")].iter().collect();

        let network = bitcoin::Network::Bitcoin;
        let bitcoin_config = BitcoinConfig {
            network,
            poll_interval_secs: time::Duration::from_secs(2),
        };

        let owner_key = descriptor::DescriptorPublicKey::from_str("xpub68JJTXc1MWK8KLW4HGLXZBJknja7kDUJuFHnM424LbziEXsfkh1WQCiEjjHw4zLqSUm4rvhgyGkkuRowE9tCJSgt3TQB5J3SKAbZ2SdcKST/<0;1>/*").unwrap();
        let heir_key = descriptor::DescriptorPublicKey::from_str("xpub68JJTXc1MWK8PEQozKsRatrUHXKFNkD1Cb1BuQU9Xr5moCv87anqGyXLyUd4KpnDyZgo3gz4aN1r3NiaoweFW8UutBsBbgKHzaD5HkTkifK/<0;1>/*").unwrap();
        let desc =
            crate::descriptors::MultipathDescriptor::new(owner_key, heir_key, 10_000).unwrap();
        let config = Config {
            bitcoin_config,
            bitcoind_config: None,
            data_dir: Some(data_dir),
            #[cfg(unix)]
            daemon: false,
            log_level: log::LevelFilter::Debug,
            main_descriptor: desc,
        };

        let handle = DaemonHandle::start(config, Some(bitcoin_interface), Some(database)).unwrap();
        DummyMinisafe { tmp_dir, handle }
    }

    #[cfg(feature = "jsonrpc_server")]
    pub fn rpc_server(self) -> Result<(), io::Error> {
        self.handle.rpc_server()?;
        fs::remove_dir_all(&self.tmp_dir)?;
        Ok(())
    }

    pub fn shutdown(self) {
        self.handle.shutdown();
        fs::remove_dir_all(&self.tmp_dir).unwrap();
    }
}
