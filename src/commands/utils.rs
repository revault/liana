use crate::database::DatabaseConnection;

use miniscript::bitcoin::{self, consensus, util::psbt::PartiallySignedTransaction as Psbt};
use serde::{de, Deserialize, Deserializer, Serializer};

/// Serialize an amount as sats
pub fn ser_amount<S: Serializer>(amount: &bitcoin::Amount, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u64(amount.to_sat())
}

/// Deserialize an amount from sats
pub fn deser_amount_from_sats<'de, D>(deserializer: D) -> Result<bitcoin::Amount, D::Error>
where
    D: Deserializer<'de>,
{
    let a = u64::deserialize(deserializer)?;
    Ok(bitcoin::Amount::from_sat(a))
}

pub fn ser_base64<S, T>(t: T, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: consensus::Encodable,
{
    s.serialize_str(&base64::encode(consensus::serialize(&t)))
}

pub fn deser_psbt_base64<'de, D>(d: D) -> Result<Psbt, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    let s = base64::decode(&s).map_err(de::Error::custom)?;
    let psbt = consensus::deserialize(&s).map_err(de::Error::custom)?;
    Ok(psbt)
}

// Utility to gather the index of a change output in a Psbt, if there is one.
pub fn change_index(psbt: &Psbt, db_conn: &mut Box<dyn DatabaseConnection>) -> Option<usize> {
    let network = db_conn.network();

    for (i, txo) in psbt.unsigned_tx.output.iter().enumerate() {
        // Small optimization. TODO: adapt once we have Taproot support.
        if !txo.script_pubkey.is_v0_p2wsh() {
            continue;
        }

        if let Ok(address) = bitcoin::Address::from_script(&txo.script_pubkey, network) {
            if let Some((_, true)) = db_conn.derivation_index_by_address(&address) {
                return Some(i);
            }
        }
    }

    None
}

/// Serialize an amount option as sats
pub fn ser_optional_amount<S: Serializer>(
    amount: &Option<bitcoin::Amount>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match amount {
        Some(amount) => s.serialize_u64(amount.to_sat()),
        None => s.serialize_none(),
    }
}

/// Deserialize an amount option from sats
pub fn deser_optional_amount_from_sats<'de, D>(
    deserializer: D,
) -> Result<Option<bitcoin::Amount>, D::Error>
where
    D: Deserializer<'de>,
{
    let a = Option::<u64>::deserialize(deserializer)?;
    Ok(a.map(bitcoin::Amount::from_sat))
}
