///! Database interface for Minisafe.
///!
///! Record wallet metadata, spent and unspent coins, ongoing transactions.
pub mod sqlite;

use crate::{
    bitcoin::BlockChainTip,
    database::sqlite::{
        schema::{DbCoin, DbSpendBlock, DbTip},
        SqliteConn, SqliteDb,
    },
};

use std::{collections::HashMap, sync};

use miniscript::bitcoin::{
    self, secp256k1,
    util::{bip32, psbt::PartiallySignedTransaction as Psbt},
};

pub trait DatabaseInterface: Send {
    fn connection(&self) -> Box<dyn DatabaseConnection>;
}

impl DatabaseInterface for SqliteDb {
    fn connection(&self) -> Box<dyn DatabaseConnection> {
        Box::new(self.connection().expect("Database must be available"))
    }
}

// FIXME: do we need to repeat the entire trait implemenation? Isn't there a nicer way?
impl DatabaseInterface for sync::Arc<sync::Mutex<dyn DatabaseInterface>> {
    fn connection(&self) -> Box<dyn DatabaseConnection> {
        self.lock().unwrap().connection()
    }
}

pub trait DatabaseConnection {
    /// Get the tip of the best chain we've seen.
    fn chain_tip(&mut self) -> Option<BlockChainTip>;

    /// The network we are operating on.
    fn network(&mut self) -> bitcoin::Network;

    /// Update our best chain seen.
    fn update_tip(&mut self, tip: &BlockChainTip);

    fn receive_index(&mut self) -> bip32::ChildNumber;

    fn change_index(&mut self) -> bip32::ChildNumber;

    fn increment_receive_index(&mut self, secp: &secp256k1::Secp256k1<secp256k1::VerifyOnly>);

    fn increment_change_index(&mut self, secp: &secp256k1::Secp256k1<secp256k1::VerifyOnly>);

    /// Get the timestamp at which to start rescaning from, if any.
    fn rescan_timestamp(&mut self) -> Option<u32>;

    /// Set a timestamp at which to start rescaning the block chain from.
    fn set_rescan(&mut self, timestamp: u32);

    /// Mark the rescan as complete.
    fn complete_rescan(&mut self);

    /// Get the derivation index for this address, as well as whether this address is change.
    fn derivation_index_by_address(
        &mut self,
        address: &bitcoin::Address,
    ) -> Option<(bip32::ChildNumber, bool)>;

    /// Get all our coins, past or present, spent or not.
    fn coins(&mut self) -> HashMap<bitcoin::OutPoint, Coin>;

    /// List coins that are being spent and whose spending transaction is still unconfirmed.
    fn list_spending_coins(&mut self) -> HashMap<bitcoin::OutPoint, Coin>;

    /// Store new UTxOs. Coins must not already be in database.
    fn new_unspent_coins(&mut self, coins: &[Coin]);

    /// Mark a set of coins as being confirmed at a specified height and block time.
    fn confirm_coins(&mut self, outpoints: &[(bitcoin::OutPoint, i32, u32)]);

    /// Mark a set of coins as being spent by a specified txid of a pending transaction.
    fn spend_coins(&mut self, outpoints: &[(bitcoin::OutPoint, bitcoin::Txid)]);

    /// Mark a set of coins as spent by a specified txid at a specified block time.
    fn confirm_spend(&mut self, outpoints: &[(bitcoin::OutPoint, bitcoin::Txid, i32, u32)]);

    /// Get specific coins from the database.
    fn coins_by_outpoints(
        &mut self,
        outpoints: &[bitcoin::OutPoint],
    ) -> HashMap<bitcoin::OutPoint, Coin>;

    fn spend_tx(&mut self, txid: &bitcoin::Txid) -> Option<Psbt>;

    /// Insert a new Spend transaction or replace an existing one.
    fn store_spend(&mut self, psbt: &Psbt);

    /// List all existing Spend transactions.
    fn list_spend(&mut self) -> Vec<Psbt>;

    /// Delete a Spend transaction from database.
    fn delete_spend(&mut self, txid: &bitcoin::Txid);

    /// Mark the given tip as the new best seen block. Update stored data accordingly.
    fn rollback_tip(&mut self, new_tip: &BlockChainTip);

    /// Retrieved a limited list of coins that where deposited or spent between the start and end timestamps.
    fn list_updated_coins(&mut self, start: u32, end: u32, limit: u64) -> Vec<Coin>;
}

impl DatabaseConnection for SqliteConn {
    fn chain_tip(&mut self) -> Option<BlockChainTip> {
        match self.db_tip() {
            DbTip {
                block_height: Some(height),
                block_hash: Some(hash),
                ..
            } => Some(BlockChainTip { height, hash }),
            _ => None,
        }
    }

    fn network(&mut self) -> bitcoin::Network {
        self.db_tip().network
    }

    fn update_tip(&mut self, tip: &BlockChainTip) {
        self.update_tip(tip)
    }

    fn receive_index(&mut self) -> bip32::ChildNumber {
        self.db_wallet().deposit_derivation_index
    }

    fn change_index(&mut self) -> bip32::ChildNumber {
        self.db_wallet().change_derivation_index
    }

    fn increment_receive_index(&mut self, secp: &secp256k1::Secp256k1<secp256k1::VerifyOnly>) {
        self.increment_deposit_index(secp)
    }

    fn increment_change_index(&mut self, secp: &secp256k1::Secp256k1<secp256k1::VerifyOnly>) {
        self.increment_change_index(secp)
    }

    fn rescan_timestamp(&mut self) -> Option<u32> {
        self.db_wallet().rescan_timestamp
    }

    fn set_rescan(&mut self, timestamp: u32) {
        self.set_wallet_rescan_timestamp(timestamp)
    }

    fn complete_rescan(&mut self) {
        self.complete_wallet_rescan()
    }

    fn coins(&mut self) -> HashMap<bitcoin::OutPoint, Coin> {
        self.coins()
            .into_iter()
            .map(|db_coin| (db_coin.outpoint, db_coin.into()))
            .collect()
    }

    fn list_spending_coins(&mut self) -> HashMap<bitcoin::OutPoint, Coin> {
        self.list_spending_coins()
            .into_iter()
            .map(|db_coin| (db_coin.outpoint, db_coin.into()))
            .collect()
    }

    fn new_unspent_coins<'a>(&mut self, coins: &[Coin]) {
        self.new_unspent_coins(coins)
    }

    fn confirm_coins<'a>(&mut self, outpoints: &[(bitcoin::OutPoint, i32, u32)]) {
        self.confirm_coins(outpoints)
    }

    fn spend_coins<'a>(&mut self, outpoints: &[(bitcoin::OutPoint, bitcoin::Txid)]) {
        self.spend_coins(outpoints)
    }

    fn confirm_spend<'a>(&mut self, outpoints: &[(bitcoin::OutPoint, bitcoin::Txid, i32, u32)]) {
        self.confirm_spend(outpoints)
    }

    fn derivation_index_by_address(
        &mut self,
        address: &bitcoin::Address,
    ) -> Option<(bip32::ChildNumber, bool)> {
        self.db_address(address)
            .map(|db_addr| (db_addr.derivation_index, address == &db_addr.change_address))
    }

    fn coins_by_outpoints(
        &mut self,
        outpoints: &[bitcoin::OutPoint],
    ) -> HashMap<bitcoin::OutPoint, Coin> {
        self.db_coins(outpoints)
            .into_iter()
            .map(|db_coin| (db_coin.outpoint, db_coin.into()))
            .collect()
    }

    fn spend_tx(&mut self, txid: &bitcoin::Txid) -> Option<Psbt> {
        self.db_spend(txid).map(|db_spend| db_spend.psbt)
    }

    fn store_spend(&mut self, psbt: &Psbt) {
        self.store_spend(psbt)
    }

    fn list_spend(&mut self) -> Vec<Psbt> {
        self.list_spend()
            .into_iter()
            .map(|db_spend| db_spend.psbt)
            .collect()
    }

    fn delete_spend(&mut self, txid: &bitcoin::Txid) {
        self.delete_spend(txid)
    }

    fn rollback_tip(&mut self, new_tip: &BlockChainTip) {
        self.rollback_tip(new_tip)
    }

    fn list_updated_coins(&mut self, start: u32, end: u32, limit: u64) -> Vec<Coin> {
        self.db_list_updated_coins(start, end, limit)
            .into_iter()
            .map(Coin::from)
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpendBlock {
    pub height: i32,
    pub time: u32,
}

impl From<DbSpendBlock> for SpendBlock {
    fn from(b: DbSpendBlock) -> SpendBlock {
        SpendBlock {
            height: b.height,
            time: b.time,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Coin {
    pub outpoint: bitcoin::OutPoint,
    pub block_height: Option<i32>,
    pub block_time: Option<u32>,
    pub amount: bitcoin::Amount,
    pub derivation_index: bip32::ChildNumber,
    pub is_change: bool,
    pub spend_txid: Option<bitcoin::Txid>,
    pub spend_block: Option<SpendBlock>,
}

impl std::convert::From<DbCoin> for Coin {
    fn from(db_coin: DbCoin) -> Coin {
        let DbCoin {
            outpoint,
            block_height,
            block_time,
            amount,
            derivation_index,
            is_change,
            spend_txid,
            spend_block,
            ..
        } = db_coin;
        Coin {
            outpoint,
            block_height,
            block_time,
            amount,
            derivation_index,
            is_change,
            spend_txid,
            spend_block: spend_block.map(SpendBlock::from),
        }
    }
}

impl Coin {
    pub fn is_confirmed(&self) -> bool {
        self.block_height.is_some()
    }

    pub fn is_spent(&self) -> bool {
        self.spend_txid.is_some()
    }
}
