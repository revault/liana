use minisafe::{
    config::Config as DaemonConfig,
    miniscript::bitcoin::{
        util::{bip32::Fingerprint, psbt::Psbt},
        Address,
    },
};

use crate::{
    app::{error::Error, view},
    daemon::model::*,
    hw::HardwareWallet,
};

#[derive(Debug)]
pub enum Message {
    Tick,
    Event(iced_native::Event),
    View(view::Message),
    LoadDaemonConfig(Box<DaemonConfig>),
    DaemonConfigLoaded(Result<(), Error>),
    Info(Result<GetInfoResult, Error>),
    ReceiveAddress(Result<Address, Error>),
    Coins(Result<Vec<Coin>, Error>),
    SpendTxs(Result<Vec<SpendTx>, Error>),
    Psbt(Result<Psbt, Error>),
    Signed(Result<(Psbt, Fingerprint), Error>),
    Updated(Result<(), Error>),
    StartRescan(Result<(), Error>),
    ConnectedHardwareWallets(Vec<HardwareWallet>),
    HistoryEvents(Result<Vec<HistoryEvent>, Error>),
}
