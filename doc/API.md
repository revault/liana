# Liana daemon API

`lianad` exposes a [JSON-RPC 2.0](https://www.jsonrpc.org/specification)
interface over a Unix Domain socket.

Commands must be sent as valid JSONRPC 2.0 requests, ending with a `\n`.

| Command                                                     | Description                                                   |
| ----------------------------------------------------------- | ----------------------------------------------------          |
| [`stop`](#stop)                                             | Stops the minisafe daemon                                     |
| [`getinfo`](#getinfo)                                       | Get general information about the daemon                      |
| [`getnewaddress`](#getnewaddress)                           | Get a new receiving address                                   |
| [`listcoins`](#listcoins)                                   | List all wallet transaction outputs.                          |
| [`listspendtxs`](#listspendtxs)                             | List all stored Spend transactions                            |
| [`delspendtx`](#delspendtx)                                 | Delete a stored Spend transaction                             |
| [`broadcastspend`](#broadcastspend)                         | Finalize a stored Spend PSBT, and broadcast it                |
| [`broadcasttx`](#broadcasttx)                               | Finalize a given PSBT, and broadcast it                       |
| [`startrescan`](#startrescan)                               | Start rescanning the block chain from a given date            |
| [`listconfirmed`](#listconfirmed)                           | List of confirmed transactions of incoming and outgoing funds |
| [`listtransactions`](#listtransactions)                     | List of transactions with the given txids                     |

# Reference

## General

### `stop`

Stops the Liana daemon.

#### Response

Returns an empty response.

| Field         | Type   | Description |
| ------------- | ------ | ----------- |

### `getinfo`

General information about the daemon

#### Request

This command does not take any parameter for now.

| Field         | Type              | Description                                                 |
| ------------- | ----------------- | ----------------------------------------------------------- |

#### Response

| Field                | Type    | Description                                                                                        |
| -------------------- | ------- | -------------------------------------------------------------------------------------------------- |
| `version`            | string        | Version following the [SimVer](http://www.simver.org/) format                                |
| `network`            | string        | Answer can be `mainnet`, `testnet`, `regtest`                                                |
| `block_height`       | integer       | The block height we are synced at.                                                           |
| `sync`               | float         | The synchronization progress as percentage (`0 < sync < 1`)                                  |
| `descriptors`        | object        | Object with the name of the descriptor as key and the descriptor string as value             |
| `rescan_progress`    | float or null | Progress of an ongoing rescan as a percentage (between 0 and 1) if there is any              |

### `getnewaddress`

Get a new address for receiving coins. This will always generate a new address regardless of whether
it was used or not.

#### Request

This command does not take any parameter for now.

| Field         | Type              | Description                                                 |
| ------------- | ----------------- | ----------------------------------------------------------- |

#### Response

| Field         | Type   | Description        |
| ------------- | ------ | ------------------ |
| `address`     | string | A Bitcoin address  |


### `listcoins`

List all our transaction outputs, regardless of their state (unspent or not).

#### Request

This command does not take any parameter for now.

| Field         | Type              | Description                                                 |
| ------------- | ----------------- | ----------------------------------------------------------- |

#### Response

| Field          | Type          | Description                                                                                                        |
| -------------- | ------------- | ------------------------------------------------------------------------------------------------------------------ |
| `amount`       | int           | Value of the TxO in satoshis.                                                                                      |
| `outpoint`     | string        | Transaction id and output index of this coin.                                                                      |
| `block_height` | int or null   | Block height the transaction was confirmed at, or `null`.                                                          |
| `spend_info`   | object        | Information about the transaction spending this coin. See [Spending transaction info](#spending_transaction_info). |


##### Spending transaction info

| Field      | Type        | Description                                                    |
| ---------- | ----------- | -------------------------------------------------------------- |
| `txid`     | str         | Spending transaction's id.                                     |
| `height`   | int or null | Block height the spending tx was included at, if confirmed.    |


### `createspend`

Create a transaction spending one or more of our coins. All coins must exist and not be spent.

Will error if the given coins are not sufficient to cover the transaction cost at 90% (or more) of
the given feerate. If on the contrary the transaction is more than sufficiently funded, it will
create a change output when economically rationale to do so.

This command will refuse to create any output worth less than 5k sats.

#### Request

| Field          | Type              | Description                                                       |
| -------------- | ----------------- | ----------------------------------------------------------------- |
| `outpoints`    | list of string    | List of the coins to be spent, as `txid:vout`.                    |
| `destinations` | object            | Map from Bitcoin address to value                                 |
| `feerate`      | integer           | Target feerate for the transaction, in satoshis per virtual byte. |

#### Response

| Field          | Type      | Description                                          |
| -------------- | --------- | ---------------------------------------------------- |
| `psbt`         | string    | PSBT of the spending transaction, encoded as base64. |


### `updatespend`

Store the PSBT of a Spend transaction in database, updating it if it already exists.

Will merge the partial signatures for all inputs if a PSBT for a transaction with the same txid
exists in DB.

#### Request

| Field     | Type   | Description                                 |
| --------- | ------ | ------------------------------------------- |
| `psbt`    | string | Base64-encoded PSBT of a Spend transaction. |

#### Response

This command does not return anything for now.

| Field          | Type      | Description                                          |
| -------------- | --------- | ---------------------------------------------------- |


### `listspendtxs`

List stored Spend transactions.

#### Request

This command does not take any parameter for now.

| Field         | Type              | Description                                                 |
| ------------- | ----------------- | ----------------------------------------------------------- |

#### Response

| Field          | Type          | Description                                                      |
| -------------- | ------------- | ---------------------------------------------------------------- |
| `spend_txs`    | array         | Array of Spend tx entries                                        |

##### Spend tx entry

| Field          | Type              | Description                                                             |
| -------------- | ----------------- | ----------------------------------------------------------------------- |
| `psbt`         | string            | Base64-encoded PSBT of the Spend transaction.                           |
| `change_index` | int or null       | Index of the change output in the transaction outputs, if there is one. |


### `delspendtx`

#### Request

| Field    | Type   | Description                                         |
| -------- | ------ | --------------------------------------------------- |
| `txid`   | string | Hex encoded txid of the Spend transaction to delete |

#### Response

This command does not return anything for now.

| Field          | Type      | Description                                          |
| -------------- | --------- | ---------------------------------------------------- |

### `broadcastspend`

#### Request

| Field    | Type   | Description                                            |
| -------- | ------ | ------------------------------------------------------ |
| `txid`   | string | Hex encoded txid of the Spend transaction to broadcast |

#### Response

This command does not return anything for now.

| Field          | Type      | Description                                          |
| -------------- | --------- | ---------------------------------------------------- |

### `broadcasttx`

#### Request

| Field    | Type   | Description                                            |
| -------- | ------ | ------------------------------------------------------ |
| `psbt`   | string | Base64-encoded PSBT of a Spend transaction.            |

#### Response

This command does not return anything for now.

| Field          | Type      | Description                                          |
| -------------- | --------- | ---------------------------------------------------- |

### `startrescan`

#### Request

| Field        | Type   | Description                                            |
| ------------ | ------ | ------------------------------------------------------ |
| `timestamp`  | int    | Date to start rescanning from, as a UNIX timestamp     |

#### Response

This command does not return anything for now.

| Field          | Type      | Description                                          |
| -------------- | --------- | ---------------------------------------------------- |

### `listconfirmed`

`listconfirmed` retrieves a paginated and ordered list of transactions that were confirmed within a given time window.
Confirmation time is based on the timestamp of blocks.

#### Request

| Field         | Type         | Description                                |
| ------------- | ------------ | ------------------------------------------ |
| `start`       | int          | Inclusive lower bound of the time window   |
| `end`         | int          | Inclusive upper bound of the time window   |
| `limit`       | int          | Maximum number of transactions to retrieve |

#### Response

| Field          | Type   | Description                                            |
| -------------- | ------ | ------------------------------------------------------ |
| `transactions` | array  | Array of [Transaction resource](#transaction-resource) |

##### Transaction Resource

| Field    | Type          | Description                                                               |
| -------- | ------------- | ------------------------------------------------------------------------- |
| `height` | int or `null` | Block height of the transaction, `null` if the transaction is unconfirmed |
| `time`   | int or `null` | Block time of the transaction, `null` if the transaction is unconfirmed   |
| `tx`     | string        | hex encoded bitcoin transaction                                           |

### `listtransactions`

`listtransactions` retrieves the transactions with the given txids.

#### Request

| Field         | Type            | Description                           |
| ------------- | --------------- | ------------------------------------- |
| `txids`       | array of string | Ids of the transactions  to retrieve  |

#### Response

| Field          | Type   | Description                                            |
| -------------- | ------ | ------------------------------------------------------ |
| `transactions` | array  | Array of [Transaction resource](#transaction-resource) |
