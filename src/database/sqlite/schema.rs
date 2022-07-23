use std::{convert::TryFrom, str::FromStr};

use miniscript::{
    bitcoin::{self, consensus::encode, util::bip32},
    Descriptor, DescriptorPublicKey,
};

pub const SCHEMA: &str = "\
CREATE TABLE version (
    version INTEGER NOT NULL
);

/* About the Bitcoin network. */
CREATE TABLE tip (
    network TEXT NOT NULL,
    blockheight INTEGER,
    blockhash BLOB
);

/* This stores metadata about our wallet. We only support single wallet for
 * now (and the foreseeable future).
 */
CREATE TABLE wallets (
    id INTEGER PRIMARY KEY NOT NULL,
    timestamp INTEGER NOT NULL,
    main_descriptor TEXT NOT NULL,
    deposit_derivation_index INTEGER NOT NULL
);
";

/// A row in the "tip" table.
#[derive(Clone, Debug)]
pub struct DbTip {
    pub network: bitcoin::Network,
    pub block_height: Option<i32>,
    pub block_hash: Option<bitcoin::BlockHash>,
}

impl TryFrom<&rusqlite::Row<'_>> for DbTip {
    type Error = rusqlite::Error;

    fn try_from(row: &rusqlite::Row) -> Result<Self, Self::Error> {
        let network: String = row.get(0)?;
        let network = bitcoin::Network::from_str(&network)
            .expect("Insane database: can't parse network string");

        let block_height: Option<i32> = row.get(1)?;
        let block_hash: Option<Vec<u8>> = row.get(2)?;
        let block_hash: Option<bitcoin::BlockHash> = block_hash
            .map(|h| encode::deserialize(&h).expect("Insane database: can't parse network string"));

        Ok(DbTip {
            network,
            block_height,
            block_hash,
        })
    }
}

/// A row in the "wallets" table.
#[derive(Clone, Debug)]
pub struct DbWallet {
    pub id: i64,
    pub timestamp: u32,
    pub main_descriptor: Descriptor<DescriptorPublicKey>,
    pub deposit_derivation_index: bip32::ChildNumber,
}

impl TryFrom<&rusqlite::Row<'_>> for DbWallet {
    type Error = rusqlite::Error;

    fn try_from(row: &rusqlite::Row) -> Result<Self, Self::Error> {
        let id = row.get(0)?;
        let timestamp = row.get(1)?;

        let desc_str: String = row.get(2)?;
        let main_descriptor = Descriptor::<DescriptorPublicKey>::from_str(&desc_str)
            .expect("Insane database: can't parse deposit descriptor");

        let der_idx: u32 = row.get(3)?;
        let deposit_derivation_index = bip32::ChildNumber::from(der_idx);

        Ok(DbWallet {
            id,
            timestamp,
            main_descriptor,
            deposit_derivation_index,
        })
    }
}