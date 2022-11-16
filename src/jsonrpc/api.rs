use crate::{
    jsonrpc::{Error, Params, Request, Response},
    DaemonControl,
};

use std::{collections::HashMap, convert::TryInto, str::FromStr};

use miniscript::bitcoin::{self, consensus, util::psbt::PartiallySignedTransaction as Psbt};

fn create_spend(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let outpoints = params
        .get(0, "outpoints")
        .ok_or_else(|| Error::invalid_params("Missing 'outpoints' parameter."))?
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .map(|entry| {
                    entry
                        .as_str()
                        .and_then(|e| bitcoin::OutPoint::from_str(e).ok())
                })
                .collect::<Option<Vec<bitcoin::OutPoint>>>()
        })
        .ok_or_else(|| Error::invalid_params("Invalid 'outpoints' parameter."))?;
    let destinations = params
        .get(1, "destinations")
        .ok_or_else(|| Error::invalid_params("Missing 'destinations' parameter."))?
        .as_object()
        .and_then(|obj| {
            obj.into_iter()
                .map(|(k, v)| {
                    let addr = bitcoin::Address::from_str(k).ok()?;
                    let amount: u64 = v.as_i64()?.try_into().ok()?;
                    Some((addr, amount))
                })
                .collect::<Option<HashMap<bitcoin::Address, u64>>>()
        })
        .ok_or_else(|| Error::invalid_params("Invalid 'destinations' parameter."))?;
    let feerate: u64 = params
        .get(2, "feerate")
        .ok_or_else(|| Error::invalid_params("Missing 'feerate' parameter."))?
        .as_i64()
        .and_then(|i| i.try_into().ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'feerate' parameter."))?;

    let res = control.create_spend(&outpoints, &destinations, feerate)?;
    Ok(serde_json::json!(&res))
}

fn update_spend(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let psbt: Psbt = params
        .get(0, "psbt")
        .ok_or_else(|| Error::invalid_params("Missing 'psbt' parameter."))?
        .as_str()
        .and_then(|s| base64::decode(s).ok())
        .and_then(|bytes| consensus::deserialize(&bytes).ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'psbt' parameter."))?;
    control.update_spend(psbt)?;

    Ok(serde_json::json!({}))
}

fn delete_spend(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let txid = params
        .get(0, "txid")
        .ok_or_else(|| Error::invalid_params("Missing 'txid' parameter."))?
        .as_str()
        .and_then(|s| bitcoin::Txid::from_str(s).ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'txid' parameter."))?;
    control.delete_spend(&txid);

    Ok(serde_json::json!({}))
}

fn broadcast_spend(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let txid = params
        .get(0, "txid")
        .ok_or_else(|| Error::invalid_params("Missing 'txid' parameter."))?
        .as_str()
        .and_then(|s| bitcoin::Txid::from_str(s).ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'txid' parameter."))?;
    control.broadcast_spend(&txid)?;

    Ok(serde_json::json!({}))
}

fn start_rescan(control: &DaemonControl, params: Params) -> Result<serde_json::Value, Error> {
    let timestamp: u32 = params
        .get(0, "timestamp")
        .ok_or_else(|| Error::invalid_params("Missing 'timestamp' parameter."))?
        .as_u64()
        .and_then(|t| t.try_into().ok())
        .ok_or_else(|| Error::invalid_params("Invalid 'timestamp' parameter."))?;
    control.start_rescan(timestamp)?;

    Ok(serde_json::json!({}))
}

/// Handle an incoming JSONRPC2 request.
pub fn handle_request(control: &DaemonControl, req: Request) -> Result<Response, Error> {
    let result = match req.method.as_str() {
        "broadcastspend" => {
            let params = req
                .params
                .ok_or_else(|| Error::invalid_params("Missing 'txid' parameter."))?;
            broadcast_spend(control, params)?
        }
        "createspend" => {
            let params = req.params.ok_or_else(|| {
                Error::invalid_params(
                    "Missing 'outpoints', 'destinations' and 'feerate' parameters.",
                )
            })?;
            create_spend(control, params)?
        }
        "delspendtx" => {
            let params = req
                .params
                .ok_or_else(|| Error::invalid_params("Missing 'txid' parameter."))?;
            delete_spend(control, params)?
        }
        "getinfo" => serde_json::json!(&control.get_info()),
        "getnewaddress" => serde_json::json!(&control.get_new_address()),
        "listcoins" => serde_json::json!(&control.list_coins()),
        "listspendtxs" => serde_json::json!(&control.list_spend()),
        "startrescan" => {
            let params = req
                .params
                .ok_or_else(|| Error::invalid_params("Missing 'timestamp' parameter."))?;
            start_rescan(control, params)?
        }
        "stop" => serde_json::json!({}),
        "updatespend" => {
            let params = req
                .params
                .ok_or_else(|| Error::invalid_params("Missing 'psbt' parameter."))?;
            update_spend(control, params)?
        }
        "gethistory" => match req.params {
            Some(Params::Array(params)) => {
                if params.len() != 3 {
                    return Err(Error::invalid_params("command requires 3 parameters"));
                }
                if let (Some(start), Some(end), Some(limit)) = (
                    serde_json::Value::as_u64(&params[0]),
                    serde_json::Value::as_u64(&params[1]),
                    serde_json::Value::as_u64(&params[2]),
                ) {
                    serde_json::json!(&control.gethistory(start as u32, end as u32, limit))
                } else {
                    return Err(Error::invalid_params("command requires 3 parameters"));
                }
            }
            None => return Err(Error::invalid_params("command requires 3 parameters")),
            _ => return Err(Error::invalid_params("invalid parameters")),
        },
        _ => {
            return Err(Error::method_not_found());
        }
    };

    Ok(Response::success(req.id, result))
}
