// Copyright (c) 2022 Boog900

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

use lmdb::{Cursor, Database, Environment, EnvironmentFlags, Transaction, WriteFlags};
use monero::consensus::{deserialize, serialize, Decodable, Encodable};
use monero::cryptonote::hash::Hashable;
use monero::database::block::{AltBlock, BlockHeight, BlockInfo};
use monero::database::transaction::{
    OutTx, PreRctOutkey, RctOutkey, TransactionPruned, TxIndex, TxOutputIdx, TxPoolMeta,
};
use monero::{Block, Hash, PublicKey};
use std::fmt::Debug;
use std::path::Path;

use super::sub_db::MoneroSubDB;
use super::{Error, ZERO_KEY};

/// Struct containing the data needed to interact with a
/// Monero database
///
pub struct MoneroDB {
    env: Environment,
    sub_dbs: MoneroSubDB,
    read_only: bool,
}

impl MoneroDB {
    /// Opens the Monero the database
    ///
    pub fn open(dir: &Path, read_only: bool) -> Result<Self, Error> {
        let mut env = Environment::new();
        let mut flags = EnvironmentFlags::NO_READAHEAD;
        if read_only {
            flags |= EnvironmentFlags::READ_ONLY
        }
        env.set_max_dbs(32)
            .set_map_size(1 << 30)
            .set_max_readers(126)
            .set_flags(flags);
        let env = env.open(dir)?;
        env.check_do_resize()?;
        let sub_dbs = MoneroSubDB::new(&env)?;
        Ok(MoneroDB {
            env,
            sub_dbs,
            read_only,
        })
    }

    /// Gets alternative block from the database.
    ///
    pub fn get_alt_block(&self, block_hash: &Hash) -> Result<AltBlock, Error> {
        get_item(
            &self.env,
            self.sub_dbs.alt_blocks,
            block_hash.as_bytes(),
            &[0],
            15,
        )
    }

    /// Gets block from the database.
    ///
    pub fn get_block(&self, block_height: u64) -> Result<Vec<u8>, Error> {

        get_raw_item(&self.env,
                     self.sub_dbs.blocks,
                     &block_height.to_le_bytes(),
                     &[0],
                     15,)
    }

    /// Gets block info from the database
    ///
    pub fn get_block_info(&self, block_height: u64) -> Result<BlockInfo, Error> {
        get_item(
            &self.env,
            self.sub_dbs.block_info,
            &ZERO_KEY,
            &block_height.to_le_bytes(),
            2,
        )
    }

    /// Gets the blocks difficulty from the database
    ///
    pub fn get_block_difficulty(&self, block_height: u64) -> Result<u128, Error> {
        let prev_block = get_item::<BlockInfo>(
            &self.env,
            self.sub_dbs.block_info,
            &ZERO_KEY,
            &(block_height - 1).to_le_bytes(),
            2,
        )?;
        let block = get_item::<BlockInfo>(
            &self.env,
            self.sub_dbs.block_info,
            &ZERO_KEY,
            &block_height.to_le_bytes(),
            2,
        )?;

        Ok(block.cumulative_difficulty() - prev_block.cumulative_difficulty())
    }

    /// Gets block height from database
    ///
    pub fn get_block_height(&self, block_hash: &Hash) -> Result<BlockHeight, Error> {
        get_item(
            &self.env,
            self.sub_dbs.block_heights,
            &ZERO_KEY,
            block_hash.as_bytes(),
            2,
        )
    }

    /// Get the height of the blockchain (1 + height of max block)
    ///
    pub fn get_blockchain_height(&self) -> Result<u64, Error> {
        let transaction = self.env.begin_ro_txn()?;
        let stats = transaction.stat(self.sub_dbs.block_heights)?;
        Ok(stats.entries() as u64)
    }

    /// Gets the blocks hard fork version
    ///
    pub fn get_hf_version(&self, block_height: u64) -> Result<u8, Error> {
        get_item::<u8>(
            &self.env,
            self.sub_dbs.hf_versions,
            &block_height.to_le_bytes(),
            &[0],
            15,
        )
    }

    /// Gets the pruned part of the transaction
    ///
    pub fn get_tx_pruned(&self, txn_id: u64) -> Result<Vec<u8>, Error> {

        get_raw_item(&self.env,
                     self.sub_dbs.txs_pruned,
                     &txn_id.to_le_bytes(),
                     &[0],
                     15,)
    }

    /// Gets the prunable part of the transaction
    ///
    pub fn get_tx_prunable(&self, txn_id: u64) -> Result<Vec<u8>, Error> {
        get_raw_item(
            &self.env,
            self.sub_dbs.txs_prunable,
            &txn_id.to_le_bytes(),
            &[0],
            15,
        )
    }

    /// Gets the [`Outkey`] of a transactions output
    ///
    pub fn get_output_rct_outkey(
        &self,
        amount: u64,
        amount_output_index: u64,
    ) -> Result<RctOutkey, Error> {
        get_item(
            &self.env,
            self.sub_dbs.output_amounts,
            &amount.to_le_bytes(),
            &amount_output_index.to_le_bytes(),
            2,
        )
    }

    /// Gets the [`PreRctOutkey`] of a transactions output
    ///
    pub fn get_output_pre_rct_outkey(
        &self,
        amount: u64,
        amount_output_index: u64,
    ) -> Result<PreRctOutkey, Error> {
        get_item(
            &self.env,
            self.sub_dbs.output_amounts,
            &amount.to_le_bytes(),
            &amount_output_index.to_le_bytes(),
            2,
        )
    }

    /// Gets amount output indices of the transaction outputs
    ///
    pub fn get_tx_output_idx(&self, txn_id: u64) -> Result<TxOutputIdx, Error> {
        get_item(
            &self.env,
            self.sub_dbs.tx_outputs,
            &txn_id.to_le_bytes(),
            &[0],
            15,
        )
    }

    /// Gets the hash of the prunable part of the transaction
    ///
    pub fn get_txs_prunable_hash(&self, txn_id: u64) -> Result<Hash, Error> {
        get_item(
            &self.env,
            self.sub_dbs.txs_prunable_hash,
            &txn_id.to_le_bytes(),
            &[0],
            15,
        )
    }

    /// Gets the height of the transaction if that transactions block height + 5500 is >= the blockchain height
    ///
    pub fn get_txs_prunable_tip(&self, txn_id: u64) -> Result<u64, Error> {
        get_item(
            &self.env,
            self.sub_dbs.txs_prunable_tip,
            &txn_id.to_le_bytes(),
            &[0],
            15,
        )
    }

    /// Gets the height of the first block where the blocks height + 5500 is = the blockchain height
    ///
    pub fn get_prunable_tip(&self) -> Result<u64, Error> {
        get_item::<u64>(&self.env, self.sub_dbs.txs_prunable_tip, &[0], &[0], 0)
    }

    /// Gets the [`OutTx`] of an output
    ///
    pub fn get_output_tx(&self, output_id: u64) -> Result<OutTx, Error> {
        get_item(
            &self.env,
            self.sub_dbs.output_txs,
            &ZERO_KEY,
            &output_id.to_le_bytes(),
            2,
        )
    }

    /// Get the [`TxIndex`] from a transaction  
    ///
    pub fn get_tx_indices(&self, txn_hash: &Hash) -> Result<TxIndex, Error> {
        get_item(
            &self.env,
            self.sub_dbs.tx_indices,
            &ZERO_KEY,
            txn_hash.as_bytes(),
            2,
        )
    }

    /// Returns if a key image has already been spent
    ///
    pub fn is_key_image_spent(&self, spent_key: &[u8]) -> Result<bool, Error> {
        let data =
            get_item::<PublicKey>(&self.env, self.sub_dbs.spent_keys, &ZERO_KEY, spent_key, 2);
        if let Err(Error::DatabaseError(e)) = data {
            // key not found
            if e.to_err_code() == -30798 {
                return Ok(false);
            }
            return Err(Error::DatabaseError(e));
        }
        Ok(true)
    }

    /// Get the transaction from transaction pool
    ///
    pub fn get_txpool_tx(&self, txn_hash: &Hash) -> Result<monero::Transaction, Error> {
        get_item(
            &self.env,
            self.sub_dbs.txpool_blob,
            txn_hash.as_bytes(),
            &[0],
            15,
        )
    }

    /// Get the TxPoolMeta from transaction pool
    ///
    pub fn get_txpool_meta(&self, txn_hash: &Hash) -> Result<TxPoolMeta, Error> {
        get_item(
            &self.env,
            self.sub_dbs.txpool_meta,
            txn_hash.as_bytes(),
            &[0],
            15,
        )
    }

    /// Gets the version of the database, the current version is 5
    ///
    pub fn get_db_version(&self) -> Result<u32, Error> {
        let key = b"version\0";
        get_item::<u32>(&self.env, self.sub_dbs.properties, key, &[0], 15)
    }

    /// Gets the pruning seed of the database
    ///
    pub fn get_db_pruning_seed(&self) -> Result<u32, Error> {
        let key = b"pruning_seed\0";
        get_item::<u32>(&self.env, self.sub_dbs.properties, key, &[0], 15)
    }

    /// Gets the max block size
    ///
    pub fn get_max_block_size(&self) -> Result<u64, Error> {
        let key = b"max_block_size\0";
        get_item::<u64>(&self.env, self.sub_dbs.properties, key, &[0], 15)
    }

    /// Returns if the database is readonly
    ///
    pub fn is_readonly(&self) -> bool {
        self.read_only
    }

    // ##################### WRITE TRANSACTIONS #####################

    /// Adds an alt block to the database
    ///
    pub fn add_alt_block(&self, alt_block: &AltBlock) -> Result<(), Error> {
        if self.is_readonly() {
            return Err(Error::ReadOnly);
        }
        let block_id = alt_block.block.id().as_bytes().to_vec();
        put_item(
            &self.env,
            self.sub_dbs.alt_blocks,
            &block_id,
            &serialize(alt_block),
            WriteFlags::NO_DUP_DATA,
        )
    }

    /// Adds a transaction to the transaction pool
    ///
    pub fn add_txpool_tx(
        &self,
        tx: &monero::Transaction,
        tx_meta: &TxPoolMeta,
    ) -> Result<(), Error> {
        if self.is_readonly() {
            return Err(Error::ReadOnly);
        }
        let tx_hash = tx.hash().as_bytes().to_vec();
        put_item(
            &self.env,
            self.sub_dbs.txpool_meta,
            &tx_hash,
            &serialize(tx_meta),
            WriteFlags::NO_DUP_DATA,
        )?;
        put_item(
            &self.env,
            self.sub_dbs.txpool_blob,
            &tx_hash,
            &serialize(tx),
            WriteFlags::NO_DUP_DATA,
        )?;
        Ok(())
    }
}

fn get_raw_item(
    env: &Environment,
    db: Database,
    key: &[u8],
    data: &[u8],
    op: u32,
) -> Result<Vec<u8>, Error> {
    let transaction = env.begin_ro_txn()?;
    let curser = transaction.open_ro_cursor(db)?;
    let value = curser.get(Some(key), Some(data), op)?;

    Ok(value.1.to_vec())
}

fn get_item<T: Decodable + Encodable + Debug>(
    env: &Environment,
    db: Database,
    key: &[u8],
    data: &[u8],
    op: u32,
) -> Result<T, Error> {
    let value = get_raw_item(env, db, key, data, op)?;

    Ok(deserialize(&value)?)
}

fn put_item(
    env: &Environment,
    db: Database,
    key: &Vec<u8>,
    data: &Vec<u8>,
    flags: WriteFlags,
) -> Result<(), Error> {
    let mut transaction = env.begin_rw_txn()?;
    let mut curser = transaction.open_rw_cursor(db)?;
    Ok(curser.put(key, data, flags)?)
}
